use std::collections::HashSet;

use iced::alignment::Alignment;
use iced::widget::text::Wrapping;
use iced::widget::{
    button, column, container, horizontal_space, rich_text, row, scrollable, span,
    tooltip, vertical_space,
};
use iced::{Color, Element, Font, Length};

use echo_audio::PlaybackStatus;
use echo_lyrics::{contains_cjk_or_hangul_or_kana, romanize_text};

use crate::icons;
use crate::theme::{
    badge_pill_style, color_from_rgb, elevated_card_style, ghost_row_button_style, glass_panel_style,
    icon_button_style, outlined_pill_button_style, scroll_style, top_pill_button_style,
    with_alpha,
};
use crate::{
    bold, ellipsize, mono, semibold, text, EchoApp, Message, RightTab,
};

impl EchoApp {    pub(crate) fn view_right_rail(&self) -> Element<'_, Message> {
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
            container(
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
            )
            .width(Length::Fill)
            .clip(true),
            button(icons::icon(icons::CLOSE, 16.0, muted))
                .padding(6)
                .style(icon_button_style(p, false))
                .on_press(Message::ToggleRightSidebar),
        ]
        .spacing(8)
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
                            let line_btn = button(
                                text(if line.text.is_empty() { "♪" } else { &line.text })
                                    .size(14)
                                    .font(bold())
                                    .color(if active_line { color_from_rgb(p.primary) } else { muted })
                                    .wrapping(Wrapping::None),
                            )
                            .padding([3, 6])
                            .width(Length::Fill)
                            .style(ghost_row_button_style(p, active_line))
                            .on_press(Message::SeekToMs(line.time_ms));
                            preview_col = preview_col.push(line_btn);
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
                    container(
                        column![
                            icons::icon(icons::QUEUE, 32.0, muted),
                            text("Queue is empty").size(15).font(bold()),
                            text("Add songs to listen next or queue up an album.").size(12).color(muted),
                        ]
                        .spacing(8)
                        .align_x(Alignment::Center),
                    )
                    .width(Length::Fill)
                    .padding(32)
                    .into()
                } else {
                    let mut list = column![].spacing(14);

                    let cur_idx = self.queue.current_index.unwrap_or(0);
                    let mut prev_items: Vec<(usize, &echo_core::QueueItem)> = Vec::new();
                    let mut user_items: Vec<(usize, &echo_core::QueueItem)> = Vec::new();
                    let mut context_items: Vec<(usize, &echo_core::QueueItem)> = Vec::new();

                    for (idx, item) in self.queue.items.iter().enumerate() {
                        if idx < cur_idx {
                            prev_items.push((idx, item));
                        } else if idx > cur_idx {
                            if item.is_user_queued {
                                user_items.push((idx, item));
                            } else {
                                context_items.push((idx, item));
                            }
                        }
                    }

                    if !prev_items.is_empty() {
                        let mut prev_col = column![
                            row![
                                text("Previous Tracks").size(12).font(bold()).color(muted),
                                horizontal_space(),
                                text(format!("{} earlier", prev_items.len())).size(11).color(muted),
                            ]
                            .align_y(Alignment::Center),
                        ]
                        .spacing(4);

                        let show_count = prev_items.len().min(3);
                        for (idx, item) in prev_items.iter().rev().take(show_count).rev() {
                            let idx = *idx;
                            prev_col = prev_col.push(
                                button(
                                    row![
                                        self.artwork_widget(
                                            item.track.thumbnail_url.as_deref(),
                                            32.0,
                                            false,
                                        ),
                                        column![
                                            text(ellipsize(&item.track.title, 24))
                                                .size(12)
                                                .font(semibold())
                                                .color(with_alpha(Color::WHITE, 0.60))
                                                .wrapping(Wrapping::None),
                                            text(ellipsize(&item.track.artist_display(), 26))
                                                .size(11)
                                                .color(with_alpha(muted, 0.70))
                                                .wrapping(Wrapping::None),
                                        ]
                                        .spacing(2)
                                        .width(Length::Fill),
                                    ]
                                    .spacing(8)
                                    .align_y(Alignment::Center),
                                )
                                .width(Length::Fill)
                                .padding([4, 6])
                                .style(ghost_row_button_style(p, false))
                                .on_press(Message::QueueJump(idx)),
                            );
                        }
                        list = list.push(prev_col);
                    }

                    // Section 1: Now Playing
                    if let Some(cur_idx) = self.queue.current_index {
                        if let Some(cur) = self.queue.items.get(cur_idx) {
                            list = list.push(
                                column![
                                    text("Now Playing").size(12).font(bold()).color(muted),
                                    container(
                                        button(
                                            row![
                                                self.artwork_widget(
                                                    cur.track.thumbnail_url.as_deref(),
                                                    40.0,
                                                    false,
                                                ),
                                                column![
                                                    text(ellipsize(&cur.track.title, 26))
                                                        .size(13)
                                                        .font(bold())
                                                        .color(color_from_rgb(p.primary))
                                                        .wrapping(Wrapping::None),
                                                    text(ellipsize(&cur.track.artist_display(), 28))
                                                        .size(11)
                                                        .color(muted)
                                                        .wrapping(Wrapping::None),
                                                ]
                                                .spacing(2)
                                                .width(Length::Fill),
                                                container(icons::icon(icons::VOLUME, 14.0, color_from_rgb(p.primary)))
                                                    .padding([2, 6]),
                                            ]
                                            .spacing(10)
                                            .align_y(Alignment::Center),
                                        )
                                        .width(Length::Fill)
                                        .padding([6, 8])
                                        .style(ghost_row_button_style(p, true))
                                        .on_press(Message::ToggleExpandedPlayer),
                                    )
                                    .style(elevated_card_style(p)),
                                ]
                                .spacing(6),
                            );
                        }
                    }

                    let total_items = self.queue.items.len();

                    // Section 2: Next In Queue (User-Queued)
                    if !user_items.is_empty() {
                        let mut user_col = column![
                            row![
                                text("Next in Queue").size(12).font(bold()),
                                horizontal_space(),
                                text("Added by you").size(10).font(semibold()).color(color_from_rgb(p.primary)),
                            ]
                            .align_y(Alignment::Center),
                        ]
                        .spacing(4);

                        for (idx, item) in &user_items {
                            let idx = *idx;
                            let is_last = idx + 1 >= total_items;
                            user_col = user_col.push(
                                row![
                                    button(
                                        row![
                                            self.artwork_widget(
                                                item.track.thumbnail_url.as_deref(),
                                                34.0,
                                                false,
                                            ),
                                            column![
                                                text(ellipsize(&item.track.title, 20))
                                                    .size(12)
                                                    .font(semibold())
                                                    .wrapping(Wrapping::None),
                                                text(ellipsize(&item.track.artist_display(), 22))
                                                    .size(11)
                                                    .color(muted)
                                                    .wrapping(Wrapping::None),
                                            ]
                                            .spacing(2)
                                            .width(Length::Fill),
                                        ]
                                        .spacing(8)
                                        .align_y(Alignment::Center),
                                    )
                                    .width(Length::Fill)
                                    .padding([4, 6])
                                    .style(ghost_row_button_style(p, false))
                                    .on_press(Message::QueueJump(idx)),
                                    button(icons::icon(icons::CHEVRON_UP, 13.0, muted))
                                        .padding([4, 4])
                                        .style(icon_button_style(p, false))
                                        .on_press(Message::QueueMoveUp(idx)),
                                    button(icons::icon(icons::CHEVRON_DOWN, 13.0, if is_last { with_alpha(muted, 0.4) } else { muted }))
                                        .padding([4, 4])
                                        .style(icon_button_style(p, false))
                                        .on_press(Message::QueueMoveDown(idx)),
                                    button(icons::icon(icons::CLOSE, 12.0, muted))
                                        .padding([4, 4])
                                        .style(icon_button_style(p, false))
                                        .on_press(Message::QueueRemove(idx)),
                                ]
                                .spacing(2)
                                .align_y(Alignment::Center),
                            );
                        }
                        list = list.push(user_col);
                    }

                    // Section 3: Next From Context (Album, Playlist, Artist, etc.)
                    if !context_items.is_empty() {
                        let context_label = self
                            .queue
                            .current()
                            .and_then(|c| c.track.album.as_ref().map(|a| format!("Next from: {}", a.title)))
                            .unwrap_or_else(|| "Next up".to_string());

                        let mut ctx_col = column![
                            text(ellipsize(&context_label, 30))
                                .size(12)
                                .font(bold())
                                .color(muted),
                        ]
                        .spacing(4);

                        for (idx, item) in &context_items {
                            let idx = *idx;
                            let is_last = idx + 1 >= total_items;
                            ctx_col = ctx_col.push(
                                row![
                                    button(
                                        row![
                                            self.artwork_widget(
                                                item.track.thumbnail_url.as_deref(),
                                                34.0,
                                                false,
                                            ),
                                            column![
                                                container(
                                                    text(ellipsize(&item.track.title, 26))
                                                        .size(12)
                                                        .font(semibold())
                                                        .wrapping(Wrapping::None),
                                                )
                                                .width(Length::Fill)
                                                .clip(true),
                                                container(
                                                    text(ellipsize(&item.track.artist_display(), 28))
                                                        .size(11)
                                                        .color(muted)
                                                        .wrapping(Wrapping::None),
                                                )
                                                .width(Length::Fill)
                                                .clip(true),
                                            ]
                                            .spacing(2)
                                            .width(Length::Fill),
                                        ]
                                        .spacing(8)
                                        .align_y(Alignment::Center),
                                    )
                                    .width(Length::Fill)
                                    .padding([4, 6])
                                    .style(ghost_row_button_style(p, false))
                                    .on_press(Message::QueueJump(idx)),
                                    button(icons::icon(icons::CHEVRON_UP, 13.0, muted))
                                        .padding([4, 4])
                                        .style(icon_button_style(p, false))
                                        .on_press(Message::QueueMoveUp(idx)),
                                    button(icons::icon(icons::CHEVRON_DOWN, 13.0, if is_last { with_alpha(muted, 0.4) } else { muted }))
                                        .padding([4, 4])
                                        .style(icon_button_style(p, false))
                                        .on_press(Message::QueueMoveDown(idx)),
                                    button(icons::icon(icons::CLOSE, 12.0, with_alpha(muted, 0.75)))
                                        .padding([4, 5])
                                        .style(icon_button_style(p, false))
                                        .on_press(Message::QueueRemove(idx)),
                                ]
                                .spacing(2)
                                .align_y(Alignment::Center),
                            );
                        }
                        list = list.push(ctx_col);
                    }

                    if user_items.is_empty() && context_items.is_empty() {
                        list = list.push(
                            container(
                                text(if self.settings.auto_load_more {
                                    "Continuous auto-recommendations active when current track ends."
                                } else {
                                    "End of queue. Add tracks to continue playback."
                                })
                                .size(12)
                                .color(muted),
                            )
                            .padding(8),
                        );
                    }

                    column![
                        row![
                            text(format!("{} tracks", self.queue.items.len()))
                                .size(11)
                                .font(mono())
                                .color(muted),
                            horizontal_space(),
                            button(
                                row![
                                    icons::icon(icons::PLUS, 11.0, Color::WHITE),
                                    text("Save").size(11).font(semibold()).color(Color::WHITE),
                                ]
                                .spacing(4)
                                .align_y(Alignment::Center),
                            )
                            .padding([4, 8])
                            .style(top_pill_button_style(p, false))
                            .on_press(Message::SaveQueueAsPlaylist),
                            button(text("Clear").size(11).color(muted))
                                .padding([4, 8])
                                .style(top_pill_button_style(p, false))
                                .on_press(Message::QueueClear),
                        ]
                        .spacing(6)
                        .align_y(Alignment::Center),
                        self.vscroll(list),
                    ]
                    .spacing(8)
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

    pub(crate) fn view_lyrics_fallback_visualizer(&self, is_cinema: bool, base_size: u16) -> Element<'_, Message> {
        let p = self.palette;
        let muted = color_from_rgb(p.text_muted);
        let accent = color_from_rgb(p.primary);
        let playing = matches!(self.status, PlaybackStatus::Playing);

        let mut visualizer_bars = row![].spacing(if is_cinema { 6 } else { 4 }).align_y(Alignment::End);
        let max_height = if is_cinema { 120.0 } else { 46.0 };
        let min_height = if is_cinema { 8.0 } else { 4.0 };

        for (i, &band) in self.spectrum_bands.iter().enumerate() {
            let amp = if playing {
                let wave = ((self.anim_phase * 2.2 + i as f32 * 0.75).sin() * 0.5 + 0.5) * 0.28;
                (band + wave).clamp(0.08, 1.0)
            } else {
                0.10
            };
            let bar_h = (max_height * amp).max(min_height);
            let bar_color = if i % 2 == 0 {
                accent
            } else {
                with_alpha(Color::WHITE, 0.70 + 0.30 * amp)
            };
            visualizer_bars = visualizer_bars.push(
                container(horizontal_space())
                    .width(if is_cinema { 8 } else { 4 })
                    .height(bar_h)
                    .style(move |_| container::Style {
                        background: Some(iced::Background::Color(bar_color)),
                        border: iced::Border {
                            radius: 3.0.into(),
                            ..Default::default()
                        },
                        ..Default::default()
                    }),
            );
        }

        if is_cinema {
            let track_title = self.queue.current().map(|c| c.track.title.as_str()).unwrap_or("Playing");
            let track_artist = self.queue.current().map(|c| c.track.artist_display()).unwrap_or_default();

            container(
                column![
                    container(visualizer_bars)
                        .height(130)
                        .align_y(iced::alignment::Vertical::Bottom),
                    vertical_space().height(16),
                    container(
                        row![
                            icons::icon(icons::DISC, 14.0, accent),
                            text("Audio Visualizer Active").size(12).font(bold()).color(Color::WHITE),
                        ]
                        .spacing(6)
                        .align_y(Alignment::Center),
                    )
                    .padding([5, 14])
                    .style(badge_pill_style(p)),
                    vertical_space().height(4),
                    text(track_title)
                        .size(24)
                        .font(bold())
                        .wrapping(Wrapping::None),
                    text(track_artist)
                        .size(15)
                        .color(muted)
                        .wrapping(Wrapping::None),
                    vertical_space().height(8),
                    text("No synchronized lyrics loaded for this track.")
                        .size(13)
                        .color(with_alpha(muted, 0.8)),
                ]
                .spacing(4)
                .align_x(Alignment::Center),
            )
            .width(Length::Fill)
            .height(Length::Fill)
            .align_x(iced::alignment::Horizontal::Center)
            .align_y(iced::alignment::Vertical::Center)
            .into()
        } else {
            container(
                column![
                    container(visualizer_bars)
                        .height(55)
                        .align_y(iced::alignment::Vertical::Bottom),
                    vertical_space().height(8),
                    text("Visualizer Active").size(13).font(bold()),
                    text("No synced lyrics available.").size(base_size).color(muted),
                ]
                .spacing(4)
                .align_x(Alignment::Center),
            )
            .width(Length::Fill)
            .padding(24)
            .align_x(iced::alignment::Horizontal::Center)
            .into()
        }
    }

    pub(crate) fn view_lyrics_inner(&self, active_size: u16, base_size: u16) -> Element<'_, Message> {
        let p = self.palette;
        let muted = color_from_rgb(p.text_muted);
        let is_cinema = active_size >= 24;

        if self.lyrics_loading {
            return container(
                column![
                    icons::icon(icons::LYRICS, if is_cinema { 32.0 } else { 20.0 }, color_from_rgb(p.primary)),
                    text("Fetching synchronized lyrics...").size(base_size).color(muted),
                ]
                .spacing(8)
                .align_x(Alignment::Center),
            )
            .width(Length::Fill)
            .padding(if is_cinema { 40 } else { 20 })
            .align_x(iced::alignment::Horizontal::Center)
            .into();
        }

        let Some(res) = &self.active_lyrics else {
            return self.view_lyrics_fallback_visualizer(is_cinema, base_size);
        };

        let has_cjk = res.synced.iter().any(|l| contains_cjk_or_hangul_or_kana(&l.text))
            || res.plain.as_deref().map(contains_cjk_or_hangul_or_kana).unwrap_or(false);

        let rom_btn: Option<Element<'_, Message>> = if has_cjk || self.settings.romanize_lyrics {
            Some(
                self.with_tooltip(
                    button(
                        text(if is_cinema { "Romanize" } else { "Rom" })
                            .size(11)
                            .font(if self.settings.romanize_lyrics { bold() } else { semibold() })
                            .wrapping(Wrapping::None),
                    )
                    .padding([3, if is_cinema { 10 } else { 8 }])
                    .style(top_pill_button_style(p, self.settings.romanize_lyrics))
                    .on_press(Message::ToggleRomanizeLyrics),
                    "Romanize non-Latin lyrics (pinyin/romaji)",
                    tooltip::Position::Top,
                ),
            )
        } else {
            None
        };

        let share_content: Element<'_, Message> = if is_cinema {
            row![
                icons::icon(icons::SHARE, 12.0, muted),
                text("Card").size(11).wrapping(Wrapping::None).color(muted),
            ]
            .spacing(4)
            .align_y(Alignment::Center)
            .into()
        } else {
            icons::icon(icons::SHARE, 12.0, muted).into()
        };

        let share_btn = self.with_tooltip(
            button(share_content)
                .padding([3, if is_cinema { 8 } else { 6 }])
                .style(top_pill_button_style(p, false))
                .on_press(Message::ShareLyricsCard),
            "Share lyrics card",
            tooltip::Position::Top,
        );

        let offset_sec = self.settings.lyrics_offset_ms as f32 / 1000.0;
        let sync_stepper = row![
            button(text("–0.5s").size(10).wrapping(Wrapping::None))
                .padding([3, 6])
                .style(top_pill_button_style(p, false))
                .on_press(Message::AdjustLyricsOffset(-500)),
            button(
                text(if self.settings.lyrics_offset_ms == 0 {
                    "Sync: 0.0s".to_string()
                } else {
                    format!("Sync: {:+.1}s", offset_sec)
                })
                .size(10)
                .wrapping(Wrapping::None),
            )
            .padding([3, 6])
            .style(top_pill_button_style(p, self.settings.lyrics_offset_ms != 0))
            .on_press(Message::ResetLyricsOffset),
            button(text("+0.5s").size(10).wrapping(Wrapping::None))
                .padding([3, 6])
                .style(top_pill_button_style(p, false))
                .on_press(Message::AdjustLyricsOffset(500)),
        ]
        .spacing(2)
        .align_y(Alignment::Center);

        if !res.synced.is_empty() {
            let calibrated_ms = (self.position_ms as i64 + self.settings.lyrics_offset_ms).max(0) as u64;
            let active_idx = res.active_index(calibrated_ms);
            let top_pad = if is_cinema {
                (self.window_size.1 * 0.22).clamp(70.0, 160.0)
            } else {
                80.0
            };
            let bottom_pad = if is_cinema {
                (self.window_size.1 * 0.32).clamp(120.0, 240.0)
            } else {
                120.0
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
                            let sung = calibrated_ms >= w.time_ms;
                            let is_current = sung && line.words.get(wi + 1).map(|nw| calibrated_ms < nw.time_ms).unwrap_or(true);
                            let w_color = if is_current {
                                Color::WHITE
                            } else if sung {
                                Color::from_rgba(1.0, 1.0, 1.0, 0.92)
                            } else {
                                Color::from_rgba(1.0, 1.0, 1.0, 0.36)
                            };
                            let piece = if w.ends_with_space && wi + 1 < line.words.len() {
                                format!("{} ", w.text)
                            } else {
                                w.text.clone()
                            };
                            let mut s = span::<Message, Font>(piece).color(w_color);
                            if is_current {
                                s = s.font(bold());
                            }
                            spans.push(s);
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

            let header_bar: Element<'_, Message> = if is_cinema {
                let mut controls = row![sync_stepper, share_btn].spacing(6).align_y(Alignment::Center);
                if let Some(rb) = rom_btn {
                    controls = controls.push(rb);
                }
                row![
                    text(format!("Synced Lyrics • {}", res.source.label()))
                        .size(12)
                        .wrapping(Wrapping::None)
                        .color(Color::from_rgba(1.0, 1.0, 1.0, 0.65)),
                    horizontal_space(),
                    controls,
                ]
                .spacing(10)
                .width(Length::Fill)
                .align_y(Alignment::Center)
                .into()
            } else {
                let mut top_actions = row![share_btn].spacing(4).align_y(Alignment::Center);
                if let Some(rb) = rom_btn {
                    top_actions = top_actions.push(rb);
                }
                let top_row = row![
                    container(
                        text(format!("Synced Lyrics • {}", res.source.label()))
                            .size(11)
                            .font(semibold())
                            .wrapping(Wrapping::None)
                            .color(muted),
                    )
                    .width(Length::Fill)
                    .clip(true),
                    top_actions,
                ]
                .spacing(6)
                .width(Length::Fill)
                .align_y(Alignment::Center);

                let sync_row = row![
                    text("Sync timing").size(11).color(muted),
                    horizontal_space(),
                    sync_stepper,
                ]
                .spacing(4)
                .width(Length::Fill)
                .align_y(Alignment::Center);

                column![top_row, sync_row]
                    .spacing(6)
                    .width(Length::Fill)
                    .into()
            };

            column![
                header_bar,
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
            let mut plain_actions = row![].spacing(4).align_y(Alignment::Center);
            if let Some(rb) = rom_btn {
                plain_actions = plain_actions.push(rb);
            }
            let plain_header = row![
                container(
                    text(format!("Lyrics • {}", res.source.label()))
                        .size(11)
                        .wrapping(Wrapping::None)
                        .color(muted),
                )
                .width(Length::Fill)
                .clip(true),
                plain_actions,
            ]
            .spacing(6)
            .width(Length::Fill)
            .align_y(Alignment::Center);

            column![
                plain_header,
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

}
