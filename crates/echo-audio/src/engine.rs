use crate::dsp::{is_silent_packet, Equalizer, SpatialProcessor};
use crate::normalization::{apply_gain_and_soft_limit, compute_normalization_gain};
use crate::source::HttpRangeSource;
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::SampleFormat;
use echo_core::{EqPreset, SpatialMode};
use std::collections::VecDeque;
use std::fs::File;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{mpsc, Arc, Mutex};
use std::time::Duration;
use symphonia::core::audio::SampleBuffer;
use symphonia::core::codecs::DecoderOptions;
use symphonia::core::errors::Error as SymphoniaError;
use symphonia::core::formats::{FormatOptions, SeekMode, SeekTo};
use symphonia::core::io::{MediaSource, MediaSourceStream};
use symphonia::core::meta::MetadataOptions;
use symphonia::core::probe::Hint;
use symphonia::core::units::Time;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PlaybackStatus {
    Stopped,
    Loading,
    Playing,
    Paused,
    Error(String),
}

#[derive(Debug, Clone)]
pub struct PlaybackSnapshot {
    pub status: PlaybackStatus,
    pub position_ms: u64,
    pub duration_ms: u64,
    pub volume: f32,
    pub normalization_enabled: bool,
    pub loudness_db: Option<f32>,
    pub finished_seq: u64,
}

#[derive(Debug, Clone)]
pub enum AudioSourceSpec {
    Url {
        url: String,
        user_agent: String,
        mime_hint: Option<String>,
        content_length: Option<u64>,
        approx_duration_ms: Option<u64>,
        loudness_db: Option<f32>,
    },
    File {
        path: PathBuf,
        loudness_db: Option<f32>,
    },
}

enum Command {
    Play(AudioSourceSpec),
    Pause,
    Resume,
    Stop,
    Seek(u64),
}

struct SharedState {
    status: Mutex<PlaybackStatus>,
    position_ms: AtomicU64,
    duration_ms: AtomicU64,
    volume_bits: AtomicU64,
    norm_gain_bits: AtomicU64,
    fade_factor_bits: AtomicU64,
    normalization_enabled: AtomicBool,
    skip_silence: AtomicBool,
    dsp_config: Mutex<(EqPreset, SpatialMode)>,
    dsp_generation: AtomicU64,
    loudness_db: Mutex<Option<f32>>,
    is_local: AtomicBool,
    finished_seq: AtomicU64,
    paused: AtomicBool,
    ring: Mutex<VecDeque<f32>>,
}

impl SharedState {
    fn new(initial_volume: f32, normalization_enabled: bool) -> Self {
        let vol = initial_volume.clamp(0.0, 4.0);
        Self {
            status: Mutex::new(PlaybackStatus::Stopped),
            position_ms: AtomicU64::new(0),
            duration_ms: AtomicU64::new(0),
            volume_bits: AtomicU64::new((vol as f64).to_bits()),
            norm_gain_bits: AtomicU64::new((1.0f64).to_bits()),
            fade_factor_bits: AtomicU64::new((1.0f64).to_bits()),
            normalization_enabled: AtomicBool::new(normalization_enabled),
            skip_silence: AtomicBool::new(false),
            dsp_config: Mutex::new((EqPreset::Flat, SpatialMode::Off)),
            dsp_generation: AtomicU64::new(0),
            loudness_db: Mutex::new(None),
            is_local: AtomicBool::new(false),
            finished_seq: AtomicU64::new(0),
            paused: AtomicBool::new(false),
            ring: Mutex::new(VecDeque::with_capacity(48_000 * 4)),
        }
    }

    fn total_gain(&self) -> f32 {
        let vol = f64::from_bits(self.volume_bits.load(Ordering::Relaxed)) as f32;
        let norm = f64::from_bits(self.norm_gain_bits.load(Ordering::Relaxed)) as f32;
        let fade = f64::from_bits(self.fade_factor_bits.load(Ordering::Relaxed)) as f32;
        vol * norm * fade.clamp(0.0, 1.0)
    }

    fn refresh_norm(&self) {
        let enabled = self.normalization_enabled.load(Ordering::Relaxed);
        let ldb = *self.loudness_db.lock().expect("loudness");
        let is_local = self.is_local.load(Ordering::Relaxed);
        let gain = compute_normalization_gain(enabled, ldb, is_local);
        self.norm_gain_bits
            .store((gain as f64).to_bits(), Ordering::Relaxed);
    }
}

