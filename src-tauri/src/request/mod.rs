use std::{
    fs::{self, File},
    io::Write,
    path::{Path, PathBuf},
    sync::RwLockReadGuard,
};
use toml;

use crate::{
    collection::{self, models::CreateCollectionDto},
    commands::models::CreateNewRequest,
    error::{Error, Result},
    request::models::{CreateRequestDto, HttpReq, ReqToml, ReqTomlConfig, ReqTomlMeta},
    space,
    state::SharedState,
    store::spaces::buffer::SpaceBuf,
    utils,
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
    spacebuf_rlock: &RwLockReadGuard<'_, SpaceBuf>,
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

    if let Some(req_buf) = spacebuf_rlock.requests.get(&relpath) {
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

/// Creates a new request file and updates the shared state
///
/// Validates the request path, creates any necessary parent collections, sanitizes
/// the filename, and creates a TOML file. The shared state is updated with the new
/// space structure after creation.
///
/// Example: Creating a request at `"API/Users/get-user"` will:
/// - Create collections: `api/users`
/// - Generate file: `api/users/get-user.toml`
/// - Update shared state with refreshed space
///
/// - `dto`: Request creation data containing parent path and desired relative path
/// - `sharedstate`: Mutable reference to the application's shared state
///
/// Returns a `Result<CreateNewRequest>` with the created request's path information
pub fn create_req(
    dto: &CreateRequestDto,
    sharedstate: &mut SharedState,
) -> Result<CreateNewRequest> {
    if dto.relpath.trim().is_empty() {
        return Err(Error::FileNotFound(
            "Cannot create a request without name".to_string(),
        ));
    }

    let space = sharedstate
        .space
        .clone()
        .ok_or_else(|| Error::FileNotFound("Active space not found".to_string()))?;
    let space_abspath = PathBuf::from(&space.abspath);

    let relpath = Path::new(&dto.relpath);
    let (parsed_parent_relpath, reqname) = match relpath.parent() {
        Some(parent) if parent != Path::new("") => {
            let parent_str = parent.to_string_lossy().to_string();
            let reqname = relpath.file_name().unwrap().to_string_lossy().to_string();
            (Some(parent_str), reqname)
        }
        _ => (None, dto.relpath.clone()),
    };

    let file_sanitized_name = utils::sanitize_pathseg(&reqname);

    let (file_parent_relpath, file_sanitized_name) = match parsed_parent_relpath {
        Some(ref parent_relpath) => {
            let col_dto = CreateCollectionDto {
                parent_relpath: dto.parent_relpath.clone(),
                relpath: parent_relpath.clone(),
            };

            let parent_sanitized_relpath =
                collection::create_collections_all(&space_abspath, &col_dto)?;

            let full_parent_relpath = utils::join_strpaths(vec![
                dto.parent_relpath.as_str(),
                parent_sanitized_relpath.as_str(),
            ]);

            (full_parent_relpath, file_sanitized_name)
        }
        None => (dto.parent_relpath.clone(), file_sanitized_name),
    };

    let file_abspath = space_abspath
        .join(&file_parent_relpath)
        .join(&file_sanitized_name);
    let file_relpath = utils::join_strpaths(vec![
        file_parent_relpath.as_str(),
        &format!("{file_sanitized_name}.toml"),
    ]);

    create_reqtoml(&file_abspath, &reqname)?;

    let created = CreateNewRequest {
        parent_relpath: file_parent_relpath,
        relpath: file_relpath,
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
