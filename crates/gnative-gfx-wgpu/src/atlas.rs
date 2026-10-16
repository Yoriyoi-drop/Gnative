//! Atlas font bitmap 5x7 — teks nyata ganti placeholder.
//!
//! Font 5x7 ASCII 32..126 tertanam (tanpa dependency): tiap glyph 7 baris,
//! bit 4..0 = piksel kiri→kanan. Diraster ke tekstur RGBA 128x48 (sel
//! 8x8, 16 kolom x 6 baris); sel 95 solid putih untuk quad non-teks.
//! Karakter di luar cakupan → `None` (renderer memajukan spasi kosong).

/// Lebar/tinggi glyph dalam piksel atlas.
pub const GLYPH_W: usize = 5;
/// Tinggi glyph.
pub const GLYPH_H: usize = 7;
/// Ukuran sel (glyph + padding).
pub const CELL: usize = 8;
/// Kolom sel per baris atlas.
pub const COLS: usize = 16;
/// Lebar/tinggi tekstur atlas.
pub const ATLAS_W: usize = COLS * CELL;
/// Tinggi tekstur atlas.
pub const ATLAS_H: usize = 6 * CELL;
/// Jumlah glyph (95 ASCII + 1 sel solid).
pub const GLYPH_COUNT: usize = 96;
/// Index sel solid putih.
pub const SOLID_CELL: usize = 95;

