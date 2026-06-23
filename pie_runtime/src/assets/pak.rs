/// Cooked asset types stored in a `.pak` file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum CookedAssetKind {
    Mesh = 0x01,
    Texture = 0x02,
    Shader = 0x03,
    Material = 0x04,
}

impl CookedAssetKind {
    pub fn from_u8(value: u8) -> Option<Self> {
        match value {
            0x01 => Some(Self::Mesh),
            0x02 => Some(Self::Texture),
            0x03 => Some(Self::Shader),
            0x04 => Some(Self::Material),
            _ => None,
        }
    }
}

/// File header for a `.pak` file.
///
/// Layout: `[magic: 4 bytes][version: u32 LE][asset_count: u32 LE]`
pub const PAK_MAGIC: &[u8; 4] = b"PIE\0";
pub const PAK_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq)]
pub struct PakFile {
    pub assets: Vec<PakAsset>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PakAsset {
    pub kind: CookedAssetKind,
    pub name: String,
    pub data: Vec<u8>,
}

impl PakFile {
    /// Serialize a `.pak` file into a writer.
    pub fn write<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        writer.write_all(PAK_MAGIC)?;
        writer.write_all(&PAK_VERSION.to_le_bytes())?;
        writer.write_all(&(self.assets.len() as u32).to_le_bytes())?;

        for asset in &self.assets {
            writer.write_all(&[asset.kind as u8])?;
            writer.write_all(&(asset.name.len() as u32).to_le_bytes())?;
            writer.write_all(asset.name.as_bytes())?;
            writer.write_all(&(asset.data.len() as u64).to_le_bytes())?;
            writer.write_all(&asset.data)?;
        }

        Ok(())
    }

    /// Deserialize a `.pak` file from a reader.
    pub fn read<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let mut magic = [0u8; 4];
        reader.read_exact(&mut magic)?;
        if &magic != PAK_MAGIC {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("invalid pak magic: expected PIE\\0, got {:?}", magic),
            ));
        }

        let mut version_buf = [0u8; 4];
        reader.read_exact(&mut version_buf)?;
        let version = u32::from_le_bytes(version_buf);
        if version != PAK_VERSION {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("unsupported pak version: {version}"),
            ));
        }

        let mut count_buf = [0u8; 4];
        reader.read_exact(&mut count_buf)?;
        let asset_count = u32::from_le_bytes(count_buf) as usize;
        // Sanity bound: a pak with > 100k assets is almost certainly corrupt
        // or malicious. Avoids a multi-GB `Vec::with_capacity` allocation.
        const MAX_ASSET_COUNT: usize = 100_000;
        if asset_count > MAX_ASSET_COUNT {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!(
                    "pak asset count {asset_count} exceeds sane maximum {MAX_ASSET_COUNT}"
                ),
            ));
        }

        let mut assets = Vec::with_capacity(asset_count);
        for _ in 0..asset_count {
            let mut kind_buf = [0u8; 1];
            reader.read_exact(&mut kind_buf)?;
            let kind = CookedAssetKind::from_u8(kind_buf[0]).ok_or_else(|| {
                std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    format!("unknown asset kind: {}", kind_buf[0]),
                )
            })?;

            let mut name_len_buf = [0u8; 4];
            reader.read_exact(&mut name_len_buf)?;
            let name_len = u32::from_le_bytes(name_len_buf) as usize;
            // Sanity bound on asset names: > 4 KB is corrupt/malicious.
            const MAX_NAME_LEN: usize = 4096;
            if name_len > MAX_NAME_LEN {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    format!("asset name length {name_len} exceeds sane maximum {MAX_NAME_LEN}"),
                ));
            }

            let mut name_buf = vec![0u8; name_len];
            reader.read_exact(&mut name_buf)?;
            let name = String::from_utf8(name_buf).map_err(|error| {
                std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    format!("invalid asset name UTF-8: {error}"),
                )
            })?;

            let mut data_len_buf = [0u8; 8];
            reader.read_exact(&mut data_len_buf)?;
            let data_len = u64::from_le_bytes(data_len_buf) as usize;
            // Sanity bound on a single asset's data: > 1 GB is corrupt/malicious.
            const MAX_DATA_LEN: usize = 1024 * 1024 * 1024;
            if data_len > MAX_DATA_LEN {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    format!("asset data length {data_len} exceeds sane maximum {MAX_DATA_LEN}"),
                ));
            }

            let mut data = vec![0u8; data_len];
            reader.read_exact(&mut data)?;

            assets.push(PakAsset { kind, name, data });
        }

        Ok(Self { assets })
    }

    /// Write a `.pak` file to disk.
    pub fn write_to_path(&self, path: &std::path::Path) -> std::io::Result<()> {
        let mut file = std::fs::File::create(path)?;
        self.write(&mut file)
    }

    /// Read a `.pak` file from disk.
    pub fn read_from_path(path: &std::path::Path) -> std::io::Result<Self> {
        let mut file = std::fs::File::open(path)?;
        Self::read(&mut file)
    }
}

