use ::image::imageops::FilterType;
use echo_core::{DynamicPalette, RgbColor, ACCENT};
use iced::gradient::Linear;
use iced::overlay::menu;
use iced::theme::Palette;
use iced::widget::{button, container, image, pick_list, scrollable, slider, text_input, toggler};
use iced::{border, Background, Border, Color, Gradient, Radians, Shadow, Theme, Vector};

pub fn color_from_rgb(c: RgbColor) -> Color {
    Color::from_rgb(c.r, c.g, c.b)
}

pub fn with_alpha(c: Color, a: f32) -> Color {
    Color { a, ..c }
}

fn white(a: f32) -> Color {
    Color::from_rgba(1.0, 1.0, 1.0, a)
}

pub fn to_iced_theme(p: &DynamicPalette) -> Theme {
    Theme::custom(
        "Echo".to_string(),
        Palette {
            background: color_from_rgb(p.background),
            text: color_from_rgb(p.text),
            primary: color_from_rgb(p.primary),
            success: color_from_rgb(ACCENT),
            danger: color_from_rgb(p.danger),
        },
    )
}

// ---------------------------------------------------------------------------------------------
// Surfaces. Flat, no borders, no shadows: depth comes from three close shades of near-black.
// ---------------------------------------------------------------------------------------------

pub fn app_shell_style(p: DynamicPalette) -> impl Fn(&Theme) -> container::Style {
    move |_| container::Style {
        background: Some(Background::Color(color_from_rgb(p.background))),
        text_color: Some(color_from_rgb(p.text)),
        ..Default::default()
    }
}

pub fn glass_panel_style(p: DynamicPalette) -> impl Fn(&Theme) -> container::Style {
    move |_| container::Style {
        background: Some(Background::Color(color_from_rgb(p.surface))),
        text_color: Some(color_from_rgb(p.text)),
        border: Border {
            radius: 12.0.into(),
            ..Default::default()
        },
        ..Default::default()
    }
}

pub fn elevated_card_style(p: DynamicPalette) -> impl Fn(&Theme) -> container::Style {
    move |_| container::Style {
        background: Some(Background::Color(color_from_rgb(p.surface_elevated))),
        text_color: Some(color_from_rgb(p.text)),
        border: Border {
            radius: 10.0.into(),
            ..Default::default()
        },
        ..Default::default()
    }
}

pub fn settings_card_style(p: DynamicPalette) -> impl Fn(&Theme) -> container::Style {
    move |_| {
        let base = color_from_rgb(p.surface_elevated);
        container::Style {
            background: Some(Background::Color(base)),
            text_color: Some(color_from_rgb(p.text)),
            border: Border {
                color: white(0.06),
                width: 1.0,
                radius: 12.0.into(),
            },
            shadow: Shadow {
                color: Color::from_rgba(0.0, 0.0, 0.0, 0.20),
                offset: Vector::new(0.0, 2.0),
                blur_radius: 8.0,
            },
        }
    }
}

pub fn card_icon_badge_style(p: DynamicPalette) -> impl Fn(&Theme) -> container::Style {
    move |_| {
        let accent = color_from_rgb(p.primary);
        container::Style {
            background: Some(Background::Color(with_alpha(accent, 0.14))),
            text_color: Some(accent),
            border: Border {
                color: with_alpha(accent, 0.28),
                width: 1.0,
                radius: 9.0.into(),
            },
            ..Default::default()
        }
    }
}

pub fn eq_band_strip_style(_p: DynamicPalette) -> impl Fn(&Theme) -> container::Style {
    move |_| container::Style {
        background: Some(Background::Color(white(0.035))),
        border: Border {
            color: white(0.05),
            width: 1.0,
            radius: 8.0.into(),
        },
        ..Default::default()
    }
}

pub fn segmented_pill_style(_p: DynamicPalette) -> impl Fn(&Theme) -> container::Style {
    move |_| container::Style {
        background: Some(Background::Color(white(0.05))),
        border: Border {
            color: white(0.08),
            width: 1.0,
            radius: 999.0.into(),
        },
        ..Default::default()
    }
}

pub fn nav_tab_badge_style(p: DynamicPalette, active: bool) -> impl Fn(&Theme) -> container::Style {
    move |_| {
        if active {
            container::Style {
                background: Some(Background::Color(Color::from_rgba(0.0, 0.0, 0.0, 0.18))),
                text_color: Some(Color::BLACK),
                border: Border {
                    radius: 999.0.into(),
                    ..Default::default()
                },
                ..Default::default()
            }
        } else {
            container::Style {
                background: Some(Background::Color(white(0.08))),
                text_color: Some(color_from_rgb(p.text_muted)),
                border: Border {
                    radius: 999.0.into(),
                    ..Default::default()
                },
                ..Default::default()
            }
        }
    }
}

pub fn bento_card_style(p: DynamicPalette) -> impl Fn(&Theme) -> container::Style {
    move |_| {
        let top = color_from_rgb(p.surface_elevated.lerp(p.tint, 0.25));
        let bottom = color_from_rgb(p.surface_elevated);
        let grad = Linear::new(Radians(std::f32::consts::FRAC_PI_4 * 3.0))
            .add_stop(0.0, top)
            .add_stop(1.0, bottom);
        container::Style {
            background: Some(Background::Gradient(Gradient::Linear(grad))),
            text_color: Some(color_from_rgb(p.text)),
            border: Border {
                radius: 12.0.into(),
                ..Default::default()
            },
            ..Default::default()
        }
    }
}

