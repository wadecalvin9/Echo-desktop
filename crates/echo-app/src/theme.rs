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
            width: 1.0,
            color: white(0.05),
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
            width: 1.0,
            color: white(0.06),
        },
        ..Default::default()
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
                width: 1.0,
                color: white(0.08),
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
                width: 1.0,
                color: white(0.07),
            },
            ..Default::default()
        }
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
        background: Some(Background::Color(Color::from_rgb(0.02, 0.36, 0.24))),
        text_color: Some(Color::from_rgb(0.118, 0.843, 0.376)),
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
) -> impl Fn(&Theme) -> container::Style {
    move |_| container::Style {
        background: Some(Background::Color(if signed_in {
            color_from_rgb(p.primary)
        } else {
            Color::from_rgb(0.16, 0.16, 0.18)
        })),
        text_color: Some(if signed_in { Color::BLACK } else { Color::WHITE }),
        border: Border {
            radius: 999.0.into(),
            ..Default::default()
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

/// Page header: the cover's colour fading into the panel, the way Spotify headers do.
pub fn hero_banner_style(p: DynamicPalette) -> impl Fn(&Theme) -> container::Style {
    move |_| {
        let top = color_from_rgb(p.tint);
        let bottom = color_from_rgb(p.surface);
        let grad = Linear::new(Radians(std::f32::consts::PI))
            .add_stop(0.0, top)
            .add_stop(1.0, bottom);
        container::Style {
            background: Some(Background::Gradient(Gradient::Linear(grad))),
            text_color: Some(color_from_rgb(p.text)),
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

/// The player sits directly on the backdrop with a subtle top highlight.
pub fn player_dock_style(p: DynamicPalette) -> impl Fn(&Theme) -> container::Style {
    move |_| container::Style {
        background: Some(Background::Color(color_from_rgb(p.background))),
        text_color: Some(color_from_rgb(p.text)),
        ..Default::default()
    }
}

pub fn logo_badge_style(p: DynamicPalette) -> impl Fn(&Theme) -> container::Style {
    move |_| container::Style {
        background: Some(Background::Color(color_from_rgb(p.primary))),
        text_color: Some(Color::BLACK),
        border: Border {
            radius: 999.0.into(),
            ..Default::default()
        },
        shadow: Shadow {
            color: Color::from_rgba(0.0, 0.0, 0.0, 0.50),
            offset: Vector::new(0.0, 6.0),
            blur_radius: 16.0,
        },
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

/// Chips: a selected chip is solid white with black text, the rest are quiet grey.
pub fn top_pill_button_style(
    _p: DynamicPalette,
    active: bool,
) -> impl Fn(&Theme, button::Status) -> button::Style {
    move |_, status| {
        let (bg, fg) = if active {
            (
                if matches!(status, button::Status::Hovered) {
                    Color::from_rgb(0.92, 0.92, 0.92)
                } else {
                    Color::WHITE
                },
                Color::BLACK,
            )
        } else {
            (
                white(if matches!(status, button::Status::Hovered) {
                    0.17
                } else {
                    0.10
                }),
                Color::WHITE,
            )
        };
        button::Style {
            background: Some(Background::Color(bg)),
            text_color: fg,
            border: Border {
                radius: 999.0.into(),
                ..Default::default()
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

/// The big accent play button on page headers. Black glyph on green with ambient elevation.
pub fn glowing_play_button_style(
    p: DynamicPalette,
) -> impl Fn(&Theme, button::Status) -> button::Style {
    move |_, status| {
        let accent = color_from_rgb(p.primary);
        let hovered = matches!(status, button::Status::Hovered | button::Status::Pressed);
        let bg = if hovered {
            Color::from_rgb(
                (accent.r + 0.12).min(1.0),
                (accent.g + 0.06).min(1.0),
                (accent.b + 0.12).min(1.0),
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

/// Thin rail, white fill that turns green with a visible handle on hover, as Spotify's does.
pub fn slim_slider_style(fill: Color) -> impl Fn(&Theme, slider::Status) -> slider::Style {
    move |_, status| {
        let hot = matches!(status, slider::Status::Hovered | slider::Status::Dragged);
        let is_white = fill.r > 0.99 && fill.g > 0.99 && fill.b > 0.99;
        let fill_color = if hot && is_white {
            color_from_rgb(ACCENT)
        } else {
            fill
        };
        slider::Style {
            rail: slider::Rail {
                backgrounds: (
                    Background::Color(fill_color),
                    Background::Color(white(0.30)),
                ),
                width: 4.0,
                border: Border {
                    radius: 2.0.into(),
                    ..Default::default()
                },
            },
            handle: slider::Handle {
                shape: slider::HandleShape::Circle {
                    radius: if hot { 6.0 } else { 0.5 },
                },
                background: Background::Color(Color::WHITE),
                border_width: 0.0,
                border_color: Color::TRANSPARENT,
            },
        }
    }
}

/// A scrollbar that stays out of the way until the pointer is over it.
pub fn scroll_style(_p: DynamicPalette) -> impl Fn(&Theme, scrollable::Status) -> scrollable::Style {
    move |theme, status| {
        let mut s = scrollable::default(theme, status);
        let hot = !matches!(status, scrollable::Status::Active);
        let thumb = white(if hot { 0.45 } else { 0.22 });
        for rail in [&mut s.vertical_rail, &mut s.horizontal_rail] {
            rail.background = None;
            rail.border = Border::default();
            rail.scroller.color = thumb;
            rail.scroller.border = Border {
                radius: 999.0.into(),
                ..Default::default()
            };
        }
        s.gap = None;
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

/// The pill behind the top search box. The input inside it is drawn bare.
pub fn search_field_style(p: DynamicPalette) -> impl Fn(&Theme) -> container::Style {
    move |_| container::Style {
        background: Some(Background::Color(Color::from_rgb(0.141, 0.141, 0.155))),
        text_color: Some(color_from_rgb(p.text)),
        border: Border {
            radius: 999.0.into(),
            width: 1.0,
            color: white(0.08),
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
                        (accent.r + 0.10).min(1.0),
                        (accent.g + 0.06).min(1.0),
                        (accent.b + 0.10).min(1.0),
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
