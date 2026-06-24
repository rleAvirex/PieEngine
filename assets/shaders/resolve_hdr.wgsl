// Fullscreen resolve shader: blits HDR scene texture to sRGB swapchain
// with screen-space god rays (sun shafts through clouds).
//
// The sRGB swapchain format (Rgba8UnormSrgb) automatically applies
// linear → sRGB gamma correction in hardware, so the shader outputs
// linear values and lets the hardware handle gamma.
//
// God rays: screen-space radial blur from the sun's projected position.
// Marches toward the sun's NDC position, accumulating bright HDR samples.
// Cheap (~16 extra samples per pixel) but only visible when the sun is
// on-screen.

@group(0) @binding(0)
var hdr_texture: texture_2d<f32>;

@group(0) @binding(1)
var hdr_sampler: sampler;

struct ResolveUniform {
    // xy = sun position in NDC [-1, 1], z = god ray intensity (0 = off),
    // w unused.
    sun_ndc: vec4<f32>,
};

@group(0) @binding(2)
var<uniform> u_resolve: ResolveUniform;

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

/// Fullscreen triangle vertex shader using the well-known trick.
@vertex
fn vs_main(@builtin(vertex_index) vid: u32) -> VertexOutput {
    var output: VertexOutput;
    let x = -1.0 + f32(vid & 1u) * 4.0;
    let y = -1.0 + f32(vid >> 1u) * 4.0;
    output.clip_position = vec4<f32>(x, y, 0.0, 1.0);
    output.uv = vec2<f32>(f32(vid & 1u) * 2.0, 1.0 - f32(vid >> 1u) * 2.0);
    return output;
}

@fragment
fn fs_main(input: VertexOutput) -> @location(0) vec4<f32> {
    let base_color = textureSample(hdr_texture, hdr_sampler, input.uv);

    // God rays: only if intensity > 0 (sun is on-screen).
    let god_ray_intensity = u_resolve.sun_ndc.z;
    if (god_ray_intensity <= 0.0) {
        return base_color;
    }

    // Sun position in UV space [0,1].
    let sun_uv = u_resolve.sun_ndc.xy * vec2<f32>(0.5, -0.5) + vec2<f32>(0.5, 0.5);

    // Direction from current pixel to the sun.
    let to_sun = sun_uv - input.uv;
    let dist = length(to_sun);
    if (dist < 0.001) {
        return base_color;
    }
    let dir = to_sun / dist;

    // Radial march: 16 samples toward the sun, accumulating bright HDR
    // values. The decay factor makes closer samples contribute more.
    const NUM_SAMPLES: u32 = 16;
    const MAX_DIST: f32 = 0.8;
    let step_dist = min(dist, MAX_DIST) / f32(NUM_SAMPLES);

    var accumulation = vec3<f32>(0.0);
    var total_weight = 0.0;
    for (var i: u32 = 1u; i <= NUM_SAMPLES; i = i + 1u) {
        let sample_uv = input.uv + dir * step_dist * f32(i);
        let sample_color = textureSample(hdr_texture, hdr_sampler, sample_uv);
        // Only accumulate bright pixels (sky/sun, not dark geometry).
        let luminance = dot(sample_color.rgb, vec3<f32>(0.299, 0.587, 0.114));
        let brightness = smoothstep(0.5, 1.5, luminance);
        // Decay: closer samples weigh more.
        let weight = brightness * (1.0 - f32(i) / f32(NUM_SAMPLES));
        accumulation += sample_color.rgb * weight;
        total_weight += weight;
    }

    if (total_weight > 0.0) {
        let god_rays = accumulation / total_weight;
        // Scale by intensity and distance falloff (stronger near the sun).
        let falloff = smoothstep(MAX_DIST, 0.0, dist);
        let god_ray_color = god_rays * god_ray_intensity * falloff;
        return vec4<f32>(base_color.rgb + god_ray_color, base_color.a);
    }

    return base_color;
}
