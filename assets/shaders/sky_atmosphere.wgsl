// Sky atmosphere shader — UE5 / Hillaire 2020 style
//
// Single-scattering ray march through a spherical atmosphere with a
// Hillaire 2020 §5 multiple-scattering approximation. Produces the
// characteristic UE5 look: deep blue zenith, warm horizon glow, soft
// aerial perspective, sharp sun disk with subtle halo.
//
// References:
//   "A Scalable and Production Ready Sky and Atmosphere Rendering Technique"
//     — Hillaire 2020
//   "Precomputed Atmospheric Scattering" — Bruneton & Neyret 2008
//
// Coordinate system: the planet center is at the world origin; the +Y axis
// points "up" (away from the planet center). The camera's world-space Y
// coordinate is interpreted as its altitude above sea level in meters and
// converted to km to match the uniform's units.

struct Camera {
    view_proj:      mat4x4<f32>,
    position:       vec4<f32>,
    world_right:    vec4<f32>,
    world_up:       vec4<f32>,
    world_forward:  vec4<f32>,
    tan_half_fov:   f32,
    aspect:         f32,
    // Remaining 8 bytes of the 144-byte CameraUniform — declared so the WGSL
    // struct layout matches the Rust side exactly. The runtime writes zeros
    // here. We reconstruct the view ray from interpolated UVs instead of
    // needing the viewport size (the previous shader divided by zero here).
    _pad0:          f32,
    _pad1:          f32,
};

struct SkyParams {
    // xyz = direction TO the sun in world space (normalized), w unused.
    sun_direction:          vec4<f32>,
    // Multiplier on the final in-scattered radiance. Typically driven by the
    // directional light intensity (e.g. 40 for clear daylight).
    sun_intensity:          f32,
    // Multipliers on the base (sea-level) Rayleigh / Mie scattering
    // coefficients. 1.0 = physically standard Earth atmosphere.
    rayleigh_coefficient:   f32,
    mie_coefficient:        f32,
    // Scale heights (km) — altitude at which density falls by 1/e.
    rayleigh_scale_height:  f32,
    mie_scale_height:       f32,
    // Mie phase function anisotropy (Henyey–Greenstein g). 0.7..0.85 typical.
    mie_directionality:     f32,
    // Planet & atmosphere radii (km).
    planet_radius:          f32,
    atmosphere_radius:      f32,
    // Sample counts for the primary view ray and per-sample sun ray.
    ray_samples:            u32,
    mie_samples:            u32,
    _padding:               vec2<u32>,
};

@group(0) @binding(0) var<uniform> camera: Camera;
@group(0) @binding(1) var<uniform> sky:   SkyParams;

// Base sea-level scattering coefficients (km⁻¹) — Hillaire 2020 / Bruneton.
// Rayleigh is strongly wavelength-dependent (blue dominates → blue sky).
// Mie is slightly warm-tinted aerosol scattering.
const RAYLEIGH_BETA_BASE: vec3<f32> = vec3<f32>(5.802e-3, 13.558e-3, 33.1e-3);
const MIE_BETA_BASE:      vec3<f32> = vec3<f32>(2.10e-2,  2.60e-2,   3.10e-2);

const PI: f32 = 3.141592653589793;

// Sun angular radius ≈ 0.2666°. cos(0.2666°) ≈ 0.999989.
const SUN_ANGULAR_COS: f32 = 0.999989;

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0)       uv:            vec2<f32>,
};

// Fullscreen triangle — covers the entire viewport with a single triangle.
@vertex
fn vs_main(@builtin(vertex_index) vertex_index: u32) -> VertexOutput {
    var positions = array<vec2<f32>, 3>(
        vec2<f32>(-1.0, -1.0),
        vec2<f32>( 3.0, -1.0),
        vec2<f32>(-1.0,  3.0),
    );
    // UVs in [0,1] across the visible viewport: (0,0) bottom-left,
    // (1,1) top-right. Excess (clipped) regions get uv outside [0,1] but
    // those fragments are clipped by the rasterizer.
    var uvs = array<vec2<f32>, 3>(
        vec2<f32>(0.0, 0.0),
        vec2<f32>(2.0, 0.0),
        vec2<f32>(0.0, 2.0),
    );
    let pos = positions[vertex_index];
    var output: VertexOutput;
    output.clip_position = vec4<f32>(pos, 0.0, 1.0);
    output.uv = uvs[vertex_index];
    return output;
}

