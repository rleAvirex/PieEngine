// Sky Atmosphere shader for Pie Engine
//
// Implements a simplified but physically-inspired sky model based on:
//   - Rayleigh scattering (molecules, short wavelengths scatter more → blue sky)
//   - Mie scattering (aerosols, forward-peaked → sun disk glow / haze)
//
// This is a single-pass fullscreen triangle shader. The sky dome is computed
// per-pixel by raytracing through a simplified atmospheric shell.
//
// Performance notes:
//   - RAY_STEP_COUNT, MIE_STEP_COUNT are configurable via uniform.
//     Low=8/4, Medium=16/8, High=32/16
//   - At max settings the full cost is ~64 ray-march steps per pixel.
//     For 1080p that's ~132M steps — still fast on modern GPUs.
//   - The shader is only used for the sky background, not per-object.

struct Camera {
    view_proj: mat4x4<f32>,
    position: vec4<f32>,
    // Camera basis vectors in world space for ray reconstruction
    // (avoids needing inverse(view_proj) which doesn't exist for mat4x4 in WGSL)
    world_right: vec4<f32>,
    world_up: vec4<f32>,
    world_forward: vec4<f32>,
    tan_half_fov: f32,
    aspect: f32,
};

// Sun data — direction comes from the scene's DirectionalLight
struct SkyParams {
    sun_direction: vec4<f32>,     // xyz = direction TO the sun, w = unused
    sun_intensity: f32,          // intensity multiplier
    rayleigh_coefficient: f32,   // Rayleigh scattering strength (default ~5.5e-6)
    mie_coefficient: f32,        // Mie scattering strength (default ~2.0e-5)
    rayleigh_scale_height: f32,  // Rayleigh density falloff altitude (default 8.0 km)
    mie_scale_height: f32,       // Mie density falloff altitude (default 1.2 km)
    mie_directionality: f32,     // Henyey-Greenstein g parameter (default 0.8)
    planet_radius: f32,          // Planet radius in km (default 6360.0)
    atmosphere_radius: f32,      // Atmosphere outer radius in km (default 6460.0)
    ray_samples: u32,            // Number of ray-march steps for Rayleigh/Mie
    mie_samples: u32,            // Number of ray-march steps for Mie only
};

@group(0) @binding(0) var<uniform> camera: Camera;
@group(0) @binding(1) var<uniform> sky: SkyParams;

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) world_ray: vec3<f32>,  // world-space ray direction
};

// ─── Phase functions ────────────────────────────────────────────────────────

// Henyey-Greenstein phase function for Mie scattering
fn hg_phase(cos_theta: f32, g: f32) -> f32 {
    let g2 = g * g;
    let denom = 1.0 + g2 - 2.0 * g * cos_theta;
    return (1.0 - g2) / (4.0 * 3.14159265 * pow(denom, 1.5));
}

// Rayleigh phase function (symmetric forward/backward)
fn rayleigh_phase(cos_theta: f32) -> f32 {
    return 3.0 / (16.0 * 3.14159265) * (1.0 + cos_theta * cos_theta);
}

// ─── Density sampling ───────────────────────────────────────────────────────

// Approximate atmospheric density at a given altitude.
// Returns (rayleigh_density, mie_density) as normalized values.
fn atmosphere_density(altitude: f32) -> vec2<f32> {
    let hr = sky.rayleigh_scale_height;
    let hm = sky.mie_scale_height;
    let ray_density = exp(-altitude / hr);
    let mie_density = exp(-altitude / hm);
    return vec2<f32>(ray_density, mie_density);
}

// ─── Ray-sphere intersection ────────────────────────────────────────────────

// Returns distance along ray to enter the atmosphere shell, or -1 if no hit.
// ray_origin and ray_dir should be in world space.
fn ray_atmosphere_entry(ray_origin: vec3<f32>, ray_dir: vec3<f32>) -> f32 {
    let r = sky.atmosphere_radius;
    let oc = ray_origin;
    let a = dot(ray_dir, ray_dir);
    let b = 2.0 * dot(oc, ray_dir);
    let c = dot(oc, oc) - r * r;
    let discriminant = b * b - 4.0 * a * c;
    if discriminant < 0.0 {
        return -1.0;
    }
    let sqrt_disc = sqrt(discriminant);
    let t = (-b - sqrt_disc) / (2.0 * a);
    return max(t, 0.0);
}

// Returns distance along ray to exit the atmosphere shell, or -1 if no hit.
// Used together with `ray_atmosphere_entry` to compute the total march length.
fn ray_atmosphere_exit(ray_origin: vec3<f32>, ray_dir: vec3<f32>) -> f32 {
    let r = sky.atmosphere_radius;
    let oc = ray_origin;
    let a = dot(ray_dir, ray_dir);
    let b = 2.0 * dot(oc, ray_dir);
    let c = dot(oc, oc) - r * r;
    let discriminant = b * b - 4.0 * a * c;
    if discriminant < 0.0 {
        return -1.0;
    }
    let sqrt_disc = sqrt(discriminant);
    return (-b + sqrt_disc) / (2.0 * a);
}

