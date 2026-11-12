use std::collections::HashSet;

use iced::alignment::Alignment;
use iced::widget::text::Wrapping;
use iced::widget::{
    button, column, container, horizontal_space, rich_text, row, scrollable, span, text,
    vertical_space,
};
use iced::{Color, Element, Font, Length};

use echo_lyrics::{contains_cjk_or_hangul_or_kana, romanize_text};

use crate::icons;
use crate::theme::{
    color_from_rgb, elevated_card_style, ghost_row_button_style, glass_panel_style,
    icon_button_style, outlined_pill_button_style, scroll_style, top_pill_button_style,
    with_alpha,
};
use crate::{
    bold, ellipsize, semibold, EchoApp, Message, RightTab,
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

    pub(crate) fn view_lyrics_inner(&self, active_size: u16, base_size: u16) -> Element<'_, Message> {
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

        let share_btn = button(
            row![
                icons::icon(icons::SHARE, 12.0, muted),
                text("Card").size(11).color(muted),
            ]
            .spacing(4)
            .align_y(Alignment::Center),
        )
        .padding([3, 8])
        .style(top_pill_button_style(p, false))
        .on_press(Message::ShareLyricsCard);

        let offset_sec = self.settings.lyrics_offset_ms as f32 / 1000.0;
        let sync_stepper = row![
            button(text("–0.5s").size(10))
                .padding([3, 6])
                .style(top_pill_button_style(p, false))
                .on_press(Message::AdjustLyricsOffset(-500)),
            button(text(if self.settings.lyrics_offset_ms == 0 { "Sync: 0.0s".to_string() } else { format!("Sync: {:+.1}s", offset_sec) }).size(10))
                .padding([3, 6])
                .style(top_pill_button_style(p, self.settings.lyrics_offset_ms != 0))
                .on_press(Message::ResetLyricsOffset),
            button(text("+0.5s").size(10))
                .padding([3, 6])
                .style(top_pill_button_style(p, false))
                .on_press(Message::AdjustLyricsOffset(500)),
        ]
        .spacing(2)
        .align_y(Alignment::Center);

        let is_cinema = active_size >= 24;

        if !res.synced.is_empty() {
            let calibrated_ms = (self.position_ms as i64 + self.settings.lyrics_offset_ms).max(0) as u64;
            let active_idx = res.active_index(calibrated_ms);
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
                            let sung = calibrated_ms >= w.time_ms;
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
                    sync_stepper,
                    share_btn,
                    rom_btn,
                ]
                .spacing(6)
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

}