pub fn expanded_stage_style(p: DynamicPalette) -> impl Fn(&Theme) -> container::Style {
    move |_| {
        let top_left = color_from_rgb(p.tint.lerp(p.surface, 0.15));
        let mid = color_from_rgb(p.surface.lerp(p.tint, 0.35));
        let bottom_right = color_from_rgb(p.surface);
        let grad = Linear::new(Radians(std::f32::consts::FRAC_PI_4 * 3.0))
            .add_stop(0.0, top_left)
            .add_stop(0.55, mid)
            .add_stop(1.0, bottom_right);
        container::Style {
            background: Some(Background::Gradient(Gradient::Linear(grad))),
            text_color: Some(color_from_rgb(p.text)),
            border: Border {
                radius: 12.0.into(),
                ..Default::default()
            },
            ..Default::default()
        }
    }
}

/// Flush dark-tinted scrim for the compact Mini-Player window (square outer edge).
pub fn mini_player_scrim_style(p: DynamicPalette) -> impl Fn(&Theme) -> container::Style {
    move |_| {
        let top = with_alpha(color_from_rgb(p.tint.lerp(p.background, 0.55)), 0.76);
        let bottom = with_alpha(color_from_rgb(p.surface.lerp(p.background, 0.55)), 0.92);
        let grad = Linear::new(Radians(std::f32::consts::PI))
            .add_stop(0.0, top)
            .add_stop(1.0, bottom);
        container::Style {
            background: Some(Background::Gradient(Gradient::Linear(grad))),
            text_color: Some(color_from_rgb(p.text)),
            ..Default::default()
        }
    }
}

pub fn mini_player_fallback_style(p: DynamicPalette) -> impl Fn(&Theme) -> container::Style {
    move |_| {
        let top = color_from_rgb(p.tint.lerp(p.surface, 0.25));
        let bottom = color_from_rgb(p.background.lerp(p.surface, 0.50));
        let grad = Linear::new(Radians(std::f32::consts::FRAC_PI_4 * 3.0))
            .add_stop(0.0, top)
            .add_stop(1.0, bottom);
        container::Style {
            background: Some(Background::Gradient(Gradient::Linear(grad))),
            text_color: Some(color_from_rgb(p.text)),
            ..Default::default()
        }
    }
}

/// Translucent dark-tinted scrim placed over blurred cover art so typography remains sharp.
pub fn ambient_scrim_style(
    p: DynamicPalette,
    top_alpha: f32,
    bottom_alpha: f32,
) -> impl Fn(&Theme) -> container::Style {
    move |_| {
        let top = with_alpha(color_from_rgb(p.tint.lerp(p.background, 0.55)), top_alpha);
        let bottom = with_alpha(color_from_rgb(p.surface.lerp(p.background, 0.45)), bottom_alpha);
        let grad = Linear::new(Radians(std::f32::consts::PI))
            .add_stop(0.0, top)
            .add_stop(1.0, bottom);
        container::Style {
            background: Some(Background::Gradient(Gradient::Linear(grad))),
            text_color: Some(color_from_rgb(p.text)),
            border: Border {
                radius: 12.0.into(),
                ..Default::default()
            },
            ..Default::default()
        }
    }
}

pub fn stat_bar_track_style() -> impl Fn(&Theme) -> container::Style {
    move |_| container::Style {
        background: Some(Background::Color(white(0.06))),
        border: Border {
            radius: 999.0.into(),
            ..Default::default()
        },
        ..Default::default()
    }
}

pub fn stat_bar_fill_style(color: Color, radius: f32) -> impl Fn(&Theme) -> container::Style {
    move |_| container::Style {
        background: Some(Background::Color(color)),
        border: Border {
            radius: radius.into(),
            ..Default::default()
        },
        ..Default::default()
    }
}

pub fn stats_rank_badge_style(
    p: DynamicPalette,
    is_podium: bool,
) -> impl Fn(&Theme) -> container::Style {
    move |_| container::Style {
        background: Some(Background::Color(if is_podium {
            with_alpha(color_from_rgb(p.primary), 0.16)
        } else {
            white(0.06)
        })),
        text_color: Some(if is_podium {
            color_from_rgb(p.primary)
        } else {
            color_from_rgb(p.text_muted)
        }),
        border: Border {
            radius: 999.0.into(),
            ..Default::default()
        },
        ..Default::default()
    }
}

