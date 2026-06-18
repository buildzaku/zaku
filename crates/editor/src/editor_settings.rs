pub use settings::CurrentLineHighlight;

use settings::{RegisterSetting, Settings, SettingsContent};

#[derive(Clone, Debug, PartialEq, Eq, RegisterSetting)]
pub struct EditorSettings {
    pub current_line_highlight: CurrentLineHighlight,
    pub gutter: Gutter,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct Gutter {
    pub min_line_number_digits: usize,
    pub line_numbers: bool,
}

impl Settings for EditorSettings {
    fn from_settings(content: &SettingsContent) -> Self {
        let editor = content.editor.as_ref();
        let gutter = editor
            .and_then(|editor| editor.gutter.clone())
            .unwrap_or_default();
        Self {
            current_line_highlight: editor
                .and_then(|editor| editor.current_line_highlight)
                .unwrap(),
            gutter: Gutter {
                min_line_number_digits: gutter.min_line_number_digits.unwrap(),
                line_numbers: gutter.line_numbers.unwrap(),
            },
        }
    }
}
