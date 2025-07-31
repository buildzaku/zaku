use std::fs;
use std::{path::PathBuf, sync::Arc, thread};
use tempfile;

use crate::store::{self, StateStore};
use crate::{
    request::models::{ReqCfg, ReqMeta, ReqUrl},
    space::models::SpaceReference,
    store::{state::Theme, ReqBuffer, SpaceBufferStore},
};

#[test]
fn state_store_get_creates_default_when_file_doesnt_exist() {
    let tmp_dir = tempfile::tempdir().unwrap();
    let state_store_abspath = store::utils::state_store_abspath(tmp_dir.path());

    let state_store = StateStore::get(&state_store_abspath).unwrap();

    assert!(state_store.spaceref.is_none());
    assert!(state_store.spacerefs.is_empty());
    assert_eq!(state_store.user_settings.default_theme, Theme::System);
    assert!(state_store_abspath.exists());
}

#[test]
fn state_store_get_loads_existing_store_from_filesystem() {
    let tmp_dir = tempfile::tempdir().unwrap();
    let store_abspath = store::utils::state_store_abspath(tmp_dir.path());

    if let Some(parent) = store_abspath.parent() {
        fs::create_dir_all(parent).unwrap();
    }

    let test_space_1_path = PathBuf::from("test").join("space-1");
    let test_space_2_path = PathBuf::from("test").join("space-2");

    let json_content = serde_json::json!({
        "spaceref": {
            "abspath": test_space_1_path,
            "name": "Test Space 1"
        },
        "spacerefs": [
            {
                "abspath": test_space_1_path,
                "name": "Test Space 1"
            },
            {
                "abspath": test_space_2_path,
                "name": "Test Space 2"
            }
        ],
        "user_settings": {
            "default_theme": "System"
        }
    });
    fs::write(&store_abspath, json_content.to_string()).unwrap();

    let loaded_state_store = StateStore::get(&store_abspath).unwrap();

    assert!(loaded_state_store.spaceref.is_some());
    assert_eq!(
        loaded_state_store.spaceref.as_ref().unwrap().abspath,
        test_space_1_path
    );
    assert_eq!(loaded_state_store.spacerefs.len(), 2);
    assert_eq!(loaded_state_store.spacerefs[0].name, "Test Space 1");
    assert_eq!(loaded_state_store.spacerefs[1].name, "Test Space 2");
}

#[test]
fn state_store_update_persists_changes_to_filesystem() {
    let tmp_dir = tempfile::tempdir().unwrap();
    let store_abspath = store::utils::state_store_abspath(tmp_dir.path());

    let space_ref_1 = SpaceReference {
        abspath: PathBuf::from("test").join("space-1"),
        name: "Test Space 1".to_string(),
    };
    let space_ref_2 = SpaceReference {
        abspath: PathBuf::from("test").join("space-2"),
        name: "Test Space 2".to_string(),
    };

    let mut state_store = StateStore::get(&store_abspath).unwrap();
    state_store
        .update(|state| {
            state.spaceref = Some(space_ref_1.clone());
            state.spacerefs.push(space_ref_1);
            state.spacerefs.push(space_ref_2);
            state.user_settings.default_theme = Theme::Light;
        })
        .unwrap();

    assert!(state_store.spaceref.is_some());
    assert_eq!(state_store.spacerefs.len(), 2);

    let fresh_state_store = StateStore::get(&store_abspath).unwrap();
    assert!(fresh_state_store.spaceref.is_some());
    assert_eq!(
        fresh_state_store.spaceref.as_ref().unwrap().abspath,
        PathBuf::from("test").join("space-1")
    );
    assert_eq!(fresh_state_store.user_settings.default_theme, Theme::Light);
    assert_eq!(fresh_state_store.spacerefs.len(), 2);
}

#[test]
fn state_store_handles_corrupt_json_by_overwriting_with_defaults() {
    let tmp_dir = tempfile::tempdir().unwrap();
    let store_abspath = store::utils::state_store_abspath(tmp_dir.path());

    fs::write(&store_abspath, "invalid json {").unwrap();

    let state_store = StateStore::get(&store_abspath).unwrap();
    assert!(state_store.spaceref.is_none());
    assert!(state_store.spacerefs.is_empty());

    let content = fs::read_to_string(&store_abspath).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&content).unwrap();
    assert!(parsed["spaceref"].is_null());
    assert!(parsed["spacerefs"].is_array());
    assert_eq!(parsed["spacerefs"].as_array().unwrap().len(), 0);
}

