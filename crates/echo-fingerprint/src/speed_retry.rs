use crate::shazam::Recognised;

/// Guessed playback speeds relative to normal (for sped-up / slowed-down edits).
pub const SPEED_RETRY_RATES: [f64; 6] = [1.25, 0.8, 1.35, 0.85, 1.15, 0.75];

const MAX_SKEW: f64 = 0.01;
const NEAR_MISS: f64 = 0.08;

pub fn rates_for(miss: usize) -> [f64; 2] {
    [
        SPEED_RETRY_RATES[(2 * miss) % SPEED_RETRY_RATES.len()],
        SPEED_RETRY_RATES[(2 * miss + 1) % SPEED_RETRY_RATES.len()],
    ]
}

pub fn closer_rate(rate: f64, track: &Recognised) -> Option<f64> {
    let abs_t = track.time_skew.abs();
    if (MAX_SKEW..=NEAR_MISS).contains(&abs_t) {
        Some(rate * (1.0 + track.time_skew))
    } else {
        None
    }
}

pub fn fits(track: &Recognised) -> bool {
    track.time_skew.abs() < MAX_SKEW && track.frequency_skew.abs() < MAX_SKEW
}

/// Resamples 16 kHz PCM samples as if recorded at `rate` times original speed back to normal speed.
pub fn put_back(samples: &[i16], rate: f64) -> Vec<i16> {
    if samples.is_empty() || !rate.is_finite() || rate <= 0.0 {
        return Vec::new();
    }
    let len = samples.len().min((samples.len() as f64 * rate) as usize);
    let last = samples.len() - 1;
    let mut out = Vec::with_capacity(len);
    for j in 0..len {
        let at = j as f64 / rate;
        let i = (at as usize).min(last);
        let next = (i + 1).min(last);
        let f = at - i as f64;
        let v = samples[i] as f64 + (samples[next] as f64 - samples[i] as f64) * f;
        out.push(v.round().clamp(i16::MIN as f64, i16::MAX as f64) as i16);
    }
    out
}