/// Rounded frosted bento card button for Listening Stats spotlight & horizontal artist cards.
pub fn stats_bento_button_style(
    p: DynamicPalette,
    highlight: bool,
) -> impl Fn(&Theme, button::Status) -> button::Style {
    move |_, status| {
        let hovered = matches!(status, button::Status::Hovered | button::Status::Pressed);
        let accent = color_from_rgb(p.primary);
        let base_surface = color_from_rgb(p.surface_elevated);
        let bg = if highlight {
            with_alpha(
                color_from_rgb(p.primary.lerp(p.surface_elevated, 0.84)),
                if hovered { 0.96 } else { 0.88 },
            )
        } else if hovered {
            with_alpha(base_surface, 0.96)
        } else {
            with_alpha(base_surface, 0.72)
        };
        let border_color = if highlight {
            with_alpha(accent, if hovered { 0.72 } else { 0.36 })
        } else if hovered {
            white(0.18)
        } else {
            white(0.08)
        };
        button::Style {
            background: Some(Background::Color(bg)),
            text_color: color_from_rgb(p.text),
            border: Border {
                radius: 18.0.into(),
                width: 1.0,
                color: border_color,
            },
            shadow: Shadow {
                color: Color::from_rgba(0.0, 0.0, 0.0, if hovered { 0.30 } else { 0.16 }),
                offset: Vector::new(0.0, if hovered { 6.0 } else { 3.0 }),
                blur_radius: if hovered { 16.0 } else { 10.0 },
            },
        }
    }
}

/// Elevated artwork shadow & subtle rim highlight (Apple Music / Tidal cover frame).
pub fn artwork_frame_style(radius: f32, shadow_alpha: f32) -> impl Fn(&Theme) -> container::Style {
    move |_| container::Style {
        border: Border {
            radius: radius.into(),
            width: 1.0,
            color: white(0.04),
        },
        shadow: Shadow {
            color: Color::from_rgba(0.0, 0.0, 0.0, shadow_alpha),
            offset: Vector::new(0.0, 12.0),
            blur_radius: 28.0,
        },
        ..Default::default()
    }
}

/// Subtle dark gradient scrim over card artwork on hover so the floating play FAB pops.
pub fn card_hover_scrim_style(radius: f32) -> impl Fn(&Theme) -> container::Style {
    move |_| {
        let grad = Linear::new(Radians(std::f32::consts::PI))
            .add_stop(0.0, Color::from_rgba(0.0, 0.0, 0.0, 0.05))
            .add_stop(1.0, Color::from_rgba(0.0, 0.0, 0.0, 0.52));
        container::Style {
            background: Some(Background::Gradient(Gradient::Linear(grad))),
            border: Border {
                radius: radius.into(),
                ..Default::default()
            },
            ..Default::default()
        }
    }
}

pub fn tooltip_style(p: DynamicPalette) -> impl Fn(&Theme) -> container::Style {
    move |_| container::Style {
        background: Some(Background::Color(Color::from_rgb(0.12, 0.12, 0.14))),
        text_color: Some(color_from_rgb(p.text)),
        border: Border {
            radius: 8.0.into(),
            width: 1.0,
            color: white(0.12),
        },
        shadow: Shadow {
            color: Color::from_rgba(0.0, 0.0, 0.0, 0.55),
            offset: Vector::new(0.0, 6.0),
            blur_radius: 16.0,
        },
    }
}

pub fn skeleton_block_style(alpha: f32, radius: f32) -> impl Fn(&Theme) -> container::Style {
    move |_| container::Style {
        background: Some(Background::Color(white(alpha))),
        border: Border {
            radius: radius.into(),
            ..Default::default()
        },
        ..Default::default()
    }
}

pub fn badge_pill_style(p: DynamicPalette) -> impl Fn(&Theme) -> container::Style {
    move |_| container::Style {
        background: Some(Background::Color(white(0.07))),
        text_color: Some(color_from_rgb(p.text)),
        border: Border {
            radius: 999.0.into(),
            ..Default::default()
        },
        ..Default::default()
    }
}

pub fn popover_panel_style(p: DynamicPalette) -> impl Fn(&Theme) -> container::Style {
    move |_| container::Style {
        background: Some(Background::Color(Color::from_rgb(0.14, 0.14, 0.16))),
        text_color: Some(color_from_rgb(p.text)),
        border: Border {
            radius: 10.0.into(),
            width: 1.0,
            color: white(0.10),
        },
        shadow: Shadow {
            color: Color::from_rgba(0.0, 0.0, 0.0, 0.60),
            offset: Vector::new(0.0, 10.0),
            blur_radius: 28.0,
        },
    }
}

pub fn liked_gradient_badge_style() -> impl Fn(&Theme) -> container::Style {
    move |_| {
        let grad = Linear::new(Radians(std::f32::consts::FRAC_PI_4 * 3.0))
            .add_stop(0.0, Color::from_rgb(0.27, 0.10, 0.92))
            .add_stop(1.0, Color::from_rgb(0.75, 0.85, 0.90));
        container::Style {
            background: Some(Background::Gradient(Gradient::Linear(grad))),
            text_color: Some(Color::WHITE),
            border: Border {
                radius: 6.0.into(),
                ..Default::default()
            },
            ..Default::default()
        }
    }
}

pub fn downloads_badge_style() -> impl Fn(&Theme) -> container::Style {
    move |_| container::Style {
        background: Some(Background::Color(Color::from_rgb(0.16, 0.17, 0.19))),
        text_color: Some(Color::from_rgb(0.80, 0.82, 0.85)),
        border: Border {
            radius: 6.0.into(),
            ..Default::default()
        },
        ..Default::default()
    }
}

pub fn local_badge_style() -> impl Fn(&Theme) -> container::Style {
    move |_| container::Style {
        background: Some(Background::Color(Color::from_rgb(0.16, 0.16, 0.18))),
        text_color: Some(Color::WHITE),
        border: Border {
            radius: 6.0.into(),
            ..Default::default()
        },
        ..Default::default()
    }
}

