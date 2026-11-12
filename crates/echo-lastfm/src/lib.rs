use md5::{Digest, Md5};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use thiserror::Error;

const ROOT: &str = "https://ws.audioscrobbler.com/2.0/";

#[derive(Debug, Error)]
pub enum LastFmError {
    #[error("last.fm api key/secret not configured")]
    NotConfigured,
    #[error("http error: {0}")]
    Http(#[from] reqwest::Error),
    #[error("last.fm error {code}: {message}")]
    Api { code: i32, message: String },
    #[error("unexpected empty response from last.fm")]
    EmptyResponse,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Session {
    pub name: String,
    pub key: String,
    #[serde(default)]
    pub subscriber: i32,
}

#[derive(Debug, Clone, Deserialize)]
struct TokenResponse {
    token: Option<String>,
    error: Option<i32>,
    message: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct SessionResponse {
    session: Option<Session>,
    error: Option<i32>,
    message: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct ApiResult {
    error: Option<i32>,
    message: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct SimilarResponse {
    similartracks: Option<SimilarTracks>,
    error: Option<i32>,
    message: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct SimilarTracks {
    track: Option<Vec<SimilarTrack>>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct SimilarTrack {
    pub name: String,
    pub artist: Option<SimilarArtist>,
    #[serde(default, deserialize_with = "de_opt_f64_str_or_num")]
    pub match_score: Option<f64>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct SimilarArtist {
    pub name: Option<String>,
}

fn de_opt_f64_str_or_num<'de, D>(deserializer: D) -> Result<Option<f64>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let val: Option<serde_json::Value> = Option::deserialize(deserializer)?;
    Ok(match val {
        Some(serde_json::Value::Number(n)) => n.as_f64(),
        Some(serde_json::Value::String(s)) => s.parse::<f64>().ok(),
        _ => None,
    })
}

pub struct LastFmClient {
    api_key: String,
    api_secret: String,
    http: reqwest::Client,
}

impl LastFmClient {
    pub fn new(api_key: impl Into<String>, api_secret: impl Into<String>) -> Self {
        let http = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(15))
            .build()
            .expect("reqwest client");
        Self {
            api_key: api_key.into(),
            api_secret: api_secret.into(),
            http,
        }
    }

    pub fn is_configured(&self) -> bool {
        !self.api_key.trim().is_empty() && !self.api_secret.trim().is_empty()
    }

    pub async fn request_token(&self) -> Result<String, LastFmError> {
        if !self.is_configured() {
            return Err(LastFmError::NotConfigured);
        }
        let body: TokenResponse = self
            .http
            .get(ROOT)
            .query(&[
                ("method", "auth.getToken"),
                ("api_key", self.api_key.as_str()),
                ("format", "json"),
            ])
            .send()
            .await?
            .json()
            .await?;

        if let Some(code) = body.error {
            return Err(LastFmError::Api {
                code,
                message: body.message.unwrap_or_else(|| "no message".to_string()),
            });
        }
        body.token.ok_or(LastFmError::EmptyResponse)
    }

    pub fn authorize_url(&self, token: &str) -> String {
        format!(
            "https://www.last.fm/api/auth/?api_key={}&token={}",
            self.api_key, token
        )
    }

    pub async fn session(&self, token: &str) -> Result<Session, LastFmError> {
        if !self.is_configured() {
            return Err(LastFmError::NotConfigured);
        }
        let mut params = BTreeMap::new();
        params.insert("api_key", self.api_key.as_str());
        params.insert("method", "auth.getSession");
        params.insert("token", token);
        let sig = sign_params(&params, &self.api_secret);

        let body: SessionResponse = self
            .http
            .get(ROOT)
            .query(&[
                ("api_key", self.api_key.as_str()),
                ("method", "auth.getSession"),
                ("token", token),
                ("api_sig", sig.as_str()),
                ("format", "json"),
            ])
            .send()
            .await?
            .json()
            .await?;

        if let Some(code) = body.error {
            return Err(LastFmError::Api {
                code,
                message: body.message.unwrap_or_else(|| "no message".to_string()),
            });
        }
        body.session.ok_or(LastFmError::EmptyResponse)
    }

    pub async fn update_now_playing(
        &self,
        session_key: &str,
        artist: &str,
        track: &str,
        album: Option<&str>,
        duration_secs: Option<u32>,
    ) -> Result<(), LastFmError> {
        let mut fields = BTreeMap::new();
        fields.insert("artist".to_string(), artist.to_string());
        fields.insert("track".to_string(), track.to_string());
        if let Some(a) = album.filter(|s| !s.is_empty()) {
            fields.insert("album".to_string(), a.to_string());
        }
        if let Some(d) = duration_secs.filter(|&d| d > 0) {
            fields.insert("duration".to_string(), d.to_string());
        }
        self.signed_post("track.updateNowPlaying", session_key, fields)
            .await
    }

    pub async fn scrobble(
        &self,
        session_key: &str,
        artist: &str,
        track: &str,
        started_at_secs: i64,
        album: Option<&str>,
        duration_secs: Option<u32>,
    ) -> Result<(), LastFmError> {
        let mut fields = BTreeMap::new();
        fields.insert("artist".to_string(), artist.to_string());
        fields.insert("track".to_string(), track.to_string());
        fields.insert("timestamp".to_string(), started_at_secs.to_string());
        if let Some(a) = album.filter(|s| !s.is_empty()) {
            fields.insert("album".to_string(), a.to_string());
        }
        if let Some(d) = duration_secs.filter(|&d| d > 0) {
            fields.insert("duration".to_string(), d.to_string());
        }
        self.signed_post("track.scrobble", session_key, fields).await
    }

    pub async fn similar(
        &self,
        artist: &str,
        track: &str,
        limit: u32,
    ) -> Result<Vec<SimilarTrack>, LastFmError> {
        if !self.is_configured() {
            return Err(LastFmError::NotConfigured);
        }
        if artist.trim().is_empty() || track.trim().is_empty() {
            return Ok(Vec::new());
        }
        let limit_str = limit.to_string();
        let body: SimilarResponse = self
            .http
            .get(ROOT)
            .query(&[
                ("method", "track.getSimilar"),
                ("artist", artist),
                ("track", track),
                ("limit", limit_str.as_str()),
                ("autocorrect", "1"),
                ("api_key", self.api_key.as_str()),
                ("format", "json"),
            ])
            .send()
            .await?
            .json()
            .await?;

        if let Some(code) = body.error {
            return Err(LastFmError::Api {
                code,
                message: body.message.unwrap_or_else(|| "no message".to_string()),
            });
        }
        Ok(body.similartracks.and_then(|s| s.track).unwrap_or_default())
    }

    async fn signed_post(
        &self,
        method: &str,
        session_key: &str,
        mut fields: BTreeMap<String, String>,
    ) -> Result<(), LastFmError> {
        if !self.is_configured() {
            return Err(LastFmError::NotConfigured);
        }
        fields.insert("method".to_string(), method.to_string());
        fields.insert("api_key".to_string(), self.api_key.clone());
        fields.insert("sk".to_string(), session_key.to_string());

        let borrowed: BTreeMap<&str, &str> = fields
            .iter()
            .map(|(k, v)| (k.as_str(), v.as_str()))
            .collect();
        let sig = sign_params(&borrowed, &self.api_secret);

        fields.insert("api_sig".to_string(), sig);
        fields.insert("format".to_string(), "json".to_string());

        let body: ApiResult = self
            .http
            .post(ROOT)
            .form(&fields)
            .send()
            .await?
            .json()
            .await?;
        if let Some(code) = body.error {
            return Err(LastFmError::Api {
                code,
                message: body.message.unwrap_or_else(|| "no message".to_string()),
            });
        }
        Ok(())
    }
}

/// Last.fm scrobble qualification rule: at least half the track or 4 minutes, and >= 30s long.
pub fn qualifies_for_scrobble(played_ms: u64, duration_secs: u32) -> bool {
    if duration_secs < 30 {
        return false;
    }
    let target_ms = ((duration_secs as u64) * 500).min(4 * 60 * 1000);
    played_ms >= target_ms
}

pub fn sign_params(params: &BTreeMap<&str, &str>, api_secret: &str) -> String {
    let mut joined = String::new();
    for (k, v) in params {
        joined.push_str(k);
        joined.push_str(v);
    }
    joined.push_str(api_secret);

    let mut hasher = Md5::new();
    hasher.update(joined.as_bytes());
    let digest = hasher.finalize();
    digest.iter().map(|b| format!("{b:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scrobble_qualification_matches_lastfm_rules() {
        assert!(!qualifies_for_scrobble(25_000, 29));
        assert!(qualifies_for_scrobble(15_000, 30));
        assert!(!qualifies_for_scrobble(14_999, 30));
        assert!(qualifies_for_scrobble(240_000, 600));
    }

    #[test]
    fn signature_sorts_keys_lexicographically() {
        let mut params = BTreeMap::new();
        params.insert("token", "tok");
        params.insert("method", "auth.getSession");
        params.insert("api_key", "key");
        let sig = sign_params(&params, "secret");
        let expected_input = "api_keykeymethodauth.getSessiontokentoksecret";
        let mut hasher = Md5::new();
        hasher.update(expected_input.as_bytes());
        let expected: String = hasher.finalize().iter().map(|b| format!("{b:02x}")).collect();
        assert_eq!(sig, expected);
    }
}

