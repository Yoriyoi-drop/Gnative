//! Shader WGSL inline (vertex + fragment, alpha blending + atlas teks + SDF).

/// Vertex shader: posisi 2D → clip space, teruskan warna + UV + geometri SDF.
pub const VERTEX_SHADER: &str = r#"
struct VsOut {
    @builtin(position) pos: vec4f,
    @location(0) color: vec4f,
    @location(1) uv: vec2f,
    @location(2) local: vec2f,
    @location(3) half: vec2f,
    @location(4) radius: f32,
}

@vertex
fn vs_main(
    @location(0) position: vec2f,
    @location(1) color: vec4f,
    @location(2) uv: vec2f,
    @location(3) local: vec2f,
    @location(4) half: vec2f,
    @location(5) radius: f32,
) -> VsOut {
    var out: VsOut;
    out.pos = vec4f(position, 0.0, 1.0);
    out.color = color;
    out.uv = uv;
    out.local = local;
    out.half = half;
    out.radius = radius;
    return out;
}
"#;

/// Fragment shader: warna × mask atlas × mask SDF (sudut membulat, AA 1px).
pub const FRAGMENT_SHADER: &str = r#"
@group(0) @binding(0) var atlas_tex: texture_2d<f32>;
@group(0) @binding(1) var atlas_smp: sampler;

@fragment
fn fs_main(
    @location(0) color: vec4f,
    @location(1) uv: vec2f,
    @location(2) local: vec2f,
    @location(3) half: vec2f,
    @location(4) radius: f32,
) -> @location(0) vec4f {
    let mask = textureSample(atlas_tex, atlas_smp, uv).a;
    // Jarak bertanda ke rounded box (radius 0 → box biasa).
    let q = abs(local) - (half - vec2f(radius, radius));
    let d = length(max(q, vec2f(0.0, 0.0))) + min(max(q.x, q.y), 0.0) - radius;
    let aa = clamp(0.5 - d, 0.0, 1.0);
    return vec4f(color.rgb, color.a * mask * aa);
}
"#;
