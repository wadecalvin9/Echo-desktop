use echo_core::{AlbumRef, ArtistRef, Track};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum SearchFilter {
    #[default]
    Songs,
    Videos,
    Albums,
    Artists,
    Playlists,
}

impl SearchFilter {
    pub const ALL: [Self; 5] = [
        Self::Songs,
        Self::Videos,
        Self::Albums,
        Self::Artists,
        Self::Playlists,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::Songs => "Songs",
            Self::Videos => "Videos",
            Self::Albums => "Albums",
            Self::Artists => "Artists",
            Self::Playlists => "Playlists",
        }
    }

    pub fn params(self) -> &'static str {
        match self {
            Self::Songs => "EgWKAQIIAWoKEAoQAxAEEAkQBQ%3D%3D",
            Self::Videos => "EgWKAQIQAWoKEAoQAxAEEAkQBQ%3D%3D",
            Self::Albums => "EgWKAQIYAWoKEAoQAxAEEAkQBQ%3D%3D",
            Self::Artists => "EgWKAQIgAWoKEAoQAxAEEAkQBQ%3D%3D",
            Self::Playlists => "EgWKAQIoAWoKEAoQAxAEEAkQBQ%3D%3D",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum BrowseItem {
    Track(Track),
    Album {
        browse_id: String,
        playlist_id: Option<String>,
        title: String,
        artist: String,
        year: Option<u32>,
        thumbnail_url: Option<String>,
    },
    Artist {
        browse_id: String,
        name: String,
        thumbnail_url: Option<String>,
    },
    Playlist {
        id: String,
        title: String,
        author: String,
        thumbnail_url: Option<String>,
    },
}

impl BrowseItem {
    pub fn title(&self) -> &str {
        match self {
            Self::Track(t) => &t.title,
            Self::Album { title, .. } => title,
            Self::Artist { name, .. } => name,
            Self::Playlist { title, .. } => title,
        }
    }

    pub fn subtitle(&self) -> String {
        match self {
            Self::Track(t) => t.artist_display(),
            Self::Album { artist, year, .. } => match year {
                Some(y) => format!("{artist} • {y}"),
                None => artist.clone(),
            },
            Self::Artist { .. } => "Artist".to_string(),
            Self::Playlist { author, .. } => author.clone(),
        }
    }

    pub fn thumbnail_url(&self) -> Option<&str> {
        match self {
            Self::Track(t) => t.thumbnail_url.as_deref(),
            Self::Album { thumbnail_url, .. }
            | Self::Artist { thumbnail_url, .. }
            | Self::Playlist { thumbnail_url, .. } => thumbnail_url.as_deref(),
        }
    }

    pub fn stable_key(&self) -> String {
        match self {
            Self::Track(t) => format!("track:{}", t.id),
            Self::Album { browse_id, .. } => format!("album:{browse_id}"),
            Self::Artist { browse_id, .. } => format!("artist:{browse_id}"),
            Self::Playlist { id, .. } => format!("playlist:{id}"),
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct HomeSection {
    pub title: String,
    pub items: Vec<BrowseItem>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HomeChip {
    pub title: String,
    pub params: String,
}

#[derive(Debug, Clone, Default)]
pub struct HomePage {
    pub chips: Vec<HomeChip>,
    pub sections: Vec<HomeSection>,
    pub continuation: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct SearchResultPage {
    pub items: Vec<BrowseItem>,
    pub continuation: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct PlaylistPage {
    pub id: String,
    pub title: String,
    pub author: Option<String>,
    pub thumbnail_url: Option<String>,
    pub track_count_text: Option<String>,
    pub tracks: Vec<Track>,
    pub continuation: Option<String>,
    pub is_editable: bool,
}

#[derive(Debug, Clone, Default)]
pub struct AlbumPage {
    pub browse_id: String,
    pub playlist_id: Option<String>,
    pub title: String,
    pub artists: Vec<ArtistRef>,
    pub year: Option<u32>,
    pub thumbnail_url: Option<String>,
    pub subtitle: Option<String>,
    pub tracks: Vec<Track>,
    pub other_versions: Vec<BrowseItem>,
}

#[derive(Debug, Clone, Default)]
pub struct ArtistSection {
    pub title: String,
    pub browse_id: Option<String>,
    pub params: Option<String>,
    pub items: Vec<BrowseItem>,
}

#[derive(Debug, Clone, Default)]
pub struct ArtistPage {
    pub browse_id: String,
    pub name: String,
    pub channel_id: Option<String>,
    pub thumbnail_url: Option<String>,
    pub description: Option<String>,
    pub subscriber_count: Option<String>,
    pub monthly_listeners: Option<String>,
    pub shuffle_playlist_id: Option<String>,
    pub radio_playlist_id: Option<String>,
    pub radio_video_id: Option<String>,
    pub sections: Vec<ArtistSection>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MoodGenreButton {
    pub title: String,
    pub browse_id: String,
    pub params: String,
    pub stripe_color: Option<u32>,
}

#[derive(Debug, Clone, Default)]
pub struct MoodGenreGroup {
    pub title: String,
    pub items: Vec<MoodGenreButton>,
}

#[derive(Debug, Clone, Default)]
pub struct ExplorePage {
    pub new_release_albums: Vec<BrowseItem>,
    pub mood_and_genres: Vec<MoodGenreButton>,
}

#[derive(Debug, Clone, Default)]
pub struct RelatedPage {
    pub tracks: Vec<Track>,
    pub playlists: Vec<BrowseItem>,
    pub artists: Vec<BrowseItem>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AccountPlaylist {
    pub id: String,
    pub title: String,
    pub author: Option<String>,
    pub song_count_text: Option<String>,
    pub thumbnail_url: Option<String>,
    pub is_editable: bool,
}

#[derive(Debug, Clone)]
pub struct ResolvedStream {
    pub video_id: String,
    pub url: String,
    pub mime_type: String,
    pub bitrate: u32,
    pub content_length: Option<u64>,
    pub approx_duration_ms: Option<u64>,
    pub loudness_db: Option<f32>,
    pub client_name: String,
    pub user_agent: String,
    pub expires_in_secs: u64,
    pub title: Option<String>,
    pub author: Option<String>,
    pub thumbnail_url: Option<String>,
    pub playback_tracking_url: Option<String>,
}

#[derive(Debug, Clone)]
struct CachedAddress {
    stream: ResolvedStream,
    expires_at: Instant,
}

/// Mirrors Echo's `StreamAddresses.kt`: keeps each song's resolved stream URL until near expiry
/// and immediately drops it when playback or HTTP refuses it.
#[derive(Clone, Default)]
pub struct StreamAddressCache {
    entries: Arc<Mutex<HashMap<String, CachedAddress>>>,
}

impl StreamAddressCache {
    const MARGIN: Duration = Duration::from_secs(120);

    pub fn get(&self, video_id: &str) -> Option<ResolvedStream> {
        let now = Instant::now();
        let guard = self.entries.lock().expect("stream cache mutex");
        guard
            .get(video_id)
            .filter(|c| c.expires_at > now + Self::MARGIN)
            .map(|c| c.stream.clone())
    }

    pub fn put(&self, stream: ResolvedStream) {
        let ttl = Duration::from_secs(stream.expires_in_secs.max(300));
        let mut guard = self.entries.lock().expect("stream cache mutex");
        guard.insert(
            stream.video_id.clone(),
            CachedAddress {
                stream,
                expires_at: Instant::now() + ttl,
            },
        );
    }

    pub fn invalidate(&self, video_id: &str) {
        let mut guard = self.entries.lock().expect("stream cache mutex");
        guard.remove(video_id);
    }

    pub fn clear(&self) {
        let mut guard = self.entries.lock().expect("stream cache mutex");
        guard.clear();
    }
}

pub fn make_track(
    id: String,
    title: String,
    artists: Vec<ArtistRef>,
    album: Option<AlbumRef>,
    duration_secs: Option<u32>,
    thumbnail_url: Option<String>,
) -> Track {
    Track {
        id,
        title,
        artists,
        album,
        duration_secs,
        thumbnail_url,
        loudness_db: None,
        liked: false,
        is_local: false,
        local_path: None,
    }
}