#[test]
fn state_store_get_loads_existing_settings_from_filesystem() {
    let tmp_dir = tempfile::tempdir().unwrap();
    let state_store_abspath = store::utils::state_store_abspath(tmp_dir.path());

    if let Some(parent) = state_store_abspath.parent() {
        fs::create_dir_all(parent).unwrap();
    }

    let json_content = serde_json::json!({
        "spaceref": null,
        "spacerefs": [],
        "user_settings": {
            "default_theme": "Dark"
        }
    });
    fs::write(&state_store_abspath, json_content.to_string()).unwrap();

    let state_store = StateStore::get(&state_store_abspath).unwrap();

    assert_eq!(state_store.user_settings.default_theme, Theme::Dark);
}

#[test]
fn state_store_handles_corrupt_json_by_using_default() {
    let tmp_dir = tempfile::tempdir().unwrap();
    let state_store_abspath = store::utils::state_store_abspath(tmp_dir.path());

    let mut state_store = StateStore::get(&state_store_abspath).unwrap();
    state_store
        .update(|state| {
            state.user_settings.default_theme = Theme::Dark;
        })
        .unwrap();

    fs::write(&state_store_abspath, "invalid json {").unwrap();

    let state_store = StateStore::get(&state_store_abspath).unwrap();
    assert_eq!(state_store.user_settings.default_theme, Theme::System);

    let content = fs::read_to_string(&state_store_abspath).unwrap();
    assert!(content.contains("System"));
}

#[test]
fn sbf_store_get_creates_empty_buffer_when_file_doesnt_exist() {
    let tmp_dir = tempfile::tempdir().unwrap();
    let datadir_abspath = tmp_dir.path();
    let space_abspath = tmp_dir.path().join("test-space");
    let sbf_store_abspath = store::utils::sbf_store_abspath(datadir_abspath, &space_abspath);

    let sbf_store = SpaceBufferStore::get(&sbf_store_abspath).unwrap();
    let sbf_store_mtx = sbf_store.lock().unwrap();

    assert!(sbf_store_mtx.requests.is_empty());
    assert!(sbf_store_abspath.exists());
}

#[test]
fn sbf_store_update_persists_changes_to_filesystem_and_returns_updated_buffer() {
    let tmp_dir = tempfile::tempdir().unwrap();
    let datadir_abspath = tmp_dir.path();
    let space_abspath = tmp_dir.path().join("test-space");
    let sbf_store_abspath = store::utils::sbf_store_abspath(datadir_abspath, &space_abspath);

    let req_buf = ReqBuffer {
        meta: ReqMeta {
            fsname: "test-req.toml".to_string(),
            name: "Test Req".to_string(),
            has_unsaved_changes: true,
            relpath: PathBuf::from("test-req.toml"),
        },
        config: ReqCfg {
            method: "GET".to_string(),
            url: ReqUrl {
                raw: Some("https://zaku.app/test-req".to_string()),
                protocol: Some("https".to_string()),
                host: Some("zaku.app".to_string()),
                path: Some("/test-req".to_string()),
            },
            headers: Vec::new(),
            parameters: Vec::new(),
            content_type: None,
            body: None,
        },
    };

    let sbf_store = SpaceBufferStore::get(&sbf_store_abspath).unwrap();
    SpaceBufferStore::update(&sbf_store, |sbf_store| {
        let mut sbf_store_mtx = sbf_store.lock().unwrap();
        sbf_store_mtx
            .requests
            .insert(PathBuf::from("test-req.toml"), req_buf);
    })
    .unwrap();

    assert!(sbf_store_abspath.exists());

    let sbf_store_mtx = sbf_store.lock().unwrap();
    assert!(sbf_store_mtx
        .requests
        .contains_key(&PathBuf::from("test-req.toml")));

    let fresh_sbf_store = SpaceBufferStore::get(&sbf_store_abspath).unwrap();
    let fresh_lock = fresh_sbf_store.lock().unwrap();
    assert!(fresh_lock
        .requests
        .contains_key(&PathBuf::from("test-req.toml")));
}

