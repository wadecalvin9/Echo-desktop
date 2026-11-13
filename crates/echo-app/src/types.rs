use std::path::PathBuf;
use std::time::Instant;

use iced::keyboard::{Key, Modifiers};
use iced::widget::image;

use echo_core::{
    AudioQuality, AutoEqProfile, CastDevice, EqPreset, RecognitionEntry, RecognitionSource,
    ScannerSensitivity, SpatialMode, ThemeMode, Track,
};
use echo_fingerprint::{Recognised, RecognitionOutcome};
use echo_lyrics::LyricsResult;
use echo_youtube::{
    AccountPlaylist, AlbumPage, ArtistPage, BrowseItem, ExplorePage, HomePage, HomeSection,
    MoodGenreGroup, PlaylistPage, SearchFilter, SearchResultPage,
};
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NavTab {
    Home,
    Explore,
    Playlists,
    Search,
    Recognize,
    Stats,
    Settings,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SleepTimerState {
    Off,
    Countdown(Instant),
    EndOfSong,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransitionFadeOption {
    Off,
    Short,
    Medium,
    Long,
}

impl TransitionFadeOption {
    pub(crate) const ALL: [Self; 4] = [Self::Off, Self::Short, Self::Medium, Self::Long];

    pub fn ms(self) -> u64 {
        match self {
            Self::Off => 0,
            Self::Short => 1500,
            Self::Medium => 3000,
            Self::Long => 5000,
        }
    }

    pub(crate) fn from_ms(ms: u64) -> Self {
        match ms {
            0 => Self::Off,
            1..=2000 => Self::Short,
            2001..=4000 => Self::Medium,
            _ => Self::Long,
        }
    }
}

impl std::fmt::Display for TransitionFadeOption {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Off => "Off",
            Self::Short => "1.5 s",
            Self::Medium => "3.0 s",
            Self::Long => "5.0 s",
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FolderTarget {
    DownloadFolder,
    ExtraImportFolder,
    LocalMediaFolder,
    ExcludedMediaFolder,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum YoutubeRegionOption {
    Us,
    Gb,
    Ca,
    Au,
    De,
    Fr,
    Es,
    It,
    Br,
    Mx,
    Jp,
    Kr,
    In,
    Id,
    Tr,
    Nl,
    Se,
    Pl,
}

impl YoutubeRegionOption {
    pub(crate) const ALL: [Self; 18] = [
        Self::Us,
        Self::Gb,
        Self::Ca,
        Self::Au,
        Self::De,
        Self::Fr,
        Self::Es,
        Self::It,
        Self::Br,
        Self::Mx,
        Self::Jp,
        Self::Kr,
        Self::In,
        Self::Id,
        Self::Tr,
        Self::Nl,
        Self::Se,
        Self::Pl,
    ];

    pub fn code(self) -> &'static str {
        match self {
            Self::Us => "US",
            Self::Gb => "GB",
            Self::Ca => "CA",
            Self::Au => "AU",
            Self::De => "DE",
            Self::Fr => "FR",
            Self::Es => "ES",
            Self::It => "IT",
            Self::Br => "BR",
            Self::Mx => "MX",
            Self::Jp => "JP",
            Self::Kr => "KR",
            Self::In => "IN",
            Self::Id => "ID",
            Self::Tr => "TR",
            Self::Nl => "NL",
            Self::Se => "SE",
            Self::Pl => "PL",
        }
    }

    pub(crate) fn from_code(code: &str) -> Self {
        Self::ALL
            .iter()
            .copied()
            .find(|r| r.code().eq_ignore_ascii_case(code.trim()))
            .unwrap_or(Self::Us)
    }
}

impl std::fmt::Display for YoutubeRegionOption {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Us => "US - United States",
            Self::Gb => "GB - United Kingdom",
            Self::Ca => "CA - Canada",
            Self::Au => "AU - Australia",
            Self::De => "DE - Germany",
            Self::Fr => "FR - France",
            Self::Es => "ES - Spain",
            Self::It => "IT - Italy",
            Self::Br => "BR - Brazil",
            Self::Mx => "MX - Mexico",
            Self::Jp => "JP - Japan",
            Self::Kr => "KR - South Korea",
            Self::In => "IN - India",
            Self::Id => "ID - Indonesia",
            Self::Tr => "TR - Turkey",
            Self::Nl => "NL - Netherlands",
            Self::Se => "SE - Sweden",
            Self::Pl => "PL - Poland",
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum YoutubeLanguageOption {
    En,
    Es,
    Fr,
    De,
    It,
    Pt,
    Ja,
    Ko,
    ZhCn,
    Hi,
    Id,
    Tr,
    Nl,
    Sv,
    Pl,
}

impl YoutubeLanguageOption {
    pub(crate) const ALL: [Self; 15] = [
        Self::En,
        Self::Es,
        Self::Fr,
        Self::De,
        Self::It,
        Self::Pt,
        Self::Ja,
        Self::Ko,
        Self::ZhCn,
        Self::Hi,
        Self::Id,
        Self::Tr,
        Self::Nl,
        Self::Sv,
        Self::Pl,
    ];

    pub fn code(self) -> &'static str {
        match self {
            Self::En => "en",
            Self::Es => "es",
            Self::Fr => "fr",
            Self::De => "de",
            Self::It => "it",
            Self::Pt => "pt",
            Self::Ja => "ja",
            Self::Ko => "ko",
            Self::ZhCn => "zh-CN",
            Self::Hi => "hi",
            Self::Id => "id",
            Self::Tr => "tr",
            Self::Nl => "nl",
            Self::Sv => "sv",
            Self::Pl => "pl",
        }
    }

    pub(crate) fn from_code(code: &str) -> Self {
        Self::ALL
            .iter()
            .copied()
            .find(|l| l.code().eq_ignore_ascii_case(code.trim()))
            .unwrap_or(Self::En)
    }
}

impl std::fmt::Display for YoutubeLanguageOption {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::En => "English (en)",
            Self::Es => "Spanish (es)",
            Self::Fr => "French (fr)",
            Self::De => "German (de)",
            Self::It => "Italian (it)",
            Self::Pt => "Portuguese (pt)",
            Self::Ja => "Japanese (ja)",
            Self::Ko => "Korean (ko)",
            Self::ZhCn => "Chinese (zh-CN)",
            Self::Hi => "Hindi (hi)",
            Self::Id => "Indonesian (id)",
            Self::Tr => "Turkish (tr)",
            Self::Nl => "Dutch (nl)",
            Self::Sv => "Swedish (sv)",
            Self::Pl => "Polish (pl)",
        })
    }
}

