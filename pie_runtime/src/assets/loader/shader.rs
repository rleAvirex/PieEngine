use std::fs;
use std::path::Path;

use crate::assets::error::AssetError;

/// Shader assets live under `{assets_root}/shaders/*.wgsl`.
pub fn shader_asset_path(assets_root: &Path, shader_name: &str) -> std::path::PathBuf {
    assets_root
        .join("shaders")
        .join(format!("{shader_name}.wgsl"))
}

pub fn load_shader_source(path: &Path) -> Result<String, AssetError> {
    fs::read_to_string(path).map_err(|error| AssetError::shader(path, error))
}

pub fn load_shader_named(assets_root: &Path, shader_name: &str) -> Result<String, AssetError> {
    let path = shader_asset_path(assets_root, shader_name);
    load_shader_source(&path)
}

#[cfg(test)]
mod tests {
    use super::{load_shader_source, shader_asset_path};
    use std::path::{Path, PathBuf};

    #[test]
    fn shader_asset_path_uses_assets_root_and_wgsl_extension() {
        let path = shader_asset_path(Path::new("assets"), "textured_mesh");

        assert_eq!(path, PathBuf::from("assets/shaders/textured_mesh.wgsl"));
    }

    #[test]
    fn load_shader_source_surfaces_io_errors() {
        let error = load_shader_source(Path::new("assets/shaders/does_not_exist.wgsl"))
            .expect_err("missing shader should fail");

        assert!(error.to_string().contains("failed to load shader"));
    }
}
