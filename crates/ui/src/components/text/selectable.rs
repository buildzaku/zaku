use gpui::{
    Anchor, AnyElement, App, Bounds, CursorStyle, DispatchPhase, Div, Element, ElementId, Entity,
    FontWeight, GlobalElementId, Hitbox, InspectorElementId, InteractiveElement, Interactivity,
    LayoutId, MouseButton, MouseMoveEvent, MouseUpEvent, Pixels, RenderOnce, SharedString,
    StyleRefinement, Styled, StyledText, WeakEntity, Window, prelude::*,
};
use std::{ops::Range, rc::Rc};

use theme::{ActiveTheme, ThemeSettings};

use crate::{Color, LineHeightStyle, TextSize};

use super::{
    TextCommon, TextStyle, insert_text_hitboxes, interaction::TextInteractionState,
    selection::paint_text_selection,
};

#[derive(IntoElement)]
pub struct SelectableText<T: Copy + Ord + 'static> {
    base: Div,
    interaction_state: WeakEntity<TextInteractionState<T>>,
    selectable: bool,
    id: T,
    text: SharedString,
    style: TextStyle,
}

impl<T: Copy + Ord + 'static> SelectableText<T> {
    pub fn new(
        interaction_state: &Entity<TextInteractionState<T>>,
        id: T,
        text: impl Into<SharedString>,
    ) -> Self {
        Self {
            base: gpui::div(),
            interaction_state: interaction_state.downgrade(),
            selectable: true,
            id,
            text: text.into(),
            style: TextStyle::default(),
        }
    }

    pub fn set_text(&mut self, text: impl Into<SharedString>) {
        self.text = text.into();
    }

    pub fn truncate_start(mut self) -> Self {
        self.style.truncate_start = true;
        self
    }

    pub fn selectable(mut self, selectable: bool) -> Self {
        self.selectable = selectable;
        self
    }

    fn style(&mut self) -> &mut StyleRefinement {
        self.base.style()
    }

    gpui::margin_style_methods!({
        visibility: pub
    });
}

impl<T: Copy + Ord + 'static> TextCommon for SelectableText<T> {
    fn size(mut self, size: TextSize) -> Self {
        self.style.size = size;
        self
    }

    fn weight(mut self, weight: FontWeight) -> Self {
        self.style.weight = Some(weight);
        self
    }

    fn line_height_style(mut self, line_height_style: LineHeightStyle) -> Self {
        self.style.line_height_style = line_height_style;
        self
    }

    fn color(mut self, color: Color) -> Self {
        self.style.color = color;
        self
    }

    fn strikethrough(mut self) -> Self {
        self.style.strikethrough = true;
        self
    }

    fn italic(mut self) -> Self {
        self.style.italic = true;
        self
    }

    fn underline(mut self) -> Self {
        self.style.underline = true;
        self
    }

    fn alpha(mut self, alpha: f32) -> Self {
        self.style.alpha = Some(alpha);
        self
    }

    fn truncate(mut self) -> Self {
        self.style.truncate = true;
        self
    }

    fn single_line(mut self) -> Self {
        self.text = SharedString::from(self.text.replace('\n', "\u{23ce}"));
        self.style.single_line = true;
        self
    }

    fn font_buffer(mut self, cx: &App) -> Self {
        self.base = self
            .base
            .font(ThemeSettings::get_global(cx).buffer_font.clone());
        self
    }

    fn inline_code(mut self, cx: &App) -> Self {
        self.base = self
            .base
            .font(ThemeSettings::get_global(cx).buffer_font.clone())
            .bg(cx.theme().colors().element_background)
            .rounded_sm()
            .px_0p5();
        self
    }
}

impl<T: Copy + Ord + 'static> RenderOnce for SelectableText<T> {
    fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        let Self {
            base,
            interaction_state,
            selectable,
            id,
            text,
            style,
        } = self;
        let interaction_state = if selectable {
            interaction_state.upgrade()
        } else {
            None
        };
        let selected_range = interaction_state
            .as_ref()
            .and_then(|state| state.read(cx).selected_range_for_text(id, text.as_ref()));
        let element = SelectableTextElement {
            interaction_state: interaction_state.as_ref().map(Entity::downgrade),
            id,
            text: text.clone(),
            styled_text: StyledText::new(text),
            selected_range,
            selectable,
        };
        style.apply(base, cx).child(element)
    }
}

