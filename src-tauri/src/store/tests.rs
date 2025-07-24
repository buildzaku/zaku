use std::fs;
use std::{sync::Arc, thread};
use tempfile;

use crate::store::{self, UserSettingsStore};
use crate::{
    request::models::{ReqCfg, ReqMeta, ReqUrl},
    space::models::SpaceReference,
    store::{ReqBuffer, SpaceBufferStore, Store, Theme},
};

#[test]
fn store_get_creates_default_when_file_doesnt_exist() {
    let tmp_dir = tempfile::tempdir().unwrap();
    let store_abspath = store::utils::store_abspath(tmp_dir.path());

    let store = Store::get(&store_abspath).unwrap();

    assert!(store.spaceref.is_none());
    assert!(store.spacerefs.is_empty());
    assert!(store_abspath.exists());
}

#[test]
fn store_get_loads_existing_store_from_filesystem() {
    let tmp_dir = tempfile::tempdir().unwrap();
    let store_abspath = store::utils::store_abspath(tmp_dir.path());

    if let Some(parent) = store_abspath.parent() {
        fs::create_dir_all(parent).unwrap();
    }

    let json_content = serde_json::json!({
        "spaceref": {
            "path": "/test/space-1",
            "name": "Test Space 1"
        },
        "spacerefs": [
            {
                "path": "/test/space-1",
                "name": "Test Space 1"
            },
            {
                "path": "/test/space-2",
                "name": "Test Space 2"
            }
        ]
    });
    fs::write(&store_abspath, json_content.to_string()).unwrap();

    let loaded_store = Store::get(&store_abspath).unwrap();

    assert!(loaded_store.spaceref.is_some());
    assert_eq!(
        loaded_store.spaceref.as_ref().unwrap().path,
        "/test/space-1"
    );
    assert_eq!(loaded_store.spacerefs.len(), 2);
    assert_eq!(loaded_store.spacerefs[0].name, "Test Space 1");
    assert_eq!(loaded_store.spacerefs[1].name, "Test Space 2");
}

#[test]
fn store_update_persists_changes_to_filesystem() {
    let tmp_dir = tempfile::tempdir().unwrap();
    let store_abspath = store::utils::store_abspath(tmp_dir.path());

    let space_ref_1 = SpaceReference {
        path: "/test/space-1".to_string(),
        name: "Test Space 1".to_string(),
    };
    let space_ref_2 = SpaceReference {
        path: "/test/space-2".to_string(),
        name: "Test Space 2".to_string(),
    };

    let mut store = Store::get(&store_abspath).unwrap();
    store
        .update(|store| {
            store.spaceref = Some(space_ref_1.clone());
            store.spacerefs.push(space_ref_1);
            store.spacerefs.push(space_ref_2);
        })
        .unwrap();

    assert!(store.spaceref.is_some());
    assert_eq!(store.spacerefs.len(), 2);

    let fresh_store = Store::get(&store_abspath).unwrap();
    assert!(fresh_store.spaceref.is_some());
    assert_eq!(fresh_store.spaceref.as_ref().unwrap().path, "/test/space-1");
    assert_eq!(fresh_store.spacerefs.len(), 2);
}

#[test]
fn store_handles_corrupt_json_by_overwriting_with_defaults() {
    let tmp_dir = tempfile::tempdir().unwrap();
    let store_abspath = store::utils::store_abspath(tmp_dir.path());

    fs::write(&store_abspath, "invalid json {").unwrap();

    let store = Store::get(&store_abspath).unwrap();
    assert!(store.spaceref.is_none());
    assert!(store.spacerefs.is_empty());

    let content = fs::read_to_string(&store_abspath).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&content).unwrap();
    assert!(parsed["spaceref"].is_null());
    assert!(parsed["spacerefs"].is_array());
    assert_eq!(parsed["spacerefs"].as_array().unwrap().len(), 0);
}

#[test]
fn user_settings_get_creates_default_when_file_doesnt_exist() {
    let tmp_dir = tempfile::tempdir().unwrap();
    let ust_store_abspath = store::utils::ust_store_abspath(tmp_dir.path());

    let ust_store = UserSettingsStore::get(&ust_store_abspath).unwrap();

    assert_eq!(ust_store.default_theme, Theme::System);
    assert!(ust_store_abspath.exists());
}

#[test]
fn user_settings_get_loads_existing_settings_from_filesystem() {
    let tmp_dir = tempfile::tempdir().unwrap();
    let ust_store_abspath = store::utils::ust_store_abspath(tmp_dir.path());

    if let Some(parent) = ust_store_abspath.parent() {
        fs::create_dir_all(parent).unwrap();
    }

    let json_content = serde_json::json!({
        "default_theme": "Dark"
    });
    fs::write(&ust_store_abspath, json_content.to_string()).unwrap();

    let ust_store = UserSettingsStore::get(&ust_store_abspath).unwrap();

    assert_eq!(ust_store.default_theme, Theme::Dark);
}

#[test]
fn user_settings_update_persists_changes_to_filesystem() {
    let tmp_dir = tempfile::tempdir().unwrap();
    let ust_store_abspath = store::utils::ust_store_abspath(tmp_dir.path());

    let mut ust_store = UserSettingsStore::get(&ust_store_abspath).unwrap();
    ust_store
        .update(|settings| {
            settings.default_theme = Theme::Light;
        })
        .unwrap();

    assert_eq!(ust_store.default_theme, Theme::Light);

    let fresh_ust_store = UserSettingsStore::get(&ust_store_abspath).unwrap();
    assert_eq!(fresh_ust_store.default_theme, Theme::Light);
}

