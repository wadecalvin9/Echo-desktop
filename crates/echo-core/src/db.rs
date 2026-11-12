use crate::models::{
    AlbumRef, AlbumStats, ArtistRef, ArtistStats, DailyListening, ListeningSummary, PendingScrobble,
    Playlist, RecognitionEntry, Track, TrackStats,
};
use crate::recommendations::{
    build_recommendations, EngineOutput, ListenRecord, RelatedEdge,
};
use chrono::{Datelike, Local, NaiveDate, TimeZone, Timelike};
use rusqlite::{params, Connection, OptionalExtension};
use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::sync::{Arc, Mutex};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum DbError {
    #[error("sqlite error: {0}")]
    Sql(#[from] rusqlite::Error),
    #[error("serialization error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("database I/O error: {0}")]
    Io(#[from] std::io::Error),
}

#[derive(Clone)]
pub struct EchoDb {
    conn: Arc<Mutex<Connection>>,
}

impl EchoDb {
    pub fn open(path: &Path) -> Result<Self, DbError> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let conn = Connection::open(path)?;
        let db = Self {
            conn: Arc::new(Mutex::new(conn)),
        };
        db.migrate()?;
        Ok(db)
    }

    pub fn open_in_memory() -> Result<Self, DbError> {
        let conn = Connection::open_in_memory()?;
        let db = Self {
            conn: Arc::new(Mutex::new(conn)),
        };
        db.migrate()?;
        Ok(db)
    }

    fn migrate(&self) -> Result<(), DbError> {
        let conn = self.conn.lock().expect("db mutex poisoned");
        conn.execute_batch(
            "
            PRAGMA journal_mode = WAL;
            PRAGMA foreign_keys = ON;

            CREATE TABLE IF NOT EXISTS songs (
                id TEXT PRIMARY KEY,
                title TEXT NOT NULL,
                artists_json TEXT NOT NULL,
                album_id TEXT,
                album_title TEXT,
                duration_secs INTEGER,
                thumbnail_url TEXT,
                loudness_db REAL,
                liked INTEGER NOT NULL DEFAULT 0,
                is_local INTEGER NOT NULL DEFAULT 0,
                local_path TEXT,
                updated_at_ms INTEGER NOT NULL DEFAULT 0
            );

            CREATE TABLE IF NOT EXISTS play_history (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                song_id TEXT NOT NULL,
                played_at_ms INTEGER NOT NULL,
                duration_played_secs INTEGER NOT NULL DEFAULT 0,
                FOREIGN KEY (song_id) REFERENCES songs(id) ON DELETE CASCADE
            );

            CREATE TABLE IF NOT EXISTS recognition_history (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                recognized_at_ms INTEGER NOT NULL,
                title TEXT NOT NULL,
                artist TEXT,
                artwork_url TEXT,
                isrc TEXT,
                shazam_key TEXT,
                video_id TEXT
            );

            CREATE TABLE IF NOT EXISTS playlists (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL,
                description TEXT,
                thumbnail_url TEXT,
                is_remote INTEGER NOT NULL DEFAULT 0
            );

            CREATE TABLE IF NOT EXISTS playlist_songs (
                playlist_id TEXT NOT NULL,
                song_id TEXT NOT NULL,
                position INTEGER NOT NULL,
                PRIMARY KEY (playlist_id, song_id),
                FOREIGN KEY (playlist_id) REFERENCES playlists(id) ON DELETE CASCADE,
                FOREIGN KEY (song_id) REFERENCES songs(id) ON DELETE CASCADE
            );

            CREATE TABLE IF NOT EXISTS pending_scrobbles (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                artist TEXT NOT NULL,
                track TEXT NOT NULL,
                album TEXT,
                duration_secs INTEGER,
                started_at_secs INTEGER NOT NULL
            );

            CREATE TABLE IF NOT EXISTS downloads (
                song_id TEXT PRIMARY KEY,
                file_path TEXT NOT NULL,
                mime_type TEXT,
                bytes_total INTEGER NOT NULL DEFAULT 0,
                created_at_ms INTEGER NOT NULL,
                FOREIGN KEY (song_id) REFERENCES songs(id) ON DELETE CASCADE
            );

            CREATE TABLE IF NOT EXISTS related_edges (
                seed_id TEXT NOT NULL,
                related_id TEXT NOT NULL,
                source INTEGER NOT NULL DEFAULT 0,
                updated_at_ms INTEGER NOT NULL DEFAULT 0,
                PRIMARY KEY (seed_id, related_id, source),
                FOREIGN KEY (related_id) REFERENCES songs(id) ON DELETE CASCADE
            );

            CREATE TABLE IF NOT EXISTS home_impressions (
                song_id TEXT NOT NULL,
                shown_at_ms INTEGER NOT NULL
            );
            ",
        )?;
        Ok(())
    }

    pub fn export_snapshot(&self, dest_path: &Path) -> Result<(), DbError> {
        let conn = self.conn.lock().expect("db mutex poisoned");
        let escaped = dest_path.to_string_lossy().replace('\'', "''");
        conn.execute(&format!("VACUUM INTO '{escaped}'"), [])?;
        Ok(())
    }

    pub fn upsert_track(&self, track: &Track) -> Result<(), DbError> {
        let now_ms = chrono::Utc::now().timestamp_millis();
        let conn = self.conn.lock().expect("db mutex poisoned");
        upsert_track_conn(&conn, track, now_ms)
    }

    pub fn save_related_edges(
        &self,
        seed_id: &str,
        related: &[Track],
        source: u8,
    ) -> Result<(), DbError> {
        if seed_id.trim().is_empty() || related.is_empty() {
            return Ok(());
        }
        let now_ms = chrono::Utc::now().timestamp_millis();
        let mut conn = self.conn.lock().expect("db mutex poisoned");
        let tx = conn.transaction()?;
        for t in related {
            if t.id == seed_id || t.id.trim().is_empty() {
                continue;
            }
            upsert_track_conn(&tx, t, now_ms)?;
            tx.execute(
                "INSERT OR REPLACE INTO related_edges (seed_id, related_id, source, updated_at_ms)
                 VALUES (?1, ?2, ?3, ?4)",
                params![seed_id, t.id, i64::from(source), now_ms],
            )?;
        }
        tx.commit()?;
        Ok(())
    }

    pub fn record_home_impressions(&self, song_ids: &[String]) -> Result<(), DbError> {
        if song_ids.is_empty() {
            return Ok(());
        }
        let now_ms = chrono::Utc::now().timestamp_millis();
        let cutoff = now_ms - 14 * 86_400_000;
        let mut conn = self.conn.lock().expect("db mutex poisoned");
        let tx = conn.transaction()?;
        tx.execute(
            "DELETE FROM home_impressions WHERE shown_at_ms < ?1",
            params![cutoff],
        )?;
        for id in song_ids {
            tx.execute(
                "INSERT INTO home_impressions (song_id, shown_at_ms) VALUES (?1, ?2)",
                params![id, now_ms],
            )?;
        }
        tx.commit()?;
        Ok(())
    }

    pub fn build_home_recommendations(
        &self,
        disliked_artists: &HashSet<String>,
        desired_picks: usize,
        rotation_cursor: usize,
    ) -> Result<EngineOutput, DbError> {
        let now_ms = chrono::Utc::now().timestamp_millis();
        let imp_cutoff = now_ms - 14 * 86_400_000;
        let edge_cutoff = now_ms - 7 * 86_400_000;

        let conn = self.conn.lock().expect("db mutex poisoned");

        let mut songs: HashMap<String, Track> = HashMap::new();
        {
            let mut stmt = conn.prepare(
                "SELECT id, title, artists_json, album_id, album_title, duration_secs,
                        thumbnail_url, loudness_db, liked, is_local, local_path
                 FROM songs",
            )?;
            let mut rows = stmt.query([])?;
            while let Some(r) = rows.next()? {
                let t = row_to_track(r)?;
                songs.insert(t.id.clone(), t);
            }
        }

        let mut listens = Vec::new();
        {
            let mut stmt = conn.prepare(
                "SELECT song_id, played_at_ms, duration_played_secs FROM play_history",
            )?;
            let mut rows = stmt.query([])?;
            while let Some(r) = rows.next()? {
                listens.push(ListenRecord {
                    song_id: r.get(0)?,
                    played_at_ms: r.get(1)?,
                    duration_played_secs: r.get::<_, i64>(2)?.max(0) as u64,
                });
            }
        }

        let mut edges = Vec::new();
        {
            let mut stmt = conn.prepare(
                "SELECT seed_id, related_id, source, updated_at_ms
                 FROM related_edges
                 WHERE updated_at_ms >= ?1",
            )?;
            let mut rows = stmt.query(params![edge_cutoff])?;
            while let Some(r) = rows.next()? {
                edges.push(RelatedEdge {
                    seed_id: r.get(0)?,
                    related_id: r.get(1)?,
                    source: r.get::<_, i64>(2)? as u8,
                    updated_at_ms: r.get(3)?,
                });
            }
        }

        let mut impressions: HashMap<String, usize> = HashMap::new();
        {
            let mut stmt = conn.prepare(
                "SELECT song_id, COUNT(*)
                 FROM home_impressions
                 WHERE shown_at_ms >= ?1
                 GROUP BY song_id",
            )?;
            let mut rows = stmt.query(params![imp_cutoff])?;
            while let Some(r) = rows.next()? {
                let id: String = r.get(0)?;
                let cnt: i64 = r.get(1)?;
                impressions.insert(id, cnt.max(0) as usize);
            }
        }

        Ok(build_recommendations(
            now_ms,
            &songs,
            &listens,
            &edges,
            &impressions,
            disliked_artists,
            desired_picks,
            rotation_cursor,
        ))
    }

    pub fn set_liked(&self, song_id: &str, liked: bool) -> Result<(), DbError> {
        let conn = self.conn.lock().expect("db mutex poisoned");
        conn.execute(
            "UPDATE songs SET liked = ?1 WHERE id = ?2",
            params![i64::from(liked), song_id],
        )?;
        Ok(())
    }

    pub fn update_loudness(&self, song_id: &str, loudness_db: f32) -> Result<(), DbError> {
        let conn = self.conn.lock().expect("db mutex poisoned");
        conn.execute(
            "UPDATE songs SET loudness_db = ?1 WHERE id = ?2",
            params![loudness_db as f64, song_id],
        )?;
        Ok(())
    }

    pub fn get_track(&self, song_id: &str) -> Result<Option<Track>, DbError> {
        let conn = self.conn.lock().expect("db mutex poisoned");
        let mut stmt = conn.prepare(
            "SELECT id, title, artists_json, album_id, album_title, duration_secs,
                    thumbnail_url, loudness_db, liked, is_local, local_path
             FROM songs WHERE id = ?1",
        )?;
        stmt.query_row(params![song_id], row_to_track)
            .optional()
            .map_err(Into::into)
    }

    pub fn liked_tracks(&self) -> Result<Vec<Track>, DbError> {
        let conn = self.conn.lock().expect("db mutex poisoned");
        let mut stmt = conn.prepare(
            "SELECT id, title, artists_json, album_id, album_title, duration_secs,
                    thumbnail_url, loudness_db, liked, is_local, local_path
             FROM songs WHERE liked = 1 ORDER BY updated_at_ms DESC",
        )?;
        let rows = stmt.query_map([], row_to_track)?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }

    /// Returns YouTube tracks already fetched/played that are missing `loudness_db`,
    /// prioritizing liked songs first (matching Echo's "Fix song volumes" ordering).
    pub fn tracks_missing_loudness(&self) -> Result<Vec<Track>, DbError> {
        let conn = self.conn.lock().expect("db mutex poisoned");
        let mut stmt = conn.prepare(
            "SELECT id, title, artists_json, album_id, album_title, duration_secs,
                    thumbnail_url, loudness_db, liked, is_local, local_path
             FROM songs
             WHERE is_local = 0 AND loudness_db IS NULL
             ORDER BY liked DESC, updated_at_ms DESC",
        )?;
        let rows = stmt.query_map([], row_to_track)?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }

    pub fn record_play(&self, track: &Track, duration_played_secs: u32) -> Result<(), DbError> {
        self.upsert_track(track)?;
        let now_ms = chrono::Utc::now().timestamp_millis();
        let conn = self.conn.lock().expect("db mutex poisoned");
        conn.execute(
            "INSERT INTO play_history (song_id, played_at_ms, duration_played_secs)
             VALUES (?1, ?2, ?3)",
            params![track.id, now_ms, duration_played_secs as i64],
        )?;
        Ok(())
    }

    pub fn update_latest_play_duration(
        &self,
        song_id: &str,
        duration_played_secs: u32,
    ) -> Result<(), DbError> {
        let conn = self.conn.lock().expect("db mutex poisoned");
        conn.execute(
            "UPDATE play_history
             SET duration_played_secs = ?1
             WHERE id = (
                 SELECT id FROM play_history
                 WHERE song_id = ?2
                 ORDER BY played_at_ms DESC
                 LIMIT 1
             )",
            params![duration_played_secs as i64, song_id],
        )?;
        Ok(())
    }

    pub fn recent_history(&self, limit: usize) -> Result<Vec<Track>, DbError> {
        let conn = self.conn.lock().expect("db mutex poisoned");
        let mut stmt = conn.prepare(
            "SELECT s.id, s.title, s.artists_json, s.album_id, s.album_title, s.duration_secs,
                    s.thumbnail_url, s.loudness_db, s.liked, s.is_local, s.local_path
             FROM play_history h
             JOIN songs s ON s.id = h.song_id
             GROUP BY s.id
             ORDER BY MAX(h.played_at_ms) DESC
             LIMIT ?1",
        )?;
        let rows = stmt.query_map(params![limit as i64], row_to_track)?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }

    pub fn top_stats(&self, limit: usize) -> Result<Vec<TrackStats>, DbError> {
        let conn = self.conn.lock().expect("db mutex poisoned");
        let mut stmt = conn.prepare(
            "SELECT s.id, s.title, s.artists_json, s.album_id, s.album_title, s.duration_secs,
                    s.thumbnail_url, s.loudness_db, s.liked, s.is_local, s.local_path,
                    COUNT(h.id) AS play_count,
                    COALESCE(SUM(h.duration_played_secs), 0) AS total_secs,
                    MAX(h.played_at_ms) AS last_played_ms
             FROM play_history h
             JOIN songs s ON s.id = h.song_id
             GROUP BY s.id
             ORDER BY play_count DESC, total_secs DESC
             LIMIT ?1",
        )?;
        let rows = stmt.query_map(params![limit as i64], |row| {
            let track = row_to_track(row)?;
            Ok(TrackStats {
                track,
                play_count: row.get::<_, i64>(11)? as u64,
                total_played_secs: row.get::<_, i64>(12)? as u64,
                last_played_ms: row.get::<_, i64>(13)?,
            })
        })?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }

    /// Aggregates listening analytics (totals, streak, 14-day rhythm, 24h clock, top artists/albums/tracks).
    pub fn listening_summary(
        &self,
        days_window: Option<u32>,
        top_limit: usize,
    ) -> Result<ListeningSummary, DbError> {
        let cutoff_ms = match days_window {
            Some(days) => {
                (chrono::Utc::now() - chrono::Duration::days(days as i64)).timestamp_millis()
            }
            None => i64::MIN,
        };

        let conn = self.conn.lock().expect("db mutex poisoned");
        let mut stmt = conn.prepare(
            "SELECT s.id, s.title, s.artists_json, s.album_id, s.album_title, s.duration_secs,
                    s.thumbnail_url, s.loudness_db, s.liked, s.is_local, s.local_path,
                    h.played_at_ms, h.duration_played_secs
             FROM play_history h
             JOIN songs s ON s.id = h.song_id
             WHERE h.played_at_ms >= ?1
             ORDER BY h.played_at_ms DESC",
        )?;

        struct TrackAcc {
            track: Track,
            play_count: u64,
            total_played_secs: u64,
            last_played_ms: i64,
        }
        struct ArtistAcc {
            id: Option<String>,
            name: String,
            play_count: u64,
            total_played_secs: u64,
            track_ids: HashSet<String>,
            thumbnail_url: Option<String>,
        }
        struct AlbumAcc {
            id: Option<String>,
            title: String,
            artist: String,
            play_count: u64,
            total_played_secs: u64,
            thumbnail_url: Option<String>,
        }

        let mut summary = ListeningSummary::default();
        let mut tracks: HashMap<String, TrackAcc> = HashMap::new();
        let mut artists: HashMap<String, ArtistAcc> = HashMap::new();
        let mut albums: HashMap<String, AlbumAcc> = HashMap::new();
        let mut daily_map: HashMap<NaiveDate, (u64, u64)> = HashMap::new();
        let mut active_dates: HashSet<NaiveDate> = HashSet::new();

        let mut rows = stmt.query(params![cutoff_ms])?;
        while let Some(row) = rows.next()? {
            let track = row_to_track(row)?;
            let played_at_ms: i64 = row.get(11)?;
            let dur_secs: u64 = row.get::<_, i64>(12)?.max(0) as u64;

            summary.total_plays += 1;
            summary.total_played_secs += dur_secs;
            if track.is_local {
                summary.local_plays += 1;
            } else {
                summary.online_plays += 1;
            }

            if let Some(local_dt) = Local.timestamp_millis_opt(played_at_ms).single() {
                let hour = local_dt.hour() as usize;
                if hour < 24 {
                    summary.hourly_plays[hour] += 1;
                }
                let date = local_dt.date_naive();
                active_dates.insert(date);
                let day_entry = daily_map.entry(date).or_insert((0, 0));
                day_entry.0 += 1;
                day_entry.1 += dur_secs;
            }

            let t_entry = tracks.entry(track.id.clone()).or_insert_with(|| TrackAcc {
                track: track.clone(),
                play_count: 0,
                total_played_secs: 0,
                last_played_ms: played_at_ms,
            });
            t_entry.play_count += 1;
            t_entry.total_played_secs += dur_secs;
            t_entry.last_played_ms = t_entry.last_played_ms.max(played_at_ms);

            for artist in &track.artists {
                let name = artist.name.trim();
                if name.is_empty() {
                    continue;
                }
                let key = name.to_lowercase();
                let a_entry = artists.entry(key).or_insert_with(|| ArtistAcc {
                    id: artist.id.clone(),
                    name: name.to_string(),
                    play_count: 0,
                    total_played_secs: 0,
                    track_ids: HashSet::new(),
                    thumbnail_url: track.thumbnail_url.clone(),
                });
                a_entry.play_count += 1;
                a_entry.total_played_secs += dur_secs;
                a_entry.track_ids.insert(track.id.clone());
                if a_entry.id.is_none() {
                    a_entry.id = artist.id.clone();
                }
                if a_entry.thumbnail_url.is_none() {
                    a_entry.thumbnail_url = track.thumbnail_url.clone();
                }
            }

            if let Some(alb) = &track.album {
                let title = alb.title.trim();
                if !title.is_empty() {
                    let key = if let Some(id) = alb.id.as_deref().filter(|s| !s.is_empty()) {
                        id.to_string()
                    } else {
                        format!("name::{}", title.to_lowercase())
                    };
                    let al_entry = albums.entry(key).or_insert_with(|| AlbumAcc {
                        id: alb.id.clone(),
                        title: title.to_string(),
                        artist: track.artist_display(),
                        play_count: 0,
                        total_played_secs: 0,
                        thumbnail_url: track.thumbnail_url.clone(),
                    });
                    al_entry.play_count += 1;
                    al_entry.total_played_secs += dur_secs;
                    if al_entry.thumbnail_url.is_none() {
                        al_entry.thumbnail_url = track.thumbnail_url.clone();
                    }
                }
            }
        }

        summary.unique_tracks = tracks.len() as u64;
        summary.unique_artists = artists.len() as u64;
        summary.active_days = active_dates.len() as u64;
        summary.peak_hour = summary
            .hourly_plays
            .iter()
            .enumerate()
            .filter(|(_, &c)| c > 0)
            .max_by_key(|(h, &c)| (c, std::cmp::Reverse(*h)))
            .map(|(h, _)| h as u8);

        let today = Local::now().date_naive();
        let mut cursor = if active_dates.contains(&today) {
            Some(today)
        } else {
            today
                .pred_opt()
                .filter(|yesterday| active_dates.contains(yesterday))
        };
        let mut streak = 0u32;
        while let Some(d) = cursor {
            if active_dates.contains(&d) {
                streak += 1;
                cursor = d.pred_opt();
            } else {
                break;
            }
        }
        summary.current_streak_days = streak;

        summary.daily_activity = (0..14)
            .rev()
            .map(|offset| {
                let d = today - chrono::Duration::days(offset);
                let (plays, total_secs) = daily_map.get(&d).copied().unwrap_or((0, 0));
                let day_short = match d.weekday() {
                    chrono::Weekday::Mon => "M",
                    chrono::Weekday::Tue => "T",
                    chrono::Weekday::Wed => "W",
                    chrono::Weekday::Thu => "T",
                    chrono::Weekday::Fri => "F",
                    chrono::Weekday::Sat => "S",
                    chrono::Weekday::Sun => "S",
                }
                .to_string();
                DailyListening {
                    day_short,
                    date_short: d.format("%b %d").to_string(),
                    plays,
                    total_secs,
                }
            })
            .collect();

        let mut top_tracks: Vec<TrackStats> = tracks
            .into_values()
            .map(|t| TrackStats {
                track: t.track,
                play_count: t.play_count,
                total_played_secs: t.total_played_secs,
                last_played_ms: t.last_played_ms,
            })
            .collect();
        top_tracks.sort_by(|a, b| {
            b.play_count
                .cmp(&a.play_count)
                .then_with(|| b.total_played_secs.cmp(&a.total_played_secs))
                .then_with(|| b.last_played_ms.cmp(&a.last_played_ms))
        });
        top_tracks.truncate(top_limit);
        summary.top_tracks = top_tracks;

        let mut top_artists: Vec<ArtistStats> = artists
            .into_values()
            .map(|a| ArtistStats {
                id: a.id,
                name: a.name,
                play_count: a.play_count,
                total_played_secs: a.total_played_secs,
                track_count: a.track_ids.len() as u64,
                thumbnail_url: a.thumbnail_url,
            })
            .collect();
        top_artists.sort_by(|a, b| {
            b.play_count
                .cmp(&a.play_count)
                .then_with(|| b.total_played_secs.cmp(&a.total_played_secs))
        });
        top_artists.truncate(8);
        summary.top_artists = top_artists;

        let mut top_albums: Vec<AlbumStats> = albums
            .into_values()
            .map(|al| AlbumStats {
                id: al.id,
                title: al.title,
                artist: al.artist,
                play_count: al.play_count,
                total_played_secs: al.total_played_secs,
                thumbnail_url: al.thumbnail_url,
            })
            .collect();
        top_albums.sort_by(|a, b| {
            b.play_count
                .cmp(&a.play_count)
                .then_with(|| b.total_played_secs.cmp(&a.total_played_secs))
        });
        top_albums.truncate(6);
        summary.top_albums = top_albums;

        Ok(summary)
    }

    pub fn add_recognition(
        &self,
        title: &str,
        artist: Option<&str>,
        artwork_url: Option<&str>,
        isrc: Option<&str>,
        shazam_key: Option<&str>,
        video_id: Option<&str>,
    ) -> Result<i64, DbError> {
        let now_ms = chrono::Utc::now().timestamp_millis();
        let conn = self.conn.lock().expect("db mutex poisoned");
        conn.execute(
            "INSERT INTO recognition_history
             (recognized_at_ms, title, artist, artwork_url, isrc, shazam_key, video_id)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![now_ms, title, artist, artwork_url, isrc, shazam_key, video_id],
        )?;
        Ok(conn.last_insert_rowid())
    }

    pub fn update_recognition_video_id(&self, id: i64, video_id: &str) -> Result<(), DbError> {
        let conn = self.conn.lock().expect("db mutex poisoned");
        conn.execute(
            "UPDATE recognition_history SET video_id = ?1 WHERE id = ?2",
            params![video_id, id],
        )?;
        Ok(())
    }

    pub fn recognition_history(&self, limit: usize) -> Result<Vec<RecognitionEntry>, DbError> {
        let conn = self.conn.lock().expect("db mutex poisoned");
        let mut stmt = conn.prepare(
            "SELECT id, recognized_at_ms, title, artist, artwork_url, isrc, shazam_key, video_id
             FROM recognition_history
             ORDER BY recognized_at_ms DESC
             LIMIT ?1",
        )?;
        let rows = stmt.query_map(params![limit as i64], |row| {
            Ok(RecognitionEntry {
                id: row.get(0)?,
                recognized_at_ms: row.get(1)?,
                title: row.get(2)?,
                artist: row.get(3)?,
                artwork_url: row.get(4)?,
                isrc: row.get(5)?,
                shazam_key: row.get(6)?,
                video_id: row.get(7)?,
            })
        })?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }

    pub fn create_playlist(&self, id: &str, name: &str) -> Result<(), DbError> {
        let conn = self.conn.lock().expect("db mutex poisoned");
        conn.execute(
            "INSERT OR IGNORE INTO playlists (id, name, is_remote) VALUES (?1, ?2, 0)",
            params![id, name],
        )?;
        Ok(())
    }

    pub fn add_to_playlist(&self, playlist_id: &str, track: &Track) -> Result<(), DbError> {
        self.upsert_track(track)?;
        let conn = self.conn.lock().expect("db mutex poisoned");
        let next_pos: i64 = conn.query_row(
            "SELECT COALESCE(MAX(position) + 1, 0) FROM playlist_songs WHERE playlist_id = ?1",
            params![playlist_id],
            |r| r.get(0),
        )?;
        conn.execute(
            "INSERT OR IGNORE INTO playlist_songs (playlist_id, song_id, position)
             VALUES (?1, ?2, ?3)",
            params![playlist_id, track.id, next_pos],
        )?;
        Ok(())
    }

    pub fn remove_from_playlist(&self, playlist_id: &str, song_id: &str) -> Result<(), DbError> {
        let conn = self.conn.lock().expect("db mutex poisoned");
        conn.execute(
            "DELETE FROM playlist_songs WHERE playlist_id = ?1 AND song_id = ?2",
            params![playlist_id, song_id],
        )?;
        Ok(())
    }

    pub fn delete_playlist(&self, playlist_id: &str) -> Result<(), DbError> {
        let conn = self.conn.lock().expect("db mutex poisoned");
        conn.execute("DELETE FROM playlists WHERE id = ?1", params![playlist_id])?;
        Ok(())
    }

    pub fn playlists(&self) -> Result<Vec<Playlist>, DbError> {
        let conn = self.conn.lock().expect("db mutex poisoned");
        let mut stmt = conn.prepare(
            "SELECT p.id, p.name, p.description, p.thumbnail_url, p.is_remote,
                    COUNT(ps.song_id) AS cnt
             FROM playlists p
             LEFT JOIN playlist_songs ps ON ps.playlist_id = p.id
             GROUP BY p.id
             ORDER BY p.name ASC",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok(Playlist {
                id: row.get(0)?,
                name: row.get(1)?,
                description: row.get(2)?,
                thumbnail_url: row.get(3)?,
                is_remote: row.get::<_, i64>(4)? != 0,
                track_count: row.get::<_, i64>(5)? as usize,
            })
        })?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }

    pub fn playlist_tracks(&self, playlist_id: &str) -> Result<Vec<Track>, DbError> {
        let conn = self.conn.lock().expect("db mutex poisoned");
        let mut stmt = conn.prepare(
            "SELECT s.id, s.title, s.artists_json, s.album_id, s.album_title, s.duration_secs,
                    s.thumbnail_url, s.loudness_db, s.liked, s.is_local, s.local_path
             FROM playlist_songs ps
             JOIN songs s ON s.id = ps.song_id
             WHERE ps.playlist_id = ?1
             ORDER BY ps.position ASC",
        )?;
        let rows = stmt.query_map(params![playlist_id], row_to_track)?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }

    pub fn enqueue_pending_scrobble(
        &self,
        artist: &str,
        track: &str,
        album: Option<&str>,
        duration_secs: Option<u32>,
        started_at_secs: i64,
    ) -> Result<(), DbError> {
        let conn = self.conn.lock().expect("db mutex poisoned");
        conn.execute(
            "INSERT INTO pending_scrobbles (artist, track, album, duration_secs, started_at_secs)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![
                artist,
                track,
                album,
                duration_secs.map(|d| d as i64),
                started_at_secs
            ],
        )?;
        Ok(())
    }

    pub fn pending_scrobbles(&self) -> Result<Vec<PendingScrobble>, DbError> {
        let conn = self.conn.lock().expect("db mutex poisoned");
        let mut stmt = conn.prepare(
            "SELECT id, artist, track, album, duration_secs, started_at_secs
             FROM pending_scrobbles ORDER BY started_at_secs ASC",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok(PendingScrobble {
                id: row.get(0)?,
                artist: row.get(1)?,
                track: row.get(2)?,
                album: row.get(3)?,
                duration_secs: row.get::<_, Option<i64>>(4)?.map(|v| v as u32),
                started_at_secs: row.get(5)?,
            })
        })?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }

    pub fn delete_pending_scrobble(&self, id: i64) -> Result<(), DbError> {
        let conn = self.conn.lock().expect("db mutex poisoned");
        conn.execute("DELETE FROM pending_scrobbles WHERE id = ?1", params![id])?;
        Ok(())
    }

    pub fn upsert_download(
        &self,
        track: &Track,
        file_path: &str,
        mime_type: Option<&str>,
        bytes_total: u64,
    ) -> Result<(), DbError> {
        self.upsert_track(track)?;
        let now_ms = chrono::Utc::now().timestamp_millis();
        let conn = self.conn.lock().expect("db mutex poisoned");
        conn.execute(
            "INSERT INTO downloads (song_id, file_path, mime_type, bytes_total, created_at_ms)
             VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT(song_id) DO UPDATE SET
                file_path = excluded.file_path,
                mime_type = excluded.mime_type,
                bytes_total = excluded.bytes_total,
                created_at_ms = excluded.created_at_ms",
            params![track.id, file_path, mime_type, bytes_total as i64, now_ms],
        )?;
        Ok(())
    }

    pub fn get_download_path(&self, song_id: &str) -> Option<std::path::PathBuf> {
        let conn = self.conn.lock().ok()?;
        let path_str: String = conn
            .query_row(
                "SELECT file_path FROM downloads WHERE song_id = ?1",
                params![song_id],
                |row| row.get(0),
            )
            .ok()?;
        let pb = std::path::PathBuf::from(path_str);
        pb.is_file().then_some(pb)
    }

    pub fn downloaded_tracks(&self) -> Result<Vec<(Track, std::path::PathBuf, u64)>, DbError> {
        let conn = self.conn.lock().expect("db mutex poisoned");
        let mut stmt = conn.prepare(
            "SELECT s.id, s.title, s.artists_json, s.album_id, s.album_title, s.duration_secs,
                    s.thumbnail_url, s.loudness_db, s.liked, s.is_local, s.local_path,
                    d.file_path, d.bytes_total
             FROM downloads d
             JOIN songs s ON s.id = d.song_id
             ORDER BY d.created_at_ms DESC",
        )?;
        let rows = stmt.query_map([], |row| {
            let track = row_to_track(row)?;
            let path: String = row.get(11)?;
            let bytes: i64 = row.get(12)?;
            Ok((track, std::path::PathBuf::from(path), bytes.max(0) as u64))
        })?;
        let mut out = Vec::new();
        for item in rows {
            let (t, pb, b) = item?;
            if pb.is_file() {
                out.push((t, pb, b));
            }
        }
        Ok(out)
    }

    pub fn remove_download(&self, song_id: &str) -> Result<(), DbError> {
        let path = self.get_download_path(song_id);
        let conn = self.conn.lock().expect("db mutex poisoned");
        conn.execute("DELETE FROM downloads WHERE song_id = ?1", params![song_id])?;
        drop(conn);
        if let Some(p) = path {
            let _ = std::fs::remove_file(p);
        }
        Ok(())
    }

    pub fn clear_downloads(&self) -> Result<usize, DbError> {
        let existing = self.downloaded_tracks()?;
        let count = existing.len();
        let conn = self.conn.lock().expect("db mutex poisoned");
        conn.execute("DELETE FROM downloads", [])?;
        drop(conn);
        for (_, p, _) in existing {
            let _ = std::fs::remove_file(p);
        }
        Ok(count)
    }

    pub fn local_tracks(&self) -> Result<Vec<Track>, DbError> {
        let conn = self.conn.lock().expect("db mutex poisoned");
        let mut stmt = conn.prepare(
            "SELECT id, title, artists_json, album_id, album_title, duration_secs,
                    thumbnail_url, loudness_db, liked, is_local, local_path
             FROM songs WHERE is_local = 1 ORDER BY updated_at_ms DESC",
        )?;
        let rows = stmt.query_map([], row_to_track)?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }

    pub fn replace_local_tracks(&self, scanned: &[Track]) -> Result<usize, DbError> {
        {
            let conn = self.conn.lock().expect("db mutex poisoned");
            conn.execute("DELETE FROM songs WHERE is_local = 1", [])?;
        }
        for t in scanned {
            self.upsert_track(t)?;
        }
        Ok(scanned.len())
    }
}

