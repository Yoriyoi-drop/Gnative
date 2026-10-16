//! `WgpuRenderer` — implementasi `RenderBackend` di atas wgpu.
//!
//! Terima `&Scene` (dari `gnative-gfx`) → bangun vertex buffer → render pass
//! → present. Memakai pipeline quad berwarna (tanpa tekstur, milestone 0.7).

use gnative_core::{GnError, GnResult, Size};
use gnative_gfx::backend::{BackendKind, RenderBackend};
use gnative_gfx::frame::FrameStats;
use gnative_gfx::scene::{Node, Scene};
use wgpu::util::DeviceExt;

use crate::atlas::{self, Atlas};
use crate::device::{create_instance, request_adapter, request_device, Gpu};
use crate::shader::{FRAGMENT_SHADER, VERTEX_SHADER};
use crate::surface::SurfaceState;
use crate::vertex::Vertex;

/// Renderer wgpu — PRIMARY backend.
pub struct WgpuRenderer {
    surface: SurfaceState,
    gpu: Gpu,
    pipeline: wgpu::RenderPipeline,
    atlas: Atlas,
    size: Size,
}

impl WgpuRenderer {
    /// Buat renderer dari sebuah window winit (Arc agar surface `'static`).
    pub fn new(window: std::sync::Arc<winit::window::Window>) -> GnResult<Self> {
        let instance = create_instance()?;
        let adapter = request_adapter(&instance, None)?;
        let gpu = request_device(&adapter)?;
        let surface = SurfaceState::new(&instance, window, &adapter, &gpu.device)?;
        let size = Size::new(surface.config.width as f32, surface.config.height as f32);

        let atlas = Atlas::new(&gpu.device, &gpu.queue);
        let pipeline = create_pipeline(&gpu.device, surface.config.format, &atlas.layout);

        Ok(Self {
            surface,
            gpu,
            pipeline,
            atlas,
            size,
        })
    }
}

impl RenderBackend for WgpuRenderer {
    fn kind(&self) -> BackendKind {
        BackendKind::Wgpu
    }

    fn name(&self) -> &'static str {
        "wgpu"
    }

    fn resize(&mut self, size: Size) -> GnResult<()> {
        self.surface.resize(&self.gpu.device, size);
        self.size = size;
        Ok(())
    }

    fn render_scene(&mut self, scene: &Scene) -> GnResult<FrameStats> {
        if !self.surface.configured {
            return Ok(FrameStats::default());
        }

        // Ambil frame berikutnya dari surface.
        let frame = match self.surface.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(t) => t,
            wgpu::CurrentSurfaceTexture::Suboptimal(t) => t,
            wgpu::CurrentSurfaceTexture::Timeout
            | wgpu::CurrentSurfaceTexture::Occluded
            | wgpu::CurrentSurfaceTexture::Validation => return Ok(FrameStats::default()),
            wgpu::CurrentSurfaceTexture::Outdated => {
                self.surface
                    .surface
                    .configure(&self.gpu.device, &self.surface.config);
                return Ok(FrameStats::default());
            }
            wgpu::CurrentSurfaceTexture::Lost => {
                return Err(GnError::Render("device lost".into()));
            }
        };
        let view = frame
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());

        // Bangun vertex buffer dari scene (quad + teks placeholder).
        let (vertices, stats) = build_vertices(scene);

        let mut encoder = self
            .gpu
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("gnative-encoder"),
            });

        if !vertices.is_empty() {
            let buffer = self
                .gpu
                .device
                .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("gnative-vertices"),
                    contents: bytemuck::cast_slice(&vertices),
                    usage: wgpu::BufferUsages::VERTEX,
                });
            {
                let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("gnative-render"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: &view,
                        resolve_target: None,
                        depth_slice: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Clear(wgpu::Color {
                                r: scene.clear.r as f64,
                                g: scene.clear.g as f64,
                                b: scene.clear.b as f64,
                                // P2-11: hormati alpha clear scene (dulu dipaksa 1.0).
                                a: scene.clear.a as f64,
                            }),
                            store: wgpu::StoreOp::Store,
                        },
                    })],
                    depth_stencil_attachment: None,
                    occlusion_query_set: None,
                    timestamp_writes: None,
                    multiview_mask: None,
                });
                pass.set_pipeline(&self.pipeline);
                pass.set_bind_group(0, self.atlas.bind_group(), &[]);
                // P2-11: batasi gambar ke area scene (clip ke ukuran target).
                let sw = (scene.size.width.max(0.0) as u32)
                    .min(self.surface.config.width)
                    .max(1);
                let sh = (scene.size.height.max(0.0) as u32)
                    .min(self.surface.config.height)
                    .max(1);
                pass.set_scissor_rect(0, 0, sw, sh);
                pass.set_vertex_buffer(0, buffer.slice(..));
                pass.draw(0..vertices.len() as u32, 0..1);
            }
        }

        self.gpu.queue.submit(std::iter::once(encoder.finish()));
        self.gpu.queue.present(frame);
        Ok(stats)
    }
}

