//! Pemilihan backend render (fallback chain): wgpu → cuda.

use gnative_core::{GnError, GnResult};
use gnative_gfx::backend::RenderBackend;

/// Coba backend berurutan: wgpu (PRIMARY) → cuda (FALLBACK).
///
/// `window` dipakai wgpu untuk membuat surface. Jika wgpu gagal
/// (no adapter/surface), coba `CudaRenderer::new()`. Jika semua gagal,
/// kembalikan error `BackendUnavailable` yang jelas.
pub fn select_backend(
    window: &std::sync::Arc<winit::window::Window>,
) -> GnResult<Box<dyn RenderBackend>> {
    // 1. wgpu — PRIMARY.
    match gnative_gfx_wgpu::WgpuRenderer::new(window.clone()) {
        Ok(backend) => {
            println!("[gnative] backend render: wgpu");
            return Ok(Box::new(backend));
        }
        Err(e) => {
            eprintln!("[gnative] wgpu tidak tersedia: {e}");
        }
    }

    // 2. cuda — FALLBACK.
    match gnative_gfx_cuda::CudaRenderer::new() {
        Ok(backend) => {
            println!("[gnative] backend render: cuda");
            return Ok(Box::new(backend));
        }
        Err(e) => {
            eprintln!("[gnative] cuda tidak tersedia: {e}");
        }
    }

    Err(GnError::BackendUnavailable(
        "tidak ada backend rendering yang tersedia (wgpu/cuda)".into(),
    ))
}