pub fn account_badge_style(
    p: DynamicPalette,
    signed_in: bool,
    active: bool,
) -> impl Fn(&Theme) -> container::Style {
    move |_| container::Style {
        background: Some(Background::Color(if signed_in {
            color_from_rgb(p.primary)
        } else if active {
            white(0.18)
        } else {
            white(0.10)
        })),
        text_color: Some(if signed_in { Color::BLACK } else { Color::WHITE }),
        border: Border {
            color: if active {
                if signed_in {
                    white(0.65)
                } else {
                    color_from_rgb(p.primary)
                }
            } else {
                Color::TRANSPARENT
            },
            width: if active { 1.5 } else { 0.0 },
            radius: 999.0.into(),
        },
        ..Default::default()
    }
}

pub fn outlined_pill_button_style(
    p: DynamicPalette,
    active: bool,
) -> impl Fn(&Theme, button::Status) -> button::Style {
    move |_, status| {
        let hovered = matches!(status, button::Status::Hovered | button::Status::Pressed);
        let border_color = if active {
            color_from_rgb(p.primary)
        } else if hovered {
            Color::WHITE
        } else {
            white(0.32)
        };
        button::Style {
            background: Some(Background::Color(if hovered {
                white(0.08)
            } else {
                Color::TRANSPARENT
            })),
            text_color: if active {
                color_from_rgb(p.primary)
            } else {
                Color::WHITE
            },
            border: Border {
                color: border_color,
                width: 1.0,
                radius: 999.0.into(),
            },
            ..Default::default()
        }
    }
}

/// Page header: smooth atmospheric color glow fading seamlessly into the panel surface.
pub fn hero_banner_style(p: DynamicPalette) -> impl Fn(&Theme) -> container::Style {
    hero_banner_tinted_style(p.tint, p.surface)
}

/// Page header with explicit cover-reactive tint color.
pub fn hero_banner_tinted_style(tint: RgbColor, surface: RgbColor) -> impl Fn(&Theme) -> container::Style {
    move |_| {
        let top = color_from_rgb(tint);
        let mid_hi = color_from_rgb(tint.lerp(surface, 0.36));
        let mid_lo = color_from_rgb(tint.lerp(surface, 0.78));
        let bottom = color_from_rgb(surface);
        let grad = Linear::new(Radians(std::f32::consts::PI))
            .add_stop(0.0, top)
            .add_stop(0.32, mid_hi)
            .add_stop(0.72, mid_lo)
            .add_stop(1.0, bottom);
        container::Style {
            background: Some(Background::Gradient(Gradient::Linear(grad))),
            text_color: Some(Color::WHITE),
            border: Border {
                radius: border::Radius {
                    top_left: 12.0,
                    top_right: 12.0,
                    bottom_right: 0.0,
                    bottom_left: 0.0,
                },
                ..Default::default()
            },
            ..Default::default()
        }
    }
}

/// The player dock blends seamlessly into the app backdrop with soft atmospheric top highlight & elevation shadow.
pub fn player_dock_style(p: DynamicPalette) -> impl Fn(&Theme) -> container::Style {
    move |_| {
        let highlight = color_from_rgb(p.surface_elevated.lerp(p.tint, 0.16));
        let base = color_from_rgb(p.surface.lerp(p.background, 0.46));
        let grad = Linear::new(Radians(std::f32::consts::FRAC_PI_2))
            .add_stop(0.0, with_alpha(highlight, 0.35))
            .add_stop(0.05, base)
            .add_stop(1.0, base);
        container::Style {
            background: Some(Background::Gradient(Gradient::Linear(grad))),
            text_color: Some(color_from_rgb(p.text)),
            shadow: Shadow {
                color: Color::from_rgba(0.0, 0.0, 0.0, 0.50),
                offset: Vector::new(0.0, -8.0),
                blur_radius: 20.0,
            },
            ..Default::default()
        }
    }
}

pub fn soft_pill_button_style(
    p: DynamicPalette,
) -> impl Fn(&Theme, button::Status) -> button::Style {
    move |_, status| {
        let hovered = matches!(status, button::Status::Hovered | button::Status::Pressed);
        button::Style {
            background: Some(Background::Color(white(if hovered { 0.14 } else { 0.07 }))),
            text_color: if hovered {
                Color::WHITE
            } else {
                with_alpha(color_from_rgb(p.text_muted), 0.92)
            },
            border: Border {
                radius: 999.0.into(),
                ..Default::default()
            },
            ..Default::default()
        }
    }
}

pub fn pager_button_style(
    p: DynamicPalette,
    enabled: bool,
) -> impl Fn(&Theme, button::Status) -> button::Style {
    move |_, status| {
        let hovered = enabled && matches!(status, button::Status::Hovered | button::Status::Pressed);
        button::Style {
            background: Some(Background::Color(if hovered {
                white(0.15)
            } else {
                white(if enabled { 0.07 } else { 0.025 })
            })),
            text_color: if enabled {
                color_from_rgb(p.text)
            } else {
                with_alpha(color_from_rgb(p.text_muted), 0.30)
            },
            border: Border {
                radius: 999.0.into(),
                ..Default::default()
            },
            ..Default::default()
        }
    }
}

