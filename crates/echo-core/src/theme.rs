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

/// Echo's fixed default accent (Oceanic Sage / Muted Acoustic Teal #3EA89E - organic, smooth, restrained).
pub const ACCENT: RgbColor = RgbColor::from_u8(62, 168, 158);

/// Derives a vivid, high-contrast accent color from an album cover seed for `ThemeMode::Dynamic`.
/// Falls back toward [`ACCENT`] when the cover is nearly grayscale.
fn derive_dynamic_accent(seed: RgbColor) -> RgbColor {
    let max_c = seed.r.max(seed.g).max(seed.b);
    let min_c = seed.r.min(seed.g).min(seed.b);
    let sat = if max_c > 0.0 {
        (max_c - min_c) / max_c
    } else {
        0.0
    };
    if sat < 0.18 || max_c < 0.05 {
        return ACCENT;
    }

    // Scale peak channel moderately rather than blasting saturation to 100%,
    // keeping dynamic accents rich and legible without screaming fluorescent tones.
    let scale = (1.0 / max_c.max(0.25)).min(1.35);
    let mut r = (seed.r * scale).clamp(0.0, 1.0);
    let mut g = (seed.g * scale).clamp(0.0, 1.0);
    let mut b = (seed.b * scale).clamp(0.0, 1.0);

    // Ensure perceptual luma is bright enough (>= 0.56) for black icons/text on accent pills.
    let mut candidate = RgbColor { r, g, b };
    let luma = candidate.luma();
    if luma < 0.56 {
        let boost = ((0.56 - luma) / (1.0 - luma).max(0.05)).clamp(0.0, 0.72);
        r += (1.0 - r) * boost;
        g += (1.0 - g) * boost;
        b += (1.0 - b) * boost;
        candidate = RgbColor { r, g, b };
    }
    candidate
}

/// The surfaces and accent are fixed so the app always reads as one coherent, restrained
/// design. In [`ThemeMode::Dynamic`], [`DynamicPalette::tint`] and [`DynamicPalette::primary`]
/// adapt to the active album art.
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
        Self::from_seed(RgbColor::from_u8(28, 64, 60), mode)
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

        let (background, surface, surface_elevated, primary) = match mode {
            ThemeMode::OledBlack => (
                RgbColor::from_u8(0, 0, 0),
                RgbColor::from_u8(10, 12, 16),
                RgbColor::from_u8(20, 24, 31),
                ACCENT,
            ),
            ThemeMode::Dark => (
                RgbColor::from_u8(8, 9, 12),
                RgbColor::from_u8(17, 19, 24),
                RgbColor::from_u8(28, 31, 38),
                ACCENT,
            ),
            ThemeMode::Dynamic => {
                let base_surface = RgbColor::from_u8(17, 19, 24);
                let base_elevated = RgbColor::from_u8(28, 31, 38);
                (
                    RgbColor::from_u8(7, 8, 11),
                    base_surface.lerp(tint, 0.12),
                    base_elevated.lerp(tint, 0.18),
                    derive_dynamic_accent(seed),
                )
            }
        };

        Self {
            background,
            surface,
            surface_elevated,
            primary,
            text: RgbColor::from_u8(245, 247, 250),
            text_muted: RgbColor::from_u8(142, 149, 163),
            danger: RgbColor::from_u8(244, 88, 107),
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dynamic_mode_adapts_primary_while_dark_keeps_accent() {
        let warm_seed = RgbColor::from_u8(210, 60, 40);
        let dark = DynamicPalette::from_seed(warm_seed, ThemeMode::Dark);
        assert_eq!(dark.primary, ACCENT);

        let oled = DynamicPalette::from_seed(warm_seed, ThemeMode::OledBlack);
        assert_eq!(oled.primary, ACCENT);

        let dyn_p = DynamicPalette::from_seed(warm_seed, ThemeMode::Dynamic);
        assert_ne!(dyn_p.primary, ACCENT);
        assert!(dyn_p.primary.luma() >= 0.55);
        assert!(dyn_p.tint.luma() <= 0.301);
    }

    #[test]
    fn grayscale_seed_falls_back_to_brand_accent() {
        let gray = RgbColor::from_u8(120, 120, 120);
        let dyn_p = DynamicPalette::from_seed(gray, ThemeMode::Dynamic);
        assert_eq!(dyn_p.primary, ACCENT);
    }
}
