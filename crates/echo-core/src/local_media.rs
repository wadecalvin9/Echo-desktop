use crate::models::{AlbumRef, ArtistRef, Track};
use crate::settings::ScannerSensitivity;
use lofty::config::ParseOptions;
use lofty::file::{AudioFile, TaggedFileExt};
use lofty::probe::Probe;
use lofty::tag::{Accessor, ItemKey};
use std::collections::{BTreeMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

fn fnv1a_hex(s: &str) -> String {
    let mut hash: u64 = 0xcbf29ce484222325;
    for &b in s.as_bytes() {
        hash ^= b as u64;
        hash = hash.wrapping_mul(0x100000001b3);
    }
    format!("{hash:016x}")
}

const SUPPORTED_EXTS: &[&str] = &[
    "mp3", "flac", "m4a", "mp4", "aac", "ogg", "opus", "wav", "wv", "ape", "tta",
];

#[derive(Debug, Clone)]
pub struct LocalMediaScanConfig {
    pub include_folders: Vec<PathBuf>,
    pub excluded_folders: Vec<PathBuf>,
    pub sensitivity: ScannerSensitivity,
    pub strict_file_names: bool,
    pub cover_cache_dir: Option<PathBuf>,
}

/// Parses a ReplayGain string such as `"-6.45 dB"` or `"+1.2 dB"` and converts it to Echo's
/// `loudness_db` scale (relative to -14 LKFS vs ReplayGain -18 LUFS).
pub fn parse_replaygain_db(raw: &str) -> Option<f32> {
    let cleaned = raw
        .trim()
        .trim_end_matches(|c: char| !c.is_ascii_digit() && c != '.' && c != '-' && c != '+')
        .trim();
    let rg_gain: f32 = cleaned.parse().ok()?;
    if !rg_gain.is_finite() {
        return None;
    }
    Some(-rg_gain - 4.0)
}

/// Parses an RFC 7845 / Ogg Opus `R128_TRACK_GAIN` / `R128_ALBUM_GAIN` Q7.8 integer string
/// (relative to -23 LUFS) into Echo's `loudness_db` scale (relative to -14 LKFS).
pub fn parse_r128_gain_db(raw: &str) -> Option<f32> {
    let q78: i32 = raw.trim().parse().ok()?;
    let gain_db = (q78 as f32) / 256.0;
    Some(-gain_db - 9.0)
}

pub fn scan_local_folders(cfg: &LocalMediaScanConfig) -> Vec<Track> {
    let mut files = Vec::new();
    let excluded: HashSet<PathBuf> = cfg
        .excluded_folders
        .iter()
        .map(|p| fs::canonicalize(p).unwrap_or_else(|_| p.clone()))
        .collect();

    for root in &cfg.include_folders {
        if root.is_dir() {
            walk_audio_files(root, &excluded, &mut files);
        }
    }

    if let Some(dir) = &cfg.cover_cache_dir {
        let _ = fs::create_dir_all(dir);
    }

    let mut tracks = Vec::new();
    let mut dedup_keys = HashSet::new();

    for (idx, path) in files.iter().enumerate() {
        if idx > 0 && idx % 250 == 0 {
            // Yield thread control periodically to keep the runtime responsive
            std::thread::yield_now();
        }
        let Some(track) = read_local_track(path, cfg) else {
            continue;
        };
        let dedup = match cfg.sensitivity {
            ScannerSensitivity::MatchTitle => track.title.trim().to_lowercase(),
            ScannerSensitivity::MatchTitleAndArtists => format!(
                "{}::{}",
                track.title.trim().to_lowercase(),
                track.artist_display().to_lowercase()
            ),
            ScannerSensitivity::MatchTitleArtistsAlbums => format!(
                "{}::{}::{}",
                track.title.trim().to_lowercase(),
                track.artist_display().to_lowercase(),
                track
                    .album
                    .as_ref()
                    .map(|a| a.title.to_lowercase())
                    .unwrap_or_default()
            ),
        };
        if dedup_keys.insert(dedup) {
            tracks.push(track);
        }
    }
    tracks
}

fn walk_audio_files(dir: &Path, excluded: &HashSet<PathBuf>, out: &mut Vec<PathBuf>) {
    let canon = fs::canonicalize(dir).unwrap_or_else(|_| dir.to_path_buf());
    if excluded.contains(&canon) || excluded.contains(dir) || dir.join(".nomedia").exists() {
        return;
    }
    let Ok(rd) = fs::read_dir(dir) else {
        return;
    };
    for entry in rd.flatten() {
        let name = entry.file_name();
        if name.to_string_lossy().starts_with('.') {
            continue;
        }
        let path = entry.path();
        if path.is_dir() {
            walk_audio_files(&path, excluded, out);
        } else if path.is_file() {
            let ok = path
                .extension()
                .and_then(|e| e.to_str())
                .map(|ext| SUPPORTED_EXTS.contains(&ext.to_ascii_lowercase().as_str()))
                .unwrap_or(false);
            if ok {
                out.push(path);
            }
        }
    }
}

fn read_local_track(path: &Path, cfg: &LocalMediaScanConfig) -> Option<Track> {
    let path_str = path.to_string_lossy().into_owned();
    let id = format!("local_{}", fnv1a_hex(&path_str));

    let raw_stem = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("Unknown Track")
        .to_string();

    let (fallback_artist, fallback_title) = if !cfg.strict_file_names {
        if let Some((a, t)) = raw_stem.split_once(" - ") {
            (Some(a.trim().to_string()), t.trim().to_string())
        } else {
            (None, raw_stem.clone())
        }
    } else {
        (None, raw_stem.clone())
    };

    let tagged = Probe::open(path)
        .ok()
        .and_then(|p| p.options(ParseOptions::new().read_properties(true)).read().ok());

    let mut title = fallback_title;
    let mut artists: Vec<ArtistRef> = fallback_artist
        .into_iter()
        .map(|name| ArtistRef { id: None, name })
        .collect();
    let mut album: Option<AlbumRef> = None;
    let mut duration_secs: Option<u32> = None;
    let mut loudness_db: Option<f32> = None;
    let mut thumbnail_url: Option<String> = None;

    if let Some(tf) = tagged {
        let secs = tf.properties().duration().as_secs();
        if secs > 0 {
            duration_secs = Some(secs as u32);
        }

        if let Some(tag) = tf.primary_tag().or_else(|| tf.first_tag()) {
            if let Some(t) = tag.title().filter(|s| !s.trim().is_empty()) {
                title = t.trim().to_string();
            }
            if let Some(a) = tag.artist().filter(|s| !s.trim().is_empty()) {
                artists = a
                    .split([',', ';'])
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .map(|name| ArtistRef {
                        id: None,
                        name: name.to_string(),
                    })
                    .collect();
            }
            if let Some(alb) = tag.album().filter(|s| !s.trim().is_empty()) {
                album = Some(AlbumRef {
                    id: None,
                    title: alb.trim().to_string(),
                });
            }

            // Extract ReplayGain / EBU R128 loudness tags
            let mut rg_track: Option<f32> = None;
            let mut rg_album: Option<f32> = None;
            let mut r128_track: Option<f32> = None;
            let mut r128_album: Option<f32> = None;

            for item in tag.items() {
                let Some(val_str) = item.value().text() else {
                    continue;
                };
                match item.key() {
                    ItemKey::ReplayGainTrackGain => rg_track = parse_replaygain_db(val_str),
                    ItemKey::ReplayGainAlbumGain => rg_album = parse_replaygain_db(val_str),
                    ItemKey::Unknown(k) => {
                        let up = k.to_ascii_uppercase();
                        if up == "REPLAYGAIN_TRACK_GAIN" {
                            rg_track = parse_replaygain_db(val_str);
                        } else if up == "REPLAYGAIN_ALBUM_GAIN" {
                            rg_album = parse_replaygain_db(val_str);
                        } else if up == "R128_TRACK_GAIN" {
                            r128_track = parse_r128_gain_db(val_str);
                        } else if up == "R128_ALBUM_GAIN" {
                            r128_album = parse_r128_gain_db(val_str);
                        }
                    }
                    _ => {}
                }
            }
            loudness_db = r128_track.or(r128_album).or(rg_track).or(rg_album);

            if let (Some(cache_dir), Some(pic)) = (&cfg.cover_cache_dir, tag.pictures().first()) {
                let cover_path = cache_dir.join(format!("{id}.img"));
                if !cover_path.exists() {
                    let _ = fs::write(&cover_path, pic.data());
                }
                if cover_path.exists() {
                    thumbnail_url = Some(cover_path.to_string_lossy().into_owned());
                }
            }
        }
    }

    Some(Track {
        id,
        title,
        artists,
        album,
        duration_secs,
        thumbnail_url,
        loudness_db,
        liked: false,
        is_local: true,
        local_path: Some(path_str),
    })
}

/// Groups local tracks by parent folder for Echo's Folder Tree view.
pub fn group_tracks_by_folder(tracks: &[Track]) -> BTreeMap<String, Vec<Track>> {
    let mut map: BTreeMap<String, Vec<Track>> = BTreeMap::new();
    for t in tracks {
        let folder = t
            .local_path
            .as_deref()
            .and_then(|p| Path::new(p).parent())
            .map(|p| p.to_string_lossy().into_owned())
            .unwrap_or_else(|| "Unknown Folder".to_string());
        map.entry(folder).or_default().push(t.clone());
    }
    map
}

/// Extracts embedded lyrics (USLT, LYRICS, or synchronized tags) from a local audio file.
pub fn extract_embedded_lyrics(path: &Path) -> Option<String> {
    let tagged = Probe::open(path)
        .ok()?
        .options(ParseOptions::new().read_properties(false))
        .read()
        .ok()?;
    let tag = tagged.primary_tag().or_else(|| tagged.first_tag())?;
    for item in tag.items() {
        if item.key() == &ItemKey::Lyrics {
            if let Some(txt) = item.value().text() {
                let trimmed = txt.trim();
                if !trimmed.is_empty() {
                    return Some(trimmed.to_string());
                }
            }
        }
    }
    None
}

/// Tags a downloaded or offline audio file with metadata and cover artwork.
pub fn tag_audio_file(
    path: &Path,
    title: &str,
    artist: &str,
    album: Option<&str>,
    cover_bytes: Option<&[u8]>,
) -> Result<(), String> {
    use lofty::config::WriteOptions;
    use lofty::picture::{MimeType, Picture, PictureType};
    use lofty::tag::{Tag, TagExt};

    let mut tagged = Probe::open(path)
        .map_err(|e| e.to_string())?
        .read()
        .map_err(|e| e.to_string())?;

    let tag = match tagged.primary_tag_mut() {
        Some(t) => t,
        None => {
            let tag_type = tagged.primary_tag_type();
            tagged.insert_tag(Tag::new(tag_type));
            tagged
                .primary_tag_mut()
                .ok_or_else(|| "Could not create tag".to_string())?
        }
    };

    tag.set_title(title.to_string());
    tag.set_artist(artist.to_string());
    if let Some(alb) = album {
        tag.set_album(alb.to_string());
    }

    if let Some(bytes) = cover_bytes {
        if !bytes.is_empty() {
            let mime = if bytes.starts_with(&[0x89, 0x50, 0x4E, 0x47]) {
                MimeType::Png
            } else {
                MimeType::Jpeg
            };
            let pic = Picture::new_unchecked(
                PictureType::CoverFront,
                Some(mime),
                None,
                bytes.to_vec(),
            );
            tag.push_picture(pic);
        }
    }

    tag.save_to_path(path, WriteOptions::default())
        .map_err(|e| e.to_string())?;
    Ok(())
}

