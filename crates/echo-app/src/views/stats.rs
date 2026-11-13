use iced::alignment::Alignment;
use iced::widget::text::Wrapping;
use iced::widget::{
    button, column, container, horizontal_space, mouse_area, row, scrollable,
    tooltip, vertical_space,
};
use iced::{Color, Element, Font, Length};

use echo_core::Track;

use crate::icons;
use crate::theme::{
    artwork_frame_style, bento_card_style, card_button_style, color_from_rgb, divider_style,
    ghost_row_button_style, glowing_play_button_style, hero_banner_style, icon_button_style,
    outlined_pill_button_style, scroll_style, soft_pill_button_style, stat_bar_fill_style,
    stat_bar_track_style, stats_bento_button_style, stats_rank_badge_style,
    top_pill_button_style, with_alpha,
};
use crate::{
    bold, format_duration_hm, format_hour_12, semibold, text, EchoApp, Message, StatsRange,
};

impl EchoApp {    pub(crate) fn view_stats(&self) -> Element<'_, Message> {
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

        let metrics_layout: Element<'_, Message> = if self.center_width() < 780.0 {
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
                ]
                .spacing(10)
                .align_y(Alignment::Center),
                container(horizontal_space())
                    .width(Length::Fill)
                    .height(1)
                    .style(divider_style(0.06)),
                row![
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
            ]
            .spacing(10)
            .into()
        } else {
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
            .align_y(Alignment::Center)
            .into()
        };

        container(
            column![
                metrics_layout,
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

}
