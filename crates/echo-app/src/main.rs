mod auth;
mod dialogs;
mod icons;
mod ranking;
mod smtc;
mod taskbar;
mod tasks;
mod theme;
mod tray;
mod types;
mod views;
mod update;

pub use ranking::*;
pub use types::*;
use crate::theme::{
    app_shell_style, color_from_rgb, divider_style, ghost_row_button_style, glass_panel_style,
    icon_button_style, popover_panel_style, to_iced_theme,
};
use echo_audio::{available_output_devices, AudioEngine, PlaybackStatus};
use echo_core::{
    AppSettings, CastDevice,
    DynamicPalette, EchoDb, ListenerContext, ListeningSummary,
    Playlist, QueueState, RecognitionEntry, RgbColor, Track, TrackStats,
};
use echo_fingerprint::{Recognised, ShazamClient};
use echo_lyrics::{LyricsFetcher, LyricsResult};
use echo_youtube::{
    AccountPlaylist, BrowseItem, ExplorePage, HomeChip, HomeSection, MoodGenreGroup, SearchFilter,
    YouTubeMusicClient,
};
use chrono::Timelike;
use iced::keyboard::{self};
use iced::widget::text::Wrapping;
use iced::widget::{
    button, column, container, horizontal_space, image, mouse_area, row, scrollable, stack,
    text_input, vertical_space,
};
use iced::{Alignment, Color, Element, Font, Length, Subscription, Task, Theme};
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
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


pub(crate) struct EchoApp {
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
    stream_retry_count: usize,
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
    cover_tints: HashMap<String, RgbColor>,
    loading_covers: HashSet<String>,
    cover_order: std::collections::VecDeque<String>,
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
    taskbar: Option<taskbar::TaskbarProgress>,
    smtc_requested: bool,
    tray: Option<tray::TrayBridge>,
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

    songs_sort_column: SongsSortColumn,
    songs_sort_ascending: bool,
    all_songs: Vec<Track>,
    show_cast_modal: bool,
    discovered_cast_devices: Vec<CastDevice>,
    active_cast_device: Option<CastDevice>,
    cast_scanning: bool,
    cast_address_input: String,
    available_audio_devices: Vec<String>,
    settings_section: SettingsSection,
    settings_search_query: String,
    playlist_filter_query: String,
    show_stream_inspector: bool,
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
        audio.set_eq_bands(settings.custom_eq_bands);
        audio.set_playback_speed(settings.playback_speed);
        if let Some(ref dev) = settings.output_device {
            audio.set_output_device(Some(dev.clone()));
        }
        let available_audio_devices = available_output_devices();
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
            left_collapsed: true,
            show_right_rail: false,
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
            stream_retry_count: 0,
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
            cover_tints: HashMap::new(),
            loading_covers: HashSet::new(),
            cover_order: std::collections::VecDeque::new(),
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
            taskbar: None,
            smtc_requested: false,
            tray: None,
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
            songs_sort_column: SongsSortColumn::Title,
            songs_sort_ascending: true,
            all_songs: Vec::new(),
            show_cast_modal: false,
            discovered_cast_devices: Vec::new(),
            active_cast_device: None,
            cast_scanning: false,
            cast_address_input: String::new(),
            available_audio_devices,
            settings_section: SettingsSection::All,
            settings_search_query: String::new(),
            playlist_filter_query: String::new(),
            show_stream_inspector: false,
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
        if app.yt.has_cookie() {
            tasks.push(app.update(Message::SyncYouTubeAccount));
        }
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


    fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            // Playback & Audio Engine
            Message::AnimTick
            | Message::Tick
            | Message::PlayTrack(..)
            | Message::PlayTracks { .. }
            | Message::QueuePlayNext(..)
            | Message::QueueAddBack(..)
            | Message::QueueJump(..)
            | Message::QueueRemove(..)
            | Message::QueueMoveUp(..)
            | Message::QueueMoveDown(..)
            | Message::QueueClear
            | Message::SaveQueueAsPlaylist
            | Message::TogglePlayPause
            | Message::NextTrack
            | Message::PrevTrack
            | Message::ToggleRepeat
            | Message::ToggleShuffle
            | Message::SeekChanged(..)
            | Message::SeekReleased
            | Message::SeekToMs(..)
            | Message::VolumeChanged(..)
            | Message::ToggleMute
            | Message::OutputDeviceSelected(..)
            | Message::PlaybackSpeedChanged(..)
            | Message::CycleSleepTimer
            | Message::ToggleSkipSilence(..)
            | Message::TransitionFadeSelected(..)
            | Message::EqPresetSelected(..)
            | Message::AutoEqProfileSelected(..)
            | Message::EqBandChanged(..)
            | Message::ResetCustomEq
            | Message::ApplyCustomEqCurve(..)
            | Message::SpatialModeSelected(..)
            | Message::ToggleNormalization(..)
            | Message::AudioQualitySelected(..) => self.update_playback(message),

