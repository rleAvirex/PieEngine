// Volumetric cloud shader — UE5 / Schneider-style ray march.
//
// Instead of a flat billboard with one noise sample, this marches a ray
// through the cloud's volume, accumulating density and computing proper
// light transport:
//
//   * Beer-Lambert transmittance: exp(-density × step)
//   * Henyey-Greenstein phase function for anisotropic scattering
//   * Light march toward the sun (6 steps) with Beer-Powder for dark edges
//   * Silver lining at grazing angles (Cody-Schneider term)
//   * Powder effect for denser-looking cloud bottoms
//
// References:
//   Schneider, "The Real-time Volumetric Cloudscapes of Horizon Zero Dawn",
//     SIGGRAPH 2015.
//   Hillaire, "Physically Based Sky, Atmosphere and Cloud Rendering in
//     Frostbite", SIGGRAPH 2020.
//   Heckel, "Real-time Cloudscapes with Volumetric Raymarching" (blog).
//
// The billboard is camera-facing, but the fragment shader marches a ray
// through a virtual volume centered on the billboard. This gives volumetric
// parallax (the cloud "shifts" as you move) at billboard cost.

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

// Ray-march configuration.
const PRIMARY_STEPS:  u32 = 16;   // samples along the view ray through the cloud
const LIGHT_STEPS:    u32 = 4;    // samples toward the sun per primary step
const MARCH_SIZE:     f32 = 0.06; // fraction of cloud size per primary step
const LIGHT_MARCH:    f32 = 0.08; // fraction of cloud size per light step
const DENSITY_THRESHOLD: f32 = 0.35; // below this → no cloud (carves sky)
const ABSORPTION:     f32 = 1.5;  // higher = darker, denser clouds

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0)       uv:            vec2<f32>,   // [-1, 1] across the billboard
    @location(1)       world_pos:     vec3<f32>,
}

