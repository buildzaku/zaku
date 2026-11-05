use gpui::Action;
use schemars::JsonSchema;
use serde::Deserialize;

/// Decrease the font size in the editor.
#[derive(Clone, Default, PartialEq, Deserialize, JsonSchema, Action)]
#[action(namespace = zaku)]
#[serde(deny_unknown_fields)]
pub struct DecreaseBufferFontSize {
    #[serde(default)]
    pub persist: bool,
}

/// Increase the font size in the editor.
#[derive(Clone, Default, PartialEq, Deserialize, JsonSchema, Action)]
#[action(namespace = zaku)]
#[serde(deny_unknown_fields)]
pub struct IncreaseBufferFontSize {
    #[serde(default)]
    pub persist: bool,
}

/// Reset the editor font size to the default value.
#[derive(Clone, Default, PartialEq, Deserialize, JsonSchema, Action)]
#[action(namespace = zaku)]
#[serde(deny_unknown_fields)]
pub struct ResetBufferFontSize {
    #[serde(default)]
    pub persist: bool,
}

/// Decrease the font size of the user interface.
#[derive(Clone, Default, PartialEq, Deserialize, JsonSchema, Action)]
#[action(namespace = zaku)]
#[serde(deny_unknown_fields)]
pub struct DecreaseUiFontSize {
    #[serde(default)]
    pub persist: bool,
}

/// Increase the font size of the user interface.
#[derive(Clone, Default, PartialEq, Deserialize, JsonSchema, Action)]
#[action(namespace = zaku)]
#[serde(deny_unknown_fields)]
pub struct IncreaseUiFontSize {
    #[serde(default)]
    pub persist: bool,
}

/// Reset the user interface font size to the default value.
#[derive(Clone, Default, PartialEq, Deserialize, JsonSchema, Action)]
#[action(namespace = zaku)]
#[serde(deny_unknown_fields)]
pub struct ResetUiFontSize {
    #[serde(default)]
    pub persist: bool,
}

/// Reset the user interface and editor font sizes to their default values.
#[derive(Clone, Default, PartialEq, Deserialize, JsonSchema, Action)]
#[action(namespace = zaku)]
#[serde(deny_unknown_fields)]
pub struct ResetAllZoom {
    #[serde(default)]
    pub persist: bool,
}

gpui::actions!(
    zaku,
    [
        /// Show information about Zaku.
        About,
        /// Copy system specifications into the clipboard.
        CopySystemSpecsIntoClipboard,
        /// Hide the application.
        Hide,
        /// Hide other applications.
        HideOthers,
        /// Minimize the current window.
        Minimize,
        /// Open the settings JSON file.
        OpenSettingsFile,
        /// Open the keymap JSON file.
        OpenKeymapFile,
        /// Open logs.
        OpenLogs,
        /// Show all hidden windows.
        ShowAll,
        /// Zoom the current window.
        Zoom,
        /// Quit the application.
        Quit,
    ]
);
