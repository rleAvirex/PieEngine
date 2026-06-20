use glam::Vec3;

use crate::assets::{AssetRegistry, MaterialAsset, MeshAsset, MeshVertex};
use crate::components::{ActiveCamera, Camera, MeshRenderer, Name, Transform, Velocity};
use crate::core::{BootstrapSceneResult, SimulationCore};
use crate::rendering::camera::look_at_camera_transform;

/// Visual bring-up target when no glTF scene is available.
pub fn bootstrap_fallback_render_scene(
    core: &mut SimulationCore,
    registry: &mut AssetRegistry,
) -> BootstrapSceneResult {
    let material = registry.insert_material(MaterialAsset::pbr(
        "fallback_material",
        [1.0, 1.0, 1.0, 1.0],
        0.0,
        1.0,
    ));
    let (vertices, indices) = fallback_cube_geometry();
    let mesh = registry
        .insert_mesh(MeshAsset {
            name: "FallbackCube".to_string(),
            vertices,
            indices,
            material,
        })
        .expect("fallback cube geometry should be valid");

    // Position the camera so the default view shows the cube from a pleasant
    // three-quarter angle (two faces visible) rather than straight-on to a
    // single side.
    let eye = Vec3::new(3.0, 2.5, 3.0);
    let active_camera = core.world_mut().spawn((
        Name::new("FallbackCamera"),
        ActiveCamera,
        Camera::default(),
        look_at_camera_transform(eye, Vec3::ZERO),
    ));

    core.world_mut().spawn((
        Name::new("FallbackCube"),
        Transform::default(),
        Velocity(Vec3::ZERO),
        MeshRenderer { mesh },
    ));

    BootstrapSceneResult { active_camera }
}

