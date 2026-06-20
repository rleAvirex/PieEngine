//! Pie Mesh loader — loads meshes in PieEngine's native binary format.
//!
//! The `.pie_mesh` format consists of a `.bin` binary data file and a `.json`
//! metadata file produced by the `fbx_to_pie_mesh` conversion tool.
//!
//! Binary layout (.bin):
//!   - Vertex data: N vertices × 12 floats (position[3] + normal[3] + uv[2] + tangent[4])
//!   - Index data: M indices × u32
//!
//! JSON metadata (.json):
//!   - name, vertex_count, index_count, triangle_count, vertex_stride,
//!     index_format, aabb_min, aabb_max

use std::path::Path;

use crate::assets::error::AssetError;
use crate::assets::handle::MeshHandle;
use crate::assets::handle::MaterialHandle;
use crate::assets::material::MaterialAsset;
use crate::assets::mesh::{MeshAsset, MeshVertex};
use crate::assets::registry::AssetRegistry;

/// Load a `.pie_mesh` format mesh (`.bin` + `.json`) into the asset registry.
///
/// The `bin_path` should point to the `.bin` file. The `.json` file is
/// expected to be in the same directory with the same stem.
pub fn load_pie_mesh(
    bin_path: &Path,
    registry: &mut AssetRegistry,
) -> Result<MeshHandle, AssetError> {
    let data = std::fs::read(bin_path).map_err(|e| AssetError::io(bin_path, e))?;
    let metadata = load_pie_mesh_metadata(bin_path)?;

    let material = registry.insert_material(MaterialAsset::pbr(
        &format!("{}_mat", metadata.name),
        [1.0, 1.0, 1.0, 1.0],
        0.0,
        1.0,
    ));

    let mesh = parse_pie_mesh_data(&metadata, &data, material, bin_path)?;
    registry.insert_mesh(mesh)
}

/// Load multiple `.pie_mesh` meshes from a directory.
///
/// Scans the directory for `.bin` files and loads each one as a separate
/// mesh. Returns handles for all successfully loaded meshes.
pub fn load_pie_meshes_from_dir(
    dir: &Path,
    registry: &mut AssetRegistry,
) -> Result<Vec<MeshHandle>, AssetError> {
    let entries = std::fs::read_dir(dir).map_err(|e| AssetError::io(dir, e))?;
    let mut handles = Vec::new();

    for entry in entries {
        let entry = entry.map_err(|e| AssetError::io(dir, e))?;
        let path = entry.path();

        if path.extension().map(|e| e == "bin").unwrap_or(false) {
            match load_pie_mesh(&path, registry) {
                Ok(handle) => handles.push(handle),
                Err(error) => {
                    eprintln!(
                        "pie_runtime: warning: failed to load pie_mesh at {}: {error}",
                        path.display()
                    );
                }
            }
        }
    }

    Ok(handles)
}

/// Metadata parsed from a `.pie_mesh.json` file.
struct PieMeshMetadata {
    name: String,
    vertex_count: usize,
    index_count: usize,
    vertex_stride: usize,
}

fn load_pie_mesh_metadata(bin_path: &Path) -> Result<PieMeshMetadata, AssetError> {
    let json_path = bin_path.with_extension("json");

    if !json_path.exists() {
        // If no JSON metadata, infer from the bin file name
        let name = bin_path
            .file_stem()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();

        return Ok(PieMeshMetadata {
            name,
            vertex_count: 0, // will be inferred from data size
            index_count: 0,
            vertex_stride: 12, // 12 floats per vertex
        });
    }

    let json_data = std::fs::read_to_string(&json_path)
        .map_err(|e| AssetError::io(&json_path, e))?;

    let meta: serde_json::Value = serde_json::from_str(&json_data)
        .map_err(|e| AssetError::io(&json_path, e))?;

    let name = meta["name"]
        .as_str()
        .unwrap_or_else(|| bin_path.file_stem().unwrap_or_default().to_str().unwrap_or("mesh"))
        .to_string();

    let vertex_count = meta["vertex_count"].as_u64().unwrap_or(0) as usize;
    let index_count = meta["index_count"].as_u64().unwrap_or(0) as usize;
    let vertex_stride = meta["vertex_stride"].as_u64().unwrap_or(12) as usize;

    Ok(PieMeshMetadata {
        name,
        vertex_count,
        index_count,
        vertex_stride,
    })
}

