use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::{Duration, Instant};

use iced::widget::scrollable;
use iced::Task;

use echo_audio::{AudioSourceSpec, PlaybackStatus};
use echo_core::{
    base_version_key, cache_stats, extract_embedded_lyrics, is_preview_title, lookup_cached_stream,
    AppSettings, DynamicPalette, Track,
};
use echo_lastfm::LastFmClient;
use echo_lyrics::LyricsResult;
use echo_youtube::{BrowseItem, HomeSection, SearchFilter};

use crate::ranking::{item_artist_hint, item_version_key, rank_home_section};
use crate::theme::{blurred_image_handle, rounded_image_handle};
use crate::{
    ellipsize, DetailPage, EchoApp, Message, NavSnapshot, RightTab, ScrobbleTracker,
    SongsSortColumn,
};

impl EchoApp {    pub(crate) fn set_toast(&mut self, msg: impl Into<String>) {
        self.toast = Some((msg.into(), Instant::now()));
    }

    pub(crate) fn sync_lyrics_scroll(&mut self) -> Task<Message> {
        if let Some(res) = &self.active_lyrics {
            let is_lyrics_tab = self.right_tab != RightTab::Queue;
            let lyrics_visible = self.show_expanded
                || (self.mini_player && self.window_size.1 > 280.0 && is_lyrics_tab)
                || (self.show_right_rail && self.right_tab == RightTab::Lyrics);
            if !res.synced.is_empty() && lyrics_visible {
                let idx = res.active_index(self.position_ms);
                if idx != self.last_lyric_idx {
                    self.last_lyric_idx = idx;
                    let count = res.synced.len();
                    let denom = count.saturating_sub(1).max(1) as f32;
                    let y = match idx {
                        Some(i) => (i as f32 / denom).clamp(0.0, 1.0),
                        None => 0.0,
                    };
                    return scrollable::snap_to(
                        self.lyrics_scroll_id.clone(),
                        scrollable::RelativeOffset { x: 0.0, y },
                    );
                }
            }
        }
        Task::none()
    }

    pub(crate) fn refresh_db_views(&mut self) {
        self.liked_tracks = self.db.liked_tracks().unwrap_or_default();
        self.history_tracks = self.db.recent_history(100).unwrap_or_default();
        self.downloaded_tracks = self.db.downloaded_tracks().unwrap_or_default();
        self.downloaded_id_set = self
            .downloaded_tracks
            .iter()
            .map(|(t, _, _)| t.id.clone())
            .collect();
        self.local_tracks = self.db.local_tracks().unwrap_or_default();
        self.cache_stats = AppSettings::stream_cache_dir()
            .map(|d| cache_stats(&d))
            .unwrap_or((0, 0));
        self.playlists = self.db.playlists().unwrap_or_default();
        self.recognition_history = self.db.recognition_history(100).unwrap_or_default();
        self.stats = self.db.top_stats(50).unwrap_or_default();
        self.refresh_stats_summary();
        self.listener_ctx.seed_from_taste(
            &self.liked_tracks,
            &self.listening_summary.top_artists,
            &self.history_tracks,
        );
        self.missing_loudness_count = self
            .db
            .tracks_missing_loudness()
            .map(|v| v.len())
            .unwrap_or(0);

        let mut seen = std::collections::HashSet::new();
        let mut all = Vec::new();
        let mut add = |t: &Track| {
            if seen.insert(t.id.clone()) {
                all.push(t.clone());
            }
        };
        for t in &self.liked_tracks {
            add(t);
        }
        for (t, _, _) in &self.downloaded_tracks {
            add(t);
        }
        for t in &self.local_tracks {
            add(t);
        }
        for t in &self.history_tracks {
            add(t);
        }
        self.all_songs = all;
        self.sort_all_songs();
    }

