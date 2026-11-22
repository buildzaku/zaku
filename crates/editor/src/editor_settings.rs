pub use settings::CurrentLineHighlight;

use settings::{DelayMs, RegisterSetting, Settings, SettingsContent};

#[derive(Debug, Clone, PartialEq, Eq, RegisterSetting)]
pub struct EditorSettings {
    pub current_line_highlight: CurrentLineHighlight,
    pub gutter: Gutter,
    pub hover_popover: HoverPopover,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Gutter {
    pub min_line_number_digits: usize,
    pub line_numbers: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HoverPopover {
    pub enabled: bool,
    pub delay: DelayMs,
    pub hiding_delay: DelayMs,
}

impl Settings for EditorSettings {
    fn from_settings(content: &SettingsContent) -> Self {
        let editor = content.editor.as_ref();
        let gutter = editor
            .and_then(|editor| editor.gutter.clone())
            .unwrap_or_default();
        let hover_popover = editor
            .and_then(|editor| editor.hover_popover.clone())
            .unwrap_or_default();
        Self {
            current_line_highlight: editor
                .and_then(|editor| editor.current_line_highlight)
                .expect("editor current line highlight should be defaulted"),
            gutter: Gutter {
                min_line_number_digits: gutter
                    .min_line_number_digits
                    .expect("editor gutter minimum line number digits should be defaulted"),
                line_numbers: gutter
                    .line_numbers
                    .expect("editor gutter line numbers should be defaulted"),
            },
            hover_popover: HoverPopover {
                enabled: hover_popover
                    .enabled
                    .expect("editor hover popover enabled should be defaulted"),
                delay: hover_popover
                    .delay
                    .expect("editor hover popover delay should be defaulted"),
                hiding_delay: hover_popover
                    .hiding_delay
                    .expect("editor hover popover hiding delay should be defaulted"),
            },
        }
    }
}
