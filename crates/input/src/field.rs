use gpui::{App, FocusHandle, Focusable, Hsla, Length, SharedString, Window, prelude::*};
use std::sync::Arc;

use theme::ActiveTheme;
use ui::{Color, Icon, IconName, IconSize, Label, LabelCommon, LabelSize, StyledExt};

use crate::ErasedEditor;

pub struct InputFieldStyle {
    text_color: Hsla,
    icon_color: Hsla,
    background_color: Hsla,
    border_color: Hsla,
    border_focused: Hsla,
}

/// An Input Field component that can be used to create text fields like search inputs, form fields, etc.
///
/// It wraps a single line editor and allows for common field properties like labels, placeholders, icons, etc.
pub struct InputField {
    label: Option<SharedString>,
    label_size: LabelSize,
    placeholder: SharedString,
    editor: Arc<dyn ErasedEditor>,
    start_icon: Option<IconName>,
    min_width: Length,
    tab_index: Option<isize>,
    tab_stop: bool,
}

impl Focusable for InputField {
    fn focus_handle(&self, cx: &App) -> FocusHandle {
        self.editor.focus_handle(cx)
    }
}

impl InputField {
    pub fn new(window: &mut Window, cx: &mut App, placeholder_text: &str) -> Self {
        let editor_factory = crate::ERASED_EDITOR_FACTORY
            .get()
            .expect("ErasedEditorFactory to be initialized");
        let editor = (editor_factory)(window, cx);
        editor.set_placeholder_text(placeholder_text, window, cx);

        Self {
            label: None,
            label_size: LabelSize::Small,
            placeholder: SharedString::new(placeholder_text),
            editor,
            start_icon: None,
            min_width: gpui::px(192.).into(),
            tab_index: None,
            tab_stop: true,
        }
    }

    pub fn start_icon(mut self, icon: IconName) -> Self {
        self.start_icon = Some(icon);
        self
    }

    pub fn label(mut self, label: impl Into<SharedString>) -> Self {
        self.label = Some(label.into());
        self
    }

    pub fn label_size(mut self, size: LabelSize) -> Self {
        self.label_size = size;
        self
    }

    pub fn label_min_width(mut self, width: impl Into<Length>) -> Self {
        self.min_width = width.into();
        self
    }

    pub fn tab_index(mut self, index: isize) -> Self {
        self.tab_index = Some(index);
        self
    }

    pub fn tab_stop(mut self, tab_stop: bool) -> Self {
        self.tab_stop = tab_stop;
        self
    }

    pub fn is_empty(&self, cx: &App) -> bool {
        self.editor().text(cx).trim().is_empty()
    }

    pub fn editor(&self) -> &Arc<dyn ErasedEditor> {
        &self.editor
    }

    pub fn text(&self, cx: &App) -> String {
        self.editor().text(cx)
    }

    pub fn clear(&self, window: &mut Window, cx: &mut App) {
        self.editor().clear(window, cx)
    }

    pub fn set_text(&self, text: &str, window: &mut Window, cx: &mut App) {
        self.editor().set_text(text, window, cx)
    }
}

impl Render for InputField {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let editor = self.editor.clone();

        let theme_colors = cx.theme().colors();
        let style = InputFieldStyle {
            text_color: theme_colors.text,
            icon_color: theme_colors.icon_muted,
            background_color: theme_colors.editor_background,
            border_color: theme_colors.border_variant,
            border_focused: theme_colors.border_focused,
        };

        let focus_handle = self.editor.focus_handle(cx);

        let configured_handle = if let Some(tab_index) = self.tab_index {
            focus_handle.tab_index(tab_index).tab_stop(self.tab_stop)
        } else if !self.tab_stop {
            focus_handle.tab_stop(false)
        } else {
            focus_handle
        };

        gpui::div()
            .v_flex()
            .id(self.placeholder.clone())
            .w_full()
            .gap_1()
            .when_some(self.label.clone(), |this, label| {
                this.child(
                    Label::new(label)
                        .size(self.label_size)
                        .color(Color::Default),
                )
            })
            .child(
                gpui::div()
                    .h_flex()
                    .track_focus(&configured_handle)
                    .min_w(self.min_width)
                    .min_h_8()
                    .w_full()
                    .px_2()
                    .py_1p5()
                    .flex_grow()
                    .text_color(style.text_color)
                    .rounded_md()
                    .bg(style.background_color)
                    .border_1()
                    .border_color(style.border_color)
                    .when(
                        editor.focus_handle(cx).contains_focused(window, cx),
                        |this| this.border_color(style.border_focused),
                    )
                    .when_some(self.start_icon, |this, icon| {
                        this.gap_1().child(
                            Icon::new(icon)
                                .size(IconSize::Small)
                                .color(style.icon_color.into()),
                        )
                    })
                    .child(self.editor.render(window, cx)),
            )
    }
}
