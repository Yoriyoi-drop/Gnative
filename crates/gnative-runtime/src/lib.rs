//! # gnative-runtime
//!
//! Runtime aplikasi GUI native:
//! - `backend_select.rs`  → fallback chain wgpu → cuda
//! - `event_loop.rs`      → ApplicationHandler (winit 0.30)
//! - `intrinsics.rs`      → C-ABI yang dipanggil binary hasil kompiler .gn
//! - `state_store.rs`     → penyimpanan state komponen stateful
//!
//! Binary hasil kompiler (`.gn` → native) memanggil `gn_*` intrinsics
//! untuk membuka window + menjalankan event loop + render.

pub mod backend_select;
pub mod event_loop;
pub mod intrinsics;
pub mod state_store;

pub use event_loop::{run_app, run_app_dynamic};
pub use intrinsics::{gn_window_create, gn_window_run};
