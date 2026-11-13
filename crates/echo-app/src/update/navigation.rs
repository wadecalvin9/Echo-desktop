use iced::widget::text_input;
use iced::Task;
use iced::keyboard::{key::Named, Key};

use echo_core::{
    discover_ssdp_devices, parse_cast_address,
};

use crate::smtc;
use crate::tray;
use crate::types::*;
use crate::EchoApp;

impl EchoApp {
    pub(crate) fn update_navigation(&mut self, message: Message) -> Task<Message> {
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
            Message::PlaylistFilterChanged(q) => {
                self.playlist_filter_query = q;
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
            Message::ToggleRightSidebar => {
                self.show_right_rail = !self.show_right_rail;
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
            Message::OpenUrl(url) => {
                #[cfg(windows)]
                {
                    use windows::core::HSTRING;
                    use windows::core::w;
                    use windows::Win32::UI::Shell::ShellExecuteW;
                    use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;
                    unsafe {
                        let _ = ShellExecuteW(None, w!("open"), &HSTRING::from(&url), None, None, SW_SHOWNORMAL);
                    }
                }
                #[cfg(not(windows))]
                {
                    let _ = std::process::Command::new("xdg-open").arg(&url).spawn();
                }
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
            Message::ToggleStreamInspector => {
                self.show_stream_inspector = !self.show_stream_inspector;
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
            Message::FocusSearch => text_input::focus(self.search_input_id.clone()),
            Message::KeyPressed(key, modifiers, captured) => {
                if modifiers.control() && modifiers.alt() {
                    match &key {
                        Key::Named(Named::Space) => return self.update(Message::TogglePlayPause),
                        Key::Named(Named::ArrowRight) => return self.update(Message::NextTrack),
                        Key::Named(Named::ArrowLeft) => return self.update(Message::PrevTrack),
                        Key::Named(Named::ArrowUp) => {
                            return self.update(Message::VolumeChanged(self.settings.volume + 0.05));
                        }
                        Key::Named(Named::ArrowDown) => {
                            return self.update(Message::VolumeChanged(self.settings.volume - 0.05));
                        }
                        _ => {}
                    }
                }
                if modifiers.control() {
                    if let Key::Character(c) = &key {
                        if c.eq_ignore_ascii_case("k") || c.eq_ignore_ascii_case("f") {
                            return self.update(Message::FocusSearch);
                        } else if c.eq_ignore_ascii_case("m") {
                            return self.update(Message::ToggleMute);
                        } else if c.eq_ignore_ascii_case("l") {
                            return self.update(Message::ToggleLikeCurrent);
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
                        if self.show_stream_inspector {
                            self.show_stream_inspector = false;
                            return Task::none();
                        } else if self.mini_player {
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
                if self.taskbar.is_none() {
                    self.taskbar = crate::taskbar::TaskbarProgress::new(hwnd);
                }
                if self.tray.is_none() {
                    self.tray = Some(tray::TrayBridge::new(hwnd));
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
                if self.queue.current().is_some() {
                    self.set_toast("Playing in system tray • Right-click tray icon for controls");
                    if let Some(id) = self.main_window_id {
                        iced::window::minimize(id, true)
                    } else {
                        iced::window::get_oldest().then(|opt| match opt {
                            Some(id) => iced::window::minimize(id, true),
                            None => Task::none(),
                        })
                    }
                } else {
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
            }
            Message::ToggleCastModal => {
                self.show_cast_modal = !self.show_cast_modal;
                if self.show_cast_modal && self.discovered_cast_devices.is_empty() {
                    return self.update(Message::ScanCastDevices);
                }
                Task::none()
            }
            Message::ScanCastDevices => {
                self.cast_scanning = true;
                Task::perform(
                    async move {
                        tokio::task::spawn_blocking(|| {
                            discover_ssdp_devices(std::time::Duration::from_millis(1500))
                        })
                        .await
                        .unwrap_or_default()
                    },
                    Message::CastDevicesFound,
                )
            }
            Message::CastDevicesFound(devices) => {
                self.cast_scanning = false;
                self.discovered_cast_devices = devices;
                Task::none()
            }
            Message::SelectCastDevice(dev) => {
                self.active_cast_device = dev.clone();
                if let Some(d) = dev {
                    self.set_toast(format!("Casting audio to {}", d.name));
                } else {
                    self.set_toast("Playback switched back to PC");
                }
                Task::none()
            }
            Message::CastAddressInputChanged(input) => {
                self.cast_address_input = input;
                Task::none()
            }
            Message::ConnectCastAddress => {
                if let Some(dev) = parse_cast_address(&self.cast_address_input) {
                    self.active_cast_device = Some(dev.clone());
                    if !self.discovered_cast_devices.iter().any(|d| d.id == dev.id) {
                        self.discovered_cast_devices.push(dev.clone());
                    }
                    self.cast_address_input.clear();
                    self.set_toast(format!("Connected to {}", dev.name));
                } else {
                    self.set_toast("Invalid Cast address (e.g. chromecast://192.168.1.50)");
                }
                Task::none()
            }
            _ => Task::none(),
        }
    }
}

