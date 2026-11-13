use std::collections::HashSet;

use iced::alignment::Alignment;
use iced::widget::text::Wrapping;
use iced::widget::{
    button, column, container, horizontal_space, mouse_area, row, scrollable, text_input,
    tooltip, vertical_space,
};
use iced::{Color, Element, Length};

use echo_core::CastProtocol;
use echo_youtube::BrowseItem;

use crate::icons;
use crate::theme::{
    account_badge_style, bare_input_style, color_from_rgb, divider_style, downloads_badge_style,
    ghost_row_button_style, glass_panel_style, icon_button_style, liked_gradient_badge_style,
    local_badge_style, logo_badge_style, nav_item_button_style, pill_search_input_style,
    popover_panel_style, search_field_style, stats_rank_badge_style, tile_button_style,
    top_pill_button_style, window_control_button_style, with_alpha,
};
use crate::{
    bold, ellipsize, semibold, text, DetailPage, EchoApp, LibraryFilter, LibraryTab, Message,
    NavTab,
};

impl EchoApp {    pub(crate) fn view_cast_modal(&self) -> Element<'_, Message> {
        let p = self.palette;
        let muted = color_from_rgb(p.text_muted);
        let accent = color_from_rgb(p.primary);

        let header = row![
            icons::icon(icons::CAST, 20.0, accent),
            text("Cast to Device").size(17).font(bold()),
            horizontal_space(),
            button(icons::icon(icons::CLOSE, 16.0, muted))
                .padding(6)
                .style(icon_button_style(p, false))
                .on_press(Message::ToggleCastModal),
        ]
        .spacing(8)
        .align_y(Alignment::Center);

        let desc = text("Stream playback to Google Cast / Chromecast, Smart TVs, Nest Hub, DLNA / UPnP receivers, or FCast devices.")
            .size(12)
            .color(muted);

        let refresh_btn = button(
            row![
                icons::icon(icons::REFRESH, 14.0, if self.cast_scanning { accent } else { muted }),
                text(if self.cast_scanning { "Scanning..." } else { "Scan Network" }).size(12),
            ]
            .spacing(6)
            .align_y(Alignment::Center),
        )
        .padding([5, 11])
        .style(top_pill_button_style(p, false))
        .on_press(Message::ScanCastDevices);

        let scan_row = row![
            text("Discovered Devices").size(13).font(semibold()),
            horizontal_space(),
            refresh_btn,
        ]
        .align_y(Alignment::Center);

        let is_local_active = self.active_cast_device.is_none();
        let pc_item = button(
            row![
                icons::icon(icons::SPEAKER, 18.0, if is_local_active { accent } else { muted }),
                column![
                    text("This Computer (Local Playback)").size(13).font(if is_local_active { bold() } else { semibold() }),
                    text("Speakers and headphones connected directly to this PC").size(11).color(muted),
                ]
                .spacing(2),
                horizontal_space(),
                if is_local_active {
                    container(icons::icon(icons::CHECK_CIRCLE, 16.0, accent))
                } else {
                    container(horizontal_space().width(16))
                },
            ]
            .spacing(12)
            .align_y(Alignment::Center),
        )
        .padding([8, 12])
        .width(Length::Fill)
        .style(tile_button_style(p))
        .on_press(Message::SelectCastDevice(None));

        let mut dev_list = column![pc_item].spacing(6);

