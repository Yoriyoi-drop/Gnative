//! Unit ukuran: `Dp` (density-independent pixel) dan `Px` (pixel fisik).

use std::ops::{Add, Mul};

/// Density-independent pixel. 1 `Dp` = 1 pixel logis; diskalakan oleh DPI layar.
#[derive(Debug, Clone, Copy, Default, PartialEq, PartialOrd)]
pub struct Dp(pub f32);

impl Dp {
    pub const ZERO: Dp = Dp(0.0);

    pub const fn new(v: f32) -> Self {
        Self(v)
    }

    pub fn as_f32(self) -> f32 {
        self.0
    }

    /// Konversi ke pixel fisik berdasarkan scale factor DPI.
    pub fn to_px(self, dpi_scale: f32) -> Px {
        Px(self.0 * dpi_scale)
    }
}

/// Pixel fisik (framebuffer).
#[derive(Debug, Clone, Copy, Default, PartialEq, PartialOrd)]
pub struct Px(pub f32);

impl Px {
    pub const ZERO: Px = Px(0.0);

    pub const fn new(v: f32) -> Self {
        Self(v)
    }

    pub fn as_f32(self) -> f32 {
        self.0
    }
}

impl Add for Dp {
    type Output = Dp;
    fn add(self, rhs: Dp) -> Dp {
        Dp(self.0 + rhs.0)
    }
}

impl Mul<f32> for Dp {
    type Output = Dp;
    fn mul(self, rhs: f32) -> Dp {
        Dp(self.0 * rhs)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dp_to_px() {
        assert_eq!(Dp::new(10.0).to_px(2.0), Px::new(20.0));
    }
}
