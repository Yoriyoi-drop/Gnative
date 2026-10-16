//! Pemetaan event winit → `gnative_core::event` (netral, tanpa window).
//!
//! Memisahkan "membaca event OS" dari "bereaksi" sehingga pemetaan bisa
//! diuji tanpa display/GPU (P2-12). `gnative_core::event` dipakai di sini
//! agar bukan abstraksi mati.

use gnative_core::event::{
    Event, Key, KeyEvent, MouseButton as CoreButton, MouseEvent, WindowEvent as CoreWindowEvent,
};
use gnative_core::Vec2;
use winit::event::{ElementState, MouseButton, WindowEvent};
use winit::keyboard::{Key as WKey, NamedKey};

/// Petakan satu event winit ke event netral. `cursor` = posisi kursor terakhir
/// (piksel fisik) untuk event mouse yang tak membawa posisi.
/// `None` = event tak relevan untuk model input Gnative.
pub fn map(event: &WindowEvent, cursor: Vec2) -> Option<Event> {
    match event {
        WindowEvent::CloseRequested => Some(Event::Window(CoreWindowEvent::CloseRequested)),
        WindowEvent::Focused(f) => Some(Event::Window(CoreWindowEvent::Focused(*f))),
        WindowEvent::Resized(size) => Some(Event::Window(CoreWindowEvent::Resized {
            width: size.width as f32,
            height: size.height as f32,
        })),
        WindowEvent::ScaleFactorChanged { scale_factor, .. } => Some(Event::Window(
            CoreWindowEvent::ScaleFactorChanged(*scale_factor as f32),
        )),
        WindowEvent::CursorMoved { position, .. } => Some(Event::Mouse(MouseEvent::Moved {
            position: Vec2::new(position.x as f32, position.y as f32),
        })),
        WindowEvent::MouseInput { state, button, .. } => {
            let button = map_button(*button);
            let ev = match state {
                ElementState::Pressed => MouseEvent::Pressed {
                    button,
                    position: cursor,
                },
                ElementState::Released => MouseEvent::Released {
                    button,
                    position: cursor,
                },
            };
            Some(Event::Mouse(ev))
        }
        WindowEvent::MouseWheel { delta, .. } => {
            let delta_y = match delta {
                winit::event::MouseScrollDelta::LineDelta(_, y) => *y,
                winit::event::MouseScrollDelta::PixelDelta(p) => p.y as f32,
            };
            Some(Event::Mouse(MouseEvent::Wheel { delta_y }))
        }
        WindowEvent::KeyboardInput { event, .. } => {
            let key = map_key(&event.logical_key);
            let ev = match event.state {
                ElementState::Pressed => KeyEvent::Pressed(key),
                ElementState::Released => KeyEvent::Released(key),
            };
            Some(Event::Key(ev))
        }
        _ => None,
    }
}

/// Tombol mouse winit → tombol netral.
fn map_button(b: MouseButton) -> CoreButton {
    match b {
        MouseButton::Left => CoreButton::Left,
        MouseButton::Right => CoreButton::Right,
        MouseButton::Middle => CoreButton::Middle,
        MouseButton::Back | MouseButton::Forward | MouseButton::Other(_) => CoreButton::Other(0),
    }
}

/// Tombol keyboard winit → tombol netral.
fn map_key(k: &WKey) -> Key {
    match k {
        WKey::Character(s) => s.chars().next().map(Key::Character).unwrap_or(Key::Unknown),
        WKey::Named(n) => match n {
            NamedKey::Space => Key::Space,
            NamedKey::Enter => Key::Enter,
            NamedKey::Escape => Key::Escape,
            NamedKey::Backspace => Key::Backspace,
            NamedKey::Tab => Key::Tab,
            NamedKey::ArrowUp => Key::ArrowUp,
            NamedKey::ArrowDown => Key::ArrowDown,
            NamedKey::ArrowLeft => Key::ArrowLeft,
            NamedKey::ArrowRight => Key::ArrowRight,
            NamedKey::Home => Key::Home,
            NamedKey::End => Key::End,
            NamedKey::Delete => Key::Delete,
            NamedKey::F1 => Key::F(1),
            NamedKey::F2 => Key::F(2),
            NamedKey::F3 => Key::F(3),
            NamedKey::F4 => Key::F(4),
            NamedKey::F5 => Key::F(5),
            NamedKey::F6 => Key::F(6),
            NamedKey::F7 => Key::F(7),
            NamedKey::F8 => Key::F(8),
            NamedKey::F9 => Key::F(9),
            NamedKey::F10 => Key::F(10),
            NamedKey::F11 => Key::F(11),
            NamedKey::F12 => Key::F(12),
            _ => Key::Unknown,
        },
        _ => Key::Unknown,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tombol_mouse_dipetakan() {
        assert_eq!(map_button(MouseButton::Left), CoreButton::Left);
        assert_eq!(map_button(MouseButton::Right), CoreButton::Right);
        assert_eq!(map_button(MouseButton::Middle), CoreButton::Middle);
        assert_eq!(map_button(MouseButton::Back), CoreButton::Other(0));
    }

    #[test]
    fn karakter_jadi_key_character() {
        let k = WKey::Character("a".into());
        assert_eq!(map_key(&k), Key::Character('a'));
    }

    #[test]
    fn named_key_dipetakan() {
        assert_eq!(map_key(&WKey::Named(NamedKey::ArrowUp)), Key::ArrowUp);
        assert_eq!(map_key(&WKey::Named(NamedKey::Enter)), Key::Enter);
        assert_eq!(map_key(&WKey::Named(NamedKey::F5)), Key::F(5));
        assert_eq!(map_key(&WKey::Named(NamedKey::Escape)), Key::Escape);
    }

    #[test]
    fn named_key_tak_dikenal() {
        // NamedKey yang belum dipetakan → Unknown (tak panic).
        assert_eq!(map_key(&WKey::Named(NamedKey::CapsLock)), Key::Unknown);
    }

    #[test]
    fn nav_key_terklasifikasi() {
        assert!(map_key(&WKey::Named(NamedKey::ArrowLeft)).is_navigation());
    }
}
