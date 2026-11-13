use crate::adaptive_queue::extract_title_tags;
use crate::models::Track;
use chrono::{Local, TimeZone, Timelike};
use regex::Regex;
use std::collections::{HashMap, HashSet};
use std::sync::OnceLock;

const DAY_MS: f64 = 86_400_000.0;
const SESSION_GAP_MS: i64 = 30 * 60 * 1000;
const MIN_POSITIVE_SECS: u64 = 30;
const SKIP_IGNORED_FROM: f64 = 0.90;
const ACTIVATION_DECAY: f64 = 0.5;
const ACTIVATION_PER_SESSION: usize = 3;
const SATIATION_START: f64 = 10.0;
const SATIATION_RAMP: f64 = 20.0;
const FRESH_COOLDOWN_MS: i64 = 60 * 60 * 1000;
const AGAIN_WINDOW_DAYS: i64 = 14;
const DORMANT_AFTER_DAYS: i64 = 45;
const SEEDS_TOTAL: usize = 12;
const MAX_PER_ARTIST: usize = 2;
const MAX_PER_SEED: usize = 3;

#[derive(Debug, Clone)]
pub struct ListenRecord {
    pub song_id: String,
    pub played_at_ms: i64,
    pub duration_played_secs: u64,
}

#[derive(Debug, Clone)]
pub struct RelatedEdge {
    pub seed_id: String,
    pub related_id: String,
    pub source: u8,
    pub updated_at_ms: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum EngineLane {
    Related,
    Again,
    Artist,
    Rediscover,
    Explore,
}

impl EngineLane {
    fn quota_weights() -> [(EngineLane, f64); 5] {
        [
            (EngineLane::Related, 0.38),
            (EngineLane::Again, 0.24),
            (EngineLane::Artist, 0.18),
            (EngineLane::Rediscover, 0.12),
            (EngineLane::Explore, 0.08),
        ]
    }
}

#[derive(Debug, Clone)]
pub struct EngineCard {
    pub track: Track,
    pub lane: EngineLane,
    pub score: f64,
    pub seed_title: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct EngineOutput {
    pub seeds: Vec<Track>,
    pub uncached_seed_ids: Vec<String>,
    pub quick_picks: Vec<EngineCard>,
    pub discover: Vec<Track>,
    pub heavy_rotation: Vec<Track>,
    pub forgotten_favorites: Vec<Track>,
}

static BASE_STRIP_RE: OnceLock<Regex> = OnceLock::new();
static PREVIEW_RE: OnceLock<Regex> = OnceLock::new();

/// Normalizes a song title + primary artist into a canonical base version key
/// so original / (Slowed) / (Live) / (Official Video) / (Remix) share one key.
pub fn base_version_key(title: &str, artist: &str) -> String {
    let strip_re = BASE_STRIP_RE.get_or_init(|| {
        Regex::new(
            r"(?i)\s*[\(\[](?:slowed(?:\s*\+\s*reverb)?|sped\s*up|speed\s*up|nightcore|remix|Acoustic|live(?:\s+at[^\)\]]*)?|instrumental|karaoke|cover|official\s+(?:music\s+)?(?:video|audio|visualizer)|lyric(?:s)?(?:\s+video)?|audio|explicit|clean|remaster(?:ed)?(?:\s+\d{4})?)[\)\]]",
        )
        .expect("valid version regex")
    });
    let mut base = strip_re.replace_all(title, "").trim().to_lowercase();
    let art = artist.trim().to_lowercase();
    if !art.is_empty() {
        let prefix = format!("{art} - ");
        if let Some(rest) = base.strip_prefix(&prefix) {
            base = rest.trim().to_string();
        }
    }
    let collapsed = base.split_whitespace().collect::<Vec<_>>().join(" ");
    format!("{collapsed}|{art}")
}

pub fn is_preview_title(title: &str) -> bool {
    let re = PREVIEW_RE.get_or_init(|| {
        Regex::new(r"(?i)\b(?:preview|teaser|snippet)\b").expect("valid preview regex")
    });
    re.is_match(title)
}

#[derive(Default)]
struct SongStats {
    activation: f64,
    old_activation: f64,
    bucket_activation: f64,
    exposure: f64,
    old_plays: u64,
    old_play_secs: u64,
    recent_play_secs: u64,
    last_started_at: i64,
    last_good_at: i64,
    good_listens: u64,
    per_session: HashMap<usize, usize>,
}

#[derive(Default)]
struct ArtistAgg {
    strength: f64,
    good_listens: u64,
    positive_short: f64,
    positive_long: f64,
    good_by_bucket: [u64; 8],
}

fn day_part_bucket(timestamp_ms: i64) -> usize {
    Local
        .timestamp_millis_opt(timestamp_ms)
        .single()
        .map(|dt| (dt.hour() as usize / 3).min(7))
        .unwrap_or(0)
}

fn listen_engagement(l: &ListenRecord, track: &Track) -> f64 {
    if track.liked && l.duration_played_secs >= 10 {
        return 1.0;
    }
    if l.duration_played_secs < MIN_POSITIVE_SECS {
        return 0.0;
    }
    match track.duration_secs.filter(|&d| d > 0) {
        Some(dur) => {
            let r = (l.duration_played_secs as f64 / dur as f64).min(1.0);
            ((r - 0.10) / 0.70).clamp(0.0, 1.0)
        }
        None => ((l.duration_played_secs - MIN_POSITIVE_SECS) as f64 / 150.0).clamp(0.0, 0.7),
    }
}

fn listen_skip_penalty(l: &ListenRecord, track: &Track) -> f64 {
    if l.duration_played_secs == 0 || l.duration_played_secs >= MIN_POSITIVE_SECS {
        if let Some(dur) = track.duration_secs.filter(|&d| d > 0) {
            let r = (l.duration_played_secs as f64 / dur as f64).min(1.0);
            if r >= SKIP_IGNORED_FROM || l.duration_played_secs < MIN_POSITIVE_SECS {
                return if l.duration_played_secs > 0 && l.duration_played_secs < 20 {
                    0.35
                } else {
                    0.0
                };
            }
            let r30 = (MIN_POSITIVE_SECS as f64 / dur as f64).min(0.5);
            let ramp = ((r - r30) / (0.5 - r30).max(0.1)).clamp(0.0, 1.0);
            return (1.0 - r) * ramp;
        }
        return 0.0;
    }
    // Very short listen (5s..29s) signals early skip.
    if l.duration_played_secs >= 5 {
        0.4
    } else {
        0.0
    }
}

fn recency_weight(now_ms: i64, then_ms: i64) -> f64 {
    let age_hours = ((now_ms - then_ms).max(0) as f64) / 3_600_000.0;
    (age_hours + 1.0).powf(-ACTIVATION_DECAY)
}

#[allow(clippy::too_many_arguments)]
pub fn build_recommendations(
    now_ms: i64,
    songs: &HashMap<String, Track>,
    listens: &[ListenRecord],
    edges: &[RelatedEdge],
    impressions: &HashMap<String, usize>,
    disliked_artists: &HashSet<String>,
    desired_picks: usize,
    rotation_cursor: usize,
) -> EngineOutput {
    let mut ordered_listens: Vec<&ListenRecord> = listens.iter().collect();
    ordered_listens.sort_by_key(|l| l.played_at_ms);

    // Group listens into sessions (30-min gap boundary) so a loop isn't 1,000 votes.
    let mut session_ids: Vec<usize> = vec![0; ordered_listens.len()];
    let mut cur_sess = 0usize;
    for i in 0..ordered_listens.len() {
        if i > 0 && ordered_listens[i].played_at_ms - ordered_listens[i - 1].played_at_ms > SESSION_GAP_MS
        {
            cur_sess += 1;
        }
        session_ids[i] = cur_sess;
    }
    let latest_session = cur_sess;
    let current_bucket = day_part_bucket(now_ms);

    let dormant_cut = now_ms - DORMANT_AFTER_DAYS * 86_400_000;
    let forgotten_cut = now_ms - 30 * 86_400_000;
    let short_cut = now_ms - 7 * 86_400_000;
    let context_cut = now_ms - 90 * 86_400_000;

    let mut song_stats: HashMap<String, SongStats> = HashMap::new();
    let mut artist_stats: HashMap<String, ArtistAgg> = HashMap::new();
    let mut good_by_bucket = [0u64; 8];
    let mut good_in_window = 0u64;

    // Walk newest-first so per-session cap keeps the freshest plays.
    for idx in (0..ordered_listens.len()).rev() {
        let l = ordered_listens[idx];
        let sess = session_ids[idx];
        let Some(track) = songs.get(&l.song_id) else {
            continue;
        };
        let g = listen_engagement(l, track);
        let skip = listen_skip_penalty(l, track);
        let v = g - 0.5 * skip;

        let st = song_stats.entry(l.song_id.clone()).or_default();
        st.last_started_at = st.last_started_at.max(l.played_at_ms);
        let good = g >= 0.35;
        if good {
            st.good_listens += 1;
            st.last_good_at = st.last_good_at.max(l.played_at_ms);
        }
        if l.played_at_ms < forgotten_cut {
            if good {
                st.old_plays += 1;
            }
            st.old_play_secs += l.duration_played_secs;
        } else {
            st.recent_play_secs += l.duration_played_secs;
        }

        let used = *st.per_session.get(&sess).unwrap_or(&0);
        if used < ACTIVATION_PER_SESSION {
            st.per_session.insert(sess, used + 1);
            let term = v * recency_weight(now_ms, l.played_at_ms);
            st.activation += term;
            if l.played_at_ms < dormant_cut {
                st.old_activation += term;
            }
            if l.played_at_ms >= context_cut && day_part_bucket(l.played_at_ms) == current_bucket {
                st.bucket_activation += term;
            }
        }
        if l.played_at_ms >= context_cut && g > 0.0 {
            let days = ((now_ms - l.played_at_ms).max(0) as f64) / DAY_MS;
            st.exposure += g * (days + 1.0).powf(-ACTIVATION_DECAY);
        }

        let art_key = track.primary_artist().trim().to_lowercase();
        if !art_key.is_empty() {
            let agg = artist_stats.entry(art_key).or_default();
            if good {
                agg.good_listens += 1;
                if l.played_at_ms >= context_cut {
                    let b = day_part_bucket(l.played_at_ms);
                    agg.good_by_bucket[b] += 1;
                    good_by_bucket[b] += 1;
                    good_in_window += 1;
                }
            }
            if v > 0.0 {
                if l.played_at_ms >= short_cut {
                    agg.positive_short += v;
                }
                if l.played_at_ms >= context_cut {
                    agg.positive_long += v;
                }
            }
        }
    }

    // Liked pseudo-listen boost + artist strength rollup.
    for (id, track) in songs {
        if track.liked {
            let st = song_stats.entry(id.clone()).or_default();
            st.activation += 0.6;
        }
    }
    for (id, st) in &song_stats {
        if let Some(t) = songs.get(id) {
            let art_key = t.primary_artist().trim().to_lowercase();
            if !art_key.is_empty() {
                artist_stats.entry(art_key).or_default().strength += st.activation.max(0.0);
            }
        }
    }

    // Multi-pool seed sampler: session, recent today, liked, day-part bucket, lifetime strong.
    let mut seed_scores: HashMap<String, f64> = HashMap::new();
    for idx in (0..ordered_listens.len()).rev() {
        let l = ordered_listens[idx];
        let sess = session_ids[idx];
        let Some(track) = songs.get(&l.song_id) else {
            continue;
        };
        if track.is_local || disliked_artists.contains(&track.primary_artist().trim().to_lowercase())
        {
            continue;
        }
        if listen_engagement(l, track) < 0.35 && !(sess == latest_session && l.duration_played_secs == 0)
        {
            continue;
        }
        let rec = recency_weight(now_ms, l.played_at_ms);
        let pool_bonus = if sess == latest_session { 1.4 } else { 1.0 };
        let entry = seed_scores.entry(l.song_id.clone()).or_insert(0.0);
        *entry = (*entry).max(rec * pool_bonus);
    }
    for (id, track) in songs {
        if !track.is_local && track.liked && !disliked_artists.contains(&track.primary_artist().trim().to_lowercase()) {
            let entry = seed_scores.entry(id.clone()).or_insert(0.0);
            *entry = (*entry + 0.65).max(0.65);
        }
    }
    for (id, st) in &song_stats {
        if let Some(track) = songs.get(id) {
            if !track.is_local && st.activation > 0.0 && !disliked_artists.contains(&track.primary_artist().trim().to_lowercase()) {
                let sat = ((st.exposure - SATIATION_START) / SATIATION_RAMP).clamp(0.0, 0.85);
                let bucket_boost = st.bucket_activation.max(0.0) * 0.4;
                let e = seed_scores.entry(id.clone()).or_insert(0.0);
                *e = (*e + (st.activation + bucket_boost) * (1.0 - sat)).max(0.0);
            }
        }
    }

    let mut ranked_seeds: Vec<(String, f64)> = seed_scores.into_iter().collect();
    ranked_seeds.sort_by(|a, b| {
        b.1.partial_cmp(&a.1)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.0.cmp(&b.0))
    });
    // Subtle rotation across top seeds when user clicks Refresh.
    if ranked_seeds.len() > 3 && rotation_cursor > 0 {
        let shift = rotation_cursor % ranked_seeds.len().min(6);
        ranked_seeds.rotate_left(shift);
    }
    ranked_seeds.truncate(SEEDS_TOTAL);