#[cfg(test)]
mod tests {
    use super::{CookedAssetKind, PakAsset, PakFile};

    #[test]
    fn pak_round_trip_empty() {
        let pak = PakFile { assets: Vec::new() };
        let mut buf = Vec::new();
        pak.write(&mut buf).expect("write should succeed");
        let read_back = PakFile::read(&mut buf.as_slice()).expect("read should succeed");
        assert_eq!(read_back.assets.len(), 0);
    }

    #[test]
    fn pak_round_trip_multiple_assets() {
        let pak = PakFile {
            assets: vec![
                PakAsset {
                    kind: CookedAssetKind::Mesh,
                    name: "cube".to_string(),
                    data: vec![0xAA; 1024],
                },
                PakAsset {
                    kind: CookedAssetKind::Texture,
                    name: "albedo".to_string(),
                    data: vec![0xBB; 2048],
                },
                PakAsset {
                    kind: CookedAssetKind::Shader,
                    name: "debug_line".to_string(),
                    data: b"shader source".to_vec(),
                },
                PakAsset {
                    kind: CookedAssetKind::Material,
                    name: "default".to_string(),
                    data: vec![0xCC; 64],
                },
            ],
        };
        let mut buf = Vec::new();
        pak.write(&mut buf).expect("write should succeed");
        let read_back = PakFile::read(&mut buf.as_slice()).expect("read should succeed");
        assert_eq!(read_back.assets.len(), 4);
        assert_eq!(read_back.assets[0].kind, CookedAssetKind::Mesh);
        assert_eq!(read_back.assets[0].name, "cube");
        assert_eq!(read_back.assets[0].data.len(), 1024);
    }

    #[test]
    fn pak_rejects_invalid_magic() {
        let mut buf = b"NOTPAK".to_vec();
        buf.extend_from_slice(&1u32.to_le_bytes());
        buf.extend_from_slice(&0u32.to_le_bytes());
        let error = PakFile::read(&mut buf.as_slice()).expect_err("should fail");
        assert!(error.to_string().contains("invalid pak magic"));
    }

    #[test]
    fn pak_rejects_wrong_version() {
        let mut buf = b"PIE\0".to_vec();
        buf.extend_from_slice(&999u32.to_le_bytes());
        buf.extend_from_slice(&0u32.to_le_bytes());
        let error = PakFile::read(&mut buf.as_slice()).expect_err("should fail");
        assert!(error.to_string().contains("unsupported pak version"));
    }

    #[test]
    fn pak_rejects_unknown_asset_kind() {
        let mut buf = b"PIE\0".to_vec();
        buf.extend_from_slice(&1u32.to_le_bytes());
        buf.extend_from_slice(&1u32.to_le_bytes());
        buf.push(0xFF);
        buf.extend_from_slice(&4u32.to_le_bytes());
        buf.extend_from_slice(b"test");
        buf.extend_from_slice(&0u64.to_le_bytes());
        let error = PakFile::read(&mut buf.as_slice()).expect_err("should fail");
        assert!(error.to_string().contains("unknown asset kind"));
    }
}