pub struct AudioEngine {
    tx: mpsc::Sender<Command>,
    shared: Arc<SharedState>,
}

impl AudioEngine {
    pub fn new(initial_volume: f32, normalization_enabled: bool) -> Self {
        let (tx, rx) = mpsc::channel();
        let shared = Arc::new(SharedState::new(initial_volume, normalization_enabled));
        let worker_shared = Arc::clone(&shared);
        std::thread::Builder::new()
            .name("echo-audio-worker".to_string())
            .spawn(move || audio_worker_loop(rx, worker_shared))
            .expect("spawn audio thread");
        Self { tx, shared }
    }

    pub fn play(&self, spec: AudioSourceSpec) {
        let _ = self.tx.send(Command::Play(spec));
    }

    pub fn pause(&self) {
        self.shared.paused.store(true, Ordering::Relaxed);
        *self.shared.status.lock().expect("status") = PlaybackStatus::Paused;
        let _ = self.tx.send(Command::Pause);
    }

    pub fn resume(&self) {
        self.shared.paused.store(false, Ordering::Relaxed);
        *self.shared.status.lock().expect("status") = PlaybackStatus::Playing;
        let _ = self.tx.send(Command::Resume);
    }

    pub fn stop(&self) {
        let _ = self.tx.send(Command::Stop);
    }

    pub fn seek_ms(&self, ms: u64) {
        let _ = self.tx.send(Command::Seek(ms));
    }

    pub fn set_volume(&self, volume: f32) {
        let v = volume.clamp(0.0, 4.0);
        self.shared
            .volume_bits
            .store((v as f64).to_bits(), Ordering::Relaxed);
    }

    pub fn set_fade_factor(&self, factor: f32) {
        let f = factor.clamp(0.0, 1.0);
        self.shared
            .fade_factor_bits
            .store((f as f64).to_bits(), Ordering::Relaxed);
    }

    pub fn set_skip_silence(&self, enabled: bool) {
        self.shared.skip_silence.store(enabled, Ordering::Relaxed);
    }

    pub fn set_dsp(&self, eq_preset: EqPreset, spatial_mode: SpatialMode) {
        *self.shared.dsp_config.lock().expect("dsp_config") = (eq_preset, spatial_mode);
        self.shared.dsp_generation.fetch_add(1, Ordering::Relaxed);
    }

    pub fn set_normalization(&self, enabled: bool, loudness_db: Option<f32>, is_local: bool) {
        self.shared
            .normalization_enabled
            .store(enabled, Ordering::Relaxed);
        *self.shared.loudness_db.lock().expect("loudness") = loudness_db;
        self.shared.is_local.store(is_local, Ordering::Relaxed);
        self.shared.refresh_norm();
    }

    pub fn snapshot(&self) -> PlaybackSnapshot {
        PlaybackSnapshot {
            status: self.shared.status.lock().expect("status").clone(),
            position_ms: self.shared.position_ms.load(Ordering::Relaxed),
            duration_ms: self.shared.duration_ms.load(Ordering::Relaxed),
            volume: f64::from_bits(self.shared.volume_bits.load(Ordering::Relaxed)) as f32,
            normalization_enabled: self.shared.normalization_enabled.load(Ordering::Relaxed),
            loudness_db: *self.shared.loudness_db.lock().expect("loudness"),
            finished_seq: self.shared.finished_seq.load(Ordering::Relaxed),
        }
    }
}

struct ActiveDecoder {
    format: Box<dyn symphonia::core::formats::FormatReader>,
    decoder: Box<dyn symphonia::core::codecs::Decoder>,
    track_id: u32,
    time_base: Option<symphonia::core::units::TimeBase>,
    src_rate: u32,
    src_channels: usize,
    sample_buf: Option<SampleBuffer<f32>>,
}