    pub(crate) fn sort_all_songs(&mut self) {
        match self.songs_sort_column {
            SongsSortColumn::Title => {
                self.all_songs.sort_by(|a, b| a.title.to_lowercase().cmp(&b.title.to_lowercase()));
            }
            SongsSortColumn::Artist => {
                self.all_songs.sort_by(|a, b| a.primary_artist().to_lowercase().cmp(&b.primary_artist().to_lowercase()));
            }
            SongsSortColumn::Duration => {
                self.all_songs.sort_by_key(|t| t.duration_secs.unwrap_or(0));
            }
            SongsSortColumn::DateAdded => {}
            SongsSortColumn::PlayCount => {
                let stats_map: std::collections::HashMap<&str, u64> = self.stats.iter().map(|s| (s.track.id.as_str(), s.play_count)).collect();
                self.all_songs.sort_by_key(|t| std::cmp::Reverse(stats_map.get(t.id.as_str()).copied().unwrap_or(0)));
            }
        }
        if !self.songs_sort_ascending && self.songs_sort_column != SongsSortColumn::PlayCount {
            self.all_songs.reverse();
        }
    }

    pub(crate) fn refresh_stats_summary(&mut self) {
        self.listening_summary = self
            .db
            .listening_summary(self.stats_range.days(), 50)
            .unwrap_or_default();
    }

    pub(crate) fn prefetch_stats_covers(&mut self) -> Task<Message> {
        let mut urls = Vec::new();
        for st in self.listening_summary.top_tracks.iter().take(12) {
            if let Some(u) = &st.track.thumbnail_url {
                urls.push(u.clone());
            }
        }
        for ar in self.listening_summary.top_artists.iter().take(8) {
            if let Some(u) = &ar.thumbnail_url {
                urls.push(u.clone());
            }
        }
        for al in self.listening_summary.top_albums.iter().take(6) {
            if let Some(u) = &al.thumbnail_url {
                urls.push(u.clone());
            }
        }
        Task::batch(
            urls.into_iter()
                .map(|u| self.fetch_cover_if_needed(Some(&u)))
                .collect::<Vec<_>>(),
        )
    }

    pub(crate) fn load_home_task(&self) -> Task<Message> {
        let yt = self.yt.clone();
        let params = self.home_active_chip.clone();
        let base_task = Task::perform(
            async move {
                yt.home_page(params.as_deref(), None)
                    .await
                    .map_err(|e| e.to_string())
            },
            |result| Message::HomePageLoaded {
                append: false,
                result,
            },
        );
        if self.home_active_chip.is_none() {
            Task::batch([base_task, self.load_taste_shelves_task()])
        } else {
            base_task
        }
    }