/// Batas jumlah vertex per frame (P2-11) — cegah buffer raksasa/OOM pada
/// scene ekstrem. Melebihi batas → sisa geometri diabaikan.
const MAX_VERTICES: usize = 1_000_000;

/// Bangun daftar vertex dari scene (2 segitiga per quad).
fn build_vertices(scene: &Scene) -> (Vec<Vertex>, FrameStats) {
    let mut vertices = Vec::new();
    let (w, h) = (scene.size.width, scene.size.height);
    let mut stats = FrameStats::default();

    for node in &scene.nodes {
        push_node(node, w, h, &mut vertices);
    }
    stats.draw_calls = if vertices.is_empty() { 0 } else { 1 };
    stats.vertices = vertices.len();
    stats.triangles = vertices.len() / 3;
    (vertices, stats)
}

fn push_node(node: &Node, w: f32, h: f32, out: &mut Vec<Vertex>) {
    // P2-11: batasi total vertex (scene ekstrem tak boleh tumbuh tanpa batas).
    if out.len() >= MAX_VERTICES {
        return;
    }
    match node {
        Node::Quad(quad) => {
            push_quad(out, quad.rect, quad.color, quad.radius, w, h);
        }
        Node::Text(item) => {
            // Teks nyata via atlas: satu quad ber-UV per glyph yang dikenal;
            // karakter asing dilewati (maju spasi) agar tidak merusak layout.
            push_text(out, item, w, h);
        }
        Node::Group(children) => {
            for child in children {
                push_node(child, w, h, out);
            }
        }
        // Aksi klik dirender seperti isi biasa (hit-test di sisi runtime).
        Node::Action { inner, .. } => {
            push_node(inner, w, h, out);
        }
        Node::Stack(stack) => {
            for child in &stack.children {
                push_node(child, w, h, out);
            }
        }
    }
}

/// Push satu quad solid (mask atlas penuh) + radius sudut (P2-11).
fn push_quad(
    out: &mut Vec<Vertex>,
    rect: gnative_core::Rect,
    color: gnative_core::Color,
    radius: f32,
    w: f32,
    h: f32,
) {
    let c = [color.r, color.g, color.b, color.a];
    out.extend(Vertex::quad(
        rect.x,
        rect.y,
        rect.x + rect.width,
        rect.y + rect.height,
        w,
        h,
        c,
        atlas::solid_uv(),
        radius,
    ));
}

/// Push teks sebagai deretan glyph atlas.
/// Advance `0.55 × font_size` per karakter — sama seperti `measure`
/// di `gnative-gfx` agar render pas dengan layout.
fn push_text(out: &mut Vec<Vertex>, item: &gnative_gfx::scene::TextItem, w: f32, h: f32) {
    let c = [item.color.r, item.color.g, item.color.b, item.color.a];
    let adv = item.font_size * 0.55;
    for (i, ch) in item.text.chars().enumerate() {
        let Some(index) = atlas::glyph_index(ch) else {
            continue;
        };
        let x = item.rect.x + i as f32 * adv;
        out.extend(Vertex::quad(
            x,
            item.rect.y,
            x + adv,
            item.rect.y + item.font_size,
            w,
            h,
            c,
            atlas::glyph_uv(index),
            0.0,
        ));
    }
}

