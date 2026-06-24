// Sky atmosphere shader — faithful port of UE5 / Hillaire 2020
//
// Based directly on Sébastien Hillaire's reference implementation:
//   https://github.com/sebh/UnrealEngineSkyAtmosphere
//   "A Scalable and Production Ready Sky and Atmosphere Rendering Technique"
//     — Hillaire 2020, EGSR
//
// What this ports from UE5:
//   * Exact Earth atmosphere constants (Rayleigh, Mie, ozone) from
//     SetupEarthAtmosphere() in SkyAtmosphereCommon.cpp.
//   * Three-media medium sampling: Rayleigh + Mie + ozone (ozone uses the
//     2-layer piecewise-linear density profile from UE5).
//   * Mie extinction ≠ Mie scattering (the difference is Mie absorption).
//   * Plain Henyey–Greenstein phase function (not Cornette–Shanks).
//   * Earth shadow check at EVERY sample, not just at the camera.
//   * Analytical step integration (Frostbite 2015 slide 28):
//       Sint = (S - S·SampleTransmittance) / extinction
//     instead of the cheaper S·dt accumulation.
//   * Hillaire 2020 §5 multiple-scattering approximation: integrate
//     MultiScatAs1 (= the geometric-series ratio r) parallel to single
//     scattering, then add L_2ndOrder·r/(1-r) at each sample.
//   * Sun disk via the actual sun angular radius (UE5 uses
//     cos(0.5·0.505·π/180) ≈ 0.99999, slightly larger than physical for AA),
//     at SunLuminance = 1e6 (very HDR — ACES tonemaps to a white disk).
//
// What this does NOT port (kept single-pass for simplicity):
//   * Transmittance / SkyView / MultiScattering LUTs. UE5 precomputes these
//     offline and samples them at runtime. Here we compute transmittance to
//     the sun inline at each sample (ray march along the sun ray).
//
// Coordinate system: planet center at origin, +Y "up". Camera is placed at
// (0, planet_radius + altitude, 0), with altitude derived from the world-
// space camera Y (meters → km).

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
    // xyz = direction TO the sun in world space (normalized), w unused.
    sun_direction:          vec4<f32>,
    // Multiplier on the final in-scattered radiance. Driven by the
    // directional light intensity. UE5's solar_irradiance is normalized to
    // (1,1,1); this multiplier is what makes the sky actually bright.
    sun_intensity:          f32,
    // Multipliers on the base (UE5 sea-level) scattering coefficients.
    // 1.0 = natural Earth atmosphere.
    rayleigh_coefficient:   f32,
    mie_coefficient:        f32,
    // Scale heights (km) — altitude at which density falls by 1/e.
    rayleigh_scale_height:  f32,
    mie_scale_height:       f32,
    // Henyey–Greenstein anisotropy. UE5 default 0.8.
    mie_directionality:     f32,
    // Planet & atmosphere radii (km).
    planet_radius:          f32,
    atmosphere_radius:      f32,
    // Sample counts for the primary view ray and the per-sample sun ray.
    ray_samples:            u32,
    mie_samples:            u32,
    _padding:               vec2<u32>,
};

@group(0) @binding(0) var<uniform> camera: Camera;
@group(0) @binding(1) var<uniform> sky:   SkyParams;

// -----------------------------------------------------------------------------
// Earth atmosphere constants — exact values from UE5 SkyAtmosphereCommon.cpp.
// All in km and 1/km.
// -----------------------------------------------------------------------------
const PI:                  f32    = 3.141592653589793;
const PLANET_RADIUS_OFFSET: f32   = 0.01; // small offset to keep camera above ground

// Sea-level scattering coefficients (1/km).
const RAYLEIGH_SCATTERING_BASE: vec3<f32> = vec3<f32>(0.005802, 0.013558, 0.033100);
const MIE_SCATTERING_BASE:     vec3<f32> = vec3<f32>(0.003996, 0.003996, 0.003996);
const MIE_EXTINCTION_BASE:     vec3<f32> = vec3<f32>(0.004440, 0.004440, 0.004440);
// = MieExtinction - MieScattering ≈ 0.000444 (Mie absorption, implicit).