/// Tabel font: glyph `i` = karakter ASCII `32 + i` (95 entri).
pub const FONT: [[u8; 7]; 95] = [
    [0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00], // 32 spasi
    [0x04, 0x04, 0x04, 0x04, 0x04, 0x00, 0x04], // !
    [0x0A, 0x0A, 0x00, 0x00, 0x00, 0x00, 0x00], // "
    [0x0A, 0x1F, 0x0A, 0x0A, 0x1F, 0x0A, 0x00], // #
    [0x04, 0x0F, 0x14, 0x0E, 0x05, 0x1E, 0x04], // $
    [0x18, 0x19, 0x02, 0x04, 0x08, 0x13, 0x03], // %
    [0x0C, 0x12, 0x14, 0x08, 0x15, 0x12, 0x0D], // &
    [0x04, 0x04, 0x00, 0x00, 0x00, 0x00, 0x00], // '
    [0x02, 0x04, 0x08, 0x08, 0x08, 0x04, 0x02], // (
    [0x08, 0x04, 0x02, 0x02, 0x02, 0x04, 0x08], // )
    [0x00, 0x04, 0x15, 0x0E, 0x15, 0x04, 0x00], // *
    [0x00, 0x04, 0x04, 0x1F, 0x04, 0x04, 0x00], // +
    [0x00, 0x00, 0x00, 0x00, 0x0C, 0x0C, 0x08], // ,
    [0x00, 0x00, 0x00, 0x1F, 0x00, 0x00, 0x00], // -
    [0x00, 0x00, 0x00, 0x00, 0x00, 0x0C, 0x0C], // .
    [0x01, 0x02, 0x02, 0x04, 0x08, 0x08, 0x10], // /
    [0x0E, 0x11, 0x13, 0x15, 0x19, 0x11, 0x0E], // 0
    [0x04, 0x0C, 0x04, 0x04, 0x04, 0x04, 0x0E], // 1
    [0x0E, 0x11, 0x01, 0x02, 0x04, 0x08, 0x1F], // 2
    [0x1F, 0x02, 0x04, 0x02, 0x01, 0x11, 0x0E], // 3
    [0x02, 0x06, 0x0A, 0x12, 0x1F, 0x02, 0x02], // 4
    [0x1F, 0x10, 0x1E, 0x01, 0x01, 0x11, 0x0E], // 5
    [0x06, 0x08, 0x10, 0x1E, 0x11, 0x11, 0x0E], // 6
    [0x1F, 0x01, 0x02, 0x04, 0x08, 0x08, 0x08], // 7
    [0x0E, 0x11, 0x11, 0x0E, 0x11, 0x11, 0x0E], // 8
    [0x0E, 0x11, 0x11, 0x0F, 0x01, 0x02, 0x0C], // 9
    [0x00, 0x04, 0x04, 0x00, 0x00, 0x04, 0x04], // :
    [0x00, 0x04, 0x04, 0x00, 0x04, 0x04, 0x08], // ;
    [0x02, 0x04, 0x08, 0x10, 0x08, 0x04, 0x02], // <
    [0x00, 0x00, 0x1F, 0x00, 0x1F, 0x00, 0x00], // =
    [0x08, 0x04, 0x02, 0x01, 0x02, 0x04, 0x08], // >
    [0x0E, 0x11, 0x01, 0x02, 0x04, 0x00, 0x04], // ?
    [0x0E, 0x11, 0x17, 0x15, 0x17, 0x10, 0x0E], // @
    [0x0E, 0x11, 0x11, 0x1F, 0x11, 0x11, 0x11], // A
    [0x1E, 0x11, 0x11, 0x1E, 0x11, 0x11, 0x1E], // B
    [0x0E, 0x11, 0x10, 0x10, 0x10, 0x11, 0x0E], // C
    [0x1E, 0x11, 0x11, 0x11, 0x11, 0x11, 0x1E], // D
    [0x1F, 0x10, 0x10, 0x1E, 0x10, 0x10, 0x1F], // E
    [0x1F, 0x10, 0x10, 0x1E, 0x10, 0x10, 0x10], // F
    [0x0E, 0x11, 0x10, 0x17, 0x11, 0x11, 0x0F], // G
    [0x11, 0x11, 0x11, 0x1F, 0x11, 0x11, 0x11], // H
    [0x0E, 0x04, 0x04, 0x04, 0x04, 0x04, 0x0E], // I
    [0x07, 0x02, 0x02, 0x02, 0x02, 0x12, 0x0C], // J
    [0x11, 0x12, 0x14, 0x18, 0x14, 0x12, 0x11], // K
    [0x10, 0x10, 0x10, 0x10, 0x10, 0x10, 0x1F], // L
    [0x11, 0x1B, 0x15, 0x15, 0x11, 0x11, 0x11], // M
    [0x11, 0x19, 0x19, 0x15, 0x13, 0x13, 0x11], // N
    [0x0E, 0x11, 0x11, 0x11, 0x11, 0x11, 0x0E], // O
    [0x1E, 0x11, 0x11, 0x1E, 0x10, 0x10, 0x10], // P
    [0x0E, 0x11, 0x11, 0x11, 0x15, 0x12, 0x0D], // Q
    [0x1E, 0x11, 0x11, 0x1E, 0x14, 0x12, 0x11], // R
    [0x0E, 0x11, 0x10, 0x0E, 0x01, 0x11, 0x0E], // S
    [0x1F, 0x04, 0x04, 0x04, 0x04, 0x04, 0x04], // T
    [0x11, 0x11, 0x11, 0x11, 0x11, 0x11, 0x0E], // U
    [0x11, 0x11, 0x11, 0x11, 0x11, 0x0A, 0x04], // V
    [0x11, 0x11, 0x11, 0x15, 0x15, 0x1B, 0x11], // W
    [0x11, 0x11, 0x0A, 0x04, 0x0A, 0x11, 0x11], // X
    [0x11, 0x11, 0x0A, 0x04, 0x04, 0x04, 0x04], // Y
    [0x1F, 0x01, 0x02, 0x04, 0x08, 0x10, 0x1F], // Z
    [0x0E, 0x08, 0x08, 0x08, 0x08, 0x08, 0x0E], // [
    [0x10, 0x08, 0x08, 0x04, 0x02, 0x02, 0x01], // backslash
    [0x0E, 0x02, 0x02, 0x02, 0x02, 0x02, 0x0E], // ]
    [0x04, 0x0A, 0x11, 0x00, 0x00, 0x00, 0x00], // ^
    [0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x1F], // _
    [0x08, 0x04, 0x00, 0x00, 0x00, 0x00, 0x00], // `
    [0x00, 0x00, 0x0E, 0x01, 0x0F, 0x11, 0x0F], // a
    [0x10, 0x10, 0x1E, 0x11, 0x11, 0x11, 0x1E], // b
    [0x00, 0x00, 0x0E, 0x10, 0x10, 0x11, 0x0E], // c
    [0x01, 0x01, 0x0F, 0x11, 0x11, 0x11, 0x0F], // d
    [0x00, 0x00, 0x0E, 0x11, 0x1F, 0x10, 0x0E], // e
    [0x06, 0x09, 0x08, 0x1C, 0x08, 0x08, 0x08], // f
    [0x00, 0x0F, 0x11, 0x11, 0x0F, 0x01, 0x0E], // g
    [0x10, 0x10, 0x1E, 0x11, 0x11, 0x11, 0x11], // h
    [0x04, 0x00, 0x0C, 0x04, 0x04, 0x04, 0x0E], // i
    [0x02, 0x00, 0x06, 0x02, 0x02, 0x12, 0x0C], // j
    [0x10, 0x10, 0x12, 0x14, 0x18, 0x14, 0x12], // k
    [0x0C, 0x04, 0x04, 0x04, 0x04, 0x04, 0x0E], // l
    [0x00, 0x00, 0x1A, 0x15, 0x15, 0x11, 0x11], // m
    [0x00, 0x00, 0x1E, 0x11, 0x11, 0x11, 0x11], // n
    [0x00, 0x00, 0x0E, 0x11, 0x11, 0x11, 0x0E], // o
    [0x00, 0x00, 0x1E, 0x11, 0x11, 0x1E, 0x10], // p
    [0x00, 0x00, 0x0F, 0x11, 0x11, 0x0F, 0x01], // q
    [0x00, 0x00, 0x16, 0x19, 0x10, 0x10, 0x10], // r
    [0x00, 0x00, 0x0F, 0x10, 0x0E, 0x01, 0x1E], // s
    [0x08, 0x08, 0x1C, 0x08, 0x08, 0x09, 0x06], // t
    [0x00, 0x00, 0x11, 0x11, 0x11, 0x13, 0x0D], // u
    [0x00, 0x00, 0x11, 0x11, 0x11, 0x0A, 0x04], // v
    [0x00, 0x00, 0x11, 0x11, 0x15, 0x15, 0x1A], // w
    [0x00, 0x00, 0x11, 0x0A, 0x04, 0x0A, 0x11], // x
    [0x00, 0x00, 0x11, 0x11, 0x0F, 0x01, 0x0E], // y
    [0x00, 0x00, 0x1F, 0x02, 0x04, 0x08, 0x1F], // z
    [0x02, 0x04, 0x04, 0x08, 0x04, 0x04, 0x02], // {
    [0x04, 0x04, 0x04, 0x00, 0x04, 0x04, 0x04], // |
    [0x08, 0x04, 0x04, 0x02, 0x04, 0x04, 0x08], // }
    [0x00, 0x00, 0x08, 0x15, 0x02, 0x00, 0x00], // ~
];