struct SelectableTextElement<T: Copy + Ord + 'static> {
    interaction_state: Option<WeakEntity<TextInteractionState<T>>>,
    id: T,
    text: SharedString,
    styled_text: StyledText,
    selected_range: Option<Range<usize>>,
    selectable: bool,
}

impl<T: Copy + Ord + 'static> Element for SelectableTextElement<T> {
    type RequestLayoutState = ();
    type PrepaintState = Vec<Hitbox>;

    fn id(&self) -> Option<ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static core::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _id: Option<&GlobalElementId>,
        inspector_id: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, Self::RequestLayoutState) {
        self.styled_text
            .request_layout(None, inspector_id, window, cx)
    }

    fn prepaint(
        &mut self,
        _global_id: Option<&GlobalElementId>,
        inspector_id: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        state: &mut Self::RequestLayoutState,
        window: &mut Window,
        cx: &mut App,
    ) -> Self::PrepaintState {
        self.styled_text
            .prepaint(None, inspector_id, bounds, state, window, cx);
        if self.selectable {
            insert_text_hitboxes(self.styled_text.layout(), window)
        } else {
            Vec::new()
        }
    }

    fn paint(
        &mut self,
        _global_id: Option<&GlobalElementId>,
        inspector_id: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _request_layout: &mut Self::RequestLayoutState,
        hitboxes: &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        let text_layout = self.styled_text.layout();
        if self.selectable {
            for hitbox in hitboxes.as_slice() {
                window.set_cursor_style(CursorStyle::IBeam, hitbox);
            }

            if let Some(interaction_state) = self.interaction_state.as_ref()
                && let Err(error) = interaction_state.update(cx, |state, _| {
                    state.register_text_layout(self.id, self.text.clone(), text_layout);
                })
            {
                log::trace!("Failed to register selectable text layout: {error:?}");
            }

            if let Some(selected_range) = self.selected_range.clone() {
                paint_text_selection(
                    selected_range,
                    text_layout,
                    cx.theme().colors().element_selection_background,
                    window,
                );
            }
        }

        self.styled_text
            .paint(None, inspector_id, bounds, &mut (), &mut (), window, cx);
    }
}

impl<T: Copy + Ord + 'static> IntoElement for SelectableTextElement<T> {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

#[derive(IntoElement)]
pub struct SelectableTextGroup<T: Copy + Ord + 'static> {
    base: Div,
    interaction_state: WeakEntity<TextInteractionState<T>>,
    selectable: bool,
    selection_order: Vec<T>,
    copy_separator: SharedString,
    text_for_selection: Option<Rc<dyn Fn(T, &mut Window, &mut App) -> Option<SharedString>>>,
    child: Option<AnyElement>,
}

impl<T: Copy + Ord + 'static> SelectableTextGroup<T> {
    pub fn new(interaction_state: &Entity<TextInteractionState<T>>) -> Self {
        Self {
            base: gpui::div().relative(),
            interaction_state: interaction_state.downgrade(),
            selectable: true,
            selection_order: Vec::new(),
            copy_separator: SharedString::from(""),
            text_for_selection: None,
            child: None,
        }
    }

    pub fn selection_order(mut self, selection_order: impl IntoIterator<Item = T>) -> Self {
        self.selection_order = selection_order.into_iter().collect();
        self
    }

    pub fn copy_separator(mut self, copy_separator: impl Into<SharedString>) -> Self {
        self.copy_separator = copy_separator.into();
        self
    }

    pub fn text_for_selection(
        mut self,
        text_for_selection: impl Fn(T, &mut Window, &mut App) -> Option<SharedString> + 'static,
    ) -> Self {
        self.text_for_selection = Some(Rc::new(text_for_selection));
        self
    }

    pub fn child(mut self, child: impl IntoElement) -> Self {
        self.child = Some(child.into_any_element());
        self
    }

    pub fn selectable(mut self, selectable: bool) -> Self {
        self.selectable = selectable;
        self
    }
}

