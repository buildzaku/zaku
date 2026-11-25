#[cfg(target_os = "windows")]
pub mod windows;

use serde::Deserialize;
use std::fs;

#[derive(Deserialize)]
pub struct ZakuManifest {
    pub package: Package,
}

impl ZakuManifest {
    pub const PATH: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../zaku/Cargo.toml");

    pub fn load() -> anyhow::Result<Self> {
        let content = fs::read_to_string(Self::PATH)?;

        Ok(toml::from_str(&content)?)
    }
}

#[derive(Deserialize)]
pub struct Package {
    pub version: String,
    pub description: String,
    pub metadata: PackageMetadata,
}

#[derive(Deserialize)]
pub struct PackageMetadata {
    pub bundle: BundleMetadata,
}

#[derive(Deserialize)]
pub struct BundleMetadata {
    pub name: String,
    pub identifier: String,
}
