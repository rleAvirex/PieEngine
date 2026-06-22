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

// ─── Sky color computation ──────────────────────────────────────────────────

fn compute_sky_color(ray_dir: vec3<f32>) -> vec3<f32> {
    let sun_dir = normalize(sky.sun_direction.xyz);

    // Camera position — the engine camera may be anywhere (even at the origin),
    // but the sky atmosphere assumes the viewer is on the planet surface.
    // If the camera is inside the planet, project it to the surface.
    let cam_pos = camera.position.xyz;
    let cam_alt = length(cam_pos);
    let surface_alt = sky.planet_radius + 0.001;
    // If camera is at/near the origin, place it at +Y on the surface
    let ray_origin = select(
        select(cam_pos, normalize(cam_pos) * surface_alt, cam_alt >= sky.planet_radius),
        vec3<f32>(0.0, surface_alt, 0.0),
        cam_alt < 0.001
    );

    // Find where the ray enters the atmosphere
    let t_entry = ray_atmosphere_entry(ray_origin, ray_dir);
    if t_entry < 0.0 {
        // Ray doesn't hit atmosphere — return black (space)
        return vec3<f32>(0.0, 0.0, 0.0);
    }

    // Ray march through atmosphere
    let total_distance = t_entry; // simplified: just use entry distance
    let step_size = total_distance / f32(sky.ray_samples);

    let cos_theta = dot(ray_dir, sun_dir);

    // Accumulators
    var rayleigh_optical_depth = 0.0;
    var mie_optical_depth = 0.0;
    var rayleigh_inscatter = vec3<f32>(0.0);
    var mie_inscatter = vec3<f32>(0.0);

    var t = 0.0;
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
        let sun_ray_dir = sun_dir;
        let sun_cos = dot(normalize(sample_pos), sun_ray_dir);
        let sun_alt = length(sample_pos) - sky.planet_radius;
        let sun_step = sun_alt / f32(max(sky.mie_samples, 1u));
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

        // Transmittance from Beer's law
        let rayleigh_transmittance = exp(-sky.rayleigh_coefficient * (rayleigh_optical_depth + sun_rayleigh_od));
        let mie_transmittance = exp(-sky.mie_coefficient * (mie_optical_depth + sun_mie_od));

        let density_ray = atmosphere_density(altitude);
        rayleigh_inscatter = rayleigh_inscatter + density_ray.x * rayleigh_transmittance * step_size;
        mie_inscatter = mie_inscatter + density_ray.y * mie_transmittance * step_size;

        t = t + step_size * 0.5;
    }

    // Scattering coefficients
    let rayleigh_scattering = vec3<f32>(5.5e-6, 13.0e-6, 22.4e-6) * sky.rayleigh_coefficient;
    let mie_scattering = vec3<f32>(sky.mie_coefficient);

    // Phase functions
    let r_phase = rayleigh_phase(cos_theta);
    let m_phase = hg_phase(cos_theta, sky.mie_directionality);

    // Final sky radiance
    let sun_intensity = sky.sun_intensity;
    let rayleigh_contribution = rayleigh_inscatter * rayleigh_scattering * r_phase * sun_intensity;
    let mie_contribution = mie_inscatter * mie_scattering * m_phase * sun_intensity;

    return rayleigh_contribution + mie_contribution;
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
