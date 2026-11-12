use crate::models::{QueueItem, Track};
use std::collections::{HashMap, HashSet};

/// Songs immediately after the current track that are never reordered or pruned.
pub const ADAPTIVE_LOCKED: usize = 4;
/// Reduced lock window after a strong user signal (early skip or explicit like).
pub const ADAPTIVE_STRONG_LOCKED: usize = 2;
pub const ADAPTIVE_MAX_DROP_SHARE: f64 = 0.4;
pub const ADAPTIVE_STRONG_DROP_SHARE: f64 = 0.75;
pub const ADAPTIVE_DROP_BELOW: f64 = -0.25;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SongTag {
    Slowed,
    SpedUp,
    Remix,
    Live,
    Instrumental,
    Acoustic,
    Cover,
}

#[derive(Debug, Clone, Default)]
pub struct ListenerContext {
    pub recent_tags: HashSet<SongTag>,
    pub liked_artists: HashSet<String>,
    pub skipped_artists: HashMap<String, usize>,
    pub plain_run: bool,
    pub known: bool,
}

impl ListenerContext {
    pub fn record_play(&mut self, track: &Track, liked: bool) {
        self.known = true;
        let tags = extract_title_tags(&track.title);
        if tags.is_empty() {
            self.plain_run = true;
        } else {
            self.plain_run = false;
            self.recent_tags.extend(tags);
        }
        let artist = track.primary_artist().trim().to_lowercase();
        if !artist.is_empty() {
            if liked {
                self.liked_artists.insert(artist.clone());
                self.skipped_artists.remove(&artist);
            }
        }
    }

    pub fn record_early_skip(&mut self, track: &Track) {
        self.known = true;
        let artist = track.primary_artist().trim().to_lowercase();
        if !artist.is_empty() && !self.liked_artists.contains(&artist) {
            *self.skipped_artists.entry(artist).or_insert(0) += 1;
        }
    }
}

pub fn extract_title_tags(title: &str) -> HashSet<SongTag> {
    let lower = title.to_lowercase();
    let mut tags = HashSet::new();
    if lower.contains("slowed") || lower.contains("reverb") {
        tags.insert(SongTag::Slowed);
    }
    if lower.contains("sped up") || lower.contains("speed up") || lower.contains("nightcore") {
        tags.insert(SongTag::SpedUp);
    }
    if lower.contains("remix") {
        tags.insert(SongTag::Remix);
    }
    if lower.contains("live") && (lower.contains("(live") || lower.contains("- live")) {
        tags.insert(SongTag::Live);
    }
    if lower.contains("instrumental") || lower.contains("karaoke") {
        tags.insert(SongTag::Instrumental);
    }
    if lower.contains("acoustic") {
        tags.insert(SongTag::Acoustic);
    }
    if lower.contains("cover") {
        tags.insert(SongTag::Cover);
    }
    tags
}

pub fn score_candidate(track: &Track, ctx: &ListenerContext) -> f64 {
    let artist = track.primary_artist().trim().to_lowercase();
    let mut score = 0.0;

    if !artist.is_empty() {
        if let Some(&skips) = ctx.skipped_artists.get(&artist) {
            score -= (skips as f64) * 0.45;
        }
        if ctx.liked_artists.contains(&artist) {
            score += 0.4;
        }
    }

    let tags = extract_title_tags(&track.title);
    if !tags.is_empty() {
        if ctx.plain_run && !ctx.recent_tags.is_empty() {
            // Treated/edit version in the middle of a plain run that hasn't asked for edits.
            if !tags.iter().any(|t| ctx.recent_tags.contains(t)) {
                score -= 0.6;
            }
        } else if ctx.plain_run {
            score -= 0.5;
        } else if tags.iter().any(|t| ctx.recent_tags.contains(t)) {
            score += 0.3;
        }
    }

    score
}

/// Filters the provisional tail beyond `locked` items when `ctx.known`, preserving relative order
/// and never removing more than `drop_share` of the tail (or emptying it completely).
/// Returns the number of tracks dropped.
pub fn prune_queue_tail(
    items: &mut Vec<QueueItem>,
    current_index: Option<usize>,
    ctx: &ListenerContext,
    strong_signal: bool,
) -> usize {
    if !ctx.known {
        return 0;
    }
    let cur = match current_index {
        Some(i) => i,
        None => return 0,
    };
    let locked = if strong_signal {
        ADAPTIVE_STRONG_LOCKED
    } else {
        ADAPTIVE_LOCKED
    };
    let tail_start = cur + 1 + locked;
    if tail_start >= items.len() {
        return 0;
    }

    let drop_share = if strong_signal {
        ADAPTIVE_STRONG_DROP_SHARE
    } else {
        ADAPTIVE_MAX_DROP_SHARE
    };
    let tail_len = items.len() - tail_start;
    let max_drops = ((tail_len as f64 * drop_share).floor() as usize).min(tail_len.saturating_sub(1));
    if max_drops == 0 {
        return 0;
    }

    let mut dropped_indices = HashSet::new();
    let mut scored: Vec<(usize, f64)> = (tail_start..items.len())
        .filter_map(|idx| {
            let s = score_candidate(&items[idx].track, ctx);
            (s <= ADAPTIVE_DROP_BELOW).then_some((idx, s))
        })
        .collect();
    scored.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));
    for (idx, _) in scored.into_iter().take(max_drops) {
        dropped_indices.insert(idx);
    }

    if dropped_indices.is_empty() {
        return 0;
    }

    let mut i = 0usize;
    let mut removed = 0usize;
    items.retain(|_| {
        let keep = !dropped_indices.contains(&i);
        if !keep {
            removed += 1;
        }
        i += 1;
        keep
    });
    removed
}

