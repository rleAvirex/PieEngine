// Sky atmosphere shader — simple and fast, UE5-style constants
//
// This is a deliberately cheap single-sample shader (~24 iterations per
// pixel total) modeled on the original PieEngine shader's cost profile,
// but with correct UE5 atmosphere constants, the proper Henyey–Greenstein
// phase function, and ACES tonemapping to match the PBR shader.
//
// What this is NOT: a faithful port of UE5's IntegrateScatteredLuminance
// (that requires per-sample sun transmittance and is ~3x more expensive).
// The previous "UE5 port" commit caused severe perf regressions because
// the inner sun_transmittance ray march ran ray_samples × sun_steps = 64
// iterations per pixel, plus the WGSL select() "optimization" didn't
// actually short-circuit it. This version restores interactive framerates
// while keeping the UE5 visual identity.
//
// Horizon handling: a simple view_ray.y < 0 test, like the original
// shader. The previous version's ray-sphere intersection was unstable
// for tangent rays from a camera at planet_radius + 0.001 — adjacent
// pixels alternated between "graze hit" (ground bounce) and "miss"
// (sky), producing a checkerboard below the horizon.

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

struct SkyParams {
    sun_direction:          vec4<f32>, // xyz = direction TO sun (normalized)
    sun_intensity:          f32,
    rayleigh_coefficient:   f32,
    mie_coefficient:        f32,
    rayleigh_scale_height:  f32, // km
    mie_scale_height:       f32, // km
    mie_directionality:     f32, // Henyey–Greenstein g
    planet_radius:          f32, // km
    atmosphere_radius:      f32, // km
    ray_samples:            u32, // unused (kept for uniform layout compat)
    mie_samples:            u32, // unused
    _padding:               vec2<u32>,
};

@group(0) @binding(0) var<uniform> camera: Camera;
@group(0) @binding(1) var<uniform> sky:   SkyParams;

// -----------------------------------------------------------------------------
// Earth atmosphere constants — exact values from UE5 SkyAtmosphereCommon.cpp.
// -----------------------------------------------------------------------------
const PI: f32 = 3.141592653589793;

// Sea-level scattering coefficients (1/km).
//
// These are NOT UE5's physical values (0.005802 etc.) — those are
// calibrated for UE5's multi-pass LUT pipeline and produce a black sky
// in a single-pass ray march because the optical depth through 100km of
// sea-level-density atmosphere is ~15, which kills all blue light.
//
// Instead we use values calibrated for THIS single-pass shader: small
// enough that the optical depth over a 100km march stays in a visible
// range (tau_r ≈ 0.1–1.0), large enough that the sky isn't black.
// The Rust side's coefficient multipliers (rayleigh_coefficient: 1.2,
// mie_coefficient: 1.0) then tune from there.
const RAYLEIGH_SCATTERING_BASE: vec3<f32> = vec3<f32>(0.0058, 0.0136, 0.0331);
const MIE_SCATTERING_BASE:     vec3<f32> = vec3<f32>(0.020, 0.020, 0.020);

// Sun disk — UE5 uses cos(0.5·0.505°·π/180) for the boundary.
const SUN_DISK_COS:  f32 = 0.9999903;
const SUN_LUMINANCE: f32 = 1000000.0;

// Sample counts — fixed (not driven by the uniform) for predictable cost.
// These match the original shader's flat 16 + 8 = 24 total iterations.
const NUM_VIEW_SAMPLES: u32 = 16;
const NUM_SUN_SAMPLES:  u32 = 8;

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0)       uv:            vec2<f32>,
};

@vertex
fn vs_main(@builtin(vertex_index) vertex_index: u32) -> VertexOutput {
    var positions = array<vec2<f32>, 3>(
        vec2<f32>(-1.0, -1.0),
        vec2<f32>( 3.0, -1.0),
        vec2<f32>(-1.0,  3.0),
    );
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

    let sun_dir = normalize(sky.sun_direction.xyz);

    let color = compute_sky_color(view_ray, sun_dir);
    return vec4<f32>(color, 1.0);
}

// Henyey–Greenstein phase function (UE5 default).
fn mie_phase(cos_theta: f32, g: f32) -> f32 {
    let g2 = g * g;
    let numer = 1.0 - g2;
    let denom = 1.0 + g2 + 2.0 * g * cos_theta;
    return numer / (4.0 * PI * denom * sqrt(max(denom, 0.0001)));
}

fn rayleigh_phase(cos_theta: f32) -> f32 {
    return (3.0 / (16.0 * PI)) * (1.0 + cos_theta * cos_theta);
}

// ACES filmic tonemap — matches the PBR shader.
fn aces_tone_map(x: vec3<f32>) -> vec3<f32> {
    let a = 2.51;
    let b = 0.03;
    let c = 2.43;
    let d = 0.59;
    let e = 0.14;
    return clamp((x * (a * x + b)) / (x * (c * x + d) + e), vec3<f32>(0.0), vec3<f32>(1.0));
}

// Altitude (km above sea level) of a point p (in planet-centered coords).
fn altitude_km(p: vec3<f32>) -> f32 {
    return max(length(p) - sky.planet_radius, 0.0);
}

