use gpui::{Context, IntoElement, ParentElement, Render, Window, prelude::*};

use ui::{
    Color, Disableable, Label, LabelCommon, ListItem, ListItemSpacing, ScrollAxes, Scrollbars,
    StyledExt, WithScrollbar,
};

use crate::{ElementContainer, Picker, PickerDelegate, PickerEditorPosition, head::Head};

impl<D: PickerDelegate> Render for Picker<D> {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let content = gpui::div()
            .when(self.is_modal, |this| this.elevation_3(cx))
            .child(self.render_results(window, cx));

        gpui::div().relative().child(content)
    }
}

impl<D: PickerDelegate> Picker<D> {
    pub(crate) fn render_results(
        &self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let editor_position = self.delegate.editor_position();
        gpui::div()
            .key_context("Picker")
            .relative()
            .flex()
            .flex_col()
            .map(|this| {
                self.shape.apply_results_size(
                    &self.size_bounds,
                    self.vertical_padding(),
                    this,
                    window,
                )
            })
            .on_action(cx.listener(Self::select_next))
            .on_action(cx.listener(Self::select_previous))
            .on_action(cx.listener(Self::editor_move_down))
            .on_action(cx.listener(Self::editor_move_up))
            .on_action(cx.listener(Self::select_first))
            .on_action(cx.listener(Self::select_last))
            .on_action(cx.listener(Self::cancel))
            .on_action(cx.listener(Self::confirm))
            .on_action(cx.listener(Self::secondary_confirm))
            .children(match &self.head {
                Head::Editor(editor) => {
                    if editor_position == PickerEditorPosition::Start {
                        Some(gpui::div().flex().items_center().w_full().child(
                            gpui::div().flex_1().child(self.delegate.render_editor(
                                &editor.clone(),
                                window,
                                cx,
                            )),
                        ))
                    } else {
                        None
                    }
                }
                Head::Empty(empty_head) => {
                    Some(gpui::div().flex().items_center().child(empty_head.clone()))
                }
            })
            .when(self.delegate.match_count() > 0, |element| {
                element.child(
                    gpui::div()
                        .id("element-container")
                        .flex()
                        .flex_col()
                        .flex_grow_1()
                        .relative()
                        .min_h_0()
                        .when_some(
                            self.shape.results_max_height(
                                &self.size_bounds,
                                self.vertical_padding(),
                                window,
                            ),
                            |this, max_height| this.max_h(max_height),
                        )
                        .overflow_hidden()
                        .children(self.delegate.render_header(window, cx))
                        .child(self.render_element_container(cx))
                        .when(self.show_scrollbar, |this| {
                            let base_scrollbar_config = Scrollbars::new(ScrollAxes::Vertical);
                            this.map(|this| match &self.element_container {
                                ElementContainer::UniformList(scroll_handle) => this
                                    .custom_scrollbars(
                                        base_scrollbar_config.tracked_scroll_handle(scroll_handle),
                                        window,
                                        cx,
                                    ),
                            })
                        }),
                )
            })
            .when(self.delegate.match_count() == 0, |element| {
                element.when_some(
                    self.delegate.no_matches_text(window, cx),
                    |element, text| {
                        element.child(
                            gpui::div().flex().flex_col().flex_grow_1().py_2().child(
                                ListItem::new("empty_state")
                                    .inset(true)
                                    .spacing(ListItemSpacing::Sparse)
                                    .disabled(true)
                                    .child(Label::new(text).color(Color::Muted)),
                            ),
                        )
                    },
                )
            })
            .children(self.delegate.render_footer(window, cx))
            .children(match &self.head {
                Head::Editor(editor) => {
                    if editor_position == PickerEditorPosition::End {
                        Some(self.delegate.render_editor(&editor.clone(), window, cx))
                    } else {
                        None
                    }
                }
                Head::Empty(empty_head) => Some(gpui::div().child(empty_head.clone())),
            })
    }
}
