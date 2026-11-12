use regex::Regex;
use serde::{Deserialize, Serialize};
use std::sync::OnceLock;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum LyricsSource {
    BetterLyrics,
    LrcLib,
    KuGou,
    LocalLrc,
}

impl LyricsSource {
    pub fn label(self) -> &'static str {
        match self {
            Self::BetterLyrics => "Better Lyrics",
            Self::LrcLib => "LRCLIB",
            Self::KuGou => "KuGou",
            Self::LocalLrc => "Local .lrc",
        }
    }
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LyricWord {
    pub time_ms: u64,
    pub end_ms: Option<u64>,
    pub text: String,
    #[serde(default = "default_true")]
    pub ends_with_space: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LyricsLine {
    pub time_ms: u64,
    pub text: String,
    #[serde(default)]
    pub words: Vec<LyricWord>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LyricsResult {
    pub source: LyricsSource,
    pub synced: Vec<LyricsLine>,
    pub plain: Option<String>,
}

impl LyricsResult {
    /// Returns the index of the active lyric line for `position_ms`.
    pub fn active_index(&self, position_ms: u64) -> Option<usize> {
        if self.synced.is_empty() {
            return None;
        }
        let idx = self.synced.partition_point(|line| line.time_ms <= position_ms);
        idx.checked_sub(1)
    }
}

static LRC_TAG_RE: OnceLock<Regex> = OnceLock::new();

pub fn parse_lrc(raw: &str) -> Vec<LyricsLine> {
    let re = LRC_TAG_RE.get_or_init(|| {
        Regex::new(r"\[(\d{1,3}):(\d{2})(?:[.:](\d{1,3}))?\]").expect("valid lrc regex")
    });

    let mut lines = Vec::new();
    for raw_line in raw.lines() {
        let trimmed = raw_line.trim();
        if trimmed.is_empty() {
            continue;
        }

        let mut timestamps = Vec::new();
        let mut last_end = 0usize;
        for cap in re.captures_iter(trimmed) {
            let m = cap.get(0).unwrap();
            last_end = m.end();
            let mins: u64 = cap[1].parse().unwrap_or(0);
            let secs: u64 = cap[2].parse().unwrap_or(0);
            let frac_ms: u64 = match cap.get(3).map(|g| g.as_str()) {
                Some(f) if f.len() == 1 => f.parse::<u64>().unwrap_or(0) * 100,
                Some(f) if f.len() == 2 => f.parse::<u64>().unwrap_or(0) * 10,
                Some(f) => f[..3.min(f.len())].parse::<u64>().unwrap_or(0),
                None => 0,
            };
            timestamps.push(mins * 60_000 + secs * 1_000 + frac_ms);
        }

        if timestamps.is_empty() {
            continue;
        }

        let text = trimmed[last_end..].trim().to_string();
        for time_ms in timestamps {
            lines.push(LyricsLine {
                time_ms,
                text: text.clone(),
                words: Vec::new(),
            });
        }
    }

    lines.sort_by_key(|l| l.time_ms);
    lines
}