/// Index glyph untuk karakter (ASCII 32..126); di luar itu `None`.
pub fn glyph_index(c: char) -> Option<usize> {
    let code = c as u32;
    if (32..=126).contains(&code) {
        Some((code - 32) as usize)
    } else {
        None
    }
}

/// Piksel glyph menyala? `(gx 0..5, gy 0..7)`.
pub fn glyph_pixel(index: usize, gx: usize, gy: usize) -> bool {
    if index >= FONT.len() || gx >= GLYPH_W || gy >= GLYPH_H {
        return false;
    }
    (FONT[index][gy] >> (4 - gx)) & 1 == 1
}

/// UV sel atlas `(u0, v0, u1, v1)` untuk glyph (origin kiri-atas).
/// Sel solid putih (quad non-teks) lewat `solid_uv()`.
pub fn glyph_uv(index: usize) -> (f32, f32, f32, f32) {
    cell_uv(index)
}

/// UV sel solid putih (mask = 1 penuh).
pub fn solid_uv() -> (f32, f32, f32, f32) {
    cell_uv(SOLID_CELL)
}

/// UV sel `cell` (0..96).
fn cell_uv(cell: usize) -> (f32, f32, f32, f32) {
    let col = (cell % COLS) as f32;
    let row = (cell / COLS) as f32;
    (
        col * CELL as f32 / ATLAS_W as f32,
        row * CELL as f32 / ATLAS_H as f32,
        (col * CELL as f32 + CELL as f32) / ATLAS_W as f32,
        (row * CELL as f32 + CELL as f32) / ATLAS_H as f32,
    )
}

