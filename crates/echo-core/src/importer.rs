use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ImportedTrack {
    pub title: String,
    pub artist: String,
    pub album: Option<String>,
    pub duration_secs: Option<u32>,
    pub isrc: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ImportedPlaylist {
    pub name: String,
    pub tracks: Vec<ImportedTrack>,
}

/// Parses an M3U or M3U8 playlist.
pub fn parse_m3u(content: &str, default_name: &str) -> Vec<ImportedPlaylist> {
    let mut tracks = Vec::new();
    let mut pending_title = None;
    let mut pending_artist = None;
    let mut pending_dur = None;

    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        if let Some(rest) = trimmed.strip_prefix("#EXTINF:") {
            // #EXTINF:duration,Artist - Title  OR #EXTINF:duration,Title
            if let Some((dur_str, meta_str)) = rest.split_once(',') {
                let secs: Option<u32> = dur_str.trim().parse().ok().map(|s: i64| s.max(0) as u32);
                pending_dur = secs;
                let meta = meta_str.trim();
                if let Some((a, t)) = meta.split_once(" - ") {
                    pending_artist = Some(a.trim().to_string());
                    pending_title = Some(t.trim().to_string());
                } else {
                    pending_title = Some(meta.to_string());
                    pending_artist = None;
                }
            }
        } else if !trimmed.starts_with('#') {
            // This is the file path or URI line
            let title = pending_title.take().unwrap_or_else(|| {
                Path::new(trimmed)
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or(trimmed)
                    .to_string()
            });
            let artist = pending_artist.take().unwrap_or_default();
            let duration_secs = pending_dur.take();

            if !title.is_empty() {
                tracks.push(ImportedTrack {
                    title,
                    artist,
                    album: None,
                    duration_secs,
                    isrc: None,
                });
            }
        }
    }

    if tracks.is_empty() {
        Vec::new()
    } else {
        vec![ImportedPlaylist {
            name: default_name.to_string(),
            tracks,
        }]
    }
}

/// Simple CSV line splitter that respects quoted commas.
fn parse_csv_line(line: &str) -> Vec<String> {
    let mut fields = Vec::new();
    let mut current = String::new();
    let mut in_quotes = false;
    let mut chars = line.chars().peekable();

    while let Some(c) = chars.next() {
        match c {
            '"' => {
                if in_quotes && chars.peek() == Some(&'"') {
                    current.push('"');
                    chars.next();
                } else {
                    in_quotes = !in_quotes;
                }
            }
            ',' | '\t' if !in_quotes => {
                fields.push(current.trim().to_string());
                current.clear();
            }
            _ => {
                current.push(c);
            }
        }
    }
    fields.push(current.trim().to_string());
    fields
}

/// Parses Spotify (Exportify), TuneMyMusic, Soundiiz, and Apple Music CSV/TXT files.
pub fn parse_csv(content: &str, default_name: &str) -> Result<Vec<ImportedPlaylist>, String> {
    let mut lines = content.lines().filter(|l| !l.trim().is_empty());
    let header_line = lines.next().ok_or_else(|| "Empty CSV file".to_string())?;
    let headers: Vec<String> = parse_csv_line(header_line)
        .into_iter()
        .map(|h| h.trim_matches('"').to_lowercase())
        .collect();

    // Map column indices
    let mut title_idx = None;
    let mut artist_idx = None;
    let mut album_idx = None;
    let mut dur_idx = None;
    let mut isrc_idx = None;
    let mut playlist_idx = None;

    for (i, h) in headers.iter().enumerate() {
        let clean = h.trim();
        if clean == "track name" || clean == "track name(s)" || clean == "title" || clean == "name" {
            title_idx = Some(i);
        } else if clean == "artist name(s)" || clean == "artist name" || clean == "artist" || clean == "artists" {
            artist_idx = Some(i);
        } else if clean == "album name" || clean == "album" {
            album_idx = Some(i);
        } else if clean.contains("duration") || clean == "time" || clean.contains("length") {
            dur_idx = Some(i);
        } else if clean == "isrc" {
            isrc_idx = Some(i);
        } else if clean == "playlist name" || clean == "playlist" {
            playlist_idx = Some(i);
        }
    }

    let title_col = title_idx.ok_or_else(|| "No track title column found in CSV".to_string())?;
    let mut grouped_playlists: HashMap<String, Vec<ImportedTrack>> = HashMap::new();

    for line in lines {
        let fields = parse_csv_line(line);
        if fields.len() <= title_col {
            continue;
        }

        let raw_title = fields[title_col].trim_matches('"').trim();
        if raw_title.is_empty() {
            continue;
        }

        let artist = artist_idx
            .and_then(|idx| fields.get(idx))
            .map(|s| s.trim_matches('"').trim().to_string())
            .unwrap_or_default();

        let album = album_idx
            .and_then(|idx| fields.get(idx))
            .map(|s| s.trim_matches('"').trim().to_string())
            .filter(|s| !s.is_empty());

        let isrc = isrc_idx
            .and_then(|idx| fields.get(idx))
            .map(|s| s.trim_matches('"').trim().to_string())
            .filter(|s| !s.is_empty());

        let duration_secs = dur_idx.and_then(|idx| fields.get(idx)).and_then(|raw| {
            let s = raw.trim_matches('"').trim();
            if let Ok(ms) = s.parse::<u64>() {
                if ms > 1000 {
                    return Some((ms / 1000) as u32);
                } else {
                    return Some(ms as u32);
                }
            }
            if let Some(stripped) = s.strip_suffix('s') {
                return stripped.parse::<u32>().ok();
            }
            None
        });

        let pl_name = playlist_idx
            .and_then(|idx| fields.get(idx))
            .map(|s| s.trim_matches('"').trim().to_string())
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| default_name.to_string());

        grouped_playlists
            .entry(pl_name)
            .or_default()
            .push(ImportedTrack {
                title: raw_title.to_string(),
                artist,
                album,
                duration_secs,
                isrc,
            });
    }

    if grouped_playlists.is_empty() {
        return Err("No tracks extracted from CSV".to_string());
    }

    let mut result: Vec<ImportedPlaylist> = grouped_playlists
        .into_iter()
        .map(|(name, tracks)| ImportedPlaylist { name, tracks })
        .collect();
    result.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(result)
}