@fragment
fn fs_main(input: VertexOutput) -> @location(0) vec4<f32> {
    // Reconstruct NDC from interpolated UVs — avoids needing the viewport
    // size in the uniform (the previous shader divided by zero here).
    let ndc_x = input.uv.x * 2.0 - 1.0;
    let ndc_y = input.uv.y * 2.0 - 1.0;

    let right   = camera.world_right.xyz;
    let up      = camera.world_up.xyz;
    let fwd     = camera.world_forward.xyz;
    let tan_fov = camera.tan_half_fov;
    let asp     = camera.aspect;

    let view_ray = normalize(
        fwd
        + right * ndc_x * asp * tan_fov
        + up    * ndc_y        * tan_fov
    );

    // Camera altitude from world-space Y (meters → km). Clamped to keep the
    // camera inside the atmosphere; below-ground cameras are pulled up to
    // just above the surface so the ray march still produces a horizon.
    let alt_km    = clamp(camera.position.y * 0.001, 0.0, sky.atmosphere_radius - sky.planet_radius - 0.01);
    let cam_radius = sky.planet_radius + alt_km;
    let ray_origin = vec3<f32>(0.0, cam_radius, 0.0);

    let sun_dir = normalize(sky.sun_direction.xyz);

    let color = compute_sky_color(ray_origin, view_ray, sun_dir);
    return vec4<f32>(color, 1.0);
}

// --- Ray–sphere intersection ----------------------------------------------
// Returns the smallest positive t such that ro + t*rd lies on the sphere of
// the given radius (centered at the origin), or -1 if no positive hit.
fn ray_sphere_intersect(ro: vec3<f32>, rd: vec3<f32>, radius: f32) -> f32 {
    let b = dot(ro, rd);
    let c = dot(ro, ro) - radius * radius;
    let disc = b * b - c;
    if (disc < 0.0) {
        return -1.0;
    }
    let s = sqrt(disc);
    let t_near = -b - s;
    let t_far  = -b + s;
    if (t_near > 0.0) { return t_near; }
    if (t_far  > 0.0) { return t_far;  }
    return -1.0;
}

// Density of Rayleigh / Mie at a point, where `p` is the position relative
// to the planet center (km). Returns (rayleigh_density, mie_density) relative
// to sea level — i.e. exp(-h/H) for each species.
fn atmosphere_density(p: vec3<f32>) -> vec2<f32> {
    let h = max(length(p) - sky.planet_radius, 0.0);
    return vec2<f32>(
        exp(-h / sky.rayleigh_scale_height),
        exp(-h / sky.mie_scale_height),
    );
}

// Integrated optical depth along a ray from `ro` in direction `rd`, until
// the ray exits the atmosphere or hits the planet. Returns (τ_R, τ_M).
fn compute_optical_depth(ro: vec3<f32>, rd: vec3<f32>, steps: u32) -> vec2<f32> {
    let t_atmo   = ray_sphere_intersect(ro, rd, sky.atmosphere_radius);
    let t_ground = ray_sphere_intersect(ro, rd, sky.planet_radius);

    var t_end = t_atmo;
    if (t_ground > 0.0 && (t_atmo < 0.0 || t_ground < t_atmo)) {
        t_end = t_ground;
    }
    if (t_end < 0.0) {
        return vec2<f32>(0.0);
    }

    var depth = vec2<f32>(0.0);
    let step_size = t_end / f32(steps);
    for (var i: u32 = 0u; i < steps; i = i + 1u) {
        let t = (f32(i) + 0.5) * step_size;
        let p = ro + rd * t;
        // Stop early if we've crossed into the planet — shouldn't happen
        // because t_end clips at the ground, but defensive.
        if (length(p) - sky.planet_radius < 0.0) {
            break;
        }
        depth = depth + atmosphere_density(p) * step_size;
    }
    return depth;
}

fn rayleigh_phase(cos_theta: f32) -> f32 {
    return 3.0 / (16.0 * PI) * (1.0 + cos_theta * cos_theta);
}

fn mie_phase(cos_theta: f32, g: f32) -> f32 {
    let g2 = g * g;
    let denom_base = 1.0 + g2 - 2.0 * g * cos_theta;
    let num    = (1.0 - g2) * (1.0 + cos_theta * cos_theta);
    let denom  = (4.0 * PI) * denom_base * sqrt(max(denom_base, 0.0001));
    return num / denom;
}

// ACES filmic tonemap (Narkowicz fit) — matches the PBR shader so the sky
// and the lit scene end up in the same display space. The sRGB swapchain
// then applies gamma in hardware.
fn aces_tone_map(x: vec3<f32>) -> vec3<f32> {
    let a = 2.51;
    let b = 0.03;
    let c = 2.43;
    let d = 0.59;
    let e = 0.14;
    return clamp((x * (a * x + b)) / (x * (c * x + d) + e), vec3<f32>(0.0), vec3<f32>(1.0));
}

