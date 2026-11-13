use std::collections::HashSet;

use echo_core::{
    base_version_key, is_preview_title, score_artist_name, score_candidate, ListenerContext,
};
use echo_youtube::{BrowseItem, HomeSection};
pub fn open_browser_url(url: &str) -> std::io::Result<()> {
    #[cfg(target_os = "windows")]
    {
        std::process::Command::new("cmd")
            .args(["/C", "start", "", url])
            .spawn()?;
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = url;
    }
    Ok(())
}

pub fn parse_folder_list(raw: &str) -> Vec<String> {
    raw.split([';', '\n'])
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(ToString::to_string)
        .collect()
}

pub fn append_folder_entry(list: &mut Vec<String>, path: &str) -> String {
    let trimmed = path.trim();
    if !trimmed.is_empty() && !list.iter().any(|existing| existing.eq_ignore_ascii_case(trimmed)) {
        list.push(trimmed.to_string());
    }
    list.join("; ")
}

pub fn item_artist_hint(item: &BrowseItem) -> String {
    match item {
        BrowseItem::Track(t) => t.primary_artist().to_string(),
        BrowseItem::Album { artist, .. } => artist.clone(),
        BrowseItem::Artist { name, .. } => name.clone(),
        BrowseItem::Playlist { author, .. } => author.clone(),
    }
}

pub fn score_browse_item(item: &BrowseItem, ctx: &ListenerContext) -> f64 {
    match item {
        BrowseItem::Track(t) => score_candidate(t, ctx),
        BrowseItem::Album { artist, .. }
        | BrowseItem::Artist { name: artist, .. }
        | BrowseItem::Playlist { author: artist, .. } => score_artist_name(artist, ctx),
    }
}

pub fn item_version_key(item: &BrowseItem) -> Option<String> {
    match item {
        BrowseItem::Track(t) => Some(base_version_key(&t.title, t.primary_artist())),
        _ => None,
    }
}

pub fn rank_home_section(
    sec: &HomeSection,
    ctx: &ListenerContext,
    preserve_order: bool,
    seen_versions: &mut HashSet<String>,
    seen_keys: &mut HashSet<String>,
) -> Option<HomeSection> {
    if sec.items.is_empty() {
        return None;
    }
    let clean: Vec<BrowseItem> = sec
        .items
        .iter()
        .filter(|it| !is_preview_title(it.title()))
        .cloned()
        .collect();
    if clean.is_empty() {
        return None;
    }

    let mut ordered = if ctx.known && clean.len() > 3 {
        let kept: Vec<BrowseItem> = clean
            .iter()
            .filter(|it| !ctx.is_disliked_artist(&item_artist_hint(it)))
            .cloned()
            .collect();
        if kept.len() >= 3 {
            kept
        } else {
            clean
        }
    } else {
        clean
    };

    if ctx.known && !preserve_order {
        ordered.sort_by(|a, b| {
            let sa = (score_browse_item(a, ctx) * 100.0).round() as i64;
            let sb = (score_browse_item(b, ctx) * 100.0).round() as i64;
            sb.cmp(&sa)
        });
    }

    // Echo TidyPass: prevent duplicate songs/versions or duplicate cards across Home shelves.
    let mut deduped = Vec::new();
    for it in ordered {
        let key = it.stable_key();
        if seen_keys.contains(&key) {
            continue;
        }
        if let Some(vk) = item_version_key(&it) {
            if seen_versions.contains(&vk) {
                continue;
            }
            seen_versions.insert(vk);
        }
        seen_keys.insert(key);
        deduped.push(it);
    }

    if deduped.is_empty() {
        return None;
    }

    Some(HomeSection {
        title: sec.title.clone(),
        items: deduped,
    })
}
