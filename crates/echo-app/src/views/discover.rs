use std::collections::HashSet;

use iced::alignment::Alignment;
use iced::widget::text::Wrapping;
use iced::widget::{
    button, column, container, horizontal_space, responsive, row, text_input,
    vertical_space,
};
use iced::{Color, Element, Length};

use echo_core::Track;
use echo_youtube::BrowseItem;

use crate::icons;
use crate::theme::{
    artwork_frame_style, bare_input_style, bento_card_style, color_from_rgb, divider_style,
    ghost_row_button_style, glowing_play_button_style, hero_banner_style, hero_banner_tinted_style,
    icon_button_style, outlined_pill_button_style, pager_button_style, search_field_style,
    skeleton_block_style, soft_pill_button_style, sticky_header_style, tile_button_style,
    top_pill_button_style, with_alpha,
};
use crate::{
    bold, dynamic_greeting, ellipsize, semibold, text, DetailPage, EchoApp,
    LibraryTab, Message, NavTab,
};

impl EchoApp {    pub(crate) fn view_hero_and_table<'a>(
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
        let page_tint = cover_url
            .and_then(|u| {
                self.cover_tints.get(u).copied().or_else(|| {
                    let up = echo_core::upgrade_thumbnail_url(u);
                    self.cover_tints.get(&up).copied()
                })
            })
            .unwrap_or(p.tint);
        let hero = container(hero_col)
            .padding([26, 24])
            .width(Length::Fill)
            .style(hero_banner_tinted_style(page_tint, p.surface));

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

        if tracks.len() > 1 {
            let mut filter_row = row![
                icons::icon(icons::SEARCH, 14.0, muted),
                text_input("Filter in playlist...", &self.playlist_filter_query)
                    .on_input(Message::PlaylistFilterChanged)
                    .style(bare_input_style(p))
                    .padding([5, 4])
                    .size(13)
                    .width(Length::Fill),
            ]
            .spacing(6)
            .align_y(Alignment::Center);
            if !self.playlist_filter_query.is_empty() {
                filter_row = filter_row.push(
                    button(icons::icon(icons::CLOSE, 13.0, muted))
                        .padding(4)
                        .style(icon_button_style(p, false))
                        .on_press(Message::PlaylistFilterChanged(String::new())),
                );
            }
            let filter_box = container(filter_row)
                .width(200)
                .padding([2, 10])
                .style(search_field_style(p));
            toolbar = toolbar.push(horizontal_space());
            toolbar = toolbar.push(filter_box);
        }

        let filtered_tracks: Vec<&Track> = if self.playlist_filter_query.trim().is_empty() {
            tracks.iter().collect()
        } else {
            let q = self.playlist_filter_query.trim().to_lowercase();
            tracks
                .iter()
                .filter(|t| {
                    t.title.to_lowercase().contains(&q)
                        || t.artist_display().to_lowercase().contains(&q)
                        || t.album.as_ref().map(|a| a.title.to_lowercase().contains(&q)).unwrap_or(false)
                })
                .collect()
        };