fn open_decoder(spec: &AudioSourceSpec) -> Result<ActiveDecoder, String> {
    let mut hint = Hint::new();
    let source: Box<dyn MediaSource> = match spec {
        AudioSourceSpec::Url {
            url,
            user_agent,
            mime_hint,
            content_length,
            ..
        } => {
            if let Some(m) = mime_hint {
                if m.contains("mp4") {
                    hint.with_extension("m4a");
                } else if m.contains("webm") {
                    hint.with_extension("webm");
                }
            }
            Box::new(
                HttpRangeSource::open(url.clone(), user_agent.clone(), *content_length)
                    .map_err(|e| format!("stream open failed: {e}"))?,
            )
        }
        AudioSourceSpec::File { path, .. } => {
            if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
                hint.with_extension(ext);
            }
            Box::new(File::open(path).map_err(|e| format!("file open failed: {e}"))?)
        }
    };

    let mss = MediaSourceStream::new(source, Default::default());
    let probed = symphonia::default::get_probe()
        .format(
            &hint,
            mss,
            &FormatOptions {
                enable_gapless: true,
                ..Default::default()
            },
            &MetadataOptions::default(),
        )
        .map_err(|e| format!("probe failed: {e}"))?;

    let format = probed.format;
    let track = format
        .tracks()
        .iter()
        .find(|t| t.codec_params.codec != symphonia::core::codecs::CODEC_TYPE_NULL)
        .ok_or_else(|| "no audio track in stream".to_string())?;
    let track_id = track.id;
    let time_base = track.codec_params.time_base;
    let src_rate = track.codec_params.sample_rate.unwrap_or(44_100);
    let src_channels = track
        .codec_params
        .channels
        .map(|c| c.count())
        .unwrap_or(2);
    let decoder = symphonia::default::get_codecs()
        .make(&track.codec_params, &DecoderOptions::default())
        .map_err(|e| format!("decoder init failed: {e}"))?;

    Ok(ActiveDecoder {
        format,
        decoder,
        track_id,
        time_base,
        src_rate,
        src_channels,
        sample_buf: None,
    })
}

