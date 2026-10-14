//! Event input & window — representasi netral (tidak terikat winit).

use crate::Vec2;

/// Event tingkat tinggi yang diterima widget.
#[derive(Debug, Clone, PartialEq)]
pub enum Event {
    Window(WindowEvent),
    Mouse(MouseEvent),
    Key(KeyEvent),
}

/// Event terkait window.
#[derive(Debug, Clone, PartialEq)]
pub enum WindowEvent {
    CloseRequested,
    Focused(bool),
    Resized { width: f32, height: f32 },
    ScaleFactorChanged(f32),
}

/// Event mouse.
#[derive(Debug, Clone, PartialEq)]
pub enum MouseEvent {
    Moved { position: Vec2 },
    Pressed { button: MouseButton, position: Vec2 },
    Released { button: MouseButton, position: Vec2 },
    Wheel { delta_y: f32 },
}

/// Tombol mouse.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MouseButton {
    Left,
    Right,
    Middle,
    Other(u16),
}

/// Event keyboard.
#[derive(Debug, Clone, PartialEq)]
pub enum KeyEvent {
    Pressed(Key),
    Released(Key),
}

/// Tombol keyboard (subset yang relevan untuk UI).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Key {
    Character(char),
    Space,
    Enter,
    Escape,
    Backspace,
    Tab,
    ArrowUp,
    ArrowDown,
    ArrowLeft,
    ArrowRight,
    Home,
    End,
    Delete,
    F(u8),
    Unknown,
}

impl Key {
    /// Klasifikasi tombol untuk navigasi aksesibilitas dasar.
    pub fn is_navigation(&self) -> bool {
        matches!(
            self,
            Key::ArrowUp | Key::ArrowDown | Key::ArrowLeft | Key::ArrowRight | Key::Tab
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nav_keys() {
        assert!(Key::ArrowUp.is_navigation());
        assert!(!Key::Character('a').is_navigation());
    }
}