    pub(crate) fn load_taste_shelves_task(&self) -> Task<Message> {
        let mut disliked_artists: HashSet<String> = self
            .listener_ctx
            .skipped_artists
            .iter()
            .filter(|(art, &cnt)| cnt >= 2 && !self.listener_ctx.liked_artists.contains(*art))
            .map(|(art, _)| art.clone())
            .collect();
        for ex in &self.settings.excluded_artists {
            disliked_artists.insert(ex.trim().to_lowercase());
        }

        let mut seed_artists: Vec<(Option<String>, String)> = Vec::new();
        let mut seen_artists = HashSet::new();
        for ar in &self.listening_summary.top_artists {
            let key = ar.name.trim().to_lowercase();
            if !key.is_empty()
                && !self.listener_ctx.is_disliked_artist(&ar.name)
                && seen_artists.insert(key)
            {
                seed_artists.push((ar.id.clone(), ar.name.clone()));
            }
        }
        for t in self.liked_tracks.iter().chain(self.history_tracks.iter()) {
            if let Some(a) = t.artists.first() {
                let key = a.name.trim().to_lowercase();
                if !key.is_empty()
                    && !self.listener_ctx.is_disliked_artist(&a.name)
                    && seen_artists.insert(key)
                {
                    seed_artists.push((a.id.clone(), a.name.clone()));
                }
            }
        }

        let fallback_heavy: Vec<BrowseItem> = self
            .listening_summary
            .top_tracks
            .iter()
            .filter(|st| !self.listener_ctx.is_disliked_artist(st.track.primary_artist()))
            .take(12)
            .map(|st| BrowseItem::Track(st.track.clone()))
            .collect();

        let cursor = self.home_seed_cursor;
        let artist_seed = (!seed_artists.is_empty())
            .then(|| seed_artists[cursor % seed_artists.len()].clone());
        let yt = self.yt.clone();
        let db = self.db.clone();
        let ctx = self.listener_ctx.clone();

        Task::perform(
            async move {
                let initial = db
                    .build_home_recommendations(&disliked_artists, 16, cursor)
                    .ok();

                let mut fetch_ids: Vec<String> = initial
                    .as_ref()
                    .map(|o| o.uncached_seed_ids.clone())
                    .unwrap_or_default();
                if fetch_ids.is_empty() {
                    if let Some(seeds) = initial.as_ref().map(|o| &o.seeds) {
                        if !seeds.is_empty() {
                            fetch_ids.push(seeds[cursor % seeds.len()].id.clone());
                        }
                    }
                }
                let s0 = fetch_ids.first().cloned();
                let s1 = fetch_ids.get(1).cloned();
                let s2 = fetch_ids.get(2).cloned();

                let rel0_fut = async {
                    match s0.as_deref() {
                        Some(id) => Some((id.to_string(), yt.related_for_video(id).await.ok())),
                        None => None,
                    }
                };
                let radio0_fut = async {
                    match s0.as_deref() {
                        Some(id) => Some((id.to_string(), yt.next_radio(id, None).await.ok())),
                        None => None,
                    }
                };
                let rel1_fut = async {
                    match s1.as_deref() {
                        Some(id) => Some((id.to_string(), yt.related_for_video(id).await.ok())),
                        None => None,
                    }
                };
                let rel2_fut = async {
                    match s2.as_deref() {
                        Some(id) => Some((id.to_string(), yt.related_for_video(id).await.ok())),
                        None => None,
                    }
                };
                let artist_fut = async {
                    match artist_seed {
                        Some((Some(ref bid), ref name)) => {
                            let page = yt.artist(bid).await.ok();
                            (Some(name.clone()), page, Vec::new())
                        }
                        Some((None, ref name)) => {
                            let items = yt.search(name, SearchFilter::Albums).await.unwrap_or_default();
                            (Some(name.clone()), None, items)
                        }
                        None => (None, None, Vec::new()),
                    }
                };

                let (rel0, radio0, rel1, rel2, (artist_name_opt, artist_page_opt, fallback_items)) =
                    tokio::join!(rel0_fut, radio0_fut, rel1_fut, rel2_fut, artist_fut);

                let mut primary_rel = None;
                for (seed_id, page) in [rel0, rel1, rel2].into_iter().flatten() {
                    if let Some(p) = page {
                        if !p.tracks.is_empty() {
                            let _ = db.save_related_edges(&seed_id, &p.tracks, 0);
                        }
                        if primary_rel.is_none() {
                            primary_rel = Some(p);
                        }
                    }
                }
                if let Some((seed_id, Some(radio_tracks))) = radio0 {
                    if !radio_tracks.is_empty() {
                        let _ = db.save_related_edges(&seed_id, &radio_tracks, 1);
                    }
                }

                let engine_out = db
                    .build_home_recommendations(&disliked_artists, 16, cursor)
                    .ok()
                    .or(initial);

                let mut sections: Vec<HomeSection> = Vec::new();
                if let Some(out) = engine_out {
                    if !out.quick_picks.is_empty() {
                        let pick_ids: Vec<String> =
                            out.quick_picks.iter().map(|c| c.track.id.clone()).collect();
                        let _ = db.record_home_impressions(&pick_ids);
                        sections.push(HomeSection {
                            title: "Quick Picks for You".to_string(),
                            items: out
                                .quick_picks
                                .into_iter()
                                .map(|c| BrowseItem::Track(c.track))
                                .collect(),
                        });
                    }
                    if out.discover.len() >= 3 {
                        sections.push(HomeSection {
                            title: "Discover Something New".to_string(),
                            items: out.discover.into_iter().map(BrowseItem::Track).collect(),
                        });
                    }
                    let heavy: Vec<BrowseItem> = if out.heavy_rotation.len() >= 3 {
                        out.heavy_rotation
                            .into_iter()
                            .map(BrowseItem::Track)
                            .collect()
                    } else {
                        fallback_heavy.clone()
                    };
                    if heavy.len() >= 3 {
                        sections.push(HomeSection {
                            title: "Heavy Rotation".to_string(),
                            items: heavy,
                        });
                    }
                    if !out.forgotten_favorites.is_empty() {
                        sections.push(HomeSection {
                            title: "Forgotten Favorites".to_string(),
                            items: out
                                .forgotten_favorites
                                .into_iter()
                                .map(BrowseItem::Track)
                                .collect(),
                        });
                    }
                } else if fallback_heavy.len() >= 3 {
                    sections.push(HomeSection {
                        title: "Heavy Rotation".to_string(),
                        items: fallback_heavy,
                    });
                }

                if let Some(rel) = primary_rel {
                    let playlists: Vec<BrowseItem> = rel
                        .playlists
                        .into_iter()
                        .filter(|it| !ctx.is_disliked_artist(&item_artist_hint(it)))
                        .take(12)
                        .collect();
                    if !playlists.is_empty() {
                        sections.push(HomeSection {
                            title: "Mixes Picked for You".to_string(),
                            items: playlists,
                        });
                    }
                    let artists: Vec<BrowseItem> = rel
                        .artists
                        .into_iter()
                        .filter(|it| !ctx.is_disliked_artist(it.title()))
                        .take(12)
                        .collect();
                    if !artists.is_empty() {
                        sections.push(HomeSection {
                            title: "Artists for You".to_string(),
                            items: artists,
                        });
                    }
                }

                if let Some(name) = artist_name_opt {
                    let mut items = Vec::new();
                    if let Some(page) = artist_page_opt {
                        for sec in page.sections {
                            for it in sec.items {
                                if !ctx.is_disliked_artist(&item_artist_hint(&it)) {
                                    items.push(it);
                                }
                                if items.len() >= 14 {
                                    break;
                                }
                            }
                            if items.len() >= 14 {
                                break;
                            }
                        }
                    } else {
                        for it in fallback_items {
                            if !ctx.is_disliked_artist(&item_artist_hint(&it)) {
                                items.push(it);
                            }
                            if items.len() >= 12 {
                                break;
                            }
                        }
                    }
                    if !items.is_empty() {
                        sections.push(HomeSection {
                            title: format!("More from {}", ellipsize(&name, 26)),
                            items,
                        });
                    }
                }

                sections
            },
            Message::TasteShelvesLoaded,
        )
    }