#[test]
fn sbf_store_handles_concurrent_access_to_same_buffer_instance() {
    let tmp_dir = tempfile::tempdir().unwrap();
    let datadir_abspath = tmp_dir.path();
    let space_abspath = tmp_dir.path().join("test-space");
    let sbf_store_abspath = store::utils::sbf_store_abspath(datadir_abspath, &space_abspath);

    let sbf_store = SpaceBufferStore::get(&sbf_store_abspath).unwrap();
    let sbf_store_clone = Arc::clone(&sbf_store);

    let handles: Vec<_> = (0..10)
        .map(|idx| {
            let sbf_store = Arc::clone(&sbf_store);
            let req_fsname = format!("concurrent-req-{idx}.toml");

            thread::spawn(move || {
                let mut sbf_store_mtx = sbf_store.lock().unwrap();
                let req_buf = ReqBuffer {
                    meta: ReqMeta {
                        fsname: req_fsname.clone(),
                        name: format!("Concurrent Req {idx}"),
                        has_unsaved_changes: false,
                        relpath: PathBuf::from(req_fsname.clone()),
                    },
                    config: ReqCfg {
                        method: "GET".to_string(),
                        url: ReqUrl {
                            raw: Some(format!("https://zaku.app/concurrent-req-{idx}")),
                            protocol: Some("https".to_string()),
                            host: Some("zaku.app".to_string()),
                            path: Some(format!("/concurrent-req-{idx}")),
                        },
                        headers: Vec::new(),
                        parameters: Vec::new(),
                        content_type: None,
                        body: None,
                    },
                };
                sbf_store_mtx
                    .requests
                    .insert(PathBuf::from(req_fsname), req_buf);
            })
        })
        .collect();

    for handle in handles {
        handle.join().unwrap();
    }

    let sbf_store_mtx = sbf_store_clone.lock().unwrap();
    assert_eq!(sbf_store_mtx.requests.len(), 10);

    for idx in 0..10 {
        let req_relpath = PathBuf::from(format!("concurrent-req-{idx}.toml"));
        assert!(sbf_store_mtx.requests.contains_key(&req_relpath));
    }
}

#[test]
fn sbf_store_maintains_persistence_across_separate_get_calls() {
    let tmp_dir = tempfile::tempdir().unwrap();
    let datadir_abspath = tmp_dir.path();
    let space_abspath = tmp_dir.path().join("test-space");
    let sbf_store_abspath = store::utils::sbf_store_abspath(datadir_abspath, &space_abspath);

    let req_buf = ReqBuffer {
        meta: ReqMeta {
            fsname: "persistent-req.toml".to_string(),
            name: "Persistent Req".to_string(),
            has_unsaved_changes: false,
            relpath: PathBuf::from("persistent-req.toml"),
        },
        config: ReqCfg {
            method: "POST".to_string(),
            url: ReqUrl {
                raw: Some("https://zaku.app/persistent-req".to_string()),
                protocol: Some("https".to_string()),
                host: Some("zaku.app".to_string()),
                path: Some("/persistent-req".to_string()),
            },
            headers: Vec::new(),
            parameters: Vec::new(),
            content_type: None,
            body: None,
        },
    };

    let sbf_store = SpaceBufferStore::get(&sbf_store_abspath).unwrap();
    SpaceBufferStore::update(&sbf_store, |sbf_store| {
        let mut sbf_store_mtx = sbf_store.lock().unwrap();
        sbf_store_mtx
            .requests
            .insert(PathBuf::from("persistent-req.toml"), req_buf);
    })
    .unwrap();

    let sbf_store = SpaceBufferStore::get(&sbf_store_abspath).unwrap();
    let sbf_store_mtx = sbf_store.lock().unwrap();
    assert!(sbf_store_mtx
        .requests
        .contains_key(&PathBuf::from("persistent-req.toml")));

    let persisted_req = sbf_store_mtx
        .requests
        .get(&PathBuf::from("persistent-req.toml"))
        .unwrap();
    assert_eq!(persisted_req.meta.name, "Persistent Req");
    assert_eq!(persisted_req.config.method, "POST");
}

