use crate::client::{
    parse_cookie_map, sapisid_auth_header, YouTubeClientProfile, YouTubeLocale,
    API_URL_YOUTUBE_MUSIC, ORIGIN_YOUTUBE_MUSIC, REFERER_YOUTUBE_MUSIC,
};
use crate::models::{
    make_track, AccountPlaylist, AlbumPage, ArtistPage, ArtistSection, BrowseItem, ExplorePage,
    HomeChip, HomePage, HomeSection, MoodGenreButton, MoodGenreGroup, PlaylistPage, RelatedPage,
    ResolvedStream, SearchFilter, SearchResultPage, StreamAddressCache,
};
use echo_core::{AlbumRef, ArtistRef, AudioQuality, Track};
use reqwest::header::{HeaderMap, HeaderValue, CONTENT_TYPE, COOKIE, ORIGIN, REFERER, USER_AGENT};
use serde_json::{json, Value};
use std::collections::HashSet;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum YouTubeError {
    #[error("http error: {0}")]
    Http(#[from] reqwest::Error),
    #[error("json parse error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("unplayable track: {0}")]
    Unplayable(String),
    #[error("no playable audio stream found")]
    NoStream,
}

#[derive(Clone)]
pub struct YouTubeMusicClient {
    http: reqwest::Client,
    locale: Arc<Mutex<YouTubeLocale>>,
    visitor_data: Arc<Mutex<Option<String>>>,
    cookie: Arc<Mutex<Option<String>>>,
    stream_cache: StreamAddressCache,
}

impl Default for YouTubeMusicClient {
    fn default() -> Self {
        Self::new()
    }
}

impl YouTubeMusicClient {
    pub fn new() -> Self {
        let http = reqwest::Client::builder()
            .connect_timeout(Duration::from_secs(15))
            .timeout(Duration::from_secs(25))
            .build()
            .expect("reqwest client");
        Self {
            http,
            locale: Arc::new(Mutex::new(YouTubeLocale {
                gl: "US".to_string(),
                hl: "en".to_string(),
            })),
            visitor_data: Arc::new(Mutex::new(None)),
            cookie: Arc::new(Mutex::new(None)),
            stream_cache: StreamAddressCache::default(),
        }
    }

    pub fn set_locale(&self, gl: String, hl: String) {
        let mut loc = self.locale.lock().expect("locale mutex");
        loc.gl = gl;
        loc.hl = hl;
    }

    pub fn set_visitor_data(&self, visitor_data: Option<String>) {
        *self.visitor_data.lock().expect("visitor_data mutex") = visitor_data;
    }

    pub fn set_cookie(&self, cookie: Option<String>) {
        *self.cookie.lock().expect("cookie mutex") = cookie;
    }

    pub fn has_cookie(&self) -> bool {
        self.cookie
            .lock()
            .expect("cookie mutex")
            .as_deref()
            .map(|c| !c.trim().is_empty())
            .unwrap_or(false)
    }

    pub fn invalidate_stream(&self, video_id: &str) {
        self.stream_cache.invalidate(video_id);
    }

    pub async fn ensure_visitor_data(&self) -> Option<String> {
        if let Some(v) = self.visitor_data.lock().expect("visitor").clone() {
            if !v.is_empty() {
                return Some(v);
            }
        }
        let resp = self
            .http
            .get("https://music.youtube.com/sw.js_data")
            .header(USER_AGENT, YouTubeClientProfile::WEB_REMIX.user_agent)
            .send()
            .await
            .ok()?;
        let text = resp.text().await.ok()?;
        let stripped = text.trim_start_matches(")]}'").trim();
        let val: Value = serde_json::from_str(stripped).ok()?;
        let found = find_visitor_data(&val)?;
        self.set_visitor_data(Some(found.clone()));
        Some(found)
    }

    async fn post_innertube(
        &self,
        endpoint: &str,
        profile: &YouTubeClientProfile,
        body: Value,
        with_login: bool,
    ) -> Result<Value, YouTubeError> {
        let mut headers = HeaderMap::new();
        headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
        headers.insert("X-Goog-Api-Format-Version", HeaderValue::from_static("1"));
        headers.insert(
            "X-YouTube-Client-Name",
            HeaderValue::from_str(profile.client_id).unwrap(),
        );
        headers.insert(
            "X-YouTube-Client-Version",
            HeaderValue::from_str(profile.client_version).unwrap(),
        );
        headers.insert("X-Origin", HeaderValue::from_static(ORIGIN_YOUTUBE_MUSIC));
        headers.insert(ORIGIN, HeaderValue::from_static(ORIGIN_YOUTUBE_MUSIC));
        headers.insert(REFERER, HeaderValue::from_static(REFERER_YOUTUBE_MUSIC));
        headers.insert(USER_AGENT, HeaderValue::from_str(profile.user_agent).unwrap());

        if with_login && profile.login_supported {
            if let Some(cookie) = self.cookie.lock().expect("cookie").clone() {
                if let Ok(val) = HeaderValue::from_str(&cookie) {
                    headers.insert(COOKIE, val);
                }
                let map = parse_cookie_map(&cookie);
                if let Some(sapisid) = map.get("SAPISID") {
                    let unix_secs = std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .map(|d| d.as_secs() as i64)
                        .unwrap_or(0);
                    let auth = sapisid_auth_header(sapisid, unix_secs);
                    if let Ok(v) = HeaderValue::from_str(&auth) {
                        headers.insert("Authorization", v);
                    }
                }
            }
        }

        let url = format!("{API_URL_YOUTUBE_MUSIC}{endpoint}?prettyPrint=false");
        let resp = self
            .http
            .post(url)
            .headers(headers)
            .json(&body)
            .send()
            .await?
            .error_for_status()?;
        let val = resp.json::<Value>().await?;
        if self.visitor_data.lock().expect("vd").is_none() {
            if let Some(vd) = val
                .pointer("/responseContext/visitorData")
                .and_then(|v| v.as_str())
                .filter(|s| !s.is_empty())
            {
                self.set_visitor_data(Some(vd.to_string()));
            }
        }
        Ok(val)
    }

    fn base_context(&self, profile: &YouTubeClientProfile, hl_override: Option<&str>) -> Value {
        let loc = self.locale.lock().expect("locale").clone();
        let vd = self.visitor_data.lock().expect("vd").clone();
        profile.context_json(&loc.gl, hl_override.unwrap_or(&loc.hl), vd.as_deref(), None)
    }

    pub async fn search_suggestions(&self, query: &str) -> Result<Vec<String>, YouTubeError> {
        if query.trim().is_empty() {
            return Ok(Vec::new());
        }
        let body = json!({
            "context": self.base_context(&YouTubeClientProfile::WEB_REMIX, None),
            "input": query
        });
        let val = self
            .post_innertube(
                "music/get_search_suggestions",
                &YouTubeClientProfile::WEB_REMIX,
                body,
                false,
            )
            .await?;

        let mut suggestions = Vec::new();
        if let Some(arr) = val.pointer("/contents/0/searchSuggestionsSectionRenderer/contents").and_then(|v| v.as_array()) {
            for item in arr {
                if let Some(runs) = item.pointer("/searchSuggestionRenderer/suggestion/runs").and_then(|v| v.as_array()) {
                    let s: String = runs.iter().filter_map(|r| r.get("text").and_then(|t| t.as_str())).collect();
                    if !s.is_empty() {
                        suggestions.push(s);
                    }
                }
            }
        }
        Ok(suggestions)
    }

    pub async fn search(
        &self,
        query: &str,
        filter: SearchFilter,
    ) -> Result<Vec<BrowseItem>, YouTubeError> {
        Ok(self.search_page(query, filter).await?.items)
    }

    pub async fn search_page(
        &self,
        query: &str,
        filter: SearchFilter,
    ) -> Result<SearchResultPage, YouTubeError> {
        self.ensure_visitor_data().await;
        let body = json!({
            "context": self.base_context(&YouTubeClientProfile::WEB_REMIX, None),
            "query": query,
            "params": filter.params()
        });
        let val = self
            .post_innertube("search", &YouTubeClientProfile::WEB_REMIX, body, false)
            .await?;
        Ok(parse_search_response(&val, filter))
    }

    pub async fn search_continuation(
        &self,
        continuation: &str,
        filter: SearchFilter,
    ) -> Result<SearchResultPage, YouTubeError> {
        self.ensure_visitor_data().await;
        let body = json!({
            "context": self.base_context(&YouTubeClientProfile::WEB_REMIX, None),
            "continuation": continuation,
        });
        let val = self
            .post_innertube("search", &YouTubeClientProfile::WEB_REMIX, body, false)
            .await?;
        Ok(parse_search_continuation_response(&val, filter))
    }

    pub async fn home(&self) -> Result<Vec<HomeSection>, YouTubeError> {
        Ok(self.home_page(None, None).await?.sections)
    }

    pub async fn home_page(
        &self,
        params: Option<&str>,
        continuation: Option<&str>,
    ) -> Result<HomePage, YouTubeError> {
        self.ensure_visitor_data().await;
        let mut body = serde_json::Map::new();
        body.insert(
            "context".into(),
            self.base_context(&YouTubeClientProfile::WEB_REMIX, None),
        );
        if let Some(cont) = continuation {
            body.insert("continuation".into(), json!(cont));
        } else {
            body.insert("browseId".into(), json!("FEmusic_home"));
            if let Some(p) = params {
                body.insert("params".into(), json!(p));
            }
        }
        let val = self
            .post_innertube(
                "browse",
                &YouTubeClientProfile::WEB_REMIX,
                Value::Object(body),
                true,
            )
            .await?;
        Ok(parse_home_response(&val, continuation.is_some()))
    }

    pub async fn browse_playlist(&self, playlist_id: &str) -> Result<Vec<Track>, YouTubeError> {
        Ok(self.playlist_page(playlist_id).await?.tracks)
    }

    pub async fn playlist_page(&self, playlist_id: &str) -> Result<PlaylistPage, YouTubeError> {
        self.ensure_visitor_data().await;
        let browse_id = if playlist_id.starts_with("VL") {
            playlist_id.to_string()
        } else {
            format!("VL{playlist_id}")
        };
        let body = json!({
            "context": self.base_context(&YouTubeClientProfile::WEB_REMIX, None),
            "browseId": browse_id
        });
        let val = self
            .post_innertube("browse", &YouTubeClientProfile::WEB_REMIX, body, true)
            .await?;
        Ok(parse_playlist_response(playlist_id, &val))
    }

    pub async fn playlist_continuation(
        &self,
        playlist_id: &str,
        continuation: &str,
    ) -> Result<PlaylistPage, YouTubeError> {
        let body = json!({
            "context": self.base_context(&YouTubeClientProfile::WEB_REMIX, None),
            "continuation": continuation
        });
        let val = self
            .post_innertube("browse", &YouTubeClientProfile::WEB_REMIX, body, true)
            .await?;
        Ok(parse_playlist_continuation_response(playlist_id, &val))
    }

    pub async fn album(&self, browse_id: &str) -> Result<AlbumPage, YouTubeError> {
        self.ensure_visitor_data().await;
        let body = json!({
            "context": self.base_context(&YouTubeClientProfile::WEB_REMIX, None),
            "browseId": browse_id
        });
        let val = self
            .post_innertube("browse", &YouTubeClientProfile::WEB_REMIX, body, true)
            .await?;
        let mut page = parse_album_response(browse_id, &val);
        if let Some(pid) = page.playlist_id.clone() {
            if let Ok(pl) = self.playlist_page(&pid).await {
                if !pl.tracks.is_empty() {
                    page.tracks = pl.tracks;
                }
            }
        }
        // Propagate album metadata/cover if fallback tracks omitted it
        for track in &mut page.tracks {
            if track.album.is_none() {
                track.album = Some(AlbumRef {
                    id: Some(page.browse_id.clone()),
                    title: page.title.clone(),
                });
            }
            if track.artists.is_empty() {
                track.artists = page.artists.clone();
            }
            if track.thumbnail_url.is_none() {
                track.thumbnail_url = page.thumbnail_url.clone();
            }
        }
        Ok(page)
    }

    pub async fn artist(&self, browse_id: &str) -> Result<ArtistPage, YouTubeError> {
        self.ensure_visitor_data().await;
        let body = json!({
            "context": self.base_context(&YouTubeClientProfile::WEB_REMIX, None),
            "browseId": browse_id
        });
        let val = self
            .post_innertube("browse", &YouTubeClientProfile::WEB_REMIX, body, true)
            .await?;
        Ok(parse_artist_response(browse_id, &val))
    }

    pub async fn artist_items(
        &self,
        browse_id: &str,
        params: Option<&str>,
    ) -> Result<(String, Vec<BrowseItem>, Option<String>), YouTubeError> {
        self.ensure_visitor_data().await;
        let mut body = serde_json::Map::new();
        body.insert(
            "context".into(),
            self.base_context(&YouTubeClientProfile::WEB_REMIX, None),
        );
        body.insert("browseId".into(), json!(browse_id));
        if let Some(p) = params {
            body.insert("params".into(), json!(p));
        }
        let val = self
            .post_innertube(
                "browse",
                &YouTubeClientProfile::WEB_REMIX,
                Value::Object(body),
                true,
            )
            .await?;
        Ok(parse_artist_items_response(&val))
    }

    pub async fn explore(&self) -> Result<ExplorePage, YouTubeError> {
        self.ensure_visitor_data().await;
        let body = json!({
            "context": self.base_context(&YouTubeClientProfile::WEB_REMIX, None),
            "browseId": "FEmusic_explore"
        });
        let val = self
            .post_innertube("browse", &YouTubeClientProfile::WEB_REMIX, body, true)
            .await?;
        Ok(parse_explore_response(&val))
    }

    pub async fn mood_and_genres(&self) -> Result<Vec<MoodGenreGroup>, YouTubeError> {
        self.ensure_visitor_data().await;
        let body = json!({
            "context": self.base_context(&YouTubeClientProfile::WEB_REMIX, None),
            "browseId": "FEmusic_moods_and_genres"
        });
        let val = self
            .post_innertube("browse", &YouTubeClientProfile::WEB_REMIX, body, true)
            .await?;
        Ok(parse_mood_and_genres_response(&val))
    }

    pub async fn browse_category(
        &self,
        browse_id: &str,
        params: Option<&str>,
    ) -> Result<(String, Vec<HomeSection>), YouTubeError> {
        self.ensure_visitor_data().await;
        let mut body = serde_json::Map::new();
        body.insert(
            "context".into(),
            self.base_context(&YouTubeClientProfile::WEB_REMIX, None),
        );
        body.insert("browseId".into(), json!(browse_id));
        if let Some(p) = params {
            body.insert("params".into(), json!(p));
        }
        let val = self
            .post_innertube(
                "browse",
                &YouTubeClientProfile::WEB_REMIX,
                Value::Object(body),
                true,
            )
            .await?;
        Ok(parse_browse_category_response(&val))
    }

    pub async fn related_for_video(&self, video_id: &str) -> Result<RelatedPage, YouTubeError> {
        let next_body = json!({
            "context": self.base_context(&YouTubeClientProfile::WEB_REMIX, None),
            "videoId": video_id,
            "isAudioOnly": true,
        });
        let next_val = self
            .post_innertube("next", &YouTubeClientProfile::WEB_REMIX, next_body, true)
            .await?;
        let Some(tabs) = next_val
            .pointer("/contents/singleColumnMusicWatchNextResultsRenderer/tabbedRenderer/watchNextTabbedResultsRenderer/tabs")
            .and_then(|v| v.as_array())
        else {
            return Ok(RelatedPage::default());
        };
        let related_endpoint = tabs
            .iter()
            .find_map(|tab| {
                let ep = tab.pointer("/tabRenderer/endpoint/browseEndpoint")?;
                let id = ep.get("browseId").and_then(|v| v.as_str())?;
                id.starts_with("MPTR").then(|| {
                    (
                        id.to_string(),
                        ep.get("params").and_then(|v| v.as_str()).map(ToString::to_string),
                    )
                })
            })
            .or_else(|| {
                let ep = tabs.get(2)?.pointer("/tabRenderer/endpoint/browseEndpoint")?;
                let id = ep.get("browseId").and_then(|v| v.as_str())?;
                Some((
                    id.to_string(),
                    ep.get("params").and_then(|v| v.as_str()).map(ToString::to_string),
                ))
            });
        let Some((browse_id, params)) = related_endpoint else {
            return Ok(RelatedPage::default());
        };
        let mut body = serde_json::Map::new();
        body.insert(
            "context".into(),
            self.base_context(&YouTubeClientProfile::WEB_REMIX, None),
        );
        body.insert("browseId".into(), json!(browse_id));
        if let Some(p) = params {
            body.insert("params".into(), json!(p));
        }
        let val = self
            .post_innertube(
                "browse",
                &YouTubeClientProfile::WEB_REMIX,
                Value::Object(body),
                true,
            )
            .await?;
        Ok(parse_related_response(&val))
    }

    pub async fn account_playlists(&self) -> Result<Vec<AccountPlaylist>, YouTubeError> {
        if !self.has_cookie() {
            return Err(YouTubeError::Unplayable(
                "YouTube Music cookie is not configured".to_string(),
            ));
        }
        let body = json!({
            "context": self.base_context(&YouTubeClientProfile::WEB_REMIX, None),
            "browseId": "FEmusic_liked_playlists"
        });
        let val = self
            .post_innertube("browse", &YouTubeClientProfile::WEB_REMIX, body, true)
            .await?;
        Ok(parse_account_playlists_response(&val))
    }

    pub async fn liked_songs_remote(&self) -> Result<PlaylistPage, YouTubeError> {
        if !self.has_cookie() {
            return Err(YouTubeError::Unplayable(
                "YouTube Music cookie is not configured".to_string(),
            ));
        }
        self.playlist_page("VLLM").await
    }

    pub async fn like_video(&self, video_id: &str, like: bool) -> Result<(), YouTubeError> {
        if !self.has_cookie() {
            return Ok(());
        }
        let endpoint = if like { "like/like" } else { "like/removelike" };
        let body = json!({
            "context": self.base_context(&YouTubeClientProfile::WEB_REMIX, None),
            "target": { "videoId": video_id }
        });
        self.post_innertube(endpoint, &YouTubeClientProfile::WEB_REMIX, body, true)
            .await?;
        Ok(())
    }

    pub async fn subscribe_channel(
        &self,
        channel_id: &str,
        subscribe: bool,
    ) -> Result<(), YouTubeError> {
        if !self.has_cookie() {
            return Err(YouTubeError::Unplayable(
                "Sign in with YouTube cookie in Settings to subscribe".to_string(),
            ));
        }
        let endpoint = if subscribe {
            "subscription/subscribe"
        } else {
            "subscription/unsubscribe"
        };
        let body = json!({
            "context": self.base_context(&YouTubeClientProfile::WEB_REMIX, None),
            "channelIds": [channel_id]
        });
        self.post_innertube(endpoint, &YouTubeClientProfile::WEB_REMIX, body, true)
            .await?;
        Ok(())
    }

    pub async fn create_remote_playlist(&self, title: &str) -> Result<String, YouTubeError> {
        if !self.has_cookie() {
            return Err(YouTubeError::Unplayable(
                "YouTube Music cookie is not configured".to_string(),
            ));
        }
        let body = json!({
            "context": self.base_context(&YouTubeClientProfile::WEB_REMIX, None),
            "title": title,
            "privacyStatus": "PRIVATE"
        });
        let val = self
            .post_innertube("playlist/create", &YouTubeClientProfile::WEB_REMIX, body, true)
            .await?;
        val.get("playlistId")
            .and_then(|v| v.as_str())
            .map(ToString::to_string)
            .ok_or_else(|| YouTubeError::Unplayable("Missing playlistId in response".to_string()))
    }

    pub async fn add_to_remote_playlist(
        &self,
        playlist_id: &str,
        video_id: &str,
    ) -> Result<(), YouTubeError> {
        if !self.has_cookie() {
            return Err(YouTubeError::Unplayable(
                "YouTube Music cookie is not configured".to_string(),
            ));
        }
        let pid = playlist_id.strip_prefix("VL").unwrap_or(playlist_id);
        let body = json!({
            "context": self.base_context(&YouTubeClientProfile::WEB_REMIX, None),
            "playlistId": pid,
            "actions": [{
                "addedVideoId": video_id,
                "action": "ACTION_ADD_VIDEO"
            }]
        });
        self.post_innertube(
            "browse/edit_playlist",
            &YouTubeClientProfile::WEB_REMIX,
            body,
            true,
        )
        .await?;
        Ok(())
    }

    pub async fn register_playback(
        &self,
        tracking_url: &str,
        playlist_id: Option<&str>,
    ) -> Result<(), YouTubeError> {
        if !self.has_cookie() || tracking_url.is_empty() {
            return Ok(());
        }
        let normalized = tracking_url.replace("https://s.youtube.com", "https://music.youtube.com");
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let cpn = format!("{nanos:016x}");
        let cpn = &cpn[..cpn.len().min(16)];
        let sep = if normalized.contains('?') { '&' } else { '?' };
        let mut full_url = format!("{normalized}{sep}ver=2&c=WEB_REMIX&cpn={cpn}");
        if let Some(pid) = playlist_id.filter(|s| !s.is_empty()) {
            full_url.push_str(&format!("&list={}", urlencoding::encode(pid)));
        }
        let Some(cookie) = self.cookie.lock().expect("cookie").clone() else {
            return Ok(());
        };
        let mut headers = HeaderMap::new();
        headers.insert(USER_AGENT, HeaderValue::from_static(YouTubeClientProfile::WEB_REMIX.user_agent));
        headers.insert(ORIGIN, HeaderValue::from_static(ORIGIN_YOUTUBE_MUSIC));
        headers.insert(REFERER, HeaderValue::from_static(REFERER_YOUTUBE_MUSIC));
        if let Ok(v) = HeaderValue::from_str(&cookie) {
            headers.insert(COOKIE, v);
        }
        let map = parse_cookie_map(&cookie);
        if let Some(sapisid) = map.get("SAPISID") {
            let unix_secs = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs() as i64)
                .unwrap_or(0);
            if let Ok(v) = HeaderValue::from_str(&sapisid_auth_header(sapisid, unix_secs)) {
                headers.insert("Authorization", v);
            }
        }
        let _ = self.http.get(full_url).headers(headers).send().await;
        Ok(())
    }

    pub async fn next_radio(
        &self,
        video_id: &str,
        playlist_id: Option<&str>,
    ) -> Result<Vec<Track>, YouTubeError> {
        let mut body = serde_json::Map::new();
        body.insert(
            "context".into(),
            self.base_context(&YouTubeClientProfile::WEB_REMIX, None),
        );
        body.insert("videoId".into(), json!(video_id));
        if let Some(pid) = playlist_id {
            body.insert("playlistId".into(), json!(pid));
        } else {
            body.insert("playlistId".into(), json!(format!("RDAMVM{video_id}")));
        }
        body.insert("isAudioOnly".into(), json!(true));

        let val = self
            .post_innertube(
                "next",
                &YouTubeClientProfile::WEB_REMIX,
                Value::Object(body),
                true,
            )
            .await?;

        let mut tracks = Vec::new();
        let mut seen = HashSet::new();
        let contents = val
            .pointer("/contents/singleColumnMusicWatchNextResultsRenderer/tabbedRenderer/watchNextTabbedResultsRenderer/tabs/0/tabRenderer/content/musicQueueRenderer/content/playlistPanelRenderer/contents")
            .and_then(|v| v.as_array());
        if let Some(rows) = contents {
            for row in rows {
                if let Some(panel) = row.get("playlistPanelVideoRenderer") {
                    if let Some(track) = parse_playlist_panel_video(panel) {
                        if seen.insert(track.id.clone()) {
                            tracks.push(track);
                        }
                    }
                }
            }
        }
        Ok(tracks)
    }

    pub async fn resolve_stream(
        &self,
        video_id: &str,
        quality: AudioQuality,
    ) -> Result<ResolvedStream, YouTubeError> {
        if let Some(cached) = self.stream_cache.get(video_id) {
            return Ok(cached);
        }

        self.ensure_visitor_data().await;
        let mut last_err: Option<YouTubeError> = None;

        for profile in &YouTubeClientProfile::PLAYER_FALLBACK_CHAIN {
            let body = json!({
                "context": self.base_context(profile, Some("en")),
                "videoId": video_id,
                "contentCheckOk": true,
                "racyCheckOk": true
            });
            let val = match self.post_innertube("player", profile, body, false).await {
                Ok(v) => v,
                Err(e) => {
                    last_err = Some(e);
                    continue;
                }
            };

            let status = val
                .pointer("/playabilityStatus/status")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            if status != "OK" {
                let reason = val
                    .pointer("/playabilityStatus/reason")
                    .and_then(|v| v.as_str())
                    .unwrap_or(status)
                    .to_string();
                last_err = Some(YouTubeError::Unplayable(reason));
                continue;
            }

            if let Some(resolved) = pick_best_audio_stream(video_id, &val, profile, quality) {
                // Verify the stream actually serves bytes (Echo StreamCheck equivalent) so a 403
                // from one client profile immediately falls through to the next client.
                let ok = self
                    .http
                    .get(&resolved.url)
                    .header(USER_AGENT, &resolved.user_agent)
                    .header(reqwest::header::RANGE, "bytes=0-1")
                    .send()
                    .await
                    .map(|r| r.status().is_success())
                    .unwrap_or(false);
                if ok {
                    self.stream_cache.put(resolved.clone());
                    return Ok(resolved);
                }
                last_err = Some(YouTubeError::Unplayable(format!(
                    "{} stream URL rejected",
                    profile.id
                )));
            }
        }

        Err(last_err.unwrap_or(YouTubeError::NoStream))
    }

    pub async fn fetch_loudness_db(&self, video_id: &str) -> Result<Option<f32>, YouTubeError> {
        self.ensure_visitor_data().await;
        let profile = &YouTubeClientProfile::VISIONOS;
        let body = json!({
            "context": self.base_context(profile, Some("en")),
            "videoId": video_id,
            "contentCheckOk": true,
            "racyCheckOk": true
        });
        let val = self.post_innertube("player", profile, body, false).await?;
        Ok(extract_effective_loudness_db(&val))
    }
}

