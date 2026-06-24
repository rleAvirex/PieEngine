// Sky atmosphere shader with per-pixel ray reconstruction
// Based on "A Scalable and Production Ready Sky and Atmosphere Rendering Technique" (Hillaire 2020)

struct Camera {
    view_proj: mat4x4<f32>,
    position: vec4<f32>,
    world_right: vec4<f32>,
    world_up: vec4<f32>,
    world_forward: vec4<f32>,
    tan_half_fov: f32,
    aspect: f32,
    viewport_width: f32,
    viewport_height: f32,
};

@group(0) @binding(0) var<uniform> camera: Camera;
@group(0) @binding(1) var sun_direction: texture_2d<f32>;
@group(0) @binding(2) var sun_sampler: sampler;

// Atmosphere constants
const RAYLEIGH_BETA: vec3<f32> = vec3<f32>(5.802e-6, 13.558e-6, 33.1e-6);
const MIE_BETA: vec3<f32> = vec3<f32>(3.996e-6);
const RAYLEIGH_HEIGHT: f32 = 8500.0;
const MIE_HEIGHT: f32 = 1200.0;
const MIE_G: f32 = 0.8;
const EARTH_RADIUS: f32 = 6371000.0;
const ATMO_RADIUS: f32 = 6471000.0;
const SUN_INTENSITY: f32 = 22.0;
const NUM_SCATTER_STEPS: u32 = 16;
const NUM_LIGHT_STEPS: u32 = 8;

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
}

@vertex
fn vs_main(@builtin(vertex_index) vertex_index: u32) -> VertexOutput {
    var positions = array<vec2<f32>, 3>(
        vec2<f32>(-1.0, -1.0),
        vec2<f32>(3.0, -1.0),
        vec2<f32>(-1.0, 3.0),
    );
    let pos = positions[vertex_index];
    var output: VertexOutput;
    output.clip_position = vec4<f32>(pos, 0.0, 1.0);
    return output;
}

@fragment
fn fs_main(
    @builtin(position) frag_coord: vec4<f32>,
) -> @location(0) vec4<f32> {
    // Per-pixel NDC reconstruction from fragment coordinates
    let ndc_x = (frag_coord.x / camera.viewport_width) * 2.0 - 1.0;
    let ndc_y = 1.0 - (frag_coord.y / camera.viewport_height) * 2.0;

    let right = camera.world_right.xyz;
    let up = camera.world_up.xyz;
    let fwd = camera.world_forward.xyz;
    let tan_fov = camera.tan_half_fov;
    let asp = camera.aspect;

    let view_ray = normalize(
        fwd
        + right * ndc_x * asp * tan_fov
        + up * ndc_y * tan_fov
    );

    let sky_color = compute_sky_color(view_ray);
    return vec4<f32>(sky_color, 1.0);
}

fn compute_sky_color(view_ray: vec3<f32>) -> vec3<f32> {
    let sun_dir = get_sun_direction();

    // Check if ray hits earth (below horizon)
    let cam_pos = camera.position.xyz;
    let ray_origin = vec3<f32>(0.0, EARTH_RADIUS + 100.0, 0.0);

    // Simple ground check
    if (view_ray.y < 0.0) {
        return vec3<f32>(0.04, 0.04, 0.05);
    }

    // Rayleigh and Mie scattering
    let phase_r = rayleigh_phase(dot(view_ray, sun_dir));
    let phase_m = mie_phase(dot(view_ray, sun_dir), MIE_G);

    // Optical depth
    let view_origin = vec3<f32>(0.0, EARTH_RADIUS + 100.0, 0.0);
    let view_dir = view_ray;

    var optical_depth_r: f32 = 0.0;
    var optical_depth_m: f32 = 0.0;

    let step_size = get_rayleigh_step_size();

    for (var i: u32 = 0u; i < NUM_SCATTER_STEPS; i = i + 1u) {
        if (i >= NUM_SCATTER_STEPS) {
            break;
        }
        let sample_pos = view_origin + view_dir * (f32(i) + 0.5) * step_size;
        let altitude = length(sample_pos) - EARTH_RADIUS;
        if (altitude < 0.0) {
            break;
        }
        optical_depth_r = optical_depth_r + exp(-altitude / RAYLEIGH_HEIGHT) * step_size;
        optical_depth_m = optical_depth_m + exp(-altitude / MIE_HEIGHT) * step_size;
    }

    // Light ray optical depth
    var sun_optical_depth_r: f32 = 0.0;
    var sun_optical_depth_m: f32 = 0.0;
    let sun_step_size = get_sun_step_size();

    for (var j: u32 = 0u; j < NUM_LIGHT_STEPS; j = j + 1u) {
        if (j >= NUM_LIGHT_STEPS) {
            break;
        }
        let sun_sample_pos = view_origin + sun_dir * (f32(j) + 0.5) * sun_step_size;
        let sun_altitude = length(sun_sample_pos) - EARTH_RADIUS;
        if (sun_altitude < 0.0) {
            break;
        }
        sun_optical_depth_r = sun_optical_depth_r + exp(-sun_altitude / RAYLEIGH_HEIGHT) * sun_step_size;
        sun_optical_depth_m = sun_optical_depth_m + exp(-sun_altitude / MIE_HEIGHT) * sun_step_size;
    }

    let tau_r = RAYLEIGH_BETA * (optical_depth_r + sun_optical_depth_r);
    let tau_m = MIE_BETA * 1.1 * (optical_depth_m + sun_optical_depth_m);

    let attenuation = exp(-(tau_r + tau_m));

    let inscatter = SUN_INTENSITY * (attenuation * (phase_r * RAYLEIGH_BETA + phase_m * MIE_BETA));

    // Sun glow
    let sun_dot = max(dot(view_ray, sun_dir), 0.0);
    let sun_glow = pow(sun_dot, 800.0) * 50.0;
    let sun_disk = pow(sun_dot, 256.0) * 15.0;

    var color = vec3<f32>(inscatter) + vec3<f32>(sun_glow + sun_disk) * SUN_INTENSITY * 0.02;

    // Tone mapping (simple Reinhard)
    color = color / (color + vec3<f32>(1.0));
    // Gamma correction
    color = pow(color, vec3<f32>(1.0 / 2.2));

    return color;
}

fn get_sun_direction() -> vec3<f32> {
    // Default sun direction (up-right)
    return normalize(vec3<f32>(0.5, 0.7, 0.3));
}

fn get_rayleigh_step_size() -> f32 {
    return 100.0;
}

fn get_sun_step_size() -> f32 {
    return 500.0;
}

fn rayleigh_phase(cos_theta: f32) -> f32 {
    return 3.0 / (16.0 * 3.14159265) * (1.0 + cos_theta * cos_theta);
}

fn mie_phase(cos_theta: f32, g: f32) -> f32 {
    let g2 = g * g;
    let num = (1.0 - g2);
    let denom = pow(1.0 + g2 - 2.0 * g * cos_theta, 1.5);
    return num / (4.0 * 3.14159265 * denom);
}
