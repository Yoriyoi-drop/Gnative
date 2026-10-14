//! # gnative-gfx-cuda
//!
//! Backend rendering **CUDA** (FALLBACK) untuk Gnative.
//! Feature `cuda` menghidupkan backend; tanpa fitur, kode tetap
//! terkompilasi dan `CudaRenderer::new()` mengembalikan `BackendUnavailable`.

pub mod renderer;

pub use renderer::CudaRenderer;
