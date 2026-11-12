use echo_core::{EqPreset, SpatialMode};
use std::f32::consts::PI;

pub const EQ_BANDS_HZ: [f32; 10] = [
    31.25, 62.5, 125.0, 250.0, 500.0, 1000.0, 2000.0, 4000.0, 8000.0, 16000.0,
];

#[derive(Debug, Clone, Copy)]
struct BiquadCoeffs {
    b0: f32,
    b1: f32,
    b2: f32,
    a1: f32,
    a2: f32,
}

impl BiquadCoeffs {
    fn bypass() -> Self {
        Self {
            b0: 1.0,
            b1: 0.0,
            b2: 0.0,
            a1: 0.0,
            a2: 0.0,
        }
    }

    fn peaking(sample_rate: f32, freq_hz: f32, q: f32, gain_db: f32) -> Self {
        if gain_db.abs() < 0.05 || freq_hz >= sample_rate * 0.45 {
            return Self::bypass();
        }
        let a = 10.0f32.powf(gain_db / 40.0);
        let w0 = 2.0 * PI * freq_hz / sample_rate;
        let cos_w0 = w0.cos();
        let alpha = w0.sin() / (2.0 * q);

        let b0 = 1.0 + alpha * a;
        let b1 = -2.0 * cos_w0;
        let b2 = 1.0 - alpha * a;
        let a0 = 1.0 + alpha / a;
        let a1 = -2.0 * cos_w0;
        let a2 = 1.0 - alpha / a;

        Self {
            b0: b0 / a0,
            b1: b1 / a0,
            b2: b2 / a0,
            a1: a1 / a0,
            a2: a2 / a0,
        }
    }
}

#[derive(Debug, Clone, Copy, Default)]
struct BiquadState {
    x1: f32,
    x2: f32,
    y1: f32,
    y2: f32,
}

impl BiquadState {
    #[inline]
    fn process(&mut self, x: f32, c: BiquadCoeffs) -> f32 {
        let y = c.b0 * x + c.b1 * self.x1 + c.b2 * self.x2 - c.a1 * self.y1 - c.a2 * self.y2;
        self.x2 = self.x1;
        self.x1 = x;
        self.y2 = self.y1;
        self.y1 = y;
        y
    }
}

pub struct Equalizer {
    coeffs: [BiquadCoeffs; 10],
    states: Vec<[BiquadState; 10]>,
    active: bool,
}

impl Equalizer {
    pub fn new(sample_rate: u32, channels: usize, preset: EqPreset) -> Self {
        let mut eq = Self {
            coeffs: [BiquadCoeffs::bypass(); 10],
            states: vec![[BiquadState::default(); 10]; channels.max(1)],
            active: false,
        };
        eq.configure(sample_rate, channels, preset);
        eq
    }

    pub fn configure_bands(&mut self, sample_rate: u32, channels: usize, gains: [f32; 10]) {
        self.active = gains.iter().any(|g| g.abs() >= 0.05);
        if self.states.len() != channels.max(1) {
            self.states = vec![[BiquadState::default(); 10]; channels.max(1)];
        }
        let sr = sample_rate.max(8_000) as f32;
        for (i, &freq) in EQ_BANDS_HZ.iter().enumerate() {
            self.coeffs[i] = BiquadCoeffs::peaking(sr, freq, 1.41, gains[i]);
        }
    }

    pub fn configure(&mut self, sample_rate: u32, channels: usize, preset: EqPreset) {
        self.configure_bands(sample_rate, channels, preset.gains_db());
    }

    pub fn reset(&mut self) {
        for ch in &mut self.states {
            *ch = [BiquadState::default(); 10];
        }
    }

    pub fn process_interleaved(&mut self, samples: &mut [f32], channels: usize) {
        if !self.active || channels == 0 {
            return;
        }
        if self.states.len() != channels {
            self.states = vec![[BiquadState::default(); 10]; channels];
        }
        for frame in samples.chunks_exact_mut(channels) {
            for (ch_idx, s) in frame.iter_mut().enumerate() {
                let mut v = *s;
                let band_states = &mut self.states[ch_idx];
                for b in 0..10 {
                    v = band_states[b].process(v, self.coeffs[b]);
                }
                *s = v;
            }
        }
    }
}

