use std::path::Path;

use image::ImageReader;

use crate::assets::error::AssetError;
use crate::assets::texture::TextureAsset;

pub fn load_texture_from_path(path: &Path) -> Result<TextureAsset, AssetError> {
    let image = ImageReader::open(path)
        .map_err(|error| AssetError::image(path, error))?
        .decode()
        .map_err(|error| AssetError::image(path, error))?
        .into_rgba8();

    let (width, height) = image.dimensions();

    Ok(TextureAsset {
        name: path
            .file_stem()
            .and_then(|name| name.to_str())
            .unwrap_or("texture")
            .to_string(),
        width,
        height,
        rgba: image.into_raw(),
    })
}

pub fn texture_from_rgba(
    name: impl Into<String>,
    width: u32,
    height: u32,
    rgba: Vec<u8>,
) -> Result<TextureAsset, AssetError> {
    let texture = TextureAsset {
        name: name.into(),
        width,
        height,
        rgba,
    };
    texture
        .validate()
        .map_err(|message| AssetError::image("memory", message))?;
    Ok(texture)
}

#[cfg(test)]
mod tests {
    use super::texture_from_rgba;

    #[test]
    fn texture_from_rgba_rejects_invalid_buffers() {
        let error = texture_from_rgba("bad", 2, 2, vec![255; 3]).expect_err("invalid buffer");

        assert!(error.to_string().contains("texture"));
    }
}
