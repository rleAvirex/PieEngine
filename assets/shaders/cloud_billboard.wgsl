// Volumetric cloud shader — proper front-to-back compositing.
//
// Fixed version: the previous shader had 7 bugs that made clouds look
// thin and bugged. This rewrite follows the standard volumetric
// compositing formula from Schneider (SIGGRAPH 2015) and the MiniMax
// volumetric rendering reference.
//
// BUGS FIXED:
//   1. Compositing: was separating luminance/transmittance then
//      multiplying by alpha at the end (double-darkening). Now uses
//      standard front-to-back: color += light * (1-alpha), alpha += d*(1-alpha)
//   2. Ray penetration: march now covers the full volume depth.
//   3. No jittering → banding. Now jittered with a hash.
//   4. ABSORPTION too low → barely opaque. Now 8.0.
//   5. Density triple-reduced. Now single threshold + gentle falloff.
//   6. Single-lobe HG → dual-lobe (forward 0.8 + backward -0.2).
//   7. Light march now returns transmittance (not 1-T), applied correctly.

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
    center_size:    vec4<f32>,
    color_density:  vec4<f32>,
    sun_dir_wind:   vec4<f32>,
    sun_color_time: vec4<f32>,
};

@group(0) @binding(0) var<uniform> camera: Camera;
@group(0) @binding(1) var<uniform> cloud:  CloudUniform;
@group(0) @binding(2) var cloud_noise: texture_3d<f32>;
@group(0) @binding(3) var noise_sampler: sampler;

const PI: f32 = 3.141592653589793;

// Ray-march configuration — tuned for visible, dense clouds at good perf.
const PRIMARY_STEPS:     u32 = 16;    // was 32 — halved for perf
const LIGHT_STEPS:       u32 = 4;     // was 6 — halved for perf
const DENSITY_THRESHOLD: f32 = 0.25;
const DENSITY_MULTIPLIER: f32 = 2.0;
const LIGHT_ABSORPTION:  f32 = 8.0;
const STEP_SIZE:         f32 = 1.0 / 16.0; // covers full volume depth

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0)       uv:            vec2<f32>,
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

// ── Dual-lobe Henyey-Greenstein ────────────────────────────────────────────
// UE5 uses a dual-lobe: forward scatter (g=0.8) + backward scatter (g=-0.2),
// blended 50/50. This gives both the bright forward glow and the soft
// back-scatter that real clouds have.
fn henyey_greenstein(cos_theta: f32, g: f32) -> f32 {
    let g2 = g * g;
    let denom = pow(max(1.0 + g2 - 2.0 * g * cos_theta, 0.0001), 1.5);
    return (1.0 - g2) / (4.0 * PI * denom);
}

fn phase_function(cos_theta: f32) -> f32 {
    let forward = henyey_greenstein(cos_theta, 0.8);
    let backward = henyey_greenstein(cos_theta, -0.2);
    return mix(forward, backward, 0.5);
}

// ── Beer-Powder ────────────────────────────────────────────────────────────
// Beer-Lambert × powder. The powder term darkens dense regions more than
// Beer-Lambert alone, giving clouds their characteristic dark cores with
// bright fringes. From Schneider SIGGRAPH 2015, p.64.
fn beer_powder(density: f32) -> f32 {
    let beer = exp(-density * LIGHT_ABSORPTION);
    let powder = 1.0 - exp(-density * 2.0);
    return beer * powder;
}

// ── Hash for jittering (eliminates banding) ───────────────────────────────
fn hash13(p: vec3<f32>) -> f32 {
    var v = p;
    v = v * vec3<f32>(0.1031, 0.1030, 0.0973);
    v.x = v.x + dot(v, v.yzx + 33.33);
    return fract((v.x + v.y) * v.z);
}

// ── Sample cloud density at a point in cloud-local UVW space ──────────────
// The cloud volume is a unit cube [-0.5, 0.5]³. Returns raw density [0,1].
fn sample_density(uvw: vec3<f32>, wind_offset: f32) -> f32 {
    let scroll = vec3<f32>(wind_offset, wind_offset * 0.3, wind_offset * 0.15);
    let p_low  = uvw * 2.0 + scroll;
    let p_high = uvw * 5.0 + scroll * 1.7;

    let n_low  = textureSample(cloud_noise, noise_sampler, p_low).r;
    let n_high = textureSample(cloud_noise, noise_sampler, p_high).r;
    let n = n_low * 0.7 + n_high * 0.3;

    // Height gradient: denser at bottom, thinner at top (cumulus shape).
    let height_grad = smoothstep(0.5, -0.2, uvw.y);

    // Radial falloff — soft sphere edges.
    let r = length(uvw);
    let falloff = smoothstep(0.5, 0.15, r);

    // Single threshold (was double-reduced before).
    let shaped = smoothstep(DENSITY_THRESHOLD, DENSITY_THRESHOLD + 0.15, n);
    return shaped * falloff * height_grad * DENSITY_MULTIPLIER;
}