@vertex
fn vs_main(@builtin(vertex_index) vertex_index: u32) -> VertexOutput {
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

// ── Henyey-Greenstein phase function ──────────────────────────────────────
// Models anisotropic scattering in clouds. g > 0 = forward scatter (bright
// when looking toward the sun), g < 0 = backward scatter. UE5 uses a
// dual-lobe (forward + backward) but a single forward lobe with g≈0.2 is
// a good cheap approximation.
fn henyey_greenstein(cos_theta: f32, g: f32) -> f32 {
    let g2 = g * g;
    let denom = pow(1.0 + g2 - 2.0 * g * cos_theta, 1.5);
    return (1.0 - g2) / (4.0 * PI * max(denom, 0.0001));
}

// ── Beer-Lambert extinction ───────────────────────────────────────────────
// Transmittance through a medium of given density over a step. The core of
// volumetric cloud lighting — denser cloud = less light passes through.
fn beer_lambert(density: f32, step_size: f32) -> f32 {
    return exp(-density * step_size * ABSORPTION);
}

// ── Powder effect ─────────────────────────────────────────────────────────
// Approximates the fact that thick cloud regions scatter more light back
// toward the viewer at their edges. Combined with Beer-Lambert via
// multiplication, it darkens dense cores while keeping bright fringes.
// Non-physical but visually important (Schneider SIGGRAPH 2015, p.64).
fn powder(density: f32) -> f32 {
    return 1.0 - exp(-density * 4.0);
}

// ── Sample cloud density at a point in cloud-local UVW space ──────────────
// The cloud volume is a unit cube in UVW space [-0.5, 0.5]³. We sample the
// 3D noise texture with wind scroll. A radial falloff softens the edges so
// the cloud is roundish, not cubic.
fn sample_density(uvw: vec3<f32>, wind_offset: f32) -> f32 {
    // Convert to [0,1] texture coords, with wind scroll on all 3 axes
    // (different rates for organic motion).
    let scroll = vec3<f32>(wind_offset, wind_offset * 0.3, wind_offset * 0.15);
    let p_low  = uvw * 2.0 + scroll;
    let p_high = uvw * 5.0 + scroll * 1.7;

    // Two-octave fbm: low-frequency structure + high-frequency detail.
    let n_low  = textureSample(cloud_noise, noise_sampler, p_low).r;
    let n_high = textureSample(cloud_noise, noise_sampler, p_high).r;
    let n = n_low * 0.65 + n_high * 0.35;

    // Radial falloff — sphere of radius 0.5 centered at origin.
    let r = length(uvw);
    let falloff = smoothstep(0.5, 0.1, r);

    // Carve out sky between clouds (density threshold).
    let shaped = smoothstep(DENSITY_THRESHOLD, DENSITY_THRESHOLD + 0.2, n);
    return shaped * falloff;
}

// ── Light march: accumulate transmittance from a point toward the sun ─────
// Marches a short ray toward the sun and returns how much light reaches the
// point. Uses Beer-Lambert + powder for the dark-edge look.
fn light_march(uvw: vec3<f32>, sun_dir_local: vec3<f32>, wind_offset: f32) -> f32 {
    var total_transmittance = 1.0;
    var step_pos = uvw;
    for (var i: u32 = 0u; i < LIGHT_STEPS; i = i + 1u) {
        step_pos = step_pos + sun_dir_local * LIGHT_MARCH;
        let d = sample_density(step_pos, wind_offset);
        if (d > 0.0) {
            // Beer-Lambert × powder: darken dense regions, brighten fringes.
            total_transmittance *= beer_lambert(d, LIGHT_MARCH) * mix(1.0, powder(d), 0.5);
        }
    }
    return 1.0 - total_transmittance;  // luminance reaching this point
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

    // View direction (from camera to fragment).
    let view_dir = normalize(input.world_pos - camera.position.xyz);

    // ── Set up the ray through the cloud volume ────────────────────────────
    // The billboard's UV maps to the cloud's local X/Y. The ray marches
    // along the camera's view direction, so as the camera moves the
    // sampling point shifts through the 3D noise — giving volumetric
    // parallax. The march depth is proportional to cloud size.
    let cam_forward = camera.world_forward.xyz;
    // Project the view direction onto the billboard's local Z (cam forward).
    // This gives us how deep into the volume each pixel samples.
    let depth_scale = abs(dot(view_dir, cam_forward));

    // Build the ray origin in cloud-local UVW space [-0.5, 0.5]³.
    // X/Y come from the billboard UV; Z starts at the near face.
    let local_origin = vec3<f32>(input.uv * 0.5, -0.5);
    // March direction in local space: straight through along Z (camera
    // forward), modulated by view angle for parallax.
    let local_dir = normalize(vec3<f32>(
        dot(view_dir, camera.world_right.xyz) * 0.3,
        dot(view_dir, camera.world_up.xyz) * 0.3,
        1.0,
    ));

    // Sun direction in cloud-local space (for the light march).
    let sun_dir_local = normalize(vec3<f32>(
        dot(sun_dir, camera.world_right.xyz),
        dot(sun_dir, camera.world_up.xyz),
        dot(sun_dir, cam_forward),
    ));

    // ── Henyey-Greenstein phase ───────────────────────────────────────────
    // g = 0.2 → forward scatter. Bright when looking toward the sun.
    let cos_theta = dot(view_dir, sun_dir);
    let phase = henyey_greenstein(cos_theta, 0.2);

    // ── Primary ray march ─────────────────────────────────────────────────
    var total_transmittance = 1.0;
    var total_luminance = vec3<f32>(0.0);

    for (var i: u32 = 0u; i < PRIMARY_STEPS; i = i + 1u) {
        let t = f32(i) * MARCH_SIZE;
        let sample_pos = local_origin + local_dir * t;

        // Discard samples outside the volume sphere.
        if (length(sample_pos) > 0.5) {
            continue;
        }

        let d = sample_density(sample_pos, wind_offset) * density_mul;
        if (d <= 0.001) {
            continue;
        }

        // Beer-Lambert extinction for this step.
        let step_transmittance = beer_lambert(d, MARCH_SIZE);
        // Powder effect — darkens dense cores.
        let powder_term = mix(1.0, powder(d), 0.6);

        // Light march toward the sun from this sample point.
        let light_energy = light_march(sample_pos, sun_dir_local, wind_offset);

        // Silver lining: brighten at grazing angles (where cos_theta ≈ 0).
        // This is the Schneider term that gives clouds their rim light.
        let silver_lining = pow(max(1.0 - abs(cos_theta), 0.0), 8.0) * 0.3;

        // Combine: phase (anisotropic scatter) × light_energy (sun march)
        // × powder (dark edges) + silver lining (bright rim).
        let luminance = (light_energy * phase * powder_term + silver_lining) * d;

        // Accumulate with the current transmittance (front-to-back).
        total_luminance += total_transmittance * luminance * sun_color * base_color;
        total_transmittance *= step_transmittance;

        // Early-out if we're fully opaque — no point continuing.
        if (total_transmittance < 0.01) {
            break;
        }
    }

    // Final color and alpha. Alpha = 1 - transmittance (how much the cloud
    // blocks the sky behind it).
    let alpha = clamp(1.0 - total_transmittance, 0.0, 1.0);
    if (alpha < 0.01) {
        discard;
    }

    // Ambient lift so fully-shadowed cloud parts aren't pitch black.
    let ambient = base_color * 0.15 * sun_color;
    let color = total_luminance + ambient * alpha;

    // Premultiplied alpha for correct compositing over the sky.
    return vec4<f32>(color * alpha, alpha);
}
