use gpui::{
    Action, ActionBuildError, App, InvalidKeystrokeError, KEYSTROKE_PARSE_EXPECTED_MESSAGE,
    KeyBinding, KeyBindingContextPredicate, NoAction, SharedString, Unbind,
};
use indexmap::IndexMap;
use serde::Deserialize;
use serde_json::{Deserializer, Value};
use std::{fmt::Write, rc::Rc};

use crate::SettingsAssets;
use util::asset_str;

gpui::register_action!(ActionSequence);

pub struct ActionSequence(pub Vec<Box<dyn Action>>);

impl ActionSequence {
    fn build_sequence(value: Value, cx: &App) -> Result<Box<dyn Action>, ActionBuildError> {
        match value {
            Value::Array(values) => {
                let actions = values
                    .into_iter()
                    .enumerate()
                    .map(|(index, action)| {
                        match KeymapFile::build_keymap_action_from_value(&action, cx) {
                            Ok((action, _)) => Ok(action),
                            Err(error) => Err(ActionBuildError::BuildError {
                                name: Self::name_for_type().to_string(),
                                error: anyhow::anyhow!("Error at sequence index {index}: {error}"),
                            }),
                        }
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                Ok(Box::new(Self(actions)))
            }
            _ => Err(Self::expected_array_error()),
        }
    }

    fn expected_array_error() -> ActionBuildError {
        ActionBuildError::BuildError {
            name: Self::name_for_type().to_string(),
            error: anyhow::anyhow!("expected array of actions"),
        }
    }
}

impl Action for ActionSequence {
    fn name(&self) -> &'static str {
        Self::name_for_type()
    }

    fn name_for_type() -> &'static str
    where
        Self: Sized,
    {
        "action::Sequence"
    }

    fn partial_eq(&self, action: &dyn Action) -> bool {
        action.as_any().downcast_ref::<Self>().is_some_and(|other| {
            self.0.len() == other.0.len()
                && self
                    .0
                    .iter()
                    .zip(other.0.iter())
                    .all(|(left, right)| left.partial_eq(right.as_ref()))
        })
    }

    fn boxed_clone(&self) -> Box<dyn Action> {
        Box::new(Self(
            self.0
                .iter()
                .map(|action| action.boxed_clone())
                .collect::<Vec<_>>(),
        ))
    }

    fn build(_value: Value) -> anyhow::Result<Box<dyn Action>> {
        Err(anyhow::anyhow!(
            "{} cannot be built directly",
            Self::name_for_type()
        ))
    }
}

#[derive(Debug, Deserialize, Default, Clone)]
pub struct KeymapSection {
    #[serde(default)]
    pub context: String,
    #[serde(default)]
    use_key_equivalents: bool,
    #[serde(default)]
    unbind: Option<IndexMap<String, UnbindTargetAction>>,
    #[serde(default)]
    bindings: Option<IndexMap<String, KeymapAction>>,
    #[serde(flatten)]
    unrecognized_fields: IndexMap<String, Value>,
}

#[derive(Debug, Deserialize, Default, Clone)]
#[serde(transparent)]
pub struct KeymapAction(Value);

#[derive(Debug, Deserialize, Default, Clone)]
#[serde(transparent)]
pub struct UnbindTargetAction(Value);

#[derive(Debug)]
#[must_use]
pub enum KeymapFileLoadResult {
    Success {
        key_bindings: Vec<KeyBinding>,
    },
    SomeFailedToLoad {
        key_bindings: Vec<KeyBinding>,
        error_message: String,
    },
    JsonParseFailure {
        error: anyhow::Error,
    },
}

#[derive(Debug, Deserialize, Default, Clone)]
#[serde(transparent)]
pub struct KeymapFile(Vec<KeymapSection>);

impl KeymapFile {
    pub fn parse(content: &str) -> anyhow::Result<Self> {
        if content.trim().is_empty() {
            return Ok(Self(Vec::new()));
        }

        let mut deserializer = Deserializer::from_str(content);
        Ok(serde_path_to_error::deserialize(&mut deserializer)?)
    }

