pub mod adaptive_queue;
pub mod backup;
pub mod cache;
pub mod db;
pub mod downloads;
pub mod local_media;
pub mod models;
pub mod recommendations;
pub mod settings;
pub mod theme;

pub use adaptive_queue::{
    extract_title_tags, prune_queue_tail, score_artist_name, score_candidate, ListenerContext,
    SongTag,
};
pub use recommendations::{
    base_version_key, build_recommendations, is_preview_title, EngineCard, EngineLane,
    EngineOutput, ListenRecord, RelatedEdge,
};
pub use backup::{default_backup_path, export_backup, validate_and_unpack_backup, BackupError};
pub use cache::{
    cache_stats, clear_cache_dir, enforce_lru_limit, lookup_cached_stream, store_cached_stream,
};
pub use db::{DbError, EchoDb};
pub use downloads::{
    download_file_path, extension_for_mime, extract_youtube_id_from_filename,
    migrate_download_file, sanitize_filename, scan_download_folders,
};
pub use local_media::{
    group_tracks_by_folder, parse_r128_gain_db, parse_replaygain_db, scan_local_folders,
    LocalMediaScanConfig,
};
pub use models::{
    AlbumRef, AlbumStats, ArtistRef, ArtistStats, AudioQuality, DailyListening, ListeningSummary,
    PendingScrobble, Playlist, QueueItem, QueueState, RecognitionEntry, RepeatMode, Track,
    TrackStats,
};
pub use settings::{
    AppSettings, EqPreset, RecognitionSource, ScannerSensitivity, SettingsError, SpatialMode,
    ThemeMode,
};
pub use theme::{DynamicPalette, RgbColor, ACCENT};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn startup_volume_clamps_above_unity() {
        let mut s = AppSettings {
            volume: 2.5,
            ..Default::default()
        };
        s.clamp_startup_volume();
        assert_eq!(s.volume, 1.0);

        let mut quiet = AppSettings {
            volume: 0.45,
            ..Default::default()
        };
        quiet.clamp_startup_volume();
        assert!((quiet.volume - 0.45).abs() < f32::EPSILON);
    }

    #[test]
    fn sqlite_roundtrip_tracks_history_and_recognition() {
        let db = EchoDb::open_in_memory().unwrap();
        let t = Track {
            id: "dQw4w9WgXcQ".to_string(),
            title: "Never Gonna Give You Up".to_string(),
            artists: vec![ArtistRef {
                id: Some("UC123".to_string()),
                name: "Rick Astley".to_string(),
            }],
            album: Some(AlbumRef {
                id: Some("MPRE1".to_string()),
                title: "Whenever You Need Somebody".to_string(),
            }),
            duration_secs: Some(213),
            thumbnail_url: None,
            loudness_db: None,
            liked: true,
            is_local: false,
            local_path: None,
        };

        db.record_play(&t, 180).unwrap();
        db.record_play(&t, 200).unwrap();
        assert_eq!(db.recent_history(10).unwrap().len(), 1);
        assert_eq!(db.tracks_missing_loudness().unwrap().len(), 1);
        db.update_loudness(&t.id, -5.2).unwrap();
        assert_eq!(db.tracks_missing_loudness().unwrap().len(), 0);

        let stats = db.top_stats(10).unwrap();
        assert_eq!(stats.len(), 1);
        assert_eq!(stats[0].play_count, 2);
        assert_eq!(stats[0].track.loudness_db, Some(-5.2));

        db.add_recognition("Song", Some("Artist"), None, Some("US123"), Some("k1"), Some("v1"))
            .unwrap();
        let recs = db.recognition_history(10).unwrap();
        assert_eq!(recs.len(), 1);
        assert_eq!(recs[0].video_id.as_deref(), Some("v1"));

        let summary = db.listening_summary(None, 20).unwrap();
        assert_eq!(summary.total_plays, 2);
        assert_eq!(summary.total_played_secs, 380);
        assert_eq!(summary.unique_tracks, 1);
        assert_eq!(summary.unique_artists, 1);
        assert_eq!(summary.current_streak_days, 1);
        assert_eq!(summary.top_artists[0].name, "Rick Astley");
        assert_eq!(summary.top_albums[0].title, "Whenever You Need Somebody");
        assert_eq!(summary.daily_activity.len(), 14);

        let mut taste_ctx = ListenerContext::default();
        taste_ctx.seed_from_taste(
            std::slice::from_ref(&t),
            &summary.top_artists,
            std::slice::from_ref(&t),
        );
        assert!(taste_ctx.known);
        assert!(score_candidate(&t, &taste_ctx) > 0.0);
        assert!(!taste_ctx.is_disliked_artist("Rick Astley"));
    }

    #[test]
    fn queue_advances_and_repeats() {
        let mut q = QueueState::default();
        let make = |id: &str| Track {
            id: id.to_string(),
            title: id.to_string(),
            artists: vec![],
            album: None,
            duration_secs: Some(100),
            thumbnail_url: None,
            loudness_db: None,
            liked: false,
            is_local: false,
            local_path: None,
        };
        q.replace_with(vec![make("a"), make("b")], 0);
        assert_eq!(q.current().unwrap().track.id, "a");
        assert_eq!(q.advance_next().unwrap().track.id, "b");
        assert!(q.advance_next().is_none());

        q.repeat_mode = RepeatMode::All;
        assert_eq!(q.advance_next().unwrap().track.id, "a");

        q.repeat_mode = RepeatMode::Off;
        q.shuffle = true;
        let next_shuffled = q.advance_next().unwrap().track.id.clone();
        assert_eq!(next_shuffled, "b");
    }

    #[test]
    fn playlist_crud_add_remove_delete() {
        let db = EchoDb::open_in_memory().unwrap();
        let t = Track {
            id: "v1".to_string(),
            title: "Song 1".to_string(),
            artists: vec![],
            album: None,
            duration_secs: Some(120),
            thumbnail_url: None,
            loudness_db: None,
            liked: false,
            is_local: false,
            local_path: None,
        };
        db.create_playlist("pl1", "Chill").unwrap();
        db.add_to_playlist("pl1", &t).unwrap();
        assert_eq!(db.playlist_tracks("pl1").unwrap().len(), 1);
        db.remove_from_playlist("pl1", "v1").unwrap();
        assert_eq!(db.playlist_tracks("pl1").unwrap().len(), 0);
        db.delete_playlist("pl1").unwrap();
        assert_eq!(db.playlists().unwrap().len(), 0);
    }

    #[test]
    fn adaptive_queue_preserves_locked_head_and_prunes_skipped_artist() {
        let mk = |id: &str, title: &str, artist: &str| Track {
            id: id.into(),
            title: title.into(),
            artists: vec![ArtistRef {
                id: None,
                name: artist.into(),
            }],
            album: None,
            duration_secs: None,
            thumbnail_url: None,
            loudness_db: None,
            liked: false,
            is_local: false,
            local_path: None,
        };
        let mut q = QueueState::default();
        let tracks = vec![
            mk("0", "Now", "A"),
            mk("1", "Lock1", "Skipped"),
            mk("2", "Lock2", "B"),
            mk("3", "Lock3", "C"),
            mk("4", "Lock4", "D"),
            mk("5", "DropMe", "Skipped"),
            mk("6", "KeepMe", "E"),
            mk("7", "DropEdit (Slowed + Reverb)", "F"),
        ];
        q.replace_with(tracks, 0);

        let mut ctx = ListenerContext::default();
        ctx.record_play(&q.items[0].track, false);
        ctx.record_early_skip(&mk("s1", "Bad", "Skipped"));

        let dropped = prune_queue_tail(&mut q.items, q.current_index, &ctx, false);
        assert_eq!(dropped, 1);
        assert_eq!(q.items[1].track.id, "1");
        assert!(q.items.iter().any(|it| it.track.id == "6"));
    }

    #[test]
    fn backup_export_and_unpack_roundtrips_sqlite() {
        let dir = std::env::temp_dir().join(format!(
            "echo-bak-test-{}",
            chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0)
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let src_db_path = dir.join("src.db");
        let dst_db_path = dir.join("dst.db");
        let bak_path = dir.join("test.echobak");

        let db = EchoDb::open(&src_db_path).unwrap();
        db.create_playlist("p1", "Saved Mix").unwrap();
        let settings = AppSettings::default();

        let bytes = export_backup(&db, &settings, &bak_path).unwrap();
        assert!(bytes > 100);
        drop(db);

        let restored_settings = validate_and_unpack_backup(&bak_path, &dst_db_path).unwrap();
        assert_eq!(restored_settings.youtube_gl, settings.youtube_gl);
        let db2 = EchoDb::open(&dst_db_path).unwrap();
        assert_eq!(db2.playlists().unwrap()[0].name, "Saved Mix");
        drop(db2);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn youtube_id_extraction_and_disk_cache_lru_work() {
        let dir = std::env::temp_dir().join(format!(
            "echo-cache-test-{}",
            chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0)
        ));
        std::fs::create_dir_all(&dir).unwrap();

        let p = dir.join("My Song [dQw4w9WgXcQ].m4a");
        std::fs::write(&p, b"audio").unwrap();
        assert_eq!(
            extract_youtube_id_from_filename(&p).as_deref(),
            Some("dQw4w9WgXcQ")
        );
        let ignored_dir = dir.join("hidden");
        std::fs::create_dir_all(&ignored_dir).unwrap();
        std::fs::write(ignored_dir.join(".nomedia"), b"").unwrap();
        std::fs::write(ignored_dir.join("Skip [AAAAAAAAAAA].m4a"), b"x").unwrap();

        let scanned = scan_download_folders(&[dir.clone()]);
        assert_eq!(scanned.len(), 1);
        assert_eq!(scanned[0].0, "dQw4w9WgXcQ");

        // LRU eviction keeps newest within max limit
        let cache_dir = dir.join("cache");
        let chunk = vec![7u8; 700 * 1024];
        store_cached_stream(&cache_dir, "song1", &chunk, 1).unwrap();
        std::thread::sleep(std::time::Duration::from_millis(20));
        store_cached_stream(&cache_dir, "song2", &chunk, 1).unwrap();
        let (count, total) = cache_stats(&cache_dir);
        assert_eq!(count, 1);
        assert!(total <= 1024 * 1024);
        assert!(lookup_cached_stream(&cache_dir, "song2").is_some());

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn replaygain_and_r128_parsers_and_local_scan_respect_nomedia() {
        assert!((parse_replaygain_db("-6.5 dB").unwrap() - 2.5).abs() < 0.01);
        assert!((parse_r128_gain_db("-256").unwrap() - (-8.0)).abs() < 0.01);

        let dir = std::env::temp_dir().join(format!(
            "echo-local-test-{}",
            chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0)
        ));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("Artist - Title.mp3"), b"not real mp3").unwrap();
        let skipped = dir.join("Secret");
        std::fs::create_dir_all(&skipped).unwrap();
        std::fs::write(skipped.join(".nomedia"), b"").unwrap();
        std::fs::write(skipped.join("Hidden.mp3"), b"hidden").unwrap();

        let tracks = scan_local_folders(&LocalMediaScanConfig {
            include_folders: vec![dir.clone()],
            excluded_folders: Vec::new(),
            sensitivity: ScannerSensitivity::MatchTitleAndArtists,
            strict_file_names: false,
            cover_cache_dir: None,
        });
        assert_eq!(tracks.len(), 1);
        assert_eq!(tracks[0].title, "Title");
        assert_eq!(tracks[0].primary_artist(), "Artist");

        let _ = std::fs::remove_dir_all(&dir);
    }
}