pub fn logo_badge_style(p: DynamicPalette) -> impl Fn(&Theme) -> container::Style {
    move |_| {
        let accent = color_from_rgb(p.primary);
        container::Style {
            background: Some(Background::Color(accent)),
            text_color: Some(Color::BLACK),
            border: Border {
                radius: 8.0.into(),
                ..Default::default()
            },
            shadow: Shadow {
                color: Color::from_rgba(0.0, 0.0, 0.0, 0.40),
                offset: Vector::new(0.0, 3.0),
                blur_radius: 10.0,
            },
        }
    }
}

pub fn card_play_fab_style(p: DynamicPalette) -> impl Fn(&Theme) -> container::Style {
    move |_| {
        let accent = color_from_rgb(p.primary);
        container::Style {
            background: Some(Background::Color(accent)),
            text_color: Some(Color::BLACK),
            border: Border {
                radius: 999.0.into(),
                ..Default::default()
            },
            shadow: Shadow {
                color: Color::from_rgba(0.0, 0.0, 0.0, 0.50),
                offset: Vector::new(0.0, 4.0),
                blur_radius: 12.0,
            },
        }
    }
}

pub fn divider_style(alpha: f32) -> impl Fn(&Theme) -> container::Style {
    move |_| container::Style {
        background: Some(Background::Color(white(alpha))),
        ..Default::default()
    }
}

// ---------------------------------------------------------------------------------------------
// Buttons
// ---------------------------------------------------------------------------------------------

/// Chips: selected chip is high-contrast crisp white, inactive chips have frosted glass borders.
pub fn top_pill_button_style(
    _p: DynamicPalette,
    active: bool,
) -> impl Fn(&Theme, button::Status) -> button::Style {
    move |_, status| {
        let hovered = matches!(status, button::Status::Hovered | button::Status::Pressed);
        let (bg, fg, border_color) = if active {
            (
                if hovered {
                    Color::from_rgb(0.92, 0.92, 0.92)
                } else {
                    Color::WHITE
                },
                Color::BLACK,
                white(0.0),
            )
        } else {
            (
                white(if hovered { 0.16 } else { 0.08 }),
                if hovered {
                    Color::WHITE
                } else {
                    white(0.90)
                },
                white(if hovered { 0.22 } else { 0.10 }),
            )
        };
        button::Style {
            background: Some(Background::Color(bg)),
            text_color: fg,
            border: Border {
                radius: 999.0.into(),
                width: if active { 0.0 } else { 1.0 },
                color: border_color,
            },
            ..Default::default()
        }
    }
}

/// List rows and sidebar items: invisible until hovered or selected.
pub fn ghost_row_button_style(
    p: DynamicPalette,
    active: bool,
) -> impl Fn(&Theme, button::Status) -> button::Style {
    move |_, status| {
        let bg = if active {
            white(0.10)
        } else if matches!(status, button::Status::Hovered | button::Status::Pressed) {
            white(0.07)
        } else {
            Color::TRANSPARENT
        };
        button::Style {
            background: Some(Background::Color(bg)),
            text_color: color_from_rgb(p.text),
            border: Border {
                radius: 6.0.into(),
                ..Default::default()
            },
            ..Default::default()
        }
    }
}

/// Spotify-style primary navigation row button with subtle hover elevation and active highlight.
pub fn nav_item_button_style(
    p: DynamicPalette,
    active: bool,
) -> impl Fn(&Theme, button::Status) -> button::Style {
    move |_, status| {
        let (bg, fg) = if active {
            (white(0.12), Color::WHITE)
        } else if matches!(status, button::Status::Hovered | button::Status::Pressed) {
            (white(0.06), Color::WHITE)
        } else {
            (Color::TRANSPARENT, color_from_rgb(p.text_muted))
        };
        button::Style {
            background: Some(Background::Color(bg)),
            text_color: fg,
            border: Border {
                radius: 8.0.into(),
                ..Default::default()
            },
            ..Default::default()
        }
    }
}

/// Frosted translucent sticky table header style for playlists and albums.
pub fn sticky_header_style(p: DynamicPalette) -> impl Fn(&Theme) -> container::Style {
    move |_| container::Style {
        background: Some(Background::Color(with_alpha(color_from_rgb(p.surface), 0.96))),
        text_color: Some(color_from_rgb(p.text_muted)),
        border: Border {
            color: white(0.06),
            width: 1.0,
            ..Default::default()
        },
        ..Default::default()
    }
}

/// Shelf cards: transparent until hovered (matches Spotify Desktop).
pub fn card_button_style(p: DynamicPalette) -> impl Fn(&Theme, button::Status) -> button::Style {
    move |_, status| {
        let bg = if matches!(status, button::Status::Hovered | button::Status::Pressed) {
            white(0.09)
        } else {
            Color::TRANSPARENT
        };
        button::Style {
            background: Some(Background::Color(bg)),
            text_color: color_from_rgb(p.text),
            border: Border {
                radius: 8.0.into(),
                ..Default::default()
            },
            ..Default::default()
        }
    }
}

/// The compact tiles in the home greeting grid.
pub fn tile_button_style(p: DynamicPalette) -> impl Fn(&Theme, button::Status) -> button::Style {
    move |_, status| {
        let bg = if matches!(status, button::Status::Hovered | button::Status::Pressed) {
            white(0.20)
        } else {
            white(0.10)
        };
        button::Style {
            background: Some(Background::Color(bg)),
            text_color: color_from_rgb(p.text),
            border: Border {
                radius: 6.0.into(),
                ..Default::default()
            },
            ..Default::default()
        }
    }
}

