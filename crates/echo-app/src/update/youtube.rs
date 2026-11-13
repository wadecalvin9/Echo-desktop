use iced::Task;
use std::collections::HashSet;

use echo_audio::{AudioSourceSpec, PlaybackStatus};
use echo_core::{
    prune_queue_tail, score_candidate, store_cached_stream, AppSettings,
    DynamicPalette,
};
use echo_youtube::{
    BrowseItem, SearchFilter,
};

use crate::types::*;
use crate::EchoApp;

impl EchoApp {
    pub(crate) fn update_youtube(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::SearchExact(query) => {
                self.search_query = query;
                self.update(Message::SubmitSearch)
            }
            Message::OpenTrackArtist(track) => {
                self.track_menu = None;
                if let Some(artist) = track.artists.iter().find(|a| a.id.is_some()) {
                    if let Some(id) = artist.id.clone() {
                        return self.update(Message::OpenArtist {
                            browse_id: id,
                            name: artist.name.clone(),
                        });
                    }
                }
                let q = track.primary_artist().trim().to_string();
                if !q.is_empty() {
                    self.push_nav_history();
                    self.search_query = q;
                    self.search_filter = SearchFilter::Artists;
                    return self.update(Message::SubmitSearch);
                }
                Task::none()
            }
            Message::OpenTrackAlbum(track) => {
                self.track_menu = None;
                if let Some(album) = track.album.as_ref() {
                    if let Some(id) = album.id.clone() {
                        return self.update(Message::OpenAlbum {
                            browse_id: id,
                            title: album.title.clone(),
                        });
                    }
                    if !album.title.trim().is_empty() {
                        self.push_nav_history();
                        self.search_query = album.title.clone();
                        self.search_filter = SearchFilter::Albums;
                        return self.update(Message::SubmitSearch);
                    }
                }
                Task::none()
            }
            Message::LoadHome => {
                self.home_loading = true;
                self.home_error = None;
                self.home_seed_cursor = self.home_seed_cursor.wrapping_add(1);
                self.load_home_task()
            }
            Message::SelectHomeChip(params) => {
                self.home_active_chip = if self.home_active_chip == params {
                    None
                } else {
                    params
                };
                self.home_loading = true;
                self.home_error = None;
                self.load_home_task()
            }
            Message::LoadMoreHome => {
                let Some(cont) = self.home_continuation.clone() else {
                    return Task::none();
                };
                if self.home_loading_more {
                    return Task::none();
                }
                self.home_loading_more = true;
                let yt = self.yt.clone();
                Task::perform(
                    async move { yt.home_page(None, Some(&cont)).await.map_err(|e| e.to_string()) },
                    |result| Message::HomePageLoaded {
                        append: true,
                        result,
                    },
                )
            }
            Message::HomePageLoaded { append, result } => {
                self.home_loading = false;
                self.home_loading_more = false;
                let mut tasks = Vec::new();
                match result {
                    Ok(page) => {
                        for sec in &page.sections {
                            for item in sec.items.iter().take(12) {
                                tasks.push(self.fetch_cover_if_needed(item.thumbnail_url()));
                            }
                        }
                        if append {
                            self.home_global_sections.extend(page.sections);
                        } else {
                            if !page.chips.is_empty() || self.home_active_chip.is_none() {
                                self.home_chips = page.chips;
                            }
                            self.shelf_offsets.clear();
                            self.home_global_sections = page.sections;
                        }
                        self.rebuild_home_sections();
                        self.home_continuation = page.continuation;
                    }
                    Err(e) if !append => self.home_error = Some(e),
                    Err(e) => self.set_toast(format!("Could not load more: {e}")),
                }
                Task::batch(tasks)
            }
            Message::TasteShelvesLoaded(sections) => {
                let mut tasks = Vec::new();
                for sec in &sections {
                    for item in sec.items.iter().take(12) {
                        tasks.push(self.fetch_cover_if_needed(item.thumbnail_url()));
                    }
                }
                self.home_taste_sections = sections;
                self.rebuild_home_sections();
                Task::batch(tasks)
            }
            Message::LoadExplore => {
                self.explore_loading = true;
                self.explore_error = None;
                let yt = self.yt.clone();
                Task::perform(
                    async move {
                        let explore = yt.explore().await.map_err(|e| e.to_string());
                        let moods = yt.mood_and_genres().await.unwrap_or_default();
                        (explore, moods)
                    },
                    |(explore, moods)| Message::ExploreLoaded { explore, moods },
                )
            }
            Message::ExploreLoaded { explore, moods } => {
                self.explore_loading = false;
                let mut tasks = Vec::new();
                match explore {
                    Ok(page) => {
                        for item in page.new_release_albums.iter().take(20) {
                            tasks.push(self.fetch_cover_if_needed(item.thumbnail_url()));
                        }
                        self.explore_page = Some(page);
                        self.mood_groups = moods;
                    }
                    Err(e) => self.explore_error = Some(e),
                }
                Task::batch(tasks)
            }
            Message::OpenCategory {
                title,
                browse_id,
                params,
            } => {
                self.push_nav_history();
                self.detail_loading = Some(title.clone());
                self.show_expanded = false;
                let yt = self.yt.clone();
                let bid = browse_id.clone();
                let p = (!params.is_empty()).then_some(params);
                Task::perform(
                    async move {
                        yt.browse_category(&bid, p.as_deref())
                            .await
                            .map(|(t, s)| (if t.is_empty() { title } else { t }, s))
                            .map_err(|e| e.to_string())
                    },
                    |result| Message::CategoryLoaded { result },
                )
            }
            Message::CategoryLoaded { result } => {
                self.detail_loading = None;
                match result {
                    Ok((title, sections)) => {
                        let mut tasks = Vec::new();
                        for sec in &sections {
                            for item in sec.items.iter().take(15) {
                                tasks.push(self.fetch_cover_if_needed(item.thumbnail_url()));
                            }
                        }
                        self.viewing_detail = Some(DetailPage::Category { title, sections });
                        return Task::batch(tasks);
                    }
                    Err(e) => self.set_toast(format!("Could not open category: {e}")),
                }
                Task::none()
            }
            Message::OpenPlaylist { id, title } => {
                self.push_nav_history();
                self.show_expanded = false;
                self.detail_loading = Some(title);
                let yt = self.yt.clone();
                Task::perform(
                    async move { yt.playlist_page(&id).await.map_err(|e| e.to_string()) },
                    Message::PlaylistPageLoaded,
                )
            }
            Message::PlaylistPageLoaded(res) => {
                self.detail_loading = None;
                match res {
                    Ok(page) => {
                        let mut tasks: Vec<_> = page
                            .tracks
                            .iter()
                            .take(24)
                            .map(|t| self.fetch_cover_if_needed(t.thumbnail_url.as_deref()))
                            .collect();
                        tasks.push(self.fetch_cover_if_needed(page.thumbnail_url.as_deref()));
                        self.viewing_detail = Some(DetailPage::Playlist(page));
                        return Task::batch(tasks);
                    }
                    Err(e) => self.set_toast(format!("Could not open playlist: {e}")),
                }
                Task::none()
            }
            Message::LoadMorePlaylist => {
                let Some(DetailPage::Playlist(ref pl)) = self.viewing_detail else {
                    return Task::none();
                };
                let Some(cont) = pl.continuation.clone() else {
                    return Task::none();
                };
                if self.playlist_loading_more {
                    return Task::none();
                }
                self.playlist_loading_more = true;
                let id = pl.id.clone();
                let yt = self.yt.clone();
                Task::perform(
                    async move {
                        yt.playlist_continuation(&id, &cont)
                            .await
                            .map_err(|e| e.to_string())
                    },
                    Message::PlaylistContinuationLoaded,
                )
            }
            Message::PlaylistContinuationLoaded(res) => {
                self.playlist_loading_more = false;
                match res {
                    Ok(next_page) => {
                        let mut tasks = Vec::new();
                        let mut new_urls = Vec::new();
                        if let Some(DetailPage::Playlist(ref mut pl)) = self.viewing_detail {
                            let existing: HashSet<String> =
                                pl.tracks.iter().map(|t| t.id.clone()).collect();
                            for t in next_page.tracks {
                                if !existing.contains(&t.id) {
                                    if let Some(u) = t.thumbnail_url.clone() {
                                        new_urls.push(u);
                                    }
                                    pl.tracks.push(t);
                                }
                            }
                            pl.continuation = next_page.continuation;
                        }
                        for u in new_urls {
                            tasks.push(self.fetch_cover_if_needed(Some(&u)));
                        }
                        return Task::batch(tasks);
                    }
                    Err(e) => self.set_toast(format!("Could not load more tracks: {e}")),
                }
                Task::none()
            }
            Message::OpenAlbum { browse_id, title } => {
                self.push_nav_history();
                self.show_expanded = false;
                self.detail_loading = Some(title);
                let yt = self.yt.clone();
                Task::perform(
                    async move { yt.album(&browse_id).await.map_err(|e| e.to_string()) },
                    Message::AlbumLoaded,
                )
            }
            Message::AlbumLoaded(res) => {
                self.detail_loading = None;
                match res {
                    Ok(page) => {
                        let mut tasks: Vec<_> = page
                            .tracks
                            .iter()
                            .take(24)
                            .map(|t| self.fetch_cover_if_needed(t.thumbnail_url.as_deref()))
                            .collect();
                        tasks.push(self.fetch_cover_if_needed(page.thumbnail_url.as_deref()));
                        for ov in page.other_versions.iter().take(10) {
                            tasks.push(self.fetch_cover_if_needed(ov.thumbnail_url()));
                        }
                        self.viewing_detail = Some(DetailPage::Album(page));
                        return Task::batch(tasks);
                    }
                    Err(e) => self.set_toast(format!("Could not open album: {e}")),
                }
                Task::none()
            }
            Message::OpenArtist { browse_id, name } => {
                self.push_nav_history();
                self.show_expanded = false;
                self.detail_loading = Some(name);
                let yt = self.yt.clone();
                Task::perform(
                    async move { yt.artist(&browse_id).await.map_err(|e| e.to_string()) },
                    Message::ArtistLoaded,
                )
            }
            Message::ArtistLoaded(res) => {
                self.detail_loading = None;
                match res {
                    Ok(page) => {
                        let mut tasks = vec![self.fetch_cover_if_needed(page.thumbnail_url.as_deref())];
                        for sec in &page.sections {
                            for it in sec.items.iter().take(12) {
                                tasks.push(self.fetch_cover_if_needed(it.thumbnail_url()));
                            }
                        }
                        self.viewing_detail = Some(DetailPage::Artist {
                            page,
                            subscribed: false,
                        });
                        return Task::batch(tasks);
                    }
                    Err(e) => self.set_toast(format!("Could not open artist: {e}")),
                }
                Task::none()
            }
            Message::OpenShelfItems { title, items } => {
                self.push_nav_history();
                let tasks: Vec<_> = items
                    .iter()
                    .take(30)
                    .map(|it| self.fetch_cover_if_needed(it.thumbnail_url()))
                    .collect();
                self.viewing_detail = Some(DetailPage::ArtistItems { title, items });
                self.show_expanded = false;
                Task::batch(tasks)
            }
            Message::OpenArtistItems { browse_id, params } => {
                self.push_nav_history();
                self.detail_loading = Some("Items".to_string());
                let yt = self.yt.clone();
                Task::perform(
                    async move {
                        yt.artist_items(&browse_id, params.as_deref())
                            .await
                            .map_err(|e| e.to_string())
                    },
                    Message::ArtistItemsLoaded,
                )
            }
            Message::ArtistItemsLoaded(res) => {
                self.detail_loading = None;
                match res {
                    Ok((title, items, _continuation)) => {
                        let tasks: Vec<_> = items
                            .iter()
                            .take(24)
                            .map(|it| self.fetch_cover_if_needed(it.thumbnail_url()))
                            .collect();
                        self.viewing_detail = Some(DetailPage::ArtistItems { title, items });
                        return Task::batch(tasks);
                    }
                    Err(e) => self.set_toast(format!("Could not load artist items: {e}")),
                }
                Task::none()
            }
            Message::SubscribeArtist {
                channel_id,
                subscribe,
            } => {
                let yt = self.yt.clone();
                Task::perform(
                    async move {
                        yt.subscribe_channel(&channel_id, subscribe)
                            .await
                            .map_err(|e| e.to_string())
                    },
                    move |result| Message::SubscribeResult {
                        subscribed: subscribe,
                        result,
                    },
                )
            }
            Message::SubscribeResult { subscribed, result } => {
                match result {
                    Ok(()) => {
                        if let Some(DetailPage::Artist { subscribed: s, .. }) =
                            &mut self.viewing_detail
                        {
                            *s = subscribed;
                        }
                        self.set_toast(if subscribed {
                            "Subscribed on YouTube Music"
                        } else {
                            "Unsubscribed"
                        });
                    }
                    Err(e) => self.set_toast(e),
                }
                Task::none()
            }
            Message::SearchQueryChanged(q) => {
                self.search_query = q.clone();
                if q.trim().len() < 2 {
                    self.search_suggestions.clear();
                    self.search_dropdown_open = false;
                    return Task::none();
                }
                self.search_dropdown_open = true;
                let yt = self.yt.clone();
                Task::perform(
                    async move { yt.search_suggestions(&q).await.unwrap_or_default() },
                    Message::SuggestionsLoaded,
                )
            }
            Message::SuggestionsLoaded(s) => {
                self.search_suggestions = s;
                if !self.search_suggestions.is_empty() && !self.search_query.trim().is_empty() {
                    self.search_dropdown_open = true;
                }
                Task::none()
            }
            Message::CloseSearchDropdown => {
                self.search_dropdown_open = false;
                Task::none()
            }
            Message::ApplySuggestion(q) => {
                self.search_query = q;
                self.search_suggestions.clear();
                self.search_dropdown_open = false;
                self.tab = NavTab::Search;
                self.show_expanded = false;
                self.clear_detail();
                self.update(Message::SubmitSearch)
            }
            Message::MoveHomeSectionUp(idx) => {
                if idx > 0 && idx < self.home_sections.len() {
                    self.home_sections.swap(idx, idx - 1);
                }
                Task::none()
            }
            Message::MoveHomeSectionDown(idx) => {
                if idx + 1 < self.home_sections.len() {
                    self.home_sections.swap(idx, idx + 1);
                }
                Task::none()
            }
            Message::SearchFilterSelected(f) => {
                self.search_filter = f;
                if !self.search_query.trim().is_empty() {
                    self.update(Message::SubmitSearch)
                } else {
                    Task::none()
                }
            }
            Message::SubmitSearch => {
                let q = self.search_query.trim().to_string();
                if q.is_empty() {
                    return Task::none();
                }
                self.tab = NavTab::Search;
                self.show_expanded = false;
                self.search_dropdown_open = false;
                self.clear_detail();
                self.search_loading = true;
                self.search_error = None;
                self.search_continuation = None;
                self.search_suggestions.clear();
                let yt = self.yt.clone();
                let filter = self.search_filter;
                Task::perform(
                    async move { yt.search_page(&q, filter).await.map_err(|e| e.to_string()) },
                    |result| Message::SearchPageLoaded {
                        append: false,
                        result,
                    },
                )
            }
            Message::LoadMoreSearch => {
                let Some(cont) = self.search_continuation.clone() else {
                    return Task::none();
                };
                if self.search_loading_more {
                    return Task::none();
                }
                self.search_loading_more = true;
                let yt = self.yt.clone();
                let filter = self.search_filter;
                Task::perform(
                    async move {
                        yt.search_continuation(&cont, filter)
                            .await
                            .map_err(|e| e.to_string())
                    },
                    |result| Message::SearchPageLoaded {
                        append: true,
                        result,
                    },
                )
            }
            Message::SearchPageLoaded { append, result } => {
                self.search_loading = false;
                self.search_loading_more = false;
                let mut tasks = Vec::new();
                match result {
                    Ok(page) => {
                        for item in page.items.iter().take(25) {
                            tasks.push(self.fetch_cover_if_needed(item.thumbnail_url()));
                        }
                        if append {
                            let existing: HashSet<String> =
                                self.search_results.iter().map(BrowseItem::stable_key).collect();
                            for item in page.items {
                                if !existing.contains(&item.stable_key()) {
                                    self.search_results.push(item);
                                }
                            }
                        } else {
                            self.search_results = page.items;
                        }
                        self.search_continuation = page.continuation;
                    }
                    Err(e) if !append => self.search_error = Some(e),
                    Err(e) => self.set_toast(format!("Load more failed: {e}")),
                }
                Task::batch(tasks)
            }
            Message::RadioLoaded(Ok(mut tracks)) => {
                let mut tasks = Vec::new();
                let cur_id = self.queue.current().map(|c| c.track.id.clone());
                if let Some(ref seed_id) = cur_id {
                    if !tracks.is_empty() {
                        let _ = self.db.save_related_edges(seed_id, &tracks, 1);
                    }
                }
                tracks.sort_by(|a, b| {
                    score_candidate(b, &self.listener_ctx)
                        .partial_cmp(&score_candidate(a, &self.listener_ctx))
                        .unwrap_or(std::cmp::Ordering::Equal)
                });
                for t in tracks {
                    if Some(&t.id) != cur_id.as_ref()
                        && !self.listener_ctx.is_disliked_artist(t.primary_artist())
                        && !self.queue.items.iter().any(|q| q.track.id == t.id)
                    {
                        tasks.push(self.fetch_cover_if_needed(t.thumbnail_url.as_deref()));
                        self.queue.push_back(t);
                    }
                }
                prune_queue_tail(
                    &mut self.queue.items,
                    self.queue.current_index,
                    &self.listener_ctx,
                    false,
                );
                Task::batch(tasks)
            }
            Message::RadioLoaded(Err(_)) => Task::none(),
            Message::StreamResolved { mut track, result } => {
                if self.queue.current().map(|c| &c.track.id) != Some(&track.id) {
                    return Task::none();
                }
                match result {
                    Ok(stream) => {
                        if let Some(ldb) = stream.loudness_db {
                            track.loudness_db = Some(ldb);
                            if let Some(cur) = self
                                .queue
                                .current_index
                                .and_then(|i| self.queue.items.get_mut(i))
                            {
                                cur.track.loudness_db = Some(ldb);
                            }
                        }
                        if track.thumbnail_url.is_none() {
                            track.thumbnail_url = stream.thumbnail_url.clone();
                        }
                        let _ = self.db.upsert_track(&track);
                        let _ = self.db.record_play(&track, 0);
                        self.refresh_db_views();
                        if let Some(tracking_url) = stream.playback_tracking_url.clone() {
                            if self.yt.has_cookie() {
                                let yt = self.yt.clone();
                                tokio::spawn(async move {
                                    let _ = yt.register_playback(&tracking_url, None).await;
                                });
                            }
                        }
                        self.audio.play(AudioSourceSpec::Url {
                            url: stream.url.clone(),
                            user_agent: stream.user_agent.clone(),
                            mime_hint: Some(stream.mime_type.clone()),
                            content_length: stream.content_length,
                            approx_duration_ms: stream.approx_duration_ms,
                            loudness_db: track.loudness_db,
                        });
                        if self.settings.max_cache_size_mb > 0 {
                            let max_mb = self.settings.max_cache_size_mb;
                            if stream.content_length.unwrap_or(0) <= (32 * 1024 * 1024) {
                                if let Ok(cache_dir) = AppSettings::stream_cache_dir() {
                                    let http = self.http.clone();
                                    let url = stream.url;
                                    let ua = stream.user_agent;
                                    let sid = track.id.clone();
                                    tokio::spawn(async move {
                                        if let Ok(resp) = http
                                            .get(&url)
                                            .header(reqwest::header::USER_AGENT, ua)
                                            .send()
                                            .await
                                        {
                                            if resp.status().is_success() {
                                                if let Ok(bytes) = resp.bytes().await {
                                                    let _ = tokio::task::spawn_blocking(move || {
                                                        let _ = store_cached_stream(
                                                            &cache_dir,
                                                            &sid,
                                                            &bytes,
                                                            max_mb,
                                                        );
                                                    })
                                                    .await;
                                                }
                                            }
                                        }
                                    });
                                }
                            }
                        }
                    }
                    Err(e) => {
                        self.yt.invalidate_stream(&track.id);
                        self.status = PlaybackStatus::Error(e.clone());
                        self.set_toast(format!("Playback error: {e}"));
                    }
                }
                Task::none()
            }
            Message::ArtworkLoaded {
                url,
                rounded,
                circle,
                blurred,
                tint,
                bytes,
            } => {
                self.loading_covers.remove(&url);
                let upgraded = echo_core::upgrade_thumbnail_url(&url);
                if let Some(t) = tint {
                    self.cover_tints.insert(url.clone(), t);
                    if upgraded != url {
                        self.cover_tints.insert(upgraded.clone(), t);
                    }
                }
                if let Some(r) = rounded {
                    self.rounded_covers.insert(url.clone(), r.clone());
                    if upgraded != url {
                        self.rounded_covers.insert(upgraded.clone(), r);
                    }
                } else if let Some(ref raw) = bytes {
                    let h = iced::widget::image::Handle::from_bytes(raw.clone());
                    self.rounded_covers.insert(url.clone(), h.clone());
                    if upgraded != url {
                        self.rounded_covers.insert(upgraded.clone(), h);
                    }
                }
                if let Some(c) = circle {
                    self.circle_covers.insert(url.clone(), c.clone());
                    if upgraded != url {
                        self.circle_covers.insert(upgraded.clone(), c);
                    }
                }
                if let Some(b) = blurred {
                    self.blurred_covers.insert(url.clone(), b.clone());
                    if upgraded != url {
                        self.blurred_covers.insert(upgraded.clone(), b);
                    }
                }
                if let Some(raw) = bytes {
                    let is_current_cover = self
                        .queue
                        .current()
                        .and_then(|c| c.track.thumbnail_url.as_deref())
                        .is_some_and(|u| u == url.as_str() || u == upgraded.as_str());
                    if is_current_cover {
                        if let Some(p) =
                            DynamicPalette::from_image_bytes(&raw, self.settings.theme_mode)
                        {
                            self.palette = p;
                        }
                    }
                    self.cover_bytes.insert(url.clone(), raw.clone());
                    if upgraded != url {
                        self.cover_bytes.insert(upgraded, raw);
                    }
                }
                Task::none()
            }
            Message::LyricsLoaded { track_id, lyrics } => {
                if self.queue.current().map(|c| c.track.id.as_str()) == Some(track_id.as_str()) {
                    self.lyrics_loading = false;
                    self.active_lyrics = lyrics;
                    self.last_lyric_idx = None;
                    return self.sync_lyrics_scroll();
                }
                Task::none()
            }
            Message::StartGoogleSignIn => {
                self.account_sync_status = Some("Opening Google sign-in window...".to_string());
                Task::perform(
                    async move {
                        tokio::task::spawn_blocking(move || {
                            crate::auth::launch_google_signin_webview()
                        })
                        .await
                        .map_err(|e| format!("Task execution error: {e}"))
                        .and_then(|r| r)
                    },
                    Message::GoogleSignInFinished,
                )
            }
            Message::GoogleSignInFinished(result) => {
                match result {
                    Ok(cookie_header) => {
                        self.yt_cookie_input = cookie_header.clone();
                        self.settings.youtube_cookie = Some(cookie_header.clone());
                        self.yt.set_cookie(Some(cookie_header));
                        let _ = self.settings.save();
                        self.account_sync_status =
                            Some("Connected! Syncing your library & playlists...".to_string());
                        self.update(Message::SyncYouTubeAccount)
                    }
                    Err(err) => {
                        self.account_sync_status = Some(format!("{err}"));
                        Task::none()
                    }
                }
            }
            Message::YouTubeCookieChanged(c) => {
                self.yt_cookie_input = c;
                Task::none()
            }
            Message::SaveYouTubeCookie => {
                let trimmed = self.yt_cookie_input.trim().to_string();
                let cookie_opt = (!trimmed.is_empty()).then_some(trimmed);
                self.settings.youtube_cookie = cookie_opt.clone();
                self.yt.set_cookie(cookie_opt);
                let _ = self.settings.save();
                self.account_sync_status = Some(if self.yt.has_cookie() {
                    "Cookie saved. Click Sync Account to import playlists & liked songs."
                        .to_string()
                } else {
                    "YouTube cookie cleared.".to_string()
                });
                Task::none()
            }
            Message::ClearYouTubeCookie => {
                self.yt_cookie_input.clear();
                self.settings.youtube_cookie = None;
                self.yt.set_cookie(None);
                self.account_playlists.clear();
                self.account_sync_status = Some("Signed out of YouTube Music".to_string());
                let _ = self.settings.save();
                Task::none()
            }
            Message::SyncYouTubeAccount => {
                if !self.yt.has_cookie() {
                    self.account_sync_status =
                        Some("Paste and save a YouTube Music cookie first.".to_string());
                    return Task::none();
                }
                if self.account_syncing {
                    return Task::none();
                }
                self.account_syncing = true;
                self.account_sync_status =
                    Some("Syncing account playlists & liked songs...".to_string());
                let yt = self.yt.clone();
                Task::perform(
                    async move {
                        let playlists = yt.account_playlists().await.map_err(|e| e.to_string());
                        let liked = yt.liked_songs_remote().await.map_err(|e| e.to_string());
                        (playlists, liked)
                    },
                    |(playlists, liked)| Message::AccountSynced { playlists, liked },
                )
            }
            Message::YoutubeRegionSelected(region) => {
                self.settings.youtube_gl = region.code().to_string();
                self.yt.set_locale(
                    self.settings.youtube_gl.clone(),
                    self.settings.youtube_hl.clone(),
                );
                let _ = self.settings.save();
                self.set_toast(format!("YouTube Music region: {}", region.code()));
                self.update(Message::LoadHome)
            }
            Message::YoutubeLanguageSelected(lang) => {
                self.settings.youtube_hl = lang.code().to_string();
                self.yt.set_locale(
                    self.settings.youtube_gl.clone(),
                    self.settings.youtube_hl.clone(),
                );
                let _ = self.settings.save();
                self.set_toast(format!("YouTube Music language: {}", lang.code()));
                self.update(Message::LoadHome)
            }
            Message::AccountSynced { playlists, liked } => {
                self.account_syncing = false;
                let mut tasks = Vec::new();
                let mut parts = Vec::new();
                match playlists {
                    Ok(pls) => {
                        parts.push(format!("{} playlists", pls.len()));
                        for pl in pls.iter().take(15) {
                            tasks.push(self.fetch_cover_if_needed(pl.thumbnail_url.as_deref()));
                        }
                        self.account_playlists = pls;
                    }
                    Err(e) => parts.push(format!("playlists error: {e}")),
                }
                match liked {
                    Ok(page) => {
                        let count = page.tracks.len();
                        for mut t in page.tracks {
                            t.liked = true;
                            let _ = self.db.upsert_track(&t);
                            let _ = self.db.set_liked(&t.id, true);
                        }
                        self.refresh_db_views();
                        parts.push(format!("{count} liked songs synced"));
                    }
                    Err(e) => parts.push(format!("liked songs error: {e}")),
                }
                self.account_sync_status = Some(parts.join(" • "));
                self.set_toast("YouTube account sync finished");
                Task::batch(tasks)
            }
            Message::AdjustLyricsOffset(delta_ms) => {
                self.settings.lyrics_offset_ms = (self.settings.lyrics_offset_ms + delta_ms).clamp(-10000, 10000);
                let _ = self.settings.save();
                self.set_toast(format!("Lyrics offset: {:+.1}s", self.settings.lyrics_offset_ms as f32 / 1000.0));
                Task::none()
            }
            Message::ResetLyricsOffset => {
                self.settings.lyrics_offset_ms = 0;
                let _ = self.settings.save();
                self.set_toast("Lyrics offset reset to 0.0s");
                Task::none()
            }
            Message::ShareLyricsCard => {
                if let Some(cur) = self.queue.current() {
                    let calibrated_ms = (self.position_ms as i64 + self.settings.lyrics_offset_ms).max(0) as u64;
                    let quote_line = self
                        .active_lyrics
                        .as_ref()
                        .and_then(|r| r.synced.get(r.active_index(calibrated_ms).unwrap_or(0)))
                        .map(|l| l.text.as_str())
                        .unwrap_or(&cur.track.title);
                    let quote_card = format!(
                        "“{}”\n\n— {}, {}\n🎧 Shared from Echo",
                        quote_line,
                        cur.track.primary_artist(),
                        cur.track.title
                    );
                    self.set_toast("Lyrics quote copied to clipboard!");
                    return iced::clipboard::write(quote_card);
                }
                Task::none()
            }
            Message::ToggleBetterLyrics(on) => {
                self.settings.better_lyrics_enabled = on;
                let _ = self.settings.save();
                if let Some(cur) = self.queue.current() {
                    self.lyrics_loading = true;
                    return self.fetch_lyrics_for_track(&cur.track);
                }
                Task::none()
            }
            Message::ToggleAutoLoadMore(on) => {
                self.settings.auto_load_more = on;
                let _ = self.settings.save();
                Task::none()
            }
            Message::ToggleRomanizeLyrics => {
                self.settings.romanize_lyrics = !self.settings.romanize_lyrics;
                let _ = self.settings.save();
                Task::none()
            }
            _ => Task::none(),
        }
    }
}