#[derive(Debug, Clone)]
pub enum DetailPage {
    Playlist(PlaylistPage),
    Album(AlbumPage),
    Artist {
        page: ArtistPage,
        subscribed: bool,
    },
    ArtistItems {
        title: String,
        items: Vec<BrowseItem>,
    },
    Category {
        title: String,
        sections: Vec<HomeSection>,
    },
}

#[derive(Debug, Clone)]
pub struct NavSnapshot {
    pub tab: NavTab,
    pub library_tab: LibraryTab,
    pub viewing_detail: Option<DetailPage>,
    pub show_expanded: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LibraryTab {
    Liked,
    History,
    Downloads,
    Local,
    Songs,
    Artists,
    Albums,
    Playlist(usize),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LibraryFilter {
    All,
    Songs,
    Artists,
    Albums,
    Playlists,
    Downloads,
    Local,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SongsSortColumn {
    Title,
    Artist,
    Duration,
    DateAdded,
    PlayCount,
}

impl SongsSortColumn {
    pub const ALL: [Self; 5] = [
        Self::Title,
        Self::Artist,
        Self::Duration,
        Self::DateAdded,
        Self::PlayCount,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::Title => "Title",
            Self::Artist => "Artist",
            Self::Duration => "Duration",
            Self::DateAdded => "Date Added",
            Self::PlayCount => "Play Count",
        }
    }
}

impl std::fmt::Display for SongsSortColumn {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.label())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OutputDeviceOption {
    pub label: String,
    pub device: Option<String>,
}

impl std::fmt::Display for OutputDeviceOption {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.label)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SettingsSection {
    #[default]
    All,
    AudioDsp,
    LibraryMedia,
    Accounts,
    AppearanceSystem,
}

impl SettingsSection {
    pub const ALL: [Self; 5] = [
        Self::All,
        Self::AudioDsp,
        Self::LibraryMedia,
        Self::Accounts,
        Self::AppearanceSystem,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::All => "All Settings",
            Self::AudioDsp => "Audio & DSP",
            Self::LibraryMedia => "Library & Storage",
            Self::Accounts => "Accounts & Sync",
            Self::AppearanceSystem => "Appearance & System",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RightTab {
    Now,
    Queue,
    Lyrics,
    Top,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StatsRange {
    AllTime,
    Days30,
    Days7,
}

impl StatsRange {
    pub(crate) const ALL: [Self; 3] = [Self::AllTime, Self::Days30, Self::Days7];

    pub(crate) fn days(self) -> Option<u32> {
        match self {
            Self::AllTime => None,
            Self::Days30 => Some(30),
            Self::Days7 => Some(7),
        }
    }

    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::AllTime => "All Time",
            Self::Days30 => "30 Days",
            Self::Days7 => "7 Days",
        }
    }
}

#[derive(Debug, Clone)]
pub enum Message {
    SelectTab(NavTab),
    SelectLibraryTab(LibraryTab),
    LibraryFilterSelected(LibraryFilter),
    SelectRightTab(RightTab),
    ToggleRightTab(RightTab),
    ToggleLeftSidebar,
    ExpandAndNewPlaylist,
    ToggleRightSidebar,
    ToggleNewPlaylistInput,
    CardHovered(Option<String>),
    RowHovered(Option<String>),
    ShelfScroll {
        key: String,
        delta: isize,
    },
    OpenTrackMenu(Track),
    CloseTrackMenu,
    AddTrackToPlaylist {
        playlist_id: String,
        track: Track,
    },
    OpenTrackArtist(Track),
    OpenTrackAlbum(Track),
    SelectStatsRange(StatsRange),
    SearchExact(String),
    Tick,
    AnimTick,

    // Discover & Browse
    LoadHome,
    SelectHomeChip(Option<String>),
    LoadMoreHome,
    HomePageLoaded {
        append: bool,
        result: Result<HomePage, String>,
    },
    TasteShelvesLoaded(Vec<HomeSection>),
    LoadExplore,
    ExploreLoaded {
        explore: Result<ExplorePage, String>,
        moods: Vec<MoodGenreGroup>,
    },
    OpenCategory {
        title: String,
        browse_id: String,
        params: String,
    },
    CategoryLoaded {
        result: Result<(String, Vec<HomeSection>), String>,
    },
    OpenPlaylist {
        id: String,
        title: String,
    },
    PlaylistPageLoaded(Result<PlaylistPage, String>),
    LoadMorePlaylist,
    PlaylistContinuationLoaded(Result<PlaylistPage, String>),
    OpenAlbum {
        browse_id: String,
        title: String,
    },
    AlbumLoaded(Result<AlbumPage, String>),
    OpenArtist {
        browse_id: String,
        name: String,
    },
    ArtistLoaded(Result<ArtistPage, String>),
    OpenArtistItems {
        browse_id: String,
        params: Option<String>,
    },
    OpenShelfItems {
        title: String,
        items: Vec<BrowseItem>,
    },
    ArtistItemsLoaded(Result<(String, Vec<BrowseItem>, Option<String>), String>),
    SubscribeArtist {
        channel_id: String,
        subscribe: bool,
    },
    SubscribeResult {
        subscribed: bool,
        result: Result<(), String>,
    },
    NavigateBackDetail,
    NavigateForwardDetail,

    // Search
    SearchQueryChanged(String),
    CloseSearchDropdown,
    SearchFilterSelected(SearchFilter),
    SubmitSearch,
    SuggestionsLoaded(Vec<String>),
    SearchPageLoaded {
        append: bool,
        result: Result<SearchResultPage, String>,
    },
    LoadMoreSearch,
    ApplySuggestion(String),
    MoveHomeSectionUp(usize),
    MoveHomeSectionDown(usize),

    // Playback & Queue
    PlayTrack(Track),
    PlayTracks(Vec<Track>, usize),
    QueuePlayNext(Track),
    QueueAddBack(Track),
    QueueJump(usize),
    QueueRemove(usize),
    QueueMoveUp(usize),
    QueueMoveDown(usize),
    QueueClear,
    SaveQueueAsPlaylist,
    TogglePlayPause,
    NextTrack,
    PrevTrack,
    ToggleRepeat,
    ToggleShuffle,
    SeekChanged(f32),
    SeekReleased,
    VolumeChanged(f32),
    ToggleMute,
    ToggleLikeCurrent,
    ToggleLikeTrack(Track),
    StreamResolved {
        track: Track,
        result: Result<echo_youtube::ResolvedStream, String>,
    },
    RadioLoaded(Result<Vec<Track>, String>),

    // UI Overlays & Artwork
    ToggleExpandedPlayer,
    ToggleMiniPlayer,
    ToggleStreamInspector,
    ArtworkLoaded {
        url: String,
        rounded: Option<image::Handle>,
        circle: Option<image::Handle>,
        blurred: Option<image::Handle>,
        tint: Option<echo_core::RgbColor>,
        bytes: Option<Vec<u8>>,
    },

    // Lyrics
    LyricsLoaded {
        track_id: String,
        lyrics: Option<LyricsResult>,
    },
    SeekToMs(u64),

    // Library & Playlists
    PlaylistFilterChanged(String),
    NewPlaylistNameChanged(String),
    CreatePlaylist,
    DeletePlaylist(String),
    RemoveFromPlaylist {
        playlist_id: String,
        song_id: String,
    },
    AddCurrentToPlaylist(String),

    // Keyboard & Focus
    FocusSearch,
    KeyPressed(Key, Modifiers, bool),

    // Recognition
    ListenAndRecognize,
    RecognitionFinished(Result<RecognitionOutcome, String>),
    RecognitionTrackResolved {
        recognition_id: Option<i64>,
        track: Option<Track>,
    },
    PlayRecognised(Recognised),
    OpenHistoryRecognition(RecognitionEntry),

    // Last.fm
    LastFmApiKeyChanged(String),
    LastFmApiSecretChanged(String),
    StartLastFmAuth,
    LastFmTokenReceived(Result<(String, String), String>),
    CompleteLastFmAuth,
    LastFmSessionReceived(Result<echo_lastfm::Session, String>),
    DisconnectLastFm,
    ToggleLastFmScrobbling(bool),

    // YouTube Account Sync & Google Sign-In
    StartGoogleSignIn,
    GoogleSignInFinished(Result<String, String>),
    YouTubeCookieChanged(String),
    SaveYouTubeCookie,
    ClearYouTubeCookie,
    SyncYouTubeAccount,
    YoutubeRegionSelected(YoutubeRegionOption),
    YoutubeLanguageSelected(YoutubeLanguageOption),
    AccountSynced {
        playlists: Result<Vec<AccountPlaylist>, String>,
        liked: Result<PlaylistPage, String>,
    },

    // Offline Downloads & Stream Cache
    DownloadTrack(Track),
    DownloadTracks(Vec<Track>),
    DownloadCompleted {
        track: Track,
        result: Result<(String, String, u64), String>,
    },
    RemoveDownload(String),
    ClearDownloads,
    ScanDownloadFolders,
    DownloadFoldersScanned(Result<Vec<(Track, String, u64)>, String>),
    MigrateDownloads,
    ClearStreamCache,
    MaxCacheSizeChanged(String),
    DownloadFolderChanged(String),
    ExtraImportFoldersChanged(String),
    SaveDownloadSettings,
    PickFolder(FolderTarget),
    FolderPicked(FolderTarget, Result<Option<PathBuf>, String>),

    // Local Media Library
    LocalFoldersChanged(String),
    ExcludedFoldersChanged(String),
    ScannerSensitivitySelected(ScannerSensitivity),
    ToggleStrictFileNames(bool),
    ToggleAutoScanLocal(bool),
    ToggleLocalFolderTree,
    ScanLocalMedia,
    LocalMediaScanned(Vec<Track>),

    // Settings, DSP, Sleep Timer & OS Integration
    WindowOpened(iced::window::Id),
    WindowResized(iced::Size),
    WindowHandleReady(Option<isize>),
    DragWindow,
    MinimizeWindow,
    ToggleMaximizeWindow,
    WindowMaximizedState(bool),
    CloseWindow,
    CycleSleepTimer,
    ToggleNormalization(bool),
    ToggleAutoLoadMore(bool),
    ToggleSkipSilence(bool),
    ToggleBetterLyrics(bool),
    ToggleRomanizeLyrics,
    TransitionFadeSelected(TransitionFadeOption),
    EqPresetSelected(EqPreset),
    SpatialModeSelected(SpatialMode),
    ExportBackup,
    ExportBackupTo(Result<Option<PathBuf>, String>),
    ImportBackup,
    ImportBackupFrom(Result<Option<PathBuf>, String>),
    AudioQualitySelected(AudioQuality),
    ThemeModeSelected(ThemeMode),
    RecognitionSourceSelected(RecognitionSource),
    StartFixVolumes,
    StopFixVolumes,
    FixOneVolumeDone {
        track_id: String,
        loudness_db: Option<f32>,
        ok: bool,
    },

    // DSP & Audio Enhancements
    AutoEqProfileSelected(AutoEqProfile),
    EqBandChanged(usize, f32),
    ResetCustomEq,
    ApplyCustomEqCurve([f32; 10]),
    PlaybackSpeedChanged(f32),
    OutputDeviceSelected(Option<String>),
    SelectSettingsSection(SettingsSection),

    // Lyrics Calibration & Share
    AdjustLyricsOffset(i64),
    ResetLyricsOffset,
    ShareLyricsCard,

    // Recommendation Exclusions & Snooze
    ExcludeTrack(Track),
    ExcludeArtist(String),
    RemoveExcludedArtist(String),
    RemoveExcludedTrack(String),

    // Library Views & External Importer
    SongsSortColumnChanged(SongsSortColumn),
    SongsSortAscendingToggled,
    PickPlaylistImportFile,
    PlaylistImportFilePicked(Result<Option<PathBuf>, String>),

    // Casting & Remote Playback
    ToggleCastModal,
    ScanCastDevices,
    CastDevicesFound(Vec<CastDevice>),
    SelectCastDevice(Option<CastDevice>),
    CastAddressInputChanged(String),
    ConnectCastAddress,
}

pub struct ScrobbleTracker {
    pub track: Track,
    pub started_at_secs: i64,
    pub accumulated_ms: u64,
    pub scrobbled: bool,
}