fn compute_sky_color(
    ray_origin: vec3<f32>,
    view_ray:   vec3<f32>,
    sun_dir:    vec3<f32>,
) -> vec3<f32> {
    let beta_r = RAYLEIGH_BETA_BASE * sky.rayleigh_coefficient;
    let beta_m = MIE_BETA_BASE      * sky.mie_coefficient;
    let g      = sky.mie_directionality;

    // Intersect the view ray with the atmosphere and (optionally) the planet.
    let t_atmo   = ray_sphere_intersect(ray_origin, view_ray, sky.atmosphere_radius);
    let t_ground = ray_sphere_intersect(ray_origin, view_ray, sky.planet_radius);

    var t_end = t_atmo;
    var hit_ground = false;
    if (t_ground > 0.0 && (t_atmo < 0.0 || t_ground < t_atmo)) {
        t_end = t_ground;
        hit_ground = true;
    }
    if (t_end < 0.0) {
        // Ray escapes to space — return deep-space black (no atmosphere).
        return vec3<f32>(0.0);
    }

    let steps     = sky.ray_samples;
    let step_size = t_end / f32(steps);

    let cos_theta = dot(view_ray, sun_dir);
    let phase_r   = rayleigh_phase(cos_theta);
    let phase_m   = mie_phase(cos_theta, g);

    var total_inscatter = vec3<f32>(0.0);
    var transmittance   = vec3<f32>(1.0); // camera → current sample

    // ----- Single-scattering ray march -----
    for (var i: u32 = 0u; i < steps; i = i + 1u) {
        let t          = (f32(i) + 0.5) * step_size;
        let sample_pos = ray_origin + view_ray * t;
        if (length(sample_pos) - sky.planet_radius < 0.0) {
            break;
        }

        let density = atmosphere_density(sample_pos);

        // Optical depth of the current step (for transmittance update).
        let view_step_optical = beta_r * (density.x * step_size)
                              + beta_m * (density.y * step_size);

        // Optical depth along the sun ray (sample → atmosphere exit / ground).
        let sun_depth  = compute_optical_depth(sample_pos, sun_dir, sky.mie_samples);
        let sun_optical = beta_r * sun_depth.x + beta_m * sun_depth.y;

        // Sun → sample transmittance. (The camera → sample transmittance is
        // tracked separately in `transmittance`.)
        let sun_atten = exp(-sun_optical);

        // Single-scattering contribution at this sample, attenuated by the
        // camera → sample transmittance.
        let scatter = (phase_r * beta_r * density.x
                     + phase_m * beta_m * density.y) * step_size;
        total_inscatter = total_inscatter + transmittance * sun_atten * scatter;

        // Multiple-scattering approximation (Hillaire 2020 §5, simplified):
        // a small isotropic ambient term proportional to sun transmittance.
        // Without this the horizon is too dark and lacks the warm glow that
        // higher-order scattering produces.
        let ms_ambient = sun_atten * 0.1;
        let ms_scatter = ms_ambient * (beta_r * density.x + beta_m * density.y)
                       * step_size * 0.3;
        total_inscatter = total_inscatter + transmittance * ms_scatter;

        // Advance transmittance for the next sample.
        transmittance = transmittance * exp(-view_step_optical);
    }

    var color = total_inscatter * sky.sun_intensity;

    if (!hit_ground) {
        // ----- Sun disk + halo -----
        // Only rendered when the view ray doesn't hit the planet — the disk
        // is invisible from below the horizon.
        let sun_cos = max(dot(view_ray, sun_dir), 0.0);

        // Sharp disk with a tiny soft edge for AA.
        let disk = smoothstep(SUN_ANGULAR_COS - 0.00002,
                              SUN_ANGULAR_COS + 0.00002,
                              sun_cos);

        // Two-tier halo: tight glow just around the disk + broad warm bloom.
        let halo = pow(sun_cos, 800.0) * 0.05
                 + pow(sun_cos,  64.0) * 0.01;

        // The sun is many orders of magnitude brighter than the sky; we scale
        // to a HDR range that ACES tonemaps to a near-white disk with a soft
        // halo roll-off.
        color = color + vec3<f32>(disk * 50.0 + halo) * sky.sun_intensity;
    } else {
        // ----- Aerial perspective at the horizon -----
        // Where the view ray hits the ground, blend the computed in-scatter
        // (which already encodes the long atmospheric path) with a neutral
        // ground tint modulated by sun transmittance. This gives distant
        // terrain a hazy atmospheric fade instead of a hard cutoff.
        let ground_tint = vec3<f32>(0.18, 0.16, 0.14);
        let sun_depth   = compute_optical_depth(
            ray_origin + view_ray * t_end, sun_dir, sky.mie_samples,
        );
        let sun_atten = exp(-(beta_r * sun_depth.x + beta_m * sun_depth.y));
        color = mix(ground_tint * sun_atten * sky.sun_intensity, color, 0.6);
    }

    // ACES tonemap (matches the PBR shader). Output is in linear display
    // space; the sRGB swapchain applies gamma in hardware.
    color = aces_tone_map(color);

    return color;
}