// ─── Sky color computation ──────────────────────────────────────────────────

fn compute_sky_color(ray_dir: vec3<f32>) -> vec3<f32> {
    let sun_dir = normalize(sky.sun_direction.xyz);

    // UE5-style sky dome: the sky color depends ONLY on the view ray direction,
    // never on the camera position. Using the actual camera position makes the
    // horizon tilt / shift as the camera moves, which looks wrong for a sky
    // dome attached to the world. Fix the ray origin at the north-pole surface
    // point (planet_radius + epsilon on +Y) so every view ray produces the same
    // color regardless of where the camera is.
    let surface_alt = sky.planet_radius + 0.001;
    let ray_origin = vec3<f32>(0.0, surface_alt, 0.0);

    // Scattering coefficients — defined here (before the ray-march loop) so
    // the transmittance formulas can use them. The old code used
    // `sky.rayleigh_coefficient` (the raw multiplier, 800) directly as the
    // extinction coefficient, which was ~45000× too high (800 vs 17.9e-3 per
    // km for blue). That made transmittance exp(-800 * 8) = 0 for any
    // meaningful path → no inscatter → black sky.
    //
    // The base values (5.5e-6, 13e-6, 22.4e-6 for Rayleigh, 2e-5 for Mie)
    // times the user multiplier (800, 400) give the scattering coefficient
    // per km. For Rayleigh blue at multiplier 800: 22.4e-6 * 800 = 17.9e-3
    // per km. For an 8 km upward path: exp(-17.9e-3 * 8) = 0.87 transmittance
    // — most light gets through, giving a visible blue sky.
    let rayleigh_scattering =
        vec3<f32>(5.5e-6, 13.0e-6, 22.4e-6) * sky.rayleigh_coefficient;
    let mie_scattering = vec3<f32>(2.0e-5 * sky.mie_coefficient);

    // Find where the ray enters the atmosphere
    let t_entry = ray_atmosphere_entry(ray_origin, ray_dir);
    if t_entry < 0.0 {
        // Ray doesn't hit atmosphere — return black (space)
        return vec3<f32>(0.0, 0.0, 0.0);
    }

    // Compute the total distance to march: from where the ray enters the
    // atmosphere (or from the camera, if it's already inside) to where it
    // exits. The old code used `total_distance = t_entry`, which (a) was 0
    // whenever the camera was already inside the atmosphere (the normal
    // case), collapsing all samples onto the camera position, and (b) for a
    // camera outside the atmosphere, marched from the camera *to* the entry
    // point — entirely outside the atmosphere — producing zero scattering.
    let t_exit = ray_atmosphere_exit(ray_origin, ray_dir);
    let t_start = max(t_entry, 0.0);
    let total_distance = max(t_exit - t_start, 0.0);
    let step_size = total_distance / f32(max(sky.ray_samples, 1u));

    let cos_theta = dot(ray_dir, sun_dir);

    // Accumulators
    var rayleigh_optical_depth = 0.0;
    var mie_optical_depth = 0.0;
    var rayleigh_inscatter = vec3<f32>(0.0);
    var mie_inscatter = vec3<f32>(0.0);

    var t = t_start;
    for (var i = 0u; i < sky.ray_samples; i = i + 1u) {
        t = t + step_size * 0.5; // sample at midpoint
        let sample_pos = ray_origin + ray_dir * t;
        let altitude = length(sample_pos) - sky.planet_radius;

        if altitude < 0.0 {
            break; // hit the ground
        }

        let density = atmosphere_density(altitude);
        rayleigh_optical_depth = rayleigh_optical_depth + density.x * step_size;
        mie_optical_depth = mie_optical_depth + density.y * step_size;

        // Sun ray: how much light reaches this sample point from the sun?
        // March from sample_pos along sun_dir until the ray exits the
        // atmosphere shell. The old code reused `sun_alt` (the sample point's
        // altitude) as the march length, which grossly under-estimated the
        // optical depth except when the sun was directly overhead.
        let sun_ray_dir = sun_dir;
        let sun_t_exit = ray_atmosphere_exit(sample_pos, sun_ray_dir);
        let sun_total = max(sun_t_exit, 0.0);
        let sun_step = sun_total / f32(max(sky.mie_samples, 1u));
        var sun_rayleigh_od = 0.0;
        var sun_mie_od = 0.0;
        var sun_t = 0.0;
        for (var j = 0u; j < sky.mie_samples; j = j + 1u) {
            sun_t = sun_t + sun_step * 0.5;
            let sun_pos = sample_pos + sun_ray_dir * sun_t;
            let sun_pos_alt = length(sun_pos) - sky.planet_radius;
            if (sun_pos_alt < 0.0) {
                sun_rayleigh_od = 1e10; // fully occluded
                sun_mie_od = 1e10;
                break;
            }
            let sun_density = atmosphere_density(sun_pos_alt);
            sun_rayleigh_od = sun_rayleigh_od + sun_density.x * sun_step;
            sun_mie_od = sun_mie_od + sun_density.y * sun_step;
        }

        // Transmittance from Beer's law: exp(-scattering_coefficient * optical_depth).
        // Uses the actual scattering coefficients (per km), NOT the raw
        // multiplier values from the uniform. rayleigh_scattering and
        // mie_scattering are vec3 for wavelength-dependent extinction.
        let rayleigh_transmittance = exp(-rayleigh_scattering * (rayleigh_optical_depth + sun_rayleigh_od));
        let mie_transmittance = exp(-mie_scattering.x * (mie_optical_depth + sun_mie_od));

        let density_ray = atmosphere_density(altitude);
        rayleigh_inscatter = rayleigh_inscatter + density_ray.x * rayleigh_transmittance * step_size;
        mie_inscatter = mie_inscatter + density_ray.y * mie_transmittance * step_size;

        t = t + step_size * 0.5;
    }

    // Phase functions
    let r_phase = rayleigh_phase(cos_theta);
    let m_phase = hg_phase(cos_theta, sky.mie_directionality);

    // Final sky radiance
    let sun_intensity = sky.sun_intensity;
    let rayleigh_contribution = rayleigh_inscatter * rayleigh_scattering * r_phase * sun_intensity;
    let mie_contribution = mie_inscatter * mie_scattering * m_phase * sun_intensity;

    let atmosphere_result = rayleigh_contribution + mie_contribution;

    // For below-horizon rays (which hit the ground and break the ray-march
    // with zero inscatter), blend in a horizon-color floor so the editor
    // viewport doesn't go pure black when the camera looks down. The
    // atmosphere result is used at full strength above the horizon; below
    // the horizon, we fade to a dim horizon color. This is purely cosmetic
    // — the ray-march itself produces the correct sky above the horizon.
    let up = vec3<f32>(0.0, 1.0, 0.0);
    let up_dot = clamp(dot(ray_dir, up), 0.0, 1.0);
    let horizon_floor = vec3<f32>(0.15, 0.18, 0.22) * (0.3 + 0.7 * up_dot);

    // Sun disk glow (added on top, visible in all directions near the sun).
    let sun_dot = max(dot(ray_dir, sun_dir), 0.0);
    let sun_glow = vec3<f32>(1.0, 0.85, 0.6) * pow(sun_dot, 200.0) * 5.0;

    // Use the atmosphere result where it's nonzero; fall back to the
    // horizon floor for ground-hit rays. Add the sun glow on top.
    let atmosphere_strength = clamp(length(atmosphere_result) * 50.0, 0.0, 1.0);
    let sky_color = mix(horizon_floor, atmosphere_result, atmosphere_strength);
    return sky_color + sun_glow;
}

