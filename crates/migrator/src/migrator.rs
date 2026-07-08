mod migrations;

use anyhow::Context;
use jsonc_parser::cst::CstRootNode;

use settings_content::JSONC_PARSE_OPTIONS;

#[derive(Debug, Clone, Copy)]
pub enum Migration {
    MoveProperty {
        from: &'static str,
        to: &'static str,
    },
    RemoveProperty {
        path: &'static str,
    },
    RenameKeymapAction {
        from: &'static str,
        to: &'static str,
    },
    RenameKeymapContextPredicate {
        from: &'static str,
        to: &'static str,
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
            Self::RenameKeymapAction { from, to } => rename_keymap_action(content, from, to),
            Self::RenameKeymapContextPredicate { from, to } => {
                rename_keymap_context_predicate(content, from, to)
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
            Self::RenameKeymapAction { from, to } => {
                assert!(
                    !from.is_empty(),
                    "keymap action migration source cannot be empty"
                );
                assert!(
                    !to.is_empty(),
                    "keymap action migration destination cannot be empty"
                );
                assert!(
                    from != to,
                    "keymap action migration source and destination cannot be the same"
                );
            }
            Self::RenameKeymapContextPredicate { from, to } => {
                assert!(
                    !from.is_empty(),
                    "keymap context predicate migration source cannot be empty"
                );
                assert!(
                    from.chars().all(is_context_identifier_char),
                    "keymap context predicate migration source must be an identifier"
                );
                assert!(
                    !to.is_empty(),
                    "keymap context predicate migration destination cannot be empty"
                );
                assert!(
                    to.chars().all(is_context_identifier_char),
                    "keymap context predicate migration destination must be an identifier"
                );
                assert!(
                    from != to,
                    "keymap context predicate migration source and destination cannot be the same"
                );
            }
        }
    }
}

pub fn migrate_settings(content: &str) -> anyhow::Result<Option<String>> {
    run_migrations(content, migrations::SETTINGS_MIGRATIONS)
}

