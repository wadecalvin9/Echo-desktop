use crate::settings::ThemeMode;
use image::imageops::FilterType;

/// RGB color normalized in `0.0..=1.0` so `echo-core` stays UI-framework agnostic.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RgbColor {
    pub r: f32,
    pub g: f32,
    pub b: f32,
}

impl RgbColor {
    pub const fn from_u8(r: u8, g: u8, b: u8) -> Self {
        Self {
            r: r as f32 / 255.0,
            g: g as f32 / 255.0,
            b: b as f32 / 255.0,
        }
    }

    pub fn lerp(self, other: Self, t: f32) -> Self {
        let t = t.clamp(0.0, 1.0);
        Self {
            r: self.r + (other.r - self.r) * t,
            g: self.g + (other.g - self.g) * t,
            b: self.b + (other.b - self.b) * t,
        }
    }

    fn luma(self) -> f32 {
        0.2126 * self.r + 0.7152 * self.g + 0.0722 * self.b
    }
}

/// Echo's fixed brand accent.
pub const ACCENT: RgbColor = RgbColor::from_u8(30, 215, 96);

/// The surfaces and accent are fixed so the app always reads as one coherent, restrained
/// design. Only [`DynamicPalette::tint`] follows the album art, and only the page hero uses it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DynamicPalette {
    /// Window backdrop, visible as the gutters between panels.
    pub background: RgbColor,
    /// Main panels.
    pub surface: RgbColor,
    /// Cards, inputs and hovered rows.
    pub surface_elevated: RgbColor,
    pub primary: RgbColor,
    pub text: RgbColor,
    pub text_muted: RgbColor,
    pub danger: RgbColor,
    /// A darkened colour taken from the current cover, for the hero gradient.
    pub tint: RgbColor,
}

impl DynamicPalette {
    pub fn default_for(mode: ThemeMode) -> Self {
        Self::from_seed(RgbColor::from_u8(83, 83, 100), mode)
    }

    pub fn from_seed(seed: RgbColor, mode: ThemeMode) -> Self {
        // Keep the hero tint deep enough that white text always has contrast on it.
        let mut tint = seed;
        let l = tint.luma();
        if l > 0.30 {
            let k = 0.30 / l;
            tint = RgbColor {
                r: tint.r * k,
                g: tint.g * k,
                b: tint.b * k,
            };
        }

        let (background, surface, surface_elevated) = match mode {
            ThemeMode::OledBlack => (
                RgbColor::from_u8(0, 0, 0),
                RgbColor::from_u8(9, 9, 11),
                RgbColor::from_u8(20, 20, 24),
            ),
            ThemeMode::Dark => (
                RgbColor::from_u8(7, 7, 9),
                RgbColor::from_u8(18, 18, 21),
                RgbColor::from_u8(30, 30, 35),
            ),
            ThemeMode::Dynamic => {
                let base_surface = RgbColor::from_u8(16, 16, 19);
                let base_elevated = RgbColor::from_u8(28, 28, 33);
                (
                    RgbColor::from_u8(5, 5, 7),
                    base_surface.lerp(tint, 0.08),
                    base_elevated.lerp(tint, 0.14),
                )
            }
        };

        Self {
            background,
            surface,
            surface_elevated,
            primary: ACCENT,
            text: RgbColor::from_u8(255, 255, 255),
            text_muted: RgbColor::from_u8(167, 167, 167),
            danger: RgbColor::from_u8(241, 94, 108),
            tint,
        }
    }

    pub fn from_image_bytes(bytes: &[u8], mode: ThemeMode) -> Option<Self> {
        let img = image::load_from_memory(bytes).ok()?;
        let small = img.resize(24, 24, FilterType::Triangle).to_rgb8();
        let seed = extract_vibrant_seed(small.as_raw())?;
        Some(Self::from_seed(seed, mode))
    }
}

fn extract_vibrant_seed(rgb_bytes: &[u8]) -> Option<RgbColor> {
    let mut best_score = -1.0f32;
    let mut best = None;
    let mut avg_r = 0.0f32;
    let mut avg_g = 0.0f32;
    let mut avg_b = 0.0f32;
    let mut count = 0.0f32;

    for px in rgb_bytes.chunks_exact(3) {
        let r = px[0] as f32 / 255.0;
        let g = px[1] as f32 / 255.0;
        let b = px[2] as f32 / 255.0;
        avg_r += r;
        avg_g += g;
        avg_b += b;
        count += 1.0;

        let max_c = r.max(g).max(b);
        let min_c = r.min(g).min(b);
        let sat = if max_c > 0.0 {
            (max_c - min_c) / max_c
        } else {
            0.0
        };
        let luma = 0.2126 * r + 0.7152 * g + 0.0722 * b;
        let luma_weight = 1.0 - (luma - 0.45).abs() * 1.4;
        let score = sat * 2.0 + luma_weight.max(0.0);
        if score > best_score && sat > 0.15 {
            best_score = score;
            best = Some(RgbColor { r, g, b });
        }
    }

    best.or_else(|| {
        (count > 0.0).then_some(RgbColor {
            r: avg_r / count,
            g: avg_g / count,
            b: avg_b / count,
        })
    })
}
