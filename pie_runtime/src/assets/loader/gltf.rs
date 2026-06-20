use std::path::Path;

use glam::{Quat, Vec3};

use crate::assets::error::AssetError;
use crate::assets::handle::MeshHandle;
use crate::assets::loader::image::texture_from_rgba;
use crate::assets::material::MaterialAsset;
use crate::assets::mesh::{MeshAsset, MeshVertex};
use crate::assets::registry::AssetRegistry;

#[derive(Debug, Clone, PartialEq)]
pub struct ImportedNode {
    pub name: String,
    pub translation: Vec3,
    pub rotation: Quat,
    pub scale: Vec3,
    pub mesh: Option<MeshHandle>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ImportedScene {
    pub path: std::path::PathBuf,
    pub nodes: Vec<ImportedNode>,
    pub active_camera_index: Option<usize>,
}

pub fn load_gltf_scene(
    path: &Path,
    registry: &mut AssetRegistry,
) -> Result<ImportedScene, AssetError> {
    let (document, buffers, images) =
        gltf::import(path).map_err(|error| AssetError::gltf(path, error))?;

    let mut texture_handles = Vec::new();
    for (index, image) in images.into_iter().enumerate() {
        let texture = texture_from_rgba(
            format!("gltf_image_{index}"),
            image.width,
            image.height,
            image.pixels,
        )?;
        texture_handles.push(registry.insert_texture(texture)?);
    }

    let mut material_handles = Vec::new();
    for (index, material) in document.materials().enumerate() {
        let pbr = material.pbr_metallic_roughness();
        let base_color_texture = pbr
            .base_color_texture()
            .map(|info| info.texture().source().index())
            .and_then(|source_index| texture_handles.get(source_index).copied());

        let normal_texture = material
            .normal_texture()
            .map(|info| info.texture().source().index())
            .and_then(|source_index| texture_handles.get(source_index).copied());

        material_handles.push(
            registry.insert_material(MaterialAsset {
                name: material
                    .name()
                    .map(str::to_string)
                    .unwrap_or_else(|| format!("material_{index}")),
                base_color_texture,
                normal_texture,
                base_color_factor: pbr.base_color_factor(),
                metallic_factor: pbr.metallic_factor(),
                roughness_factor: pbr.roughness_factor(),
            }),
        );
    }

    let default_material = registry.insert_material(MaterialAsset::pbr(
        "default",
        [1.0, 1.0, 1.0, 1.0],
        0.0,
        1.0,
    ));

    let mut mesh_handles = Vec::new();

    for mesh in document.meshes() {
        let mesh_name = mesh
            .name()
            .map(str::to_string)
            .unwrap_or_else(|| format!("mesh_{}", mesh_handles.len()));

        let mut combined_vertices = Vec::new();
        let mut combined_indices = Vec::new();
        let mut mesh_material = default_material;

        for primitive in mesh.primitives() {
            let reader = primitive
                .reader(|buffer| buffers.get(buffer.index()).map(|data| data.0.as_slice()));

            let positions = reader
                .read_positions()
                .ok_or_else(|| AssetError::EmptyMesh {
                    path: path.to_path_buf(),
                    mesh_name: mesh_name.clone(),
                })?
                .map(|position| [position[0], position[1], position[2]])
                .collect::<Vec<_>>();

            let normals = reader
                .read_normals()
                .map(|normals| {
                    normals
                        .map(|normal| [normal[0], normal[1], normal[2]])
                        .collect::<Vec<_>>()
                })
                .unwrap_or_else(|| vec![[0.0, 1.0, 0.0]; positions.len()]);

            let uvs = reader
                .read_tex_coords(0)
                .map(|coords| {
                    coords
                        .into_f32()
                        .map(|uv| [uv[0], uv[1]])
                        .collect::<Vec<_>>()
                })
                .unwrap_or_else(|| vec![[0.0, 0.0]; positions.len()]);

            let tangents = reader
                .read_tangents()
                .map(|t| {
                    t.map(|t| [t[0], t[1], t[2], t[3]])
                        .collect::<Vec<_>>()
                })
                .unwrap_or_else(|| vec![[1.0, 0.0, 0.0, 1.0]; positions.len()]);

            let base_index = combined_vertices.len() as u32;
            for vertex_index in 0..positions.len() {
                combined_vertices.push(MeshVertex {
                    position: positions[vertex_index],
                    normal: normals[vertex_index],
                    uv: uvs[vertex_index],
                    tangent: tangents[vertex_index],
                });
            }

            if let Some(material) = primitive.material().index() {
                if let Some(handle) = material_handles.get(material) {
                    mesh_material = *handle;
                }
            }

            if let Some(indices) = reader.read_indices() {
                combined_indices.extend(indices.into_u32().map(|index| index + base_index));
            } else {
                combined_indices
                    .extend((0..positions.len() as u32).map(|index| index + base_index));
            }
        }

        if combined_vertices.is_empty() || combined_indices.is_empty() {
            return Err(AssetError::EmptyMesh {
                path: path.to_path_buf(),
                mesh_name,
            });
        }

        mesh_handles.push(registry.insert_mesh(MeshAsset {
            name: mesh_name,
            vertices: combined_vertices,
            indices: combined_indices,
            material: mesh_material,
        })?);
    }

    let scene = document
        .default_scene()
        .or_else(|| document.scenes().next())
        .ok_or(AssetError::MissingScene)?;

    let mut nodes = Vec::new();
    let mut active_camera_index = None;

    for node in scene.nodes() {
        collect_node(
            path,
            node,
            &mesh_handles,
            &mut nodes,
            &mut active_camera_index,
        );
    }

    Ok(ImportedScene {
        path: path.to_path_buf(),
        nodes,
        active_camera_index,
    })
}

fn collect_node(
    path: &Path,
    node: gltf::Node,
    mesh_handles: &[MeshHandle],
    nodes: &mut Vec<ImportedNode>,
    active_camera_index: &mut Option<usize>,
) {
    let (translation, rotation, scale) = node.transform().decomposed();
    let imported = ImportedNode {
        name: node
            .name()
            .map(str::to_string)
            .unwrap_or_else(|| format!("node_{}", nodes.len())),
        translation: Vec3::from_array(translation),
        rotation: Quat::from_array(rotation),
        scale: Vec3::from_array(scale),
        mesh: node.mesh().map(|mesh| mesh.index()).and_then(|index| {
            mesh_handles
                .get(index)
                .copied()
                .ok_or_else(|| AssetError::InvalidHandle {
                    kind: "mesh",
                    index: index as u32,
                })
                .ok()
        }),
    };

    if node.camera().is_some() {
        *active_camera_index = Some(nodes.len());
    }

    nodes.push(imported);

    for child in node.children() {
        collect_node(path, child, mesh_handles, nodes, active_camera_index);
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::load_gltf_scene;
    use crate::assets::registry::AssetRegistry;

    fn sample_scene_path() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../assets/sample/scene.gltf")
    }

    #[test]
    fn load_gltf_scene_loads_sample_mesh_and_texture() {
        let path = sample_scene_path();
        if !path.exists() {
            eprintln!(
                "skipping glTF test; sample scene missing at {}",
                path.display()
            );
            return;
        }

        let mut registry = AssetRegistry::new();
        let scene = load_gltf_scene(&path, &mut registry).expect("sample scene should load");

        assert!(!scene.nodes.is_empty());
        assert_eq!(registry.meshes().len(), 1);
        assert_eq!(registry.textures().len(), 1);
        assert_eq!(registry.materials().len(), 2);
        assert!(scene.nodes.iter().any(|node| node.mesh.is_some()));
    }

    #[test]
    fn load_gltf_scene_reports_missing_files_clearly() {
        let mut registry = AssetRegistry::new();
        let error = load_gltf_scene(PathBuf::from("missing/scene.gltf").as_path(), &mut registry)
            .expect_err("missing glTF should fail");

        assert!(error.to_string().contains("failed to load glTF"));
    }
}
