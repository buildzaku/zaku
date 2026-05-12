use gpui::{
    App, Context, Entity, FocusHandle, FocusOutEvent, Focusable, Subscription, WeakEntity, Window,
    prelude::*,
};
use theme::ActiveTheme;

use crate::{RequestEditor, Workspace, welcome::WelcomePage};

pub struct Pane {
    focus_handle: FocusHandle,
    was_focused: bool,
    should_display_welcome_page: bool,
    welcome_page: Option<Entity<WelcomePage>>,
    workspace: WeakEntity<Workspace>,
    request_editor: Entity<RequestEditor>,
    _subscriptions: Vec<Subscription>,
}

impl Pane {
    pub fn new(
        workspace: WeakEntity<Workspace>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let focus_handle = cx.focus_handle();
        let subscriptions = vec![
            cx.on_focus(&focus_handle, window, Pane::focus_in),
            cx.on_focus_in(&focus_handle, window, Pane::focus_in),
            cx.on_focus_out(&focus_handle, window, Pane::focus_out),
        ];
        let request_editor = cx.new({
            let workspace = workspace.clone();
            move |cx| RequestEditor::new(workspace.clone(), window, cx)
        });

        Self {
            focus_handle,
            was_focused: false,
            should_display_welcome_page: false,
            welcome_page: None,
            workspace,
            request_editor,
            _subscriptions: subscriptions,
        }
    }

    pub fn workspace(&self) -> WeakEntity<Workspace> {
        self.workspace.clone()
    }

    pub fn set_should_display_welcome_page(&mut self, should_display_welcome_page: bool) {
        self.should_display_welcome_page = should_display_welcome_page;
    }

    pub fn should_display_welcome_page(&self) -> bool {
        self.should_display_welcome_page
    }

    fn focus_in(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.was_focused {
            self.was_focused = true;
            cx.notify();
        }

        if self.focus_handle.is_focused(window) {
            cx.on_next_frame(window, |_, _, cx| {
                cx.notify();
            });
        }

        if self.should_display_welcome_page()
            && let Some(welcome_page) = self.welcome_page.as_ref()
            && self.focus_handle.is_focused(window)
        {
            welcome_page.read(cx).focus_handle(cx).focus(window, cx);
        }
    }

    fn focus_out(&mut self, _event: FocusOutEvent, _window: &mut Window, cx: &mut Context<Self>) {
        self.was_focused = false;
        cx.notify();
    }

    pub fn send_request(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.request_editor.update(cx, |request_editor, cx| {
            request_editor.send_request(window, cx);
        });
    }
}

impl Focusable for Pane {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for Pane {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let has_worktree = self
            .workspace
            .upgrade()
            .is_some_and(|workspace| workspace.read(cx).has_worktree(cx));

        if !has_worktree && self.should_display_welcome_page() {
            if self.welcome_page.is_none() {
                let workspace = self.workspace.clone();
                self.welcome_page = Some(cx.new(|cx| WelcomePage::new(workspace, window, cx)));
            }

            return gpui::div()
                .track_focus(&self.focus_handle)
                .size_full()
                .overflow_hidden()
                .bg(cx.theme().colors().panel_background)
                .child(
                    ui::h_flex()
                        .size_full()
                        .justify_center()
                        .when_some(self.welcome_page.clone(), |container, welcome_page| {
                            container.child(welcome_page)
                        }),
                );
        }

        gpui::div()
            .track_focus(&self.focus_handle)
            .size_full()
            .overflow_hidden()
            .bg(cx.theme().colors().panel_background)
            .child(self.request_editor.clone())
    }
}