    let seed_ids: HashSet<String> = ranked_seeds.iter().map(|(id, _)| id.clone()).collect();
    let seeds: Vec<Track> = ranked_seeds
        .iter()
        .filter_map(|(id, _)| songs.get(id).cloned())
        .collect();

    let mut edge_seeds: HashSet<String> = HashSet::new();
    let mut referrers: HashMap<String, Vec<String>> = HashMap::new();
    for edge in edges {
        edge_seeds.insert(edge.seed_id.clone());
        if seed_ids.contains(&edge.seed_id) {
            referrers
                .entry(edge.related_id.clone())
                .or_default()
                .push(edge.seed_id.clone());
        }
    }
    let uncached_seed_ids: Vec<String> = ranked_seeds
        .iter()
        .map(|(id, _)| id.clone())
        .filter(|id| !edge_seeds.contains(id))
        .take(4)
        .collect();

    // Active run tag context (tracks in last 90 minutes).
    let run_cut = now_ms - 90 * 60 * 1000;
    let recent_tags: HashSet<_> = ordered_listens
        .iter()
        .rev()
        .filter(|l| l.played_at_ms >= run_cut)
        .take(5)
        .filter_map(|l| songs.get(&l.song_id))
        .flat_map(|t| extract_title_tags(&t.title))
        .collect();