    pub fn load_asset(asset_path: &str, cx: &App) -> anyhow::Result<Vec<KeyBinding>> {
        match Self::load(asset_str::<SettingsAssets>(asset_path).as_ref(), cx) {
            KeymapFileLoadResult::Success { key_bindings } => Ok(key_bindings),
            KeymapFileLoadResult::SomeFailedToLoad { error_message, .. } => {
                anyhow::bail!("Error loading built-in keymap \"{asset_path}\": {error_message}");
            }
            KeymapFileLoadResult::JsonParseFailure { error } => {
                anyhow::bail!("JSON parse error in built-in keymap \"{asset_path}\": {error}");
            }
        }
    }

    pub fn load(content: &str, cx: &App) -> KeymapFileLoadResult {
        let keymap_file = match Self::parse(content) {
            Ok(keymap_file) => keymap_file,
            Err(error) => return KeymapFileLoadResult::JsonParseFailure { error },
        };

        let mut errors = Vec::new();
        let mut key_bindings = Vec::new();

        for section in &keymap_file.0 {
            let context_predicate = if section.context.is_empty() {
                None
            } else {
                match KeyBindingContextPredicate::parse(&section.context) {
                    Ok(context_predicate) => Some(Rc::new(context_predicate)),
                    Err(error) => {
                        errors.push((
                            section.context.clone(),
                            format!("Parse error in section context field: {error}"),
                        ));
                        continue;
                    }
                }
            };

            let mut section_errors = String::new();

            if !section.unrecognized_fields.is_empty() {
                let field_names = section
                    .unrecognized_fields
                    .keys()
                    .map(|field| format!("{field:?}"))
                    .collect::<Vec<_>>()
                    .join(", ");
                let _ = write!(section_errors, "\n- Unrecognized fields: {field_names}");
            }

            if let Some(unbind) = &section.unbind {
                for (keystrokes, action) in unbind {
                    match Self::load_unbinding(
                        keystrokes,
                        action,
                        context_predicate.clone(),
                        section.use_key_equivalents,
                        cx,
                    ) {
                        Ok(key_binding) => key_bindings.push(key_binding),
                        Err(error) => {
                            let _ = write!(section_errors, "\n- In unbind {keystrokes:?}, {error}");
                        }
                    }
                }
            }

            if let Some(bindings) = &section.bindings {
                for (keystrokes, action) in bindings {
                    match Self::load_keybinding_from_action_value(
                        keystrokes,
                        &action.0,
                        context_predicate.clone(),
                        section.use_key_equivalents,
                        cx,
                    ) {
                        Ok(key_binding) => key_bindings.push(key_binding),
                        Err(error) => {
                            let _ =
                                write!(section_errors, "\n- In binding {keystrokes:?}, {error}");
                        }
                    }
                }
            }

            if !section_errors.is_empty() {
                errors.push((section.context.clone(), section_errors));
            }
        }

        if errors.is_empty() {
            KeymapFileLoadResult::Success { key_bindings }
        } else {
            let mut error_message = String::from("Errors in user keymap file.");

            for (context, section_errors) in errors {
                if context.is_empty() {
                    let _ = write!(error_message, "\nIn section without context predicate:");
                } else {
                    let _ = write!(error_message, "\nIn section with context = {context:?}:");
                }
                let _ = write!(error_message, "{section_errors}");
            }

            KeymapFileLoadResult::SomeFailedToLoad {
                key_bindings,
                error_message,
            }
        }
    }