pub fn extract_effective_loudness_db(player: &Value) -> Option<f32> {
    let audio_cfg = player.pointer("/playerConfig/audioConfig")?;
    if let Some(ldb) = audio_cfg.get("loudnessDb").and_then(|v| v.as_f64()) {
        return Some(ldb as f32);
    }
    let perceptual = audio_cfg.get("perceptualLoudnessDb").and_then(|v| v.as_f64())?;
    let target = audio_cfg
        .get("loudnessTargetLkfs")
        .and_then(|v| v.as_f64())
        .unwrap_or(-14.0);
    Some((perceptual - target) as f32)
}

fn pick_best_audio_stream(
    video_id: &str,
    player: &Value,
    profile: &YouTubeClientProfile,
    quality: AudioQuality,
) -> Option<ResolvedStream> {
    let streaming = player.get("streamingData")?;
    let expires_in_secs = streaming
        .get("expiresInSeconds")
        .and_then(|v| v.as_u64().or_else(|| v.as_str().and_then(|s| s.parse().ok())))
        .unwrap_or(21_600);

    let adaptive = streaming.get("adaptiveFormats")?.as_array()?;
    let mut mp4_formats: Vec<&Value> = adaptive
        .iter()
        .filter(|f| {
            let mime = f.get("mimeType").and_then(|m| m.as_str()).unwrap_or("");
            let url = f.get("url").and_then(|u| u.as_str()).unwrap_or("");
            mime.starts_with("audio/mp4") && !url.is_empty()
        })
        .collect();

    if mp4_formats.is_empty() {
        return None;
    }

    mp4_formats.sort_by_key(|f| f.get("bitrate").and_then(|b| b.as_u64()).unwrap_or(0));

    let chosen = match quality {
        AudioQuality::Max | AudioQuality::Auto => mp4_formats.last().copied(),
        AudioQuality::High => mp4_formats
            .iter()
            .rev()
            .find(|f| f.get("bitrate").and_then(|b| b.as_u64()).unwrap_or(0) <= 170_000)
            .copied()
            .or_else(|| mp4_formats.first().copied()),
        AudioQuality::Low => mp4_formats.first().copied(),
    }?;

    let url = chosen.get("url")?.as_str()?.to_string();
    let mime_type = chosen.get("mimeType")?.as_str()?.to_string();
    let bitrate = chosen.get("bitrate").and_then(|b| b.as_u64()).unwrap_or(0) as u32;
    let content_length = chosen
        .get("contentLength")
        .and_then(|c| c.as_u64().or_else(|| c.as_str().and_then(|s| s.parse().ok())));
    let approx_duration_ms = chosen
        .get("approxDurationMs")
        .and_then(|c| c.as_u64().or_else(|| c.as_str().and_then(|s| s.parse().ok())));
    let format_loudness = chosen.get("loudnessDb").and_then(|v| v.as_f64()).map(|v| v as f32);
    let loudness_db = extract_effective_loudness_db(player).or(format_loudness);

    let details = player.get("videoDetails");
    let title = details.and_then(|d| d.get("title")).and_then(|v| v.as_str()).map(ToString::to_string);
    let author = details.and_then(|d| d.get("author")).and_then(|v| v.as_str()).map(ToString::to_string);
    let thumbnail_url = details
        .and_then(|d| d.pointer("/thumbnail/thumbnails"))
        .and_then(|t| t.as_array())
        .and_then(|arr| arr.last())
        .and_then(|t| t.get("url"))
        .and_then(|u| u.as_str())
        .map(ToString::to_string);

    let playback_tracking_url = player
        .pointer("/playbackTracking/videostatsPlaybackUrl/baseUrl")
        .and_then(|v| v.as_str())
        .map(ToString::to_string);

    Some(ResolvedStream {
        video_id: video_id.to_string(),
        url,
        mime_type,
        bitrate,
        content_length,
        approx_duration_ms,
        loudness_db,
        client_name: profile.client_name.to_string(),
        user_agent: profile.user_agent.to_string(),
        expires_in_secs,
        title,
        author,
        thumbnail_url,
        playback_tracking_url,
    })
}

