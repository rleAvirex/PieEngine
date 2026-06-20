use glam::{Quat, Vec3};
use hecs::Entity;

use crate::assets::loader::gltf::ImportedScene;
use crate::components::{ActiveCamera, Camera, MeshRenderer, Name, Transform};
use crate::core::{BootstrapSceneResult, SimulationCore};

fn look_at_camera_transform(eye: Vec3, target: Vec3) -> Transform {
    let forward = (target - eye).normalize_or_zero();
    let rotation = if forward.length_squared() > f32::EPSILON {
        Quat::from_rotation_arc(Vec3::NEG_Z, forward)
    } else {
        Quat::IDENTITY
    };

    Transform {
        translation: eye,
        rotation,
        scale: Vec3::ONE,
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct SpawnedScene {
    pub active_camera: Entity,
    pub mesh_entities: Vec<Entity>,
}

pub fn spawn_imported_scene(core: &mut SimulationCore, imported: &ImportedScene) -> SpawnedScene {
    let mut mesh_entities = Vec::new();

    for node in &imported.nodes {
        let Some(mesh) = node.mesh else {
            continue;
        };

        let entity = core.world_mut().spawn((
            Name::new(node.name.clone()),
            Transform {
                translation: node.translation,
                rotation: node.rotation,
                scale: node.scale,
            },
            MeshRenderer { mesh },
        ));
        mesh_entities.push(entity);
    }

    let active_camera = if let Some(camera_index) = imported.active_camera_index {
        let node = &imported.nodes[camera_index];
        core.world_mut().spawn((
            Name::new(format!("{}Camera", node.name)),
            ActiveCamera,
            Camera::default(),
            Transform {
                translation: node.translation,
                rotation: node.rotation,
                scale: node.scale,
            },
        ))
    } else {
        core.world_mut().spawn((
            Name::new("ImportedSceneCamera"),
            ActiveCamera,
            Camera::default(),
            default_scene_camera(),
        ))
    };

    SpawnedScene {
        active_camera,
        mesh_entities,
    }
}

pub fn bootstrap_scene_result_from_spawned(spawned: &SpawnedScene) -> BootstrapSceneResult {
    BootstrapSceneResult {
        active_camera: spawned.active_camera,
    }
}

fn default_scene_camera() -> Transform {
    look_at_camera_transform(Vec3::new(0.0, 1.0, 5.0), Vec3::ZERO)
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::spawn_imported_scene;
    use crate::assets::loader::gltf::load_gltf_scene;
    use crate::assets::registry::AssetRegistry;
    use crate::components::{ActiveCamera, MeshRenderer, Name};
    use crate::core::SimulationCore;

    #[test]
    fn spawn_imported_scene_creates_mesh_entities_and_camera() {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../assets/sample/scene.gltf");
        if !path.exists() {
            eprintln!(
                "skipping spawn test; sample scene missing at {}",
                path.display()
            );
            return;
        }

        let mut registry = AssetRegistry::new();
        let imported = load_gltf_scene(&path, &mut registry).expect("sample scene should load");
        let mut core = SimulationCore::new();
        let spawned = spawn_imported_scene(&mut core, &imported);

        assert_eq!(spawned.mesh_entities.len(), 1);
        assert_eq!(core.active_camera(), Some(spawned.active_camera));

        let mesh_entity = spawned.mesh_entities[0];
        let _mesh = core
            .world()
            .get::<&MeshRenderer>(mesh_entity)
            .expect("spawned entity should have mesh renderer");
        let _name = core
            .world()
            .get::<&Name>(mesh_entity)
            .expect("spawned entity should have a name");
        let _camera = core
            .world()
            .get::<&ActiveCamera>(spawned.active_camera)
            .expect("camera should be active");
    }
}
