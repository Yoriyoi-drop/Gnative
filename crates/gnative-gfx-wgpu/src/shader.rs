//! Shader WGSL inline (vertex + fragment, alpha blending + atlas teks).

/// Vertex shader: posisi 2D → clip space, teruskan warna + UV.
pub const VERTEX_SHADER: &str = r#"
struct VsOut {
    @builtin(position) pos: vec4f,
    @location(0) color: vec4f,
    @location(1) uv: vec2f,
}

@vertex
fn vs_main(
    @location(0) position: vec2f,
    @location(1) color: vec4f,
    @location(2) uv: vec2f,
) -> VsOut {
    var out: VsOut;
    out.pos = vec4f(position, 0.0, 1.0);
    out.color = color;
    out.uv = uv;
    return out;
}
"#;

/// Fragment shader: warna × mask atlas (teks bergerigi tajam, quad solid penuh).
pub const FRAGMENT_SHADER: &str = r#"
@group(0) @binding(0) var atlas_tex: texture_2d<f32>;
@group(0) @binding(1) var atlas_smp: sampler;

@fragment
fn fs_main(@location(0) color: vec4f, @location(1) uv: vec2f) -> @location(0) vec4f {
    let mask = textureSample(atlas_tex, atlas_smp, uv).a;
    return vec4f(color.rgb, color.a * mask);
}
"#;