fn find_visitor_data(val: &Value) -> Option<String> {
    match val {
        Value::String(s) if s.starts_with("Cgt") && s.len() > 16 => Some(s.clone()),
        Value::Array(arr) => arr.iter().find_map(find_visitor_data),
        Value::Object(map) => {
            if let Some(Value::String(v)) = map.get("visitorData") {
                return Some(v.clone());
            }
            map.values().find_map(find_visitor_data)
        }
        _ => None,
    }
}

fn best_thumbnail(node: &Value) -> Option<String> {
    node.pointer("/musicThumbnailRenderer/thumbnail/thumbnails")
        .or_else(|| node.pointer("/thumbnails"))
        .and_then(|v| v.as_array())
        .and_then(|arr| arr.last())
        .and_then(|t| t.get("url"))
        .and_then(|u| u.as_str())
        .map(ToString::to_string)
}

fn parse_duration_text(text: &str) -> Option<u32> {
    let parts: Vec<u32> = text
        .trim()
        .split(':')
        .map(|p| p.parse::<u32>().ok())
        .collect::<Option<Vec<_>>>()?;
    match parts.as_slice() {
        [m, s] => Some(m * 60 + s),
        [h, m, s] => Some(h * 3600 + m * 60 + s),
        _ => None,
    }
}