#[test]
fn sbf_store_serializes_concurrent_update_calls_without_data_loss() {
    let tmp_dir = tempfile::tempdir().unwrap();
    let datadir_abspath = tmp_dir.path();
    let space_abspath = tmp_dir.path().join("test-space");
    let sbf_store_abspath = store::utils::sbf_store_abspath(datadir_abspath, &space_abspath);

    let sbf_store = SpaceBufferStore::get(&sbf_store_abspath).unwrap();

    let handles: Vec<_> = (0..10)
        .map(|idx| {
            let sbf_store = Arc::clone(&sbf_store);

            thread::spawn(move || {
                let req_buf = ReqBuffer {
                    meta: ReqMeta {
                        fsname: format!("update-req-{idx}.toml"),
                        name: format!("Update Req {idx}"),
                        has_unsaved_changes: true,
                        relpath: PathBuf::from(format!("update-req-{idx}.toml")),
                    },
                    config: ReqCfg {
                        method: "PUT".to_string(),
                        url: ReqUrl {
                            raw: Some(format!("https://zaku.app/update-req-{idx}")),
                            protocol: Some("https".to_string()),
                            host: Some("zaku.app".to_string()),
                            path: Some(format!("/update-req-{idx}")),
                        },
                        headers: Vec::new(),
                        parameters: Vec::new(),
                        content_type: None,
                        body: None,
                    },
                };

                SpaceBufferStore::update(&sbf_store, |sbf_store| {
                    let mut sbf_store_mtx = sbf_store.lock().unwrap();
                    let req_relpath = PathBuf::from(format!("update-req-{idx}.toml"));
                    sbf_store_mtx.requests.insert(req_relpath, req_buf);
                })
                .unwrap();
            })
        })
        .collect();

    for handle in handles {
        handle.join().unwrap();
    }

    let sbf_store_mtx = sbf_store.lock().unwrap();
    assert_eq!(sbf_store_mtx.requests.len(), 10);

    for idx in 0..10 {
        let req_relpath = PathBuf::from(format!("update-req-{idx}.toml"));
        assert!(sbf_store_mtx.requests.contains_key(&req_relpath));

        let req = sbf_store_mtx.requests.get(&req_relpath).unwrap();
        assert_eq!(req.meta.name, format!("Update Req {idx}"));
        assert_eq!(req.config.method, "PUT");
        assert!(req.meta.has_unsaved_changes);
    }
}

#[test]
fn sck_store_get_creates_default_when_file_doesnt_exist() {
    let tmp_dir = tempfile::tempdir().unwrap();
    let datadir_abspath = tmp_dir.path();
    let space_abspath = tmp_dir.path().join("test-space");
    let sck_store_abspath = store::utils::sck_store_abspath(datadir_abspath, &space_abspath);

    let sck_store = store::SpaceCookieStore::get(&sck_store_abspath).unwrap();
    let sck_store_mtx = sck_store.cookies.lock().unwrap();

    assert!(sck_store_mtx.iter_any().count() == 0);
    assert!(sck_store_abspath.exists());
}

#[test]
fn sck_store_get_loads_existing_store_from_filesystem() {
    let tmp_dir = tempfile::tempdir().unwrap();
    let datadir_abspath = tmp_dir.path();
    let space_abspath = tmp_dir.path().join("test-space");
    let sck_store_abspath = store::utils::sck_store_abspath(datadir_abspath, &space_abspath);

    if let Some(parent) = sck_store_abspath.parent() {
        fs::create_dir_all(parent).unwrap();
    }

    let future_date = time::OffsetDateTime::now_utc() + time::Duration::days(30);
    let expires_at_rfc2822 = future_date
        .format(&time::format_description::well_known::Rfc2822)
        .unwrap();
    let expires_at_rfc3339 = future_date
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap();
    let cookie_json = serde_json::json!([
        {
            "raw_cookie": format!("cookie_1=value1; Path=/; Domain=zaku.app; Expires={}",
                expires_at_rfc2822),
            "path": ["/", true],
            "domain": { "Suffix": "zaku.app" },
            "expires": { "AtUtc": expires_at_rfc3339 }
        },
        {
            "raw_cookie": format!("cookie_2=value2; Path=/; Domain=zaku.app; Expires={}",
                expires_at_rfc2822),
            "path": ["/", true],
            "domain": { "Suffix": "zaku.app" },
            "expires": { "AtUtc": expires_at_rfc3339 }
        }
    ]);
    fs::write(&sck_store_abspath, cookie_json.to_string()).unwrap();

    let sck_store = store::SpaceCookieStore::get(&sck_store_abspath).unwrap();
    let sck_store_mtx = sck_store.cookies.lock().unwrap();

    assert!(sck_store_mtx.iter_any().count() == 2);
    assert!(sck_store_abspath.exists());
}

#[test]
fn sck_store_handles_corrupt_cookie_json_by_using_default() {
    let tmp_dir = tempfile::tempdir().unwrap();
    let datadir_abspath = tmp_dir.path();
    let space_abspath = tmp_dir.path().join("test-space");
    let sck_store_abspath = store::utils::sck_store_abspath(datadir_abspath, &space_abspath);

    if let Some(parent) = sck_store_abspath.parent() {
        fs::create_dir_all(parent).unwrap();
    }

    fs::write(&sck_store_abspath, "invalid json {").unwrap();

    let sck_store = store::SpaceCookieStore::get(&sck_store_abspath).unwrap();
    let sck_store_mtx = sck_store.cookies.lock().unwrap();

    assert!(sck_store_mtx.iter_any().count() == 0);
    assert!(sck_store_abspath.exists());
}

