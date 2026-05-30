use gpui::{
    App, Context, Entity, EventEmitter, FocusHandle, Focusable, Render, SharedString, Subscription,
    Window, prelude::*,
};
use std::{any::Any, collections::VecDeque, sync::Arc};

use editor::{Editor, EditorEvent};
use icons::FileIcons;
use language::{Buffer, Capability};
use multi_buffer::MultiBuffer;
use ui::Icon;
use workspace::{
    Item, ItemEvent, Root,
    notifications::{NotificationId, simple_message_notification::MessageNotification},
};

struct LogsView {
    editor: Entity<Editor>,
    _editor_subscription: Subscription,
}

impl LogsView {
    fn new(editor: Entity<Editor>, cx: &mut Context<Self>) -> Self {
        let subscription = cx.subscribe(&editor, |_, _, event: &EditorEvent, cx| cx.emit(*event));

        Self {
            editor,
            _editor_subscription: subscription,
        }
    }
}

impl EventEmitter<EditorEvent> for LogsView {}

impl Focusable for LogsView {
    fn focus_handle(&self, cx: &App) -> FocusHandle {
        self.editor.read(cx).focus_handle(cx)
    }
}

impl Render for LogsView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.editor.update(cx, |editor, cx| {
            editor.render(window, cx).into_any_element()
        })
    }
}

impl Item for LogsView {
    type Event = EditorEvent;

    fn to_item_events(event: &Self::Event, f: &mut dyn FnMut(ItemEvent)) {
        Editor::to_item_events(event, f);
    }

    fn tab_content_text(&self, detail: usize, cx: &App) -> SharedString {
        self.editor.read(cx).tab_content_text(detail, cx)
    }

    fn tab_tooltip_text(&self, cx: &App) -> Option<SharedString> {
        self.editor.read(cx).tab_tooltip_text(cx)
    }

    fn tab_icon(&self, _: &Window, cx: &App) -> Option<Icon> {
        FileIcons::get_icon(settings::log_file(), cx).map(Icon::from_path)
    }

    fn capability(&self, cx: &App) -> Capability {
        self.editor.read(cx).capability(cx)
    }

    fn deactivated(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.editor
            .update(cx, |editor, cx| editor.deactivated(window, cx));
    }

    fn navigate(
        &mut self,
        data: Arc<dyn Any + Send>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        self.editor
            .update(cx, |editor, cx| editor.navigate(data, window, cx))
    }
}

pub fn open_log_file(cx: &mut App) {
    const MAX_LINES: usize = 1000;
    struct OpenLogFileErrorNotification;

    cx.defer(|cx| {
        let Some(window) = cx
            .active_window()
            .and_then(|window| window.downcast::<Root>())
        else {
            log::error!("Cannot open log file without an active workspace");
            return;
        };

        if let Err(error) = window.update(cx, |root, window, cx| {
            root.workspace().update(cx, |workspace, cx| {
                let fs = workspace.shared_state().fs.clone();
                cx.spawn_in(window, async move |workspace, cx| {
                    let log = {
                        let result = futures::join!(
                            fs.load(settings::old_log_file()),
                            fs.load(settings::log_file())
                        );

                        match result {
                            (Err(_), Err(error)) => Err(error),
                            (old_log, new_log) => {
                                let mut lines = VecDeque::with_capacity(MAX_LINES);
                                for line in old_log
                                    .iter()
                                    .flat_map(|log| log.lines())
                                    .chain(new_log.iter().flat_map(|log| log.lines()))
                                {
                                    if lines.len() == MAX_LINES {
                                        lines.pop_front();
                                    }
                                    lines.push_back(line);
                                }

                                Ok(lines
                                    .into_iter()
                                    .flat_map(|line| [line, "\n"])
                                    .collect::<String>())
                            }
                        }
                    };

                    let log = match log {
                        Ok(log) => log,
                        Err(error) => {
                            if let Err(update_error) = workspace.update(cx, |workspace, cx| {
                                workspace.show_notification(
                                    &NotificationId::unique::<OpenLogFileErrorNotification>(),
                                    cx,
                                    |cx| {
                                        cx.new(|cx| {
                                            MessageNotification::new(
                                                format!(
                                                    "Unable to access/open log file at path {}: {error:#}",
                                                    settings::log_file().display()
                                                ),
                                                cx,
                                            )
                                        })
                                    },
                                );
                            }) {
                                log::error!(
                                    "Failed to show log file error notification: {update_error}"
                                );
                            }
                            return anyhow::Ok(());
                        }
                    };

                    workspace.update_in(cx, |workspace, window, cx| {
                        let buffer = cx.new(|cx| {
                            let mut buffer = Buffer::local(log, cx);
                            buffer.set_capability(Capability::ReadOnly, cx);
                            buffer
                        });
                        let buffer = cx.new(|cx| {
                            MultiBuffer::singleton(buffer, cx).with_title("Logs".into())
                        });
                        let editor = cx.new(|cx| {
                            let mut editor = Editor::for_multibuffer(buffer, window, cx);
                            editor.set_read_only(true);
                            editor.move_selection_to_end(cx);
                            editor
                        });
                        workspace.add_item_to_active_pane(
                            Box::new(cx.new(|cx| LogsView::new(editor, cx))),
                            None,
                            true,
                            window,
                            cx,
                        );
                    })?;

                    anyhow::Ok(())
                })
                .detach_and_log_err(cx);
            });
        }) {
            log::error!("Failed to open log file: {error}");
        }
    });
}