// ─── Vertex shader ──────────────────────────────────────────────────────────

@vertex
fn vs_main(@builtin(vertex_index) vertex_index: u32) -> VertexOutput {
    // Fullscreen triangle trick — 3 vertices, no vertex buffer needed
    var positions = array<vec2<f32>, 3>(
        vec2<f32>(-1.0, -1.0),
        vec2<f32>( 3.0, -1.0),
        vec2<f32>(-1.0,  3.0),
    );

    var output: VertexOutput;
    output.clip_position = vec4<f32>(positions[vertex_index], 0.0, 1.0);

    // Reconstruct world-space ray direction from camera basis vectors.
    // NDC xy maps to a point on the near plane in view space:
    //   view_ray = forward + right * ndc.x * aspect * tan_half_fov + up * ndc.y * tan_half_fov
    let ndc = positions[vertex_index];
    let right = camera.world_right.xyz;
    let up = camera.world_up.xyz;
    let fwd = camera.world_forward.xyz;
    let tan_fov = camera.tan_half_fov;
    let asp = camera.aspect;

    let view_ray = normalize(
        fwd
        + right * ndc.x * asp * tan_fov
        + up * ndc.y * tan_fov
    );
    output.world_ray = view_ray;

    return output;
}

// ─── Fragment shader ────────────────────────────────────────────────────────

@fragment
fn fs_main(input: VertexOutput) -> @location(0) vec4<f32> {
    let sky_color = compute_sky_color(normalize(input.world_ray));

    // Output in linear HDR — tone mapping happens in the resolve pass
    return vec4<f32>(sky_color, 1.0);
}