/// Parses an Apple Music / iTunes library XML export.
pub fn parse_apple_music_xml(content: &str) -> Result<Vec<ImportedPlaylist>, String> {
    // Quick, robust XML token extractor without heavy dependencies
    let mut track_map: HashMap<String, ImportedTrack> = HashMap::new();

    // Look for <key>Tracks</key><dict> section
    if let Some(tracks_start) = content.find("<key>Tracks</key>") {
        let tracks_content = &content[tracks_start..];
        let end_idx = tracks_content.find("<key>Playlists</key>").unwrap_or(tracks_content.len());
        let tracks_block = &tracks_content[..end_idx];

        for dict_chunk in tracks_block.split("<key>") {
            if let Some((track_id_str, rest)) = dict_chunk.split_once("</key><dict>") {
                let track_id = track_id_str.trim();
                let mut title = None;
                let mut artist = String::new();
                let mut album = None;
                let mut duration_secs = None;

                for pair in rest.split("<key>") {
                    if let Some((k, v_rest)) = pair.split_once("</key>") {
                        let key = k.trim();
                        if key == "Name" {
                            if let Some(val) = extract_xml_string(v_rest) {
                                title = Some(val);
                            }
                        } else if key == "Artist" {
                            if let Some(val) = extract_xml_string(v_rest) {
                                artist = val;
                            }
                        } else if key == "Album" {
                            album = extract_xml_string(v_rest);
                        } else if key == "Total Time" {
                            if let Some(ms_str) = extract_xml_integer(v_rest) {
                                if let Ok(ms) = ms_str.parse::<u64>() {
                                    duration_secs = Some((ms / 1000) as u32);
                                }
                            }
                        }
                    }
                }

                if let Some(t) = title {
                    track_map.insert(
                        track_id.to_string(),
                        ImportedTrack {
                            title: t,
                            artist,
                            album,
                            duration_secs,
                            isrc: None,
                        },
                    );
                }
            }
        }
    }

    let mut playlists = Vec::new();
    if let Some(pl_start) = content.find("<key>Playlists</key>") {
        let pl_block = &content[pl_start..];
        for pl_chunk in pl_block.split("<dict>") {
            let mut name = None;
            let mut track_ids = Vec::new();

            for pair in pl_chunk.split("<key>") {
                if let Some((k, v_rest)) = pair.split_once("</key>") {
                    let key = k.trim();
                    if key == "Name" {
                        name = extract_xml_string(v_rest);
                    } else if key == "Track ID" {
                        if let Some(id) = extract_xml_integer(v_rest) {
                            track_ids.push(id);
                        }
                    }
                }
            }

            if let Some(pl_name) = name {
                // Filter out Apple Music internal playlists like Library, Music, etc. if desired,
                // or keep them as named playlists!
                let tracks: Vec<ImportedTrack> = track_ids
                    .iter()
                    .filter_map(|id| track_map.get(id).cloned())
                    .collect();

                if !tracks.is_empty() {
                    playlists.push(ImportedPlaylist {
                        name: pl_name,
                        tracks,
                    });
                }
            }
        }
    }

    if playlists.is_empty() && !track_map.is_empty() {
        playlists.push(ImportedPlaylist {
            name: "Apple Music Library".to_string(),
            tracks: track_map.into_values().collect(),
        });
    }

    if playlists.is_empty() {
        Err("No playlists or tracks found in Apple Music XML".to_string())
    } else {
        Ok(playlists)
    }
}

fn extract_xml_string(s: &str) -> Option<String> {
    let start = s.find("<string>")? + 8;
    let end = s[start..].find("</string>")? + start;
    Some(s[start..end].replace("&amp;", "&").replace("&quot;", "\"").replace("&lt;", "<").replace("&gt;", ">"))
}

fn extract_xml_integer(s: &str) -> Option<String> {
    let start = s.find("<integer>")? + 9;
    let end = s[start..].find("</integer>")? + start;
    Some(s[start..end].trim().to_string())
}

/// Automatically detects the format and imports playlists from a file.
pub fn import_playlist_file(path: &Path) -> Result<Vec<ImportedPlaylist>, String> {
    let raw = std::fs::read_to_string(path).map_err(|e| format!("Failed to read file: {e}"))?;
    let stem = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("Imported Playlist");

    let ext = path
        .extension()
        .and_then(|s| s.to_str())
        .map(|s| s.to_lowercase())
        .unwrap_or_default();

    if ext == "m3u" || ext == "m3u8" || raw.starts_with("#EXTM3U") {
        let pls = parse_m3u(&raw, stem);
        if pls.is_empty() {
            Err("No tracks found in M3U file".to_string())
        } else {
            Ok(pls)
        }
    } else if ext == "xml" || raw.contains("<plist") {
        parse_apple_music_xml(&raw)
    } else {
        parse_csv(&raw, stem)
    }
}
