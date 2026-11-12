use crate::signature::SIGNATURE_SAMPLE_RATE_HZ;
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{SampleFormat, StreamConfig};
use echo_core::RecognitionSource;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum CaptureError {
    #[error("no audio capture device available")]
    NoDevice,
    #[error("unsupported device config: {0}")]
    Config(String),
    #[error("audio stream error: {0}")]
    Stream(String),
}

/// Captures `duration` of audio from either the default microphone or the default output's
/// WASAPI loopback on Windows, downmixed to mono and resampled to 16 kHz `i16`.
pub fn capture_16khz_mono(
    source: RecognitionSource,
    duration: Duration,
) -> Result<Vec<i16>, CaptureError> {
    let host = cpal::default_host();
    let (device, config) = match source {
        RecognitionSource::Microphone => {
            let dev = host.default_input_device().ok_or(CaptureError::NoDevice)?;
            let cfg = dev
                .default_input_config()
                .map_err(|e| CaptureError::Config(e.to_string()))?;
            (dev, cfg)
        }
        RecognitionSource::SystemLoopback => {
            // On Windows WASAPI, building an input stream on an output device captures loopback.
            let dev = host
                .default_output_device()
                .or_else(|| host.default_input_device())
                .ok_or(CaptureError::NoDevice)?;
            let cfg = dev
                .default_output_config()
                .map_err(|e| CaptureError::Config(e.to_string()))?;
            (dev, cfg)
        }
    };

    let channels = config.channels() as usize;
    let src_rate = config.sample_rate().0;
    let sample_format = config.sample_format();
    let stream_cfg: StreamConfig = config.into();

    let buf: Arc<Mutex<Vec<f32>>> = Arc::new(Mutex::new(Vec::with_capacity(
        src_rate as usize * duration.as_secs().max(1) as usize,
    )));
    let buf_clone = Arc::clone(&buf);

    let err_fn = |err| {
        eprintln!("cpal capture stream error: {err}");
    };

    let stream = match sample_format {
        SampleFormat::F32 => device
            .build_input_stream(
                &stream_cfg,
                move |data: &[f32], _| push_mono_f32(&buf_clone, data, channels),
                err_fn,
                None,
            )
            .map_err(|e| CaptureError::Stream(e.to_string()))?,
        SampleFormat::I16 => device
            .build_input_stream(
                &stream_cfg,
                move |data: &[i16], _| {
                    let f: Vec<f32> = data.iter().map(|&s| s as f32 / 32768.0).collect();
                    push_mono_f32(&buf_clone, &f, channels);
                },
                err_fn,
                None,
            )
            .map_err(|e| CaptureError::Stream(e.to_string()))?,
        SampleFormat::U16 => device
            .build_input_stream(
                &stream_cfg,
                move |data: &[u16], _| {
                    let f: Vec<f32> = data
                        .iter()
                        .map(|&s| (s as f32 - 32768.0) / 32768.0)
                        .collect();
                    push_mono_f32(&buf_clone, &f, channels);
                },
                err_fn,
                None,
            )
            .map_err(|e| CaptureError::Stream(e.to_string()))?,
        other => {
            return Err(CaptureError::Config(format!(
                "unsupported sample format: {other:?}"
            )));
        }
    };

    stream
        .play()
        .map_err(|e| CaptureError::Stream(e.to_string()))?;
    std::thread::sleep(duration);
    drop(stream);

    let mono_src = buf.lock().expect("capture mutex").clone();
    Ok(resample_to_16k_i16(&mono_src, src_rate))
}

fn push_mono_f32(dest: &Mutex<Vec<f32>>, interleaved: &[f32], channels: usize) {
    if channels == 0 {
        return;
    }
    let mut guard = dest.lock().expect("capture mutex");
    for frame in interleaved.chunks(channels) {
        let sum: f32 = frame.iter().copied().sum();
        guard.push(sum / frame.len() as f32);
    }
}

pub fn resample_to_16k_i16(mono: &[f32], src_rate: u32) -> Vec<i16> {
    if mono.is_empty() || src_rate == 0 {
        return Vec::new();
    }
    if src_rate == SIGNATURE_SAMPLE_RATE_HZ {
        return mono
            .iter()
            .map(|&s| (s.clamp(-1.0, 1.0) * 32767.0).round() as i16)
            .collect();
    }
    let out_len = ((mono.len() as f64) * SIGNATURE_SAMPLE_RATE_HZ as f64 / src_rate as f64)
        .floor() as usize;
    let ratio = src_rate as f64 / SIGNATURE_SAMPLE_RATE_HZ as f64;
    let mut out = Vec::with_capacity(out_len);
    for i in 0..out_len {
        let pos = i as f64 * ratio;
        let idx = pos.floor() as usize;
        let frac = (pos - idx as f64) as f32;
        let s0 = mono[idx];
        let s1 = mono.get(idx + 1).copied().unwrap_or(s0);
        let interp = s0 + (s1 - s0) * frac;
        out.push((interp.clamp(-1.0, 1.0) * 32767.0).round() as i16);
    }
    out
}

