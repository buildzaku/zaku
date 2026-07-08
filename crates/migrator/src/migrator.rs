mod migrations;

#[derive(Debug, Clone, Copy)]
pub enum Migration {
    MoveProperty {
        from: &'static str,
        to: &'static str,
    },
    RemoveProperty {
        path: &'static str,
    },
}

impl Migration {
    fn apply(&self, content: &str) -> anyhow::Result<Option<String>> {
        match *self {
            Self::MoveProperty { from, to } => {
                settings_jsonc::move_property_at_jsonc_path(content, from, to)
            }
            Self::RemoveProperty { path } => {
                settings_jsonc::remove_property_at_jsonc_path(content, path)
            }
        }
    }

    fn validate(&self) {
        match *self {
            Self::MoveProperty { from, to } => {
                let from_segments = path_segments(from, "migration source path");
                let to_segments = path_segments(to, "migration destination path");

                assert!(
                    from_segments != to_segments,
                    "migration source and destination cannot be the same path"
                );
                assert!(
                    !to_segments.starts_with(&from_segments),
                    "migration destination path cannot be inside source path"
                );
                assert!(
                    !from_segments.starts_with(&to_segments),
                    "migration source path cannot be inside destination path"
                );
            }
            Self::RemoveProperty { path } => {
                path_segments(path, "migration path");
            }
        }
    }
}

pub fn migrate_settings(content: &str) -> anyhow::Result<Option<String>> {
    run_migrations(content, migrations::SETTINGS_MIGRATIONS)
}

fn run_migrations(content: &str, migrations: &[Migration]) -> anyhow::Result<Option<String>> {
    if content.trim().is_empty() {
        return Ok(None);
    }

    let mut current_content = content.to_string();
    let mut did_migrate = false;

    for migration in migrations {
        migration.validate();
    }

    for migration in migrations {
        let migrated_content = migration.apply(&current_content)?;

        if let Some(migrated_content) = migrated_content {
            current_content = migrated_content;
            did_migrate = true;
        }
    }

    if !did_migrate || current_content == content {
        return Ok(None);
    }

    Ok(Some(current_content))
}

fn path_segments<'a>(path: &'a str, label: &str) -> Vec<&'a str> {
    assert!(!path.is_empty(), "{label} cannot be empty");

    let segments = path.split('.').collect::<Vec<_>>();
    assert!(
        segments.iter().all(|segment| !segment.is_empty()),
        "{label} is invalid: {path}"
    );

    segments
}

#[cfg(test)]
mod tests {
    use super::*;

    use indoc::indoc;

    #[track_caller]
    fn assert_migration(
        migrations: &[Migration],
        input: &str,
        expected: Option<&str>,
    ) -> Option<String> {
        let migrated = run_migrations(input, migrations).unwrap();

        match (migrated.as_deref(), expected) {
            (Some(migrated), Some(expected)) => {
                pretty_assertions::assert_str_eq!(expected, migrated);
            }
            _ => pretty_assertions::assert_eq!(migrated.as_deref(), expected),
        }

        if let Some(migrated) = migrated.as_deref() {
            let rerun = run_migrations(migrated, migrations).unwrap();
            pretty_assertions::assert_eq!(rerun.as_deref(), None);
        }

        migrated
    }