/// Frosted glass quick-pick tiles in the Home greeting bento grid.
pub fn quick_tile_button_style(
    p: DynamicPalette,
    is_playing: bool,
) -> impl Fn(&Theme, button::Status) -> button::Style {
    move |_, status| {
        let hovered = matches!(status, button::Status::Hovered | button::Status::Pressed);
        let accent = color_from_rgb(p.primary);
        let bg = if is_playing {
            with_alpha(
                color_from_rgb(p.primary.lerp(p.surface_elevated, 0.80)),
                if hovered { 0.95 } else { 0.88 },
            )
        } else if hovered {
            white(0.145)
        } else {
            white(0.075)
        };
        let border_color = if is_playing {
            with_alpha(accent, if hovered { 0.85 } else { 0.58 })
        } else if hovered {
            white(0.22)
        } else {
            white(0.09)
        };
        button::Style {
            background: Some(Background::Color(bg)),
            text_color: color_from_rgb(p.text),
            border: Border {
                radius: 12.0.into(),
                width: 1.0,
                color: border_color,
            },
            shadow: Shadow {
                color: Color::from_rgba(0.0, 0.0, 0.0, if hovered { 0.28 } else { 0.14 }),
                offset: Vector::new(0.0, if hovered { 6.0 } else { 3.0 }),
                blur_radius: if hovered { 14.0 } else { 8.0 },
            },
        }
    }
}

/// Up Next preview card in Cinema Stage left showcase column.
pub fn up_next_card_button_style(
    p: DynamicPalette,
) -> impl Fn(&Theme, button::Status) -> button::Style {
    move |_, status| {
        let hovered = matches!(status, button::Status::Hovered | button::Status::Pressed);
        let accent = color_from_rgb(p.primary);
        button::Style {
            background: Some(Background::Color(if hovered {
                white(0.12)
            } else {
                white(0.065)
            })),
            text_color: Color::WHITE,
            border: Border {
                radius: 12.0.into(),
                width: 1.0,
                color: if hovered {
                    with_alpha(accent, 0.45)
                } else {
                    white(0.09)
                },
            },
            shadow: Shadow {
                color: Color::from_rgba(0.0, 0.0, 0.0, if hovered { 0.32 } else { 0.16 }),
                offset: Vector::new(0.0, if hovered { 4.0 } else { 2.0 }),
                blur_radius: if hovered { 12.0 } else { 6.0 },
            },
        }
    }
}

/// Circular action indicator on the trailing edge of a Home quick-pick tile.
pub fn quick_tile_play_badge_style(
    p: DynamicPalette,
    active: bool,
) -> impl Fn(&Theme) -> container::Style {
    move |_| {
        if active {
            container::Style {
                background: Some(Background::Color(color_from_rgb(p.primary))),
                text_color: Some(Color::BLACK),
                border: Border {
                    radius: 999.0.into(),
                    ..Default::default()
                },
                shadow: Shadow {
                    color: with_alpha(color_from_rgb(p.primary), 0.35),
                    offset: Vector::new(0.0, 2.0),
                    blur_radius: 10.0,
                },
            }
        } else {
            container::Style {
                background: Some(Background::Color(white(0.065))),
                text_color: Some(white(0.65)),
                border: Border {
                    radius: 999.0.into(),
                    width: 1.0,
                    color: white(0.08),
                },
                ..Default::default()
            }
        }
    }
}

pub fn icon_button_style(
    p: DynamicPalette,
    active: bool,
) -> impl Fn(&Theme, button::Status) -> button::Style {
    move |_, status| {
        let bg = if matches!(status, button::Status::Hovered | button::Status::Pressed) {
            white(0.08)
        } else {
            Color::TRANSPARENT
        };
        button::Style {
            background: Some(Background::Color(bg)),
            text_color: if active {
                color_from_rgb(p.primary)
            } else {
                color_from_rgb(p.text_muted)
            },
            border: Border {
                radius: 999.0.into(),
                ..Default::default()
            },
            ..Default::default()
        }
    }
}

pub fn window_control_button_style(
    p: DynamicPalette,
    is_close: bool,
) -> impl Fn(&Theme, button::Status) -> button::Style {
    move |_, status| {
        let hovered = matches!(status, button::Status::Hovered | button::Status::Pressed);
        let bg = if is_close && hovered {
            Color::from_rgb(0.84, 0.14, 0.18)
        } else if hovered {
            white(0.10)
        } else {
            Color::TRANSPARENT
        };
        button::Style {
            background: Some(Background::Color(bg)),
            text_color: if hovered {
                Color::WHITE
            } else {
                color_from_rgb(p.text_muted)
            },
            border: Border {
                radius: 6.0.into(),
                ..Default::default()
            },
            ..Default::default()
        }
    }
}

