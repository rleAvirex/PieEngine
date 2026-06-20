#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextureAsset {
    pub name: String,
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

impl TextureAsset {
    pub fn validate(&self) -> Result<(), &'static str> {
        if self.width == 0 || self.height == 0 {
            return Err("texture dimensions must be non-zero");
        }

        let expected_len = (self.width as usize)
            .checked_mul(self.height as usize)
            .and_then(|pixels| pixels.checked_mul(4))
            .ok_or("texture dimensions overflow")?;

        if self.rgba.len() != expected_len {
            return Err("texture pixel buffer length does not match dimensions");
        }

        Ok(())
    }
}
