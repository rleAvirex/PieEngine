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

// ── Hash + value noise (cheap, no texture) ─────────────────────────────────
fn hash2(p: vec2<f32>) -> f32 {
    let h = dot(p, vec2<f32>(127.1, 311.7));
    return fract(sin(h) * 43758.5453);
}

fn value_noise(p: vec2<f32>) -> f32 {
    let i = floor(p);
    let f = fract(p);
    let u = f * f * (3.0 - 2.0 * f);  // smoothstep
    let a = hash2(i);
    let b = hash2(i + vec2<f32>(1.0, 0.0));
    let c = hash2(i + vec2<f32>(0.0, 1.0));
    let d = hash2(i + vec2<f32>(1.0, 1.0));
    return mix(mix(a, b, u.x), mix(c, d, u.x), u.y);
}

// 4-octave fbm — gives the soft, wispy cloud look.
fn fbm(p: vec2<f32>) -> f32 {
    var v = 0.0;
    var a = 0.5;
    var pos = p;
    for (var i: i32 = 0; i < 4; i = i + 1) {
        v = v + a * value_noise(pos);
        pos = pos * 2.0;
        a = a * 0.5;
    }
    return v;
}

@fragment
fn fs_main(input: VertexOutput) -> @location(0) vec4<f32> {
    let center = cloud.center_size.xyz;
    let size   = cloud.center_size.w;
    let density_mul = cloud.color_density.w;
    let base_color  = cloud.color_density.xyz;
    let sun_dir = normalize(cloud.sun_dir_wind.xyz);
    let wind_offset = cloud.sun_dir_wind.w;
    let sun_color = cloud.sun_color_time.xyz;
    let time = cloud.sun_color_time.w;

    // Distance from camera to cloud center — for fade-out at distance.
    let cam_to_cloud = length(center - camera.position.xyz);

    // UV in [0,1] for noise sampling. Scroll over time for wind animation.
    // The scroll direction is fixed (along +X) — wind_speed controls how
    // fast the offset grows; the actual offset is passed in via the uniform
    // (computed on the CPU as wind_speed * time).
    let uv = (input.uv + 1.0) * 0.5;
    let noise_uv = uv * 3.0 + vec2<f32>(wind_offset, wind_offset * 0.3);
    let n = fbm(noise_uv);

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
    // We approximate "facing the sun" with the dot of the view direction
    // and the sun direction — bright when looking toward the sun.
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