/// The big accent play button on page headers. Black glyph on accent pill with ambient elevation.
pub fn glowing_play_button_style(
    p: DynamicPalette,
) -> impl Fn(&Theme, button::Status) -> button::Style {
    move |_, status| {
        let accent = color_from_rgb(p.primary);
        let hovered = matches!(status, button::Status::Hovered | button::Status::Pressed);
        let bg = if hovered {
            Color::from_rgb(
                (accent.r + 0.09).min(1.0),
                (accent.g + 0.09).min(1.0),
                (accent.b + 0.09).min(1.0),
            )
        } else {
            accent
        };
        button::Style {
            background: Some(Background::Color(bg)),
            text_color: Color::BLACK,
            border: Border {
                radius: 999.0.into(),
                ..Default::default()
            },
            shadow: Shadow {
                color: Color::from_rgba(0.0, 0.0, 0.0, if hovered { 0.55 } else { 0.40 }),
                offset: Vector::new(0.0, if hovered { 8.0 } else { 5.0 }),
                blur_radius: if hovered { 20.0 } else { 14.0 },
            },
        }
    }
}

/// The white play/pause circle in the player.
pub fn dock_play_button_style(
    _p: DynamicPalette,
) -> impl Fn(&Theme, button::Status) -> button::Style {
    move |_, status| {
        let bg = if matches!(status, button::Status::Hovered | button::Status::Pressed) {
            Color::from_rgb(0.93, 0.93, 0.93)
        } else {
            Color::WHITE
        };
        button::Style {
            background: Some(Background::Color(bg)),
            text_color: Color::BLACK,
            border: Border {
                radius: 999.0.into(),
                ..Default::default()
            },
            ..Default::default()
        }
    }
}

// ---------------------------------------------------------------------------------------------
// Inputs
// ---------------------------------------------------------------------------------------------

pub fn pill_search_input_style(
    p: DynamicPalette,
) -> impl Fn(&Theme, text_input::Status) -> text_input::Style {
    move |_, status| {
        let border_color = match status {
            text_input::Status::Focused => Color::WHITE,
            text_input::Status::Hovered => white(0.35),
            _ => Color::TRANSPARENT,
        };
        text_input::Style {
            background: Background::Color(Color::from_rgb(0.141, 0.141, 0.141)),
            border: Border {
                color: border_color,
                width: 1.0,
                radius: 999.0.into(),
            },
            icon: color_from_rgb(p.text_muted),
            placeholder: color_from_rgb(p.text_muted),
            value: color_from_rgb(p.text),
            selection: with_alpha(color_from_rgb(p.primary), 0.35),
        }
    }
}

/// Thin rail, white fill that turns accent with a visible handle on hover, as Spotify's does.
pub fn slim_slider_style(
    fill: Color,
    hover_accent: Color,
) -> impl Fn(&Theme, slider::Status) -> slider::Style {
    move |_, status| {
        let hot = matches!(status, slider::Status::Hovered | slider::Status::Dragged);
        let is_white = fill.r > 0.99 && fill.g > 0.99 && fill.b > 0.99;
        let fill_color = if hot && is_white { hover_accent } else { fill };
        slider::Style {
            rail: slider::Rail {
                backgrounds: (
                    Background::Color(fill_color),
                    Background::Color(white(if hot { 0.36 } else { 0.24 })),
                ),
                width: if hot { 5.0 } else { 4.0 },
                border: Border {
                    radius: 2.5.into(),
                    ..Default::default()
                },
            },
            handle: slider::Handle {
                shape: slider::HandleShape::Circle {
                    radius: if hot { 6.5 } else { 0.5 },
                },
                background: Background::Color(Color::WHITE),
                border_width: 0.0,
                border_color: Color::TRANSPARENT,
            },
        }
    }
}

/// An overlay-style scrollbar that stays invisible until hovered or dragged.
pub fn scroll_style(_p: DynamicPalette) -> impl Fn(&Theme, scrollable::Status) -> scrollable::Style {
    move |theme, status| {
        let mut s = scrollable::default(theme, status);
        s.container = container::Style::default();
        s.gap = None;

        let (v_alpha, h_alpha) = match status {
            scrollable::Status::Active { .. } => (0.0, 0.0),
            scrollable::Status::Hovered {
                is_vertical_scrollbar_hovered,
                is_horizontal_scrollbar_hovered,
                ..
            } => (
                if is_vertical_scrollbar_hovered { 0.38 } else { 0.14 },
                if is_horizontal_scrollbar_hovered { 0.38 } else { 0.14 },
            ),
            scrollable::Status::Dragged {
                is_vertical_scrollbar_dragged,
                is_horizontal_scrollbar_dragged,
                ..
            } => (
                if is_vertical_scrollbar_dragged { 0.56 } else { 0.22 },
                if is_horizontal_scrollbar_dragged { 0.56 } else { 0.22 },
            ),
        };

        for (rail, alpha) in [
            (&mut s.vertical_rail, v_alpha),
            (&mut s.horizontal_rail, h_alpha),
        ] {
            rail.background = None;
            rail.border = Border::default();
            rail.scroller.color = if alpha > 0.0 {
                white(alpha)
            } else {
                Color::TRANSPARENT
            };
            rail.scroller.border = Border {
                radius: 999.0.into(),
                width: 0.0,
                color: Color::TRANSPARENT,
            };
        }
        s
    }
}

