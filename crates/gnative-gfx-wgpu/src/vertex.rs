//! Vertex — posisi 2D + warna + UV atlas (28 byte, `#[repr(C)]`).

use bytemuck::{Pod, Zeroable};

/// Vertex: posisi dalam koordinat NDC + warna RGBA + UV tekstur atlas.
/// Quad non-teks memakai UV sel solid (mask = 1).
#[repr(C)]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
pub struct Vertex {
    pub position: [f32; 2],
    pub color: [f32; 4],
    pub uv: [f32; 2],
}

impl Vertex {
    /// Quad NDC dari rect piksel + warna + UV.
    #[allow(clippy::too_many_arguments)]
    pub fn quad(
        x0: f32,
        y0: f32,
        x1: f32,
        y1: f32,
        w: f32,
        h: f32,
        color: [f32; 4],
        uv: (f32, f32, f32, f32),
    ) -> [Vertex; 6] {
        let nx0 = x0 / w * 2.0 - 1.0;
        let ny0 = 1.0 - y0 / h * 2.0;
        let nx1 = x1 / w * 2.0 - 1.0;
        let ny1 = 1.0 - y1 / h * 2.0;
        let (u0, v0, u1, v1) = uv;
        [
            Vertex {
                position: [nx0, ny0],
                color,
                uv: [u0, v0],
            },
            Vertex {
                position: [nx1, ny0],
                color,
                uv: [u1, v0],
            },
            Vertex {
                position: [nx0, ny1],
                color,
                uv: [u0, v1],
            },
            Vertex {
                position: [nx0, ny1],
                color,
                uv: [u0, v1],
            },
            Vertex {
                position: [nx1, ny0],
                color,
                uv: [u1, v0],
            },
            Vertex {
                position: [nx1, ny1],
                color,
                uv: [u1, v1],
            },
        ]
    }

    pub fn desc() -> wgpu::VertexBufferLayout<'static> {
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<Vertex>() as u64,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &[
                wgpu::VertexAttribute {
                    offset: 0,
                    shader_location: 0,
                    format: wgpu::VertexFormat::Float32x2,
                },
                wgpu::VertexAttribute {
                    offset: 8,
                    shader_location: 1,
                    format: wgpu::VertexFormat::Float32x4,
                },
                wgpu::VertexAttribute {
                    offset: 24,
                    shader_location: 2,
                    format: wgpu::VertexFormat::Float32x2,
                },
            ],
        }
    }
}
