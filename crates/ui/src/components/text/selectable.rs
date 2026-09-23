use gpui::{
    Anchor, AnyElement, App, Bounds, CursorStyle, DispatchPhase, Div, Element, ElementId, Entity,
    FontWeight, GlobalElementId, Hitbox, InspectorElementId, InteractiveElement, Interactivity,
    LayoutId, MouseButton, MouseMoveEvent, MouseUpEvent, Pixels, RenderOnce, SharedString,
    StyleRefinement, Styled, StyledText, WeakEntity, Window, prelude::*,
};
use std::rc::Rc;

use theme::{ActiveTheme, ThemeSettings};

use crate::{Color, LineHeightStyle, TextSize};

use super::{
    TextCommon, TextStyle, insert_text_hitboxes,
    interaction::TextInteractionState,
    selection::{RenderedText, paint_text_selection},
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
        let text = RenderedText::new(text);
        let text = if style.single_line {
            text.single_line()
        } else {
            text
        };
        let styled_text = StyledText::new(text.rendered.clone());
        let element = SelectableTextElement {
            interaction_state: interaction_state.as_ref().map(Entity::downgrade),
            id,
            text,
            styled_text,
            selectable,
        };
        style.apply(base, cx).child(element)
    }
}

struct SelectableTextElement<T: Copy + Ord + 'static> {
    interaction_state: Option<WeakEntity<TextInteractionState<T>>>,
    id: T,
    text: RenderedText,
    styled_text: StyledText,
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

            let selected_range =
                self.interaction_state.as_ref().and_then(
                    |interaction_state| match interaction_state.update(cx, |state, _| {
                        state.text_selection.register_layout(
                            self.id,
                            0,
                            self.text.clone(),
                            text_layout,
                        );
                        state.text_selection.selected_rendered_range_for_id(self.id)
                    }) {
                        Ok(range) => range,
                        Err(error) => {
                            log::trace!("Failed to register selectable text layout: {error:?}");
                            None
                        }
                    },
                );

            if let Some(selected_range) = selected_range {
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
                state.text_selection.clear_layouts();
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
                                    state.text_selection.set_selection_bounds(bounds);
                                });

                                window.on_mouse_event({
                                    let interaction_state = interaction_state.clone();

                                    move |event: &MouseMoveEvent, phase, _, cx| {
                                        if phase == DispatchPhase::Bubble {
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
    use std::ops::{Deref, DerefMut, Range};

    use settings::SettingsStore;
    use theme::LoadThemes;
    use util::test;

    use crate::{Indicator, TextSelectionPoint, components::text::selection::TextSelection};

    fn init_test(cx: &mut TestAppContext) {
        cx.update(|cx| {
            let settings_store = SettingsStore::test_new(cx);
            cx.set_global(settings_store);
            theme::init(LoadThemes::JustBase, cx);
        });
    }

    struct SelectableTextTestContext {
        cx: VisualTestContext,
        view: Entity<TestSelectableTextGroup>,
    }

    impl SelectableTextTestContext {
        fn new(cx: &mut TestAppContext) -> Self {
            let window = cx.add_window(|_, cx| TestSelectableTextGroup::new(cx));
            let mut cx = VisualTestContext::from_window(window.into(), cx);
            let view = window.root(&mut cx).unwrap();
            Self { cx, view }
        }

        fn set_single_line(&mut self, single_line: bool) {
            self.view.update(&mut self.cx, |view, cx| {
                view.single_line = single_line;
                cx.notify();
            });
        }

        #[track_caller]
        fn set_state(&mut self, marked_items: impl IntoIterator<Item: AsRef<str>>) {
            let (items, selection) = marked_text_state(marked_items);
            self.view.update(&mut self.cx, |view, cx| {
                view.items = items;
                view.interaction_state.update(cx, |state, _| {
                    state.clear_text_selection();
                    if let Some(selection) = selection {
                        state
                            .text_selection
                            .select_all(selection.start, selection.end);
                    }
                });
                cx.notify();
            });
        }

        fn selection(&self) -> Option<TextSelection<usize>> {
            self.view.read_with(&self.cx, |view, cx| {
                view.interaction_state
                    .read(cx)
                    .text_selection
                    .selection
                    .clone()
            })
        }

        fn selected_text(&mut self) -> Option<String> {
            self.view.update_in(&mut self.cx, |view, window, cx| {
                let selection_order = (0..view.items.len()).collect::<Vec<_>>();
                let text_for_selection =
                    |item_id, _: &mut Window, _: &mut App| view.items.get(item_id).cloned();
                view.interaction_state.update(cx, |state, cx| {
                    state.selected_text(
                        &selection_order,
                        view.copy_separator.as_ref(),
                        &text_for_selection,
                        window,
                        cx,
                    )
                })
            })
        }

        #[track_caller]
        fn group_bounds(&mut self) -> Bounds<Pixels> {
            self.cx.debug_bounds("SELECTABLE_TEXT_GROUP").unwrap()
        }

        #[track_caller]
        fn pixel_position_for(&self, point: TextSelectionPoint<usize>) -> Point<Pixels> {
            self.view.read_with(&self.cx, |view, cx| {
                view.interaction_state
                    .read(cx)
                    .text_selection
                    .position_for_id_offset(point.id, point.offset)
                    .unwrap()
            })
        }

        #[track_caller]
        fn assert_state(&self, marked_items: impl IntoIterator<Item: AsRef<str>>) {
            let (expected_items, expected_selection) = marked_text_state(marked_items);
            let selection = self.selection();
            self.view.read_with(&self.cx, |view, _| {
                pretty_assertions::assert_eq!(view.items, expected_items);
                let normalize_point = |mut point: TextSelectionPoint<usize>| {
                    let source = view
                        .items
                        .get(point.id)
                        .expect("selection point should reference an existing item");
                    if point.offset == usize::MAX {
                        point.offset = source.len();
                    }
                    assert!(source.is_char_boundary(point.offset));
                    point
                };
                let selection = selection.map(|selection| {
                    normalize_point(selection.tail())..normalize_point(selection.head())
                });
                pretty_assertions::assert_eq!(selection, expected_selection);
            });
        }
    }

    impl Deref for SelectableTextTestContext {
        type Target = VisualTestContext;

        fn deref(&self) -> &Self::Target {
            &self.cx
        }
    }

    impl DerefMut for SelectableTextTestContext {
        fn deref_mut(&mut self) -> &mut Self::Target {
            &mut self.cx
        }
    }

    struct TestSelectableTextGroup {
        interaction_state: Entity<TextInteractionState<usize>>,
        items: Vec<SharedString>,
        copy_separator: SharedString,
        single_line: bool,
    }

    impl TestSelectableTextGroup {
        fn new(cx: &mut Context<Self>) -> Self {
            Self {
                interaction_state: cx.new(|cx| TextInteractionState::new(cx)),
                items: Vec::new(),
                copy_separator: "\t".into(),
                single_line: false,
            }
        }
    }

    impl Render for TestSelectableTextGroup {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            let interaction_state = self.interaction_state.clone();
            let copy_separator = self.copy_separator.clone();
            let single_line = self.single_line;
            let item_texts = self
                .items
                .iter()
                .enumerate()
                .map(|(id, text)| (id, text.clone()))
                .collect::<Vec<_>>();

            gpui::div().w(gpui::px(500.0)).h(gpui::px(64.0)).child(
                SelectableTextGroup::new(&interaction_state)
                    .debug_selector(|| "SELECTABLE_TEXT_GROUP".into())
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
                                    .when(single_line, |this| this.single_line())
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

    #[track_caller]
    fn marked_text_state(
        marked_items: impl IntoIterator<Item: AsRef<str>>,
    ) -> (Vec<SharedString>, Option<Range<TextSelectionPoint<usize>>>) {
        let marked_items = marked_items
            .into_iter()
            .map(|item| item.as_ref().replace('•', " "))
            .collect::<Vec<_>>();
        // Keep an item's end distinct from the next item's start, including empty items.
        let marked_text = marked_items.join("\n");
        pretty_assertions::assert_eq!(
            marked_text.matches('«').count(),
            marked_text.matches('»').count(),
        );
        let (_, mut selections) = test::marked_text_ranges(&marked_text, true);
        assert!(selections.len() <= 1, "expected at most one selection");
        let items = marked_items
            .iter()
            .map(|item| item.replace(['«', '»', 'ˇ'], "").into())
            .collect::<Vec<SharedString>>();
        let selection = selections.pop().map(|selection| {
            let point_for_offset = |offset| {
                let mut start = 0;
                items
                    .iter()
                    .enumerate()
                    .find_map(|(id, item)| {
                        let end = start + item.len();
                        let point = (start..=end)
                            .contains(&offset)
                            .then(|| TextSelectionPoint::new(id, offset - start));
                        start = end + 1;
                        point
                    })
                    .expect("selection endpoint should be inside an item")
            };
            point_for_offset(selection.start)..point_for_offset(selection.end)
        });
        (items, selection)
    }

    fn simulate_drag(cx: &mut VisualTestContext, start: Point<Pixels>, end: Point<Pixels>) {
        cx.simulate_mouse_down(start, MouseButton::Left, Modifiers::default());
        cx.simulate_mouse_move(end, MouseButton::Left, Modifiers::default());
        cx.simulate_mouse_up(end, MouseButton::Left, Modifiers::default());
    }

    #[gpui::test]
    fn test_selectable_text_group_select_all(cx: &mut TestAppContext) {
        init_test(cx);
        let mut cx = SelectableTextTestContext::new(cx);
        cx.set_state(["foo", "bar", "baz"]);

        let group_bounds = cx.group_bounds();
        cx.simulate_click(group_bounds.center(), Modifiers::default());
        cx.dispatch_action(actions::text::SelectAll);

        cx.assert_state(["«foo", "bar", "bazˇ»"]);
        pretty_assertions::assert_eq!(cx.selected_text().as_deref(), Some("foo\tbar\tbaz"));
    }

    #[gpui::test]
    fn test_selectable_text_group_drag_selects_from_padding(cx: &mut TestAppContext) {
        init_test(cx);
        let mut cx = SelectableTextTestContext::new(cx);
        cx.set_state(["foo", "bar", "baz"]);

        let group_bounds = cx.group_bounds();
        let inside_group_offset = gpui::px(2.0);
        let start = gpui::point(
            group_bounds.left() + inside_group_offset,
            group_bounds.center().y,
        );
        let end = gpui::point(
            group_bounds.right() - inside_group_offset,
            group_bounds.center().y,
        );
        simulate_drag(&mut cx, start, end);

        cx.assert_state(["«foo", "bar", "bazˇ»"]);
        pretty_assertions::assert_eq!(cx.selected_text().as_deref(), Some("foo\tbar\tbaz"));
    }

    #[gpui::test]
    fn test_selectable_text_group_drag_below_selects_from_padding(cx: &mut TestAppContext) {
        init_test(cx);
        let mut cx = SelectableTextTestContext::new(cx);
        cx.set_state(["foo", "bar", "baz"]);

        let group_bounds = cx.group_bounds();
        let inside_group_offset = gpui::px(2.0);
        let outside_group_offset = gpui::px(5.0);
        let start = gpui::point(
            group_bounds.left() + inside_group_offset,
            group_bounds.center().y,
        );
        let end = gpui::point(
            group_bounds.center().x,
            group_bounds.bottom() + outside_group_offset,
        );
        simulate_drag(&mut cx, start, end);

        cx.assert_state(["«foo", "bar", "bazˇ»"]);
        pretty_assertions::assert_eq!(cx.selected_text().as_deref(), Some("foo\tbar\tbaz"));
    }

    #[gpui::test]
    fn test_selectable_text_group_drag_above_selects_from_padding(cx: &mut TestAppContext) {
        init_test(cx);
        let mut cx = SelectableTextTestContext::new(cx);
        cx.set_state(["foo", "bar", "baz"]);

        let group_bounds = cx.group_bounds();
        let inside_group_offset = gpui::px(2.0);
        let outside_group_offset = gpui::px(5.0);
        let start = gpui::point(
            group_bounds.right() - inside_group_offset,
            group_bounds.center().y,
        );
        let end = gpui::point(
            group_bounds.center().x,
            group_bounds.top() - outside_group_offset,
        );
        simulate_drag(&mut cx, start, end);

        cx.assert_state(["«ˇfoo", "bar", "baz»"]);
        pretty_assertions::assert_eq!(cx.selected_text().as_deref(), Some("foo\tbar\tbaz"));
    }

    #[gpui::test]
    fn test_selectable_text_group_drag_below_selects_from_text_start(cx: &mut TestAppContext) {
        init_test(cx);
        let mut cx = SelectableTextTestContext::new(cx);
        cx.set_state(["foo", "ˇbar", "baz"]);

        let group_bounds = cx.group_bounds();
        let outside_group_offset = gpui::px(5.0);
        let selection = cx.selection().unwrap();
        let start = gpui::point(
            cx.pixel_position_for(selection.head()).x + gpui::px(1.0),
            group_bounds.center().y,
        );
        let end = gpui::point(
            group_bounds.center().x,
            group_bounds.bottom() + outside_group_offset,
        );
        simulate_drag(&mut cx, start, end);

        cx.assert_state(["foo", "«bar", "bazˇ»"]);
        pretty_assertions::assert_eq!(cx.selected_text().as_deref(), Some("bar\tbaz"));
    }

    #[gpui::test]
    fn test_selectable_text_group_drag_above_selects_from_text_start(cx: &mut TestAppContext) {
        init_test(cx);
        let mut cx = SelectableTextTestContext::new(cx);
        cx.set_state(["foo", "ˇbar", "baz"]);

        let group_bounds = cx.group_bounds();
        let outside_group_offset = gpui::px(5.0);
        let selection = cx.selection().unwrap();
        let start = gpui::point(
            cx.pixel_position_for(selection.head()).x + gpui::px(1.0),
            group_bounds.center().y,
        );
        let end = gpui::point(
            group_bounds.center().x,
            group_bounds.top() - outside_group_offset,
        );
        simulate_drag(&mut cx, start, end);

        cx.assert_state(["«ˇfoo", "»bar", "baz"]);
        pretty_assertions::assert_eq!(cx.selected_text().as_deref(), Some("foo"));
    }

    #[gpui::test]
    fn test_selectable_text_group_drag_below_selects_from_text_offset(cx: &mut TestAppContext) {
        init_test(cx);
        let mut cx = SelectableTextTestContext::new(cx);
        cx.set_state(["foo", "bˇar", "baz"]);

        let group_bounds = cx.group_bounds();
        let outside_group_offset = gpui::px(5.0);
        let selection = cx.selection().unwrap();
        let start = gpui::point(
            cx.pixel_position_for(selection.head()).x,
            group_bounds.center().y,
        );
        let end = gpui::point(
            group_bounds.center().x,
            group_bounds.bottom() + outside_group_offset,
        );
        simulate_drag(&mut cx, start, end);

        cx.assert_state(["foo", "b«ar", "bazˇ»"]);
        pretty_assertions::assert_eq!(cx.selected_text().as_deref(), Some("ar\tbaz"));
    }

    #[gpui::test]
    fn test_selectable_text_group_drag_above_selects_from_text_offset(cx: &mut TestAppContext) {
        init_test(cx);
        let mut cx = SelectableTextTestContext::new(cx);
        cx.set_state(["foo", "bˇar", "baz"]);

        let group_bounds = cx.group_bounds();
        let outside_group_offset = gpui::px(5.0);
        let selection = cx.selection().unwrap();
        let start = gpui::point(
            cx.pixel_position_for(selection.head()).x,
            group_bounds.center().y,
        );
        let end = gpui::point(
            group_bounds.center().x,
            group_bounds.top() - outside_group_offset,
        );
        simulate_drag(&mut cx, start, end);

        cx.assert_state(["«ˇfoo", "b»ar", "baz"]);
        pretty_assertions::assert_eq!(cx.selected_text().as_deref(), Some("foo\tb"));
    }

    #[gpui::test]
    fn test_single_line_select_all_preserves_control_characters(cx: &mut TestAppContext) {
        init_test(cx);
        let mut cx = SelectableTextTestContext::new(cx);
        cx.set_single_line(true);
        cx.set_state(["föö\nö\t bár🚀", "\r\0\x7f", "a\u{0085}b"]);

        let group_bounds = cx.group_bounds();
        cx.simulate_click(group_bounds.center(), Modifiers::default());
        cx.dispatch_action(actions::text::SelectAll);

        cx.assert_state(["«föö\nö\t bár🚀", "\r\0\x7f", "a\u{0085}bˇ»"]);
        pretty_assertions::assert_eq!(
            cx.selected_text().as_deref(),
            Some("föö\nö\t bár🚀\t\r\0\x7f\ta\u{0085}b"),
        );
    }

    #[gpui::test]
    fn test_single_line_partial_selection_preserves_control_characters(cx: &mut TestAppContext) {
        init_test(cx);
        let mut cx = SelectableTextTestContext::new(cx);
        cx.set_single_line(true);
        cx.set_state(["fööˇ\nö\t bár🚀"]);

        let group_bounds = cx.group_bounds();
        let selection = cx.selection().unwrap();
        let start = gpui::point(
            cx.pixel_position_for(selection.head()).x,
            group_bounds.center().y,
        );
        let end = gpui::point(
            cx.pixel_position_for(TextSelectionPoint::new(0, "föö\nö\t".len()))
                .x,
            group_bounds.center().y,
        );

        simulate_drag(&mut cx, start, end);

        cx.assert_state(["föö«\nö\tˇ» bár🚀"]);
        pretty_assertions::assert_eq!(cx.selected_text().as_deref(), Some("\nö\t"));

        simulate_drag(&mut cx, end, start);

        cx.assert_state(["föö«ˇ\nö\t» bár🚀"]);
        pretty_assertions::assert_eq!(cx.selected_text().as_deref(), Some("\nö\t"));
    }
}