    fn load_keybinding_from_action_value(
        keystrokes: &str,
        action: &Value,
        context: Option<Rc<KeyBindingContextPredicate>>,
        use_key_equivalents: bool,
        cx: &App,
    ) -> Result<KeyBinding, String> {
        let (action, action_input_string) = Self::build_keymap_action_from_value(action, cx)?;

        KeyBinding::load(
            keystrokes,
            action,
            context,
            use_key_equivalents,
            action_input_string.map(SharedString::from),
            cx.keyboard_mapper().as_ref(),
        )
        .map_err(|InvalidKeystrokeError { keystroke }| {
            format!("Invalid keystroke {keystroke:?}. {KEYSTROKE_PARSE_EXPECTED_MESSAGE}")
        })
    }

    fn load_unbinding(
        keystrokes: &str,
        action: &UnbindTargetAction,
        context: Option<Rc<KeyBindingContextPredicate>>,
        use_key_equivalents: bool,
        cx: &App,
    ) -> Result<KeyBinding, String> {
        let key_binding = Self::load_keybinding_from_action_value(
            keystrokes,
            &action.0,
            context,
            use_key_equivalents,
            cx,
        )?;

        if key_binding.action().partial_eq(&NoAction) {
            return Err(String::from(
                "Expected action name string or [name, input] array.",
            ));
        }

        if key_binding.action().name() == Unbind::name_for_type() {
            return Err(format!(
                "Cannot use {:?} as an unbind target.",
                Unbind::name_for_type()
            ));
        }

        KeyBinding::load(
            keystrokes,
            Box::new(Unbind(key_binding.action().name().into())),
            key_binding.predicate(),
            use_key_equivalents,
            key_binding.action_input(),
            cx.keyboard_mapper().as_ref(),
        )
        .map_err(|InvalidKeystrokeError { keystroke }| {
            format!("Invalid keystroke {keystroke:?}. {KEYSTROKE_PARSE_EXPECTED_MESSAGE}")
        })
    }

    fn parse_action_value(action: &Value) -> Result<Option<(&String, Option<&Value>)>, String> {
        match action {
            Value::Array(items) => {
                if items.len() != 2 {
                    return Err(format!(
                        "Expected two-element array of [name, input]. Instead found {action}."
                    ));
                }

                let Value::String(name) = &items[0] else {
                    return Err(format!(
                        "Expected [name, input] array with a string action name. Instead found {action}."
                    ));
                };

                Ok(Some((name, Some(&items[1]))))
            }
            Value::String(name) => Ok(Some((name, None))),
            Value::Null => Ok(None),
            _ => Err(format!(
                "Expected action string, [name, input] array, or null. Instead found {action}."
            )),
        }
    }