fn parse_runs_artists_and_album(runs: &[Value]) -> (Vec<ArtistRef>, Option<AlbumRef>, Option<u32>) {
    let mut segments: Vec<Vec<&Value>> = vec![Vec::new()];
    for r in runs {
        let text = r.get("text").and_then(|t| t.as_str()).unwrap_or("");
        if text == " • " {
            segments.push(Vec::new());
        } else {
            segments.last_mut().unwrap().push(r);
        }
    }

    let mut artists = Vec::new();
    let mut album = None;
    let mut duration = None;

    for seg in &segments {
        for r in seg {
            let text = r.get("text").and_then(|t| t.as_str()).unwrap_or("").trim();
            if text.is_empty() || text == "," || text == "&" {
                continue;
            }
            if let Some(d) = parse_duration_text(text) {
                duration = Some(d);
                continue;
            }
            let browse_id = r
                .pointer("/navigationEndpoint/browseEndpoint/browseId")
                .and_then(|v| v.as_str());
            let page_type = r
                .pointer("/navigationEndpoint/browseEndpoint/browseEndpointContextSupportedConfigs/browseEndpointContextMusicConfig/pageType")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            if page_type == "MUSIC_PAGE_TYPE_ALBUM" || browse_id.map(|id| id.starts_with("MPRE")).unwrap_or(false) {
                album = Some(AlbumRef {
                    id: browse_id.map(ToString::to_string),
                    title: text.to_string(),
                });
            } else if page_type == "MUSIC_PAGE_TYPE_ARTIST" || browse_id.map(|id| id.starts_with("UC")).unwrap_or(false) {
                artists.push(ArtistRef {
                    id: browse_id.map(ToString::to_string),
                    name: text.to_string(),
                });
            }
        }
    }

    if artists.is_empty() {
        // Fallback: take first segment text before separator
        if let Some(first_seg) = segments.first() {
            let name: String = first_seg
                .iter()
                .filter_map(|r| r.get("text").and_then(|t| t.as_str()))
                .collect();
            let trimmed = name.trim();
            if !trimmed.is_empty() && parse_duration_text(trimmed).is_none() {
                // If first segment looks like "Song • Artist", check second segment
                if matches!(trimmed, "Song" | "Video" | "Single" | "EP" | "Album") {
                    if let Some(sec) = segments.get(1) {
                        let sec_name: String = sec
                            .iter()
                            .filter_map(|r| r.get("text").and_then(|t| t.as_str()))
                            .collect();
                        if !sec_name.trim().is_empty() {
                            artists.push(ArtistRef {
                                id: None,
                                name: sec_name.trim().to_string(),
                            });
                        }
                    }
                } else {
                    artists.push(ArtistRef {
                        id: None,
                        name: trimmed.to_string(),
                    });
                }
            }
        }
    }

    (artists, album, duration)
}

fn parse_responsive_item(r: &Value, filter: SearchFilter) -> Option<BrowseItem> {
    let title = r
        .pointer("/flexColumns/0/musicResponsiveListItemFlexColumnRenderer/text/runs/0/text")
        .and_then(|v| v.as_str())?
        .to_string();
    let thumb = r.get("thumbnail").and_then(best_thumbnail);

    let video_id = r
        .pointer("/playlistItemData/videoId")
        .or_else(|| r.pointer("/overlay/musicItemThumbnailOverlayRenderer/content/musicPlayButtonRenderer/playNavigationEndpoint/watchEndpoint/videoId"))
        .and_then(|v| v.as_str())
        .map(ToString::to_string);

    let sub_runs = r
        .pointer("/flexColumns/1/musicResponsiveListItemFlexColumnRenderer/text/runs")
        .and_then(|v| v.as_array())
        .map(|v| v.as_slice())
        .unwrap_or(&[]);
    let (artists, album, duration) = parse_runs_artists_and_album(sub_runs);

    if let Some(id) = video_id {
        return Some(BrowseItem::Track(make_track(
            id, title, artists, album, duration, thumb,
        )));
    }

    let browse_id = r
        .pointer("/navigationEndpoint/browseEndpoint/browseId")
        .and_then(|v| v.as_str())
        .map(ToString::to_string)?;
    let page_type = r
        .pointer("/navigationEndpoint/browseEndpoint/browseEndpointContextSupportedConfigs/browseEndpointContextMusicConfig/pageType")
        .and_then(|v| v.as_str())
        .unwrap_or("");

    if page_type == "MUSIC_PAGE_TYPE_ARTIST" || filter == SearchFilter::Artists {
        return Some(BrowseItem::Artist {
            browse_id,
            name: title,
            thumbnail_url: thumb,
        });
    }
    if page_type == "MUSIC_PAGE_TYPE_ALBUM" || filter == SearchFilter::Albums {
        let playlist_id = r
            .pointer("/overlay/musicItemThumbnailOverlayRenderer/content/musicPlayButtonRenderer/playNavigationEndpoint/watchPlaylistEndpoint/playlistId")
            .and_then(|v| v.as_str())
            .map(ToString::to_string);
        let artist = artists
            .first()
            .map(|a| a.name.clone())
            .unwrap_or_else(|| "Unknown Artist".to_string());
        return Some(BrowseItem::Album {
            browse_id,
            playlist_id,
            title,
            artist,
            year: None,
            thumbnail_url: thumb,
        });
    }

    let pid = browse_id.strip_prefix("VL").unwrap_or(&browse_id).to_string();
    let author = artists
        .first()
        .map(|a| a.name.clone())
        .unwrap_or_else(|| "YouTube Music".to_string());
    Some(BrowseItem::Playlist {
        id: pid,
        title,
        author,
        thumbnail_url: thumb,
    })
}

