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

/// Fullscreen triangle vertex shader — generates a triangle that covers
/// the entire screen using the trick: vertex ID 0,1,2 maps to positions
/// that form a full-screen quad with a single triangle.
@vertex
fn vs_main(@builtin(vertex_index) vertex_index: u32) -> VertexOutput {
    var output: VertexOutput;
    // Fullscreen triangle: covers [-1,1] clip space
    let x = f32(i32(vertex_index & 1u) * 2 - 1);
    let y = f32(i32(vertex_index >> 1u) * 2 - 1);
    output.clip_position = vec4<f32>(x, y, 0.0, 1.0);
    // UV: flip Y so texture is right-side up
    output.uv = vec2<f32>(f32(i32(vertex_index & 1u)), 1.0 - f32(i32(vertex_index >> 1u)));
    return output;
}

@fragment
fn fs_main(input: VertexOutput) -> @location(0) vec4<f32> {
    // Sample HDR texture in linear space and output directly.
    // The sRGB swapchain applies gamma correction automatically.
    return textureSample(hdr_texture, hdr_sampler, input.uv);
}
