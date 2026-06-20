use std::fs;
use std::path::{Path, PathBuf};

use pie_runtime::assets::{load_gltf_scene, AssetError, AssetRegistry};

use crate::pak::{CookedAssetKind, PakAsset, PakFile};

/// Errors that can occur during the cooking process.
#[derive(Debug)]
pub enum CookError {
    Io(std::io::Error),
    Asset(AssetError),
    MissingInput(PathBuf),
}

impl std::fmt::Display for CookError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(error) => write!(f, "io error: {error}"),
            Self::Asset(error) => write!(f, "asset error: {error}"),
            Self::MissingInput(path) => write!(f, "input path does not exist: {}", path.display()),
        }
    }
}

impl std::error::Error for CookError {}

impl From<std::io::Error> for CookError {
    fn from(error: std::io::Error) -> Self {
        Self::Io(error)
    }
}

impl From<AssetError> for CookError {
    fn from(error: AssetError) -> Self {
        Self::Asset(error)
    }
}

/// Cook all assets from `input_dir` and produce a `PakFile`.
///
/// Expected input layout:
///   `{input_dir}/shaders/*.wgsl`       — shader source files
///   `{input_dir}/sample/scene.gltf`    — glTF scene (with external mesh/texture refs)
///   `{input_dir}/sample/*.png`         – texture images referenced by glTF
pub fn cook_assets(input_dir: &Path) -> Result<PakFile, CookError> {
    if !input_dir.exists() {
        return Err(CookError::MissingInput(input_dir.to_path_buf()));
    }

    let mut assets = Vec::new();

    // 1. Cook shaders first (glTF materials may reference them indirectly).
    cook_shaders(input_dir, &mut assets)?;

    // 2. Cook the glTF scene (meshes + materials + embedded textures).
    cook_gltf_scene(input_dir, &mut assets)?;

    Ok(PakFile { assets })
}

/// Cook all `.wgsl` shader files from `{input_dir}/shaders/`.
fn cook_shaders(input_dir: &Path, assets: &mut Vec<PakAsset>) -> Result<(), CookError> {
    let shaders_dir = input_dir.join("shaders");
    if !shaders_dir.exists() {
        return Ok(());
    }

    for entry in fs::read_dir(&shaders_dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.extension().and_then(|ext| ext.to_str()) != Some("wgsl") {
            continue;
        }

        let name = path
            .file_stem()
            .and_then(|stem| stem.to_str())
            .unwrap_or("unknown")
            .to_string();
        let source = fs::read_to_string(&path)?;

        assets.push(PakAsset {
            kind: CookedAssetKind::Shader,
            name,
            data: source.into_bytes(),
        });
    }

    Ok(())
}

