use std::fs::{self, File};
use std::io::Write;
use std::path::{Path, PathBuf};
use toml;

use crate::collection::models::CreateCollectionDto;
use crate::commands::models::CreateNewRequest;
use crate::error::{Error, Result};
use crate::request::models::{CreateRequestDto, ReqToml, ReqTomlConfig, ReqTomlMeta};
use crate::state::SharedState;
use crate::{collection, space, utils};

pub mod models;

pub fn create_req(
    dto: &CreateRequestDto,
    sharedstate: &mut SharedState,
) -> Result<CreateNewRequest> {
    if dto.relpath.trim().is_empty() {
        return Err(Error::FileNotFound(
            "Cannot create a request without name".to_string(),
        ));
    }

    let active_space = sharedstate
        .active_space
        .clone()
        .ok_or_else(|| Error::FileNotFound("Active space not found".to_string()))?;

    let space_abspath = PathBuf::from(&active_space.abspath);

    let (parsed_parent_relpath, reqname) = match dto.relpath.rfind('/') {
        Some(index) => {
            let parent = &dto.relpath[..index];
            let name = &dto.relpath[index + 1..];
            (Some(parent.to_string()), name.to_string())
        }
        None => (None, dto.relpath.clone()),
    };

    let file_sanitized_name = utils::sanitize_path_segment(&reqname);

    let (file_parent_relpath, file_sanitized_name) = match parsed_parent_relpath {
        Some(ref parent_relpath) => {
            let col_dto = CreateCollectionDto {
                parent_relpath: dto.parent_relpath.clone(),
                relpath: parent_relpath.clone(),
            };

            let parent_sanitized_relpath =
                collection::create_collections_all(&space_abspath, &col_dto)?;

            let full_parent_relpath = utils::join_str_paths(vec![
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
    let file_relpath = utils::join_str_paths(vec![
        file_parent_relpath.as_str(),
        &format!("{file_sanitized_name}.toml"),
    ]);

    create_reqtoml(&file_abspath, &reqname)?;

    let created = CreateNewRequest {
        parent_relpath: file_parent_relpath,
        relpath: file_relpath,
    };

    sharedstate.active_space = Some(space::parse_space(&space_abspath)?);

    Ok(created)
}

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

    Ok(())
}
