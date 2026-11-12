use echo_audio::PlaybackStatus;
use echo_core::Track;
use souvlaki::{
    MediaControlEvent, MediaControls, MediaMetadata, MediaPlayback, MediaPosition, PlatformConfig,
    SeekDirection,
};
use std::ffi::c_void;
use std::sync::mpsc;
use std::time::{Duration, Instant};

#[derive(Debug, Clone, Copy)]
pub enum SmtcAction {
    Play,
    Pause,
    Toggle,
    Next,
    Previous,
    Stop,
    SeekForward,
    SeekBackward,
    SetPositionMs(u64),
}

pub struct SmtcBridge {
    controls: MediaControls,
    rx: mpsc::Receiver<MediaControlEvent>,
    last_track_id: Option<String>,
    last_state_key: Option<(bool, bool)>,
    last_pos_sync: Instant,
}

impl SmtcBridge {
    pub fn new(hwnd: Option<isize>) -> Option<Self> {
        #[cfg(target_os = "windows")]
        let hwnd_ptr = Some(hwnd? as *mut c_void);
        #[cfg(not(target_os = "windows"))]
        let hwnd_ptr = hwnd.map(|h| h as *mut c_void);

        let config = PlatformConfig {
            dbus_name: "io.github.echo_desktop.Echo",
            display_name: "Echo",
            hwnd: hwnd_ptr,
        };
        let mut controls = MediaControls::new(config).ok()?;
        let (tx, rx) = mpsc::channel();
        controls
            .attach(move |event| {
                let _ = tx.send(event);
            })
            .ok()?;

        Some(Self {
            controls,
            rx,
            last_track_id: None,
            last_state_key: None,
            last_pos_sync: Instant::now(),
        })
    }

    pub fn sync(
        &mut self,
        track: Option<&Track>,
        status: &PlaybackStatus,
        position_ms: u64,
        duration_ms: u64,
    ) {
        let track_id = track.map(|t| t.id.clone());
        if track_id != self.last_track_id {
            self.last_track_id = track_id;
            if let Some(t) = track {
                let artist = t.artist_display();
                let album = t.album.as_ref().map(|a| a.title.as_str());
                let dur = t
                    .duration_secs
                    .map(|s| Duration::from_secs(s as u64))
                    .or_else(|| (duration_ms > 0).then(|| Duration::from_millis(duration_ms)));
                let cover = t.thumbnail_url.as_deref().map(|u| {
                    if u.starts_with("http://") || u.starts_with("https://") || u.starts_with("file://") {
                        u.to_string()
                    } else {
                        let normalized = u.replace('\\', "/");
                        if normalized.starts_with('/') {
                            format!("file://{normalized}")
                        } else {
                            format!("file:///{normalized}")
                        }
                    }
                });
                let _ = self.controls.set_metadata(MediaMetadata {
                    title: Some(&t.title),
                    artist: Some(&artist),
                    album,
                    cover_url: cover.as_deref(),
                    duration: dur,
                });
            } else {
                let _ = self.controls.set_metadata(MediaMetadata::default());
            }
        }

        let is_playing = matches!(status, PlaybackStatus::Playing);
        let has_track = track.is_some();
        let state_key = Some((has_track, is_playing));
        let now = Instant::now();
        if state_key != self.last_state_key
            || (is_playing && now.duration_since(self.last_pos_sync) >= Duration::from_secs(2))
        {
            self.last_state_key = state_key;
            self.last_pos_sync = now;
            let progress = Some(MediaPosition(Duration::from_millis(position_ms)));
            let playback = if !has_track || matches!(status, PlaybackStatus::Stopped) {
                MediaPlayback::Stopped
            } else if is_playing {
                MediaPlayback::Playing { progress }
            } else {
                MediaPlayback::Paused { progress }
            };
            let _ = self.controls.set_playback(playback);
        }
    }

    pub fn poll_actions(&self) -> Vec<SmtcAction> {
        let mut out = Vec::new();
        while let Ok(ev) = self.rx.try_recv() {
            match ev {
                MediaControlEvent::Play => out.push(SmtcAction::Play),
                MediaControlEvent::Pause => out.push(SmtcAction::Pause),
                MediaControlEvent::Toggle => out.push(SmtcAction::Toggle),
                MediaControlEvent::Next => out.push(SmtcAction::Next),
                MediaControlEvent::Previous => out.push(SmtcAction::Previous),
                MediaControlEvent::Stop => out.push(SmtcAction::Stop),
                MediaControlEvent::Seek(SeekDirection::Forward)
                | MediaControlEvent::SeekBy(SeekDirection::Forward, _) => {
                    out.push(SmtcAction::SeekForward);
                }
                MediaControlEvent::Seek(SeekDirection::Backward)
                | MediaControlEvent::SeekBy(SeekDirection::Backward, _) => {
                    out.push(SmtcAction::SeekBackward);
                }
                MediaControlEvent::SetPosition(pos) => {
                    out.push(SmtcAction::SetPositionMs(pos.0.as_millis() as u64));
                }
                _ => {}
            }
        }
        out
    }
}