/// Cook a glTF scene and its referenced assets.
///
/// Uses the existing `load_gltf_scene` path to parse the glTF, then serializes
/// the resulting runtime assets into pak form.
fn cook_gltf_scene(input_dir: &Path, assets: &mut Vec<PakAsset>) -> Result<(), CookError> {
    // Look for the sample scene at the well-known path.
    let scene_path = input_dir.join("sample/scene.gltf");
    if !scene_path.exists() {
        return Ok(());
    }

    let mut registry = AssetRegistry::new();
    let imported = load_gltf_scene(&scene_path, &mut registry)?;

    // Cook textures.
    for (index, texture) in registry.textures().iter().enumerate() {
        let mut data = Vec::new();
        // Serialize: width: u32, height: u32, rgba: [u8]
        data.extend_from_slice(&texture.width.to_le_bytes());
        data.extend_from_slice(&texture.height.to_le_bytes());
        data.extend_from_slice(&texture.rgba);

        assets.push(PakAsset {
            kind: CookedAssetKind::Texture,
            name: texture.name.clone(),
            data,
        });
        let _ = index; // suppress unused warning
    }

    // Cook materials.
    for material in registry.materials() {
        let mut data = Vec::new();
        // base_color_factor: [f32; 4]
        for component in &material.base_color_factor {
            data.extend_from_slice(&component.to_le_bytes());
        }
        // metallic_factor: f32
        data.extend_from_slice(&material.metallic_factor.to_le_bytes());
        // roughness_factor: f32
        data.extend_from_slice(&material.roughness_factor.to_le_bytes());
        // base_color_texture index: u32 (or u32::MAX if none)
        let texture_index = material
            .base_color_texture
            .map(|h| h.index())
            .unwrap_or(u32::MAX);
        data.extend_from_slice(&texture_index.to_le_bytes());
        // normal_texture index: u32 (or u32::MAX if none)
        let normal_index = material
            .normal_texture
            .map(|h| h.index())
            .unwrap_or(u32::MAX);
        data.extend_from_slice(&normal_index.to_le_bytes());

        assets.push(PakAsset {
            kind: CookedAssetKind::Material,
            name: material.name.clone(),
            data,
        });
    }

    // Cook meshes.
    for mesh in registry.meshes() {
        let mut data = Vec::new();
        // material handle index: u32
        data.extend_from_slice(&mesh.material.index().to_le_bytes());
        // vertex count: u32
        let vertex_count = mesh.vertices.len() as u32;
        data.extend_from_slice(&vertex_count.to_le_bytes());
        // vertices: each is 32 bytes (position 12 + normal 12 + uv 8 + tangent 16)
        for vertex in &mesh.vertices {
            for component in &vertex.position {
                data.extend_from_slice(&component.to_le_bytes());
            }
            for component in &vertex.normal {
                data.extend_from_slice(&component.to_le_bytes());
            }
            for component in &vertex.uv {
                data.extend_from_slice(&component.to_le_bytes());
            }
            for component in &vertex.tangent {
                data.extend_from_slice(&component.to_le_bytes());
            }
        }
        // index count: u32
        let index_count = mesh.indices.len() as u32;
        data.extend_from_slice(&index_count.to_le_bytes());
        // indices: [u32]
        for index in &mesh.indices {
            data.extend_from_slice(&index.to_le_bytes());
        }

        assets.push(PakAsset {
            kind: CookedAssetKind::Mesh,
            name: mesh.name.clone(),
            data,
        });
    }

    let _ = imported; // scene import info used for entity spawning at runtime
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::cook_assets;
    use crate::pak::CookedAssetKind;
    use std::path::{Path, PathBuf};

    fn sample_assets_dir() -> std::path::PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../assets")
    }

    #[test]
    fn cook_sample_assets_produces_expected_assets() {
        let input_dir = sample_assets_dir();
        if !input_dir.join("sample/scene.gltf").exists() {
            eprintln!("skipping cook test; sample scene missing");
            return;
        }

        let pak = cook_assets(&input_dir).expect("cooking should succeed");

        // Should have at least: 2 shaders, 1 texture, 2 materials, 1 mesh
        let shader_count = pak
            .assets
            .iter()
            .filter(|a| a.kind == CookedAssetKind::Shader)
            .count();
        let texture_count = pak
            .assets
            .iter()
            .filter(|a| a.kind == CookedAssetKind::Texture)
            .count();
        let material_count = pak
            .assets
            .iter()
            .filter(|a| a.kind == CookedAssetKind::Material)
            .count();
        let mesh_count = pak
            .assets
            .iter()
            .filter(|a| a.kind == CookedAssetKind::Mesh)
            .count();

        assert!(
            shader_count >= 2,
            "expected at least 2 shaders, got {shader_count}"
        );
        assert!(
            texture_count >= 1,
            "expected at least 1 texture, got {texture_count}"
        );
        assert!(
            material_count >= 2,
            "expected at least 2 materials, got {material_count}"
        );
        assert!(
            mesh_count >= 1,
            "expected at least 1 mesh, got {mesh_count}"
        );
    }

    #[test]
    fn cook_missing_input_reports_error() {
        let result = cook_assets(Path::new("/nonexistent/path"));
        assert!(result.is_err());
    }
}