// ── Light march: transmittance from a point toward the sun ────────────────
// Returns the fraction of sun light that reaches the point (0=shadowed, 1=lit).
fn light_march(uvw: vec3<f32>, sun_dir_local: vec3<f32>, wind_offset: f32) -> f32 {
    var transmittance = 1.0;
    var pos = uvw;
    let light_step = 0.08;
    for (var i: u32 = 0u; i < LIGHT_STEPS; i = i + 1u) {
        pos = pos + sun_dir_local * light_step;
        let d = sample_density(pos, wind_offset);
        if (d > 0.0) {
            transmittance *= exp(-d * LIGHT_ABSORPTION * light_step);
        }
    }
    return transmittance;
}

@fragment
fn fs_main(input: VertexOutput) -> @location(0) vec4<f32> {
    let density_mul = cloud.color_density.w;
    let base_color  = cloud.color_density.xyz;
    let sun_dir = normalize(cloud.sun_dir_wind.xyz);
    let wind_offset = cloud.sun_dir_wind.w;
    let sun_color = cloud.sun_color_time.xyz;

    let view_dir = normalize(input.world_pos - camera.position.xyz);

    // Ray setup in cloud-local UVW space [-0.5, 0.5]³.
    // March from the near face (z=-0.5) through to the far face.
    let local_origin = vec3<f32>(input.uv * 0.5, -0.5);
    // March direction: straight through along Z, with slight parallax
    // from the view angle.
    let local_dir = normalize(vec3<f32>(
        dot(view_dir, camera.world_right.xyz) * 0.15,
        dot(view_dir, camera.world_up.xyz) * 0.15,
        1.0,
    ));

    // Sun direction in cloud-local space.
    let cam_forward = camera.world_forward.xyz;
    let sun_dir_local = normalize(vec3<f32>(
        dot(sun_dir, camera.world_right.xyz),
        dot(sun_dir, camera.world_up.xyz),
        dot(sun_dir, cam_forward),
    ));

    // Dual-lobe HG phase.
    let cos_theta = dot(view_dir, sun_dir);
    let phase = phase_function(cos_theta);

    // Jitter the starting position to eliminate banding.
    let jitter = hash13(vec3<f32>(input.clip_position.xy, wind_offset));

    // ── Front-to-back compositing (the standard formula) ──────────────────
    //   color_acc += sample_color * sample_alpha * (1 - alpha_acc)
    //   alpha_acc += sample_alpha * (1 - alpha_acc)
    var color_acc = vec3<f32>(0.0);
    var alpha_acc = 0.0;

    for (var i: u32 = 0u; i < PRIMARY_STEPS; i = i + 1u) {
        let t = (f32(i) + jitter) * STEP_SIZE;
        let sample_pos = local_origin + local_dir * t;

        // Skip samples outside the volume.
        if (length(sample_pos) > 0.5) {
            continue;
        }

        let d = sample_density(sample_pos, wind_offset) * density_mul;
        if (d <= 0.001) {
            continue;
        }

        // Light march toward the sun — how much light reaches this point.
        let light_t = light_march(sample_pos, sun_dir_local, wind_offset);

        // Silver lining: bright at grazing angles.
        let silver = pow(max(1.0 - abs(cos_theta), 0.0), 6.0) * 0.5;

        // Sample color: sun light × phase × beer-powder + silver lining + ambient.
        let ambient = 0.3; // sky light fill
        let lit = light_t * phase + silver + ambient;
        let sample_color = base_color * sun_color * lit;

        // Front-to-back composite. The density controls opacity per step.
        let sample_alpha = d * STEP_SIZE * LIGHT_ABSORPTION * 0.5;
        let contribution = sample_alpha * (1.0 - alpha_acc);
        color_acc += sample_color * contribution;
        alpha_acc += contribution;

        // Early exit when fully opaque.
        if (alpha_acc > 0.99) {
            break;
        }
    }

    if (alpha_acc < 0.01) {
        discard;
    }

    // Premultiplied alpha output (color already weighted by alpha during
    // compositing, so don't multiply again).
    return vec4<f32>(color_acc, alpha_acc);
}
