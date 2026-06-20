struct Camera {
    view_proj: mat4x4<f32>,
    position: vec4<f32>,
};

struct Model {
    model: mat4x4<f32>,
    normal_matrix: mat4x4<f32>,
};

struct Light {
    direction: vec4<f32>,
    color: vec4<f32>,
};

struct Material {
    base_color_factor: vec4<f32>,
    parameters: vec4<f32>,
};

@group(0) @binding(0)
var<uniform> camera: Camera;

@group(1) @binding(0)
var<uniform> model: Model;

@group(2) @binding(0)
var<uniform> light: Light;

@group(3) @binding(0)
var<uniform> material: Material;

@group(3) @binding(1)
var base_color_texture: texture_2d<f32>;

@group(3) @binding(2)
var normal_texture: texture_2d<f32>;

@group(3) @binding(3)
var base_color_sampler: sampler;

struct VertexInput {
    @location(0) position: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) uv: vec2<f32>,
    @location(3) tangent: vec4<f32>,
};

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) world_position: vec3<f32>,
    @location(1) world_normal: vec3<f32>,
    @location(2) uv: vec2<f32>,
    @location(3) tangent: vec4<f32>,
};

fn tone_map(color: vec3<f32>) -> vec3<f32> {
    let a = 2.51;
    let b = 0.03;
    let c = 2.43;
    let d = 0.59;
    let e = 0.14;
    return clamp((color * (a * color + b)) / (color * (c * color + d) + e), vec3<f32>(0.0), vec3<f32>(1.0));
}

fn fresnel_schlick(cos_theta: f32, f0: vec3<f32>) -> vec3<f32> {
    return f0 + (vec3<f32>(1.0) - f0) * pow(1.0 - cos_theta, 5.0);
}

fn distribution_ggx(n_dot_h: f32, roughness: f32) -> f32 {
    let a = roughness * roughness;
    let a2 = a * a;
    let denom = n_dot_h * n_dot_h * (a2 - 1.0) + 1.0;
    return a2 / max(3.14159265 * denom * denom, 0.0001);
}

fn geometry_schlick_ggx(n_dot_v: f32, roughness: f32) -> f32 {
    let r = roughness + 1.0;
    let k = (r * r) / 8.0;
    return n_dot_v / max(n_dot_v * (1.0 - k) + k, 0.0001);
}

fn geometry_smith(n_dot_v: f32, n_dot_l: f32, roughness: f32) -> f32 {
    return geometry_schlick_ggx(n_dot_v, roughness) * geometry_schlick_ggx(n_dot_l, roughness);
}

/// Decode a normal from the normal map (stored in [0,1]) back to [-1,1] world tangent space,
/// then transform into world space via the TBN matrix.
fn sample_normal(normal_map: texture_2d<f32>, samp: sampler, uv: vec2<f32>, world_normal: vec3<f32>, world_tangent: vec3<f32>, world_bitangent: vec3<f32>) -> vec3<f32> {
    let sampled = textureSample(normal_map, samp, uv).rgb;
    let tangent_normal = sampled * 2.0 - vec3<f32>(1.0);
    return normalize(
        tangent_normal.x * world_tangent +
        tangent_normal.y * world_bitangent +
        tangent_normal.z * world_normal
    );
}

@vertex
fn vs_main(input: VertexInput) -> VertexOutput {
    var output: VertexOutput;
    let world_pos = model.model * vec4<f32>(input.position, 1.0);
    output.clip_position = camera.view_proj * world_pos;
    output.world_position = world_pos.xyz;
    output.world_normal = normalize((model.normal_matrix * vec4<f32>(input.normal, 0.0)).xyz);
    output.uv = input.uv;
    output.tangent = vec4<f32>(normalize((model.normal_matrix * vec4<f32>(input.tangent.xyz, 0.0)).xyz), input.tangent.w);
    return output;
}

@fragment
fn fs_main(input: VertexOutput) -> @location(0) vec4<f32> {
    let texture_color = textureSample(base_color_texture, base_color_sampler, input.uv);
    let base_color = texture_color * material.base_color_factor;

    let metallic = clamp(material.parameters.x, 0.0, 1.0);
    let roughness = clamp(material.parameters.y, 0.04, 1.0);

    // Build TBN matrix from interpolated vertex tangent for normal mapping.
    let world_normal = normalize(input.world_normal);
    let world_tangent = normalize(input.tangent.xyz);
    let world_bitangent = cross(world_normal, world_tangent) * input.tangent.w;

    // Sample normal map and transform from tangent to world space.
    let normal = sample_normal(normal_texture, base_color_sampler, input.uv, world_normal, world_tangent, world_bitangent);

    let view_direction = normalize(camera.position.xyz - input.world_position);
    let light_direction = normalize(-light.direction.xyz);
    let half_vector = normalize(view_direction + light_direction);

    let n_dot_l = max(dot(normal, light_direction), 0.0);
    let n_dot_v = max(dot(normal, view_direction), 0.0);
    let n_dot_h = max(dot(normal, half_vector), 0.0);
    let v_dot_h = max(dot(view_direction, half_vector), 0.0);

    let dielectric_f0 = vec3<f32>(0.04, 0.04, 0.04);
    let f0 = mix(dielectric_f0, base_color.rgb, metallic);
    let f = fresnel_schlick(v_dot_h, f0);
    let d = distribution_ggx(n_dot_h, roughness);
    let g = geometry_smith(n_dot_v, n_dot_l, roughness);

    let numerator = d * g * f;
    let denominator = max(4.0 * n_dot_v * n_dot_l, 0.001);
    let specular = numerator / denominator;

    let k_d = (vec3<f32>(1.0) - f) * (1.0 - metallic);
    let diffuse = k_d * base_color.rgb / 3.14159265;

    let light_radiance = light.color.rgb * light.color.a;
    let ambient = base_color.rgb * 0.03;
    let lighting = ambient + (diffuse + specular) * light_radiance * n_dot_l;

    let mapped = tone_map(lighting);
    let gamma_corrected = pow(mapped, vec3<f32>(1.0 / 2.2));

    return vec4<f32>(gamma_corrected, base_color.a);
}