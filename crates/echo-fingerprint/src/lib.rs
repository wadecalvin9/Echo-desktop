pub mod capture;
pub mod format;
pub mod shazam;
pub mod signature;
pub mod speed_retry;

pub use capture::{capture_16khz_mono, resample_to_16k_i16, CaptureError};
pub use format::{DecodedSignature, FrequencyPeak, SampleRate, SignatureFormatError, DATA_URI_PREFIX};
pub use shazam::{Recognised, RecognitionOutcome, ShazamClient};
pub use signature::{SignatureGenerator, SIGNATURE_SAMPLE_RATE_HZ};
pub use speed_retry::{closer_rate, fits, put_back, rates_for, SPEED_RETRY_RATES};

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn read_pcm16_wav_mono(path: &Path) -> Vec<i16> {
        let bytes = std::fs::read(path).expect("read wav");
        // Find "data" chunk
        let mut pos = 12usize;
        while pos + 8 <= bytes.len() {
            let id = &bytes[pos..pos + 4];
            let len = u32::from_le_bytes([bytes[pos + 4], bytes[pos + 5], bytes[pos + 6], bytes[pos + 7]]) as usize;
            if id == b"data" {
                let data = &bytes[pos + 8..pos + 8 + len];
                return data
                    .chunks_exact(2)
                    .map(|c| i16::from_le_bytes([c[0], c[1]]))
                    .collect();
            }
            pos += 8 + len + (len % 2);
        }
        panic!("wav missing data chunk");
    }

    #[test]
    fn golden_signature_roundtrips_byte_for_byte() {
        let golden_path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../../Echo/fingerprint/src/test/resources/ss-s16le-1c-16khz.golden.txt");
        let uri = std::fs::read_to_string(golden_path)
            .expect("golden signature")
            .trim()
            .to_string();

        let decoded = DecodedSignature::decode_from_uri(&uri).expect("decode golden");
        assert_eq!(decoded.sample_rate_hz, 16_000);
        assert_eq!(decoded.number_samples, 192_000);
        let reencoded = decoded.encode_to_uri().expect("reencode");
        assert_eq!(uri, reencoded);
    }

    #[test]
    fn wav_matches_golden_signature_peaks() {
        let wav_path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../../Echo/fingerprint/src/test/resources/ss-s16le-1c-16khz.wav");
        let golden_path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../../Echo/fingerprint/src/test/resources/ss-s16le-1c-16khz.golden.txt");

        let wav = read_pcm16_wav_mono(&wav_path);
        // SongRec/Audile centres the 10s (160,000 sample) wav in a 12s (192,000 sample) window.
        let mut padded = vec![0i16; 192_000];
        let lead = (192_000 - wav.len()) / 2;
        padded[lead..lead + wav.len()].copy_from_slice(&wav);

        let generated = SignatureGenerator::make_signature(&padded);
        let golden = DecodedSignature::decode_from_uri(
            std::fs::read_to_string(golden_path).unwrap().trim(),
        )
        .unwrap();

        assert_eq!(generated.number_samples, golden.number_samples);
        assert_eq!(generated.sample_rate_hz, golden.sample_rate_hz);
        // Check peak counts are in the exact same ballpark and container encodes cleanly.
        let gen_total = generated.total_peaks();
        let gold_total = golden.total_peaks();
        assert!(
            (gen_total as isize - gold_total as isize).abs() <= 5,
            "generated {gen_total} peaks vs golden {gold_total}"
        );
    }

    #[test]
    fn silence_produces_no_peaks() {
        let sig = SignatureGenerator::make_signature(&vec![0i16; 32_000]);
        assert_eq!(sig.total_peaks(), 0);
    }

    #[test]
    fn speed_retry_skew_and_resample_behave_like_echo() {
        let samples = vec![1000i16; 1600];
        let slowed_back = put_back(&samples, 1.25);
        assert_eq!(slowed_back.len(), 1600);
        let sped_back = put_back(&samples, 0.8);
        assert_eq!(sped_back.len(), 1280);

        let good = Recognised {
            title: "Song".into(),
            artist: None,
            artwork_url: None,
            isrc: None,
            shazam_key: None,
            offset_seconds: 1.0,
            time_skew: 0.002,
            frequency_skew: -0.001,
        };
        assert!(fits(&good));
        assert_eq!(closer_rate(0.85, &good), None);

        let near = Recognised {
            time_skew: -0.037,
            ..good
        };
        assert!(!fits(&near));
        let refined = closer_rate(0.85, &near).unwrap();
        assert!((refined - 0.85 * 0.963).abs() < 1e-6);
    }
}

