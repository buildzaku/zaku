use std::sync::Arc;

use gpui::{
    App, Div, FocusHandle, Focusable, Hsla, Length, MouseButton, MouseDownEvent, SharedString,
    Window, div, prelude::*, px, rgb,
};

use ui::{Icon, IconName, IconSize};

use crate::ErasedEditor;

pub struct InputFieldStyle {
    text_color: Hsla,
    label_color: Hsla,
    icon_color: Hsla,
    background_color: Hsla,
    border_color: Hsla,
    border_focused: Hsla,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum LabelSize {
    #[default]
    Default,
    Large,
    Small,
    XSmall,
}

impl LabelSize {
    fn apply(self, element: Div) -> Div {
        match self {
            LabelSize::Default => element.text_sm(),
            LabelSize::Large => element.text_base(),
            LabelSize::Small => element.text_xs(),
            LabelSize::XSmall => element.text_xs(),
        }
    }
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
            min_width: px(192.).into(),
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

    fn render_label(&self, label: SharedString, style: &InputFieldStyle) -> Div {
        let base = div().text_color(style.label_color).child(label);
        self.label_size.apply(base)
    }
}

impl Render for InputField {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let editor = self.editor.clone();

        let style = InputFieldStyle {
            text_color: rgb(0xffffff).into(),
            label_color: rgb(0xb0b0b0).into(),
            icon_color: rgb(0x8a8a8a).into(),
            background_color: rgb(0x1a1a1a).into(),
            border_color: rgb(0x2a2a2a).into(),
            border_focused: rgb(0x41d4dc).into(),
        };

        let focus_handle = self.editor.focus_handle(cx);

        let configured_handle = if let Some(tab_index) = self.tab_index {
            focus_handle.tab_index(tab_index).tab_stop(self.tab_stop)
        } else if !self.tab_stop {
            focus_handle.tab_stop(false)
        } else {
            focus_handle
        };

        div()
            .flex()
            .flex_col()
            .w_full()
            .gap_1()
            .when_some(self.label.clone(), |this, label| {
                this.child(self.render_label(label, &style))
            })
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .track_focus(&configured_handle)
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|field, _: &MouseDownEvent, window, cx| {
                            let focus_handle = field.editor.focus_handle(cx);
                            focus_handle.focus(window, cx);
                            cx.stop_propagation();
                        }),
                    )
                    .min_w(self.min_width)
                    .h_8()
                    .w_full()
                    .pl_2()
                    .pr_1()
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
                                .color(style.icon_color),
                        )
                    })
                    .child(self.editor.render(window, cx)),
            )
    }
}
