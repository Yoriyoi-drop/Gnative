//! # gnative-gfx-wgpu
//!
//! Backend rendering **wgpu** (PRIMARY) untuk Gnative.
//! Satu file = satu tanggung jawab:
//! - `device.rs`   → Instance / Adapter / Device / Queue
//! - `surface.rs`  → Surface + SurfaceConfiguration
//! - `vertex.rs`   → Vertex + layout
//! - `shader.rs`   → source WGSL
//! - `atlas.rs`    → font bitmap 5x7 + tekstur atlas teks
//! - `renderer.rs` → implementasi `RenderBackend`

pub mod atlas;
pub mod device;
pub mod renderer;
pub mod shader;
pub mod surface;
pub mod vertex;

pub use renderer::WgpuRenderer;