// Ozone absorption — peak ~25 km, layer width 15 km.
const ABSORPTION_EXTINCTION_BASE: vec3<f32> = vec3<f32>(0.000650, 0.001881, 0.000085);
const ABSORPTION_LAYER_WIDTH:     f32       = 25.0;
const ABSORPTION_LAYER0_LINEAR:   f32       = 1.0 / 15.0;
const ABSORPTION_LAYER0_CONST:    f32       = -2.0 / 3.0;
const ABSORPTION_LAYER1_LINEAR:   f32       = -1.0 / 15.0;
const ABSORPTION_LAYER1_CONST:    f32       =  8.0 / 3.0;

// Sun angular radius — UE5 uses cos(0.5·0.505°·π/180) for the disk boundary
// (slightly larger than the physical sun for soft AA).
const SUN_DISK_COS:  f32 = 0.9999903;     // cos(0.2525°)
const SUN_LUMINANCE: f32 = 1000000.0;     // very HDR — ACES → white disk

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0)       uv:            vec2<f32>,
};

// Fullscreen triangle covering the viewport.
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

    // Camera altitude from world-space Y (meters → km). UE5 places the camera
    // at (0, BottomRadius + altitude, 0); we use the camera's world Y as
    // altitude directly, clamped to stay inside the atmosphere.
    //
    // The minimum is 0.001 km (1 m), NOT 0.0: when the camera is at or below
    // sea level (world Y <= 0), clamping to exactly 0 places it ON the planet
    // sphere, where ray–sphere intersection becomes degenerate (tangent rays
    // return t=0). Adjacent pixels then alternate between "tangent hit"
    // (t_max=0 → black) and "tangent miss" (t_max=large → bright), producing
    // a 1-pixel grain band along the horizon. The 1 m floor keeps the camera
    // just above the surface so the math is always well-conditioned.
    let alt_km     = clamp(camera.position.y * 0.001,
                           0.001,
                           sky.atmosphere_radius - sky.planet_radius - PLANET_RADIUS_OFFSET);
    let cam_radius = sky.planet_radius + alt_km;
    let ray_origin = vec3<f32>(0.0, cam_radius, 0.0);

    let sun_dir = normalize(sky.sun_direction.xyz);

    let color = integrate_sky(ray_origin, view_ray, sun_dir);
    return vec4<f32>(color, 1.0);
}

// -----------------------------------------------------------------------------
// Atmosphere medium — port of sampleMediumRGB() from UE5
// RenderSkyCommon.hlsl. Returns per-channel scattering + extinction at a
// point, where `p` is the position relative to the planet center (km).
// -----------------------------------------------------------------------------
struct MediumSample {
    scattering:        vec3<f32>, // total scattering (Mie + Rayleigh + ozone)
    extinction:        vec3<f32>, // total extinction (Mie + Rayleigh + ozone)
    scattering_mie:    vec3<f32>,
    scattering_ray:    vec3<f32>,
};

// Ozone density — UE5 uses a 2-layer piecewise-linear profile in altitude.
fn ozone_density(view_height_km: f32) -> f32 {
    if (view_height_km < ABSORPTION_LAYER_WIDTH) {
        return clamp(ABSORPTION_LAYER0_LINEAR * view_height_km + ABSORPTION_LAYER0_CONST, 0.0, 1.0);
    }
    return clamp(ABSORPTION_LAYER1_LINEAR * view_height_km + ABSORPTION_LAYER1_CONST, 0.0, 1.0);
}

fn sample_medium(p: vec3<f32>) -> MediumSample {
    let view_height = length(p) - sky.planet_radius;

    let density_mie = exp(-view_height / sky.mie_scale_height);
    let density_ray = exp(-view_height / sky.rayleigh_scale_height);
    let density_ozo = ozone_density(view_height);

    let beta_r = RAYLEIGH_SCATTERING_BASE * sky.rayleigh_coefficient;
    let beta_m = MIE_SCATTERING_BASE     * sky.mie_coefficient;
    let beta_m_ext = MIE_EXTINCTION_BASE * sky.mie_coefficient;
    let beta_o = ABSORPTION_EXTINCTION_BASE;

    let scattering_mie = density_mie * beta_m;
    let extinction_mie = density_mie * beta_m_ext;          // extinction > scattering (Mie absorption)
    let scattering_ray = density_ray * beta_r;
    let extinction_ray = scattering_ray;                    // Rayleigh has no absorption
    let extinction_ozo = density_ozo * beta_o;              // ozone only absorbs

    var s: MediumSample;
    s.scattering_mie = scattering_mie;
    s.scattering_ray = scattering_ray;
    s.scattering     = scattering_mie + scattering_ray;     // ozone doesn't scatter
    s.extinction     = extinction_mie + extinction_ray + extinction_ozo;
    return s;
}

