use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

fn safe_cache_name(song_id: &str) -> String {
    let cleaned: String = song_id
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect();
    format!("{cleaned}.cache")
}

pub fn lookup_cached_stream(cache_dir: &Path, song_id: &str) -> Option<PathBuf> {
    let path = cache_dir.join(safe_cache_name(song_id));
    if path.is_file() {
        // Touch access time by rewriting mtime if possible via OpenOptions write(true) without truncating.
        let _ = fs::OpenOptions::new()
            .write(true)
            .open(&path)
            .and_then(|f| f.set_modified(SystemTime::now()));
        Some(path)
    } else {
        None
    }
}

pub fn store_cached_stream(
    cache_dir: &Path,
    song_id: &str,
    bytes: &[u8],
    max_cache_mb: u64,
) -> io::Result<Option<PathBuf>> {
    if max_cache_mb == 0 || bytes.is_empty() {
        return Ok(None);
    }
    let max_bytes = max_cache_mb.saturating_mul(1024 * 1024);
    if (bytes.len() as u64) > max_bytes {
        return Ok(None);
    }
    fs::create_dir_all(cache_dir)?;
    let dest = cache_dir.join(safe_cache_name(song_id));
    fs::write(&dest, bytes)?;
    enforce_lru_limit(cache_dir, max_bytes)?;
    Ok(Some(dest))
}

pub fn enforce_lru_limit(cache_dir: &Path, max_bytes: u64) -> io::Result<()> {
    if !cache_dir.is_dir() {
        return Ok(());
    }
    let mut entries: Vec<(PathBuf, u64, SystemTime)> = Vec::new();
    let mut total: u64 = 0;
    for entry in fs::read_dir(cache_dir)? {
        let entry = match entry {
            Ok(e) => e,
            Err(_) => continue,
        };
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let Ok(meta) = entry.metadata() else {
            continue;
        };
        let len = meta.len();
        let mtime = meta.modified().unwrap_or(SystemTime::UNIX_EPOCH);
        total = total.saturating_add(len);
        entries.push((path, len, mtime));
    }
    if total <= max_bytes {
        return Ok(());
    }
    entries.sort_by_key(|(_, _, mtime)| *mtime);
    for (path, len, _) in entries {
        if total <= max_bytes {
            break;
        }
        if fs::remove_file(&path).is_ok() {
            total = total.saturating_sub(len);
        }
    }
    Ok(())
}

pub fn cache_stats(cache_dir: &Path) -> (usize, u64) {
    let Ok(rd) = fs::read_dir(cache_dir) else {
        return (0, 0);
    };
    let mut count = 0usize;
    let mut total = 0u64;
    for entry in rd.flatten() {
        if let Ok(meta) = entry.metadata() {
            if meta.is_file() {
                count += 1;
                total = total.saturating_add(meta.len());
            }
        }
    }
    (count, total)
}

pub fn clear_cache_dir(cache_dir: &Path) -> io::Result<usize> {
    if !cache_dir.exists() {
        return Ok(0);
    }
    let mut removed = 0usize;
    for entry in fs::read_dir(cache_dir)? {
        let entry = entry?;
        if entry.path().is_file() && fs::remove_file(entry.path()).is_ok() {
            removed += 1;
        }
    }
    Ok(removed)
}

