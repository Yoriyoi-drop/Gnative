//! `CudaRenderer` — backend CUDA (FALLBACK).
//!
//! Milestone 0.7: kerangka. Implementasi kernel rasterizer penuh adalah
//! roadmap 0.8. Tanpa fitur `cuda`, `new()` mengembalikan
//! `GnError::BackendUnavailable` (pola lama yang terbukti).

use gnative_core::{GnError, GnResult, Size};
use gnative_gfx::backend::{BackendKind, RenderBackend};
use gnative_gfx::frame::FrameStats;
use gnative_gfx::scene::Scene;

/// Renderer CUDA — fallback ketika wgpu tidak tersedia.
pub struct CudaRenderer;

impl CudaRenderer {
    /// Buat renderer CUDA.
    ///
    /// Tanpa fitur `cuda` (default) → `BackendUnavailable` yang jelas.
    /// Dengan fitur `cuda` + toolkit → implementasi menyusul (roadmap 0.8).
    pub fn new() -> GnResult<Self> {
        #[cfg(not(feature = "cuda"))]
        {
            Err(GnError::BackendUnavailable(
                "backend CUDA nonaktif (build tanpa fitur `cuda`). \
                 Aktifkan: --features cuda"
                    .into(),
            ))
        }
        #[cfg(feature = "cuda")]
        {
            // Toolkit CUDA tersedia di build ini, tapi kernel rasterizer
            // masih roadmap 0.8 — tandai jelas.
            Err(GnError::BackendUnavailable(
                "backend CUDA belum diimplementasikan (roadmap 0.8)".into(),
            ))
        }
    }
}

impl Default for CudaRenderer {
    fn default() -> Self {
        Self
    }
}

impl RenderBackend for CudaRenderer {
    fn kind(&self) -> BackendKind {
        BackendKind::Cuda
    }

    fn name(&self) -> &'static str {
        "cuda"
    }

    fn resize(&mut self, _size: Size) -> GnResult<()> {
        Ok(())
    }

    fn render_scene(&mut self, _scene: &Scene) -> GnResult<FrameStats> {
        Err(GnError::BackendUnavailable(
            "backend CUDA belum diimplementasikan (roadmap 0.8)".into(),
        ))
    }
}