/// Spatial headphone crossfeed & surround stereo widener mirroring Echo's stage-width
/// headphones renderer and surround upmix.
pub struct SpatialProcessor {
    mode: SpatialMode,
    delay_l: Vec<f32>,
    delay_r: Vec<f32>,
    write_pos: usize,
    lp_l: f32,
    lp_r: f32,
}

impl SpatialProcessor {
    pub fn new(sample_rate: u32, mode: SpatialMode) -> Self {
        let delay_len = ((sample_rate.max(8_000) as f32 * 0.00032).round() as usize).clamp(4, 64);
        Self {
            mode,
            delay_l: vec![0.0; delay_len],
            delay_r: vec![0.0; delay_len],
            write_pos: 0,
            lp_l: 0.0,
            lp_r: 0.0,
        }
    }

    pub fn configure(&mut self, sample_rate: u32, mode: SpatialMode) {
        self.mode = mode;
        let delay_len = ((sample_rate.max(8_000) as f32 * 0.00032).round() as usize).clamp(4, 64);
        if self.delay_l.len() != delay_len {
            self.delay_l = vec![0.0; delay_len];
            self.delay_r = vec![0.0; delay_len];
            self.write_pos = 0;
        }
    }

    pub fn reset(&mut self) {
        self.delay_l.fill(0.0);
        self.delay_r.fill(0.0);
        self.write_pos = 0;
        self.lp_l = 0.0;
        self.lp_r = 0.0;
    }

    pub fn process_interleaved(&mut self, samples: &mut [f32], channels: usize) {
        if channels < 2 || self.mode == SpatialMode::Off {
            return;
        }
        let len = self.delay_l.len();
        for frame in samples.chunks_exact_mut(channels) {
            let l = frame[0];
            let r = frame[1];
            match self.mode {
                SpatialMode::Off => {}
                SpatialMode::Headphones => {
                    // Interaural time delay (~0.32ms) + head-shadowed low-pass crossfeed
                    // with subtle stereo stage enhancement (30° virtual speaker width).
                    let delayed_r = self.delay_r[self.write_pos];
                    let delayed_l = self.delay_l[self.write_pos];
                    self.delay_l[self.write_pos] = l;
                    self.delay_r[self.write_pos] = r;
                    self.write_pos = (self.write_pos + 1) % len;

                    self.lp_l = self.lp_l * 0.72 + delayed_r * 0.28;
                    self.lp_r = self.lp_r * 0.72 + delayed_l * 0.28;

                    let mid = 0.5 * (l + r);
                    let side = 0.5 * (l - r) * 1.18;
                    frame[0] = (mid + side) * 0.86 + self.lp_l * 0.24;
                    frame[1] = (mid - side) * 0.86 + self.lp_r * 0.24;
                }
                SpatialMode::SurroundUpmix => {
                    // Wider M/S soundstage + light ambient reflection delay.
                    let delayed_l = self.delay_l[self.write_pos];
                    let delayed_r = self.delay_r[self.write_pos];
                    self.delay_l[self.write_pos] = l;
                    self.delay_r[self.write_pos] = r;
                    self.write_pos = (self.write_pos + 1) % len;

                    let mid = 0.5 * (l + r);
                    let side = 0.5 * (l - r) * 1.38;
                    frame[0] = (mid + side) * 0.88 + delayed_r * 0.10;
                    frame[1] = (mid - side) * 0.88 + delayed_l * 0.10;
                }
            }
        }
    }
}

/// Returns true if an interleaved audio buffer's RMS falls below the silence gate threshold (~-48 dBFS).
pub fn is_silent_packet(samples: &[f32]) -> bool {
    if samples.is_empty() {
        return true;
    }
    let sum_sq: f32 = samples.iter().map(|s| s * s).sum();
    let rms = (sum_sq / samples.len() as f32).sqrt();
    rms < 0.004
}

