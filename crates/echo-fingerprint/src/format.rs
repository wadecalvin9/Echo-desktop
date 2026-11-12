use base64::engine::general_purpose::STANDARD;
use base64::Engine;
use crc32fast::Hasher as Crc32;
use thiserror::Error;

pub const DATA_URI_PREFIX: &str = "data:audio/vnd.shazam.sig;base64,";

const HEADER_SIZE: usize = 48;
const MAGIC1: u32 = 0xcafe_2580;
const MAGIC2: u32 = 0x9411_9c00;
const FIXED_VALUE: u32 = (15 << 19) + 0x40000;
const PEAK_CHUNK_TAG: u32 = 0x6003_0040;
const SECOND_BLOCK_MARKER: u32 = 0x4000_0000;

#[derive(Debug, Error)]
pub enum SignatureFormatError {
    #[error("invalid base64 payload: {0}")]
    Base64(#[from] base64::DecodeError),
    #[error("signature buffer too short")]
    TooShort,
    #[error("bad magic1 in signature header")]
    BadMagic1,
    #[error("bad magic2 in signature header")]
    BadMagic2,
    #[error("crc32 mismatch: expected {expected:#010x}, got {actual:#010x}")]
    CrcMismatch { expected: u32, actual: u32 },
    #[error("unsupported sample rate: {0}")]
    UnsupportedSampleRate(u32),
    #[error("unexpected chunk tag {0:#010x}")]
    UnexpectedChunkTag(u32),
    #[error("peaks must be ordered by fft pass within a band")]
    UnorderedPeaks,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FrequencyPeak {
    pub fft_pass_number: u32,
    pub peak_magnitude: u16,
    pub corrected_peak_frequency_bin: u16,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SampleRate {
    Hz8000 = 1,
    Hz11025 = 2,
    Hz16000 = 3,
    Hz32000 = 4,
    Hz44100 = 5,
    Hz48000 = 6,
}

impl SampleRate {
    pub fn hz(self) -> u32 {
        match self {
            Self::Hz8000 => 8000,
            Self::Hz11025 => 11025,
            Self::Hz16000 => 16000,
            Self::Hz32000 => 32000,
            Self::Hz44100 => 44100,
            Self::Hz48000 => 48000,
        }
    }

    pub fn of_hz(hz: u32) -> Result<Self, SignatureFormatError> {
        match hz {
            8000 => Ok(Self::Hz8000),
            11025 => Ok(Self::Hz11025),
            16000 => Ok(Self::Hz16000),
            32000 => Ok(Self::Hz32000),
            44100 => Ok(Self::Hz44100),
            48000 => Ok(Self::Hz48000),
            _ => Err(SignatureFormatError::UnsupportedSampleRate(hz)),
        }
    }

    pub fn of_id(id: u32) -> Result<Self, SignatureFormatError> {
        match id {
            1 => Ok(Self::Hz8000),
            2 => Ok(Self::Hz11025),
            3 => Ok(Self::Hz16000),
            4 => Ok(Self::Hz32000),
            5 => Ok(Self::Hz44100),
            6 => Ok(Self::Hz48000),
            _ => Err(SignatureFormatError::UnsupportedSampleRate(id)),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DecodedSignature {
    pub sample_rate_hz: u32,
    pub number_samples: u32,
    pub peaks_by_band: [Vec<FrequencyPeak>; 4],
}

impl DecodedSignature {
    pub fn total_peaks(&self) -> usize {
        self.peaks_by_band.iter().map(|b| b.len()).sum()
    }

    pub fn encode_to_uri(&self) -> Result<String, SignatureFormatError> {
        let bin = self.encode_to_binary()?;
        Ok(format!("{DATA_URI_PREFIX}{}", STANDARD.encode(bin)))
    }

    pub fn encode_to_binary(&self) -> Result<Vec<u8>, SignatureFormatError> {
        let rate = SampleRate::of_hz(self.sample_rate_hz)?;
        let mut chunk_bytes = Vec::new();

        for (band, peaks) in self.peaks_by_band.iter().enumerate() {
            if peaks.is_empty() {
                continue;
            }
            let mut body = Vec::with_capacity(peaks.len() * 5);
            let mut fft_pass_number: u32 = 0;

            for peak in peaks {
                if peak.fft_pass_number < fft_pass_number {
                    return Err(SignatureFormatError::UnorderedPeaks);
                }
                if peak.fft_pass_number - fft_pass_number >= 255 {
                    body.push(0xff);
                    body.extend_from_slice(&peak.fft_pass_number.to_le_bytes());
                    fft_pass_number = peak.fft_pass_number;
                }
                body.push((peak.fft_pass_number - fft_pass_number) as u8);
                body.extend_from_slice(&peak.peak_magnitude.to_le_bytes());
                body.extend_from_slice(&peak.corrected_peak_frequency_bin.to_le_bytes());
                fft_pass_number = peak.fft_pass_number;
            }

            chunk_bytes.extend_from_slice(&(PEAK_CHUNK_TAG + band as u32).to_le_bytes());
            chunk_bytes.extend_from_slice(&(body.len() as u32).to_le_bytes());
            let pad = (4 - (body.len() % 4)) % 4;
            chunk_bytes.extend_from_slice(&body);
            chunk_bytes.resize(chunk_bytes.len() + pad, 0);
        }

        let total = HEADER_SIZE + 8 + chunk_bytes.len();
        let size_minus_header = (total - HEADER_SIZE) as u32;

        let mut out = Vec::with_capacity(total);
        out.extend_from_slice(&MAGIC1.to_le_bytes());
        out.extend_from_slice(&0u32.to_le_bytes()); // crc32 placeholder
        out.extend_from_slice(&size_minus_header.to_le_bytes());
        out.extend_from_slice(&MAGIC2.to_le_bytes());
        out.extend_from_slice(&0u32.to_le_bytes());
        out.extend_from_slice(&0u32.to_le_bytes());
        out.extend_from_slice(&0u32.to_le_bytes());
        out.extend_from_slice(&((rate as u32) << 27).to_le_bytes());
        out.extend_from_slice(&0u32.to_le_bytes());
        out.extend_from_slice(&0u32.to_le_bytes());
        let adjusted_samples = self.number_samples + (self.sample_rate_hz as f32 * 0.24) as u32;
        out.extend_from_slice(&adjusted_samples.to_le_bytes());
        out.extend_from_slice(&FIXED_VALUE.to_le_bytes());
        out.extend_from_slice(&SECOND_BLOCK_MARKER.to_le_bytes());
        out.extend_from_slice(&size_minus_header.to_le_bytes());
        out.extend_from_slice(&chunk_bytes);

        let mut crc = Crc32::new();
        crc.update(&out[8..]);
        let crc_val = crc.finalize();
        out[4..8].copy_from_slice(&crc_val.to_le_bytes());

        Ok(out)
    }

    pub fn decode_from_uri(uri: &str) -> Result<Self, SignatureFormatError> {
        let b64 = uri.strip_prefix(DATA_URI_PREFIX).unwrap_or(uri);
        let bytes = STANDARD.decode(b64.trim())?;
        Self::decode_from_binary(&bytes)
    }

    pub fn decode_from_binary(bytes: &[u8]) -> Result<Self, SignatureFormatError> {
        if bytes.len() < HEADER_SIZE + 8 {
            return Err(SignatureFormatError::TooShort);
        }

        let read_u32 = |off: usize| -> Result<u32, SignatureFormatError> {
            bytes
                .get(off..off + 4)
                .map(|s| u32::from_le_bytes([s[0], s[1], s[2], s[3]]))
                .ok_or(SignatureFormatError::TooShort)
        };
        let read_u16 = |off: usize| -> Result<u16, SignatureFormatError> {
            bytes
                .get(off..off + 2)
                .map(|s| u16::from_le_bytes([s[0], s[1]]))
                .ok_or(SignatureFormatError::TooShort)
        };

        if read_u32(0)? != MAGIC1 {
            return Err(SignatureFormatError::BadMagic1);
        }
        if read_u32(12)? != MAGIC2 {
            return Err(SignatureFormatError::BadMagic2);
        }

        let stored_crc = read_u32(4)?;
        let mut crc = Crc32::new();
        crc.update(&bytes[8..]);
        let actual_crc = crc.finalize();
        if stored_crc != actual_crc {
            return Err(SignatureFormatError::CrcMismatch {
                expected: stored_crc,
                actual: actual_crc,
            });
        }

        let sample_rate = SampleRate::of_id(read_u32(28)? >> 27)?;
        let sample_rate_hz = sample_rate.hz();
        let number_samples = read_u32(40)? - (sample_rate_hz as f32 * 0.24) as u32;

        let mut bands: [Vec<FrequencyPeak>; 4] = Default::default();
        let mut pos = HEADER_SIZE + 8;

        while pos + 8 <= bytes.len() {
            let tag = read_u32(pos)?;
            let len = read_u32(pos + 4)? as usize;
            let band = tag.wrapping_sub(PEAK_CHUNK_TAG) as usize;
            if band >= 4 {
                return Err(SignatureFormatError::UnexpectedChunkTag(tag));
            }
            let mut p = pos + 8;
            let end = p + len;
            if end > bytes.len() {
                return Err(SignatureFormatError::TooShort);
            }
            let mut fft_pass_number: u32 = 0;
            while p < end {
                let delta = bytes[p];
                p += 1;
                if delta == 0xff {
                    fft_pass_number = read_u32(p)?;
                    p += 4;
                    continue;
                }
                fft_pass_number += delta as u32;
                let magnitude = read_u16(p)?;
                let bin = read_u16(p + 2)?;
                p += 4;
                bands[band].push(FrequencyPeak {
                    fft_pass_number,
                    peak_magnitude: magnitude,
                    corrected_peak_frequency_bin: bin,
                });
            }
            pos = end + ((4 - (len % 4)) % 4);
        }

        Ok(Self {
            sample_rate_hz,
            number_samples,
            peaks_by_band: bands,
        })
    }
}