fn parse_two_row_item(r: &Value) -> Option<BrowseItem> {
    let title = r
        .pointer("/title/runs/0/text")
        .and_then(|v| v.as_str())?
        .to_string();
    let thumb = r.pointer("/thumbnailRenderer").and_then(best_thumbnail);
    let sub_runs = r
        .pointer("/subtitle/runs")
        .and_then(|v| v.as_array())
        .map(|v| v.as_slice())
        .unwrap_or(&[]);
    let (artists, album, duration) = parse_runs_artists_and_album(sub_runs);

    if let Some(video_id) = r
        .pointer("/navigationEndpoint/watchEndpoint/videoId")
        .and_then(|v| v.as_str())
    {
        return Some(BrowseItem::Track(make_track(
            video_id.to_string(),
            title,
            artists,
            album,
            duration,
            thumb,
        )));
    }

    let browse_id = r
        .pointer("/navigationEndpoint/browseEndpoint/browseId")
        .and_then(|v| v.as_str())
        .map(ToString::to_string)?;
    let page_type = r
        .pointer("/navigationEndpoint/browseEndpoint/browseEndpointContextSupportedConfigs/browseEndpointContextMusicConfig/pageType")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    if page_type == "MUSIC_PAGE_TYPE_ALBUM" {
        let playlist_id = r
            .pointer("/thumbnailOverlay/musicItemThumbnailOverlayRenderer/content/musicPlayButtonRenderer/playNavigationEndpoint/watchPlaylistEndpoint/playlistId")
            .and_then(|v| v.as_str())
            .map(ToString::to_string);
        let artist = artists
            .first()
            .map(|a| a.name.clone())
            .unwrap_or_else(|| "Various Artists".to_string());
        Some(BrowseItem::Album {
            browse_id,
            playlist_id,
            title,
            artist,
            year: None,
            thumbnail_url: thumb,
        })
    } else if page_type == "MUSIC_PAGE_TYPE_ARTIST" {
        Some(BrowseItem::Artist {
            browse_id,
            name: title,
            thumbnail_url: thumb,
        })
    } else {
        let pid = browse_id.strip_prefix("VL").unwrap_or(&browse_id).to_string();
        let author = artists
            .first()
            .map(|a| a.name.clone())
            .unwrap_or_else(|| "YouTube Music".to_string());
        Some(BrowseItem::Playlist {
            id: pid,
            title,
            author,
            thumbnail_url: thumb,
        })
    }
}

fn parse_playlist_panel_video(panel: &Value) -> Option<Track> {
    let id = panel.get("videoId").and_then(|v| v.as_str())?.to_string();
    let title = panel
        .pointer("/title/runs/0/text")
        .and_then(|v| v.as_str())?
        .to_string();
    let sub_runs = panel
        .pointer("/longBylineText/runs")
        .or_else(|| panel.pointer("/shortBylineText/runs"))
        .and_then(|v| v.as_array())
        .map(|v| v.as_slice())
        .unwrap_or(&[]);
    let (artists, album, _) = parse_runs_artists_and_album(sub_runs);
    let duration = panel
        .pointer("/lengthText/runs/0/text")
        .and_then(|v| v.as_str())
        .and_then(parse_duration_text);
    let thumb = panel.get("thumbnail").and_then(best_thumbnail);
    Some(make_track(id, title, artists, album, duration, thumb))
}

fn join_runs_text(runs: Option<&Value>) -> String {
    runs.and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|r| r.get("text").and_then(|t| t.as_str()))
                .collect::<String>()
        })
        .unwrap_or_default()
        .trim()
        .to_string()
}

fn extract_continuation(node: Option<&Value>, items: Option<&[Value]>) -> Option<String> {
    if let Some(tok) = node
        .and_then(|n| n.pointer("/continuations/0/nextContinuationData/continuation"))
        .and_then(|v| v.as_str())
    {
        return Some(tok.to_string());
    }
    if let Some(arr) = items {
        for entry in arr.iter().rev() {
            if let Some(tok) = entry
                .pointer("/continuationItemRenderer/continuationEndpoint/continuationCommand/token")
                .and_then(|v| v.as_str())
            {
                return Some(tok.to_string());
            }
        }
    }
    None
}

fn parse_browse_entries(entries: &[Value], default_filter: SearchFilter) -> Vec<BrowseItem> {
    let mut items = Vec::new();
    let mut seen = HashSet::new();
    for entry in entries {
        let parsed = if let Some(r) = entry.get("musicResponsiveListItemRenderer") {
            parse_responsive_item(r, default_filter)
        } else if let Some(r) = entry.get("musicTwoRowItemRenderer") {
            parse_two_row_item(r)
        } else {
            None
        };
        if let Some(item) = parsed {
            if seen.insert(item.stable_key()) {
                items.push(item);
            }
        }
    }
    items
}

fn parse_track_rows(rows: &[Value]) -> Vec<Track> {
    let mut tracks = Vec::new();
    let mut seen = HashSet::new();
    for row in rows {
        if let Some(r) = row.get("musicResponsiveListItemRenderer") {
            if let Some(BrowseItem::Track(t)) = parse_responsive_item(r, SearchFilter::Songs) {
                if seen.insert(t.id.clone()) {
                    tracks.push(t);
                }
            }
        }
    }
    tracks
}

fn parse_search_response(val: &Value, filter: SearchFilter) -> SearchResultPage {
    let mut items = Vec::new();
    let mut seen = HashSet::new();
    let mut continuation = None;
    let Some(sections) = val
        .pointer("/contents/tabbedSearchResultsRenderer/tabs/0/tabRenderer/content/sectionListRenderer/contents")
        .and_then(|v| v.as_array())
    else {
        return SearchResultPage::default();
    };

    for sec in sections {
        if let Some(shelf) = sec.get("musicShelfRenderer") {
            if let Some(contents) = shelf.get("contents").and_then(|v| v.as_array()) {
                if continuation.is_none() {
                    continuation = extract_continuation(Some(shelf), Some(contents));
                }
                for item in parse_browse_entries(contents, filter) {
                    if seen.insert(item.stable_key()) {
                        items.push(item);
                    }
                }
            }
        }
        if let Some(contents) = sec
            .pointer("/itemSectionRenderer/contents")
            .and_then(|v| v.as_array())
        {
            for item in parse_browse_entries(contents, filter) {
                if seen.insert(item.stable_key()) {
                    items.push(item);
                }
            }
        }
    }
    SearchResultPage {
        items,
        continuation,
    }
}

fn parse_search_continuation_response(val: &Value, filter: SearchFilter) -> SearchResultPage {
    if let Some(shelf) = val.pointer("/continuationContents/musicShelfContinuation") {
        let contents = shelf
            .get("contents")
            .and_then(|v| v.as_array())
            .map(Vec::as_slice)
            .unwrap_or(&[]);
        return SearchResultPage {
            items: parse_browse_entries(contents, filter),
            continuation: extract_continuation(Some(shelf), Some(contents)),
        };
    }
    SearchResultPage::default()
}

fn parse_home_response(val: &Value, is_continuation: bool) -> HomePage {
    let section_list = if is_continuation {
        val.pointer("/continuationContents/sectionListContinuation")
    } else {
        val.pointer("/contents/singleColumnBrowseResultsRenderer/tabs/0/tabRenderer/content/sectionListRenderer")
    };
    let Some(sl) = section_list else {
        return HomePage::default();
    };

    let mut chips = Vec::new();
    if let Some(raw_chips) = sl
        .pointer("/header/chipCloudRenderer/chips")
        .and_then(|v| v.as_array())
    {
        for c in raw_chips {
            let renderer = c.get("chipCloudChipRenderer").unwrap_or(c);
            let title = join_runs_text(renderer.pointer("/text/runs"));
            let params = renderer
                .pointer("/navigationEndpoint/browseEndpoint/params")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            if !title.is_empty() && !params.is_empty() {
                chips.push(HomeChip { title, params });
            }
        }
    }

    let mut sections = Vec::new();
    if let Some(raw_sections) = sl.get("contents").and_then(|v| v.as_array()) {
        for sec in raw_sections {
            if let Some(shelf) = sec.get("musicCarouselShelfRenderer") {
                let title = shelf
                    .pointer("/header/musicCarouselShelfBasicHeaderRenderer/title/runs/0/text")
                    .and_then(|v| v.as_str())
                    .unwrap_or("Discover")
                    .to_string();
                let items = shelf
                    .get("contents")
                    .and_then(|v| v.as_array())
                    .map(|arr| parse_browse_entries(arr, SearchFilter::Songs))
                    .unwrap_or_default();
                if !items.is_empty() {
                    sections.push(HomeSection { title, items });
                }
            }
        }
    }

    HomePage {
        chips,
        sections,
        continuation: extract_continuation(Some(sl), None),
    }
}