/// Buat render pipeline quad (blending alpha + atlas teks).
///
/// Di wgpu 30 `create_render_pipeline` mengembalikan pipeline langsung
/// (bukan `Result`); error validasi muncul saat pipeline dipakai.
fn create_pipeline(
    device: &wgpu::Device,
    format: wgpu::TextureFormat,
    atlas_layout: &wgpu::BindGroupLayout,
) -> wgpu::RenderPipeline {
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("gnative-shader"),
        source: wgpu::ShaderSource::Wgsl(format!("{}\n{}", VERTEX_SHADER, FRAGMENT_SHADER).into()),
    });

    let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("gnative-pipeline-layout"),
        bind_group_layouts: &[Some(atlas_layout)],
        immediate_size: 0,
    });

    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("gnative-pipeline"),
        layout: Some(&layout),
        vertex: wgpu::VertexState {
            module: &shader,
            entry_point: Some("vs_main"),
            compilation_options: Default::default(),
            buffers: &[Some(Vertex::desc())],
        },
        fragment: Some(wgpu::FragmentState {
            module: &shader,
            entry_point: Some("fs_main"),
            compilation_options: Default::default(),
            targets: &[Some(wgpu::ColorTargetState {
                format,
                blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                write_mask: wgpu::ColorWrites::ALL,
            })],
        }),
        primitive: wgpu::PrimitiveState::default(),
        depth_stencil: None,
        multisample: wgpu::MultisampleState::default(),
        multiview_mask: None,
        cache: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use gnative_core::{Color, Rect, Size};
    use gnative_gfx::scene::{Scene, TextItem};

    fn scene_teks(text: &str) -> Scene {
        Scene {
            size: Size::new(800.0, 600.0),
            clear: Color::new(0.0, 0.0, 0.0, 1.0),
            nodes: vec![Node::Text(TextItem {
                rect: Rect::new(10.0, 10.0, 200.0, 30.0),
                text: text.to_string(),
                font_size: 20.0,
                color: Color::new(1.0, 1.0, 1.0, 1.0),
            })],
        }
    }

    #[test]
    fn teks_emit_per_glyph() {
        // "Hi" = 2 glyph × 6 vertex.
        let (vertices, _) = build_vertices(&scene_teks("Hi"));
        assert_eq!(vertices.len(), 12);
        // UV glyph 'H' (sel 40) berbeda dari 'i' (sel 73).
        assert_ne!(vertices[0].uv, vertices[6].uv);
        // Posisi maju searah x.
        assert!(vertices[6].position[0] > vertices[0].position[0]);
    }

    #[test]
    fn karakter_asing_dilewati() {
        // Emoji tak ada di atlas → nol vertex (tak merusak layout).
        let (vertices, _) = build_vertices(&scene_teks("👋"));
        assert_eq!(vertices.len(), 0);
    }

    #[test]
    fn quad_solid_pakai_mask_penuh() {
        let scene = Scene {
            size: Size::new(800.0, 600.0),
            clear: Color::new(0.0, 0.0, 0.0, 1.0),
            nodes: vec![Node::Quad(gnative_gfx::scene::Quad::new(
                Rect::new(0.0, 0.0, 10.0, 10.0),
                Color::new(1.0, 0.0, 0.0, 1.0),
            ))],
        };
        let (vertices, _) = build_vertices(&scene);
        assert_eq!(vertices.len(), 6);
        let solid = atlas::solid_uv();
        assert_eq!(vertices[0].uv, [solid.0, solid.1]);
    }
}