pub fn migrate_keymap(content: &str) -> anyhow::Result<Option<String>> {
    run_migrations(content, migrations::KEYMAP_MIGRATIONS)
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

fn rename_keymap_action(content: &str, from: &str, to: &str) -> anyhow::Result<Option<String>> {
    let root_node = CstRootNode::parse(content, &JSONC_PARSE_OPTIONS)
        .context("keymap file could not be parsed; fix syntax errors before migrating")?;
    let Some(sections) = root_node.value().and_then(|value| value.as_array()) else {
        return Ok(None);
    };

    let mut did_migrate = false;
    for section in sections.elements() {
        let Some(section) = section.as_object() else {
            continue;
        };

        for section_key in ["bindings", "unbind"] {
            let Some(actions) = section
                .get(section_key)
                .and_then(|property| property.value())
                .and_then(|value| value.as_object())
            else {
                continue;
            };

            for action in actions.properties() {
                let Some(action_value) = action.value() else {
                    continue;
                };
                let action_name = action_value.as_string_lit().or_else(|| {
                    action_value.as_array().and_then(|action_array| {
                        action_array
                            .elements()
                            .into_iter()
                            .next()
                            .and_then(|element| element.as_string_lit())
                    })
                });
                let Some(action_name) = action_name else {
                    continue;
                };

                let Ok(value) = action_name.decoded_value() else {
                    continue;
                };
                if value != from {
                    continue;
                }

                action_name.set_raw_value(serde_json::to_string(to)?);
                did_migrate = true;
            }
        }
    }

    if !did_migrate {
        return Ok(None);
    }

    let new_content = root_node.to_string();
    if new_content == content {
        return Ok(None);
    }

    Ok(Some(new_content))
}

fn rename_keymap_context_predicate(
    content: &str,
    from: &str,
    to: &str,
) -> anyhow::Result<Option<String>> {
    let root_node = CstRootNode::parse(content, &JSONC_PARSE_OPTIONS)
        .context("keymap file could not be parsed; fix syntax errors before migrating")?;
    let Some(sections) = root_node.value().and_then(|value| value.as_array()) else {
        return Ok(None);
    };

    let mut did_migrate = false;
    for section in sections.elements() {
        let Some(section) = section.as_object() else {
            continue;
        };
        let Some(context) = section
            .get("context")
            .and_then(|property| property.value())
            .and_then(|value| value.as_string_lit())
        else {
            continue;
        };

        let Ok(old_value) = context.decoded_value() else {
            continue;
        };
        let Some(new_value) = rename_context_identifier(&old_value, from, to) else {
            continue;
        };

        context.set_raw_value(serde_json::to_string(&new_value)?);
        did_migrate = true;
    }

    if !did_migrate {
        return Ok(None);
    }

    let new_content = root_node.to_string();
    if new_content == content {
        return Ok(None);
    }

    Ok(Some(new_content))
}

fn rename_context_identifier(value: &str, from: &str, to: &str) -> Option<String> {
    if from.is_empty() {
        return None;
    }

    let mut new_value = String::with_capacity(value.len());
    let mut remaining_value = value;
    let mut did_rename = false;

    while let Some(start) = remaining_value.find(from) {
        let end = start + from.len();
        let before_match = remaining_value.get(..start)?;
        let after_match = remaining_value.get(end..)?;
        let is_inside_identifier = before_match
            .chars()
            .next_back()
            .is_some_and(is_context_identifier_char)
            || after_match
                .chars()
                .next()
                .is_some_and(is_context_identifier_char);

        new_value.push_str(before_match);
        if is_inside_identifier {
            new_value.push_str(from);
        } else {
            new_value.push_str(to);
            did_rename = true;
        }

        remaining_value = after_match;
    }

    if !did_rename {
        return None;
    }

    new_value.push_str(remaining_value);
    Some(new_value)
}

fn is_context_identifier_char(character: char) -> bool {
    character.is_alphanumeric() || character == '_' || character == '-'
}

#[cfg(test)]
mod tests {
    use super::*;

    use indoc::indoc;

    #[track_caller]
    fn assert_migration(
        migrations: &[Migration],
        content: &str,
        expected: Option<&str>,
    ) -> Option<String> {
        let migrated = run_migrations(content, migrations).unwrap();

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

    #[test]
    fn test_rename_keymap_action() {
        let migrations = &[Migration::RenameKeymapAction {
            from: "workspace::OldAction",
            to: "workspace::NewAction",
        }];
        let content = indoc! {r#"
            [
              {
                "unbind": {
                  "cmd-a": "workspace::OldAction",
                  "cmd-b": ["workspace::OldAction", { "value": true }]
                },
                "bindings": {
                  "cmd-c": "workspace::OldAction",
                  "cmd-d": ["workspace::OldAction", { "value": true }],
                  "cmd-e": "workspace::OtherAction",
                  "cmd-f": null
                }
              }
            ]
        "#};

        assert_migration(
            migrations,
            content,
            Some(indoc! {r#"
                [
                  {
                    "unbind": {
                      "cmd-a": "workspace::NewAction",
                      "cmd-b": ["workspace::NewAction", { "value": true }]
                    },
                    "bindings": {
                      "cmd-c": "workspace::NewAction",
                      "cmd-d": ["workspace::NewAction", { "value": true }],
                      "cmd-e": "workspace::OtherAction",
                      "cmd-f": null
                    }
                  }
                ]
            "#}),
        );
    }

    #[test]
    fn test_rename_keymap_context_predicate() {
        let migrations = &[Migration::RenameKeymapContextPredicate {
            from: "old_context",
            to: "new_context",
        }];
        let content = indoc! {r#"
            [
              {
                "context": "Workspace && old_context && !old_context_disabled",
                "bindings": {
                  "cmd-a": "workspace::Action"
                }
              }
            ]
        "#};

        assert_migration(
            migrations,
            content,
            Some(indoc! {r#"
                [
                  {
                    "context": "Workspace && new_context && !old_context_disabled",
                    "bindings": {
                      "cmd-a": "workspace::Action"
                    }
                  }
                ]
            "#}),
        );
    }

    #[test]
    fn test_keymap_migrations_run_sequentially() {
        let migrations = &[
            Migration::RenameKeymapAction {
                from: "workspace::FirstAction",
                to: "workspace::SecondAction",
            },
            Migration::RenameKeymapAction {
                from: "workspace::SecondAction",
                to: "workspace::ThirdAction",
            },
        ];
        let content = indoc! {r#"
            [
              {
                "bindings": {
                  "cmd-a": "workspace::FirstAction"
                }
              }
            ]
        "#};

        assert_migration(
            migrations,
            content,
            Some(indoc! {r#"
                [
                  {
                    "bindings": {
                      "cmd-a": "workspace::ThirdAction"
                    }
                  }
                ]
            "#}),
        );
    }

    #[test]
    fn test_keymap_migration_preserves_comments() {
        let migrations = &[
            Migration::RenameKeymapContextPredicate {
                from: "old_context",
                to: "new_context",
            },
            Migration::RenameKeymapAction {
                from: "workspace::OldAction",
                to: "workspace::NewAction",
            },
        ];
        let content = indoc! {r#"
            [
              // Line comment.
              {
                "context": "Workspace && old_context", // Trailing comment.
                /*
                 * Block comment.
                 */
                "bindings": {
                  "cmd-a": "workspace::OldAction"
                }
              }
            ]
        "#};

        assert_migration(
            migrations,
            content,
            Some(indoc! {r#"
                [
                  // Line comment.
                  {
                    "context": "Workspace && new_context", // Trailing comment.
                    /*
                     * Block comment.
                     */
                    "bindings": {
                      "cmd-a": "workspace::NewAction"
                    }
                  }
                ]
            "#}),
        );
    }
}
