mod dialogs;
mod icons;
mod smtc;
mod theme;

use crate::theme::{
    account_badge_style, ambient_scrim_style, app_shell_style, artwork_frame_style,
    badge_pill_style, bare_input_style, bento_card_style, blurred_image_handle, card_button_style,
    card_hover_scrim_style, card_play_fab_style, color_from_rgb, divider_style, dock_play_button_style,
    downloads_badge_style, elevated_card_style, expanded_stage_style, ghost_row_button_style,
    glass_panel_style, glowing_play_button_style, hero_banner_style, icon_button_style,
    liked_gradient_badge_style, local_badge_style, logo_badge_style, outlined_pill_button_style,
    pager_button_style, pick_list_menu_style, pick_list_style, pill_search_input_style,
    player_dock_style, popover_panel_style, quick_tile_button_style, quick_tile_play_badge_style,
    rounded_image_handle, scroll_style, search_field_style,
    skeleton_block_style, slim_slider_style, soft_pill_button_style, stat_bar_fill_style,
    stat_bar_track_style, stats_bento_button_style, stats_rank_badge_style, tile_button_style,
    to_iced_theme, toggler_style, tooltip_style, top_pill_button_style,
    up_next_card_button_style, window_control_button_style, with_alpha,
    mini_player_fallback_style, mini_player_scrim_style,
};
use echo_audio::{AudioEngine, AudioSourceSpec, PlaybackStatus};
use echo_core::{
    base_version_key, cache_stats, clear_cache_dir, default_backup_path, download_file_path,
    export_backup, is_preview_title, lookup_cached_stream, migrate_download_file, prune_queue_tail,
    scan_download_folders, scan_local_folders, score_artist_name, score_candidate,
    store_cached_stream, validate_and_unpack_backup, AppSettings, AudioQuality, DynamicPalette,
    EchoDb, EqPreset, ListenerContext, ListeningSummary, LocalMediaScanConfig, Playlist,
    QueueState, RecognitionEntry, RecognitionSource, RepeatMode, ScannerSensitivity, SpatialMode,
    ThemeMode, Track, TrackStats,
};
use echo_fingerprint::{capture_16khz_mono, Recognised, RecognitionOutcome, ShazamClient};
use echo_lastfm::{qualifies_for_scrobble, LastFmClient};
use echo_lyrics::{
    contains_cjk_or_hangul_or_kana, romanize_text, LyricsFetcher, LyricsResult,
};
use echo_youtube::{
    AccountPlaylist, AlbumPage, ArtistPage, BrowseItem, ExplorePage, HomeChip, HomePage,
    HomeSection, MoodGenreGroup, PlaylistPage, SearchFilter, SearchResultPage,
    YouTubeMusicClient,
};
use chrono::Timelike;
use iced::keyboard::{self, key::Named, Key, Modifiers};
use iced::widget::text::Wrapping;
use iced::widget::{
    button, column, container, horizontal_space, image, mouse_area, pick_list, responsive,
    rich_text, row, scrollable, slider, span, stack, text, text_input, toggler, tooltip,
    vertical_space,
};
use iced::{Alignment, Color, Element, Font, Length, Subscription, Task, Theme};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

fn load_app_icon() -> Option<iced::window::Icon> {
    const ICON_BYTES: &[u8] = include_bytes!("../assets/icon.png");
    let img = ::image::load_from_memory(ICON_BYTES).ok()?.into_rgba8();
    let (width, height) = img.dimensions();
    iced::window::icon::from_rgba(img.into_raw(), width, height).ok()
}

pub fn main() -> iced::Result {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "echo_app=info".into()),
        )
        .init();

    iced::application("Echo", EchoApp::update, EchoApp::view)
        .theme(EchoApp::theme)
        .subscription(EchoApp::subscription)
        .window(iced::window::Settings {
            size: iced::Size::new(1360.0, 840.0),
            decorations: false,
            icon: load_app_icon(),
            ..Default::default()
        })
        .run_with(EchoApp::new)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum NavTab {
    Home,
    Explore,
    Playlists,
    Search,
    Recognize,
    Stats,
    Settings,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SleepTimerState {
    Off,
    Countdown(Instant),
    EndOfSong,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TransitionFadeOption {
    Off,
    Short,
    Medium,
    Long,
}

impl TransitionFadeOption {
    const ALL: [Self; 4] = [Self::Off, Self::Short, Self::Medium, Self::Long];

    fn ms(self) -> u64 {
        match self {
            Self::Off => 0,
            Self::Short => 1500,
            Self::Medium => 3000,
            Self::Long => 5000,
        }
    }

    fn from_ms(ms: u64) -> Self {
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
enum FolderTarget {
    DownloadFolder,
    ExtraImportFolder,
    LocalMediaFolder,
    ExcludedMediaFolder,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum YoutubeRegionOption {
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
    const ALL: [Self; 18] = [
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

    fn code(self) -> &'static str {
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

    fn from_code(code: &str) -> Self {
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
enum YoutubeLanguageOption {
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
    const ALL: [Self; 15] = [
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

    fn code(self) -> &'static str {
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

    fn from_code(code: &str) -> Self {
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
enum DetailPage {
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
struct NavSnapshot {
    tab: NavTab,
    library_tab: LibraryTab,
    viewing_detail: Option<DetailPage>,
    show_expanded: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LibraryTab {
    Liked,
    History,
    Downloads,
    Local,
    Playlist(usize),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LibraryFilter {
    All,
    Playlists,
    Downloads,
    Local,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RightTab {
    Now,
    Queue,
    Lyrics,
    Top,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum StatsRange {
    AllTime,
    Days30,
    Days7,
}

impl StatsRange {
    const ALL: [Self; 3] = [Self::AllTime, Self::Days30, Self::Days7];

    fn days(self) -> Option<u32> {
        match self {
            Self::AllTime => None,
            Self::Days30 => Some(30),
            Self::Days7 => Some(7),
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::AllTime => "All Time",
            Self::Days30 => "30 Days",
            Self::Days7 => "7 Days",
        }
    }
}

#[derive(Debug, Clone)]
enum Message {
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

    // Playback & Queue
    PlayTrack(Track),
    PlayTracks(Vec<Track>, usize),
    QueuePlayNext(Track),
    QueueAddBack(Track),
    QueueJump(usize),
    QueueRemove(usize),
    QueueClear,
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
    ArtworkLoaded {
        url: String,
        rounded: Option<image::Handle>,
        circle: Option<image::Handle>,
        blurred: Option<image::Handle>,
        bytes: Option<Vec<u8>>,
    },

    // Lyrics
    LyricsLoaded {
        track_id: String,
        lyrics: Option<LyricsResult>,
    },
    SeekToMs(u64),

    // Library & Playlists
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

    // YouTube Account Sync
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
}

struct ScrobbleTracker {
    track: Track,
    started_at_secs: i64,
    accumulated_ms: u64,
    scrobbled: bool,
}

struct EchoApp {
    tab: NavTab,
    library_tab: LibraryTab,
    library_filter: LibraryFilter,
    right_tab: RightTab,
    left_collapsed: bool,
    show_right_rail: bool,
    show_new_playlist_input: bool,
    show_expanded: bool,
    hovered_card: Option<String>,
    hovered_row: Option<String>,

    settings: AppSettings,
    db: EchoDb,
    yt: YouTubeMusicClient,
    lyrics_fetcher: Arc<LyricsFetcher>,
    shazam: Arc<ShazamClient>,
    audio: Arc<AudioEngine>,
    http: reqwest::Client,

    palette: DynamicPalette,
    queue: QueueState,
    status: PlaybackStatus,
    position_ms: u64,
    duration_ms: u64,
    seeking_ms: Option<u64>,
    last_finished_seq: u64,
    last_tick: Instant,
    sleep_timer: SleepTimerState,
    fade_in_remaining_ms: u64,

    // Discover & Detail Pages
    home_sections: Vec<HomeSection>,
    home_taste_sections: Vec<HomeSection>,
    home_global_sections: Vec<HomeSection>,
    home_quick_tiles: Vec<BrowseItem>,
    home_seed_cursor: usize,
    home_chips: Vec<HomeChip>,
    home_active_chip: Option<String>,
    home_continuation: Option<String>,
    home_loading: bool,
    home_loading_more: bool,
    home_error: Option<String>,
    explore_page: Option<ExplorePage>,
    explore_loading: bool,
    explore_error: Option<String>,
    mood_groups: Vec<MoodGenreGroup>,
    viewing_detail: Option<DetailPage>,
    nav_history: Vec<NavSnapshot>,
    nav_forward: Vec<NavSnapshot>,
    detail_loading: Option<String>,
    playlist_loading_more: bool,

    // Search
    search_query: String,
    search_filter: SearchFilter,
    search_suggestions: Vec<String>,
    search_dropdown_open: bool,
    search_results: Vec<BrowseItem>,
    search_continuation: Option<String>,
    search_loading: bool,
    search_loading_more: bool,
    search_error: Option<String>,

    // YouTube Account
    yt_cookie_input: String,
    account_playlists: Vec<AccountPlaylist>,
    account_sync_status: Option<String>,
    account_syncing: bool,

    // Library & Stats
    liked_tracks: Vec<Track>,
    history_tracks: Vec<Track>,
    downloaded_tracks: Vec<(Track, PathBuf, u64)>,
    downloaded_id_set: HashSet<String>,
    downloading_ids: HashSet<String>,
    local_tracks: Vec<Track>,
    local_folder_tree: bool,
    local_scanning: bool,
    local_scan_status: Option<String>,
    download_status: Option<String>,
    cache_stats: (usize, u64),
    cache_size_input: String,
    download_folder_input: String,
    extra_import_input: String,
    local_folders_input: String,
    excluded_folders_input: String,
    playlists: Vec<Playlist>,
    selected_playlist_tracks: Vec<Track>,
    selected_playlist_id: Option<String>,
    new_playlist_name: String,
    stats: Vec<TrackStats>,
    stats_range: StatsRange,
    listening_summary: ListeningSummary,

    // Lyrics & Artwork caches
    active_lyrics: Option<LyricsResult>,
    lyrics_loading: bool,
    lyrics_scroll_id: scrollable::Id,
    last_lyric_idx: Option<usize>,
    search_input_id: text_input::Id,
    pre_mute_volume: f32,
    rounded_covers: HashMap<String, image::Handle>,
    circle_covers: HashMap<String, image::Handle>,
    blurred_covers: HashMap<String, image::Handle>,
    cover_bytes: HashMap<String, Vec<u8>>,
    loading_covers: HashSet<String>,
    shelf_offsets: HashMap<String, usize>,
    track_menu: Option<Track>,
    anim_phase: f32,
    spectrum_bands: [f32; 8],
    mini_player: bool,
    was_maximized_before_mini: bool,
    normal_window_size: (f32, f32),

    // Recognition
    recognizing: bool,
    recognition_status: Option<String>,
    last_recognised: Option<Recognised>,
    last_recognised_track: Option<Track>,
    recognition_history: Vec<RecognitionEntry>,

    // Last.fm
    lastfm_pending_token: Option<String>,
    lastfm_status: Option<String>,
    scrobble_tracker: Option<ScrobbleTracker>,

    // Volume repair & Adaptive / Backup / SMTC state
    main_window_id: Option<iced::window::Id>,
    window_hwnd: Option<isize>,
    window_size: (f32, f32),
    window_maximized: bool,
    smtc: Option<smtc::SmtcBridge>,
    smtc_requested: bool,
    listener_ctx: ListenerContext,
    backup_status: Option<String>,
    repair_running: bool,
    repair_queue: Vec<Track>,
    repair_total: usize,
    repair_done: usize,
    repair_fixed: usize,
    repair_consecutive_failures: usize,
    repair_status: Option<String>,
    missing_loudness_count: usize,

    toast: Option<(String, Instant)>,
}

impl EchoApp {
    fn new() -> (Self, Task<Message>) {
        let settings = AppSettings::load_or_default();
        let db = AppSettings::db_path()
            .ok()
            .and_then(|p| EchoDb::open(&p).ok())
            .unwrap_or_else(|| EchoDb::open_in_memory().expect("in-memory sqlite"));

        let yt = YouTubeMusicClient::new();
        yt.set_locale(settings.youtube_gl.clone(), settings.youtube_hl.clone());
        yt.set_visitor_data(settings.youtube_visitor_data.clone());
        yt.set_cookie(settings.youtube_cookie.clone());

        let audio = Arc::new(AudioEngine::new(settings.volume, settings.audio_normalization));
        audio.set_skip_silence(settings.skip_silence);
        audio.set_dsp(settings.eq_preset, settings.spatial_mode);
        let palette = DynamicPalette::default_for(settings.theme_mode);
        let pre_mute_volume = settings.volume.max(0.5);
        let yt_cookie_input = settings.youtube_cookie.clone().unwrap_or_default();
        let cache_size_input = settings.max_cache_size_mb.to_string();
        let download_folder_input = settings.download_folder.clone().unwrap_or_default();
        let extra_import_input = settings.extra_import_folders.join("; ");
        let local_folders_input = settings.local_media_folders.join("; ");
        let excluded_folders_input = settings.excluded_media_folders.join("; ");

        let mut app = Self {
            tab: NavTab::Home,
            library_tab: LibraryTab::Liked,
            library_filter: LibraryFilter::All,
            right_tab: RightTab::Now,
            left_collapsed: false,
            show_right_rail: true,
            show_new_playlist_input: false,
            show_expanded: false,
            hovered_card: None,
            hovered_row: None,
            settings,
            db,
            yt,
            lyrics_fetcher: Arc::new(LyricsFetcher::new()),
            shazam: Arc::new(ShazamClient::new()),
            audio,
            http: reqwest::Client::new(),
            palette,
            queue: QueueState::default(),
            status: PlaybackStatus::Stopped,
            position_ms: 0,
            duration_ms: 0,
            seeking_ms: None,
            last_finished_seq: 0,
            last_tick: Instant::now(),
            sleep_timer: SleepTimerState::Off,
            fade_in_remaining_ms: 0,
            home_sections: Vec::new(),
            home_taste_sections: Vec::new(),
            home_global_sections: Vec::new(),
            home_quick_tiles: Vec::new(),
            home_seed_cursor: 0,
            home_chips: Vec::new(),
            home_active_chip: None,
            home_continuation: None,
            home_loading: true,
            home_loading_more: false,
            home_error: None,
            explore_page: None,
            explore_loading: false,
            explore_error: None,
            mood_groups: Vec::new(),
            viewing_detail: None,
            nav_history: Vec::new(),
            nav_forward: Vec::new(),
            detail_loading: None,
            playlist_loading_more: false,
            search_query: String::new(),
            search_filter: SearchFilter::Songs,
            search_suggestions: Vec::new(),
            search_dropdown_open: false,
            search_results: Vec::new(),
            search_continuation: None,
            search_loading: false,
            search_loading_more: false,
            search_error: None,
            yt_cookie_input,
            account_playlists: Vec::new(),
            account_sync_status: None,
            account_syncing: false,
            liked_tracks: Vec::new(),
            history_tracks: Vec::new(),
            downloaded_tracks: Vec::new(),
            downloaded_id_set: HashSet::new(),
            downloading_ids: HashSet::new(),
            local_tracks: Vec::new(),
            local_folder_tree: false,
            local_scanning: false,
            local_scan_status: None,
            download_status: None,
            cache_stats: (0, 0),
            cache_size_input,
            download_folder_input,
            extra_import_input,
            local_folders_input,
            excluded_folders_input,
            playlists: Vec::new(),
            selected_playlist_tracks: Vec::new(),
            selected_playlist_id: None,
            new_playlist_name: String::new(),
            stats: Vec::new(),
            stats_range: StatsRange::AllTime,
            listening_summary: ListeningSummary::default(),
            active_lyrics: None,
            lyrics_loading: false,
            lyrics_scroll_id: scrollable::Id::new("echo-lyrics-scroll"),
            last_lyric_idx: None,
            search_input_id: text_input::Id::new("echo-top-search"),
            pre_mute_volume,
            rounded_covers: HashMap::new(),
            circle_covers: HashMap::new(),
            blurred_covers: HashMap::new(),
            cover_bytes: HashMap::new(),
            loading_covers: HashSet::new(),
            shelf_offsets: HashMap::new(),
            track_menu: None,
            anim_phase: 0.0,
            spectrum_bands: [0.0; 8],
            mini_player: false,
            was_maximized_before_mini: false,
            normal_window_size: (1360.0, 840.0),
            recognizing: false,
            recognition_status: None,
            last_recognised: None,
            last_recognised_track: None,
            recognition_history: Vec::new(),
            lastfm_pending_token: None,
            lastfm_status: None,
            scrobble_tracker: None,
            main_window_id: None,
            window_hwnd: None,
            window_size: (1360.0, 840.0),
            window_maximized: false,
            smtc: None,
            smtc_requested: false,
            listener_ctx: ListenerContext::default(),
            backup_status: None,
            repair_running: false,
            repair_queue: Vec::new(),
            repair_total: 0,
            repair_done: 0,
            repair_fixed: 0,
            repair_consecutive_failures: 0,
            repair_status: None,
            missing_loudness_count: 0,
            toast: None,
        };

        app.refresh_db_views();
        let initial_urls: Vec<String> = app
            .stats
            .iter()
            .take(10)
            .filter_map(|st| st.track.thumbnail_url.clone())
            .chain(
                app.liked_tracks
                    .iter()
                    .take(10)
                    .filter_map(|t| t.thumbnail_url.clone()),
            )
            .collect();
        let mut tasks = vec![app.load_home_task()];
        if app.settings.auto_scan_local_on_startup {
            tasks.push(app.update(Message::ScanLocalMedia));
        }
        for url in initial_urls {
            tasks.push(app.fetch_cover_if_needed(Some(&url)));
        }
        (app, Task::batch(tasks))
    }

    fn theme(&self) -> Theme {
        to_iced_theme(&self.palette)
    }

    fn subscription(&self) -> Subscription<Message> {
        let animate = matches!(self.status, PlaybackStatus::Playing)
            || self.home_loading
            || self.explore_loading
            || self.detail_loading.is_some();
        let mut subs = vec![
            iced::time::every(Duration::from_millis(250)).map(|_| Message::Tick),
            iced::event::listen_with(|event, status, _id| match event {
                iced::Event::Keyboard(keyboard::Event::KeyPressed { key, modifiers, .. }) => {
                    Some(Message::KeyPressed(
                        key,
                        modifiers,
                        matches!(status, iced::event::Status::Captured),
                    ))
                }
                _ => None,
            }),
            iced::window::open_events().map(Message::WindowOpened),
            iced::window::resize_events().map(|(_id, size)| Message::WindowResized(size)),
        ];
        if animate {
            subs.push(iced::time::every(Duration::from_millis(90)).map(|_| Message::AnimTick));
        }
        Subscription::batch(subs)
    }

    fn set_toast(&mut self, msg: impl Into<String>) {
        self.toast = Some((msg.into(), Instant::now()));
    }

    fn sync_lyrics_scroll(&mut self) -> Task<Message> {
        if let Some(res) = &self.active_lyrics {
            let is_lyrics_tab = self.right_tab != RightTab::Queue;
            let lyrics_visible = self.show_expanded
                || (self.mini_player && self.window_size.1 > 280.0 && is_lyrics_tab)
                || (self.show_right_rail && self.right_tab == RightTab::Lyrics);
            if !res.synced.is_empty() && lyrics_visible {
                let idx = res.active_index(self.position_ms);
                if idx != self.last_lyric_idx {
                    self.last_lyric_idx = idx;
                    let count = res.synced.len();
                    let denom = count.saturating_sub(1).max(1) as f32;
                    let y = match idx {
                        Some(i) => (i as f32 / denom).clamp(0.0, 1.0),
                        None => 0.0,
                    };
                    return scrollable::snap_to(
                        self.lyrics_scroll_id.clone(),
                        scrollable::RelativeOffset { x: 0.0, y },
                    );
                }
            }
        }
        Task::none()
    }

    fn refresh_db_views(&mut self) {
        self.liked_tracks = self.db.liked_tracks().unwrap_or_default();
        self.history_tracks = self.db.recent_history(100).unwrap_or_default();
        self.downloaded_tracks = self.db.downloaded_tracks().unwrap_or_default();
        self.downloaded_id_set = self
            .downloaded_tracks
            .iter()
            .map(|(t, _, _)| t.id.clone())
            .collect();
        self.local_tracks = self.db.local_tracks().unwrap_or_default();
        self.cache_stats = AppSettings::stream_cache_dir()
            .map(|d| cache_stats(&d))
            .unwrap_or((0, 0));
        self.playlists = self.db.playlists().unwrap_or_default();
        self.recognition_history = self.db.recognition_history(100).unwrap_or_default();
        self.stats = self.db.top_stats(50).unwrap_or_default();
        self.refresh_stats_summary();
        self.listener_ctx.seed_from_taste(
            &self.liked_tracks,
            &self.listening_summary.top_artists,
            &self.history_tracks,
        );
        self.missing_loudness_count = self
            .db
            .tracks_missing_loudness()
            .map(|v| v.len())
            .unwrap_or(0);
    }

    fn refresh_stats_summary(&mut self) {
        self.listening_summary = self
            .db
            .listening_summary(self.stats_range.days(), 50)
            .unwrap_or_default();
    }

    fn prefetch_stats_covers(&mut self) -> Task<Message> {
        let mut urls = Vec::new();
        for st in self.listening_summary.top_tracks.iter().take(12) {
            if let Some(u) = &st.track.thumbnail_url {
                urls.push(u.clone());
            }
        }
        for ar in self.listening_summary.top_artists.iter().take(8) {
            if let Some(u) = &ar.thumbnail_url {
                urls.push(u.clone());
            }
        }
        for al in self.listening_summary.top_albums.iter().take(6) {
            if let Some(u) = &al.thumbnail_url {
                urls.push(u.clone());
            }
        }
        Task::batch(
            urls.into_iter()
                .map(|u| self.fetch_cover_if_needed(Some(&u)))
                .collect::<Vec<_>>(),
        )
    }

    fn load_home_task(&self) -> Task<Message> {
        let yt = self.yt.clone();
        let params = self.home_active_chip.clone();
        let base_task = Task::perform(
            async move {
                yt.home_page(params.as_deref(), None)
                    .await
                    .map_err(|e| e.to_string())
            },
            |result| Message::HomePageLoaded {
                append: false,
                result,
            },
        );
        if self.home_active_chip.is_none() {
            Task::batch([base_task, self.load_taste_shelves_task()])
        } else {
            base_task
        }
    }

    fn load_taste_shelves_task(&self) -> Task<Message> {
        let disliked_artists: HashSet<String> = self
            .listener_ctx
            .skipped_artists
            .iter()
            .filter(|(art, &cnt)| cnt >= 2 && !self.listener_ctx.liked_artists.contains(*art))
            .map(|(art, _)| art.clone())
            .collect();

        let mut seed_artists: Vec<(Option<String>, String)> = Vec::new();
        let mut seen_artists = HashSet::new();
        for ar in &self.listening_summary.top_artists {
            let key = ar.name.trim().to_lowercase();
            if !key.is_empty()
                && !self.listener_ctx.is_disliked_artist(&ar.name)
                && seen_artists.insert(key)
            {
                seed_artists.push((ar.id.clone(), ar.name.clone()));
            }
        }
        for t in self.liked_tracks.iter().chain(self.history_tracks.iter()) {
            if let Some(a) = t.artists.first() {
                let key = a.name.trim().to_lowercase();
                if !key.is_empty()
                    && !self.listener_ctx.is_disliked_artist(&a.name)
                    && seen_artists.insert(key)
                {
                    seed_artists.push((a.id.clone(), a.name.clone()));
                }
            }
        }

        let fallback_heavy: Vec<BrowseItem> = self
            .listening_summary
            .top_tracks
            .iter()
            .filter(|st| !self.listener_ctx.is_disliked_artist(st.track.primary_artist()))
            .take(12)
            .map(|st| BrowseItem::Track(st.track.clone()))
            .collect();

        let cursor = self.home_seed_cursor;
        let artist_seed = (!seed_artists.is_empty())
            .then(|| seed_artists[cursor % seed_artists.len()].clone());
        let yt = self.yt.clone();
        let db = self.db.clone();
        let ctx = self.listener_ctx.clone();

        Task::perform(
            async move {
                let initial = db
                    .build_home_recommendations(&disliked_artists, 16, cursor)
                    .ok();

                let mut fetch_ids: Vec<String> = initial
                    .as_ref()
                    .map(|o| o.uncached_seed_ids.clone())
                    .unwrap_or_default();
                if fetch_ids.is_empty() {
                    if let Some(seeds) = initial.as_ref().map(|o| &o.seeds) {
                        if !seeds.is_empty() {
                            fetch_ids.push(seeds[cursor % seeds.len()].id.clone());
                        }
                    }
                }
                let s0 = fetch_ids.first().cloned();
                let s1 = fetch_ids.get(1).cloned();
                let s2 = fetch_ids.get(2).cloned();

                let rel0_fut = async {
                    match s0.as_deref() {
                        Some(id) => Some((id.to_string(), yt.related_for_video(id).await.ok())),
                        None => None,
                    }
                };
                let radio0_fut = async {
                    match s0.as_deref() {
                        Some(id) => Some((id.to_string(), yt.next_radio(id, None).await.ok())),
                        None => None,
                    }
                };
                let rel1_fut = async {
                    match s1.as_deref() {
                        Some(id) => Some((id.to_string(), yt.related_for_video(id).await.ok())),
                        None => None,
                    }
                };
                let rel2_fut = async {
                    match s2.as_deref() {
                        Some(id) => Some((id.to_string(), yt.related_for_video(id).await.ok())),
                        None => None,
                    }
                };
                let artist_fut = async {
                    match artist_seed {
                        Some((Some(ref bid), ref name)) => {
                            let page = yt.artist(bid).await.ok();
                            (Some(name.clone()), page, Vec::new())
                        }
                        Some((None, ref name)) => {
                            let items = yt.search(name, SearchFilter::Albums).await.unwrap_or_default();
                            (Some(name.clone()), None, items)
                        }
                        None => (None, None, Vec::new()),
                    }
                };

                let (rel0, radio0, rel1, rel2, (artist_name_opt, artist_page_opt, fallback_items)) =
                    tokio::join!(rel0_fut, radio0_fut, rel1_fut, rel2_fut, artist_fut);

                let mut primary_rel = None;
                for (seed_id, page) in [rel0, rel1, rel2].into_iter().flatten() {
                    if let Some(p) = page {
                        if !p.tracks.is_empty() {
                            let _ = db.save_related_edges(&seed_id, &p.tracks, 0);
                        }
                        if primary_rel.is_none() {
                            primary_rel = Some(p);
                        }
                    }
                }
                if let Some((seed_id, Some(radio_tracks))) = radio0 {
                    if !radio_tracks.is_empty() {
                        let _ = db.save_related_edges(&seed_id, &radio_tracks, 1);
                    }
                }

                let engine_out = db
                    .build_home_recommendations(&disliked_artists, 16, cursor)
                    .ok()
                    .or(initial);

                let mut sections: Vec<HomeSection> = Vec::new();
                if let Some(out) = engine_out {
                    if !out.quick_picks.is_empty() {
                        let pick_ids: Vec<String> =
                            out.quick_picks.iter().map(|c| c.track.id.clone()).collect();
                        let _ = db.record_home_impressions(&pick_ids);
                        sections.push(HomeSection {
                            title: "Quick Picks for You".to_string(),
                            items: out
                                .quick_picks
                                .into_iter()
                                .map(|c| BrowseItem::Track(c.track))
                                .collect(),
                        });
                    }
                    if out.discover.len() >= 3 {
                        sections.push(HomeSection {
                            title: "Discover Something New".to_string(),
                            items: out.discover.into_iter().map(BrowseItem::Track).collect(),
                        });
                    }
                    let heavy: Vec<BrowseItem> = if out.heavy_rotation.len() >= 3 {
                        out.heavy_rotation
                            .into_iter()
                            .map(BrowseItem::Track)
                            .collect()
                    } else {
                        fallback_heavy.clone()
                    };
                    if heavy.len() >= 3 {
                        sections.push(HomeSection {
                            title: "Heavy Rotation".to_string(),
                            items: heavy,
                        });
                    }
                    if !out.forgotten_favorites.is_empty() {
                        sections.push(HomeSection {
                            title: "Forgotten Favorites".to_string(),
                            items: out
                                .forgotten_favorites
                                .into_iter()
                                .map(BrowseItem::Track)
                                .collect(),
                        });
                    }
                } else if fallback_heavy.len() >= 3 {
                    sections.push(HomeSection {
                        title: "Heavy Rotation".to_string(),
                        items: fallback_heavy,
                    });
                }

                if let Some(rel) = primary_rel {
                    let playlists: Vec<BrowseItem> = rel
                        .playlists
                        .into_iter()
                        .filter(|it| !ctx.is_disliked_artist(&item_artist_hint(it)))
                        .take(12)
                        .collect();
                    if !playlists.is_empty() {
                        sections.push(HomeSection {
                            title: "Mixes Picked for You".to_string(),
                            items: playlists,
                        });
                    }
                    let artists: Vec<BrowseItem> = rel
                        .artists
                        .into_iter()
                        .filter(|it| !ctx.is_disliked_artist(it.title()))
                        .take(12)
                        .collect();
                    if !artists.is_empty() {
                        sections.push(HomeSection {
                            title: "Artists for You".to_string(),
                            items: artists,
                        });
                    }
                }

                if let Some(name) = artist_name_opt {
                    let mut items = Vec::new();
                    if let Some(page) = artist_page_opt {
                        for sec in page.sections {
                            for it in sec.items {
                                if !ctx.is_disliked_artist(&item_artist_hint(&it)) {
                                    items.push(it);
                                }
                                if items.len() >= 14 {
                                    break;
                                }
                            }
                            if items.len() >= 14 {
                                break;
                            }
                        }
                    } else {
                        for it in fallback_items {
                            if !ctx.is_disliked_artist(&item_artist_hint(&it)) {
                                items.push(it);
                            }
                            if items.len() >= 12 {
                                break;
                            }
                        }
                    }
                    if !items.is_empty() {
                        sections.push(HomeSection {
                            title: format!("More from {}", ellipsize(&name, 26)),
                            items,
                        });
                    }
                }

                sections
            },
            Message::TasteShelvesLoaded,
        )
    }

    fn rebuild_home_sections(&mut self) {
        let mut combined = Vec::new();
        let mut seen_titles = HashSet::new();
        let mut seen_versions = HashSet::new();
        let mut seen_keys = HashSet::new();
        if self.home_active_chip.is_none() {
            for sec in &self.home_taste_sections {
                if let Some(ranked) = rank_home_section(
                    sec,
                    &self.listener_ctx,
                    true,
                    &mut seen_versions,
                    &mut seen_keys,
                ) {
                    seen_titles.insert(ranked.title.to_lowercase());
                    combined.push(ranked);
                }
            }
        }
        for sec in &self.home_global_sections {
            if let Some(ranked) = rank_home_section(
                sec,
                &self.listener_ctx,
                false,
                &mut seen_versions,
                &mut seen_keys,
            ) {
                if seen_titles.insert(ranked.title.to_lowercase()) {
                    combined.push(ranked);
                }
            }
        }
        self.home_sections = combined;

        let mut quick = Vec::new();
        let mut seen_tiles = HashSet::new();
        let mut seen_tile_versions = HashSet::new();
        if self.home_active_chip.is_none() {
            for t in self.history_tracks.iter().chain(self.liked_tracks.iter()) {
                if !self.listener_ctx.is_disliked_artist(t.primary_artist())
                    && !is_preview_title(&t.title)
                {
                    let vk = base_version_key(&t.title, t.primary_artist());
                    let it = BrowseItem::Track(t.clone());
                    if seen_tile_versions.insert(vk) && seen_tiles.insert(it.stable_key()) {
                        quick.push(it);
                    }
                    if quick.len() >= 3 {
                        break;
                    }
                }
            }
        }
        for sec in &self.home_sections {
            for it in &sec.items {
                let vk = item_version_key(it);
                let ok_ver = vk.is_none_or(|k| seen_tile_versions.insert(k));
                if ok_ver && seen_tiles.insert(it.stable_key()) {
                    quick.push(it.clone());
                }
                if quick.len() >= 8 {
                    break;
                }
            }
            if quick.len() >= 8 {
                break;
            }
        }
        self.home_quick_tiles = quick;
    }

    fn push_nav_history(&mut self) {
        self.nav_forward.clear();
        let snap = NavSnapshot {
            tab: self.tab,
            library_tab: self.library_tab,
            viewing_detail: self.viewing_detail.clone(),
            show_expanded: self.show_expanded,
        };
        let is_dup = self.nav_history.last().is_some_and(|prev| {
            prev.tab == snap.tab
                && prev.library_tab == snap.library_tab
                && prev.show_expanded == snap.show_expanded
                && prev.viewing_detail.is_none()
                && snap.viewing_detail.is_none()
        });
        if !is_dup {
            self.nav_history.push(snap);
            if self.nav_history.len() > 48 {
                self.nav_history.remove(0);
            }
        }
    }

    fn clear_detail(&mut self) {
        self.viewing_detail = None;
        self.detail_loading = None;
    }

    fn active_playlist_id(&self) -> Option<&str> {
        match &self.viewing_detail {
            Some(DetailPage::Playlist(pl)) => Some(&pl.id),
            _ => None,
        }
    }

    fn fetch_cover_if_needed(&mut self, url: Option<&str>) -> Task<Message> {
        static SEM: std::sync::OnceLock<Arc<tokio::sync::Semaphore>> = std::sync::OnceLock::new();
        let sem = Arc::clone(SEM.get_or_init(|| Arc::new(tokio::sync::Semaphore::new(6))));

        let Some(url) = url.filter(|u| !u.is_empty()) else {
            return Task::none();
        };
        if self.rounded_covers.contains_key(url) || self.loading_covers.contains(url) {
            return Task::none();
        }
        let url_owned = url.to_string();
        self.loading_covers.insert(url_owned.clone());
        let http = self.http.clone();
        Task::perform(
            async move {
                let bytes = if url_owned.starts_with("http://") || url_owned.starts_with("https://")
                {
                    let _permit = sem.acquire().await.ok();
                    match http.get(&url_owned).send().await {
                        Ok(r) if r.status().is_success() => r.bytes().await.ok().map(|b| b.to_vec()),
                        _ => None,
                    }
                } else {
                    tokio::fs::read(&url_owned).await.ok()
                };
                let (rounded, circle, blurred) = if let Some(ref raw) = bytes {
                    let raw_clone = raw.clone();
                    tokio::task::spawn_blocking(move || {
                        let r = rounded_image_handle(&raw_clone, 256, 14.0, false);
                        let c = rounded_image_handle(&raw_clone, 160, 80.0, true);
                        let b = blurred_image_handle(&raw_clone);
                        (r, c, b)
                    })
                    .await
                    .unwrap_or((None, None, None))
                } else {
                    (None, None, None)
                };
                (url_owned, rounded, circle, blurred, bytes)
            },
            |(url, rounded, circle, blurred, bytes)| Message::ArtworkLoaded {
                url,
                rounded,
                circle,
                blurred,
                bytes,
            },
        )
    }

    fn fetch_lyrics_for_track(&self, track: &Track) -> Task<Message> {
        let lf = Arc::clone(&self.lyrics_fetcher);
        let t_lyrics = track.clone();
        let better_enabled = self.settings.better_lyrics_enabled;
        Task::perform(
            async move {
                let res = lf
                    .fetch_with_options(
                        &t_lyrics.title,
                        t_lyrics.primary_artist(),
                        t_lyrics.duration_secs,
                        t_lyrics.album.as_ref().map(|a| a.title.as_str()),
                        t_lyrics.local_path.as_deref(),
                        better_enabled,
                    )
                    .await;
                (t_lyrics.id, res)
            },
            |(track_id, lyrics)| Message::LyricsLoaded { track_id, lyrics },
        )
    }

    fn flush_active_play_duration(&self) {
        if let Some(tr) = &self.scrobble_tracker {
            let played_secs = tr.accumulated_ms.max(self.position_ms) / 1000;
            if played_secs > 0 {
                let _ = self
                    .db
                    .update_latest_play_duration(&tr.track.id, played_secs as u32);
            }
        }
    }

    fn start_playing_current(&mut self) -> Task<Message> {
        self.flush_active_play_duration();
        let Some(track) = self.queue.current().map(|q| q.track.clone()) else {
            return Task::none();
        };

        self.status = PlaybackStatus::Loading;
        self.position_ms = 0;
        self.duration_ms = track.duration_secs.unwrap_or(0) as u64 * 1000;
        self.active_lyrics = None;
        self.last_lyric_idx = None;
        self.lyrics_loading = true;
        self.toast = None;

        self.scrobble_tracker = Some(ScrobbleTracker {
            track: track.clone(),
            started_at_secs: chrono::Utc::now().timestamp(),
            accumulated_ms: 0,
            scrobbled: false,
        });
        self.listener_ctx.record_play(&track, track.liked);

        let mut tasks = Vec::new();
        tasks.push(self.fetch_cover_if_needed(track.thumbnail_url.as_deref()));

        if let Some(url) = &track.thumbnail_url {
            if let Some(bytes) = self.cover_bytes.get(url) {
                if let Some(p) = DynamicPalette::from_image_bytes(bytes, self.settings.theme_mode) {
                    self.palette = p;
                }
            }
        }

        tasks.push(self.fetch_lyrics_for_track(&track));

        if self.settings.lastfm_scrobbling_enabled {
            if let Some(sk) = self.settings.lastfm_session_key.clone() {
                let client = LastFmClient::new(
                    self.settings.lastfm_api_key.clone(),
                    self.settings.lastfm_api_secret.clone(),
                );
                if client.is_configured() {
                    let t = track.clone();
                    tokio::spawn(async move {
                        let _ = client
                            .update_now_playing(
                                &sk,
                                t.primary_artist(),
                                &t.title,
                                t.album.as_ref().map(|a| a.title.as_str()),
                                t.duration_secs,
                            )
                            .await;
                    });
                }
            }
        }

        if track.is_local {
            if let Some(path) = &track.local_path {
                let _ = self.db.record_play(&track, 0);
                self.refresh_db_views();
                self.audio.play(AudioSourceSpec::File {
                    path: path.into(),
                    loudness_db: track.loudness_db,
                });
            }
            return Task::batch(tasks);
        }

        if let Some(dl_path) = self.db.get_download_path(&track.id) {
            let _ = self.db.record_play(&track, 0);
            self.refresh_db_views();
            self.audio.play(AudioSourceSpec::File {
                path: dl_path,
                loudness_db: track.loudness_db,
            });
            return Task::batch(tasks);
        }

        if self.settings.max_cache_size_mb > 0 {
            if let Some(cached_path) = AppSettings::stream_cache_dir()
                .ok()
                .and_then(|d| lookup_cached_stream(&d, &track.id))
            {
                let _ = self.db.record_play(&track, 0);
                self.refresh_db_views();
                self.audio.play(AudioSourceSpec::File {
                    path: cached_path,
                    loudness_db: track.loudness_db,
                });
                return Task::batch(tasks);
            }
        }

        let yt = self.yt.clone();
        let quality = self.settings.audio_quality;
        let t_resolve = track.clone();
        tasks.push(Task::perform(
            async move {
                let res = yt
                    .resolve_stream(&t_resolve.id, quality)
                    .await
                    .map_err(|e| e.to_string());
                (t_resolve, res)
            },
            |(track, result)| Message::StreamResolved { track, result },
        ));

        Task::batch(tasks)
    }

    fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::SelectTab(tab) => {
                if self.tab != tab || self.viewing_detail.is_some() || self.show_expanded {
                    self.push_nav_history();
                }
                self.tab = tab;
                self.show_expanded = false;
                self.clear_detail();
                if matches!(tab, NavTab::Explore)
                    && self.explore_page.is_none()
                    && !self.explore_loading
                {
                    return self.update(Message::LoadExplore);
                }
                if matches!(
                    tab,
                    NavTab::Playlists | NavTab::Stats | NavTab::Recognize | NavTab::Settings
                ) {
                    self.refresh_db_views();
                    if matches!(tab, NavTab::Stats) {
                        return self.prefetch_stats_covers();
                    }
                }
                Task::none()
            }
            Message::SelectStatsRange(range) => {
                self.stats_range = range;
                self.refresh_stats_summary();
                self.prefetch_stats_covers()
            }
            Message::SearchExact(query) => {
                self.search_query = query;
                self.update(Message::SubmitSearch)
            }
            Message::SelectLibraryTab(lt) => {
                if self.tab != NavTab::Playlists
                    || self.library_tab != lt
                    || self.viewing_detail.is_some()
                    || self.show_expanded
                {
                    self.push_nav_history();
                }
                self.tab = NavTab::Playlists;
                self.show_expanded = false;
                self.clear_detail();
                if let LibraryTab::Playlist(idx) = lt {
                    if let Some(pl) = self.playlists.get(idx) {
                        self.selected_playlist_id = Some(pl.id.clone());
                        self.selected_playlist_tracks =
                            self.db.playlist_tracks(&pl.id).unwrap_or_default();
                    }
                }
                self.library_tab = lt;
                self.refresh_db_views();
                Task::none()
            }
            Message::LibraryFilterSelected(f) => {
                self.library_filter = if self.library_filter == f {
                    LibraryFilter::All
                } else {
                    f
                };
                Task::none()
            }
            Message::CardHovered(id) => {
                self.hovered_card = id;
                Task::none()
            }
            Message::RowHovered(id) => {
                self.hovered_row = id;
                Task::none()
            }
            Message::ShelfScroll { key, delta } => {
                let entry = self.shelf_offsets.entry(key).or_insert(0);
                if delta < 0 {
                    *entry = entry.saturating_sub(delta.unsigned_abs());
                } else {
                    *entry = entry.saturating_add(delta as usize);
                }
                Task::none()
            }
            Message::OpenTrackMenu(t) => {
                self.track_menu = Some(t);
                Task::none()
            }
            Message::CloseTrackMenu => {
                self.track_menu = None;
                Task::none()
            }
            Message::AddTrackToPlaylist { playlist_id, track } => {
                self.track_menu = None;
                let _ = self.db.add_to_playlist(&playlist_id, &track);
                self.refresh_db_views();
                if self.selected_playlist_id.as_deref() == Some(playlist_id.as_str()) {
                    self.selected_playlist_tracks =
                        self.db.playlist_tracks(&playlist_id).unwrap_or_default();
                }
                self.set_toast("Added to playlist");
                Task::none()
            }
            Message::AnimTick => {
                self.anim_phase = (self.anim_phase + 0.35) % std::f32::consts::TAU;
                if matches!(self.status, PlaybackStatus::Playing) {
                    self.spectrum_bands = self.audio.spectrum_bands();
                    if self.seeking_ms.is_none() {
                        let snap = self.audio.snapshot();
                        self.position_ms = snap.position_ms;
                        if snap.duration_ms > 0 {
                            self.duration_ms = snap.duration_ms;
                        }
                        return self.sync_lyrics_scroll();
                    }
                } else {
                    for b in &mut self.spectrum_bands {
                        *b *= 0.65;
                    }
                }
                Task::none()
            }
            Message::SelectRightTab(rt) => {
                self.right_tab = rt;
                self.show_right_rail = true;
                if rt == RightTab::Lyrics {
                    self.last_lyric_idx = None;
                    return self.sync_lyrics_scroll();
                }
                Task::none()
            }
            Message::ToggleRightTab(rt) => {
                if self.show_right_rail && self.right_tab == rt {
                    self.show_right_rail = false;
                } else {
                    self.right_tab = rt;
                    self.show_right_rail = true;
                    if rt == RightTab::Lyrics {
                        self.last_lyric_idx = None;
                        return self.sync_lyrics_scroll();
                    }
                }
                Task::none()
            }
            Message::ToggleLeftSidebar => {
                self.left_collapsed = !self.left_collapsed;
                Task::none()
            }
            Message::ExpandAndNewPlaylist => {
                self.left_collapsed = false;
                self.show_new_playlist_input = true;
                Task::none()
            }
            Message::OpenTrackArtist(track) => {
                self.track_menu = None;
                if let Some(artist) = track.artists.iter().find(|a| a.id.is_some()) {
                    if let Some(id) = artist.id.clone() {
                        return self.update(Message::OpenArtist {
                            browse_id: id,
                            name: artist.name.clone(),
                        });
                    }
                }
                let q = track.primary_artist().trim().to_string();
                if !q.is_empty() {
                    self.push_nav_history();
                    self.search_query = q;
                    self.search_filter = SearchFilter::Artists;
                    return self.update(Message::SubmitSearch);
                }
                Task::none()
            }
            Message::OpenTrackAlbum(track) => {
                self.track_menu = None;
                if let Some(album) = track.album.as_ref() {
                    if let Some(id) = album.id.clone() {
                        return self.update(Message::OpenAlbum {
                            browse_id: id,
                            title: album.title.clone(),
                        });
                    }
                    if !album.title.trim().is_empty() {
                        self.push_nav_history();
                        self.search_query = album.title.clone();
                        self.search_filter = SearchFilter::Albums;
                        return self.update(Message::SubmitSearch);
                    }
                }
                Task::none()
            }
            Message::ToggleRightSidebar => {
                self.show_right_rail = !self.show_right_rail;
                Task::none()
            }
            Message::ToggleNewPlaylistInput => {
                self.show_new_playlist_input = !self.show_new_playlist_input;
                Task::none()
            }
            Message::Tick => {
                let now = Instant::now();
                let dt_ms = now.duration_since(self.last_tick).as_millis() as u64;
                self.last_tick = now;

                let mut extra_tasks = Vec::new();
                if !self.smtc_requested {
                    self.smtc_requested = true;
                    extra_tasks.push(iced::window::get_oldest().then(|id_opt| match id_opt {
                        Some(id) => iced::window::run_with_handle(id, |h| {
                            use iced::window::raw_window_handle::RawWindowHandle;
                            match h.as_raw() {
                                RawWindowHandle::Win32(w) => {
                                    Message::WindowHandleReady(Some(w.hwnd.get()))
                                }
                                _ => Message::WindowHandleReady(None),
                            }
                        }),
                        None => Task::done(Message::WindowHandleReady(None)),
                    }));
                }

                if let Some(ref mut smtc) = self.smtc {
                    let actions = smtc.poll_actions();
                    let cur_track = self.queue.current().map(|c| c.track.clone());
                    smtc.sync(
                        cur_track.as_ref(),
                        &self.status,
                        self.position_ms,
                        self.duration_ms,
                    );
                    for act in actions {
                        match act {
                            smtc::SmtcAction::Play => self.audio.resume(),
                            smtc::SmtcAction::Pause => self.audio.pause(),
                            smtc::SmtcAction::Toggle => {
                                extra_tasks.push(self.update(Message::TogglePlayPause));
                            }
                            smtc::SmtcAction::Next => {
                                extra_tasks.push(self.update(Message::NextTrack));
                            }
                            smtc::SmtcAction::Previous => {
                                extra_tasks.push(self.update(Message::PrevTrack));
                            }
                            smtc::SmtcAction::Stop => self.audio.stop(),
                            smtc::SmtcAction::SeekForward => {
                                let t = (self.position_ms + 5_000).min(self.duration_ms.max(1));
                                extra_tasks.push(self.update(Message::SeekToMs(t)));
                            }
                            smtc::SmtcAction::SeekBackward => {
                                let t = self.position_ms.saturating_sub(5_000);
                                extra_tasks.push(self.update(Message::SeekToMs(t)));
                            }
                            smtc::SmtcAction::SetPositionMs(ms) => {
                                extra_tasks.push(self.update(Message::SeekToMs(ms)));
                            }
                        }
                    }
                }

                let snap = self.audio.snapshot();
                if let PlaybackStatus::Error(ref err) = snap.status {
                    if !matches!(&self.status, PlaybackStatus::Error(prev) if prev == err) {
                        self.set_toast(format!("Audio error: {err}"));
                        if let Some(cur) = self.queue.current() {
                            self.yt.invalidate_stream(&cur.track.id);
                        }
                    }
                }
                if let Some((_, shown_at)) = &self.toast {
                    if now.duration_since(*shown_at) >= Duration::from_secs(4) {
                        self.toast = None;
                    }
                }
                self.status = snap.status;
                if self.seeking_ms.is_none() {
                    self.position_ms = snap.position_ms;
                }
                if snap.duration_ms > 0 {
                    self.duration_ms = snap.duration_ms;
                }

                // Auto-scroll synced lyrics when the active line advances.
                let scroll_task = self.sync_lyrics_scroll();

                if matches!(self.status, PlaybackStatus::Playing) {
                    if let Some(tr) = self.scrobble_tracker.as_mut() {
                        tr.accumulated_ms += dt_ms.min(1000);
                        let dur = tr
                            .track
                            .duration_secs
                            .unwrap_or((self.duration_ms / 1000) as u32);
                        if !tr.scrobbled && qualifies_for_scrobble(tr.accumulated_ms, dur) {
                            tr.scrobbled = true;
                            let track_copy = tr.track.clone();
                            let started = tr.started_at_secs;
                            let _ = self.db.update_latest_play_duration(
                                &track_copy.id,
                                (tr.accumulated_ms / 1000) as u32,
                            );
                            self.refresh_db_views();

                            if self.settings.lastfm_scrobbling_enabled {
                                if let Some(sk) = self.settings.lastfm_session_key.clone() {
                                    let client = LastFmClient::new(
                                        self.settings.lastfm_api_key.clone(),
                                        self.settings.lastfm_api_secret.clone(),
                                    );
                                    if client.is_configured() {
                                        let db = self.db.clone();
                                        tokio::spawn(async move {
                                            let res = client
                                                .scrobble(
                                                    &sk,
                                                    track_copy.primary_artist(),
                                                    &track_copy.title,
                                                    started,
                                                    track_copy.album.as_ref().map(|a| a.title.as_str()),
                                                    Some(dur),
                                                )
                                                .await;
                                            if res.is_err() {
                                                let _ = db.enqueue_pending_scrobble(
                                                    track_copy.primary_artist(),
                                                    &track_copy.title,
                                                    track_copy.album.as_ref().map(|a| a.title.as_str()),
                                                    Some(dur),
                                                    started,
                                                );
                                            }
                                        });
                                    }
                                }
                            }
                        }
                    }
                }

                // Sleep timer + Transition fade gain envelope
                let mut sleep_factor = 1.0f32;
                let mut sleep_triggered = false;
                match self.sleep_timer {
                    SleepTimerState::Off => {}
                    SleepTimerState::Countdown(deadline) => {
                        if now >= deadline {
                            self.sleep_timer = SleepTimerState::Off;
                            sleep_triggered = true;
                        } else {
                            let rem_ms = deadline.duration_since(now).as_millis() as u64;
                            if rem_ms < 10_000 {
                                sleep_factor = (rem_ms as f32 / 10_000.0).clamp(0.0, 1.0);
                            }
                        }
                    }
                    SleepTimerState::EndOfSong => {
                        if self.duration_ms > 10_000 && matches!(self.status, PlaybackStatus::Playing) {
                            let rem_ms = self.duration_ms.saturating_sub(self.position_ms);
                            if rem_ms < 10_000 {
                                sleep_factor = (rem_ms as f32 / 10_000.0).clamp(0.0, 1.0);
                            }
                        }
                    }
                }
                if sleep_triggered {
                    self.audio.pause();
                    self.audio.set_fade_factor(1.0);
                    self.fade_in_remaining_ms = 0;
                    self.set_toast("Sleep timer paused playback");
                } else {
                    let fade_len = self.settings.transition_fade_ms;
                    let mut transition_factor = 1.0f32;
                    if fade_len > 0 && matches!(self.status, PlaybackStatus::Playing) {
                        if self.fade_in_remaining_ms > 0 {
                            let elapsed_in = fade_len.saturating_sub(self.fade_in_remaining_ms);
                            transition_factor = (elapsed_in as f32 / fade_len as f32).clamp(0.0, 1.0);
                            self.fade_in_remaining_ms =
                                self.fade_in_remaining_ms.saturating_sub(dt_ms);
                        } else if self.duration_ms > fade_len * 2 {
                            let rem_ms = self.duration_ms.saturating_sub(self.position_ms);
                            if rem_ms < fade_len {
                                transition_factor = (rem_ms as f32 / fade_len as f32).clamp(0.0, 1.0);
                            }
                        }
                    } else {
                        self.fade_in_remaining_ms = 0;
                    }
                    self.audio
                        .set_fade_factor((sleep_factor * transition_factor).clamp(0.0, 1.0));
                }

                extra_tasks.push(scroll_task);
                if snap.finished_seq != self.last_finished_seq {
                    self.last_finished_seq = snap.finished_seq;
                    if matches!(self.sleep_timer, SleepTimerState::EndOfSong) {
                        self.sleep_timer = SleepTimerState::Off;
                        self.audio.set_fade_factor(1.0);
                        self.fade_in_remaining_ms = 0;
                        self.set_toast("Sleep timer stopped after song");
                        return Task::batch(extra_tasks);
                    }
                    if self.queue.advance_next().is_some() {
                        if self.settings.transition_fade_ms > 0 {
                            self.fade_in_remaining_ms = self.settings.transition_fade_ms;
                        }
                        extra_tasks.push(self.start_playing_current());
                        return Task::batch(extra_tasks);
                    } else if self.settings.auto_load_more {
                        if let Some(cur) = self.queue.current().map(|c| c.track.clone()) {
                            let yt = self.yt.clone();
                            extra_tasks.push(Task::perform(
                                async move {
                                    yt.next_radio(&cur.id, None)
                                        .await
                                        .map_err(|e| e.to_string())
                                },
                                Message::RadioLoaded,
                            ));
                            return Task::batch(extra_tasks);
                        }
                    }
                }
                Task::batch(extra_tasks)
            }
            Message::LoadHome => {
                self.home_loading = true;
                self.home_error = None;
                self.home_seed_cursor = self.home_seed_cursor.wrapping_add(1);
                self.load_home_task()
            }
            Message::SelectHomeChip(params) => {
                self.home_active_chip = if self.home_active_chip == params {
                    None
                } else {
                    params
                };
                self.home_loading = true;
                self.home_error = None;
                self.load_home_task()
            }
            Message::LoadMoreHome => {
                let Some(cont) = self.home_continuation.clone() else {
                    return Task::none();
                };
                if self.home_loading_more {
                    return Task::none();
                }
                self.home_loading_more = true;
                let yt = self.yt.clone();
                Task::perform(
                    async move { yt.home_page(None, Some(&cont)).await.map_err(|e| e.to_string()) },
                    |result| Message::HomePageLoaded {
                        append: true,
                        result,
                    },
                )
            }
            Message::HomePageLoaded { append, result } => {
                self.home_loading = false;
                self.home_loading_more = false;
                let mut tasks = Vec::new();
                match result {
                    Ok(page) => {
                        for sec in &page.sections {
                            for item in sec.items.iter().take(12) {
                                tasks.push(self.fetch_cover_if_needed(item.thumbnail_url()));
                            }
                        }
                        if append {
                            self.home_global_sections.extend(page.sections);
                        } else {
                            if !page.chips.is_empty() || self.home_active_chip.is_none() {
                                self.home_chips = page.chips;
                            }
                            self.shelf_offsets.clear();
                            self.home_global_sections = page.sections;
                        }
                        self.rebuild_home_sections();
                        self.home_continuation = page.continuation;
                    }
                    Err(e) if !append => self.home_error = Some(e),
                    Err(e) => self.set_toast(format!("Could not load more: {e}")),
                }
                Task::batch(tasks)
            }
            Message::TasteShelvesLoaded(sections) => {
                let mut tasks = Vec::new();
                for sec in &sections {
                    for item in sec.items.iter().take(12) {
                        tasks.push(self.fetch_cover_if_needed(item.thumbnail_url()));
                    }
                }
                self.home_taste_sections = sections;
                self.rebuild_home_sections();
                Task::batch(tasks)
            }
            Message::LoadExplore => {
                self.explore_loading = true;
                self.explore_error = None;
                let yt = self.yt.clone();
                Task::perform(
                    async move {
                        let explore = yt.explore().await.map_err(|e| e.to_string());
                        let moods = yt.mood_and_genres().await.unwrap_or_default();
                        (explore, moods)
                    },
                    |(explore, moods)| Message::ExploreLoaded { explore, moods },
                )
            }
            Message::ExploreLoaded { explore, moods } => {
                self.explore_loading = false;
                let mut tasks = Vec::new();
                match explore {
                    Ok(page) => {
                        for item in page.new_release_albums.iter().take(20) {
                            tasks.push(self.fetch_cover_if_needed(item.thumbnail_url()));
                        }
                        self.explore_page = Some(page);
                        self.mood_groups = moods;
                    }
                    Err(e) => self.explore_error = Some(e),
                }
                Task::batch(tasks)
            }
            Message::OpenCategory {
                title,
                browse_id,
                params,
            } => {
                self.push_nav_history();
                self.detail_loading = Some(title.clone());
                self.show_expanded = false;
                let yt = self.yt.clone();
                let bid = browse_id.clone();
                let p = (!params.is_empty()).then_some(params);
                Task::perform(
                    async move {
                        yt.browse_category(&bid, p.as_deref())
                            .await
                            .map(|(t, s)| (if t.is_empty() { title } else { t }, s))
                            .map_err(|e| e.to_string())
                    },
                    |result| Message::CategoryLoaded { result },
                )
            }
            Message::CategoryLoaded { result } => {
                self.detail_loading = None;
                match result {
                    Ok((title, sections)) => {
                        let mut tasks = Vec::new();
                        for sec in &sections {
                            for item in sec.items.iter().take(15) {
                                tasks.push(self.fetch_cover_if_needed(item.thumbnail_url()));
                            }
                        }
                        self.viewing_detail = Some(DetailPage::Category { title, sections });
                        return Task::batch(tasks);
                    }
                    Err(e) => self.set_toast(format!("Could not open category: {e}")),
                }
                Task::none()
            }
            Message::OpenPlaylist { id, title } => {
                self.push_nav_history();
                self.show_expanded = false;
                self.detail_loading = Some(title);
                let yt = self.yt.clone();
                Task::perform(
                    async move { yt.playlist_page(&id).await.map_err(|e| e.to_string()) },
                    Message::PlaylistPageLoaded,
                )
            }
            Message::PlaylistPageLoaded(res) => {
                self.detail_loading = None;
                match res {
                    Ok(page) => {
                        let mut tasks: Vec<_> = page
                            .tracks
                            .iter()
                            .take(24)
                            .map(|t| self.fetch_cover_if_needed(t.thumbnail_url.as_deref()))
                            .collect();
                        tasks.push(self.fetch_cover_if_needed(page.thumbnail_url.as_deref()));
                        self.viewing_detail = Some(DetailPage::Playlist(page));
                        return Task::batch(tasks);
                    }
                    Err(e) => self.set_toast(format!("Could not open playlist: {e}")),
                }
                Task::none()
            }
            Message::LoadMorePlaylist => {
                let Some(DetailPage::Playlist(ref pl)) = self.viewing_detail else {
                    return Task::none();
                };
                let Some(cont) = pl.continuation.clone() else {
                    return Task::none();
                };
                if self.playlist_loading_more {
                    return Task::none();
                }
                self.playlist_loading_more = true;
                let id = pl.id.clone();
                let yt = self.yt.clone();
                Task::perform(
                    async move {
                        yt.playlist_continuation(&id, &cont)
                            .await
                            .map_err(|e| e.to_string())
                    },
                    Message::PlaylistContinuationLoaded,
                )
            }
            Message::PlaylistContinuationLoaded(res) => {
                self.playlist_loading_more = false;
                match res {
                    Ok(next_page) => {
                        let mut tasks = Vec::new();
                        let mut new_urls = Vec::new();
                        if let Some(DetailPage::Playlist(ref mut pl)) = self.viewing_detail {
                            let existing: HashSet<String> =
                                pl.tracks.iter().map(|t| t.id.clone()).collect();
                            for t in next_page.tracks {
                                if !existing.contains(&t.id) {
                                    if let Some(u) = t.thumbnail_url.clone() {
                                        new_urls.push(u);
                                    }
                                    pl.tracks.push(t);
                                }
                            }
                            pl.continuation = next_page.continuation;
                        }
                        for u in new_urls {
                            tasks.push(self.fetch_cover_if_needed(Some(&u)));
                        }
                        return Task::batch(tasks);
                    }
                    Err(e) => self.set_toast(format!("Could not load more tracks: {e}")),
                }
                Task::none()
            }
            Message::OpenAlbum { browse_id, title } => {
                self.push_nav_history();
                self.show_expanded = false;
                self.detail_loading = Some(title);
                let yt = self.yt.clone();
                Task::perform(
                    async move { yt.album(&browse_id).await.map_err(|e| e.to_string()) },
                    Message::AlbumLoaded,
                )
            }
            Message::AlbumLoaded(res) => {
                self.detail_loading = None;
                match res {
                    Ok(page) => {
                        let mut tasks: Vec<_> = page
                            .tracks
                            .iter()
                            .take(24)
                            .map(|t| self.fetch_cover_if_needed(t.thumbnail_url.as_deref()))
                            .collect();
                        tasks.push(self.fetch_cover_if_needed(page.thumbnail_url.as_deref()));
                        for ov in page.other_versions.iter().take(10) {
                            tasks.push(self.fetch_cover_if_needed(ov.thumbnail_url()));
                        }
                        self.viewing_detail = Some(DetailPage::Album(page));
                        return Task::batch(tasks);
                    }
                    Err(e) => self.set_toast(format!("Could not open album: {e}")),
                }
                Task::none()
            }
            Message::OpenArtist { browse_id, name } => {
                self.push_nav_history();
                self.show_expanded = false;
                self.detail_loading = Some(name);
                let yt = self.yt.clone();
                Task::perform(
                    async move { yt.artist(&browse_id).await.map_err(|e| e.to_string()) },
                    Message::ArtistLoaded,
                )
            }
            Message::ArtistLoaded(res) => {
                self.detail_loading = None;
                match res {
                    Ok(page) => {
                        let mut tasks = vec![self.fetch_cover_if_needed(page.thumbnail_url.as_deref())];
                        for sec in &page.sections {
                            for it in sec.items.iter().take(12) {
                                tasks.push(self.fetch_cover_if_needed(it.thumbnail_url()));
                            }
                        }
                        self.viewing_detail = Some(DetailPage::Artist {
                            page,
                            subscribed: false,
                        });
                        return Task::batch(tasks);
                    }
                    Err(e) => self.set_toast(format!("Could not open artist: {e}")),
                }
                Task::none()
            }
            Message::OpenArtistItems { browse_id, params } => {
                self.push_nav_history();
                self.detail_loading = Some("Items".to_string());
                let yt = self.yt.clone();
                Task::perform(
                    async move {
                        yt.artist_items(&browse_id, params.as_deref())
                            .await
                            .map_err(|e| e.to_string())
                    },
                    Message::ArtistItemsLoaded,
                )
            }
            Message::ArtistItemsLoaded(res) => {
                self.detail_loading = None;
                match res {
                    Ok((title, items, _continuation)) => {
                        let tasks: Vec<_> = items
                            .iter()
                            .take(24)
                            .map(|it| self.fetch_cover_if_needed(it.thumbnail_url()))
                            .collect();
                        self.viewing_detail = Some(DetailPage::ArtistItems { title, items });
                        return Task::batch(tasks);
                    }
                    Err(e) => self.set_toast(format!("Could not load artist items: {e}")),
                }
                Task::none()
            }
            Message::SubscribeArtist {
                channel_id,
                subscribe,
            } => {
                let yt = self.yt.clone();
                Task::perform(
                    async move {
                        yt.subscribe_channel(&channel_id, subscribe)
                            .await
                            .map_err(|e| e.to_string())
                    },
                    move |result| Message::SubscribeResult {
                        subscribed: subscribe,
                        result,
                    },
                )
            }
            Message::SubscribeResult { subscribed, result } => {
                match result {
                    Ok(()) => {
                        if let Some(DetailPage::Artist { subscribed: s, .. }) =
                            &mut self.viewing_detail
                        {
                            *s = subscribed;
                        }
                        self.set_toast(if subscribed {
                            "Subscribed on YouTube Music"
                        } else {
                            "Unsubscribed"
                        });
                    }
                    Err(e) => self.set_toast(e),
                }
                Task::none()
            }
            Message::NavigateBackDetail => {
                self.detail_loading = None;
                if let Some(prev) = self.nav_history.pop() {
                    self.nav_forward.push(NavSnapshot {
                        tab: self.tab,
                        library_tab: self.library_tab,
                        viewing_detail: self.viewing_detail.take(),
                        show_expanded: self.show_expanded,
                    });
                    self.tab = prev.tab;
                    self.library_tab = prev.library_tab;
                    self.viewing_detail = prev.viewing_detail;
                    self.show_expanded = prev.show_expanded;
                } else {
                    self.viewing_detail = None;
                    self.show_expanded = false;
                }
                Task::none()
            }
            Message::NavigateForwardDetail => {
                self.detail_loading = None;
                if let Some(next) = self.nav_forward.pop() {
                    self.nav_history.push(NavSnapshot {
                        tab: self.tab,
                        library_tab: self.library_tab,
                        viewing_detail: self.viewing_detail.take(),
                        show_expanded: self.show_expanded,
                    });
                    self.tab = next.tab;
                    self.library_tab = next.library_tab;
                    self.viewing_detail = next.viewing_detail;
                    self.show_expanded = next.show_expanded;
                }
                Task::none()
            }
            Message::SearchQueryChanged(q) => {
                self.search_query = q.clone();
                if q.trim().len() < 2 {
                    self.search_suggestions.clear();
                    self.search_dropdown_open = false;
                    return Task::none();
                }
                self.search_dropdown_open = true;
                let yt = self.yt.clone();
                Task::perform(
                    async move { yt.search_suggestions(&q).await.unwrap_or_default() },
                    Message::SuggestionsLoaded,
                )
            }
            Message::SuggestionsLoaded(s) => {
                self.search_suggestions = s;
                if !self.search_suggestions.is_empty() && !self.search_query.trim().is_empty() {
                    self.search_dropdown_open = true;
                }
                Task::none()
            }
            Message::CloseSearchDropdown => {
                self.search_dropdown_open = false;
                Task::none()
            }
            Message::ApplySuggestion(q) => {
                self.search_query = q;
                self.search_suggestions.clear();
                self.search_dropdown_open = false;
                self.tab = NavTab::Search;
                self.show_expanded = false;
                self.clear_detail();
                self.update(Message::SubmitSearch)
            }
            Message::SearchFilterSelected(f) => {
                self.search_filter = f;
                if !self.search_query.trim().is_empty() {
                    self.update(Message::SubmitSearch)
                } else {
                    Task::none()
                }
            }
            Message::SubmitSearch => {
                let q = self.search_query.trim().to_string();
                if q.is_empty() {
                    return Task::none();
                }
                self.tab = NavTab::Search;
                self.show_expanded = false;
                self.search_dropdown_open = false;
                self.clear_detail();
                self.search_loading = true;
                self.search_error = None;
                self.search_continuation = None;
                self.search_suggestions.clear();
                let yt = self.yt.clone();
                let filter = self.search_filter;
                Task::perform(
                    async move { yt.search_page(&q, filter).await.map_err(|e| e.to_string()) },
                    |result| Message::SearchPageLoaded {
                        append: false,
                        result,
                    },
                )
            }
            Message::LoadMoreSearch => {
                let Some(cont) = self.search_continuation.clone() else {
                    return Task::none();
                };
                if self.search_loading_more {
                    return Task::none();
                }
                self.search_loading_more = true;
                let yt = self.yt.clone();
                let filter = self.search_filter;
                Task::perform(
                    async move {
                        yt.search_continuation(&cont, filter)
                            .await
                            .map_err(|e| e.to_string())
                    },
                    |result| Message::SearchPageLoaded {
                        append: true,
                        result,
                    },
                )
            }
            Message::SearchPageLoaded { append, result } => {
                self.search_loading = false;
                self.search_loading_more = false;
                let mut tasks = Vec::new();
                match result {
                    Ok(page) => {
                        for item in page.items.iter().take(25) {
                            tasks.push(self.fetch_cover_if_needed(item.thumbnail_url()));
                        }
                        if append {
                            let existing: HashSet<String> =
                                self.search_results.iter().map(BrowseItem::stable_key).collect();
                            for item in page.items {
                                if !existing.contains(&item.stable_key()) {
                                    self.search_results.push(item);
                                }
                            }
                        } else {
                            self.search_results = page.items;
                        }
                        self.search_continuation = page.continuation;
                    }
                    Err(e) if !append => self.search_error = Some(e),
                    Err(e) => self.set_toast(format!("Load more failed: {e}")),
                }
                Task::batch(tasks)
            }
            Message::PlayTrack(track) => {
                self.queue.replace_with(vec![track.clone()], 0);
                let mut tasks = vec![self.start_playing_current()];
                if self.settings.auto_load_more && !track.is_local {
                    let yt = self.yt.clone();
                    let vid = track.id.clone();
                    tasks.push(Task::perform(
                        async move { yt.next_radio(&vid, None).await.map_err(|e| e.to_string()) },
                        Message::RadioLoaded,
                    ));
                    let lfm = LastFmClient::new(
                        self.settings.lastfm_api_key.clone(),
                        self.settings.lastfm_api_secret.clone(),
                    );
                    if lfm.is_configured() && !track.primary_artist().is_empty() {
                        let yt2 = self.yt.clone();
                        let artist = track.primary_artist().to_string();
                        let title = track.title.clone();
                        tasks.push(Task::perform(
                            async move {
                                let sim = lfm.similar(&artist, &title, 5).await.unwrap_or_default();
                                let mut resolved = Vec::new();
                                for s in sim.into_iter().take(3) {
                                    let q = format!(
                                        "{} {}",
                                        s.name,
                                        s.artist
                                            .as_ref()
                                            .and_then(|a| a.name.as_deref())
                                            .unwrap_or("")
                                    );
                                    if let Ok(items) = yt2.search(&q, SearchFilter::Songs).await {
                                        if let Some(t) = items.into_iter().find_map(|it| match it {
                                            BrowseItem::Track(tr) => Some(tr),
                                            _ => None,
                                        }) {
                                            resolved.push(t);
                                        }
                                    }
                                }
                                Ok(resolved)
                            },
                            Message::RadioLoaded,
                        ));
                    }
                }
                Task::batch(tasks)
            }
            Message::PlayTracks(tracks, idx) => {
                self.queue.replace_with(tracks, idx);
                self.start_playing_current()
            }
            Message::QueuePlayNext(t) => {
                self.track_menu = None;
                let was_empty = self.queue.current().is_none();
                let thumb = t.thumbnail_url.clone();
                self.queue.insert_next(t);
                if was_empty {
                    self.start_playing_current()
                } else {
                    self.set_toast("Playing next");
                    self.fetch_cover_if_needed(thumb.as_deref())
                }
            }
            Message::QueueAddBack(t) => {
                self.track_menu = None;
                let was_empty = self.queue.current().is_none();
                let thumb = t.thumbnail_url.clone();
                self.queue.push_back(t);
                if was_empty {
                    self.start_playing_current()
                } else {
                    self.set_toast("Added to queue");
                    self.fetch_cover_if_needed(thumb.as_deref())
                }
            }
            Message::QueueJump(idx) => {
                if idx < self.queue.items.len() {
                    self.queue.current_index = Some(idx);
                    return self.start_playing_current();
                }
                Task::none()
            }
            Message::QueueRemove(idx) => {
                self.queue.remove_at(idx);
                Task::none()
            }
            Message::QueueClear => {
                self.flush_active_play_duration();
                self.queue.items.clear();
                self.queue.current_index = None;
                self.scrobble_tracker = None;
                self.audio.stop();
                Task::none()
            }
            Message::RadioLoaded(Ok(mut tracks)) => {
                let mut tasks = Vec::new();
                let cur_id = self.queue.current().map(|c| c.track.id.clone());
                if let Some(ref seed_id) = cur_id {
                    if !tracks.is_empty() {
                        let _ = self.db.save_related_edges(seed_id, &tracks, 1);
                    }
                }
                tracks.sort_by(|a, b| {
                    score_candidate(b, &self.listener_ctx)
                        .partial_cmp(&score_candidate(a, &self.listener_ctx))
                        .unwrap_or(std::cmp::Ordering::Equal)
                });
                for t in tracks {
                    if Some(&t.id) != cur_id.as_ref()
                        && !self.listener_ctx.is_disliked_artist(t.primary_artist())
                        && !self.queue.items.iter().any(|q| q.track.id == t.id)
                    {
                        tasks.push(self.fetch_cover_if_needed(t.thumbnail_url.as_deref()));
                        self.queue.push_back(t);
                    }
                }
                prune_queue_tail(
                    &mut self.queue.items,
                    self.queue.current_index,
                    &self.listener_ctx,
                    false,
                );
                Task::batch(tasks)
            }
            Message::RadioLoaded(Err(_)) => Task::none(),
            Message::StreamResolved { mut track, result } => {
                if self.queue.current().map(|c| &c.track.id) != Some(&track.id) {
                    return Task::none();
                }
                match result {
                    Ok(stream) => {
                        if let Some(ldb) = stream.loudness_db {
                            track.loudness_db = Some(ldb);
                            if let Some(cur) = self
                                .queue
                                .current_index
                                .and_then(|i| self.queue.items.get_mut(i))
                            {
                                cur.track.loudness_db = Some(ldb);
                            }
                        }
                        if track.thumbnail_url.is_none() {
                            track.thumbnail_url = stream.thumbnail_url.clone();
                        }
                        let _ = self.db.upsert_track(&track);
                        let _ = self.db.record_play(&track, 0);
                        self.refresh_db_views();
                        if let Some(tracking_url) = stream.playback_tracking_url.clone() {
                            if self.yt.has_cookie() {
                                let yt = self.yt.clone();
                                tokio::spawn(async move {
                                    let _ = yt.register_playback(&tracking_url, None).await;
                                });
                            }
                        }
                        self.audio.play(AudioSourceSpec::Url {
                            url: stream.url.clone(),
                            user_agent: stream.user_agent.clone(),
                            mime_hint: Some(stream.mime_type.clone()),
                            content_length: stream.content_length,
                            approx_duration_ms: stream.approx_duration_ms,
                            loudness_db: track.loudness_db,
                        });
                        if self.settings.max_cache_size_mb > 0 {
                            let max_mb = self.settings.max_cache_size_mb;
                            if stream.content_length.unwrap_or(0) <= (32 * 1024 * 1024) {
                                if let Ok(cache_dir) = AppSettings::stream_cache_dir() {
                                    let http = self.http.clone();
                                    let url = stream.url;
                                    let ua = stream.user_agent;
                                    let sid = track.id.clone();
                                    tokio::spawn(async move {
                                        if let Ok(resp) = http
                                            .get(&url)
                                            .header(reqwest::header::USER_AGENT, ua)
                                            .send()
                                            .await
                                        {
                                            if resp.status().is_success() {
                                                if let Ok(bytes) = resp.bytes().await {
                                                    let _ = tokio::task::spawn_blocking(move || {
                                                        let _ = store_cached_stream(
                                                            &cache_dir,
                                                            &sid,
                                                            &bytes,
                                                            max_mb,
                                                        );
                                                    })
                                                    .await;
                                                }
                                            }
                                        }
                                    });
                                }
                            }
                        }
                    }
                    Err(e) => {
                        self.yt.invalidate_stream(&track.id);
                        self.status = PlaybackStatus::Error(e.clone());
                        self.set_toast(format!("Playback error: {e}"));
                    }
                }
                Task::none()
            }
            Message::TogglePlayPause => {
                self.fade_in_remaining_ms = 0;
                self.audio.set_fade_factor(1.0);
                match self.status {
                    PlaybackStatus::Playing => self.audio.pause(),
                    PlaybackStatus::Paused => self.audio.resume(),
                    PlaybackStatus::Stopped | PlaybackStatus::Error(_) => {
                        if self.queue.current().is_some() {
                            return self.start_playing_current();
                        }
                    }
                    PlaybackStatus::Loading => {}
                }
                Task::none()
            }
            Message::NextTrack => {
                self.fade_in_remaining_ms = 0;
                self.audio.set_fade_factor(1.0);
                if let Some(cur) = self.queue.current().map(|c| c.track.clone()) {
                    let early = self.duration_ms > 20_000
                        && self.position_ms < (self.duration_ms * 35) / 100;
                    if early {
                        self.listener_ctx.record_early_skip(&cur);
                        prune_queue_tail(
                            &mut self.queue.items,
                            self.queue.current_index,
                            &self.listener_ctx,
                            true,
                        );
                        self.rebuild_home_sections();
                    }
                }
                if self.queue.advance_next().is_some() {
                    self.start_playing_current()
                } else {
                    Task::none()
                }
            }
            Message::PrevTrack => {
                self.fade_in_remaining_ms = 0;
                self.audio.set_fade_factor(1.0);
                if self.position_ms > 3000 {
                    self.audio.seek_ms(0);
                    Task::none()
                } else if self.queue.step_prev().is_some() {
                    self.start_playing_current()
                } else {
                    Task::none()
                }
            }
            Message::ToggleRepeat => {
                self.queue.repeat_mode = self.queue.repeat_mode.next();
                Task::none()
            }
            Message::ToggleShuffle => {
                self.queue.shuffle = !self.queue.shuffle;
                Task::none()
            }
            Message::SeekChanged(val) => {
                self.seeking_ms = Some(val as u64);
                Task::none()
            }
            Message::SeekReleased => {
                self.fade_in_remaining_ms = 0;
                self.audio.set_fade_factor(1.0);
                if let Some(ms) = self.seeking_ms.take() {
                    self.position_ms = ms;
                    self.audio.seek_ms(ms);
                    self.last_lyric_idx = None;
                    return self.sync_lyrics_scroll();
                }
                Task::none()
            }
            Message::SeekToMs(ms) => {
                self.fade_in_remaining_ms = 0;
                self.audio.set_fade_factor(1.0);
                self.position_ms = ms;
                self.audio.seek_ms(ms);
                self.last_lyric_idx = None;
                self.sync_lyrics_scroll()
            }
            Message::VolumeChanged(v) => {
                let clamped = v.clamp(0.0, 4.0);
                if clamped > 0.01 {
                    self.pre_mute_volume = clamped;
                }
                self.settings.volume = clamped;
                self.audio.set_volume(self.settings.volume);
                let _ = self.settings.save();
                Task::none()
            }
            Message::ToggleMute => {
                if self.settings.volume > 0.01 {
                    self.pre_mute_volume = self.settings.volume;
                    self.settings.volume = 0.0;
                } else {
                    self.settings.volume = self.pre_mute_volume.max(0.5);
                }
                self.audio.set_volume(self.settings.volume);
                let _ = self.settings.save();
                Task::none()
            }
            Message::ToggleLikeCurrent => {
                if let Some(idx) = self.queue.current_index {
                    let mut liked_track: Option<Track> = None;
                    if let Some(item) = self.queue.items.get_mut(idx) {
                        item.track.liked = !item.track.liked;
                        let liked_now = item.track.liked;
                        let track_id = item.track.id.clone();
                        let _ = self.db.upsert_track(&item.track);
                        let _ = self.db.set_liked(&track_id, liked_now);
                        if self.yt.has_cookie() && !item.track.is_local {
                            let yt = self.yt.clone();
                            tokio::spawn(async move {
                                let _ = yt.like_video(&track_id, liked_now).await;
                            });
                        }
                        if liked_now {
                            liked_track = Some(item.track.clone());
                        }
                        self.refresh_db_views();
                    }
                    if let Some(t) = liked_track {
                        self.listener_ctx.record_play(&t, true);
                        prune_queue_tail(
                            &mut self.queue.items,
                            self.queue.current_index,
                            &self.listener_ctx,
                            true,
                        );
                    }
                    self.rebuild_home_sections();
                }
                Task::none()
            }
            Message::ToggleLikeTrack(mut track) => {
                let existing = self.db.get_track(&track.id).ok().flatten();
                let next_liked = !existing.as_ref().map(|t| t.liked).unwrap_or(track.liked);
                track.liked = next_liked;
                let _ = self.db.upsert_track(&track);
                let _ = self.db.set_liked(&track.id, next_liked);
                if self.yt.has_cookie() && !track.is_local {
                    let yt = self.yt.clone();
                    let vid = track.id.clone();
                    tokio::spawn(async move {
                        let _ = yt.like_video(&vid, next_liked).await;
                    });
                }
                if let Some(cur) = self
                    .queue
                    .current_index
                    .and_then(|i| self.queue.items.get_mut(i))
                    .filter(|item| item.track.id == track.id)
                {
                    cur.track.liked = next_liked;
                }
                self.refresh_db_views();
                if next_liked {
                    self.listener_ctx.record_play(&track, true);
                }
                self.rebuild_home_sections();
                Task::none()
            }
            Message::ToggleExpandedPlayer => {
                self.show_expanded = !self.show_expanded;
                if self.show_expanded {
                    self.last_lyric_idx = None;
                    return self.sync_lyrics_scroll();
                }
                Task::none()
            }
            Message::ToggleMiniPlayer => {
                let next = !self.mini_player;
                if next {
                    self.was_maximized_before_mini = self.window_maximized;
                    if !self.window_maximized && self.window_size.0 > 520.0 && self.window_size.1 > 280.0 {
                        self.normal_window_size = self.window_size;
                    }
                    self.mini_player = true;
                    self.window_maximized = false;
                    self.track_menu = None;
                    self.search_dropdown_open = false;
                    let apply = |id: iced::window::Id| {
                        Task::batch([
                            iced::window::maximize(id, false),
                            iced::window::change_level(id, iced::window::Level::AlwaysOnTop),
                            iced::window::resize(id, iced::Size::new(440.0, 176.0)),
                        ])
                    };
                    if let Some(id) = self.main_window_id {
                        apply(id)
                    } else {
                        iced::window::get_oldest().then(move |opt| match opt {
                            Some(id) => apply(id),
                            None => Task::none(),
                        })
                    }
                } else {
                    self.mini_player = false;
                    let (w, h) = self.normal_window_size;
                    let restore_max = self.was_maximized_before_mini;
                    self.window_maximized = restore_max;
                    let restore = move |id: iced::window::Id| {
                        let mut tasks = vec![
                            iced::window::change_level(id, iced::window::Level::Normal),
                            iced::window::resize(id, iced::Size::new(w.max(960.0), h.max(640.0))),
                        ];
                        if restore_max {
                            tasks.push(iced::window::maximize(id, true));
                        }
                        Task::batch(tasks)
                    };
                    if let Some(id) = self.main_window_id {
                        restore(id)
                    } else {
                        iced::window::get_oldest().then(move |opt| match opt {
                            Some(id) => restore(id),
                            None => Task::none(),
                        })
                    }
                }
            }
            Message::ArtworkLoaded {
                url,
                rounded,
                circle,
                blurred,
                bytes,
            } => {
                self.loading_covers.remove(&url);
                if let Some(r) = rounded {
                    self.rounded_covers.insert(url.clone(), r);
                } else if let Some(ref raw) = bytes {
                    self.rounded_covers
                        .insert(url.clone(), image::Handle::from_bytes(raw.clone()));
                }
                if let Some(c) = circle {
                    self.circle_covers.insert(url.clone(), c);
                }
                if let Some(b) = blurred {
                    self.blurred_covers.insert(url.clone(), b);
                }
                if let Some(raw) = bytes {
                    let is_current_cover = self
                        .queue
                        .current()
                        .and_then(|c| c.track.thumbnail_url.as_deref())
                        == Some(url.as_str());
                    if is_current_cover {
                        if let Some(p) =
                            DynamicPalette::from_image_bytes(&raw, self.settings.theme_mode)
                        {
                            self.palette = p;
                        }
                    }
                    self.cover_bytes.insert(url, raw);
                }
                Task::none()
            }
            Message::LyricsLoaded { track_id, lyrics } => {
                if self.queue.current().map(|c| c.track.id.as_str()) == Some(track_id.as_str()) {
                    self.lyrics_loading = false;
                    self.active_lyrics = lyrics;
                    self.last_lyric_idx = None;
                    return self.sync_lyrics_scroll();
                }
                Task::none()
            }
            Message::NewPlaylistNameChanged(s) => {
                self.new_playlist_name = s;
                Task::none()
            }
            Message::CreatePlaylist => {
                let name = self.new_playlist_name.trim().to_string();
                if !name.is_empty() {
                    let id = format!("local_{}", chrono::Utc::now().timestamp_millis());
                    let _ = self.db.create_playlist(&id, &name);
                    self.new_playlist_name.clear();
                    self.show_new_playlist_input = false;
                    self.selected_playlist_id = Some(id);
                    self.selected_playlist_tracks.clear();
                    self.refresh_db_views();
                    if !self.playlists.is_empty() {
                        self.tab = NavTab::Playlists;
                        self.library_tab = LibraryTab::Playlist(self.playlists.len() - 1);
                    }
                }
                Task::none()
            }
            Message::DeletePlaylist(pid) => {
                let _ = self.db.delete_playlist(&pid);
                if self.selected_playlist_id.as_deref() == Some(pid.as_str()) {
                    self.selected_playlist_id = None;
                    self.selected_playlist_tracks.clear();
                    self.library_tab = LibraryTab::Liked;
                }
                self.refresh_db_views();
                self.set_toast("Playlist deleted");
                Task::none()
            }
            Message::RemoveFromPlaylist { playlist_id, song_id } => {
                let _ = self.db.remove_from_playlist(&playlist_id, &song_id);
                if self.selected_playlist_id.as_deref() == Some(playlist_id.as_str()) {
                    self.selected_playlist_tracks =
                        self.db.playlist_tracks(&playlist_id).unwrap_or_default();
                }
                self.refresh_db_views();
                self.set_toast("Removed from playlist");
                Task::none()
            }
            Message::AddCurrentToPlaylist(pid) => {
                if let Some(cur) = self.queue.current() {
                    let _ = self.db.add_to_playlist(&pid, &cur.track);
                    self.refresh_db_views();
                    if self.selected_playlist_id.as_deref() == Some(pid.as_str()) {
                        self.selected_playlist_tracks =
                            self.db.playlist_tracks(&pid).unwrap_or_default();
                    }
                    self.set_toast("Added to playlist");
                }
                Task::none()
            }
            Message::FocusSearch => text_input::focus(self.search_input_id.clone()),
            Message::KeyPressed(key, modifiers, captured) => {
                if modifiers.control() {
                    if let Key::Character(c) = &key {
                        if c.eq_ignore_ascii_case("k") || c.eq_ignore_ascii_case("f") {
                            return self.update(Message::FocusSearch);
                        }
                    }
                }
                match &key {
                    Key::Named(Named::MediaPlayPause)
                    | Key::Named(Named::MediaPlay)
                    | Key::Named(Named::MediaPause) => return self.update(Message::TogglePlayPause),
                    Key::Named(Named::MediaTrackNext) => return self.update(Message::NextTrack),
                    Key::Named(Named::MediaTrackPrevious) => return self.update(Message::PrevTrack),
                    Key::Named(Named::Escape) => {
                        if self.mini_player {
                            return self.update(Message::ToggleMiniPlayer);
                        } else if self.track_menu.is_some() {
                            self.track_menu = None;
                        } else if self.search_dropdown_open {
                            self.search_dropdown_open = false;
                        } else if self.show_expanded {
                            self.show_expanded = false;
                        } else if self.viewing_detail.is_some()
                            || self.detail_loading.is_some()
                            || !self.nav_history.is_empty()
                        {
                            return self.update(Message::NavigateBackDetail);
                        }
                        return Task::none();
                    }
                    _ => {}
                }
                if captured {
                    return Task::none();
                }
                match key {
                    Key::Named(Named::Space) => self.update(Message::TogglePlayPause),
                    Key::Named(Named::ArrowRight) => {
                        let target = (self.position_ms + 5_000).min(self.duration_ms.max(1));
                        self.update(Message::SeekToMs(target))
                    }
                    Key::Named(Named::ArrowLeft) => {
                        let target = self.position_ms.saturating_sub(5_000);
                        self.update(Message::SeekToMs(target))
                    }
                    Key::Named(Named::ArrowUp) => {
                        self.update(Message::VolumeChanged(self.settings.volume + 0.1))
                    }
                    Key::Named(Named::ArrowDown) => {
                        self.update(Message::VolumeChanged(self.settings.volume - 0.1))
                    }
                    Key::Character(c) if c.eq_ignore_ascii_case("m") => {
                        self.update(Message::ToggleMute)
                    }
                    _ => Task::none(),
                }
            }
            Message::ListenAndRecognize => {
                if self.recognizing {
                    return Task::none();
                }
                self.recognizing = true;
                self.last_recognised = None;
                self.last_recognised_track = None;
                self.recognition_status = Some("Listening for 8 seconds (with speed-retry fallback)...".to_string());
                let source = self.settings.recognition_source;
                let shazam = Arc::clone(&self.shazam);
                Task::perform(
                    async move {
                        let samples = tokio::task::spawn_blocking(move || {
                            capture_16khz_mono(source, Duration::from_secs(8))
                        })
                        .await
                        .map_err(|e| e.to_string())?
                        .map_err(|e| e.to_string())?;
                        Ok(shazam.identify_with_retry(&samples).await)
                    },
                    Message::RecognitionFinished,
                )
            }
            Message::RecognitionFinished(res) => {
                self.recognizing = false;
                match res {
                    Ok(RecognitionOutcome::Match(rec)) => {
                        self.recognition_status = Some(format!(
                            "Matched: {} — {}",
                            rec.title,
                            rec.artist.as_deref().unwrap_or("")
                        ));
                        let rec_id = self
                            .db
                            .add_recognition(
                                &rec.title,
                                rec.artist.as_deref(),
                                rec.artwork_url.as_deref(),
                                rec.isrc.as_deref(),
                                rec.shazam_key.as_deref(),
                                None,
                            )
                            .ok();
                        self.last_recognised = Some(rec.clone());
                        self.refresh_db_views();

                        let yt = self.yt.clone();
                        let query = rec.search_query();
                        let resolve_task = Task::perform(
                            async move {
                                let found = yt
                                    .search(&query, SearchFilter::Songs)
                                    .await
                                    .ok()
                                    .and_then(|items| {
                                        items.into_iter().find_map(|it| match it {
                                            BrowseItem::Track(t) => Some(t),
                                            _ => None,
                                        })
                                    });
                                (rec_id, found)
                            },
                            |(recognition_id, track)| Message::RecognitionTrackResolved {
                                recognition_id,
                                track,
                            },
                        );
                        return Task::batch([
                            self.fetch_cover_if_needed(rec.artwork_url.as_deref()),
                            resolve_task,
                        ]);
                    }
                    Ok(RecognitionOutcome::NoMatch) => {
                        self.recognition_status =
                            Some("No match found (including speed retries). Try a longer sample.".to_string());
                    }
                    Ok(RecognitionOutcome::Failed {
                        reason,
                        retry_after_secs,
                    }) => {
                        let msg = if reason == "silence" {
                            "No audio detected (silence). Check your audio source.".to_string()
                        } else if let Some(s) = retry_after_secs {
                            format!("Shazam rate-limited ({reason}). Retry in {s}s.")
                        } else {
                            format!("Recognition failed: {reason}")
                        };
                        self.recognition_status = Some(msg);
                    }
                    Err(e) => {
                        self.recognition_status = Some(format!("Capture failed: {e}"));
                    }
                }
                Task::none()
            }
            Message::RecognitionTrackResolved {
                recognition_id,
                track,
            } => {
                if let Some(t) = track {
                    let _ = self.db.upsert_track(&t);
                    if let Some(rid) = recognition_id {
                        let _ = self.db.update_recognition_video_id(rid, &t.id);
                    }
                    self.last_recognised_track = Some(t.clone());
                    self.refresh_db_views();
                    return self.fetch_cover_if_needed(t.thumbnail_url.as_deref());
                }
                Task::none()
            }
            Message::PlayRecognised(rec) => {
                if let Some(track) = self.last_recognised_track.clone() {
                    return self.update(Message::PlayTrack(track));
                }
                self.tab = NavTab::Search;
                self.show_expanded = false;
                self.search_query = rec.search_query();
                self.search_filter = SearchFilter::Songs;
                self.update(Message::SubmitSearch)
            }
            Message::OpenHistoryRecognition(entry) => {
                if let Some(vid) = entry.video_id {
                    if let Ok(Some(track)) = self.db.get_track(&vid) {
                        return self.update(Message::PlayTrack(track));
                    }
                }
                self.tab = NavTab::Search;
                self.show_expanded = false;
                self.search_query = match entry.artist {
                    Some(a) if !a.is_empty() => format!("{} {a}", entry.title),
                    _ => entry.title,
                };
                self.search_filter = SearchFilter::Songs;
                self.update(Message::SubmitSearch)
            }
            Message::LastFmApiKeyChanged(k) => {
                self.settings.lastfm_api_key = k;
                let _ = self.settings.save();
                Task::none()
            }
            Message::LastFmApiSecretChanged(s) => {
                self.settings.lastfm_api_secret = s;
                let _ = self.settings.save();
                Task::none()
            }
            Message::StartLastFmAuth => {
                let client = LastFmClient::new(
                    self.settings.lastfm_api_key.clone(),
                    self.settings.lastfm_api_secret.clone(),
                );
                if !client.is_configured() {
                    self.lastfm_status =
                        Some("Enter Last.fm API key and secret first.".to_string());
                    return Task::none();
                }
                self.lastfm_status = Some("Requesting authorization token...".to_string());
                Task::perform(
                    async move {
                        let tok = client.request_token().await.map_err(|e| e.to_string())?;
                        let url = client.authorize_url(&tok);
                        Ok((tok, url))
                    },
                    Message::LastFmTokenReceived,
                )
            }
            Message::LastFmTokenReceived(res) => {
                match res {
                    Ok((token, url)) => {
                        self.lastfm_pending_token = Some(token);
                        self.lastfm_status = Some(
                            "Approve Echo in your browser, then click Complete Login.".to_string(),
                        );
                        let _ = open_browser_url(&url);
                    }
                    Err(e) => {
                        self.lastfm_status = Some(format!("Last.fm auth error: {e}"));
                    }
                }
                Task::none()
            }
            Message::CompleteLastFmAuth => {
                let Some(token) = self.lastfm_pending_token.clone() else {
                    return Task::none();
                };
                let client = LastFmClient::new(
                    self.settings.lastfm_api_key.clone(),
                    self.settings.lastfm_api_secret.clone(),
                );
                Task::perform(
                    async move { client.session(&token).await.map_err(|e| e.to_string()) },
                    Message::LastFmSessionReceived,
                )
            }
            Message::LastFmSessionReceived(res) => {
                match res {
                    Ok(session) => {
                        self.settings.lastfm_username = Some(session.name.clone());
                        self.settings.lastfm_session_key = Some(session.key);
                        self.lastfm_pending_token = None;
                        self.lastfm_status = Some(format!("Connected as {}", session.name));
                        let _ = self.settings.save();
                    }
                    Err(e) => {
                        self.lastfm_status = Some(format!("Could not finish login: {e}"));
                    }
                }
                Task::none()
            }
            Message::DisconnectLastFm => {
                self.settings.lastfm_session_key = None;
                self.settings.lastfm_username = None;
                self.lastfm_pending_token = None;
                self.lastfm_status = Some("Disconnected".to_string());
                let _ = self.settings.save();
                Task::none()
            }
            Message::ToggleLastFmScrobbling(on) => {
                self.settings.lastfm_scrobbling_enabled = on;
                let _ = self.settings.save();
                Task::none()
            }
            Message::YouTubeCookieChanged(c) => {
                self.yt_cookie_input = c;
                Task::none()
            }
            Message::SaveYouTubeCookie => {
                let trimmed = self.yt_cookie_input.trim().to_string();
                let cookie_opt = (!trimmed.is_empty()).then_some(trimmed);
                self.settings.youtube_cookie = cookie_opt.clone();
                self.yt.set_cookie(cookie_opt);
                let _ = self.settings.save();
                self.account_sync_status = Some(if self.yt.has_cookie() {
                    "Cookie saved. Click Sync Account to import playlists & liked songs."
                        .to_string()
                } else {
                    "YouTube cookie cleared.".to_string()
                });
                Task::none()
            }
            Message::ClearYouTubeCookie => {
                self.yt_cookie_input.clear();
                self.settings.youtube_cookie = None;
                self.yt.set_cookie(None);
                self.account_playlists.clear();
                self.account_sync_status = Some("Signed out of YouTube Music".to_string());
                let _ = self.settings.save();
                Task::none()
            }
            Message::SyncYouTubeAccount => {
                if !self.yt.has_cookie() {
                    self.account_sync_status =
                        Some("Paste and save a YouTube Music cookie first.".to_string());
                    return Task::none();
                }
                if self.account_syncing {
                    return Task::none();
                }
                self.account_syncing = true;
                self.account_sync_status =
                    Some("Syncing account playlists & liked songs...".to_string());
                let yt = self.yt.clone();
                Task::perform(
                    async move {
                        let playlists = yt.account_playlists().await.map_err(|e| e.to_string());
                        let liked = yt.liked_songs_remote().await.map_err(|e| e.to_string());
                        (playlists, liked)
                    },
                    |(playlists, liked)| Message::AccountSynced { playlists, liked },
                )
            }
            Message::YoutubeRegionSelected(region) => {
                self.settings.youtube_gl = region.code().to_string();
                self.yt.set_locale(
                    self.settings.youtube_gl.clone(),
                    self.settings.youtube_hl.clone(),
                );
                let _ = self.settings.save();
                self.set_toast(format!("YouTube Music region: {}", region.code()));
                self.update(Message::LoadHome)
            }
            Message::YoutubeLanguageSelected(lang) => {
                self.settings.youtube_hl = lang.code().to_string();
                self.yt.set_locale(
                    self.settings.youtube_gl.clone(),
                    self.settings.youtube_hl.clone(),
                );
                let _ = self.settings.save();
                self.set_toast(format!("YouTube Music language: {}", lang.code()));
                self.update(Message::LoadHome)
            }
            Message::AccountSynced { playlists, liked } => {
                self.account_syncing = false;
                let mut tasks = Vec::new();
                let mut parts = Vec::new();
                match playlists {
                    Ok(pls) => {
                        parts.push(format!("{} playlists", pls.len()));
                        for pl in pls.iter().take(15) {
                            tasks.push(self.fetch_cover_if_needed(pl.thumbnail_url.as_deref()));
                        }
                        self.account_playlists = pls;
                    }
                    Err(e) => parts.push(format!("playlists error: {e}")),
                }
                match liked {
                    Ok(page) => {
                        let count = page.tracks.len();
                        for mut t in page.tracks {
                            t.liked = true;
                            let _ = self.db.upsert_track(&t);
                            let _ = self.db.set_liked(&t.id, true);
                        }
                        self.refresh_db_views();
                        parts.push(format!("{count} liked songs synced"));
                    }
                    Err(e) => parts.push(format!("liked songs error: {e}")),
                }
                self.account_sync_status = Some(parts.join(" • "));
                self.set_toast("YouTube account sync finished");
                Task::batch(tasks)
            }
            Message::DownloadTrack(track) => {
                self.track_menu = None;
                if track.is_local
                    || self.downloaded_id_set.contains(&track.id)
                    || self.downloading_ids.contains(&track.id)
                {
                    return Task::none();
                }
                let Ok(dest_dir) = self.settings.effective_downloads_dir() else {
                    self.set_toast("Could not determine download folder");
                    return Task::none();
                };
                self.downloading_ids.insert(track.id.clone());
                let yt = self.yt.clone();
                let http = self.http.clone();
                let quality = self.settings.audio_quality;
                let t = track.clone();
                Task::perform(
                    async move {
                        let stream = yt
                            .resolve_stream(&t.id, quality)
                            .await
                            .map_err(|e| e.to_string())?;
                        let out_path =
                            download_file_path(&dest_dir, &t.title, &t.id, &stream.mime_type);
                        let resp = http
                            .get(&stream.url)
                            .header(reqwest::header::USER_AGENT, stream.user_agent)
                            .send()
                            .await
                            .map_err(|e| e.to_string())?
                            .error_for_status()
                            .map_err(|e| e.to_string())?;
                        let bytes = resp.bytes().await.map_err(|e| e.to_string())?;
                        let len = bytes.len() as u64;
                        let out_clone = out_path.clone();
                        tokio::task::spawn_blocking(move || -> std::io::Result<()> {
                            if let Some(parent) = out_clone.parent() {
                                std::fs::create_dir_all(parent)?;
                            }
                            std::fs::write(out_clone, &bytes)
                        })
                        .await
                        .map_err(|e| e.to_string())?
                        .map_err(|e| e.to_string())?;
                        Ok((
                            out_path.to_string_lossy().into_owned(),
                            stream.mime_type,
                            len,
                        ))
                    },
                    move |result| Message::DownloadCompleted {
                        track: track.clone(),
                        result,
                    },
                )
            }
            Message::DownloadTracks(tracks) => {
                let mut tasks = Vec::new();
                for t in tracks {
                    if !t.is_local
                        && !self.downloaded_id_set.contains(&t.id)
                        && !self.downloading_ids.contains(&t.id)
                    {
                        tasks.push(self.update(Message::DownloadTrack(t)));
                    }
                }
                if !tasks.is_empty() {
                    self.set_toast(format!("Downloading {} tracks...", tasks.len()));
                }
                Task::batch(tasks)
            }
            Message::DownloadCompleted { track, result } => {
                self.downloading_ids.remove(&track.id);
                match result {
                    Ok((path_str, mime, len)) => {
                        let _ = self.db.upsert_download(&track, &path_str, Some(&mime), len);
                        self.refresh_db_views();
                        self.set_toast(format!("Downloaded: {}", track.title));
                    }
                    Err(e) => {
                        self.download_status = Some(format!("Download failed ({}): {e}", track.title));
                        self.set_toast(format!("Download failed: {e}"));
                    }
                }
                Task::none()
            }
            Message::RemoveDownload(song_id) => {
                self.track_menu = None;
                let _ = self.db.remove_download(&song_id);
                self.refresh_db_views();
                self.set_toast("Removed download");
                Task::none()
            }
            Message::ClearDownloads => {
                let count = self.db.clear_downloads().unwrap_or(0);
                self.refresh_db_views();
                self.download_status = Some(format!("Cleared {count} downloaded tracks."));
                self.set_toast("Cleared downloads");
                Task::none()
            }
            Message::ScanDownloadFolders => {
                let mut folders = Vec::new();
                if let Ok(main_dir) = self.settings.effective_downloads_dir() {
                    folders.push(main_dir);
                }
                for p in &self.settings.extra_import_folders {
                    let trimmed = p.trim();
                    if !trimmed.is_empty() {
                        folders.push(PathBuf::from(trimmed));
                    }
                }
                let db = self.db.clone();
                let yt = self.yt.clone();
                Task::perform(
                    async move {
                        let pairs = tokio::task::spawn_blocking(move || scan_download_folders(&folders))
                            .await
                            .map_err(|e| e.to_string())?;
                        let mut imported = Vec::new();
                        for (vid, path) in pairs {
                            let len = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
                            let mut track = db.get_track(&vid).ok().flatten().unwrap_or_else(|| {
                                let title = path
                                    .file_stem()
                                    .and_then(|s| s.to_str())
                                    .unwrap_or(&vid)
                                    .replace(&format!("[{vid}]"), "")
                                    .trim()
                                    .to_string();
                                Track {
                                    id: vid.clone(),
                                    title: if title.is_empty() { vid.clone() } else { title },
                                    artists: Vec::new(),
                                    album: None,
                                    duration_secs: None,
                                    thumbnail_url: None,
                                    loudness_db: None,
                                    liked: false,
                                    is_local: false,
                                    local_path: None,
                                }
                            });
                            if track.loudness_db.is_none() {
                                if let Ok(Some(ldb)) = yt.fetch_loudness_db(&vid).await {
                                    track.loudness_db = Some(ldb);
                                }
                            }
                            imported.push((track, path.to_string_lossy().into_owned(), len));
                        }
                        Ok(imported)
                    },
                    Message::DownloadFoldersScanned,
                )
            }
            Message::DownloadFoldersScanned(res) => {
                match res {
                    Ok(items) => {
                        let count = items.len();
                        for (track, path_str, len) in items {
                            let _ = self.db.upsert_download(&track, &path_str, None, len);
                        }
                        self.refresh_db_views();
                        self.download_status =
                            Some(format!("Rescan finished: {count} [video_id] files matched."));
                        self.set_toast(format!("Matched {count} offline files"));
                    }
                    Err(e) => {
                        self.download_status = Some(format!("Rescan failed: {e}"));
                    }
                }
                Task::none()
            }
            Message::MigrateDownloads => {
                let Ok(dest_dir) = self.settings.effective_downloads_dir() else {
                    self.download_status = Some("Invalid download folder.".to_string());
                    return Task::none();
                };
                let existing = self.db.downloaded_tracks().unwrap_or_default();
                let mut moved = 0usize;
                for (t, src_path, len) in existing {
                    if let Ok(new_path) = migrate_download_file(&src_path, &dest_dir) {
                        let _ = self.db.upsert_download(
                            &t,
                            &new_path.to_string_lossy(),
                            None,
                            len,
                        );
                        moved += 1;
                    }
                }
                self.refresh_db_views();
                self.download_status = Some(format!(
                    "Migrated {moved} files to {}",
                    dest_dir.display()
                ));
                self.set_toast(format!("Migrated {moved} downloads"));
                Task::none()
            }
            Message::ClearStreamCache => {
                if let Ok(dir) = AppSettings::stream_cache_dir() {
                    let removed = clear_cache_dir(&dir).unwrap_or(0);
                    self.refresh_db_views();
                    self.download_status = Some(format!("Cleared {removed} cached streams."));
                    self.set_toast("Stream cache cleared");
                }
                Task::none()
            }
            Message::MaxCacheSizeChanged(v) => {
                self.cache_size_input = v.clone();
                if let Ok(mb) = v.trim().parse::<u64>() {
                    self.settings.max_cache_size_mb = mb;
                    let _ = self.settings.save();
                }
                Task::none()
            }
            Message::DownloadFolderChanged(v) => {
                self.download_folder_input = v;
                Task::none()
            }
            Message::ExtraImportFoldersChanged(v) => {
                self.extra_import_input = v;
                Task::none()
            }
            Message::SaveDownloadSettings => {
                let df = self.download_folder_input.trim();
                self.settings.download_folder = (!df.is_empty()).then(|| df.to_string());
                self.settings.extra_import_folders = parse_folder_list(&self.extra_import_input);
                let _ = self.settings.save();
                self.download_status = Some("Download folder settings saved.".to_string());
                self.set_toast("Download settings saved");
                Task::none()
            }
            Message::PickFolder(target) => {
                let title = match target {
                    FolderTarget::DownloadFolder => "Select Downloads Folder",
                    FolderTarget::ExtraImportFolder => "Select Extra [youtube_id] Folder",
                    FolderTarget::LocalMediaFolder => "Add Local Music Folder",
                    FolderTarget::ExcludedMediaFolder => "Add Excluded Folder",
                };
                let initial = match target {
                    FolderTarget::DownloadFolder => {
                        self.settings.effective_downloads_dir().ok()
                    }
                    FolderTarget::ExtraImportFolder => self
                        .settings
                        .extra_import_folders
                        .last()
                        .map(PathBuf::from),
                    FolderTarget::LocalMediaFolder => self
                        .settings
                        .local_media_folders
                        .last()
                        .map(PathBuf::from),
                    FolderTarget::ExcludedMediaFolder => self
                        .settings
                        .excluded_media_folders
                        .last()
                        .map(PathBuf::from),
                };
                let owner_hwnd = self.window_hwnd;
                Task::perform(
                    async move {
                        dialogs::pick_folder(owner_hwnd, title, initial.as_deref())
                    },
                    move |picked| Message::FolderPicked(target, picked),
                )
            }
            Message::FolderPicked(target, Ok(Some(path))) => {
                let path_str = path.to_string_lossy().into_owned();
                match target {
                    FolderTarget::DownloadFolder => {
                        self.download_folder_input = path_str.clone();
                        self.settings.download_folder = Some(path_str);
                        self.download_status = Some("Download folder updated.".to_string());
                        self.set_toast("Download folder updated");
                    }
                    FolderTarget::ExtraImportFolder => {
                        self.extra_import_input = append_folder_entry(
                            &mut self.settings.extra_import_folders,
                            &path_str,
                        );
                        self.download_status = Some("Added extra import folder.".to_string());
                        self.set_toast("Added import folder");
                    }
                    FolderTarget::LocalMediaFolder => {
                        self.local_folders_input = append_folder_entry(
                            &mut self.settings.local_media_folders,
                            &path_str,
                        );
                        self.set_toast("Added local music folder");
                    }
                    FolderTarget::ExcludedMediaFolder => {
                        self.excluded_folders_input = append_folder_entry(
                            &mut self.settings.excluded_media_folders,
                            &path_str,
                        );
                        self.set_toast("Added excluded folder");
                    }
                }
                let _ = self.settings.save();
                Task::none()
            }
            Message::FolderPicked(_, Ok(None)) => Task::none(),
            Message::FolderPicked(_, Err(e)) => {
                self.set_toast(format!("Folder picker error: {e}"));
                Task::none()
            }
            Message::LocalFoldersChanged(v) => {
                self.local_folders_input = v;
                self.settings.local_media_folders = parse_folder_list(&self.local_folders_input);
                let _ = self.settings.save();
                Task::none()
            }
            Message::ExcludedFoldersChanged(v) => {
                self.excluded_folders_input = v;
                self.settings.excluded_media_folders =
                    parse_folder_list(&self.excluded_folders_input);
                let _ = self.settings.save();
                Task::none()
            }
            Message::ScannerSensitivitySelected(sens) => {
                self.settings.scanner_sensitivity = sens;
                let _ = self.settings.save();
                Task::none()
            }
            Message::ToggleStrictFileNames(on) => {
                self.settings.strict_file_names = on;
                let _ = self.settings.save();
                Task::none()
            }
            Message::ToggleAutoScanLocal(on) => {
                self.settings.auto_scan_local_on_startup = on;
                let _ = self.settings.save();
                Task::none()
            }
            Message::ToggleLocalFolderTree => {
                self.local_folder_tree = !self.local_folder_tree;
                Task::none()
            }
            Message::ScanLocalMedia => {
                if self.local_scanning {
                    return Task::none();
                }
                self.local_scanning = true;
                self.local_scan_status = Some("Scanning local media folders...".to_string());
                let cfg = LocalMediaScanConfig {
                    include_folders: self
                        .settings
                        .local_media_folders
                        .iter()
                        .map(PathBuf::from)
                        .collect(),
                    excluded_folders: self
                        .settings
                        .excluded_media_folders
                        .iter()
                        .map(PathBuf::from)
                        .collect(),
                    sensitivity: self.settings.scanner_sensitivity,
                    strict_file_names: self.settings.strict_file_names,
                    cover_cache_dir: AppSettings::cover_cache_dir().ok(),
                };
                Task::perform(
                    async move {
                        tokio::task::spawn_blocking(move || scan_local_folders(&cfg))
                            .await
                            .unwrap_or_default()
                    },
                    Message::LocalMediaScanned,
                )
            }
            Message::LocalMediaScanned(tracks) => {
                self.local_scanning = false;
                let count = self.db.replace_local_tracks(&tracks).unwrap_or(tracks.len());
                self.refresh_db_views();
                let cover_urls: Vec<String> = self
                    .local_tracks
                    .iter()
                    .take(24)
                    .filter_map(|t| t.thumbnail_url.clone())
                    .collect();
                let mut tasks = Vec::new();
                for u in cover_urls {
                    tasks.push(self.fetch_cover_if_needed(Some(&u)));
                }
                self.local_scan_status = Some(format!("Indexed {count} local tracks."));
                self.set_toast(format!("Found {count} local tracks"));
                Task::batch(tasks)
            }
            Message::WindowOpened(id) => {
                self.main_window_id = Some(id);
                iced::window::get_maximized(id).map(Message::WindowMaximizedState)
            }
            Message::WindowResized(size) => {
                self.window_size = (size.width, size.height);
                if !self.mini_player && size.width > 520.0 && size.height > 280.0 {
                    self.normal_window_size = (size.width, size.height);
                }
                let mut tasks = Vec::new();
                if self.mini_player && size.height > 280.0 {
                    self.last_lyric_idx = None;
                    tasks.push(self.sync_lyrics_scroll());
                }
                if let Some(id) = self.main_window_id {
                    tasks.push(iced::window::get_maximized(id).map(Message::WindowMaximizedState));
                }
                Task::batch(tasks)
            }
            Message::WindowHandleReady(hwnd) => {
                self.window_hwnd = hwnd;
                if self.smtc.is_none() {
                    self.smtc = smtc::SmtcBridge::new(hwnd);
                }
                Task::none()
            }
            Message::DragWindow => {
                if let Some(id) = self.main_window_id {
                    iced::window::drag(id)
                } else {
                    iced::window::get_oldest().then(|opt| match opt {
                        Some(id) => iced::window::drag(id),
                        None => Task::none(),
                    })
                }
            }
            Message::MinimizeWindow => {
                if let Some(id) = self.main_window_id {
                    iced::window::minimize(id, true)
                } else {
                    iced::window::get_oldest().then(|opt| match opt {
                        Some(id) => iced::window::minimize(id, true),
                        None => Task::none(),
                    })
                }
            }
            Message::ToggleMaximizeWindow => {
                let next = !self.window_maximized;
                self.window_maximized = next;
                if let Some(id) = self.main_window_id {
                    Task::batch([
                        iced::window::toggle_maximize(id),
                        iced::window::get_maximized(id).map(Message::WindowMaximizedState),
                    ])
                } else {
                    iced::window::get_oldest().then(|opt| match opt {
                        Some(id) => iced::window::toggle_maximize(id),
                        None => Task::none(),
                    })
                }
            }
            Message::WindowMaximizedState(max) => {
                self.window_maximized = max;
                Task::none()
            }
            Message::CloseWindow => {
                self.flush_active_play_duration();
                if let Some(id) = self.main_window_id {
                    iced::window::close(id)
                } else {
                    iced::window::get_oldest().then(|opt| match opt {
                        Some(id) => iced::window::close(id),
                        None => Task::none(),
                    })
                }
            }
            Message::CycleSleepTimer => {
                let now = Instant::now();
                let next = match self.sleep_timer {
                    SleepTimerState::Off => {
                        self.set_toast("Sleep timer: 15 min");
                        SleepTimerState::Countdown(now + Duration::from_secs(15 * 60))
                    }
                    SleepTimerState::Countdown(deadline) => {
                        let rem_mins = deadline.saturating_duration_since(now).as_secs() / 60;
                        if rem_mins < 20 {
                            self.set_toast("Sleep timer: 30 min");
                            SleepTimerState::Countdown(now + Duration::from_secs(30 * 60))
                        } else if rem_mins < 40 {
                            self.set_toast("Sleep timer: 45 min");
                            SleepTimerState::Countdown(now + Duration::from_secs(45 * 60))
                        } else if rem_mins < 55 {
                            self.set_toast("Sleep timer: 60 min");
                            SleepTimerState::Countdown(now + Duration::from_secs(60 * 60))
                        } else {
                            self.set_toast("Sleep timer: End of song");
                            SleepTimerState::EndOfSong
                        }
                    }
                    SleepTimerState::EndOfSong => {
                        self.set_toast("Sleep timer off");
                        self.audio.set_fade_factor(1.0);
                        SleepTimerState::Off
                    }
                };
                self.sleep_timer = next;
                Task::none()
            }
            Message::ToggleSkipSilence(on) => {
                self.settings.skip_silence = on;
                self.audio.set_skip_silence(on);
                let _ = self.settings.save();
                Task::none()
            }
            Message::TransitionFadeSelected(opt) => {
                self.settings.transition_fade_ms = opt.ms();
                if opt.ms() == 0 {
                    self.fade_in_remaining_ms = 0;
                    self.audio.set_fade_factor(1.0);
                }
                let _ = self.settings.save();
                Task::none()
            }
            Message::EqPresetSelected(preset) => {
                self.settings.eq_preset = preset;
                self.audio.set_dsp(preset, self.settings.spatial_mode);
                let _ = self.settings.save();
                Task::none()
            }
            Message::SpatialModeSelected(mode) => {
                self.settings.spatial_mode = mode;
                self.audio.set_dsp(self.settings.eq_preset, mode);
                let _ = self.settings.save();
                Task::none()
            }
            Message::ToggleBetterLyrics(on) => {
                self.settings.better_lyrics_enabled = on;
                let _ = self.settings.save();
                if let Some(cur) = self.queue.current() {
                    self.lyrics_loading = true;
                    return self.fetch_lyrics_for_track(&cur.track);
                }
                Task::none()
            }
            Message::ToggleAutoLoadMore(on) => {
                self.settings.auto_load_more = on;
                let _ = self.settings.save();
                Task::none()
            }
            Message::ToggleRomanizeLyrics => {
                self.settings.romanize_lyrics = !self.settings.romanize_lyrics;
                let _ = self.settings.save();
                Task::none()
            }
            Message::ExportBackup => {
                let default_path = default_backup_path().ok();
                let owner_hwnd = self.window_hwnd;
                Task::perform(
                    async move {
                        dialogs::pick_backup_save(owner_hwnd, default_path.as_deref())
                    },
                    Message::ExportBackupTo,
                )
            }
            Message::ExportBackupTo(Ok(Some(path))) => {
                match export_backup(&self.db, &self.settings, &path) {
                    Ok(bytes) => {
                        let kb = bytes / 1024;
                        self.backup_status = Some(format!("Exported {kb} KB to {}", path.display()));
                        self.set_toast("Backup exported");
                    }
                    Err(e) => {
                        self.backup_status = Some(format!("Export failed: {e}"));
                    }
                }
                Task::none()
            }
            Message::ExportBackupTo(Ok(None)) => Task::none(),
            Message::ExportBackupTo(Err(e)) => {
                self.backup_status = Some(format!("Export dialog error: {e}"));
                Task::none()
            }
            Message::ImportBackup => {
                let default_dir = default_backup_path()
                    .ok()
                    .and_then(|p| p.parent().map(Path::to_path_buf));
                let owner_hwnd = self.window_hwnd;
                Task::perform(
                    async move {
                        dialogs::pick_backup_open(owner_hwnd, default_dir.as_deref())
                    },
                    Message::ImportBackupFrom,
                )
            }
            Message::ImportBackupFrom(Ok(Some(bak_path))) => {
                let db_path = match AppSettings::db_path() {
                    Ok(p) => p,
                    Err(e) => {
                        self.backup_status = Some(format!("DB path error: {e}"));
                        return Task::none();
                    }
                };
                if !bak_path.exists() {
                    self.backup_status =
                        Some(format!("No backup file found at {}", bak_path.display()));
                    return Task::none();
                }
                // Release the open SQLite handle on Windows before replacing echo.db on disk.
                self.db = EchoDb::open_in_memory().expect("in-memory sqlite");
                match validate_and_unpack_backup(&bak_path, &db_path) {
                    Ok(restored_settings) => {
                        self.db = EchoDb::open(&db_path).unwrap_or_else(|_| {
                            EchoDb::open_in_memory().expect("in-memory sqlite")
                        });
                        self.settings = restored_settings;
                        self.yt.set_locale(
                            self.settings.youtube_gl.clone(),
                            self.settings.youtube_hl.clone(),
                        );
                        self.yt
                            .set_visitor_data(self.settings.youtube_visitor_data.clone());
                        self.yt.set_cookie(self.settings.youtube_cookie.clone());
                        self.yt_cookie_input =
                            self.settings.youtube_cookie.clone().unwrap_or_default();
                        self.cache_size_input = self.settings.max_cache_size_mb.to_string();
                        self.download_folder_input =
                            self.settings.download_folder.clone().unwrap_or_default();
                        self.extra_import_input = self.settings.extra_import_folders.join("; ");
                        self.local_folders_input = self.settings.local_media_folders.join("; ");
                        self.excluded_folders_input =
                            self.settings.excluded_media_folders.join("; ");
                        self.palette = self
                            .queue
                            .current()
                            .and_then(|c| c.track.thumbnail_url.as_ref())
                            .and_then(|u| self.cover_bytes.get(u))
                            .and_then(|b| DynamicPalette::from_image_bytes(b, self.settings.theme_mode))
                            .unwrap_or_else(|| DynamicPalette::default_for(self.settings.theme_mode));
                        self.audio.set_volume(self.settings.volume);
                        self.audio.set_skip_silence(self.settings.skip_silence);
                        self.audio
                            .set_dsp(self.settings.eq_preset, self.settings.spatial_mode);
                        self.refresh_db_views();
                        self.backup_status =
                            Some(format!("Restored backup from {}", bak_path.display()));
                        self.set_toast("Backup restored");
                    }
                    Err(e) => {
                        self.db = EchoDb::open(&db_path).unwrap_or_else(|_| {
                            EchoDb::open_in_memory().expect("in-memory sqlite")
                        });
                        self.backup_status = Some(format!("Import failed: {e}"));
                    }
                }
                Task::none()
            }
            Message::ImportBackupFrom(Ok(None)) => Task::none(),
            Message::ImportBackupFrom(Err(e)) => {
                self.backup_status = Some(format!("Import dialog error: {e}"));
                Task::none()
            }
            Message::ToggleNormalization(on) => {
                self.settings.audio_normalization = on;
                let cur = self.queue.current().map(|c| &c.track);
                self.audio.set_normalization(
                    on,
                    cur.and_then(|t| t.loudness_db),
                    cur.map(|t| t.is_local).unwrap_or(false),
                );
                let _ = self.settings.save();
                Task::none()
            }
            Message::AudioQualitySelected(q) => {
                self.settings.audio_quality = q;
                let _ = self.settings.save();
                Task::none()
            }
            Message::ThemeModeSelected(mode) => {
                self.settings.theme_mode = mode;
                let current_cover = self
                    .queue
                    .current()
                    .and_then(|c| c.track.thumbnail_url.as_ref())
                    .and_then(|u| self.cover_bytes.get(u));
                self.palette = current_cover
                    .and_then(|b| DynamicPalette::from_image_bytes(b, mode))
                    .unwrap_or_else(|| DynamicPalette::default_for(mode));
                let _ = self.settings.save();
                Task::none()
            }
            Message::RecognitionSourceSelected(src) => {
                self.settings.recognition_source = src;
                let _ = self.settings.save();
                Task::none()
            }
            Message::StartFixVolumes => {
                if self.repair_running {
                    return Task::none();
                }
                let current_id = self.queue.current().map(|c| c.track.id.clone());
                let queue: Vec<Track> = self
                    .db
                    .tracks_missing_loudness()
                    .unwrap_or_default()
                    .into_iter()
                    .filter(|t| Some(&t.id) != current_id.as_ref())
                    .collect();
                if queue.is_empty() {
                    self.repair_status =
                        Some("All fetched songs already have loudness info.".to_string());
                    return Task::none();
                }
                self.repair_running = true;
                self.repair_total = queue.len();
                self.repair_done = 0;
                self.repair_fixed = 0;
                self.repair_consecutive_failures = 0;
                self.repair_queue = queue;
                self.schedule_next_volume_repair()
            }
            Message::StopFixVolumes => {
                self.repair_running = false;
                self.repair_queue.clear();
                self.repair_status = Some(format!(
                    "Stopped. Fixed {} songs, the rest are still to do.",
                    self.repair_fixed
                ));
                self.refresh_db_views();
                Task::none()
            }
            Message::FixOneVolumeDone {
                track_id,
                loudness_db,
                ok,
            } => {
                if !self.repair_running {
                    return Task::none();
                }
                self.repair_done += 1;
                if ok {
                    self.repair_consecutive_failures = 0;
                    if let Some(ldb) = loudness_db {
                        let _ = self.db.update_loudness(&track_id, ldb);
                        self.repair_fixed += 1;
                    }
                } else {
                    self.repair_consecutive_failures += 1;
                    if self.repair_consecutive_failures >= 8 {
                        self.repair_running = false;
                        self.repair_queue.clear();
                        self.repair_status = Some(format!(
                            "Stopped after several failures in a row. Fixed {}. Try again later.",
                            self.repair_fixed
                        ));
                        self.refresh_db_views();
                        return Task::none();
                    }
                }
                self.repair_status = Some(format!(
                    "Checking song {} of {}, fixed {} so far...",
                    self.repair_done, self.repair_total, self.repair_fixed
                ));
                self.schedule_next_volume_repair()
            }
        }
    }

    fn schedule_next_volume_repair(&mut self) -> Task<Message> {
        let Some(next) = self.repair_queue.pop() else {
            self.repair_running = false;
            self.repair_status = Some(format!("Finished. Fixed {} songs.", self.repair_fixed));
            self.refresh_db_views();
            return Task::none();
        };
        let yt = self.yt.clone();
        Task::perform(
            async move {
                tokio::time::sleep(Duration::from_millis(330)).await;
                match yt.fetch_loudness_db(&next.id).await {
                    Ok(ldb) => (next.id, ldb, true),
                    Err(_) => (next.id, None, false),
                }
            },
            |(track_id, loudness_db, ok)| Message::FixOneVolumeDone {
                track_id,
                loudness_db,
                ok,
            },
        )
    }

    fn left_sidebar_width(&self) -> f32 {
        if self.left_collapsed {
            72.0
        } else if self.window_size.0 < 1200.0 {
            244.0
        } else {
            290.0
        }
    }

    fn right_rail_width(&self) -> f32 {
        if self.window_size.0 < 1200.0 {
            254.0
        } else {
            290.0
        }
    }

    fn show_right_rail_effective(&self) -> bool {
        self.show_right_rail && self.window_size.0 >= 980.0
    }

    fn center_width(&self) -> f32 {
        let mut w = self.window_size.0 - 16.0 - self.left_sidebar_width() - 8.0;
        if self.show_right_rail_effective() {
            w -= self.right_rail_width() + 8.0;
        }
        w.max(320.0)
    }

    fn search_width(&self) -> f32 {
        (self.window_size.0 * 0.34).clamp(240.0, 460.0)
    }

    fn show_album_column(&self) -> bool {
        self.center_width() >= 620.0
    }

    fn view(&self) -> Element<'_, Message> {
        if self.mini_player {
            return self.view_mini_player();
        }
        let p = self.palette;

        let mut body_row = row![].spacing(8).height(Length::Fill);

        if !self.left_collapsed {
            body_row = body_row.push(self.view_left_sidebar());
        } else {
            body_row = body_row.push(self.view_collapsed_sidebar());
        }

        let center_content: Element<'_, Message> = if self.show_expanded {
            self.view_expanded_player()
        } else if let Some(label) = &self.detail_loading {
            self.view_detail_skeleton(label)
        } else if let Some(detail) = &self.viewing_detail {
            self.view_detail_page(detail)
        } else {
            match self.tab {
                NavTab::Home => self.view_discover(),
                NavTab::Explore => self.view_explore(),
                NavTab::Playlists => self.view_library(),
                NavTab::Search => self.view_search(),
                NavTab::Recognize => self.view_recognize(),
                NavTab::Stats => self.view_stats(),
                NavTab::Settings => self.view_settings(),
            }
        };

        body_row = body_row.push(
            container(center_content)
                .width(Length::Fill)
                .height(Length::Fill)
                .clip(true)
                .style(glass_panel_style(p)),
        );

        if self.show_right_rail_effective() {
            body_row = body_row.push(self.view_right_rail());
        }

        let main_content = container(
            column![self.view_top_bar(), body_row].spacing(8),
        )
        .padding(iced::Padding {
            top: 8.0,
            right: 8.0,
            bottom: 4.0,
            left: 8.0,
        })
        .width(Length::Fill)
        .height(Length::Fill);

        let base = container(
            column![main_content, self.view_player_dock()]
                .spacing(0)
                .height(Length::Fill),
        )
        .width(Length::Fill)
        .height(Length::Fill)
        .style(app_shell_style(p));

        let mut layers: Vec<Element<'_, Message>> = vec![base.into()];

        if let Some((msg, _)) = &self.toast {
            let toast_pill = container(
                text(ellipsize(msg, 64))
                    .size(13)
                    .font(bold())
                    .color(Color::WHITE)
                    .wrapping(Wrapping::None),
            )
            .padding([8, 18])
            .style(popover_panel_style(p));
            let toast_layer = container(
                column![
                    vertical_space().height(Length::Fill),
                    toast_pill,
                    vertical_space().height(92),
                ]
                .align_x(Alignment::Center),
            )
            .width(Length::Fill)
            .height(Length::Fill)
            .align_x(iced::alignment::Horizontal::Center);
            layers.push(toast_layer.into());
        }

        if self.search_dropdown_open && !self.search_suggestions.is_empty() && !self.show_expanded
        {
            let mut sug_col = column![
                row![
                    text("Suggestions").size(12).font(bold()).color(color_from_rgb(p.text_muted)),
                    horizontal_space(),
                    text("Enter to search")
                        .size(11)
                        .color(color_from_rgb(p.text_muted)),
                ]
                .padding([4, 10])
                .align_y(Alignment::Center),
            ]
            .spacing(2);
            for s in self.search_suggestions.iter().take(7) {
                sug_col = sug_col.push(
                    button(
                        row![
                            icons::icon(icons::SEARCH, 15.0, color_from_rgb(p.text_muted)),
                            text(s).size(14).color(Color::WHITE),
                        ]
                        .spacing(12)
                        .align_y(Alignment::Center),
                    )
                    .width(Length::Fill)
                    .padding([9, 12])
                    .style(ghost_row_button_style(p, false))
                    .on_press(Message::ApplySuggestion(s.clone())),
                );
            }
            let dropdown = container(sug_col)
                .width(self.search_width())
                .padding(8)
                .style(popover_panel_style(p));
            // Offset by home_btn (44) + spacing (8) so the popover aligns flush beneath search_box.
            let aligned_row = row![horizontal_space().width(52), dropdown].align_y(Alignment::Start);
            let overlay = mouse_area(
                container(
                    column![vertical_space().height(60), aligned_row].align_x(Alignment::Center),
                )
                .width(Length::Fill)
                .height(Length::Fill)
                .align_x(iced::alignment::Horizontal::Center),
            )
            .on_press(Message::CloseSearchDropdown);
            layers.push(overlay.into());
        }

        if let Some(track) = &self.track_menu {
            let muted = color_from_rgb(p.text_muted);
            let accent = color_from_rgb(p.primary);
            let is_downloaded = self.downloaded_id_set.contains(&track.id);
            let menu_row = |icon: &'static [u8],
                            label: String,
                            tint: Color,
                            msg: Message|
             -> Element<'_, Message> {
                button(
                    row![
                        icons::icon(icon, 16.0, tint),
                        text(label).size(13).color(Color::WHITE),
                    ]
                    .spacing(12)
                    .align_y(Alignment::Center),
                )
                .width(Length::Fill)
                .padding([9, 12])
                .style(ghost_row_button_style(p, false))
                .on_press(msg)
                .into()
            };

            let mut menu_col = column![
                row![
                    self.artwork_widget(track.thumbnail_url.as_deref(), 44.0, false),
                    column![
                        text(ellipsize(&track.title, 28))
                            .size(14)
                            .font(semibold())
                            .color(Color::WHITE),
                        text(ellipsize(&track.artist_display(), 30))
                            .size(12)
                            .color(muted),
                    ]
                    .spacing(2)
                    .width(Length::Fill),
                    button(icons::icon(icons::CLOSE, 15.0, muted))
                        .padding(6)
                        .style(icon_button_style(p, false))
                        .on_press(Message::CloseTrackMenu),
                ]
                .spacing(10)
                .padding([4, 6])
                .align_y(Alignment::Center),
                container(horizontal_space())
                    .height(1)
                    .width(Length::Fill)
                    .style(divider_style(0.10)),
                menu_row(
                    icons::PLAY_NEXT,
                    "Play next".to_string(),
                    accent,
                    Message::QueuePlayNext(track.clone()),
                ),
                menu_row(
                    icons::PLUS,
                    "Add to queue".to_string(),
                    Color::WHITE,
                    Message::QueueAddBack(track.clone()),
                ),
                menu_row(
                    icons::USER,
                    "Go to artist".to_string(),
                    Color::WHITE,
                    Message::OpenTrackArtist(track.clone()),
                ),
            ]
            .spacing(4);

            if track.album.is_some() {
                menu_col = menu_col.push(menu_row(
                    icons::DISC,
                    "Go to album".to_string(),
                    Color::WHITE,
                    Message::OpenTrackAlbum(track.clone()),
                ));
            }
            if !track.is_local {
                menu_col = menu_col.push(menu_row(
                    icons::DOWNLOAD,
                    if is_downloaded {
                        "Remove download".to_string()
                    } else {
                        "Download for offline".to_string()
                    },
                    if is_downloaded { accent } else { Color::WHITE },
                    if is_downloaded {
                        Message::RemoveDownload(track.id.clone())
                    } else {
                        Message::DownloadTrack(track.clone())
                    },
                ));
            }
            if !self.playlists.is_empty() {
                menu_col = menu_col.push(
                    container(horizontal_space())
                        .height(1)
                        .width(Length::Fill)
                        .style(divider_style(0.10)),
                );
                menu_col = menu_col.push(
                    container(text("Add to playlist").size(11).font(bold()).color(muted))
                        .padding([4, 10]),
                );
                for pl in self.playlists.iter().take(6) {
                    menu_col = menu_col.push(menu_row(
                        icons::LIBRARY,
                        ellipsize(&pl.name, 26),
                        muted,
                        Message::AddTrackToPlaylist {
                            playlist_id: pl.id.clone(),
                            track: track.clone(),
                        },
                    ));
                }
            }

            let card = container(menu_col)
                .width(300)
                .padding(10)
                .style(popover_panel_style(p));
            let overlay = mouse_area(
                container(card)
                    .width(Length::Fill)
                    .height(Length::Fill)
                    .align_x(iced::alignment::Horizontal::Center)
                    .align_y(iced::alignment::Vertical::Center),
            )
            .on_press(Message::CloseTrackMenu);
            layers.push(overlay.into());
        }

        stack(layers).into()
    }

    fn view_top_bar(&self) -> Element<'_, Message> {
        let p = self.palette;
        let muted = color_from_rgb(p.text_muted);

        let can_back = !self.nav_history.is_empty() || self.viewing_detail.is_some() || self.show_expanded;
        let can_fwd = !self.nav_forward.is_empty();

        let mut nav_arrows = row![
            container(icons::icon(icons::ECHO_MARK, 16.0, Color::BLACK))
                .width(28)
                .height(28)
                .align_x(iced::alignment::Horizontal::Center)
                .align_y(iced::alignment::Vertical::Center)
                .style(logo_badge_style(p)),
        ]
        .spacing(6)
        .align_y(Alignment::Center);
        if self.window_size.0 >= 1080.0 {
            nav_arrows = nav_arrows.push(text("Echo").size(14).font(bold()).color(Color::WHITE));
        }
        nav_arrows = nav_arrows.push(horizontal_space().width(4));
        let back_btn = button(icons::icon(
            icons::BACK,
            16.0,
            if can_back { Color::WHITE } else { with_alpha(muted, 0.4) },
        ))
        .padding(8)
        .style(icon_button_style(p, false));
        nav_arrows = nav_arrows.push(if can_back {
            back_btn.on_press(Message::NavigateBackDetail)
        } else {
            back_btn
        });

        let fwd_btn = button(icons::icon(
            icons::FORWARD,
            16.0,
            if can_fwd { Color::WHITE } else { with_alpha(muted, 0.4) },
        ))
        .padding(8)
        .style(icon_button_style(p, false));
        nav_arrows = nav_arrows.push(if can_fwd {
            fwd_btn.on_press(Message::NavigateForwardDetail)
        } else {
            fwd_btn
        });

        let home_active =
            self.tab == NavTab::Home && self.viewing_detail.is_none() && !self.show_expanded;
        let home_btn = self.with_tooltip(
            button(
                container(icons::icon(
                    icons::HOME,
                    21.0,
                    if home_active { Color::WHITE } else { muted },
                ))
                .width(44)
                .height(44)
                .align_x(iced::alignment::Horizontal::Center)
                .align_y(iced::alignment::Vertical::Center),
            )
            .padding(0)
            .style(top_pill_button_style(p, false))
            .on_press(Message::SelectTab(NavTab::Home)),
            "Home",
            tooltip::Position::Bottom,
        );

        let explore_active =
            self.tab == NavTab::Explore && self.viewing_detail.is_none() && !self.show_expanded;
        let mut search_row = row![
            icons::icon(icons::SEARCH, 19.0, muted),
            text_input("What do you want to play? (Ctrl+K)", &self.search_query)
                .id(self.search_input_id.clone())
                .on_input(Message::SearchQueryChanged)
                .on_submit(Message::SubmitSearch)
                .style(bare_input_style(p))
                .padding([11, 4])
                .size(14)
                .width(Length::Fill),
        ]
        .spacing(10)
        .align_y(Alignment::Center);
        if !self.search_query.is_empty() {
            search_row = search_row.push(
                button(icons::icon(icons::CLOSE, 15.0, muted))
                    .padding(6)
                    .style(icon_button_style(p, false))
                    .on_press(Message::SearchQueryChanged(String::new())),
            );
        }
        search_row = search_row
            .push(container(horizontal_space()).width(1).height(22).style(divider_style(0.16)))
            .push(self.with_tooltip(
                button(icons::icon(
                    icons::COMPASS,
                    19.0,
                    if explore_active { Color::WHITE } else { muted },
                ))
                .padding(6)
                .style(icon_button_style(p, false))
                .on_press(Message::SelectTab(NavTab::Explore)),
                "Explore",
                tooltip::Position::Bottom,
            ));
        let search_box = container(search_row)
            .width(self.search_width())
            .padding([0, 14])
            .style(search_field_style(p));

        let rail_icon =
            |icon: &'static [u8], tab: NavTab, label: &'static str| -> Element<'_, Message> {
                let active = self.tab == tab && self.viewing_detail.is_none() && !self.show_expanded;
                self.with_tooltip(
                    button(icons::icon(
                        icon,
                        18.0,
                        if active { color_from_rgb(p.primary) } else { muted },
                    ))
                    .padding(9)
                    .style(icon_button_style(p, active))
                    .on_press(Message::SelectTab(tab)),
                    label,
                    tooltip::Position::Bottom,
                )
            };

        let account_initial = if self.yt.has_cookie() { "Y" } else { "E" };
        let settings_active =
            self.tab == NavTab::Settings && self.viewing_detail.is_none() && !self.show_expanded;
        let right_actions = row![
            rail_icon(icons::MIC, NavTab::Recognize, "Song Recognition"),
            rail_icon(icons::STATS, NavTab::Stats, "Listening Stats"),
            horizontal_space().width(2),
            self.with_tooltip(
                button(
                    container(
                        text(account_initial)
                            .size(12)
                            .font(semibold())
                            .color(if self.yt.has_cookie() {
                                Color::BLACK
                            } else {
                                Color::WHITE
                            }),
                    )
                    .width(28)
                    .height(28)
                    .align_x(iced::alignment::Horizontal::Center)
                    .align_y(iced::alignment::Vertical::Center)
                    .style(account_badge_style(p, self.yt.has_cookie(), settings_active)),
                )
                .padding(0)
                .style(icon_button_style(p, false))
                .on_press(Message::SelectTab(NavTab::Settings)),
                if self.yt.has_cookie() {
                    "YouTube Music Connected — Account & Settings"
                } else {
                    "Account & Settings"
                },
                tooltip::Position::Bottom,
            ),
        ]
        .spacing(6)
        .align_y(Alignment::Center);

        let win_icon_tint = Color::from_rgba(1.0, 1.0, 1.0, 0.78);
        let win_controls = row![
            button(
                container(icons::icon(icons::WIN_MINIMIZE, 12.0, win_icon_tint))
                    .width(34)
                    .height(28)
                    .align_x(iced::alignment::Horizontal::Center)
                    .align_y(iced::alignment::Vertical::Center),
            )
            .padding(0)
            .style(window_control_button_style(p, false))
            .on_press(Message::MinimizeWindow),
            button(
                container(icons::icon(
                    if self.window_maximized {
                        icons::WIN_RESTORE
                    } else {
                        icons::WIN_MAXIMIZE
                    },
                    11.0,
                    win_icon_tint
                ))
                .width(34)
                .height(28)
                .align_x(iced::alignment::Horizontal::Center)
                .align_y(iced::alignment::Vertical::Center),
            )
            .padding(0)
            .style(window_control_button_style(p, false))
            .on_press(Message::ToggleMaximizeWindow),
            button(
                container(icons::icon(icons::WIN_CLOSE, 11.0, win_icon_tint))
                    .width(34)
                    .height(28)
                    .align_x(iced::alignment::Horizontal::Center)
                    .align_y(iced::alignment::Vertical::Center),
            )
            .padding(0)
            .style(window_control_button_style(p, true))
            .on_press(Message::CloseWindow),
        ]
        .spacing(2)
        .align_y(Alignment::Center);

        let right_cluster = row![
            right_actions,
            horizontal_space().width(10),
            container(horizontal_space())
                .width(1)
                .height(16)
                .style(divider_style(0.08)),
            horizontal_space().width(8),
            win_controls,
        ]
        .padding([0, 2])
        .align_y(Alignment::Center);

        mouse_area(
            row![
                container(nav_arrows)
                    .width(Length::FillPortion(1))
                    .padding([0, 6]),
                row![home_btn, search_box]
                    .spacing(8)
                    .align_y(Alignment::Center),
                container(right_cluster)
                    .width(Length::FillPortion(1))
                    .align_x(iced::alignment::Horizontal::Right),
            ]
            .align_y(Alignment::Center),
        )
        .on_press(Message::DragWindow)
        .into()
    }

    fn view_collapsed_sidebar(&self) -> Element<'_, Message> {
        let p = self.palette;
        let muted = color_from_rgb(p.text_muted);

        let in_library =
            self.tab == NavTab::Playlists && self.viewing_detail.is_none() && !self.show_expanded;
        let mut col = column![
            button(icons::icon(icons::LIBRARY, 22.0, muted))
                .padding(10)
                .style(icon_button_style(p, false))
                .on_press(Message::ToggleLeftSidebar),
            button(icons::icon(icons::PLUS, 18.0, muted))
                .padding(8)
                .style(icon_button_style(p, self.show_new_playlist_input))
                .on_press(Message::ExpandAndNewPlaylist),
            vertical_space().height(4),
            button(
                container(icons::icon(icons::HEART_FILLED, 20.0, Color::WHITE))
                    .width(48)
                    .height(48)
                    .align_x(iced::alignment::Horizontal::Center)
                    .align_y(iced::alignment::Vertical::Center)
                    .style(liked_gradient_badge_style()),
            )
            .padding(4)
            .style(ghost_row_button_style(
                p,
                in_library && self.library_tab == LibraryTab::Liked,
            ))
            .on_press(Message::SelectLibraryTab(LibraryTab::Liked)),
            button(
                container(icons::icon(
                    icons::DOWNLOAD,
                    20.0,
                    color_from_rgb(p.primary),
                ))
                .width(48)
                .height(48)
                .align_x(iced::alignment::Horizontal::Center)
                .align_y(iced::alignment::Vertical::Center)
                .style(downloads_badge_style()),
            )
            .padding(4)
            .style(ghost_row_button_style(
                p,
                in_library && self.library_tab == LibraryTab::Downloads,
            ))
            .on_press(Message::SelectLibraryTab(LibraryTab::Downloads)),
            button(
                container(icons::icon(icons::FOLDER, 20.0, Color::WHITE))
                    .width(48)
                    .height(48)
                    .align_x(iced::alignment::Horizontal::Center)
                    .align_y(iced::alignment::Vertical::Center)
                    .style(local_badge_style()),
            )
            .padding(4)
            .style(ghost_row_button_style(
                p,
                in_library && self.library_tab == LibraryTab::Local,
            ))
            .on_press(Message::SelectLibraryTab(LibraryTab::Local)),
        ]
        .spacing(4)
        .align_x(Alignment::Center);

        for (idx, pl) in self.playlists.iter().enumerate() {
            let active = in_library && self.library_tab == LibraryTab::Playlist(idx);
            col = col.push(
                button(self.artwork_widget(pl.thumbnail_url.as_deref(), 48.0, false))
                    .padding(4)
                    .style(ghost_row_button_style(p, active))
                    .on_press(Message::SelectLibraryTab(LibraryTab::Playlist(idx))),
            );
        }

        for pl in self.account_playlists.iter().take(6) {
            let active = self.active_playlist_id() == Some(pl.id.as_str());
            col = col.push(
                button(self.artwork_widget(pl.thumbnail_url.as_deref(), 48.0, false))
                    .padding(4)
                    .style(ghost_row_button_style(p, active))
                    .on_press(Message::OpenPlaylist {
                        id: pl.id.clone(),
                        title: pl.title.clone(),
                    }),
            );
        }

        container(self.vscroll(col))
            .width(72)
            .height(Length::Fill)
            .padding([12, 4])
            .align_x(iced::alignment::Horizontal::Center)
            .style(glass_panel_style(p))
            .into()
    }

    fn view_left_sidebar(&self) -> Element<'_, Message> {
        let p = self.palette;
        let muted = color_from_rgb(p.text_muted);

        let header = row![
            button(
                row![
                    icons::icon(icons::LIBRARY, 22.0, muted),
                    text("Your Library").size(15).font(bold()).color(muted),
                ]
                .spacing(12)
                .align_y(Alignment::Center),
            )
            .padding([6, 8])
            .style(ghost_row_button_style(p, false))
            .on_press(Message::ToggleLeftSidebar),
            horizontal_space(),
            button(icons::icon(icons::PLUS, 18.0, muted))
                .padding(8)
                .style(icon_button_style(p, self.show_new_playlist_input))
                .on_press(Message::ToggleNewPlaylistInput),
        ]
        .align_y(Alignment::Center);

        let filter_pill = |label: &'static str, f: LibraryFilter| -> Element<'_, Message> {
            let active = self.library_filter == f;
            button(text(label).size(12))
                .padding([5, 11])
                .style(top_pill_button_style(p, active))
                .on_press(Message::LibraryFilterSelected(f))
                .into()
        };
        let filter_row = row![
            filter_pill("Playlists", LibraryFilter::Playlists),
            filter_pill("Downloads", LibraryFilter::Downloads),
            filter_pill("Local", LibraryFilter::Local),
        ]
        .spacing(6);

        let mut list = column![filter_row, vertical_space().height(6)].spacing(2);

        if self.show_new_playlist_input {
            list = list.push(
                row![
                    text_input("Playlist name", &self.new_playlist_name)
                        .on_input(Message::NewPlaylistNameChanged)
                        .on_submit(Message::CreatePlaylist)
                        .style(pill_search_input_style(p))
                        .padding([8, 14])
                        .size(14)
                        .width(Length::Fill),
                    button(text("Create").size(13))
                        .padding([8, 14])
                        .style(top_pill_button_style(p, true))
                        .on_press(Message::CreatePlaylist),
                ]
                .spacing(8)
                .align_y(Alignment::Center),
            );
            list = list.push(vertical_space().height(6));
        }

        let show_playlists =
            matches!(self.library_filter, LibraryFilter::All | LibraryFilter::Playlists);
        let show_downloads =
            matches!(self.library_filter, LibraryFilter::All | LibraryFilter::Downloads);
        let show_local = matches!(self.library_filter, LibraryFilter::All | LibraryFilter::Local);

        let in_library =
            self.tab == NavTab::Playlists && self.viewing_detail.is_none() && !self.show_expanded;

        if show_playlists {
            let liked_active = in_library && self.library_tab == LibraryTab::Liked;
            let liked_badge = container(icons::icon(icons::HEART_FILLED, 20.0, Color::WHITE))
                .width(48)
                .height(48)
                .align_x(iced::alignment::Horizontal::Center)
                .align_y(iced::alignment::Vertical::Center)
                .style(liked_gradient_badge_style());
            list = list.push(self.sidebar_badge_item(
                "Liked Songs",
                format!("Playlist • {} songs", self.liked_tracks.len()),
                liked_badge.into(),
                liked_active,
                Message::SelectLibraryTab(LibraryTab::Liked),
            ));
        }

        if show_downloads {
            let dl_active = in_library && self.library_tab == LibraryTab::Downloads;
            let dl_badge =
                container(icons::icon(icons::DOWNLOAD, 20.0, color_from_rgb(p.primary)))
                    .width(48)
                    .height(48)
                    .align_x(iced::alignment::Horizontal::Center)
                    .align_y(iced::alignment::Vertical::Center)
                    .style(downloads_badge_style());
            list = list.push(self.sidebar_badge_item(
                "Downloads",
                format!("Offline • {} tracks", self.downloaded_tracks.len()),
                dl_badge.into(),
                dl_active,
                Message::SelectLibraryTab(LibraryTab::Downloads),
            ));
        }

        if show_local {
            let local_active = in_library && self.library_tab == LibraryTab::Local;
            let local_badge = container(icons::icon(icons::FOLDER, 20.0, Color::WHITE))
                .width(48)
                .height(48)
                .align_x(iced::alignment::Horizontal::Center)
                .align_y(iced::alignment::Vertical::Center)
                .style(local_badge_style());
            list = list.push(self.sidebar_badge_item(
                "Local Files",
                format!("Folder • {} tracks", self.local_tracks.len()),
                local_badge.into(),
                local_active,
                Message::SelectLibraryTab(LibraryTab::Local),
            ));
        }

        if self.library_filter == LibraryFilter::All {
            let hist_thumb = self.history_tracks.first().and_then(|t| t.thumbnail_url.as_deref());
            list = list.push(self.sidebar_item(
                "Recently Played",
                format!("History • {} tracks", self.history_tracks.len()),
                hist_thumb,
                in_library && self.library_tab == LibraryTab::History,
                Message::SelectLibraryTab(LibraryTab::History),
            ));
        }

        if show_playlists {
            for (idx, pl) in self.playlists.iter().enumerate() {
                let active = in_library && self.library_tab == LibraryTab::Playlist(idx);
                list = list.push(self.sidebar_item(
                    &pl.name,
                    format!("Playlist • {} songs", pl.track_count),
                    pl.thumbnail_url.as_deref(),
                    active,
                    Message::SelectLibraryTab(LibraryTab::Playlist(idx)),
                ));
            }
        }

        if !self.account_playlists.is_empty() {
            list = list.push(vertical_space().height(14));
            list = list.push(
                container(text("YouTube Account").size(13).font(bold()).color(muted))
                    .padding([0, 8]),
            );
            list = list.push(vertical_space().height(4));
            for pl in &self.account_playlists {
                let active = self.active_playlist_id() == Some(pl.id.as_str());
                let sub = pl
                    .song_count_text
                    .clone()
                    .unwrap_or_else(|| pl.author.clone().unwrap_or_else(|| "Account Playlist".into()));
                list = list.push(self.sidebar_item(
                    &pl.title,
                    sub,
                    pl.thumbnail_url.as_deref(),
                    active,
                    Message::OpenPlaylist {
                        id: pl.id.clone(),
                        title: pl.title.clone(),
                    },
                ));
            }
        }

        let mut seen_remote = HashSet::new();
        let remote_items: Vec<&BrowseItem> = self
            .home_sections
            .iter()
            .flat_map(|s| s.items.iter())
            .filter(|i| {
                matches!(
                    i,
                    BrowseItem::Playlist { .. }
                        | BrowseItem::Album {
                            playlist_id: Some(_),
                            ..
                        }
                ) && seen_remote.insert(i.stable_key())
            })
            .take(8)
            .collect();

        if !remote_items.is_empty() {
            list = list.push(vertical_space().height(14));
            list = list.push(
                container(text("Made for you").size(13).font(bold()).color(muted)).padding([0, 8]),
            );
            list = list.push(vertical_space().height(4));
            for item in remote_items {
                match item {
                    BrowseItem::Playlist {
                        id,
                        title,
                        author,
                        thumbnail_url,
                    } => {
                        let active = self.active_playlist_id() == Some(id.as_str());
                        list = list.push(self.sidebar_item(
                            title,
                            format!("Playlist • {author}"),
                            thumbnail_url.as_deref(),
                            active,
                            Message::OpenPlaylist {
                                id: id.clone(),
                                title: title.clone(),
                            },
                        ));
                    }
                    BrowseItem::Album {
                        browse_id,
                        playlist_id: Some(pid),
                        title,
                        artist,
                        thumbnail_url,
                        ..
                    } => {
                        let active = self.active_playlist_id() == Some(pid.as_str())
                            || matches!(
                                &self.viewing_detail,
                                Some(DetailPage::Album(a)) if &a.browse_id == browse_id
                            );
                        list = list.push(self.sidebar_item(
                            title,
                            format!("Album • {artist}"),
                            thumbnail_url.as_deref(),
                            active,
                            Message::OpenAlbum {
                                browse_id: browse_id.clone(),
                                title: title.clone(),
                            },
                        ));
                    }
                    _ => {}
                }
            }
        }

        container(
            column![
                header,
                vertical_space().height(8),
                self.vscroll(list)
            ]
            .spacing(4),
        )
        .width(self.left_sidebar_width())
        .height(Length::Fill)
        .padding([12, 8])
        .style(glass_panel_style(p))
        .into()
    }

    fn sidebar_item<'a>(
        &'a self,
        title: &'a str,
        subtitle: String,
        thumb: Option<&'a str>,
        active: bool,
        msg: Message,
    ) -> Element<'a, Message> {
        let icon = self.artwork_widget(thumb, 48.0, false);
        self.sidebar_badge_item(title, subtitle, icon, active, msg)
    }

    fn sidebar_badge_item<'a>(
        &'a self,
        title: &'a str,
        subtitle: String,
        icon: Element<'a, Message>,
        active: bool,
        msg: Message,
    ) -> Element<'a, Message> {
        let p = self.palette;
        button(
            row![
                icon,
                column![
                    text(ellipsize(title, 26))
                        .size(14)
                        .font(bold())
                        .wrapping(Wrapping::None)
                        .color(if active {
                            color_from_rgb(p.primary)
                        } else {
                            Color::WHITE
                        }),
                    text(ellipsize(&subtitle, 30))
                        .size(12)
                        .wrapping(Wrapping::None)
                        .color(color_from_rgb(p.text_muted)),
                ]
                .spacing(3)
                .width(Length::Fill),
            ]
            .spacing(12)
            .align_y(Alignment::Center),
        )
        .width(Length::Fill)
        .padding([6, 8])
        .style(ghost_row_button_style(p, active))
        .on_press(msg)
        .into()
    }

    /// A page of tracks: the cover's colour fading down from a large header, then the green play
    /// button, then the list. The whole page scrolls as one, so the header moves away as you read.
    fn view_hero_and_table<'a>(
        &'a self,
        title: &'a str,
        subtitle: String,
        cover_url: Option<&'a str>,
        tracks: &'a [Track],
        footer: Option<Element<'a, Message>>,
    ) -> Element<'a, Message> {
        let p = self.palette;
        let muted = color_from_rgb(p.text_muted);

        let total_secs: u64 = tracks
            .iter()
            .filter_map(|t| t.duration_secs.map(|d| d as u64))
            .sum();
        let dur_summary = if total_secs >= 3600 {
            format!("{} hr {} min", total_secs / 3600, (total_secs % 3600) / 60)
        } else if total_secs > 0 {
            format!("{} min", total_secs / 60)
        } else {
            String::new()
        };

        let mut parts = subtitle.splitn(2, '•');
        let kind = parts.next().unwrap_or("Playlist").trim().to_string();
        let rest = parts.next().map(|s| s.trim().to_string()).unwrap_or_default();
        let meta = [rest, dur_summary]
            .into_iter()
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>()
            .join(" • ");

        let title_len = title.chars().count();
        let title_size: u16 = if self.center_width() < 640.0 || title_len > 28 {
            32
        } else if title_len > 16 {
            46
        } else {
            60
        };

        let hero_art = container(self.artwork_widget(cover_url, 180.0, false))
            .style(artwork_frame_style(12.0, 0.55));
        let hero_col = row![
            hero_art,
            column![
                text(kind.to_uppercase())
                    .size(11)
                    .font(bold())
                    .color(Color::from_rgba(1.0, 1.0, 1.0, 0.80)),
                text(title).size(title_size).font(bold()),
                text(meta)
                    .size(14)
                    .color(Color::from_rgba(1.0, 1.0, 1.0, 0.78)),
            ]
            .spacing(8)
            .width(Length::Fill),
        ]
        .spacing(24)
        .align_y(Alignment::End);
        let hero = container(hero_col)
            .padding([26, 24])
            .width(Length::Fill)
            .style(hero_banner_style(p));

        let mut toolbar = row![].spacing(20).align_y(Alignment::Center);
        if !tracks.is_empty() {
            toolbar = toolbar.push(
                button(
                    container(icons::icon(icons::PLAY, 24.0, Color::BLACK))
                        .width(56)
                        .height(56)
                        .align_x(iced::alignment::Horizontal::Center)
                        .align_y(iced::alignment::Vertical::Center),
                )
                .padding(0)
                .style(glowing_play_button_style(p))
                .on_press(Message::PlayTracks(tracks.to_vec(), 0)),
            );
        }
        toolbar = toolbar.push(
            button(icons::icon(
                icons::SHUFFLE,
                26.0,
                if self.queue.shuffle {
                    color_from_rgb(p.primary)
                } else {
                    muted
                },
            ))
            .padding(8)
            .style(icon_button_style(p, false))
            .on_press(Message::ToggleShuffle),
        );
        if tracks.iter().any(|t| !t.is_local) {
            toolbar = toolbar.push(
                button(
                    row![
                        icons::icon(icons::DOWNLOAD, 15.0, Color::WHITE),
                        text("Download").size(13),
                    ]
                    .spacing(6)
                    .align_y(Alignment::Center),
                )
                .padding([8, 16])
                .style(top_pill_button_style(p, false))
                .on_press(Message::DownloadTracks(tracks.to_vec())),
            );
        }
        if let LibraryTab::Playlist(idx) = self.library_tab {
            if self.tab == NavTab::Playlists {
                if let Some(pl) = self.playlists.get(idx) {
                    if self.queue.current().is_some() {
                        toolbar = toolbar.push(
                            button(text("Add current song").size(13))
                                .padding([8, 16])
                                .style(top_pill_button_style(p, false))
                                .on_press(Message::AddCurrentToPlaylist(pl.id.clone())),
                        );
                    }
                    toolbar = toolbar.push(
                        button(text("Delete playlist").size(13).color(color_from_rgb(p.danger)))
                            .padding([8, 16])
                            .style(top_pill_button_style(p, false))
                            .on_press(Message::DeletePlaylist(pl.id.clone())),
                    );
                }
            }
        }

        let rows: Element<'_, Message> = if tracks.is_empty() {
            container(
                text("Nothing here yet. Play something and it will show up.")
                    .size(15)
                    .color(muted),
            )
            .padding([24, 28])
            .into()
        } else {
            let list = tracks
                .iter()
                .enumerate()
                .fold(column![].spacing(0), |col, (idx, t)| {
                    col.push(self.table_track_row(idx + 1, t, Some((tracks.to_vec(), idx))))
                });
            container(list).padding([0, 16]).into()
        };

        let mut body = column![
            hero,
            container(toolbar).padding([20, 24]),
            self.table_header(),
            rows,
        ]
        .spacing(0);
        if let Some(f) = footer {
            body = body.push(f);
        }
        self.vscroll(body.push(vertical_space().height(24)))
    }

    fn table_header(&self) -> Element<'_, Message> {
        let p = self.palette;
        let muted = color_from_rgb(p.text_muted);
        let mut cols = row![
            text("#").size(13).color(muted).width(32),
            text("Title")
                .size(13)
                .color(muted)
                .width(Length::FillPortion(5)),
        ]
        .spacing(12)
        .width(Length::Fill)
        .align_y(Alignment::Center);
        if self.show_album_column() {
            cols = cols.push(
                text("Album")
                    .size(13)
                    .color(muted)
                    .width(Length::FillPortion(3)),
            );
        }
        cols = cols.push(
            container(icons::icon(icons::CLOCK, 16.0, muted))
                .width(48)
                .align_x(iced::alignment::Horizontal::Right),
        );
        column![
            container(
                row![cols, horizontal_space().width(132)]
                    .spacing(0)
                    .padding([8, 12])
                    .align_y(Alignment::Center),
            )
            .padding([0, 16]),
            container(container(horizontal_space()).height(1).width(Length::Fill).style(divider_style(0.10)))
                .padding([0, 28]),
        ]
        .spacing(0)
        .into()
    }

    fn skeleton_pulse(&self) -> f32 {
        0.06 + 0.05 * (self.anim_phase.sin() * 0.5 + 0.5)
    }

    fn view_discover_skeleton(&self) -> Element<'_, Message> {
        let pulse = self.skeleton_pulse();
        let chips = (0..5).fold(row![].spacing(8), |r, _| {
            r.push(
                container(horizontal_space())
                    .width(74)
                    .height(30)
                    .style(skeleton_block_style(pulse, 999.0)),
            )
        });
        let tile_row = || {
            (0..4).fold(row![].spacing(12), |r, _| {
                r.push(
                    container(horizontal_space())
                        .width(Length::Fill)
                        .height(68)
                        .style(skeleton_block_style(pulse, 12.0)),
                )
            })
        };
        let card_row = || {
            (0..5).fold(row![].spacing(16), |r, _| {
                r.push(
                    column![
                        container(horizontal_space())
                            .width(148)
                            .height(148)
                            .style(skeleton_block_style(pulse, 10.0)),
                        container(horizontal_space())
                            .width(124)
                            .height(14)
                            .style(skeleton_block_style(pulse * 0.9, 4.0)),
                        container(horizontal_space())
                            .width(86)
                            .height(11)
                            .style(skeleton_block_style(pulse * 0.65, 4.0)),
                    ]
                    .spacing(8)
                    .width(172)
                    .padding(12),
                )
            })
        };
        column![
            container(horizontal_space())
                .width(190)
                .height(28)
                .style(skeleton_block_style(pulse * 1.1, 6.0)),
            chips,
            column![tile_row(), tile_row()].spacing(10),
            vertical_space().height(8),
            container(horizontal_space())
                .width(160)
                .height(22)
                .style(skeleton_block_style(pulse, 6.0)),
            card_row(),
            container(horizontal_space())
                .width(140)
                .height(22)
                .style(skeleton_block_style(pulse, 6.0)),
            card_row(),
        ]
        .spacing(16)
        .padding([22, 24])
        .into()
    }

    fn view_detail_skeleton<'a>(&'a self, label: &'a str) -> Element<'a, Message> {
        let p = self.palette;
        let pulse = self.skeleton_pulse();
        let muted = color_from_rgb(p.text_muted);
        let mut rows = column![].spacing(10);
        for i in 0..8 {
            let factor = if i % 2 == 0 { 1.0 } else { 0.78 };
            rows = rows.push(
                row![
                    container(horizontal_space())
                        .width(40)
                        .height(40)
                        .style(skeleton_block_style(pulse, 6.0)),
                    column![
                        container(horizontal_space())
                            .width(220.0 * factor)
                            .height(14)
                            .style(skeleton_block_style(pulse, 4.0)),
                        container(horizontal_space())
                            .width(140.0 * factor)
                            .height(11)
                            .style(skeleton_block_style(pulse * 0.7, 4.0)),
                    ]
                    .spacing(6)
                    .width(Length::Fill),
                    container(horizontal_space())
                        .width(42)
                        .height(12)
                        .style(skeleton_block_style(pulse * 0.7, 4.0)),
                ]
                .spacing(12)
                .align_y(Alignment::Center),
            );
        }
        column![
            container(
                row![
                    container(horizontal_space())
                        .width(180)
                        .height(180)
                        .style(skeleton_block_style(pulse * 1.1, 12.0)),
                    column![
                        text(format!("Loading {label}...")).size(13).color(muted),
                        container(horizontal_space())
                            .width(280)
                            .height(34)
                            .style(skeleton_block_style(pulse * 1.1, 6.0)),
                        container(horizontal_space())
                            .width(180)
                            .height(14)
                            .style(skeleton_block_style(pulse * 0.8, 4.0)),
                    ]
                    .spacing(12),
                ]
                .spacing(24)
                .align_y(Alignment::End),
            )
            .padding([24, 24])
            .width(Length::Fill)
            .style(hero_banner_style(p)),
            container(rows).padding([20, 24]),
        ]
        .spacing(0)
        .into()
    }

    fn browse_shelf_section<'a>(
        &'a self,
        shelf_key: String,
        title: &'a str,
        items: &'a [BrowseItem],
        show_explore_link: bool,
    ) -> Element<'a, Message> {
        let p = self.palette;
        let muted = color_from_rgb(p.text_muted);
        let cw = self.center_width();
        let per_page = (((cw - 48.0 + 16.0) / (172.0 + 16.0)).floor() as usize).clamp(1, 12);
        let max_offset = items.len().saturating_sub(per_page);
        let offset = self
            .shelf_offsets
            .get(&shelf_key)
            .copied()
            .unwrap_or(0)
            .min(max_offset);
        let can_prev = offset > 0;
        let can_next = offset < max_offset;
        let step = (per_page.max(2) / 2).max(1) as isize;

        let shelf = responsive(move |size| {
            let n = (((size.width + 16.0) / (172.0 + 16.0)).floor() as usize).clamp(1, 12);
            items
                .iter()
                .skip(offset.min(items.len().saturating_sub(n)))
                .take(n)
                .fold(row![].spacing(16), |r, it| r.push(self.browse_card(it)))
                .into()
        });

        let mut header_right = row![].spacing(6).align_y(Alignment::Center);
        if items.len() > per_page {
            let prev_btn = button(
                container(icons::icon(
                    icons::BACK,
                    14.0,
                    if can_prev { Color::WHITE } else { with_alpha(muted, 0.35) },
                ))
                .width(28)
                .height(28)
                .align_x(iced::alignment::Horizontal::Center)
                .align_y(iced::alignment::Vertical::Center),
            )
            .padding(0)
            .style(pager_button_style(p, can_prev));
            let prev_btn = if can_prev {
                prev_btn.on_press(Message::ShelfScroll {
                    key: shelf_key.clone(),
                    delta: -step,
                })
            } else {
                prev_btn
            };

            let next_btn = button(
                container(icons::icon(
                    icons::FORWARD,
                    14.0,
                    if can_next { Color::WHITE } else { with_alpha(muted, 0.35) },
                ))
                .width(28)
                .height(28)
                .align_x(iced::alignment::Horizontal::Center)
                .align_y(iced::alignment::Vertical::Center),
            )
            .padding(0)
            .style(pager_button_style(p, can_next));
            let next_btn = if can_next {
                next_btn.on_press(Message::ShelfScroll {
                    key: shelf_key,
                    delta: step,
                })
            } else {
                next_btn
            };

            header_right = header_right.push(prev_btn).push(next_btn);
        }
        if show_explore_link {
            header_right = header_right.push(
                button(text("Explore all").size(12).font(semibold()).color(muted))
                    .padding([5, 10])
                    .style(ghost_row_button_style(p, false))
                    .on_press(Message::SelectTab(NavTab::Explore)),
            );
        }

        container(
            column![
                row![
                    text(title).size(22).font(bold()),
                    horizontal_space(),
                    header_right,
                ]
                .align_y(Alignment::Center),
                container(shelf)
                    .width(Length::Fill)
                    .height(Length::Fixed(248.0)),
            ]
            .spacing(12),
        )
        .padding([12, 24])
        .into()
    }

    fn view_discover(&self) -> Element<'_, Message> {
        let p = self.palette;
        let muted = color_from_rgb(p.text_muted);

        if self.home_loading {
            return self.view_discover_skeleton();
        }

        if let Some(err) = &self.home_error {
            return container(
                column![
                    text("Couldn't reach YouTube Music")
                        .size(22)
                        .font(bold())
                        .color(color_from_rgb(p.danger)),
                    text(ellipsize(err, 140)).size(14).color(muted),
                    button(text("Try again").size(14))
                        .padding([10, 22])
                        .style(top_pill_button_style(p, true))
                        .on_press(Message::LoadHome),
                ]
                .spacing(12),
            )
            .padding(32)
            .into();
        }

        let mut header_col = column![].spacing(18);
        let accent = color_from_rgb(p.primary);
        let active_chip_title = self
            .home_active_chip
            .as_deref()
            .and_then(|params| self.home_chips.iter().find(|c| c.params == params))
            .map(|c| c.title.as_str());

        let mut chip_bar = row![
            button(text("All").size(13).font(semibold()))
                .padding([7, 16])
                .style(top_pill_button_style(p, self.home_active_chip.is_none()))
                .on_press(Message::SelectHomeChip(None)),
        ]
        .spacing(8)
        .align_y(Alignment::Center);
        for chip in self.home_chips.iter().take(8) {
            let active = self.home_active_chip.as_deref() == Some(chip.params.as_str());
            chip_bar = chip_bar.push(
                button(text(&chip.title).size(13).font(if active { semibold() } else { Font::DEFAULT }))
                    .padding([7, 16])
                    .style(top_pill_button_style(p, active))
                    .on_press(Message::SelectHomeChip(Some(chip.params.clone()))),
            );
        }
        header_col = header_col.push(
            row![
                scrollable(chip_bar)
                    .direction(scrollable::Direction::Horizontal(
                        scrollable::Scrollbar::new().width(0).scroller_width(0),
                    ))
                    .style(scroll_style(p))
                    .width(Length::Fill),
                button(
                    row![
                        icons::icon(icons::REFRESH, 13.0, muted),
                        text("Refresh").size(12).font(semibold()),
                    ]
                    .spacing(6)
                    .align_y(Alignment::Center),
                )
                .padding([7, 14])
                .style(soft_pill_button_style(p))
                .on_press(Message::LoadHome),
            ]
            .spacing(12)
            .align_y(Alignment::Center),
        );

        let mut mix_tracks: Vec<Track> = Vec::new();
        let mut seen_mix_ids = HashSet::new();
        for it in self
            .home_quick_tiles
            .iter()
            .chain(self.home_sections.iter().flat_map(|s| s.items.iter()))
        {
            if let BrowseItem::Track(t) = it {
                if seen_mix_ids.insert(t.id.clone()) {
                    mix_tracks.push(t.clone());
                    if mix_tracks.len() >= 20 {
                        break;
                    }
                }
            }
        }

        let kicker = match active_chip_title {
            Some(mood) => format!("MOOD MIX • {}", mood.to_uppercase()),
            None => greeting_kicker().to_string(),
        };
        let subline = match active_chip_title {
            Some(mood) => format!("Curated {mood} picks tuned for right now"),
            None if self.listener_ctx.known => {
                "Personalized picks shaped by your taste graph & active session".to_string()
            }
            None => "Fresh tracks, artists & mixes ready to play".to_string(),
        };
        let mut greeting_row = row![
            column![
                text(kicker).size(11).font(bold()).color(accent),
                text(greeting_label()).size(30).font(bold()),
                text(subline).size(12).color(muted),
            ]
            .spacing(3),
            horizontal_space(),
        ]
        .spacing(12)
        .align_y(Alignment::End);
        if !mix_tracks.is_empty() {
            greeting_row = greeting_row.push(
                button(
                    row![
                        icons::icon(icons::PLAY, 12.0, Color::BLACK),
                        text("Play Mix").size(13).font(bold()),
                    ]
                    .spacing(7)
                    .align_y(Alignment::Center),
                )
                .padding([9, 18])
                .style(glowing_play_button_style(p))
                .on_press(Message::PlayTracks(mix_tracks, 0)),
            );
        }
        header_col = header_col.push(greeting_row);

        let cw = self.center_width();
        let cols: usize = if cw >= 980.0 {
            4
        } else if cw >= 620.0 {
            3
        } else {
            2
        };
        let tile_items: Vec<&BrowseItem> = self.home_quick_tiles.iter().take(cols * 2).collect();
        let mut tiles = column![].spacing(12);
        for chunk in tile_items.chunks(cols) {
            let mut r = row![].spacing(12);
            for it in chunk {
                r = r.push(self.quick_tile(it));
            }
            for _ in chunk.len()..cols {
                r = r.push(horizontal_space().width(Length::Fill));
            }
            tiles = tiles.push(r);
        }

        header_col = header_col.push(tiles);

        let header = container(header_col)
            .padding([20, 24])
            .width(Length::Fill)
            .style(hero_banner_style(p));

        let mut content = column![header].spacing(6);
        for (idx, sec) in self.home_sections.iter().enumerate() {
            content = content.push(self.browse_shelf_section(
                format!("home:{idx}:{}", sec.title),
                &sec.title,
                &sec.items,
                idx == 0,
            ));
        }

        if self.home_continuation.is_some() {
            let label = if self.home_loading_more {
                "Loading more shelves..."
            } else {
                "Load more"
            };
            content = content.push(
                container(
                    button(text(label).size(13))
                        .padding([9, 20])
                        .style(top_pill_button_style(p, false))
                        .on_press(Message::LoadMoreHome),
                )
                .padding([8, 24]),
            );
        }

        self.vscroll(content.push(vertical_space().height(24)))
    }

    fn view_explore(&self) -> Element<'_, Message> {
        let p = self.palette;
        let muted = color_from_rgb(p.text_muted);

        if self.explore_loading {
            return self.view_discover_skeleton();
        }

        if let Some(err) = &self.explore_error {
            return container(
                column![
                    text("Couldn't load Explore")
                        .size(22)
                        .font(bold())
                        .color(color_from_rgb(p.danger)),
                    text(ellipsize(err, 140)).size(14).color(muted),
                    button(text("Retry").size(14))
                        .padding([10, 22])
                        .style(top_pill_button_style(p, true))
                        .on_press(Message::LoadExplore),
                ]
                .spacing(12),
            )
            .padding(32)
            .into();
        }

        let mut content = column![
            container(
                row![
                    text("Explore").size(30).font(bold()),
                    horizontal_space(),
                    button(text("Refresh").size(13))
                        .padding([8, 16])
                        .style(top_pill_button_style(p, false))
                        .on_press(Message::LoadExplore),
                ]
                .align_y(Alignment::Center),
            )
            .padding([24, 24])
            .width(Length::Fill)
            .style(hero_banner_style(p)),
        ]
        .spacing(16);

        if let Some(explore) = &self.explore_page {
            if !explore.new_release_albums.is_empty() {
                content = content.push(self.browse_shelf_section(
                    "explore:new_releases".to_string(),
                    "New Releases",
                    &explore.new_release_albums,
                    false,
                ));
            }

            if !explore.mood_and_genres.is_empty() && self.mood_groups.is_empty() {
                content = content.push(
                    container(self.mood_buttons_grid("Moods & Genres", &explore.mood_and_genres))
                        .padding([6, 24]),
                );
            }
        }

        for grp in &self.mood_groups {
            let heading = if grp.title.is_empty() {
                "Moods & Genres"
            } else {
                &grp.title
            };
            content = content.push(
                container(self.mood_buttons_grid(heading, &grp.items)).padding([6, 24]),
            );
        }

        self.vscroll(content.push(vertical_space().height(24)))
    }

    fn mood_buttons_grid<'a>(
        &'a self,
        title: &'a str,
        items: &'a [echo_youtube::MoodGenreButton],
    ) -> Element<'a, Message> {
        let p = self.palette;
        let mut grid = column![text(title).size(20).font(bold())].spacing(12);
        for chunk in items.chunks(4) {
            let mut r = row![].spacing(10);
            for btn in chunk {
                r = r.push(
                    button(text(&btn.title).size(14).font(bold()))
                        .padding([12, 16])
                        .width(Length::Fill)
                        .style(tile_button_style(p))
                        .on_press(Message::OpenCategory {
                            title: btn.title.clone(),
                            browse_id: btn.browse_id.clone(),
                            params: btn.params.clone(),
                        }),
                );
            }
            for _ in chunk.len()..4 {
                r = r.push(horizontal_space().width(Length::Fill));
            }
            grid = grid.push(r);
        }
        grid.into()
    }

    fn view_detail_page<'a>(&'a self, page: &'a DetailPage) -> Element<'a, Message> {
        let p = self.palette;
        let muted = color_from_rgb(p.text_muted);

        match page {
            DetailPage::Playlist(pl) => {
                let cover = pl
                    .thumbnail_url
                    .as_deref()
                    .or_else(|| pl.tracks.first().and_then(|t| t.thumbnail_url.as_deref()));
                let sub = format!(
                    "Playlist • {}{} songs",
                    pl.author
                        .as_deref()
                        .map(|a| format!("{a} • "))
                        .unwrap_or_default(),
                    pl.tracks.len()
                );
                let footer = pl.continuation.as_ref().map(|_| {
                    let lbl = if self.playlist_loading_more {
                        "Loading more tracks..."
                    } else {
                        "Load more tracks"
                    };
                    container(
                        button(text(lbl).size(13))
                            .padding([8, 20])
                            .style(top_pill_button_style(p, false))
                            .on_press(Message::LoadMorePlaylist),
                    )
                    .padding([14, 24])
                    .into()
                });
                self.view_hero_and_table(&pl.title, sub, cover, &pl.tracks, footer)
            }
            DetailPage::Album(alb) => {
                let mut artist_row = row![].spacing(8).align_y(Alignment::Center);
                for art in &alb.artists {
                    if let Some(id) = &art.id {
                        artist_row = artist_row.push(
                            button(text(&art.name).size(14).font(bold()))
                                .padding([5, 12])
                                .style(top_pill_button_style(p, false))
                                .on_press(Message::OpenArtist {
                                    browse_id: id.clone(),
                                    name: art.name.clone(),
                                }),
                        );
                    } else {
                        artist_row = artist_row.push(text(&art.name).size(14).color(muted));
                    }
                }
                let meta = [
                    alb.subtitle.clone().unwrap_or_else(|| "Album".to_string()),
                    format!("{} tracks", alb.tracks.len()),
                ]
                .into_iter()
                .filter(|s| !s.is_empty())
                .collect::<Vec<_>>()
                .join(" • ");

                let mut col = column![
                    container(
                        row![
                            container(self.artwork_widget(alb.thumbnail_url.as_deref(), 180.0, false))
                                .style(artwork_frame_style(12.0, 0.55)),
                            column![
                                text(
                                    alb.year
                                        .map(|y| format!("ALBUM • {y}"))
                                        .unwrap_or_else(|| "ALBUM".to_string())
                                )
                                .size(11)
                                .font(bold())
                                .color(Color::from_rgba(1.0, 1.0, 1.0, 0.80)),
                                text(&alb.title).size(40).font(bold()),
                                artist_row,
                                text(meta).size(13).color(muted),
                            ]
                            .spacing(8)
                            .width(Length::Fill),
                        ]
                        .spacing(24)
                        .align_y(Alignment::End),
                    )
                    .padding([26, 24])
                    .width(Length::Fill)
                    .style(hero_banner_style(p)),
                ]
                .spacing(0);

                if !alb.tracks.is_empty() {
                    let toolbar = container(
                        row![
                            button(
                                container(icons::icon(icons::PLAY, 24.0, Color::BLACK))
                                    .width(56)
                                    .height(56)
                                    .align_x(iced::alignment::Horizontal::Center)
                                    .align_y(iced::alignment::Vertical::Center),
                            )
                            .padding(0)
                            .style(glowing_play_button_style(p))
                            .on_press(Message::PlayTracks(alb.tracks.clone(), 0)),
                            button(icons::icon(
                                icons::SHUFFLE,
                                24.0,
                                if self.queue.shuffle {
                                    color_from_rgb(p.primary)
                                } else {
                                    muted
                                },
                            ))
                            .padding(8)
                            .style(icon_button_style(p, false))
                            .on_press(Message::ToggleShuffle),
                            button(
                                row![
                                    icons::icon(icons::DOWNLOAD, 15.0, Color::WHITE),
                                    text("Download").size(13),
                                ]
                                .spacing(6)
                                .align_y(Alignment::Center),
                            )
                            .padding([8, 16])
                            .style(top_pill_button_style(p, false))
                            .on_press(Message::DownloadTracks(alb.tracks.clone())),
                        ]
                        .spacing(16)
                        .align_y(Alignment::Center),
                    )
                    .padding([20, 24]);

                    let rows = alb.tracks.iter().enumerate().fold(
                        column![].spacing(0),
                        |c, (i, t)| {
                            c.push(self.table_track_row(
                                i + 1,
                                t,
                                Some((alb.tracks.clone(), i)),
                            ))
                        },
                    );
                    col = col
                        .push(toolbar)
                        .push(self.table_header())
                        .push(container(rows).padding([0, 16]));
                }

                if !alb.other_versions.is_empty() {
                    col = col.push(self.browse_shelf_section(
                        format!("album:{}:other_versions", alb.title),
                        "Other Versions",
                        &alb.other_versions,
                        false,
                    ));
                }

                self.vscroll(col.push(vertical_space().height(24)))
            }
            DetailPage::Artist { page, subscribed } => {
                let mut actions = row![].spacing(12).align_y(Alignment::Center);
                let top_tracks: Vec<Track> = page
                    .sections
                    .iter()
                    .flat_map(|s| s.items.iter())
                    .filter_map(|it| match it {
                        BrowseItem::Track(t) => Some(t.clone()),
                        _ => None,
                    })
                    .collect();
                if !top_tracks.is_empty() {
                    actions = actions.push(
                        button(
                            container(icons::icon(icons::PLAY, 22.0, Color::BLACK))
                                .width(50)
                                .height(50)
                                .align_x(iced::alignment::Horizontal::Center)
                                .align_y(iced::alignment::Vertical::Center),
                        )
                        .padding(0)
                        .style(glowing_play_button_style(p))
                        .on_press(Message::PlayTracks(top_tracks, 0)),
                    );
                }
                if let Some(pid) = &page.shuffle_playlist_id {
                    actions = actions.push(
                        button(text("Shuffle").size(13))
                            .padding([8, 16])
                            .style(top_pill_button_style(p, false))
                            .on_press(Message::OpenPlaylist {
                                id: pid.clone(),
                                title: format!("{} • Shuffle", page.name),
                            }),
                    );
                }
                if let Some(pid) = &page.radio_playlist_id {
                    actions = actions.push(
                        button(text("Radio").size(13))
                            .padding([8, 16])
                            .style(top_pill_button_style(p, false))
                            .on_press(Message::OpenPlaylist {
                                id: pid.clone(),
                                title: format!("{} Radio", page.name),
                            }),
                    );
                }
                if let (true, Some(cid)) = (self.yt.has_cookie(), &page.channel_id) {
                    actions = actions.push(
                        button(text(if *subscribed { "Following" } else { "Follow" }).size(13).font(bold()))
                            .padding([7, 18])
                            .style(outlined_pill_button_style(p, *subscribed))
                            .on_press(Message::SubscribeArtist {
                                channel_id: cid.clone(),
                                subscribe: !*subscribed,
                            }),
                    );
                }

                let stats_line = [
                    page.monthly_listeners.clone(),
                    page.subscriber_count.clone(),
                ]
                .into_iter()
                .flatten()
                .collect::<Vec<_>>()
                .join(" • ");

                let mut col = column![
                    container(
                        row![
                            container(self.artwork_widget(page.thumbnail_url.as_deref(), 164.0, true))
                                .style(artwork_frame_style(82.0, 0.50)),
                            column![
                                row![
                                    icons::icon(
                                        icons::CHECK_CIRCLE,
                                        16.0,
                                        Color::from_rgb(0.24, 0.62, 0.98)
                                    ),
                                    text("Verified Artist").size(13).font(bold()),
                                ]
                                .spacing(6)
                                .align_y(Alignment::Center),
                                text(&page.name).size(52).font(bold()),
                                if !stats_line.is_empty() {
                                    text(stats_line).size(14).color(Color::WHITE)
                                } else {
                                    text("").size(1)
                                },
                            ]
                            .spacing(6)
                            .width(Length::Fill),
                        ]
                        .spacing(22)
                        .align_y(Alignment::Center),
                    )
                    .padding([24, 24])
                    .width(Length::Fill)
                    .style(hero_banner_style(p)),
                    container(actions).padding([16, 24]),
                ]
                .spacing(6);

                for sec in &page.sections {
                    let all_tracks: Vec<Track> = sec
                        .items
                        .iter()
                        .filter_map(|it| match it {
                            BrowseItem::Track(t) => Some(t.clone()),
                            _ => None,
                        })
                        .collect();
                    let header_row = {
                        let mut r = row![text(&sec.title).size(20).font(bold())].align_y(Alignment::Center);
                        r = r.push(horizontal_space());
                        if let Some(bid) = &sec.browse_id {
                            r = r.push(
                                button(text("See all").size(12))
                                    .padding([6, 14])
                                    .style(top_pill_button_style(p, false))
                                    .on_press(Message::OpenArtistItems {
                                        browse_id: bid.clone(),
                                        params: sec.params.clone(),
                                    }),
                            );
                        }
                        r
                    };

                    if all_tracks.len() == sec.items.len() && !all_tracks.is_empty() {
                        let mut track_col = column![header_row].spacing(6);
                        let mut idx = 0;
                        for item in &sec.items {
                            if let BrowseItem::Track(t) = item {
                                track_col = track_col.push(self.table_track_row(
                                    idx + 1,
                                    t,
                                    Some((all_tracks.clone(), idx)),
                                ));
                                idx += 1;
                            }
                        }
                        col = col.push(container(track_col).padding([8, 24]));
                    } else {
                        let items = &sec.items;
                        let shelf = responsive(move |size| {
                            let n = (((size.width + 16.0) / (172.0 + 16.0)).floor() as usize)
                                .clamp(1, 12);
                            items
                                .iter()
                                .take(n)
                                .fold(row![].spacing(16), |r, it| r.push(self.browse_card(it)))
                                .into()
                        });
                        col = col.push(
                            container(
                                column![
                                    header_row,
                                    container(shelf)
                                        .width(Length::Fill)
                                        .height(Length::Fixed(232.0)),
                                ]
                                .spacing(12),
                            )
                            .padding([8, 24]),
                        );
                    }
                }

                if let Some(desc) = &page.description {
                    col = col.push(
                        container(
                            column![
                                text("About").size(18).font(bold()),
                                text(desc).size(13).color(muted),
                            ]
                            .spacing(6),
                        )
                        .padding([8, 24]),
                    );
                }

                self.vscroll(col.push(vertical_space().height(24)))
            }
            DetailPage::ArtistItems { title, items, .. } => {
                let mut list = column![
                    container(text(title).size(26).font(bold()))
                        .padding([22, 24])
                        .width(Length::Fill)
                        .style(hero_banner_style(p)),
                ]
                .spacing(6);
                for (i, it) in items.iter().enumerate() {
                    list = list.push(container(self.browse_item_row(i + 1, it)).padding([0, 16]));
                }
                self.vscroll(list.push(vertical_space().height(24)))
            }
            DetailPage::Category {
                title, sections, ..
            } => {
                let mut col = column![
                    container(text(title).size(28).font(bold()))
                        .padding([22, 24])
                        .width(Length::Fill)
                        .style(hero_banner_style(p)),
                ]
                .spacing(12);
                for sec in sections {
                    let items = &sec.items;
                    let shelf = responsive(move |size| {
                        let n =
                            (((size.width + 16.0) / (172.0 + 16.0)).floor() as usize).clamp(1, 12);
                        items
                            .iter()
                            .take(n)
                            .fold(row![].spacing(16), |r, it| r.push(self.browse_card(it)))
                            .into()
                    });
                    col = col.push(
                        container(
                            column![
                                text(&sec.title).size(20).font(bold()),
                                container(shelf)
                                    .width(Length::Fill)
                                    .height(Length::Fixed(232.0)),
                            ]
                            .spacing(12),
                        )
                        .padding([8, 24]),
                    );
                }
                self.vscroll(col.push(vertical_space().height(24)))
            }
        }
    }

    fn vscroll<'a>(&self, content: impl Into<Element<'a, Message>>) -> Element<'a, Message> {
        container(
            scrollable(content)
                .direction(scrollable::Direction::Vertical(
                    scrollable::Scrollbar::new()
                        .width(6)
                        .scroller_width(4)
                        .margin(2),
                ))
                .style(scroll_style(self.palette)),
        )
        .width(Length::Fill)
        .height(Length::Fill)
        .clip(true)
        .into()
    }



    fn view_library(&self) -> Element<'_, Message> {
        match self.library_tab {
            LibraryTab::Liked => {
                let cover = self.liked_tracks.first().and_then(|t| t.thumbnail_url.as_deref());
                self.view_hero_and_table(
                    "Liked Songs",
                    format!("Playlist • {} songs", self.liked_tracks.len()),
                    cover,
                    &self.liked_tracks,
                    None,
                )
            }
            LibraryTab::History => {
                let cover = self.history_tracks.first().and_then(|t| t.thumbnail_url.as_deref());
                self.view_hero_and_table(
                    "Recently Played",
                    format!("Listening Log • {} tracks", self.history_tracks.len()),
                    cover,
                    &self.history_tracks,
                    None,
                )
            }
            LibraryTab::Downloads => {
                let tracks: Vec<Track> =
                    self.downloaded_tracks.iter().map(|(t, _, _)| t.clone()).collect();
                let total_mb = self
                    .downloaded_tracks
                    .iter()
                    .map(|(_, _, b)| *b)
                    .sum::<u64>()
                    as f64
                    / (1024.0 * 1024.0);
                let mut col = column![
                    container(
                        row![
                            column![
                                text("Downloads").size(28).font(bold()),
                                text(format!(
                                    "{} offline tracks • {:.1} MB",
                                    tracks.len(),
                                    total_mb
                                ))
                                .size(13)
                                .color(color_from_rgb(self.palette.text_muted)),
                            ]
                            .spacing(4),
                            horizontal_space(),
                            button(text("Rescan Folders").size(13))
                                .padding([8, 16])
                                .style(top_pill_button_style(self.palette, false))
                                .on_press(Message::ScanDownloadFolders),
                            button(
                                text("Clear All")
                                    .size(13)
                                    .color(color_from_rgb(self.palette.danger))
                            )
                            .padding([8, 16])
                            .style(top_pill_button_style(self.palette, false))
                            .on_press(Message::ClearDownloads),
                        ]
                        .spacing(10)
                        .align_y(Alignment::Center),
                    )
                    .padding([22, 24])
                    .width(Length::Fill)
                    .style(hero_banner_style(self.palette)),
                ]
                .spacing(12);

                if !tracks.is_empty() {
                    col = col.push(
                        container(
                            button(
                                row![
                                    icons::icon(icons::PLAY, 16.0, Color::BLACK),
                                    text("Play All Offline").size(13).font(bold()),
                                ]
                                .spacing(8)
                                .align_y(Alignment::Center),
                            )
                            .padding([10, 20])
                            .style(glowing_play_button_style(self.palette))
                            .on_press(Message::PlayTracks(tracks.clone(), 0)),
                        )
                        .padding([6, 24]),
                    );
                    col = col.push(self.table_header());
                    let list = self.downloaded_tracks.iter().enumerate().fold(
                        column![].spacing(0),
                        |c, (i, (t, _, _))| {
                            c.push(self.table_track_row(i + 1, t, Some((tracks.clone(), i))))
                        },
                    );
                    col = col.push(container(list).padding([0, 16]));
                } else {
                    col = col.push(
                        container(
                            text("No downloaded songs yet. Click Download on any song or scan external [youtube_id] folders.")
                                .size(14)
                                .color(color_from_rgb(self.palette.text_muted)),
                        )
                        .padding([12, 24]),
                    );
                }
                self.vscroll(col.push(vertical_space().height(24)))
            }
            LibraryTab::Local => {
                let muted = color_from_rgb(self.palette.text_muted);
                let mut col = column![
                    container(
                        row![
                            column![
                                text("Local Files").size(28).font(bold()),
                                text(format!(
                                    "{} tracks from {} folder(s)",
                                    self.local_tracks.len(),
                                    self.settings.local_media_folders.len()
                                ))
                                .size(13)
                                .color(muted),
                            ]
                            .spacing(4),
                            horizontal_space(),
                            button(text(if self.local_folder_tree {
                                "Flat View"
                            } else {
                                "Folder Tree"
                            }).size(13))
                            .padding([8, 16])
                            .style(top_pill_button_style(self.palette, self.local_folder_tree))
                            .on_press(Message::ToggleLocalFolderTree),
                            button(text(if self.local_scanning {
                                "Scanning..."
                            } else {
                                "Rescan Folders"
                            }).size(13))
                            .padding([8, 16])
                            .style(top_pill_button_style(self.palette, true))
                            .on_press(Message::ScanLocalMedia),
                        ]
                        .spacing(10)
                        .align_y(Alignment::Center),
                    )
                    .padding([22, 24])
                    .width(Length::Fill)
                    .style(hero_banner_style(self.palette)),
                ]
                .spacing(12);

                if self.local_tracks.is_empty() {
                    col = col.push(
                        container(
                            text("No local tracks indexed yet. Configure folders in Settings and click Rescan Folders.")
                                .size(14)
                                .color(muted),
                        )
                        .padding([12, 24]),
                    );
                } else if self.local_folder_tree {
                    let mut grouped: std::collections::BTreeMap<String, Vec<usize>> =
                        std::collections::BTreeMap::new();
                    for (idx, t) in self.local_tracks.iter().enumerate() {
                        let folder = t
                            .local_path
                            .as_deref()
                            .and_then(|p| std::path::Path::new(p).parent())
                            .map(|p| p.to_string_lossy().into_owned())
                            .unwrap_or_else(|| "Unknown Folder".to_string());
                        grouped.entry(folder).or_default().push(idx);
                    }
                    for (folder, indices) in grouped {
                        let folder_tracks: Vec<Track> =
                            indices.iter().map(|&idx| self.local_tracks[idx].clone()).collect();
                        let mut sec = column![
                            row![
                                text(folder).size(15).font(bold()),
                                horizontal_space(),
                                button(text("Play Folder").size(12))
                                    .padding([5, 14])
                                    .style(top_pill_button_style(self.palette, false))
                                    .on_press(Message::PlayTracks(folder_tracks.clone(), 0)),
                            ]
                            .align_y(Alignment::Center),
                        ]
                        .spacing(6);
                        for (i, &track_idx) in indices.iter().enumerate() {
                            sec = sec.push(self.table_track_row(
                                i + 1,
                                &self.local_tracks[track_idx],
                                Some((folder_tracks.clone(), i)),
                            ));
                        }
                        col = col.push(container(sec).padding([6, 20]));
                    }
                } else {
                    col = col.push(
                        container(
                            button(
                                row![
                                    icons::icon(icons::PLAY, 16.0, Color::BLACK),
                                    text("Play All Local").size(13).font(bold()),
                                ]
                                .spacing(8)
                                .align_y(Alignment::Center),
                            )
                            .padding([10, 20])
                            .style(glowing_play_button_style(self.palette))
                            .on_press(Message::PlayTracks(self.local_tracks.clone(), 0)),
                        )
                        .padding([6, 24]),
                    );
                    col = col.push(self.table_header());
                    let list = self.local_tracks.iter().enumerate().fold(
                        column![].spacing(0),
                        |c, (i, t)| {
                            c.push(self.table_track_row(
                                i + 1,
                                t,
                                Some((self.local_tracks.clone(), i)),
                            ))
                        },
                    );
                    col = col.push(container(list).padding([0, 16]));
                }
                self.vscroll(col.push(vertical_space().height(24)))
            }
            LibraryTab::Playlist(idx) => {
                let name = self
                    .playlists
                    .get(idx)
                    .map(|p| p.name.as_str())
                    .unwrap_or("Playlist");
                let cover = self
                    .selected_playlist_tracks
                    .first()
                    .and_then(|t| t.thumbnail_url.as_deref());
                self.view_hero_and_table(
                    name,
                    format!("Playlist • {} songs", self.selected_playlist_tracks.len()),
                    cover,
                    &self.selected_playlist_tracks,
                    None,
                )
            }
        }
    }

    fn view_search(&self) -> Element<'_, Message> {
        let p = self.palette;
        let muted = color_from_rgb(p.text_muted);

        let filters = SearchFilter::ALL.iter().fold(row![].spacing(8), |r, &f| {
            let active = self.search_filter == f;
            r.push(
                button(text(f.label()).size(13).font(bold()))
                    .padding([6, 14])
                    .style(top_pill_button_style(p, active))
                    .on_press(Message::SearchFilterSelected(f)),
            )
        });

        let subtitle = if self.search_query.trim().is_empty() {
            "Search songs, albums, artists, and playlists".to_string()
        } else {
            format!("Results for \"{}\"", self.search_query.trim())
        };

        let header = container(
            column![
                row![
                    column![
                        text("Search").size(30).font(bold()),
                        text(subtitle).size(13).color(muted),
                    ]
                    .spacing(4),
                    horizontal_space(),
                    filters,
                ]
                .spacing(12)
                .align_y(Alignment::Center),
            ]
            .spacing(12),
        )
        .padding([22, 24])
        .width(Length::Fill)
        .style(hero_banner_style(p));

        let mut col = column![header].spacing(12);

        if !self.search_suggestions.is_empty() {
            let sug_row = self.search_suggestions.iter().take(6).fold(row![].spacing(8), |r, s| {
                r.push(
                    button(text(s).size(12))
                        .padding([5, 12])
                        .style(top_pill_button_style(p, false))
                        .on_press(Message::ApplySuggestion(s.clone())),
                )
            });
            col = col.push(container(sug_row).padding([0, 24]));
        }

        if self.search_loading {
            col = col.push(
                container(
                    column![
                        text("Searching YouTube Music...").size(18).font(bold()),
                        text("Looking up songs, albums, artists, and playlists.")
                            .size(13)
                            .color(muted),
                    ]
                    .spacing(6),
                )
                .padding([24, 24]),
            );
        } else if let Some(err) = &self.search_error {
            col = col.push(
                container(text(err).size(14).color(color_from_rgb(p.danger))).padding([12, 24]),
            );
        } else if self.search_results.is_empty() {
            col = col.push(
                container(
                    container(
                        column![
                            icons::icon(icons::SEARCH, 32.0, muted),
                            text("Find what you want to hear").size(18).font(bold()),
                            text("Type a song, album, artist, or playlist in the top search bar (Ctrl+K).")
                                .size(13)
                                .color(muted),
                        ]
                        .spacing(8)
                        .align_x(Alignment::Center),
                    )
                    .width(Length::Fill)
                    .padding(36)
                    .align_x(iced::alignment::Horizontal::Center)
                    .style(elevated_card_style(p)),
                )
                .padding([12, 24]),
            );
        } else {
            let mut list = self.search_results.iter().enumerate().fold(
                column![].spacing(4),
                |c, (i, item)| c.push(self.browse_item_row(i + 1, item)),
            );
            if self.search_continuation.is_some() {
                let lbl = if self.search_loading_more {
                    "Loading more results..."
                } else {
                    "Load more results"
                };
                list = list.push(
                    container(
                        button(text(lbl).size(13))
                            .padding([8, 18])
                            .style(top_pill_button_style(p, false))
                            .on_press(Message::LoadMoreSearch),
                    )
                    .padding([12, 0]),
                );
            }
            col = col.push(container(list).padding([0, 16]));
        }

        self.vscroll(col.push(vertical_space().height(24)))
    }

    fn view_recognize(&self) -> Element<'_, Message> {
        let p = self.palette;
        let muted = color_from_rgb(p.text_muted);
        let accent = color_from_rgb(p.primary);

        let source_picker = pick_list(
            &RecognitionSource::ALL[..],
            Some(self.settings.recognition_source),
            Message::RecognitionSourceSelected,
        )
        .padding([8, 14])
        .style(pick_list_style(p))
        .menu_style(pick_list_menu_style(p));

        let radar_circle = container(icons::icon(
            icons::MIC,
            34.0,
            if self.recognizing { Color::BLACK } else { Color::BLACK },
        ))
        .width(84)
        .height(84)
        .align_x(iced::alignment::Horizontal::Center)
        .align_y(iced::alignment::Vertical::Center);

        let listen_orb = if self.recognizing {
            button(radar_circle)
                .padding(0)
                .style(glowing_play_button_style(p))
        } else {
            button(radar_circle)
                .padding(0)
                .style(glowing_play_button_style(p))
                .on_press(Message::ListenAndRecognize)
        };

        let header = container(
            row![
                column![
                    text("Song Recognition").size(30).font(bold()),
                    text("Identify music playing through your speakers or microphone via Shazam")
                        .size(13)
                        .color(muted),
                ]
                .spacing(4),
                horizontal_space(),
                row![text("Input").size(12).color(muted), source_picker]
                    .spacing(10)
                    .align_y(Alignment::Center),
            ]
            .spacing(12)
            .align_y(Alignment::Center),
        )
        .padding([22, 24])
        .width(Length::Fill)
        .style(hero_banner_style(p));

        let mut radar_col = column![
            listen_orb,
            text(if self.recognizing {
                "Listening for audio (8s)..."
            } else {
                "Tap to Identify"
            })
            .size(18)
            .font(bold()),
            text(if let Some(status) = &self.recognition_status {
                status.as_str()
            } else {
                "Captures 16 kHz mono audio fingerprint and matches against Shazam"
            })
            .size(13)
            .color(if self.recognition_status.is_some() {
                accent
            } else {
                muted
            }),
        ]
        .spacing(10)
        .align_x(Alignment::Center);

        if let Some(rec) = &self.last_recognised {
            radar_col = radar_col.push(vertical_space().height(6));
            radar_col = radar_col.push(
                container(
                    row![
                        self.artwork_widget(rec.artwork_url.as_deref(), 68.0, false),
                        column![
                            text("MATCH FOUND")
                                .size(11)
                                .font(bold())
                                .color(accent),
                            text(&rec.title).size(18).font(bold()),
                            text(rec.artist.as_deref().unwrap_or("Unknown Artist"))
                                .size(14)
                                .color(muted),
                            if let Some(isrc) = &rec.isrc {
                                text(format!("ISRC: {isrc}")).size(11).color(muted)
                            } else {
                                text("").size(1)
                            }
                        ]
                        .spacing(3),
                        horizontal_space(),
                        button(
                            row![
                                icons::icon(icons::PLAY, 14.0, Color::BLACK),
                                text("Play on YouTube Music").size(13).font(bold()),
                            ]
                            .spacing(6)
                            .align_y(Alignment::Center),
                        )
                        .padding([10, 18])
                        .style(glowing_play_button_style(p))
                        .on_press(Message::PlayRecognised(rec.clone())),
                    ]
                    .spacing(14)
                    .align_y(Alignment::Center),
                )
                .width(Length::Fill)
                .padding(14)
                .style(elevated_card_style(p)),
            );
        }

        let radar_card = container(
            container(radar_col)
                .width(Length::Fill)
                .padding([26, 24])
                .align_x(iced::alignment::Horizontal::Center)
                .style(bento_card_style(p)),
        )
        .padding([4, 24]);

        let mut col = column![header, radar_card].spacing(16);

        col = col.push(
            container(text("Recognition History").size(19).font(bold())).padding([4, 24]),
        );
        if self.recognition_history.is_empty() {
            col = col.push(
                container(
                    text("No recognised songs yet. Tap the microphone above to identify a track.")
                        .size(13)
                        .color(muted),
                )
                .padding([0, 24]),
            );
        } else {
            let hist = self.recognition_history.iter().fold(column![].spacing(6), |c, entry| {
                let dt = chrono::DateTime::from_timestamp_millis(entry.recognized_at_ms)
                    .map(|d| d.format("%Y-%m-%d %H:%M").to_string())
                    .unwrap_or_default();
                c.push(
                    container(
                        row![
                            container(icons::icon(icons::MIC, 16.0, accent))
                                .width(36)
                                .height(36)
                                .align_x(iced::alignment::Horizontal::Center)
                                .align_y(iced::alignment::Vertical::Center)
                                .style(local_badge_style()),
                            column![
                                text(&entry.title).size(14).font(bold()),
                                text(format!(
                                    "{} • {}",
                                    entry.artist.as_deref().unwrap_or("Unknown"),
                                    dt
                                ))
                                .size(12)
                                .color(muted),
                            ]
                            .spacing(2),
                            horizontal_space(),
                            button(text("Open").size(12).font(bold()))
                                .padding([6, 14])
                                .style(top_pill_button_style(p, false))
                                .on_press(Message::OpenHistoryRecognition(entry.clone())),
                        ]
                        .spacing(12)
                        .align_y(Alignment::Center),
                    )
                    .padding([10, 14])
                    .style(elevated_card_style(p)),
                )
            });
            col = col.push(container(hist).padding([0, 24]));
        }

        self.vscroll(col.push(vertical_space().height(24)))
    }

    fn view_stats(&self) -> Element<'_, Message> {
        let p = self.palette;
        let muted = color_from_rgb(p.text_muted);
        let accent = color_from_rgb(p.primary);
        let s = &self.listening_summary;

        let hero = self.stats_hero_section();
        let kpi_row = self.stats_kpi_strip();

        let mut col = column![hero, kpi_row].spacing(18);

        if s.total_plays == 0 {
            let empty_card = container(
                column![
                    container(icons::icon(icons::STATS, 26.0, accent))
                        .width(52)
                        .height(52)
                        .align_x(iced::alignment::Horizontal::Center)
                        .align_y(iced::alignment::Vertical::Center)
                        .style(stats_rank_badge_style(p, true)),
                    text(match self.stats_range {
                        StatsRange::AllTime => "No listening history yet",
                        StatsRange::Days30 => "No plays in the last 30 days",
                        StatsRange::Days7 => "No plays in the last 7 days",
                    })
                    .size(18)
                    .font(bold()),
                    text("Start playing music to unlock your soundscape charts, top artists, and daily rhythm.")
                        .size(13)
                        .color(muted),
                ]
                .spacing(10)
                .align_x(Alignment::Center),
            )
            .width(Length::Fill)
            .padding(36)
            .align_x(iced::alignment::Horizontal::Center)
            .style(bento_card_style(p));
            col = col.push(container(empty_card).padding([4, 24]));
            return self.vscroll(col.push(vertical_space().height(24)));
        }

        col = col.push(self.stats_analytics_row());

        if !s.top_artists.is_empty() {
            col = col.push(self.stats_top_artists_section());
        }
        if !s.top_albums.is_empty() {
            col = col.push(self.stats_top_albums_section());
        }
        col = col.push(self.stats_top_tracks_section());

        self.vscroll(col.push(vertical_space().height(24)))
    }

    fn stats_hero_section(&self) -> Element<'_, Message> {
        let p = self.palette;
        let muted = color_from_rgb(p.text_muted);
        let accent = color_from_rgb(p.primary);
        let s = &self.listening_summary;

        let persona = match s.peak_hour {
            Some(21..=23 | 0..=4) => "Night Owl Sessions",
            Some(5..=11) => "Morning Momentum",
            Some(12..=16) => "Midday Flow",
            Some(17..=20) => "Evening Drift",
            _ => "Your Soundscape",
        };

        let range_pills = StatsRange::ALL
            .into_iter()
            .fold(row![].spacing(6), |r, range| {
                let active = self.stats_range == range;
                r.push(
                    button(
                        text(range.label())
                            .size(12)
                            .font(semibold())
                            .color(if active { Color::BLACK } else { Color::WHITE }),
                    )
                    .padding([6, 14])
                    .style(top_pill_button_style(p, active))
                    .on_press(Message::SelectStatsRange(range)),
                )
            });

        let top_bar = row![
            container(
                row![
                    icons::icon(icons::EQ_BARS, 13.0, accent),
                    text("ECHO PULSE RECAP").size(11).font(bold()).color(accent),
                ]
                .spacing(7)
                .align_y(Alignment::Center),
            )
            .padding([5, 11])
            .style(stats_rank_badge_style(p, true)),
            horizontal_space(),
            range_pills,
        ]
        .align_y(Alignment::Center);

        let top_tracks_vec: Vec<Track> = s.top_tracks.iter().take(10).map(|st| st.track.clone()).collect();
        let subtitle = if s.total_plays > 0 {
            format!(
                "{} plays across {} tracks and {} artists",
                s.total_plays, s.unique_tracks, s.unique_artists
            )
        } else {
            "Your listening milestones, peak hours, and top rotations".to_string()
        };

        let mut left_col = column![
            column![
                text(persona).size(30).font(bold()),
                text(subtitle).size(13).color(muted),
            ]
            .spacing(4),
        ]
        .spacing(16)
        .width(Length::FillPortion(3));

        if !top_tracks_vec.is_empty() {
            let mut shuffled = top_tracks_vec.clone();
            shuffled.reverse();
            left_col = left_col.push(
                row![
                    button(
                        row![
                            icons::icon(icons::PLAY, 13.0, Color::BLACK),
                            text("Play Top Mix").size(13).font(bold()).color(Color::BLACK),
                        ]
                        .spacing(8)
                        .align_y(Alignment::Center),
                    )
                    .padding([9, 18])
                    .style(glowing_play_button_style(p))
                    .on_press(Message::PlayTracks(top_tracks_vec, 0)),
                    button(
                        row![
                            icons::icon(icons::SHUFFLE, 13.0, Color::WHITE),
                            text("Shuffle Top 10").size(12).font(semibold()),
                        ]
                        .spacing(7)
                        .align_y(Alignment::Center),
                    )
                    .padding([8, 15])
                    .style(outlined_pill_button_style(p, false))
                    .on_press(Message::PlayTracks(shuffled, 0)),
                ]
                .spacing(10)
                .align_y(Alignment::Center),
            );
        }

        let mut pulse_cards = row![].spacing(12);
        if let Some(top_song) = s.top_tracks.first() {
            let mins = (top_song.total_played_secs / 60).max(1);
            let track_card = button(
                column![
                    row![
                        container(self.artwork_widget(
                            top_song.track.thumbnail_url.as_deref(),
                            48.0,
                            false,
                        ))
                        .style(artwork_frame_style(10.0, 0.35)),
                        horizontal_space(),
                        container(
                            text("#1 TRACK")
                                .size(10)
                                .font(bold())
                                .color(accent),
                        )
                        .padding([4, 9])
                        .style(stats_rank_badge_style(p, true)),
                    ]
                    .align_y(Alignment::Center),
                    column![
                        container(
                            text(&top_song.track.title)
                                .size(14)
                                .font(bold())
                                .wrapping(Wrapping::None),
                        )
                        .width(Length::Fill)
                        .clip(true),
                        container(
                            text(format!(
                                "{} • {} plays • {}m",
                                top_song.track.artist_display(),
                                top_song.play_count,
                                mins
                            ))
                            .size(11.5)
                            .color(muted)
                            .wrapping(Wrapping::None),
                        )
                        .width(Length::Fill)
                        .clip(true),
                    ]
                    .spacing(3),
                ]
                .spacing(10),
            )
            .width(Length::Fill)
            .padding(14)
            .style(stats_bento_button_style(p, true))
            .on_press(Message::PlayTrack(top_song.track.clone()));
            pulse_cards = pulse_cards.push(track_card);
        }

        if let Some(top_artist) = s.top_artists.first() {
            let artist_action = match &top_artist.id {
                Some(id) if !id.trim().is_empty() => Message::OpenArtist {
                    browse_id: id.clone(),
                    name: top_artist.name.clone(),
                },
                _ => Message::SearchExact(top_artist.name.clone()),
            };
            let artist_card = button(
                column![
                    row![
                        self.artwork_widget(top_artist.thumbnail_url.as_deref(), 48.0, true),
                        horizontal_space(),
                        container(
                            text("#1 ARTIST")
                                .size(10)
                                .font(bold())
                                .color(Color::WHITE),
                        )
                        .padding([4, 9])
                        .style(stats_rank_badge_style(p, false)),
                    ]
                    .align_y(Alignment::Center),
                    column![
                        container(
                            text(&top_artist.name)
                                .size(14)
                                .font(bold())
                                .wrapping(Wrapping::None),
                        )
                        .width(Length::Fill)
                        .clip(true),
                        container(
                            text(format!(
                                "{} plays • {} tracks",
                                top_artist.play_count, top_artist.track_count
                            ))
                            .size(11.5)
                            .color(muted)
                            .wrapping(Wrapping::None),
                        )
                        .width(Length::Fill)
                        .clip(true),
                    ]
                    .spacing(3),
                ]
                .spacing(10),
            )
            .width(Length::Fill)
            .padding(14)
            .style(stats_bento_button_style(p, false))
            .on_press(artist_action);
            pulse_cards = pulse_cards.push(artist_card);
        }

        let has_pulse = !s.top_tracks.is_empty() || !s.top_artists.is_empty();
        let body: Element<'_, Message> = if has_pulse {
            row![left_col, container(pulse_cards).width(Length::FillPortion(2))]
                .spacing(20)
                .align_y(Alignment::Center)
                .into()
        } else {
            left_col.width(Length::Fill).into()
        };

        container(
            column![top_bar, body]
                .spacing(18),
        )
        .padding([22, 24])
        .width(Length::Fill)
        .style(hero_banner_style(p))
        .into()
    }

    fn stats_kpi_strip(&self) -> Element<'_, Message> {
        let p = self.palette;
        let muted = color_from_rgb(p.text_muted);
        let accent = color_from_rgb(p.primary);
        let s = &self.listening_summary;

        let avg_per_day = if s.active_days > 0 {
            format!("{}m avg / active day", (s.total_played_secs / 60 / s.active_days).max(1))
        } else {
            "Play tracks to track daily".to_string()
        };
        let streak_text = if s.current_streak_days == 1 {
            "1 day".to_string()
        } else {
            format!("{} days", s.current_streak_days)
        };
        let peak_text = s
            .peak_hour
            .map(format_hour_12)
            .unwrap_or_else(|| "—".to_string());

        let stat_metric = |label: &'static str, value: String, sub: String| -> Element<'_, Message> {
            container(
                column![
                    text(label).size(11).font(bold()).color(muted),
                    text(value).size(26).font(bold()).color(Color::WHITE),
                    text(sub).size(12).color(accent),
                ]
                .spacing(4),
            )
            .width(Length::Fill)
            .padding([6, 12])
            .into()
        };

        let vsep = || {
            container(vertical_space())
                .width(1)
                .height(48)
                .style(divider_style(0.08))
        };

        container(
            column![
                row![
                    stat_metric(
                        "TIME PLAYED",
                        format_duration_hm(s.total_played_secs),
                        avg_per_day,
                    ),
                    vsep(),
                    stat_metric(
                        "TOTAL PLAYS",
                        s.total_plays.to_string(),
                        format!("{} unique tracks", s.unique_tracks),
                    ),
                    vsep(),
                    stat_metric(
                        "ACTIVE STREAK",
                        streak_text,
                        if s.active_days == 1 {
                            "1 active day".to_string()
                        } else {
                            format!("{} active days", s.active_days)
                        },
                    ),
                    vsep(),
                    stat_metric(
                        "PEAK HOUR",
                        peak_text,
                        format!("{} artists • {} liked", s.unique_artists, self.liked_tracks.len()),
                    ),
                ]
                .spacing(10)
                .align_y(Alignment::Center),
                container(horizontal_space())
                    .width(Length::Fill)
                    .height(1)
                    .style(divider_style(0.08)),
            ]
            .spacing(12),
        )
        .padding([0, 24])
        .into()
    }

    fn stats_analytics_row(&self) -> Element<'_, Message> {
        let p = self.palette;
        let muted = color_from_rgb(p.text_muted);
        let accent = color_from_rgb(p.primary);
        let s = &self.listening_summary;

        let max_daily_plays = s
            .daily_activity
            .iter()
            .map(|d| d.plays)
            .max()
            .unwrap_or(0)
            .max(1);
        let recent_14_secs: u64 = s.daily_activity.iter().map(|d| d.total_secs).sum();
        let daily_len = s.daily_activity.len();

        let daily_bars = s
            .daily_activity
            .iter()
            .enumerate()
            .fold(row![].spacing(6).align_y(Alignment::End), |r, (idx, day)| {
                let is_today = idx + 1 == daily_len;
                let is_peak = day.plays == max_daily_plays && day.plays > 0;
                let bar_col = if day.plays == 0 {
                    column![
                        vertical_space(),
                        container(horizontal_space())
                            .width(Length::Fill)
                            .height(3)
                            .style(stat_bar_fill_style(with_alpha(Color::WHITE, 0.10), 999.0)),
                    ]
                    .height(78)
                } else {
                    let pct = ((day.plays * 100) / max_daily_plays).clamp(12, 100) as u16;
                    let fill_color = if is_today || is_peak {
                        accent
                    } else {
                        with_alpha(accent, 0.50)
                    };
                    let mut c = column![];
                    if pct < 100 {
                        c = c.push(vertical_space().height(Length::FillPortion(100 - pct)));
                    }
                    c.push(
                        container(horizontal_space())
                            .width(Length::Fill)
                            .height(Length::FillPortion(pct))
                            .style(stat_bar_fill_style(fill_color, 5.0)),
                    )
                    .height(78)
                };

                let bar_with_tip = self.with_tooltip(
                    bar_col,
                    format!("{}: {} plays ({})", day.date_short, day.plays, format_duration_hm(day.total_secs)),
                    tooltip::Position::Top,
                );

                r.push(
                    column![
                        bar_with_tip,
                        text(&day.day_short)
                            .size(10)
                            .font(if is_today { bold() } else { Font::DEFAULT })
                            .color(if is_today { Color::WHITE } else { muted }),
                    ]
                    .spacing(6)
                    .width(Length::Fill)
                    .align_x(Alignment::Center),
                )
            });

        let rhythm_col = column![
            row![
                column![
                    text("14-Day Rhythm").size(16).font(bold()),
                    text("Daily play volume over the last two weeks")
                        .size(12)
                        .color(muted),
                ]
                .spacing(2),
                horizontal_space(),
                container(
                    text(format_duration_hm(recent_14_secs))
                        .size(12)
                        .font(semibold())
                        .color(accent),
                )
                .padding([4, 10])
                .style(stats_rank_badge_style(p, true)),
            ]
            .align_y(Alignment::Center),
            daily_bars,
        ]
        .spacing(16)
        .width(Length::FillPortion(3));

        let max_hour = s.hourly_plays.iter().copied().max().unwrap_or(0).max(1);
        let clock_bars = (0..24usize).fold(row![].spacing(3).align_y(Alignment::End), |r, hr| {
            let count = s.hourly_plays[hr];
            let is_peak = s.peak_hour == Some(hr as u8) && count > 0;
            let bar_col = if count == 0 {
                column![
                    vertical_space(),
                    container(horizontal_space())
                        .width(Length::Fill)
                        .height(3)
                        .style(stat_bar_fill_style(with_alpha(Color::WHITE, 0.09), 999.0)),
                ]
                .height(52)
            } else {
                let pct = ((count * 100) / max_hour).clamp(14, 100) as u16;
                let color = if is_peak {
                    accent
                } else {
                    with_alpha(accent, 0.44)
                };
                let mut c = column![];
                if pct < 100 {
                    c = c.push(vertical_space().height(Length::FillPortion(100 - pct)));
                }
                c.push(
                    container(horizontal_space())
                        .width(Length::Fill)
                        .height(Length::FillPortion(pct))
                        .style(stat_bar_fill_style(color, 3.0)),
                )
                .height(52)
            };
            r.push(
                container(self.with_tooltip(
                    bar_col,
                    format!("{}: {} plays", format_hour_12(hr as u8), count),
                    tooltip::Position::Top,
                ))
                .width(Length::Fill),
            )
        });

        let total_src = (s.online_plays + s.local_plays).max(1);
        let online_pct = ((s.online_plays * 100) / total_src).min(100) as u16;
        let local_pct = 100u16.saturating_sub(online_pct);
        let mut src_bar = row![].height(7);
        if online_pct > 0 {
            src_bar = src_bar.push(
                container(horizontal_space())
                    .width(Length::FillPortion(online_pct))
                    .height(Length::Fill)
                    .style(stat_bar_fill_style(accent, 999.0)),
            );
        }
        if local_pct > 0 {
            src_bar = src_bar.push(
                container(horizontal_space())
                    .width(Length::FillPortion(local_pct))
                    .height(Length::Fill)
                    .style(stat_bar_fill_style(with_alpha(Color::WHITE, 0.35), 999.0)),
            );
        }

        let clock_col = column![
            row![
                text("Listening Clock").size(16).font(bold()),
                horizontal_space(),
                text(
                    s.peak_hour
                        .map(|h| format!("Peak {}", format_hour_12(h)))
                        .unwrap_or_else(|| "24h split".to_string()),
                )
                .size(12)
                .color(accent),
            ]
            .align_y(Alignment::Center),
            column![
                clock_bars,
                row![
                    text("12a").size(10).color(muted),
                    horizontal_space(),
                    text("6a").size(10).color(muted),
                    horizontal_space(),
                    text("12p").size(10).color(muted),
                    horizontal_space(),
                    text("6p").size(10).color(muted),
                ],
            ]
            .spacing(5),
            column![
                row![
                    text("Source mix").size(11).font(semibold()).color(muted),
                    horizontal_space(),
                    text(format!(
                        "Streams {}% • Local {}%",
                        online_pct, local_pct
                    ))
                    .size(11)
                    .color(muted),
                ],
                container(src_bar)
                    .width(Length::Fill)
                    .style(stat_bar_track_style()),
            ]
            .spacing(5),
        ]
        .spacing(12)
        .width(Length::FillPortion(2));

        let panel = container(
            row![
                rhythm_col,
                container(vertical_space())
                    .width(1)
                    .height(118)
                    .style(divider_style(0.08)),
                clock_col,
            ]
            .spacing(22)
            .align_y(Alignment::Center),
        )
        .padding([18, 20])
        .width(Length::Fill)
        .style(bento_card_style(p));

        container(panel)
            .padding([0, 24])
            .into()
    }

    fn stats_top_artists_section(&self) -> Element<'_, Message> {
        let p = self.palette;
        let muted = color_from_rgb(p.text_muted);
        let accent = color_from_rgb(p.primary);
        let artists = &self.listening_summary.top_artists;

        let cards = artists.iter().enumerate().fold(row![].spacing(10), |r, (rank, ar)| {
            let action = match &ar.id {
                Some(id) if !id.trim().is_empty() => Message::OpenArtist {
                    browse_id: id.clone(),
                    name: ar.name.clone(),
                },
                _ => Message::SearchExact(ar.name.clone()),
            };

            let card = button(
                column![
                    container(self.artwork_widget(ar.thumbnail_url.as_deref(), 108.0, true))
                        .width(Length::Fill)
                        .align_x(iced::alignment::Horizontal::Center),
                    column![
                        container(
                            text(&ar.name)
                                .size(14)
                                .font(bold())
                                .color(if rank == 0 { accent } else { Color::WHITE })
                                .wrapping(Wrapping::None),
                        )
                        .width(Length::Fill)
                        .align_x(iced::alignment::Horizontal::Center)
                        .clip(true),
                        container(
                            text(format!(
                                "#{} • {} plays • {}",
                                rank + 1,
                                ar.play_count,
                                format_duration_hm(ar.total_played_secs)
                            ))
                            .size(11.5)
                            .color(muted)
                            .wrapping(Wrapping::None),
                        )
                        .width(Length::Fill)
                        .align_x(iced::alignment::Horizontal::Center)
                        .clip(true),
                    ]
                    .spacing(3),
                ]
                .spacing(10),
            )
            .width(146)
            .padding([12, 10])
            .style(card_button_style(p))
            .on_press(action);

            r.push(card)
        });

        let hscroll = scrollable(container(cards).padding(iced::Padding {
            top: 2.0,
            right: 4.0,
            bottom: 8.0,
            left: 0.0,
        }))
        .direction(scrollable::Direction::Horizontal(
            scrollable::Scrollbar::new()
                .width(5)
                .scroller_width(4)
                .margin(2),
        ))
        .style(scroll_style(p))
        .width(Length::Fill);

        container(
            column![
                row![
                    text("Top Artists").size(19).font(bold()),
                    horizontal_space(),
                    text(format!("Top {} in range", artists.len()))
                        .size(12)
                        .color(muted),
                ]
                .align_y(Alignment::Center),
                hscroll,
            ]
            .spacing(10),
        )
        .padding([0, 24])
        .into()
    }

    fn stats_top_albums_section(&self) -> Element<'_, Message> {
        let p = self.palette;
        let muted = color_from_rgb(p.text_muted);
        let albums = &self.listening_summary.top_albums;

        let cards = albums.iter().fold(row![].spacing(10), |r, al| {
            let action = match &al.id {
                Some(id) if !id.trim().is_empty() => Message::OpenAlbum {
                    browse_id: id.clone(),
                    title: al.title.clone(),
                },
                _ => Message::SearchExact(format!("{} {}", al.title, al.artist)),
            };
            r.push(
                button(
                    column![
                        container(self.artwork_widget(al.thumbnail_url.as_deref(), 132.0, false))
                            .style(artwork_frame_style(12.0, 0.35)),
                        container(
                            text(&al.title)
                                .size(13)
                                .font(bold())
                                .wrapping(Wrapping::None),
                        )
                        .width(Length::Fill)
                        .clip(true),
                        container(
                            text(format!("{} • {} plays", al.artist, al.play_count))
                                .size(11.5)
                                .color(muted)
                                .wrapping(Wrapping::None),
                        )
                        .width(Length::Fill)
                        .clip(true),
                    ]
                    .spacing(5)
                    .width(Length::Fill),
                )
                .width(156)
                .padding(10)
                .style(card_button_style(p))
                .on_press(action),
            )
        });

        let hscroll = scrollable(container(cards).padding(iced::Padding {
            top: 2.0,
            right: 4.0,
            bottom: 8.0,
            left: 0.0,
        }))
        .direction(scrollable::Direction::Horizontal(
            scrollable::Scrollbar::new()
                .width(5)
                .scroller_width(4)
                .margin(2),
        ))
        .style(scroll_style(p))
        .width(Length::Fill);

        container(
            column![
                text("Top Albums").size(19).font(bold()),
                hscroll,
            ]
            .spacing(10),
        )
        .padding([0, 24])
        .into()
    }

    fn stats_top_tracks_section(&self) -> Element<'_, Message> {
        let p = self.palette;
        let muted = color_from_rgb(p.text_muted);
        let accent = color_from_rgb(p.primary);
        let top_ten: Vec<_> = self.listening_summary.top_tracks.iter().take(10).collect();
        let max_plays = top_ten.iter().map(|t| t.play_count).max().unwrap_or(1).max(1);
        let all_tracks: Vec<Track> = top_ten.iter().map(|st| st.track.clone()).collect();

        let list = top_ten
            .iter()
            .enumerate()
            .fold(column![].spacing(2), |c, (i, st)| {
                let pct = ((st.play_count * 100) / max_plays).clamp(8, 100) as u16;
                let mut share = row![].height(3);
                share = share.push(
                    container(horizontal_space())
                        .width(Length::FillPortion(pct))
                        .height(Length::Fill)
                        .style(stat_bar_fill_style(accent, 999.0)),
                );
                if pct < 100 {
                    share = share.push(horizontal_space().width(Length::FillPortion(100 - pct)));
                }

                let is_liked = self.liked_tracks.iter().any(|l| l.id == st.track.id) || st.track.liked;
                let row_item = mouse_area(
                    row![
                        container(
                            text(format!("{:02}", i + 1))
                                .size(13)
                                .font(bold())
                                .color(if i < 3 { accent } else { muted }),
                        )
                        .width(28)
                        .align_x(iced::alignment::Horizontal::Center),
                        button(
                            row![
                                self.artwork_widget(st.track.thumbnail_url.as_deref(), 42.0, false),
                                column![
                                    container(
                                        text(&st.track.title)
                                            .size(14)
                                            .font(semibold())
                                            .wrapping(Wrapping::None),
                                    )
                                    .width(Length::Fill)
                                    .clip(true),
                                    container(
                                        text(st.track.artist_display())
                                            .size(12)
                                            .color(muted)
                                            .wrapping(Wrapping::None),
                                    )
                                    .width(Length::Fill)
                                    .clip(true),
                                ]
                                .spacing(2)
                                .width(Length::Fill),
                            ]
                            .spacing(12)
                            .align_y(Alignment::Center),
                        )
                        .width(Length::Fill)
                        .padding([7, 8])
                        .style(ghost_row_button_style(p, false))
                        .on_press(Message::PlayTracks(all_tracks.clone(), i)),
                        column![
                            row![
                                text(format!("{} plays", st.play_count))
                                    .size(12)
                                    .font(semibold())
                                    .color(if i < 3 { accent } else { Color::WHITE }),
                                horizontal_space(),
                                text(format_duration_hm(st.total_played_secs))
                                    .size(11)
                                    .color(muted),
                            ]
                            .width(124),
                            container(share)
                                .width(124)
                                .style(stat_bar_track_style()),
                        ]
                        .spacing(5),
                        button(icons::icon(
                            if is_liked {
                                icons::HEART_FILLED
                            } else {
                                icons::HEART_OUTLINE
                            },
                            15.0,
                            if is_liked { accent } else { muted },
                        ))
                        .padding(7)
                        .style(icon_button_style(p, false))
                        .on_press(Message::ToggleLikeTrack(st.track.clone())),
                        button(icons::icon(icons::PLAY, 14.0, Color::WHITE))
                            .padding(8)
                            .style(icon_button_style(p, false))
                            .on_press(Message::PlayTracks(all_tracks.clone(), i)),
                    ]
                    .spacing(10)
                    .align_y(Alignment::Center),
                )
                .on_right_press(Message::OpenTrackMenu(st.track.clone()));

                c.push(row_item)
            });

        let play_all_btn: Element<'_, Message> = if !all_tracks.is_empty() {
            button(
                row![
                    icons::icon(icons::PLAY, 12.0, Color::WHITE),
                    text("Play Top 10").size(12).font(semibold()),
                ]
                .spacing(6)
                .align_y(Alignment::Center),
            )
            .padding([6, 13])
            .style(soft_pill_button_style(p))
            .on_press(Message::PlayTracks(all_tracks, 0))
            .into()
        } else {
            horizontal_space().width(0).into()
        };

        container(
            column![
                row![
                    text("Top 10 Songs").size(19).font(bold()),
                    horizontal_space(),
                    play_all_btn,
                ]
                .align_y(Alignment::Center),
                list,
            ]
            .spacing(12),
        )
        .padding([0, 24])
        .into()
    }

    fn view_settings(&self) -> Element<'_, Message> {
        let p = self.palette;
        let muted = color_from_rgb(p.text_muted);

        let header = container(
            column![
                text("Settings").size(30).font(bold()),
                text("Audio engine, local media library, appearance, and connected accounts")
                    .size(13)
                    .color(muted),
            ]
            .spacing(4),
        )
        .padding([22, 24])
        .width(Length::Fill)
        .style(hero_banner_style(p));

        let setting_label = |title: &'static str, hint: &'static str| -> Element<'_, Message> {
            column![
                text(title).size(14).font(bold()),
                text(hint).size(12).color(muted),
            ]
            .spacing(2)
            .into()
        };

        let repair_btn = if self.repair_running {
            button(text("Stop Volume Repair").size(13))
                .padding([8, 16])
                .style(top_pill_button_style(p, false))
                .on_press(Message::StopFixVolumes)
        } else {
            button(text(format!(
                "Fix Song Volumes ({} missing)",
                self.missing_loudness_count
            )).size(13))
            .padding([8, 16])
            .style(top_pill_button_style(p, false))
            .on_press(Message::StartFixVolumes)
        };

        let audio_card = container(
            column![
                text("Player & Audio").size(17).font(bold()),
                row![
                    setting_label(
                        "Audio normalization",
                        "Level tracks to -3 dB reference loudness"
                    ),
                    horizontal_space(),
                    toggler(self.settings.audio_normalization)
                        .style(toggler_style(p))
                        .on_toggle(Message::ToggleNormalization),
                ]
                .align_y(Alignment::Center),
                row![
                    setting_label(
                        "Skip silence",
                        "Automatically trim quiet passages during playback"
                    ),
                    horizontal_space(),
                    toggler(self.settings.skip_silence)
                        .style(toggler_style(p))
                        .on_toggle(Message::ToggleSkipSilence),
                ]
                .align_y(Alignment::Center),
                row![
                    setting_label(
                        "Better Lyrics (TTML)",
                        "Prefer word/line-synced timing when available"
                    ),
                    horizontal_space(),
                    toggler(self.settings.better_lyrics_enabled)
                        .style(toggler_style(p))
                        .on_toggle(Message::ToggleBetterLyrics),
                ]
                .align_y(Alignment::Center),
                row![
                    setting_label(
                        "Romanize lyrics",
                        "Show Latin transliteration below Japanese and Korean lyric lines"
                    ),
                    horizontal_space(),
                    toggler(self.settings.romanize_lyrics)
                        .style(toggler_style(p))
                        .on_toggle(|_| Message::ToggleRomanizeLyrics),
                ]
                .align_y(Alignment::Center),
                row![
                    setting_label(
                        "Auto-load Up Next",
                        "Queue similar radio tracks automatically when playback nears the end"
                    ),
                    horizontal_space(),
                    toggler(self.settings.auto_load_more)
                        .style(toggler_style(p))
                        .on_toggle(Message::ToggleAutoLoadMore),
                ]
                .align_y(Alignment::Center),
                row![
                    setting_label(
                        "Transition fade",
                        "Smooth fade-in/out on automatic track advance"
                    ),
                    horizontal_space(),
                    pick_list(
                        &TransitionFadeOption::ALL[..],
                        Some(TransitionFadeOption::from_ms(self.settings.transition_fade_ms)),
                        Message::TransitionFadeSelected,
                    )
                    .padding([7, 14])
                    .style(pick_list_style(p))
                    .menu_style(pick_list_menu_style(p)),
                ]
                .align_y(Alignment::Center),
                row![
                    setting_label("Equalizer preset", "3-band tonal shaping"),
                    horizontal_space(),
                    pick_list(
                        &EqPreset::ALL[..],
                        Some(self.settings.eq_preset),
                        Message::EqPresetSelected,
                    )
                    .padding([7, 14])
                    .style(pick_list_style(p))
                    .menu_style(pick_list_menu_style(p)),
                ]
                .align_y(Alignment::Center),
                row![
                    setting_label("Spatial audio", "Stereo widening and room ambience"),
                    horizontal_space(),
                    pick_list(
                        &SpatialMode::ALL[..],
                        Some(self.settings.spatial_mode),
                        Message::SpatialModeSelected,
                    )
                    .padding([7, 14])
                    .style(pick_list_style(p))
                    .menu_style(pick_list_menu_style(p)),
                ]
                .align_y(Alignment::Center),
                row![
                    setting_label("Streaming quality", "Preferred audio bitrate from YouTube Music"),
                    horizontal_space(),
                    pick_list(
                        &AudioQuality::ALL[..],
                        Some(self.settings.audio_quality),
                        Message::AudioQualitySelected,
                    )
                    .padding([7, 14])
                    .style(pick_list_style(p))
                    .menu_style(pick_list_menu_style(p)),
                ]
                .align_y(Alignment::Center),
                row![
                    repair_btn,
                    text(self.repair_status.as_deref().unwrap_or(""))
                        .size(12)
                        .color(muted)
                ]
                .spacing(12)
                .align_y(Alignment::Center),
            ]
            .spacing(14),
        )
        .padding(18)
        .style(elevated_card_style(p));

        let theme_card = container(
            column![
                text("Appearance & Recognition").size(17).font(bold()),
                row![
                    setting_label(
                        "Theme mode",
                        "Dynamic extracts ambient palette colors from cover art"
                    ),
                    horizontal_space(),
                    pick_list(
                        &ThemeMode::ALL[..],
                        Some(self.settings.theme_mode),
                        Message::ThemeModeSelected,
                    )
                    .padding([7, 14])
                    .style(pick_list_style(p))
                    .menu_style(pick_list_menu_style(p)),
                ]
                .align_y(Alignment::Center),
                row![
                    setting_label(
                        "Recognition audio input",
                        "System loopback or microphone capture for Shazam"
                    ),
                    horizontal_space(),
                    pick_list(
                        &RecognitionSource::ALL[..],
                        Some(self.settings.recognition_source),
                        Message::RecognitionSourceSelected,
                    )
                    .padding([7, 14])
                    .style(pick_list_style(p))
                    .menu_style(pick_list_menu_style(p)),
                ]
                .align_y(Alignment::Center),
            ]
            .spacing(14),
        )
        .padding(18)
        .style(elevated_card_style(p));

        let lastfm_controls: Element<'_, Message> = if let Some(user) = &self.settings.lastfm_username {
            row![
                text(format!("Signed in as {user}"))
                    .size(13)
                    .font(bold())
                    .color(color_from_rgb(p.primary)),
                horizontal_space(),
                toggler(self.settings.lastfm_scrobbling_enabled)
                    .label("Scrobble")
                    .style(toggler_style(p))
                    .on_toggle(Message::ToggleLastFmScrobbling),
                button(text("Disconnect").size(13))
                    .padding([7, 14])
                    .style(outlined_pill_button_style(p, false))
                    .on_press(Message::DisconnectLastFm),
            ]
            .spacing(12)
            .align_y(Alignment::Center)
            .into()
        } else {
            let mut r = row![
                button(text("Connect Last.fm (Browser)").size(13).font(bold()))
                    .padding([8, 16])
                    .style(top_pill_button_style(p, true))
                    .on_press(Message::StartLastFmAuth)
            ]
            .spacing(10);
            if self.lastfm_pending_token.is_some() {
                r = r.push(
                    button(text("Complete Login").size(13))
                        .padding([8, 16])
                        .style(outlined_pill_button_style(p, true))
                        .on_press(Message::CompleteLastFmAuth),
                );
            }
            r.into()
        };

        let lastfm_card = container(
            column![
                text("Last.fm Scrobbling").size(17).font(bold()),
                row![
                    text_input("Last.fm API Key", &self.settings.lastfm_api_key)
                        .style(pill_search_input_style(p))
                        .padding([9, 14])
                        .on_input(Message::LastFmApiKeyChanged),
                    text_input("Last.fm API Secret", &self.settings.lastfm_api_secret)
                        .style(pill_search_input_style(p))
                        .padding([9, 14])
                        .secure(true)
                        .on_input(Message::LastFmApiSecretChanged),
                ]
                .spacing(10),
                lastfm_controls,
                if let Some(st) = &self.lastfm_status {
                    text(st).size(12).color(muted)
                } else {
                    text("").size(1)
                }
            ]
            .spacing(12),
        )
        .padding(18)
        .style(elevated_card_style(p));

        let account_card = container(
            column![
                text("YouTube Music Account & Sync").size(17).font(bold()),
                text(
                    "Paste your music.youtube.com Cookie header to sync library playlists, liked songs, subscriptions, and play history."
                )
                .size(12)
                .color(muted),
                text_input("Cookie (SAPISID=...; __Secure-3PAPISID=...)", &self.yt_cookie_input)
                    .style(pill_search_input_style(p))
                    .padding([9, 14])
                    .secure(true)
                    .on_input(Message::YouTubeCookieChanged)
                    .on_submit(Message::SaveYouTubeCookie),
                row![
                    button(text("Save Cookie").size(13).font(bold()))
                        .padding([8, 16])
                        .style(top_pill_button_style(p, true))
                        .on_press(Message::SaveYouTubeCookie),
                    button(text(if self.account_syncing {
                        "Syncing..."
                    } else {
                        "Sync Playlists & Likes"
                    }).size(13))
                        .padding([8, 16])
                        .style(top_pill_button_style(p, false))
                        .on_press(Message::SyncYouTubeAccount),
                    button(text("Clear").size(13))
                        .padding([8, 14])
                        .style(outlined_pill_button_style(p, false))
                        .on_press(Message::ClearYouTubeCookie),
                ]
                .spacing(10),
                row![
                    setting_label(
                        "Content region",
                        "Country catalog used for Home feed, Explore, and charts"
                    ),
                    horizontal_space(),
                    pick_list(
                        &YoutubeRegionOption::ALL[..],
                        Some(YoutubeRegionOption::from_code(&self.settings.youtube_gl)),
                        Message::YoutubeRegionSelected,
                    )
                    .padding([7, 14])
                    .style(pick_list_style(p))
                    .menu_style(pick_list_menu_style(p)),
                ]
                .align_y(Alignment::Center),
                row![
                    setting_label(
                        "Language",
                        "Preferred display language for YouTube Music metadata"
                    ),
                    horizontal_space(),
                    pick_list(
                        &YoutubeLanguageOption::ALL[..],
                        Some(YoutubeLanguageOption::from_code(&self.settings.youtube_hl)),
                        Message::YoutubeLanguageSelected,
                    )
                    .padding([7, 14])
                    .style(pick_list_style(p))
                    .menu_style(pick_list_menu_style(p)),
                ]
                .align_y(Alignment::Center),
                text(
                    self.account_sync_status
                        .as_deref()
                        .unwrap_or(if self.yt.has_cookie() {
                            "Signed in via saved YouTube cookie."
                        } else {
                            "Not signed in (anonymous mode)."
                        })
                )
                .size(12)
                .color(if self.yt.has_cookie() {
                    color_from_rgb(p.primary)
                } else {
                    muted
                }),
            ]
            .spacing(12),
        )
        .padding(18)
        .style(elevated_card_style(p));

        let backup_path_hint = default_backup_path()
            .map(|p| p.display().to_string())
            .unwrap_or_else(|_| "echo-backup.echobak".to_string());

        let backup_card = container(
            column![
                text("Backup & Restore").size(17).font(bold()),
                text(format!("Choose a .echobak snapshot location (default: {backup_path_hint})"))
                    .size(12)
                    .color(muted),
                row![
                    button(text("Export Backup...").size(13).font(bold()))
                        .padding([8, 16])
                        .style(top_pill_button_style(p, true))
                        .on_press(Message::ExportBackup),
                    button(text("Import Backup...").size(13))
                        .padding([8, 16])
                        .style(top_pill_button_style(p, false))
                        .on_press(Message::ImportBackup),
                ]
                .spacing(10),
                if let Some(st) = &self.backup_status {
                    text(st).size(12).color(color_from_rgb(p.primary))
                } else {
                    text("").size(1)
                }
            ]
            .spacing(12),
        )
        .padding(18)
        .style(elevated_card_style(p));

        let dl_default_hint = self
            .settings
            .effective_downloads_dir()
            .map(|d| d.display().to_string())
            .unwrap_or_else(|_| "Echo Downloads".to_string());
        let cache_mb_used = self.cache_stats.1 as f64 / (1024.0 * 1024.0);
        let folder_btn = |label: &'static str, target: FolderTarget| -> Element<'_, Message> {
            button(
                row![
                    icons::icon(icons::FOLDER, 14.0, muted),
                    text(label).size(12),
                ]
                .spacing(6)
                .align_y(Alignment::Center),
            )
            .padding([8, 14])
            .style(outlined_pill_button_style(p, false))
            .on_press(Message::PickFolder(target))
            .into()
        };
        let downloads_cache_card = container(
            column![
                text("Downloads & Streaming Cache").size(17).font(bold()),
                row![
                    column![
                        text("Max stream cache (MB)").size(14).font(bold()),
                        text(format!(
                            "Currently storing {} files ({:.1} MB)",
                            self.cache_stats.0, cache_mb_used
                        ))
                        .size(12)
                        .color(muted),
                    ]
                    .spacing(2),
                    horizontal_space(),
                    text_input("512", &self.cache_size_input)
                        .style(pill_search_input_style(p))
                        .padding([8, 12])
                        .width(96)
                        .on_input(Message::MaxCacheSizeChanged),
                    button(text("Clear Cache").size(12))
                        .padding([8, 14])
                        .style(outlined_pill_button_style(p, false))
                        .on_press(Message::ClearStreamCache),
                ]
                .spacing(10)
                .align_y(Alignment::Center),
                text(format!("Active download folder: {dl_default_hint}"))
                    .size(12)
                    .color(muted),
                row![
                    text_input("Custom download folder (blank = default)", &self.download_folder_input)
                        .style(pill_search_input_style(p))
                        .padding([9, 14])
                        .on_input(Message::DownloadFolderChanged)
                        .on_submit(Message::SaveDownloadSettings)
                        .width(Length::Fill),
                    folder_btn("Browse", FolderTarget::DownloadFolder),
                ]
                .spacing(8)
                .align_y(Alignment::Center),
                row![
                    text_input(
                        "Extra import folders containing [youtube_id] files (semicolon-separated)",
                        &self.extra_import_input
                    )
                    .style(pill_search_input_style(p))
                    .padding([9, 14])
                    .on_input(Message::ExtraImportFoldersChanged)
                    .on_submit(Message::SaveDownloadSettings)
                    .width(Length::Fill),
                    folder_btn("Add", FolderTarget::ExtraImportFolder),
                ]
                .spacing(8)
                .align_y(Alignment::Center),
                row![
                    button(text("Save Folders").size(13).font(bold()))
                        .padding([8, 16])
                        .style(top_pill_button_style(p, true))
                        .on_press(Message::SaveDownloadSettings),
                    button(text("Rescan [id] Folders").size(13))
                        .padding([8, 14])
                        .style(top_pill_button_style(p, false))
                        .on_press(Message::ScanDownloadFolders),
                    button(text("Migrate Downloads").size(13))
                        .padding([8, 14])
                        .style(top_pill_button_style(p, false))
                        .on_press(Message::MigrateDownloads),
                ]
                .spacing(10),
                if let Some(st) = &self.download_status {
                    text(st).size(12).color(color_from_rgb(p.primary))
                } else {
                    text("").size(1)
                }
            ]
            .spacing(12),
        )
        .padding(18)
        .style(elevated_card_style(p));

        let local_media_card = container(
            column![
                text("Local Media Library").size(17).font(bold()),
                row![
                    text_input(
                        "Music folders to scan (semicolon-separated paths)",
                        &self.local_folders_input
                    )
                    .style(pill_search_input_style(p))
                    .padding([9, 14])
                    .on_input(Message::LocalFoldersChanged)
                    .on_submit(Message::ScanLocalMedia)
                    .width(Length::Fill),
                    folder_btn("Add", FolderTarget::LocalMediaFolder),
                ]
                .spacing(8)
                .align_y(Alignment::Center),
                row![
                    text_input(
                        "Excluded folders (semicolon-separated paths)",
                        &self.excluded_folders_input
                    )
                    .style(pill_search_input_style(p))
                    .padding([9, 14])
                    .on_input(Message::ExcludedFoldersChanged)
                    .width(Length::Fill),
                    folder_btn("Add", FolderTarget::ExcludedMediaFolder),
                ]
                .spacing(8)
                .align_y(Alignment::Center),
                row![
                    setting_label(
                        "Deduplication sensitivity",
                        "How strictly local files are matched against library tracks"
                    ),
                    horizontal_space(),
                    pick_list(
                        &ScannerSensitivity::ALL[..],
                        Some(self.settings.scanner_sensitivity),
                        Message::ScannerSensitivitySelected,
                    )
                    .padding([7, 14])
                    .style(pick_list_style(p))
                    .menu_style(pick_list_menu_style(p)),
                ]
                .align_y(Alignment::Center),
                row![
                    setting_label(
                        "Strict filenames",
                        "Ignore 'Artist - Title' filename guessing when tags are missing"
                    ),
                    horizontal_space(),
                    toggler(self.settings.strict_file_names)
                        .style(toggler_style(p))
                        .on_toggle(Message::ToggleStrictFileNames),
                ]
                .align_y(Alignment::Center),
                row![
                    setting_label(
                        "Auto-rescan on startup",
                        "Index local folders automatically when Echo launches"
                    ),
                    horizontal_space(),
                    toggler(self.settings.auto_scan_local_on_startup)
                        .style(toggler_style(p))
                        .on_toggle(Message::ToggleAutoScanLocal),
                ]
                .align_y(Alignment::Center),
                row![
                    button(text(if self.local_scanning {
                        "Scanning..."
                    } else {
                        "Scan Local Media Now"
                    }).size(13).font(bold()))
                    .padding([8, 16])
                    .style(top_pill_button_style(p, true))
                    .on_press(Message::ScanLocalMedia),
                    text(self.local_scan_status.as_deref().unwrap_or(""))
                        .size(12)
                        .color(muted),
                ]
                .spacing(12)
                .align_y(Alignment::Center),
            ]
            .spacing(12),
        )
        .padding(18)
        .style(elevated_card_style(p));

        self.vscroll(
            column![
                header,
                container(
                    column![
                        audio_card,
                        downloads_cache_card,
                        local_media_card,
                        account_card,
                        theme_card,
                        lastfm_card,
                        backup_card,
                    ]
                    .spacing(14),
                )
                .padding([8, 24]),
                vertical_space().height(24),
            ]
            .spacing(10),
        )
    }

    fn view_expanded_player(&self) -> Element<'_, Message> {
        let p = self.palette;
        let muted = color_from_rgb(p.text_muted);
        let accent = color_from_rgb(p.primary);

        let Some(cur) = self.queue.current() else {
            return container(
                column![
                    button(
                        row![
                            icons::icon(icons::BACK, 16.0, Color::WHITE),
                            text("Back").size(13).font(bold()),
                        ]
                        .spacing(6)
                        .align_y(Alignment::Center),
                    )
                    .padding([8, 14])
                    .style(top_pill_button_style(p, false))
                    .on_press(Message::ToggleExpandedPlayer),
                    text("Nothing is playing.").size(16).color(muted),
                ]
                .spacing(14),
            )
            .padding(28)
            .into();
        };

        let t = &cur.track;
        let is_liked = self.liked_tracks.iter().any(|l| l.id == t.id) || t.liked;
        let loudness_text = match t.loudness_db {
            Some(db) => format!("{db:+.1} dB LUFS"),
            None if t.is_local => "Local Audio".to_string(),
            None => "Auto Leveling".to_string(),
        };

        let album_pill = t.album.as_ref().map(|a| a.title.as_str()).unwrap_or("Single");

        let album_pill_el: Element<'_, Message> = if t.album.is_some() {
            button(text(ellipsize(album_pill, 22)).size(11).color(Color::WHITE))
                .padding([5, 11])
                .style(soft_pill_button_style(p))
                .on_press(Message::OpenTrackAlbum(t.clone()))
                .into()
        } else {
            container(text(ellipsize(album_pill, 22)).size(11).color(muted))
                .padding([5, 11])
                .style(badge_pill_style(p))
                .into()
        };

        let art_size = (self.window_size.1 - 420.0).clamp(200.0, 280.0);

        let quality_text = if t.is_local {
            "Local Lossless"
        } else {
            match self.settings.audio_quality {
                AudioQuality::Max => "Hi-Res 24-bit",
                AudioQuality::High => "Lossless 48kHz",
                AudioQuality::Auto => "Adaptive HQ",
                AudioQuality::Low => "Data Saver",
            }
        };

        let badges_row = row![
            container(
                text(loudness_text)
                    .size(11)
                    .font(mono())
                    .color(Color::from_rgba(1.0, 1.0, 1.0, 0.72)),
            )
            .padding([3, 9])
            .style(badge_pill_style(p)),
            container(
                text(quality_text)
                    .size(11)
                    .font(mono())
                    .color(Color::from_rgba(1.0, 1.0, 1.0, 0.72)),
            )
            .padding([3, 9])
            .style(badge_pill_style(p)),
        ]
        .spacing(6)
        .align_y(Alignment::Center);

        let next_track: Option<&Track> = match (self.queue.current_index, self.queue.repeat_mode, self.queue.shuffle) {
            (Some(idx), RepeatMode::One, _) => self.queue.items.get(idx).map(|q| &q.track),
            (Some(idx), RepeatMode::All, _) if !self.queue.items.is_empty() => {
                let next_idx = (idx + 1) % self.queue.items.len();
                self.queue.items.get(next_idx).map(|q| &q.track)
            }
            (Some(idx), RepeatMode::Off, _) => {
                self.queue.items.get(idx + 1).map(|q| &q.track)
            }
            _ => None,
        };

        let up_next_el: Element<'_, Message> = if let Some(nt) = next_track {
            self.with_tooltip(
                button(
                    row![
                        container(self.artwork_widget(nt.thumbnail_url.as_deref(), 38.0, false))
                            .style(artwork_frame_style(6.0, 0.35)),
                        column![
                            text("UP NEXT")
                                .size(9)
                                .font(bold())
                                .color(with_alpha(accent, 0.85)),
                            container(
                                text(&nt.title)
                                    .size(12)
                                    .font(semibold())
                                    .color(Color::WHITE)
                                    .wrapping(Wrapping::None),
                            )
                            .width(Length::Fill)
                            .clip(true),
                            container(
                                text(nt.artist_display())
                                    .size(11)
                                    .color(Color::from_rgba(1.0, 1.0, 1.0, 0.60))
                                    .wrapping(Wrapping::None),
                            )
                            .width(Length::Fill)
                            .clip(true),
                        ]
                        .spacing(1)
                        .width(Length::Fill),
                        icons::icon(icons::NEXT, 14.0, Color::from_rgba(1.0, 1.0, 1.0, 0.45)),
                    ]
                    .spacing(10)
                    .align_y(Alignment::Center),
                )
                .padding([8, 12])
                .style(up_next_card_button_style(p))
                .on_press(Message::NextTrack)
                .width(Length::Fill),
                "Click to play next track",
                tooltip::Position::Top,
            )
        } else {
            container(
                row![
                    icons::icon(icons::QUEUE, 13.0, Color::from_rgba(1.0, 1.0, 1.0, 0.45)),
                    text(if self.settings.auto_load_more {
                        "Continuous radio queue active"
                    } else {
                        "End of queue"
                    })
                    .size(11)
                    .color(Color::from_rgba(1.0, 1.0, 1.0, 0.55)),
                ]
                .spacing(7)
                .align_y(Alignment::Center),
            )
            .padding([8, 12])
            .into()
        };

        let left_inner = column![
            row![
                button(
                    row![
                        icons::icon(icons::BACK, 14.0, Color::WHITE),
                        text("Close Stage").size(12).font(semibold()),
                    ]
                    .spacing(6)
                    .align_y(Alignment::Center),
                )
                .padding([6, 12])
                .style(outlined_pill_button_style(p, false))
                .on_press(Message::ToggleExpandedPlayer),
                horizontal_space(),
                album_pill_el,
            ]
            .align_y(Alignment::Center),
            vertical_space().height(6),
            container(
                container(self.artwork_widget(t.thumbnail_url.as_deref(), art_size, false))
                    .style(artwork_frame_style(16.0, 0.55)),
            )
            .width(Length::Fill)
            .align_x(iced::alignment::Horizontal::Center),
            vertical_space().height(8),
            row![
                column![
                    container(
                        text(&t.title)
                            .size(22)
                            .font(bold())
                            .wrapping(Wrapping::None),
                    )
                    .width(Length::Fill)
                    .clip(true),
                    button(
                        container(
                            text(t.artist_display())
                                .size(14)
                                .wrapping(Wrapping::None)
                                .color(Color::from_rgba(1.0, 1.0, 1.0, 0.76)),
                        )
                        .width(Length::Fill)
                        .clip(true),
                    )
                    .padding([1, 0])
                    .style(ghost_row_button_style(p, false))
                    .on_press(Message::OpenTrackArtist(t.clone())),
                ]
                .spacing(2)
                .width(Length::Fill),
                button(icons::icon(
                    if is_liked {
                        icons::HEART_FILLED
                    } else {
                        icons::HEART_OUTLINE
                    },
                    20.0,
                    if is_liked { accent } else { muted },
                ))
                .padding(8)
                .style(icon_button_style(p, false))
                .on_press(Message::ToggleLikeCurrent),
            ]
            .spacing(8)
            .align_y(Alignment::Center),
            badges_row,
            vertical_space().height(6),
            up_next_el,
            vertical_space().height(6),
            self.live_spectrum_strip(accent),
        ]
        .width(332)
        .spacing(6);

        let left = container(left_inner)
            .height(Length::Fill)
            .align_y(iced::alignment::Vertical::Center);

        let lyrics_panel = self.view_lyrics_inner(30, 20);

        let stage_foreground = container(
            row![
                left,
                container(lyrics_panel)
                    .width(Length::Fill)
                    .height(Length::Fill)
                    .padding([8, 16]),
            ]
            .spacing(38)
            .align_y(Alignment::Center),
        )
        .padding([20, 32])
        .width(Length::Fill)
        .height(Length::Fill);

        if let Some(blur_handle) = t
            .thumbnail_url
            .as_deref()
            .and_then(|u| self.blurred_covers.get(u))
        {
            container(
                stack![
                    container(
                        image(blur_handle.clone())
                            .width(Length::Fill)
                            .height(Length::Fill)
                            .content_fit(iced::ContentFit::Cover),
                    )
                    .width(Length::Fill)
                    .height(Length::Fill)
                    .clip(true),
                    container(stage_foreground)
                        .width(Length::Fill)
                        .height(Length::Fill)
                        .style(ambient_scrim_style(p, 0.68, 0.88)),
                ],
            )
            .width(Length::Fill)
            .height(Length::Fill)
            .clip(true)
            .into()
        } else {
            stage_foreground.style(expanded_stage_style(p)).into()
        }
    }

    fn view_right_rail(&self) -> Element<'_, Message> {
        let p = self.palette;
        let muted = color_from_rgb(p.text_muted);
        let accent = color_from_rgb(p.primary);

        let tab_pill = |lbl: &'static str, tab: RightTab| -> Element<'_, Message> {
            button(text(lbl).size(12))
                .padding([5, 10])
                .style(top_pill_button_style(p, self.right_tab == tab))
                .on_press(Message::SelectRightTab(tab))
                .into()
        };

        let header = row![
            text(match self.right_tab {
                RightTab::Now => self
                    .queue
                    .current()
                    .and_then(|c| c.track.album.as_ref().map(|a| a.title.as_str()))
                    .unwrap_or("Now Playing"),
                RightTab::Queue => "Queue",
                RightTab::Lyrics => "Lyrics",
                RightTab::Top => "Top Picks",
            })
            .size(15)
            .font(bold())
            .wrapping(Wrapping::None),
            horizontal_space(),
            button(icons::icon(icons::CLOSE, 16.0, muted))
                .padding(6)
                .style(icon_button_style(p, false))
                .on_press(Message::ToggleRightSidebar),
        ]
        .align_y(Alignment::Center);

        let tabs = row![
            tab_pill("Now", RightTab::Now),
            tab_pill("Queue", RightTab::Queue),
            tab_pill("Lyrics", RightTab::Lyrics),
            tab_pill("Top", RightTab::Top),
        ]
        .spacing(4);

        let top_content: Element<'_, Message> = match self.right_tab {
            RightTab::Now => {
                let mut col = column![].spacing(14);
                if let Some(cur) = self.queue.current() {
                    let t = &cur.track;
                    let is_liked =
                        self.liked_tracks.iter().any(|l| l.id == t.id) || t.liked;
                    let rail_w = self.right_rail_width();
                    col = col.push(self.artwork_widget(t.thumbnail_url.as_deref(), rail_w - 30.0, false));
                    col = col.push(
                        row![
                            column![
                                container(
                                    text(&t.title)
                                        .size(18)
                                        .font(bold())
                                        .wrapping(Wrapping::None),
                                )
                                .width(Length::Fill)
                                .clip(true),
                                button(
                                    container(
                                        text(t.artist_display())
                                            .size(13)
                                            .color(muted)
                                            .wrapping(Wrapping::None),
                                    )
                                    .width(Length::Fill)
                                    .clip(true),
                                )
                                .padding([2, 0])
                                .style(ghost_row_button_style(p, false))
                                .on_press(Message::OpenTrackArtist(t.clone())),
                            ]
                            .spacing(2)
                            .width(Length::Fill),
                            button(if is_liked {
                                icons::icon(icons::HEART_FILLED, 20.0, accent)
                            } else {
                                icons::icon(icons::HEART_OUTLINE, 20.0, muted)
                            })
                            .padding(6)
                            .style(icon_button_style(p, false))
                            .on_press(Message::ToggleLikeCurrent),
                        ]
                        .align_y(Alignment::Center),
                    );
                } else {
                    col = col.push(
                        container(
                            column![
                                text("Nothing playing").size(16).font(bold()),
                                text("Select a song, playlist, or album to begin.")
                                    .size(13)
                                    .color(muted),
                            ]
                            .spacing(6),
                        )
                        .width(Length::Fill)
                        .padding(16)
                        .style(elevated_card_style(p)),
                    );
                }

                // Lyrics preview card matching Spotify Screenshot 1 right rail
                let mut preview_col = column![
                    row![
                        text("Lyrics preview").size(14).font(bold()),
                        horizontal_space(),
                        button(text("Show lyrics").size(11).color(Color::WHITE))
                            .padding([4, 10])
                            .style(outlined_pill_button_style(p, false))
                            .on_press(Message::SelectRightTab(RightTab::Lyrics)),
                    ]
                    .align_y(Alignment::Center),
                ]
                .spacing(8);
                if let Some(res) = &self.active_lyrics {
                    if !res.synced.is_empty() {
                        let start = res
                            .active_index(self.position_ms)
                            .unwrap_or(0)
                            .saturating_sub(1);
                        for (offset, line) in res.synced.iter().skip(start).take(4).enumerate() {
                            let active_line = start + offset == res.active_index(self.position_ms).unwrap_or(usize::MAX);
                            preview_col = preview_col.push(
                                text(if line.text.is_empty() { "♪" } else { &line.text })
                                    .size(15)
                                    .font(bold())
                                    .color(if active_line { Color::WHITE } else { muted }),
                            );
                        }
                    } else if let Some(plain) = &res.plain {
                        let snippet: String = plain.lines().take(4).collect::<Vec<_>>().join("\n");
                        preview_col = preview_col.push(text(snippet).size(13).color(muted));
                    }
                } else {
                    preview_col = preview_col.push(
                        text(if self.lyrics_loading {
                            "Loading synced lyrics..."
                        } else {
                            "Synced lyrics appear here while music plays."
                        })
                        .size(13)
                        .color(muted),
                    );
                }
                col = col.push(
                    container(preview_col)
                        .width(Length::Fill)
                        .padding(14)
                        .style(elevated_card_style(p)),
                );

                // Next in queue mini card
                if let Some(cur_idx) = self.queue.current_index {
                    if let Some(next_item) = self.queue.items.get(cur_idx + 1) {
                        col = col.push(
                            container(
                                column![
                                    row![
                                        text("Next in queue").size(13).font(bold()),
                                        horizontal_space(),
                                        button(text("Open queue").size(11).color(muted))
                                            .padding([2, 6])
                                            .style(ghost_row_button_style(p, false))
                                            .on_press(Message::SelectRightTab(RightTab::Queue)),
                                    ]
                                    .align_y(Alignment::Center),
                                    button(
                                        row![
                                            self.artwork_widget(
                                                next_item.track.thumbnail_url.as_deref(),
                                                42.0,
                                                false
                                            ),
                                            column![
                                                text(ellipsize(&next_item.track.title, 24))
                                                    .size(13)
                                                    .font(bold())
                                                    .wrapping(Wrapping::None),
                                                text(ellipsize(
                                                    &next_item.track.artist_display(),
                                                    26
                                                ))
                                                .size(12)
                                                .color(muted)
                                                .wrapping(Wrapping::None),
                                            ]
                                            .spacing(2)
                                            .width(Length::Fill),
                                        ]
                                        .spacing(10)
                                        .align_y(Alignment::Center),
                                    )
                                    .width(Length::Fill)
                                    .padding([6, 4])
                                    .style(ghost_row_button_style(p, false))
                                    .on_press(Message::QueueJump(cur_idx + 1)),
                                ]
                                .spacing(8),
                            )
                            .width(Length::Fill)
                            .padding(14)
                            .style(elevated_card_style(p)),
                        );
                    }
                }

                // Compact Shazam Recognition tile
                let listen_label = if self.recognizing {
                    "Listening (8s)..."
                } else {
                    "Identify Song (Shazam)"
                };
                let mut rec_btn = button(
                    row![
                        icons::icon(icons::MIC, 15.0, accent),
                        text(listen_label).size(12).font(bold()),
                    ]
                    .spacing(8)
                    .align_y(Alignment::Center),
                )
                .width(Length::Fill)
                .padding([9, 12])
                .style(outlined_pill_button_style(p, self.recognizing));
                if !self.recognizing {
                    rec_btn = rec_btn.on_press(Message::ListenAndRecognize);
                }
                col = col.push(rec_btn);
                self.vscroll(col)
            }
            RightTab::Top => {
                if self.stats.is_empty() && self.liked_tracks.is_empty() {
                    text("Play tracks to see your top artists & songs here.")
                        .size(12)
                        .color(muted)
                        .into()
                } else {
                    let list = if !self.stats.is_empty() {
                        let mut seen_artists = HashSet::new();
                        self.stats
                            .iter()
                            .filter(|st| {
                                let key = st.track.primary_artist().trim().to_lowercase();
                                key.is_empty() || seen_artists.insert(key)
                            })
                            .take(8)
                            .fold(column![].spacing(4), |col, st| {
                            col.push(
                                button(
                                    row![
                                        self.artwork_widget(
                                            st.track.thumbnail_url.as_deref(),
                                            38.0,
                                            true
                                        ),
                                        column![
                                            text(st.track.primary_artist()).size(13),
                                            text(format!("{} • {} plays", st.track.title, st.play_count))
                                                .size(11)
                                                .color(muted),
                                        ]
                                        .spacing(2)
                                        .width(Length::Fill),
                                    ]
                                    .spacing(10)
                                    .align_y(Alignment::Center),
                                )
                                .width(Length::Fill)
                                .padding([6, 6])
                                .style(ghost_row_button_style(p, false))
                                .on_press(Message::PlayTrack(st.track.clone())),
                            )
                        })
                    } else {
                        self.liked_tracks.iter().take(8).fold(column![].spacing(4), |col, t| {
                            col.push(
                                button(
                                    row![
                                        self.artwork_widget(t.thumbnail_url.as_deref(), 38.0, true),
                                        column![
                                            text(&t.title).size(13),
                                            text(t.artist_display()).size(11).color(muted),
                                        ]
                                        .spacing(2)
                                        .width(Length::Fill),
                                    ]
                                    .spacing(10)
                                    .align_y(Alignment::Center),
                                )
                                .width(Length::Fill)
                                .padding([6, 6])
                                .style(ghost_row_button_style(p, false))
                                .on_press(Message::PlayTrack(t.clone())),
                            )
                        })
                    };
                    self.vscroll(list)
                }
            }
            RightTab::Queue => {
                if self.queue.items.is_empty() {
                    text("Queue is empty.").size(13).color(muted).into()
                } else {
                    let list = self.queue.items.iter().enumerate().fold(
                        column![].spacing(4),
                        |col, (idx, item)| {
                            let is_cur = self.queue.current_index == Some(idx);
                            col.push(
                                row![
                                    button(
                                        row![
                                            self.artwork_widget(
                                                item.track.thumbnail_url.as_deref(),
                                                34.0,
                                                false
                                            ),
                                            column![
                                                text(&item.track.title).size(12).color(if is_cur {
                                                    color_from_rgb(p.primary)
                                                } else {
                                                    color_from_rgb(p.text)
                                                }),
                                                text(item.track.artist_display())
                                                    .size(11)
                                                    .color(muted),
                                            ]
                                            .width(Length::Fill),
                                        ]
                                        .spacing(8)
                                        .align_y(Alignment::Center),
                                    )
                                    .width(Length::Fill)
                                    .padding([5, 6])
                                    .style(ghost_row_button_style(p, is_cur))
                                    .on_press(Message::QueueJump(idx)),
                                    button(icons::icon(icons::CLOSE, 13.0, muted))
                                        .padding(6)
                                        .style(icon_button_style(p, false))
                                        .on_press(Message::QueueRemove(idx)),
                                ]
                                .align_y(Alignment::Center),
                            )
                        },
                    );
                    column![
                        row![
                            text(format!("{} tracks", self.queue.items.len()))
                                .size(11)
                                .color(muted),
                            horizontal_space(),
                            button(text("Clear").size(11).color(muted))
                                .padding([4, 10])
                                .style(top_pill_button_style(p, false))
                                .on_press(Message::QueueClear),
                        ]
                        .align_y(Alignment::Center),
                        self.vscroll(list),
                    ]
                    .spacing(6)
                    .into()
                }
            }
            RightTab::Lyrics => self.view_lyrics_inner(15, 13),
        };

        container(column![header, tabs, top_content].spacing(12))
            .width(self.right_rail_width())
            .height(Length::Fill)
            .padding(14)
            .style(glass_panel_style(p))
            .into()
    }

    fn view_lyrics_inner(&self, active_size: u16, base_size: u16) -> Element<'_, Message> {
        let p = self.palette;
        let muted = color_from_rgb(p.text_muted);

        if self.lyrics_loading {
            return text("Fetching lyrics...").size(base_size).color(muted).into();
        }

        let Some(res) = &self.active_lyrics else {
            return text("No lyrics loaded for the current track.")
                .size(base_size)
                .color(muted)
                .into();
        };

        let rom_btn = button(text("Romanize").size(11))
            .padding([3, 10])
            .style(top_pill_button_style(p, self.settings.romanize_lyrics))
            .on_press(Message::ToggleRomanizeLyrics);

        let is_cinema = active_size >= 24;

        if !res.synced.is_empty() {
            let active_idx = res.active_index(self.position_ms);
            let top_pad = if is_cinema {
                (self.window_size.1 * 0.32).clamp(180.0, 260.0)
            } else {
                110.0
            };
            let bottom_pad = if is_cinema {
                (self.window_size.1 * 0.48).clamp(260.0, 420.0)
            } else {
                180.0
            };

            let mut lines_col = column![vertical_space().height(top_pad)]
                .spacing(if is_cinema { 14 } else { 6 });

            for (i, line) in res.synced.iter().enumerate() {
                let is_active = active_idx == Some(i);
                let dist = active_idx.map(|a| a.abs_diff(i)).unwrap_or(99);
                let is_past = active_idx.map(|a| i < a).unwrap_or(false);
                let label = if line.text.is_empty() {
                    "•  •  •"
                } else {
                    &line.text
                };
                let color = if is_cinema {
                    if is_active {
                        Color::WHITE
                    } else if dist == 1 && !is_past {
                        Color::from_rgba(1.0, 1.0, 1.0, 0.72)
                    } else if is_past {
                        Color::from_rgba(1.0, 1.0, 1.0, 0.26)
                    } else {
                        let fade = (0.58 - (dist.saturating_sub(1) as f32) * 0.08).max(0.26);
                        Color::from_rgba(1.0, 1.0, 1.0, fade)
                    }
                } else if is_active {
                    color_from_rgb(p.primary)
                } else if dist == 1 && !is_past {
                    Color::from_rgba(1.0, 1.0, 1.0, 0.80)
                } else {
                    muted
                };
                let line_body: Element<'_, Message> =
                    if is_cinema && is_active && !line.words.is_empty() {
                        let mut spans = Vec::with_capacity(line.words.len() * 2);
                        for (wi, w) in line.words.iter().enumerate() {
                            let sung = self.position_ms >= w.time_ms;
                            let w_color = if sung {
                                Color::WHITE
                            } else {
                                Color::from_rgba(1.0, 1.0, 1.0, 0.40)
                            };
                            let piece = if w.ends_with_space && wi + 1 < line.words.len() {
                                format!("{} ", w.text)
                            } else {
                                w.text.clone()
                            };
                            spans.push(span::<Message, Font>(piece).color(w_color));
                        }
                        rich_text(spans).size(active_size).font(bold()).into()
                    } else {
                        let mut txt = text(label)
                            .size(if is_active {
                                active_size
                            } else if is_cinema && dist == 1 && !is_past {
                                base_size + 2
                            } else {
                                base_size
                            })
                            .color(color);
                        if is_active {
                            txt = txt.font(bold());
                        } else if is_cinema && dist == 1 && !is_past {
                            txt = txt.font(semibold());
                        }
                        txt.into()
                    };
                let mut line_col = column![line_body].spacing(2);
                if self.settings.romanize_lyrics && contains_cjk_or_hangul_or_kana(&line.text) {
                    line_col = line_col.push(
                        text(romanize_text(&line.text))
                            .size((base_size.saturating_sub(2)).max(11))
                            .color(with_alpha(color, 0.72)),
                    );
                }
                lines_col = lines_col.push(
                    button(line_col)
                        .width(Length::Fill)
                        .padding(if is_cinema { [10, 16] } else { [4, 6] })
                        .style(ghost_row_button_style(p, is_active && !is_cinema))
                        .on_press(Message::SeekToMs(line.time_ms)),
                );
            }

            lines_col = lines_col.push(vertical_space().height(bottom_pad));

            column![
                row![
                    text(format!("Synced Lyrics • {}", res.source.label()))
                        .size(12)
                        .color(if is_cinema {
                            Color::from_rgba(1.0, 1.0, 1.0, 0.65)
                        } else {
                            muted
                        }),
                    horizontal_space(),
                    rom_btn,
                ]
                .align_y(Alignment::Center),
                scrollable(lines_col)
                    .id(self.lyrics_scroll_id.clone())
                    .direction(scrollable::Direction::Vertical(
                        scrollable::Scrollbar::new()
                            .width(6)
                            .scroller_width(4)
                            .margin(2),
                    ))
                    .style(scroll_style(p)),
            ]
            .spacing(10)
            .into()
        } else if let Some(plain) = &res.plain {
            let body_text = if self.settings.romanize_lyrics && contains_cjk_or_hangul_or_kana(plain) {
                romanize_text(plain)
            } else {
                plain.clone()
            };
            column![
                row![
                    text(format!("Lyrics • {}", res.source.label()))
                        .size(12)
                        .color(muted),
                    horizontal_space(),
                    rom_btn,
                ]
                .align_y(Alignment::Center),
                self.vscroll(
                    container(text(body_text).size(base_size).color(Color::from_rgba(1.0, 1.0, 1.0, 0.85)))
                        .padding(if is_cinema { [16, 12] } else { [4, 2] })
                ),
            ]
            .spacing(10)
            .into()
        } else {
            text("No lyrics available.").size(base_size).color(muted).into()
        }
    }

    fn view_player_dock(&self) -> Element<'_, Message> {
        let p = self.palette;
        let muted = color_from_rgb(p.text_muted);
        let accent = color_from_rgb(p.primary);

        let (title, artist, thumb, liked) = match self.queue.current() {
            Some(c) => {
                let is_liked =
                    self.liked_tracks.iter().any(|l| l.id == c.track.id) || c.track.liked;
                (
                    c.track.title.clone(),
                    c.track.artist_display(),
                    c.track.thumbnail_url.clone(),
                    is_liked,
                )
            }
            None => (
                "Nothing playing".to_string(),
                "Pick something to listen to".to_string(),
                None,
                false,
            ),
        };

        let is_playing = matches!(self.status, PlaybackStatus::Playing);
        let loading = matches!(self.status, PlaybackStatus::Loading);
        let play_icon = icons::icon(
            if is_playing { icons::PAUSE } else { icons::PLAY },
            16.0,
            Color::BLACK,
        );

        let heart_icon = if liked {
            icons::icon(icons::HEART_FILLED, 18.0, accent)
        } else {
            icons::icon(icons::HEART_OUTLINE, 18.0, muted)
        };

        let pos_display = self.seeking_ms.unwrap_or(self.position_ms);
        let dur_max = self.duration_ms.max(1) as f32;
        let seek_slider = slider(
            0.0..=dur_max,
            pos_display.min(self.duration_ms.max(1)) as f32,
            Message::SeekChanged,
        )
        .style(slim_slider_style(Color::WHITE, accent))
        .on_release(Message::SeekReleased)
        .width(Length::Fill);

        let boosted = self.settings.volume > 1.01;
        let vol_fill = if boosted { accent } else { Color::WHITE };
        let vol_pct = (self.settings.volume * 100.0).round() as u32;

        let now_playing = if let Some(cur) = self.queue.current() {
            let t = cur.track.clone();
            row![
                self.with_tooltip(
                    button(
                        container(self.artwork_widget(thumb.as_deref(), 52.0, false))
                            .style(artwork_frame_style(8.0, 0.42)),
                    )
                    .padding(0)
                    .style(ghost_row_button_style(p, false))
                    .on_press(Message::ToggleExpandedPlayer),
                    "Open Cinema Stage",
                    tooltip::Position::Top,
                ),
                column![
                    button(
                        container(
                            text(title)
                                .size(14)
                                .font(semibold())
                                .wrapping(Wrapping::None),
                        )
                        .clip(true),
                    )
                    .padding([1, 2])
                    .style(ghost_row_button_style(p, false))
                    .on_press(Message::ToggleExpandedPlayer),
                    button(
                        container(
                            text(artist)
                                .size(12)
                                .wrapping(Wrapping::None)
                                .color(muted),
                        )
                        .clip(true),
                    )
                    .padding([1, 2])
                    .style(ghost_row_button_style(p, false))
                    .on_press(Message::OpenTrackArtist(t)),
                ]
                .spacing(2)
                .max_width(210),
                self.with_tooltip(
                    button(heart_icon)
                        .padding(7)
                        .style(icon_button_style(p, false))
                        .on_press(Message::ToggleLikeCurrent),
                    if liked { "Remove from Liked" } else { "Save to Liked Songs" },
                    tooltip::Position::Top,
                ),
            ]
            .spacing(10)
            .align_y(Alignment::Center)
        } else {
            row![
                self.artwork_widget(None, 52.0, false),
                column![
                    text(title).size(14).font(semibold()).wrapping(Wrapping::None),
                    text(artist).size(12).wrapping(Wrapping::None).color(muted),
                ]
                .spacing(3)
                .max_width(210)
                .clip(true),
            ]
            .spacing(12)
            .align_y(Alignment::Center)
        };

        let repeat_active = self.queue.repeat_mode != RepeatMode::Off;
        let repeat_btn_content: Element<'_, Message> = if self.queue.repeat_mode == RepeatMode::One {
            row![
                icons::icon(icons::REPEAT, 18.0, accent),
                text("1").size(10).font(bold()).color(accent),
            ]
            .spacing(2)
            .align_y(Alignment::Center)
            .into()
        } else {
            icons::icon(
                icons::REPEAT,
                18.0,
                if repeat_active { accent } else { muted },
            )
        };
        let controls = row![
            self.with_tooltip(
                button(icons::icon(
                    icons::SHUFFLE,
                    16.0,
                    if self.queue.shuffle { accent } else { muted }
                ))
                .padding(7)
                .style(icon_button_style(p, false))
                .on_press(Message::ToggleShuffle),
                if self.queue.shuffle { "Shuffle: On" } else { "Shuffle: Off" },
                tooltip::Position::Top,
            ),
            button(icons::icon(icons::PREV, 17.0, Color::WHITE))
                .padding(7)
                .style(icon_button_style(p, false))
                .on_press(Message::PrevTrack),
            button(
                container(play_icon)
                    .width(36)
                    .height(36)
                    .align_x(iced::alignment::Horizontal::Center)
                    .align_y(iced::alignment::Vertical::Center),
            )
            .padding(0)
            .style(dock_play_button_style(p))
            .on_press(Message::TogglePlayPause),
            button(icons::icon(icons::NEXT, 17.0, Color::WHITE))
                .padding(7)
                .style(icon_button_style(p, false))
                .on_press(Message::NextTrack),
            self.with_tooltip(
                button(repeat_btn_content)
                    .padding(7)
                    .style(icon_button_style(p, false))
                    .on_press(Message::ToggleRepeat),
                match self.queue.repeat_mode {
                    RepeatMode::Off => "Repeat: Off",
                    RepeatMode::All => "Repeat: All",
                    RepeatMode::One => "Repeat: One",
                },
                tooltip::Position::Top,
            ),
        ]
        .spacing(8)
        .align_y(Alignment::Center);

        let timeline = row![
            text(if loading {
                "Loading".to_string()
            } else {
                format_ms(pos_display)
            })
            .size(11)
            .font(if self.seeking_ms.is_some() { bold() } else { mono() })
            .color(if self.seeking_ms.is_some() { accent } else { muted })
            .width(42)
            .align_x(iced::alignment::Horizontal::Right),
            seek_slider,
            text(format_ms(self.duration_ms))
                .size(11)
                .font(mono())
                .color(muted)
                .width(42),
        ]
        .spacing(10)
        .align_y(Alignment::Center);

        let center = column![controls, timeline]
            .spacing(4)
            .max_width(580)
            .align_x(Alignment::Center);

        let rail_toggle = |icon: &'static [u8], tab: RightTab, tip: &'static str| -> Element<'_, Message> {
            let on = self.show_right_rail && self.right_tab == tab;
            self.with_tooltip(
                button(icons::icon(icon, 18.0, if on { accent } else { muted }))
                    .padding(8)
                    .style(icon_button_style(p, false))
                    .on_press(Message::ToggleRightTab(tab)),
                tip,
                tooltip::Position::Top,
            )
        };

        let sleep_label: Option<String> = match self.sleep_timer {
            SleepTimerState::Off => None,
            SleepTimerState::EndOfSong => Some("End".to_string()),
            SleepTimerState::Countdown(deadline) => {
                let secs = deadline.saturating_duration_since(Instant::now()).as_secs();
                Some(format!("{}:{:02}", secs / 60, secs % 60))
            }
        };
        let sleep_active = sleep_label.is_some();
        let sleep_btn: Element<'_, Message> = if let Some(lbl) = sleep_label {
            button(
                row![
                    icons::icon(icons::CLOCK, 16.0, accent),
                    text(lbl).size(12).font(mono()).color(accent),
                ]
                .spacing(5)
                .align_y(Alignment::Center),
            )
            .padding([6, 10])
            .style(top_pill_button_style(p, false))
            .on_press(Message::CycleSleepTimer)
            .into()
        } else {
            self.with_tooltip(
                button(icons::icon(icons::CLOCK, 18.0, muted))
                    .padding(8)
                    .style(icon_button_style(p, sleep_active))
                    .on_press(Message::CycleSleepTimer),
                "Sleep Timer",
                tooltip::Position::Top,
            )
        };

        let muted_audio = self.settings.volume <= 0.01;
        let mut right = row![
            sleep_btn,
            rail_toggle(icons::LYRICS, RightTab::Lyrics, "Synced Lyrics"),
            rail_toggle(icons::QUEUE, RightTab::Queue, "Play Queue"),
            horizontal_space().width(2),
            container(horizontal_space())
                .width(1)
                .height(16)
                .style(divider_style(0.08)),
            horizontal_space().width(2),
            self.with_tooltip(
                button(icons::icon(
                    if muted_audio {
                        icons::VOLUME_MUTED
                    } else {
                        icons::VOLUME
                    },
                    17.0,
                    if muted_audio {
                        color_from_rgb(p.danger)
                    } else if boosted {
                        accent
                    } else {
                        muted
                    }
                ))
                .padding(7)
                .style(icon_button_style(p, false))
                .on_press(Message::ToggleMute),
                "Mute / Unmute (M)",
                tooltip::Position::Top,
            ),
            slider(0.0..=4.0, self.settings.volume, Message::VolumeChanged)
                .step(0.05_f32)
                .style(slim_slider_style(vol_fill, accent))
                .width(86),
        ]
        .spacing(4)
        .align_y(Alignment::Center);
        if boosted {
            right = right.push(
                button(text(format!("+{vol_pct}%")).size(11).font(mono()).color(accent))
                    .padding([4, 8])
                    .style(soft_pill_button_style(p))
                    .on_press(Message::VolumeChanged(1.0)),
            );
        }
        let dsp_label = if self.settings.spatial_mode != SpatialMode::Off {
            Some("Spatial")
        } else if self.settings.eq_preset != EqPreset::Flat {
            Some("EQ")
        } else if self.settings.audio_normalization {
            Some("-3 dB")
        } else {
            None
        };
        if let Some(badge) = dsp_label {
            right = right.push(self.with_tooltip(
                button(text(badge).size(10).font(mono()))
                    .padding([4, 9])
                    .style(soft_pill_button_style(p))
                    .on_press(Message::SelectTab(NavTab::Settings)),
                "Audio & DSP Settings",
                tooltip::Position::Top,
            ));
        }
        right = right
            .push(horizontal_space().width(2))
            .push(
                container(horizontal_space())
                    .width(1)
                    .height(16)
                    .style(divider_style(0.08)),
            )
            .push(horizontal_space().width(2))
            .push(self.with_tooltip(
                button(icons::icon(icons::MINI_PLAYER, 16.0, muted))
                    .padding(7)
                    .style(icon_button_style(p, false))
                    .on_press(Message::ToggleMiniPlayer),
                "Always-on-Top Mini Player",
                tooltip::Position::Top,
            ))
            .push(self.with_tooltip(
                button(icons::icon(
                    icons::EXPAND,
                    17.0,
                    if self.show_expanded { accent } else { muted },
                ))
                .padding(7)
                .style(icon_button_style(p, false))
                .on_press(Message::ToggleExpandedPlayer),
                "Cinema Lyrics Stage",
                tooltip::Position::Top,
            ));

        let dock_content = row![
            container(now_playing).width(Length::Shrink),
            container(center)
                .width(Length::Fill)
                .align_x(iced::alignment::Horizontal::Center),
            container(right)
                .width(Length::Shrink)
                .align_x(iced::alignment::Horizontal::Right),
        ]
        .spacing(16)
        .align_y(Alignment::Center);

        container(
            column![
                container(horizontal_space())
                    .width(Length::Fill)
                    .height(1)
                    .style(divider_style(0.06)),
                container(dock_content)
                    .padding(iced::Padding {
                        top: 8.0,
                        right: 18.0,
                        bottom: 10.0,
                        left: 18.0,
                    })
                    .width(Length::Fill),
            ]
        )
        .width(Length::Fill)
        .style(player_dock_style(p))
        .into()
    }



    fn artwork_widget(&self, url: Option<&str>, size: f32, circle: bool) -> Element<'_, Message> {
        if let Some(u) = url {
            let map = if circle {
                &self.circle_covers
            } else {
                &self.rounded_covers
            };
            if let Some(handle) = map.get(u) {
                return image(handle.clone())
                    .width(size)
                    .height(size)
                    .content_fit(iced::ContentFit::Cover)
                    .into();
            }
        }
        container(icons::icon(
            icons::LYRICS,
            (size * 0.42).max(14.0),
            color_from_rgb(self.palette.text_muted),
        ))
        .width(size)
        .height(size)
        .align_x(iced::alignment::Horizontal::Center)
        .align_y(iced::alignment::Vertical::Center)
        .style(move |_| iced::widget::container::Style {
            background: Some(iced::Background::Color(with_alpha(
                color_from_rgb(self.palette.surface_elevated),
                0.9,
            ))),
            border: iced::Border {
                radius: (if circle { size / 2.0 } else { 10.0 }).into(),
                ..Default::default()
            },
            ..Default::default()
        })
        .into()
    }

    fn item_action(item: &BrowseItem) -> Message {
        match item {
            BrowseItem::Track(t) => Message::PlayTrack(t.clone()),
            BrowseItem::Playlist { id, title, .. } => Message::OpenPlaylist {
                id: id.clone(),
                title: title.clone(),
            },
            BrowseItem::Album {
                browse_id, title, ..
            } => Message::OpenAlbum {
                browse_id: browse_id.clone(),
                title: title.clone(),
            },
            BrowseItem::Artist {
                browse_id, name, ..
            } => Message::OpenArtist {
                browse_id: browse_id.clone(),
                name: name.clone(),
            },
        }
    }

    /// The compact frosted-glass tiles in the greeting bento grid at the top of Home.
    fn quick_tile<'a>(&'a self, item: &'a BrowseItem) -> Element<'a, Message> {
        let p = self.palette;
        let muted = color_from_rgb(p.text_muted);
        let key = format!("tile:{}", item.stable_key());
        let hovered = self.hovered_card.as_deref() == Some(key.as_str());
        let is_playing = matches!(
            item,
            BrowseItem::Track(t) if self.queue.current().is_some_and(|c| c.track.id == t.id)
        );
        let raw_sub = item.subtitle();
        let subtitle = if raw_sub.trim().is_empty() {
            match item {
                BrowseItem::Track(_) => "Song".to_string(),
                BrowseItem::Album { .. } => "Album".to_string(),
                BrowseItem::Artist { .. } => "Artist".to_string(),
                BrowseItem::Playlist { .. } => "Playlist".to_string(),
            }
        } else {
            ellipsize(&raw_sub, 28)
        };
        let title_color = if is_playing {
            color_from_rgb(p.primary)
        } else {
            color_from_rgb(p.text)
        };
        let badge_active = hovered || is_playing;
        let badge_icon = if is_playing { icons::EQ_BARS } else { icons::PLAY };
        let badge_fg = if badge_active {
            Color::BLACK
        } else {
            with_alpha(Color::WHITE, 0.72)
        };

        let tile_row = row![
            container(self.artwork_widget(
                item.thumbnail_url(),
                52.0,
                matches!(item, BrowseItem::Artist { .. }),
            ))
            .padding(iced::Padding {
                top: 8.0,
                right: 0.0,
                bottom: 8.0,
                left: 8.0,
            }),
            container(
                column![
                    text(ellipsize(item.title(), 26))
                        .size(13.5)
                        .font(semibold())
                        .color(title_color)
                        .wrapping(Wrapping::None),
                    text(subtitle)
                        .size(11.5)
                        .color(muted)
                        .wrapping(Wrapping::None),
                ]
                .spacing(2),
            )
            .width(Length::Fill)
            .padding([0, 8]),
            container(
                container(icons::icon(
                    badge_icon,
                    if is_playing { 13.0 } else { 11.5 },
                    badge_fg,
                ))
                .width(30)
                .height(30)
                .align_x(iced::alignment::Horizontal::Center)
                .align_y(iced::alignment::Vertical::Center)
                .style(quick_tile_play_badge_style(p, badge_active)),
            )
            .padding(iced::Padding {
                top: 0.0,
                right: 12.0,
                bottom: 0.0,
                left: 2.0,
            }),
        ]
        .spacing(4)
        .align_y(Alignment::Center);

        let btn = button(tile_row)
            .width(Length::Fill)
            .height(Length::Fixed(68.0))
            .padding(0)
            .style(quick_tile_button_style(p, is_playing))
            .on_press(Self::item_action(item));

        mouse_area(btn)
            .on_enter(Message::CardHovered(Some(key)))
            .on_exit(Message::CardHovered(None))
            .into()
    }

    fn browse_card<'a>(&'a self, item: &'a BrowseItem) -> Element<'a, Message> {
        let p = self.palette;
        let muted = color_from_rgb(p.text_muted);
        let is_artist = matches!(item, BrowseItem::Artist { .. });
        let key = item.stable_key();
        let hovered = self.hovered_card.as_deref() == Some(key.as_str());

        let art_base = self.artwork_widget(item.thumbnail_url(), 148.0, is_artist);
        let cover_slot: Element<'_, Message> = if hovered {
            let scrim = container(horizontal_space())
                .width(148)
                .height(148)
                .style(card_hover_scrim_style(if is_artist { 74.0 } else { 10.0 }));
            let play_fab = container(
                container(icons::icon(icons::PLAY, 16.0, Color::BLACK))
                    .width(38)
                    .height(38)
                    .align_x(iced::alignment::Horizontal::Center)
                    .align_y(iced::alignment::Vertical::Center)
                    .style(card_play_fab_style(p)),
            )
            .width(148)
            .height(148)
            .padding(10)
            .align_x(iced::alignment::Horizontal::Right)
            .align_y(iced::alignment::Vertical::Bottom);
            stack![art_base, scrim, play_fab].into()
        } else {
            art_base
        };

        let card = button(
            column![
                cover_slot,
                column![
                    container(
                        text(ellipsize(item.title(), 34))
                            .size(13)
                            .font(semibold()),
                    )
                    .width(Length::Fill)
                    .height(Length::Fixed(36.0))
                    .clip(true),
                    text(ellipsize(&item.subtitle(), 26))
                        .size(12)
                        .color(muted)
                        .wrapping(Wrapping::None),
                ]
                .spacing(4),
            ]
            .spacing(10)
            .width(148),
        )
        .width(172)
        .height(Length::Fixed(246.0))
        .padding(12)
        .style(card_button_style(p))
        .on_press(Self::item_action(item));

        let ma = mouse_area(card)
            .on_enter(Message::CardHovered(Some(key)))
            .on_exit(Message::CardHovered(None));
        if let BrowseItem::Track(t) = item {
            ma.on_right_press(Message::OpenTrackMenu(t.clone())).into()
        } else {
            ma.into()
        }
    }



    fn browse_item_row<'a>(&'a self, num: usize, item: &'a BrowseItem) -> Element<'a, Message> {
        let p = self.palette;
        let muted = color_from_rgb(p.text_muted);
        match item {
            BrowseItem::Track(t) => self.table_track_row(num, t, None),
            BrowseItem::Playlist {
                id,
                title,
                author,
                thumbnail_url,
            } => button(
                row![
                    text(num.to_string()).size(14).color(muted).width(32),
                    self.artwork_widget(thumbnail_url.as_deref(), 42.0, false),
                    column![
                        container(
                            text(title).size(15).font(bold()).wrapping(Wrapping::None),
                        )
                        .width(Length::Fill)
                        .clip(true),
                        text(format!("Playlist • {author}"))
                            .size(13)
                            .color(muted)
                            .wrapping(Wrapping::None),
                    ]
                    .spacing(3)
                    .width(Length::Fill),
                    container(text("Playlist").size(11).color(muted))
                        .padding([4, 10])
                        .style(local_badge_style()),
                    icons::icon(icons::FORWARD, 14.0, muted),
                ]
                .spacing(12)
                .align_y(Alignment::Center),
            )
            .width(Length::Fill)
            .padding([8, 12])
            .style(ghost_row_button_style(p, false))
            .on_press(Message::OpenPlaylist {
                id: id.clone(),
                title: title.clone(),
            })
            .into(),
            BrowseItem::Album {
                browse_id,
                title,
                artist,
                thumbnail_url,
                ..
            } => button(
                row![
                    text(num.to_string()).size(14).color(muted).width(32),
                    self.artwork_widget(thumbnail_url.as_deref(), 42.0, false),
                    column![
                        container(
                            text(title).size(15).font(bold()).wrapping(Wrapping::None),
                        )
                        .width(Length::Fill)
                        .clip(true),
                        text(format!("Album • {artist}"))
                            .size(13)
                            .color(muted)
                            .wrapping(Wrapping::None),
                    ]
                    .spacing(3)
                    .width(Length::Fill),
                    container(text("Album").size(11).color(muted))
                        .padding([4, 10])
                        .style(local_badge_style()),
                    icons::icon(icons::FORWARD, 14.0, muted),
                ]
                .spacing(12)
                .align_y(Alignment::Center),
            )
            .width(Length::Fill)
            .padding([8, 12])
            .style(ghost_row_button_style(p, false))
            .on_press(Message::OpenAlbum {
                browse_id: browse_id.clone(),
                title: title.clone(),
            })
            .into(),
            BrowseItem::Artist {
                browse_id,
                name,
                thumbnail_url,
            } => button(
                row![
                    text(num.to_string()).size(14).color(muted).width(32),
                    self.artwork_widget(thumbnail_url.as_deref(), 42.0, true),
                    column![
                        container(
                            text(name).size(15).font(bold()).wrapping(Wrapping::None),
                        )
                        .width(Length::Fill)
                        .clip(true),
                        text("Artist").size(13).color(muted),
                    ]
                    .spacing(3)
                    .width(Length::Fill),
                    container(text("Artist").size(11).color(muted))
                        .padding([4, 10])
                        .style(local_badge_style()),
                    icons::icon(icons::FORWARD, 14.0, muted),
                ]
                .spacing(12)
                .align_y(Alignment::Center),
            )
            .width(Length::Fill)
            .padding([8, 12])
            .style(ghost_row_button_style(p, false))
            .on_press(Message::OpenArtist {
                browse_id: browse_id.clone(),
                name: name.clone(),
            })
            .into(),
        }
    }

    fn table_track_row<'a>(
        &'a self,
        num: usize,
        track: &'a Track,
        play_list: Option<(Vec<Track>, usize)>,
    ) -> Element<'a, Message> {
        let p = self.palette;
        let muted = color_from_rgb(p.text_muted);
        let accent = color_from_rgb(p.primary);
        let is_current = self
            .queue
            .current()
            .map(|c| c.track.id == track.id)
            .unwrap_or(false);
        let row_key = format!("{num}:{}", track.id);
        let row_hovered = self.hovered_row.as_deref() == Some(row_key.as_str());

        let play_msg = match play_list {
            Some((list, idx)) => Message::PlayTracks(list, idx),
            None => Message::PlayTrack(track.clone()),
        };

        let dur = track
            .duration_secs
            .map(|d| format!("{}:{:02}", d / 60, d % 60))
            .unwrap_or_else(|| "–".to_string());
        let album_title = track.album.as_ref().map(|a| a.title.as_str()).unwrap_or("");
        let is_liked = self.liked_tracks.iter().any(|t| t.id == track.id) || track.liked;

        let is_playing_current = is_current && matches!(self.status, PlaybackStatus::Playing);
        let index_cell: Element<'_, Message> = if row_hovered {
            container(icons::icon(
                if is_playing_current {
                    icons::PAUSE
                } else {
                    icons::PLAY
                },
                14.0,
                if is_current { accent } else { Color::WHITE },
            ))
            .width(32)
            .align_x(iced::alignment::Horizontal::Center)
            .into()
        } else if is_current {
            container(self.live_eq_bars(is_playing_current, accent))
                .width(32)
                .align_x(iced::alignment::Horizontal::Center)
                .into()
        } else {
            text(num.to_string())
                .size(13)
                .font(mono())
                .color(muted)
                .width(32)
                .into()
        };

        let title_col = container(
            column![
                text(ellipsize(&track.title, 48))
                    .size(14)
                    .font(if is_current { semibold() } else { Font::DEFAULT })
                    .wrapping(Wrapping::None)
                    .color(if is_current { accent } else { Color::WHITE }),
                text(ellipsize(&track.artist_display(), 52))
                    .size(12)
                    .wrapping(Wrapping::None)
                    .color(muted),
            ]
            .spacing(2),
        )
        .width(Length::Fill)
        .clip(true);

        let mut main_cols = row![
            index_cell,
            row![
                self.artwork_widget(track.thumbnail_url.as_deref(), 40.0, false),
                title_col,
            ]
            .spacing(12)
            .width(Length::FillPortion(5))
            .align_y(Alignment::Center),
        ]
        .spacing(12)
        .align_y(Alignment::Center);
        if self.show_album_column() {
            main_cols = main_cols.push(
                container(
                    text(ellipsize(album_title, 34))
                        .size(13)
                        .wrapping(Wrapping::None)
                        .color(muted),
                )
                .width(Length::FillPortion(3))
                .clip(true),
            );
        }
        main_cols = main_cols.push(
            text(dur)
                .size(12)
                .font(mono())
                .color(muted)
                .width(48)
                .align_x(iced::alignment::Horizontal::Right),
        );

        let play_action = if is_current && matches!(self.status, PlaybackStatus::Playing | PlaybackStatus::Paused) {
            Message::TogglePlayPause
        } else {
            play_msg
        };
        let main_btn = button(main_cols)
            .width(Length::Fill)
            .padding([8, 12])
            .style(ghost_row_button_style(p, is_current))
            .on_press(play_action);

        let subtle = with_alpha(muted, if row_hovered { 0.95 } else { 0.45 });
        let mut actions = row![
            button(if is_liked {
                icons::icon(icons::HEART_FILLED, 16.0, accent)
            } else {
                icons::icon(icons::HEART_OUTLINE, 16.0, subtle)
            })
            .padding(6)
            .style(icon_button_style(p, false))
            .on_press(Message::ToggleLikeTrack(track.clone())),
            button(icons::icon(icons::PLUS, 15.0, subtle))
                .padding(6)
                .style(icon_button_style(p, false))
                .on_press(Message::QueueAddBack(track.clone())),
        ]
        .spacing(0)
        .align_y(Alignment::Center);
        if !track.is_local {
            let is_downloaded = self.downloaded_id_set.contains(&track.id);
            let is_downloading = self.downloading_ids.contains(&track.id);
            let dl_color = if is_downloaded || is_downloading {
                accent
            } else {
                subtle
            };
            let dl_msg = if is_downloaded {
                Message::RemoveDownload(track.id.clone())
            } else {
                Message::DownloadTrack(track.clone())
            };
            actions = actions.push(
                button(icons::icon(icons::DOWNLOAD, 15.0, dl_color))
                    .padding(6)
                    .style(icon_button_style(p, false))
                    .on_press(dl_msg),
            );
        }
        if self.tab == NavTab::Playlists {
            if let LibraryTab::Playlist(idx) = self.library_tab {
                if let Some(pl) = self.playlists.get(idx) {
                    actions = actions.push(
                        button(icons::icon(icons::CLOSE, 14.0, subtle))
                            .padding(6)
                            .style(icon_button_style(p, false))
                            .on_press(Message::RemoveFromPlaylist {
                                playlist_id: pl.id.clone(),
                                song_id: track.id.clone(),
                            }),
                    );
                }
            }
        }
        actions = actions.push(
            button(icons::icon(icons::MORE_HORIZ, 15.0, subtle))
                .padding(6)
                .style(icon_button_style(p, false))
                .on_press(Message::OpenTrackMenu(track.clone())),
        );

        let actions_slot = container(actions)
            .width(Length::Fixed(132.0))
            .align_x(iced::alignment::Horizontal::Right)
            .align_y(iced::alignment::Vertical::Center);

        mouse_area(
            row![main_btn, actions_slot]
                .spacing(0)
                .align_y(Alignment::Center),
        )
        .on_enter(Message::RowHovered(Some(row_key)))
        .on_exit(Message::RowHovered(None))
        .on_right_press(Message::OpenTrackMenu(track.clone()))
        .into()
    }

    fn with_tooltip<'a>(
        &'a self,
        content: impl Into<Element<'a, Message>>,
        label: impl iced::widget::text::IntoFragment<'a>,
        pos: tooltip::Position,
    ) -> Element<'a, Message> {
        let p = self.palette;
        tooltip(
            content,
            container(text(label).size(12).font(semibold()).color(Color::WHITE))
                .padding([5, 10])
                .style(tooltip_style(p)),
            pos,
        )
        .gap(6)
        .snap_within_viewport(true)
        .into()
    }

    fn live_eq_bars(&self, playing: bool, color: Color) -> Element<'_, Message> {
        let band_idx = [1usize, 3, 5];
        let mut bars = row![].spacing(2).height(14).align_y(Alignment::End);
        for (i, &bi) in band_idx.iter().enumerate() {
            let amp = if playing {
                let real = self.spectrum_bands[bi];
                let gentle_idle =
                    0.22 + 0.18 * ((self.anim_phase * 1.4 + i as f32 * 1.9).sin() * 0.5 + 0.5);
                real.max(gentle_idle).clamp(0.18, 1.0)
            } else {
                [0.42, 0.72, 0.35][i]
            };
            let h = (14.0 * amp).max(3.0);
            bars = bars.push(
                container(horizontal_space())
                    .width(3.0)
                    .height(h)
                    .style(move |_| container::Style {
                        background: Some(iced::Background::Color(color)),
                        border: iced::Border {
                            radius: 1.5.into(),
                            ..Default::default()
                        },
                        ..Default::default()
                    }),
            );
        }
        bars.into()
    }

    fn live_spectrum_strip(&self, accent: Color) -> Element<'_, Message> {
        let playing = matches!(self.status, PlaybackStatus::Playing);
        let mut bars = row![].spacing(5).height(26).align_y(Alignment::End);
        for (i, &level) in self.spectrum_bands.iter().enumerate() {
            let amp = if playing {
                let floor = 0.12 + 0.08 * ((self.anim_phase + i as f32 * 0.8).sin() * 0.5 + 0.5);
                level.max(floor).clamp(0.10, 1.0)
            } else {
                0.12
            };
            let bar_color = with_alpha(accent, 0.40 + 0.55 * amp);
            bars = bars.push(
                container(horizontal_space())
                    .width(Length::Fill)
                    .height((24.0 * amp).max(3.0))
                    .style(move |_| container::Style {
                        background: Some(iced::Background::Color(bar_color)),
                        border: iced::Border {
                            radius: 2.5.into(),
                            ..Default::default()
                        },
                        ..Default::default()
                    }),
            );
        }
        container(bars)
            .width(Length::Fill)
            .padding([2, 4])
            .into()
    }

    fn view_mini_queue(&self) -> Element<'_, Message> {
        let p = self.palette;
        let muted = color_from_rgb(p.text_muted);
        if self.queue.items.is_empty() {
            return container(text("Queue is empty.").size(12).color(muted))
                .padding(20)
                .width(Length::Fill)
                .align_x(iced::alignment::Horizontal::Center)
                .into();
        }
        let list = self.queue.items.iter().enumerate().fold(
            column![].spacing(3),
            |col, (idx, item)| {
                let is_cur = self.queue.current_index == Some(idx);
                col.push(
                    row![
                        button(
                            row![
                                container(self.artwork_widget(
                                    item.track.thumbnail_url.as_deref(),
                                    32.0,
                                    false,
                                ))
                                .style(artwork_frame_style(6.0, 0.35)),
                                column![
                                    container(
                                        text(&item.track.title)
                                            .size(12)
                                            .font(if is_cur { bold() } else { semibold() })
                                            .color(if is_cur {
                                                color_from_rgb(p.primary)
                                            } else {
                                                Color::WHITE
                                            })
                                            .wrapping(Wrapping::None),
                                    )
                                    .width(Length::Fill)
                                    .clip(true),
                                    container(
                                        text(item.track.artist_display())
                                            .size(11)
                                            .color(muted)
                                            .wrapping(Wrapping::None),
                                    )
                                    .width(Length::Fill)
                                    .clip(true),
                                ]
                                .spacing(1)
                                .width(Length::Fill),
                            ]
                            .spacing(8)
                            .align_y(Alignment::Center),
                        )
                        .width(Length::Fill)
                        .padding([4, 6])
                        .style(ghost_row_button_style(p, is_cur))
                        .on_press(Message::QueueJump(idx)),
                        button(icons::icon(icons::CLOSE, 12.0, muted))
                            .padding(5)
                            .style(icon_button_style(p, false))
                            .on_press(Message::QueueRemove(idx)),
                    ]
                    .spacing(4)
                    .align_y(Alignment::Center),
                )
            },
        );
        self.vscroll(list)
    }

    fn view_mini_player(&self) -> Element<'_, Message> {
        let p = self.palette;
        let muted = color_from_rgb(p.text_muted);
        let accent = color_from_rgb(p.primary);
        let is_playing = matches!(self.status, PlaybackStatus::Playing);

        let (title, artist, thumb, liked, cur_track) = match self.queue.current() {
            Some(c) => {
                let is_liked =
                    self.liked_tracks.iter().any(|l| l.id == c.track.id) || c.track.liked;
                (
                    c.track.title.clone(),
                    c.track.artist_display(),
                    c.track.thumbnail_url.clone(),
                    is_liked,
                    Some(c.track.clone()),
                )
            }
            None => (
                "Nothing playing".to_string(),
                "Select a track to listen".to_string(),
                None,
                false,
                None,
            ),
        };

        let loudness_text = match cur_track.as_ref().and_then(|t| t.loudness_db) {
            Some(db) => format!("{db:+.1} dB LUFS"),
            None if cur_track.as_ref().map(|t| t.is_local).unwrap_or(false) => "Local Audio".to_string(),
            None => "Auto Leveling".to_string(),
        };

        let quality_badge_label = if cur_track.as_ref().map(|t| t.is_local).unwrap_or(false) {
            "Local Lossless"
        } else {
            match self.settings.audio_quality {
                AudioQuality::Max => "Hi-Res 24-bit",
                AudioQuality::High => "Lossless 48kHz",
                AudioQuality::Auto => "Adaptive HQ",
                AudioQuality::Low => "Data Saver",
            }
        };

        let pos_display = self.seeking_ms.unwrap_or(self.position_ms);
        let dur_max = self.duration_ms.max(1) as f32;

        let make_timeline = |dur_ms: u64, pos: u64, seeking: Option<u64>, acc: Color| -> Element<'_, Message> {
            let p_disp = seeking.unwrap_or(pos);
            let d_max = dur_ms.max(1) as f32;
            let bar = slider(
                0.0..=d_max,
                p_disp.min(dur_ms.max(1)) as f32,
                Message::SeekChanged,
            )
            .style(slim_slider_style(Color::WHITE, acc))
            .on_release(Message::SeekReleased)
            .width(Length::Fill);

            row![
                text(format_ms(p_disp))
                    .size(10)
                    .font(mono())
                    .color(Color::from_rgba(1.0, 1.0, 1.0, 0.68)),
                bar,
                text(format_ms(dur_ms))
                    .size(10)
                    .font(mono())
                    .color(Color::from_rgba(1.0, 1.0, 1.0, 0.68)),
            ]
            .spacing(8)
            .align_y(Alignment::Center)
            .into()
        };

        let make_full_controls = |playing: bool, pal: DynamicPalette, acc: Color, repeat_mode: RepeatMode, shuffle: bool| -> Element<'_, Message> {
            row![
                button(icons::icon(
                    icons::SHUFFLE,
                    14.0,
                    if shuffle { acc } else { Color::from_rgba(1.0, 1.0, 1.0, 0.65) },
                ))
                .padding(5)
                .style(icon_button_style(pal, false))
                .on_press(Message::ToggleShuffle),
                button(icons::icon(icons::PREV, 15.0, Color::WHITE))
                    .padding(5)
                    .style(icon_button_style(pal, false))
                    .on_press(Message::PrevTrack),
                button(
                    container(icons::icon(
                        if playing { icons::PAUSE } else { icons::PLAY },
                        15.0,
                        Color::BLACK,
                    ))
                    .width(34)
                    .height(34)
                    .align_x(iced::alignment::Horizontal::Center)
                    .align_y(iced::alignment::Vertical::Center),
                )
                .padding(0)
                .style(dock_play_button_style(pal))
                .on_press(Message::TogglePlayPause),
                button(icons::icon(icons::NEXT, 15.0, Color::WHITE))
                    .padding(5)
                    .style(icon_button_style(pal, false))
                    .on_press(Message::NextTrack),
                button(icons::icon(
                    icons::REPEAT,
                    14.0,
                    if repeat_mode != RepeatMode::Off { acc } else { Color::from_rgba(1.0, 1.0, 1.0, 0.65) },
                ))
                .padding(5)
                .style(icon_button_style(pal, false))
                .on_press(Message::ToggleRepeat),
            ]
            .spacing(10)
            .align_y(Alignment::Center)
            .into()
        };

        let muted_audio = self.settings.volume <= 0.01;
        let vol_pct = (self.settings.volume * 100.0).round() as u32;
        let compact_volume = row![
            button(icons::icon(
                if muted_audio { icons::VOLUME_MUTED } else { icons::VOLUME },
                14.0,
                if muted_audio { color_from_rgb(p.danger) } else { Color::from_rgba(1.0, 1.0, 1.0, 0.70) },
            ))
            .padding(4)
            .style(icon_button_style(p, false))
            .on_press(Message::ToggleMute),
            slider(0.0..=2.0, self.settings.volume, Message::VolumeChanged)
                .step(0.05_f32)
                .style(slim_slider_style(Color::WHITE, accent))
                .width(80),
            text(format!("{vol_pct}%"))
                .size(10)
                .font(mono())
                .color(Color::from_rgba(1.0, 1.0, 1.0, 0.60)),
        ]
        .spacing(4)
        .align_y(Alignment::Center);

        let utility_row = row![
            compact_volume,
            horizontal_space(),
            row![
                container(text(loudness_text.clone()).size(9).font(mono()))
                    .padding([2, 6])
                    .style(badge_pill_style(p)),
                container(text(quality_badge_label).size(9).font(mono()))
                    .padding([2, 6])
                    .style(badge_pill_style(p)),
            ]
            .spacing(5)
            .align_y(Alignment::Center),
        ]
        .spacing(8)
        .align_y(Alignment::Center)
        .width(Length::Fill);

        let is_queue = self.right_tab == RightTab::Queue;
        let tab_row = row![
            button(
                row![
                    icons::icon(icons::LYRICS, 12.0, if !is_queue { accent } else { muted }),
                    text("Lyrics").size(11).font(if !is_queue { bold() } else { semibold() }),
                ]
                .spacing(4)
                .align_y(Alignment::Center),
            )
            .padding([4, 10])
            .style(top_pill_button_style(p, !is_queue))
            .on_press(Message::SelectRightTab(RightTab::Lyrics)),
            button(
                row![
                    icons::icon(icons::QUEUE, 12.0, if is_queue { accent } else { muted }),
                    text("Queue").size(11).font(if is_queue { bold() } else { semibold() }),
                ]
                .spacing(4)
                .align_y(Alignment::Center),
            )
            .padding([4, 10])
            .style(top_pill_button_style(p, is_queue))
            .on_press(Message::SelectRightTab(RightTab::Queue)),
            horizontal_space(),
        ]
        .spacing(6)
        .align_y(Alignment::Center);

        let (win_w, win_h) = self.window_size;

        let content: Element<'_, Message> = if win_h <= 280.0 {
            // Ultra-compact floating mini player
            let drag_header = mouse_area(
                row![
                    container(icons::icon(icons::ECHO_MARK, 12.0, Color::BLACK))
                        .width(20)
                        .height(20)
                        .align_x(iced::alignment::Horizontal::Center)
                        .align_y(iced::alignment::Vertical::Center)
                        .style(logo_badge_style(p)),
                    text("MINI PLAYER")
                        .size(10)
                        .font(bold())
                        .color(Color::from_rgba(1.0, 1.0, 1.0, 0.55)),
                    horizontal_space(),
                    self.with_tooltip(
                        button(icons::icon(icons::EXPAND, 13.0, Color::WHITE))
                            .padding(5)
                            .style(icon_button_style(p, false))
                            .on_press(Message::ToggleMiniPlayer),
                        "Exit Mini Player",
                        tooltip::Position::Bottom,
                    ),
                    button(icons::icon(icons::WIN_CLOSE, 11.0, Color::from_rgba(1.0, 1.0, 1.0, 0.78)))
                        .padding(5)
                        .style(window_control_button_style(p, true))
                        .on_press(Message::CloseWindow),
                ]
                .spacing(8)
                .align_y(Alignment::Center),
            )
            .on_press(Message::DragWindow);

            let active_lyric = self
                .active_lyrics
                .as_ref()
                .and_then(|res| {
                    res.active_index(self.position_ms)
                        .and_then(|i| res.synced.get(i))
                })
                .map(|l| l.text.as_str())
                .filter(|s| !s.is_empty());

            let track_row = row![
                container(self.artwork_widget(thumb.as_deref(), 54.0, false))
                    .style(artwork_frame_style(8.0, 0.50)),
                column![
                    container(
                        text(ellipsize(&title, 34))
                            .size(14)
                            .font(bold())
                            .color(Color::WHITE)
                            .wrapping(Wrapping::None),
                    )
                    .width(Length::Fill)
                    .clip(true),
                    container(
                        text(ellipsize(active_lyric.unwrap_or(&artist), 42))
                            .size(12)
                            .color(if active_lyric.is_some() {
                                accent
                            } else {
                                Color::from_rgba(1.0, 1.0, 1.0, 0.72)
                            })
                            .wrapping(Wrapping::None),
                    )
                    .width(Length::Fill)
                    .clip(true),
                ]
                .spacing(2)
                .width(Length::Fill),
                button(icons::icon(
                    if liked { icons::HEART_FILLED } else { icons::HEART_OUTLINE },
                    16.0,
                    if liked { accent } else { muted },
                ))
                .padding(6)
                .style(icon_button_style(p, false))
                .on_press(Message::ToggleLikeCurrent),
            ]
            .spacing(10)
            .align_y(Alignment::Center);

            let compact_seek = slider(
                0.0..=dur_max,
                pos_display.min(self.duration_ms.max(1)) as f32,
                Message::SeekChanged,
            )
            .style(slim_slider_style(Color::WHITE, accent))
            .on_release(Message::SeekReleased)
            .width(Length::Fill);

            let bottom_row = row![
                button(icons::icon(icons::PREV, 15.0, Color::WHITE))
                    .padding(5)
                    .style(icon_button_style(p, false))
                    .on_press(Message::PrevTrack),
                button(
                    container(icons::icon(
                        if is_playing { icons::PAUSE } else { icons::PLAY },
                        14.0,
                        Color::BLACK,
                    ))
                    .width(30)
                    .height(30)
                    .align_x(iced::alignment::Horizontal::Center)
                    .align_y(iced::alignment::Vertical::Center),
                )
                .padding(0)
                .style(dock_play_button_style(p))
                .on_press(Message::TogglePlayPause),
                button(icons::icon(icons::NEXT, 15.0, Color::WHITE))
                    .padding(5)
                    .style(icon_button_style(p, false))
                    .on_press(Message::NextTrack),
                horizontal_space().width(4),
                text(format_ms(pos_display))
                    .size(10)
                    .font(mono())
                    .color(Color::from_rgba(1.0, 1.0, 1.0, 0.68)),
                compact_seek,
                text(format_ms(self.duration_ms))
                    .size(10)
                    .font(mono())
                    .color(Color::from_rgba(1.0, 1.0, 1.0, 0.68)),
            ]
            .spacing(6)
            .align_y(Alignment::Center);

            column![drag_header, track_row, bottom_row]
                .spacing(8)
                .width(Length::Fill)
                .height(Length::Fill)
                .into()
        } else if win_w >= 660.0 {
            // Fullscreen or wide split dock
            let drag_header = mouse_area(
                row![
                    container(icons::icon(icons::ECHO_MARK, 12.0, Color::BLACK))
                        .width(20)
                        .height(20)
                        .align_x(iced::alignment::Horizontal::Center)
                        .align_y(iced::alignment::Vertical::Center)
                        .style(logo_badge_style(p)),
                    text("MINI PLAYER")
                        .size(11)
                        .font(bold())
                        .color(Color::from_rgba(1.0, 1.0, 1.0, 0.70)),
                    container(text(quality_badge_label).size(10).font(mono()))
                        .padding([2, 7])
                        .style(badge_pill_style(p)),
                    horizontal_space(),
                    self.with_tooltip(
                        button(icons::icon(icons::EXPAND, 13.0, Color::WHITE))
                            .padding(6)
                            .style(icon_button_style(p, false))
                            .on_press(Message::ToggleMiniPlayer),
                        "Exit Mini Player",
                        tooltip::Position::Bottom,
                    ),
                    self.with_tooltip(
                        button(icons::icon(
                            if self.window_maximized { icons::WIN_RESTORE } else { icons::WIN_MAXIMIZE },
                            12.0,
                            Color::WHITE,
                        ))
                        .padding(6)
                        .style(icon_button_style(p, false))
                        .on_press(Message::ToggleMaximizeWindow),
                        if self.window_maximized { "Restore Window" } else { "Maximize Window" },
                        tooltip::Position::Bottom,
                    ),
                    button(icons::icon(icons::WIN_CLOSE, 11.0, Color::from_rgba(1.0, 1.0, 1.0, 0.78)))
                        .padding(6)
                        .style(window_control_button_style(p, true))
                        .on_press(Message::CloseWindow),
                ]
                .spacing(8)
                .align_y(Alignment::Center),
            )
            .on_press(Message::DragWindow);

            let left_col_w = (win_w * 0.38).clamp(280.0, 380.0);
            let art_size = (win_h * 0.34).clamp(160.0, 260.0);

            let left_pane = column![
                container(
                    container(self.artwork_widget(thumb.as_deref(), art_size, false))
                        .style(artwork_frame_style(16.0, 0.55)),
                )
                .width(Length::Fill)
                .align_x(iced::alignment::Horizontal::Center),
                vertical_space().height(4),
                row![
                    column![
                        container(
                            text(title.clone())
                                .size(18)
                                .font(bold())
                                .color(Color::WHITE)
                                .wrapping(Wrapping::None),
                        )
                        .width(Length::Fill)
                        .clip(true),
                        container(
                            text(artist.clone())
                                .size(13)
                                .color(Color::from_rgba(1.0, 1.0, 1.0, 0.74))
                                .wrapping(Wrapping::None),
                        )
                        .width(Length::Fill)
                        .clip(true),
                    ]
                    .spacing(2)
                    .width(Length::Fill),
                    button(icons::icon(
                        if liked { icons::HEART_FILLED } else { icons::HEART_OUTLINE },
                        16.0,
                        if liked { accent } else { muted },
                    ))
                    .padding(6)
                    .style(icon_button_style(p, false))
                    .on_press(Message::ToggleLikeCurrent),
                ]
                .spacing(8)
                .align_y(Alignment::Center),
                vertical_space().height(2),
                make_timeline(self.duration_ms, self.position_ms, self.seeking_ms, accent),
                container(make_full_controls(is_playing, p, accent, self.queue.repeat_mode, self.queue.shuffle))
                    .width(Length::Fill)
                    .align_x(iced::alignment::Horizontal::Center),
                utility_row,
                vertical_space().height(2),
                self.live_spectrum_strip(accent),
            ]
            .spacing(6)
            .width(left_col_w);

            let right_pane = column![
                tab_row,
                container(if is_queue {
                    self.view_mini_queue()
                } else {
                    self.view_lyrics_inner(26, 16)
                })
                .width(Length::Fill)
                .height(Length::Fill),
            ]
            .spacing(8)
            .width(Length::Fill)
            .height(Length::Fill);

            column![
                drag_header,
                row![left_pane, right_pane]
                    .spacing(24)
                    .width(Length::Fill)
                    .height(Length::Fill),
            ]
            .spacing(12)
            .width(Length::Fill)
            .height(Length::Fill)
            .into()
        } else {
            // Narrow vertical sidebar dock (e.g. snapped to side on a narrow column)
            let drag_header = mouse_area(
                row![
                    container(icons::icon(icons::ECHO_MARK, 12.0, Color::BLACK))
                        .width(20)
                        .height(20)
                        .align_x(iced::alignment::Horizontal::Center)
                        .align_y(iced::alignment::Vertical::Center)
                        .style(logo_badge_style(p)),
                    text("MINI PLAYER")
                        .size(10)
                        .font(bold())
                        .color(Color::from_rgba(1.0, 1.0, 1.0, 0.65)),
                    horizontal_space(),
                    self.with_tooltip(
                        button(icons::icon(icons::EXPAND, 13.0, Color::WHITE))
                            .padding(5)
                            .style(icon_button_style(p, false))
                            .on_press(Message::ToggleMiniPlayer),
                        "Exit Mini Player",
                        tooltip::Position::Bottom,
                    ),
                    self.with_tooltip(
                        button(icons::icon(
                            if self.window_maximized { icons::WIN_RESTORE } else { icons::WIN_MAXIMIZE },
                            12.0,
                            Color::WHITE,
                        ))
                        .padding(5)
                        .style(icon_button_style(p, false))
                        .on_press(Message::ToggleMaximizeWindow),
                        if self.window_maximized { "Restore Window" } else { "Maximize Window" },
                        tooltip::Position::Bottom,
                    ),
                    button(icons::icon(icons::WIN_CLOSE, 11.0, Color::from_rgba(1.0, 1.0, 1.0, 0.78)))
                        .padding(5)
                        .style(window_control_button_style(p, true))
                        .on_press(Message::CloseWindow),
                ]
                .spacing(6)
                .align_y(Alignment::Center),
            )
            .on_press(Message::DragWindow);

            let hero_art_size = (win_h * 0.16).clamp(80.0, 130.0);
            let hero_card = column![
                row![
                    container(self.artwork_widget(thumb.as_deref(), hero_art_size, false))
                        .style(artwork_frame_style(10.0, 0.45)),
                    column![
                        container(
                            text(title.clone())
                                .size(15)
                                .font(bold())
                                .color(Color::WHITE)
                                .wrapping(Wrapping::None),
                        )
                        .width(Length::Fill)
                        .clip(true),
                        container(
                            text(artist.clone())
                                .size(12)
                                .color(Color::from_rgba(1.0, 1.0, 1.0, 0.72))
                                .wrapping(Wrapping::None),
                        )
                        .width(Length::Fill)
                        .clip(true),
                        vertical_space().height(2),
                        row![
                            container(text(loudness_text.clone()).size(9).font(mono()))
                                .padding([2, 5])
                                .style(badge_pill_style(p)),
                            button(icons::icon(
                                if liked { icons::HEART_FILLED } else { icons::HEART_OUTLINE },
                                16.0,
                                if liked { accent } else { muted },
                            ))
                            .padding(6)
                            .style(icon_button_style(p, false))
                            .on_press(Message::ToggleLikeCurrent),
                        ]
                        .spacing(6)
                        .align_y(Alignment::Center),
                    ]
                    .spacing(2)
                    .width(Length::Fill),
                ]
                .spacing(10)
                .align_y(Alignment::Center),
                make_timeline(self.duration_ms, self.position_ms, self.seeking_ms, accent),
                container(make_full_controls(is_playing, p, accent, self.queue.repeat_mode, self.queue.shuffle))
                    .width(Length::Fill)
                    .align_x(iced::alignment::Horizontal::Center),
            ]
            .spacing(6)
            .width(Length::Fill);

            let lower_section = container(if is_queue {
                self.view_mini_queue()
            } else {
                self.view_lyrics_inner(18, 13)
            })
            .width(Length::Fill)
            .height(Length::Fill);

            column![
                drag_header,
                hero_card,
                tab_row,
                lower_section,
            ]
            .spacing(8)
            .width(Length::Fill)
            .height(Length::Fill)
            .into()
        };

        let content_container = container(content)
            .padding(if win_h <= 280.0 { [8, 12] } else { [12, 18] })
            .width(Length::Fill)
            .height(Length::Fill);

        if let Some(blur_handle) = thumb.as_deref().and_then(|u| self.blurred_covers.get(u)) {
            stack![
                image(blur_handle.clone())
                    .width(Length::Fill)
                    .height(Length::Fill)
                    .content_fit(iced::ContentFit::Cover),
                container(content_container)
                    .width(Length::Fill)
                    .height(Length::Fill)
                    .style(mini_player_scrim_style(p)),
            ]
            .width(Length::Fill)
            .height(Length::Fill)
            .into()
        } else {
            container(content_container)
                .width(Length::Fill)
                .height(Length::Fill)
                .style(mini_player_fallback_style(p))
                .into()
        }
    }
}

fn greeting_label() -> &'static str {
    match chrono::Local::now().hour() {
        5..=11 => "Good morning",
        12..=16 => "Good afternoon",
        17..=21 => "Good evening",
        _ => "Late night listening",
    }
}

fn greeting_kicker() -> &'static str {
    match chrono::Local::now().hour() {
        5..=11 => "MORNING ROTATION",
        12..=16 => "MIDDAY FLOW",
        17..=21 => "EVENING SESSION",
        _ => "AFTER HOURS",
    }
}

fn bold() -> Font {
    Font {
        weight: iced::font::Weight::Bold,
        ..Font::DEFAULT
    }
}

fn semibold() -> Font {
    Font {
        weight: iced::font::Weight::Semibold,
        ..Font::DEFAULT
    }
}

fn mono() -> Font {
    Font::MONOSPACE
}

fn ellipsize(s: &str, max_chars: usize) -> String {
    let mut chars = s.chars();
    let taken: String = chars.by_ref().take(max_chars).collect();
    if chars.next().is_some() {
        format!("{}...", taken.trim_end())
    } else {
        taken
    }
}

fn format_ms(ms: u64) -> String {
    let total_secs = ms / 1000;
    format!("{:02}:{:02}", total_secs / 60, total_secs % 60)
}

fn format_duration_hm(secs: u64) -> String {
    if secs >= 3600 {
        format!("{}h {}m", secs / 3600, (secs % 3600) / 60)
    } else if secs >= 60 {
        format!("{} min", secs / 60)
    } else if secs > 0 {
        "<1 min".to_string()
    } else {
        "0 min".to_string()
    }
}

fn format_hour_12(hour: u8) -> String {
    match hour % 24 {
        0 => "12 AM".to_string(),
        1..=11 => format!("{hour} AM"),
        12 => "12 PM".to_string(),
        h => format!("{} PM", h - 12),
    }
}

fn open_browser_url(url: &str) -> std::io::Result<()> {
    #[cfg(target_os = "windows")]
    {
        std::process::Command::new("cmd")
            .args(["/C", "start", "", url])
            .spawn()?;
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = url;
    }
    Ok(())
}

fn parse_folder_list(raw: &str) -> Vec<String> {
    raw.split([';', '\n'])
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(ToString::to_string)
        .collect()
}

fn append_folder_entry(list: &mut Vec<String>, path: &str) -> String {
    let trimmed = path.trim();
    if !trimmed.is_empty() && !list.iter().any(|existing| existing.eq_ignore_ascii_case(trimmed)) {
        list.push(trimmed.to_string());
    }
    list.join("; ")
}

fn item_artist_hint(item: &BrowseItem) -> String {
    match item {
        BrowseItem::Track(t) => t.primary_artist().to_string(),
        BrowseItem::Album { artist, .. } => artist.clone(),
        BrowseItem::Artist { name, .. } => name.clone(),
        BrowseItem::Playlist { author, .. } => author.clone(),
    }
}

fn score_browse_item(item: &BrowseItem, ctx: &ListenerContext) -> f64 {
    match item {
        BrowseItem::Track(t) => score_candidate(t, ctx),
        BrowseItem::Album { artist, .. }
        | BrowseItem::Artist { name: artist, .. }
        | BrowseItem::Playlist { author: artist, .. } => score_artist_name(artist, ctx),
    }
}

fn item_version_key(item: &BrowseItem) -> Option<String> {
    match item {
        BrowseItem::Track(t) => Some(base_version_key(&t.title, t.primary_artist())),
        _ => None,
    }
}

fn rank_home_section(
    sec: &HomeSection,
    ctx: &ListenerContext,
    preserve_order: bool,
    seen_versions: &mut HashSet<String>,
    seen_keys: &mut HashSet<String>,
) -> Option<HomeSection> {
    if sec.items.is_empty() {
        return None;
    }
    let clean: Vec<BrowseItem> = sec
        .items
        .iter()
        .filter(|it| !is_preview_title(it.title()))
        .cloned()
        .collect();
    if clean.is_empty() {
        return None;
    }

    let mut ordered = if ctx.known && clean.len() > 3 {
        let kept: Vec<BrowseItem> = clean
            .iter()
            .filter(|it| !ctx.is_disliked_artist(&item_artist_hint(it)))
            .cloned()
            .collect();
        if kept.len() >= 3 {
            kept
        } else {
            clean
        }
    } else {
        clean
    };

    if ctx.known && !preserve_order {
        ordered.sort_by(|a, b| {
            let sa = (score_browse_item(a, ctx) * 100.0).round() as i64;
            let sb = (score_browse_item(b, ctx) * 100.0).round() as i64;
            sb.cmp(&sa)
        });
    }

    // Echo TidyPass: prevent duplicate songs/versions or duplicate cards across Home shelves.
    let mut deduped = Vec::new();
    for it in ordered {
        let key = it.stable_key();
        if seen_keys.contains(&key) {
            continue;
        }
        if let Some(vk) = item_version_key(&it) {
            if seen_versions.contains(&vk) {
                continue;
            }
            seen_versions.insert(vk);
        }
        seen_keys.insert(key);
        deduped.push(it);
    }

    if deduped.is_empty() {
        return None;
    }

    Some(HomeSection {
        title: sec.title.clone(),
        items: deduped,
    })
}
