pub mod dock;
pub mod pane;
pub mod panel;
pub mod status_bar;
pub mod welcome;

use anyhow::Result;
use futures::channel::oneshot;
use gpui::{
    Action, App, Axis, Bounds, DragMoveEvent, Empty, Entity, EntityId, FocusHandle, Focusable,
    KeyBinding, KeyContext, MouseButton, MouseDownEvent, PathPromptOptions, Pixels, Point,
    PromptLevel, Subscription, Task, Window, prelude::*,
};
use std::path::PathBuf;

use theme::{ActiveTheme, GlobalTheme, SystemAppearance};
use ui::StyledTypography;

use crate::{
    dock::Dock,
    pane::Pane,
    panel::{ProjectPanel, ResponsePanel, buttons::PanelButtons, project_panel, response_panel},
    status_bar::StatusBar,
};

gpui::actions!(
    workspace,
    [
        OpenWorkspace,
        CloseProject,
        SendRequest,
        ToggleBottomDock,
        ToggleLeftDock,
        ToggleRightDock
    ]
);

const KEY_CONTEXT: &str = "Workspace";
const MIN_DOCK_WIDTH: Pixels = gpui::px(110.0);
const MIN_PANE_WIDTH: Pixels = gpui::px(250.0);
const MIN_CONFIG_PANE_HEIGHT: Pixels = gpui::px(180.0);
const MIN_RESPONSE_PANE_HEIGHT: Pixels = gpui::px(110.0);

pub fn init(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new("enter", SendRequest, Some("RequestUrl > Editor")),
        #[cfg(target_os = "macos")]
        KeyBinding::new("cmd-o", OpenWorkspace, None),
        #[cfg(not(target_os = "macos"))]
        KeyBinding::new("ctrl-k ctrl-o", OpenWorkspace, None),
        #[cfg(target_os = "macos")]
        KeyBinding::new("cmd-j", ToggleBottomDock, Some(KEY_CONTEXT)),
        #[cfg(not(target_os = "macos"))]
        KeyBinding::new("ctrl-j", ToggleBottomDock, Some(KEY_CONTEXT)),
        #[cfg(target_os = "macos")]
        KeyBinding::new("cmd-b", ToggleLeftDock, Some(KEY_CONTEXT)),
        #[cfg(not(target_os = "macos"))]
        KeyBinding::new("ctrl-b", ToggleLeftDock, Some(KEY_CONTEXT)),
        #[cfg(target_os = "macos")]
        KeyBinding::new(
            "cmd-shift-r",
            response_panel::ToggleFocus,
            Some(KEY_CONTEXT),
        ),
        #[cfg(not(target_os = "macos"))]
        KeyBinding::new(
            "ctrl-shift-r",
            response_panel::ToggleFocus,
            Some(KEY_CONTEXT),
        ),
        #[cfg(target_os = "macos")]
        KeyBinding::new("cmd-shift-e", project_panel::ToggleFocus, Some(KEY_CONTEXT)),
        #[cfg(not(target_os = "macos"))]
        KeyBinding::new(
            "ctrl-shift-e",
            project_panel::ToggleFocus,
            Some(KEY_CONTEXT),
        ),
    ]);
}

pub fn prompt_and_open_workspace(
    workspace: &mut Workspace,
    options: PathPromptOptions,
    window: &mut Window,
    cx: &mut Context<Workspace>,
) {
    let selected_path = workspace.prompt_open_path(options, window, cx);

    cx.spawn_in(window, async move |this, cx| {
        let Ok(Some(selected_path)) = selected_path.await else {
            return;
        };

        let open_task = match this.update_in(cx, |workspace, window, cx| {
            workspace.open_workspace_for_path(false, selected_path, window, cx)
        }) {
            Ok(open_task) => open_task,
            Err(_) => return,
        };

        if open_task.await.is_err() {
            return;
        }
    })
    .detach();
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum DockPosition {
    Left,
    Bottom,
    Right,
}

impl DockPosition {
    pub fn label(&self) -> &'static str {
        match self {
            DockPosition::Left => "Left",
            DockPosition::Bottom => "Bottom",
            DockPosition::Right => "Right",
        }
    }

    pub fn axis(&self) -> Axis {
        match self {
            DockPosition::Left | DockPosition::Right => Axis::Horizontal,
            DockPosition::Bottom => Axis::Vertical,
        }
    }
}

