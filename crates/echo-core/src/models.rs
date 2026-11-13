use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArtistRef {
    pub id: Option<String>,
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AlbumRef {
    pub id: Option<String>,
    pub title: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Track {
    pub id: String,
    pub title: String,
    pub artists: Vec<ArtistRef>,
    pub album: Option<AlbumRef>,
    pub duration_secs: Option<u32>,
    pub thumbnail_url: Option<String>,
    /// YouTube loudnessDb (how far above -14 LKFS target this track sits).
    pub loudness_db: Option<f32>,
    pub liked: bool,
    pub is_local: bool,
    pub local_path: Option<String>,
}

impl Track {
    pub fn artist_display(&self) -> String {
        if self.artists.is_empty() {
            "Unknown Artist".to_string()
        } else {
            self.artists
                .iter()
                .map(|a| a.name.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        }
    }

    pub fn primary_artist(&self) -> &str {
        self.artists
            .first()
            .map(|a| a.name.as_str())
            .unwrap_or("")
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Playlist {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub thumbnail_url: Option<String>,
    pub track_count: usize,
    pub is_remote: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum RepeatMode {
    #[default]
    Off,
    All,
    One,
}

impl RepeatMode {
    pub fn next(self) -> Self {
        match self {
            Self::Off => Self::All,
            Self::All => Self::One,
            Self::One => Self::Off,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum AudioQuality {
    #[default]
    Auto,
    Max,
    High,
    Low,
}

impl AudioQuality {
    pub const ALL: [Self; 4] = [Self::Auto, Self::Max, Self::High, Self::Low];

    pub fn label(self) -> &'static str {
        match self {
            Self::Auto => "Auto",
            Self::Max => "Max",
            Self::High => "High",
            Self::Low => "Low",
        }
    }
}

impl std::fmt::Display for AudioQuality {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.label())
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct QueueItem {
    pub uid: u64,
    pub track: Track,
    #[serde(default)]
    pub is_user_queued: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct QueueState {
    pub items: Vec<QueueItem>,
    pub current_index: Option<usize>,
    pub repeat_mode: RepeatMode,
    pub shuffle: bool,
    next_uid: u64,
}

impl QueueState {
    pub fn current(&self) -> Option<&QueueItem> {
        self.current_index.and_then(|idx| self.items.get(idx))
    }

    pub fn replace_with(&mut self, tracks: Vec<Track>, start_index: usize) {
        self.items.clear();
        for track in tracks {
            let uid = self.alloc_uid();
            self.items.push(QueueItem {
                uid,
                track,
                is_user_queued: false,
            });
        }
        self.current_index = if self.items.is_empty() {
            None
        } else {
            Some(start_index.min(self.items.len() - 1))
        };
    }

    pub fn push_back(&mut self, track: Track) {
        let uid = self.alloc_uid();
        self.items.push(QueueItem {
            uid,
            track,
            is_user_queued: false,
        });
        if self.current_index.is_none() {
            self.current_index = Some(0);
        }
    }

    pub fn push_user(&mut self, track: Track) {
        let uid = self.alloc_uid();
        self.items.push(QueueItem {
            uid,
            track,
            is_user_queued: true,
        });
        if self.current_index.is_none() {
            self.current_index = Some(0);
        }
    }

    pub fn insert_next(&mut self, track: Track) {
        let uid = self.alloc_uid();
        let insert_at = match self.current_index {
            Some(idx) => (idx + 1).min(self.items.len()),
            None => 0,
        };
        self.items.insert(
            insert_at,
            QueueItem {
                uid,
                track,
                is_user_queued: false,
            },
        );
        if self.current_index.is_none() {
            self.current_index = Some(0);
        }
    }

    pub fn insert_user_next(&mut self, track: Track) {
        let uid = self.alloc_uid();
        let insert_at = match self.current_index {
            Some(idx) => (idx + 1).min(self.items.len()),
            None => 0,
        };
        self.items.insert(
            insert_at,
            QueueItem {
                uid,
                track,
                is_user_queued: true,
            },
        );
        if self.current_index.is_none() {
            self.current_index = Some(0);
        }
    }

    pub fn move_up(&mut self, idx: usize) {
        if idx == 0 || idx >= self.items.len() {
            return;
        }
        self.items.swap(idx, idx - 1);
        if let Some(cur) = self.current_index {
            if cur == idx {
                self.current_index = Some(idx - 1);
            } else if cur == idx - 1 {
                self.current_index = Some(idx);
            }
        }
    }

    pub fn move_down(&mut self, idx: usize) {
        if idx + 1 >= self.items.len() {
            return;
        }
        self.items.swap(idx, idx + 1);
        if let Some(cur) = self.current_index {
            if cur == idx {
                self.current_index = Some(idx + 1);
            } else if cur == idx + 1 {
                self.current_index = Some(idx);
            }
        }
    }

    pub fn remove_at(&mut self, idx: usize) {
        if idx >= self.items.len() {
            return;
        }
        self.items.remove(idx);
        if self.items.is_empty() {
            self.current_index = None;
            return;
        }
        if let Some(cur) = self.current_index {
            if idx < cur {
                self.current_index = Some(cur - 1);
            } else if idx == cur {
                self.current_index = Some(cur.min(self.items.len() - 1));
            }
        }
    }

    pub fn advance_next(&mut self) -> Option<&QueueItem> {
        if self.items.is_empty() {
            return None;
        }
        let next = match (self.current_index, self.repeat_mode, self.shuffle) {
            (Some(idx), RepeatMode::One, _) => Some(idx),
            (Some(idx), _, true) if self.items.len() > 1 => {
                let nanos = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.subsec_nanos() as usize)
                    .unwrap_or(1);
                let mut picked = nanos % self.items.len();
                if picked == idx {
                    picked = (idx + 1) % self.items.len();
                }
                Some(picked)
            }
            (Some(idx), RepeatMode::All, _) => Some((idx + 1) % self.items.len()),
            (Some(idx), RepeatMode::Off, _) if idx + 1 < self.items.len() => Some(idx + 1),
            (None, _, _) => Some(0),
            _ => None,
        };
        if let Some(n) = next {
            self.current_index = Some(n);
            self.items.get(n)
        } else {
            None
        }
    }

    pub fn step_prev(&mut self) -> Option<&QueueItem> {
        if self.items.is_empty() {
            return None;
        }
        let prev = match (self.current_index, self.repeat_mode) {
            (Some(0), RepeatMode::All) => Some(self.items.len() - 1),
            (Some(0), _) => Some(0),
            (Some(idx), _) => Some(idx - 1),
            (None, _) => Some(0),
        };
        self.current_index = prev;
        self.current()
    }

    fn alloc_uid(&mut self) -> u64 {
        self.next_uid += 1;
        self.next_uid
    }
}

/// Entry in Recognition History (mirrors Echo's `Heard` model).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RecognitionEntry {
    pub id: i64,
    pub recognized_at_ms: i64,
    pub title: String,
    pub artist: Option<String>,
    pub artwork_url: Option<String>,
    pub isrc: Option<String>,
    pub shazam_key: Option<String>,
    pub video_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TrackStats {
    pub track: Track,
    pub play_count: u64,
    pub total_played_secs: u64,
    pub last_played_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArtistStats {
    pub id: Option<String>,
    pub name: String,
    pub play_count: u64,
    pub total_played_secs: u64,
    pub track_count: u64,
    pub thumbnail_url: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AlbumStats {
    pub id: Option<String>,
    pub title: String,
    pub artist: String,
    pub play_count: u64,
    pub total_played_secs: u64,
    pub thumbnail_url: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DailyListening {
    pub day_short: String,
    pub date_short: String,
    pub plays: u64,
    pub total_secs: u64,
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct ListeningSummary {
    pub total_plays: u64,
    pub total_played_secs: u64,
    pub unique_tracks: u64,
    pub unique_artists: u64,
    pub online_plays: u64,
    pub local_plays: u64,
    pub active_days: u64,
    pub current_streak_days: u32,
    pub peak_hour: Option<u8>,
    pub hourly_plays: [u64; 24],
    pub daily_activity: Vec<DailyListening>,
    pub top_artists: Vec<ArtistStats>,
    pub top_albums: Vec<AlbumStats>,
    pub top_tracks: Vec<TrackStats>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PendingScrobble {
    pub id: i64,
    pub artist: String,
    pub track: String,
    pub album: Option<String>,
    pub duration_secs: Option<u32>,
    pub started_at_secs: i64,
}

/// Upgrades Google / YouTube thumbnail URLs to their highest clean resolution.
///
/// YouTube Music API responses by default return 60x60 or 120x120 thumbnails (`=w120-h120-l90-rj`)
/// designed for small web list rows. Google's image server automatically generates high-definition
/// album artwork when requested with `=w800-h800-l90-rj` (or `=s800`).
/// Similarly, YouTube video thumbnails often have `?sqp=...` downscaling parameters or use `default.jpg`.
pub fn upgrade_thumbnail_url(url: &str) -> String {
    let u = url.trim();
    if u.is_empty() {
        return String::new();
    }

    // Google User Content (lh3.googleusercontent.com, yt3.googleusercontent.com, *.ggpht.com)
    if u.contains("googleusercontent.com") || u.contains("ggpht.com") {
        let base = if let Some(idx) = u.find('?') {
            &u[..idx]
        } else {
            u
        };

        if let Some(idx) = base.rfind("=w") {
            let suffix = &base[idx + 2..];
            if let Some(dash_idx) = suffix.find("-h") {
                if let Ok(w) = suffix[..dash_idx].parse::<u32>() {
                    if w < 800 {
                        return format!("{}=w800-h800-l90-rj", &base[..idx]);
                    }
                }
            }
        } else if let Some(idx) = base.rfind("=s") {
            let suffix = &base[idx + 2..];
            let num_str: String = suffix.chars().take_while(|c| c.is_ascii_digit()).collect();
            if let Ok(s) = num_str.parse::<u32>() {
                if s < 800 {
                    let rest = &suffix[num_str.len()..];
                    return format!("{}=s800{}", &base[..idx], rest);
                }
            }
        }
        return base.to_string();
    }

    // YouTube static thumbnails (i.ytimg.com)
    if u.contains("i.ytimg.com") {
        let base = if let Some(idx) = u.find('?') {
            &u[..idx]
        } else {
            u
        };

        if base.ends_with("/default.jpg") {
            return base.replace("/default.jpg", "/hqdefault.jpg");
        } else if base.ends_with("/mqdefault.jpg") {
            return base.replace("/mqdefault.jpg", "/hqdefault.jpg");
        }
        return base.to_string();
    }

    u.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_upgrade_google_usercontent_thumbnails() {
        assert_eq!(
            upgrade_thumbnail_url("https://yt3.googleusercontent.com/test=w120-h120-l90-rj"),
            "https://yt3.googleusercontent.com/test=w800-h800-l90-rj"
        );
        assert_eq!(
            upgrade_thumbnail_url("https://lh3.googleusercontent.com/test=w60-h60-l90-rj"),
            "https://lh3.googleusercontent.com/test=w800-h800-l90-rj"
        );
        assert_eq!(
            upgrade_thumbnail_url("https://yt3.googleusercontent.com/test=s120-c-k-c0x00ffffff-no-rj"),
            "https://yt3.googleusercontent.com/test=s800-c-k-c0x00ffffff-no-rj"
        );
        assert_eq!(
            upgrade_thumbnail_url("https://yt3.ggpht.com/test=s88"),
            "https://yt3.ggpht.com/test=s800"
        );
        // Already 800+ should remain untouched
        assert_eq!(
            upgrade_thumbnail_url("https://yt3.googleusercontent.com/test=w1024-h1024-l90-rj"),
            "https://yt3.googleusercontent.com/test=w1024-h1024-l90-rj"
        );
    }

    #[test]
    fn test_upgrade_ytimg_thumbnails() {
        assert_eq!(
            upgrade_thumbnail_url("https://i.ytimg.com/vi/abc/hqdefault.jpg?sqp=-oaymwEWCJADEOEB&rs=AMzJ"),
            "https://i.ytimg.com/vi/abc/hqdefault.jpg"
        );
        assert_eq!(
            upgrade_thumbnail_url("https://i.ytimg.com/vi/abc/default.jpg?sqp=xyz"),
            "https://i.ytimg.com/vi/abc/hqdefault.jpg"
        );
        assert_eq!(
            upgrade_thumbnail_url("https://i.ytimg.com/vi/abc/hq720.jpg?sqp=xyz"),
            "https://i.ytimg.com/vi/abc/hq720.jpg"
        );
    }
}


