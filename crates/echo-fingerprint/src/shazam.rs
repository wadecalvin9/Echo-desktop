use crate::signature::{SignatureGenerator, SIGNATURE_SAMPLE_RATE_HZ};
use serde_json::{json, Value};
use uuid::Uuid;

const ENDPOINT: &str = "https://amp.shazam.com/discovery/v5/en/US/android/-/tag/";
const USER_AGENT: &str = "Dalvik/2.1.0 (Linux; U; Android 13; Pixel 7 Build/TQ3A.230805.001)";
const MIN_USEFUL_PEAKS: usize = 30;

#[derive(Debug, Clone, PartialEq)]
pub struct Recognised {
    pub title: String,
    pub artist: Option<String>,
    pub artwork_url: Option<String>,
    pub isrc: Option<String>,
    pub shazam_key: Option<String>,
    pub offset_seconds: f64,
    pub time_skew: f64,
    pub frequency_skew: f64,
}

impl Recognised {
    pub fn search_query(&self) -> String {
        match &self.artist {
            Some(a) if !a.is_empty() => format!("{} {}", self.title, a),
            _ => self.title.clone(),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum RecognitionOutcome {
    Match(Recognised),
    NoMatch,
    Failed {
        reason: String,
        retry_after_secs: Option<u64>,
    },
}

pub struct ShazamClient {
    http: reqwest::Client,
}

impl Default for ShazamClient {
    fn default() -> Self {
        Self::new()
    }
}

impl ShazamClient {
    pub fn new() -> Self {
        let http = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(20))
            .build()
            .expect("reqwest client");
        Self { http }
    }

    pub async fn identify_with_retry(&self, samples: &[i16]) -> RecognitionOutcome {
        let primary = self.identify(samples).await;
        if !matches!(primary, RecognitionOutcome::NoMatch) {
            return primary;
        }
        for rate in crate::speed_retry::rates_for(0)
            .into_iter()
            .chain(crate::speed_retry::rates_for(1))
        {
            let adjusted = crate::speed_retry::put_back(samples, rate);
            match self.identify(&adjusted).await {
                RecognitionOutcome::Match(rec) if crate::speed_retry::fits(&rec) => {
                    return RecognitionOutcome::Match(rec);
                }
                RecognitionOutcome::Match(rec) => {
                    if let Some(better) = crate::speed_retry::closer_rate(rate, &rec) {
                        let refined = crate::speed_retry::put_back(samples, better);
                        if let RecognitionOutcome::Match(rec2) = self.identify(&refined).await {
                            if crate::speed_retry::fits(&rec2) {
                                return RecognitionOutcome::Match(rec2);
                            }
                        }
                    }
                }
                RecognitionOutcome::Failed {
                    retry_after_secs: Some(s),
                    reason,
                } => {
                    return RecognitionOutcome::Failed {
                        reason,
                        retry_after_secs: Some(s),
                    };
                }
                _ => {}
            }
        }
        RecognitionOutcome::NoMatch
    }

    pub async fn identify(&self, samples: &[i16]) -> RecognitionOutcome {
        let signature = SignatureGenerator::make_signature(samples);
        let peaks = signature.total_peaks();
        if peaks < MIN_USEFUL_PEAKS {
            return RecognitionOutcome::Failed {
                reason: "silence".to_string(),
                retry_after_secs: None,
            };
        }

        let uri = match signature.encode_to_uri() {
            Ok(u) => u,
            Err(e) => {
                return RecognitionOutcome::Failed {
                    reason: format!("encode error: {e}"),
                    retry_after_secs: None,
                }
            }
        };

        let timestamp = chrono::Utc::now().timestamp();
        let sample_ms = (samples.len() as u64 * 1000) / SIGNATURE_SAMPLE_RATE_HZ as u64;
        let payload = json!({
            "geolocation": {
                "altitude": 150.0,
                "latitude": 45.0,
                "longitude": 2.0
            },
            "signature": {
                "samplems": sample_ms,
                "timestamp": timestamp,
                "uri": uri
            },
            "timestamp": timestamp,
            "timezone": "UTC"
        });

        let uuid1 = Uuid::new_v4().to_string().to_uppercase();
        let uuid2 = Uuid::new_v4().to_string();
        let url = format!(
            "{ENDPOINT}{uuid1}/{uuid2}?sync=true&webv3=true&sampling=true&connected=&shazamapiversion=v3&sharehub=true&video=v3"
        );

        let resp = match self
            .http
            .post(url)
            .header("Content-Language", "en_US")
            .header("User-Agent", USER_AGENT)
            .json(&payload)
            .send()
            .await
        {
            Ok(r) => r,
            Err(e) => {
                return RecognitionOutcome::Failed {
                    reason: format!("network: {e}"),
                    retry_after_secs: None,
                }
            }
        };

        let status = resp.status();
        let retry_after_secs = resp
            .headers()
            .get(reqwest::header::RETRY_AFTER)
            .and_then(|v| v.to_str().ok())
            .and_then(|s| s.trim().parse::<u64>().ok());

        let text = match resp.text().await {
            Ok(t) => t,
            Err(e) => {
                return RecognitionOutcome::Failed {
                    reason: format!("read body: {e}"),
                    retry_after_secs,
                }
            }
        };

        if !status.is_success() {
            return RecognitionOutcome::Failed {
                reason: format!("http {}", status.as_u16()),
                retry_after_secs,
            };
        }

        parse_response(&text)
    }
}

pub fn parse_response(text: &str) -> RecognitionOutcome {
    let Ok(root) = serde_json::from_str::<Value>(text) else {
        return RecognitionOutcome::Failed {
            reason: "parse".to_string(),
            retry_after_secs: None,
        };
    };

    let Some(track) = root.get("track").and_then(|v| v.as_object()) else {
        return RecognitionOutcome::NoMatch;
    };
    let Some(title) = track.get("title").and_then(|v| v.as_str()).filter(|s| !s.is_empty()) else {
        return RecognitionOutcome::NoMatch;
    };

    let artist = track
        .get("subtitle")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .map(ToString::to_string);

    let images = track.get("images").and_then(|v| v.as_object());
    let artwork_url = images
        .and_then(|m| m.get("coverarthq").or_else(|| m.get("coverart")))
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .map(ToString::to_string);

    let isrc = track
        .get("isrc")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .map(ToString::to_string);
    let shazam_key = track
        .get("key")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .map(ToString::to_string);

    let first_match = root
        .get("matches")
        .and_then(|v| v.as_array())
        .and_then(|arr| arr.first())
        .and_then(|v| v.as_object());

    RecognitionOutcome::Match(Recognised {
        title: title.to_string(),
        artist,
        artwork_url,
        isrc,
        shazam_key,
        offset_seconds: first_match
            .and_then(|m| m.get("offset"))
            .and_then(|v| v.as_f64())
            .unwrap_or(0.0),
        time_skew: first_match
            .and_then(|m| m.get("timeskew"))
            .and_then(|v| v.as_f64())
            .unwrap_or(0.0),
        frequency_skew: first_match
            .and_then(|m| m.get("frequencyskew"))
            .and_then(|v| v.as_f64())
            .unwrap_or(0.0),
    })
}

