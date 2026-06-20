struct LineCamera {
    view_proj: mat4x4<f32>,
};

struct LineColor {
    color: vec4<f32>,
};

@group(0) @binding(0)
var<uniform> camera: LineCamera;

@group(1) @binding(0)
var<uniform> line_color: LineColor;

struct VertexInput {
    @location(0) position: vec3<f32>,
};

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) color: vec4<f32>,
};

@vertex
fn vs_main(input: VertexInput) -> VertexOutput {
    var output: VertexOutput;
    output.clip_position = camera.view_proj * vec4<f32>(input.position, 1.0);
    output.color = line_color.color;
    return output;
}

@fragment
fn fs_main(input: VertexOutput) -> @location(0) vec4<f32> {
    return input.color;
}
