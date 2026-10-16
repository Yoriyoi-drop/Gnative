//! Vertex — posisi 2D + warna + UV atlas + geometri SDF (52 byte, `#[repr(C)]`).

use bytemuck::{Pod, Zeroable};

/// Vertex: posisi NDC + warna RGBA + UV atlas + info sudut membulat (SDF).
///
/// `local`/`half`/`radius` (piksel) dipakai fragment shader untuk memotong
/// sudut quad (P2-11: radius kini benar-benar dirender, bukan disimpan saja).
#[repr(C)]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
pub struct Vertex {
    pub position: [f32; 2],
    pub color: [f32; 4],
    pub uv: [f32; 2],
    /// Posisi piksel relatif terhadap pusat quad.
    pub local: [f32; 2],
    /// Setengah ukuran quad (piksel).
    pub half: [f32; 2],
    /// Radius sudut (piksel; 0 = siku).
    pub radius: f32,
}

impl Vertex {
    /// Quad NDC dari rect piksel + warna + UV + radius sudut.
    ///
    /// P2-11: `w`/`h` nol atau non-finite tidak lagi menghasilkan NaN/Inf
    /// (dijaga ke nilai aman); koordinat/warna non-finite di-nol-kan.
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
        radius: f32,
    ) -> [Vertex; 6] {
        let w = safe_extent(w);
        let h = safe_extent(h);
        let x0 = safe_coord(x0);
        let y0 = safe_coord(y0);
        let x1 = safe_coord(x1);
        let y1 = safe_coord(y1);
        let color = [
            safe_unit(color[0]),
            safe_unit(color[1]),
            safe_unit(color[2]),
            safe_unit(color[3]),
        ];
        let (u0, v0, u1, v1) = uv;
        let (u0, v0, u1, v1) = (
            safe_coord(u0),
            safe_coord(v0),
            safe_coord(u1),
            safe_coord(v1),
        );

        let nx0 = x0 / w * 2.0 - 1.0;
        let ny0 = 1.0 - y0 / h * 2.0;
        let nx1 = x1 / w * 2.0 - 1.0;
        let ny1 = 1.0 - y1 / h * 2.0;

        // Geometri SDF: setengah ukuran + radius dijepit agar `half - radius ≥ 0`.
        let hw = ((x1 - x0).abs() * 0.5).max(0.5);
        let hh = ((y1 - y0).abs() * 0.5).max(0.5);
        let radius = safe_coord(radius).max(0.0).min(hw.min(hh));
        let half = [hw, hh];
        let mk = |pos: [f32; 2], uv: [f32; 2], local: [f32; 2]| Vertex {
            position: pos,
            color,
            uv,
            local,
            half,
            radius,
        };
        let l00 = [-hw, -hh];
        let l10 = [hw, -hh];
        let l01 = [-hw, hh];
        let l11 = [hw, hh];
        [
            mk([nx0, ny0], [u0, v0], l00),
            mk([nx1, ny0], [u1, v0], l10),
            mk([nx0, ny1], [u0, v1], l01),
            mk([nx0, ny1], [u0, v1], l01),
            mk([nx1, ny0], [u1, v0], l10),
            mk([nx1, ny1], [u1, v1], l11),
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
                wgpu::VertexAttribute {
                    offset: 32,
                    shader_location: 3,
                    format: wgpu::VertexFormat::Float32x2,
                },
                wgpu::VertexAttribute {
                    offset: 40,
                    shader_location: 4,
                    format: wgpu::VertexFormat::Float32x2,
                },
                wgpu::VertexAttribute {
                    offset: 48,
                    shader_location: 5,
                    format: wgpu::VertexFormat::Float32,
                },
            ],
        }
    }
}

/// Lebar/tinggi aman untuk pembagi NDC: finite dan > 0 (else 1.0).
fn safe_extent(v: f32) -> f32 {
    if v.is_finite() && v > 0.0 {
        v
    } else {
        1.0
    }
}

/// Koordinat finite (NaN/Inf → 0.0) agar tak bocor ke GPU.
fn safe_coord(v: f32) -> f32 {
    if v.is_finite() {
        v
    } else {
        0.0
    }
}

/// Komponen warna finite (clamp 0..=1).
fn safe_unit(v: f32) -> f32 {
    if v.is_finite() {
        v.clamp(0.0, 1.0)
    } else {
        0.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn all_finite(v: &[Vertex; 6]) -> bool {
        v.iter().all(|vx| {
            vx.position.iter().all(|c| c.is_finite())
                && vx.color.iter().all(|c| c.is_finite())
                && vx.local.iter().all(|c| c.is_finite())
                && vx.half.iter().all(|c| c.is_finite())
                && vx.radius.is_finite()
        })
    }

    #[test]
    fn quad_normal_finite() {
        let v = Vertex::quad(
            0.0,
            0.0,
            10.0,
            10.0,
            100.0,
            100.0,
            [1.0; 4],
            (0.0, 0.0, 1.0, 1.0),
            0.0,
        );
        assert!(all_finite(&v));
    }

    #[test]
    fn quad_nol_ukuran_tidak_nan() {
        // P2-11: w/h nol (div0) tidak boleh hasilkan NaN/Inf.
        let v = Vertex::quad(
            0.0,
            0.0,
            10.0,
            10.0,
            0.0,
            0.0,
            [1.0; 4],
            (0.0, 0.0, 1.0, 1.0),
            0.0,
        );
        assert!(all_finite(&v), "quad div0 harus finite");
    }

    #[test]
    fn quad_non_finite_disanitasi() {
        let v = Vertex::quad(
            f32::NAN,
            f32::INFINITY,
            10.0,
            10.0,
            100.0,
            100.0,
            [f32::NAN, 2.0, -1.0, 1.0],
            (0.0, 0.0, 1.0, 1.0),
            0.0,
        );
        assert!(all_finite(&v), "input non-finite harus disanitasi");
        assert_eq!(v[0].color, [0.0, 1.0, 0.0, 1.0]);
    }

    #[test]
    fn radius_dijepit_dan_diteruskan() {
        // P2-11: radius dijepit ≤ min(half) dan diteruskan ke vertex.
        let v = Vertex::quad(
            0.0,
            0.0,
            20.0,
            10.0,
            100.0,
            100.0,
            [1.0; 4],
            (0.0, 0.0, 1.0, 1.0),
            100.0,
        );
        assert_eq!(v[0].radius, 5.0, "radius dijepit ke min(half)=5");
        assert_eq!(v[0].half, [10.0, 5.0]);
    }
}