    pub(crate) fn rebuild_home_sections(&mut self) {
        let mut combined = Vec::new();
        let mut seen_titles = HashSet::new();
        let mut seen_versions = HashSet::new();
        let mut seen_keys = HashSet::new();
        if self.home_active_chip.is_none() {
            for sec in &self.home_taste_sections {
                if let Some(ranked) = rank_home_section(
                    sec,
                    &self.listener_ctx,
                    true,
                    &mut seen_versions,
                    &mut seen_keys,
                ) {
                    seen_titles.insert(ranked.title.to_lowercase());
                    combined.push(ranked);
                }
            }
        }
        for sec in &self.home_global_sections {
            if let Some(ranked) = rank_home_section(
                sec,
                &self.listener_ctx,
                false,
                &mut seen_versions,
                &mut seen_keys,
            ) {
                if seen_titles.insert(ranked.title.to_lowercase()) {
                    combined.push(ranked);
                }
            }
        }
        self.home_sections = combined;

        let mut quick = Vec::new();
        let mut seen_tiles = HashSet::new();
        let mut seen_tile_versions = HashSet::new();
        let mut artist_counts: HashMap<String, usize> = HashMap::new();
        if self.home_active_chip.is_none() {
            for t in self.history_tracks.iter().chain(self.liked_tracks.iter()) {
                let artist = t.primary_artist().trim().to_lowercase();
                if !self.listener_ctx.is_disliked_artist(t.primary_artist())
                    && !is_preview_title(&t.title)
                    && *artist_counts.get(&artist).unwrap_or(&0) < 1
                {
                    let vk = base_version_key(&t.title, t.primary_artist());
                    let it = BrowseItem::Track(t.clone());
                    if seen_tile_versions.insert(vk) && seen_tiles.insert(it.stable_key()) {
                        *artist_counts.entry(artist).or_insert(0) += 1;
                        quick.push(it);
                    }
                    if quick.len() >= 3 {
                        break;
                    }
                }
            }
        }
        for sec in &self.home_sections {
            for it in &sec.items {
                let artist = match it {
                    BrowseItem::Track(t) => t.primary_artist().trim().to_lowercase(),
                    BrowseItem::Artist { name, .. } => name.trim().to_lowercase(),
                    BrowseItem::Album { artist, .. } => artist.trim().to_lowercase(),
                    BrowseItem::Playlist { author, .. } => author.trim().to_lowercase(),
                };
                if !artist.is_empty() && *artist_counts.get(&artist).unwrap_or(&0) >= 1 && quick.len() < 7 {
                    continue;
                }
                let vk = item_version_key(it);
                let ok_ver = vk.is_none_or(|k| seen_tile_versions.insert(k));
                if ok_ver && seen_tiles.insert(it.stable_key()) {
                    if !artist.is_empty() {
                        *artist_counts.entry(artist).or_insert(0) += 1;
                    }
                    quick.push(it.clone());
                }
                if quick.len() >= 8 {
                    break;
                }
            }
            if quick.len() >= 8 {
                break;
            }
        }
        // Fallback pass: if still under 6 tiles, relax the artist cap to fill remaining slots
        if quick.len() < 6 {
            for sec in &self.home_sections {
                for it in &sec.items {
                    let vk = item_version_key(it);
                    let ok_ver = vk.is_none_or(|k| seen_tile_versions.insert(k));
                    if ok_ver && seen_tiles.insert(it.stable_key()) {
                        quick.push(it.clone());
                    }
                    if quick.len() >= 8 {
                        break;
                    }
                }
                if quick.len() >= 8 {
                    break;
                }
            }
        }
        self.home_quick_tiles = quick;
    }

