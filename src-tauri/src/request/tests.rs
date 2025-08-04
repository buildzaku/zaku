use std::{fs, path::PathBuf};

use crate::{
    collection,
    error::Error,
    models::SanitizedSegment,
    request::{
        self,
        models::{HttpReq, ReqToml, ReqTomlConfig, ReqTomlMeta},
    },
    store::{
        self, ReqBuffer,
        spaces::buffer::{ReqBufferCfg, ReqBufferMeta, ReqBufferUrl, SpaceBufferStore},
    },
};

#[test]
fn parse_req_returns_none_for_non_toml_file() {
    let (_tmp_datadir, tmp_spacedir, _state_store) = store::utils::temp_space("Test Space");
    let tmp_space_abspath = tmp_spacedir.path();

    let txt_file_abspath = tmp_space_abspath.join("parent-req-1.txt");
    fs::write(&txt_file_abspath, "not a toml file").unwrap();

    let sbf_store_abspath = store::utils::sbf_store_abspath(tmp_space_abspath);
    let sbf_store = SpaceBufferStore::get(&sbf_store_abspath).unwrap();
    let sbf_store_mtx = sbf_store
        .lock()
        .map_err(|_| Error::LockError("Failed to acquire mutex lock".into()))
        .unwrap();

    let result = request::parse_req(&txt_file_abspath, tmp_space_abspath, &sbf_store_mtx);
    assert!(result.is_none());
}

#[test]
fn parse_req_returns_none_for_directory() {
    let (_tmp_datadir, tmp_spacedir, _state_store) = store::utils::temp_space("Test Space");
    let tmp_space_abspath = tmp_spacedir.path();

    let dir_abspath = tmp_space_abspath.join("parent-col-1");
    fs::create_dir_all(&dir_abspath).unwrap();

    let sbf_store_abspath = store::utils::sbf_store_abspath(tmp_space_abspath);
    let sbf_store = SpaceBufferStore::get(&sbf_store_abspath).unwrap();
    let sbf_store_mtx = sbf_store
        .lock()
        .map_err(|_| Error::LockError("Failed to acquire mutex lock".into()))
        .unwrap();

    let result = request::parse_req(&dir_abspath, tmp_space_abspath, &sbf_store_mtx);
    assert!(result.is_none());
}

#[test]
fn parse_req_successfully_parses_valid_toml_file() {
    let (_tmp_datadir, tmp_spacedir, _state_store) = store::utils::temp_space("Test Space");
    let tmp_space_abspath = tmp_spacedir.path();

    let reqfile_abspath = tmp_space_abspath.join("parent-req-1");
    request::create_reqtoml(&reqfile_abspath, "Parent Req 1").unwrap();

    let sbf_store_abspath = store::utils::sbf_store_abspath(tmp_space_abspath);
    let sbf_store = SpaceBufferStore::get(&sbf_store_abspath).unwrap();
    let sbf_store_mtx = sbf_store
        .lock()
        .map_err(|_| Error::LockError("Failed to acquire mutex lock".into()))
        .unwrap();

    let toml_file = reqfile_abspath.with_extension("toml");
    let result = request::parse_req(&toml_file, tmp_space_abspath, &sbf_store_mtx);
    assert!(result.is_some());

    let http_req = result.unwrap();
    assert_eq!(http_req.meta.name, "Parent Req 1");
    assert_eq!(http_req.meta.fsname, "parent-req-1.toml");
    assert!(!http_req.meta.has_unsaved_changes);
}

#[test]
fn parse_req_returns_none_for_invalid_toml() {
    let (_tmp_datadir, tmp_spacedir, _state_store) = store::utils::temp_space("Test Space");
    let tmp_space_abspath = tmp_spacedir.path();

    let reqfile_abspath = tmp_space_abspath.join("invalid-req-1.toml");
    let invalid_toml = "[meta\nname = \"Invalid Req 1\"";
    fs::write(&reqfile_abspath, invalid_toml).unwrap();

    let sbf_store_abspath = store::utils::sbf_store_abspath(tmp_space_abspath);
    let sbf_store = SpaceBufferStore::get(&sbf_store_abspath).unwrap();
    let sbf_store_mtx = sbf_store
        .lock()
        .map_err(|_| Error::LockError("Failed to acquire mutex lock".into()))
        .unwrap();

    let result = request::parse_req(&reqfile_abspath, tmp_space_abspath, &sbf_store_mtx);
    assert!(result.is_none());
}

