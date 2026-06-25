mod head;
mod render;
mod shape;

use gpui::{
    AnyElement, App, ClickEvent, Context, DismissEvent, Div, EventEmitter, FocusHandle, Focusable,
    IntoElement, ListSizingBehavior, MouseButton, MouseUpEvent, Rems, ScrollStrategy, SharedString,
    Task, UniformListScrollHandle, Window, prelude::*,
};
use std::{ops::Range, sync::Arc, time::Duration};

use input::{ErasedEditor, ErasedEditorEvent};
use theme::ActiveTheme;
use workspace::ModalView;

use crate::{
    head::Head,
    shape::{RelativeHeight, RelativeWidth, Shape, SizeBounds, VerticalPadding},
};

enum ElementContainer {
    UniformList(UniformListScrollHandle),
}

pub enum Direction {
    Up,
    Down,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScrollBehavior {
    RevealSelected,
    PreserveOffset,
}

struct PendingUpdateMatches {
    delegate_update_matches: Option<Task<()>>,
    _task: Task<anyhow::Result<()>>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub enum PickerEditorPosition {
    #[default]
    Start,
    End,
}

pub trait PickerDelegate: Sized + 'static {
    type ListItem: IntoElement;

    fn name() -> &'static str;
    fn match_count(&self) -> usize;
    fn selected_index(&self) -> usize;

    fn separators_after_indices(&self) -> Vec<usize> {
        Vec::new()
    }

    fn set_selected_index(
        &mut self,
        index: usize,
        window: &mut Window,
        cx: &mut Context<Picker<Self>>,
    );

    fn select_history(
        &mut self,
        _: Direction,
        _: &str,
        _: &mut Window,
        _: &mut App,
    ) -> Option<String> {
        None
    }

    fn can_select(&self, _: usize, _: &mut Window, _: &mut Context<Picker<Self>>) -> bool {
        true
    }

    fn select_on_hover(&self) -> bool {
        true
    }

    fn selected_index_changed(
        &self,
        _: usize,
        _: &mut Window,
        _: &mut Context<Picker<Self>>,
    ) -> Option<Box<dyn Fn(&mut Window, &mut App) + 'static>> {
        None
    }

    fn placeholder_text(&self, _: &mut Window, _: &mut App) -> Arc<str>;

    fn no_matches_text(&self, _: &mut Window, _: &mut App) -> Option<SharedString> {
        Some("No matches".into())
    }

    fn update_matches(
        &mut self,
        query: String,
        window: &mut Window,
        cx: &mut Context<Picker<Self>>,
    ) -> Task<()>;

    fn finalize_update_matches(
        &mut self,
        _: String,
        _: Duration,
        _: &mut Window,
        _: &mut Context<Picker<Self>>,
    ) -> bool {
        false
    }

    fn confirm_update_query(
        &mut self,
        _: &mut Window,
        _: &mut Context<Picker<Self>>,
    ) -> Option<String> {
        None
    }

    fn confirm(&mut self, secondary: bool, window: &mut Window, cx: &mut Context<Picker<Self>>);
    fn dismissed(&mut self, window: &mut Window, cx: &mut Context<Picker<Self>>);

    fn should_dismiss(&self) -> bool {
        true
    }

    fn editor_position(&self) -> PickerEditorPosition {
        PickerEditorPosition::default()
    }

    fn searchbar_trailer(
        &self,
        _: &mut Window,
        _: &mut Context<Picker<Self>>,
    ) -> Option<AnyElement> {
        None
    }

    fn render_editor(
        &self,
        editor: &Arc<dyn ErasedEditor>,
        window: &mut Window,
        cx: &mut Context<Picker<Self>>,
    ) -> Div {
        gpui::div()
            .flex()
            .flex_col()
            .when(
                self.editor_position() == PickerEditorPosition::End,
                |this| {
                    this.border_t_1()
                        .border_color(cx.theme().colors().border_variant)
                },
            )
            .child(
                gpui::div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .overflow_hidden()
                    .flex_none()
                    .h_9()
                    .px_2p5()
                    .child(gpui::div().flex_1().child(editor.render(window, cx)))
                    .children(self.searchbar_trailer(window, cx)),
            )
            .when(
                self.editor_position() == PickerEditorPosition::Start,
                |this| {
                    this.border_b_1()
                        .border_color(cx.theme().colors().border_variant)
                },
            )
    }

    fn render_match(
        &self,
        index: usize,
        selected: bool,
        window: &mut Window,
        cx: &mut Context<Picker<Self>>,
    ) -> Option<Self::ListItem>;

    fn render_header(&self, _: &mut Window, _: &mut Context<Picker<Self>>) -> Option<AnyElement> {
        None
    }

