use std::fs::{self, File};
use std::io::Write;
use std::path::{Path, PathBuf};
use toml;

use crate::error::{Error, Result};
use crate::request::models::{ReqToml, ReqTomlConfig, ReqTomlMeta};

pub mod models;

pub fn create_reqtoml(abspath: &Path, display_name: &str) -> Result<()> {
    let mut reqtoml_file = File::create_new(&abspath.with_extension("toml"))?;

    let req_toml = ReqToml {
        meta: ReqTomlMeta {
            name: display_name.to_string(),
        },
        config: ReqTomlConfig {
            method: "GET".to_string(),
            url: None,
            headers: None,
            parameters: None,
            content_type: None,
            body: None,
        },
    };

    let toml_str = toml::to_string_pretty(&req_toml)?;

    reqtoml_file.write_all(toml_str.as_bytes())?;

    Ok(())
}

pub fn parse_reqtoml(abspath: &PathBuf) -> Result<ReqToml> {
    let toml_str = std::fs::read_to_string(abspath)?;
    let req_toml = toml::from_str(&toml_str)?;

    Ok(req_toml)
}

pub fn persist_reqtoml(req_abspath: &Path, req_toml: &ReqToml) -> Result<()> {
    if !req_abspath.exists() {
        return Err(Error::FileNotFound(req_abspath.display().to_string()));
    }

    let toml_str = toml::to_string_pretty(&req_toml)?;
    fs::write(req_abspath, toml_str)?;

    return Ok(());
}
