pub mod betterlyrics;
pub mod kugou;
pub mod lrc;
pub mod lrclib;
pub mod romanize;

pub use betterlyrics::{parse_ttml, BetterLyricsClient};
pub use kugou::{generate_keyword, normalize_kugou_lrc, KuGouClient, KuGouError};
pub use lrc::{parse_lrc, LyricWord, LyricsLine, LyricsResult, LyricsSource};
pub use lrclib::{LrcLibClient, LrcLibError, LrcLibTrack};
pub use romanize::{contains_cjk_or_hangul_or_kana, romanize_text};

pub fn load_local_lrc(audio_path: &std::path::Path) -> Option<LyricsResult> {
    let lrc_path = audio_path.with_extension("lrc");
    let raw = std::fs::read_to_string(lrc_path).ok()?;
    let synced = parse_lrc(&raw);
    if !synced.is_empty() {
        Some(LyricsResult {
            source: LyricsSource::LocalLrc,
            synced,
            plain: None,
        })
    } else if !raw.trim().is_empty() {
        Some(LyricsResult {
            source: LyricsSource::LocalLrc,
            synced: Vec::new(),
            plain: Some(raw),
        })
    } else {
        None
    }
}

pub struct LyricsFetcher {
    better: BetterLyricsClient,
    lrclib: LrcLibClient,
    kugou: KuGouClient,
}

impl Default for LyricsFetcher {
    fn default() -> Self {
        Self::new()
    }
}

impl LyricsFetcher {
    pub fn new() -> Self {
        Self {
            better: BetterLyricsClient::new(),
            lrclib: LrcLibClient::new(),
            kugou: KuGouClient::new(),
        }
    }

    pub async fn fetch(
        &self,
        title: &str,
        artist: &str,
        duration_secs: Option<u32>,
        album: Option<&str>,
    ) -> Option<LyricsResult> {
        self.fetch_with_options(title, artist, duration_secs, album, None, true)
            .await
    }

    pub async fn fetch_with_options(
        &self,
        title: &str,
        artist: &str,
        duration_secs: Option<u32>,
        album: Option<&str>,
        local_audio_path: Option<&str>,
        use_better_lyrics: bool,
    ) -> Option<LyricsResult> {
        if let Some(path) = local_audio_path {
            if let Some(local) = load_local_lrc(std::path::Path::new(path)) {
                return Some(local);
            }
        }

        let lrclib_res = if use_better_lyrics {
            let (bl_opt, lrc_res) = tokio::join!(
                self.better.fetch_lyrics(title, artist, duration_secs, album),
                self.lrclib.fetch_lyrics(title, artist, duration_secs, album),
            );
            if let Some(bl) = bl_opt.filter(|r| !r.synced.is_empty()) {
                return Some(bl);
            }
            lrc_res
        } else {
            self.lrclib
                .fetch_lyrics(title, artist, duration_secs, album)
                .await
        };

        if let Ok(res) = lrclib_res {
            if !res.synced.is_empty() {
                return Some(res);
            }
            if let Ok(kg) = self.kugou.fetch_lyrics(title, artist, duration_secs).await {
                return Some(kg);
            }
            return Some(res);
        }

        self.kugou
            .fetch_lyrics(title, artist, duration_secs)
            .await
            .ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_synced_lrc_and_tracks_active_line() {
        let raw = "[ti:Test]\n[00:05.10]Line one\n[00:12.500]Line two\n";
        let synced = parse_lrc(raw);
        assert_eq!(synced.len(), 2);
        assert_eq!(synced[0].time_ms, 5100);
        assert_eq!(synced[0].text, "Line one");
        assert_eq!(synced[1].time_ms, 12500);

        let res = LyricsResult {
            source: LyricsSource::LrcLib,
            synced,
            plain: None,
        };
        assert_eq!(res.active_index(1000), None);
        assert_eq!(res.active_index(5100), Some(0));
        assert_eq!(res.active_index(15000), Some(1));
    }

    #[test]
    fn kugou_keyword_preserves_parenthesized_title_parts_properly() {
        let kw = generate_keyword("(It Goes Like) Nanana (Edit)", "Peggy Gou");
        assert_eq!(kw, "Nanana - Peggy Gou");
    }

    #[test]
    fn parses_better_lyrics_ttml_and_romanizes_hangul_and_kana() {
        let ttml = r#"
            <tt><body><div>
                <p begin="00:05.25" end="00:08.10"><span>Hello</span> <span>world</span></p>
                <p begin="1:12.500" end="1:15.00">Second &amp; line</p>
            </div></body></tt>
        "#;
        let parsed = parse_ttml(ttml);
        assert_eq!(parsed.len(), 2);
        assert_eq!(parsed[0].time_ms, 5250);
        assert_eq!(parsed[0].text, "Hello world");
        assert_eq!(parsed[1].time_ms, 72500);
        assert_eq!(parsed[1].text, "Second & line");

        assert_eq!(romanize_text("한글"), "hangeul");
        assert_eq!(romanize_text("こんにちは"), "konnichiha");
        assert_eq!(romanize_text("トーキョー"), "tookyoo");
    }

    #[test]
    fn parses_word_level_ttml_spans() {
        let ttml = r#"
            <tt><body><div>
                <p begin="00:10.00" end="00:13.00">
                    <span begin="00:10.00" end="00:11.00">Feel</span>
                    <span begin="00:11.20" end="00:12.10">the</span>
                    <span begin="00:12.20" end="00:13.00">beat</span>
                </p>
            </div></body></tt>
        "#;
        let parsed = parse_ttml(ttml);
        assert_eq!(parsed.len(), 1);
        assert_eq!(parsed[0].words.len(), 3);
        assert_eq!(parsed[0].words[0].text, "Feel");
        assert_eq!(parsed[0].words[1].time_ms, 11_200);
        assert_eq!(parsed[0].words[2].end_ms, Some(13_000));
    }

    #[test]
    fn parses_nested_spans_syllables_and_dur_times() {
        let ttml = r#"
            <tt><body><div>
                <p>
                    <span begin="1000ms" dur="500ms">Ev</span><span begin="1500ms" end="2000ms">ery</span><span begin="2.0s" end="2.4s">day</span>
                    <span ttm:role="x-bg">(<span begin="2500ms" end="3200ms">yeah</span>)</span>
                </p>
            </div></body></tt>
        "#;
        let parsed = parse_ttml(ttml);
        assert_eq!(parsed.len(), 1);
        assert_eq!(parsed[0].time_ms, 1_000);
        assert_eq!(parsed[0].words[0].text, "Ev");
        assert!(!parsed[0].words[0].ends_with_space);
        assert_eq!(parsed[0].words[1].text, "ery");
        assert!(!parsed[0].words[1].ends_with_space);
        assert_eq!(parsed[0].words[2].text, "day");
        assert!(parsed[0].words[2].ends_with_space);
        assert!(parsed[0].words.iter().any(|w| w.text == "yeah" && w.time_ms == 2500));
    }
}

