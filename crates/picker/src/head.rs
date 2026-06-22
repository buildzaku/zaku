use gpui::{App, Context, Entity, FocusHandle, Focusable, Window, prelude::*};
use std::sync::Arc;

use input::{ErasedEditor, ErasedEditorEvent};

pub(crate) enum Head {
    Editor(Arc<dyn ErasedEditor>),
    Empty(Entity<EmptyHead>),
}

impl Head {
    pub(crate) fn editor<V: 'static>(
        placeholder_text: &str,
        mut edit_handler: impl FnMut(&mut V, ErasedEditorEvent, &mut Window, &mut Context<V>) + 'static,
        window: &mut Window,
        cx: &mut Context<V>,
    ) -> Self {
        let editor_factory = input::ERASED_EDITOR_FACTORY
            .get()
            .expect("Editor factory should be initialized");
        let editor = (editor_factory)(window, cx);

        editor.set_placeholder_text(placeholder_text, window, cx);
        let this = cx.weak_entity();
        editor
            .subscribe(
                Box::new(move |event, window, cx| {
                    if let Err(error) = this.update(cx, |this, cx| {
                        edit_handler(this, event, window, cx);
                    }) {
                        log::debug!("Failed to update picker editor state: {error:?}");
                    }
                }),
                window,
                cx,
            )
            .detach();
        Self::Editor(editor)
    }

    pub(crate) fn empty<V: 'static>(
        blur_handler: impl FnMut(&mut V, &mut Window, &mut Context<V>) + 'static,
        window: &mut Window,
        cx: &mut Context<V>,
    ) -> Self {
        let head = cx.new(EmptyHead::new);
        cx.on_blur(&head.focus_handle(cx), window, blur_handler)
            .detach();
        Self::Empty(head)
    }
}

pub(crate) struct EmptyHead {
    focus_handle: FocusHandle,
}

impl EmptyHead {
    fn new(cx: &mut Context<Self>) -> Self {
        Self {
            focus_handle: cx.focus_handle(),
        }
    }
}

impl Render for EmptyHead {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        gpui::div().track_focus(&self.focus_handle(cx))
    }
}

impl Focusable for EmptyHead {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}
