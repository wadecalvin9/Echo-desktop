use crate::db::EchoDb;
use crate::settings::AppSettings;
use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use thiserror::Error;

const BACKUP_FORMAT_VERSION: u32 = 1;

#[derive(Debug, Error)]
pub enum BackupError {
    #[error("backup directory unavailable")]
    NoPath,
    #[error("i/o error: {0}")]
    Io(#[from] std::io::Error),
    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("database error: {0}")]
    Db(String),
    #[error("invalid backup file: {0}")]
    Invalid(String),
}

#[derive(Debug, Serialize, Deserialize)]
struct BackupBundle {
    format_version: u32,
    created_at_ms: i64,
    settings: AppSettings,
    db_base64: String,
}

pub fn default_backup_path() -> Result<PathBuf, BackupError> {
    if let Some(user_dirs) = directories::UserDirs::new() {
        if let Some(docs) = user_dirs.document_dir() {
            return Ok(docs.join("echo-backup.echobak"));
        }
    }
    Ok(AppSettings::app_dirs()
        .map_err(|_| BackupError::NoPath)?
        .data_dir()
        .join("echo-backup.echobak"))
}

pub fn export_backup(
    db: &EchoDb,
    settings: &AppSettings,
    dest_path: &Path,
) -> Result<usize, BackupError> {
    if let Some(parent) = dest_path.parent() {
        fs::create_dir_all(parent)?;
    }
    let tmp = std::env::temp_dir().join(format!(
        "echo-export-{}.db",
        chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0)
    ));
    let _ = fs::remove_file(&tmp);
    db.export_snapshot(&tmp)
        .map_err(|e| BackupError::Db(e.to_string()))?;
    let bytes = fs::read(&tmp)?;
    let _ = fs::remove_file(&tmp);

    let bundle = BackupBundle {
        format_version: BACKUP_FORMAT_VERSION,
        created_at_ms: chrono::Utc::now().timestamp_millis(),
        settings: settings.clone(),
        db_base64: BASE64.encode(&bytes),
    };
    let json = serde_json::to_string_pretty(&bundle)?;
    let out_len = json.len();
    fs::write(dest_path, json)?;
    Ok(out_len)
}

pub fn validate_and_unpack_backup(
    src_path: &Path,
    target_db_path: &Path,
) -> Result<AppSettings, BackupError> {
    let raw = fs::read_to_string(src_path)?;
    let bundle: BackupBundle = serde_json::from_str(&raw)?;
    if bundle.format_version == 0 || bundle.format_version > BACKUP_FORMAT_VERSION {
        return Err(BackupError::Invalid(format!(
            "unsupported backup version {}",
            bundle.format_version
        )));
    }
    let db_bytes = BASE64
        .decode(&bundle.db_base64)
        .map_err(|e| BackupError::Invalid(format!("corrupt db payload: {e}")))?;
    if db_bytes.len() < 16 || &db_bytes[..16] != b"SQLite format 3\0" {
        return Err(BackupError::Invalid(
            "payload is not a valid SQLite database".to_string(),
        ));
    }

    let tmp = std::env::temp_dir().join(format!(
        "echo-import-{}.db",
        chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0)
    ));
    fs::write(&tmp, &db_bytes)?;
    {
        let check_db = EchoDb::open(&tmp).map_err(|e| BackupError::Db(e.to_string()))?;
        let _ = check_db
            .playlists()
            .map_err(|e| BackupError::Db(e.to_string()))?;
    }

    if let Some(parent) = target_db_path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(target_db_path, &db_bytes)?;
    let _ = fs::remove_file(&tmp);

    let mut settings = bundle.settings;
    settings.clamp_startup_volume();
    settings
        .save()
        .map_err(|e| BackupError::Invalid(e.to_string()))?;
    Ok(settings)
}

