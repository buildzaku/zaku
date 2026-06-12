use gpui::{App, Context, Window};

use actions::editor::ToggleLineNumbers;
use settings::Settings;

use crate::{Editor, EditorSettings};

impl Editor {
    pub fn toggle_line_numbers(
        &mut self,
        _: &ToggleLineNumbers,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let mut editor_settings = EditorSettings::get_global(cx).clone();
        editor_settings.gutter.line_numbers = !editor_settings.gutter.line_numbers;
        EditorSettings::override_global(editor_settings, cx);
    }

    pub fn line_numbers_enabled(&self, cx: &App) -> bool {
        if let Some(show_line_numbers) = self.show_line_numbers {
            return show_line_numbers;
        }
        EditorSettings::get_global(cx).gutter.line_numbers
    }

    pub fn set_show_gutter(&mut self, show_gutter: bool, cx: &mut Context<Self>) {
        self.show_gutter = show_gutter;
        cx.notify();
    }

    pub fn set_show_line_numbers(&mut self, show_line_numbers: bool, cx: &mut Context<Self>) {
        self.show_line_numbers = Some(show_line_numbers);
        cx.notify();
    }
}
