use std::collections::HashSet;

use iced::alignment::Alignment;
use iced::widget::{
    button, column, container, horizontal_space, responsive, row, text,
    vertical_space,
};
use iced::{Color, Element, Length};

use echo_core::Track;
use echo_youtube::BrowseItem;

use crate::icons;
use crate::theme::{
    artwork_frame_style, color_from_rgb, divider_style, ghost_row_button_style,
    glowing_play_button_style, hero_banner_style, icon_button_style, outlined_pill_button_style,
    pager_button_style, skeleton_block_style, soft_pill_button_style,
    tile_button_style, top_pill_button_style, with_alpha,
};
use crate::{
    bold, ellipsize, greeting_kicker, greeting_label, semibold, DetailPage, EchoApp,
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

        let kicker = greeting_kicker().to_string();
        let subline = if self.listener_ctx.known {
            "Personalized picks shaped by your taste graph & active session".to_string()
        } else {
            "Fresh tracks, artists & mixes ready to play".to_string()
        };
        let mut greeting_row = row![
            column![
                text(kicker).size(11).font(bold()).color(accent),
                text(greeting_label()).size(30).font(bold()),
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
                                    &t,
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

}
