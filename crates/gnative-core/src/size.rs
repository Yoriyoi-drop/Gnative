//! Tipe `Size` — lebar & tinggi.

/// Ukuran 2D.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Size {
    pub width: f32,
    pub height: f32,
}

impl Size {
    pub const ZERO: Self = Self {
        width: 0.0,
        height: 0.0,
    };

    pub const fn new(width: f32, height: f32) -> Self {
        Self { width, height }
    }

    pub fn is_zero(&self) -> bool {
        self.width <= 0.0 || self.height <= 0.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn size_zero() {
        assert!(Size::ZERO.is_zero());
        assert!(!Size::new(10.0, 10.0).is_zero());
    }
}
