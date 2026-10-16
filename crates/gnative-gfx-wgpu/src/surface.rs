//! Surface + konfigurasi presentasi (format sRGB, Fifo).

use std::sync::Arc;

use gnative_core::{GnError, GnResult, Size};

/// Surface wgpu yang sudah dikonfigurasi.
pub struct SurfaceState {
    pub surface: wgpu::Surface<'static>,
    pub config: wgpu::SurfaceConfiguration,
    pub configured: bool,
}

impl SurfaceState {
    /// Buat surface dari window + konfigurasi awal.
    ///
    /// `window` diambil sebagai `Arc` agar surface bisa hidup seumur
    /// hidup aplikasi (`Surface<'static>`); winit 0.30 `Window` tidak `Clone`.
    pub fn new(
        instance: &wgpu::Instance,
        window: Arc<winit::window::Window>,
        adapter: &wgpu::Adapter,
        device: &wgpu::Device,
    ) -> GnResult<Self> {
        let size = window.inner_size();
        let surface = instance
            .create_surface(window)
            .map_err(|e| GnError::Render(format!("create_surface gagal: {e}")))?;

        let caps = surface.get_capabilities(adapter);
        // P2-10: jangan indeks `[0]` buta — surface tanpa format/alpha mode
        // yang didukung harus jadi error rapi, bukan panic.
        let format = caps
            .formats
            .iter()
            .find(|f| f.is_srgb())
            .copied()
            .or_else(|| caps.formats.first().copied())
            .ok_or_else(|| GnError::Render("surface tidak punya format yang didukung".into()))?;
        let alpha_mode = caps.alpha_modes.first().copied().ok_or_else(|| {
            GnError::Render("surface tidak punya alpha mode yang didukung".into())
        })?;

        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format,
            width: size.width.max(1),
            height: size.height.max(1),
            present_mode: wgpu::PresentMode::Fifo,
            alpha_mode,
            view_formats: vec![],
            desired_maximum_frame_latency: 2,
            color_space: wgpu::SurfaceColorSpace::Auto,
        };
        surface.configure(device, &config);

        Ok(Self {
            surface,
            config,
            configured: true,
        })
    }

    /// Sesuaikan ukuran saat window di-resize.
    pub fn resize(&mut self, device: &wgpu::Device, size: Size) {
        self.config.width = size.width.max(1.0) as u32;
        self.config.height = size.height.max(1.0) as u32;
        self.surface.configure(device, &self.config);
        self.configured = true;
    }
}