pub struct Workspace {
    left_dock: Entity<Dock>,
    bottom_dock: Entity<Dock>,
    right_dock: Entity<Dock>,
    pane: Entity<Pane>,
    response_panel: Entity<ResponsePanel>,
    status_bar: Entity<StatusBar>,
    bounds: Bounds<Pixels>,
    previous_dock_drag_coordinates: Option<Point<Pixels>>,
    _window_appearance_subscription: Subscription,
}

#[derive(Clone)]
pub struct DraggedDock(pub DockPosition);

impl Render for DraggedDock {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        Empty
    }
}

impl Workspace {
    fn dock_at_position(&self, position: DockPosition) -> &Entity<Dock> {
        match position {
            DockPosition::Left => &self.left_dock,
            DockPosition::Bottom => &self.bottom_dock,
            DockPosition::Right => &self.right_dock,
        }
    }

    pub fn prompt_open_path(
        &mut self,
        options: PathPromptOptions,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> oneshot::Receiver<Option<PathBuf>> {
        let (sender, receiver) = oneshot::channel();
        let path_prompt = cx.prompt_for_paths(options);

        cx.spawn_in(window, async move |workspace, cx| {
            let selection = match path_prompt.await {
                Ok(selection) => selection,
                Err(_) => return Ok::<(), anyhow::Error>(()),
            };

            match selection {
                Ok(selected_paths) => {
                    let selected_path = selected_paths.and_then(|paths| paths.into_iter().next());

                    if sender.send(selected_path).is_err() {
                        return Ok::<(), anyhow::Error>(());
                    }
                }
                Err(error) => {
                    let error_message = error.to_string();
                    let prompt = workspace.update_in(cx, |_, window, cx| {
                        window.prompt(
                            PromptLevel::Critical,
                            "Failed to open project",
                            Some(&error_message),
                            &["Ok"],
                            cx,
                        )
                    })?;

                    if prompt.await.is_err() {
                        return Ok::<(), anyhow::Error>(());
                    }
                }
            }

            Ok::<(), anyhow::Error>(())
        })
        .detach();

        receiver
    }

    pub fn open_workspace_for_path(
        &mut self,
        _replace_current_window: bool,
        _path: PathBuf,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Task<Result<()>> {
        self.pane.update(cx, |pane, cx| {
            pane.set_should_display_welcome_page(false, cx);
        });
        let focus_handle = self.pane.read(cx).focus_handle(cx);
        window.focus(&focus_handle, cx);
        cx.notify();

        Task::ready(Ok(()))
    }

    fn close_project(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.pane.update(cx, |pane, cx| {
            pane.set_should_display_welcome_page(true, cx);
        });
        let focus_handle = self.pane.read(cx).focus_handle(cx);
        window.focus(&focus_handle, cx);
        cx.notify();
    }

    fn toggle_dock(&mut self, position: DockPosition, window: &mut Window, cx: &mut Context<Self>) {
        let dock = self.dock_at_position(position).clone();
        let was_visible = dock.read(cx).is_open();
        if was_visible && !window.bindings_for_action(&menu::Cancel).is_empty() {
            // Move focus back to the center so dismissing a menu does not focus a hidden dock element.
            let focus_handle = self.pane.read(cx).focus_handle(cx);
            window.focus(&focus_handle, cx);
        }
        window.dispatch_action(menu::Cancel.boxed_clone(), cx);

        let mut focus_center = false;

        dock.update(cx, |dock, cx| {
            if !was_visible {
                let needs_enabled_panel = dock
                    .active_panel()
                    .is_none_or(|active_panel| !active_panel.enabled(cx));

                if needs_enabled_panel {
                    let Some(panel_index) = dock.first_enabled_panel_idx(cx) else {
                        return;
                    };
                    dock.set_active_panel_index(Some(panel_index), cx);
                }
            }

            if was_visible && dock.focus_handle(cx).contains_focused(window, cx) {
                focus_center = true;
            }
            dock.set_open(!was_visible, cx);

            if let Some(active_panel) = dock.active_panel() {
                if !was_visible {
                    let focus_handle = active_panel.panel_focus_handle(cx);
                    window.focus(&focus_handle, cx);
                }
            }
        });

        if focus_center {
            let focus_handle = self.pane.read(cx).focus_handle(cx);
            window.focus(&focus_handle, cx);
        }

        cx.notify();
    }

    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let window_appearance_subscription =
            cx.observe_window_appearance(window, |_, window, cx| {
                let window_appearance = window.appearance();
                *SystemAppearance::global_mut(cx) = SystemAppearance(window_appearance.into());
                GlobalTheme::reload_theme(cx);
            });

        let workspace = cx.entity().downgrade();
        let pane = cx.new(|cx| Pane::new(workspace.clone(), window, cx));
        let pane_focus_handle = pane.read(cx).focus_handle(cx);
        window.focus(&pane_focus_handle, cx);

        let left_dock = cx.new(|cx| Dock::new(DockPosition::Left, workspace.clone(), cx));
        let bottom_dock = cx.new(|cx| Dock::new(DockPosition::Bottom, workspace.clone(), cx));
        let right_dock = cx.new(|cx| Dock::new(DockPosition::Right, workspace.clone(), cx));

        let pane_handle = pane.downgrade();
        let left_dock_panel = cx.new(|cx| ProjectPanel::new(pane_handle.clone(), cx));
        left_dock.update(cx, |left_dock, cx| {
            left_dock.add_panel(left_dock_panel, window, cx);
        });

        let response_panel = cx.new(|cx| ResponsePanel::new(pane_handle, window, cx));
        bottom_dock.update(cx, |bottom_dock, cx| {
            bottom_dock.add_panel(response_panel.clone(), window, cx);
        });

        let left_dock_buttons = cx.new(|cx| PanelButtons::new(left_dock.clone(), cx));
        let bottom_dock_buttons = cx.new(|cx| PanelButtons::new(bottom_dock.clone(), cx));
        let right_dock_buttons = cx.new(|cx| PanelButtons::new(right_dock.clone(), cx));

        pane.update(cx, |pane, cx| {
            pane.set_should_display_welcome_page(true, cx);
        });

        let status_bar = cx.new(|cx| StatusBar::new(pane.clone(), cx));
        status_bar.update(cx, |status_bar, cx| {
            status_bar.add_left_item(left_dock_buttons, cx);
            status_bar.add_right_item(bottom_dock_buttons, cx);
            status_bar.add_right_item(right_dock_buttons, cx);
        });

        let pane_focus_handle = pane.read(cx).focus_handle(cx);
        window.focus(&pane_focus_handle, cx);

        Self {
            left_dock,
            bottom_dock,
            right_dock,
            pane,
            response_panel,
            status_bar,
            bounds: Bounds::default(),
            previous_dock_drag_coordinates: None,
            _window_appearance_subscription: window_appearance_subscription,
        }
    }

    fn resize_left_dock(&mut self, size: Pixels, window: &mut Window, cx: &mut App) {
        let size = size
            .min(self.bounds.size.width - MIN_PANE_WIDTH)
            .max(MIN_DOCK_WIDTH);
        self.left_dock.update(cx, |dock, cx| {
            dock.resize_active_panel(Some(size), window, cx);
        })
    }

    fn resize_right_dock(&mut self, size: Pixels, window: &mut Window, cx: &mut App) {
        let size = size
            .min(self.bounds.size.width - MIN_PANE_WIDTH)
            .max(MIN_DOCK_WIDTH);
        self.right_dock.update(cx, |dock, cx| {
            dock.resize_active_panel(Some(size), window, cx);
        })
    }

    fn resize_bottom_dock(&mut self, size: Pixels, window: &mut Window, cx: &mut App) {
        let size = size
            .min(self.bounds.size.height - MIN_CONFIG_PANE_HEIGHT)
            .max(MIN_RESPONSE_PANE_HEIGHT);
        self.bottom_dock.update(cx, |dock, cx| {
            dock.resize_active_panel(Some(size), window, cx);
        })
    }

    fn toggle_panel_focus(
        &mut self,
        panel_id: EntityId,
        dock: &Entity<Dock>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let mut focus_center = false;
        dock.update(cx, |dock, cx| {
            dock.activate_panel(panel_id, cx);
            let Some(active_panel) = dock.active_panel() else {
                return;
            };

            let focus_handle = active_panel.panel_focus_handle(cx);
            if focus_handle.contains_focused(window, cx) {
                focus_center = true;
            } else {
                dock.set_open(true, cx);
                window.focus(&focus_handle, cx);
            }
        });

        if focus_center {
            let focus_handle = self.pane.read(cx).focus_handle(cx);
            window.focus(&focus_handle, cx);
        }

        cx.notify();
    }

    fn toggle_project_panel(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let panel_id = self
            .left_dock
            .read(cx)
            .active_panel()
            .and_then(|panel| panel.enabled(cx).then_some(panel.panel_id()));
        if let Some(panel_id) = panel_id {
            let dock = self.left_dock.clone();
            self.toggle_panel_focus(panel_id, &dock, window, cx);
        }
    }

    fn toggle_response_panel(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let panel_id = self
            .bottom_dock
            .read(cx)
            .active_panel()
            .and_then(|panel| panel.enabled(cx).then_some(panel.panel_id()));
        let Some(panel_id) = panel_id else {
            return;
        };
        let dock = self.bottom_dock.clone();
        self.toggle_panel_focus(panel_id, &dock, window, cx);
    }

    fn open_response_panel(&mut self, cx: &mut Context<Self>) -> Entity<ResponsePanel> {
        let panel_id = Entity::entity_id(&self.response_panel);
        self.bottom_dock.update(cx, |dock, cx| {
            dock.activate_panel(panel_id, cx);
        });
        self.response_panel.clone()
    }
}

impl Render for Workspace {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let ui_font = theme::setup_ui_font(window, cx);
        let theme_colors = cx.theme().colors();
        let mut context = KeyContext::new_with_defaults();
        context.add(KEY_CONTEXT);
        if self.left_dock.read(cx).is_open() {
            if let Some(active_panel) = self.left_dock.read(cx).active_panel() {
                context.set("left_dock", active_panel.persistent_name());
            }
        }
        if self.right_dock.read(cx).is_open() {
            if let Some(active_panel) = self.right_dock.read(cx).active_panel() {
                context.set("right_dock", active_panel.persistent_name());
            }
        }
        if self.bottom_dock.read(cx).is_open() {
            if let Some(active_panel) = self.bottom_dock.read(cx).active_panel() {
                context.set("bottom_dock", active_panel.persistent_name());
            }
        }
        let focus_handle = self.focus_handle(cx);
        gpui::div()
            .id("workspace")
            .key_context(context)
            .track_focus(&focus_handle)
            .relative()
            .flex()
            .flex_col()
            .bg(theme_colors.background)
            .text_color(theme_colors.text)
            .font(ui_font)
            .text_ui(cx)
            .size_full()
            .on_action(cx.listener(|workspace, _: &OpenWorkspace, window, cx| {
                prompt_and_open_workspace(
                    workspace,
                    PathPromptOptions {
                        files: false,
                        directories: true,
                        multiple: false,
                        prompt: None,
                    },
                    window,
                    cx,
                );
            }))
            .on_action(cx.listener(|workspace, _: &CloseProject, window, cx| {
                workspace.close_project(window, cx);
            }))
            .on_action(cx.listener(|workspace, _: &ToggleLeftDock, window, cx| {
                workspace.toggle_dock(DockPosition::Left, window, cx);
            }))
            .on_action(cx.listener(|workspace, _: &ToggleRightDock, window, cx| {
                workspace.toggle_dock(DockPosition::Right, window, cx);
            }))
            .on_action(cx.listener(|workspace, _: &ToggleBottomDock, window, cx| {
                workspace.toggle_dock(DockPosition::Bottom, window, cx);
            }))
            .on_action(
                cx.listener(|workspace, _: &project_panel::ToggleFocus, window, cx| {
                    workspace.toggle_project_panel(window, cx);
                }),
            )
            .on_action(
                cx.listener(|workspace, _: &response_panel::ToggleFocus, window, cx| {
                    workspace.toggle_response_panel(window, cx);
                }),
            )
            .on_drag_move(
                cx.listener(|workspace, e: &DragMoveEvent<DraggedDock>, window, cx| {
                    if workspace.previous_dock_drag_coordinates != Some(e.event.position) {
                        workspace.previous_dock_drag_coordinates = Some(e.event.position);
                        match e.drag(cx).0 {
                            DockPosition::Left => {
                                workspace.resize_left_dock(
                                    e.event.position.x - workspace.bounds.left(),
                                    window,
                                    cx,
                                );
                            }
                            DockPosition::Right => {
                                workspace.resize_right_dock(
                                    workspace.bounds.right() - e.event.position.x,
                                    window,
                                    cx,
                                );
                            }
                            DockPosition::Bottom => {
                                workspace.resize_bottom_dock(
                                    workspace.bounds.bottom() - e.event.position.y,
                                    window,
                                    cx,
                                );
                            }
                        }
                    }
                }),
            )
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|workspace, _: &MouseDownEvent, window, cx| {
                    if !window.default_prevented() {
                        let focus_handle = workspace.focus_handle(cx);
                        window.focus(&focus_handle, cx);
                    }
                }),
            )
            .child(
                gpui::div()
                    .relative()
                    .flex()
                    .flex_row()
                    .flex_1()
                    .overflow_hidden()
                    .child({
                        let this = cx.entity();
                        gpui::canvas(
                            move |bounds, _window, cx| {
                                this.update(cx, |this, _cx| {
                                    this.bounds = bounds;
                                });
                            },
                            |_, _, _, _| {},
                        )
                        .absolute()
                        .size_full()
                    })
                    .child(
                        gpui::div()
                            .flex_none()
                            .overflow_hidden()
                            .child(self.left_dock.clone()),
                    )
                    .child(
                        gpui::div()
                            .flex()
                            .flex_col()
                            .flex_1()
                            .h_full()
                            .overflow_hidden()
                            .child(
                                gpui::div()
                                    .flex_1()
                                    .overflow_hidden()
                                    .child(self.pane.clone()),
                            )
                            .child(self.bottom_dock.clone()),
                    )
                    .child(
                        gpui::div()
                            .flex_none()
                            .overflow_hidden()
                            .child(self.right_dock.clone()),
                    ),
            )
            .child(self.status_bar.clone())
    }
}