impl<T: Copy + Ord + 'static> Styled for SelectableTextGroup<T> {
    fn style(&mut self) -> &mut StyleRefinement {
        self.base.style()
    }
}

impl<T: Copy + Ord + 'static> InteractiveElement for SelectableTextGroup<T> {
    fn interactivity(&mut self) -> &mut Interactivity {
        self.base.interactivity()
    }
}

impl<T: Copy + Ord + 'static> RenderOnce for SelectableTextGroup<T> {
    fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        let Self {
            base,
            interaction_state,
            selectable,
            selection_order,
            copy_separator,
            text_for_selection,
            child,
        } = self;
        let interaction_state = interaction_state.upgrade();
        let focus_handle = if selectable {
            interaction_state
                .as_ref()
                .map(|state| state.read(cx).focus_handle())
        } else {
            None
        };
        let context_menu = if selectable {
            interaction_state
                .as_ref()
                .and_then(|state| state.read(cx).context_menu())
        } else {
            None
        };
        if let Some(interaction_state) = interaction_state.as_ref() {
            interaction_state.update(cx, |state, _| {
                state.clear_text_layouts();
                if !selectable {
                    state.clear_text_selection();
                }
            });
        }
        let interaction_state = if selectable { interaction_state } else { None };
        let selection_order = Rc::new(selection_order);

        base.when_some(focus_handle.as_ref(), |this, focus_handle| {
            this.track_focus(focus_handle)
        })
        .when_some(
            interaction_state.zip(text_for_selection),
            |this, (interaction_state, text_for_selection)| {
                this.key_context("Text")
                    .on_mouse_down(MouseButton::Left, {
                        let interaction_state = interaction_state.clone();
                        move |event, window, cx| {
                            interaction_state.update(cx, |state, cx| {
                                state.begin_text_selection_at_position(
                                    event.position,
                                    event.click_count,
                                    window,
                                    cx,
                                );
                            });
                            cx.stop_propagation();
                            window.prevent_default();
                        }
                    })
                    .on_mouse_down(MouseButton::Right, {
                        let interaction_state = interaction_state.clone();
                        let selection_order = selection_order.clone();
                        let copy_separator = copy_separator.clone();
                        let text_for_selection = text_for_selection.clone();

                        move |event, window, cx| {
                            interaction_state.update(cx, |state, cx| {
                                state.deploy_text_context_menu(
                                    selection_order.as_ref(),
                                    copy_separator.as_ref(),
                                    text_for_selection.as_ref(),
                                    event.position,
                                    window,
                                    cx,
                                );
                            });
                            cx.stop_propagation();
                            window.prevent_default();
                        }
                    })
                    .on_action({
                        let interaction_state = interaction_state.clone();
                        let selection_order = selection_order.clone();
                        let copy_separator = copy_separator.clone();
                        let text_for_selection = text_for_selection.clone();

                        move |_: &actions::text::Copy, window: &mut Window, cx: &mut App| {
                            interaction_state.update(cx, |state, cx| {
                                state.copy_selected_text(
                                    selection_order.as_ref(),
                                    copy_separator.as_ref(),
                                    text_for_selection.as_ref(),
                                    window,
                                    cx,
                                );
                            });
                        }
                    })
                    .on_action({
                        let interaction_state = interaction_state.clone();
                        let selection_order = selection_order.clone();

                        move |_: &actions::text::SelectAll, _: &mut Window, cx: &mut App| {
                            interaction_state.update(cx, |state, cx| {
                                state.select_all_text(selection_order.as_ref(), cx);
                            });
                        }
                    })
                    .child(
                        gpui::canvas(|_, _, _| {}, {
                            let interaction_state = interaction_state.clone();

                            move |bounds, (), window, cx| {
                                interaction_state.update(cx, |state, _| {
                                    state.set_text_selection_bounds(bounds);
                                });

                                window.on_mouse_event({
                                    let interaction_state = interaction_state.clone();

                                    move |event: &MouseMoveEvent, phase, _, cx| {
                                        if phase == DispatchPhase::Bubble && event.dragging() {
                                            interaction_state.update(cx, |state, cx| {
                                                state.update_text_selection_at_position(
                                                    event.position,
                                                    cx,
                                                );
                                            });
                                        }
                                    }
                                });

                                window.on_mouse_event({
                                    let interaction_state = interaction_state.clone();

                                    move |event: &MouseUpEvent, phase, _, cx| {
                                        if phase == DispatchPhase::Bubble
                                            && event.button == MouseButton::Left
                                        {
                                            interaction_state.update(cx, |state, cx| {
                                                state.end_text_selection_drag(cx);
                                            });
                                        }
                                    }
                                });
                            }
                        })
                        .absolute()
                        .inset_0(),
                    )
            },
        )
        .children(child)
        .when(context_menu.is_some(), |this| {
            this.child(
                gpui::div()
                    .absolute()
                    .top_0()
                    .right_0()
                    .bottom_0()
                    .left_0()
                    .occlude(),
            )
        })
        .children(context_menu.as_ref().map(|(menu, position)| {
            gpui::deferred(
                gpui::anchored()
                    .position(*position)
                    .anchor(Anchor::TopLeft)
                    .child(menu.clone()),
            )
            .with_priority(3)
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use gpui::{Context, Modifiers, Point, Render, TestAppContext, VisualTestContext};

    use settings::SettingsStore;
    use theme::LoadThemes;

    use crate::Indicator;

    fn init_test(cx: &mut TestAppContext) {
        cx.update(|cx| {
            let settings_store = SettingsStore::test_new(cx);
            cx.set_global(settings_store);
            theme::init(LoadThemes::JustBase, cx);
        });
    }

    struct TestSelectableTextGroup {
        interaction_state: Entity<TextInteractionState<usize>>,
        items: Vec<SharedString>,
        copy_separator: SharedString,
    }

    impl TestSelectableTextGroup {
        fn new<I, S>(items: I, cx: &mut Context<Self>) -> Self
        where
            I: IntoIterator<Item = S>,
            S: Into<SharedString>,
        {
            Self {
                interaction_state: cx.new(|cx| TextInteractionState::new(cx)),
                items: items.into_iter().map(Into::into).collect(),
                copy_separator: "\t".into(),
            }
        }

        fn selected_text(&self, window: &mut Window, cx: &mut App) -> Option<String> {
            let selection_order = (0..self.items.len()).collect::<Vec<_>>();
            let text_for_selection =
                |item_id, _: &mut Window, _: &mut App| self.items.get(item_id).cloned();

            self.interaction_state.update(cx, |state, cx| {
                state.selected_text(
                    &selection_order,
                    self.copy_separator.as_ref(),
                    &text_for_selection,
                    window,
                    cx,
                )
            })
        }

        #[track_caller]
        fn position_for_text_offset(
            &self,
            id: usize,
            byte_offset: usize,
            cx: &mut Context<Self>,
        ) -> Point<Pixels> {
            self.interaction_state
                .read_with(cx, |state, _| {
                    state.position_for_text_offset(id, byte_offset)
                })
                .unwrap()
        }
    }

    impl Render for TestSelectableTextGroup {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            let interaction_state = self.interaction_state.clone();
            let copy_separator = self.copy_separator.clone();
            let item_texts = self
                .items
                .iter()
                .enumerate()
                .map(|(id, text)| (id, text.clone()))
                .collect::<Vec<_>>();

            gpui::div().w(gpui::px(500.0)).h(gpui::px(64.0)).child(
                SelectableTextGroup::new(&interaction_state)
                    .debug_selector(|| "selectable-text-group".into())
                    .flex()
                    .items_center()
                    .justify_end()
                    .w_full()
                    .h_full()
                    .px_3()
                    .selection_order(0..item_texts.len())
                    .copy_separator(copy_separator)
                    .text_for_selection({
                        let item_texts = item_texts.clone();

                        move |item_id, _, _| item_texts.get(item_id).map(|(_, text)| text.clone())
                    })
                    .child(gpui::div().flex().items_center().gap_2().children(
                        item_texts.into_iter().flat_map({
                            let interaction_state = interaction_state.clone();

                            move |(id, text)| {
                                let text = SelectableText::new(&interaction_state, id, text)
                                    .into_any_element();

                                if id == 0 {
                                    vec![text]
                                } else {
                                    vec![Indicator::dot().into_any_element(), text]
                                }
                            }
                        }),
                    )),
            )
        }
    }

    fn simulate_drag(cx: &mut VisualTestContext, start: Point<Pixels>, end: Point<Pixels>) {
        cx.simulate_mouse_down(start, MouseButton::Left, Modifiers::default());
        cx.simulate_mouse_move(end, MouseButton::Left, Modifiers::default());
        cx.simulate_mouse_up(end, MouseButton::Left, Modifiers::default());
    }

    #[track_caller]
    fn assert_selection(
        expected: Option<&str>,
        view: &Entity<TestSelectableTextGroup>,
        cx: &mut VisualTestContext,
    ) {
        assert_eq!(
            view.update_in(cx, |view, window, cx| view.selected_text(window, cx))
                .as_deref(),
            expected,
        );
    }

    #[gpui::test]
    fn test_selectable_text_group_select_all(cx: &mut TestAppContext) {
        init_test(cx);

        let items = ["foo", "bar", "baz"];
        let (view, cx) = cx.add_window_view(move |_, cx| TestSelectableTextGroup::new(items, cx));
        let group_bounds = cx.debug_bounds("selectable-text-group").unwrap();
        cx.simulate_click(group_bounds.center(), Modifiers::default());
        cx.dispatch_action(actions::text::SelectAll);

        assert_selection(Some("foo\tbar\tbaz"), &view, cx);
    }

    #[gpui::test]
    fn test_selectable_text_group_drag_selects_from_padding(cx: &mut TestAppContext) {
        init_test(cx);

        let items = ["foo", "bar", "baz"];
        let (view, cx) = cx.add_window_view(move |_, cx| TestSelectableTextGroup::new(items, cx));
        let group_bounds = cx.debug_bounds("selectable-text-group").unwrap();
        let inside_group_offset = gpui::px(2.0);
        let start = gpui::point(
            group_bounds.left() + inside_group_offset,
            group_bounds.center().y,
        );
        let end = gpui::point(
            group_bounds.right() - inside_group_offset,
            group_bounds.center().y,
        );
        simulate_drag(cx, start, end);

        assert_selection(Some("foo\tbar\tbaz"), &view, cx);
    }

    #[gpui::test]
    fn test_selectable_text_group_drag_below_selects_from_padding(cx: &mut TestAppContext) {
        init_test(cx);

        let items = ["foo", "bar", "baz"];
        let (view, cx) = cx.add_window_view(move |_, cx| TestSelectableTextGroup::new(items, cx));
        let group_bounds = cx.debug_bounds("selectable-text-group").unwrap();
        let inside_group_offset = gpui::px(2.0);
        let outside_group_offset = gpui::px(8.0);
        let start = gpui::point(
            group_bounds.left() + inside_group_offset,
            group_bounds.center().y,
        );
        let end = gpui::point(
            group_bounds.center().x,
            group_bounds.bottom() + outside_group_offset,
        );
        simulate_drag(cx, start, end);

        assert_selection(Some("foo\tbar\tbaz"), &view, cx);
    }

    #[gpui::test]
    fn test_selectable_text_group_drag_above_selects_from_padding(cx: &mut TestAppContext) {
        init_test(cx);

        let items = ["foo", "bar", "baz"];
        let (view, cx) = cx.add_window_view(move |_, cx| TestSelectableTextGroup::new(items, cx));
        let group_bounds = cx.debug_bounds("selectable-text-group").unwrap();
        let inside_group_offset = gpui::px(2.0);
        let outside_group_offset = gpui::px(8.0);
        let start = gpui::point(
            group_bounds.right() - inside_group_offset,
            group_bounds.center().y,
        );
        let end = gpui::point(
            group_bounds.center().x,
            group_bounds.top() - outside_group_offset,
        );
        simulate_drag(cx, start, end);

        assert_selection(Some("foo\tbar\tbaz"), &view, cx);
    }

    #[gpui::test]
    fn test_selectable_text_group_drag_below_selects_from_text_start(cx: &mut TestAppContext) {
        init_test(cx);

        let items = ["foo", "bar", "baz"];
        let (view, cx) = cx.add_window_view(move |_, cx| TestSelectableTextGroup::new(items, cx));
        let group_bounds = cx.debug_bounds("selectable-text-group").unwrap();
        let outside_group_offset = gpui::px(8.0);
        let text_boundary_offset = gpui::px(1.0);
        let bar_id = items.iter().position(|item| *item == "bar").unwrap();
        let byte_offset = 0;
        let mut start = view.update(cx, |view, cx| {
            view.position_for_text_offset(bar_id, byte_offset, cx)
        });
        start.x += text_boundary_offset;
        start.y = group_bounds.center().y;
        let end = gpui::point(
            group_bounds.center().x,
            group_bounds.bottom() + outside_group_offset,
        );
        simulate_drag(cx, start, end);

        assert_selection(Some("bar\tbaz"), &view, cx);
    }

    #[gpui::test]
    fn test_selectable_text_group_drag_above_selects_from_text_start(cx: &mut TestAppContext) {
        init_test(cx);

        let items = ["foo", "bar", "baz"];
        let (view, cx) = cx.add_window_view(move |_, cx| TestSelectableTextGroup::new(items, cx));
        let group_bounds = cx.debug_bounds("selectable-text-group").unwrap();
        let outside_group_offset = gpui::px(8.0);
        let text_boundary_offset = gpui::px(1.0);
        let bar_id = items.iter().position(|item| *item == "bar").unwrap();
        let byte_offset = 0;
        let mut start = view.update(cx, |view, cx| {
            view.position_for_text_offset(bar_id, byte_offset, cx)
        });
        start.x += text_boundary_offset;
        start.y = group_bounds.center().y;
        let end = gpui::point(
            group_bounds.center().x,
            group_bounds.top() - outside_group_offset,
        );
        simulate_drag(cx, start, end);

        assert_selection(Some("foo"), &view, cx);
    }

    #[gpui::test]
    fn test_selectable_text_group_drag_below_selects_from_text_offset(cx: &mut TestAppContext) {
        init_test(cx);

        let items = ["foo", "bar", "baz"];
        let (view, cx) = cx.add_window_view(move |_, cx| TestSelectableTextGroup::new(items, cx));
        let group_bounds = cx.debug_bounds("selectable-text-group").unwrap();
        let outside_group_offset = gpui::px(8.0);
        let bar_id = items.iter().position(|item| *item == "bar").unwrap();
        let byte_offset = 1;
        let mut start = view.update(cx, |view, cx| {
            view.position_for_text_offset(bar_id, byte_offset, cx)
        });
        start.y = group_bounds.center().y;
        let end = gpui::point(
            group_bounds.center().x,
            group_bounds.bottom() + outside_group_offset,
        );
        simulate_drag(cx, start, end);

        assert_selection(Some("ar\tbaz"), &view, cx);
    }

    #[gpui::test]
    fn test_selectable_text_group_drag_above_selects_from_text_offset(cx: &mut TestAppContext) {
        init_test(cx);

        let items = ["foo", "bar", "baz"];
        let (view, cx) = cx.add_window_view(move |_, cx| TestSelectableTextGroup::new(items, cx));
        let group_bounds = cx.debug_bounds("selectable-text-group").unwrap();
        let outside_group_offset = gpui::px(8.0);
        let bar_id = items.iter().position(|item| *item == "bar").unwrap();
        let byte_offset = 1;
        let mut start = view.update(cx, |view, cx| {
            view.position_for_text_offset(bar_id, byte_offset, cx)
        });
        start.y = group_bounds.center().y;
        let end = gpui::point(
            group_bounds.center().x,
            group_bounds.top() - outside_group_offset,
        );
        simulate_drag(cx, start, end);

        assert_selection(Some("foo\tb"), &view, cx);
    }
}