    let mut fresh_excluded_groups: HashSet<String> = HashSet::new();
    for (id, st) in &song_stats {
        if st.last_started_at >= now_ms - FRESH_COOLDOWN_MS {
            if let Some(t) = songs.get(id) {
                fresh_excluded_groups.insert(base_version_key(&t.title, t.primary_artist()));
            }
        }
    }
    let seed_groups: HashSet<String> = seeds
        .iter()
        .map(|s| base_version_key(&s.title, s.primary_artist()))
        .collect();

    let total_short: f64 = artist_stats.values().map(|a| a.positive_short).sum();
    let total_long: f64 = artist_stats.values().map(|a| a.positive_long).sum();
    let bucket_share = if good_in_window > 0 {
        good_by_bucket[current_bucket] as f64 / good_in_window as f64
    } else {
        0.0
    };

    let score_song = |song: &Track| -> f64 {
        let st = song_stats.get(&song.id);
        let art_key = song.primary_artist().trim().to_lowercase();
        let art = artist_stats.get(&art_key);

        let act = st.map(|s| s.activation / (1.0 + s.activation.abs())).unwrap_or(0.0);
        let sat = st
            .map(|s| ((s.exposure - SATIATION_START) / SATIATION_RAMP).clamp(0.0, 1.0))
            .unwrap_or(0.0);
        let dorm = st
            .filter(|_| {
                st.is_none_or(|s| s.last_good_at < dormant_cut)
            })
            .map(|s| {
                let old_a = s.old_activation.max(0.0);
                old_a / (old_a + 0.25)
            })
            .unwrap_or(0.0);
        let like_x = if song.liked { 1.0 } else { 0.0 };
        let ref_weight: f64 = referrers
            .get(&song.id)
            .map(|refs| {
                refs.iter()
                    .map(|sid| {
                        let a = song_stats.get(sid).map(|s| s.activation).unwrap_or(0.0);
                        0.5 + 0.5 * (a / (1.0 + a.abs())).max(0.0)
                    })
                    .sum()
            })
            .unwrap_or(0.0);
        let x_seed = ref_weight / (1.0 + ref_weight);
        let art_strength = art.map(|a| a.strength).unwrap_or(0.0);
        let x_art = art_strength / (art_strength + 2.0);
        let song_new = st.is_none_or(|s| s.good_listens == 0);
        let artist_new = art.is_none_or(|a| a.good_listens == 0);
        let x_novel = match (song_new, artist_new) {
            (true, true) => 1.0,
            (true, false) => 0.5,
            _ => 0.0,
        };
        let imp_count = impressions.get(&song.id).copied().unwrap_or(0) as f64;
        let x_imp = 1.0 - (-imp_count / 3.0).exp();
        let x_ctx = art
            .map(|a| {
                let good_win: u64 = a.good_by_bucket.iter().sum();
                if good_win == 0 {
                    0.0
                } else {
                    let rate = (a.good_by_bucket[current_bucket] as f64 + 4.0 * bucket_share)
                        / (good_win as f64 + 4.0);
                    rate - bucket_share
                }
            })
            .unwrap_or(0.0);
        let x_over = art
            .filter(|a| a.positive_long > 0.0)
            .map(|a| {
                let s7 = if total_short > 0.0 { a.positive_short / total_short } else { 0.0 };
                let s90 = if total_long > 0.0 { a.positive_long / total_long } else { 0.0 };
                (s7 - s90).max(0.0)
            })
            .unwrap_or(0.0);
        let tags = extract_title_tags(&song.title);
        let x_fit = if recent_tags.is_empty() && tags.is_empty() {
            0.0
        } else if !recent_tags.is_empty() && tags.iter().any(|t| recent_tags.contains(t)) {
            0.6
        } else if recent_tags.is_empty() && !tags.is_empty() {
            -0.5
        } else {
            0.0
        };

        1.0 * act
            - 0.8 * sat
            + 0.5 * dorm
            + 0.3 * like_x
            + 1.35 * x_seed
            + 0.45 * x_art
            + 0.35 * x_novel
            - 1.0 * x_imp
            + 0.55 * x_ctx
            - 1.0 * x_over
            + 0.6 * x_fit
    };