fn audio_worker_loop(rx: mpsc::Receiver<Command>, shared: Arc<SharedState>) {
    let host = cpal::default_host();
    let output_device = host.default_output_device();
    let (out_rate, out_channels, _stream) = match output_device {
        Some(dev) => match dev.default_output_config() {
            Ok(cfg) => {
                let rate = cfg.sample_rate().0;
                let channels = cfg.channels() as usize;
                let fmt = cfg.sample_format();
                let stream_cfg = cfg.into();
                let stream_shared = Arc::clone(&shared);
                let stream = match fmt {
                    SampleFormat::F32 => dev.build_output_stream(
                        &stream_cfg,
                        move |data: &mut [f32], _| fill_output_f32(&stream_shared, data),
                        |err| tracing::warn!("cpal output error: {err}"),
                        None,
                    ),
                    SampleFormat::I16 => dev.build_output_stream(
                        &stream_cfg,
                        move |data: &mut [i16], _| {
                            let mut tmp = vec![0.0f32; data.len()];
                            fill_output_f32(&stream_shared, &mut tmp);
                            for (dst, src) in data.iter_mut().zip(tmp) {
                                *dst = (src.clamp(-1.0, 1.0) * 32767.0) as i16;
                            }
                        },
                        |err| tracing::warn!("cpal output error: {err}"),
                        None,
                    ),
                    SampleFormat::U16 => dev.build_output_stream(
                        &stream_cfg,
                        move |data: &mut [u16], _| {
                            let mut tmp = vec![0.0f32; data.len()];
                            fill_output_f32(&stream_shared, &mut tmp);
                            for (dst, src) in data.iter_mut().zip(tmp) {
                                *dst = ((src.clamp(-1.0, 1.0) * 32767.0) + 32768.0) as u16;
                            }
                        },
                        |err| tracing::warn!("cpal output error: {err}"),
                        None,
                    ),
                    other => {
                        *shared.status.lock().expect("status") =
                            PlaybackStatus::Error(format!("unsupported sample format {other:?}"));
                        return;
                    }
                };
                match stream {
                    Ok(s) => {
                        let _ = s.play();
                        (rate, channels, Some(s))
                    }
                    Err(e) => {
                        *shared.status.lock().expect("status") =
                            PlaybackStatus::Error(format!("audio stream error: {e}"));
                        return;
                    }
                }
            }
            Err(e) => {
                *shared.status.lock().expect("status") =
                    PlaybackStatus::Error(format!("audio config error: {e}"));
                return;
            }
        },
        None => {
            *shared.status.lock().expect("status") =
                PlaybackStatus::Error("no output audio device".to_string());
            return;
        }
    };

    let mut active: Option<ActiveDecoder> = None;
    let max_ring_samples = (out_rate as usize) * out_channels / 2; // ~500 ms buffer
    let (init_eq, init_spatial) = *shared.dsp_config.lock().expect("dsp_config");
    let mut seen_dsp_gen = shared.dsp_generation.load(Ordering::Relaxed);
    let mut equalizer = Equalizer::new(out_rate, out_channels, init_eq);
    let mut spatial = SpatialProcessor::new(out_rate, init_spatial);

    loop {
        // Drain pending commands without blocking if actively decoding.
        let cmd_opt = if active.is_some() && !shared.paused.load(Ordering::Relaxed) {
            rx.try_recv().ok()
        } else {
            rx.recv().ok()
        };

        if let Some(cmd) = cmd_opt {
            match cmd {
                Command::Play(spec) => {
                    shared.ring.lock().expect("ring").clear();
                    shared.position_ms.store(0, Ordering::Relaxed);
                    shared.paused.store(false, Ordering::Relaxed);
                    equalizer.reset();
                    spatial.reset();
                    *shared.status.lock().expect("status") = PlaybackStatus::Loading;

                    match &spec {
                        AudioSourceSpec::Url {
                            approx_duration_ms,
                            loudness_db,
                            ..
                        } => {
                            shared
                                .duration_ms
                                .store(approx_duration_ms.unwrap_or(0), Ordering::Relaxed);
                            *shared.loudness_db.lock().expect("loudness") = *loudness_db;
                            shared.is_local.store(false, Ordering::Relaxed);
                        }
                        AudioSourceSpec::File { loudness_db, .. } => {
                            shared.duration_ms.store(0, Ordering::Relaxed);
                            *shared.loudness_db.lock().expect("loudness") = *loudness_db;
                            shared.is_local.store(true, Ordering::Relaxed);
                        }
                    }
                    shared.refresh_norm();

                    match open_decoder(&spec) {
                        Ok(dec) => {
                            if let Some(track) = dec.format.tracks().iter().find(|t| t.id == dec.track_id) {
                                if let (Some(n_frames), Some(tb)) =
                                    (track.codec_params.n_frames, track.codec_params.time_base)
                                {
                                    let t = tb.calc_time(n_frames);
                                    let ms = t.seconds * 1000 + (t.frac * 1000.0) as u64;
                                    if ms > 0 {
                                        shared.duration_ms.store(ms, Ordering::Relaxed);
                                    }
                                }
                            }
                            *shared.status.lock().expect("status") = PlaybackStatus::Playing;
                            active = Some(dec);
                        }
                        Err(err) => {
                            active = None;
                            *shared.status.lock().expect("status") = PlaybackStatus::Error(err);
                        }
                    }
                }
                Command::Pause => {
                    shared.paused.store(true, Ordering::Relaxed);
                    *shared.status.lock().expect("status") = PlaybackStatus::Paused;
                }
                Command::Resume => {
                    shared.paused.store(false, Ordering::Relaxed);
                    if active.is_some() {
                        *shared.status.lock().expect("status") = PlaybackStatus::Playing;
                    }
                }
                Command::Stop => {
                    active = None;
                    shared.ring.lock().expect("ring").clear();
                    shared.position_ms.store(0, Ordering::Relaxed);
                    *shared.status.lock().expect("status") = PlaybackStatus::Stopped;
                }
                Command::Seek(ms) => {
                    if let Some(dec) = active.as_mut() {
                        let secs = ms / 1000;
                        let frac = (ms % 1000) as f64 / 1000.0;
                        let seek_to = SeekTo::Time {
                            time: Time::new(secs, frac),
                            track_id: Some(dec.track_id),
                        };
                        if dec.format.seek(SeekMode::Accurate, seek_to).is_ok() {
                            dec.decoder.reset();
                            shared.ring.lock().expect("ring").clear();
                            shared.position_ms.store(ms, Ordering::Relaxed);
                        }
                    }
                }
            }
            continue;
        }

        let Some(dec) = active.as_mut() else {
            continue;
        };

        if shared.paused.load(Ordering::Relaxed) {
            continue;
        }

        let ring_len = shared.ring.lock().expect("ring").len();
        if ring_len >= max_ring_samples {
            std::thread::sleep(Duration::from_millis(12));
            continue;
        }

        match dec.format.next_packet() {
            Ok(packet) => {
                if packet.track_id() != dec.track_id {
                    continue;
                }
                if let Some(tb) = dec.time_base {
                    let t = tb.calc_time(packet.ts());
                    let pos_ms = t.seconds * 1000 + (t.frac * 1000.0) as u64;
                    shared.position_ms.store(pos_ms, Ordering::Relaxed);
                }

                match dec.decoder.decode(&packet) {
                    Ok(decoded) => {
                        let spec = *decoded.spec();
                        let frames = decoded.frames();
                        let src_rate = spec.rate;
                        let src_ch = spec.channels.count();
                        dec.src_rate = src_rate;
                        dec.src_channels = src_ch;

                        let buf = dec
                            .sample_buf
                            .get_or_insert_with(|| SampleBuffer::<f32>::new(frames as u64, spec));
                        if buf.capacity() < frames * src_ch {
                            *buf = SampleBuffer::<f32>::new(frames as u64, spec);
                        }
                        buf.copy_interleaved_ref(decoded);
                        let mut resampled = resample_and_remix(
                            buf.samples(),
                            src_rate,
                            src_ch,
                            out_rate,
                            out_channels,
                        );
                        if shared.skip_silence.load(Ordering::Relaxed)
                            && is_silent_packet(&resampled)
                        {
                            continue;
                        }
                        let cur_gen = shared.dsp_generation.load(Ordering::Relaxed);
                        if cur_gen != seen_dsp_gen {
                            let (eq_preset, sp_mode) = *shared.dsp_config.lock().expect("dsp");
                            equalizer.configure(out_rate, out_channels, eq_preset);
                            spatial.configure(out_rate, sp_mode);
                            seen_dsp_gen = cur_gen;
                        }
                        equalizer.process_interleaved(&mut resampled, out_channels);
                        spatial.process_interleaved(&mut resampled, out_channels);
                        shared.ring.lock().expect("ring").extend(resampled);
                    }
                    Err(SymphoniaError::DecodeError(_)) => continue,
                    Err(e) => {
                        active = None;
                        *shared.status.lock().expect("status") =
                            PlaybackStatus::Error(format!("decode error: {e}"));
                    }
                }
            }
            Err(SymphoniaError::IoError(ref e))
                if e.kind() == std::io::ErrorKind::UnexpectedEof =>
            {
                // Wait for ring buffer to drain before marking track finished.
                while !shared.ring.lock().expect("ring").is_empty()
                    && !shared.paused.load(Ordering::Relaxed)
                {
                    if rx.try_recv().is_ok() {
                        break;
                    }
                    std::thread::sleep(Duration::from_millis(20));
                }
                active = None;
                *shared.status.lock().expect("status") = PlaybackStatus::Stopped;
                shared.finished_seq.fetch_add(1, Ordering::Relaxed);
            }
            Err(SymphoniaError::ResetRequired) => {
                dec.decoder.reset();
            }
            Err(e) => {
                active = None;
                *shared.status.lock().expect("status") =
                    PlaybackStatus::Error(format!("stream read error: {e}"));
            }
        }
    }
}

