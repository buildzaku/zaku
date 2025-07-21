use std::{
    fs,
    path::{Path, PathBuf},
};
use tempfile;

use crate::{
    collection,
    error::Error,
    models::SanitizedSegment,
    request::{
        self,
        models::{HttpReq, ReqCfg, ReqMeta, ReqToml, ReqTomlConfig, ReqTomlMeta, ReqUrl},
    },
    space::{self, models::CreateSpaceDto},
    state::SharedState,
    store::{models::ReqBuf, spaces::buffer::SpaceBuf},
};

fn tmp_space_sharedstate(tmp_path: &Path) -> SharedState {
    let dto = CreateSpaceDto {
        name: "Req Space".to_string(),
        location: tmp_path.to_string_lossy().to_string(),
    };

    let mut sharedstate = SharedState::default();
    space::create_space(dto, &mut sharedstate).expect("Failed to create test space");

    sharedstate
}

#[test]
fn parse_req_returns_none_for_non_toml_file() {
    let tmp_dir = tempfile::tempdir().unwrap();
    let space_abspath = tmp_dir.path();

    let txt_file_abspath = space_abspath.join("parent-req-1.txt");
    fs::write(&txt_file_abspath, "not a toml file").unwrap();

    let space_buffer = SpaceBuf::load(space_abspath).unwrap();
    let spacebuf_rlock = space_buffer
        .read()
        .map_err(|_| Error::LockError("Failed to acquire read lock".into()))
        .unwrap();

    let result = request::parse_req(&txt_file_abspath, space_abspath, &spacebuf_rlock);
    assert!(result.is_none());
}

#[test]
fn parse_req_returns_none_for_directory() {
    let tmp_dir = tempfile::tempdir().unwrap();
    let space_abspath = tmp_dir.path();

    let dir_abspath = space_abspath.join("parent-col-1");
    fs::create_dir_all(&dir_abspath).unwrap();

    let space_buffer = SpaceBuf::load(space_abspath).unwrap();
    let spacebuf_rlock = space_buffer
        .read()
        .map_err(|_| Error::LockError("Failed to acquire read lock".into()))
        .unwrap();

    let result = request::parse_req(&dir_abspath, space_abspath, &spacebuf_rlock);
    assert!(result.is_none());
}

#[test]
fn parse_req_successfully_parses_valid_toml_file() {
    let tmp_dir = tempfile::tempdir().unwrap();
    let space_abspath = tmp_dir.path();

    let reqfile_abspath = space_abspath.join("parent-req-1");
    request::create_reqtoml(&reqfile_abspath, "Parent Req 1").unwrap();

    let space_buffer = SpaceBuf::load(space_abspath).unwrap();
    let spacebuf_rlock = space_buffer
        .read()
        .map_err(|_| Error::LockError("Failed to acquire read lock".into()))
        .unwrap();

    let toml_file = reqfile_abspath.with_extension("toml");
    let result = request::parse_req(&toml_file, space_abspath, &spacebuf_rlock);
    assert!(result.is_some());

    let http_req = result.unwrap();
    assert_eq!(http_req.meta.name, "Parent Req 1");
    assert_eq!(http_req.meta.fsname, "parent-req-1.toml");
    assert!(!http_req.meta.has_unsaved_changes);
}

#[test]
fn parse_req_returns_none_for_invalid_toml() {
    let tmp_dir = tempfile::tempdir().unwrap();
    let space_abspath = tmp_dir.path();

    let reqfile_abspath = space_abspath.join("invalid-req-1.toml");
    let invalid_toml = "[meta\nname = \"Invalid Req 1\"";
    fs::write(&reqfile_abspath, invalid_toml).unwrap();

    let space_buffer = SpaceBuf::load(space_abspath).unwrap();
    let spacebuf_rlock = space_buffer
        .read()
        .map_err(|_| Error::LockError("Failed to acquire read lock".into()))
        .unwrap();

    let result = request::parse_req(&reqfile_abspath, space_abspath, &spacebuf_rlock);
    assert!(result.is_none());
}