    let is_eligible = |song: &Track| -> bool {
        if is_preview_title(&song.title) {
            return false;
        }
        let art = song.primary_artist().trim().to_lowercase();
        if !art.is_empty() && disliked_artists.contains(&art) {
            return false;
        }
        let gk = base_version_key(&song.title, song.primary_artist());
        !fresh_excluded_groups.contains(&gk)
    };

    let seed_title_for = |song_id: &str| -> Option<String> {
        referrers
            .get(song_id)
            .and_then(|refs| refs.first())
            .and_then(|sid| songs.get(sid))
            .map(|s| s.title.clone())
    };

    // Build per-lane sorted candidate pools.
    let mut related_pool = Vec::new();
    let mut explore_pool = Vec::new();
    let mut again_pool = Vec::new();
    let mut artist_pool = Vec::new();
    let mut rediscover_pool = Vec::new();

    let again_cut = now_ms - AGAIN_WINDOW_DAYS * 86_400_000;
    let artist_quiet_cut = now_ms - AGAIN_WINDOW_DAYS * 86_400_000;

    for (id, song) in songs {
        if !is_eligible(song) {
            continue;
        }
        let z = score_song(song);
        let st = song_stats.get(id);
        let art_key = song.primary_artist().trim().to_lowercase();
        let art_strength = artist_stats.get(&art_key).map(|a| a.strength).unwrap_or(0.0);

        if referrers.contains_key(id) && !seed_groups.contains(&base_version_key(&song.title, song.primary_artist())) {
            let never_heard = st.is_none_or(|s| s.good_listens == 0);
            let card = EngineCard {
                track: song.clone(),
                lane: if never_heard {
                    EngineLane::Explore
                } else {
                    EngineLane::Related
                },
                score: z,
                seed_title: seed_title_for(id),
            };
            if never_heard {
                explore_pool.push(card.clone());
            }
            related_pool.push(card);
        }

        if let Some(s) = st {
            if s.good_listens > 0 && s.last_good_at >= again_cut && s.activation > 0.1 {
                again_pool.push(EngineCard {
                    track: song.clone(),
                    lane: EngineLane::Again,
                    score: z,
                    seed_title: None,
                });
            }
            if s.old_plays >= 3 && s.old_play_secs > 0 && (s.recent_play_secs as f64) < 0.2 * (s.old_play_secs as f64) {
                rediscover_pool.push(EngineCard {
                    track: song.clone(),
                    lane: EngineLane::Rediscover,
                    score: z + 0.5,
                    seed_title: None,
                });
            }
        }

        if art_strength > 0.5 && st.is_none_or(|s| s.last_good_at < artist_quiet_cut) {
            artist_pool.push(EngineCard {
                track: song.clone(),
                lane: EngineLane::Artist,
                score: z,
                seed_title: None,
            });
        }
    }

