use std::time::Instant;

use iced::alignment::Alignment;
use iced::widget::text::Wrapping;
use iced::widget::{
    button, column, container, horizontal_space, image, mouse_area, row, slider, stack,
    tooltip, vertical_space,
};
use iced::{Color, Element, Length};

use echo_audio::PlaybackStatus;
use echo_core::{AudioQuality, DynamicPalette, RepeatMode, Track};

use crate::icons;
use crate::theme::{
    ambient_scrim_style, artwork_frame_style, badge_pill_style, bento_card_style,
    color_from_rgb, divider_style, dock_play_button_style, expanded_stage_style,
    ghost_row_button_style, icon_button_style, logo_badge_style, mini_player_fallback_style,
    mini_player_scrim_style, outlined_pill_button_style, player_dock_style,
    popover_panel_style, slim_slider_style, soft_pill_button_style, top_pill_button_style,
    up_next_card_button_style, window_control_button_style, with_alpha,
};
use crate::{
    bold, ellipsize, format_ms, mono, semibold, text, EchoApp, Message, RightTab,
    SleepTimerState,
};

impl EchoApp {    pub(crate) fn view_expanded_player(&self) -> Element<'_, Message> {
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

        let is_compact_stage = self.window_size.0 < 1250.0 || self.window_size.1 < 750.0;
        let art_size = if is_compact_stage {
            (self.window_size.1 - 440.0).clamp(130.0, 190.0)
        } else {
            (self.window_size.1 - 420.0).clamp(190.0, 270.0)
        };
        let left_w = if is_compact_stage { 260.0 } else { 310.0 };

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
        .width(left_w)
        .spacing(if is_compact_stage { 4 } else { 6 });

        let left = container(left_inner)
            .height(Length::Fill)
            .align_y(iced::alignment::Vertical::Center);

        let (lyrics_active_sz, lyrics_base_sz) = if is_compact_stage {
            (24, 16)
        } else {
            (28, 18)
        };
        let lyrics_panel = self.view_lyrics_inner(lyrics_active_sz, lyrics_base_sz);

        let stage_foreground = container(
            row![
                left,
                container(lyrics_panel)
                    .width(Length::Fill)
                    .height(Length::Fill)
                    .padding(if is_compact_stage { [4, 8] } else { [8, 16] }),
            ]
            .spacing(if is_compact_stage { 18 } else { 32 })
            .align_y(Alignment::Center),
        )
        .padding(if is_compact_stage { [10, 16] } else { [16, 28] })
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


    pub(crate) fn view_player_dock(&self) -> Element<'_, Message> {
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