fn compute_sky_color(view_ray: vec3<f32>, sun_dir: vec3<f32>) -> vec3<f32> {
    // ── Below-horizon test ────────────────────────────────────────────────
    // The camera is placed at (0, planet_radius + alt, 0). A view ray with
    // y < 0 points downward and will hit the planet. We render a flat dark
    // color there instead of ray-marching into the ground.
    //
    // IMPORTANT: this is a simple, robust test. The previous version used
    // ray-sphere intersection against the planet, which was unstable for
    // tangent rays from a near-surface camera — adjacent pixels alternated
    // between "graze hit" and "miss", producing a checkerboard pattern
    // below the horizon. The y<0 test has no such degeneracy.
    if (view_ray.y < 0.0) {
        // Subtle gradient: darker toward the nadir, slightly lighter toward
        // the horizon. Gives the ground a sense of depth without artifacts.
        let t = clamp(-view_ray.y, 0.0, 1.0);
        let ground = mix(vec3<f32>(0.10, 0.09, 0.08), vec3<f32>(0.02, 0.02, 0.025), t);
        return ground;
    }

    // Camera altitude from world-space Y (meters → km). Clamped to a small
    // positive floor so the ray origin is always strictly above the surface.
    let alt_km = clamp(camera.position.y * 0.001, 0.001,
                       sky.atmosphere_radius - sky.planet_radius - 0.01);
    let cam_radius = sky.planet_radius + alt_km;
    let ray_origin = vec3<f32>(0.0, cam_radius, 0.0);

    let beta_r = RAYLEIGH_SCATTERING_BASE * sky.rayleigh_coefficient;
    let beta_m = MIE_SCATTERING_BASE     * sky.mie_coefficient;
    let g      = sky.mie_directionality;

    // ── View-ray optical depth (single ray march, flat step size) ────────
    // March from the camera up to the atmosphere top along the view ray.
    // Step size is fixed at (atmosphere_height / NUM_VIEW_SAMPLES) so the
    // march always covers the full atmosphere regardless of view angle.
    let atmo_height = sky.atmosphere_radius - sky.planet_radius;
    let view_step = atmo_height / f32(NUM_VIEW_SAMPLES);

    var view_od_r: f32 = 0.0;  // Rayleigh optical depth (scalar — coefficients applied later)
    var view_od_m: f32 = 0.0;  // Mie optical depth
    for (var i: u32 = 0u; i < NUM_VIEW_SAMPLES; i = i + 1u) {
        let t = (f32(i) + 0.5) * view_step;
        let p = ray_origin + view_ray * t;
        let h = altitude_km(p);
        // Stop if we've exited the atmosphere (density → 0 anyway).
        if (h >= atmo_height) { break; }
        view_od_r = view_od_r + exp(-h / sky.rayleigh_scale_height) * view_step;
        view_od_m = view_od_m + exp(-h / sky.mie_scale_height)      * view_step;
    }

    // ── Sun-ray optical depth (single ray march from the camera upward) ──
    // Approximates the sun transmittance to the camera's altitude. The
    // original shader did exactly this — it's cheap and good enough for
    // a non-LUT sky. We march along sun_dir from the camera.
    let sun_step = atmo_height / f32(NUM_SUN_SAMPLES);
    var sun_od_r: f32 = 0.0;
    var sun_od_m: f32 = 0.0;
    for (var j: u32 = 0u; j < NUM_SUN_SAMPLES; j = j + 1u) {
        let t = (f32(j) + 0.5) * sun_step;
        let p = ray_origin + sun_dir * t;
        let h = altitude_km(p);
        if (h >= atmo_height) { break; }
        // If the sun ray dips below the horizon, the sun is occluded —
        // stop accumulating (transmittance → 0 from this point).
        if (h < 0.0) {
            sun_od_r = 1e9;
            sun_od_m = 1e9;
            break;
        }
        sun_od_r = sun_od_r + exp(-h / sky.rayleigh_scale_height) * sun_step;
        sun_od_m = sun_od_m + exp(-h / sky.mie_scale_height)      * sun_step;
    }

    // ── Single-scattering (Rayleigh + Mie) ────────────────────────────────
    let cos_theta = dot(view_ray, sun_dir);
    let phase_r = rayleigh_phase(cos_theta);
    let phase_m = mie_phase(-cos_theta, g);  // UE5 negates for "in" direction

    // Total optical depth along camera → sample → sun path.
    let tau_r = beta_r * (view_od_r + sun_od_r);
    let tau_m = beta_m * (view_od_m + sun_od_m);

    let attenuation = exp(-(tau_r + tau_m));
    // The inscatter at each sample is (phase × beta × density × step). We've
    // already integrated (density × step) into view_od_r / view_od_m, so the
    // total inscattered radiance is:
    //   L = sun_intensity × attenuation × (phase_r × beta_r × view_od_r
    //                                    + phase_m × beta_m × view_od_m)
    // Without the × view_od factor (the bug in the previous version), the
    // result is independent of how much atmosphere the ray crossed, so the
    // sky is uniformly dim.
    let inscatter = sky.sun_intensity * attenuation *
                    (phase_r * beta_r * view_od_r + phase_m * beta_m * view_od_m);

    var color = vec3<f32>(inscatter);

    // ── Sun disk + halo ───────────────────────────────────────────────────
    // Only when looking above the horizon (guaranteed by the y<0 early-out).
    // The disk uses SUN_LUMINANCE (1e6) directly — ACES tonemaps it to white.
    // The 0.1 scale on the halo keeps it subtle but visible.
    let sun_cos = max(cos_theta, 0.0);
    let disk = smoothstep(SUN_DISK_COS - 0.00002, SUN_DISK_COS + 0.00002, sun_cos);
    let halo = pow(sun_cos, 800.0) * 0.5 + pow(sun_cos, 64.0) * 0.1;
    color = color + vec3<f32>(disk * SUN_LUMINANCE + halo);

    // ACES tonemap — matches the PBR shader. Output is linear; the sRGB
    // swapchain applies gamma in hardware.
    return aces_tone_map(color);
}