    for pool in [
        &mut related_pool,
        &mut explore_pool,
        &mut again_pool,
        &mut artist_pool,
        &mut rediscover_pool,
    ] {
        pool.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
    }

    // 4/5-Lane balanced Quick Picks assembly enforcing version uniqueness and artist/seed caps.
    let mut quick_picks = Vec::new();
    let mut taken_groups: HashSet<String> = HashSet::new();
    let mut per_artist: HashMap<String, usize> = HashMap::new();
    let mut per_seed: HashMap<String, usize> = HashMap::new();

    let quotas: Vec<(EngineLane, usize)> = EngineLane::quota_weights()
        .into_iter()
        .map(|(lane, w)| (lane, ((w * desired_picks as f64).round() as usize).max(1)))
        .collect();

    let take_from_pool = |pool: &[EngineCard],
                          count: usize,
                          offset: usize,
                          max_per_artist: usize,
                          quick_picks: &mut Vec<EngineCard>,
                          taken_groups: &mut HashSet<String>,
                          per_artist: &mut HashMap<String, usize>,
                          per_seed: &mut HashMap<String, usize>| {
        if pool.is_empty() || count == 0 {
            return;
        }
        let mut added = 0usize;
        let len = pool.len();
        let start = if len > count * 2 {
            offset % (len / 2).max(1)
        } else {
            0
        };
        for step in 0..len {
            if added >= count || quick_picks.len() >= desired_picks {
                break;
            }
            let c = &pool[(start + step) % len];
            let gk = base_version_key(&c.track.title, c.track.primary_artist());
            if taken_groups.contains(&gk) {
                continue;
            }
            let art = c.track.primary_artist().trim().to_lowercase();
            if !art.is_empty() && *per_artist.get(&art).unwrap_or(&0) >= max_per_artist {
                continue;
            }
            if let Some(stitle) = &c.seed_title {
                if *per_seed.get(stitle).unwrap_or(&0) >= MAX_PER_SEED {
                    continue;
                }
            }
            taken_groups.insert(gk);
            if !art.is_empty() {
                *per_artist.entry(art).or_insert(0) += 1;
            }
            if let Some(stitle) = &c.seed_title {
                *per_seed.entry(stitle.clone()).or_insert(0) += 1;
            }
            quick_picks.push(c.clone());
            added += 1;
        }
    };

