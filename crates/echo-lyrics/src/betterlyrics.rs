use crate::lrc::{LyricsLine, LyricsResult, LyricsSource};
use regex::Regex;
use serde::Deserialize;
use std::sync::OnceLock;
use std::time::Duration;

const BASE_URL: &str = "https://lyrics-api.boidu.dev/getLyrics";

#[derive(Debug, Deserialize)]
struct BetterLyricsResponse {
    ttml: Option<String>,
}

pub struct BetterLyricsClient {
    http: reqwest::Client,
}

impl Default for BetterLyricsClient {
    fn default() -> Self {
        Self::new()
    }
}

impl BetterLyricsClient {
    pub fn new() -> Self {
        let http = reqwest::Client::builder()
            .timeout(Duration::from_secs(10))
            .build()
            .expect("reqwest client");
        Self { http }
    }

    pub async fn fetch_lyrics(
        &self,
        title: &str,
        artist: &str,
        duration_secs: Option<u32>,
        album: Option<&str>,
    ) -> Option<LyricsResult> {
        let mut url = format!(
            "{BASE_URL}?s={}&a={}",
            urlencoding::encode(title.trim()),
            urlencoding::encode(artist.trim())
        );
        if let Some(d) = duration_secs.filter(|&d| d > 0) {
            url.push_str(&format!("&d={d}"));
        }
        if let Some(alb) = album.filter(|a| !a.trim().is_empty()) {
            url.push_str(&format!("&al={}", urlencoding::encode(alb.trim())));
        }

        let resp = self.http.get(url).send().await.ok()?;
        if !resp.status().is_success() {
            return None;
        }
        let body = resp.text().await.ok()?;
        let ttml = if let Ok(json) = serde_json::from_str::<BetterLyricsResponse>(&body) {
            json.ttml?
        } else if body.contains("<tt") || body.contains("<p ") {
            body
        } else {
            return None;
        };

        let synced = parse_ttml(&ttml);
        if synced.is_empty() {
            None
        } else {
            Some(LyricsResult {
                source: LyricsSource::BetterLyrics,
                synced,
                plain: None,
            })
        }
    }
}

static TTML_P_RE: OnceLock<Regex> = OnceLock::new();
static TTML_BEGIN_RE: OnceLock<Regex> = OnceLock::new();
static XML_TAG_RE: OnceLock<Regex> = OnceLock::new();

pub fn parse_ttml(raw: &str) -> Vec<LyricsLine> {
    let p_re = TTML_P_RE
        .get_or_init(|| Regex::new(r"(?is)<p\b([^>]*)>(.*?)</p>").expect("valid p regex"));
    let begin_re = TTML_BEGIN_RE
        .get_or_init(|| Regex::new(r#"(?i)\bbegin\s*=\s*["']([^"']+)["']"#).expect("begin regex"));
    let tag_re = XML_TAG_RE.get_or_init(|| Regex::new(r"<[^>]+>").expect("tag regex"));

    let mut lines = Vec::new();
    for cap in p_re.captures_iter(raw) {
        let attrs = &cap[1];
        let inner = &cap[2];
        let Some(begin_cap) = begin_re.captures(attrs) else {
            continue;
        };
        let Some(time_ms) = parse_ttml_time(&begin_cap[1]) else {
            continue;
        };
        let stripped = tag_re.replace_all(inner, "");
        let cleaned = decode_xml_entities(&stripped)
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ");
        if !cleaned.is_empty() {
            lines.push(LyricsLine {
                time_ms,
                text: cleaned,
            });
        }
    }
    lines.sort_by_key(|l| l.time_ms);
    lines
}

fn parse_ttml_time(s: &str) -> Option<u64> {
    let t = s.trim();
    if let Some(secs_str) = t.strip_suffix('s') {
        let secs: f64 = secs_str.parse().ok()?;
        return Some((secs * 1000.0).round() as u64);
    }
    let parts: Vec<&str> = t.split(':').collect();
    match parts.as_slice() {
        [m, s] => {
            let mins: u64 = m.parse().ok()?;
            let secs: f64 = s.parse().ok()?;
            Some(mins * 60_000 + (secs * 1000.0).round() as u64)
        }
        [h, m, s] => {
            let hrs: u64 = h.parse().ok()?;
            let mins: u64 = m.parse().ok()?;
            let secs: f64 = s.parse().ok()?;
            Some(hrs * 3_600_000 + mins * 60_000 + (secs * 1000.0).round() as u64)
        }
        _ => None,
    }
}

fn decode_xml_entities(s: &str) -> String {
    s.replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
        .replace("&#39;", "'")
}

