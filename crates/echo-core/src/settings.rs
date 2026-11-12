use crate::models::AudioQuality;
use directories::ProjectDirs;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum SettingsError {
    #[error("unable to determine application data directory")]
    NoDataDir,
    #[error("settings I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("settings serialization error: {0}")]
    Json(#[from] serde_json::Error),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum ThemeMode {
    #[default]
    Dynamic,
    OledBlack,
    Dark,
}

impl ThemeMode {
    pub const ALL: [Self; 3] = [Self::Dynamic, Self::OledBlack, Self::Dark];

    pub fn label(self) -> &'static str {
        match self {
            Self::Dynamic => "Dynamic (Album Art)",
            Self::OledBlack => "OLED True Black",
            Self::Dark => "Dark",
        }
    }
}

impl std::fmt::Display for ThemeMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.label())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum RecognitionSource {
    #[default]
    SystemLoopback,
    Microphone,
}

impl RecognitionSource {
    pub const ALL: [Self; 2] = [Self::SystemLoopback, Self::Microphone];

    pub fn label(self) -> &'static str {
        match self {
            Self::SystemLoopback => "System Audio (WASAPI Loopback)",
            Self::Microphone => "Microphone",
        }
    }
}

impl std::fmt::Display for RecognitionSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.label())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum EqPreset {
    #[default]
    Flat,
    BassBoost,
    Vocal,
    TrebleBoost,
    Acoustic,
    Electronic,
    LateNight,
}

impl EqPreset {
    pub const ALL: [Self; 7] = [
        Self::Flat,
        Self::BassBoost,
        Self::Vocal,
        Self::TrebleBoost,
        Self::Acoustic,
        Self::Electronic,
        Self::LateNight,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::Flat => "Flat",
            Self::BassBoost => "Bass Boost",
            Self::Vocal => "Vocal Presence",
            Self::TrebleBoost => "Treble Boost",
            Self::Acoustic => "Acoustic",
            Self::Electronic => "Electronic",
            Self::LateNight => "Late Night",
        }
    }

    /// 10-band gains in dB for [31, 62, 125, 250, 500, 1k, 2k, 4k, 8k, 16k Hz].
    pub fn gains_db(self) -> [f32; 10] {
        match self {
            Self::Flat => [0.0; 10],
            Self::BassBoost => [5.5, 4.5, 3.0, 1.5, 0.0, 0.0, 0.0, 0.0, 0.5, 0.5],
            Self::Vocal => [-1.5, -1.0, 0.0, 1.0, 2.5, 3.5, 3.5, 2.0, 0.5, 0.0],
            Self::TrebleBoost => [0.0, 0.0, 0.0, 0.0, 0.5, 1.0, 2.5, 3.5, 4.5, 4.5],
            Self::Acoustic => [2.5, 2.0, 1.0, 0.5, 1.5, 1.5, 2.0, 2.5, 2.0, 1.5],
            Self::Electronic => [4.5, 3.5, 1.5, -0.5, -1.5, 0.5, 1.5, 2.5, 3.5, 4.0],
            Self::LateNight => [-2.0, -1.0, 0.5, 1.5, 2.0, 2.0, 1.5, 0.5, -1.0, -2.0],
        }
    }
}

impl std::fmt::Display for EqPreset {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.label())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum SpatialMode {
    #[default]
    Off,
    Headphones,
    SurroundUpmix,
}

impl SpatialMode {
    pub const ALL: [Self; 3] = [Self::Off, Self::Headphones, Self::SurroundUpmix];

    pub fn label(self) -> &'static str {
        match self {
            Self::Off => "Off",
            Self::Headphones => "Headphones (Binaural Stage)",
            Self::SurroundUpmix => "Surround Upmix",
        }
    }
}

impl std::fmt::Display for SpatialMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.label())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum ScannerSensitivity {
    MatchTitle,
    #[default]
    MatchTitleAndArtists,
    MatchTitleArtistsAlbums,
}

impl ScannerSensitivity {
    pub const ALL: [Self; 3] = [
        Self::MatchTitle,
        Self::MatchTitleAndArtists,
        Self::MatchTitleArtistsAlbums,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::MatchTitle => "Match Title",
            Self::MatchTitleAndArtists => "Match Title & Artists",
            Self::MatchTitleArtistsAlbums => "Match Title, Artists & Album",
        }
    }
}

