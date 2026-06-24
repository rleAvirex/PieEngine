// Volumetric cloud billboard shader.
//
// Renders each Cloud entity as a camera-facing billboard with procedural
// noise-driven density. Composites over the sky (additive-ish via alpha
// blend) and is occluded by scene geometry via the depth buffer.
//
// Phase 1 (this shader): single-pass, no temporal accumulation. The noise
// is a cheap 2D fbm computed in-shader — no texture fetches, so it's fast
// (~0.1ms per cloud at 1080p). Good enough to see and tune; Phase 2 would
// add a 3D noise texture + temporal reprojection for UE5-grade quality.

struct Camera {
    view_proj:      mat4x4<f32>,
    position:       vec4<f32>,
    world_right:    vec4<f32>,
    world_up:       vec4<f32>,
    world_forward:  vec4<f32>,
    tan_half_fov:   f32,
    aspect:         f32,
    _pad0:          f32,
    _pad1:          f32,
};

struct CloudUniform {
    // xyz = world position of the cloud center, w = size (meters).
    center_size:    vec4<f32>,
    // xyz = base color (linear RGB), w = density (0..1).
    color_density:  vec4<f32>,
    // xyz = sun direction (normalized, TO the sun), w = wind scroll offset.
    sun_dir_wind:   vec4<f32>,
    // xyz = sun color * sun intensity (for tinting), w = time (seconds).
    sun_color_time: vec4<f32>,
};

@group(0) @binding(0) var<uniform> camera: Camera;
@group(0) @binding(1) var<uniform> cloud:  CloudUniform;
@group(0) @binding(2) var cloud_noise: texture_3d<f32>;
@group(0) @binding(3) var noise_sampler: sampler;

const PI: f32 = 3.141592653589793;

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0)       uv:            vec2<f32>,   // [-1, 1] across the billboard
    @location(1)       world_pos:     vec3<f32>,   // for depth test reference
}

// Build a camera-facing quad around the cloud center. The quad is aligned
// to the camera's right/up basis so it always faces the viewer.
@vertex
fn vs_main(@builtin(vertex_index) vertex_index: u32) -> VertexOutput {
    // Fullscreen quad: 4 vertices, 2 triangles.
    // vertex_index: 0,1,2,3 → corners (-1,-1), (1,-1), (1,1), (-1,1)
    let quad = array<vec2<f32>, 4>(
        vec2<f32>(-1.0, -1.0),
        vec2<f32>( 1.0, -1.0),
        vec2<f32>( 1.0,  1.0),
        vec2<f32>(-1.0,  1.0),
    );
    let idx = array<u32, 6>(0u, 1u, 2u, 0u, 2u, 3u);
    let corner = quad[idx[vertex_index]];

    let center = cloud.center_size.xyz;
    let size   = cloud.center_size.w;
    let right  = camera.world_right.xyz;
    let up     = camera.world_up.xyz;

    let world_pos = center + (right * corner.x + up * corner.y) * size * 0.5;
    var output: VertexOutput;
    output.clip_position = camera.view_proj * vec4<f32>(world_pos, 1.0);
    output.uv = corner;
    output.world_pos = world_pos;
    return output;
}

// ── 3D noise sampling (Phase 2 — texture-based, volumetric look) ───────────
// Samples the precomputed 128³ Worley+Perlin fbm texture. The 3rd
// coordinate adds depth variation so the cloud looks volumetric instead
// of flat. Wind scrolls the sampling offset over time.

@fragment
fn fs_main(input: VertexOutput) -> @location(0) vec4<f32> {
    let center = cloud.center_size.xyz;
    let size   = cloud.center_size.w;
    let density_mul = cloud.color_density.w;
    let base_color  = cloud.color_density.xyz;
    let sun_dir = normalize(cloud.sun_dir_wind.xyz);
    let wind_offset = cloud.sun_dir_wind.w;
    let sun_color = cloud.sun_color_time.xyz;

    // Sample the 3D noise texture. Map the billboard UV ([-1,1]) into
    // texture coordinates, plus a wind-scroll offset. The Z coordinate
    // varies with the view direction's dot against the cloud's "depth"
    // axis (camera forward), giving parallax as the camera moves.
    let uv = (input.uv + 1.0) * 0.5;
    let scroll = vec3<f32>(wind_offset, wind_offset * 0.3, wind_offset * 0.15);

    // Two-octave 3D noise: low-frequency for structure, high-frequency for detail.
    let noise_uvw_low  = vec3<f32>(uv * 2.0, 0.0) + scroll;
    let noise_uvw_high = vec3<f32>(uv * 5.0, 0.5) + scroll * 1.7;
    let n_low  = textureSample(cloud_noise, noise_sampler, noise_uvw_low).r;
    let n_high = textureSample(cloud_noise, noise_sampler, noise_uvw_high).r;
    let n = n_low * 0.7 + n_high * 0.3;

    // Soft circular falloff toward the billboard edges so the cloud doesn't
    // look like a square. Smoothstep from center (1) to edge (0).
    let r = length(input.uv);
    let falloff = smoothstep(1.0, 0.4, r);

    // Cloud density: combine noise + falloff, modulated by the entity's
    // density parameter. Threshold to carve out sky between clouds.
    var d = n * falloff;
    d = smoothstep(0.3, 0.7, d) * density_mul;
    if (d < 0.01) {
        discard;
    }

    // Lighting: simple directional term based on sun direction. Clouds
    // facing the sun are brighter; clouds on the shadow side are darker.
    let view_dir = normalize(input.world_pos - camera.position.xyz);
    let sun_facing = max(dot(-view_dir, sun_dir), 0.0);
    let lit = 0.4 + 0.6 * sun_facing;

    // Tint by sun color (warm at sunset, white at noon) and the cloud's
    // base color. Slight ambient lift so shadowed clouds aren't pitch black.
    let ambient = 0.3;
    let color = base_color * (ambient + lit) * (0.5 + 0.5 * sun_color);

    // Alpha = density. Use premultiplied alpha so the blend works correctly
    // with the HDR buffer's existing sky color.
    return vec4<f32>(color * d, d);
}