#[test]
fn parse_req_returns_buffered_request_when_available() {
    let (_tmp_datadir, tmp_spacedir, _state_store) = store::utils::temp_space("Test Space");
    let tmp_space_abspath = tmp_spacedir.path();

    let reqfile_abspath = tmp_space_abspath.join("buffered-req-1.toml");
    request::create_reqtoml(&reqfile_abspath.with_extension(""), "Buffered Req 1").unwrap();

    let sbf_store_abspath = store::utils::sbf_store_abspath(tmp_space_abspath);
    let sbf_store = SpaceBufferStore::get(&sbf_store_abspath).unwrap();
    {
        let mut sbf_store_mtx = sbf_store
            .lock()
            .map_err(|_| Error::LockError("Failed to acquire mutex lock".into()))
            .unwrap();

        let req_buf = ReqBuffer {
            meta: ReqBufferMeta {
                fsname: "buffered-req-1.toml".to_string(),
                name: "Modified Buffered Req 1".to_string(),
            },
            config: ReqBufferCfg {
                method: "POST".to_string(),
                url: ReqBufferUrl {
                    raw: Some("https://zaku.app/buffered-req-1".to_string()),
                    protocol: Some("https".to_string()),
                    host: Some("zaku.app".to_string()),
                    path: Some("/buffered-req-1".to_string()),
                },
                headers: Vec::new(),
                parameters: Vec::new(),
                content_type: None,
                body: None,
            },
        };

        sbf_store_mtx
            .requests
            .insert(PathBuf::from("buffered-req-1.toml"), req_buf);
    }

    let sbf_store_mtx = sbf_store
        .lock()
        .map_err(|_| Error::LockError("Failed to acquire mutex lock".into()))
        .unwrap();

    let result = request::parse_req(&reqfile_abspath, tmp_space_abspath, &sbf_store_mtx);
    assert!(result.is_some());

    let http_req = result.unwrap();
    assert_eq!(http_req.meta.name, "Modified Buffered Req 1");
    assert_eq!(http_req.config.method, "POST");
    assert!(http_req.meta.has_unsaved_changes);
}

#[test]
fn create_req_basic() {
    let (_tmp_datadir, _tmp_spacedir, state_store) = store::utils::temp_space("Req Space");
    let tmp_space_abspath = state_store.spaceref.as_ref().unwrap().abspath.clone();

    fs::create_dir_all(tmp_space_abspath.join("parent-col-1")).unwrap();

    let req_segment = SanitizedSegment {
        name: "Child Req 1".to_string(),
        fsname: "child-req-1".to_string(),
    };

    let result = request::create_req(
        &PathBuf::from("parent-col-1"),
        &req_segment,
        &tmp_space_abspath,
    )
    .expect("Failed to create request");

    assert_eq!(
        result.relpath,
        PathBuf::from("parent-col-1").join("child-req-1.toml")
    );

    let expected_reqfile_abspath = tmp_space_abspath
        .join("parent-col-1")
        .join("child-req-1.toml");
    assert!(expected_reqfile_abspath.exists());

    let req_toml = request::parse_reqtoml(&expected_reqfile_abspath).unwrap();
    assert_eq!(req_toml.meta.name, "Child Req 1");
    assert_eq!(req_toml.config.method, "GET");
}

#[test]
fn create_req_with_nested_collections() {
    let (_tmp_datadir, _tmp_spacedir, state_store) = store::utils::temp_space("Req Space");
    let tmp_space_abspath = state_store.spaceref.as_ref().unwrap().abspath.clone();

    let location_relpath = PathBuf::from("");
    let (location_relpath, req_segment) = collection::create_parent_collections_if_missing(
        &location_relpath,
        &PathBuf::from("Grand Parent Col 1/Parent Col 1/Child Req 1"),
        &tmp_space_abspath,
    )
    .expect("Failed to create parent collections");

    let result = request::create_req(&location_relpath, &req_segment, &tmp_space_abspath)
        .expect("Failed to create request with nested collections");

    assert_eq!(
        result.relpath,
        PathBuf::from("grand-parent-col-1")
            .join("parent-col-1")
            .join("child-req-1.toml")
    );
    assert!(
        tmp_space_abspath
            .join("grand-parent-col-1")
            .join("parent-col-1")
            .join("child-req-1.toml")
            .exists()
    );
    assert!(tmp_space_abspath.join("grand-parent-col-1").exists());
    assert!(
        tmp_space_abspath
            .join("grand-parent-col-1")
            .join("parent-col-1")
            .exists()
    );
}