/// Decodes raw image bytes into a square `size x size` image with rounded corners (or a circle),
/// so cover art has crisp rounded edges.
pub fn rounded_image_handle(raw: &[u8], size: u32, radius: f32, circle: bool) -> Option<image::Handle> {
    let img = ::image::load_from_memory(raw).ok()?;
    let mut rgba = img.resize_to_fill(size, size, FilterType::CatmullRom).to_rgba8();
    let r = if circle {
        size as f32 / 2.0
    } else {
        radius.min(size as f32 / 2.0)
    };
    let max_coord = size as f32;

    for y in 0..size {
        for x in 0..size {
            let fx = x as f32 + 0.5;
            let fy = y as f32 + 0.5;
            let dx = if fx < r {
                r - fx
            } else if fx > max_coord - r {
                fx - (max_coord - r)
            } else {
                0.0
            };
            let dy = if fy < r {
                r - fy
            } else if fy > max_coord - r {
                fy - (max_coord - r)
            } else {
                0.0
            };

            if dx > 0.0 && dy > 0.0 {
                let dist = (dx * dx + dy * dy).sqrt();
                if dist > r + 0.5 {
                    rgba.get_pixel_mut(x, y)[3] = 0;
                } else if dist > r - 0.5 {
                    let coverage = (r + 0.5 - dist).clamp(0.0, 1.0);
                    let px = rgba.get_pixel_mut(x, y);
                    px[3] = ((px[3] as f32) * coverage) as u8;
                }
            }
        }
    }

    Some(image::Handle::from_rgba(size, size, rgba.into_raw()))
}

/// Produces a soft atmospheric blurred cover texture for the Expanded Lyrics Stage and Hero headers.
pub fn blurred_image_handle(raw: &[u8]) -> Option<image::Handle> {
    let img = ::image::load_from_memory(raw).ok()?;
    let small = img.resize_to_fill(84, 84, FilterType::Triangle).to_rgba8();
    let blurred = ::image::imageops::blur(&small, 11.0);
    let (w, h) = blurred.dimensions();
    Some(image::Handle::from_rgba(w, h, blurred.into_raw()))
}

/// The pill behind the top search box. The input inside it is drawn bare.
pub fn search_field_style(p: DynamicPalette) -> impl Fn(&Theme) -> container::Style {
    move |_| container::Style {
        background: Some(Background::Color(Color::from_rgb(0.141, 0.141, 0.155))),
        text_color: Some(color_from_rgb(p.text)),
        border: Border {
            radius: 999.0.into(),
            ..Default::default()
        },
        ..Default::default()
    }
}

pub fn bare_input_style(
    p: DynamicPalette,
) -> impl Fn(&Theme, text_input::Status) -> text_input::Style {
    move |_, _| text_input::Style {
        background: Background::Color(Color::TRANSPARENT),
        border: Border::default(),
        icon: color_from_rgb(p.text_muted),
        placeholder: color_from_rgb(p.text_muted),
        value: color_from_rgb(p.text),
        selection: with_alpha(color_from_rgb(p.primary), 0.35),
    }
}

pub fn pick_list_style(
    p: DynamicPalette,
) -> impl Fn(&Theme, pick_list::Status) -> pick_list::Style {
    move |_, status| {
        let hovered = matches!(status, pick_list::Status::Hovered | pick_list::Status::Opened);
        pick_list::Style {
            text_color: color_from_rgb(p.text),
            placeholder_color: color_from_rgb(p.text_muted),
            handle_color: if hovered {
                Color::WHITE
            } else {
                color_from_rgb(p.text_muted)
            },
            background: Background::Color(if hovered {
                white(0.14)
            } else {
                white(0.09)
            }),
            border: Border {
                radius: 999.0.into(),
                width: 1.0,
                color: if hovered { white(0.28) } else { white(0.12) },
            },
        }
    }
}

pub fn pick_list_menu_style(p: DynamicPalette) -> impl Fn(&Theme) -> menu::Style {
    move |_| menu::Style {
        background: Background::Color(Color::from_rgb(0.13, 0.13, 0.15)),
        border: Border {
            width: 1.0,
            radius: 10.0.into(),
            color: white(0.12),
        },
        text_color: color_from_rgb(p.text),
        selected_text_color: Color::BLACK,
        selected_background: Background::Color(color_from_rgb(p.primary)),
    }
}

pub fn toggler_style(p: DynamicPalette) -> impl Fn(&Theme, toggler::Status) -> toggler::Style {
    move |_, status| {
        let (is_toggled, hovered) = match status {
            toggler::Status::Active { is_toggled } => (is_toggled, false),
            toggler::Status::Hovered { is_toggled } => (is_toggled, true),
            toggler::Status::Disabled => (false, false),
        };
        let accent = color_from_rgb(p.primary);
        toggler::Style {
            background: if is_toggled {
                if hovered {
                    Color::from_rgb(
                        (accent.r + 0.09).min(1.0),
                        (accent.g + 0.09).min(1.0),
                        (accent.b + 0.09).min(1.0),
                    )
                } else {
                    accent
                }
            } else if hovered {
                white(0.24)
            } else {
                white(0.16)
            },
            background_border_width: 1.0,
            background_border_color: if is_toggled {
                Color::TRANSPARENT
            } else {
                white(0.12)
            },
            foreground: if is_toggled {
                Color::BLACK
            } else {
                Color::WHITE
            },
            foreground_border_width: 0.0,
            foreground_border_color: Color::TRANSPARENT,
        }
    }
}
