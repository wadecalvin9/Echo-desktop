pub mod dsp;
pub mod engine;
pub mod normalization;
pub mod source;

pub use dsp::{is_silent_packet, Equalizer, SpatialProcessor, EQ_BANDS_HZ};
pub use engine::{AudioEngine, AudioSourceSpec, PlaybackSnapshot, PlaybackStatus};
pub use normalization::{
    apply_gain_and_soft_limit, compute_normalization_gain, NORMALIZATION_REFERENCE_DB,
    UNKNOWN_LOUDNESS_DB,
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalization_matches_echo_constants() {
        // Off -> unity
        assert_eq!(compute_normalization_gain(false, Some(6.0), false), 1.0);
        // Local without tag -> unity
        assert_eq!(compute_normalization_gain(true, None, true), 1.0);
        // Unknown YouTube -> attenuated by 10 - (-3) = 13 dB
        let unk = compute_normalization_gain(true, None, false);
        assert!((unk - 10.0f32.powf(-13.0 / 20.0)).abs() < 1e-4);
        // Very quiet track (-10 dB) is never boosted above 1.0
        assert_eq!(compute_normalization_gain(true, Some(-10.0), false), 1.0);
    }

    #[test]
    fn soft_limiter_prevents_hard_clipping_under_boost() {
        let boosted = apply_gain_and_soft_limit(0.8, 4.0);
        assert!(boosted > 0.92 && boosted <= 1.0);
    }

    #[test]
    #[ignore]
    fn live_probe_decode_youtube_stream() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        let yt = echo_youtube::YouTubeMusicClient::new();
        let (home_tracks, stream) = rt.block_on(async {
            let secs = yt.home().await.unwrap();
            let tracks: Vec<echo_core::Track> = secs
                .into_iter()
                .flat_map(|s| s.items)
                .filter_map(|i| match i {
                    echo_youtube::BrowseItem::Track(t) => Some(t),
                    _ => None,
                })
                .take(3)
                .collect();
            let first_id = tracks.first().map(|t| t.id.clone()).unwrap_or_else(|| "lYBUbBu4W08".to_string());
            let stream = yt.resolve_stream(&first_id, echo_core::AudioQuality::Auto).await.unwrap();
            (tracks, stream)
        });
        println!("home tracks: {:?}", home_tracks.iter().map(|t| (&t.id, &t.title)).collect::<Vec<_>>());
        println!("resolved url client={} mime={} len={:?}", stream.client_name, stream.mime_type, stream.content_length);

        let engine = AudioEngine::new(1.0, true);
        engine.play(AudioSourceSpec::Url {
            url: stream.url,
            user_agent: stream.user_agent,
            mime_hint: Some(stream.mime_type),
            content_length: stream.content_length,
            approx_duration_ms: stream.approx_duration_ms,
            loudness_db: stream.loudness_db,
        });

        for i in 0..15 {
            std::thread::sleep(std::time::Duration::from_millis(200));
            let snap = engine.snapshot();
            println!("t={}ms status={:?} pos={}ms dur={}ms", i * 200, snap.status, snap.position_ms, snap.duration_ms);
            if matches!(snap.status, PlaybackStatus::Error(_)) {
                panic!("AudioEngine failed: {:?}", snap.status);
            }
        }
        assert!(matches!(engine.snapshot().status, PlaybackStatus::Playing));
    }

    #[test]
    fn dsp_equalizer_spatial_and_silence_gate_work() {
        let quiet = vec![0.0005f32; 512];
        assert!(is_silent_packet(&quiet));
        let loud = vec![0.25f32; 512];
        assert!(!is_silent_packet(&loud));

        let mut eq = Equalizer::new(48_000, 2, echo_core::EqPreset::BassBoost);
        let mut buf: Vec<f32> = (0..256)
            .flat_map(|i| {
                let s = (i as f32 * 0.08).sin() * 0.3;
                [s, s]
            })
            .collect();
        let before_energy: f32 = buf.iter().map(|x| x * x).sum();
        eq.process_interleaved(&mut buf, 2);
        let after_energy: f32 = buf.iter().map(|x| x * x).sum();
        assert!(after_energy.is_finite() && after_energy > 0.0);
        assert!((after_energy - before_energy).abs() > 0.01);

        let mut sp = SpatialProcessor::new(48_000, echo_core::SpatialMode::Headphones);
        sp.process_interleaved(&mut buf, 2);
        assert!(buf.iter().all(|s| s.is_finite()));
    }
}
