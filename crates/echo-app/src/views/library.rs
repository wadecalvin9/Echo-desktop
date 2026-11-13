use iced::alignment::Alignment;
use iced::widget::text::Wrapping;
use iced::widget::{
    button, column, container, horizontal_space, pick_list, row, vertical_space,
};
use iced::{Color, Element, Length};

use echo_core::{RecognitionSource, Track};
use echo_youtube::SearchFilter;

use crate::icons;
use crate::theme::{
    bento_card_style, color_from_rgb, elevated_card_style, glowing_play_button_style,
    hero_banner_style, local_badge_style, pick_list_menu_style, pick_list_style,
    tile_button_style, top_pill_button_style,
};
use crate::{
    bold, semibold, text, EchoApp, LibraryTab, Message, SongsSortColumn,
};

impl EchoApp {    pub(crate) fn view_library(&self) -> Element<'_, Message> {
        match self.library_tab {
            LibraryTab::Liked => {
                let cover = self.liked_tracks.first().and_then(|t| t.thumbnail_url.as_deref());
                self.view_hero_and_table(
                    "Liked Songs",
                    format!("Playlist • {} songs", self.liked_tracks.len()),
                    cover,
                    &self.liked_tracks,
                    None,
                )
            }
            LibraryTab::History => {
                let cover = self.history_tracks.first().and_then(|t| t.thumbnail_url.as_deref());
                self.view_hero_and_table(
                    "Recently Played",
                    format!("Listening Log • {} tracks", self.history_tracks.len()),
                    cover,
                    &self.history_tracks,
                    None,
                )
            }
            LibraryTab::Downloads => {
                let tracks: Vec<Track> =
                    self.downloaded_tracks.iter().map(|(t, _, _)| t.clone()).collect();
                let total_mb = self
                    .downloaded_tracks
                    .iter()
                    .map(|(_, _, b)| *b)
                    .sum::<u64>()
                    as f64
                    / (1024.0 * 1024.0);
                let mut col = column![
                    container(
                        row![
                            column![
                                text("Downloads").size(28).font(bold()),
                                text(format!(
                                    "{} offline tracks • {:.1} MB",
                                    tracks.len(),
                                    total_mb
                                ))
                                .size(13)
                                .color(color_from_rgb(self.palette.text_muted)),
                            ]
                            .spacing(4),
                            horizontal_space(),
                            button(text("Rescan Folders").size(13))
                                .padding([8, 16])
                                .style(top_pill_button_style(self.palette, false))
                                .on_press(Message::ScanDownloadFolders),
                            button(
                                text("Clear All")
                                    .size(13)
                                    .color(color_from_rgb(self.palette.danger))
                            )
                            .padding([8, 16])
                            .style(top_pill_button_style(self.palette, false))
                            .on_press(Message::ClearDownloads),
                        ]
                        .spacing(10)
                        .align_y(Alignment::Center),
                    )
                    .padding([22, 24])
                    .width(Length::Fill)
                    .style(hero_banner_style(self.palette)),
                ]
                .spacing(12);

                if !tracks.is_empty() {
                    col = col.push(
                        container(
                            button(
                                row![
                                    icons::icon(icons::PLAY, 16.0, Color::BLACK),
                                    text("Play All Offline").size(13).font(bold()),
                                ]
                                .spacing(8)
                                .align_y(Alignment::Center),
                            )
                            .padding([10, 20])
                            .style(glowing_play_button_style(self.palette))
                            .on_press(Message::PlayTracks(tracks.clone(), 0)),
                        )
                        .padding([6, 24]),
                    );
                    col = col.push(self.table_header());
                    let list = self.downloaded_tracks.iter().enumerate().fold(
                        column![].spacing(0),
                        |c, (i, (t, _, _))| {
                            c.push(self.table_track_row(i + 1, t, Some((tracks.clone(), i))))
                        },
                    );
                    col = col.push(container(list).padding([0, 16]));
                } else {
                    col = col.push(
                        container(
                            text("No downloaded songs yet. Click Download on any song or scan external [youtube_id] folders.")
                                .size(14)
                                .color(color_from_rgb(self.palette.text_muted)),
                        )
                        .padding([12, 24]),
                    );
                }
                self.vscroll(col.push(vertical_space().height(24)))
            }
            LibraryTab::Local => {
                let muted = color_from_rgb(self.palette.text_muted);
                let mut col = column![
                    container(
                        row![
                            column![
                                text("Local Files").size(28).font(bold()),
                                text(format!(
                                    "{} tracks from {} folder(s)",
                                    self.local_tracks.len(),
                                    self.settings.local_media_folders.len()
                                ))
                                .size(13)
                                .color(muted),
                            ]
                            .spacing(4),
                            horizontal_space(),
                            button(text(if self.local_folder_tree {
                                "Flat View"
                            } else {
                                "Folder Tree"
                            }).size(13))
                            .padding([8, 16])
                            .style(top_pill_button_style(self.palette, self.local_folder_tree))
                            .on_press(Message::ToggleLocalFolderTree),
                            button(text(if self.local_scanning {
                                "Scanning..."
                            } else {
                                "Rescan Folders"
                            }).size(13))
                            .padding([8, 16])
                            .style(top_pill_button_style(self.palette, true))
                            .on_press(Message::ScanLocalMedia),
                        ]
                        .spacing(10)
                        .align_y(Alignment::Center),
                    )
                    .padding([22, 24])
                    .width(Length::Fill)
                    .style(hero_banner_style(self.palette)),
                ]
                .spacing(12);

                if self.local_tracks.is_empty() {
                    col = col.push(
                        container(
                            text("No local tracks indexed yet. Configure folders in Settings and click Rescan Folders.")
                                .size(14)
                                .color(muted),
                        )
                        .padding([12, 24]),
                    );
                } else if self.local_folder_tree {
                    let mut grouped: std::collections::BTreeMap<String, Vec<usize>> =
                        std::collections::BTreeMap::new();
                    for (idx, t) in self.local_tracks.iter().enumerate() {
                        let folder = t
                            .local_path
                            .as_deref()
                            .and_then(|p| std::path::Path::new(p).parent())
                            .map(|p| p.to_string_lossy().into_owned())
                            .unwrap_or_else(|| "Unknown Folder".to_string());
                        grouped.entry(folder).or_default().push(idx);
                    }
                    for (folder, indices) in grouped {
                        let folder_tracks: Vec<Track> =
                            indices.iter().map(|&idx| self.local_tracks[idx].clone()).collect();
                        let mut sec = column![
                            row![
                                text(folder).size(15).font(bold()),
                                horizontal_space(),
                                button(text("Play Folder").size(12))
                                    .padding([5, 14])
                                    .style(top_pill_button_style(self.palette, false))
                                    .on_press(Message::PlayTracks(folder_tracks.clone(), 0)),
                            ]
                            .align_y(Alignment::Center),
                        ]
                        .spacing(6);
                        for (i, &track_idx) in indices.iter().enumerate() {
                            sec = sec.push(self.table_track_row(
                                i + 1,
                                &self.local_tracks[track_idx],
                                Some((folder_tracks.clone(), i)),
                            ));
                        }
                        col = col.push(container(sec).padding([6, 20]));
                    }
                } else {
                    col = col.push(
                        container(
                            button(
                                row![
                                    icons::icon(icons::PLAY, 16.0, Color::BLACK),
                                    text("Play All Local").size(13).font(bold()),
                                ]
                                .spacing(8)
                                .align_y(Alignment::Center),
                            )
                            .padding([10, 20])
                            .style(glowing_play_button_style(self.palette))
                            .on_press(Message::PlayTracks(self.local_tracks.clone(), 0)),
                        )
                        .padding([6, 24]),
                    );
                    col = col.push(self.table_header());
                    let list = self.local_tracks.iter().enumerate().fold(
                        column![].spacing(0),
                        |c, (i, t)| {
                            c.push(self.table_track_row(
                                i + 1,
                                t,
                                Some((self.local_tracks.clone(), i)),
                            ))
                        },
                    );
                    col = col.push(container(list).padding([0, 16]));
                }
                self.vscroll(col.push(vertical_space().height(24)))
            }
            LibraryTab::Songs => {
                let sort_row = row![
                    text("Sort by:").size(12).color(color_from_rgb(self.palette.text_muted)),
                    pick_list(
                        &SongsSortColumn::ALL[..],
                        Some(self.songs_sort_column),
                        Message::SongsSortColumnChanged,
                    )
                    .padding([5, 10])
                    .style(pick_list_style(self.palette))
                    .menu_style(pick_list_menu_style(self.palette)),
                    button(text(if self.songs_sort_ascending { "Asc ↑" } else { "Desc ↓" }).size(12))
                        .padding([5, 10])
                        .style(top_pill_button_style(self.palette, false))
                        .on_press(Message::SongsSortAscendingToggled),
                    horizontal_space(),
                    button(
                        row![
                            icons::icon(icons::SHARE, 14.0, Color::WHITE),
                            text("Import Playlist").size(12).font(bold()),
                        ]
                        .spacing(6)
                        .align_y(Alignment::Center)
                    )
                    .padding([6, 14])
                    .style(top_pill_button_style(self.palette, true))
                    .on_press(Message::PickPlaylistImportFile),
                ]
                .spacing(8)
                .align_y(Alignment::Center);

                let mut col = column![
                    container(
                        row![
                            column![
                                text("Songs").size(28).font(bold()),
                                text(format!("{} master tracks in library", self.all_songs.len()))
                                    .size(13)
                                    .color(color_from_rgb(self.palette.text_muted)),
                            ]
                            .spacing(4),
                            horizontal_space(),
                            if !self.all_songs.is_empty() {
                                button(
                                    row![
                                        icons::icon(icons::PLAY, 16.0, Color::BLACK),
                                        text("Play All").size(13).font(bold()).color(Color::BLACK),
                                    ]
                                    .spacing(6)
                                    .align_y(Alignment::Center)
                                )
                                .padding([8, 18])
                                .style(glowing_play_button_style(self.palette))
                                .on_press(Message::PlayTracks(self.all_songs.clone(), 0))
                            } else {
                                button(text("No Tracks").size(13))
                            },
                        ]
                        .spacing(12)
                        .align_y(Alignment::Center),
                    )
                    .padding([22, 24])
                    .width(Length::Fill)
                    .style(hero_banner_style(self.palette)),
                    container(sort_row).padding([0, 20]),
                ]
                .spacing(14);

                if self.all_songs.is_empty() {
                    col = col.push(
                        container(
                            column![
                                text("No songs in your library yet.").size(15).font(semibold()),
                                text("Import your favorite playlists from Spotify, Apple Music XML, or M3U files, or scan local folders.")
                                    .size(12)
                                    .color(color_from_rgb(self.palette.text_muted)),
                                vertical_space().height(10),
                                button(text("Import Playlist File").size(13))
                                    .padding([8, 16])
                                    .style(top_pill_button_style(self.palette, true))
                                    .on_press(Message::PickPlaylistImportFile),
                            ]
                            .spacing(6)
                            .align_x(Alignment::Center),
                        )
                        .padding(40)
                        .width(Length::Fill)
                        .align_x(iced::alignment::Horizontal::Center),
                    );
                } else {
                    col = col.push(self.table_header());
                    let all_cloned = self.all_songs.clone();
                    let list = self.all_songs.iter().enumerate().fold(
                        column![].spacing(0),
                        |c, (i, t)| {
                            c.push(self.table_track_row(
                                i + 1,
                                t,
                                Some((all_cloned.clone(), i)),
                            ))
                        },
                    );
                    col = col.push(container(list).padding([0, 16]));
                }
                self.vscroll(col.push(vertical_space().height(24)))
            }
            LibraryTab::Artists => {
                let mut artist_map: std::collections::BTreeMap<String, (usize, Option<String>)> = std::collections::BTreeMap::new();
                for t in &self.all_songs {
                    let art_name = t.primary_artist().trim();
                    if !art_name.is_empty() {
                        let entry = artist_map.entry(art_name.to_string()).or_insert((0, None));
                        entry.0 += 1;
                        if entry.1.is_none() && t.thumbnail_url.is_some() {
                            entry.1 = t.thumbnail_url.clone();
                        }
                    }
                }
                let mut artist_list: Vec<(String, usize, Option<String>)> = artist_map
                    .into_iter()
                    .map(|(k, (count, thumb))| (k, count, thumb))
                    .collect();
                artist_list.sort_by(|a, b| b.1.cmp(&a.1));

                let mut col = column![
                    container(
                        row![
                            column![
                                text("Artists").size(28).font(bold()),
                                text(format!("{} artists in library", artist_list.len()))
                                    .size(13)
                                    .color(color_from_rgb(self.palette.text_muted)),
                            ]
                            .spacing(4),
                        ]
                        .align_y(Alignment::Center),
                    )
                    .padding([22, 24])
                    .width(Length::Fill)
                    .style(hero_banner_style(self.palette)),
                ]
                .spacing(14);

                if artist_list.is_empty() {
                    col = col.push(
                        container(text("No artists found in your library.").size(14).color(color_from_rgb(self.palette.text_muted)))
                            .padding(40)
                            .align_x(iced::alignment::Horizontal::Center)
                    );
                } else {
                    let cw = self.center_width();
                    let cols: usize = if cw >= 1200.0 {
                        6
                    } else if cw >= 980.0 {
                        5
                    } else if cw >= 740.0 {
                        4
                    } else if cw >= 520.0 {
                        3
                    } else {
                        2
                    };
                    let mut grid_col = column![].spacing(12);
                    for chunk in artist_list.chunks(cols) {
                        let mut r = row![].spacing(12);
                        for (name, count, thumb) in chunk.iter().cloned() {
                            let search_query = name.clone();
                            let tile = button(
                                column![
                                    self.artwork_widget(thumb.as_deref(), 110.0, true),
                                    container(
                                        text(name)
                                            .size(13)
                                            .font(bold())
                                            .wrapping(Wrapping::None),
                                    )
                                    .width(Length::Fill)
                                    .clip(true),
                                    text(format!("{} tracks", count))
                                        .size(11)
                                        .color(color_from_rgb(self.palette.text_muted)),
                                ]
                                .spacing(6)
                                .align_x(Alignment::Center),
                            )
                            .width(Length::Fill)
                            .padding(12)
                            .style(tile_button_style(self.palette))
                            .on_press(Message::SearchExact(search_query));
                            r = r.push(tile);
                        }
                        for _ in chunk.len()..cols {
                            r = r.push(horizontal_space().width(Length::Fill));
                        }
                        grid_col = grid_col.push(r);
                    }
                    col = col.push(container(grid_col).padding([0, 20]));
                }
                self.vscroll(col.push(vertical_space().height(24)))
            }
            LibraryTab::Albums => {
                let mut album_map: std::collections::BTreeMap<String, (String, usize, Option<String>, Vec<Track>)> = std::collections::BTreeMap::new();
                for t in &self.all_songs {
                    if let Some(ref alb) = t.album {
                        let alb_title = alb.title.trim();
                        if !alb_title.is_empty() {
                            let entry = album_map.entry(alb_title.to_string()).or_insert_with(|| {
                                (t.primary_artist().to_string(), 0, t.thumbnail_url.clone(), Vec::new())
                            });
                            entry.1 += 1;
                            if entry.2.is_none() && t.thumbnail_url.is_some() {
                                entry.2 = t.thumbnail_url.clone();
                            }
                            entry.3.push(t.clone());
                        }
                    }
                }
                let mut album_list: Vec<(String, String, usize, Option<String>, Vec<Track>)> = album_map
                    .into_iter()
                    .map(|(title, (artist, count, thumb, tracks))| (title, artist, count, thumb, tracks))
                    .collect();
                album_list.sort_by(|a, b| b.2.cmp(&a.2));

                let mut col = column![
                    container(
                        row![
                            column![
                                text("Albums").size(28).font(bold()),
                                text(format!("{} albums in library", album_list.len()))
                                    .size(13)
                                    .color(color_from_rgb(self.palette.text_muted)),
                            ]
                            .spacing(4),
                        ]
                        .align_y(Alignment::Center),
                    )
                    .padding([22, 24])
                    .width(Length::Fill)
                    .style(hero_banner_style(self.palette)),
                ]
                .spacing(14);

                if album_list.is_empty() {
                    col = col.push(
                        container(text("No albums found in your library.").size(14).color(color_from_rgb(self.palette.text_muted)))
                            .padding(40)
                            .align_x(iced::alignment::Horizontal::Center)
                    );
                } else {
                    let cw = self.center_width();
                    let cols: usize = if cw >= 1200.0 {
                        6
                    } else if cw >= 980.0 {
                        5
                    } else if cw >= 740.0 {
                        4
                    } else if cw >= 520.0 {
                        3
                    } else {
                        2
                    };
                    let mut grid_col = column![].spacing(12);
                    for chunk in album_list.chunks(cols) {
                        let mut r = row![].spacing(12);
                        for (title, artist, count, thumb, tracks) in chunk.iter().cloned() {
                            let tile = button(
                                column![
                                    self.artwork_widget(thumb.as_deref(), 120.0, false),
                                    container(
                                        text(title)
                                            .size(13)
                                            .font(bold())
                                            .wrapping(Wrapping::None),
                                    )
                                    .width(Length::Fill)
                                    .clip(true),
                                    container(
                                        text(artist)
                                            .size(11)
                                            .color(color_from_rgb(self.palette.text_muted))
                                            .wrapping(Wrapping::None),
                                    )
                                    .width(Length::Fill)
                                    .clip(true),
                                    text(format!("{} tracks", count))
                                        .size(10)
                                        .color(color_from_rgb(self.palette.text_muted)),
                                ]
                                .spacing(4)
                                .align_x(Alignment::Center),
                            )
                            .width(Length::Fill)
                            .padding(10)
                            .style(tile_button_style(self.palette))
                            .on_press(Message::PlayTracks(tracks, 0));
                            r = r.push(tile);
                        }
                        for _ in chunk.len()..cols {
                            r = r.push(horizontal_space().width(Length::Fill));
                        }
                        grid_col = grid_col.push(r);
                    }
                    col = col.push(container(grid_col).padding([0, 20]));
                }
                self.vscroll(col.push(vertical_space().height(24)))
            }
            LibraryTab::Playlist(idx) => {
                let name = self
                    .playlists
                    .get(idx)
                    .map(|p| p.name.as_str())
                    .unwrap_or("Playlist");
                let cover = self
                    .selected_playlist_tracks
                    .first()
                    .and_then(|t| t.thumbnail_url.as_deref());
                self.view_hero_and_table(
                    name,
                    format!("Playlist • {} songs", self.selected_playlist_tracks.len()),
                    cover,
                    &self.selected_playlist_tracks,
                    None,
                )
            }
        }
    }

    pub(crate) fn view_search(&self) -> Element<'_, Message> {
        let p = self.palette;
        let muted = color_from_rgb(p.text_muted);

        let filters = SearchFilter::ALL.iter().fold(row![].spacing(8), |r, &f| {
            let active = self.search_filter == f;
            r.push(
                button(text(f.label()).size(13).font(bold()))
                    .padding([6, 14])
                    .style(top_pill_button_style(p, active))
                    .on_press(Message::SearchFilterSelected(f)),
            )
        });

        let subtitle = if self.search_query.trim().is_empty() {
            "Search songs, albums, artists, and playlists".to_string()
        } else {
            format!("Results for \"{}\"", self.search_query.trim())
        };

        let header = container(
            column![
                row![
                    column![
                        text("Search").size(30).font(bold()),
                        text(subtitle).size(13).color(muted),
                    ]
                    .spacing(4),
                    horizontal_space(),
                    filters,
                ]
                .spacing(12)
                .align_y(Alignment::Center),
            ]
            .spacing(12),
        )
        .padding([22, 24])
        .width(Length::Fill)
        .style(hero_banner_style(p));

        let mut col = column![header].spacing(12);

        if !self.search_suggestions.is_empty() {
            let sug_row = self.search_suggestions.iter().take(6).fold(row![].spacing(8), |r, s| {
                r.push(
                    button(text(s).size(12))
                        .padding([5, 12])
                        .style(top_pill_button_style(p, false))
                        .on_press(Message::ApplySuggestion(s.clone())),
                )
            });
            col = col.push(container(sug_row).padding([0, 24]));
        }

        if self.search_loading {
            col = col.push(
                container(
                    column![
                        text("Searching YouTube Music...").size(18).font(bold()),
                        text("Looking up songs, albums, artists, and playlists.")
                            .size(13)
                            .color(muted),
                    ]
                    .spacing(6),
                )
                .padding([24, 24]),
            );
        } else if let Some(err) = &self.search_error {
            col = col.push(
                container(text(err).size(14).color(color_from_rgb(p.danger))).padding([12, 24]),
            );
        } else if self.search_results.is_empty() {
            col = col.push(
                container(
                    container(
                        column![
                            icons::icon(icons::SEARCH, 32.0, muted),
                            text("Find what you want to hear").size(18).font(bold()),
                            text("Type a song, album, artist, or playlist in the top search bar (Ctrl+K).")
                                .size(13)
                                .color(muted),
                        ]
                        .spacing(8)
                        .align_x(Alignment::Center),
                    )
                    .width(Length::Fill)
                    .padding(36)
                    .align_x(iced::alignment::Horizontal::Center)
                    .style(elevated_card_style(p)),
                )
                .padding([12, 24]),
            );
        } else {
            let mut list = self.search_results.iter().enumerate().fold(
                column![].spacing(4),
                |c, (i, item)| c.push(self.browse_item_row(i + 1, item)),
            );
            if self.search_continuation.is_some() {
                let lbl = if self.search_loading_more {
                    "Loading more results..."
                } else {
                    "Load more results"
                };
                list = list.push(
                    container(
                        button(text(lbl).size(13))
                            .padding([8, 18])
                            .style(top_pill_button_style(p, false))
                            .on_press(Message::LoadMoreSearch),
                    )
                    .padding([12, 0]),
                );
            }
            col = col.push(container(list).padding([0, 16]));
        }

        self.vscroll(col.push(vertical_space().height(24)))
    }

    pub(crate) fn view_recognize(&self) -> Element<'_, Message> {
        let p = self.palette;
        let muted = color_from_rgb(p.text_muted);
        let accent = color_from_rgb(p.primary);

        let source_picker = pick_list(
            &RecognitionSource::ALL[..],
            Some(self.settings.recognition_source),
            Message::RecognitionSourceSelected,
        )
        .padding([8, 14])
        .style(pick_list_style(p))
        .menu_style(pick_list_menu_style(p));

        let radar_circle = container(icons::icon(
            icons::MIC,
            34.0,
            if self.recognizing { Color::BLACK } else { Color::BLACK },
        ))
        .width(84)
        .height(84)
        .align_x(iced::alignment::Horizontal::Center)
        .align_y(iced::alignment::Vertical::Center);

        let listen_orb = if self.recognizing {
            button(radar_circle)
                .padding(0)
                .style(glowing_play_button_style(p))
        } else {
            button(radar_circle)
                .padding(0)
                .style(glowing_play_button_style(p))
                .on_press(Message::ListenAndRecognize)
        };

        let header = container(
            row![
                column![
                    text("Song Recognition").size(30).font(bold()),
                    text("Identify music playing through your speakers or microphone via Shazam")
                        .size(13)
                        .color(muted),
                ]
                .spacing(4),
                horizontal_space(),
                row![text("Input").size(12).color(muted), source_picker]
                    .spacing(10)
                    .align_y(Alignment::Center),
            ]
            .spacing(12)
            .align_y(Alignment::Center),
        )
        .padding([22, 24])
        .width(Length::Fill)
        .style(hero_banner_style(p));

        let mut radar_col = column![
            listen_orb,
            text(if self.recognizing {
                "Listening for audio (8s)..."
            } else {
                "Tap to Identify"
            })
            .size(18)
            .font(bold()),
            text(if let Some(status) = &self.recognition_status {
                status.as_str()
            } else {
                "Captures 16 kHz mono audio fingerprint and matches against Shazam"
            })
            .size(13)
            .color(if self.recognition_status.is_some() {
                accent
            } else {
                muted
            }),
        ]
        .spacing(10)
        .align_x(Alignment::Center);

        if let Some(rec) = &self.last_recognised {
            radar_col = radar_col.push(vertical_space().height(6));
            radar_col = radar_col.push(
                container(
                    row![
                        self.artwork_widget(rec.artwork_url.as_deref(), 68.0, false),
                        column![
                            text("MATCH FOUND")
                                .size(11)
                                .font(bold())
                                .color(accent),
                            text(&rec.title).size(18).font(bold()),
                            text(rec.artist.as_deref().unwrap_or("Unknown Artist"))
                                .size(14)
                                .color(muted),
                            if let Some(isrc) = &rec.isrc {
                                Element::from(text(format!("ISRC: {isrc}")).size(11).color(muted))
                            } else {
                                Element::from(horizontal_space().width(0))
                            }
                        ]
                        .spacing(3),
                        horizontal_space(),
                        button(
                            row![
                                icons::icon(icons::PLAY, 14.0, Color::BLACK),
                                text("Play on YouTube Music").size(13).font(bold()),
                            ]
                            .spacing(6)
                            .align_y(Alignment::Center),
                        )
                        .padding([10, 18])
                        .style(glowing_play_button_style(p))
                        .on_press(Message::PlayRecognised(rec.clone())),
                    ]
                    .spacing(14)
                    .align_y(Alignment::Center),
                )
                .width(Length::Fill)
                .padding(14)
                .style(elevated_card_style(p)),
            );
        }

        let radar_card = container(
            container(radar_col)
                .width(Length::Fill)
                .padding([26, 24])
                .align_x(iced::alignment::Horizontal::Center)
                .style(bento_card_style(p)),
        )
        .padding([4, 24]);

        let mut col = column![header, radar_card].spacing(16);

        col = col.push(
            container(text("Recognition History").size(19).font(bold())).padding([4, 24]),
        );
        if self.recognition_history.is_empty() {
            col = col.push(
                container(
                    text("No recognised songs yet. Tap the microphone above to identify a track.")
                        .size(13)
                        .color(muted),
                )
                .padding([0, 24]),
            );
        } else {
            let hist = self.recognition_history.iter().fold(column![].spacing(6), |c, entry| {
                let dt = chrono::DateTime::from_timestamp_millis(entry.recognized_at_ms)
                    .map(|d| d.format("%Y-%m-%d %H:%M").to_string())
                    .unwrap_or_default();
                c.push(
                    container(
                        row![
                            container(icons::icon(icons::MIC, 16.0, accent))
                                .width(36)
                                .height(36)
                                .align_x(iced::alignment::Horizontal::Center)
                                .align_y(iced::alignment::Vertical::Center)
                                .style(local_badge_style()),
                            column![
                                text(&entry.title).size(14).font(bold()),
                                text(format!(
                                    "{} • {}",
                                    entry.artist.as_deref().unwrap_or("Unknown"),
                                    dt
                                ))
                                .size(12)
                                .color(muted),
                            ]
                            .spacing(2),
                            horizontal_space(),
                            button(text("Open").size(12).font(bold()))
                                .padding([6, 14])
                                .style(top_pill_button_style(p, false))
                                .on_press(Message::OpenHistoryRecognition(entry.clone())),
                        ]
                        .spacing(12)
                        .align_y(Alignment::Center),
                    )
                    .padding([10, 14])
                    .style(elevated_card_style(p)),
                )
            });
            col = col.push(container(hist).padding([0, 24]));
        }

        self.vscroll(col.push(vertical_space().height(24)))
    }

}
