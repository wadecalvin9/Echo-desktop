use iced::alignment::Alignment;
use iced::widget::text::Wrapping;
use iced::widget::{
    scrollable,
    button, column, container, horizontal_space, image, mouse_area, row, stack, text,
    tooltip,
};
use iced::{Color, Element, Font, Length};

use echo_audio::PlaybackStatus;
use echo_core::Track;
use echo_youtube::BrowseItem;

use crate::icons;
use crate::theme::{
    scroll_style,
    card_button_style, card_hover_scrim_style, card_play_fab_style, color_from_rgb,
    ghost_row_button_style, icon_button_style, local_badge_style, quick_tile_button_style,
    quick_tile_play_badge_style, tooltip_style, with_alpha,
};
use crate::{
    bold, ellipsize, mono, semibold, EchoApp, LibraryTab, Message, NavTab,
};

impl EchoApp {    pub(crate) fn artwork_widget(&self, url: Option<&str>, size: f32, circle: bool) -> Element<'_, Message> {
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

    pub(crate) fn item_action(item: &BrowseItem) -> Message {
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
    pub(crate) fn quick_tile<'a>(&'a self, item: &'a BrowseItem) -> Element<'a, Message> {
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

    pub(crate) fn browse_card<'a>(&'a self, item: &'a BrowseItem) -> Element<'a, Message> {
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



    pub(crate) fn browse_item_row<'a>(&'a self, num: usize, item: &'a BrowseItem) -> Element<'a, Message> {
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

    pub(crate) fn table_track_row<'a>(
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

        let subtle = with_alpha(muted, 0.90);
        let mut actions = row![].spacing(2).align_y(Alignment::Center);

        if is_liked {
            actions = actions.push(
                button(icons::icon(icons::HEART_FILLED, 16.0, accent))
                    .padding(6)
                    .style(icon_button_style(p, false))
                    .on_press(Message::ToggleLikeTrack(track.clone())),
            );
        } else if row_hovered {
            actions = actions.push(
                button(icons::icon(icons::HEART_OUTLINE, 16.0, subtle))
                    .padding(6)
                    .style(icon_button_style(p, false))
                    .on_press(Message::ToggleLikeTrack(track.clone())),
            );
        } else {
            actions = actions.push(container(horizontal_space()).width(28));
        }

        if row_hovered {
            actions = actions.push(
                button(icons::icon(icons::PLUS, 15.0, subtle))
                    .padding(6)
                    .style(icon_button_style(p, false))
                    .on_press(Message::QueueAddBack(track.clone())),
            );
        }

        if !track.is_local {
            let is_downloaded = self.downloaded_id_set.contains(&track.id);
            let is_downloading = self.downloading_ids.contains(&track.id);
            if is_downloaded || is_downloading {
                let dl_color = accent;
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
            } else if row_hovered {
                actions = actions.push(
                    button(icons::icon(icons::DOWNLOAD, 15.0, subtle))
                        .padding(6)
                        .style(icon_button_style(p, false))
                        .on_press(Message::DownloadTrack(track.clone())),
                );
            }
        }

        if self.tab == NavTab::Playlists {
            if let LibraryTab::Playlist(idx) = self.library_tab {
                if let Some(pl) = self.playlists.get(idx) {
                    if row_hovered {
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
        }

        if row_hovered {
            actions = actions.push(
                button(icons::icon(icons::MORE_HORIZ, 15.0, subtle))
                    .padding(6)
                    .style(icon_button_style(p, false))
                    .on_press(Message::OpenTrackMenu(track.clone())),
            );
        }

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

    pub(crate) fn with_tooltip<'a>(
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

    pub(crate) fn live_eq_bars(&self, playing: bool, color: Color) -> Element<'_, Message> {
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

    pub(crate) fn live_spectrum_strip(&self, accent: Color) -> Element<'_, Message> {
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


    pub(crate) fn vscroll<'a>(&self, content: impl Into<Element<'a, Message>>) -> Element<'a, Message> {
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
}
