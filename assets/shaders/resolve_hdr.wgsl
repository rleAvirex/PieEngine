// Fullscreen resolve shader: blits HDR scene texture to sRGB swapchain.
// The sRGB swapchain format (Rgba8UnormSrgb) automatically applies
// linear → sRGB gamma correction in hardware, so the shader just
// samples the HDR texture and passes the value through unchanged.

@group(0) @binding(0)
var hdr_texture: texture_2d<f32>;

@group(0) @binding(1)
var hdr_sampler: sampler;

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

/// Fullscreen triangle vertex shader using the well-known trick.
/// Generates a single triangle that covers the entire viewport:
///   vertex 0: position (-1, -1), uv (0, 1)   — bottom-left
///   vertex 1: position ( 3, -1), uv (2, 1)   — bottom-right (far right)
///   vertex 2: position (-1,  3), uv (0, -1)   — top-left (far top)
/// The rasterizer clips to the viewport, effectively producing a full-screen quad.
@vertex
fn vs_main(@builtin(vertex_index) vid: u32) -> VertexOutput {
    var output: VertexOutput;
    // Map vertex index to clip-space position:
    // vid=0 → (-1,-1), vid=1 → (3,-1), vid=2 → (-1,3)
    let x = -1.0 + f32(vid & 1u) * 4.0;
    let y = -1.0 + f32(vid >> 1u) * 4.0;
    output.clip_position = vec4<f32>(x, y, 0.0, 1.0);
    // UV: maps the visible viewport (0,0)-(1,1), Y-flipped for texture
    output.uv = vec2<f32>(f32(vid & 1u) * 2.0, 1.0 - f32(vid >> 1u) * 2.0);
    return output;
}

@fragment
fn fs_main(input: VertexOutput) -> @location(0) vec4<f32> {
    // Sample HDR texture in linear space and output directly.
    // The sRGB swapchain applies gamma correction automatically.
    return textureSample(hdr_texture, hdr_sampler, input.uv);
}
