use crate::models::{AlbumRef, ArtistRef, PendingScrobble, Playlist, RecognitionEntry, Track, TrackStats};
use rusqlite::{params, Connection, OptionalExtension};
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
        let artists_json = serde_json::to_string(&track.artists)?;
        let conn = self.conn.lock().expect("db mutex poisoned");
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