        for dev in &self.discovered_cast_devices {
            let is_active = self.active_cast_device.as_ref().map(|d| d.id == dev.id).unwrap_or(false);
            let proto_badge = match dev.protocol {
                CastProtocol::Chromecast => "Chromecast",
                CastProtocol::Dlna => "DLNA",
                CastProtocol::FCast => "FCast",
            };
            let dev_clone = dev.clone();
            let row_item = button(
                row![
                    icons::icon(icons::CAST, 18.0, if is_active { accent } else { muted }),
                    column![
                        text(&dev.name).size(13).font(if is_active { bold() } else { semibold() }),
                        row![
                            container(text(proto_badge).size(9).font(bold()).color(Color::WHITE))
                                .padding([2, 5])
                                .style(stats_rank_badge_style(p, true)),
                            text(format!(" • {}", dev.address)).size(11).color(muted),
                        ]
                        .spacing(4)
                        .align_y(Alignment::Center),
                    ]
                    .spacing(2),
                    horizontal_space(),
                    if is_active {
                        container(icons::icon(icons::CHECK_CIRCLE, 16.0, accent))
                    } else {
                        container(horizontal_space().width(16))
                    },
                ]
                .spacing(12)
                .align_y(Alignment::Center),
            )
            .padding([8, 12])
            .width(Length::Fill)
            .style(tile_button_style(p))
            .on_press(Message::SelectCastDevice(Some(dev_clone)));
            dev_list = dev_list.push(row_item);
        }

        let manual_connect_row = row![
            text_input("chromecast://192.168.1.50 or http://...", &self.cast_address_input)
                .on_input(Message::CastAddressInputChanged)
                .on_submit(Message::ConnectCastAddress)
                .padding([7, 12])
                .size(12)
                .style(pill_search_input_style(p))
                .width(Length::Fill),
            button(text("Connect").size(12))
                .padding([7, 14])
                .style(top_pill_button_style(p, true))
                .on_press(Message::ConnectCastAddress),
        ]
        .spacing(8)
        .align_y(Alignment::Center);

        let content = column![
            header,
            desc,
            container(horizontal_space()).height(1).width(Length::Fill).style(divider_style(0.08)),
            scan_row,
            container(dev_list).max_height(220),
            container(horizontal_space()).height(1).width(Length::Fill).style(divider_style(0.08)),
            text("Manual Device IP or URL").size(12).font(semibold()),
            manual_connect_row,
        ]
        .spacing(12);

        let card = container(content)
            .width(440)
            .padding(18)
            .style(popover_panel_style(p));