#[test]
fn create_req_with_nested_collections_and_backslash() {
    let (_tmp_datadir, _tmp_spacedir, state_store) = store::utils::temp_space("Req Space");
    let tmp_space_abspath = state_store.spaceref.as_ref().unwrap().abspath.clone();

    let location_relpath = PathBuf::from("");
    let (location_relpath, req_segment) = collection::create_parent_collections_if_missing(
        &location_relpath,
        &PathBuf::from("Grand Parent Col 1\\Parent Col 1\\Child Req 1"),
        &tmp_space_abspath,
    )
    .expect("Failed to create parent collections");

    let result = request::create_req(&location_relpath, &req_segment, &tmp_space_abspath)
        .expect("Failed to create request with nested collections");

    assert_eq!(
        result.relpath,
        PathBuf::from("grand-parent-col-1-parent-col-1-child-req-1.toml")
    );
    assert!(
        tmp_space_abspath
            .join("grand-parent-col-1-parent-col-1-child-req-1.toml")
            .exists()
    );
}

#[test]
fn create_req_empty_fsname_should_fail() {
    let (_tmp_datadir, _tmp_spacedir, state_store) = store::utils::temp_space("Req Space");
    let tmp_space_abspath = state_store.spaceref.as_ref().unwrap().abspath.clone();

    let req_segment = SanitizedSegment {
        name: "Empty Req 1".to_string(),
        fsname: "   ".to_string(),
    };

    let result = request::create_req(
        &PathBuf::from("parent-col-1"),
        &req_segment,
        &tmp_space_abspath,
    );
    assert!(matches!(result, Err(Error::InvalidName(_))));
}

#[test]
fn create_req_missing_space_should_fail() {
    let req_segment = SanitizedSegment {
        name: "Child Req 1".to_string(),
        fsname: "child-req-1".to_string(),
    };

    let (_tmp_datadir, tmp_spacedir, _state_store) = store::utils::temp_space("Test Space");
    let tmp_space_abspath = tmp_spacedir.path();
    let result = request::create_req(
        &PathBuf::from("parent-col-1"),
        &req_segment,
        tmp_space_abspath,
    );
    assert!(matches!(result, Err(Error::Io(_))));
}

#[test]
fn create_req_sanitizes_filename() {
    let (_tmp_datadir, _tmp_spacedir, state_store) = store::utils::temp_space("Req Space");
    let tmp_space_abspath = state_store.spaceref.as_ref().unwrap().abspath.clone();

    fs::create_dir_all(tmp_space_abspath.join("parent-col-1")).unwrap();

    let req_segment = SanitizedSegment {
        name: "Special*Chars<>|Req 1".to_string(),
        fsname: "special-chars-req-1".to_string(),
    };

    let result = request::create_req(
        &PathBuf::from("parent-col-1"),
        &req_segment,
        &tmp_space_abspath,
    )
    .expect("Failed to create request with special characters");

    assert_eq!(
        result.relpath,
        PathBuf::from("parent-col-1").join("special-chars-req-1.toml")
    );

    let expected_reqfile_abspath = tmp_space_abspath
        .join("parent-col-1")
        .join("special-chars-req-1.toml");
    assert!(expected_reqfile_abspath.exists());

    let req_toml = request::parse_reqtoml(&expected_reqfile_abspath).unwrap();
    assert_eq!(req_toml.meta.name, "Special*Chars<>|Req 1");
}

#[test]
fn create_req_with_unicode_characters() {
    let (_tmp_datadir, _tmp_spacedir, state_store) = store::utils::temp_space("Req Space");
    let tmp_space_abspath = state_store.spaceref.as_ref().unwrap().abspath.clone();

    fs::create_dir_all(tmp_space_abspath.join("parent-col-1")).unwrap();

    let req_segment = SanitizedSegment {
        name: "ザク Unicode Req 1".to_string(),
        fsname: "ザク-unicode-req-1".to_string(),
    };

    let result = request::create_req(
        &PathBuf::from("parent-col-1"),
        &req_segment,
        &tmp_space_abspath,
    )
    .expect("Failed to create request with unicode characters");

    assert_eq!(
        result.relpath,
        PathBuf::from("parent-col-1").join("ザク-unicode-req-1.toml")
    );

    let expected_reqfile_abspath = tmp_space_abspath
        .join("parent-col-1")
        .join("ザク-unicode-req-1.toml");
    assert!(expected_reqfile_abspath.exists());

    let req_toml = request::parse_reqtoml(&expected_reqfile_abspath).unwrap();
    assert_eq!(req_toml.meta.name, "ザク Unicode Req 1");
}

