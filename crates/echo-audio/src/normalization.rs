pub const NORMALIZATION_REFERENCE_DB: f32 = -3.0;
pub const UNKNOWN_LOUDNESS_DB: f32 = 10.0;

/// Mirrors Echo's `MusicService.kt` loudness normalization rules:
/// - When normalization is off: unity (1.0).
/// - When `loudness_db` is known (valid in -60.0..=40.0): attenuate down toward -3 dB reference, never boost (> 1.0).
/// - Local files without ReplayGain/R128 tags stay at unity.
/// - Unknown YouTube tracks assume `UNKNOWN_LOUDNESS_DB` (10 dB) so a missing figure never blasts loudly.
pub fn compute_normalization_gain(
    enabled: bool,
    loudness_db: Option<f32>,
    is_local: bool,
) -> f32 {
    if !enabled {
        return 1.0;
    }

    let valid_db = loudness_db.filter(|v| v.is_finite() && (-60.0..=40.0).contains(v));
    match valid_db {
        Some(db) => 10.0f32.powf(-(db - NORMALIZATION_REFERENCE_DB) / 20.0).min(1.0),
        None if is_local => 1.0,
        None => 10.0f32
            .powf(-(UNKNOWN_LOUDNESS_DB - NORMALIZATION_REFERENCE_DB) / 20.0)
            .min(1.0),
    }
}

/// Applies master volume (0.0..=4.0, up to +12 dB boost) * normalization gain,
/// rounding off peaks smoothly above 0.92 instead of hard clipping when boosting.
#[inline]
pub fn apply_gain_and_soft_limit(sample: f32, total_gain: f32) -> f32 {
    let x = sample * total_gain;
    let abs_x = x.abs();
    if abs_x <= 0.92 {
        x
    } else {
        let sign = x.signum();
        let excess = abs_x - 0.92;
        // Smooth saturation asymptotically approaching 1.0
        sign * (0.92 + 0.08 * (excess / 0.08).tanh())
    }
}