    for (lane, want) in &quotas {
        let pool = match lane {
            EngineLane::Related => &related_pool,
            EngineLane::Again => &again_pool,
            EngineLane::Artist => &artist_pool,
            EngineLane::Rediscover => &rediscover_pool,
            EngineLane::Explore => &explore_pool,
        };
        take_from_pool(
            pool,
            *want,
            rotation_cursor,
            1,
            &mut quick_picks,
            &mut taken_groups,
            &mut per_artist,
            &mut per_seed,
        );
    }

    // Backfill remaining slots from combined scored pool enforcing artist diversity first.
    for pool in [&related_pool, &again_pool, &artist_pool, &explore_pool] {
        if quick_picks.len() >= desired_picks {
            break;
        }
        take_from_pool(
            pool,
            desired_picks - quick_picks.len(),
            0,
            1,
            &mut quick_picks,
            &mut taken_groups,
            &mut per_artist,
            &mut per_seed,
        );
    }
    // Final backfill relaxing artist cap if library has few distinct artists
    if quick_picks.len() < desired_picks {
        for pool in [&related_pool, &again_pool, &artist_pool, &explore_pool] {
            if quick_picks.len() >= desired_picks {
                break;
            }
            take_from_pool(
                pool,
                desired_picks - quick_picks.len(),
                0,
                MAX_PER_ARTIST,
                &mut quick_picks,
                &mut taken_groups,
                &mut per_artist,
                &mut per_seed,
            );
        }
    }

    let discover: Vec<Track> = if !explore_pool.is_empty() {
        explore_pool.into_iter().map(|c| c.track).take(14).collect()
    } else {
        related_pool.iter().map(|c| c.track.clone()).take(14).collect()
    };