#[test]
fn create_req_with_whitespace_handling() {
    let (_tmp_datadir, _tmp_spacedir, state_store) = store::utils::temp_space("Req Space");
    let tmp_space_abspath = state_store.spaceref.as_ref().unwrap().abspath.clone();

    fs::create_dir_all(tmp_space_abspath.join("parent-col-1")).unwrap();

    let req_segment = SanitizedSegment {
        name: "  Multiple   Spaces  Req 1  ".to_string(),
        fsname: "multiple-spaces-req-1".to_string(),
    };

    let result = request::create_req(
        &PathBuf::from("parent-col-1"),
        &req_segment,
        &tmp_space_abspath,
    )
    .expect("Failed to create request with whitespace");

    assert_eq!(
        result.relpath,
        PathBuf::from("parent-col-1").join("multiple-spaces-req-1.toml")
    );
    assert!(
        tmp_space_abspath
            .join("parent-col-1")
            .join("multiple-spaces-req-1.toml")
            .exists()
    );
}

#[test]
fn create_req_duplicate_name_should_fail() {
    let (_tmp_datadir, _tmp_spacedir, state_store) = store::utils::temp_space("Req Space");
    let tmp_space_abspath = state_store.spaceref.as_ref().unwrap().abspath.clone();

    fs::create_dir_all(tmp_space_abspath.join("parent-col-1")).unwrap();

    let req_segment = SanitizedSegment {
        name: "Duplicate Req 1".to_string(),
        fsname: "duplicate-req-1".to_string(),
    };

    request::create_req(
        &PathBuf::from("parent-col-1"),
        &req_segment,
        &tmp_space_abspath,
    )
    .expect("Failed to create first request");

    let result = request::create_req(
        &PathBuf::from("parent-col-1"),
        &req_segment,
        &tmp_space_abspath,
    );

    assert!(result.is_err());
}

#[test]
fn create_reqtoml_creates_valid_file() {
    let (_tmp_datadir, tmp_spacedir, _state_store) = store::utils::temp_space("Test Space");
    let reqfile_abspath = tmp_spacedir.path().join("child-req-1");

    request::create_reqtoml(&reqfile_abspath, "Child Req 1").expect("Failed to create TOML file");

    let toml_file = reqfile_abspath.with_extension("toml");
    assert!(toml_file.exists());

    let req_toml = request::parse_reqtoml(&toml_file).unwrap();
    assert_eq!(req_toml.meta.name, "Child Req 1");
    assert_eq!(req_toml.config.method, "GET");
    assert!(req_toml.config.url.is_none());
    assert!(req_toml.config.headers.is_none());
    assert!(req_toml.config.parameters.is_none());
    assert!(req_toml.config.content_type.is_none());
    assert!(req_toml.config.body.is_none());
}

#[test]
fn create_reqtoml_skips_serializing_empty_optional_fields() {
    let (_tmp_datadir, tmp_spacedir, _state_store) = store::utils::temp_space("Test Space");
    let reqfile_abspath = tmp_spacedir.path().join("empty-fields-req-1");

    request::create_reqtoml(&reqfile_abspath, "Empty Fields Req 1")
        .expect("Failed to create TOML file");

    let toml_file = reqfile_abspath.with_extension("toml");
    let toml_content = fs::read_to_string(&toml_file).unwrap();

    assert!(!toml_content.contains("url ="));
    assert!(!toml_content.contains("headers ="));
    assert!(!toml_content.contains("parameters ="));
    assert!(!toml_content.contains("content_type ="));
    assert!(!toml_content.contains("body ="));
}

#[test]
fn create_reqtoml_with_special_characters_in_name() {
    let (_tmp_datadir, tmp_spacedir, _state_store) = store::utils::temp_space("Test Space");
    let reqfile_abspath = tmp_spacedir.path().join("special-chars-req-1");

    request::create_reqtoml(&reqfile_abspath, "Special*Chars<>|Req 1")
        .expect("Failed to create TOML file");

    let toml_file = reqfile_abspath.with_extension("toml");
    let req_toml = request::parse_reqtoml(&toml_file).unwrap();
    assert_eq!(req_toml.meta.name, "Special*Chars<>|Req 1");
}

#[test]
fn create_reqtoml_with_unicode_name() {
    let (_tmp_datadir, tmp_spacedir, _state_store) = store::utils::temp_space("Test Space");
    let reqfile_abspath = tmp_spacedir.path().join("unicode-req-1");

    request::create_reqtoml(&reqfile_abspath, "ザク Unicode Req 1")
        .expect("Failed to create TOML file");

    let toml_file = reqfile_abspath.with_extension("toml");
    let req_toml = request::parse_reqtoml(&toml_file).unwrap();
    assert_eq!(req_toml.meta.name, "ザク Unicode Req 1");
}

