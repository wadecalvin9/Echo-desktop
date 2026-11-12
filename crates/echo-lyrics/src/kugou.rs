use crate::lrc::{parse_lrc, LyricsResult, LyricsSource};
use base64::engine::general_purpose::STANDARD;
use base64::Engine;
use regex::Regex;
use serde::Deserialize;
use std::sync::OnceLock;
use thiserror::Error;

const DURATION_TOLERANCE: i64 = 8;
const HEAD_CUT_LIMIT: usize = 30;

#[derive(Debug, Error)]
pub enum KuGouError {
    #[error("http error: {0}")]
    Http(#[from] reqwest::Error),
    #[error("base64 error: {0}")]
    Base64(#[from] base64::DecodeError),
    #[error("no KuGou lyrics candidate found")]
    NotFound,
}

#[derive(Debug, Deserialize)]
struct SearchSongResponse {
    data: SearchSongData,
}

#[derive(Debug, Deserialize)]
struct SearchSongData {
    info: Vec<SongInfo>,
}

#[derive(Debug, Deserialize)]
struct SongInfo {
    duration: i64,
    hash: String,
}

#[derive(Debug, Deserialize)]
struct SearchLyricsResponse {
    candidates: Vec<Candidate>,
}

#[derive(Debug, Deserialize)]
struct Candidate {
    id: i64,
    accesskey: String,
}

#[derive(Debug, Deserialize)]
struct DownloadLyricsResponse {
    content: String,
}

pub struct KuGouClient {
    http: reqwest::Client,
}

impl Default for KuGouClient {
    fn default() -> Self {
        Self::new()
    }
}

impl KuGouClient {
    pub fn new() -> Self {
        let http = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(12))
            .build()
            .expect("reqwest client");
        Self { http }
    }

    pub async fn fetch_lyrics(
        &self,
        title: &str,
        artist: &str,
        duration_secs: Option<u32>,
    ) -> Result<LyricsResult, KuGouError> {
        let keyword = generate_keyword(title, artist);
        let dur_i64 = duration_secs.map(|d| d as i64);

        let songs: SearchSongResponse = self
            .http
            .get("https://mobileservice.kugou.com/api/v3/search/song")
            .query(&[
                ("version", "9108"),
                ("plat", "0"),
                ("pagesize", "8"),
                ("showtype", "0"),
                ("keyword", &keyword),
            ])
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;

        let mut candidate = None;
        for song in songs.data.info {
            let within = dur_i64
                .map(|d| (song.duration - d).abs() <= DURATION_TOLERANCE)
                .unwrap_or(true);
            if within {
                let res: SearchLyricsResponse = self
                    .http
                    .get("https://lyrics.kugou.com/search")
                    .query(&[
                        ("ver", "1"),
                        ("man", "yes"),
                        ("client", "pc"),
                        ("hash", &song.hash),
                    ])
                    .send()
                    .await?
                    .error_for_status()?
                    .json()
                    .await?;
                if let Some(c) = res.candidates.into_iter().next() {
                    candidate = Some(c);
                    break;
                }
            }
        }

        if candidate.is_none() {
            let mut query = vec![
                ("ver", "1".to_string()),
                ("man", "yes".to_string()),
                ("client", "pc".to_string()),
                ("keyword", keyword),
            ];
            if let Some(d) = dur_i64 {
                query.push(("duration", (d * 1000).to_string()));
            }
            let res: SearchLyricsResponse = self
                .http
                .get("https://lyrics.kugou.com/search")
                .query(&query)
                .send()
                .await?
                .error_for_status()?
                .json()
                .await?;
            candidate = res.candidates.into_iter().next();
        }

        let cand = candidate.ok_or(KuGouError::NotFound)?;
        let dl: DownloadLyricsResponse = self
            .http
            .get("https://lyrics.kugou.com/download")
            .query(&[
                ("fmt", "lrc"),
                ("charset", "utf8"),
                ("client", "pc"),
                ("ver", "1"),
                ("id", &cand.id.to_string()),
                ("accesskey", &cand.accesskey),
            ])
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;

        let raw_bytes = STANDARD.decode(dl.content.trim())?;
        let raw_text = String::from_utf8_lossy(&raw_bytes);
        let normalized = normalize_kugou_lrc(&raw_text);
        if normalized.is_empty()
            || normalized.contains("纯音乐，请欣赏")
            || normalized.contains("酷狗音乐  就是歌多")
        {
            return Err(KuGouError::NotFound);
        }

        let synced = parse_lrc(&normalized);
        if synced.is_empty() {
            return Err(KuGouError::NotFound);
        }

        Ok(LyricsResult {
            source: LyricsSource::KuGou,
            synced,
            plain: Some(normalized),
        })
    }
}

pub fn generate_keyword(title: &str, artist: &str) -> String {
    static BRACKETS_RE: OnceLock<Regex> = OnceLock::new();
    static ARTIST_PAREN_RE: OnceLock<Regex> = OnceLock::new();

    let brackets = BRACKETS_RE.get_or_init(|| {
        Regex::new(r"\(.*?\)|（.*?）|「.*?」|『.*?』|<.*?>|《.*?》|〈.*?〉|＜.*?＞").unwrap()
    });
    let artist_paren = ARTIST_PAREN_RE.get_or_init(|| Regex::new(r"\(.*\)|（.*）").unwrap());

    let norm_title = brackets.replace_all(title, "").trim().to_string();
    let norm_artist = artist
        .replace(", ", "、")
        .replace(" & ", "、")
        .replace('.', "")
        .replace('和', "、");
    let norm_artist = artist_paren.replace_all(&norm_artist, "").trim().to_string();
    format!("{norm_title} - {norm_artist}")
}

pub fn normalize_kugou_lrc(raw: &str) -> String {
    static ACCEPTED_RE: OnceLock<Regex> = OnceLock::new();
    static BANNED_RE: OnceLock<Regex> = OnceLock::new();

    let accepted =
        ACCEPTED_RE.get_or_init(|| Regex::new(r"^\[\d{2}:\d{2}\.\d{2,3}\].*").unwrap());
    let banned = BANNED_RE.get_or_init(|| Regex::new(r".+\].+[:：].+").unwrap());

    let lines: Vec<String> = raw
        .replace("&apos;", "'")
        .lines()
        .map(|l| l.trim().to_string())
        .filter(|l| accepted.is_match(l))
        .collect();

    if lines.is_empty() {
        return String::new();
    }

    let mut head_cut = 0usize;
    let max_head = HEAD_CUT_LIMIT.min(lines.len() - 1);
    for i in (0..=max_head).rev() {
        if banned.is_match(&lines[i]) {
            head_cut = i + 1;
            break;
        }
    }

    let after_head = &lines[head_cut..];
    if after_head.is_empty() {
        return String::new();
    }

    let mut tail_cut = 0usize;
    let max_tail = after_head
        .len()
        .saturating_sub(HEAD_CUT_LIMIT)
        .min(after_head.len() - 1);
    for i in (0..=max_tail).rev() {
        if banned.is_match(&after_head[after_head.len() - 1 - i]) {
            tail_cut = i + 1;
            break;
        }
    }

    let end = after_head.len().saturating_sub(tail_cut);
    after_head[..end].join("\n")
}