    fn render_footer(&self, _: &mut Window, _: &mut Context<Picker<Self>>) -> Option<AnyElement> {
        None
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
enum ContainerKind {
    UniformList,
}

pub struct Picker<D: PickerDelegate> {
    pub delegate: D,
    element_container: ElementContainer,
    head: Head,
    pending_update_matches: Option<PendingUpdateMatches>,
    confirm_on_update: Option<bool>,
    shape: Shape,
    vertical_padding: VerticalPadding,
    size_bounds: SizeBounds,
    show_scrollbar: bool,
    is_modal: bool,
}

impl<D: PickerDelegate> Picker<D> {
    pub fn uniform_list(delegate: D, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let placeholder_text = delegate.placeholder_text(window, cx);
        let head = Head::editor(
            placeholder_text.as_ref(),
            Self::on_input_editor_event,
            window,
            cx,
        );

        Self::new(delegate, ContainerKind::UniformList, head, window, cx)
    }

    pub fn nonsearchable_uniform_list(
        delegate: D,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let head = Head::empty(Self::on_empty_head_blur, window, cx);

        Self::new(delegate, ContainerKind::UniformList, head, window, cx)
    }

    fn new(
        delegate: D,
        container: ContainerKind,
        head: Head,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let element_container = Self::create_element_container(container);
        let mut this = Self {
            delegate,
            element_container,
            head,
            pending_update_matches: None,
            confirm_on_update: None,
            shape: Shape::default(),
            vertical_padding: VerticalPadding::default(),
            size_bounds: SizeBounds::default(),
            show_scrollbar: false,
            is_modal: true,
        };
        this.update_matches(String::new(), window, cx);
        this.delegate
            .finalize_update_matches(String::new(), Duration::from_millis(4), window, cx);
        this
    }

    fn create_element_container(container: ContainerKind) -> ElementContainer {
        match container {
            ContainerKind::UniformList => {
                ElementContainer::UniformList(UniformListScrollHandle::new())
            }
        }
    }

    pub fn initial_width(mut self, width: impl Into<RelativeWidth>) -> Self {
        self.shape.set_initial_width(width);
        self
    }

    pub fn minimum_results_width(mut self, width: impl Into<Rems>) -> Self {
        self.size_bounds.min_results.width = width.into();
        self
    }

    pub fn height(mut self, height: impl Into<RelativeHeight>) -> Self {
        self.shape.set_initial_height(height);
        self
    }

    pub fn no_vertical_padding(mut self) -> Self {
        self.vertical_padding = VerticalPadding::None;
        self
    }

    pub(crate) fn vertical_padding(&self) -> VerticalPadding {
        self.vertical_padding
    }

    pub fn show_scrollbar(mut self, show_scrollbar: bool) -> Self {
        self.show_scrollbar = show_scrollbar;
        self
    }

    pub fn modal(mut self, modal: bool) -> Self {
        self.is_modal = modal;
        self
    }

    pub fn focus(&self, window: &mut Window, cx: &mut App) {
        self.focus_handle(cx).focus(window, cx);
    }

    pub fn set_selected_index(
        &mut self,
        mut index: usize,
        fallback_direction: Option<Direction>,
        scroll_to_index: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let match_count = self.delegate.match_count();
        if match_count == 0 {
            return;
        }

        if let Some(direction) = fallback_direction {
            let mut current_index = index;
            while !self.delegate.can_select(current_index, window, cx) {
                current_index = match direction {
                    Direction::Down => {
                        if current_index == match_count - 1 {
                            0
                        } else {
                            current_index + 1
                        }
                    }
                    Direction::Up => {
                        if current_index == 0 {
                            match_count - 1
                        } else {
                            current_index - 1
                        }
                    }
                };
                if index == current_index {
                    return;
                }
            }
            index = current_index;
        } else if !self.delegate.can_select(index, window, cx) {
            return;
        }

        let previous_index = self.delegate.selected_index();
        self.delegate.set_selected_index(index, window, cx);
        let current_index = self.delegate.selected_index();

        if previous_index != current_index {
            if let Some(action) = self.delegate.selected_index_changed(index, window, cx) {
                action(window, cx);
            }
            if scroll_to_index {
                self.scroll_to_item_index(index);
            }
        }
    }

    pub fn select_next(
        &mut self,
        _: &actions::menu::SelectNext,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let query = self.query(cx);
        if let Some(query) = self
            .delegate
            .select_history(Direction::Down, &query, window, cx)
        {
            self.set_query(&query, window, cx);
            return;
        }
        let count = self.delegate.match_count();
        if count > 0 {
            let index = self.delegate.selected_index();
            let next_index = if index == count - 1 { 0 } else { index + 1 };
            self.set_selected_index(next_index, Some(Direction::Down), true, window, cx);
            cx.notify();
        }
    }

    pub fn editor_move_up(
        &mut self,
        _: &actions::editor::MoveUp,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.select_previous(&actions::menu::SelectPrevious, window, cx);
    }

    fn select_previous(
        &mut self,
        _: &actions::menu::SelectPrevious,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let query = self.query(cx);
        if let Some(query) = self
            .delegate
            .select_history(Direction::Up, &query, window, cx)
        {
            self.set_query(&query, window, cx);
            return;
        }
        let count = self.delegate.match_count();
        if count > 0 {
            let index = self.delegate.selected_index();
            let previous_index = if index == 0 { count - 1 } else { index - 1 };
            self.set_selected_index(previous_index, Some(Direction::Up), true, window, cx);
            cx.notify();
        }
    }

    pub fn editor_move_down(
        &mut self,
        _: &actions::editor::MoveDown,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.select_next(&actions::menu::SelectNext, window, cx);
    }

    pub fn select_first(
        &mut self,
        _: &actions::menu::SelectFirst,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let count = self.delegate.match_count();
        if count > 0 {
            self.set_selected_index(0, Some(Direction::Down), true, window, cx);
            cx.notify();
        }
    }

    fn select_last(
        &mut self,
        _: &actions::menu::SelectLast,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let count = self.delegate.match_count();
        if count > 0 {
            self.set_selected_index(count - 1, Some(Direction::Up), true, window, cx);
            cx.notify();
        }
    }

    pub fn cycle_selection(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let count = self.delegate.match_count();
        let index = self.delegate.selected_index();
        let new_index = if index + 1 == count { 0 } else { index + 1 };
        self.set_selected_index(new_index, Some(Direction::Down), true, window, cx);
        cx.notify();
    }

    pub fn cancel(
        &mut self,
        _: &actions::menu::Cancel,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.delegate.should_dismiss() {
            self.delegate.dismissed(window, cx);
            cx.emit(DismissEvent);
        }
    }

    fn confirm(&mut self, _: &actions::menu::Confirm, window: &mut Window, cx: &mut Context<Self>) {
        if self.pending_update_matches.is_some()
            && !self.delegate.finalize_update_matches(
                self.query(cx),
                Duration::from_millis(16),
                window,
                cx,
            )
        {
            self.confirm_on_update = Some(false);
        } else {
            self.pending_update_matches.take();
            self.do_confirm(false, window, cx);
        }
    }

    fn secondary_confirm(
        &mut self,
        _: &actions::menu::SecondaryConfirm,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.pending_update_matches.is_some()
            && !self.delegate.finalize_update_matches(
                self.query(cx),
                Duration::from_millis(16),
                window,
                cx,
            )
        {
            self.confirm_on_update = Some(true);
        } else {
            self.do_confirm(true, window, cx);
        }
    }

    fn handle_click(
        &mut self,
        index: usize,
        secondary: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        cx.stop_propagation();
        window.prevent_default();
        if !self.delegate.can_select(index, window, cx) {
            return;
        }
        self.set_selected_index(index, None, false, window, cx);
        self.do_confirm(secondary, window, cx);
    }

    fn do_confirm(&mut self, secondary: bool, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(update_query) = self.delegate.confirm_update_query(window, cx) {
            self.set_query(&update_query, window, cx);
            self.set_selected_index(0, Some(Direction::Down), false, window, cx);
        } else {
            self.delegate.confirm(secondary, window, cx);
        }
    }

    fn on_input_editor_event(
        &mut self,
        editor: &dyn ErasedEditor,
        event: ErasedEditorEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match event {
            ErasedEditorEvent::BufferEdited => {
                let query = editor.text(cx);
                self.update_matches(query, window, cx);
            }
            ErasedEditorEvent::Blurred => {
                if self.is_modal && window.is_window_active() {
                    self.cancel(&actions::menu::Cancel, window, cx);
                }
            }
        }
    }

    fn on_empty_head_blur(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if window.is_window_active() {
            self.cancel(&actions::menu::Cancel, window, cx);
        }
    }

    pub fn refresh(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let query = self.query(cx);
        self.update_matches(query, window, cx);
    }

    pub fn update_matches(&mut self, query: String, window: &mut Window, cx: &mut Context<Self>) {
        self.update_matches_with_options(query, ScrollBehavior::RevealSelected, window, cx);
    }

    pub fn update_matches_with_options(
        &mut self,
        query: String,
        scroll_behavior: ScrollBehavior,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let delegate_pending_update_matches = self.delegate.update_matches(query, window, cx);

        self.matches_updated(scroll_behavior, window, cx);
        self.pending_update_matches = Some(PendingUpdateMatches {
            delegate_update_matches: Some(delegate_pending_update_matches),
            _task: cx.spawn_in(window, async move |this, cx| {
                let delegate_pending_update_matches = this.update(cx, |this, _| {
                    this.pending_update_matches
                        .as_mut()
                        .and_then(|pending| pending.delegate_update_matches.take())
                })?;

                let Some(delegate_pending_update_matches) = delegate_pending_update_matches else {
                    return Ok(());
                };

                delegate_pending_update_matches.await;
                this.update_in(cx, |this, window, cx| {
                    this.matches_updated(scroll_behavior, window, cx);
                })?;

                Ok(())
            }),
        });
    }

    fn matches_updated(
        &mut self,
        scroll_behavior: ScrollBehavior,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match scroll_behavior {
            ScrollBehavior::RevealSelected => {
                let index = self.delegate.selected_index();
                self.scroll_to_item_index(index);
            }
            ScrollBehavior::PreserveOffset => {}
        }
        self.pending_update_matches = None;
        if let Some(secondary) = self.confirm_on_update.take() {
            self.do_confirm(secondary, window, cx);
        }
        cx.notify();
    }

    pub fn query(&self, cx: &App) -> String {
        match &self.head {
            Head::Editor(input) => input.text(cx),
            Head::Empty(_) => String::new(),
        }
    }

    pub fn set_query(&self, query: &str, window: &mut Window, cx: &mut App) {
        if let Head::Editor(input) = &self.head {
            input.set_text(query, window, cx);
            input.move_selection_to_end(window, cx);
        }
    }

    fn scroll_to_item_index(&mut self, index: usize) {
        match &mut self.element_container {
            ElementContainer::UniformList(scroll_handle) => {
                scroll_handle.scroll_to_item(index, ScrollStrategy::Nearest);
            }
        }
    }

    pub fn is_scrolled_to_end(&self) -> Option<bool> {
        match &self.element_container {
            ElementContainer::UniformList(scroll_handle) => scroll_handle.is_scrolled_to_end(),
        }
    }

    fn render_element(
        &self,
        window: &mut Window,
        cx: &mut Context<Self>,
        index: usize,
    ) -> impl IntoElement + use<D> {
        let selectable =
            index < self.delegate.match_count() && self.delegate.can_select(index, window, cx);

        gpui::div()
            .id(("item", index))
            .when(selectable, |this| this.cursor_pointer())
            .when(!self.delegate.select_on_hover(), |this| {
                this.on_mouse_down(MouseButton::Left, |_, window, _| {
                    window.prevent_default();
                })
            })
            .on_click(cx.listener(move |this, event: &ClickEvent, window, cx| {
                this.handle_click(index, event.modifiers().secondary(), window, cx);
            }))
            .on_mouse_up(
                MouseButton::Right,
                cx.listener(move |this, event: &MouseUpEvent, window, cx| {
                    this.handle_click(index, event.modifiers.platform, window, cx);
                }),
            )
            .when(self.delegate.select_on_hover(), |this| {
                this.on_hover(cx.listener(move |this, hovered: &bool, window, cx| {
                    if *hovered {
                        this.set_selected_index(index, None, false, window, cx);
                        cx.notify();
                    }
                }))
            })
            .children(self.delegate.render_match(
                index,
                index == self.delegate.selected_index(),
                window,
                cx,
            ))
            .when(
                self.delegate.separators_after_indices().contains(&index),
                |picker| {
                    picker
                        .border_color(cx.theme().colors().border_variant)
                        .border_b_1()
                        .py(gpui::px(-1.0))
                },
            )
    }

    pub(crate) fn render_element_container(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let sizing_behavior = match self.vertical_padding() {
            VerticalPadding::None => ListSizingBehavior::Infer,
            VerticalPadding::Pad => ListSizingBehavior::Auto,
        };

        match &self.element_container {
            ElementContainer::UniformList(scroll_handle) => gpui::uniform_list(
                "candidates",
                self.delegate.match_count(),
                cx.processor(move |picker, visible_range: Range<usize>, window, cx| {
                    visible_range
                        .map(|index| picker.render_element(window, cx, index))
                        .collect::<Vec<_>>()
                }),
            )
            .with_sizing_behavior(sizing_behavior)
            .flex_grow_1()
            .py_1()
            .track_scroll(scroll_handle)
            .into_any_element(),
        }
    }
}

impl<D: PickerDelegate> Focusable for Picker<D> {
    fn focus_handle(&self, cx: &App) -> FocusHandle {
        match &self.head {
            Head::Editor(input) => input.focus_handle(cx),
            Head::Empty(head) => head.focus_handle(cx),
        }
    }
}

impl<D: PickerDelegate> EventEmitter<DismissEvent> for Picker<D> {}

impl<D: PickerDelegate> ModalView for Picker<D> {}