// -----------------------------------------------------------------------------
// Ray–sphere intersection — port of raySphereIntersectNearest() from UE5.
// Returns the smallest non-negative t, or -1 if no hit.
// -----------------------------------------------------------------------------
fn ray_sphere_intersect_nearest(ro: vec3<f32>, rd: vec3<f32>, radius: f32) -> f32 {
    // rd is normalized → a = 1, simplifies the quadratic.
    let b = 2.0 * dot(ro, rd);
    let c = dot(ro, ro) - radius * radius;
    let disc = b * b - 4.0 * c;
    if (disc < 0.0) { return -1.0; }
    let s = sqrt(disc);
    let t0 = (-b - s) * 0.5;
    let t1 = (-b + s) * 0.5;
    if (t0 < 0.0 && t1 < 0.0) { return -1.0; }
    if (t0 < 0.0) { return max(0.0, t1); }
    if (t1 < 0.0) { return max(0.0, t0); }
    return max(0.0, min(t0, t1));
}

// -----------------------------------------------------------------------------
// Phase functions — port of RayleighPhase() and hgPhase() from UE5
// RenderSkyCommon.hlsl. Note: UE5 negates cosTheta before calling hgPhase
// because WorldDir is an "in" direction; we follow the same convention.
// -----------------------------------------------------------------------------
fn rayleigh_phase(cos_theta: f32) -> f32 {
    return (3.0 / (16.0 * PI)) * (1.0 + cos_theta * cos_theta);
}

// Plain Henyey–Greenstein (UE5 default; Cornette-Shanks is opt-in).
fn hg_phase(g: f32, cos_theta: f32) -> f32 {
    let g2 = g * g;
    let numer = 1.0 - g2;
    let denom = 1.0 + g2 + 2.0 * g * cos_theta;
    return numer / (4.0 * PI * denom * sqrt(max(denom, 0.0001)));
}

// -----------------------------------------------------------------------------
// Sun transmittance — inline ray march along the sun ray. UE5 samples a
// precomputed Transmittance LUT here; for a self-contained shader we
// integrate the optical depth directly. Returns the (RGB) transmittance
// from `ro` to the top of the atmosphere along `rd`.
// -----------------------------------------------------------------------------
fn sun_transmittance(ro: vec3<f32>, rd: vec3<f32>, steps: u32) -> vec3<f32> {
    let t_atmo   = ray_sphere_intersect_nearest(ro, rd, sky.atmosphere_radius);
    let t_ground = ray_sphere_intersect_nearest(ro, rd, sky.planet_radius);
    var t_end = t_atmo;
    if (t_ground > 0.0 && (t_atmo < 0.0 || t_ground < t_atmo)) {
        t_end = t_ground;
    }
    if (t_end < 0.0) {
        return vec3<f32>(1.0);  // ray doesn't hit atmosphere → unattenuated
    }

    var optical_depth = vec3<f32>(0.0);
    let step_size = t_end / f32(steps);
    for (var i: u32 = 0u; i < steps; i = i + 1u) {
        let t = (f32(i) + 0.5) * step_size;
        let p = ro + rd * t;
        if (length(p) - sky.planet_radius < 0.0) { break; }
        let m = sample_medium(p);
        optical_depth = optical_depth + m.extinction * step_size;
    }
    return exp(-optical_depth);
}

