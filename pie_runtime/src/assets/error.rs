use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AssetError {
    Io { path: PathBuf, message: String },
    Gltf { path: PathBuf, message: String },
    Image { path: PathBuf, message: String },
    Shader { path: PathBuf, message: String },
    MissingScene,
    InvalidHandle { kind: &'static str, index: u32 },
    EmptyMesh { path: PathBuf, mesh_name: String },
}

impl AssetError {
    pub fn io(path: impl AsRef<Path>, error: impl std::fmt::Display) -> Self {
        Self::Io {
            path: path.as_ref().to_path_buf(),
            message: error.to_string(),
        }
    }

    pub fn gltf(path: impl AsRef<Path>, error: impl std::fmt::Display) -> Self {
        Self::Gltf {
            path: path.as_ref().to_path_buf(),
            message: error.to_string(),
        }
    }

    pub fn image(path: impl AsRef<Path>, error: impl std::fmt::Display) -> Self {
        Self::Image {
            path: path.as_ref().to_path_buf(),
            message: error.to_string(),
        }
    }

    pub fn shader(path: impl AsRef<Path>, error: impl std::fmt::Display) -> Self {
        Self::Shader {
            path: path.as_ref().to_path_buf(),
            message: error.to_string(),
        }
    }
}

impl std::fmt::Display for AssetError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io { path, message } => {
                write!(f, "failed to read asset at {}: {message}", path.display())
            }
            Self::Gltf { path, message } => {
                write!(f, "failed to load glTF at {}: {message}", path.display())
            }
            Self::Image { path, message } => {
                write!(f, "failed to load texture at {}: {message}", path.display())
            }
            Self::Shader { path, message } => {
                write!(f, "failed to load shader at {}: {message}", path.display())
            }
            Self::MissingScene => write!(f, "glTF file did not contain a default scene"),
            Self::InvalidHandle { kind, index } => {
                write!(f, "invalid {kind} handle with index {index}")
            }
            Self::EmptyMesh { path, mesh_name } => {
                write!(
                    f,
                    "glTF mesh `{mesh_name}` in {} did not contain drawable geometry",
                    path.display()
                )
            }
        }
    }
}

impl std::error::Error for AssetError {}
