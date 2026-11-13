use iced::Task;
use std::time::{Duration, Instant};

use echo_audio::PlaybackStatus;
use echo_core::{
    prune_queue_tail, AutoEqProfile, EqPreset,
};
use echo_lastfm::{qualifies_for_scrobble, LastFmClient};
use echo_youtube::{
    BrowseItem, SearchFilter,
};

use crate::smtc;
use crate::tray;
use crate::types::*;
use crate::EchoApp;

impl EchoApp {
    pub(crate) fn update_playback(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::AnimTick => {
                self.anim_phase = (self.anim_phase + 0.35) % std::f32::consts::TAU;
                if matches!(self.status, PlaybackStatus::Playing) {
                    self.spectrum_bands = self.audio.spectrum_bands();
                    if self.seeking_ms.is_none() {
                        let snap = self.audio.snapshot();
                        self.position_ms = snap.position_ms;
                        if snap.duration_ms > 0 {
                            self.duration_ms = snap.duration_ms;
                        }
                        return self.sync_lyrics_scroll();
                    }
                } else {
                    for b in &mut self.spectrum_bands {
                        *b *= 0.65;
                    }
                }
                Task::none()
            }
            Message::Tick => {
                let now = Instant::now();
                let dt_ms = now.duration_since(self.last_tick).as_millis() as u64;
                self.last_tick = now;

                let mut extra_tasks = Vec::new();
                if !self.smtc_requested {
                    self.smtc_requested = true;
                    extra_tasks.push(iced::window::get_oldest().then(|id_opt| match id_opt {
                        Some(id) => iced::window::run_with_handle(id, |h| {
                            use iced::window::raw_window_handle::RawWindowHandle;
                            match h.as_raw() {
                                RawWindowHandle::Win32(w) => {
                                    Message::WindowHandleReady(Some(w.hwnd.get()))
                                }
                                _ => Message::WindowHandleReady(None),
                            }
                        }),
                        None => Task::done(Message::WindowHandleReady(None)),
                    }));
                }

                if let Some(ref mut smtc) = self.smtc {
                    let actions = smtc.poll_actions();
                    let cur_track = self.queue.current().map(|c| c.track.clone());
                    smtc.sync(
                        cur_track.as_ref(),
                        &self.status,
                        self.position_ms,
                        self.duration_ms,
                    );
                    for act in actions {
                        match act {
                            smtc::SmtcAction::Play => self.audio.resume(),
                            smtc::SmtcAction::Pause => self.audio.pause(),
                            smtc::SmtcAction::Toggle => {
                                extra_tasks.push(self.update(Message::TogglePlayPause));
                            }
                            smtc::SmtcAction::Next => {
                                extra_tasks.push(self.update(Message::NextTrack));
                            }
                            smtc::SmtcAction::Previous => {
                                extra_tasks.push(self.update(Message::PrevTrack));
                            }
                            smtc::SmtcAction::Stop => self.audio.stop(),
                            smtc::SmtcAction::SeekForward => {
                                let t = (self.position_ms + 5_000).min(self.duration_ms.max(1));
                                extra_tasks.push(self.update(Message::SeekToMs(t)));
                            }
                            smtc::SmtcAction::SeekBackward => {
                                let t = self.position_ms.saturating_sub(5_000);
                                extra_tasks.push(self.update(Message::SeekToMs(t)));
                            }
                            smtc::SmtcAction::SetPositionMs(ms) => {
                                extra_tasks.push(self.update(Message::SeekToMs(ms)));
                            }
                        }
                    }
                }

                if let Some(ref mut tray) = self.tray {
                    let actions = tray.poll_actions();
                    let cur = self.queue.current();
                    let is_playing = matches!(self.status, PlaybackStatus::Playing);
                    tray.sync(
                        cur.map(|c| c.track.title.as_str()),
                        cur.map(|c| c.track.artist_display()).as_deref(),
                        is_playing,
                    );
                    for act in actions {
                        match act {
                            tray::TrayAction::PlayPause => {
                                extra_tasks.push(self.update(Message::TogglePlayPause));
                            }
                            tray::TrayAction::Next => {
                                extra_tasks.push(self.update(Message::NextTrack));
                            }
                            tray::TrayAction::Previous => {
                                extra_tasks.push(self.update(Message::PrevTrack));
                            }
                            tray::TrayAction::RestoreWindow => {
                                if let Some(id) = self.main_window_id {
                                    extra_tasks.push(iced::window::minimize(id, false));
                                } else {
                                    extra_tasks.push(iced::window::get_oldest().then(|opt| match opt {
                                        Some(id) => iced::window::minimize(id, false),
                                        None => Task::none(),
                                    }));
                                }
                            }
                            tray::TrayAction::Exit => {
                                self.flush_active_play_duration();
                                if let Some(id) = self.main_window_id {
                                    extra_tasks.push(iced::window::close(id));
                                } else {
                                    extra_tasks.push(iced::window::get_oldest().then(|opt| match opt {
                                        Some(id) => iced::window::close(id),
                                        None => Task::none(),
                                    }));
                                }
                            }
                        }
                    }
                }

                let snap = self.audio.snapshot();
                if let PlaybackStatus::Error(ref err) = snap.status {
                    if !matches!(&self.status, PlaybackStatus::Error(prev) if prev == err) {
                        if let Some(cur) = self.queue.current() {
                            self.yt.invalidate_stream(&cur.track.id);
                            let is_stream_err = err.contains("403")
                                || err.contains("Forbidden")
                                || err.contains("stream open failed")
                                || err.contains("probe failed")
                                || err.contains("source read error")
                                || err.contains("broken pipe")
                                || err.contains("connection reset");
                            if !cur.track.is_local && is_stream_err && self.stream_retry_count < 2 {
                                self.stream_retry_count += 1;
                                let resume_pos = if self.position_ms > 0 {
                                    Some(self.position_ms)
                                } else {
                                    self.seeking_ms
                                };
                                self.seeking_ms = resume_pos;
                                self.set_toast(format!("Stream expired — refreshing audio (attempt {}/2)...", self.stream_retry_count));
                                extra_tasks.push(self.start_playing_current());
                            } else {
                                self.set_toast(format!("Audio error: {err}"));
                            }
                        } else {
                            self.set_toast(format!("Audio error: {err}"));
                        }
                    }
                }
                if let Some((_, shown_at)) = &self.toast {
                    if now.duration_since(*shown_at) >= Duration::from_secs(4) {
                        self.toast = None;
                    }
                }
                self.status = snap.status;
                if self.seeking_ms.is_none() {
                    self.position_ms = snap.position_ms;
                }
                if snap.duration_ms > 0 {
                    self.duration_ms = snap.duration_ms;
                }

                if let Some(ref mut taskbar) = self.taskbar {
                    let is_playing = matches!(self.status, PlaybackStatus::Playing);
                    let is_paused = matches!(self.status, PlaybackStatus::Paused);
                    taskbar.update(is_playing, is_paused, self.position_ms, self.duration_ms);
                }

                // Auto-scroll synced lyrics when the active line advances.
                let scroll_task = self.sync_lyrics_scroll();

                if matches!(self.status, PlaybackStatus::Playing) {
                    self.stream_retry_count = 0;
                    if let Some(tr) = self.scrobble_tracker.as_mut() {
                        tr.accumulated_ms += dt_ms.min(1000);
                        let dur = tr
                            .track
                            .duration_secs
                            .unwrap_or((self.duration_ms / 1000) as u32);
                        if !tr.scrobbled && qualifies_for_scrobble(tr.accumulated_ms, dur) {
                            tr.scrobbled = true;
                            let track_copy = tr.track.clone();
                            let started = tr.started_at_secs;
                            let _ = self.db.update_latest_play_duration(
                                &track_copy.id,
                                (tr.accumulated_ms / 1000) as u32,
                            );
                            self.refresh_db_views();

                            if self.settings.lastfm_scrobbling_enabled {
                                if let Some(sk) = self.settings.lastfm_session_key.clone() {
                                    let client = LastFmClient::new(
                                        self.settings.lastfm_api_key.clone(),
                                        self.settings.lastfm_api_secret.clone(),
                                    );
                                    if client.is_configured() {
                                        let db = self.db.clone();
                                        tokio::spawn(async move {
                                            let res = client
                                                .scrobble(
                                                    &sk,
                                                    track_copy.primary_artist(),
                                                    &track_copy.title,
                                                    started,
                                                    track_copy.album.as_ref().map(|a| a.title.as_str()),
                                                    Some(dur),
                                                )
                                                .await;
                                            if res.is_err() {
                                                let _ = db.enqueue_pending_scrobble(
                                                    track_copy.primary_artist(),
                                                    &track_copy.title,
                                                    track_copy.album.as_ref().map(|a| a.title.as_str()),
                                                    Some(dur),
                                                    started,
                                                );
                                            }
                                        });
                                    }
                                }
                            }
                        }
                    }
                }

                // Sleep timer + Transition fade gain envelope
                let mut sleep_factor = 1.0f32;
                let mut sleep_triggered = false;
                match self.sleep_timer {
                    SleepTimerState::Off => {}
                    SleepTimerState::Countdown(deadline) => {
                        if now >= deadline {
                            self.sleep_timer = SleepTimerState::Off;
                            sleep_triggered = true;
                        } else {
                            let rem_ms = deadline.duration_since(now).as_millis() as u64;
                            if rem_ms < 10_000 {
                                sleep_factor = (rem_ms as f32 / 10_000.0).clamp(0.0, 1.0);
                            }
                        }
                    }
                    SleepTimerState::EndOfSong => {
                        if self.duration_ms > 10_000 && matches!(self.status, PlaybackStatus::Playing) {
                            let rem_ms = self.duration_ms.saturating_sub(self.position_ms);
                            if rem_ms < 10_000 {
                                sleep_factor = (rem_ms as f32 / 10_000.0).clamp(0.0, 1.0);
                            }
                        }
                    }
                }
                if sleep_triggered {
                    self.audio.pause();
                    self.audio.set_fade_factor(1.0);
                    self.fade_in_remaining_ms = 0;
                    self.set_toast("Sleep timer paused playback");
                } else {
                    let fade_len = self.settings.transition_fade_ms;
                    let mut transition_factor = 1.0f32;
                    if fade_len > 0 && matches!(self.status, PlaybackStatus::Playing) {
                        if self.fade_in_remaining_ms > 0 {
                            let elapsed_in = fade_len.saturating_sub(self.fade_in_remaining_ms);
                            transition_factor = (elapsed_in as f32 / fade_len as f32).clamp(0.0, 1.0);
                            self.fade_in_remaining_ms =
                                self.fade_in_remaining_ms.saturating_sub(dt_ms);
                        } else if self.duration_ms > fade_len * 2 {
                            let rem_ms = self.duration_ms.saturating_sub(self.position_ms);
                            if rem_ms < fade_len {
                                transition_factor = (rem_ms as f32 / fade_len as f32).clamp(0.0, 1.0);
                            }
                        }
                    } else {
                        self.fade_in_remaining_ms = 0;
                    }
                    self.audio
                        .set_fade_factor((sleep_factor * transition_factor).clamp(0.0, 1.0));
                }

                extra_tasks.push(scroll_task);
                if snap.finished_seq != self.last_finished_seq {
                    self.last_finished_seq = snap.finished_seq;
                    if matches!(self.sleep_timer, SleepTimerState::EndOfSong) {
                        self.sleep_timer = SleepTimerState::Off;
                        self.audio.set_fade_factor(1.0);
                        self.fade_in_remaining_ms = 0;
                        self.set_toast("Sleep timer stopped after song");
                        return Task::batch(extra_tasks);
                    }
                    if self.queue.advance_next().is_some() {
                        if self.settings.transition_fade_ms > 0 {
                            self.fade_in_remaining_ms = self.settings.transition_fade_ms;
                        }
                        extra_tasks.push(self.start_playing_current());
                        return Task::batch(extra_tasks);
                    } else if self.settings.auto_load_more {
                        if let Some(cur) = self.queue.current().map(|c| c.track.clone()) {
                            let yt = self.yt.clone();
                            extra_tasks.push(Task::perform(
                                async move {
                                    yt.next_radio(&cur.id, None)
                                        .await
                                        .map_err(|e| e.to_string())
                                },
                                Message::RadioLoaded,
                            ));
                            return Task::batch(extra_tasks);
                        }
                    }
                }
                Task::batch(extra_tasks)
            }
            Message::PlayTrack(track) => {
                self.stream_retry_count = 0;
                self.queue.replace_with(vec![track.clone()], 0);
                let mut tasks = vec![self.start_playing_current()];
                if self.settings.auto_load_more && !track.is_local {
                    let yt = self.yt.clone();
                    let vid = track.id.clone();
                    tasks.push(Task::perform(
                        async move { yt.next_radio(&vid, None).await.map_err(|e| e.to_string()) },
                        Message::RadioLoaded,
                    ));
                    let lfm = LastFmClient::new(
                        self.settings.lastfm_api_key.clone(),
                        self.settings.lastfm_api_secret.clone(),
                    );
                    if lfm.is_configured() && !track.primary_artist().is_empty() {
                        let yt2 = self.yt.clone();
                        let artist = track.primary_artist().to_string();
                        let title = track.title.clone();
                        tasks.push(Task::perform(
                            async move {
                                let sim = lfm.similar(&artist, &title, 5).await.unwrap_or_default();
                                let mut resolved = Vec::new();
                                for s in sim.into_iter().take(3) {
                                    let q = format!(
                                        "{} {}",
                                        s.name,
                                        s.artist
                                            .as_ref()
                                            .and_then(|a| a.name.as_deref())
                                            .unwrap_or("")
                                    );
                                    if let Ok(items) = yt2.search(&q, SearchFilter::Songs).await {
                                        if let Some(t) = items.into_iter().find_map(|it| match it {
                                            BrowseItem::Track(tr) => Some(tr),
                                            _ => None,
                                        }) {
                                            resolved.push(t);
                                        }
                                    }
                                }
                                Ok(resolved)
                            },
                            Message::RadioLoaded,
                        ));
                    }
                }
                Task::batch(tasks)
            }
            Message::PlayTracks(tracks, idx) => {
                self.stream_retry_count = 0;
                self.queue.replace_with(tracks, idx);
                self.start_playing_current()
            }
            Message::QueuePlayNext(t) => {
                self.track_menu = None;
                let was_empty = self.queue.current().is_none();
                let thumb = t.thumbnail_url.clone();
                self.queue.insert_user_next(t);
                if was_empty {
                    self.start_playing_current()
                } else {
                    self.set_toast("Playing next");
                    self.fetch_cover_if_needed(thumb.as_deref())
                }
            }
            Message::QueueAddBack(t) => {
                self.track_menu = None;
                let was_empty = self.queue.current().is_none();
                let thumb = t.thumbnail_url.clone();
                self.queue.push_user(t);
                if was_empty {
                    self.start_playing_current()
                } else {
                    self.set_toast("Added to queue");
                    self.fetch_cover_if_needed(thumb.as_deref())
                }
            }
            Message::QueueJump(idx) => {
                if idx < self.queue.items.len() {
                    self.queue.current_index = Some(idx);
                    return self.start_playing_current();
                }
                Task::none()
            }
            Message::QueueRemove(idx) => {
                self.queue.remove_at(idx);
                Task::none()
            }
            Message::QueueMoveUp(idx) => {
                self.queue.move_up(idx);
                Task::none()
            }
            Message::QueueMoveDown(idx) => {
                self.queue.move_down(idx);
                Task::none()
            }
            Message::SaveQueueAsPlaylist => {
                if self.queue.items.is_empty() {
                    self.set_toast("Queue is empty");
                    return Task::none();
                }
                let pl_id = format!("local_queue_{}", chrono::Utc::now().timestamp_millis());
                let pl_name = format!("Queue ({})", chrono::Local::now().format("%b %d, %H:%M"));
                if self.db.create_playlist(&pl_id, &pl_name).is_ok() {
                    for item in &self.queue.items {
                        let _ = self.db.add_to_playlist(&pl_id, &item.track);
                    }
                    self.refresh_db_views();
                    self.set_toast(format!("Saved {} tracks as playlist: {}", self.queue.items.len(), pl_name));
                }
                Task::none()
            }
            Message::QueueClear => {
                self.flush_active_play_duration();
                self.queue.items.clear();
                self.queue.current_index = None;
                self.scrobble_tracker = None;
                self.audio.stop();
                Task::none()
            }
            Message::TogglePlayPause => {
                self.fade_in_remaining_ms = 0;
                self.audio.set_fade_factor(1.0);
                match self.status {
                    PlaybackStatus::Playing => self.audio.pause(),
                    PlaybackStatus::Paused => self.audio.resume(),
                    PlaybackStatus::Stopped | PlaybackStatus::Error(_) => {
                        if self.queue.current().is_some() {
                            return self.start_playing_current();
                        }
                    }
                    PlaybackStatus::Loading => {}
                }
                Task::none()
            }
            Message::NextTrack => {
                self.fade_in_remaining_ms = 0;
                self.audio.set_fade_factor(1.0);
                if let Some(cur) = self.queue.current().map(|c| c.track.clone()) {
                    let early = self.duration_ms > 20_000
                        && self.position_ms < (self.duration_ms * 35) / 100;
                    if early {
                        self.listener_ctx.record_early_skip(&cur);
                        prune_queue_tail(
                            &mut self.queue.items,
                            self.queue.current_index,
                            &self.listener_ctx,
                            true,
                        );
                        self.rebuild_home_sections();
                    }
                }
                if self.queue.advance_next().is_some() {
                    self.start_playing_current()
                } else {
                    Task::none()
                }
            }
            Message::PrevTrack => {
                self.fade_in_remaining_ms = 0;
                self.audio.set_fade_factor(1.0);
                if self.position_ms > 3000 {
                    self.audio.seek_ms(0);
                    Task::none()
                } else if self.queue.step_prev().is_some() {
                    self.start_playing_current()
                } else {
                    Task::none()
                }
            }
            Message::ToggleRepeat => {
                self.queue.repeat_mode = self.queue.repeat_mode.next();
                Task::none()
            }
            Message::ToggleShuffle => {
                self.queue.shuffle = !self.queue.shuffle;
                Task::none()
            }
            Message::SeekChanged(val) => {
                self.seeking_ms = Some(val as u64);
                Task::none()
            }
            Message::SeekReleased => {
                self.fade_in_remaining_ms = 0;
                self.audio.set_fade_factor(1.0);
                if let Some(ms) = self.seeking_ms.take() {
                    self.position_ms = ms;
                    self.audio.seek_ms(ms);
                    self.last_lyric_idx = None;
                    return self.sync_lyrics_scroll();
                }
                Task::none()
            }
            Message::SeekToMs(ms) => {
                self.fade_in_remaining_ms = 0;
                self.audio.set_fade_factor(1.0);
                self.position_ms = ms;
                self.audio.seek_ms(ms);
                self.last_lyric_idx = None;
                self.sync_lyrics_scroll()
            }
            Message::VolumeChanged(v) => {
                let clamped = v.clamp(0.0, 4.0);
                if clamped > 0.01 {
                    self.pre_mute_volume = clamped;
                }
                self.settings.volume = clamped;
                self.audio.set_volume(self.settings.volume);
                let _ = self.settings.save();
                Task::none()
            }
            Message::ToggleMute => {
                if self.settings.volume > 0.01 {
                    self.pre_mute_volume = self.settings.volume;
                    self.settings.volume = 0.0;
                } else {
                    self.settings.volume = self.pre_mute_volume.max(0.5);
                }
                self.audio.set_volume(self.settings.volume);
                let _ = self.settings.save();
                Task::none()
            }
            Message::CycleSleepTimer => {
                let now = Instant::now();
                let next = match self.sleep_timer {
                    SleepTimerState::Off => {
                        self.set_toast("Sleep timer: 15 min");
                        SleepTimerState::Countdown(now + Duration::from_secs(15 * 60))
                    }
                    SleepTimerState::Countdown(deadline) => {
                        let rem_mins = deadline.saturating_duration_since(now).as_secs() / 60;
                        if rem_mins < 20 {
                            self.set_toast("Sleep timer: 30 min");
                            SleepTimerState::Countdown(now + Duration::from_secs(30 * 60))
                        } else if rem_mins < 40 {
                            self.set_toast("Sleep timer: 45 min");
                            SleepTimerState::Countdown(now + Duration::from_secs(45 * 60))
                        } else if rem_mins < 55 {
                            self.set_toast("Sleep timer: 60 min");
                            SleepTimerState::Countdown(now + Duration::from_secs(60 * 60))
                        } else {
                            self.set_toast("Sleep timer: End of song");
                            SleepTimerState::EndOfSong
                        }
                    }
                    SleepTimerState::EndOfSong => {
                        self.set_toast("Sleep timer off");
                        self.audio.set_fade_factor(1.0);
                        SleepTimerState::Off
                    }
                };
                self.sleep_timer = next;
                Task::none()
            }
            Message::ToggleSkipSilence(on) => {
                self.settings.skip_silence = on;
                self.audio.set_skip_silence(on);
                let _ = self.settings.save();
                Task::none()
            }
            Message::TransitionFadeSelected(opt) => {
                self.settings.transition_fade_ms = opt.ms();
                if opt.ms() == 0 {
                    self.fade_in_remaining_ms = 0;
                    self.audio.set_fade_factor(1.0);
                }
                let _ = self.settings.save();
                Task::none()
            }
            Message::EqPresetSelected(preset) => {
                self.settings.eq_preset = preset;
                self.settings.custom_eq_bands = preset.gains_db();
                self.settings.auto_eq_profile = AutoEqProfile::None;
                self.audio.set_eq_bands(preset.gains_db());
                let _ = self.settings.save();
                Task::none()
            }
            Message::AutoEqProfileSelected(profile) => {
                self.settings.auto_eq_profile = profile;
                if let Some(bands) = profile.gains_db() {
                    self.settings.custom_eq_bands = bands;
                    self.audio.set_eq_bands(bands);
                }
                let _ = self.settings.save();
                self.set_toast(format!("AutoEQ: {}", profile.label()));
                Task::none()
            }
            Message::EqBandChanged(idx, val) => {
                if idx < 10 {
                    self.settings.custom_eq_bands[idx] = val;
                    self.settings.auto_eq_profile = AutoEqProfile::None;
                    self.audio.set_eq_bands(self.settings.custom_eq_bands);
                    let _ = self.settings.save();
                }
                Task::none()
            }
            Message::ResetCustomEq => {
                self.settings.custom_eq_bands = [0.0; 10];
                self.settings.auto_eq_profile = AutoEqProfile::None;
                self.settings.eq_preset = EqPreset::Flat;
                self.audio.set_eq_bands(self.settings.custom_eq_bands);
                let _ = self.settings.save();
                self.set_toast("Equalizer reset to Flat (0 dB)");
                Task::none()
            }
            Message::ApplyCustomEqCurve(bands) => {
                self.settings.custom_eq_bands = bands;
                self.settings.auto_eq_profile = AutoEqProfile::None;
                self.audio.set_eq_bands(bands);
                let _ = self.settings.save();
                self.set_toast("Equalizer curve applied");
                Task::none()
            }
            Message::PlaybackSpeedChanged(speed) => {
                let s = speed.clamp(0.5, 2.0);
                self.settings.playback_speed = s;
                self.audio.set_playback_speed(s);
                let _ = self.settings.save();
                self.set_toast(format!("Playback speed: {:.2}×", s));
                Task::none()
            }
            Message::OutputDeviceSelected(dev) => {
                self.settings.output_device = dev.clone();
                self.audio.set_output_device(dev.clone());
                let _ = self.settings.save();
                self.set_toast(format!("Audio output: {}", dev.as_deref().unwrap_or("System Default")));
                Task::none()
            }
            Message::SpatialModeSelected(mode) => {
                self.settings.spatial_mode = mode;
                self.audio.set_dsp(self.settings.eq_preset, mode);
                let _ = self.settings.save();
                Task::none()
            }
            Message::ToggleNormalization(on) => {
                self.settings.audio_normalization = on;
                let cur = self.queue.current().map(|c| &c.track);
                self.audio.set_normalization(
                    on,
                    cur.and_then(|t| t.loudness_db),
                    cur.map(|t| t.is_local).unwrap_or(false),
                );
                let _ = self.settings.save();
                Task::none()
            }
            Message::AudioQualitySelected(q) => {
                self.settings.audio_quality = q;
                let _ = self.settings.save();
                Task::none()
            }
            _ => Task::none(),
        }
    }
}

