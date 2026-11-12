pub mod api;
pub mod client;
pub mod models;
pub mod sabr;

pub use api::{extract_effective_loudness_db, YouTubeError, YouTubeMusicClient};
pub use client::{YouTubeClientProfile, YouTubeLocale};
pub use models::{
    AccountPlaylist, AlbumPage, ArtistPage, ArtistSection, BrowseItem, ExplorePage, HomeChip,
    HomePage, HomeSection, MoodGenreButton, MoodGenreGroup, PlaylistPage, RelatedPage,
    ResolvedStream, SearchFilter, SearchResultPage, StreamAddressCache,
};
pub use sabr::ChunkBuffer;

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn effective_loudness_uses_perceptual_fallback() {
        let old = json!({ "playerConfig": { "audioConfig": { "loudnessDb": 6.27 } } });
        assert!((extract_effective_loudness_db(&old).unwrap() - 6.27).abs() < 0.01);

        let modern = json!({
            "playerConfig": {
                "audioConfig": {
                    "perceptualLoudnessDb": -7.73,
                    "loudnessTargetLkfs": -14.0
                }
            }
        });
        assert!((extract_effective_loudness_db(&modern).unwrap() - 6.27).abs() < 0.01);
    }

    #[test]
    fn chunk_buffer_reads_contiguous_ranges() {
        let mut buf = ChunkBuffer::new(Some(1024), 512 * 1024);
        buf.insert(0, vec![1, 2, 3, 4], 0);
        buf.insert(4, vec![5, 6, 7, 8], 0);
        let mut out = [0u8; 6];
        let n = buf.read_at(2, &mut out);
        assert_eq!(n, 6);
        assert_eq!(out, [3, 4, 5, 6, 7, 8]);
    }

    #[tokio::test]
    #[ignore]
    async fn live_probe_playback_stream() {
        let yt = YouTubeMusicClient::new();
        let home = yt.home().await;
        println!("home ok={}", home.is_ok());
        if let Ok(secs) = &home {
            for s in secs.iter().take(2) {
                println!("section: {} ({} items)", s.title, s.items.len());
            }
        }
        let vd = yt.ensure_visitor_data().await;
        println!("visitor_data from sw.js_data: {:?}", vd);

        let search = yt.search("Never Gonna Give You Up Rick Astley", SearchFilter::Songs).await;
        println!("search len: {:?}", search.as_ref().map(|v| v.len()));
        let vid = search
            .ok()
            .and_then(|v| v.into_iter().find_map(|i| match i {
                BrowseItem::Track(t) => Some(t.id),
                _ => None,
            }))
            .unwrap_or_else(|| "dQw4w9WgXcQ".to_string());
        println!("probing video_id={vid}");

        let res = yt.resolve_stream(&vid, echo_core::AudioQuality::Auto).await;
        println!("resolve_stream result: {:?}", res);
    }
}
