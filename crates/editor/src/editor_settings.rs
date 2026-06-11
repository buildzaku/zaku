use settings::{RegisterSetting, Settings};

#[derive(Clone, Debug, PartialEq, Eq, RegisterSetting)]
pub struct EditorSettings {
    pub gutter: Gutter,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct Gutter {
    pub min_line_number_digits: usize,
    pub line_numbers: bool,
}

impl Settings for EditorSettings {
    fn from_settings(content: &settings::SettingsContent) -> Self {
        let gutter = content.gutter();
        Self {
            gutter: Gutter {
                min_line_number_digits: gutter.min_line_number_digits.unwrap(),
                line_numbers: gutter.line_numbers.unwrap(),
            },
        }
    }
}