impl Focusable for Workspace {
    fn focus_handle(&self, cx: &App) -> FocusHandle {
        self.pane.read(cx).focus_handle(cx)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use fs::TempFs;
    use gpui::TestAppContext;
    use indoc::indoc;
    use serde_json::json;
    use settings::SettingsStore;
    use theme::LoadThemes;
    use util_macros::path;

    fn init_test(cx: &mut TestAppContext) {
        cx.update(|cx| {
            let settings_store = SettingsStore::test(cx);
            cx.set_global(settings_store);
            theme::init(LoadThemes::JustBase, cx);
            editor::init(cx);
            crate::init(cx);
        });
    }

    #[gpui::test]
    async fn test_docks_are_disabled_on_welcome_page(cx: &mut TestAppContext) {
        init_test(cx);

        let (workspace, cx) = cx.add_window_view(|window, cx| Workspace::new(window, cx));

        workspace.update_in(cx, |workspace, window, cx| {
            assert!(workspace.pane.read(cx).should_display_welcome_page());
            assert!(!workspace.left_dock.read(cx).is_open());
            assert!(!workspace.bottom_dock.read(cx).is_open());

            workspace.toggle_dock(DockPosition::Left, window, cx);
            workspace.toggle_dock(DockPosition::Bottom, window, cx);

            assert!(!workspace.left_dock.read(cx).is_open());
            assert!(!workspace.bottom_dock.read(cx).is_open());
        });
    }

    #[gpui::test]
    async fn test_open_workspace_hides_welcome_page(cx: &mut TestAppContext) {
        init_test(cx);

        let (workspace, cx) = cx.add_window_view(|window, cx| Workspace::new(window, cx));
        let temp_fs = TempFs::new();

        temp_fs.insert_tree(
            path!("project"),
            json!({
                ".gitignore": indoc! {"
                    .DS_Store
                "},
                "auth": {
                    "login.toml": indoc! {r#"
                        [config]
                        method = "POST"
                        url = "zaku.dev/auth/login"
                    "#}
                }
            }),
        );

        let project_path = temp_fs.path().join(path!("project"));

        workspace.update_in(cx, |workspace, window, cx| {
            assert!(workspace.pane.read(cx).should_display_welcome_page());

            workspace
                .open_workspace_for_path(false, project_path.clone(), window, cx)
                .detach_and_log_err(cx);

            assert!(!workspace.pane.read(cx).should_display_welcome_page());
        });
    }

    #[gpui::test]
    async fn test_send_request_opens_response_panel(cx: &mut TestAppContext) {
        init_test(cx);

        let (workspace, cx) = cx.add_window_view(|window, cx| Workspace::new(window, cx));
        let temp_fs = TempFs::new();

        temp_fs.insert_tree(
            path!("project"),
            json!({
                ".gitignore": indoc! {"
                    .DS_Store
                "},
                "auth": {
                    "login.toml": indoc! {r#"
                        [config]
                        method = "GET"
                        url = "zaku.dev/users/me"
                    "#}
                }
            }),
        );

        let project_path = temp_fs.path().join(path!("project"));

        workspace.update_in(cx, |workspace, window, cx| {
            workspace
                .open_workspace_for_path(false, project_path.clone(), window, cx)
                .detach_and_log_err(cx);
        });

        let pane = workspace.update_in(cx, |workspace, _, _| workspace.pane.clone());
        pane.update_in(cx, |pane, window, cx| {
            pane.send_request(window, cx);
        });

        workspace.update_in(cx, |workspace, _, cx| {
            let response_panel_id = Entity::entity_id(&workspace.response_panel);
            let active_panel_id = workspace
                .bottom_dock
                .read(cx)
                .active_panel()
                .map(|panel| panel.panel_id());

            assert!(workspace.bottom_dock.read(cx).is_open());
            assert_eq!(active_panel_id, Some(response_panel_id));
        });
    }
}