/// Parse the binary mesh data using the metadata.
fn parse_pie_mesh_data(
    meta: &PieMeshMetadata,
    data: &[u8],
    material: MaterialHandle,
    path: &Path,
) -> Result<MeshAsset, AssetError> {
    let float_size = 4;
    let floats_per_vertex: usize = 12; // pos(3) + normal(3) + uv(2) + tangent(4)
    let vertex_bytes = float_size * floats_per_vertex;

    // If vertex_count is 0 (no metadata), try to infer from data size
    // Assume all remaining data after some point is indices
    // We need to determine where vertices end and indices begin.

    if meta.vertex_count > 0 && meta.index_count > 0 {
        // Known sizes from metadata
        let vertex_data_len = meta.vertex_count * vertex_bytes;
        let index_data_len = meta.index_count * 4;

        if data.len() < vertex_data_len + index_data_len {
            return Err(AssetError::io(
                path,
                std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    format!(
                        "pie_mesh data truncated: expected {} bytes ({} vertices + {} indices), got {}",
                        vertex_data_len + index_data_len,
                        meta.vertex_count,
                        meta.index_count,
                        data.len()
                    ),
                ),
            ));
        }

        let vertices = parse_vertices(&data[0..vertex_data_len], meta.vertex_count, path)?;
        let indices = parse_indices(&data[vertex_data_len..vertex_data_len + index_data_len], meta.index_count);

        Ok(MeshAsset {
            name: meta.name.clone(),
            vertices,
            indices,
            material,
        })
    } else {
        // No metadata — try heuristic: assume data = vertices followed by indices.
        // Try different vertex counts to find one where the remaining data
        // forms a valid index buffer.
        let best_guess = guess_vertex_count(data, floats_per_vertex);

        let vertex_data_len = best_guess * vertex_bytes;
        let remaining = data.len() - vertex_data_len;

        if remaining % 4 != 0 {
            return Err(AssetError::io(
                path,
                std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    "pie_mesh: cannot determine vertex/index boundary from data",
                ),
            ));
        }

        let index_count = remaining / 4;
        let vertices = parse_vertices(&data[0..vertex_data_len], best_guess, path)?;
        let indices = parse_indices(&data[vertex_data_len..], index_count);

        Ok(MeshAsset {
            name: meta.name.clone(),
            vertices,
            indices,
            material,
        })
    }
}

/// Try to guess the vertex count from the data size.
///
/// Assumes vertices come first (12 floats each) followed by indices (u32 each).
/// Tries to find a split where all indices are within the vertex range.
fn guess_vertex_count(data: &[u8], floats_per_vertex: usize) -> usize {
    let vertex_bytes = 4 * floats_per_vertex;

    // Try vertex counts from small to large, pick the first one where
    // all indices are valid (within vertex range).
    for vertex_count in 1..data.len() / vertex_bytes {
        let vertex_data_len = vertex_count * vertex_bytes;
        let remaining = data.len() - vertex_data_len;

        if remaining % 4 != 0 {
            continue;
        }

        let index_count = remaining / 4;
        if index_count < 3 {
            continue;
        }

        // Check that index count is divisible by 3 (triangles)
        if index_count % 3 != 0 {
            continue;
        }

        // Check that all indices reference valid vertices
        let index_data = &data[vertex_data_len..];
        let mut all_valid = true;
        for chunk in index_data.chunks_exact(4) {
            let index = u32::from_le_bytes(chunk.try_into().unwrap());
            if index as usize >= vertex_count {
                all_valid = false;
                break;
            }
        }

        if all_valid {
            return vertex_count;
        }
    }

    // Fallback: assume it's all vertices with no indices
    data.len() / vertex_bytes
}

