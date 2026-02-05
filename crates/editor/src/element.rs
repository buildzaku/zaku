use gpui::{
    Action, App, Bounds, Context, CursorStyle, DispatchPhase, Element, ElementId,
    ElementInputHandler, Entity, GlobalElementId, Hitbox, HitboxBehavior, MouseButton,
    MouseDownEvent, MouseMoveEvent, MouseUpEvent, PaintQuad, Pixels, ShapedLine, Style, TextRun,
    UnderlineStyle, Window, fill, point, prelude::*, px, size,
};
use std::any::TypeId;

use theme::ActiveTheme;

use crate::{Editor, EditorStyle, HandleInput};

pub struct EditorElement {
    editor: Entity<Editor>,
    style: EditorStyle,
}

impl EditorElement {
    pub fn new(editor: &Entity<Editor>, style: EditorStyle) -> Self {
        Self {
            editor: editor.clone(),
            style,
        }
    }

    fn register_actions(&self, window: &mut Window) {
        let editor = &self.editor;
        register_action(editor, window, Editor::move_left);
        register_action(editor, window, Editor::move_right);
        register_action(editor, window, Editor::select_left);
        register_action(editor, window, Editor::select_right);
        register_action(editor, window, Editor::select_all);
        register_action(editor, window, Editor::move_to_beginning);
        register_action(editor, window, Editor::move_to_end);
        register_action(editor, window, Editor::select_to_beginning);
        register_action(editor, window, Editor::select_to_end);
        register_action(editor, window, Editor::move_to_previous_word_start);
        register_action(editor, window, Editor::move_to_next_word_end);
        register_action(editor, window, Editor::move_to_previous_subword_start);
        register_action(editor, window, Editor::move_to_next_subword_end);
        register_action(editor, window, Editor::select_to_previous_word_start);
        register_action(editor, window, Editor::select_to_next_word_end);
        register_action(editor, window, Editor::select_to_previous_subword_start);
        register_action(editor, window, Editor::select_to_next_subword_end);
        register_action(editor, window, Editor::delete_to_previous_word_start);
        register_action(editor, window, Editor::delete_to_previous_subword_start);
        register_action(editor, window, Editor::delete_to_next_word_end);
        register_action(editor, window, Editor::delete_to_next_subword_end);
        register_action(editor, window, Editor::backspace);
        register_action(editor, window, Editor::delete);
        register_action(editor, window, Editor::copy);
        register_action(editor, window, Editor::cut);
        register_action(editor, window, Editor::paste);
        register_action(editor, window, Editor::undo);
        register_action(editor, window, Editor::redo);
        register_action(editor, window, Editor::undo_selection);
        register_action(editor, window, Editor::redo_selection);
        register_action(editor, window, Editor::move_to_beginning_of_line);
        register_action(editor, window, Editor::move_to_end_of_line);
        register_action(editor, window, Editor::select_to_beginning_of_line);
        register_action(editor, window, Editor::select_to_end_of_line);
        register_action(editor, window, Editor::delete_to_beginning_of_line);
        register_action(editor, window, Editor::delete_to_end_of_line);
        register_action(
            editor,
            window,
            |editor, action: &HandleInput, window, cx| {
                editor.handle_input(&action.0, window, cx);
            },
        );
    }
}

#[derive(Default)]
pub struct PrepaintState {
    line: Option<ShapedLine>,
    cursor: Option<PaintQuad>,
    selection: Option<PaintQuad>,
    hitbox: Option<Hitbox>,
}

impl IntoElement for EditorElement {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl Element for EditorElement {
    type RequestLayoutState = ();
    type PrepaintState = PrepaintState;

    fn id(&self) -> Option<ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static core::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&gpui::InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (gpui::LayoutId, Self::RequestLayoutState) {
        let mut style = Style::default();
        style.size.width = gpui::relative(1.).into();
        style.size.height = self
            .style
            .text
            .line_height_in_pixels(window.rem_size())
            .into();
        (window.request_layout(style, [], cx), ())
    }