    #[test]
    fn test_move_migration() {
        let migrations = &[Migration::MoveProperty {
            from: "object_key",
            to: "object.key",
        }];
        let content = indoc! {r#"
            {
              "string": "text",
              "number": 999,
              "boolean": true,
              "object_key": "value"
            }
        "#};

        assert_migration(
            migrations,
            content,
            Some(indoc! {r#"
                {
                  "string": "text",
                  "number": 999,
                  "boolean": true,
                  "object": {
                    "key": "value"
                  }
                }
            "#}),
        );
    }

    #[test]
    fn test_remove_migration() {
        let migrations = &[Migration::RemoveProperty {
            path: "object.key1",
        }];
        let content = indoc! {r#"
            {
              "string": "text",
              "object": {
                "key1": "value1",
                "key2": "value2"
              },
              "boolean": true
            }
        "#};

        assert_migration(
            migrations,
            content,
            Some(indoc! {r#"
                {
                  "string": "text",
                  "object": {
                    "key2": "value2"
                  },
                  "boolean": true
                }
            "#}),
        );
    }

    #[test]
    fn test_move_migration_keeps_existing_destination() {
        let migrations = &[Migration::MoveProperty {
            from: "object_key",
            to: "object.key",
        }];
        let content = indoc! {r#"
            {
              "object_key": "old value",
              "object": {
                "key": "existing value"
              }
            }
        "#};

        assert_migration(
            migrations,
            content,
            Some(indoc! {r#"
                {
                  "object": {
                    "key": "existing value"
                  }
                }
            "#}),
        );
    }

    #[test]
    fn test_move_migration_ignores_missing_source() {
        let migrations = &[Migration::MoveProperty {
            from: "object_key",
            to: "object.key",
        }];
        let content = indoc! {r#"
            {
              "string": "text",
              "number": 999,
              "boolean": true
            }
        "#};

        assert_migration(migrations, content, None);
    }

    #[test]
    fn test_move_migration_destination_parent_must_be_object() {
        let migrations = &[Migration::MoveProperty {
            from: "boolean",
            to: "number.value",
        }];
        let content = indoc! {r#"
            {
              "number": 999,
              "boolean": true
            }
        "#};

        run_migrations(content, migrations).unwrap_err();
    }

    #[test]
    fn test_migrations_run_sequentially() {
        let migrations = &[
            Migration::MoveProperty {
                from: "object_key",
                to: "object.key",
            },
            Migration::MoveProperty {
                from: "object.key",
                to: "object.inner.key",
            },
        ];
        let content = indoc! {r#"
            {
              "object_key": "value"
            }
        "#};

        assert_migration(
            migrations,
            content,
            Some(indoc! {r#"
                {
                  "object": {
                    "inner": {
                      "key": "value"
                    }
                  }
                }
            "#}),
        );
    }

    #[test]
    fn test_move_migration_preserves_unrelated_comments() {
        let migrations = &[Migration::MoveProperty {
            from: "object_key",
            to: "object.key",
        }];
        let content = indoc! {r#"
            {
              // Line comment.
              "string": "text",
              "object_key": "value", // Trailing comment.
              /*
               * Block comment.
               */
              "number": 999,
              "boolean": true
            }
        "#};

        assert_migration(
            migrations,
            content,
            Some(indoc! {r#"
                {
                  // Line comment.
                  "string": "text",
                  /*
                   * Block comment.
                   */
                  "number": 999,
                  "boolean": true,
                  "object": {
                    "key": "value"
                  }
                }
            "#}),
        );
    }

    #[test]
    fn test_move_migration_source_and_destination_must_not_be_equal() {
        let result = std::panic::catch_unwind(|| {
            run_migrations(
                "{}",
                &[Migration::MoveProperty {
                    from: "object.key",
                    to: "object.key",
                }],
            )
            .unwrap();
        });

        assert!(result.is_err());
    }

    #[test]
    fn test_migration_path_must_not_be_empty() {
        for path in ["", "object..key"] {
            let result = std::panic::catch_unwind(|| {
                run_migrations("{}", &[Migration::RemoveProperty { path }]).unwrap();
            });

            assert!(result.is_err());
        }
    }

    #[test]
    fn test_move_migration_destination_must_not_be_descendant_of_source() {
        let result = std::panic::catch_unwind(|| {
            run_migrations(
                "{}",
                &[Migration::MoveProperty {
                    from: "object",
                    to: "object.key",
                }],
            )
            .unwrap();
        });

        assert!(result.is_err());
    }

    #[test]
    fn test_move_migration_source_must_not_be_descendant_of_destination() {
        let result = std::panic::catch_unwind(|| {
            run_migrations(
                "{}",
                &[Migration::MoveProperty {
                    from: "object.key.value",
                    to: "object.key",
                }],
            )
            .unwrap();
        });

        assert!(result.is_err());
    }
}
