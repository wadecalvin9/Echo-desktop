use crate::lrc::{LyricWord, LyricsLine, LyricsResult, LyricsSource};
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
            .timeout(Duration::from_secs(5))
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
        let (clean_title, clean_artist) = clean_query_metadata(title, artist);
        let q_title = if clean_title.is_empty() {
            title.trim()
        } else {
            clean_title.as_str()
        };
        let q_artist = if clean_artist.is_empty() {
            artist.trim()
        } else {
            clean_artist.as_str()
        };

        if let Some(res) = self
            .fetch_once(q_title, q_artist, duration_secs, album)
            .await
        {
            return Some(res);
        }

        // Album metadata from YouTube (e.g. deluxe/single mismatch) can cause false misses.
        if album.is_some_and(|a| !a.trim().is_empty()) {
            if let Some(res) = self.fetch_once(q_title, q_artist, duration_secs, None).await {
                return Some(res);
            }
        }

        None
    }

    async fn fetch_once(
        &self,
        title: &str,
        artist: &str,
        duration_secs: Option<u32>,
        album: Option<&str>,
    ) -> Option<LyricsResult> {
        if title.is_empty() {
            return None;
        }
        let mut url = format!(
            "{BASE_URL}?s={}&a={}",
            urlencoding::encode(title),
            urlencoding::encode(artist)
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

static YT_SUFFIX_RE: OnceLock<Regex> = OnceLock::new();

fn clean_query_metadata(title: &str, artist: &str) -> (String, String) {
    let re = YT_SUFFIX_RE.get_or_init(|| {
        Regex::new(r"(?i)\s*[\(\[](?:official\s+(?:music\s+)?(?:video|audio|visualizer)|lyric(?:s)?(?:\s+video)?|audio|hd|4k|explicit|clean)[\)\]]")
            .expect("valid suffix regex")
    });
    let clean_title = re
        .replace_all(title, "")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    let clean_artist = artist
        .trim()
        .strip_suffix("- Topic")
        .unwrap_or(artist.trim())
        .trim()
        .to_string();
    (clean_title, clean_artist)
}

static TTML_P_RE: OnceLock<Regex> = OnceLock::new();
static TTML_BEGIN_RE: OnceLock<Regex> = OnceLock::new();
static TTML_END_RE: OnceLock<Regex> = OnceLock::new();
static TTML_DUR_RE: OnceLock<Regex> = OnceLock::new();
static XML_TAG_RE: OnceLock<Regex> = OnceLock::new();

pub fn parse_ttml(raw: &str) -> Vec<LyricsLine> {
    let p_re = TTML_P_RE
        .get_or_init(|| Regex::new(r"(?is)<p\b([^>]*)>(.*?)</p>").expect("valid p regex"));
    let begin_re = TTML_BEGIN_RE
        .get_or_init(|| Regex::new(r#"(?i)\bbegin\s*=\s*["']([^"']+)["']"#).expect("begin regex"));
    let end_re = TTML_END_RE
        .get_or_init(|| Regex::new(r#"(?i)\bend\s*=\s*["']([^"']+)["']"#).expect("end regex"));
    let dur_re = TTML_DUR_RE
        .get_or_init(|| Regex::new(r#"(?i)\bdur\s*=\s*["']([^"']+)["']"#).expect("dur regex"));
    let tag_re = XML_TAG_RE.get_or_init(|| Regex::new(r"<[^>]+>").expect("tag regex"));

    let mut lines = Vec::new();
    for cap in p_re.captures_iter(raw) {
        let attrs = &cap[1];
        let inner = &cap[2];
        let p_begin = begin_re
            .captures(attrs)
            .and_then(|c| parse_ttml_time(&c[1]));

        let words = extract_ttml_spans(inner, p_begin, begin_re, end_re, dur_re);
        let Some(time_ms) = p_begin.or_else(|| words.first().map(|w| w.time_ms)) else {
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
                words,
            });
        }
    }
    lines.sort_by_key(|l| l.time_ms);
    lines
}

#[derive(Clone, Copy)]
struct SpanFrame {
    begin_ms: Option<u64>,
    end_ms: Option<u64>,
}

/// Walks `<span ...>...</span>` tags recursively (handling nested spans like background vocals)
/// while tracking whether each timed span is followed by whitespace (preserving syllable joins).
fn extract_ttml_spans(
    inner: &str,
    fallback_begin: Option<u64>,
    begin_re: &Regex,
    end_re: &Regex,
    dur_re: &Regex,
) -> Vec<LyricWord> {
    let mut stack: Vec<SpanFrame> = Vec::new();
    let mut tokens: Vec<LyricWord> = Vec::new();
    let mut has_timed_span = false;
    let mut pos = 0usize;
    let bytes = inner.as_bytes();

    while pos < bytes.len() {
        if let Some(rel_lt) = inner[pos..].find('<') {
            if rel_lt > 0 {
                push_text_token(
                    &inner[pos..pos + rel_lt],
                    stack.last().copied(),
                    fallback_begin,
                    &mut tokens,
                );
                pos += rel_lt;
            }
            let Some(rel_gt) = inner[pos..].find('>') else {
                break;
            };
            let tag = &inner[pos + 1..pos + rel_gt];
            let trimmed_tag = tag.trim();
            if trimmed_tag.to_ascii_lowercase().starts_with("span")
                && !trimmed_tag.ends_with('/')
            {
                let tag_attrs = &trimmed_tag[4..];
                let own_begin = begin_re
                    .captures(tag_attrs)
                    .and_then(|c| parse_ttml_time(&c[1]));
                if own_begin.is_some() {
                    has_timed_span = true;
                }
                let begin_ms = own_begin.or_else(|| stack.last().and_then(|f| f.begin_ms));
                let mut end_ms = end_re
                    .captures(tag_attrs)
                    .and_then(|c| parse_ttml_time(&c[1]));
                if end_ms.is_none() {
                    if let (Some(b), Some(dur)) = (
                        begin_ms,
                        dur_re
                            .captures(tag_attrs)
                            .and_then(|c| parse_ttml_time(&c[1])),
                    ) {
                        end_ms = Some(b.saturating_add(dur));
                    }
                }
                stack.push(SpanFrame {
                    begin_ms,
                    end_ms: end_ms.or_else(|| stack.last().and_then(|f| f.end_ms)),
                });
            } else if trimmed_tag.to_ascii_lowercase().starts_with("/span") {
                stack.pop();
            }
            pos += rel_gt + 1;
        } else {
            push_text_token(
                &inner[pos..],
                stack.last().copied(),
                fallback_begin,
                &mut tokens,
            );
            break;
        }
    }

    if !has_timed_span {
        return Vec::new();
    }
    if let Some(last) = tokens.last_mut() {
        last.ends_with_space = false;
    }
    tokens.sort_by_key(|w| w.time_ms);
    tokens
}

fn push_text_token(
    raw_text: &str,
    frame: Option<SpanFrame>,
    fallback_begin: Option<u64>,
    tokens: &mut Vec<LyricWord>,
) {
    let decoded = decode_xml_entities(raw_text);
    let has_leading_space = decoded.chars().next().is_some_and(char::is_whitespace);
    let has_trailing_space = decoded.chars().last().is_some_and(char::is_whitespace);

    if has_leading_space {
        if let Some(prev) = tokens.last_mut() {
            prev.ends_with_space = true;
        }
    }

    let cleaned = decoded.split_whitespace().collect::<Vec<_>>().join(" ");
    if cleaned.is_empty() {
        return;
    }

    let Some(frame) = frame else {
        return;
    };
    let Some(time_ms) = frame.begin_ms.or(fallback_begin) else {
        return;
    };
    tokens.push(LyricWord {
        time_ms,
        end_ms: frame.end_ms,
        text: cleaned,
        ends_with_space: has_trailing_space,
    });
}

fn parse_ttml_time(s: &str) -> Option<u64> {
    let t = s.trim();
    if let Some(ms_str) = t.strip_suffix("ms") {
        let ms: f64 = ms_str.trim().parse().ok()?;
        return Some(ms.max(0.0).round() as u64);
    }
    if let Some(secs_str) = t.strip_suffix('s') {
        let secs: f64 = secs_str.trim().parse().ok()?;
        return Some((secs.max(0.0) * 1000.0).round() as u64);
    }
    if let Some(mins_str) = t.strip_suffix('m') {
        let mins: f64 = mins_str.trim().parse().ok()?;
        return Some((mins.max(0.0) * 60_000.0).round() as u64);
    }
    let parts: Vec<&str> = t.split(':').collect();
    match parts.as_slice() {
        [m, s] => {
            let mins: u64 = m.trim().parse().ok()?;
            let secs: f64 = s.trim().parse().ok()?;
            Some(mins * 60_000 + (secs.max(0.0) * 1000.0).round() as u64)
        }
        [h, m, s] => {
            let hrs: u64 = h.trim().parse().ok()?;
            let mins: u64 = m.trim().parse().ok()?;
            let secs: f64 = s.trim().parse().ok()?;
            Some(hrs * 3_600_000 + mins * 60_000 + (secs.max(0.0) * 1000.0).round() as u64)
        }
        [h, m, s, frames] => {
            let hrs: u64 = h.trim().parse().ok()?;
            let mins: u64 = m.trim().parse().ok()?;
            let secs: u64 = s.trim().parse().ok()?;
            let fr: u64 = frames.trim().parse().unwrap_or(0);
            Some(
                hrs * 3_600_000
                    + mins * 60_000
                    + secs * 1_000
                    + (fr * 1000) / 30,
            )
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
        .replace("&#x27;", "'")
        .replace("&nbsp;", " ")
}