        let track_info_w = if self.window_size.0 >= 1200.0 {
            260.0
        } else if self.window_size.0 >= 960.0 {
            210.0
        } else {
            160.0
        };

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
                .width(Length::Fixed(track_info_w)),
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
                .width(Length::Fixed(track_info_w)),
            ]
            .spacing(10)
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
                    .padding(7)
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
            .padding([5, 9])
            .style(top_pill_button_style(p, false))
            .on_press(Message::CycleSleepTimer)
            .into()
        } else {
            self.with_tooltip(
                button(icons::icon(icons::CLOCK, 18.0, muted))
                    .padding(7)
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
            container(horizontal_space())
                .width(1)
                .height(16)
                .style(divider_style(0.12)),
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
                .width(if boosted { 68 } else { 80 }),
        ]
        .spacing(5)
        .align_y(Alignment::Center);
        if boosted {
            right = right.push(
                self.with_tooltip(
                    button(text(format!("{vol_pct}%")).size(10).font(mono()).color(accent))
                        .padding([3, 6])
                        .style(soft_pill_button_style(p))
                        .on_press(Message::VolumeChanged(1.0)),
                    "Volume boosted. Click to reset to 100%",
                    tooltip::Position::Top,
                ),
            );
        }

        right = right
            .push(
                container(horizontal_space())
                    .width(1)
                    .height(16)
                    .style(divider_style(0.12)),
            )
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

        let left_slot_w = (track_info_w + 106.0).max(270.0);

        let dock_content = row![
            container(now_playing)
                .width(Length::Fixed(left_slot_w))
                .align_y(iced::alignment::Vertical::Center),
            container(center)
                .width(Length::Fill)
                .align_x(iced::alignment::Horizontal::Center)
                .align_y(iced::alignment::Vertical::Center),
            container(right)
                .width(Length::Shrink)
                .align_x(iced::alignment::Horizontal::Right)
                .align_y(iced::alignment::Vertical::Center),
        ]
        .spacing(16)
        .height(Length::Fixed(56.0))
        .align_y(Alignment::Center);

        container(dock_content)
            .padding(iced::Padding {
                top: 10.0,
                right: 18.0,
                bottom: 10.0,
                left: 18.0,
            })
            .width(Length::Fill)
            .height(Length::Fixed(76.0))
            .style(player_dock_style(p))
            .into()
    }




    pub(crate) fn view_mini_queue(&self) -> Element<'_, Message> {
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

    pub(crate) fn view_mini_player(&self) -> Element<'_, Message> {
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

    pub(crate) fn view_stream_inspector(&self) -> Element<'_, Message> {
        let p = self.palette;
        let muted = color_from_rgb(p.text_muted);
        let accent = color_from_rgb(p.primary);

        let cur = self.queue.current().map(|c| &c.track);

        let format_info = if let Some(t) = cur {
            if t.is_local {
                ("Local Media / PCM", "FLAC / ALAC / WAV / Lossless Container", "Bit-perfect direct decode")
            } else {
                ("YouTube Music Stream", "Opus in WebM / AAC in MP4 (InnerTube)", "Chunked range buffer • 48.0 kHz")
            }
        } else {
            ("No Active Stream", "Engine Standby", "Select track to begin")
        };

        let loudness_info = if let Some(t) = cur {
            match t.loudness_db {
                Some(db) => format!("{db:+.2} dB LUFS (EBU R128 Normalized to -3.0 dB)"),
                None if t.is_local => "Local Audio Tag / Fallback ReplayGain".to_string(),
                None => "Auto Dynamic Perceptual Leveling".to_string(),
            }
        } else {
            "Audio Engine Normalization Active".to_string()
        };

        let output_device_name = self.settings.output_device.clone()
            .unwrap_or_else(|| "Default System Audio Device (WASAPI)".to_string());

        let eq_preset_name = format!("{:?}", self.settings.eq_preset);
        let spatial_name = format!("{:?}", self.settings.spatial_mode);

        let header = row![
            icons::icon(icons::SPEAKER, 18.0, accent),
            column![
                text("Stream Inspector & Diagnostics").size(15).font(bold()),
                text("Real-time bit-depth, codec resolution & audio DSP pipeline").size(11).color(muted),
            ]
            .spacing(2)
            .width(Length::Fill),
            button(icons::icon(icons::CLOSE, 15.0, muted))
                .padding(6)
                .style(icon_button_style(p, false))
                .on_press(Message::ToggleStreamInspector),
        ]
        .spacing(10)
        .align_y(Alignment::Center);

        let info_block = |title: &'static str, val: String, sub: &'static str| -> Element<'_, Message> {
            container(
                column![
                    text(title).size(10).font(bold()).color(with_alpha(accent, 0.90)),
                    text(val).size(13).font(semibold()).color(Color::WHITE),
                    text(sub).size(10).color(muted),
                ]
                .spacing(2),
            )
            .width(Length::Fill)
            .padding(10)
            .style(bento_card_style(p))
            .into()
        };

        let grid = column![
            row![
                info_block("SOURCE & CONTAINER", format_info.0.to_string(), format_info.1),
                info_block("ENCODING & BITRATE", format_info.2.to_string(), "Decoded via Symphonia 0.5"),
            ]
            .spacing(8),
            row![
                info_block("OUTPUT DEVICE", output_device_name, "CPAL Low-Latency Audio Stream"),
                info_block("LOUDNESS CALIBRATION", loudness_info, "Echo Soft-Clipped Headroom Limiter"),
            ]
            .spacing(8),
            row![
                info_block("EQUALIZER PRESET", eq_preset_name, "10-Band Biquad Peaking Filter"),
                info_block("SPATIAL AUDIO", spatial_name, if self.settings.skip_silence { "Silence Gating: On" } else { "Silence Gating: Off" }),
            ]
            .spacing(8),
        ]
        .spacing(8);

        let action_row = row![
            button(text("Open Audio & DSP Settings").size(12).font(semibold()))
                .padding([7, 16])
                .style(top_pill_button_style(p, true))
                .on_press(Message::SelectSettingsSection(crate::SettingsSection::AudioDsp)),
            horizontal_space(),
            button(text("Close").size(12))
                .padding([7, 16])
                .style(top_pill_button_style(p, false))
                .on_press(Message::ToggleStreamInspector),
        ]
        .align_y(Alignment::Center);

        let card = container(
            column![header, vertical_space().height(4), grid, vertical_space().height(6), action_row]
                .spacing(10),
        )
        .width(520)
        .padding(18)
        .style(popover_panel_style(p));

        mouse_area(
            container(card)
                .width(Length::Fill)
                .height(Length::Fill)
                .align_x(iced::alignment::Horizontal::Center)
                .align_y(iced::alignment::Vertical::Center),
        )
        .on_press(Message::ToggleStreamInspector)
        .into()
    }
}
