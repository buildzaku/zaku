use gpui::{
    AnyElement, App, Bounds, CursorStyle, DispatchPhase, Div, Element, ElementId, Entity,
    FontWeight, GlobalElementId, Hitbox, InspectorElementId, LayoutId, MouseButton, MouseMoveEvent,
    MouseUpEvent, Pixels, RenderOnce, SharedString, StyleRefinement, Styled, StyledText,
    WeakEntity, Window, prelude::*,
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
            id,
            text,
            style,
        } = self;
        let interaction_state = interaction_state.upgrade();
        let selected_range = interaction_state
            .as_ref()
            .and_then(|state| state.read(cx).selected_range_for_text(id, text.as_ref()));
        let element = SelectableTextElement {
            interaction_state: interaction_state.as_ref().map(Entity::downgrade),
            id,
            text: text.clone(),
            styled_text: StyledText::new(text),
            selected_range,
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
        insert_text_hitboxes(self.styled_text.layout(), window)
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
}

impl<T: Copy + Ord + 'static> Styled for SelectableTextGroup<T> {
    fn style(&mut self) -> &mut StyleRefinement {
        self.base.style()
    }
}

impl<T: Copy + Ord + 'static> RenderOnce for SelectableTextGroup<T> {
    fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        let Self {
            base,
            interaction_state,
            selection_order,
            copy_separator,
            text_for_selection,
            child,
        } = self;
        let interaction_state = interaction_state.upgrade();
        let focus_handle = interaction_state
            .as_ref()
            .map(|state| state.read(cx).focus_handle());
        if let Some(interaction_state) = interaction_state.as_ref() {
            interaction_state.update(cx, |state, _| {
                state.clear_text_layouts();
            });
        }
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
                    .on_action({
                        let interaction_state = interaction_state.clone();
                        let selection_order = selection_order.clone();
                        let copy_separator = copy_separator.clone();
                        let text_for_selection = text_for_selection.clone();

                        move |_: &actions::editor::Copy, window: &mut Window, cx: &mut App| {
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

                        move |_: &actions::editor::SelectAll, _: &mut Window, cx: &mut App| {
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
    }
}