            // Navigation, Window & Shell UI
            Message::SelectTab(..)
            | Message::PlaylistFilterChanged(..)
            | Message::SelectStatsRange(..)
            | Message::SelectLibraryTab(..)
            | Message::LibraryFilterSelected(..)
            | Message::CardHovered(..)
            | Message::RowHovered(..)
            | Message::ShelfScroll { .. }
            | Message::SelectRightTab(..)
            | Message::ToggleRightTab(..)
            | Message::ToggleLeftSidebar
            | Message::ToggleRightSidebar
            | Message::ToggleExpandedPlayer
            | Message::ToggleMiniPlayer
            | Message::ToggleStreamInspector
            | Message::FocusSearch
            | Message::KeyPressed(..)
            | Message::WindowOpened(..)
            | Message::WindowResized(..)
            | Message::WindowHandleReady(..)
            | Message::DragWindow
            | Message::MinimizeWindow
            | Message::ToggleMaximizeWindow
            | Message::WindowMaximizedState(..)
            | Message::CloseWindow
            | Message::NavigateBackDetail
            | Message::NavigateForwardDetail
            | Message::OpenUrl(..)
            | Message::ToggleCastModal
            | Message::ScanCastDevices
            | Message::CastDevicesFound(..)
            | Message::SelectCastDevice(..)
            | Message::CastAddressInputChanged(..)
            | Message::ConnectCastAddress => self.update_navigation(message),

            // YouTube Catalog, Stream Resolving, Search & Lyrics
            Message::RadioLoaded(..)
            | Message::LoadHome
            | Message::SelectHomeChip(..)
            | Message::LoadMoreHome
            | Message::HomePageLoaded { .. }
            | Message::TasteShelvesLoaded(..)
            | Message::LoadExplore
            | Message::ExploreLoaded { .. }
            | Message::OpenCategory { .. }
            | Message::CategoryLoaded { .. }
            | Message::OpenPlaylist { .. }
            | Message::PlaylistPageLoaded(..)
            | Message::LoadMorePlaylist
            | Message::PlaylistContinuationLoaded(..)
            | Message::OpenAlbum { .. }
            | Message::AlbumLoaded(..)
            | Message::OpenArtist { .. }
            | Message::ArtistLoaded(..)
            | Message::OpenArtistItems { .. }
            | Message::OpenShelfItems { .. }
            | Message::ArtistItemsLoaded(..)
            | Message::SubscribeArtist { .. }
            | Message::SubscribeResult { .. }
            | Message::OpenTrackArtist(..)
            | Message::OpenTrackAlbum(..)
            | Message::SearchExact(..)
            | Message::SearchQueryChanged(..)
            | Message::SuggestionsLoaded(..)
            | Message::CloseSearchDropdown
            | Message::ApplySuggestion(..)
            | Message::MoveHomeSectionUp(..)
            | Message::MoveHomeSectionDown(..)
            | Message::SearchFilterSelected(..)
            | Message::SubmitSearch
            | Message::LoadMoreSearch
            | Message::SearchPageLoaded { .. }
            | Message::StreamResolved { .. }
            | Message::ArtworkLoaded { .. }
            | Message::LyricsLoaded { .. }
            | Message::AdjustLyricsOffset(..)
            | Message::ResetLyricsOffset
            | Message::ShareLyricsCard
            | Message::ToggleBetterLyrics(..)
            | Message::ToggleRomanizeLyrics
            | Message::ToggleAutoLoadMore(..)
            | Message::StartGoogleSignIn
            | Message::GoogleSignInFinished(..)
            | Message::YouTubeCookieChanged(..)
            | Message::SaveYouTubeCookie
            | Message::ClearYouTubeCookie
            | Message::SyncYouTubeAccount
            | Message::YoutubeRegionSelected(..)
            | Message::YoutubeLanguageSelected(..)
            | Message::AccountSynced { .. } => self.update_youtube(message),

