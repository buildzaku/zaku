use gpui::TestAppContext;
use indoc::indoc;
use language::LanguageRegistry;
use serde_json::{Value, json};
use std::{cell::RefCell, rc::Rc, sync::Arc};

use collections::BTreeMap;
use fs::{Fs, RemoveOptions, TempFs};
use path::{PathStyle, RelPath, rel_path};
use project::{Project, ProjectEvent, ProjectItem, ProjectPath, RequestBuffer, RequestBufferEvent};
use util_macros::path;
use worktree::{
    ConfigFileMeta, EnvironmentColor, EnvironmentFile, EnvironmentSection, FolderFile, ProjectFile,
    RequestFile, RequestFileHttp, RequestFileMeta, RequestFileState, RequestSection,
    SCHEMA_VERSION, Variable, WorktreeModelHandle,
};

#[gpui::test]
async fn test_newer_find_or_create_worktree_request_supersedes_previous_request(
    cx: &mut TestAppContext,
) {
    cx.executor().allow_parking();

    let temp_fs = TempFs::new(cx.executor());
    temp_fs.insert_tree(path!("first"), Value::default());
    temp_fs.insert_tree(path!("second"), Value::default());

    let first_path = temp_fs.path().join("first");
    let second_path = temp_fs.path().join("second");

    let languages = Arc::new(LanguageRegistry::test_new(cx.executor()));
    let project = cx
        .update(|cx| {
            Project::open(
                temp_fs.clone(),
                languages.clone(),
                temp_fs.path().join("project"),
                cx,
            )
        })
        .await
        .expect("project open should succeed");

    project
        .read_with(cx, |project, cx| project.wait_for_initial_scan(cx))
        .await;

    let first_open = project.update(cx, |project, cx| {
        project.find_or_create_worktree(&first_path, true, cx)
    });
    let second_open = project.update(cx, |project, cx| {
        project.find_or_create_worktree(&second_path, true, cx)
    });

    second_open
        .await
        .expect("newer project open should succeed");

    project
        .read_with(cx, |project, cx| project.wait_for_initial_scan(cx))
        .await;

    assert!(
        first_open.await.is_err(),
        "older project open should not report success once superseded"
    );
    assert_eq!(cx.update(|cx| project.read(cx).root(cx)), Some(second_path));
}

#[gpui::test]
async fn test_remove_worktree_invalidates_pending_find_or_create_worktree_request(
    cx: &mut TestAppContext,
) {
    cx.executor().allow_parking();

    let temp_fs = TempFs::new(cx.executor());
    temp_fs.insert_tree(path!("first"), Value::default());
    temp_fs.insert_tree(path!("second"), Value::default());

    let first_path = temp_fs.path().join("first");
    let second_path = temp_fs.path().join("second");

    let languages = Arc::new(LanguageRegistry::test_new(cx.executor()));
    let project = cx
        .update(|cx| Project::open(temp_fs.clone(), languages.clone(), first_path, cx))
        .await
        .expect("project open should succeed");

    project
        .read_with(cx, |project, cx| project.wait_for_initial_scan(cx))
        .await;

    let second_open = project.update(cx, |project, cx| {
        project.find_or_create_worktree(&second_path, true, cx)
    });

    project.update(cx, |project, cx| {
        project.remove_worktree(cx);
    });
    cx.run_until_parked();

    assert!(
        second_open.await.is_err(),
        "pending project open should be invalidated once the current worktree is removed"
    );
    assert!(cx.update(|cx| project.read(cx).root_worktree(cx)).is_none());
    assert!(cx.update(|cx| project.read(cx).root(cx)).is_none());
}

#[gpui::test]
async fn test_open_project_creates_worktree(cx: &mut TestAppContext) {
    cx.executor().allow_parking();

    let temp_fs = TempFs::new(cx.executor());
    temp_fs.insert_tree(path!("project"), Value::default());
    let project_path = temp_fs.path().join("project");

    let languages = Arc::new(LanguageRegistry::test_new(cx.executor()));
    let project = cx
        .update(|cx| Project::open(temp_fs.clone(), languages.clone(), project_path.clone(), cx))
        .await
        .expect("project open should succeed");

    project
        .read_with(cx, |project, cx| project.wait_for_initial_scan(cx))
        .await;

    let (current_worktree, current_root) = cx.update(|cx| {
        let project = project.read(cx);
        (project.root_worktree(cx), project.root(cx))
    });

    assert!(current_worktree.is_some());
    assert_eq!(current_root, Some(project_path));
}

