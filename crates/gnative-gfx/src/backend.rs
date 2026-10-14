//! Kontrak backend rendering: trait `RenderBackend` + `BackendKind`.

use gnative_core::{GnResult, Size};

use crate::frame::FrameStats;
use crate::scene::Scene;

/// Jenis backend rendering.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BackendKind {
    /// wgpu (Vulkan / Metal / DX12 / GLES) — PRIMARY.
    Wgpu,
    /// CUDA (rasterizer komputasi GPU NVIDIA) — FALLBACK.
    Cuda,
}

impl BackendKind {
    pub fn name(self) -> &'static str {
        match self {
            BackendKind::Wgpu => "wgpu",
            BackendKind::Cuda => "cuda",
        }
    }
}

/// Kontrak minimal backend rendering.
///
/// Backend apapun (wgpu, CUDA, masa depan: software) memenuhi kontrak ini.
/// `gnative-gfx` tidak tahu menahu API GPU — hanya tahu `Scene` dan
/// `FrameStats`.
pub trait RenderBackend {
    fn kind(&self) -> BackendKind;

    /// Nama backend (untuk laporan/diagnostik).
    fn name(&self) -> &'static str;

    /// Sesuaikan ukuran surface/framebuffer (dipanggil saat window resize).
    fn resize(&mut self, size: Size) -> GnResult<()>;

    /// Render satu frame dari scene.
    fn render_scene(&mut self, scene: &Scene) -> GnResult<FrameStats>;
}