#[test]
fn user_settings_handles_corrupt_json_by_using_default() {
    let tmp_dir = tempfile::tempdir().unwrap();
    let ust_store_abspath = store::utils::ust_store_abspath(tmp_dir.path());

    let mut ust_store = UserSettingsStore::get(&ust_store_abspath).unwrap();
    ust_store
        .update(|settings| {
            settings.default_theme = Theme::Dark;
        })
        .unwrap();

    fs::write(&ust_store_abspath, "invalid json {").unwrap();

    let ust_store = UserSettingsStore::get(&ust_store_abspath).unwrap();
    assert_eq!(ust_store.default_theme, Theme::System);

    let content = fs::read_to_string(&ust_store_abspath).unwrap();
    assert!(content.contains("System"));
}

#[test]
fn spacebuf_get_creates_empty_buffer_when_file_doesnt_exist() {
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
fn spacebuf_update_persists_changes_to_filesystem_and_returns_updated_buffer() {
    let tmp_dir = tempfile::tempdir().unwrap();
    let datadir_abspath = tmp_dir.path();
    let space_abspath = tmp_dir.path().join("test-space");
    let sbf_store_abspath = store::utils::sbf_store_abspath(datadir_abspath, &space_abspath);

    let req_buf = ReqBuffer {
        meta: ReqMeta {
            fsname: "test-req.toml".to_string(),
            name: "Test Req".to_string(),
            has_unsaved_changes: true,
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
            .insert("test-req.toml".to_string(), req_buf);
    })
    .unwrap();

    assert!(sbf_store_abspath.exists());

    let sbf_store_mtx = sbf_store.lock().unwrap();
    assert!(sbf_store_mtx.requests.contains_key("test-req.toml"));

    let fresh_sbf_store = SpaceBufferStore::get(&sbf_store_abspath).unwrap();
    let fresh_lock = fresh_sbf_store.lock().unwrap();
    assert!(fresh_lock.requests.contains_key("test-req.toml"));
}

#[test]
fn spacebuf_handles_concurrent_access_to_same_buffer_instance() {
    let tmp_dir = tempfile::tempdir().unwrap();
    let datadir_abspath = tmp_dir.path();
    let space_abspath = tmp_dir.path().join("test-space");
    let sbf_store_abspath = store::utils::sbf_store_abspath(datadir_abspath, &space_abspath);

    let sbf_store = SpaceBufferStore::get(&sbf_store_abspath).unwrap();
    let sbf_store_clone = Arc::clone(&sbf_store);

    let handles: Vec<_> = (0..10)
        .map(|idx| {
            let sbf_store = Arc::clone(&sbf_store);
            let key = format!("concurrent-req-{idx}.toml");

            thread::spawn(move || {
                let mut sbf_store_mtx = sbf_store.lock().unwrap();
                let req_buf = ReqBuffer {
                    meta: ReqMeta {
                        fsname: key.clone(),
                        name: format!("Concurrent Req {idx}"),
                        has_unsaved_changes: false,
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
                sbf_store_mtx.requests.insert(key, req_buf);
            })
        })
        .collect();

    for handle in handles {
        handle.join().unwrap();
    }

    let sbf_store_mtx = sbf_store_clone.lock().unwrap();
    assert_eq!(sbf_store_mtx.requests.len(), 10);

    for idx in 0..10 {
        let key = format!("concurrent-req-{idx}.toml");
        assert!(sbf_store_mtx.requests.contains_key(&key));
    }
}

#[test]
fn spacebuf_maintains_persistence_across_separate_get_calls() {
    let tmp_dir = tempfile::tempdir().unwrap();
    let datadir_abspath = tmp_dir.path();
    let space_abspath = tmp_dir.path().join("test-space");
    let sbf_store_abspath = store::utils::sbf_store_abspath(datadir_abspath, &space_abspath);

    let req_buf = ReqBuffer {
        meta: ReqMeta {
            fsname: "persistent-req.toml".to_string(),
            name: "Persistent Req".to_string(),
            has_unsaved_changes: false,
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
            .insert("persistent-req.toml".to_string(), req_buf);
    })
    .unwrap();

    let sbf_store = SpaceBufferStore::get(&sbf_store_abspath).unwrap();
    let sbf_store_mtx = sbf_store.lock().unwrap();
    assert!(sbf_store_mtx.requests.contains_key("persistent-req.toml"));

    let persisted_req = sbf_store_mtx.requests.get("persistent-req.toml").unwrap();
    assert_eq!(persisted_req.meta.name, "Persistent Req");
    assert_eq!(persisted_req.config.method, "POST");
}

#[test]
fn spacebuf_serializes_concurrent_update_calls_without_data_loss() {
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
                    let key = format!("update-req-{idx}.toml");
                    sbf_store_mtx.requests.insert(key, req_buf);
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
        let key = format!("update-req-{idx}.toml");
        assert!(sbf_store_mtx.requests.contains_key(&key));

        let req = sbf_store_mtx.requests.get(&key).unwrap();
        assert_eq!(req.meta.name, format!("Update Req {idx}"));
        assert_eq!(req.config.method, "PUT");
        assert!(req.meta.has_unsaved_changes);
    }
}
