//! # gnative-core
//!
//! Fondasi Gnative: tipe dasar yang dipakai seluruh crate lain.
//! **Tanpa dependency eksternal** (aturan repo — lihat AGENTS.md).
//!
//! Satu file = satu tanggung jawab:
//! - `color.rs`  → tipe `Color`
//! - `vec.rs`    → tipe `Vec2`
//! - `rect.rs`   → tipe `Rect`
//! - `size.rs`   → tipe `Size`
//! - `units.rs`  → unit `Dp` / `Px`
//! - `error.rs`  → `GnError` + alias `GnResult`
//! - `event.rs`  → event input/window
//! - `path.rs`   → sanitasi path (anti path-traversal)
//!
//! Lihat `DESIGN.md` §10 untuk peta tanggung jawab lengkap.

pub mod color;
pub mod error;
pub mod event;
pub mod path;
pub mod rect;
pub mod size;
pub mod units;
pub mod vec;

pub use color::Color;
pub use error::{GnError, GnResult};
pub use event::{Event, Key, KeyEvent, MouseButton, MouseEvent, WindowEvent};
pub use path::safe_join;
pub use rect::Rect;
pub use size::Size;
pub use units::{Dp, Px};
pub use vec::Vec2;
