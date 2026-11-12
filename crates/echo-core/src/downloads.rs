use regex::Regex;
use std::collections::HashSet;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

static YT_ID_RE: OnceLock<Regex> = OnceLock::new();

/// Extracts an embedded `[youtube_id]` from filenames such as `Midnight City [dX3k_QDnzHE].m4a`,
/// matching Echo's external download folder specification (`Downloading-Songs.md`).
pub fn extract_youtube_id_from_filename(path: &Path) -> Option<String> {
    let stem = path.file_stem()?.to_str()?;
    let re = YT_ID_RE
        .get_or_init(|| Regex::new(r"\[([A-Za-z0-9_-]{11})\]").expect("valid youtube id regex"));
    re.captures(stem).map(|c| c[1].to_string())
}

pub fn sanitize_filename(name: &str) -> String {
    let cleaned: String = name
        .chars()
        .map(|c| match c {
            '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*' => '_',
            c if c.is_control() => '_',
            c => c,
        })
        .collect();
    let trimmed = cleaned.trim().trim_end_matches('.');
    if trimmed.is_empty() {
        "track".to_string()
    } else {
        trimmed.chars().take(120).collect()
    }
}

pub fn extension_for_mime(mime: &str) -> &'static str {
    let m = mime.to_lowercase();
    if m.contains("webm") || m.contains("opus") {
        "opus"
    } else if m.contains("ogg") {
        "ogg"
    } else if m.contains("flac") {
        "flac"
    } else if m.contains("mpeg") || m.contains("mp3") {
        "mp3"
    } else {
        "m4a"
    }
}

pub fn download_file_path(dir: &Path, title: &str, video_id: &str, mime_type: &str) -> PathBuf {
    let ext = extension_for_mime(mime_type);
    dir.join(format!("{} [{}].{}", sanitize_filename(title), video_id, ext))
}

/// Recursively scans download and extra import folders for files named `... [video_id].ext`.
pub fn scan_download_folders(folders: &[PathBuf]) -> Vec<(String, PathBuf)> {
    let mut found = Vec::new();
    let mut seen_ids = HashSet::new();
    for root in folders {
        if !root.is_dir() {
            continue;
        }
        walk_download_dir(root, &mut seen_ids, &mut found);
    }
    found
}

fn walk_download_dir(dir: &Path, seen: &mut HashSet<String>, out: &mut Vec<(String, PathBuf)>) {
    if dir.join(".nomedia").exists() {
        return;
    }
    let Ok(rd) = fs::read_dir(dir) else {
        return;
    };
    for entry in rd.flatten() {
        let name = entry.file_name();
        let name_str = name.to_string_lossy();
        if name_str.starts_with('.') {
            continue;
        }
        let path = entry.path();
        if path.is_dir() {
            walk_download_dir(&path, seen, out);
        } else if path.is_file() {
            let is_audio = path
                .extension()
                .and_then(|e| e.to_str())
                .map(|ext| {
                    matches!(
                        ext.to_ascii_lowercase().as_str(),
                        "m4a" | "mp4" | "mp3" | "opus" | "ogg" | "flac" | "wav" | "aac" | "webm"
                    )
                })
                .unwrap_or(false);
            if is_audio {
                if let Some(vid) = extract_youtube_id_from_filename(&path) {
                    if seen.insert(vid.clone()) {
                        out.push((vid, path));
                    }
                }
            }
        }
    }
}

pub fn migrate_download_file(src: &Path, dest_dir: &Path) -> io::Result<PathBuf> {
    fs::create_dir_all(dest_dir)?;
    let Some(file_name) = src.file_name() else {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, "missing filename"));
    };
    let dst = dest_dir.join(file_name);
    if src == dst {
        return Ok(dst);
    }
    if fs::rename(src, &dst).is_err() {
        fs::copy(src, &dst)?;
        fs::remove_file(src)?;
    }
    Ok(dst)
}

