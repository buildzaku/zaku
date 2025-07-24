use std::{
    fs::{self, File},
    io::Write,
    path::{Path, PathBuf},
    sync::MutexGuard,
};
use toml;

use crate::{
    commands::models::CreateNewRequest,
    error::{Error, Result},
    models::SanitizedSegment,
    request::models::{HttpReq, ReqToml, ReqTomlConfig, ReqTomlMeta},
    space,
    state::SharedState,
    store::SpaceBuf,
};

pub mod models;

#[cfg(test)]
pub mod tests;

/// Parses a request file into `HttpReq` struct
///
/// Checks if the entry is a valid TOML file, then attempts to parse it. First checks
/// the space buffer for any unsaved changes, if not then read directly from
/// the filesystem. Returns `None` if the file is not a
/// valid TOML file or if parsing fails.
///
/// - `entry_abspath`: Absolute path to the request file
/// - `space_abspath`: Absolute path of the space
/// - `spacebuf_rlock`: Read lock guard for the space buffer
///
/// Returns an `Option<HttpReq>` containing the parsed request
pub fn parse_req(
    entry_abspath: &Path,
    space_abspath: &Path,
    spacebuf_lock: &MutexGuard<'_, SpaceBuf>,
) -> Option<HttpReq> {
    let is_file = entry_abspath.is_file();
    let is_toml = entry_abspath.extension().and_then(|e| e.to_str()) == Some("toml");
    if !is_file || !is_toml {
        return None;
    }

    let relpath = entry_abspath
        .strip_prefix(space_abspath)
        .unwrap()
        .to_string_lossy()
        .into_owned();

    if let Some(req_buf) = spacebuf_lock.requests.get(&relpath) {
        Some(HttpReq::from_reqbuf(req_buf))
    } else {
        let fsname = entry_abspath
            .file_name()
            .unwrap()
            .to_string_lossy()
            .into_owned();

        match parse_reqtoml(entry_abspath) {
            Ok(req_toml) => Some(HttpReq::from_reqtoml(&req_toml, fsname)),
            Err(_) => {
                eprintln!("Invalid request TOML: '{}'", entry_abspath.display());
                None
            }
        }
    }
}

/// Creates a new request in the specified collection directory
///
/// Creates a new TOML request file in the parent collection directory and updates
/// the shared state
///
/// - `parent_relpath`: Relative path to the parent collection directory
/// - `req_segment`: Sanitized segment containing the request name and filesystem name
/// - `sharedstate`: Mutable reference to the application's shared state
///
/// Returns a `Result<CreateNewRequest>` containing the created request's paths
pub fn create_req(
    parent_relpath: &Path,
    req_segment: &SanitizedSegment,
    sharedstate: &mut SharedState,
) -> Result<CreateNewRequest> {
    if req_segment.fsname.trim().is_empty() {
        return Err(Error::FileNotFound(
            "Cannot create a request without name".to_string(),
        ));
    }

    let space = sharedstate
        .space
        .clone()
        .ok_or_else(|| Error::FileNotFound("Active space not found".to_string()))?;
    let space_abspath = PathBuf::from(&space.abspath);

    let reqfile_abspath = space_abspath.join(parent_relpath).join(&req_segment.fsname);
    let reqfile_relpath = parent_relpath.join(format!("{}.toml", &req_segment.fsname));

    create_reqtoml(&reqfile_abspath, &req_segment.name)?;

    let created = CreateNewRequest {
        parent_relpath: parent_relpath.to_string_lossy().to_string(),
        relpath: reqfile_relpath.to_string_lossy().to_string(),
    };

    sharedstate.space = Some(space::parse_space(&space_abspath)?);

    Ok(created)
}

/// Creates a TOML request file
///
/// The file includes metadata and a basic request configuration
///
/// - `abspath`: Absolute path where the request file should be created
/// - `name`: Name for the request
///
/// Returns a `Result<()>` indicating success or failure
pub fn create_reqtoml(abspath: &Path, name: &str) -> Result<()> {
    let mut reqtoml_file = File::create_new(abspath.with_extension("toml"))?;

    let req_toml = ReqToml {
        meta: ReqTomlMeta {
            name: name.to_string(),
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

/// Parses a TOML request file from the filesystem
///
/// Reads and deserializes a TOML file containing request configuration into a
/// `ReqToml` structure. The file must contain valid TOML syntax and request schema.
///
/// - `abspath`: Absolute path to the TOML file
///
/// Returns a `Result<ReqToml>` containing the parsed request configuration
pub fn parse_reqtoml(abspath: &Path) -> Result<ReqToml> {
    let toml_str = std::fs::read_to_string(abspath)?;
    let req_toml = toml::from_str(&toml_str)?;

    Ok(req_toml)
}

/// Writes a TOML request file on the filesystem
///
/// Serializes the provided `ReqToml` structure into pretty-formatted TOML and
/// writes it to the specified file path. The file must already exist or an error
/// will be returned.
///
/// - `req_abspath`: Absolute path to the TOML request file to update
/// - `req_toml`: Request configuration
///
/// Returns a `Result<()>` indicating success or failure
pub fn update_reqtoml(req_abspath: &Path, req_toml: &ReqToml) -> Result<()> {
    if !req_abspath.exists() {
        return Err(Error::FileNotFound(req_abspath.display().to_string()));
    }

    let toml_str = toml::to_string_pretty(&req_toml)?;
    fs::write(req_abspath, toml_str)?;

    Ok(())
}