fn fill_output_f32(shared: &SharedState, out: &mut [f32]) {
    if shared.paused.load(Ordering::Relaxed) {
        out.fill(0.0);
        return;
    }
    let gain = shared.total_gain();
    let mut ring = shared.ring.lock().expect("ring");
    for sample in out.iter_mut() {
        if let Some(s) = ring.pop_front() {
            *sample = apply_gain_and_soft_limit(s, gain);
        } else {
            *sample = 0.0;
        }
    }
}

fn resample_and_remix(
    input: &[f32],
    src_rate: u32,
    src_ch: usize,
    dst_rate: u32,
    dst_ch: usize,
) -> Vec<f32> {
    if input.is_empty() || src_ch == 0 || dst_ch == 0 || src_rate == 0 || dst_rate == 0 {
        return Vec::new();
    }
    let src_frames = input.len() / src_ch;
    if src_frames == 0 {
        return Vec::new();
    }
    let dst_frames = ((src_frames as f64) * dst_rate as f64 / src_rate as f64).round() as usize;
    let ratio = src_rate as f64 / dst_rate as f64;
    let mut out = Vec::with_capacity(dst_frames * dst_ch);

    for f in 0..dst_frames {
        let pos = f as f64 * ratio;
        let idx0 = (pos.floor() as usize).min(src_frames - 1);
        let idx1 = (idx0 + 1).min(src_frames - 1);
        let frac = (pos - idx0 as f64) as f32;

        for c in 0..dst_ch {
            let sc = if src_ch == 1 { 0 } else { c % src_ch };
            let s0 = input[idx0 * src_ch + sc];
            let s1 = input[idx1 * src_ch + sc];
            out.push(s0 + (s1 - s0) * frac);
        }
    }
    out
}