/// Build a unit cube centred at the origin.
///
/// Each face has its own 4 vertices (no vertex sharing) so that normals,
/// UVs and tangents are per-face and unambiguous.  Winding is CCW when
/// viewed from outside the cube, matching the renderer's `FrontFace::Ccw`
/// + back-face culling pipeline.
///
/// Vertex layout per your reference:
///   position(3) + uv(2) + normal(3) + tangent(4) = 12 floats
pub fn fallback_cube_geometry() -> (Vec<MeshVertex>, Vec<u32>) {
    let mut vertices: Vec<MeshVertex> = Vec::with_capacity(24);
    let mut indices: Vec<u32> = Vec::with_capacity(36);

    // Helper: push a quad (4 verts, 6 indices) with CCW winding.
    // Vertices are ordered: bottom-left, bottom-right, top-right, top-left
    // as seen from outside the face.
    let mut push_quad = |bl: [f32; 3], br: [f32; 3], tr: [f32; 3], tl: [f32; 3],
                         normal: [f32; 3], tangent: [f32; 4]| {
        let base = vertices.len() as u32;
        vertices.extend_from_slice(&[
            MeshVertex { position: bl, uv: [0.0, 0.0], normal, tangent },
            MeshVertex { position: br, uv: [1.0, 0.0], normal, tangent },
            MeshVertex { position: tr, uv: [1.0, 1.0], normal, tangent },
            MeshVertex { position: tl, uv: [0.0, 1.0], normal, tangent },
        ]);
        indices.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
    };

    // --- Front  (normal:  0,  0,  1, tangent: 1, 0, 0, -1) ---
    push_quad(
        [-0.5, -0.5,  0.5], // bottom-left
        [ 0.5, -0.5,  0.5], // bottom-right
        [ 0.5,  0.5,  0.5], // top-right
        [-0.5,  0.5,  0.5], // top-left
        [0.0, 0.0, 1.0],
        [1.0, 0.0, 0.0, -1.0],
    );

    // --- Back  (normal:  0,  0, -1, tangent: -1, 0, 0, -1) ---
    push_quad(
        [ 0.5, -0.5, -0.5], // bottom-left  (viewed from -Z)
        [-0.5, -0.5, -0.5], // bottom-right
        [-0.5,  0.5, -0.5], // top-right
        [ 0.5,  0.5, -0.5], // top-left
        [0.0, 0.0, -1.0],
        [-1.0, 0.0, 0.0, -1.0],
    );

    // --- Left  (normal: -1,  0,  0, tangent: 0, 0, 1, -1) ---
    // Fixed: was wound with normal pointing +X instead of -X, causing
    // back-face culling to hide this face from outside the cube.
    push_quad(
        [-0.5, -0.5, -0.5], // bottom-left  (viewed from -X)
        [-0.5, -0.5,  0.5], // bottom-right
        [-0.5,  0.5,  0.5], // top-right
        [-0.5,  0.5, -0.5], // top-left
        [-1.0, 0.0, 0.0],
        [0.0, 0.0, 1.0, -1.0],
    );

    // --- Right  (normal:  1,  0,  0, tangent: 0, 0, 1, -1) ---
    // Fixed: was wound with normal pointing -X instead of +X, causing
    // back-face culling to hide this face from outside the cube.
    push_quad(
        [ 0.5, -0.5,  0.5], // bottom-left  (viewed from +X)
        [ 0.5, -0.5, -0.5], // bottom-right
        [ 0.5,  0.5, -0.5], // top-right
        [ 0.5,  0.5,  0.5], // top-left
        [1.0, 0.0, 0.0],
        [0.0, 0.0, 1.0, -1.0],
    );

    // --- Top  (normal:  0,  1,  0, tangent: 1, 0, 0, -1) ---
    push_quad(
        [-0.5,  0.5,  0.5], // bottom-left  (viewed from +Y)
        [ 0.5,  0.5,  0.5], // bottom-right
        [ 0.5,  0.5, -0.5], // top-right
        [-0.5,  0.5, -0.5], // top-left
        [0.0, 1.0, 0.0],
        [1.0, 0.0, 0.0, -1.0],
    );

    // --- Bottom  (normal:  0, -1,  0, tangent: 1, 0, 0, 1) ---
    push_quad(
        [-0.5, -0.5, -0.5], // bottom-left  (viewed from -Y)
        [ 0.5, -0.5, -0.5], // bottom-right
        [ 0.5, -0.5,  0.5], // top-right
        [-0.5, -0.5,  0.5], // top-left
        [0.0, -1.0, 0.0],
        [1.0, 0.0, 0.0, 1.0],
    );

    // Validate: vertex winding must match stored normals (CCW from outside).
    // This catches the exact class of bug where cube.bin was generated with
    // flipped winding on some faces, causing back-face culling to hide them.
    for fi in 0..6 {
        let v = &vertices[fi * 4];
        let e1 = glam::Vec3::from(vertices[fi * 4 + 1].position) - glam::Vec3::from(v.position);
        let e2 = glam::Vec3::from(vertices[fi * 4 + 2].position) - glam::Vec3::from(v.position);
        let computed = e1.cross(e2).normalize();
        let stored = glam::Vec3::from(v.normal);
        assert!(
            computed.dot(stored) > 0.99,
            "face {} winding is CW from outside (computed={:?}, stored={:?})",
            fi, computed, stored,
        );
    }

    (vertices, indices)
}

#[cfg(test)]
mod tests {
    use super::{bootstrap_fallback_render_scene, fallback_cube_geometry};
    use crate::assets::AssetRegistry;
    use crate::components::{ActiveCamera, MeshRenderer, Name};
    use crate::core::SimulationCore;

    #[test]
    fn fallback_cube_geometry_has_expected_counts() {
        let (vertices, indices) = fallback_cube_geometry();

        assert_eq!(vertices.len(), 24);
        assert_eq!(indices.len(), 36);
    }

    #[test]
    fn bootstrap_fallback_render_scene_spawns_camera_and_mesh_renderer() {
        let mut core = SimulationCore::new();
        let mut registry = AssetRegistry::new();
        let scene = bootstrap_fallback_render_scene(&mut core, &mut registry);

        assert_eq!(registry.meshes().len(), 1);
        assert_eq!(core.active_camera(), Some(scene.active_camera));

        let mut mesh_renderers = 0;
        for (_entity, mesh_renderer) in core.world().query::<&MeshRenderer>().iter() {
            mesh_renderers += 1;
            assert!(mesh_renderer.mesh.is_valid());
        }

        assert_eq!(mesh_renderers, 1);

        let camera_name = core
            .world()
            .get::<&Name>(scene.active_camera)
            .expect("camera should have a name");
        let _active = core
            .world()
            .get::<&ActiveCamera>(scene.active_camera)
            .expect("camera should be active");

        assert_eq!(camera_name.0, "FallbackCamera");
    }
}