#[test]
fn parse_req_returns_buffered_request_when_available() {
    let tmp_dir = tempfile::tempdir().unwrap();
    let space_abspath = tmp_dir.path();

    let reqfile_abspath = space_abspath.join("buffered-req-1.toml");
    request::create_reqtoml(&reqfile_abspath.with_extension(""), "Buffered Req 1").unwrap();

    let space_buffer = SpaceBuf::load(space_abspath).unwrap();
    {
        let mut spacebuf_wlock = space_buffer
            .write()
            .map_err(|_| Error::LockError("Failed to acquire write lock".into()))
            .unwrap();

        let req_buf = ReqBuf {
            meta: ReqMeta {
                fsname: "buffered-req-1.toml".to_string(),
                name: "Modified Buffered Req 1".to_string(),
                has_unsaved_changes: true,
            },
            config: ReqCfg {
                method: "POST".to_string(),
                url: ReqUrl {
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

        spacebuf_wlock
            .requests
            .insert("buffered-req-1.toml".to_string(), req_buf);
    }

    let spacebuf_rlock = space_buffer
        .read()
        .map_err(|_| Error::LockError("Failed to acquire read lock".into()))
        .unwrap();

    let result = request::parse_req(&reqfile_abspath, space_abspath, &spacebuf_rlock);
    assert!(result.is_some());

    let http_req = result.unwrap();
    assert_eq!(http_req.meta.name, "Modified Buffered Req 1");
    assert_eq!(http_req.config.method, "POST");
    assert!(http_req.meta.has_unsaved_changes);
}

#[test]
fn create_req_basic() {
    let tmp_dir = tempfile::tempdir().unwrap();
    let mut sharedstate = tmp_space_sharedstate(tmp_dir.path());
    let space_abspath = PathBuf::from(&sharedstate.space.as_ref().unwrap().abspath);

    fs::create_dir_all(space_abspath.join("parent-col-1")).unwrap();

    let req_segment = SanitizedSegment {
        name: "Child Req 1".to_string(),
        fsname: "child-req-1".to_string(),
    };

    let result = request::create_req(Path::new("parent-col-1"), &req_segment, &mut sharedstate)
        .expect("Failed to create request");

    let expected_reqfile_relpath = PathBuf::from("parent-col-1").join("child-req-1.toml");
    assert_eq!(result.relpath, expected_reqfile_relpath.to_string_lossy());

    let expected_reqfile_abspath = space_abspath.join("parent-col-1/child-req-1.toml");
    assert!(expected_reqfile_abspath.exists());

    let req_toml = request::parse_reqtoml(&expected_reqfile_abspath).unwrap();
    assert_eq!(req_toml.meta.name, "Child Req 1");
    assert_eq!(req_toml.config.method, "GET");
}

#[test]
fn create_req_with_nested_collections() {
    let tmp_dir = tempfile::tempdir().unwrap();
    let mut sharedstate = tmp_space_sharedstate(tmp_dir.path());
    let space_abspath = PathBuf::from(&sharedstate.space.as_ref().unwrap().abspath);

    let (parent_relpath, req_segment) = collection::create_parent_collections_if_missing(
        Path::new(""),
        "Grand Parent Col 1/Parent Col 1/Child Req 1",
        &mut sharedstate,
    )
    .expect("Failed to create parent collections");

    let result = request::create_req(&parent_relpath, &req_segment, &mut sharedstate)
        .expect("Failed to create request with nested collections");

    let expected_reqfile_relpath = PathBuf::from("grand-parent-col-1")
        .join("parent-col-1")
        .join("child-req-1.toml");
    assert_eq!(result.relpath, expected_reqfile_relpath.to_string_lossy());

    let expected_reqfile_abspath =
        space_abspath.join("grand-parent-col-1/parent-col-1/child-req-1.toml");
    assert!(expected_reqfile_abspath.exists());

    assert!(space_abspath.join("grand-parent-col-1").exists());
    assert!(space_abspath
        .join("grand-parent-col-1/parent-col-1")
        .exists());
}

#[test]
fn create_req_empty_fsname_should_fail() {
    let tmp_dir = tempfile::tempdir().unwrap();
    let mut sharedstate = tmp_space_sharedstate(tmp_dir.path());

    let req_segment = SanitizedSegment {
        name: "Empty Req 1".to_string(),
        fsname: "   ".to_string(),
    };

    let result = request::create_req(Path::new("parent-col-1"), &req_segment, &mut sharedstate);
    assert!(matches!(result, Err(Error::FileNotFound(_))));
}

#[test]
fn create_req_missing_space_should_fail() {
    let req_segment = SanitizedSegment {
        name: "Child Req 1".to_string(),
        fsname: "child-req-1".to_string(),
    };

    let mut sharedstate = SharedState::default();
    let result = request::create_req(Path::new("parent-col-1"), &req_segment, &mut sharedstate);
    assert!(matches!(result, Err(Error::FileNotFound(_))));
}

#[test]
fn create_req_sanitizes_filename() {
    let tmp_dir = tempfile::tempdir().unwrap();
    let mut sharedstate = tmp_space_sharedstate(tmp_dir.path());
    let space_abspath = PathBuf::from(&sharedstate.space.as_ref().unwrap().abspath);

    fs::create_dir_all(space_abspath.join("parent-col-1")).unwrap();

    let req_segment = SanitizedSegment {
        name: "Special*Chars<>|Req 1".to_string(),
        fsname: "special-chars-req-1".to_string(),
    };

    let result = request::create_req(Path::new("parent-col-1"), &req_segment, &mut sharedstate)
        .expect("Failed to create request with special characters");

    let expected_reqfile_relpath = PathBuf::from("parent-col-1").join("special-chars-req-1.toml");
    assert_eq!(result.relpath, expected_reqfile_relpath.to_string_lossy());

    let expected_reqfile_abspath = space_abspath.join("parent-col-1/special-chars-req-1.toml");
    assert!(expected_reqfile_abspath.exists());

    let req_toml = request::parse_reqtoml(&expected_reqfile_abspath).unwrap();
    assert_eq!(req_toml.meta.name, "Special*Chars<>|Req 1");
}

#[test]
fn create_req_with_unicode_characters() {
    let tmp_dir = tempfile::tempdir().unwrap();
    let mut sharedstate = tmp_space_sharedstate(tmp_dir.path());
    let space_abspath = PathBuf::from(&sharedstate.space.as_ref().unwrap().abspath);

    fs::create_dir_all(space_abspath.join("parent-col-1")).unwrap();

    let req_segment = SanitizedSegment {
        name: "ザク Unicode Req 1".to_string(),
        fsname: "ザク-unicode-req-1".to_string(),
    };

    let result = request::create_req(Path::new("parent-col-1"), &req_segment, &mut sharedstate)
        .expect("Failed to create request with unicode characters");

    let expected_reqfile_relpath = PathBuf::from("parent-col-1").join("ザク-unicode-req-1.toml");
    assert_eq!(result.relpath, expected_reqfile_relpath.to_string_lossy());

    let expected_reqfile_abspath = space_abspath.join("parent-col-1/ザク-unicode-req-1.toml");
    assert!(expected_reqfile_abspath.exists());

    let req_toml = request::parse_reqtoml(&expected_reqfile_abspath).unwrap();
    assert_eq!(req_toml.meta.name, "ザク Unicode Req 1");
}

#[test]
fn create_req_with_whitespace_handling() {
    let tmp_dir = tempfile::tempdir().unwrap();
    let mut sharedstate = tmp_space_sharedstate(tmp_dir.path());
    let space_abspath = PathBuf::from(&sharedstate.space.as_ref().unwrap().abspath);

    fs::create_dir_all(space_abspath.join("parent-col-1")).unwrap();

    let req_segment = SanitizedSegment {
        name: "  Multiple   Spaces  Req 1  ".to_string(),
        fsname: "multiple-spaces-req-1".to_string(),
    };

    let result = request::create_req(Path::new("parent-col-1"), &req_segment, &mut sharedstate)
        .expect("Failed to create request with whitespace");

    let expected_reqfile_relpath = PathBuf::from("parent-col-1").join("multiple-spaces-req-1.toml");
    assert_eq!(result.relpath, expected_reqfile_relpath.to_string_lossy());

    let expected_reqfile_abspath = space_abspath.join("parent-col-1/multiple-spaces-req-1.toml");
    assert!(expected_reqfile_abspath.exists());
}

#[test]
fn create_req_duplicate_name_should_fail() {
    let tmp_dir = tempfile::tempdir().unwrap();
    let mut sharedstate = tmp_space_sharedstate(tmp_dir.path());
    let space_abspath = PathBuf::from(&sharedstate.space.as_ref().unwrap().abspath);

    fs::create_dir_all(space_abspath.join("parent-col-1")).unwrap();

    let req_segment = SanitizedSegment {
        name: "Duplicate Req 1".to_string(),
        fsname: "duplicate-req-1".to_string(),
    };

    request::create_req(Path::new("parent-col-1"), &req_segment, &mut sharedstate)
        .expect("Failed to create first request");

    let result = request::create_req(Path::new("parent-col-1"), &req_segment, &mut sharedstate);

    assert!(result.is_err());
}

#[test]
fn create_reqtoml_creates_valid_file() {
    let tmp_dir = tempfile::tempdir().unwrap();
    let reqfile_abspath = tmp_dir.path().join("child-req-1");

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
    let tmp_dir = tempfile::tempdir().unwrap();
    let reqfile_abspath = tmp_dir.path().join("empty-fields-req-1");

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
    let tmp_dir = tempfile::tempdir().unwrap();
    let reqfile_abspath = tmp_dir.path().join("special-chars-req-1");

    request::create_reqtoml(&reqfile_abspath, "Special*Chars<>|Req 1")
        .expect("Failed to create TOML file");

    let toml_file = reqfile_abspath.with_extension("toml");
    let req_toml = request::parse_reqtoml(&toml_file).unwrap();
    assert_eq!(req_toml.meta.name, "Special*Chars<>|Req 1");
}

#[test]
fn create_reqtoml_with_unicode_name() {
    let tmp_dir = tempfile::tempdir().unwrap();
    let reqfile_abspath = tmp_dir.path().join("unicode-req-1");

    request::create_reqtoml(&reqfile_abspath, "ザク Unicode Req 1")
        .expect("Failed to create TOML file");

    let toml_file = reqfile_abspath.with_extension("toml");
    let req_toml = request::parse_reqtoml(&toml_file).unwrap();
    assert_eq!(req_toml.meta.name, "ザク Unicode Req 1");
}

#[test]
fn parse_reqtoml_successfully_parses_valid_file() {
    let tmp_dir = tempfile::tempdir().unwrap();
    let reqfile_abspath = tmp_dir.path().join("parent-req-1");

    request::create_reqtoml(&reqfile_abspath, "Parent Req 1").expect("Failed to create TOML");

    let toml_file = reqfile_abspath.with_extension("toml");
    let result = request::parse_reqtoml(&toml_file).expect("Failed to parse TOML");

    assert_eq!(result.meta.name, "Parent Req 1");
    assert_eq!(result.config.method, "GET");
}

#[test]
fn parse_reqtoml_with_custom_config() {
    let tmp_dir = tempfile::tempdir().unwrap();
    let reqfile_abspath = tmp_dir.path().join("custom-req-1");

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
    let tmp_dir = tempfile::tempdir().unwrap();
    let reqfile_abspath = tmp_dir.path().join("invalid-req-1.toml");

    let invalid_toml = "[meta\nname = \"Invalid Req 1\"";
    fs::write(&reqfile_abspath, invalid_toml).unwrap();

    let result = request::parse_reqtoml(&reqfile_abspath);
    assert!(result.is_err());
}

#[test]
fn parse_reqtoml_fails_for_nonexistent_file() {
    let tmp_dir = tempfile::tempdir().unwrap();
    let nonexistent_reqfile_abspath = tmp_dir.path().join("nonexistent-req-1.toml");

    let result = request::parse_reqtoml(&nonexistent_reqfile_abspath);
    assert!(result.is_err());
}

#[test]
fn update_reqtoml_successfully_updates_existing_file() {
    let tmp_dir = tempfile::tempdir().unwrap();
    let reqfile_abspath = tmp_dir.path().join("update-req-1");

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
    let tmp_dir = tempfile::tempdir().unwrap();
    let nonexistent_reqfile_abspath = tmp_dir.path().join("nonexistent-req-1.toml");

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
    let tmp_dir = tempfile::tempdir().unwrap();
    let reqfile_abspath = tmp_dir.path().join("headers-req-1");

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
    let tmp_dir = tempfile::tempdir().unwrap();
    let reqfile_abspath = tmp_dir.path().join("empty-update-req-1");

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

    let http_req = HttpReq::from_reqtoml(&req_toml, "url-parse-req-1.toml".to_string());

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

    let http_req = HttpReq::from_reqtoml(&req_toml, "invalid-url-req-1.toml".to_string());

    assert_eq!(http_req.config.url.raw, Some("not-a-valid-url".to_string()));
    assert!(http_req.config.url.protocol.is_none());
    assert!(http_req.config.url.host.is_none());
    assert!(http_req.config.url.path.is_none());
}

#[test]
fn http_req_from_reqbuf_has_unsaved_changes() {
    let req_buf = ReqBuf {
        meta: ReqMeta {
            fsname: "buffer-req-1.toml".to_string(),
            name: "Buffer Req 1".to_string(),
            has_unsaved_changes: true,
        },
        config: ReqCfg {
            method: "POST".to_string(),
            url: ReqUrl {
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

    let http_req = HttpReq::from_reqbuf(&req_buf);

    assert!(http_req.meta.has_unsaved_changes);
    assert_eq!(http_req.meta.name, "Buffer Req 1");
    assert_eq!(http_req.config.method, "POST");
}

#[test]
fn create_req_creates_parent_collections_with_proper_hierarchy() {
    let tmp_dir = tempfile::tempdir().unwrap();
    let mut sharedstate = tmp_space_sharedstate(tmp_dir.path());
    let space_abspath = PathBuf::from(&sharedstate.space.as_ref().unwrap().abspath);

    let (parent_relpath, req_segment) = collection::create_parent_collections_if_missing(
        Path::new(""),
        "Great Grand Parent Col 1/Grand Parent Col 1/Parent Col 1/Child Req 1",
        &mut sharedstate,
    )
    .expect("Failed to create parent collections");

    let result = request::create_req(&parent_relpath, &req_segment, &mut sharedstate)
        .expect("Failed to create request with deep hierarchy");

    let expected_reqfile_relpath = PathBuf::from("great-grand-parent-col-1")
        .join("grand-parent-col-1")
        .join("parent-col-1")
        .join("child-req-1.toml");
    assert_eq!(result.relpath, expected_reqfile_relpath.to_string_lossy());

    assert!(space_abspath.join("great-grand-parent-col-1").exists());
    assert!(space_abspath
        .join("great-grand-parent-col-1/grand-parent-col-1")
        .exists());
    assert!(space_abspath
        .join("great-grand-parent-col-1/grand-parent-col-1/parent-col-1")
        .exists());
    assert!(space_abspath
        .join("great-grand-parent-col-1/grand-parent-col-1/parent-col-1/child-req-1.toml")
        .exists());
}

#[test]
fn create_req_handles_mixed_invalid_characters_and_unicode() {
    let tmp_dir = tempfile::tempdir().unwrap();
    let mut sharedstate = tmp_space_sharedstate(tmp_dir.path());
    let space_abspath = PathBuf::from(&sharedstate.space.as_ref().unwrap().abspath);

    fs::create_dir_all(space_abspath.join("parent-col-1")).unwrap();

    let req_segment = SanitizedSegment {
        name: "ザク*Special<>|Chars:Req\\1".to_string(),
        fsname: "ザク-special-chars-req-1".to_string(),
    };

    let result = request::create_req(Path::new("parent-col-1"), &req_segment, &mut sharedstate)
        .expect("Failed to create request with mixed characters");

    let expected_reqfile_relpath =
        PathBuf::from("parent-col-1").join("ザク-special-chars-req-1.toml");
    assert_eq!(result.relpath, expected_reqfile_relpath.to_string_lossy());

    let expected_reqfile_abspath = space_abspath.join("parent-col-1/ザク-special-chars-req-1.toml");
    assert!(expected_reqfile_abspath.exists());

    let req_toml = request::parse_reqtoml(&expected_reqfile_abspath).unwrap();
    assert_eq!(req_toml.meta.name, "ザク*Special<>|Chars:Req\\1");
}

#[test]
fn create_req_with_trailing_slash_in_relpath() {
    let tmp_dir = tempfile::tempdir().unwrap();
    let mut sharedstate = tmp_space_sharedstate(tmp_dir.path());

    let (parent_relpath, req_segment) = collection::create_parent_collections_if_missing(
        Path::new(""),
        "Parent Col 1/Child Col 1/Trailing Req 1/",
        &mut sharedstate,
    )
    .expect("Failed to create parent collections");

    let result = request::create_req(&parent_relpath, &req_segment, &mut sharedstate)
        .expect("Failed to create request with trailing slash");

    let expected_reqfile_relpath = PathBuf::from("parent-col-1")
        .join("child-col-1")
        .join("trailing-req-1.toml");
    assert_eq!(result.relpath, expected_reqfile_relpath.to_string_lossy());
}

#[test]
fn create_req_updates_shared_state() {
    let tmp_dir = tempfile::tempdir().unwrap();
    let mut sharedstate = tmp_space_sharedstate(tmp_dir.path());
    let space_abspath = PathBuf::from(&sharedstate.space.as_ref().unwrap().abspath);

    fs::create_dir_all(space_abspath.join("parent-col-1")).unwrap();

    let req_segment = SanitizedSegment {
        name: "State Update Req 1".to_string(),
        fsname: "state-update-req-1".to_string(),
    };

    request::create_req(Path::new("parent-col-1"), &req_segment, &mut sharedstate)
        .expect("Failed to create request");

    assert!(sharedstate.space.is_some());
}

#[test]
fn create_req_with_backslash_characters() {
    let tmp_dir = tempfile::tempdir().unwrap();
    let mut sharedstate = tmp_space_sharedstate(tmp_dir.path());
    let space_abspath = PathBuf::from(&sharedstate.space.as_ref().unwrap().abspath);

    fs::create_dir_all(space_abspath.join("parent-col-1")).unwrap();

    let req_segment = SanitizedSegment {
        name: "Back\\Slash Req 1".to_string(),
        fsname: "back-slash-req-1".to_string(),
    };

    let result = request::create_req(Path::new("parent-col-1"), &req_segment, &mut sharedstate)
        .expect("Failed to create request with backslash");

    let expected_reqfile_relpath = PathBuf::from("parent-col-1").join("back-slash-req-1.toml");
    assert_eq!(result.relpath, expected_reqfile_relpath.to_string_lossy());

    let expected_reqfile_abspath = space_abspath.join("parent-col-1/back-slash-req-1.toml");
    assert!(expected_reqfile_abspath.exists());

    let req_toml = request::parse_reqtoml(&expected_reqfile_abspath).unwrap();
    assert_eq!(req_toml.meta.name, "Back\\Slash Req 1");
}

#[test]
fn create_req_with_multiple_slashes_in_relpath() {
    let tmp_dir = tempfile::tempdir().unwrap();
    let mut sharedstate = tmp_space_sharedstate(tmp_dir.path());

    let (parent_relpath, req_segment) = collection::create_parent_collections_if_missing(
        Path::new(""),
        "Parent Col 1///Child Col 1//Multiple Slash Req 1",
        &mut sharedstate,
    )
    .expect("Failed to create parent collections");

    let result = request::create_req(&parent_relpath, &req_segment, &mut sharedstate)
        .expect("Failed to create request with multiple slashes");

    let expected_reqfile_relpath = PathBuf::from("parent-col-1")
        .join("child-col-1")
        .join("multiple-slash-req-1.toml");
    assert_eq!(result.relpath, expected_reqfile_relpath.to_string_lossy());
}

#[test]
fn create_req_integrated_flow() {
    let tmp_dir = tempfile::tempdir().unwrap();
    let mut sharedstate = tmp_space_sharedstate(tmp_dir.path());
    let space_abspath = PathBuf::from(&sharedstate.space.as_ref().unwrap().abspath);

    let (parent_relpath, req_segment) = collection::create_parent_collections_if_missing(
        Path::new(""),
        "Parent Col 1/Child Col 1/Grand Child Req 1",
        &mut sharedstate,
    )
    .expect("Failed to create parent collections");

    let result = request::create_req(&parent_relpath, &req_segment, &mut sharedstate)
        .expect("Failed to create request");

    let expected_reqfile_relpath = PathBuf::from("parent-col-1")
        .join("child-col-1")
        .join("grand-child-req-1.toml");
    assert_eq!(result.relpath, expected_reqfile_relpath.to_string_lossy());

    assert!(space_abspath.join("parent-col-1").exists());
    assert!(space_abspath.join("parent-col-1/child-col-1").exists());
    assert!(space_abspath
        .join("parent-col-1/child-col-1/grand-child-req-1.toml")
        .exists());

    let req_toml = request::parse_reqtoml(
        &space_abspath.join("parent-col-1/child-col-1/grand-child-req-1.toml"),
    )
    .unwrap();
    assert_eq!(req_toml.meta.name, "Grand Child Req 1");
}