    fn build_keymap_action_from_value(
        action: &Value,
        cx: &App,
    ) -> Result<(Box<dyn Action>, Option<String>), String> {
        let (build_result, action_input_string) = match Self::parse_action_value(action)? {
            Some((name, action_input)) if name.as_str() == ActionSequence::name_for_type() => {
                match action_input {
                    Some(action_input) => (
                        ActionSequence::build_sequence(action_input.clone(), cx),
                        None,
                    ),
                    None => (Err(ActionSequence::expected_array_error()), None),
                }
            }
            Some((name, Some(action_input))) => {
                let action_input_string = action_input.to_string();
                (
                    cx.build_action(name, Some(action_input.clone())),
                    Some(action_input_string),
                )
            }
            Some((name, None)) => (cx.build_action(name, None), None),
            None => (Ok(NoAction.boxed_clone()), None),
        };

        match build_result {
            Ok(action) => Ok((action, action_input_string)),
            Err(ActionBuildError::NotFound { name }) => {
                Err(format!("Did not find an action named {name:?}."))
            }
            Err(ActionBuildError::BuildError { name, error }) => {
                if let Some(action_input_string) = action_input_string {
                    Err(format!(
                        "Cannot build {name:?} action from input value {action_input_string}: {error}"
                    ))
                } else {
                    Err(format!(
                        "Cannot build {name:?} action without input data: {error}"
                    ))
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use indoc::indoc;

    gpui::actions!(test_keymap_file, [StringAction, InputAction]);

    #[gpui::test]
    fn test_keymap_section_unbinds_are_loaded_before_bindings(cx: &mut App) {
        let key_bindings = match KeymapFile::load(
            indoc! {r#"
                [
                    {
                        "unbind": {
                            "ctrl-a": "test_keymap_file::StringAction",
                            "ctrl-b": ["test_keymap_file::InputAction", {}]
                        },
                        "bindings": {
                            "ctrl-c": "test_keymap_file::StringAction"
                        }
                    }
                ]
            "#},
            cx,
        ) {
            KeymapFileLoadResult::Success { key_bindings } => key_bindings,
            KeymapFileLoadResult::SomeFailedToLoad { error_message, .. } => {
                panic!("{error_message}");
            }
            KeymapFileLoadResult::JsonParseFailure { error } => {
                panic!("JSON parse error: {error}");
            }
        };

        assert_eq!(key_bindings.len(), 3);
        assert!(
            key_bindings[0]
                .action()
                .partial_eq(&Unbind("test_keymap_file::StringAction".into()))
        );
        assert_eq!(key_bindings[0].action_input(), None);
        assert!(
            key_bindings[1]
                .action()
                .partial_eq(&Unbind("test_keymap_file::InputAction".into()))
        );
        assert_eq!(
            key_bindings[1]
                .action_input()
                .as_ref()
                .map(ToString::to_string),
            Some("{}".to_string())
        );
        assert_eq!(
            key_bindings[2].action().name(),
            "test_keymap_file::StringAction"
        );
    }

    #[gpui::test]
    fn test_keymap_unbind_loads_valid_target_action_with_input(cx: &mut App) {
        let key_bindings = match KeymapFile::load(
            indoc! {r#"
                [
                    {
                        "unbind": {
                            "ctrl-a": ["test_keymap_file::InputAction", {}]
                        }
                    }
                ]
            "#},
            cx,
        ) {
            KeymapFileLoadResult::Success { key_bindings } => key_bindings,
            other => panic!("expected Success, got {other:?}"),
        };

        assert_eq!(key_bindings.len(), 1);
        assert!(
            key_bindings[0]
                .action()
                .partial_eq(&Unbind("test_keymap_file::InputAction".into()))
        );
        assert_eq!(
            key_bindings[0]
                .action_input()
                .as_ref()
                .map(ToString::to_string),
            Some("{}".to_string())
        );
    }

    #[gpui::test]
    fn test_keymap_unbind_rejects_null(cx: &mut App) {
        match KeymapFile::load(
            indoc! {r#"
                [
                    {
                        "unbind": {
                            "ctrl-a": null
                        }
                    }
                ]
            "#},
            cx,
        ) {
            KeymapFileLoadResult::SomeFailedToLoad {
                key_bindings,
                error_message,
            } => {
                assert!(key_bindings.is_empty());
                assert!(
                    error_message.contains("Expected action name string or [name, input] array.")
                );
            }
            other => panic!("expected SomeFailedToLoad, got {other:?}"),
        }
    }

    #[gpui::test]
    fn test_keymap_unbind_rejects_unbind_action(cx: &mut App) {
        let keymap = indoc! {r#"
            [
                {
                    "unbind": {
                        "ctrl-a": ["__UNBIND__", "test_keymap_file::StringAction"]
                    }
                }
            ]
        "#}
        .replace("__UNBIND__", Unbind::name_for_type());

        match KeymapFile::load(&keymap, cx) {
            KeymapFileLoadResult::SomeFailedToLoad {
                key_bindings,
                error_message,
            } => {
                assert!(key_bindings.is_empty());
                assert!(error_message.contains(&format!(
                    "Cannot use {:?} as an unbind target.",
                    Unbind::name_for_type()
                )));
            }
            other => panic!("expected SomeFailedToLoad, got {other:?}"),
        }
    }
}