    pub(crate) fn push_nav_history(&mut self) {
        self.nav_forward.clear();
        let snap = NavSnapshot {
            tab: self.tab,
            library_tab: self.library_tab,
            viewing_detail: self.viewing_detail.clone(),
            show_expanded: self.show_expanded,
        };
        let is_dup = self.nav_history.last().is_some_and(|prev| {
            prev.tab == snap.tab
                && prev.library_tab == snap.library_tab
                && prev.show_expanded == snap.show_expanded
                && prev.viewing_detail.is_none()
                && snap.viewing_detail.is_none()
        });
        if !is_dup {
            self.nav_history.push(snap);
            if self.nav_history.len() > 48 {
                self.nav_history.remove(0);
            }
        }
    }

    pub(crate) fn clear_detail(&mut self) {
        self.viewing_detail = None;
        self.detail_loading = None;
        self.playlist_filter_query.clear();
    }

    pub(crate) fn active_playlist_id(&self) -> Option<&str> {
        match &self.viewing_detail {
            Some(DetailPage::Playlist(pl)) => Some(&pl.id),
            _ => None,
        }
    }

    pub(crate) fn fetch_cover_if_needed(&mut self, url: Option<&str>) -> Task<Message> {
        static SEM: std::sync::OnceLock<Arc<tokio::sync::Semaphore>> = std::sync::OnceLock::new();
        let sem = Arc::clone(SEM.get_or_init(|| Arc::new(tokio::sync::Semaphore::new(6))));

        let Some(url) = url.filter(|u| !u.is_empty()) else {
            return Task::none();
        };
        if self.rounded_covers.contains_key(url) || self.loading_covers.contains(url) {
            return Task::none();
        }
        let url_owned = url.to_string();
        self.loading_covers.insert(url_owned.clone());
        let fetch_url = echo_core::upgrade_thumbnail_url(&url_owned);
        let http = self.http.clone();
        let theme_mode = self.settings.theme_mode;
        Task::perform(
            async move {
                let bytes = if fetch_url.starts_with("http://") || fetch_url.starts_with("https://")
                {
                    let _permit = sem.acquire().await.ok();
                    match http.get(&fetch_url).send().await {
                        Ok(r) if r.status().is_success() => r.bytes().await.ok().map(|b| b.to_vec()),
                        _ if fetch_url != url_owned => match http.get(&url_owned).send().await {
                            Ok(r) if r.status().is_success() => r.bytes().await.ok().map(|b| b.to_vec()),
                            _ => None,
                        },
                        _ => None,
                    }
                } else {
                    tokio::fs::read(&url_owned).await.ok()
                };
                let (rounded, circle, blurred, tint) = if let Some(ref raw) = bytes {
                    let raw_clone = raw.clone();
                    tokio::task::spawn_blocking(move || {
                        let r = rounded_image_handle(&raw_clone, 512, 28.0, false);
                        let c = rounded_image_handle(&raw_clone, 320, 160.0, true);
                        let b = blurred_image_handle(&raw_clone);
                        let t = DynamicPalette::from_image_bytes(&raw_clone, theme_mode).map(|p| p.tint);
                        (r, c, b, t)
                    })
                    .await
                    .unwrap_or((None, None, None, None))
                } else {
                    (None, None, None, None)
                };
                (url_owned, rounded, circle, blurred, tint, bytes)
            },
            |(url, rounded, circle, blurred, tint, bytes)| Message::ArtworkLoaded {
                url,
                rounded,
                circle,
                blurred,
                tint,
                bytes,
            },
        )
    }

