use crate::format::{DecodedSignature, FrequencyPeak};
use rustfft::num_complex::Complex;
use rustfft::FftPlanner;
use std::sync::Arc;

pub const SIGNATURE_SAMPLE_RATE_HZ: u32 = 16_000;

const CHUNK: usize = 128;
const FFT_SIZE: usize = 2048;
const BINS: usize = FFT_SIZE / 2 + 1;
const RING: usize = 256;

const FREQUENCY_NEIGHBOURS: [isize; 8] = [-10, -7, -4, -3, 1, 2, 5, 8];
const TIME_NEIGHBOURS: [isize; 14] = [
    -53, -45, 165, 172, 179, 186, 193, 200, 214, 221, 228, 235, 242, 249,
];

const HANNING_BYTES: &[u8] = include_bytes!("hanning2048_reference.f32");

fn reference_hanning() -> [f32; FFT_SIZE] {
    let mut out = [0.0f32; FFT_SIZE];
    for (i, chunk) in HANNING_BYTES.chunks_exact(4).take(FFT_SIZE).enumerate() {
        out[i] = f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]);
    }
    out
}

pub struct SignatureGenerator;

impl SignatureGenerator {
    pub fn make_signature(samples: &[i16]) -> DecodedSignature {
        let window = reference_hanning();
        let mut state = State::new(window);
        let mut offset = 0usize;

        while offset + CHUNK <= samples.len() {
            state.do_fft(&samples[offset..offset + CHUNK]);
            state.do_peak_spreading();
            state.num_spread_ffts_done += 1;
            if state.num_spread_ffts_done >= 46 {
                state.do_peak_recognition();
            }
            offset += CHUNK;
        }

        DecodedSignature {
            sample_rate_hz: SIGNATURE_SAMPLE_RATE_HZ,
            number_samples: samples.len() as u32,
            peaks_by_band: state.peaks,
        }
    }
}

struct State {
    window: [f32; FFT_SIZE],
    ring_of_samples: [i16; FFT_SIZE],
    ring_of_samples_index: usize,
    fft: Arc<dyn rustfft::Fft<f32>>,
    fft_buf: Vec<Complex<f32>>,
    fft_outputs: Vec<[f32; BINS]>,
    fft_outputs_index: usize,
    spread_fft_outputs: Vec<[f32; BINS]>,
    spread_fft_outputs_index: usize,
    num_spread_ffts_done: u32,
    peaks: [Vec<FrequencyPeak>; 4],
}

impl State {
    fn new(window: [f32; FFT_SIZE]) -> Self {
        let mut planner = FftPlanner::<f32>::new();
        let fft = planner.plan_fft_forward(FFT_SIZE);
        Self {
            window,
            ring_of_samples: [0; FFT_SIZE],
            ring_of_samples_index: 0,
            fft,
            fft_buf: vec![Complex::new(0.0, 0.0); FFT_SIZE],
            fft_outputs: vec![[0.0; BINS]; RING],
            fft_outputs_index: 0,
            spread_fft_outputs: vec![[0.0; BINS]; RING],
            spread_fft_outputs_index: 0,
            num_spread_ffts_done: 0,
            peaks: Default::default(),
        }
    }

    fn do_fft(&mut self, chunk: &[i16]) {
        self.ring_of_samples[self.ring_of_samples_index..self.ring_of_samples_index + CHUNK]
            .copy_from_slice(chunk);
        self.ring_of_samples_index = (self.ring_of_samples_index + CHUNK) & (FFT_SIZE - 1);

        for i in 0..FFT_SIZE {
            let s = self.ring_of_samples[(i + self.ring_of_samples_index) & (FFT_SIZE - 1)];
            self.fft_buf[i] = Complex::new(s as f32 * self.window[i], 0.0);
        }

        self.fft.process(&mut self.fft_buf);

        let out = &mut self.fft_outputs[self.fft_outputs_index];
        for (bin, c) in self.fft_buf.iter().take(BINS).enumerate() {
            let mag_sq = c.re * c.re + c.im * c.im;
            out[bin] = (mag_sq / 131_072.0).max(0.000_000_000_1);
        }
        self.fft_outputs_index = (self.fft_outputs_index + 1) & (RING - 1);
    }

    fn do_peak_spreading(&mut self) {
        let source_idx = self.fft_outputs_index.wrapping_sub(1) & (RING - 1);
        let spread = &mut self.spread_fft_outputs[self.spread_fft_outputs_index];
        spread.copy_from_slice(&self.fft_outputs[source_idx]);

        for pos in 0..=1022 {
            spread[pos] = spread[pos].max(spread[pos + 1]).max(spread[pos + 2]);
        }

        let snapshot = *spread;
        for former in [1usize, 3, 6] {
            let target_idx = self.spread_fft_outputs_index.wrapping_sub(former) & (RING - 1);
            let target = &mut self.spread_fft_outputs[target_idx];
            for pos in 0..BINS {
                target[pos] = target[pos].max(snapshot[pos]);
            }
        }

        self.spread_fft_outputs_index = (self.spread_fft_outputs_index + 1) & (RING - 1);
    }

    fn do_peak_recognition(&mut self) {
        let fft_minus_46 = &self.fft_outputs[self.fft_outputs_index.wrapping_sub(46) & (RING - 1)];
        let fft_minus_49 =
            &self.spread_fft_outputs[self.spread_fft_outputs_index.wrapping_sub(49) & (RING - 1)];

        for bin in 10..=1014 {
            let magnitude = fft_minus_46[bin];
            if magnitude < (1.0 / 64.0) || magnitude < fft_minus_49[bin - 1] {
                continue;
            }

            let mut max_neighbour = 0.0f32;
            for offset in FREQUENCY_NEIGHBOURS {
                let idx = (bin as isize + offset) as usize;
                max_neighbour = max_neighbour.max(fft_minus_49[idx]);
            }
            if magnitude <= max_neighbour {
                continue;
            }

            let mut max_other = max_neighbour;
            for offset in TIME_NEIGHBOURS {
                let ring_idx = (self.spread_fft_outputs_index as isize + offset).rem_euclid(RING as isize)
                    as usize;
                max_other = max_other.max(self.spread_fft_outputs[ring_idx][bin - 1]);
            }
            if magnitude <= max_other {
                continue;
            }

            let here = magnitude.ln().max(1.0 / 64.0) * 1477.3 + 6144.0;
            let before = fft_minus_46[bin - 1].ln().max(1.0 / 64.0) * 1477.3 + 6144.0;
            let after = fft_minus_46[bin + 1].ln().max(1.0 / 64.0) * 1477.3 + 6144.0;

            let variation1 = here * 2.0 - before - after;
            if variation1 <= 0.0 {
                continue;
            }
            let variation2 = (after - before) * 32.0 / variation1;
            let corrected_bin = ((bin as i32 * 64) + (variation2 as i32)) as u32 & 0xFFFF;
            let frequency_hz = (corrected_bin as f32 * (16_000.0 / 2.0 / 1024.0 / 64.0)) as u32;

            let band = match frequency_hz {
                250..=519 => 0,
                520..=1449 => 1,
                1450..=3499 => 2,
                3500..=5500 => 3,
                _ => continue,
            };

            self.peaks[band].push(FrequencyPeak {
                fft_pass_number: self.num_spread_ffts_done - 46,
                peak_magnitude: (here as i32).clamp(0, 0xFFFF) as u16,
                corrected_peak_frequency_bin: corrected_bin as u16,
            });
        }
    }
}