    fn prepaint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&gpui::InspectorElementId>,
        bounds: Bounds<Pixels>,
        _request_layout: &mut Self::RequestLayoutState,
        window: &mut Window,
        cx: &mut App,
    ) -> Self::PrepaintState {
        let editor = self.editor.read(cx);
        let snapshot = editor.snapshot();
        let content = snapshot.text();
        let selected_range = editor.selected_range.clone();
        let cursor_offset = editor.cursor_offset();
        let style = self.style.text.clone();

        let (display_text, text_color) = if content.is_empty() {
            (
                editor.placeholder.clone(),
                cx.theme().colors().text_placeholder,
            )
        } else {
            (editor.display_text(), style.color)
        };

        let base_run = TextRun {
            len: display_text.len(),
            font: style.font(),
            color: text_color,
            background_color: None,
            underline: None,
            strikethrough: None,
        };

        let runs = if let Some(marked_range) = editor.marked_range.as_ref() {
            let display_start = editor.display_offset_for_text_offset(marked_range.start);
            let display_end = editor.display_offset_for_text_offset(marked_range.end);
            let mut composed_runs = Vec::new();

            if display_start > 0 {
                composed_runs.push(TextRun {
                    len: display_start,
                    ..base_run.clone()
                });
            }
            if display_end > display_start {
                composed_runs.push(TextRun {
                    len: display_end - display_start,
                    underline: Some(UnderlineStyle {
                        color: Some(base_run.color),
                        thickness: px(1.0),
                        wavy: false,
                    }),
                    ..base_run.clone()
                });
            }
            if display_end < display_text.len() {
                composed_runs.push(TextRun {
                    len: display_text.len() - display_end,
                    ..base_run
                });
            }

            composed_runs
        } else {
            vec![base_run]
        };

        let font_size = style.font_size.to_pixels(window.rem_size());
        let line = window
            .text_system()
            .shape_line(display_text, font_size, &runs, None);

        let display_cursor = editor.display_offset_for_text_offset(cursor_offset);
        let cursor_pos = line.x_for_index(display_cursor);
        let display_start = editor.display_offset_for_text_offset(selected_range.start);
        let display_end = editor.display_offset_for_text_offset(selected_range.end);

        let selection = if selected_range.is_empty() {
            None
        } else {
            Some(fill(
                Bounds::from_corners(
                    point(
                        bounds.left() + line.x_for_index(display_start),
                        bounds.top(),
                    ),
                    point(
                        bounds.left() + line.x_for_index(display_end),
                        bounds.bottom(),
                    ),
                ),
                cx.theme().status().info_background,
            ))
        };

        let cursor = Some(fill(
            Bounds::new(
                point(bounds.left() + cursor_pos, bounds.top()),
                size(px(2.), bounds.bottom() - bounds.top()),
            ),
            cx.theme().colors().editor_foreground,
        ));

        let hitbox = window.insert_hitbox(bounds, HitboxBehavior::Normal);
        window.set_focus_handle(&editor.focus_handle, cx);

        PrepaintState {
            line: Some(line),
            cursor,
            selection,
            hitbox: Some(hitbox),
        }
    }

    fn paint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&gpui::InspectorElementId>,
        bounds: Bounds<Pixels>,
        _request_layout: &mut Self::RequestLayoutState,
        prepaint: &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        let focus_handle = self.editor.read(cx).focus_handle.clone();
        let hitbox = prepaint.hitbox.take().expect("hitbox to be set");

        window.set_cursor_style(CursorStyle::IBeam, &hitbox);
        let key_context = self
            .editor
            .update(cx, |editor, cx| editor.key_context(window, cx));
        window.set_key_context(key_context);
        window.handle_input(
            &focus_handle,
            ElementInputHandler::new(bounds, self.editor.clone()),
            cx,
        );
        self.register_actions(window);

        let editor = self.editor.clone();
        let mouse_down_hitbox = hitbox.clone();
        window.on_mouse_event(move |event: &MouseDownEvent, phase, window, cx| {
            if !phase.bubble() {
                return;
            }

            if event.button == MouseButton::Left && mouse_down_hitbox.is_hovered(window) {
                editor.update(cx, |editor, cx| editor.on_mouse_down(event, window, cx));
            }
        });

        let editor = self.editor.clone();
        let mouse_up_hitbox = hitbox.clone();
        window.on_mouse_event(move |event: &MouseUpEvent, phase, window, cx| {
            if !phase.bubble() {
                return;
            }

            if event.button == MouseButton::Left && mouse_up_hitbox.is_hovered(window) {
                editor.update(cx, |editor, cx| editor.on_mouse_up(event, window, cx));
            }
        });

        let editor = self.editor.clone();
        let mouse_move_hitbox = hitbox.clone();
        window.on_mouse_event(move |event: &MouseMoveEvent, phase, window, cx| {
            if !phase.bubble() {
                return;
            }

            if mouse_move_hitbox.is_hovered(window) {
                editor.update(cx, |editor, cx| editor.on_mouse_move(event, window, cx));
            }
        });

        if !self.style.background.is_transparent() {
            window.paint_quad(fill(bounds, self.style.background));
        }

        if let Some(selection) = prepaint.selection.take() {
            window.paint_quad(selection)
        }

        let line = prepaint.line.take().expect("line to be set");
        line.paint(
            bounds.origin,
            self.style.text.line_height_in_pixels(window.rem_size()),
            gpui::TextAlign::Left,
            None,
            window,
            cx,
        )
        .ok();

        if focus_handle.is_focused(window)
            && let Some(cursor) = prepaint.cursor.take()
        {
            window.paint_quad(cursor);
        }

        self.editor.update(cx, |editor, _cx| {
            editor.last_layout = Some(line);
            editor.last_bounds = Some(bounds);
        });
    }
}

pub fn register_action<T: Action>(
    editor: &Entity<Editor>,
    window: &mut Window,
    listener: impl Fn(&mut Editor, &T, &mut Window, &mut Context<Editor>) + 'static,
) {
    let editor = editor.clone();
    window.on_action(TypeId::of::<T>(), move |action, phase, window, cx| {
        if phase == DispatchPhase::Bubble {
            let action = action.downcast_ref().expect("action type to match");
            editor.update(cx, |editor, cx| {
                listener(editor, action, window, cx);
            });
        }
    })
}