fn parse_playlist_response(playlist_id: &str, val: &Value) -> PlaylistPage {
    let header = val
        .pointer("/contents/twoColumnBrowseResultsRenderer/tabs/0/tabRenderer/content/sectionListRenderer/contents/0/musicResponsiveHeaderRenderer")
        .or_else(|| val.pointer("/header/musicDetailHeaderRenderer"))
        .or_else(|| val.pointer("/header/musicEditablePlaylistDetailHeaderRenderer/header/musicDetailHeaderRenderer"));

    let title = header
        .map(|h| join_runs_text(h.pointer("/title/runs")))
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| playlist_id.to_string());
    let author = header
        .map(|h| {
            join_runs_text(
                h.pointer("/straplineTextOne/runs")
                    .or_else(|| h.pointer("/subtitle/runs")),
            )
        })
        .filter(|s| !s.is_empty());
    let track_count_text = header
        .map(|h| join_runs_text(h.pointer("/secondSubtitle/runs")))
        .filter(|s| !s.is_empty());
    let thumbnail_url = header
        .and_then(|h| h.get("thumbnail"))
        .and_then(best_thumbnail);
    let is_editable = val
        .pointer("/header/musicEditablePlaylistDetailHeaderRenderer")
        .is_some();

    let shelf = val
        .pointer("/contents/twoColumnBrowseResultsRenderer/secondaryContents/sectionListRenderer/contents/0/musicPlaylistShelfRenderer")
        .or_else(|| val.pointer("/contents/twoColumnBrowseResultsRenderer/secondaryContents/sectionListRenderer/contents/0/musicShelfRenderer"))
        .or_else(|| val.pointer("/contents/singleColumnBrowseResultsRenderer/tabs/0/tabRenderer/content/sectionListRenderer/contents/0/musicPlaylistShelfRenderer"));

    let contents = shelf
        .and_then(|s| s.get("contents"))
        .and_then(|v| v.as_array())
        .map(Vec::as_slice)
        .unwrap_or(&[]);

    PlaylistPage {
        id: playlist_id.to_string(),
        title,
        author,
        thumbnail_url,
        track_count_text,
        tracks: parse_track_rows(contents),
        continuation: extract_continuation(shelf, Some(contents)),
        is_editable,
    }
}

fn parse_playlist_continuation_response(playlist_id: &str, val: &Value) -> PlaylistPage {
    if let Some(shelf) = val.pointer("/continuationContents/musicPlaylistShelfContinuation") {
        let contents = shelf
            .get("contents")
            .and_then(|v| v.as_array())
            .map(Vec::as_slice)
            .unwrap_or(&[]);
        return PlaylistPage {
            id: playlist_id.to_string(),
            tracks: parse_track_rows(contents),
            continuation: extract_continuation(Some(shelf), Some(contents)),
            ..PlaylistPage::default()
        };
    }
    if let Some(items) = val
        .pointer("/onResponseReceivedActions/0/appendContinuationItemsAction/continuationItems")
        .and_then(|v| v.as_array())
    {
        return PlaylistPage {
            id: playlist_id.to_string(),
            tracks: parse_track_rows(items),
            continuation: extract_continuation(None, Some(items)),
            ..PlaylistPage::default()
        };
    }
    PlaylistPage {
        id: playlist_id.to_string(),
        ..PlaylistPage::default()
    }
}

fn parse_album_response(browse_id: &str, val: &Value) -> AlbumPage {
    let playlist_id = val
        .pointer("/microformat/microformatDataRenderer/urlCanonical")
        .and_then(|v| v.as_str())
        .and_then(|url| url.rsplit('=').next())
        .filter(|s| !s.is_empty())
        .map(ToString::to_string);

    let header = val
        .pointer("/contents/twoColumnBrowseResultsRenderer/tabs/0/tabRenderer/content/sectionListRenderer/contents/0/musicResponsiveHeaderRenderer")
        .or_else(|| val.pointer("/header/musicDetailHeaderRenderer"));

    let title = header
        .map(|h| join_runs_text(h.pointer("/title/runs")))
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "Album".to_string());
    let thumbnail_url = header
        .and_then(|h| h.get("thumbnail"))
        .and_then(best_thumbnail);

    let mut artists = Vec::new();
    if let Some(runs) = header
        .and_then(|h| h.pointer("/straplineTextOne/runs"))
        .and_then(|v| v.as_array())
    {
        for r in runs {
            let name = r.get("text").and_then(|t| t.as_str()).unwrap_or("").trim();
            if name.is_empty() || name == "•" || name == "," || name == "&" {
                continue;
            }
            let id = r
                .pointer("/navigationEndpoint/browseEndpoint/browseId")
                .and_then(|v| v.as_str())
                .map(ToString::to_string);
            artists.push(ArtistRef {
                id,
                name: name.to_string(),
            });
        }
    }

    let subtitle_str = header
        .map(|h| join_runs_text(h.pointer("/subtitle/runs")))
        .filter(|s| !s.is_empty());
    let year = header
        .and_then(|h| h.pointer("/subtitle/runs"))
        .and_then(|v| v.as_array())
        .and_then(|runs| {
            runs.iter()
                .rev()
                .filter_map(|r| r.get("text").and_then(|t| t.as_str()))
                .find_map(|s| s.trim().parse::<u32>().ok())
        });

    let secondary = val
        .pointer("/contents/twoColumnBrowseResultsRenderer/secondaryContents/sectionListRenderer/contents")
        .and_then(|v| v.as_array());
    let fallback_tracks = secondary
        .and_then(|arr| arr.first())
        .and_then(|sec| sec.pointer("/musicShelfRenderer/contents"))
        .and_then(|v| v.as_array())
        .map(|rows| parse_track_rows(rows))
        .unwrap_or_default();

    let other_versions = secondary
        .and_then(|arr| arr.get(1))
        .and_then(|sec| sec.pointer("/musicCarouselShelfRenderer/contents"))
        .and_then(|v| v.as_array())
        .map(|arr| parse_browse_entries(arr, SearchFilter::Albums))
        .unwrap_or_default();

    AlbumPage {
        browse_id: browse_id.to_string(),
        playlist_id,
        title,
        artists,
        year,
        thumbnail_url,
        subtitle: subtitle_str,
        tracks: fallback_tracks,
        other_versions,
    }
}