    pub(crate) fn fetch_lyrics_for_track(&self, track: &Track) -> Task<Message> {
        let lf = Arc::clone(&self.lyrics_fetcher);
        let t_lyrics = track.clone();
        let better_enabled = self.settings.better_lyrics_enabled;
        Task::perform(
            async move {
                if let Some(path_str) = &t_lyrics.local_path {
                    let p = std::path::Path::new(path_str);
                    if let Some(raw) = extract_embedded_lyrics(p) {
                        let synced = echo_lyrics::parse_lrc(&raw);
                        let res = if !synced.is_empty() {
                            Some(LyricsResult {
                                source: echo_lyrics::LyricsSource::LocalLrc,
                                synced,
                                plain: None,
                            })
                        } else {
                            Some(LyricsResult {
                                source: echo_lyrics::LyricsSource::LocalLrc,
                                synced: Vec::new(),
                                plain: Some(raw),
                            })
                        };
                        return (t_lyrics.id, res);
                    }
                }
                let res = lf
                    .fetch_with_options(
                        &t_lyrics.title,
                        t_lyrics.primary_artist(),
                        t_lyrics.duration_secs,
                        t_lyrics.album.as_ref().map(|a| a.title.as_str()),
                        t_lyrics.local_path.as_deref(),
                        better_enabled,
                    )
                    .await;
                (t_lyrics.id, res)
            },
            |(track_id, lyrics)| Message::LyricsLoaded { track_id, lyrics },
        )
    }

    pub(crate) fn flush_active_play_duration(&self) {
        if let Some(tr) = &self.scrobble_tracker {
            let played_secs = tr.accumulated_ms.max(self.position_ms) / 1000;
            if played_secs > 0 {
                let _ = self
                    .db
                    .update_latest_play_duration(&tr.track.id, played_secs as u32);
            }
        }
    }