impl std::fmt::Display for ScannerSensitivity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.label())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct AppSettings {
    pub audio_normalization: bool,
    pub audio_quality: AudioQuality,
    pub auto_load_more: bool,
    pub skip_silence: bool,
    pub transition_fade_ms: u64,
    pub eq_preset: EqPreset,
    pub spatial_mode: SpatialMode,
    pub better_lyrics_enabled: bool,
    pub romanize_lyrics: bool,
    pub max_cache_size_mb: u64,
    pub download_folder: Option<String>,
    pub extra_import_folders: Vec<String>,
    pub local_media_folders: Vec<String>,
    pub excluded_media_folders: Vec<String>,
    pub scanner_sensitivity: ScannerSensitivity,
    pub strict_file_names: bool,
    pub auto_scan_local_on_startup: bool,
    /// Master player gain (0.0..=4.0, where 1.0 = 100%, 4.0 = +12 dB boost).
    /// Values > 1.0 are reset back to 1.0 on a fresh launch, matching Echo.
    pub volume: f32,
    pub theme_mode: ThemeMode,
    pub recognition_source: RecognitionSource,
    pub youtube_gl: String,
    pub youtube_hl: String,
    pub youtube_visitor_data: Option<String>,
    pub youtube_cookie: Option<String>,
    pub lastfm_api_key: String,
    pub lastfm_api_secret: String,
    pub lastfm_session_key: Option<String>,
    pub lastfm_username: Option<String>,
    pub lastfm_scrobbling_enabled: bool,
}

impl Default for AppSettings {
    fn default() -> Self {
        let default_local = directories::UserDirs::new()
            .and_then(|u| u.audio_dir().map(|p| p.to_string_lossy().into_owned()))
            .into_iter()
            .collect();
        Self {
            audio_normalization: true,
            audio_quality: AudioQuality::Auto,
            auto_load_more: true,
            skip_silence: false,
            transition_fade_ms: 0,
            eq_preset: EqPreset::Flat,
            spatial_mode: SpatialMode::Off,
            better_lyrics_enabled: true,
            romanize_lyrics: false,
            max_cache_size_mb: 512,
            download_folder: None,
            extra_import_folders: Vec::new(),
            local_media_folders: default_local,
            excluded_media_folders: Vec::new(),
            scanner_sensitivity: ScannerSensitivity::MatchTitleAndArtists,
            strict_file_names: false,
            auto_scan_local_on_startup: false,
            volume: 1.0,
            theme_mode: ThemeMode::Dynamic,
            recognition_source: RecognitionSource::SystemLoopback,
            youtube_gl: "US".to_string(),
            youtube_hl: "en".to_string(),
            youtube_visitor_data: None,
            youtube_cookie: None,
            lastfm_api_key: std::env::var("ECHO_LASTFM_API_KEY").unwrap_or_default(),
            lastfm_api_secret: std::env::var("ECHO_LASTFM_API_SECRET").unwrap_or_default(),
            lastfm_session_key: None,
            lastfm_username: None,
            lastfm_scrobbling_enabled: true,
        }
    }
}

impl AppSettings {
    pub fn app_dirs() -> Result<ProjectDirs, SettingsError> {
        ProjectDirs::from("io.github", "echo-desktop", "Echo")
            .ok_or(SettingsError::NoDataDir)
    }

    pub fn settings_path() -> Result<PathBuf, SettingsError> {
        Ok(Self::app_dirs()?.config_dir().join("settings.json"))
    }

    pub fn db_path() -> Result<PathBuf, SettingsError> {
        Ok(Self::app_dirs()?.data_dir().join("echo.db"))
    }

    pub fn stream_cache_dir() -> Result<PathBuf, SettingsError> {
        Ok(Self::app_dirs()?.cache_dir().join("streams"))
    }

    pub fn cover_cache_dir() -> Result<PathBuf, SettingsError> {
        Ok(Self::app_dirs()?.cache_dir().join("covers"))
    }

    pub fn default_downloads_dir() -> Result<PathBuf, SettingsError> {
        if let Some(u) = directories::UserDirs::new() {
            if let Some(music) = u.audio_dir() {
                return Ok(music.join("Echo Downloads"));
            }
        }
        Ok(Self::app_dirs()?.data_dir().join("downloads"))
    }

    pub fn effective_downloads_dir(&self) -> Result<PathBuf, SettingsError> {
        if let Some(custom) = self.download_folder.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
            return Ok(PathBuf::from(custom));
        }
        Self::default_downloads_dir()
    }

    pub fn load_or_default() -> Self {
        let Ok(path) = Self::settings_path() else {
            return Self::default();
        };
        let Ok(raw) = fs::read_to_string(path) else {
            return Self::default();
        };
        let mut settings: Self = serde_json::from_str(&raw).unwrap_or_default();
        settings.clamp_startup_volume();
        settings
    }

    pub fn save(&self) -> Result<(), SettingsError> {
        let path = Self::settings_path()?;
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let body = serde_json::to_string_pretty(self)?;
        fs::write(path, body)?;
        Ok(())
    }

    /// Echo rule (`Volume-and-loudness.md`): anything above 100% is not remembered across
    /// restarts so a +12 dB boost never surprises the listener on the next launch.
    pub fn clamp_startup_volume(&mut self) {
        if !self.volume.is_finite() || self.volume < 0.0 {
            self.volume = 1.0;
        } else if self.volume > 1.0 {
            self.volume = 1.0;
        }
    }
}