fn row_to_track(row: &rusqlite::Row<'_>) -> rusqlite::Result<Track> {
    let artists_json: String = row.get(2)?;
    let artists: Vec<ArtistRef> = serde_json::from_str(&artists_json).unwrap_or_default();
    let album_id: Option<String> = row.get(3)?;
    let album_title: Option<String> = row.get(4)?;
    let album = album_title.map(|title| AlbumRef { id: album_id, title });
    Ok(Track {
        id: row.get(0)?,
        title: row.get(1)?,
        artists,
        album,
        duration_secs: row.get::<_, Option<i64>>(5)?.map(|v| v as u32),
        thumbnail_url: row.get(6)?,
        loudness_db: row.get::<_, Option<f64>>(7)?.map(|v| v as f32),
        liked: row.get::<_, i64>(8)? != 0,
        is_local: row.get::<_, i64>(9)? != 0,
        local_path: row.get(10)?,
    })
}

fn upsert_track_conn(conn: &Connection, track: &Track, now_ms: i64) -> Result<(), DbError> {
    let artists_json = serde_json::to_string(&track.artists)?;
    conn.execute(
        "
        INSERT INTO songs (
            id, title, artists_json, album_id, album_title, duration_secs,
            thumbnail_url, loudness_db, liked, is_local, local_path, updated_at_ms
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)
        ON CONFLICT(id) DO UPDATE SET
            title = excluded.title,
            artists_json = excluded.artists_json,
            album_id = COALESCE(excluded.album_id, songs.album_id),
            album_title = COALESCE(excluded.album_title, songs.album_title),
            duration_secs = COALESCE(excluded.duration_secs, songs.duration_secs),
            thumbnail_url = COALESCE(excluded.thumbnail_url, songs.thumbnail_url),
            loudness_db = COALESCE(excluded.loudness_db, songs.loudness_db),
            is_local = excluded.is_local,
            local_path = COALESCE(excluded.local_path, songs.local_path),
            updated_at_ms = excluded.updated_at_ms
        ",
        params![
            track.id,
            track.title,
            artists_json,
            track.album.as_ref().and_then(|a| a.id.clone()),
            track.album.as_ref().map(|a| a.title.clone()),
            track.duration_secs.map(|d| d as i64),
            track.thumbnail_url,
            track.loudness_db.map(|v| v as f64),
            i64::from(track.liked),
            i64::from(track.is_local),
            track.local_path,
            now_ms,
        ],
    )?;
    Ok(())
}

