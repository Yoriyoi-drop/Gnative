//! Statistik satu frame yang dirender.

/// Statistik frame — dipakai laporan performa & debugging.
#[derive(Debug, Clone, Default)]
pub struct FrameStats {
    pub draw_calls: usize,
    pub vertices: usize,
    pub triangles: usize,
    /// Waktu present (ms).
    pub present_ms: f32,
}