/// Raster seluruh atlas ke RGBA8 (putih menutup, transparan buka).
/// Murni (tanpa GPU) agar bisa diuji headless.
pub fn build_image() -> Vec<u8> {
    let mut img = vec![0u8; ATLAS_W * ATLAS_H * 4];
    for cell in 0..GLYPH_COUNT {
        let solid = cell == SOLID_CELL;
        for y in 0..CELL {
            for x in 0..CELL {
                let on = if solid {
                    true
                } else if x < GLYPH_W && y < GLYPH_H {
                    glyph_pixel(cell, x, y)
                } else {
                    false
                };
                let px = (cell % COLS) * CELL + x;
                let py = (cell / COLS) * CELL + y;
                let o = (py * ATLAS_W + px) * 4;
                if on {
                    img[o] = 255;
                    img[o + 1] = 255;
                    img[o + 2] = 255;
                    img[o + 3] = 255;
                }
            }
        }
    }
    img
}

/// Tekstur atlas di GPU + bind group (dibuat sekali per renderer).
pub struct Atlas {
    /// Layout bind group (dipakai pipeline renderer).
    pub layout: wgpu::BindGroupLayout,
    bind_group: wgpu::BindGroup,
}

impl Atlas {
    /// Unggah citra atlas ke GPU.
    pub fn new(device: &wgpu::Device, queue: &wgpu::Queue) -> Self {
        let img = build_image();
        let size = wgpu::Extent3d {
            width: ATLAS_W as u32,
            height: ATLAS_H as u32,
            depth_or_array_layers: 1,
        };
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("gnative-atlas"),
            size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            &img,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(4 * ATLAS_W as u32),
                rows_per_image: Some(ATLAS_H as u32),
            },
            size,
        );
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("gnative-atlas-sampler"),
            mag_filter: wgpu::FilterMode::Nearest,
            min_filter: wgpu::FilterMode::Nearest,
            ..Default::default()
        });
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("gnative-atlas-layout"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("gnative-atlas-group"),
            layout: &layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&sampler),
                },
            ],
        });
        Self { layout, bind_group }
    }

    /// Bind group untuk `set_bind_group` saat render pass.
    pub fn bind_group(&self) -> &wgpu::BindGroup {
        &self.bind_group
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tabel_95_glyph() {
        assert_eq!(FONT.len(), 95);
    }

    #[test]
    fn ascii_terpetakan() {
        assert_eq!(glyph_index('A'), Some(33));
        assert_eq!(glyph_index(' '), Some(0));
        assert_eq!(glyph_index('~'), Some(94));
        assert_eq!(glyph_index('👋'), None);
        assert_eq!(glyph_index('é'), None);
    }

    #[test]
    fn piksel_a_masuk_akal() {
        // Baris 3 'A' = 0x1F (penuh), baris 0 = 0x0E (tengah).
        assert!(glyph_pixel(33, 0, 3));
        assert!(glyph_pixel(33, 4, 3));
        assert!(!glyph_pixel(33, 0, 0));
        assert!(glyph_pixel(33, 1, 0));
        assert!(!glyph_pixel(0, 2, 2)); // spasi kosong
    }

    #[test]
    fn uv_dalam_rentang() {
        for i in [0, 33, 94] {
            let (u0, v0, u1, v1) = glyph_uv(i);
            assert!(0.0 <= u0 && u0 < u1 && u1 <= 1.0);
            assert!(0.0 <= v0 && v0 < v1 && v1 <= 1.0);
        }
        let (u0, v0, u1, v1) = solid_uv();
        assert!(0.0 <= u0 && u0 < u1 && u1 <= 1.0);
        assert!(0.0 <= v0 && v0 < v1 && v1 <= 1.0);
    }

    #[test]
    fn citra_terisi_benar() {
        let img = build_image();
        assert_eq!(img.len(), ATLAS_W * ATLAS_H * 4);
        // Piksel glyph 'A' (sel 33: kolom 1, baris 2) menyala.
        let ax = CELL;
        let ay = 2 * CELL + 3; // baris glyph 3 (penuh)
        let o = (ay * ATLAS_W + ax) * 4;
        assert_eq!(img[o + 3], 255);
        // Spasi (sel 0) transparan.
        assert_eq!(img[3], 0);
        // Sel solid penuh.
        let sx = (SOLID_CELL % COLS) * CELL;
        let sy = (SOLID_CELL / COLS) * CELL;
        let o = (sy * ATLAS_W + sx) * 4;
        assert_eq!(&img[o..o + 4], &[255, 255, 255, 255]);
    }
}
