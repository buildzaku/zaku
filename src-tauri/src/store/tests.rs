use std::{sync::Arc, thread};
use tempfile;

use crate::request::models::{ReqCfg, ReqMeta, ReqUrl};
use crate::store::ReqBuf;
use crate::store::SpaceBuf;

#[test]
fn spacebuf_get_creates_empty_buffer_when_file_doesnt_exist() {
    let tmp_dir = tempfile::tempdir().unwrap();
    let space_abspath = tmp_dir.path();

    let spacebuf = SpaceBuf::get(space_abspath).unwrap();
    let spacebuf_lock = spacebuf.lock().unwrap();

    assert_eq!(spacebuf_lock.abspath, space_abspath);
    assert!(spacebuf_lock.requests.is_empty());
}

#[test]
fn spacebuf_update_persists_changes_to_filesystem_and_returns_updated_buffer() {
    let tmp_dir = tempfile::tempdir().unwrap();
    let space_abspath = tmp_dir.path();

    let req_buf = ReqBuf {
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

    let spacebuf = SpaceBuf::update(space_abspath, |spacebuf| {
        let mut spacebuf_lock = spacebuf.lock().unwrap();
        spacebuf_lock
            .requests
            .insert("test-req.toml".to_string(), req_buf);
    })
    .unwrap();

    let spacebuf_filepath = SpaceBuf::filepath(space_abspath);
    assert!(spacebuf_filepath.exists());

    let spacebuf_lock = spacebuf.lock().unwrap();
    assert!(spacebuf_lock.requests.contains_key("test-req.toml"));

    let fresh_spacebuf = SpaceBuf::get(space_abspath).unwrap();
    let fresh_lock = fresh_spacebuf.lock().unwrap();
    assert!(fresh_lock.requests.contains_key("test-req.toml"));
}

#[test]
fn spacebuf_handles_concurrent_access_to_same_buffer_instance() {
    let tmp_dir = tempfile::tempdir().unwrap();
    let space_abspath = tmp_dir.path().to_path_buf();

    let spacebuf = SpaceBuf::get(&space_abspath).unwrap();
    let spacebuf_clone = Arc::clone(&spacebuf);

    let handles: Vec<_> = (0..10)
        .map(|idx| {
            let spacebuf = Arc::clone(&spacebuf);
            let key = format!("concurrent-req-{idx}.toml");

            thread::spawn(move || {
                let mut spacebuf_lock = spacebuf.lock().unwrap();
                let req_buf = ReqBuf {
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
                spacebuf_lock.requests.insert(key, req_buf);
            })
        })
        .collect();

    for handle in handles {
        handle.join().unwrap();
    }

    let spacebuf_lock = spacebuf_clone.lock().unwrap();
    assert_eq!(spacebuf_lock.requests.len(), 10);

    for idx in 0..10 {
        let key = format!("concurrent-req-{idx}.toml");
        assert!(spacebuf_lock.requests.contains_key(&key));
    }
}

#[test]
fn spacebuf_maintains_persistence_across_separate_get_calls() {
    let tmp_dir = tempfile::tempdir().unwrap();
    let space_abspath = tmp_dir.path();

    let req_buf = ReqBuf {
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

    SpaceBuf::update(space_abspath, |spacebuf| {
        let mut spacebuf_lock = spacebuf.lock().unwrap();
        spacebuf_lock
            .requests
            .insert("persistent-req.toml".to_string(), req_buf);
    })
    .unwrap();

    let spacebuf = SpaceBuf::get(space_abspath).unwrap();
    let spacebuf_lock = spacebuf.lock().unwrap();
    assert!(spacebuf_lock.requests.contains_key("persistent-req.toml"));

    let persisted_req = spacebuf_lock.requests.get("persistent-req.toml").unwrap();
    assert_eq!(persisted_req.meta.name, "Persistent Req");
    assert_eq!(persisted_req.config.method, "POST");
}

#[test]
fn spacebuf_serializes_concurrent_update_calls_without_data_loss() {
    let tmp_dir = tempfile::tempdir().unwrap();
    let space_abspath = tmp_dir.path().to_path_buf();

    let handles: Vec<_> = (0..10)
        .map(|idx| {
            let space_path = space_abspath.clone();

            thread::spawn(move || {
                let req_buf = ReqBuf {
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

                SpaceBuf::update(&space_path, |spacebuf| {
                    let mut spacebuf_lock = spacebuf.lock().unwrap();
                    let key = format!("update-req-{idx}.toml");
                    spacebuf_lock.requests.insert(key, req_buf);
                })
                .unwrap();
            })
        })
        .collect();

    for handle in handles {
        handle.join().unwrap();
    }

    let spacebuf = SpaceBuf::get(&space_abspath).unwrap();
    let spacebuf_lock = spacebuf.lock().unwrap();
    assert_eq!(spacebuf_lock.requests.len(), 10);

    for idx in 0..10 {
        let key = format!("update-req-{idx}.toml");
        assert!(spacebuf_lock.requests.contains_key(&key));

        let req = spacebuf_lock.requests.get(&key).unwrap();
        assert_eq!(req.meta.name, format!("Update Req {idx}"));
        assert_eq!(req.config.method, "PUT");
        assert!(req.meta.has_unsaved_changes);
    }
}