#[gpui::test]
async fn test_project_config_files(cx: &mut TestAppContext) {
    cx.executor().allow_parking();

    let staging_env = indoc! {r#"
        [meta]
        version = 1

        [environment]
        variables = [{ name = "base_url", value = "http://localhost:4321" }]
    "#};
    let temp_fs = TempFs::new(cx.executor());
    temp_fs.insert_tree(
        path!("project"),
        json!({
            ".zaku": {
                "project.toml": indoc! {r#"
                    [meta]
                    version = 1

                    [request]
                    variables = [{ name = "base_url", value = "https://api.zaku.dev" }]
                "#},
                "folder.toml": indoc! {r#"
                    [meta]
                    version = 1

                    [request]
                    variables = [{ name = "user_id", value = "2" }]
                "#},
                "environments": {
                    "dev.toml": indoc! {r#"
                        [meta]
                        version = 1

                        [environment]
                        color = "green"
                        variables = [{ name = "base_url", value = "http://localhost:8000" }]
                    "#},
                    "prod.toml": indoc! {r#"
                        [meta]
                        version = 1

                        [environment]
                        color = "red"
                        variables = [{ name = "base_url", value = "https://api.zaku.dev" }]
                    "#},
                    "nested": {
                        "staging.toml": staging_env,
                    },
                },
            },
            "users": {
                ".zaku": {
                    "folder.toml": indoc! {r#"
                        [meta]
                        version = 1

                        [request]
                        variables = [{ name = "user_id", value = "1" }]
                    "#},
                    "environments": {
                        "staging.toml": staging_env,
                    },
                },
                "foo.toml": "",
            },
        }),
    );

    let project_path = temp_fs.path().join(path!("project"));
    let project = Project::test_new(temp_fs.clone(), &project_path, cx).await;
    let root_worktree = project.update(cx, |project, cx| project.root_worktree(cx).unwrap());
    cx.run_until_parked();

    project.read_with(cx, |project, cx| {
        let project_config_store = project.project_config_store().read(cx);

        assert_eq!(
            project_config_store.project_file(),
            Some(&ProjectFile {
                meta: ConfigFileMeta {
                    version: SCHEMA_VERSION,
                },
                request: RequestSection {
                    variables: vec![Variable {
                        name: "base_url".to_string(),
                        value: "https://api.zaku.dev".to_string(),
                        disabled: false,
                    }],
                },
            })
        );
        assert_eq!(
            project_config_store.environments().collect::<Vec<_>>(),
            vec![
                (
                    "dev",
                    &EnvironmentFile {
                        meta: ConfigFileMeta {
                            version: SCHEMA_VERSION,
                        },
                        environment: EnvironmentSection {
                            color: Some(EnvironmentColor::Green),
                            variables: vec![Variable {
                                name: "base_url".to_string(),
                                value: "http://localhost:8000".to_string(),
                                disabled: false,
                            }],
                        },
                    }
                ),
                (
                    "prod",
                    &EnvironmentFile {
                        meta: ConfigFileMeta {
                            version: SCHEMA_VERSION,
                        },
                        environment: EnvironmentSection {
                            color: Some(EnvironmentColor::Red),
                            variables: vec![Variable {
                                name: "base_url".to_string(),
                                value: "https://api.zaku.dev".to_string(),
                                disabled: false,
                            }],
                        },
                    }
                ),
            ]
        );
        assert_eq!(
            project_config_store.folder_file(rel_path("users")),
            Some(&FolderFile {
                meta: ConfigFileMeta {
                    version: SCHEMA_VERSION,
                },
                request: RequestSection {
                    variables: vec![Variable {
                        name: "user_id".to_string(),
                        value: "1".to_string(),
                        disabled: false,
                    }],
                },
            })
        );
        assert_eq!(project_config_store.folder_file(RelPath::empty()), None);
    });

    temp_fs
        .write(
            &project_path.join(path!(".zaku/environments/dev.toml")),
            indoc! {br#"
                [meta]
                version = 1

                [environment]
                variables = [{ name = "base_url", value = "http://localhost:3000" }]
            "#},
        )
        .await
        .unwrap();
    temp_fs
        .write(
            &project_path.join(path!(".zaku/environments/staging.toml")),
            indoc! {br#"
                [meta]
                version = 1

                [environment]
                variables = [{ name = "base_url", value = "http://localhost:5173" }]
            "#},
        )
        .await
        .unwrap();
    temp_fs
        .write(
            &project_path.join(path!("users/.zaku/folder.toml")),
            indoc! {br#"
                [meta]
                version = 1

                [request]
                variables = [{ name = "user_id", value = "3" }]
            "#},
        )
        .await
        .unwrap();
    root_worktree.flush_fs_events(cx).await;
    cx.run_until_parked();

    project.read_with(cx, |project, cx| {
        let project_config_store = project.project_config_store().read(cx);

        assert_eq!(
            project_config_store.environments().collect::<Vec<_>>(),
            vec![
                (
                    "dev",
                    &EnvironmentFile {
                        meta: ConfigFileMeta {
                            version: SCHEMA_VERSION,
                        },
                        environment: EnvironmentSection {
                            color: None,
                            variables: vec![Variable {
                                name: "base_url".to_string(),
                                value: "http://localhost:3000".to_string(),
                                disabled: false,
                            }],
                        },
                    }
                ),
                (
                    "prod",
                    &EnvironmentFile {
                        meta: ConfigFileMeta {
                            version: SCHEMA_VERSION,
                        },
                        environment: EnvironmentSection {
                            color: Some(EnvironmentColor::Red),
                            variables: vec![Variable {
                                name: "base_url".to_string(),
                                value: "https://api.zaku.dev".to_string(),
                                disabled: false,
                            }],
                        },
                    }
                ),
                (
                    "staging",
                    &EnvironmentFile {
                        meta: ConfigFileMeta {
                            version: SCHEMA_VERSION,
                        },
                        environment: EnvironmentSection {
                            color: None,
                            variables: vec![Variable {
                                name: "base_url".to_string(),
                                value: "http://localhost:5173".to_string(),
                                disabled: false,
                            }],
                        },
                    }
                ),
            ]
        );
        assert_eq!(
            project_config_store.folder_file(rel_path("users")),
            Some(&FolderFile {
                meta: ConfigFileMeta {
                    version: SCHEMA_VERSION,
                },
                request: RequestSection {
                    variables: vec![Variable {
                        name: "user_id".to_string(),
                        value: "3".to_string(),
                        disabled: false,
                    }],
                },
            })
        );
    });

    temp_fs
        .remove_file(
            &project_path.join(path!(".zaku/environments/dev.toml")),
            RemoveOptions::default(),
        )
        .await
        .unwrap();
    root_worktree.flush_fs_events(cx).await;
    cx.run_until_parked();

    project.read_with(cx, |project, cx| {
        assert_eq!(
            project
                .project_config_store()
                .read(cx)
                .environments()
                .collect::<Vec<_>>(),
            vec![
                (
                    "prod",
                    &EnvironmentFile {
                        meta: ConfigFileMeta {
                            version: SCHEMA_VERSION,
                        },
                        environment: EnvironmentSection {
                            color: Some(EnvironmentColor::Red),
                            variables: vec![Variable {
                                name: "base_url".to_string(),
                                value: "https://api.zaku.dev".to_string(),
                                disabled: false,
                            }],
                        },
                    }
                ),
                (
                    "staging",
                    &EnvironmentFile {
                        meta: ConfigFileMeta {
                            version: SCHEMA_VERSION,
                        },
                        environment: EnvironmentSection {
                            color: None,
                            variables: vec![Variable {
                                name: "base_url".to_string(),
                                value: "http://localhost:5173".to_string(),
                                disabled: false,
                            }],
                        },
                    }
                ),
            ]
        );
    });

    temp_fs
        .remove_dir(
            &project_path.join(path!(".zaku")),
            RemoveOptions {
                recursive: true,
                ignore_if_not_exists: false,
            },
        )
        .await
        .unwrap();
    root_worktree.flush_fs_events(cx).await;
    cx.run_until_parked();

    project.read_with(cx, |project, cx| {
        let project_config_store = project.project_config_store().read(cx);

        assert_eq!(project_config_store.project_file(), None);
        assert_eq!(
            project_config_store.environments().collect::<Vec<_>>(),
            vec![]
        );
        assert_eq!(
            project_config_store.folder_file(rel_path("users")),
            Some(&FolderFile {
                meta: ConfigFileMeta {
                    version: SCHEMA_VERSION,
                },
                request: RequestSection {
                    variables: vec![Variable {
                        name: "user_id".to_string(),
                        value: "3".to_string(),
                        disabled: false,
                    }],
                },
            })
        );
    });
}

#[gpui::test]
async fn test_invalid_config_file(cx: &mut TestAppContext) {
    cx.executor().allow_parking();

    let temp_fs = TempFs::new(cx.executor());
    temp_fs.insert_tree(
        path!("project"),
        json!({
            ".zaku": {
                "environments": {
                    "prod.toml": indoc! {r#"
                        [meta]
                        version = 1

                        [environment]
                        variables = [{ name = "base_url", value = "https://api.zaku.dev" }]
                    "#},
                },
            },
        }),
    );

    let project_path = temp_fs.path().join(path!("project"));
    let project = Project::test_new(temp_fs.clone(), &project_path, cx).await;
    let root_worktree = project.update(cx, |project, cx| project.root_worktree(cx).unwrap());
    cx.run_until_parked();

    let toast_events = Rc::new(RefCell::new(Vec::new()));
    project.update(cx, |_, cx| {
        let toast_events = toast_events.clone();
        cx.subscribe(&project, move |_, _, event, _| match event {
            ProjectEvent::Toast {
                notification_id,
                message,
            } => {
                toast_events
                    .borrow_mut()
                    .push((notification_id.clone(), Some(message.clone())));
            }
            ProjectEvent::HideToast { notification_id } => {
                toast_events
                    .borrow_mut()
                    .push((notification_id.clone(), None));
            }
            _ => {}
        })
        .detach();
    });

    temp_fs
        .write(
            &project_path.join(path!(".zaku/environments/prod.toml")),
            b"[environment",
        )
        .await
        .unwrap();
    root_worktree.flush_fs_events(cx).await;
    cx.run_until_parked();

    project.read_with(cx, |project, cx| {
        assert_eq!(
            project
                .project_config_store()
                .read(cx)
                .environments()
                .collect::<Vec<_>>(),
            vec![(
                "prod",
                &EnvironmentFile {
                    meta: ConfigFileMeta {
                        version: SCHEMA_VERSION,
                    },
                    environment: EnvironmentSection {
                        color: None,
                        variables: vec![Variable {
                            name: "base_url".to_string(),
                            value: "https://api.zaku.dev".to_string(),
                            disabled: false,
                        }],
                    },
                }
            )]
        );
    });
    let (notification_id, message) = toast_events.borrow().last().cloned().unwrap();
    assert_eq!(
        message,
        Some(format!(
            "Failed to parse environment config file {}:\nunclosed table, expected `]` at line 1 column 13",
            rel_path(".zaku/environments/prod.toml").display(PathStyle::local())
        ))
    );

    temp_fs
        .write(
            &project_path.join(path!(".zaku/environments/prod.toml")),
            indoc! {br#"
                [meta]
                version = 1

                [environment]
                variables = [
                  { name = "base_url", value = "https://api.zaku.dev" },
                  { name = "channel", value = "stable" }
                ]
            "#},
        )
        .await
        .unwrap();
    root_worktree.flush_fs_events(cx).await;
    cx.run_until_parked();

    project.read_with(cx, |project, cx| {
        assert_eq!(
            project
                .project_config_store()
                .read(cx)
                .environments()
                .collect::<Vec<_>>(),
            vec![(
                "prod",
                &EnvironmentFile {
                    meta: ConfigFileMeta {
                        version: SCHEMA_VERSION,
                    },
                    environment: EnvironmentSection {
                        color: None,
                        variables: vec![
                            Variable {
                                name: "base_url".to_string(),
                                value: "https://api.zaku.dev".to_string(),
                                disabled: false,
                            },
                            Variable {
                                name: "channel".to_string(),
                                value: "stable".to_string(),
                                disabled: false,
                            },
                        ],
                    },
                }
            )]
        );
    });
    assert_eq!(toast_events.borrow().last(), Some(&(notification_id, None)));
}

#[gpui::test]
async fn test_request_variables(cx: &mut TestAppContext) {
    cx.executor().allow_parking();

    let temp_fs = TempFs::new(cx.executor());
    temp_fs.insert_tree(
        path!("project"),
        json!({
            ".zaku": {
                "project.toml": indoc! {r#"
                    [meta]
                    version = 1

                    [request]
                    variables = [{ name = "base_url", value = "https://api.zaku.dev" }]
                "#},
                "environments": {
                    "dev.toml": indoc! {r#"
                        [meta]
                        version = 1

                        [environment]
                        variables = [{ name = "base_url", value = "http://localhost:8000" }]
                    "#},
                },
            },
            "users": {
                ".zaku": {
                    "folder.toml": indoc! {r#"
                        [meta]
                        version = 1

                        [request]
                        variables = [{ name = "user_id", value = "1" }]
                    "#},
                },
                "admins": {
                    ".zaku": {
                        "folder.toml": indoc! {r#"
                            [meta]
                            version = 1

                            [request]
                            variables = [
                              { name = "user_id", value = "2" },
                              { name = "base_url", value = "http://localhost:3000", disabled = true }
                            ]
                        "#},
                    },
                    "baz.toml": "",
                },
                "bar.toml": "",
            },
        }),
    );

    let project_path = temp_fs.path().join(path!("project"));
    let project = Project::test_new(temp_fs.clone(), &project_path, cx).await;
    let root_worktree = project.update(cx, |project, cx| project.root_worktree(cx).unwrap());
    let worktree_id = root_worktree.read_with(cx, |worktree, _| worktree.id());
    cx.run_until_parked();

    project.read_with(cx, |project, cx| {
        assert_eq!(
            project
                .project_config_store()
                .read(cx)
                .variables_for_request(&ProjectPath {
                    worktree_id,
                    path: Arc::from(rel_path("users/bar.toml")),
                })
                .iter()
                .map(|(name, value)| (name.as_str(), value.as_str()))
                .collect::<BTreeMap<_, _>>(),
            BTreeMap::from_iter([("base_url", "https://api.zaku.dev"), ("user_id", "1")])
        );
    });

    project.update(cx, |project, cx| {
        project
            .project_config_store()
            .update(cx, |project_config_store, cx| {
                project_config_store.activate_environment(Some("dev".to_string()), cx);
            });
    });
    project.read_with(cx, |project, cx| {
        assert_eq!(
            project
                .project_config_store()
                .read(cx)
                .variables_for_request(&ProjectPath {
                    worktree_id,
                    path: Arc::from(rel_path("users/admins/baz.toml")),
                })
                .iter()
                .map(|(name, value)| (name.as_str(), value.as_str()))
                .collect::<BTreeMap<_, _>>(),
            BTreeMap::from_iter([("base_url", "http://localhost:8000"), ("user_id", "2")])
        );
    });

    temp_fs
        .remove_file(
            &project_path.join(path!(".zaku/environments/dev.toml")),
            RemoveOptions::default(),
        )
        .await
        .unwrap();
    root_worktree.flush_fs_events(cx).await;
    cx.run_until_parked();

    project.read_with(cx, |project, cx| {
        assert_eq!(
            project
                .project_config_store()
                .read(cx)
                .variables_for_request(&ProjectPath {
                    worktree_id,
                    path: Arc::from(rel_path("users/admins/baz.toml")),
                })
                .iter()
                .map(|(name, value)| (name.as_str(), value.as_str()))
                .collect::<BTreeMap<_, _>>(),
            BTreeMap::from_iter([("base_url", "https://api.zaku.dev"), ("user_id", "2")])
        );
    });
}

#[gpui::test]
async fn test_open_buffer_at_uses_hidden_worktree_for_external_file(cx: &mut TestAppContext) {
    cx.executor().allow_parking();

    let temp_fs = TempFs::new(cx.executor());
    temp_fs.insert_tree(path!("project"), Value::default());
    let settings_content = indoc! {r#"
        {
          "ui": { "font_size": 14 },
          "editor": { "font_size": 12 }
        }
    "#};
    temp_fs.insert_tree(path!("settings.jsonc"), json!(settings_content));

    let project_path = temp_fs.path().join("project");
    let settings_path = temp_fs.path().join("settings.jsonc");

    let languages = Arc::new(LanguageRegistry::test_new(cx.executor()));
    let project = cx
        .update(|cx| Project::open(temp_fs.clone(), languages.clone(), project_path.clone(), cx))
        .await
        .expect("project open should succeed");

    project
        .read_with(cx, |project, cx| project.wait_for_initial_scan(cx))
        .await;

    let buffer = project
        .update(cx, |project, cx| project.open_buffer_at(&settings_path, cx))
        .await
        .expect("external file should open");

    let (buffer_text, opened_path) = buffer.read_with(cx, |buffer, cx| {
        (
            buffer.as_rope().to_string(),
            buffer.project_path(cx).expect("buffer should have a path"),
        )
    });
    let hidden_worktree = cx.update(|cx| {
        project
            .read(cx)
            .worktree_for_id(opened_path.worktree_id, cx)
            .expect("hidden worktree should exist")
    });

    assert_eq!(buffer_text, settings_content);
    assert_eq!(
        cx.update(|cx| project.read(cx).root(cx)),
        Some(project_path)
    );
    assert!(opened_path.path.is_empty());
    assert!(!hidden_worktree.read_with(cx, |worktree, _| worktree.is_visible()));
}

#[gpui::test]
async fn test_find_or_create_worktree_replaces_existing_worktree(cx: &mut TestAppContext) {
    cx.executor().allow_parking();

    let temp_fs = TempFs::new(cx.executor());
    temp_fs.insert_tree(path!("first"), Value::default());
    temp_fs.insert_tree(path!("second"), Value::default());

    let first_path = temp_fs.path().join("first");
    let second_path = temp_fs.path().join("second");

    let languages = Arc::new(LanguageRegistry::test_new(cx.executor()));
    let project = cx
        .update(|cx| Project::open(temp_fs.clone(), languages.clone(), first_path.clone(), cx))
        .await
        .expect("project open should succeed");

    project
        .read_with(cx, |project, cx| project.wait_for_initial_scan(cx))
        .await;

    let first_worktree = cx.update(|cx| project.read(cx).root_worktree(cx)).unwrap();
    let first_worktree_id = first_worktree.read_with(cx, |worktree, _| worktree.id());

    let (second_worktree, _) = project
        .update(cx, |project, cx| {
            project.find_or_create_worktree(&second_path, true, cx)
        })
        .await
        .expect("second project open should succeed");

    project
        .read_with(cx, |project, cx| project.wait_for_initial_scan(cx))
        .await;

    assert_ne!(first_worktree.entity_id(), second_worktree.entity_id());
    assert_eq!(cx.update(|cx| project.read(cx).root(cx)), Some(second_path));

    let current_worktree_id = cx.update(|cx| {
        project
            .read(cx)
            .root_worktree(cx)
            .map(|worktree| worktree.read(cx).id())
    });
    assert_ne!(current_worktree_id, Some(first_worktree_id));
}

#[gpui::test]
async fn test_find_or_create_worktree_reuses_existing_worktree_for_equivalent_canonicalized_path(
    cx: &mut TestAppContext,
) {
    cx.executor().allow_parking();

    let temp_fs = TempFs::new(cx.executor());
    temp_fs.insert_tree(path!("project"), Value::default());

    let canonical_project_path = temp_fs.path().join("project");
    let alternate_project_path = canonical_project_path.join("..").join("project");

    let languages = Arc::new(LanguageRegistry::test_new(cx.executor()));
    let project = cx
        .update(|cx| {
            Project::open(
                temp_fs.clone(),
                languages.clone(),
                canonical_project_path.clone(),
                cx,
            )
        })
        .await
        .expect("project open should succeed");

    project
        .read_with(cx, |project, cx| project.wait_for_initial_scan(cx))
        .await;

    let first_worktree = cx.update(|cx| project.read(cx).root_worktree(cx)).unwrap();
    let (second_worktree, _) = project
        .update(cx, |project, cx| {
            project.find_or_create_worktree(&alternate_project_path, true, cx)
        })
        .await
        .expect("canonicalized project open should reuse the current worktree");

    assert_eq!(first_worktree.entity_id(), second_worktree.entity_id());
    assert_eq!(
        cx.update(|cx| project.read(cx).root(cx)),
        Some(canonical_project_path)
    );
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
#[gpui::test]
async fn test_find_or_create_worktree_reuses_existing_worktree_for_equivalent_symlinked_path(
    cx: &mut TestAppContext,
) {
    cx.executor().allow_parking();

    let temp_fs = TempFs::new(cx.executor());
    temp_fs.insert_tree(path!("project"), Value::default());

    let project_path = temp_fs.path().join("project");
    let alias_project_path = temp_fs.path().join("project-alias");

    temp_fs
        .create_symlink(&alias_project_path, project_path.clone())
        .await
        .unwrap();

    let languages = Arc::new(LanguageRegistry::test_new(cx.executor()));
    let project = cx
        .update(|cx| {
            Project::open(
                temp_fs.clone(),
                languages.clone(),
                alias_project_path.clone(),
                cx,
            )
        })
        .await
        .expect("project open should succeed");

    project
        .read_with(cx, |project, cx| project.wait_for_initial_scan(cx))
        .await;

    let first_worktree = cx.update(|cx| project.read(cx).root_worktree(cx)).unwrap();

    assert_eq!(
        cx.update(|cx| project.read(cx).root(cx)),
        Some(project_path.clone())
    );

    let (second_worktree, _) = project
        .update(cx, |project, cx| {
            project.find_or_create_worktree(&project_path, true, cx)
        })
        .await
        .expect("second project open should succeed");

    assert_eq!(first_worktree.entity_id(), second_worktree.entity_id());
    assert_eq!(
        cx.update(|cx| project.read(cx).root(cx)),
        Some(project_path)
    );
}

#[gpui::test]
async fn test_find_or_create_worktree_replaces_config_files(cx: &mut TestAppContext) {
    cx.executor().allow_parking();

    let temp_fs = TempFs::new(cx.executor());
    temp_fs.insert_tree(
        path!("first"),
        json!({
            ".zaku": {
                "project.toml": indoc! {r#"
                    [meta]
                    version = 1

                    [request]
                    variables = [{ name = "base_url", value = "https://api.zaku.dev" }]
                "#},
                "environments": {
                    "dev.toml": indoc! {r#"
                        [meta]
                        version = 1

                        [environment]
                        variables = [{ name = "base_url", value = "http://localhost:8000" }]
                    "#},
                },
            },
        }),
    );
    temp_fs.insert_tree(
        path!("second"),
        json!({
            ".zaku": {
                "project.toml": indoc! {r#"
                    [meta]
                    version = 1

                    [request]
                    variables = [{ name = "base_url", value = "http://localhost:4321" }]
                "#},
            },
        }),
    );

    let project = Project::test_new(temp_fs.clone(), &temp_fs.path().join("first"), cx).await;
    cx.run_until_parked();

    project.read_with(cx, |project, cx| {
        let project_config_store = project.project_config_store().read(cx);

        assert_eq!(
            project_config_store.project_file(),
            Some(&ProjectFile {
                meta: ConfigFileMeta {
                    version: SCHEMA_VERSION,
                },
                request: RequestSection {
                    variables: vec![Variable {
                        name: "base_url".to_string(),
                        value: "https://api.zaku.dev".to_string(),
                        disabled: false,
                    }],
                },
            })
        );
        assert_eq!(
            project_config_store.environments().collect::<Vec<_>>(),
            vec![(
                "dev",
                &EnvironmentFile {
                    meta: ConfigFileMeta {
                        version: SCHEMA_VERSION,
                    },
                    environment: EnvironmentSection {
                        color: None,
                        variables: vec![Variable {
                            name: "base_url".to_string(),
                            value: "http://localhost:8000".to_string(),
                            disabled: false,
                        }],
                    },
                }
            )]
        );
    });

    project
        .update(cx, |project, cx| {
            project.find_or_create_worktree(temp_fs.path().join("second"), true, cx)
        })
        .await
        .unwrap();
    project
        .read_with(cx, |project, cx| project.wait_for_initial_scan(cx))
        .await;
    cx.run_until_parked();

    project.read_with(cx, |project, cx| {
        let project_config_store = project.project_config_store().read(cx);

        assert_eq!(
            project_config_store.project_file(),
            Some(&ProjectFile {
                meta: ConfigFileMeta {
                    version: SCHEMA_VERSION,
                },
                request: RequestSection {
                    variables: vec![Variable {
                        name: "base_url".to_string(),
                        value: "http://localhost:4321".to_string(),
                        disabled: false,
                    }],
                },
            })
        );
        assert_eq!(
            project_config_store.environments().collect::<Vec<_>>(),
            vec![]
        );
    });
}

#[gpui::test]
async fn test_absolute_path_resolves_relative_paths_against_current_root(cx: &mut TestAppContext) {
    cx.executor().allow_parking();

    let temp_fs = TempFs::new(cx.executor());
    temp_fs.insert_tree(
        path!("project"),
        json!({
            "nested": {
                "request.toml": indoc! {"
                    [meta]
                    version = 1
                "}
            }
        }),
    );

    let project_path = temp_fs.path().join("project");
    let request_path = project_path.join("nested").join("request.toml");
    let project = Project::test_new(temp_fs.clone(), &project_path, cx).await;

    let (resolved_request_path, resolved_project_path) = cx.update(|cx| {
        let project = project.read(cx);
        (
            project.absolutize(rel_path("nested/request.toml"), cx),
            project.absolutize(RelPath::empty(), cx),
        )
    });

    assert_eq!(resolved_request_path, Some(request_path));
    assert_eq!(resolved_project_path, Some(project_path));
}

#[gpui::test]
async fn test_initial_scan_complete(cx: &mut TestAppContext) {
    cx.executor().allow_parking();

    let temp_fs = TempFs::new(cx.executor());
    temp_fs.insert_tree(path!("project"), Value::default());

    let project_path = temp_fs.path().join("project");
    let project = Project::test_new(temp_fs.clone(), &project_path, cx).await;

    project
        .read_with(cx, |project, cx| project.wait_for_initial_scan(cx))
        .await;

    project.read_with(cx, |project, cx| {
        assert!(
            project.worktree_store().read(cx).initial_scan_completed(),
            "expected initial scan to be completed after awaiting wait_for_initial_scan"
        );
    });
}

#[gpui::test(iterations = 10)]
async fn test_buffer_identity_across_renames(cx: &mut TestAppContext) {
    cx.executor().allow_parking();

    let temp_fs = TempFs::new(cx.executor());
    temp_fs.insert_tree(
        path!("project"),
        json!({
            "folder": {
                "request.toml": indoc! {r#"
                    [meta]
                    version = 1

                    [http]
                    method = "GET"
                    url = "https://api.zaku.dev/me"
                "#}
            }
        }),
    );

    let project_path = temp_fs.path().join(path!("project"));
    let project = Project::test_new(temp_fs, &project_path, cx).await;
    let worktree = project.update(cx, |project, cx| project.root_worktree(cx).unwrap());
    let worktree_id = worktree.update(cx, |worktree, _| worktree.id());

    let entry_id_for_path = |path: &'static str, cx: &mut TestAppContext| {
        project.update(cx, |project, cx| {
            let worktree = project.root_worktree(cx).unwrap();
            worktree.read(cx).entry_for_path(rel_path(path)).unwrap().id
        })
    };

    let folder_id = entry_id_for_path("folder", cx);
    let request_entry_id = entry_id_for_path("folder/request.toml", cx);
    let buffer = cx
        .update(|cx| {
            <RequestBuffer as ProjectItem>::try_open(
                &project,
                &(worktree_id, rel_path("folder/request.toml")).into(),
                cx,
            )
            .unwrap()
        })
        .await
        .unwrap();
    buffer.update(cx, |buffer, _| assert!(!buffer.is_dirty()));

    let received_file_handle_changed = Rc::new(RefCell::new(false));
    buffer.update(cx, |_, cx| {
        let received_file_handle_changed = received_file_handle_changed.clone();
        cx.subscribe(&buffer, move |_, _, event, _| {
            if matches!(event, RequestBufferEvent::FileHandleChanged) {
                *received_file_handle_changed.borrow_mut() = true;
            }
        })
        .detach();
    });

    project
        .update(cx, |project, cx| {
            project.rename_entry(folder_id, (worktree_id, rel_path("renamed")).into(), cx)
        })
        .await
        .unwrap();
    cx.run_until_parked();
    worktree.flush_fs_events(cx).await;

    assert_eq!(entry_id_for_path("renamed", cx), folder_id);
    assert_eq!(
        entry_id_for_path("renamed/request.toml", cx),
        request_entry_id
    );
    assert!(
        *received_file_handle_changed.borrow(),
        "RequestBufferEvent::FileHandleChanged must be emitted when the open request is moved by a parent rename"
    );
    buffer.update(cx, |buffer, _| {
        assert!(!buffer.is_dirty());
        assert_eq!(
            buffer.file().path.as_ref(),
            rel_path("renamed/request.toml")
        );
    });
}

#[gpui::test]
async fn test_edit_request_buffer_while_it_reloads(cx: &mut TestAppContext) {
    cx.executor().allow_parking();

    let temp_fs = TempFs::new(cx.executor());
    temp_fs.insert_tree(
        path!("project"),
        json!({
            "users": {
                "get-user.toml": indoc! {r#"
                    [meta]
                    version = 1

                    [http]
                    method = "GET"
                    url = "https://api.zaku.dev/users/1"
                "#}
            }
        }),
    );

    let project_path = temp_fs.path().join(path!("project"));
    let project = Project::test_new(temp_fs.clone(), &project_path, cx).await;
    let worktree = project.update(cx, |project, cx| project.root_worktree(cx).unwrap());
    let worktree_id = worktree.update(cx, |worktree, _| worktree.id());
    let buffer = cx
        .update(|cx| {
            <RequestBuffer as ProjectItem>::try_open(
                &project,
                &(worktree_id, rel_path("users/get-user.toml")).into(),
                cx,
            )
            .unwrap()
        })
        .await
        .unwrap();

    temp_fs
        .write(
            &project_path.join(path!("users/get-user.toml")),
            indoc! {br#"
                [meta]
                version = 1

                [http]
                method = "GET"
                url = "https://api.zaku.dev/users/me"
            "#},
        )
        .await
        .unwrap();

    let reload_task = project.update(cx, |project, cx| project.reload_request_buffer(&buffer, cx));
    let edited_request = RequestFileState::Parsed(RequestFile {
        meta: RequestFileMeta {
            version: SCHEMA_VERSION,
        },
        http: RequestFileHttp {
            method: "GET".to_string(),
            url: "https://api.zaku.dev/users/2".to_string(),
            params: vec![],
            headers: vec![],
            body: None,
        },
    });
    buffer.update(cx, |buffer, cx| {
        buffer.set_request_file(edited_request.clone(), cx);
        buffer.set_dirty(true, cx);
    });
    reload_task.await.unwrap();

    buffer.update(cx, |buffer, _| {
        assert_eq!(buffer.request_file(), &edited_request);
        assert!(buffer.is_dirty());
        assert!(buffer.has_conflict());
    });
}

#[gpui::test]
async fn test_edit_request_buffer_while_it_saves(cx: &mut TestAppContext) {
    cx.executor().allow_parking();

    let temp_fs = TempFs::new(cx.executor());
    temp_fs.insert_tree(
        path!("project"),
        json!({
            "users": {
                "get-user.toml": indoc! {r#"
                    [meta]
                    version = 1

                    [http]
                    method = "GET"
                    url = "https://api.zaku.dev/users/1"
                "#}
            }
        }),
    );

    let project_path = temp_fs.path().join(path!("project"));
    let project = Project::test_new(temp_fs.clone(), &project_path, cx).await;
    let worktree = project.update(cx, |project, cx| project.root_worktree(cx).unwrap());
    let worktree_id = worktree.update(cx, |worktree, _| worktree.id());
    let buffer = cx
        .update(|cx| {
            <RequestBuffer as ProjectItem>::try_open(
                &project,
                &(worktree_id, rel_path("users/get-user.toml")).into(),
                cx,
            )
            .unwrap()
        })
        .await
        .unwrap();

    buffer.update(cx, |buffer, cx| {
        buffer.set_request_file(
            RequestFileState::Parsed(RequestFile {
                meta: RequestFileMeta {
                    version: SCHEMA_VERSION,
                },
                http: RequestFileHttp {
                    method: "GET".to_string(),
                    url: "https://api.zaku.dev/users/2".to_string(),
                    params: vec![],
                    headers: vec![],
                    body: None,
                },
            }),
            cx,
        );
        buffer.set_dirty(true, cx);
    });

    let save_task = project.update(cx, |project, cx| project.save_request_buffer(&buffer, cx));
    buffer.update(cx, |buffer, cx| {
        buffer.set_request_file(
            RequestFileState::Parsed(RequestFile {
                meta: RequestFileMeta {
                    version: SCHEMA_VERSION,
                },
                http: RequestFileHttp {
                    method: "GET".to_string(),
                    url: "https://api.zaku.dev/users/3".to_string(),
                    params: vec![],
                    headers: vec![],
                    body: None,
                },
            }),
            cx,
        );
        buffer.set_dirty(true, cx);
    });
    save_task.await.unwrap();

    buffer.update(cx, |buffer, _| {
        assert!(buffer.is_dirty());
        assert!(!buffer.has_conflict());
    });
    assert_eq!(
        temp_fs
            .load("project/users/get-user.toml".as_ref())
            .await
            .unwrap(),
        indoc! {r#"
            [meta]
            version = 1

            [http]
            method = "GET"
            url = "https://api.zaku.dev/users/2"
        "#}
    );
}