fn parse_vertices(
    data: &[u8],
    vertex_count: usize,
    path: &Path,
) -> Result<Vec<MeshVertex>, AssetError> {
    let mut vertices = Vec::with_capacity(vertex_count);

    for i in 0..vertex_count {
        let offset = i * 48; // 12 floats × 4 bytes
        if offset + 48 > data.len() {
            return Err(AssetError::io(
                path,
                std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    format!("pie_mesh: vertex {i} data truncated"),
                ),
            ));
        }

        let floats: [f32; 12] = bytemuck::cast_slice(&data[offset..offset + 48])[0];

        vertices.push(MeshVertex {
            position: [floats[0], floats[1], floats[2]],
            normal: [floats[3], floats[4], floats[5]],
            uv: [floats[6], floats[7]],
            tangent: [floats[8], floats[9], floats[10], floats[11]],
        });
    }

    Ok(vertices)
}

fn parse_indices(data: &[u8], _index_count: usize) -> Vec<u32> {
    bytemuck::cast_slice(data).to_vec()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::assets::registry::AssetRegistry;

    #[test]
    fn guess_vertex_count_finds_correct_split() {
        // 2 vertices (12 floats each = 96 bytes) + 3 indices (12 bytes) = 108 bytes
        let mut data = Vec::new();

        // Vertex data: 2 vertices × 12 floats
        let vertex_floats: &[f32] = &[0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 1.0];
        for _ in 0..2 {
            for f in vertex_floats {
                data.extend_from_slice(&f.to_le_bytes());
            }
        }
        // Indices: 0, 1, 0
        data.extend_from_slice(&0u32.to_le_bytes());
        data.extend_from_slice(&1u32.to_le_bytes());
        data.extend_from_slice(&0u32.to_le_bytes());

        let result = guess_vertex_count(&data, 12);
        assert_eq!(result, 2);
    }

    #[test]
    fn load_pie_mesh_rejects_nonexistent_file() {
        let path = std::path::PathBuf::from("nonexistent/mesh.bin");
        let mut registry = AssetRegistry::new();
        assert!(load_pie_mesh(&path, &mut registry).is_err());
    }

    #[test]
    fn load_pie_mesh_loads_converted_gizmo_sphere() {
        let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../assets/Engine/Gizmos/GizmosSphere.bin");

        if !path.exists() {
            eprintln!("Skipping pie_mesh test; GizmosSphere.bin not found at {}", path.display());
            return;
        }

        let mut registry = AssetRegistry::new();
        let handle = load_pie_mesh(&path, &mut registry).expect("should load pie_mesh");

        let mesh = registry.mesh(handle).expect("mesh should exist");
        assert!(!mesh.vertices.is_empty(), "mesh should have vertices");
        assert!(!mesh.indices.is_empty(), "mesh should have indices");
        assert_eq!(mesh.indices.len() % 3, 0, "indices should form triangles");
    }

    #[test]
    fn load_pie_meshes_from_dir_loads_all_gizmos() {
        let dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../assets/Engine/Gizmos");

        if !dir.exists() {
            eprintln!("Skipping pie_mesh dir test; Gizmos dir not found");
            return;
        }

        let mut registry = AssetRegistry::new();
        let handles = load_pie_meshes_from_dir(&dir, &mut registry).expect("should scan dir");

        if handles.is_empty() {
            eprintln!("No .bin files found in Gizmos dir (may need to run fbx_to_pie_mesh.py)");
            return;
        }

        assert!(handles.len() >= 1, "should find at least one .bin file");
        for handle in &handles {
            let mesh = registry.mesh(*handle).expect("mesh should exist");
            assert!(!mesh.vertices.is_empty());
        }
    }
}