            // Library, Playlists, Downloads, Local Scan, Scrobbler & Settings
            _ => self.update_library(message),
        }
    }

    fn left_sidebar_width(&self) -> f32 {
        if self.left_collapsed {
            72.0
        } else if self.window_size.0 < 1200.0 {
            260.0
        } else {
            300.0
        }
    }

    fn right_rail_width(&self) -> f32 {
        if self.window_size.0 < 1200.0 {
            280.0
        } else {
            320.0
        }
    }

    fn show_right_rail_effective(&self) -> bool {
        !self.show_expanded && self.show_right_rail && self.window_size.0 >= 980.0
    }

    fn center_width(&self) -> f32 {
        let mut w = self.window_size.0 - 16.0 - self.left_sidebar_width() - 8.0;
        if self.show_right_rail_effective() {
            w -= self.right_rail_width() + 8.0;
        }
        w.max(320.0)
    }

    fn search_width(&self) -> f32 {
        let max_w = if self.window_size.0 >= 1400.0 { 480.0 } else { 420.0 };
        (self.window_size.0 * 0.30).clamp(280.0, max_w)
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
            let aligned_row = row![dropdown].align_y(Alignment::Start);
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
            menu_col = menu_col.push(
                container(horizontal_space())
                    .height(1)
                    .width(Length::Fill)
                    .style(divider_style(0.10)),
            );
            menu_col = menu_col.push(menu_row(
                icons::CLOSE,
                "Don't suggest this song".to_string(),
                muted,
                Message::ExcludeTrack(track.clone()),
            ));
            menu_col = menu_col.push(menu_row(
                icons::CLOSE,
                format!("Don't suggest {}", ellipsize(track.primary_artist(), 20)),
                muted,
                Message::ExcludeArtist(track.primary_artist().to_string()),
            ));
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

        if self.show_cast_modal {
            layers.push(self.view_cast_modal());
        }

        if self.show_stream_inspector {
            layers.push(self.view_stream_inspector());
        }

        stack(layers).into()
    }

}

pub(crate) fn dynamic_greeting(known_taste: bool) -> (&'static str, String, &'static str) {
    let now = chrono::Local::now();
    let hour = now.hour();
    let weekday = now.format("%A").to_string();

    let (kicker, title) = match hour {
        5..=10 => ("MORNING FLOW", format!("Start Your {weekday}")),
        11..=13 => ("MIDDAY BEATS", "Your Midday Soundtrack".to_string()),
        14..=17 => ("AFTERNOON ROTATION", "Jump Back In".to_string()),
        18..=22 => ("EVENING SESSIONS", format!("{weekday} Evening Vibes")),
        _ => ("AFTER HOURS", "Late Night Frequency".to_string()),
    };

    let sub = if known_taste {
        "Personalized picks shaped by your taste graph & active session"
    } else {
        "Fresh tracks, artist picks & curated mixes ready to play"
    };

    (kicker, title, sub)
}

pub(crate) fn text<'a>(
    content: impl iced::widget::text::IntoFragment<'a>,
) -> iced::widget::Text<'a, Theme, iced::Renderer> {
    iced::widget::text(content).shaping(iced::widget::text::Shaping::Advanced)
}

pub(crate) fn bold() -> Font {
    Font {
        weight: iced::font::Weight::Bold,
        ..Font::DEFAULT
    }
}

pub(crate) fn semibold() -> Font {
    Font {
        weight: iced::font::Weight::Semibold,
        ..Font::DEFAULT
    }
}

pub(crate) fn mono() -> Font {
    Font::MONOSPACE
}

pub(crate) fn ellipsize(s: &str, max_chars: usize) -> String {
    let mut chars = s.chars();
    let taken: String = chars.by_ref().take(max_chars).collect();
    if chars.next().is_some() {
        format!("{}...", taken.trim_end())
    } else {
        taken
    }
}

pub(crate) fn format_ms(ms: u64) -> String {
    let total_secs = ms / 1000;
    format!("{:02}:{:02}", total_secs / 60, total_secs % 60)
}

pub(crate) fn format_duration_hm(secs: u64) -> String {
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

pub(crate) fn format_hour_12(hour: u8) -> String {
    match hour % 24 {
        0 => "12 AM".to_string(),
        1..=11 => format!("{hour} AM"),
        12 => "12 PM".to_string(),
        h => format!("{} PM", h - 12),
    }
}