        mouse_area(
            container(card)
                .width(Length::Fill)
                .height(Length::Fill)
                .align_x(iced::alignment::Horizontal::Center)
                .align_y(iced::alignment::Vertical::Center),
        )
        .on_press(Message::ToggleCastModal)
        .into()
    }

    pub(crate) fn view_top_bar(&self) -> Element<'_, Message> {
        let p = self.palette;
        let muted = color_from_rgb(p.text_muted);

        let can_back = !self.nav_history.is_empty() || self.viewing_detail.is_some() || self.show_expanded;
        let can_fwd = !self.nav_forward.is_empty();

        let current_page_title = match &self.viewing_detail {
            Some(DetailPage::Playlist(pl)) => ellipsize(&pl.title, 26),
            Some(DetailPage::Album(alb)) => ellipsize(&alb.title, 26),
            Some(DetailPage::Artist { page, .. }) => ellipsize(&page.name, 26),
            Some(DetailPage::Category { title, .. }) => ellipsize(title, 26),
            Some(DetailPage::ArtistItems { title, .. }) => ellipsize(title, 26),
            None => match self.tab {
                NavTab::Home => "Home".to_string(),
                NavTab::Explore => "Explore".to_string(),
                NavTab::Playlists => match self.library_tab {
                    LibraryTab::Liked => "Liked Songs".to_string(),
                    LibraryTab::History => "Recently Played".to_string(),
                    LibraryTab::Downloads => "Downloads".to_string(),
                    LibraryTab::Local => "Local Files".to_string(),
                    LibraryTab::Songs => "Songs".to_string(),
                    LibraryTab::Artists => "Artists".to_string(),
                    LibraryTab::Albums => "Albums".to_string(),
                    LibraryTab::Playlist(idx) => self.playlists.get(idx).map(|pl| ellipsize(&pl.name, 26)).unwrap_or_else(|| "Playlist".to_string()),
                },
                NavTab::Search => "Search".to_string(),
                NavTab::Recognize => "Song Recognition".to_string(),
                NavTab::Stats => "Listening Stats".to_string(),
                NavTab::Settings => "Settings".to_string(),
            },
        };

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

        if self.window_size.0 >= 1150.0 {
            nav_arrows = nav_arrows.push(
                container(
                    row![
                        text("•").size(12).color(with_alpha(muted, 0.6)),
                        text(current_page_title)
                            .size(13)
                            .font(semibold())
                            .color(Color::from_rgba(1.0, 1.0, 1.0, 0.78))
                            .wrapping(Wrapping::None),
                    ]
                    .spacing(6)
                    .align_y(Alignment::Center),
                )
                .clip(true)
                .padding([0, 4]),
            );
        }

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
        let search_box = container(search_row)
            .width(self.search_width())
            .padding([0, 14])
            .style(search_field_style(p));

        let account_initial = if self.yt.has_cookie() { "Y" } else { "E" };
        let settings_active =
            self.tab == NavTab::Settings && self.viewing_detail.is_none() && !self.show_expanded;
        let right_actions = row![
            self.with_tooltip(
                button(icons::icon(
                    icons::CAST,
                    18.0,
                    if self.active_cast_device.is_some() { color_from_rgb(p.primary) } else { muted }
                ))
                .padding(8)
                .style(icon_button_style(p, self.active_cast_device.is_some()))
                .on_press(Message::ToggleCastModal),
                "Cast to Device",
                tooltip::Position::Bottom,
            ),
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
                container(search_box)
                    .align_x(iced::alignment::Horizontal::Center),
                container(right_cluster)
                    .width(Length::FillPortion(1))
                    .align_x(iced::alignment::Horizontal::Right),
            ]
            .align_y(Alignment::Center),
        )
        .on_press(Message::DragWindow)
        .into()
    }

    pub(crate) fn view_collapsed_sidebar(&self) -> Element<'_, Message> {
        let p = self.palette;
        let muted = color_from_rgb(p.text_muted);

        let is_home = self.tab == NavTab::Home && self.viewing_detail.is_none() && !self.show_expanded;
        let is_explore = self.tab == NavTab::Explore && self.viewing_detail.is_none() && !self.show_expanded;
        let is_stats = self.tab == NavTab::Stats && self.viewing_detail.is_none() && !self.show_expanded;
        let is_recognize = self.tab == NavTab::Recognize && self.viewing_detail.is_none() && !self.show_expanded;
        let in_library = self.tab == NavTab::Playlists && self.viewing_detail.is_none() && !self.show_expanded;

        let nav_pill = |icon: &'static [u8], active: bool, tip: &'static str, msg: Message| -> Element<'_, Message> {
            self.with_tooltip(
                button(
                    container(icons::icon(icon, 20.0, if active { Color::WHITE } else { muted }))
                        .width(42)
                        .height(42)
                        .align_x(iced::alignment::Horizontal::Center)
                        .align_y(iced::alignment::Vertical::Center),
                )
                .padding(0)
                .style(nav_item_button_style(p, active))
                .on_press(msg),
                tip,
                tooltip::Position::Right,
            )
        };

        let mut col = column![
            nav_pill(icons::HOME, is_home, "Home", Message::SelectTab(NavTab::Home)),
            nav_pill(icons::COMPASS, is_explore, "Explore", Message::SelectTab(NavTab::Explore)),
            nav_pill(icons::STATS, is_stats, "Listening Stats", Message::SelectTab(NavTab::Stats)),
            nav_pill(icons::MIC, is_recognize, "Song Recognition", Message::SelectTab(NavTab::Recognize)),
            vertical_space().height(4),
            container(horizontal_space()).height(1).width(44).style(divider_style(0.10)),
            vertical_space().height(4),
            self.with_tooltip(
                button(
                    container(icons::icon(icons::LIBRARY, 20.0, if in_library { Color::WHITE } else { muted }))
                        .width(42)
                        .height(42)
                        .align_x(iced::alignment::Horizontal::Center)
                        .align_y(iced::alignment::Vertical::Center),
                )
                .padding(0)
                .style(nav_item_button_style(p, in_library))
                .on_press(Message::ToggleLeftSidebar),
                "Expand Your Library",
                tooltip::Position::Right,
            ),
            self.with_tooltip(
                button(
                    container(icons::icon(icons::PLUS, 18.0, muted))
                        .width(42)
                        .height(42)
                        .align_x(iced::alignment::Horizontal::Center)
                        .align_y(iced::alignment::Vertical::Center),
                )
                .padding(0)
                .style(nav_item_button_style(p, self.show_new_playlist_input))
                .on_press(Message::ExpandAndNewPlaylist),
                "Create Playlist",
                tooltip::Position::Right,
            ),
            vertical_space().height(4),
            self.with_tooltip(
                button(
                    container(icons::icon(icons::HEART_FILLED, 18.0, Color::WHITE))
                        .width(42)
                        .height(42)
                        .align_x(iced::alignment::Horizontal::Center)
                        .align_y(iced::alignment::Vertical::Center)
                        .style(liked_gradient_badge_style()),
                )
                .padding(2)
                .style(ghost_row_button_style(
                    p,
                    in_library && self.library_tab == LibraryTab::Liked,
                ))
                .on_press(Message::SelectLibraryTab(LibraryTab::Liked)),
                "Liked Songs",
                tooltip::Position::Right,
            ),
            self.with_tooltip(
                button(
                    container(icons::icon(
                        icons::DOWNLOAD,
                        18.0,
                        color_from_rgb(p.primary),
                    ))
                    .width(42)
                    .height(42)
                    .align_x(iced::alignment::Horizontal::Center)
                    .align_y(iced::alignment::Vertical::Center)
                    .style(downloads_badge_style()),
                )
                .padding(2)
                .style(ghost_row_button_style(
                    p,
                    in_library && self.library_tab == LibraryTab::Downloads,
                ))
                .on_press(Message::SelectLibraryTab(LibraryTab::Downloads)),
                "Downloads",
                tooltip::Position::Right,
            ),
            self.with_tooltip(
                button(
                    container(icons::icon(icons::FOLDER, 18.0, Color::WHITE))
                        .width(42)
                        .height(42)
                        .align_x(iced::alignment::Horizontal::Center)
                        .align_y(iced::alignment::Vertical::Center)
                        .style(local_badge_style()),
                )
                .padding(2)
                .style(ghost_row_button_style(
                    p,
                    in_library && self.library_tab == LibraryTab::Local,
                ))
                .on_press(Message::SelectLibraryTab(LibraryTab::Local)),
                "Local Files",
                tooltip::Position::Right,
            ),
        ]
        .spacing(4)
        .align_x(Alignment::Center);

        for (idx, pl) in self.playlists.iter().enumerate() {
            let active = in_library && self.library_tab == LibraryTab::Playlist(idx);
            col = col.push(self.with_tooltip(
                button(self.artwork_widget(pl.thumbnail_url.as_deref(), 42.0, false))
                    .padding(2)
                    .style(ghost_row_button_style(p, active))
                    .on_press(Message::SelectLibraryTab(LibraryTab::Playlist(idx))),
                pl.name.as_str(),
                tooltip::Position::Right,
            ));
        }

        for pl in self.account_playlists.iter().take(6) {
            let active = self.active_playlist_id() == Some(pl.id.as_str());
            col = col.push(self.with_tooltip(
                button(self.artwork_widget(pl.thumbnail_url.as_deref(), 42.0, false))
                    .padding(2)
                    .style(ghost_row_button_style(p, active))
                    .on_press(Message::OpenPlaylist {
                        id: pl.id.clone(),
                        title: pl.title.clone(),
                    }),
                pl.title.as_str(),
                tooltip::Position::Right,
            ));
        }

        container(self.vscroll(col))
            .width(72)
            .height(Length::Fill)
            .padding([12, 4])
            .align_x(iced::alignment::Horizontal::Center)
            .style(glass_panel_style(p))
            .into()
    }

    pub(crate) fn view_left_sidebar(&self) -> Element<'_, Message> {
        let p = self.palette;
        let muted = color_from_rgb(p.text_muted);

        let is_home = self.tab == NavTab::Home && self.viewing_detail.is_none() && !self.show_expanded;
        let is_explore = self.tab == NavTab::Explore && self.viewing_detail.is_none() && !self.show_expanded;
        let is_stats = self.tab == NavTab::Stats && self.viewing_detail.is_none() && !self.show_expanded;
        let is_recognize = self.tab == NavTab::Recognize && self.viewing_detail.is_none() && !self.show_expanded;
        let in_library = self.tab == NavTab::Playlists && self.viewing_detail.is_none() && !self.show_expanded;

        let primary_nav_item = |icon: &'static [u8], label: &'static str, active: bool, msg: Message| -> Element<'_, Message> {
            button(
                row![
                    icons::icon(icon, 20.0, if active { Color::WHITE } else { muted }),
                    text(label)
                        .size(14)
                        .font(if active { bold() } else { semibold() })
                        .color(if active { Color::WHITE } else { muted }),
                ]
                .spacing(14)
                .align_y(Alignment::Center),
            )
            .width(Length::Fill)
            .padding([9, 12])
            .style(nav_item_button_style(p, active))
            .on_press(msg)
            .into()
        };

        let primary_nav = column![
            primary_nav_item(icons::HOME, "Home", is_home, Message::SelectTab(NavTab::Home)),
            primary_nav_item(icons::COMPASS, "Explore", is_explore, Message::SelectTab(NavTab::Explore)),
            primary_nav_item(icons::STATS, "Listening Stats", is_stats, Message::SelectTab(NavTab::Stats)),
            primary_nav_item(icons::MIC, "Song Recognition", is_recognize, Message::SelectTab(NavTab::Recognize)),
        ]
        .spacing(2);

        let header = row![
            button(
                row![
                    icons::icon(icons::LIBRARY, 20.0, if in_library { Color::WHITE } else { muted }),
                    text("Your Library")
                        .size(14)
                        .font(bold())
                        .color(if in_library { Color::WHITE } else { muted }),
                ]
                .spacing(10)
                .align_y(Alignment::Center),
            )
            .padding([6, 8])
            .style(ghost_row_button_style(p, false))
            .on_press(Message::SelectTab(NavTab::Playlists)),
            horizontal_space(),
            button(icons::icon(icons::PLUS, 17.0, muted))
                .padding(7)
                .style(icon_button_style(p, self.show_new_playlist_input))
                .on_press(Message::ToggleNewPlaylistInput),
            button(icons::icon(icons::BACK, 15.0, muted))
                .padding(7)
                .style(icon_button_style(p, false))
                .on_press(Message::ToggleLeftSidebar),
        ]
        .align_y(Alignment::Center);

        let filter_pill = |filter: LibraryFilter, label: &'static str| -> Element<'_, Message> {
            let active = self.library_filter == filter;
            button(
                text(label)
                    .size(11.5)
                    .font(if active { bold() } else { semibold() })
                    .color(if active { Color::BLACK } else { Color::WHITE }),
            )
            .padding([4, 10])
            .style(top_pill_button_style(p, active))
            .on_press(Message::LibraryFilterSelected(filter))
            .into()
        };

        let filter_pills = scrollable(
            row![
                filter_pill(LibraryFilter::Playlists, "Playlists"),
                filter_pill(LibraryFilter::Songs, "Songs"),
                filter_pill(LibraryFilter::Albums, "Albums"),
                filter_pill(LibraryFilter::Artists, "Artists"),
                filter_pill(LibraryFilter::Downloads, "Downloaded"),
            ]
            .spacing(5)
            .align_y(Alignment::Center),
        )
        .direction(scrollable::Direction::Horizontal(
            scrollable::Scrollbar::new()
                .width(0)
                .scroller_width(0)
                .margin(0),
        ))
        .height(30)
        .width(Length::Fill);

        let mut list = column![].spacing(3);

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

        let show_songs = matches!(self.library_filter, LibraryFilter::All | LibraryFilter::Songs);
        let show_artists = matches!(self.library_filter, LibraryFilter::All | LibraryFilter::Artists);
        let show_albums = matches!(self.library_filter, LibraryFilter::All | LibraryFilter::Albums);
        let show_playlists =
            matches!(self.library_filter, LibraryFilter::All | LibraryFilter::Playlists);
        let show_downloads =
            matches!(self.library_filter, LibraryFilter::All | LibraryFilter::Downloads);
        let show_local = matches!(self.library_filter, LibraryFilter::All | LibraryFilter::Local);

        if show_songs {
            let songs_active = in_library && self.library_tab == LibraryTab::Songs;
            let songs_badge = container(icons::icon(icons::DISC, 18.0, Color::WHITE))
                .width(40)
                .height(40)
                .align_x(iced::alignment::Horizontal::Center)
                .align_y(iced::alignment::Vertical::Center)
                .style(local_badge_style());
            list = list.push(self.sidebar_badge_item(
                "Songs",
                "Master list • All tracks".to_string(),
                songs_badge.into(),
                songs_active,
                Message::SelectLibraryTab(LibraryTab::Songs),
            ));
        }

        if show_artists {
            let artists_active = in_library && self.library_tab == LibraryTab::Artists;
            let artists_badge = container(icons::icon(icons::USER, 18.0, Color::WHITE))
                .width(40)
                .height(40)
                .align_x(iced::alignment::Horizontal::Center)
                .align_y(iced::alignment::Vertical::Center)
                .style(liked_gradient_badge_style());
            list = list.push(self.sidebar_badge_item(
                "Artists",
                "Library • Artists & tiles".to_string(),
                artists_badge.into(),
                artists_active,
                Message::SelectLibraryTab(LibraryTab::Artists),
            ));
        }

        if show_albums {
            let albums_active = in_library && self.library_tab == LibraryTab::Albums;
            let albums_badge = container(icons::icon(icons::LIBRARY, 18.0, Color::WHITE))
                .width(40)
                .height(40)
                .align_x(iced::alignment::Horizontal::Center)
                .align_y(iced::alignment::Vertical::Center)
                .style(downloads_badge_style());
            list = list.push(self.sidebar_badge_item(
                "Albums",
                "Library • Saved & local albums".to_string(),
                albums_badge.into(),
                albums_active,
                Message::SelectLibraryTab(LibraryTab::Albums),
            ));
        }

        if show_playlists {
            let liked_active = in_library && self.library_tab == LibraryTab::Liked;
            let liked_badge = container(icons::icon(icons::HEART_FILLED, 18.0, Color::WHITE))
                .width(40)
                .height(40)
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
                container(icons::icon(icons::DOWNLOAD, 18.0, color_from_rgb(p.primary)))
                    .width(40)
                    .height(40)
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
            let local_badge = container(icons::icon(icons::FOLDER, 18.0, Color::WHITE))
                .width(40)
                .height(40)
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

        let sidebar_content = column![
            primary_nav,
            vertical_space().height(6),
            container(horizontal_space()).height(1).width(Length::Fill).style(divider_style(0.08)),
            vertical_space().height(6),
            header,
            vertical_space().height(4),
            filter_pills,
            vertical_space().height(4),
            self.vscroll(list),
        ]
        .spacing(2);

        container(sidebar_content)
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
        let icon = self.artwork_widget(thumb, 40.0, false);
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
                        .size(13.5)
                        .font(bold())
                        .wrapping(Wrapping::None)
                        .color(if active {
                            color_from_rgb(p.primary)
                        } else {
                            Color::WHITE
                        }),
                    text(ellipsize(&subtitle, 30))
                        .size(11.5)
                        .wrapping(Wrapping::None)
                        .color(color_from_rgb(p.text_muted)),
                ]
                .spacing(2)
                .width(Length::Fill),
            ]
            .spacing(10)
            .align_y(Alignment::Center),
        )
        .width(Length::Fill)
        .padding([5, 6])
        .style(ghost_row_button_style(p, active))
        .on_press(msg)
        .into()
    }
}
