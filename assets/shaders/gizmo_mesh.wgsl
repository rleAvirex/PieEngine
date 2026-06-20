//! Gizmo mesh shader — unlit, per-vertex color, triangle topology.
//!
//! Used for rendering solid gizmo geometry (axis shafts as camera-facing
//! quads, cone arrowheads, origin sphere) with flat, vibrant colors
//! matching UE5/Unity editor gizmo style.

struct GizmoCamera {
    view_proj: mat4x4<f32>,
};

@group(0) @binding(0)
var<uniform> camera: GizmoCamera;

struct VertexInput {
    @location(0) position: vec3<f32>,
    @location(1) color: vec4<f32>,
};

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) color: vec4<f32>,
};

@vertex
fn vs_main(input: VertexInput) -> VertexOutput {
    var output: VertexOutput;
    output.clip_position = camera.view_proj * vec4<f32>(input.position, 1.0);
    output.color = input.color;
    return output;
}

@fragment
fn fs_main(input: VertexOutput) -> @location(0) vec4<f32> {
    return input.color;
}
