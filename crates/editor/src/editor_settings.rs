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
        let gutter = content.gutter();
        Self {
            current_line_highlight: content.current_line_highlight().unwrap(),
            gutter: Gutter {
                min_line_number_digits: gutter.min_line_number_digits.unwrap(),
                line_numbers: gutter.line_numbers.unwrap(),
            },
        }
    }
}
