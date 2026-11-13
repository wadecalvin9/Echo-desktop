use iced::Task;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use echo_core::{
    clear_cache_dir, default_backup_path, download_file_path, export_backup,
    import_playlist_file, migrate_download_file, prune_queue_tail,
    scan_download_folders, scan_local_folders, tag_audio_file,
    validate_and_unpack_backup, AlbumRef, AppSettings, ArtistRef,
    DynamicPalette, EchoDb, LocalMediaScanConfig, Track,
};
use echo_fingerprint::{capture_16khz_mono, RecognitionOutcome};
use echo_lastfm::LastFmClient;
use echo_youtube::{
    BrowseItem, SearchFilter,
};

use crate::dialogs;
use crate::ranking::*;
use crate::types::*;
use crate::EchoApp;

impl EchoApp {
    pub(crate) fn update_library(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::OpenTrackMenu(t) => {
                self.track_menu = Some(t);
                Task::none()
            }
            Message::CloseTrackMenu => {
                self.track_menu = None;
                Task::none()
            }
            Message::AddTrackToPlaylist { playlist_id, track } => {
                self.track_menu = None;
                let _ = self.db.add_to_playlist(&playlist_id, &track);
                self.refresh_db_views();
                if self.selected_playlist_id.as_deref() == Some(playlist_id.as_str()) {
                    self.selected_playlist_tracks =
                        self.db.playlist_tracks(&playlist_id).unwrap_or_default();
                }
                self.set_toast("Added to playlist");
                Task::none()
            }
            Message::ExpandAndNewPlaylist => {
                self.left_collapsed = false;
                self.show_new_playlist_input = true;
                Task::none()
            }
            Message::ToggleNewPlaylistInput => {
                self.show_new_playlist_input = !self.show_new_playlist_input;
                Task::none()
            }
            Message::ToggleLikeCurrent => {
                if let Some(idx) = self.queue.current_index {
                    let mut liked_track: Option<Track> = None;
                    if let Some(item) = self.queue.items.get_mut(idx) {
                        item.track.liked = !item.track.liked;
                        let liked_now = item.track.liked;
                        let track_id = item.track.id.clone();
                        let _ = self.db.upsert_track(&item.track);
                        let _ = self.db.set_liked(&track_id, liked_now);
                        if self.yt.has_cookie() && !item.track.is_local {
                            let yt = self.yt.clone();
                            tokio::spawn(async move {
                                let _ = yt.like_video(&track_id, liked_now).await;
                            });
                        }
                        if liked_now {
                            liked_track = Some(item.track.clone());
                        }
                        self.refresh_db_views();
                    }
                    if let Some(t) = liked_track {
                        self.listener_ctx.record_play(&t, true);
                        prune_queue_tail(
                            &mut self.queue.items,
                            self.queue.current_index,
                            &self.listener_ctx,
                            true,
                        );
                    }
                    self.rebuild_home_sections();
                }
                Task::none()
            }
            Message::ToggleLikeTrack(mut track) => {
                let existing = self.db.get_track(&track.id).ok().flatten();
                let next_liked = !existing.as_ref().map(|t| t.liked).unwrap_or(track.liked);
                track.liked = next_liked;
                let _ = self.db.upsert_track(&track);
                let _ = self.db.set_liked(&track.id, next_liked);
                if self.yt.has_cookie() && !track.is_local {
                    let yt = self.yt.clone();
                    let vid = track.id.clone();
                    tokio::spawn(async move {
                        let _ = yt.like_video(&vid, next_liked).await;
                    });
                }
                if let Some(cur) = self
                    .queue
                    .current_index
                    .and_then(|i| self.queue.items.get_mut(i))
                    .filter(|item| item.track.id == track.id)
                {
                    cur.track.liked = next_liked;
                }
                self.refresh_db_views();
                if next_liked {
                    self.listener_ctx.record_play(&track, true);
                }
                self.rebuild_home_sections();
                Task::none()
            }
            Message::NewPlaylistNameChanged(s) => {
                self.new_playlist_name = s;
                Task::none()
            }
            Message::CreatePlaylist => {
                let name = self.new_playlist_name.trim().to_string();
                if !name.is_empty() {
                    let id = format!("local_{}", chrono::Utc::now().timestamp_millis());
                    let _ = self.db.create_playlist(&id, &name);
                    self.new_playlist_name.clear();
                    self.show_new_playlist_input = false;
                    self.selected_playlist_id = Some(id);
                    self.selected_playlist_tracks.clear();
                    self.refresh_db_views();
                    if !self.playlists.is_empty() {
                        self.tab = NavTab::Playlists;
                        self.library_tab = LibraryTab::Playlist(self.playlists.len() - 1);
                    }
                }
                Task::none()
            }
            Message::DeletePlaylist(pid) => {
                let _ = self.db.delete_playlist(&pid);
                if self.selected_playlist_id.as_deref() == Some(pid.as_str()) {
                    self.selected_playlist_id = None;
                    self.selected_playlist_tracks.clear();
                    self.library_tab = LibraryTab::Liked;
                }
                self.refresh_db_views();
                self.set_toast("Playlist deleted");
                Task::none()
            }
            Message::RemoveFromPlaylist { playlist_id, song_id } => {
                let _ = self.db.remove_from_playlist(&playlist_id, &song_id);
                if self.selected_playlist_id.as_deref() == Some(playlist_id.as_str()) {
                    self.selected_playlist_tracks =
                        self.db.playlist_tracks(&playlist_id).unwrap_or_default();
                }
                self.refresh_db_views();
                self.set_toast("Removed from playlist");
                Task::none()
            }
            Message::AddCurrentToPlaylist(pid) => {
                if let Some(cur) = self.queue.current() {
                    let _ = self.db.add_to_playlist(&pid, &cur.track);
                    self.refresh_db_views();
                    if self.selected_playlist_id.as_deref() == Some(pid.as_str()) {
                        self.selected_playlist_tracks =
                            self.db.playlist_tracks(&pid).unwrap_or_default();
                    }
                    self.set_toast("Added to playlist");
                }
                Task::none()
            }
            Message::ListenAndRecognize => {
                if self.recognizing {
                    return Task::none();
                }
                self.recognizing = true;
                self.last_recognised = None;
                self.last_recognised_track = None;
                self.recognition_status = Some("Listening for 8 seconds (with speed-retry fallback)...".to_string());
                let source = self.settings.recognition_source;
                let shazam = Arc::clone(&self.shazam);
                Task::perform(
                    async move {
                        let samples = tokio::task::spawn_blocking(move || {
                            capture_16khz_mono(source, Duration::from_secs(8))
                        })
                        .await
                        .map_err(|e| e.to_string())?
                        .map_err(|e| e.to_string())?;
                        Ok(shazam.identify_with_retry(&samples).await)
                    },
                    Message::RecognitionFinished,
                )
            }
            Message::RecognitionFinished(res) => {
                self.recognizing = false;
                match res {
                    Ok(RecognitionOutcome::Match(rec)) => {
                        self.recognition_status = Some(format!(
                            "Matched: {} — {}",
                            rec.title,
                            rec.artist.as_deref().unwrap_or("")
                        ));
                        let rec_id = self
                            .db
                            .add_recognition(
                                &rec.title,
                                rec.artist.as_deref(),
                                rec.artwork_url.as_deref(),
                                rec.isrc.as_deref(),
                                rec.shazam_key.as_deref(),
                                None,
                            )
                            .ok();
                        self.last_recognised = Some(rec.clone());
                        self.refresh_db_views();

                        let yt = self.yt.clone();
                        let query = rec.search_query();
                        let resolve_task = Task::perform(
                            async move {
                                let found = yt
                                    .search(&query, SearchFilter::Songs)
                                    .await
                                    .ok()
                                    .and_then(|items| {
                                        items.into_iter().find_map(|it| match it {
                                            BrowseItem::Track(t) => Some(t),
                                            _ => None,
                                        })
                                    });
                                (rec_id, found)
                            },
                            |(recognition_id, track)| Message::RecognitionTrackResolved {
                                recognition_id,
                                track,
                            },
                        );
                        return Task::batch([
                            self.fetch_cover_if_needed(rec.artwork_url.as_deref()),
                            resolve_task,
                        ]);
                    }
                    Ok(RecognitionOutcome::NoMatch) => {
                        self.recognition_status =
                            Some("No match found (including speed retries). Try a longer sample.".to_string());
                    }
                    Ok(RecognitionOutcome::Failed {
                        reason,
                        retry_after_secs,
                    }) => {
                        let msg = if reason == "silence" {
                            "No audio detected (silence). Check your audio source.".to_string()
                        } else if let Some(s) = retry_after_secs {
                            format!("Shazam rate-limited ({reason}). Retry in {s}s.")
                        } else {
                            format!("Recognition failed: {reason}")
                        };
                        self.recognition_status = Some(msg);
                    }
                    Err(e) => {
                        self.recognition_status = Some(format!("Capture failed: {e}"));
                    }
                }
                Task::none()
            }
            Message::RecognitionTrackResolved {
                recognition_id,
                track,
            } => {
                if let Some(t) = track {
                    let _ = self.db.upsert_track(&t);
                    if let Some(rid) = recognition_id {
                        let _ = self.db.update_recognition_video_id(rid, &t.id);
                    }
                    self.last_recognised_track = Some(t.clone());
                    self.refresh_db_views();
                    return self.fetch_cover_if_needed(t.thumbnail_url.as_deref());
                }
                Task::none()
            }
            Message::PlayRecognised(rec) => {
                if let Some(track) = self.last_recognised_track.clone() {
                    return self.update(Message::PlayTrack(track));
                }
                self.tab = NavTab::Search;
                self.show_expanded = false;
                self.search_query = rec.search_query();
                self.search_filter = SearchFilter::Songs;
                self.update(Message::SubmitSearch)
            }
            Message::OpenHistoryRecognition(entry) => {
                if let Some(vid) = entry.video_id {
                    if let Ok(Some(track)) = self.db.get_track(&vid) {
                        return self.update(Message::PlayTrack(track));
                    }
                }
                self.tab = NavTab::Search;
                self.show_expanded = false;
                self.search_query = match entry.artist {
                    Some(a) if !a.is_empty() => format!("{} {a}", entry.title),
                    _ => entry.title,
                };
                self.search_filter = SearchFilter::Songs;
                self.update(Message::SubmitSearch)
            }
            Message::LastFmApiKeyChanged(k) => {
                self.settings.lastfm_api_key = k;
                let _ = self.settings.save();
                Task::none()
            }
            Message::LastFmApiSecretChanged(s) => {
                self.settings.lastfm_api_secret = s;
                let _ = self.settings.save();
                Task::none()
            }
            Message::StartLastFmAuth => {
                let client = LastFmClient::new(
                    self.settings.lastfm_api_key.clone(),
                    self.settings.lastfm_api_secret.clone(),
                );
                if !client.is_configured() {
                    self.lastfm_status =
                        Some("Enter Last.fm API key and secret first.".to_string());
                    return Task::none();
                }
                self.lastfm_status = Some("Requesting authorization token...".to_string());
                Task::perform(
                    async move {
                        let tok = client.request_token().await.map_err(|e| e.to_string())?;
                        let url = client.authorize_url(&tok);
                        Ok((tok, url))
                    },
                    Message::LastFmTokenReceived,
                )
            }
            Message::LastFmTokenReceived(res) => {
                match res {
                    Ok((token, url)) => {
                        self.lastfm_pending_token = Some(token);
                        self.lastfm_status = Some(
                            "Approve Echo in your browser, then click Complete Login.".to_string(),
                        );
                        let _ = open_browser_url(&url);
                    }
                    Err(e) => {
                        self.lastfm_status = Some(format!("Last.fm auth error: {e}"));
                    }
                }
                Task::none()
            }
            Message::CompleteLastFmAuth => {
                let Some(token) = self.lastfm_pending_token.clone() else {
                    return Task::none();
                };
                let client = LastFmClient::new(
                    self.settings.lastfm_api_key.clone(),
                    self.settings.lastfm_api_secret.clone(),
                );
                Task::perform(
                    async move { client.session(&token).await.map_err(|e| e.to_string()) },
                    Message::LastFmSessionReceived,
                )
            }
            Message::LastFmSessionReceived(res) => {
                match res {
                    Ok(session) => {
                        self.settings.lastfm_username = Some(session.name.clone());
                        self.settings.lastfm_session_key = Some(session.key);
                        self.lastfm_pending_token = None;
                        self.lastfm_status = Some(format!("Connected as {}", session.name));
                        let _ = self.settings.save();
                    }
                    Err(e) => {
                        self.lastfm_status = Some(format!("Could not finish login: {e}"));
                    }
                }
                Task::none()
            }
            Message::DisconnectLastFm => {
                self.settings.lastfm_session_key = None;
                self.settings.lastfm_username = None;
                self.lastfm_pending_token = None;
                self.lastfm_status = Some("Disconnected".to_string());
                let _ = self.settings.save();
                Task::none()
            }
            Message::ToggleLastFmScrobbling(on) => {
                self.settings.lastfm_scrobbling_enabled = on;
                let _ = self.settings.save();
                Task::none()
            }
            Message::DownloadTrack(track) => {
                self.track_menu = None;
                if track.is_local
                    || self.downloaded_id_set.contains(&track.id)
                    || self.downloading_ids.contains(&track.id)
                {
                    return Task::none();
                }
                let Ok(dest_dir) = self.settings.effective_downloads_dir() else {
                    self.set_toast("Could not determine download folder");
                    return Task::none();
                };
                self.downloading_ids.insert(track.id.clone());
                let yt = self.yt.clone();
                let http = self.http.clone();
                let quality = self.settings.audio_quality;
                let t = track.clone();
                Task::perform(
                    async move {
                        let stream = yt
                            .resolve_stream(&t.id, quality)
                            .await
                            .map_err(|e| e.to_string())?;
                        let out_path =
                            download_file_path(&dest_dir, &t.title, &t.id, &stream.mime_type);
                        let resp = http
                            .get(&stream.url)
                            .header(reqwest::header::USER_AGENT, stream.user_agent)
                            .send()
                            .await
                            .map_err(|e| e.to_string())?
                            .error_for_status()
                            .map_err(|e| e.to_string())?;
                        let bytes = resp.bytes().await.map_err(|e| e.to_string())?;
                        let len = bytes.len() as u64;
                        let out_clone = out_path.clone();
                        tokio::task::spawn_blocking(move || -> std::io::Result<()> {
                            if let Some(parent) = out_clone.parent() {
                                std::fs::create_dir_all(parent)?;
                            }
                            std::fs::write(out_clone, &bytes)
                        })
                        .await
                        .map_err(|e| e.to_string())?
                        .map_err(|e| e.to_string())?;
                        Ok((
                            out_path.to_string_lossy().into_owned(),
                            stream.mime_type,
                            len,
                        ))
                    },
                    move |result| Message::DownloadCompleted {
                        track: track.clone(),
                        result,
                    },
                )
            }
            Message::DownloadTracks(tracks) => {
                let mut tasks = Vec::new();
                for t in tracks {
                    if !t.is_local
                        && !self.downloaded_id_set.contains(&t.id)
                        && !self.downloading_ids.contains(&t.id)
                    {
                        tasks.push(self.update(Message::DownloadTrack(t)));
                    }
                }
                if !tasks.is_empty() {
                    self.set_toast(format!("Downloading {} tracks...", tasks.len()));
                }
                Task::batch(tasks)
            }
            Message::DownloadCompleted { track, result } => {
                self.downloading_ids.remove(&track.id);
                match result {
                    Ok((path_str, mime, len)) => {
                        let path_clone = path_str.clone();
                        let title = track.title.clone();
                        let artist = track.artist_display();
                        let album = track.album.as_ref().map(|a| a.title.clone());
                        let cover = self.cover_bytes.get(&track.id).cloned();
                        tokio::task::spawn_blocking(move || {
                            let _ = tag_audio_file(
                                std::path::Path::new(&path_clone),
                                &title,
                                &artist,
                                album.as_deref(),
                                cover.as_deref(),
                            );
                        });
                        let _ = self.db.upsert_download(&track, &path_str, Some(&mime), len);
                        self.refresh_db_views();
                        self.set_toast(format!("Downloaded & tagged: {}", track.title));
                    }
                    Err(e) => {
                        self.download_status = Some(format!("Download failed ({}): {e}", track.title));
                        self.set_toast(format!("Download failed: {e}"));
                    }
                }
                Task::none()
            }
            Message::RemoveDownload(song_id) => {
                self.track_menu = None;
                let _ = self.db.remove_download(&song_id);
                self.refresh_db_views();
                self.set_toast("Removed download");
                Task::none()
            }
            Message::ClearDownloads => {
                let count = self.db.clear_downloads().unwrap_or(0);
                self.refresh_db_views();
                self.download_status = Some(format!("Cleared {count} downloaded tracks."));
                self.set_toast("Cleared downloads");
                Task::none()
            }
            Message::ScanDownloadFolders => {
                let mut folders = Vec::new();
                if let Ok(main_dir) = self.settings.effective_downloads_dir() {
                    folders.push(main_dir);
                }
                for p in &self.settings.extra_import_folders {
                    let trimmed = p.trim();
                    if !trimmed.is_empty() {
                        folders.push(PathBuf::from(trimmed));
                    }
                }
                let db = self.db.clone();
                let yt = self.yt.clone();
                Task::perform(
                    async move {
                        let pairs = tokio::task::spawn_blocking(move || scan_download_folders(&folders))
                            .await
                            .map_err(|e| e.to_string())?;
                        let mut imported = Vec::new();
                        for (vid, path) in pairs {
                            let len = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
                            let mut track = db.get_track(&vid).ok().flatten().unwrap_or_else(|| {
                                let title = path
                                    .file_stem()
                                    .and_then(|s| s.to_str())
                                    .unwrap_or(&vid)
                                    .replace(&format!("[{vid}]"), "")
                                    .trim()
                                    .to_string();
                                Track {
                                    id: vid.clone(),
                                    title: if title.is_empty() { vid.clone() } else { title },
                                    artists: Vec::new(),
                                    album: None,
                                    duration_secs: None,
                                    thumbnail_url: None,
                                    loudness_db: None,
                                    liked: false,
                                    is_local: false,
                                    local_path: None,
                                }
                            });
                            if track.loudness_db.is_none() {
                                if let Ok(Some(ldb)) = yt.fetch_loudness_db(&vid).await {
                                    track.loudness_db = Some(ldb);
                                }
                            }
                            imported.push((track, path.to_string_lossy().into_owned(), len));
                        }
                        Ok(imported)
                    },
                    Message::DownloadFoldersScanned,
                )
            }
            Message::DownloadFoldersScanned(res) => {
                match res {
                    Ok(items) => {
                        let count = items.len();
                        for (track, path_str, len) in items {
                            let _ = self.db.upsert_download(&track, &path_str, None, len);
                        }
                        self.refresh_db_views();
                        self.download_status =
                            Some(format!("Rescan finished: {count} [video_id] files matched."));
                        self.set_toast(format!("Matched {count} offline files"));
                    }
                    Err(e) => {
                        self.download_status = Some(format!("Rescan failed: {e}"));
                    }
                }
                Task::none()
            }
            Message::MigrateDownloads => {
                let Ok(dest_dir) = self.settings.effective_downloads_dir() else {
                    self.download_status = Some("Invalid download folder.".to_string());
                    return Task::none();
                };
                let existing = self.db.downloaded_tracks().unwrap_or_default();
                let mut moved = 0usize;
                for (t, src_path, len) in existing {
                    if let Ok(new_path) = migrate_download_file(&src_path, &dest_dir) {
                        let _ = self.db.upsert_download(
                            &t,
                            &new_path.to_string_lossy(),
                            None,
                            len,
                        );
                        moved += 1;
                    }
                }
                self.refresh_db_views();
                self.download_status = Some(format!(
                    "Migrated {moved} files to {}",
                    dest_dir.display()
                ));
                self.set_toast(format!("Migrated {moved} downloads"));
                Task::none()
            }
            Message::ClearStreamCache => {
                if let Ok(dir) = AppSettings::stream_cache_dir() {
                    let removed = clear_cache_dir(&dir).unwrap_or(0);
                    self.refresh_db_views();
                    self.download_status = Some(format!("Cleared {removed} cached streams."));
                    self.set_toast("Stream cache cleared");
                }
                Task::none()
            }
            Message::MaxCacheSizeChanged(v) => {
                self.cache_size_input = v.clone();
                if let Ok(mb) = v.trim().parse::<u64>() {
                    self.settings.max_cache_size_mb = mb;
                    let _ = self.settings.save();
                }
                Task::none()
            }
            Message::DownloadFolderChanged(v) => {
                self.download_folder_input = v;
                Task::none()
            }
            Message::ExtraImportFoldersChanged(v) => {
                self.extra_import_input = v;
                Task::none()
            }
            Message::SaveDownloadSettings => {
                let df = self.download_folder_input.trim();
                self.settings.download_folder = (!df.is_empty()).then(|| df.to_string());
                self.settings.extra_import_folders = parse_folder_list(&self.extra_import_input);
                let _ = self.settings.save();
                self.download_status = Some("Download folder settings saved.".to_string());
                self.set_toast("Download settings saved");
                Task::none()
            }
            Message::PickFolder(target) => {
                let title = match target {
                    FolderTarget::DownloadFolder => "Select Downloads Folder",
                    FolderTarget::ExtraImportFolder => "Select Extra [youtube_id] Folder",
                    FolderTarget::LocalMediaFolder => "Add Local Music Folder",
                    FolderTarget::ExcludedMediaFolder => "Add Excluded Folder",
                };
                let initial = match target {
                    FolderTarget::DownloadFolder => {
                        self.settings.effective_downloads_dir().ok()
                    }
                    FolderTarget::ExtraImportFolder => self
                        .settings
                        .extra_import_folders
                        .last()
                        .map(PathBuf::from),
                    FolderTarget::LocalMediaFolder => self
                        .settings
                        .local_media_folders
                        .last()
                        .map(PathBuf::from),
                    FolderTarget::ExcludedMediaFolder => self
                        .settings
                        .excluded_media_folders
                        .last()
                        .map(PathBuf::from),
                };
                let owner_hwnd = self.window_hwnd;
                Task::perform(
                    async move {
                        dialogs::pick_folder(owner_hwnd, title, initial.as_deref())
                    },
                    move |picked| Message::FolderPicked(target, picked),
                )
            }
            Message::FolderPicked(target, Ok(Some(path))) => {
                let path_str = path.to_string_lossy().into_owned();
                match target {
                    FolderTarget::DownloadFolder => {
                        self.download_folder_input = path_str.clone();
                        self.settings.download_folder = Some(path_str);
                        self.download_status = Some("Download folder updated.".to_string());
                        self.set_toast("Download folder updated");
                    }
                    FolderTarget::ExtraImportFolder => {
                        self.extra_import_input = append_folder_entry(
                            &mut self.settings.extra_import_folders,
                            &path_str,
                        );
                        self.download_status = Some("Added extra import folder.".to_string());
                        self.set_toast("Added import folder");
                    }
                    FolderTarget::LocalMediaFolder => {
                        self.local_folders_input = append_folder_entry(
                            &mut self.settings.local_media_folders,
                            &path_str,
                        );
                        self.set_toast("Added local music folder");
                    }
                    FolderTarget::ExcludedMediaFolder => {
                        self.excluded_folders_input = append_folder_entry(
                            &mut self.settings.excluded_media_folders,
                            &path_str,
                        );
                        self.set_toast("Added excluded folder");
                    }
                }
                let _ = self.settings.save();
                Task::none()
            }
            Message::FolderPicked(_, Ok(None)) => Task::none(),
            Message::FolderPicked(_, Err(e)) => {
                self.set_toast(format!("Folder picker error: {e}"));
                Task::none()
            }
            Message::LocalFoldersChanged(v) => {
                self.local_folders_input = v;
                self.settings.local_media_folders = parse_folder_list(&self.local_folders_input);
                let _ = self.settings.save();
                Task::none()
            }
            Message::ExcludedFoldersChanged(v) => {
                self.excluded_folders_input = v;
                self.settings.excluded_media_folders =
                    parse_folder_list(&self.excluded_folders_input);
                let _ = self.settings.save();
                Task::none()
            }
            Message::ScannerSensitivitySelected(sens) => {
                self.settings.scanner_sensitivity = sens;
                let _ = self.settings.save();
                Task::none()
            }
            Message::ToggleStrictFileNames(on) => {
                self.settings.strict_file_names = on;
                let _ = self.settings.save();
                Task::none()
            }
            Message::ToggleAutoScanLocal(on) => {
                self.settings.auto_scan_local_on_startup = on;
                let _ = self.settings.save();
                Task::none()
            }
            Message::ToggleLocalFolderTree => {
                self.local_folder_tree = !self.local_folder_tree;
                Task::none()
            }
            Message::ScanLocalMedia => {
                if self.local_scanning {
                    return Task::none();
                }
                self.local_scanning = true;
                self.local_scan_status = Some("Scanning local media folders...".to_string());
                let cfg = LocalMediaScanConfig {
                    include_folders: self
                        .settings
                        .local_media_folders
                        .iter()
                        .map(PathBuf::from)
                        .collect(),
                    excluded_folders: self
                        .settings
                        .excluded_media_folders
                        .iter()
                        .map(PathBuf::from)
                        .collect(),
                    sensitivity: self.settings.scanner_sensitivity,
                    strict_file_names: self.settings.strict_file_names,
                    cover_cache_dir: AppSettings::cover_cache_dir().ok(),
                };
                Task::perform(
                    async move {
                        tokio::task::spawn_blocking(move || scan_local_folders(&cfg))
                            .await
                            .unwrap_or_default()
                    },
                    Message::LocalMediaScanned,
                )
            }
            Message::LocalMediaScanned(tracks) => {
                self.local_scanning = false;
                let count = self.db.replace_local_tracks(&tracks).unwrap_or(tracks.len());
                self.refresh_db_views();
                let cover_urls: Vec<String> = self
                    .local_tracks
                    .iter()
                    .take(24)
                    .filter_map(|t| t.thumbnail_url.clone())
                    .collect();
                let mut tasks = Vec::new();
                for u in cover_urls {
                    tasks.push(self.fetch_cover_if_needed(Some(&u)));
                }
                self.local_scan_status = Some(format!("Indexed {count} local tracks."));
                self.set_toast(format!("Found {count} local tracks"));
                Task::batch(tasks)
            }
            Message::SelectSettingsSection(sec) => {
                self.settings_section = sec;
                Task::none()
            }
            Message::SettingsSearchChanged(q) => {
                self.settings_search_query = q;
                Task::none()
            }
            Message::ExcludeTrack(t) => {
                if !self.settings.excluded_tracks.contains(&t.id) {
                    self.settings.excluded_tracks.push(t.id.clone());
                    let _ = self.settings.save();
                    self.set_toast(format!("Will not recommend '{}'", t.title));
                }
                self.track_menu = None;
                Task::none()
            }
            Message::ExcludeArtist(artist) => {
                let a_trim = artist.trim().to_string();
                if !a_trim.is_empty() && !self.settings.excluded_artists.iter().any(|x| x.eq_ignore_ascii_case(&a_trim)) {
                    self.settings.excluded_artists.push(a_trim.clone());
                    let _ = self.settings.save();
                    self.set_toast(format!("Will not recommend songs by '{}'", a_trim));
                }
                self.track_menu = None;
                Task::none()
            }
            Message::RemoveExcludedArtist(artist) => {
                self.settings.excluded_artists.retain(|x| !x.eq_ignore_ascii_case(&artist));
                let _ = self.settings.save();
                self.set_toast(format!("Restored '{}' to recommendations", artist));
                Task::none()
            }
            Message::RemoveExcludedTrack(id) => {
                self.settings.excluded_tracks.retain(|x| x != &id);
                let _ = self.settings.save();
                self.set_toast("Restored song to recommendations");
                Task::none()
            }
            Message::SongsSortColumnChanged(col) => {
                if self.songs_sort_column == col {
                    self.songs_sort_ascending = !self.songs_sort_ascending;
                } else {
                    self.songs_sort_column = col;
                    self.songs_sort_ascending = true;
                }
                self.sort_all_songs();
                Task::none()
            }
            Message::SongsSortAscendingToggled => {
                self.songs_sort_ascending = !self.songs_sort_ascending;
                self.sort_all_songs();
                Task::none()
            }
            Message::PickPlaylistImportFile => {
                let hwnd = self.window_hwnd;
                Task::perform(
                    async move {
                        tokio::task::spawn_blocking(move || {
                            dialogs::pick_playlist_open(hwnd, None)
                        })
                        .await
                        .map_err(|e| e.to_string())?
                    },
                    Message::PlaylistImportFilePicked,
                )
            }
            Message::PlaylistImportFilePicked(res) => {
                match res {
                    Ok(Some(path)) => {
                        match import_playlist_file(&path) {
                            Ok(imported_pls) => {
                                let mut total_tracks = 0;
                                for pl in imported_pls {
                                    total_tracks += pl.tracks.len();
                                    let clean_name: String = pl.name.chars().filter(|c| c.is_alphanumeric() || *c == ' ' || *c == '_').collect();
                                    let pl_id = format!("local_imp_{}_{}", chrono::Utc::now().timestamp_millis(), clean_name.replace(' ', "_"));
                                    let _ = self.db.create_playlist(&pl_id, &pl.name);
                                    for (i, imp_t) in pl.tracks.into_iter().enumerate() {
                                        let track_id = format!("imp_{}_{}", chrono::Utc::now().timestamp_millis(), i);
                                        let track = Track {
                                            id: track_id,
                                            title: imp_t.title,
                                            artists: vec![ArtistRef { id: None, name: imp_t.artist }],
                                            album: imp_t.album.map(|a| AlbumRef { id: None, title: a }),
                                            duration_secs: imp_t.duration_secs,
                                            thumbnail_url: None,
                                            loudness_db: None,
                                            liked: false,
                                            is_local: false,
                                            local_path: None,
                                        };
                                        let _ = self.db.add_to_playlist(&pl_id, &track);
                                    }
                                }
                                self.refresh_db_views();
                                self.set_toast(format!("Imported {} tracks into playlists!", total_tracks));
                            }
                            Err(e) => {
                                self.set_toast(format!("Playlist import error: {e}"));
                            }
                        }
                    }
                    Ok(None) => {}
                    Err(e) => {
                        self.set_toast(format!("Failed to open file: {e}"));
                    }
                }
                Task::none()
            }
            Message::ExportBackup => {
                let default_path = default_backup_path().ok();
                let owner_hwnd = self.window_hwnd;
                Task::perform(
                    async move {
                        dialogs::pick_backup_save(owner_hwnd, default_path.as_deref())
                    },
                    Message::ExportBackupTo,
                )
            }
            Message::ExportBackupTo(Ok(Some(path))) => {
                match export_backup(&self.db, &self.settings, &path) {
                    Ok(bytes) => {
                        let kb = bytes / 1024;
                        self.backup_status = Some(format!("Exported {kb} KB to {}", path.display()));
                        self.set_toast("Backup exported");
                    }
                    Err(e) => {
                        self.backup_status = Some(format!("Export failed: {e}"));
                    }
                }
                Task::none()
            }
            Message::ExportBackupTo(Ok(None)) => Task::none(),
            Message::ExportBackupTo(Err(e)) => {
                self.backup_status = Some(format!("Export dialog error: {e}"));
                Task::none()
            }
            Message::ImportBackup => {
                let default_dir = default_backup_path()
                    .ok()
                    .and_then(|p| p.parent().map(Path::to_path_buf));
                let owner_hwnd = self.window_hwnd;
                Task::perform(
                    async move {
                        dialogs::pick_backup_open(owner_hwnd, default_dir.as_deref())
                    },
                    Message::ImportBackupFrom,
                )
            }
            Message::ImportBackupFrom(Ok(Some(bak_path))) => {
                let db_path = match AppSettings::db_path() {
                    Ok(p) => p,
                    Err(e) => {
                        self.backup_status = Some(format!("DB path error: {e}"));
                        return Task::none();
                    }
                };
                if !bak_path.exists() {
                    self.backup_status =
                        Some(format!("No backup file found at {}", bak_path.display()));
                    return Task::none();
                }
                // Release the open SQLite handle on Windows before replacing echo.db on disk.
                self.db = EchoDb::open_in_memory().expect("in-memory sqlite");
                match validate_and_unpack_backup(&bak_path, &db_path) {
                    Ok(restored_settings) => {
                        self.db = EchoDb::open(&db_path).unwrap_or_else(|_| {
                            EchoDb::open_in_memory().expect("in-memory sqlite")
                        });
                        self.settings = restored_settings;
                        self.yt.set_locale(
                            self.settings.youtube_gl.clone(),
                            self.settings.youtube_hl.clone(),
                        );
                        self.yt
                            .set_visitor_data(self.settings.youtube_visitor_data.clone());
                        self.yt.set_cookie(self.settings.youtube_cookie.clone());
                        self.yt_cookie_input =
                            self.settings.youtube_cookie.clone().unwrap_or_default();
                        self.cache_size_input = self.settings.max_cache_size_mb.to_string();
                        self.download_folder_input =
                            self.settings.download_folder.clone().unwrap_or_default();
                        self.extra_import_input = self.settings.extra_import_folders.join("; ");
                        self.local_folders_input = self.settings.local_media_folders.join("; ");
                        self.excluded_folders_input =
                            self.settings.excluded_media_folders.join("; ");
                        self.palette = self
                            .queue
                            .current()
                            .and_then(|c| c.track.thumbnail_url.as_ref())
                            .and_then(|u| self.cover_bytes.get(u))
                            .and_then(|b| DynamicPalette::from_image_bytes(b, self.settings.theme_mode))
                            .unwrap_or_else(|| DynamicPalette::default_for(self.settings.theme_mode));
                        self.audio.set_volume(self.settings.volume);
                        self.audio.set_skip_silence(self.settings.skip_silence);
                        self.audio
                            .set_dsp(self.settings.eq_preset, self.settings.spatial_mode);
                        self.refresh_db_views();
                        self.backup_status =
                            Some(format!("Restored backup from {}", bak_path.display()));
                        self.set_toast("Backup restored");
                    }
                    Err(e) => {
                        self.db = EchoDb::open(&db_path).unwrap_or_else(|_| {
                            EchoDb::open_in_memory().expect("in-memory sqlite")
                        });
                        self.backup_status = Some(format!("Import failed: {e}"));
                    }
                }
                Task::none()
            }
            Message::ImportBackupFrom(Ok(None)) => Task::none(),
            Message::ImportBackupFrom(Err(e)) => {
                self.backup_status = Some(format!("Import dialog error: {e}"));
                Task::none()
            }
            Message::ThemeModeSelected(mode) => {
                self.settings.theme_mode = mode;
                let current_cover = self
                    .queue
                    .current()
                    .and_then(|c| c.track.thumbnail_url.as_ref())
                    .and_then(|u| self.cover_bytes.get(u));
                self.palette = current_cover
                    .and_then(|b| DynamicPalette::from_image_bytes(b, mode))
                    .unwrap_or_else(|| DynamicPalette::default_for(mode));
                let _ = self.settings.save();
                Task::none()
            }
            Message::RecognitionSourceSelected(src) => {
                self.settings.recognition_source = src;
                let _ = self.settings.save();
                Task::none()
            }
            Message::StartFixVolumes => {
                if self.repair_running {
                    return Task::none();
                }
                let current_id = self.queue.current().map(|c| c.track.id.clone());
                let queue: Vec<Track> = self
                    .db
                    .tracks_missing_loudness()
                    .unwrap_or_default()
                    .into_iter()
                    .filter(|t| Some(&t.id) != current_id.as_ref())
                    .collect();
                if queue.is_empty() {
                    self.repair_status =
                        Some("All fetched songs already have loudness info.".to_string());
                    return Task::none();
                }
                self.repair_running = true;
                self.repair_total = queue.len();
                self.repair_done = 0;
                self.repair_fixed = 0;
                self.repair_consecutive_failures = 0;
                self.repair_queue = queue;
                self.schedule_next_volume_repair()
            }
            Message::StopFixVolumes => {
                self.repair_running = false;
                self.repair_queue.clear();
                self.repair_status = Some(format!(
                    "Stopped. Fixed {} songs, the rest are still to do.",
                    self.repair_fixed
                ));
                self.refresh_db_views();
                Task::none()
            }
            Message::FixOneVolumeDone {
                track_id,
                loudness_db,
                ok,
            } => {
                if !self.repair_running {
                    return Task::none();
                }
                self.repair_done += 1;
                if ok {
                    self.repair_consecutive_failures = 0;
                    if let Some(ldb) = loudness_db {
                        let _ = self.db.update_loudness(&track_id, ldb);
                        self.repair_fixed += 1;
                    }
                } else {
                    self.repair_consecutive_failures += 1;
                    if self.repair_consecutive_failures >= 8 {
                        self.repair_running = false;
                        self.repair_queue.clear();
                        self.repair_status = Some(format!(
                            "Stopped after several failures in a row. Fixed {}. Try again later.",
                            self.repair_fixed
                        ));
                        self.refresh_db_views();
                        return Task::none();
                    }
                }
                self.repair_status = Some(format!(
                    "Checking song {} of {}, fixed {} so far...",
                    self.repair_done, self.repair_total, self.repair_fixed
                ));
                self.schedule_next_volume_repair()
            }
            _ => Task::none(),
        }
    }
}

