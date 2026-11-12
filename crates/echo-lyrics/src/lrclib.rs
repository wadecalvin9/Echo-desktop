use crate::lrc::{parse_lrc, LyricsResult, LyricsSource};
use serde::Deserialize;
use thiserror::Error;

const BASE_URL: &str = "https://lrclib.net";

#[derive(Debug, Error)]
pub enum LrcLibError {
    #[error("http error: {0}")]
    Http(#[from] reqwest::Error),
    #[error("no matching lyrics found")]
    NotFound,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LrcLibTrack {
    pub id: i64,
    pub track_name: String,
    pub artist_name: String,
    pub duration: Option<f64>,
    pub plain_lyrics: Option<String>,
    pub synced_lyrics: Option<String>,
    pub album_name: Option<String>,
    #[serde(default)]
    pub instrumental: bool,
}

pub struct LrcLibClient {
    http: reqwest::Client,
}

impl Default for LrcLibClient {
    fn default() -> Self {
        Self::new()
    }
}

impl LrcLibClient {
    pub fn new() -> Self {
        let http = reqwest::Client::builder()
            .user_agent("EchoDesktop/0.1 (https://github.com/wadecalvin9/echo)")
            .timeout(std::time::Duration::from_secs(12))
            .build()
            .expect("reqwest client");
        Self { http }
    }

    pub async fn get_exact(
        &self,
        title: &str,
        artist: &str,
        duration_secs: u32,
    ) -> Result<Option<LrcLibTrack>, LrcLibError> {
        let resp = self
            .http
            .get(format!("{BASE_URL}/api/get"))
            .query(&[
                ("track_name", title),
                ("artist_name", artist),
                ("duration", &duration_secs.to_string()),
            ])
            .send()
            .await?;
        if resp.status() == reqwest::StatusCode::NOT_FOUND {
            return Ok(None);
        }
        let track = resp.error_for_status()?.json::<LrcLibTrack>().await?;
        Ok(Some(track))
    }

    pub async fn search(
        &self,
        title: &str,
        artist: &str,
        album: Option<&str>,
    ) -> Result<Vec<LrcLibTrack>, LrcLibError> {
        let mut req = self
            .http
            .get(format!("{BASE_URL}/api/search"))
            .query(&[("track_name", title), ("artist_name", artist)]);
        if let Some(a) = album.filter(|s| !s.is_empty()) {
            req = req.query(&[("album_name", a)]);
        }
        let tracks = req.send().await?.error_for_status()?.json::<Vec<LrcLibTrack>>().await?;
        Ok(tracks)
    }

    pub async fn fetch_lyrics(
        &self,
        title: &str,
        artist: &str,
        duration_secs: Option<u32>,
        album: Option<&str>,
    ) -> Result<LyricsResult, LrcLibError> {
        if let Some(dur) = duration_secs {
            if let Some(exact) = self.get_exact(title, artist, dur).await? {
                if let Some(res) = track_to_result(&exact) {
                    return Ok(res);
                }
            }
        }

        let candidates = self.search(title, artist, album).await?;
        let best = if let Some(dur) = duration_secs {
            candidates
                .iter()
                .find(|t| {
                    t.synced_lyrics.is_some()
                        && t.duration
                            .map(|d| (d.round() as i64 - dur as i64).abs() <= 2)
                            .unwrap_or(false)
                })
                .or_else(|| candidates.iter().find(|t| t.synced_lyrics.is_some()))
                .or_else(|| {
                    candidates.iter().find(|t| {
                        t.plain_lyrics.is_some()
                            && t.duration
                                .map(|d| (d.round() as i64 - dur as i64).abs() <= 2)
                                .unwrap_or(false)
                    })
                })
        } else {
            candidates
                .iter()
                .find(|t| t.synced_lyrics.is_some())
                .or_else(|| candidates.iter().find(|t| t.plain_lyrics.is_some()))
        };

        best.and_then(track_to_result)
            .ok_or(LrcLibError::NotFound)
    }
}

fn track_to_result(track: &LrcLibTrack) -> Option<LyricsResult> {
    if track.instrumental {
        return None;
    }
    let synced = track
        .synced_lyrics
        .as_deref()
        .map(parse_lrc)
        .unwrap_or_default();
    let plain = track.plain_lyrics.clone().filter(|s| !s.trim().is_empty());
    if synced.is_empty() && plain.is_none() {
        return None;
    }
    Some(LyricsResult {
        source: LyricsSource::LrcLib,
        synced,
        plain,
    })
}

