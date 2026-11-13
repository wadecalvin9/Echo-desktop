use echo_core::{
    default_backup_path, AudioQuality, AutoEqProfile, EqPreset, RecognitionSource,
    ScannerSensitivity, SpatialMode, ThemeMode,
};
use iced::alignment::Alignment;
use iced::widget::{
    button, column, container, horizontal_space, pick_list, row, scrollable, slider,
    text_input, toggler, vertical_space,
};
use iced::{Color, Element, Font, Length};

use crate::icons;
use crate::theme::{
    badge_pill_style, color_from_rgb, divider_style, eq_band_strip_style,
    ghost_row_button_style, icon_button_style,
    nav_tab_badge_style, outlined_pill_button_style, pick_list_menu_style, pick_list_style,
    pill_search_input_style, scroll_style, search_field_style, segmented_pill_style,
    settings_card_style, slim_slider_style, stat_bar_fill_style, stat_bar_track_style,
    toggler_style, top_pill_button_style,
};
use crate::{
    bold, mono, semibold, text, EchoApp, FolderTarget, Message, OutputDeviceOption, SettingsSection,
    TransitionFadeOption, YoutubeLanguageOption, YoutubeRegionOption,
};

impl EchoApp {    fn settings_row<'a>(
        &self,
        title: &'static str,
        hint: &'static str,
        control: Element<'a, Message>,
    ) -> Element<'a, Message> {
        let muted = color_from_rgb(self.palette.text_muted);
        if self.center_width() < 720.0 {
            column![
                column![
                    text(title).size(13).font(bold()),
                    text(hint).size(11).color(muted),
                ]
                .spacing(2),
                control,
            ]
            .spacing(8)
            .into()
        } else {
            row![
                column![
                    text(title).size(13).font(bold()),
                    text(hint).size(11).color(muted),
                ]
                .spacing(2)
                .width(Length::Fill),
                control,
            ]
            .spacing(16)
            .align_y(Alignment::Center)
            .into()
        }
    }

    fn settings_card_header<'a>(
        &self,
        _icon_bytes: &'static [u8],
        title: &'static str,
        subtitle: &'static str,
        accessory: Option<Element<'a, Message>>,
    ) -> Element<'a, Message> {
        let p = self.palette;
        let muted = color_from_rgb(p.text_muted);
        let mut r = row![
            column![
                text(title).size(17).font(bold()),
                text(subtitle).size(12).color(muted),
            ]
            .spacing(2)
            .width(Length::Fill),
        ]
        .spacing(12)
        .align_y(Alignment::Center);

        if let Some(acc) = accessory {
            r = r.push(acc);
        }

        r.into()
    }

    pub(crate) fn view_settings(&self) -> Element<'_, Message> {
        let p = self.palette;
        let muted = color_from_rgb(p.text_muted);
        let center_w = self.center_width();
        let is_compact = center_w < 960.0;
        let is_narrow = center_w < 680.0;

        let section_buttons = SettingsSection::ALL.iter().map(|&sec| {
            let active = self.settings_section == sec;
            let (sec_icon, label, count_hint): (&'static [u8], &'static str, usize) = match sec {
                SettingsSection::All => (icons::SETTINGS, if is_compact { "All" } else { "All Settings" }, 10),
                SettingsSection::AudioDsp => (icons::SPEAKER, if is_narrow { "Audio" } else { "Audio & DSP" }, 4),
                SettingsSection::LibraryMedia => (icons::FOLDER, if is_compact { "Library" } else { "Library & Storage" }, 2),
                SettingsSection::Accounts => (icons::USER, if is_compact { "Accounts" } else { "Accounts & Sync" }, 2),
                SettingsSection::AppearanceSystem => (icons::REFRESH, if is_compact { "Appearance" } else { "Appearance & System" }, 2),
            };
            button(
                row![
                    icons::icon(sec_icon, 13.0, if active { Color::BLACK } else { muted }),
                    text(label).size(12).font(if active { bold() } else { Font::DEFAULT }),
                    container(text(format!("{count_hint}")).size(10).font(mono()))
                        .padding([1, 6])
                        .style(nav_tab_badge_style(p, active)),
                ]
                .spacing(6)
                .align_y(Alignment::Center)
            )
            .padding([6, if is_compact { 10 } else { 12 }])
            .style(top_pill_button_style(p, active))
            .on_press(Message::SelectSettingsSection(sec))
            .into()
        });

        let pills_bar = scrollable(
            row(section_buttons)
                .spacing(6)
                .align_y(Alignment::Center),
        )
        .direction(scrollable::Direction::Horizontal(
            scrollable::Scrollbar::new()
                .width(0)
                .scroller_width(0)
                .margin(0),
        ))
        .height(36)
        .width(Length::Fill);

        let header = row![
            column![
                text("Settings").size(32).font(bold()),
                text("Audio, library, playback preferences & appearance").size(13).color(muted),
            ]
            .spacing(2),
            horizontal_space(),
            pills_bar,
        ]
        .spacing(16)
        .align_y(Alignment::Center);

        let row_divider = || -> Element<'static, Message> {
            container(horizontal_space())
                .height(1)
                .width(Length::Fill)
                .style(divider_style(0.06))
                .into()
        };

        let mut dev_options = vec![OutputDeviceOption {
            label: "System Default Device".to_string(),
            device: None,
        }];
        for d in &self.available_audio_devices {
            dev_options.push(OutputDeviceOption {
                label: d.clone(),
                device: Some(d.clone()),
            });
        }
        let cur_dev_option = if let Some(ref d) = self.settings.output_device {
            dev_options
                .iter()
                .find(|o| o.device.as_ref() == Some(d))
                .cloned()
                .unwrap_or(OutputDeviceOption {
                    label: d.clone(),
                    device: Some(d.clone()),
                })
        } else {
            dev_options[0].clone()
        };

        // --- Card 1: Audio Output & Playback ---
        let current_speed = self.settings.playback_speed;
        let speed_presets = [0.75_f32, 1.0, 1.25, 1.5];
        let mut speed_controls = row![].spacing(6).align_y(Alignment::Center);

        let stepper = container(
            row![
                button(text("–").size(14).font(bold()))
                    .padding([4, 10])
                    .style(ghost_row_button_style(p, false))
                    .on_press(Message::PlaybackSpeedChanged((current_speed - 0.25).max(0.5))),
                container(text(format!("{:.2}×", current_speed)).size(12).font(mono()))
                    .padding([4, 8]),
                button(text("+").size(14).font(bold()))
                    .padding([4, 10])
                    .style(ghost_row_button_style(p, false))
                    .on_press(Message::PlaybackSpeedChanged((current_speed + 0.25).min(2.0))),
            ]
            .align_y(Alignment::Center)
        )
        .style(segmented_pill_style(p));

        speed_controls = speed_controls.push(stepper);

        for sp in speed_presets {
            let active = (current_speed - sp).abs() < 0.05;
            speed_controls = speed_controls.push(
                button(text(format!("{:.2}×", sp)).size(11).font(if active { bold() } else { Font::DEFAULT }))
                    .padding([5, 10])
                    .style(top_pill_button_style(p, active))
                    .on_press(Message::PlaybackSpeedChanged(sp))
            );
        }

        if (current_speed - 1.0).abs() >= 0.05 {
            speed_controls = speed_controls.push(
                button(text("Reset").size(11))
                    .padding([5, 10])
                    .style(outlined_pill_button_style(p, false))
                    .on_press(Message::PlaybackSpeedChanged(1.0))
            );
        }

        let audio_output_card = container(
            column![
                self.settings_card_header(
                    icons::SPEAKER,
                    "Audio Output & Playback",
                    "Device routing, bitrates, speed adjustment, and transitions",
                    Some(
                        container(
                            row![
                                icons::icon(icons::CHECK_CIRCLE, 11.0, color_from_rgb(p.primary)),
                                text("Hardware Engine").size(10).font(bold()).color(color_from_rgb(p.primary)),
                            ]
                            .spacing(4)
                            .align_y(Alignment::Center)
                        )
                        .padding([3, 8])
                        .style(badge_pill_style(p))
                        .into()
                    )
                ),
                row_divider(),
                self.settings_row(
                    "Audio output device",
                    "Route playback directly to a dedicated DAC, USB interface, or headphones",
                    pick_list(
                        dev_options,
                        Some(cur_dev_option),
                        |opt| Message::OutputDeviceSelected(opt.device),
                    )
                    .padding([7, 14])
                    .style(pick_list_style(p))
                    .menu_style(pick_list_menu_style(p))
                    .into(),
                ),
                row_divider(),
                self.settings_row(
                    "Streaming quality",
                    "Preferred audio bitrate streamed from YouTube Music (high = 256k AAC/Opus)",
                    pick_list(
                        &AudioQuality::ALL[..],
                        Some(self.settings.audio_quality),
                        Message::AudioQualitySelected,
                    )
                    .padding([7, 14])
                    .style(pick_list_style(p))
                    .menu_style(pick_list_menu_style(p))
                    .into(),
                ),
                row_divider(),
                self.settings_row(
                    "Playback speed",
                    "Adjust audio tempo from 0.5× to 2.0× without pitch distortion",
                    speed_controls.into(),
                ),
                row_divider(),
                self.settings_row(
                    "Transition fade",
                    "Smooth crossfade when advancing between consecutive tracks",
                    pick_list(
                        &TransitionFadeOption::ALL[..],
                        Some(TransitionFadeOption::from_ms(self.settings.transition_fade_ms)),
                        Message::TransitionFadeSelected,
                    )
                    .padding([7, 14])
                    .style(pick_list_style(p))
                    .menu_style(pick_list_menu_style(p))
                    .into(),
                ),
                row_divider(),
                self.settings_row(
                    "Audio normalization",
                    "Level track volumes to -3 dB reference loudness across songs (EBU R128)",
                    toggler(self.settings.audio_normalization)
                        .style(toggler_style(p))
                        .on_toggle(Message::ToggleNormalization)
                        .into(),
                ),
                row_divider(),
                self.settings_row(
                    "Skip silence",
                    "Automatically trim quiet introductory gaps and dead air at track boundaries",
                    toggler(self.settings.skip_silence)
                        .style(toggler_style(p))
                        .on_toggle(Message::ToggleSkipSilence)
                        .into(),
                ),
            ]
            .spacing(12),
        )
        .padding(20)
        .style(settings_card_style(p));

        // --- Card 2: AutoEQ & Harman DSP ---
        let dsp_is_active = self.settings.spatial_mode != SpatialMode::Off || self.settings.eq_preset != EqPreset::Flat;
        let autoeq_dsp_card = container(
            column![
                self.settings_card_header(
                    icons::EQ_BARS,
                    "Harman Tuning & Spatial DSP",
                    "Calibrated headphone compensation curves and stereo room widening",
                    Some(
                        container(
                            text(if dsp_is_active { "DSP Active" } else { "DSP Ready" })
                                .size(10)
                                .font(bold())
                                .color(if dsp_is_active { color_from_rgb(p.primary) } else { muted })
                        )
                        .padding([3, 8])
                        .style(badge_pill_style(p))
                        .into()
                    )
                ),
                row_divider(),
                self.settings_row(
                    "AutoEQ headphone profile",
                    "Target frequency response curve for popular audiophile headphones",
                    pick_list(
                        &AutoEqProfile::ALL[..],
                        Some(self.settings.auto_eq_profile),
                        Message::AutoEqProfileSelected,
                    )
                    .padding([7, 14])
                    .style(pick_list_style(p))
                    .menu_style(pick_list_menu_style(p))
                    .into(),
                ),
                row_divider(),
                self.settings_row(
                    "Spatial audio engine",
                    "Binaural stereo widening and simulated room acoustics",
                    pick_list(
                        &SpatialMode::ALL[..],
                        Some(self.settings.spatial_mode),
                        Message::SpatialModeSelected,
                    )
                    .padding([7, 14])
                    .style(pick_list_style(p))
                    .menu_style(pick_list_menu_style(p))
                    .into(),
                ),
                row_divider(),
                self.settings_row(
                    "Equalizer 3-band preset",
                    "Quick low / mid / high tonal shaping preset",
                    pick_list(
                        &EqPreset::ALL[..],
                        Some(self.settings.eq_preset),
                        Message::EqPresetSelected,
                    )
                    .padding([7, 14])
                    .style(pick_list_style(p))
                    .menu_style(pick_list_menu_style(p))
                    .into(),
                ),
            ]
            .spacing(12),
        )
        .padding(20)
        .style(settings_card_style(p));

        // --- Card 3: 10-Band Studio Graphic Equalizer ---
        let eq_curves: [(&'static str, [f32; 10]); 5] = [
            ("Flat", [0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0]),
            ("Bass Boost", [5.5, 5.0, 4.0, 2.5, 1.0, 0.0, 0.0, 0.5, 1.0, 1.5]),
            ("Treble Boost", [0.0, 0.0, 0.0, 0.5, 1.0, 2.0, 3.5, 5.0, 6.0, 6.5]),
            ("Vocal", [-1.0, -0.5, 0.0, 1.0, 2.5, 3.5, 3.0, 1.5, 0.5, 0.0]),
            ("Electronic", [4.5, 4.0, 2.0, 0.0, -1.0, 1.0, 2.0, 3.5, 4.0, 4.5]),
        ];

        let is_curve_active = |curve: &[f32; 10]| -> bool {
            self.settings.custom_eq_bands.iter().zip(curve.iter()).all(|(a, b)| (a - b).abs() < 0.15)
        };
        let is_any_band_active = self.settings.custom_eq_bands.iter().any(|b| b.abs() > 0.05);

        let mut curve_pills = row![
            text("Studio Presets:").size(12).font(bold()).color(muted)
        ]
        .spacing(8)
        .align_y(Alignment::Center);

        for (name, curve_bands) in eq_curves {
            let active = is_curve_active(&curve_bands);
            curve_pills = curve_pills.push(
                button(text(name).size(11).font(if active { bold() } else { Font::DEFAULT }))
                    .padding([5, 12])
                    .style(top_pill_button_style(p, active))
                    .on_press(Message::ApplyCustomEqCurve(curve_bands))
            );
        }
        if center_w >= 760.0 {
            curve_pills = curve_pills.push(horizontal_space());
        }
        if is_any_band_active {
            curve_pills = curve_pills.push(
                button(text("Reset to Flat").size(11))
                    .padding([5, 12])
                    .style(outlined_pill_button_style(p, false))
                    .on_press(Message::ResetCustomEq)
            );
        }

        let eq_freqs = ["32 Hz", "64 Hz", "125 Hz", "250 Hz", "500 Hz", "1 kHz", "2 kHz", "4 kHz", "8 kHz", "16 kHz"];
        let mut eq_sliders_row = row![].spacing(8).align_y(Alignment::Center);
        for (i, &freq) in eq_freqs.iter().enumerate() {
            let gain = self.settings.custom_eq_bands[i];
            let gain_color = if gain > 0.1 {
                color_from_rgb(p.primary)
            } else if gain < -0.1 {
                Color::from_rgb(0.45, 0.82, 1.0)
            } else {
                muted
            };
            let band_col = container(
                column![
                    container(
                        text(format!("{:+.1} dB", gain)).size(10).font(mono()).color(gain_color)
                    )
                    .padding([2, 4]),
                    slider(-12.0..=12.0, gain, move |val| Message::EqBandChanged(i, val))
                        .step(0.5_f32)
                        .style(slim_slider_style(Color::WHITE, color_from_rgb(p.primary)))
                        .width(74),
                    text(freq).size(10).font(semibold()).color(muted),
                ]
                .spacing(5)
                .align_x(Alignment::Center)
            )
            .padding([8, 6])
            .style(eq_band_strip_style(p));

            eq_sliders_row = eq_sliders_row.push(band_col);
        }

        let custom_eq_card = container(
            column![
                self.settings_card_header(
                    icons::EQ_BARS,
                    "10-Band Studio Graphic Equalizer",
                    "Precision multi-band gain calibration (-12 dB to +12 dB)",
                    Some(
                        container(
                            text(if is_any_band_active { "Custom Equalized" } else { "Flat Bypass" })
                                .size(10)
                                .font(bold())
                                .color(if is_any_band_active { color_from_rgb(p.primary) } else { muted })
                        )
                        .padding([3, 8])
                        .style(badge_pill_style(p))
                        .into()
                    )
                ),
                if center_w < 760.0 {
                    Element::from(
                        scrollable(curve_pills)
                            .direction(scrollable::Direction::Horizontal(
                                scrollable::Scrollbar::new().width(0).scroller_width(0).margin(0),
                            ))
                            .height(34)
                            .width(Length::Fill),
                    )
                } else {
                    Element::from(curve_pills)
                },
                row_divider(),
                scrollable(container(eq_sliders_row).padding([6, 2]))
                    .direction(scrollable::Direction::Horizontal(
                        scrollable::Scrollbar::new().width(4).scroller_width(3).margin(1)
                    ))
                    .height(175)
                    .width(Length::Fill)
                    .style(scroll_style(p)),
            ]
            .spacing(12),
        )
        .padding(20)
        .style(settings_card_style(p));

        // --- Card 4: Lyrics & Smart Radio ---
        let mut excl_chips = column![].spacing(8);
        if self.settings.excluded_artists.is_empty() && self.settings.excluded_tracks.is_empty() {
            excl_chips = excl_chips.push(
                container(
                    text("No exclusions active. Use \"Don't suggest this song / artist\" in track context menus to filter recommendations.")
                        .size(11)
                        .color(muted)
                )
                .padding([6, 10])
                .style(badge_pill_style(p))
            );
        } else {
            if !self.settings.excluded_artists.is_empty() {
                let mut art_chips = row![
                    text("Excluded Artists:").size(11).font(bold()).color(muted)
                ]
                .spacing(6)
                .align_y(Alignment::Center);

                for art in &self.settings.excluded_artists {
                    let art_name = art.clone();
                    art_chips = art_chips.push(
                        container(
                            row![
                                text(art).size(11),
                                button(icons::icon(icons::CLOSE, 10.0, muted))
                                    .padding(2)
                                    .style(icon_button_style(p, false))
                                    .on_press(Message::RemoveExcludedArtist(art_name)),
                            ]
                            .spacing(4)
                            .align_y(Alignment::Center),
                        )
                        .padding([4, 9])
                        .style(search_field_style(p)),
                    );
                }
                excl_chips = excl_chips.push(art_chips);
            }
            if !self.settings.excluded_tracks.is_empty() {
                let mut track_chips = row![
                    text("Excluded Tracks:").size(11).font(bold()).color(muted)
                ]
                .spacing(6)
                .align_y(Alignment::Center);

                for tid in &self.settings.excluded_tracks {
                    let track_id = tid.clone();
                    track_chips = track_chips.push(
                        container(
                            row![
                                text(format!("Track #{tid}")).size(11),
                                button(icons::icon(icons::CLOSE, 10.0, muted))
                                    .padding(2)
                                    .style(icon_button_style(p, false))
                                    .on_press(Message::RemoveExcludedTrack(track_id)),
                            ]
                            .spacing(4)
                            .align_y(Alignment::Center),
                        )
                        .padding([4, 9])
                        .style(search_field_style(p)),
                    );
                }
                excl_chips = excl_chips.push(track_chips);
            }
        }

        let smart_features_card = container(
            column![
                self.settings_card_header(
                    icons::LYRICS,
                    "Lyrics & Discovery Radio",
                    "Word-synced lyric engines, transliteration, and exclusion filters",
                    None,
                ),
                row_divider(),
                self.settings_row(
                    "Better Lyrics (TTML & LRCLIB)",
                    "Prefer rich word/line-synced timing from LRCLIB and TTML",
                    toggler(self.settings.better_lyrics_enabled)
                        .style(toggler_style(p))
                        .on_toggle(Message::ToggleBetterLyrics)
                        .into(),
                ),
                row_divider(),
                self.settings_row(
                    "Romanize Asian lyrics",
                    "Display Latin transliteration beneath Japanese, Korean, and Chinese lyric lines",
                    toggler(self.settings.romanize_lyrics)
                        .style(toggler_style(p))
                        .on_toggle(|_| Message::ToggleRomanizeLyrics)
                        .into(),
                ),
                row_divider(),
                self.settings_row(
                    "Auto-load Up Next radio",
                    "Continuously queue similar recommended tracks when reaching the queue tail",
                    toggler(self.settings.auto_load_more)
                        .style(toggler_style(p))
                        .on_toggle(Message::ToggleAutoLoadMore)
                        .into(),
                ),
                row_divider(),
                column![
                    text("Recommendation Exclusions & Snooze").size(13).font(bold()),
                    excl_chips,
                ]
                .spacing(6),
            ]
            .spacing(12),
        )
        .padding(20)
        .style(settings_card_style(p));

        // --- Card 5: Downloads & Streaming Cache Visualizer ---
        let cache_mb_used = self.cache_stats.1 as f64 / (1024.0 * 1024.0);
        let max_cache_mb = self.settings.max_cache_size_mb as f64;
        let cache_pct = if max_cache_mb > 0.0 {
            (cache_mb_used / max_cache_mb).clamp(0.0, 1.0) as f32
        } else {
            0.0
        };
        let fill_portion = ((cache_pct * 100.0).round() as u16).max(1);
        let empty_portion = 100u16.saturating_sub(fill_portion);

        let folder_btn = |label: &'static str, target: FolderTarget| -> Element<'_, Message> {
            button(
                row![
                    icons::icon(icons::FOLDER, 13.0, muted),
                    text(label).size(12),
                ]
                .spacing(6)
                .align_y(Alignment::Center),
            )
            .padding([8, 14])
            .style(outlined_pill_button_style(p, false))
            .on_press(Message::PickFolder(target))
            .into()
        };

        let dl_default_hint = self
            .settings
            .effective_downloads_dir()
            .map(|d| d.display().to_string())
            .unwrap_or_else(|_| "Echo Downloads".to_string());

        let storage_meter = column![
            row![
                column![
                    row![
                        text("Streaming Audio Cache").size(13).font(bold()),
                        container(
                            text(format!("{:.0}% used", cache_pct * 100.0))
                                .size(10)
                                .font(bold())
                                .color(color_from_rgb(p.primary))
                        )
                        .padding([2, 6])
                        .style(badge_pill_style(p)),
                    ]
                    .spacing(8)
                    .align_y(Alignment::Center),
                    text(format!(
                        "{} cached stream files • {:.1} MB of {} MB allocated",
                        self.cache_stats.0, cache_mb_used, self.settings.max_cache_size_mb
                    ))
                    .size(11)
                    .color(muted),
                ]
                .spacing(2)
                .width(Length::Fill),
                row![
                    text("Limit:").size(12).color(muted),
                    text_input("512", &self.cache_size_input)
                        .style(pill_search_input_style(p))
                        .padding([6, 10])
                        .width(72)
                        .on_input(Message::MaxCacheSizeChanged),
                    text("MB").size(11).color(muted),
                    button(text("Clear Cache").size(11))
                        .padding([6, 12])
                        .style(outlined_pill_button_style(p, false))
                        .on_press(Message::ClearStreamCache),
                ]
                .spacing(8)
                .align_y(Alignment::Center),
            ]
            .align_y(Alignment::Center),
            container(
                row![
                    container(horizontal_space())
                        .width(Length::FillPortion(fill_portion))
                        .height(7)
                        .style(stat_bar_fill_style(color_from_rgb(p.primary), 999.0)),
                    container(horizontal_space())
                        .width(Length::FillPortion(empty_portion))
                        .height(7),
                ]
            )
            .width(Length::Fill)
            .height(7)
            .style(stat_bar_track_style()),
        ]
        .spacing(10);

        let downloads_cache_card = container(
            column![
                self.settings_card_header(
                    icons::DOWNLOAD,
                    "Storage & Stream Cache",
                    "Local stream buffer limits, disk usage, and offline download folders",
                    Some(
                        container(
                            text(format!("{:.1} MB Cached", cache_mb_used))
                                .size(10)
                                .font(mono())
                                .color(color_from_rgb(p.primary))
                        )
                        .padding([3, 8])
                        .style(badge_pill_style(p))
                        .into()
                    )
                ),
                storage_meter,
                row_divider(),
                column![
                    row![
                        text("Active Download Folder").size(13).font(bold()),
                        horizontal_space(),
                        text(dl_default_hint).size(11).color(muted),
                    ]
                    .align_y(Alignment::Center),
                    row![
                        text_input("Custom download folder (blank = default)", &self.download_folder_input)
                            .style(pill_search_input_style(p))
                            .padding([9, 14])
                            .on_input(Message::DownloadFolderChanged)
                            .on_submit(Message::SaveDownloadSettings)
                            .width(Length::Fill),
                        folder_btn("Browse...", FolderTarget::DownloadFolder),
                    ]
                    .spacing(8)
                    .align_y(Alignment::Center),
                ]
                .spacing(6),
                row_divider(),
                column![
                    text("Extra import folders containing [youtube_id] tracks (semicolon-separated):")
                        .size(12)
                        .color(muted),
                    row![
                        text_input(
                            "e.g. D:/Music/Echo; E:/Backup/Tracks",
                            &self.extra_import_input
                        )
                        .style(pill_search_input_style(p))
                        .padding([9, 14])
                        .on_input(Message::ExtraImportFoldersChanged)
                        .on_submit(Message::SaveDownloadSettings)
                        .width(Length::Fill),
                        folder_btn("Add Folder...", FolderTarget::ExtraImportFolder),
                    ]
                    .spacing(8)
                    .align_y(Alignment::Center),
                ]
                .spacing(6),
                row_divider(),
                row![
                    button(text("Save Folder Settings").size(12).font(bold()))
                        .padding([8, 16])
                        .style(top_pill_button_style(p, true))
                        .on_press(Message::SaveDownloadSettings),
                    button(text("Rescan [id] Folders").size(12))
                        .padding([8, 14])
                        .style(outlined_pill_button_style(p, false))
                        .on_press(Message::ScanDownloadFolders),
                    button(text("Migrate Downloads").size(12))
                        .padding([8, 14])
                        .style(outlined_pill_button_style(p, false))
                        .on_press(Message::MigrateDownloads),
                    horizontal_space(),
                    if let Some(st) = &self.download_status {
                        Element::from(
                            container(
                                row![
                                    icons::icon(icons::CHECK_CIRCLE, 12.0, color_from_rgb(p.primary)),
                                    text(st).size(12).color(color_from_rgb(p.primary)),
                                ]
                                .spacing(6)
                                .align_y(Alignment::Center)
                            )
                            .padding([4, 10])
                            .style(badge_pill_style(p))
                        )
                    } else {
                        horizontal_space().into()
                    },
                ]
                .spacing(10)
                .align_y(Alignment::Center),
            ]
            .spacing(12),
        )
        .padding(20)
        .style(settings_card_style(p));

        // --- Card 6: Local Media Library ---
        let local_media_card = container(
            column![
                self.settings_card_header(
                    icons::FOLDER,
                    "Local Music Library",
                    "Index and match offline .flac, .mp3, .m4a, and .opus files",
                    Some(
                        container(
                            text(if self.local_scanning { "Indexing..." } else { "Local Indexer" })
                                .size(10)
                                .font(bold())
                                .color(if self.local_scanning { color_from_rgb(p.primary) } else { muted })
                        )
                        .padding([3, 8])
                        .style(badge_pill_style(p))
                        .into()
                    )
                ),
                row_divider(),
                column![
                    text("Music folders to scan (semicolon-separated):").size(12).color(muted),
                    row![
                        text_input(
                            "e.g. C:/Users/Name/Music; D:/FLACs",
                            &self.local_folders_input
                        )
                        .style(pill_search_input_style(p))
                        .padding([9, 14])
                        .on_input(Message::LocalFoldersChanged)
                        .on_submit(Message::ScanLocalMedia)
                        .width(Length::Fill),
                        folder_btn("Add Folder...", FolderTarget::LocalMediaFolder),
                    ]
                    .spacing(8)
                    .align_y(Alignment::Center),
                ]
                .spacing(6),
                row_divider(),
                column![
                    text("Excluded folders from indexer:").size(12).color(muted),
                    row![
                        text_input(
                            "e.g. C:/Users/Name/Music/Podcasts",
                            &self.excluded_folders_input
                        )
                        .style(pill_search_input_style(p))
                        .padding([9, 14])
                        .on_input(Message::ExcludedFoldersChanged)
                        .width(Length::Fill),
                        folder_btn("Add Folder...", FolderTarget::ExcludedMediaFolder),
                    ]
                    .spacing(8)
                    .align_y(Alignment::Center),
                ]
                .spacing(6),
                row_divider(),
                self.settings_row(
                    "Deduplication sensitivity",
                    "Strictness when matching local audio against cloud streaming tracks",
                    pick_list(
                        &ScannerSensitivity::ALL[..],
                        Some(self.settings.scanner_sensitivity),
                        Message::ScannerSensitivitySelected,
                    )
                    .padding([7, 14])
                    .style(pick_list_style(p))
                    .menu_style(pick_list_menu_style(p))
                    .into(),
                ),
                row_divider(),
                self.settings_row(
                    "Strict filenames",
                    "Do not guess missing artist/title metadata from file names",
                    toggler(self.settings.strict_file_names)
                        .style(toggler_style(p))
                        .on_toggle(Message::ToggleStrictFileNames)
                        .into(),
                ),
                row_divider(),
                self.settings_row(
                    "Auto-rescan on startup",
                    "Automatically check local folders for new tracks when Echo launches",
                    toggler(self.settings.auto_scan_local_on_startup)
                        .style(toggler_style(p))
                        .on_toggle(Message::ToggleAutoScanLocal)
                        .into(),
                ),
                row_divider(),
                row![
                    button(text(if self.local_scanning {
                        "Scanning Folders..."
                    } else {
                        "Scan Local Media Now"
                    }).size(12).font(bold()))
                    .padding([8, 16])
                    .style(top_pill_button_style(p, true))
                    .on_press(Message::ScanLocalMedia),
                    horizontal_space(),
                    if let Some(ref st) = self.local_scan_status {
                        Element::from(
                            container(
                                text(st).size(12).color(muted)
                            )
                            .padding([4, 10])
                            .style(badge_pill_style(p))
                        )
                    } else {
                        horizontal_space().into()
                    },
                ]
                .spacing(12)
                .align_y(Alignment::Center),
            ]
            .spacing(12),
        )
        .padding(20)
        .style(settings_card_style(p));

        // --- Card 7: YouTube Music Account & Sync ---
        let has_cookie = self.yt.has_cookie();
        let yt_status_badge = container(
            row![
                container(horizontal_space())
                    .width(8)
                    .height(8)
                    .style(stat_bar_fill_style(
                        if has_cookie { color_from_rgb(p.primary) } else { muted },
                        999.0
                    )),
                text(if has_cookie { "Account Connected" } else { "Guest / Anonymous" })
                    .size(11)
                    .font(bold())
                    .color(if has_cookie { color_from_rgb(p.primary) } else { muted }),
            ]
            .spacing(6)
            .align_y(Alignment::Center)
        )
        .padding([4, 10])
        .style(badge_pill_style(p));

        let account_card = container(
            column![
                self.settings_card_header(
                    icons::USER,
                    "YouTube Music Account & Catalog",
                    "Synchronize personal playlists, liked songs, and configure catalog region",
                    Some(yt_status_badge.into()),
                ),
                row_divider(),
                column![
                    row![
                        text("YouTube Music Session Cookie Header").size(13).font(bold()),
                        horizontal_space(),
                        if has_cookie {
                            text("Cookie active (stored locally)").size(11).color(color_from_rgb(p.primary))
                        } else {
                            text("Required for personal library, likes & private playlists").size(11).color(muted)
                        },
                    ]
                    .align_y(Alignment::Center),
                    text("Paste your music.youtube.com Cookie header (SAPISID=...; __Secure-3PAPISID=...):")
                        .size(11)
                        .color(muted),
                    text_input(
                        if has_cookie { "Cookie is saved. Paste new cookie here to replace..." } else { "Cookie header string" },
                        &self.yt_cookie_input
                    )
                    .style(pill_search_input_style(p))
                    .padding([9, 14])
                    .secure(true)
                    .on_input(Message::YouTubeCookieChanged)
                    .on_submit(Message::SaveYouTubeCookie),
                ]
                .spacing(6),
                row![
                    button(text("Save Cookie").size(12).font(bold()))
                        .padding([8, 16])
                        .style(top_pill_button_style(p, true))
                        .on_press(Message::SaveYouTubeCookie),
                    button(text(if self.account_syncing {
                        "Syncing Playlists..."
                    } else {
                        "Sync Playlists & Likes"
                    }).size(12))
                        .padding([8, 14])
                        .style(outlined_pill_button_style(p, false))
                        .on_press(Message::SyncYouTubeAccount),
                    button(text(if has_cookie { "Disconnect Cookie" } else { "Clear" }).size(12))
                        .padding([8, 14])
                        .style(outlined_pill_button_style(p, false))
                        .on_press(Message::ClearYouTubeCookie),
                    horizontal_space(),
                    if let Some(st) = &self.account_sync_status {
                        Element::from(
                            container(
                                row![
                                    icons::icon(icons::CHECK_CIRCLE, 12.0, color_from_rgb(p.primary)),
                                    text(st).size(12).color(color_from_rgb(p.primary)),
                                ]
                                .spacing(6)
                                .align_y(Alignment::Center)
                            )
                            .padding([4, 10])
                            .style(badge_pill_style(p))
                        )
                    } else {
                        horizontal_space().into()
                    },
                ]
                .spacing(10)
                .align_y(Alignment::Center),
                row_divider(),
                self.settings_row(
                    "Content catalog region",
                    "Country storefront used for Home mixes, charts, and recommendations",
                    pick_list(
                        &YoutubeRegionOption::ALL[..],
                        Some(YoutubeRegionOption::from_code(&self.settings.youtube_gl)),
                        Message::YoutubeRegionSelected,
                    )
                    .padding([7, 14])
                    .style(pick_list_style(p))
                    .menu_style(pick_list_menu_style(p))
                    .into(),
                ),
                row_divider(),
                self.settings_row(
                    "Catalog display language",
                    "Language used by YouTube Music for artist bios, track names, and genres",
                    pick_list(
                        &YoutubeLanguageOption::ALL[..],
                        Some(YoutubeLanguageOption::from_code(&self.settings.youtube_hl)),
                        Message::YoutubeLanguageSelected,
                    )
                    .padding([7, 14])
                    .style(pick_list_style(p))
                    .menu_style(pick_list_menu_style(p))
                    .into(),
                ),
            ]
            .spacing(12),
        )
        .padding(20)
        .style(settings_card_style(p));
        // --- Card 8: Last.fm Scrobbler ---
        let lastfm_is_connected = self.settings.lastfm_username.is_some();
        let lastfm_status_badge = container(
            row![
                container(horizontal_space())
                    .width(8)
                    .height(8)
                    .style(stat_bar_fill_style(
                        if lastfm_is_connected { color_from_rgb(p.primary) } else { muted },
                        999.0
                    )),
                text(if let Some(u) = &self.settings.lastfm_username {
                    format!("@{u}")
                } else {
                    "Not Connected".to_string()
                })
                .size(11)
                .font(bold())
                .color(if lastfm_is_connected { color_from_rgb(p.primary) } else { muted }),
            ]
            .spacing(6)
            .align_y(Alignment::Center)
        )
        .padding([4, 10])
        .style(badge_pill_style(p));

        let lastfm_controls: Element<'_, Message> = if let Some(user) = &self.settings.lastfm_username {
            row![
                text(format!("Authenticated with Last.fm as {user}"))
                    .size(12)
                    .color(muted),
                horizontal_space(),
                toggler(self.settings.lastfm_scrobbling_enabled)
                    .label("Scrobble")
                    .style(toggler_style(p))
                    .on_toggle(Message::ToggleLastFmScrobbling),
                button(text("Disconnect").size(12))
                    .padding([7, 14])
                    .style(outlined_pill_button_style(p, false))
                    .on_press(Message::DisconnectLastFm),
            ]
            .spacing(12)
            .align_y(Alignment::Center)
            .into()
        } else {
            let mut r = row![
                button(text("Connect Last.fm (Browser)").size(12).font(bold()))
                    .padding([8, 16])
                    .style(top_pill_button_style(p, true))
                    .on_press(Message::StartLastFmAuth)
            ]
            .spacing(10);
            if self.lastfm_pending_token.is_some() {
                r = r.push(
                    button(text("Complete Login").size(12))
                        .padding([8, 16])
                        .style(outlined_pill_button_style(p, true))
                        .on_press(Message::CompleteLastFmAuth),
                );
            }
            r.into()
        };

        let lastfm_card = container(
            column![
                self.settings_card_header(
                    icons::STATS,
                    "Last.fm Scrobbler",
                    "Track your listening statistics and scrobble plays to your Last.fm profile",
                    Some(lastfm_status_badge.into()),
                ),
                row_divider(),
                if self.center_width() < 720.0 {
                    Element::from(
                        column![
                            column![
                                text("Last.fm API Key").size(11).font(bold()),
                                text_input("Enter API Key", &self.settings.lastfm_api_key)
                                    .style(pill_search_input_style(p))
                                    .padding([9, 14])
                                    .on_input(Message::LastFmApiKeyChanged),
                            ]
                            .spacing(4),
                            column![
                                text("Last.fm API Shared Secret").size(11).font(bold()),
                                text_input("Enter API Secret", &self.settings.lastfm_api_secret)
                                    .style(pill_search_input_style(p))
                                    .padding([9, 14])
                                    .secure(true)
                                    .on_input(Message::LastFmApiSecretChanged),
                            ]
                            .spacing(4),
                        ]
                        .spacing(10),
                    )
                } else {
                    Element::from(
                        row![
                            column![
                                text("Last.fm API Key").size(11).font(bold()),
                                text_input("Enter API Key", &self.settings.lastfm_api_key)
                                    .style(pill_search_input_style(p))
                                    .padding([9, 14])
                                    .on_input(Message::LastFmApiKeyChanged),
                            ]
                            .spacing(4)
                            .width(Length::Fill),
                            column![
                                text("Last.fm API Shared Secret").size(11).font(bold()),
                                text_input("Enter API Secret", &self.settings.lastfm_api_secret)
                                    .style(pill_search_input_style(p))
                                    .padding([9, 14])
                                    .secure(true)
                                    .on_input(Message::LastFmApiSecretChanged),
                            ]
                            .spacing(4)
                            .width(Length::Fill),
                        ]
                        .spacing(14),
                    )
                },
                lastfm_controls,
                if let Some(st) = &self.lastfm_status {
                    Element::from(
                        container(
                            text(st).size(12).color(muted)
                        )
                        .padding([4, 10])
                        .style(badge_pill_style(p))
                    )
                } else {
                    horizontal_space().into()
                }
            ]
            .spacing(12),
        )
        .padding(20)
        .style(settings_card_style(p));

        // --- Card 9: Appearance & Recognition ---
        let current_theme = self.settings.theme_mode;
        let mut theme_pills = row![].spacing(8).align_y(Alignment::Center);
        for &mode in &ThemeMode::ALL {
            let active = current_theme == mode;
            let (dot_color, label) = match mode {
                ThemeMode::Dynamic => (color_from_rgb(p.primary), "Dynamic (Artwork)"),
                ThemeMode::Dark => (Color::from_rgb(0.45, 0.45, 0.45), "Dark Charcoal"),
                ThemeMode::OledBlack => (Color::BLACK, "OLED True Black"),
            };
            theme_pills = theme_pills.push(
                button(
                    row![
                        container(horizontal_space())
                            .width(8)
                            .height(8)
                            .style(stat_bar_fill_style(
                                if active { color_from_rgb(p.primary) } else { dot_color },
                                999.0
                            )),
                        text(label).size(11).font(if active { bold() } else { Font::DEFAULT }),
                    ]
                    .spacing(6)
                    .align_y(Alignment::Center)
                )
                .padding([6, 12])
                .style(top_pill_button_style(p, active))
                .on_press(Message::ThemeModeSelected(mode))
            );
        }

        let appearance_card = container(
            column![
                self.settings_card_header(
                    icons::SETTINGS,
                    "Appearance & Recognition",
                    "Visual styling, ambient artwork palettes, and acoustic recognition input",
                    Some(
                        container(
                            text(current_theme.label())
                                .size(10)
                                .font(bold())
                                .color(color_from_rgb(p.primary))
                        )
                        .padding([3, 8])
                        .style(badge_pill_style(p))
                        .into()
                    )
                ),
                row_divider(),
                self.settings_row(
                    "Theme preset mode",
                    "Dynamic extracts ambient palette colors directly from active album artwork",
                    theme_pills.into(),
                ),
                row_divider(),
                self.settings_row(
                    "Recognition audio input",
                    "System loopback or microphone capture for acoustic Shazam identification",
                    pick_list(
                        &RecognitionSource::ALL[..],
                        Some(self.settings.recognition_source),
                        Message::RecognitionSourceSelected,
                    )
                    .padding([7, 14])
                    .style(pick_list_style(p))
                    .menu_style(pick_list_menu_style(p))
                    .into(),
                ),
            ]
            .spacing(12),
        )
        .padding(20)
        .style(settings_card_style(p));

        // --- Card 10: Loudness Repair & Vault Backup ---
        let repair_btn = if self.repair_running {
            button(text("Stop Volume Repair").size(12))
                .padding([8, 16])
                .style(outlined_pill_button_style(p, true))
                .on_press(Message::StopFixVolumes)
        } else {
            button(text(format!(
                "Fix Song Volumes ({} missing)",
                self.missing_loudness_count
            )).size(12).font(bold()))
            .padding([8, 16])
            .style(top_pill_button_style(p, false))
            .on_press(Message::StartFixVolumes)
        };

        let backup_path_hint = default_backup_path()
            .map(|p| p.display().to_string())
            .unwrap_or_else(|_| "echo-backup.echobak".to_string());

        let maintenance_vault_card = container(
            column![
                self.settings_card_header(
                    icons::REFRESH,
                    "Loudness Repair & Vault Backup",
                    "Song volume normalization index repair and .echobak library snapshots",
                    Some(
                        container(
                            text(".echobak Vault")
                                .size(10)
                                .font(bold())
                                .color(muted)
                        )
                        .padding([3, 8])
                        .style(badge_pill_style(p))
                        .into()
                    )
                ),
                row_divider(),
                self.settings_row(
                    "Loudness normalization repair",
                    "Compute replaygain & loudness index for songs missing volume tags",
                    row![
                        repair_btn,
                        if let Some(st) = &self.repair_status {
                            Element::from(
                                container(
                                    text(st).size(11).color(muted)
                                )
                                .padding([4, 8])
                                .style(badge_pill_style(p))
                            )
                        } else {
                            horizontal_space().into()
                        }
                    ]
                    .spacing(8)
                    .align_y(Alignment::Center)
                    .into(),
                ),
                row_divider(),
                column![
                    text("Database & Library Snapshot Vault").size(13).font(bold()),
                    text(format!("Export or import a full .echobak snapshot (default path: {backup_path_hint})"))
                        .size(11)
                        .color(muted),
                    row![
                        button(text("Export Backup...").size(12).font(bold()))
                            .padding([8, 16])
                            .style(top_pill_button_style(p, true))
                            .on_press(Message::ExportBackup),
                        button(text("Import Backup...").size(12))
                            .padding([8, 16])
                            .style(outlined_pill_button_style(p, false))
                            .on_press(Message::ImportBackup),
                        horizontal_space(),
                        if let Some(st) = &self.backup_status {
                            Element::from(
                                container(
                                    row![
                                        icons::icon(icons::CHECK_CIRCLE, 12.0, color_from_rgb(p.primary)),
                                        text(st).size(12).color(color_from_rgb(p.primary)),
                                    ]
                                    .spacing(6)
                                    .align_y(Alignment::Center)
                                )
                                .padding([4, 10])
                                .style(badge_pill_style(p))
                            )
                        } else {
                            horizontal_space().into()
                        },
                    ]
                    .spacing(10)
                    .align_y(Alignment::Center),
                ]
                .spacing(8),
            ]
            .spacing(12),
        )
        .padding(20)
        .style(settings_card_style(p));

        // Group cards according to the selected navigation section
        let mut cards: Vec<Element<'_, Message>> = Vec::new();
        match self.settings_section {
            SettingsSection::All => {
                cards.push(audio_output_card.into());
                cards.push(autoeq_dsp_card.into());
                cards.push(custom_eq_card.into());
                cards.push(smart_features_card.into());
                cards.push(downloads_cache_card.into());
                cards.push(local_media_card.into());
                cards.push(account_card.into());
                cards.push(lastfm_card.into());
                cards.push(appearance_card.into());
                cards.push(maintenance_vault_card.into());
            }
            SettingsSection::AudioDsp => {
                cards.push(audio_output_card.into());
                cards.push(autoeq_dsp_card.into());
                cards.push(custom_eq_card.into());
                cards.push(smart_features_card.into());
            }
            SettingsSection::LibraryMedia => {
                cards.push(downloads_cache_card.into());
                cards.push(local_media_card.into());
            }
            SettingsSection::Accounts => {
                cards.push(account_card.into());
                cards.push(lastfm_card.into());
            }
            SettingsSection::AppearanceSystem => {
                cards.push(appearance_card.into());
                cards.push(maintenance_vault_card.into());
            }
        }

        let pad_h = if self.center_width() < 720.0 { 16 } else { 28 };
        self.vscroll(
            container(
                column![
                    header,
                    vertical_space().height(12),
                    column(cards).spacing(22),
                    vertical_space().height(44),
                ]
                .spacing(12)
                .max_width(840),
            )
            .width(Length::Fill)
            .padding([24, pad_h])
            .align_x(iced::alignment::Horizontal::Center),
        )
    }

}
