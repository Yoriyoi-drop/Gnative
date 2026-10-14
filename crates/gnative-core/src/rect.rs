//! Tipe `Rect` — persegi panjang.

use crate::{Size, Vec2};

/// Persegi panjang dengan posisi (x, y) dan ukuran (width, height).
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

impl Rect {
    pub const ZERO: Self = Self {
        x: 0.0,
        y: 0.0,
        width: 0.0,
        height: 0.0,
    };

    pub const fn new(x: f32, y: f32, width: f32, height: f32) -> Self {
        Self {
            x,
            y,
            width,
            height,
        }
    }

    pub fn from_points(a: Vec2, b: Vec2) -> Self {
        let x = a.x.min(b.x);
        let y = a.y.min(b.y);
        Self::new(x, y, (a.x - b.x).abs(), (a.y - b.y).abs())
    }

    pub fn size(&self) -> Size {
        Size::new(self.width, self.height)
    }

    pub fn origin(&self) -> Vec2 {
        Vec2::new(self.x, self.y)
    }

    pub fn contains(&self, p: Vec2) -> bool {
        p.x >= self.x && p.x <= self.x + self.width && p.y >= self.y && p.y <= self.y + self.height
    }

    pub fn intersects(&self, other: &Self) -> bool {
        self.x < other.x + other.width
            && other.x < self.x + self.width
            && self.y < other.y + other.height
            && other.y < self.y + self.height
    }

    /// Kurangi ukuran di semua sisi (inset positif → mengecil).
    pub fn inset(&self, amount: f32) -> Self {
        Self::new(
            self.x + amount,
            self.y + amount,
            (self.width - 2.0 * amount).max(0.0),
            (self.height - 2.0 * amount).max(0.0),
        )
    }

    pub fn union(&self, other: &Self) -> Self {
        let x = self.x.min(other.x);
        let y = self.y.min(other.y);
        let right = (self.x + self.width).max(other.x + other.width);
        let bottom = (self.y + self.height).max(other.y + other.height);
        Self::new(x, y, right - x, bottom - y)
    }

    pub fn is_empty(&self) -> bool {
        self.width <= 0.0 || self.height <= 0.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn contains_and_intersects() {
        let a = Rect::new(0.0, 0.0, 10.0, 10.0);
        assert!(a.contains(Vec2::new(5.0, 5.0)));
        assert!(!a.contains(Vec2::new(11.0, 5.0)));
        assert!(a.intersects(&Rect::new(5.0, 5.0, 10.0, 10.0)));
        assert!(!a.intersects(&Rect::new(20.0, 20.0, 10.0, 10.0)));
    }

    #[test]
    fn inset_shrinks() {
        let a = Rect::new(0.0, 0.0, 10.0, 10.0);
        assert_eq!(a.inset(2.0), Rect::new(2.0, 2.0, 6.0, 6.0));
    }
}
