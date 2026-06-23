// Sky shader for Pie Engine — simple gradient sky dome.
//
// Replaces the previous ray-marched atmospheric scattering model with a
// clean procedural gradient. The gradient depends ONLY on the view ray
// direction (using world-up as the reference axis), so:
//   - The horizon can never tilt (it's always perpendicular to world-up).
//   - The sky color is stable regardless of camera position.
//   - No scattering coefficients to tune, no ray-march artifacts.
//
// The SkyParams uniform struct is kept (unchanged layout) so the Rust
// side doesn't need to change, but only sun_direction and sun_intensity
// are used by the gradient. The scattering / planet fields are ignored.

struct Camera {
    view_proj: mat4x4<f32>,
    position: vec4<f32>,
    // Camera basis vectors in world space for ray reconstruction
    world_right: vec4<f32>,
    world_up: vec4<f32>,
    world_forward: vec4<f32>,
    tan_half_fov: f32,
    aspect: f32,
};

// Sun + sky parameters. Only sun_direction and sun_intensity are used by
// the gradient; the rest are kept for layout compatibility with the Rust
// uniform upload.
struct SkyParams {
    sun_direction: vec4<f32>,     // xyz = direction TO the sun, w = unused
    sun_intensity: f32,           // intensity multiplier for the sun glow
    rayleigh_coefficient: f32,    // unused (gradient)
    mie_coefficient: f32,         // unused (gradient)
    rayleigh_scale_height: f32,   // unused (gradient)
    mie_scale_height: f32,        // unused (gradient)
    mie_directionality: f32,      // unused (gradient)
    planet_radius: f32,           // unused (gradient)
    atmosphere_radius: f32,       // unused (gradient)
    ray_samples: u32,             // unused (gradient)
    mie_samples: u32,             // unused (gradient)
};

@group(0) @binding(0) var<uniform> camera: Camera;
@group(0) @binding(1) var<uniform> sky: SkyParams;

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) world_ray: vec3<f32>,  // world-space ray direction
};

// ─── Sky color computation ──────────────────────────────────────────────────

fn compute_sky_color(ray_dir: vec3<f32>) -> vec3<f32> {
    let sun_dir = normalize(sky.sun_direction.xyz);
    let up = vec3<f32>(0.0, 1.0, 0.0);

    // How much the view ray points upward (1 = straight up, 0 = horizon, -1 = down).
    let up_dot = dot(ray_dir, up);

    // ── Sky gradient ────────────────────────────────────────────────────
    // Three-stop gradient: below horizon → horizon → zenith.
    // Below the horizon fades to a dim ground/haze color so the viewport
    // isn't pure black when looking down.
    let ground_color = vec3<f32>(0.08, 0.08, 0.09);
    let horizon_color = vec3<f32>(0.50, 0.62, 0.78);
    let zenith_color = vec3<f32>(0.15, 0.32, 0.65);

    // Smoothstep transitions for a natural-looking gradient.
    // Horizon band: up_dot in [-0.1, 0.15] blends ground → horizon.
    // Zenith band: up_dot in [0.0, 0.6] blends horizon → zenith.
    let horizon_t = smoothstep(-0.1, 0.15, up_dot);
    let zenith_t = smoothstep(0.0, 0.6, up_dot);

    let sky = mix(ground_color, horizon_color, horizon_t);
    let sky = mix(sky, zenith_color, zenith_t);

    // ── Sun glow ────────────────────────────────────────────────────────
    // Bright sun disk + soft halo around it.
    let sun_dot = max(dot(ray_dir, sun_dir), 0.0);
    let sun_disk = vec3<f32>(1.0, 0.95, 0.85) * pow(sun_dot, 512.0) * 15.0;
    let sun_halo = vec3<f32>(1.0, 0.85, 0.6) * pow(sun_dot, 16.0) * 0.6;

    // Sun intensity from the uniform scales the glow.
    let sun_intensity = max(sky.sun_intensity, 0.0);
    let sun_glow = (sun_disk + sun_halo) * sun_intensity;

    return sky + sun_glow;
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