#[test]
fn parse_reqtoml_successfully_parses_valid_file() {
    let (_tmp_datadir, tmp_spacedir, _state_store) = store::utils::temp_space("Test Space");
    let reqfile_abspath = tmp_spacedir.path().join("parent-req-1");

    request::create_reqtoml(&reqfile_abspath, "Parent Req 1").expect("Failed to create TOML");

    let toml_file = reqfile_abspath.with_extension("toml");
    let result = request::parse_reqtoml(&toml_file).expect("Failed to parse TOML");

    assert_eq!(result.meta.name, "Parent Req 1");
    assert_eq!(result.config.method, "GET");
}

#[test]
fn parse_reqtoml_with_custom_config() {
    let (_tmp_datadir, tmp_spacedir, _state_store) = store::utils::temp_space("Test Space");
    let reqfile_abspath = tmp_spacedir.path().join("custom-req-1");

    request::create_reqtoml(&reqfile_abspath, "Custom Req 1").expect("Failed to create TOML");

    let toml_file = reqfile_abspath.with_extension("toml");

    let mut custom_req = request::parse_reqtoml(&toml_file).unwrap();
    custom_req.config.method = "POST".to_string();
    custom_req.config.url = Some("https://zaku.app/custom-req-1".to_string());
    custom_req.config.content_type = Some("application/json".to_string());
    custom_req.config.body = Some(r#"{"name": "Custom Req 1"}"#.to_string());

    request::update_reqtoml(&toml_file, &custom_req).expect("Failed to update TOML");

    let result = request::parse_reqtoml(&toml_file).expect("Failed to parse updated TOML");
    assert_eq!(result.config.method, "POST");
    assert_eq!(
        result.config.url,
        Some("https://zaku.app/custom-req-1".to_string())
    );
    assert_eq!(
        result.config.content_type,
        Some("application/json".to_string())
    );
    assert_eq!(
        result.config.body,
        Some(r#"{"name": "Custom Req 1"}"#.to_string())
    );
}

#[test]
fn parse_reqtoml_fails_for_invalid_toml() {
    let (_tmp_datadir, tmp_spacedir, _state_store) = store::utils::temp_space("Test Space");
    let reqfile_abspath = tmp_spacedir.path().join("invalid-req-1.toml");

    let invalid_toml = "[meta\nname = \"Invalid Req 1\"";
    fs::write(&reqfile_abspath, invalid_toml).unwrap();

    let result = request::parse_reqtoml(&reqfile_abspath);
    assert!(result.is_err());
}

#[test]
fn parse_reqtoml_fails_for_nonexistent_file() {
    let (_tmp_datadir, tmp_spacedir, _state_store) = store::utils::temp_space("Test Space");
    let nonexistent_reqfile_abspath = tmp_spacedir.path().join("nonexistent-req-1.toml");

    let result = request::parse_reqtoml(&nonexistent_reqfile_abspath);
    assert!(result.is_err());
}

#[test]
fn update_reqtoml_successfully_updates_existing_file() {
    let (_tmp_datadir, tmp_spacedir, _state_store) = store::utils::temp_space("Test Space");
    let reqfile_abspath = tmp_spacedir.path().join("update-req-1");

    request::create_reqtoml(&reqfile_abspath, "Update Req 1").unwrap();
    let toml_file = reqfile_abspath.with_extension("toml");

    let updated_req = ReqToml {
        meta: ReqTomlMeta {
            name: "Updated Req 1".to_string(),
        },
        config: ReqTomlConfig {
            method: "PATCH".to_string(),
            url: Some("https://zaku.app/updated-req-1".to_string()),
            headers: None,
            parameters: None,
            content_type: Some("application/json".to_string()),
            body: Some(r#"{"updated": true}"#.to_string()),
        },
    };

    request::update_reqtoml(&toml_file, &updated_req).expect("Failed to update TOML");

    let result = request::parse_reqtoml(&toml_file).unwrap();
    assert_eq!(result.meta.name, "Updated Req 1");
    assert_eq!(result.config.method, "PATCH");
    assert_eq!(
        result.config.url,
        Some("https://zaku.app/updated-req-1".to_string())
    );
    assert_eq!(result.config.body, Some(r#"{"updated": true}"#.to_string()));
}

#[test]
fn update_reqtoml_fails_for_nonexistent_file() {
    let (_tmp_datadir, tmp_spacedir, _state_store) = store::utils::temp_space("Test Space");
    let nonexistent_reqfile_abspath = tmp_spacedir.path().join("nonexistent-req-1.toml");

    let req_toml = ReqToml {
        meta: ReqTomlMeta {
            name: "Nonexistent Req 1".to_string(),
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

    let result = request::update_reqtoml(&nonexistent_reqfile_abspath, &req_toml);
    assert!(matches!(result, Err(Error::FileNotFound(_))));
}

#[test]
fn update_reqtoml_with_headers_and_parameters() {
    let (_tmp_datadir, tmp_spacedir, _state_store) = store::utils::temp_space("Test Space");
    let reqfile_abspath = tmp_spacedir.path().join("headers-req-1");

    request::create_reqtoml(&reqfile_abspath, "Headers Req 1").unwrap();
    let toml_file = reqfile_abspath.with_extension("toml");

    let mut headers = indexmap::IndexMap::new();
    headers.insert("Authorization".to_string(), "Bearer token123".to_string());
    headers.insert("Content-Type".to_string(), "application/json".to_string());

    let mut parameters = indexmap::IndexMap::new();
    parameters.insert("page".to_string(), "1".to_string());
    parameters.insert("limit".to_string(), "10".to_string());

    let updated_req = ReqToml {
        meta: ReqTomlMeta {
            name: "Headers Req 1".to_string(),
        },
        config: ReqTomlConfig {
            method: "GET".to_string(),
            url: Some("https://zaku.app/headers-req-1".to_string()),
            headers: Some(headers),
            parameters: Some(parameters),
            content_type: Some("application/json".to_string()),
            body: None,
        },
    };

    request::update_reqtoml(&toml_file, &updated_req).expect("Failed to update TOML");

    let result = request::parse_reqtoml(&toml_file).unwrap();
    assert!(result.config.headers.is_some());
    assert!(result.config.parameters.is_some());

    let headers = result.config.headers.unwrap();
    assert_eq!(
        headers.get("Authorization"),
        Some(&"Bearer token123".to_string())
    );

    let params = result.config.parameters.unwrap();
    assert_eq!(params.get("page"), Some(&"1".to_string()));
}

#[test]
fn update_reqtoml_skips_serializing_empty_fields() {
    let (_tmp_datadir, tmp_spacedir, _state_store) = store::utils::temp_space("Test Space");
    let reqfile_abspath = tmp_spacedir.path().join("empty-update-req-1");

    request::create_reqtoml(&reqfile_abspath, "Empty Update Req 1").unwrap();
    let toml_file = reqfile_abspath.with_extension("toml");

    let updated_req = ReqToml {
        meta: ReqTomlMeta {
            name: "Empty Update Req 1".to_string(),
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

    request::update_reqtoml(&toml_file, &updated_req).expect("Failed to update TOML");

    let toml_content = fs::read_to_string(&toml_file).unwrap();
    assert!(!toml_content.contains("url ="));
    assert!(!toml_content.contains("headers ="));
    assert!(!toml_content.contains("parameters ="));
    assert!(!toml_content.contains("content_type ="));
    assert!(!toml_content.contains("body ="));
}

#[test]
fn http_req_from_reqtoml_parses_url_correctly() {
    let req_toml = ReqToml {
        meta: ReqTomlMeta {
            name: "URL Parse Req 1".to_string(),
        },
        config: ReqTomlConfig {
            method: "GET".to_string(),
            url: Some("https://zaku.app/url-parse-req-1?param=value".to_string()),
            headers: None,
            parameters: None,
            content_type: None,
            body: None,
        },
    };

    let http_req = HttpReq::from_reqtoml(&req_toml, &PathBuf::from("url-parse-req-1.toml"));

    assert_eq!(
        http_req.config.url.raw,
        Some("https://zaku.app/url-parse-req-1?param=value".to_string())
    );
    assert_eq!(http_req.config.url.protocol, Some("https".to_string()));
    assert_eq!(http_req.config.url.host, Some("zaku.app".to_string()));
    assert_eq!(
        http_req.config.url.path,
        Some("/url-parse-req-1".to_string())
    );
}

#[test]
fn http_req_from_reqtoml_handles_invalid_url() {
    let req_toml = ReqToml {
        meta: ReqTomlMeta {
            name: "Invalid URL Req 1".to_string(),
        },
        config: ReqTomlConfig {
            method: "GET".to_string(),
            url: Some("not-a-valid-url".to_string()),
            headers: None,
            parameters: None,
            content_type: None,
            body: None,
        },
    };

    let http_req = HttpReq::from_reqtoml(&req_toml, &PathBuf::from("invalid-url-req-1.toml"));

    assert_eq!(http_req.config.url.raw, Some("not-a-valid-url".to_string()));
    assert!(http_req.config.url.protocol.is_none());
    assert!(http_req.config.url.host.is_none());
    assert!(http_req.config.url.path.is_none());
}

#[test]
fn http_req_from_reqbuf_has_unsaved_changes() {
    let req_buf = ReqBuffer {
        meta: ReqBufferMeta {
            fsname: "buffer-req-1.toml".to_string(),
            name: "Buffer Req 1".to_string(),
        },
        config: ReqBufferCfg {
            method: "POST".to_string(),
            url: ReqBufferUrl {
                raw: Some("https://zaku.app/buffer-req-1".to_string()),
                protocol: Some("https".to_string()),
                host: Some("zaku.app".to_string()),
                path: Some("/buffer-req-1".to_string()),
            },
            headers: Vec::new(),
            parameters: Vec::new(),
            content_type: None,
            body: None,
        },
    };

    let http_req = HttpReq::from_reqbuf(&req_buf, &PathBuf::from("buffer-req-1.toml"));

    assert!(http_req.meta.has_unsaved_changes);
    assert_eq!(http_req.meta.name, "Buffer Req 1");
    assert_eq!(http_req.config.method, "POST");
}

#[test]
fn create_req_creates_parent_collections_with_proper_hierarchy() {
    let (_tmp_datadir, _tmp_spacedir, state_store) = store::utils::temp_space("Req Space");
    let tmp_space_abspath = state_store.spaceref.as_ref().unwrap().abspath.clone();

    let location_relpath = PathBuf::from("");
    let (location_relpath, req_segment) = collection::create_parent_collections_if_missing(
        &location_relpath,
        &PathBuf::from("Great Grand Parent Col 1/Grand Parent Col 1/Parent Col 1/Child Req 1"),
        &tmp_space_abspath,
    )
    .expect("Failed to create parent collections");

    let result = request::create_req(&location_relpath, &req_segment, &tmp_space_abspath)
        .expect("Failed to create request with deep hierarchy");

    assert_eq!(
        result.relpath,
        PathBuf::from("great-grand-parent-col-1")
            .join("grand-parent-col-1")
            .join("parent-col-1")
            .join("child-req-1.toml")
    );

    assert!(tmp_space_abspath.join("great-grand-parent-col-1").exists());
    assert!(
        tmp_space_abspath
            .join("great-grand-parent-col-1")
            .join("grand-parent-col-1")
            .exists()
    );
    assert!(
        tmp_space_abspath
            .join("great-grand-parent-col-1")
            .join("grand-parent-col-1")
            .join("parent-col-1")
            .exists()
    );
    assert!(
        tmp_space_abspath
            .join("great-grand-parent-col-1")
            .join("grand-parent-col-1")
            .join("parent-col-1")
            .join("child-req-1.toml")
            .exists()
    );
}

#[test]
fn create_req_handles_mixed_invalid_characters_and_unicode() {
    let (_tmp_datadir, _tmp_spacedir, state_store) = store::utils::temp_space("Req Space");
    let tmp_space_abspath = state_store.spaceref.as_ref().unwrap().abspath.clone();
    fs::create_dir_all(tmp_space_abspath.join("parent-col-1")).unwrap();

    let req_segment = SanitizedSegment {
        name: "ザク*Special<>|Chars:Req\\1".to_string(),
        fsname: "ザク-special-chars-req-1".to_string(),
    };

    let result = request::create_req(
        &PathBuf::from("parent-col-1"),
        &req_segment,
        &tmp_space_abspath,
    )
    .expect("Failed to create request with mixed characters");

    assert_eq!(
        result.relpath,
        PathBuf::from("parent-col-1/ザク-special-chars-req-1.toml")
    );

    let expected_reqfile_abspath = tmp_space_abspath
        .join("parent-col-1")
        .join("ザク-special-chars-req-1.toml");
    assert!(expected_reqfile_abspath.exists());

    let req_toml = request::parse_reqtoml(&expected_reqfile_abspath).unwrap();
    assert_eq!(req_toml.meta.name, "ザク*Special<>|Chars:Req\\1");
}

#[test]
fn create_req_with_trailing_slash_in_relpath() {
    let (_tmp_datadir, _tmp_spacedir, state_store) = store::utils::temp_space("Req Space");
    let tmp_space_abspath = state_store.spaceref.as_ref().unwrap().abspath.clone();

    let location_relpath = PathBuf::from("");
    let (location_relpath, req_segment) = collection::create_parent_collections_if_missing(
        &location_relpath,
        &PathBuf::from("Parent Col 1/Child Col 1/Trailing Req 1"),
        &tmp_space_abspath,
    )
    .expect("Failed to create parent collections");

    let result = request::create_req(&location_relpath, &req_segment, &tmp_space_abspath)
        .expect("Failed to create request with trailing slash");

    assert_eq!(
        result.relpath,
        PathBuf::from("parent-col-1")
            .join("child-col-1")
            .join("trailing-req-1.toml")
    );
}

#[test]
fn create_req_updates_shared_state() {
    let (_tmp_datadir, _tmp_spacedir, state_store) = store::utils::temp_space("Req Space");
    let tmp_space_abspath = state_store.spaceref.as_ref().unwrap().abspath.clone();

    fs::create_dir_all(tmp_space_abspath.join("parent-col-1")).unwrap();

    let req_segment = SanitizedSegment {
        name: "State Update Req 1".to_string(),
        fsname: "state-update-req-1".to_string(),
    };

    request::create_req(
        &PathBuf::from("parent-col-1"),
        &req_segment,
        &tmp_space_abspath,
    )
    .expect("Failed to create request");

    assert!(
        tmp_space_abspath
            .join("parent-col-1")
            .join("state-update-req-1.toml")
            .exists()
    );
}

#[test]
fn create_req_with_backslash_characters() {
    let (_tmp_datadir, _tmp_spacedir, state_store) = store::utils::temp_space("Req Space");
    let tmp_space_abspath = state_store.spaceref.as_ref().unwrap().abspath.clone();

    fs::create_dir_all(tmp_space_abspath.join("parent-col-1")).unwrap();

    let req_segment = SanitizedSegment {
        name: "Back\\Slash Req 1".to_string(),
        fsname: "back-slash-req-1".to_string(),
    };

    let result = request::create_req(
        &PathBuf::from("parent-col-1"),
        &req_segment,
        &tmp_space_abspath,
    )
    .expect("Failed to create request with backslash");

    assert_eq!(
        result.relpath,
        PathBuf::from("parent-col-1/back-slash-req-1.toml")
    );

    let expected_reqfile_abspath = tmp_space_abspath
        .join("parent-col-1")
        .join("back-slash-req-1.toml");
    assert!(expected_reqfile_abspath.exists());

    let req_toml = request::parse_reqtoml(&expected_reqfile_abspath).unwrap();
    assert_eq!(req_toml.meta.name, "Back\\Slash Req 1");
}

#[test]
fn create_req_with_multiple_slashes_in_relpath() {
    let (_tmp_datadir, _tmp_spacedir, state_store) = store::utils::temp_space("Req Space");
    let tmp_space_abspath = state_store.spaceref.as_ref().unwrap().abspath.clone();

    let location_relpath = PathBuf::from("");
    let (location_relpath, req_segment) = collection::create_parent_collections_if_missing(
        &location_relpath,
        &PathBuf::from("Parent Col 1/Child Col 1/Multiple Slash Req 1"),
        &tmp_space_abspath,
    )
    .expect("Failed to create parent collections");

    let result = request::create_req(&location_relpath, &req_segment, &tmp_space_abspath)
        .expect("Failed to create request with multiple slashes");

    assert_eq!(
        result.relpath,
        PathBuf::from("parent-col-1")
            .join("child-col-1")
            .join("multiple-slash-req-1.toml")
    );
}

#[test]
fn create_req_integrated_flow() {
    let (_tmp_datadir, _tmp_spacedir, state_store) = store::utils::temp_space("Req Space");
    let tmp_space_abspath = state_store.spaceref.as_ref().unwrap().abspath.clone();

    let location_relpath = PathBuf::from("");
    let (location_relpath, req_segment) = collection::create_parent_collections_if_missing(
        &location_relpath,
        &PathBuf::from("Parent Col 1/Child Col 1/Grand Child Req 1"),
        &tmp_space_abspath,
    )
    .expect("Failed to create parent collections");

    let result = request::create_req(&location_relpath, &req_segment, &tmp_space_abspath)
        .expect("Failed to create request");

    assert_eq!(
        result.relpath,
        PathBuf::from("parent-col-1")
            .join("child-col-1")
            .join("grand-child-req-1.toml")
    );

    assert!(tmp_space_abspath.join("parent-col-1").exists());
    assert!(
        tmp_space_abspath
            .join("parent-col-1")
            .join("child-col-1")
            .exists()
    );
    assert!(
        tmp_space_abspath
            .join("parent-col-1")
            .join("child-col-1")
            .join("grand-child-req-1.toml")
            .exists()
    );

    let req_toml_path = tmp_space_abspath.join("parent-col-1/child-col-1/grand-child-req-1.toml");
    let req_toml = request::parse_reqtoml(&req_toml_path).unwrap();
    assert_eq!(req_toml.meta.name, "Grand Child Req 1");
}