    let mut heavy: Vec<(&String, &SongStats)> = song_stats
        .iter()
        .filter(|(id, st)| {
            st.good_listens > 0
                && st.activation > 0.1
                && songs
                    .get(*id)
                    .is_some_and(|t| !disliked_artists.contains(&t.primary_artist().trim().to_lowercase()))
        })
        .collect();
    heavy.sort_by(|a, b| {
        b.1.activation
            .partial_cmp(&a.1.activation)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    let heavy_rotation: Vec<Track> = heavy
        .into_iter()
        .filter_map(|(id, _)| songs.get(id).cloned())
        .take(14)
        .collect();

    let forgotten_favorites: Vec<Track> = rediscover_pool
        .into_iter()
        .map(|c| c.track)
        .take(14)
        .collect();

    EngineOutput {
        seeds,
        uncached_seed_ids,
        quick_picks,
        discover,
        heavy_rotation,
        forgotten_favorites,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::ArtistRef;

    fn song(id: &str, title: &str, artist: &str, liked: bool) -> Track {
        Track {
            id: id.to_string(),
            title: title.to_string(),
            artists: vec![ArtistRef {
                id: Some(format!("AR_{artist}")),
                name: artist.to_string(),
            }],
            album: None,
            duration_secs: Some(200),
            thumbnail_url: None,
            loudness_db: None,
            liked,
            is_local: false,
            local_path: None,
        }
    }

    #[test]
    fn multi_seed_edges_forgotten_favorites_and_version_dedup_work() {
        let now = 1_750_000_000_000i64;
        let old_ts = now - 40 * 86_400_000;
        let recent_ts = now - 3 * 3_600_000;

        let mut songs = HashMap::new();
        for (id, title, artist, liked) in [
            ("s1", "Hope", "NF", true),
            ("s2", "The Search", "NF", true),
            ("s3", "Clouds", "NF", false),
            ("rel_multi", "No Excuses", "NF", false),
            ("rel_single", "Random One", "Someone", false),
            ("rel_dup", "No Excuses (Slowed)", "NF", false),
            ("old_fav", "Remember When", "Logic", true),
        ] {
            songs.insert(id.to_string(), song(id, title, artist, liked));
        }

        let listens = vec![
            ListenRecord {
                song_id: "s1".to_string(),
                played_at_ms: recent_ts,
                duration_played_secs: 195,
            },
            ListenRecord {
                song_id: "s2".to_string(),
                played_at_ms: recent_ts + 250_000,
                duration_played_secs: 190,
            },
            // Played 3+ times >30 days ago and stopped recently -> Forgotten Favorite.
            ListenRecord {
                song_id: "old_fav".to_string(),
                played_at_ms: old_ts,
                duration_played_secs: 190,
            },
            ListenRecord {
                song_id: "old_fav".to_string(),
                played_at_ms: old_ts + 400_000,
                duration_played_secs: 185,
            },
            ListenRecord {
                song_id: "old_fav".to_string(),
                played_at_ms: old_ts + 800_000,
                duration_played_secs: 195,
            },
        ];

        let edges = vec![
            RelatedEdge {
                seed_id: "s1".to_string(),
                related_id: "rel_multi".to_string(),
                source: 0,
                updated_at_ms: now,
            },
            RelatedEdge {
                seed_id: "s2".to_string(),
                related_id: "rel_multi".to_string(),
                source: 0,
                updated_at_ms: now,
            },
            RelatedEdge {
                seed_id: "s1".to_string(),
                related_id: "rel_dup".to_string(),
                source: 0,
                updated_at_ms: now,
            },
            RelatedEdge {
                seed_id: "s3".to_string(),
                related_id: "rel_single".to_string(),
                source: 0,
                updated_at_ms: now,
            },
        ];

        let out = build_recommendations(
            now,
            &songs,
            &listens,
            &edges,
            &HashMap::new(),
            &HashSet::new(),
            12,
            0,
        );

        assert!(!out.seeds.is_empty());
        assert!(out.forgotten_favorites.iter().any(|t| t.id == "old_fav"));
        assert!(out.discover.first().is_some_and(|t| t.id == "rel_multi"));
        // Version dedup keeps only one of "No Excuses" / "No Excuses (Slowed)" in quick_picks.
        let excuses_count = out
            .quick_picks
            .iter()
            .filter(|c| c.track.title.starts_with("No Excuses"))
            .count();
        assert!(excuses_count <= 1);
    }
}