        let rows: Element<'_, Message> = if filtered_tracks.is_empty() {
            container(
                text(if tracks.is_empty() {
                    "Nothing here yet. Play something and it will show up."
                } else {
                    "No tracks match your filter."
                })
                .size(14)
                .color(muted),
            )
            .padding([24, 28])
            .into()
        } else {
            let play_all: Vec<Track> = filtered_tracks.iter().map(|&t| t.clone()).collect();
            let list = filtered_tracks
                .iter()
                .enumerate()
                .fold(column![].spacing(0), |col, (idx, &t)| {
                    col.push(self.table_track_row(idx + 1, t, Some((play_all.clone(), idx))))
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

    pub(crate) fn table_header(&self) -> Element<'_, Message> {
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
                row![cols, horizontal_space().width(96 + 12)]
                    .spacing(0)
                    .padding([8, 12])
                    .align_y(Alignment::Center),
            )
            .padding([0, 16])
            .style(sticky_header_style(p)),
            container(container(horizontal_space()).height(1).width(Length::Fill).style(divider_style(0.10)))
                .padding([0, 16]),
        ]
        .spacing(0)
        .into()
    }

    pub(crate) fn skeleton_pulse(&self) -> f32 {
        0.06 + 0.05 * (self.anim_phase.sin() * 0.5 + 0.5)
    }

    pub(crate) fn view_discover_skeleton(&self) -> Element<'_, Message> {
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

    pub(crate) fn view_detail_skeleton<'a>(&'a self, label: &'a str) -> Element<'a, Message> {
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

    pub(crate) fn browse_shelf_section<'a>(
        &'a self,
        shelf_key: String,
        title: &'a str,
        items: &'a [BrowseItem],
        show_explore_link: bool,
        reorder: Option<(usize, usize)>,
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

        let sub_text: Option<&'static str> = {
            let t = title.to_lowercase();
            if t.contains("mix") {
                Some("Personalized mixes shaped by your music taste")
            } else if t.contains("artist") {
                Some("Bringing together the top songs from your favorite artists")
            } else if t.contains("new") || t.contains("friday") || t.contains("release") {
                Some("The newest tracks and fresh drops ready to stream")
            } else if t.contains("chart") || t.contains("trending") {
                Some("The most popular music right now")
            } else {
                None
            }
        };

        let mut header_right = row![].spacing(6).align_y(Alignment::Center);

        if let Some((idx, total)) = reorder {
            if total > 1 {
                let can_up = idx > 0;
                let can_down = idx + 1 < total;
                let up_btn = button(icons::icon(
                    icons::CHEVRON_UP,
                    14.0,
                    if can_up { Color::WHITE } else { with_alpha(muted, 0.25) },
                ))
                .padding([4, 6])
                .style(icon_button_style(p, false));
                let up_btn = if can_up {
                    up_btn.on_press(Message::MoveHomeSectionUp(idx))
                } else {
                    up_btn
                };

                let down_btn = button(icons::icon(
                    icons::CHEVRON_DOWN,
                    14.0,
                    if can_down { Color::WHITE } else { with_alpha(muted, 0.25) },
                ))
                .padding([4, 6])
                .style(icon_button_style(p, false));
                let down_btn = if can_down {
                    down_btn.on_press(Message::MoveHomeSectionDown(idx))
                } else {
                    down_btn
                };

                header_right = header_right
                    .push(self.with_tooltip(up_btn, "Move section up", iced::widget::tooltip::Position::Top))
                    .push(self.with_tooltip(down_btn, "Move section down", iced::widget::tooltip::Position::Top));
            }
        }

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

        header_right = header_right.push(
            button(text(if show_explore_link { "Explore all" } else { "Show all" }).size(12).font(bold()).color(muted))
                .padding([5, 10])
                .style(ghost_row_button_style(p, false))
                .on_press(Message::SelectTab(NavTab::Explore)),
        );

        let title_block = if let Some(sub) = sub_text {
            column![
                text(title).size(22).font(bold()),
                text(sub).size(12).color(muted),
            ]
            .spacing(2)
        } else {
            column![text(title).size(22).font(bold())].spacing(0)
        };

        container(
            column![
                row![
                    title_block,
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

    fn hero_spotlight_card<'a>(
        &'a self,
        cover_url: Option<&'a str>,
        title: String,
        subtitle: String,
        tracks: Vec<Track>,
    ) -> Element<'a, Message> {
        let p = self.palette;
        let accent = color_from_rgb(p.primary);
        let muted = color_from_rgb(p.text_muted);
        let is_playing_any = self.queue.current().is_some_and(|c| {
            tracks.iter().any(|t| t.id == c.track.id)
        });

        let cover_elem = container(self.artwork_widget(cover_url, 116.0, false))
            .style(artwork_frame_style(10.0, 0.45))
            .width(116)
            .height(116);

        let chip = row![
            icons::icon(icons::COMPASS, 11.0, accent),
            text("DAILY SPOTLIGHT")
                .size(10.0)
                .font(bold())
                .color(accent),
        ]
        .spacing(5)
        .align_y(Alignment::Center);

        let track_count = tracks.len();
        let play_btn = if !tracks.is_empty() {
            button(
                row![
                    icons::icon(
                        if is_playing_any { icons::EQ_BARS } else { icons::PLAY },
                        11.5,
                        Color::BLACK,
                    ),
                    text(if is_playing_any { "Listening Now" } else { "Play Mix" })
                        .size(12.5)
                        .font(bold()),
                ]
                .spacing(6)
                .align_y(Alignment::Center),
            )
            .padding([7, 16])
            .style(glowing_play_button_style(p))
            .on_press(Message::PlayTracks(tracks, 0))
        } else {
            button(
                row![
                    icons::icon(icons::REFRESH, 12.0, muted),
                    text("Refresh").size(12.0).font(semibold()),
                ]
                .spacing(6)
                .align_y(Alignment::Center),
            )
            .padding([7, 14])
            .style(soft_pill_button_style(p))
            .on_press(Message::LoadHome)
        };

        let mut action_row = row![play_btn].spacing(10).align_y(Alignment::Center);
        if track_count > 0 {
            action_row = action_row.push(
                text(format!("{} tracks", track_count))
                    .size(11.5)
                    .color(muted),
            );
        }

        let content_col = column![
            chip,
            text(title)
                .size(18.0)
                .font(bold())
                .wrapping(Wrapping::None),
            text(subtitle)
                .size(12.0)
                .color(muted)
                .wrapping(Wrapping::None),
            vertical_space(),
            action_row,
        ]
        .spacing(4)
        .height(Length::Fill);

        let inner_row = row![
            cover_elem,
            content_col,
        ]
        .spacing(16)
        .align_y(Alignment::Center);

        container(inner_row)
            .padding(16)
            .width(Length::Fill)
            .height(Length::Fixed(148.0))
            .style(bento_card_style(p))
            .into()
    }

    pub(crate) fn view_discover(&self) -> Element<'_, Message> {
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

        let (kicker, greeting, subline) = dynamic_greeting(self.listener_ctx.known);
        let greeting_row = row![
            column![
                text(kicker).size(11).font(bold()).color(accent),
                text(greeting).size(30).font(bold()),
                text(subline).size(12).color(muted),
            ]
            .spacing(3),
            horizontal_space(),
            button(
                row![
                    icons::icon(icons::REFRESH, 13.0, muted),
                    text("Refresh").size(12).font(semibold()),
                ]
                .spacing(6)
                .align_y(Alignment::Center),
            )
            .padding([8, 14])
            .style(soft_pill_button_style(p))
            .on_press(Message::LoadHome),
        ]
        .spacing(12)
        .align_y(Alignment::End);
        header_col = header_col.push(greeting_row);

        let first_cover = self
            .home_quick_tiles
            .first()
            .and_then(|i| i.thumbnail_url())
            .or_else(|| {
                self.home_sections
                    .first()
                    .and_then(|s| s.items.first())
                    .and_then(|i| i.thumbnail_url())
            });

        let mut seen_featured = HashSet::new();
        let mut featured_artists = Vec::new();
        for t in &mix_tracks {
            let a = t.primary_artist().trim();
            let key = a.to_lowercase();
            if !key.is_empty() && seen_featured.insert(key) {
                featured_artists.push(a);
                if featured_artists.len() >= 3 {
                    break;
                }
            }
        }

        let hero_title = if featured_artists.len() >= 2 {
            format!("{} & {} Mix", featured_artists[0], featured_artists[1])
        } else if featured_artists.len() == 1 {
            format!("{} Radio", featured_artists[0])
        } else {
            "Today's Heavy Rotation".to_string()
        };

        let hero_subtitle = if !featured_artists.is_empty() {
            format!("Featuring {} & more", featured_artists.join(", "))
        } else if self.listener_ctx.known {
            "Personalized daily selection from your taste profile".to_string()
        } else {
            "A balanced rotation of songs & recommendations".to_string()
        };

        let hero_card = self.hero_spotlight_card(
            first_cover,
            hero_title,
            hero_subtitle,
            mix_tracks,
        );

        let cw = self.center_width();
        if cw >= 840.0 {
            let comp_cols = if cw >= 1200.0 { 3 } else { 2 };
            let needed_tiles = comp_cols * 2 - 1;
            let companion_items: Vec<&BrowseItem> =
                self.home_quick_tiles.iter().take(needed_tiles).collect();

            let mut right_col = column![].spacing(12);

            // Row 1: Liked Songs + (comp_cols - 1) tiles
            let mut r1 = row![self.liked_songs_quick_tile()].spacing(12);
            for it in companion_items.iter().take(comp_cols - 1) {
                r1 = r1.push(self.quick_tile(it));
            }
            for _ in (companion_items.len().min(comp_cols - 1) + 1)..comp_cols {
                r1 = r1.push(horizontal_space().width(Length::Fill));
            }
            right_col = right_col.push(r1);

            // Row 2: remaining tiles
            let mut r2 = row![].spacing(12);
            let row2_items = companion_items.iter().skip(comp_cols - 1).take(comp_cols);
            let mut r2_count = 0;
            for it in row2_items {
                r2 = r2.push(self.quick_tile(it));
                r2_count += 1;
            }
            for _ in r2_count..comp_cols {
                r2 = r2.push(horizontal_space().width(Length::Fill));
            }
            right_col = right_col.push(r2);

            let bento_row = row![
                container(hero_card)
                    .width(Length::FillPortion(if comp_cols == 3 { 2 } else { 3 })),
                container(right_col)
                    .width(Length::FillPortion(if comp_cols == 3 { 3 } else { 4 })),
            ]
            .spacing(12);

            header_col = header_col.push(bento_row);
        } else {
            let mut stacked = column![hero_card].spacing(12);
            let companion_items: Vec<&BrowseItem> =
                self.home_quick_tiles.iter().take(3).collect();

            // Row 1: Liked Songs + 1 tile
            let mut r1 = row![self.liked_songs_quick_tile()].spacing(12);
            if let Some(first_tile) = companion_items.first() {
                r1 = r1.push(self.quick_tile(first_tile));
            } else {
                r1 = r1.push(horizontal_space().width(Length::Fill));
            }
            stacked = stacked.push(r1);

            // Row 2: remaining tiles
            let mut r2 = row![].spacing(12);
            let mut r2_count = 0;
            for it in companion_items.iter().skip(1).take(2) {
                r2 = r2.push(self.quick_tile(it));
                r2_count += 1;
            }
            for _ in r2_count..2 {
                r2 = r2.push(horizontal_space().width(Length::Fill));
            }
            stacked = stacked.push(r2);

            header_col = header_col.push(stacked);
        }

        let header = container(header_col)
            .padding([20, 24])
            .width(Length::Fill)
            .style(hero_banner_style(p));

        let mut content = column![header].spacing(6);
        let total_home_sections = self.home_sections.len();
        for (idx, sec) in self.home_sections.iter().enumerate() {
            content = content.push(self.browse_shelf_section(
                format!("home:{idx}:{}", sec.title),
                &sec.title,
                &sec.items,
                idx == 0,
                Some((idx, total_home_sections)),
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

    pub(crate) fn view_explore(&self) -> Element<'_, Message> {
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
                    None,
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

    pub(crate) fn mood_buttons_grid<'a>(
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

    pub(crate) fn view_detail_page<'a>(&'a self, page: &'a DetailPage) -> Element<'a, Message> {
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

                let page_tint = alb.thumbnail_url.as_deref()
                    .and_then(|u| {
                        self.cover_tints.get(u).copied().or_else(|| {
                            let up = echo_core::upgrade_thumbnail_url(u);
                            self.cover_tints.get(&up).copied()
                        })
                    })
                    .unwrap_or(p.tint);
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
                    .style(hero_banner_tinted_style(page_tint, p.surface)),
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
                        None,
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

                let page_tint = page.thumbnail_url.as_deref()
                    .and_then(|u| {
                        self.cover_tints.get(u).copied().or_else(|| {
                            let up = echo_core::upgrade_thumbnail_url(u);
                            self.cover_tints.get(&up).copied()
                        })
                    })
                    .unwrap_or(p.tint);

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
                                    Element::from(text(stats_line).size(14).color(Color::WHITE))
                                } else {
                                    Element::from(horizontal_space().width(0))
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
                    .style(hero_banner_tinted_style(page_tint, p.surface)),
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
                                    &t,
                                    Some((all_tracks.clone(), idx)),
                                ));
                                idx += 1;
                            }
                        }
                        col = col.push(container(track_col).padding([8, 24]));
                    } else {
                        let cw = self.center_width();
                        let n = (((cw - 48.0 + 16.0) / (172.0 + 16.0)).floor() as usize).clamp(1, 12);
                        let shelf_row = sec
                            .items
                            .iter()
                            .take(n)
                            .fold(row![].spacing(16), |r, it| r.push(self.browse_card(it)));
                        col = col.push(
                            container(
                                column![
                                    header_row,
                                    shelf_row,
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

}