#[test]
fn sst_store_get_creates_default_when_file_doesnt_exist() {
    let tmp_dir = tempfile::tempdir().unwrap();
    let datadir_abspath = tmp_dir.path();
    let space_abspath = tmp_dir.path().join("test-space");

    let state_store_abspath = store::utils::state_store_abspath(datadir_abspath);
    let _state_store = StateStore::get(&state_store_abspath).unwrap();

    let sst_store_abspath = store::utils::sst_store_abspath(datadir_abspath, &space_abspath);
    let sst_store = store::SpaceSettingsStore::get(&sst_store_abspath).unwrap();

    assert_eq!(sst_store.theme, Theme::System);
    assert!(!sst_store.notifications.audio.on_req_finish);
    assert!(sst_store_abspath.exists());
}

#[test]
fn sst_store_get_loads_existing_settings_from_filesystem() {
    let tmp_dir = tempfile::tempdir().unwrap();
    let datadir_abspath = tmp_dir.path();
    let space_abspath = tmp_dir.path().join("test-space");

    let state_store_abspath = store::utils::state_store_abspath(datadir_abspath);
    let _state_store = StateStore::get(&state_store_abspath).unwrap();

    let sst_store_abspath = store::utils::sst_store_abspath(datadir_abspath, &space_abspath);

    if let Some(parent) = sst_store_abspath.parent() {
        fs::create_dir_all(parent).unwrap();
    }

    let settings_json = serde_json::json!({
        "theme": "Dark",
        "notifications": {
            "audio": {
                "on_req_finish": true
            }
        }
    });
    fs::write(&sst_store_abspath, settings_json.to_string()).unwrap();

    let sst_store = store::SpaceSettingsStore::get(&sst_store_abspath).unwrap();

    assert_eq!(sst_store.theme, Theme::Dark);
    assert!(sst_store.notifications.audio.on_req_finish);
}

#[test]
fn sst_store_update_persists_changes_to_filesystem() {
    let tmp_dir = tempfile::tempdir().unwrap();
    let datadir_abspath = tmp_dir.path();
    let space_abspath = tmp_dir.path().join("test-space");

    let state_store_abspath = store::utils::state_store_abspath(datadir_abspath);
    let _state_store = StateStore::get(&state_store_abspath).unwrap();

    let sst_store_abspath = store::utils::sst_store_abspath(datadir_abspath, &space_abspath);
    let mut sst_store = store::SpaceSettingsStore::get(&sst_store_abspath).unwrap();

    sst_store
        .update(|settings| {
            settings.theme = Theme::Light;
            settings.notifications.audio.on_req_finish = true;
        })
        .unwrap();

    assert!(sst_store_abspath.exists());

    let fresh_sst_store = store::SpaceSettingsStore::get(&sst_store_abspath).unwrap();
    assert_eq!(fresh_sst_store.theme, Theme::Light);
    assert!(fresh_sst_store.notifications.audio.on_req_finish);
}

#[test]
fn sst_store_handles_corrupt_json_by_using_default() {
    let tmp_dir = tempfile::tempdir().unwrap();
    let datadir_abspath = tmp_dir.path();
    let space_abspath = tmp_dir.path().join("test-space");

    let state_store_abspath = store::utils::state_store_abspath(datadir_abspath);
    let _state_store = StateStore::get(&state_store_abspath).unwrap();

    let sst_store_abspath = store::utils::sst_store_abspath(datadir_abspath, &space_abspath);

    if let Some(parent) = sst_store_abspath.parent() {
        fs::create_dir_all(parent).unwrap();
    }

    fs::write(&sst_store_abspath, "invalid json {").unwrap();

    let sst_store = store::SpaceSettingsStore::get(&sst_store_abspath).unwrap();

    assert_eq!(sst_store.theme, Theme::System);
    assert!(!sst_store.notifications.audio.on_req_finish);
    assert!(sst_store_abspath.exists());
}

#[test]
fn sst_store_inherits_theme_from_state_store() {
    let tmp_dir = tempfile::tempdir().unwrap();
    let datadir_abspath = tmp_dir.path();
    let space_abspath = tmp_dir.path().join("test-space");

    let state_store_abspath = store::utils::state_store_abspath(datadir_abspath);
    let mut state_store = StateStore::get(&state_store_abspath).unwrap();
    state_store
        .update(|state| {
            state.user_settings.default_theme = Theme::Light;
        })
        .unwrap();

    let sst_store_abspath = store::utils::sst_store_abspath(datadir_abspath, &space_abspath);
    let sst_store = store::SpaceSettingsStore::get(&sst_store_abspath).unwrap();

    assert_eq!(sst_store.theme, Theme::Light);
}
