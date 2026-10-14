//! Tipe `Color` — RGBA dalam f32 0.0..=1.0 (linear, bukan sRGB).

/// Warna RGBA. Komponen dalam rentang `0.0..=1.0`.
///
/// Gnative bekerja dalam ruang warna *linear*; konversi sRGB→linear
/// dilakukan saat parsing (hex/8-bit).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Color {
    pub r: f32,
    pub g: f32,
    pub b: f32,
    pub a: f32,
}

impl Color {
    pub const TRANSPARENT: Color = Color::new(0.0, 0.0, 0.0, 0.0);
    pub const BLACK: Color = Color::new(0.0, 0.0, 0.0, 1.0);
    pub const WHITE: Color = Color::new(1.0, 1.0, 1.0, 1.0);
    pub const RED: Color = Color::new(1.0, 0.0, 0.0, 1.0);
    pub const GREEN: Color = Color::new(0.0, 1.0, 0.0, 1.0);
    pub const BLUE: Color = Color::new(0.0, 0.0, 1.0, 1.0);

    /// Konstruktor linear.
    pub const fn new(r: f32, g: f32, b: f32, a: f32) -> Self {
        Self { r, g, b, a }
    }

    /// Buat dari byte sRGB 8-bit (0..=255). Alpha default 255.
    pub fn from_rgb8(r: u8, g: u8, b: u8) -> Self {
        Self::from_rgba8(r, g, b, 255)
    }

    /// Buat dari byte sRGB 8-bit + alpha.
    pub fn from_rgba8(r: u8, g: u8, b: u8, a: u8) -> Self {
        Self::new(
            srgb_to_linear(r as f32 / 255.0),
            srgb_to_linear(g as f32 / 255.0),
            srgb_to_linear(b as f32 / 255.0),
            a as f32 / 255.0,
        )
    }

    /// Parse hex: `#RGB`, `#RGBA`, `#RRGGBB`, `#RRGGBBAA`.
    pub fn from_hex(hex: &str) -> Option<Self> {
        let h = hex.strip_prefix('#').unwrap_or(hex);
        if !h.chars().all(|c| c.is_ascii_hexdigit()) {
            return None;
        }
        let digits = |i: usize, n: usize| u8::from_str_radix(&h[i..i + n], 16).ok();
        let (r, g, b, a) = match h.len() {
            3 => (
                digits(0, 1)? * 17,
                digits(1, 1)? * 17,
                digits(2, 1)? * 17,
                255,
            ),
            4 => (
                digits(0, 1)? * 17,
                digits(1, 1)? * 17,
                digits(2, 1)? * 17,
                digits(3, 1)? * 17,
            ),
            6 => (digits(0, 2)?, digits(2, 2)?, digits(4, 2)?, 255),
            8 => (digits(0, 2)?, digits(2, 2)?, digits(4, 2)?, digits(6, 2)?),
            _ => return None,
        };
        Some(Self::from_rgba8(r, g, b, a))
    }

    /// Konversi ke byte sRGB 8-bit + alpha.
    pub fn to_rgba8(self) -> [u8; 4] {
        [
            (linear_to_srgb(self.r) * 255.0).round() as u8,
            (linear_to_srgb(self.g) * 255.0).round() as u8,
            (linear_to_srgb(self.b) * 255.0).round() as u8,
            (self.a * 255.0).round() as u8,
        ]
    }

    /// Salin dengan alpha baru.
    pub fn with_alpha(self, a: f32) -> Self {
        Self { a, ..self }
    }

    /// Interpolasi linier antara dua warna.
    pub fn lerp(self, other: Self, t: f32) -> Self {
        let t = t.clamp(0.0, 1.0);
        Self {
            r: self.r + (other.r - self.r) * t,
            g: self.g + (other.g - self.g) * t,
            b: self.b + (other.b - self.b) * t,
            a: self.a + (other.a - self.a) * t,
        }
    }
}

impl Default for Color {
    fn default() -> Self {
        Self::BLACK
    }
}

fn srgb_to_linear(c: f32) -> f32 {
    if c <= 0.04045 {
        c / 12.92
    } else {
        ((c + 0.055) / 1.055).powf(2.4)
    }
}

fn linear_to_srgb(c: f32) -> f32 {
    if c <= 0.003_130_8 {
        c * 12.92
    } else {
        1.055 * c.powf(1.0 / 2.4) - 0.055
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_rgb() {
        let c = Color::from_hex("#ff0000").unwrap();
        assert_eq!(c.to_rgba8(), [255, 0, 0, 255]);
    }

    #[test]
    fn hex_rgba_short() {
        let c = Color::from_hex("#0f08").unwrap();
        assert_eq!(c.to_rgba8(), [0, 255, 0, 136]);
    }

    #[test]
    fn lerp_clamps() {
        let a = Color::BLACK;
        let b = Color::WHITE;
        assert_eq!(a.lerp(b, 2.0), Color::WHITE);
    }
}
