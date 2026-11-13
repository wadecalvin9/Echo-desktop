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
    SetDevice(Option<String>),
}

struct PauseFade {
    current_gain: f32,
    target_gain: f32,
    is_paused: bool,
}

impl PauseFade {
    fn new() -> Self {
        Self {
            current_gain: 1.0,
            target_gain: 1.0,
            is_paused: false,
        }
    }
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
    custom_eq_bands: Mutex<[f32; 10]>,
    playback_speed_bits: AtomicU64,
    dsp_generation: AtomicU64,
    loudness_db: Mutex<Option<f32>>,
    is_local: AtomicBool,
    finished_seq: AtomicU64,
    paused: AtomicBool,
    pause_fade: Mutex<PauseFade>,
    out_rate: AtomicU64,
    out_channels: AtomicU64,
    spectrum_packed: AtomicU64,
    stream_error: AtomicBool,
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
            custom_eq_bands: Mutex::new(EqPreset::Flat.gains_db()),
            playback_speed_bits: AtomicU64::new((1.0f64).to_bits()),
            dsp_generation: AtomicU64::new(0),
            loudness_db: Mutex::new(None),
            is_local: AtomicBool::new(false),
            finished_seq: AtomicU64::new(0),
            paused: AtomicBool::new(false),
            pause_fade: Mutex::new(PauseFade::new()),
            out_rate: AtomicU64::new(48_000),
            out_channels: AtomicU64::new(2),
            spectrum_packed: AtomicU64::new(0),
            stream_error: AtomicBool::new(false),
            ring: Mutex::new(VecDeque::with_capacity(48_000 * 4)),
        }
    }

    fn trigger_pause(&self) {
        let mut fade = self.pause_fade.lock().expect("pause_fade");
        fade.target_gain = 0.0;
    }

    fn trigger_resume(&self) {
        let mut fade = self.pause_fade.lock().expect("pause_fade");
        fade.target_gain = 1.0;
        fade.is_paused = false;
        self.paused.store(false, Ordering::Relaxed);
    }

    fn reset_fade_for_new_track(&self) {
        let mut fade = self.pause_fade.lock().expect("pause_fade");
        fade.current_gain = 0.0;
        fade.target_gain = 1.0;
        fade.is_paused = false;
        self.paused.store(false, Ordering::Relaxed);
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
        self.shared.trigger_pause();
        *self.shared.status.lock().expect("status") = PlaybackStatus::Paused;
        let _ = self.tx.send(Command::Pause);
    }

    pub fn resume(&self) {
        self.shared.trigger_resume();
        *self.shared.status.lock().expect("status") = PlaybackStatus::Playing;
        let _ = self.tx.send(Command::Resume);
    }

    pub fn stop(&self) {
        {
            let mut fade = self.shared.pause_fade.lock().expect("pause_fade");
            fade.current_gain = 0.0;
            fade.target_gain = 0.0;
            fade.is_paused = true;
        }
        self.shared.paused.store(true, Ordering::Relaxed);
        *self.shared.status.lock().expect("status") = PlaybackStatus::Stopped;
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
        *self.shared.custom_eq_bands.lock().expect("custom_eq") = eq_preset.gains_db();
        self.shared.dsp_generation.fetch_add(1, Ordering::Relaxed);
    }

    pub fn set_eq_bands(&self, bands: [f32; 10]) {
        *self.shared.custom_eq_bands.lock().expect("custom_eq") = bands;
        self.shared.dsp_generation.fetch_add(1, Ordering::Relaxed);
    }

    pub fn set_spatial(&self, spatial_mode: SpatialMode) {
        self.shared.dsp_config.lock().expect("dsp_config").1 = spatial_mode;
        self.shared.dsp_generation.fetch_add(1, Ordering::Relaxed);
    }

    pub fn set_playback_speed(&self, speed: f32) {
        let s = speed.clamp(0.5, 2.0);
        self.shared
            .playback_speed_bits
            .store((s as f64).to_bits(), Ordering::Relaxed);
    }

    pub fn playback_speed(&self) -> f32 {
        f64::from_bits(self.shared.playback_speed_bits.load(Ordering::Relaxed)) as f32
    }

    pub fn set_output_device(&self, device_name: Option<String>) {
        let _ = self.tx.send(Command::SetDevice(device_name));
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

    /// Returns 8 smoothed real-time frequency band levels in `0.0..=1.0` from the active output.
    pub fn spectrum_bands(&self) -> [f32; 8] {
        let packed = self.shared.spectrum_packed.load(Ordering::Relaxed);
        let mut bands = [0.0f32; 8];
        for (i, slot) in bands.iter_mut().enumerate() {
            let byte = ((packed >> (i * 8)) & 0xFF) as u8;
            *slot = byte as f32 / 255.0;
        }
        bands
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

pub fn available_output_devices() -> Vec<String> {
    let host = cpal::default_host();
    let mut names = Vec::new();
    if let Ok(devices) = host.output_devices() {
        for dev in devices {
            if let Ok(name) = dev.name() {
                if !names.contains(&name) {
                    names.push(name);
                }
            }
        }
    }
    names
}

fn build_output_stream(
    host: &cpal::Host,
    device_name: Option<&str>,
    shared: &Arc<SharedState>,
) -> Result<(u32, usize, cpal::Stream), String> {
    let dev = if let Some(target) = device_name {
        let mut found = None;
        if let Ok(devices) = host.output_devices() {
            for d in devices {
                if let Ok(name) = d.name() {
                    if name == target {
                        found = Some(d);
                        break;
                    }
                }
            }
        }
        found.or_else(|| host.default_output_device())
    } else {
        host.default_output_device()
    }
    .ok_or_else(|| "no output audio device".to_string())?;

    let cfg = dev
        .default_output_config()
        .map_err(|e| format!("audio config error: {e}"))?;
    let rate = cfg.sample_rate().0;
    let channels = cfg.channels() as usize;
    let fmt = cfg.sample_format();
    let stream_cfg = cfg.into();
    let stream_shared = Arc::clone(shared);
    let err_shared_f32 = Arc::clone(shared);
    let err_shared_i16 = Arc::clone(shared);
    let err_shared_u16 = Arc::clone(shared);
    let stream = match fmt {
        SampleFormat::F32 => dev.build_output_stream(
            &stream_cfg,
            move |data: &mut [f32], _| fill_output_f32(&stream_shared, data),
            move |err| {
                tracing::warn!("cpal output error: {err}");
                err_shared_f32.stream_error.store(true, Ordering::Relaxed);
            },
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
            move |err| {
                tracing::warn!("cpal output error: {err}");
                err_shared_i16.stream_error.store(true, Ordering::Relaxed);
            },
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
            move |err| {
                tracing::warn!("cpal output error: {err}");
                err_shared_u16.stream_error.store(true, Ordering::Relaxed);
            },
            None,
        ),
        other => {
            return Err(format!("unsupported sample format {other:?}"));
        }
    }
    .map_err(|e| format!("audio stream error: {e}"))?;

    let _ = stream.play();
    Ok((rate, channels, stream))
}

fn audio_worker_loop(rx: mpsc::Receiver<Command>, shared: Arc<SharedState>) {
    let host = cpal::default_host();
    let (mut out_rate, mut out_channels, mut _stream) =
        match build_output_stream(&host, None, &shared) {
            Ok((r, ch, s)) => (r, ch, Some(s)),
            Err(e) => {
                tracing::warn!("Initial audio stream setup failed: {e}; will retry on worker tick");
                (48_000, 2, None)
            }
        };

    let mut active: Option<ActiveDecoder> = None;
    shared.out_rate.store(out_rate as u64, Ordering::Relaxed);
    shared.out_channels.store(out_channels as u64, Ordering::Relaxed);
    let mut max_ring_samples = (out_rate as usize) * out_channels / 2; // ~500 ms buffer
    let (init_eq, init_spatial) = *shared.dsp_config.lock().expect("dsp_config");
    let mut seen_dsp_gen = shared.dsp_generation.load(Ordering::Relaxed);
    let mut equalizer = Equalizer::new(out_rate, out_channels, init_eq);
    let mut spatial = SpatialProcessor::new(out_rate, init_spatial);

    loop {
        // Recover output stream if device was disconnected or initial build was empty
        if shared.stream_error.swap(false, Ordering::Relaxed) || _stream.is_none() {
            if let Ok((r, ch, s)) = build_output_stream(&host, None, &shared) {
                out_rate = r;
                out_channels = ch;
                _stream = Some(s);
                shared.out_rate.store(out_rate as u64, Ordering::Relaxed);
                shared.out_channels.store(out_channels as u64, Ordering::Relaxed);
                max_ring_samples = (out_rate as usize) * out_channels / 2;
                let (preset, spatial_mode) = *shared.dsp_config.lock().expect("dsp_config");
                equalizer = Equalizer::new(out_rate, out_channels, preset);
                spatial = SpatialProcessor::new(out_rate, spatial_mode);
                tracing::info!("Audio device recovered on default endpoint: {out_rate}Hz, {out_channels}ch");
            } else if _stream.is_none() {
                std::thread::sleep(Duration::from_millis(60));
            }
        }

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
                    shared.reset_fade_for_new_track();
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
                    shared.trigger_pause();
                    *shared.status.lock().expect("status") = PlaybackStatus::Paused;
                }
                Command::Resume => {
                    shared.trigger_resume();
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
                Command::SetDevice(dev_name) => {
                    match build_output_stream(&host, dev_name.as_deref(), &shared) {
                        Ok((rate, channels, s)) => {
                            out_rate = rate;
                            out_channels = channels;
                            _stream = Some(s);
                            max_ring_samples = (out_rate as usize) * out_channels / 2;
                            shared.out_rate.store(out_rate as u64, Ordering::Relaxed);
                            shared.out_channels.store(out_channels as u64, Ordering::Relaxed);
                            let custom_bands = *shared.custom_eq_bands.lock().expect("custom_eq");
                            let (_, sp_mode) = *shared.dsp_config.lock().expect("dsp");
                            equalizer.configure_bands(out_rate, out_channels, custom_bands);
                            spatial.configure(out_rate, sp_mode);
                        }
                        Err(e) => {
                            tracing::warn!("Failed to switch output device to {dev_name:?}: {e}");
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
                        let speed = f64::from_bits(shared.playback_speed_bits.load(Ordering::Relaxed)) as f32;
                        let eff_src_rate = ((src_rate as f32) * speed.clamp(0.5, 2.0)).round().max(1000.0) as u32;
                        let mut resampled = resample_and_remix(
                            buf.samples(),
                            eff_src_rate,
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
                            let (_, sp_mode) = *shared.dsp_config.lock().expect("dsp");
                            let custom_bands = *shared.custom_eq_bands.lock().expect("custom_eq");
                            equalizer.configure_bands(out_rate, out_channels, custom_bands);
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
    let channels = shared.out_channels.load(Ordering::Relaxed).max(1) as usize;
    let rate = shared.out_rate.load(Ordering::Relaxed).max(8000) as f32;
    // 80ms smooth micro-fade ramp: step per frame
    let ramp_step = 1.0 / (rate * 0.080);

    let (mut current_gain, target_gain, is_paused) = {
        let fade = shared.pause_fade.lock().expect("pause_fade");
        (fade.current_gain, fade.target_gain, fade.is_paused)
    };

    if is_paused && current_gain <= 1e-4 {
        out.fill(0.0);
        decay_spectrum(shared);
        return;
    }

    let base_gain = shared.total_gain();
    let mut non_zero = false;
    let mut finished_pausing = false;

    {
        let mut ring = shared.ring.lock().expect("ring");
        let mut sample_idx = 0;
        while sample_idx < out.len() {
            if target_gain < current_gain {
                current_gain = (current_gain - ramp_step).max(target_gain);
                if current_gain <= 1e-4 {
                    current_gain = 0.0;
                    finished_pausing = true;
                }
            } else if target_gain > current_gain {
                current_gain = (current_gain + ramp_step).min(target_gain);
            }

            let effective_gain = base_gain * current_gain;

            for _ in 0..channels {
                if sample_idx >= out.len() {
                    break;
                }
                if finished_pausing {
                    out[sample_idx] = 0.0;
                } else if let Some(s) = ring.pop_front() {
                    let v = apply_gain_and_soft_limit(s, effective_gain);
                    out[sample_idx] = v;
                    if v.abs() > 1e-5 {
                        non_zero = true;
                    }
                } else {
                    out[sample_idx] = 0.0;
                }
                sample_idx += 1;
            }
        }
    }

    {
        let mut fade = shared.pause_fade.lock().expect("pause_fade");
        fade.current_gain = current_gain;
        if finished_pausing {
            fade.is_paused = true;
            shared.paused.store(true, Ordering::Relaxed);
        }
    }

    if non_zero {
        update_spectrum(shared, out);
    } else {
        decay_spectrum(shared);
    }
}

fn decay_spectrum(shared: &SharedState) {
    let prev = shared.spectrum_packed.load(Ordering::Relaxed);
    if prev == 0 {
        return;
    }
    let mut next_packed = 0u64;
    for i in 0..8 {
        let byte = ((prev >> (i * 8)) & 0xFF) as f32;
        let decayed = (byte * 0.78).floor().clamp(0.0, 255.0) as u64;
        next_packed |= decayed << (i * 8);
    }
    shared.spectrum_packed.store(next_packed, Ordering::Relaxed);
}

fn update_spectrum(shared: &SharedState, interleaved: &[f32]) {
    const BAND_FREQS: [f32; 8] = [65.0, 140.0, 300.0, 650.0, 1400.0, 2800.0, 5600.0, 10500.0];
    let channels = (shared.out_channels.load(Ordering::Relaxed) as usize).max(1);
    let sample_rate = (shared.out_rate.load(Ordering::Relaxed) as f32).max(8_000.0);
    let total_frames = interleaved.len() / channels;
    if total_frames < 16 {
        return;
    }

    let n = total_frames.min(256);
    let mut mono = [0.0f32; 256];
    let step = (total_frames / n).max(1);
    let inv_ch = 1.0 / channels as f32;
    let effective_sr = sample_rate / step as f32;

    for (idx, dst) in mono[..n].iter_mut().enumerate() {
        let frame_base = (idx * step) * channels;
        let mut sum = 0.0f32;
        for c in 0..channels {
            sum += interleaved[frame_base + c];
        }
        // Hann-like window reduces spectral leakage cleanly without allocations.
        let w = 0.5 * (1.0 - (2.0 * std::f32::consts::PI * idx as f32 / (n - 1) as f32).cos());
        *dst = sum * inv_ch * w;
    }

    let prev_packed = shared.spectrum_packed.load(Ordering::Relaxed);
    let mut next_packed = 0u64;
    for (b, &freq) in BAND_FREQS.iter().enumerate() {
        let prev = ((prev_packed >> (b * 8)) & 0xFF) as f32 / 255.0;
        let mag = if freq < effective_sr * 0.45 {
            let omega = 2.0 * std::f32::consts::PI * freq / effective_sr;
            let coeff = 2.0 * omega.cos();
            let mut s1 = 0.0f32;
            let mut s2 = 0.0f32;
            for &x in &mono[..n] {
                let s0 = x + coeff * s1 - s2;
                s2 = s1;
                s1 = s0;
            }
            let power = (s1 * s1 + s2 * s2 - coeff * s1 * s2).max(0.0);
            let norm = (power.sqrt() / (n as f32 * 0.22)).clamp(0.0, 1.0);
            // Gentle logarithmic compression so quiet highs and loud bass both look expressive.
            (norm * 2.2).ln_1p() / 2.2f32.ln_1p()
        } else {
            0.0
        };
        let smoothed = if mag > prev {
            prev * 0.40 + mag * 0.60
        } else {
            prev * 0.72 + mag * 0.28
        };
        let byte = (smoothed.clamp(0.0, 1.0) * 255.0).round() as u64;
        next_packed |= byte << (b * 8);
    }
    shared.spectrum_packed.store(next_packed, Ordering::Relaxed);
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

