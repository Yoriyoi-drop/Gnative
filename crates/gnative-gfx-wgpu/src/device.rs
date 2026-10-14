//! Pembuatan `Instance` → `Adapter` → `Device` + `Queue`.
//!
//! `request_adapter`/`request_device` bersifat async → dipakai
//! `pollster::block_on` (pola lama yang sudah terbukti).

use gnative_core::{GnError, GnResult};
use wgpu::Instance;

/// Wgpu `Device` + `Queue` yang siap dipakai renderer.
pub struct Gpu {
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
}

/// Buat instance wgpu dengan backend PRIMARY (Vulkan/Metal/DX12).
pub fn create_instance() -> GnResult<Instance> {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
        backends: wgpu::Backends::PRIMARY,
        flags: Default::default(),
        memory_budget_thresholds: Default::default(),
        backend_options: Default::default(),
        display: None,
    });
    Ok(instance)
}

/// Request adapter yang kompatibel dengan surface (jika ada).
pub fn request_adapter(
    instance: &Instance,
    surface: Option<&wgpu::Surface<'static>>,
) -> GnResult<wgpu::Adapter> {
    let options = wgpu::RequestAdapterOptions {
        power_preference: wgpu::PowerPreference::default(),
        compatible_surface: surface,
        force_fallback_adapter: false,
        apply_limit_buckets: true,
    };
    pollster::block_on(instance.request_adapter(&options)).map_err(|e| {
        GnError::BackendUnavailable(format!(
            "tidak ada adapter wgpu yang kompatibel (Vulkan/Metal/DX12): {e}"
        ))
    })
}

/// Request device + queue dari adapter.
pub fn request_device(adapter: &wgpu::Adapter) -> GnResult<Gpu> {
    let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
        label: Some("gnative-device"),
        required_features: wgpu::Features::empty(),
        experimental_features: wgpu::ExperimentalFeatures::disabled(),
        required_limits: wgpu::Limits::default(),
        memory_hints: Default::default(),
        trace: wgpu::Trace::Off,
    }))
    .map_err(|e| GnError::Render(format!("request_device gagal: {e}")))?;
    Ok(Gpu { device, queue })
}