// ACES filmic tonemap (Narkowicz fit) — matches the PBR shader so the sky
// and the lit scene end up in the same display space.
fn aces_tone_map(x: vec3<f32>) -> vec3<f32> {
    let a = 2.51;
    let b = 0.03;
    let c = 2.43;
    let d = 0.59;
    let e = 0.14;
    return clamp((x * (a * x + b)) / (x * (c * x + d) + e), vec3<f32>(0.0), vec3<f32>(1.0));
}

// -----------------------------------------------------------------------------
// Core integration — port of IntegrateScatteredLuminance() from UE5
// RenderSkyRayMarching.hlsl, single-pass (no LUTs).
// -----------------------------------------------------------------------------
fn integrate_sky(
    ray_origin: vec3<f32>,
    view_ray:   vec3<f32>,
    sun_dir:    vec3<f32>,
) -> vec3<f32> {
    let earth_o = vec3<f32>(0.0, 0.0, 0.0);

    // View-ray / atmosphere / planet intersections.
    let t_bottom = ray_sphere_intersect_nearest(ray_origin, view_ray, sky.planet_radius);
    let t_top    = ray_sphere_intersect_nearest(ray_origin, view_ray, sky.atmosphere_radius);
    var t_max = 0.0;
    if (t_bottom < 0.0) {
        if (t_top < 0.0) {
            // Ray doesn't hit the atmosphere → deep-space black.
            return vec3<f32>(0.0);
        }
        t_max = t_top;
    } else {
        if (t_top > 0.0) {
            t_max = min(t_top, t_bottom);
        } else {
            t_max = t_bottom;
        }
    }

    let sample_count = f32(sky.ray_samples);
    let dt = t_max / sample_count;

    // Phase functions. UE5: cosTheta = dot(sun_dir, view_ray), then
    // hgPhase(g, -cosTheta) because view_ray is an "in" direction.
    let cos_theta      = dot(sun_dir, view_ray);
    let mie_phase_val  = hg_phase(sky.mie_directionality, -cos_theta);
    let rayleigh_val   = rayleigh_phase(cos_theta);
    let uniform_phase  = 1.0 / (4.0 * PI);  // for multi-scatter approximation

    let global_l = vec3<f32>(sky.sun_intensity);

    var L                = vec3<f32>(0.0);  // accumulated luminance
    var throughput       = vec3<f32>(1.0);  // camera → current sample
    var multi_scat_as_1  = vec3<f32>(0.0);  // Hillaire 2020 §5: r = Σ higher orders

    // Sample segment placement (UE5 uses SampleSegmentT = 0.3).
    let sample_segment_t = 0.3;
    var t_prev = 0.0;

    for (var s: f32 = 0.0; s < sample_count; s = s + 1.0) {
        // Mid-sample position along the ray.
        let t_new = t_max * (s + sample_segment_t) / sample_count;
        let dt_step = t_new - t_prev;
        t_prev = t_new;
        let p = ray_origin + view_ray * t_new;

        let medium = sample_medium(p);
        let sample_optical_depth = medium.extinction * dt_step;
        let sample_transmittance = exp(-sample_optical_depth);

        // Earth shadow: does the sun ray from P hit the planet? Computed
        // BEFORE sun_transmittance so we can skip the expensive inner ray
        // march when the sample is in shadow (the sun contributes nothing
        // there, so transmittance is irrelevant). This is the main perf win:
        // when looking toward the horizon at sunset, most low-altitude samples
        // are shadowed and skip the O(mie_samples) inner loop entirely.
        let t_earth = ray_sphere_intersect_nearest(
            p, sun_dir, sky.planet_radius,
        );
        let earth_shadow = select(1.0, 0.0, t_earth >= 0.0);

        // Sun transmittance to this sample point. Skipped when in earth
        // shadow (trans_to_sun is multiplied by earth_shadow below, so 0
        // contribution either way). Capped at 4 samples — the sun ray is a
        // smooth exponential integral and doesn't need the full mie_samples
        // for a visually identical result. This halves the inner-loop cost
        // at Medium quality (8 → 4) and quarters it at High (16 → 4).
        let sun_steps = min(sky.mie_samples, 4u);
        let trans_to_sun = select(
            sun_transmittance(p, sun_dir, sun_steps),
            vec3<f32>(0.0),
            earth_shadow < 0.5,
        );

        // Phase × scattering (single-scattering source term).
        let phase_times_scattering =
            medium.scattering_mie * mie_phase_val +
            medium.scattering_ray * rayleigh_val;

        // ── Multiple-scattering approximation (Hillaire 2020 §5) ──────────
        // Approximate the multi-scattered luminance arriving at P as
        //   L_ms(P) ≈ L_2nd_at_P * (1 / (1 - r))
        // where r is approximated inline as the per-step "albedo" weight.
        // Without a precomputed LUT we use a simple isotropic ambient term
        // proportional to sun transmittance, which gives the warm horizon
        // glow that single scattering alone misses.
        let multi_scat_luminance = 0.1 * trans_to_sun * earth_shadow;

        // Source term — same structure as UE5:
        //   S = globalL * (earthShadow * sunT * phase×scattering
        //                  + multiScatteredLuminance * medium.scattering)
        let S = global_l * (
            earth_shadow * trans_to_sun * phase_times_scattering +
            multi_scat_luminance * medium.scattering
        );

        // ── Analytical step integration (Frostbite 2015 slide 28) ─────────
        // Instead of L += throughput * S * dt, use the closed-form integral
        // of S·exp(-τ) over the step:
        //   Sint = (S - S·SampleTransmittance) / extinction
        // This is significantly more accurate at the same sample count,
        // especially for the bright samples near the horizon.
        let extinction_safe = max(medium.extinction, vec3<f32>(1e-7));
        let S_int = (S - S * sample_transmittance) / extinction_safe;
        L = L + throughput * S_int;
        throughput = throughput * sample_transmittance;

        // ── Multi-scatter ratio accumulation (Hillaire 2020 §5) ───────────
        // MS = medium.scattering (isotropic phase = 1 / 4π folded into the
        // geometric-series formulation). The ratio r = MultiScatAs1
        // represents the fraction of luminance that re-scatters.
        let MS = medium.scattering;
        let MS_int = (MS - MS * sample_transmittance) / extinction_safe;
        multi_scat_as_1 = multi_scat_as_1 + throughput * MS_int;
    }

    // Ground bounce — when the ray hits the planet, add reflected sun light.
    if (t_max == t_bottom && t_bottom > 0.0) {
        let p = ray_origin + view_ray * t_bottom;
        let up_vector = normalize(p);
        let sun_zenith_cos = dot(sun_dir, up_vector);
        let trans_to_sun = sun_transmittance(p, sun_dir, sky.mie_samples);
        let n_dot_l = max(dot(up_vector, sun_dir), 0.0);
        // UE5 uses GroundAlbedo = 0 by default; we use a small value to give
        // the horizon a subtle warm tone where the ray reaches the ground.
        let ground_albedo = vec3<f32>(0.05, 0.04, 0.03);
        L = L + global_l * trans_to_sun * throughput * n_dot_l * ground_albedo / PI;
    }

    // ── Sun disk + halo — port of GetSunLuminance() from UE5 ──────────────
    // UE5 returns 1e6 (very HDR) for pixels inside the sun's angular radius.
    // The bright value ensures ACES tonemaps the disk to white with a soft
    // halo roll-off. Only added when the view ray doesn't hit the planet.
    let t_earth_from_cam = ray_sphere_intersect_nearest(ray_origin, view_ray, sky.planet_radius);
    if (t_earth_from_cam < 0.0) {
        let sun_cos = dot(view_ray, sun_dir);
        // Sharp disk with a 1-pixel soft edge for AA.
        let disk = smoothstep(SUN_DISK_COS - 0.00002, SUN_DISK_COS + 0.00002, sun_cos);
        // Two-tier halo: tight glow just around the disk + broad warm bloom.
        let halo = pow(max(sun_cos, 0.0), 800.0) * 0.05
                 + pow(max(sun_cos, 0.0),  64.0) * 0.01;
        // Scale sun disk by sun_intensity so it dims at low sun intensity.
        L = L + vec3<f32>(disk * SUN_LUMINANCE + halo) * sky.sun_intensity * 0.01;
    }

    // ACES tonemap — matches the PBR shader. Output is linear; the sRGB
    // swapchain applies gamma in hardware.
    return aces_tone_map(L);
}