fn parse_artist_response(browse_id: &str, val: &Value) -> ArtistPage {
    let immersive = val.pointer("/header/musicImmersiveHeaderRenderer");
    let visual = val.pointer("/header/musicVisualHeaderRenderer");
    let basic = val.pointer("/header/musicHeaderRenderer");
    let header = immersive.or(visual).or(basic);

    let name = header
        .map(|h| join_runs_text(h.pointer("/title/runs")))
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "Artist".to_string());
    let thumbnail_url = immersive
        .and_then(|h| h.get("thumbnail"))
        .and_then(best_thumbnail)
        .or_else(|| {
            visual
                .and_then(|h| h.get("foregroundThumbnail"))
                .and_then(best_thumbnail)
        })
        .or_else(|| {
            header
                .and_then(|h| h.get("thumbnail"))
                .and_then(best_thumbnail)
        });
    let channel_id = immersive
        .and_then(|h| {
            h.pointer("/subscriptionButton/subscribeButtonRenderer/channelId")
                .and_then(|v| v.as_str())
        })
        .map(ToString::to_string);
    let subscriber_count = immersive
        .map(|h| {
            join_runs_text(
                h.pointer("/subscriptionButton/subscribeButtonRenderer/subscriberCountText/runs")
                    .or_else(|| {
                        h.pointer(
                            "/subscriptionButton/subscribeButtonRenderer/longSubscriberCountText/runs",
                        )
                    }),
            )
        })
        .filter(|s| !s.is_empty());
    let monthly_listeners = immersive
        .map(|h| join_runs_text(h.pointer("/monthlyListenerCount/runs")))
        .filter(|s| !s.is_empty());
    let description = header
        .map(|h| join_runs_text(h.pointer("/description/runs")))
        .filter(|s| !s.is_empty());

    let shuffle_playlist_id = immersive
        .and_then(|h| {
            h.pointer("/playButton/buttonRenderer/navigationEndpoint/watchEndpoint/playlistId")
                .or_else(|| {
                    h.pointer(
                        "/playButton/buttonRenderer/navigationEndpoint/watchPlaylistEndpoint/playlistId",
                    )
                })
                .and_then(|v| v.as_str())
        })
        .map(ToString::to_string);
    let radio_playlist_id = immersive
        .and_then(|h| {
            h.pointer("/startRadioButton/buttonRenderer/navigationEndpoint/watchEndpoint/playlistId")
                .and_then(|v| v.as_str())
        })
        .map(ToString::to_string);
    let radio_video_id = immersive
        .and_then(|h| {
            h.pointer("/startRadioButton/buttonRenderer/navigationEndpoint/watchEndpoint/videoId")
                .and_then(|v| v.as_str())
        })
        .map(ToString::to_string);

    let mut sections = Vec::new();
    if let Some(raw) = val
        .pointer("/contents/singleColumnBrowseResultsRenderer/tabs/0/tabRenderer/content/sectionListRenderer/contents")
        .and_then(|v| v.as_array())
    {
        for sec in raw {
            if let Some(shelf) = sec.get("musicShelfRenderer") {
                let title = join_runs_text(shelf.pointer("/title/runs"));
                let ep = shelf.pointer("/bottomEndpoint/browseEndpoint");
                let sec_browse = ep
                    .and_then(|e| e.get("browseId"))
                    .and_then(|v| v.as_str())
                    .map(ToString::to_string);
                let sec_params = ep
                    .and_then(|e| e.get("params"))
                    .and_then(|v| v.as_str())
                    .map(ToString::to_string);
                let items = shelf
                    .get("contents")
                    .and_then(|v| v.as_array())
                    .map(|arr| parse_browse_entries(arr, SearchFilter::Songs))
                    .unwrap_or_default();
                if !items.is_empty() {
                    sections.push(ArtistSection {
                        title: if title.is_empty() {
                            "Top Songs".to_string()
                        } else {
                            title
                        },
                        browse_id: sec_browse,
                        params: sec_params,
                        items,
                    });
                }
            } else if let Some(shelf) = sec.get("musicCarouselShelfRenderer") {
                let title = join_runs_text(
                    shelf.pointer("/header/musicCarouselShelfBasicHeaderRenderer/title/runs"),
                );
                let ep = shelf.pointer("/header/musicCarouselShelfBasicHeaderRenderer/moreContentButton/buttonRenderer/navigationEndpoint/browseEndpoint");
                let sec_browse = ep
                    .and_then(|e| e.get("browseId"))
                    .and_then(|v| v.as_str())
                    .map(ToString::to_string);
                let sec_params = ep
                    .and_then(|e| e.get("params"))
                    .and_then(|v| v.as_str())
                    .map(ToString::to_string);
                let items = shelf
                    .get("contents")
                    .and_then(|v| v.as_array())
                    .map(|arr| parse_browse_entries(arr, SearchFilter::Songs))
                    .unwrap_or_default();
                if !items.is_empty() {
                    sections.push(ArtistSection {
                        title: if title.is_empty() {
                            "Discography".to_string()
                        } else {
                            title
                        },
                        browse_id: sec_browse,
                        params: sec_params,
                        items,
                    });
                }
            }
        }
    }

    ArtistPage {
        browse_id: browse_id.to_string(),
        name,
        channel_id,
        thumbnail_url,
        description,
        subscriber_count,
        monthly_listeners,
        shuffle_playlist_id,
        radio_playlist_id,
        radio_video_id,
        sections,
    }
}

fn parse_artist_items_response(val: &Value) -> (String, Vec<BrowseItem>, Option<String>) {
    let first = val
        .pointer("/contents/singleColumnBrowseResultsRenderer/tabs/0/tabRenderer/content/sectionListRenderer/contents/0");
    if let Some(grid) = first.and_then(|f| f.get("gridRenderer")) {
        let title = join_runs_text(grid.pointer("/header/gridHeaderRenderer/title/runs"));
        let contents = grid
            .get("items")
            .and_then(|v| v.as_array())
            .map(Vec::as_slice)
            .unwrap_or(&[]);
        return (
            title,
            parse_browse_entries(contents, SearchFilter::Albums),
            extract_continuation(Some(grid), Some(contents)),
        );
    }
    if let Some(shelf) = first.and_then(|f| f.get("musicPlaylistShelfRenderer")) {
        let title = join_runs_text(val.pointer("/header/musicHeaderRenderer/title/runs"));
        let contents = shelf
            .get("contents")
            .and_then(|v| v.as_array())
            .map(Vec::as_slice)
            .unwrap_or(&[]);
        return (
            title,
            parse_browse_entries(contents, SearchFilter::Songs),
            extract_continuation(Some(shelf), Some(contents)),
        );
    }
    (String::new(), Vec::new(), None)
}

fn parse_mood_button(btn: &Value) -> Option<MoodGenreButton> {
    let renderer = btn.get("musicNavigationButtonRenderer").unwrap_or(btn);
    let title = join_runs_text(renderer.pointer("/buttonText/runs"));
    let ep = renderer.pointer("/clickCommand/browseEndpoint")?;
    let browse_id = ep.get("browseId")?.as_str()?.to_string();
    let params = ep.get("params").and_then(|v| v.as_str()).unwrap_or("").to_string();
    let stripe_color = renderer
        .pointer("/solid/leftStripeColor")
        .and_then(|v| v.as_u64())
        .map(|c| c as u32);
    (!title.is_empty()).then_some(MoodGenreButton {
        title,
        browse_id,
        params,
        stripe_color,
    })
}

fn parse_explore_response(val: &Value) -> ExplorePage {
    let mut page = ExplorePage::default();
    let Some(contents) = val
        .pointer("/contents/singleColumnBrowseResultsRenderer/tabs/0/tabRenderer/content/sectionListRenderer/contents")
        .and_then(|v| v.as_array())
    else {
        return page;
    };
    for sec in contents {
        let Some(carousel) = sec.get("musicCarouselShelfRenderer") else {
            continue;
        };
        let more_id = carousel
            .pointer("/header/musicCarouselShelfBasicHeaderRenderer/moreContentButton/buttonRenderer/navigationEndpoint/browseEndpoint/browseId")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let Some(entries) = carousel.get("contents").and_then(|v| v.as_array()) else {
            continue;
        };
        if more_id == "FEmusic_new_releases_albums" {
            page.new_release_albums = parse_browse_entries(entries, SearchFilter::Albums);
        } else if more_id == "FEmusic_moods_and_genres" {
            page.mood_and_genres = entries.iter().filter_map(parse_mood_button).collect();
        }
    }
    page
}

fn parse_mood_and_genres_response(val: &Value) -> Vec<MoodGenreGroup> {
    let mut groups = Vec::new();
    let Some(contents) = val
        .pointer("/contents/singleColumnBrowseResultsRenderer/tabs/0/tabRenderer/content/sectionListRenderer/contents")
        .and_then(|v| v.as_array())
    else {
        return groups;
    };
    for sec in contents {
        if let Some(grid) = sec.get("gridRenderer") {
            let title = join_runs_text(grid.pointer("/header/gridHeaderRenderer/title/runs"));
            let items: Vec<MoodGenreButton> = grid
                .get("items")
                .and_then(|v| v.as_array())
                .map(|arr| arr.iter().filter_map(parse_mood_button).collect())
                .unwrap_or_default();
            if !items.is_empty() {
                groups.push(MoodGenreGroup { title, items });
            }
        }
    }
    groups
}

fn parse_browse_category_response(val: &Value) -> (String, Vec<HomeSection>) {
    let title = join_runs_text(val.pointer("/header/musicHeaderRenderer/title/runs"));
    let mut sections = Vec::new();
    if let Some(raw) = val
        .pointer("/contents/singleColumnBrowseResultsRenderer/tabs/0/tabRenderer/content/sectionListRenderer/contents")
        .and_then(|v| v.as_array())
    {
        for sec in raw {
            if let Some(grid) = sec.get("gridRenderer") {
                let sec_title =
                    join_runs_text(grid.pointer("/header/gridHeaderRenderer/title/runs"));
                let items = grid
                    .get("items")
                    .and_then(|v| v.as_array())
                    .map(|arr| parse_browse_entries(arr, SearchFilter::Playlists))
                    .unwrap_or_default();
                if !items.is_empty() {
                    sections.push(HomeSection {
                        title: if sec_title.is_empty() {
                            "Playlists".to_string()
                        } else {
                            sec_title
                        },
                        items,
                    });
                }
            } else if let Some(carousel) = sec.get("musicCarouselShelfRenderer") {
                let sec_title = join_runs_text(
                    carousel.pointer("/header/musicCarouselShelfBasicHeaderRenderer/title/runs"),
                );
                let items = carousel
                    .get("contents")
                    .and_then(|v| v.as_array())
                    .map(|arr| parse_browse_entries(arr, SearchFilter::Playlists))
                    .unwrap_or_default();
                if !items.is_empty() {
                    sections.push(HomeSection {
                        title: if sec_title.is_empty() {
                            "Featured".to_string()
                        } else {
                            sec_title
                        },
                        items,
                    });
                }
            }
        }
    }
    (title, sections)
}