    pub(crate) fn start_playing_current(&mut self) -> Task<Message> {
        self.flush_active_play_duration();
        let Some(track) = self.queue.current().map(|q| q.track.clone()) else {
            return Task::none();
        };

        self.status = PlaybackStatus::Loading;
        self.position_ms = 0;
        self.duration_ms = track.duration_secs.unwrap_or(0) as u64 * 1000;
        self.active_lyrics = None;
        self.last_lyric_idx = None;
        self.lyrics_loading = true;
        self.toast = None;

        self.scrobble_tracker = Some(ScrobbleTracker {
            track: track.clone(),
            started_at_secs: chrono::Utc::now().timestamp(),
            accumulated_ms: 0,
            scrobbled: false,
        });
        self.listener_ctx.record_play(&track, track.liked);

        let mut tasks = Vec::new();
        tasks.push(self.fetch_cover_if_needed(track.thumbnail_url.as_deref()));

        if let Some(url) = &track.thumbnail_url {
            if let Some(bytes) = self.cover_bytes.get(url) {
                if let Some(p) = DynamicPalette::from_image_bytes(bytes, self.settings.theme_mode) {
                    self.palette = p;
                }
            }
        }

        tasks.push(self.fetch_lyrics_for_track(&track));

        if self.settings.lastfm_scrobbling_enabled {
            if let Some(sk) = self.settings.lastfm_session_key.clone() {
                let client = LastFmClient::new(
                    self.settings.lastfm_api_key.clone(),
                    self.settings.lastfm_api_secret.clone(),
                );
                if client.is_configured() {
                    let t = track.clone();
                    tokio::spawn(async move {
                        let _ = client
                            .update_now_playing(
                                &sk,
                                t.primary_artist(),
                                &t.title,
                                t.album.as_ref().map(|a| a.title.as_str()),
                                t.duration_secs,
                            )
                            .await;
                    });
                }
            }
        }

        if track.is_local {
            if let Some(path) = &track.local_path {
                let _ = self.db.record_play(&track, 0);
                self.refresh_db_views();
                self.audio.play(AudioSourceSpec::File {
                    path: path.into(),
                    loudness_db: track.loudness_db,
                });
            }
            return Task::batch(tasks);
        }

        if let Some(dl_path) = self.db.get_download_path(&track.id) {
            let _ = self.db.record_play(&track, 0);
            self.refresh_db_views();
            self.audio.play(AudioSourceSpec::File {
                path: dl_path,
                loudness_db: track.loudness_db,
            });
            return Task::batch(tasks);
        }

        if self.settings.max_cache_size_mb > 0 {
            if let Some(cached_path) = AppSettings::stream_cache_dir()
                .ok()
                .and_then(|d| lookup_cached_stream(&d, &track.id))
            {
                let _ = self.db.record_play(&track, 0);
                self.refresh_db_views();
                self.audio.play(AudioSourceSpec::File {
                    path: cached_path,
                    loudness_db: track.loudness_db,
                });
                return Task::batch(tasks);
            }
        }

        let yt = self.yt.clone();
        let quality = self.settings.audio_quality;
        let t_resolve = track.clone();
        tasks.push(Task::perform(
            async move {
                let res = yt
                    .resolve_stream(&t_resolve.id, quality)
                    .await
                    .map_err(|e| e.to_string());
                (t_resolve, res)
            },
            |(track, result)| Message::StreamResolved { track, result },
        ));

        Task::batch(tasks)
    }


    pub(crate) fn schedule_next_volume_repair(&mut self) -> Task<Message> {
        let Some(next) = self.repair_queue.pop() else {
            self.repair_running = false;
            self.repair_status = Some(format!("Finished. Fixed {} songs.", self.repair_fixed));
            self.refresh_db_views();
            return Task::none();
        };
        let yt = self.yt.clone();
        Task::perform(
            async move {
                tokio::time::sleep(Duration::from_millis(330)).await;
                match yt.fetch_loudness_db(&next.id).await {
                    Ok(ldb) => (next.id, ldb, true),
                    Err(_) => (next.id, None, false),
                }
            },
            |(track_id, loudness_db, ok)| Message::FixOneVolumeDone {
                track_id,
                loudness_db,
                ok,
            },
        )
    }

}