fn parse_related_response(val: &Value) -> RelatedPage {
    let mut page = RelatedPage::default();
    let Some(contents) = val
        .pointer("/contents/sectionListRenderer/contents")
        .and_then(|v| v.as_array())
    else {
        return page;
    };
    for sec in contents {
        if let Some(shelf) = sec.get("musicCarouselShelfRenderer") {
            let Some(entries) = shelf.get("contents").and_then(|v| v.as_array()) else {
                continue;
            };
            for item in parse_browse_entries(entries, SearchFilter::Songs) {
                match item {
                    BrowseItem::Track(t) => page.tracks.push(t),
                    other @ BrowseItem::Artist { .. } => page.artists.push(other),
                    other => page.playlists.push(other),
                }
            }
        }
    }
    page
}

fn parse_account_playlists_response(val: &Value) -> Vec<AccountPlaylist> {
    let mut playlists = Vec::new();
    let items = val
        .pointer("/contents/singleColumnBrowseResultsRenderer/tabs/0/tabRenderer/content/sectionListRenderer/contents/0/gridRenderer/items")
        .and_then(|v| v.as_array());
    let Some(arr) = items else {
        return playlists;
    };
    for entry in arr {
        let Some(r) = entry.get("musicTwoRowItemRenderer") else {
            continue;
        };
        let Some(BrowseItem::Playlist {
            id,
            title,
            author,
            thumbnail_url,
        }) = parse_two_row_item(r)
        else {
            continue;
        };
        if id == "LM" {
            continue;
        }
        let song_count_text = Some(join_runs_text(r.pointer("/subtitle/runs"))).filter(|s| !s.is_empty());
        let is_editable = r
            .pointer("/menu/menuRenderer/items")
            .and_then(|v| v.as_array())
            .map(|items| {
                items.iter().any(|it| {
                    it.pointer("/menuNavigationItemRenderer/icon/iconType")
                        .and_then(|v| v.as_str())
                        == Some("DELETE")
                })
            })
            .unwrap_or(false);
        playlists.push(AccountPlaylist {
            id,
            title,
            author: Some(author),
            song_count_text,
            thumbnail_url,
            is_editable,
        });
    }
    playlists
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_album_page_with_fallback_tracks() {
        let json = json!({
            "microformat": {
                "microformatDataRenderer": {
                    "urlCanonical": "https://music.youtube.com/playlist?list=OLAK5uy_test123"
                }
            },
            "contents": {
                "twoColumnBrowseResultsRenderer": {
                    "tabs": [{
                        "tabRenderer": {
                            "content": {
                                "sectionListRenderer": {
                                    "contents": [{
                                        "musicResponsiveHeaderRenderer": {
                                            "title": { "runs": [{ "text": "Midnight Signals" }] },
                                            "straplineTextOne": {
                                                "runs": [{
                                                    "text": "Echo Band",
                                                    "navigationEndpoint": { "browseEndpoint": { "browseId": "UC_ARTIST_1" } }
                                                }]
                                            },
                                            "subtitle": { "runs": [{ "text": "Album" }, { "text": " • " }, { "text": "2024" }] },
                                            "thumbnail": { "musicThumbnailRenderer": { "thumbnail": { "thumbnails": [{ "url": "https://img/alb.jpg" }] } } }
                                        }
                                    }]
                                }
                            }
                        }
                    }],
                    "secondaryContents": {
                        "sectionListRenderer": {
                            "contents": [{
                                "musicShelfRenderer": {
                                    "contents": [{
                                        "musicResponsiveListItemRenderer": {
                                            "playlistItemData": { "videoId": "vid_001" },
                                            "flexColumns": [
                                                { "musicResponsiveListItemFlexColumnRenderer": { "text": { "runs": [{ "text": "Track One" }] } } },
                                                { "musicResponsiveListItemFlexColumnRenderer": { "text": { "runs": [{ "text": "3:45" }] } } }
                                            ]
                                        }
                                    }]
                                }
                            }]
                        }
                    }
                }
            }
        });
        let page = parse_album_response("MPREb_test", &json);
        assert_eq!(page.title, "Midnight Signals");
        assert_eq!(page.playlist_id.as_deref(), Some("OLAK5uy_test123"));
        assert_eq!(page.year, Some(2024));
        assert_eq!(page.artists.len(), 1);
        assert_eq!(page.tracks.len(), 1);
        assert_eq!(page.tracks[0].id, "vid_001");
        assert_eq!(page.tracks[0].duration_secs, Some(225));
    }

    #[test]
    fn parses_artist_page_and_explore() {
        let artist_json = json!({
            "header": {
                "musicImmersiveHeaderRenderer": {
                    "title": { "runs": [{ "text": "Kiasmos" }] },
                    "monthlyListenerCount": { "runs": [{ "text": "1.2M monthly audience" }] },
                    "subscriptionButton": {
                        "subscribeButtonRenderer": {
                            "channelId": "UC_KIASMOS",
                            "subscriberCountText": { "runs": [{ "text": "180K subscribers" }] }
                        }
                    },
                    "thumbnail": { "musicThumbnailRenderer": { "thumbnail": { "thumbnails": [{ "url": "https://img/a.jpg" }] } } }
                }
            },
            "contents": {
                "singleColumnBrowseResultsRenderer": {
                    "tabs": [{
                        "tabRenderer": {
                            "content": {
                                "sectionListRenderer": {
                                    "contents": [{
                                        "musicCarouselShelfRenderer": {
                                            "header": { "musicCarouselShelfBasicHeaderRenderer": { "title": { "runs": [{ "text": "Albums" }] } } },
                                            "contents": [{
                                                "musicTwoRowItemRenderer": {
                                                    "title": { "runs": [{ "text": "II" }] },
                                                    "subtitle": { "runs": [{ "text": "Album" }, { "text": " • " }, { "text": "2024" }] },
                                                    "thumbnailRenderer": { "musicThumbnailRenderer": { "thumbnail": { "thumbnails": [{ "url": "https://img/ii.jpg" }] } } },
                                                    "navigationEndpoint": {
                                                        "browseEndpoint": {
                                                            "browseId": "MPREb_II",
                                                            "browseEndpointContextSupportedConfigs": {
                                                                "browseEndpointContextMusicConfig": { "pageType": "MUSIC_PAGE_TYPE_ALBUM" }
                                                            }
                                                        }
                                                    }
                                                }
                                            }]
                                        }
                                    }]
                                }
                            }
                        }
                    }]
                }
            }
        });
        let page = parse_artist_response("UC_KIASMOS", &artist_json);
        assert_eq!(page.name, "Kiasmos");
        assert_eq!(page.channel_id.as_deref(), Some("UC_KIASMOS"));
        assert_eq!(page.monthly_listeners.as_deref(), Some("1.2M monthly audience"));
        assert_eq!(page.sections.len(), 1);
        assert_eq!(page.sections[0].items.len(), 1);

        let explore_json = json!({
            "contents": {
                "singleColumnBrowseResultsRenderer": {
                    "tabs": [{
                        "tabRenderer": {
                            "content": {
                                "sectionListRenderer": {
                                    "contents": [{
                                        "musicCarouselShelfRenderer": {
                                            "header": {
                                                "musicCarouselShelfBasicHeaderRenderer": {
                                                    "moreContentButton": {
                                                        "buttonRenderer": {
                                                            "navigationEndpoint": { "browseEndpoint": { "browseId": "FEmusic_moods_and_genres" } }
                                                        }
                                                    }
                                                }
                                            },
                                            "contents": [{
                                                "musicNavigationButtonRenderer": {
                                                    "buttonText": { "runs": [{ "text": "Chill" }] },
                                                    "solid": { "leftStripeColor": 4286141768u64 },
                                                    "clickCommand": {
                                                        "browseEndpoint": { "browseId": "FEmusic_moods_and_genres_category", "params": "ggChill" }
                                                    }
                                                }
                                            }]
                                        }
                                    }]
                                }
                            }
                        }
                    }]
                }
            }
        });
        let explore = parse_explore_response(&explore_json);
        assert_eq!(explore.mood_and_genres.len(), 1);
        assert_eq!(explore.mood_and_genres[0].title, "Chill");
    }

    #[test]
    fn parses_playlist_continuation_token() {
        let cont = json!({
            "continuationContents": {
                "musicPlaylistShelfContinuation": {
                    "contents": [{
                        "musicResponsiveListItemRenderer": {
                            "playlistItemData": { "videoId": "vid_cont_1" },
                            "flexColumns": [
                                { "musicResponsiveListItemFlexColumnRenderer": { "text": { "runs": [{ "text": "Next Page Song" }] } } },
                                { "musicResponsiveListItemFlexColumnRenderer": { "text": { "runs": [{ "text": "2:30" }] } } }
                            ]
                        }
                    }],
                    "continuations": [{ "nextContinuationData": { "continuation": "TOKEN_2" } }]
                }
            }
        });
        let parsed = parse_playlist_continuation_response("PL1", &cont);
        assert_eq!(parsed.tracks.len(), 1);
        assert_eq!(parsed.continuation.as_deref(), Some("TOKEN_2"));
    }
}


