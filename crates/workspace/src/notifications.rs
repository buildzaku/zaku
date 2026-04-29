use gpui::{
    AnyElement, AnyView, App, Context, DismissEvent, ElementId, Entity, EventEmitter, Focusable,
    ParentElement, Render, RenderOnce, SharedString, Styled, Window, prelude::*,
};
use std::{
    any::TypeId,
    ops::{Deref, DerefMut},
    time::Duration,
};

use ui::{ButtonCommon, Clickable, IconButton, IconName, Label, StyledExt, Tooltip};

use crate::{Toast, Workspace};

#[derive(Default)]
pub struct Notifications {
    notifications: Vec<(NotificationId, AnyView)>,
}

impl Deref for Notifications {
    type Target = Vec<(NotificationId, AnyView)>;

    fn deref(&self) -> &Self::Target {
        &self.notifications
    }
}

impl DerefMut for Notifications {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.notifications
    }
}

#[derive(Debug, Eq, PartialEq, Clone, Hash)]
pub enum NotificationId {
    Unique(TypeId),
    Composite(TypeId, ElementId),
    Named(SharedString),
}

impl NotificationId {
    pub const fn unique<T: 'static>() -> Self {
        Self::Unique(TypeId::of::<T>())
    }

    pub fn composite<T: 'static>(id: impl Into<ElementId>) -> Self {
        Self::Composite(TypeId::of::<T>(), id.into())
    }

    pub fn named(id: SharedString) -> Self {
        Self::Named(id)
    }
}

pub trait Notification:
    EventEmitter<DismissEvent> + EventEmitter<SuppressEvent> + Focusable + Render
{
}

pub struct SuppressEvent;

impl Workspace {
    #[cfg(any(test, feature = "test-support"))]
    pub fn notification_ids(&self) -> Vec<NotificationId> {
        self.notifications
            .iter()
            .map(|(id, _)| id)
            .cloned()
            .collect()
    }

    pub fn show_notification<V: Notification>(
        &mut self,
        id: NotificationId,
        cx: &mut Context<Self>,
        build_notification: impl FnOnce(&mut Context<Self>) -> Entity<V>,
    ) {
        self.show_notification_without_handling_dismiss_events(&id, cx, |cx| {
            let notification = build_notification(cx);
            cx.subscribe(&notification, {
                let id = id.clone();
                move |this, _, _: &DismissEvent, cx| {
                    this.dismiss_notification(&id, cx);
                }
            })
            .detach();
            cx.subscribe(&notification, {
                let id = id.clone();
                move |workspace: &mut Workspace, _, _: &SuppressEvent, cx| {
                    workspace.suppress_notification(&id, cx);
                }
            })
            .detach();

            notification.into()
        });
    }

    pub(crate) fn show_notification_without_handling_dismiss_events(
        &mut self,
        id: &NotificationId,
        cx: &mut Context<Self>,
        build_notification: impl FnOnce(&mut Context<Self>) -> AnyView,
    ) {
        if self.suppressed_notifications.contains(id) {
            return;
        }
        self.dismiss_notification(id, cx);
        self.notifications
            .push((id.clone(), build_notification(cx)));
        cx.notify();
    }

    pub fn dismiss_notification(&mut self, id: &NotificationId, cx: &mut Context<Self>) {
        self.notifications.retain(|(existing_id, _)| {
            if existing_id == id {
                cx.notify();
                false
            } else {
                true
            }
        });
    }

    pub fn show_toast(&mut self, toast: Toast, cx: &mut Context<Self>) {
        self.dismiss_notification(&toast.id, cx);
        self.show_notification(toast.id.clone(), cx, |cx| {
            cx.new(|cx| match toast.on_click.as_ref() {
                Some((click_msg, on_click)) => {
                    let on_click = on_click.clone();
                    simple_message_notification::MessageNotification::new(toast.msg.clone(), cx)
                        .primary_message(click_msg.clone())
                        .primary_on_click(move |window, cx| on_click(window, cx))
                }
                None => {
                    simple_message_notification::MessageNotification::new(toast.msg.clone(), cx)
                }
            })
        });
        if toast.autohide {
            cx.spawn(async move |workspace, cx| {
                cx.background_executor()
                    .timer(Duration::from_millis(5000))
                    .await;
                workspace
                    .update(cx, |workspace, cx| workspace.dismiss_toast(&toast.id, cx))
                    .ok();
            })
            .detach();
        }
    }

    pub fn dismiss_toast(&mut self, id: &NotificationId, cx: &mut Context<Self>) {
        self.dismiss_notification(id, cx);
    }

    pub fn clear_all_notifications(&mut self, cx: &mut Context<Self>) {
        self.notifications.clear();
        cx.notify();
    }

    pub fn suppress_notification(&mut self, id: &NotificationId, cx: &mut Context<Self>) {
        self.dismiss_notification(id, cx);
        self.suppressed_notifications.insert(id.clone());
    }

    pub fn is_notification_suppressed(&self, notification_id: NotificationId) -> bool {
        self.suppressed_notifications.contains(&notification_id)
    }

    pub fn unsuppress(&mut self, notification_id: NotificationId) {
        self.suppressed_notifications.remove(&notification_id);
    }
}

#[derive(IntoElement)]
pub struct NotificationFrame {
    title: Option<SharedString>,
    show_suppress_button: bool,
    show_close_button: bool,
    close: Option<Box<dyn Fn(&bool, &mut Window, &mut App) + 'static>>,
    contents: Option<AnyElement>,
    suffix: Option<AnyElement>,
}

impl NotificationFrame {
    pub fn new() -> Self {
        Self {
            title: None,
            contents: None,
            suffix: None,
            show_suppress_button: true,
            show_close_button: true,
            close: None,
        }
    }

    pub fn with_title(mut self, title: Option<impl Into<SharedString>>) -> Self {
        self.title = title.map(Into::into);
        self
    }

    pub fn with_content(self, content: impl IntoElement) -> Self {
        Self {
            contents: Some(content.into_any_element()),
            ..self
        }
    }

    pub fn show_suppress_button(mut self, show: bool) -> Self {
        self.show_suppress_button = show;
        self
    }

    pub fn show_close_button(mut self, show: bool) -> Self {
        self.show_close_button = show;
        self
    }

    pub fn on_close(self, on_close: impl Fn(&bool, &mut Window, &mut App) + 'static) -> Self {
        Self {
            close: Some(Box::new(on_close)),
            ..self
        }
    }

    pub fn with_suffix(mut self, suffix: impl IntoElement) -> Self {
        self.suffix = Some(suffix.into_any_element());
        self
    }
}

impl RenderOnce for NotificationFrame {
    fn render(mut self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let entity = window.current_view();
        let show_suppress_button = self.show_suppress_button;
        let suppress = show_suppress_button && window.modifiers().shift;
        let (close_id, close_icon) = if suppress {
            ("suppress", IconName::Minimize)
        } else {
            ("close", IconName::Close)
        };

        ui::v_flex()
            .occlude()
            .p_3()
            .gap_2()
            .elevation_3(cx)
            .child(
                ui::h_flex()
                    .gap_4()
                    .justify_between()
                    .items_start()
                    .child(
                        ui::v_flex()
                            .gap_0p5()
                            .when_some(self.title.clone(), |div, title| {
                                div.child(Label::new(title))
                            })
                            .child(gpui::div().max_w_96().children(self.contents)),
                    )
                    .when(self.show_close_button, |this| {
                        this.on_modifiers_changed(move |_, _, cx| cx.notify(entity))
                            .child(
                                IconButton::new(close_id, close_icon)
                                    .tooltip(move |_window, cx| {
                                        if suppress {
                                            Tooltip::with_meta(
                                                "Suppress",
                                                None,
                                                "Click to Close",
                                                cx,
                                            )
                                        } else if show_suppress_button {
                                            Tooltip::with_meta(
                                                "Close",
                                                None,
                                                "Shift-click to Suppress",
                                                cx,
                                            )
                                        } else {
                                            Tooltip::simple("Close", cx)
                                        }
                                    })
                                    .on_click({
                                        let close = self.close.take();
                                        move |_, window, cx| {
                                            if let Some(close) = &close {
                                                close(&suppress, window, cx)
                                            }
                                        }
                                    }),
                            )
                    }),
            )
            .children(self.suffix)
    }
}

pub mod simple_message_notification {
    use gpui::{
        AnyElement, App, Context, DismissEvent, EventEmitter, FocusHandle, Focusable,
        ParentElement, Render, ScrollHandle, SharedString, Styled, Window, prelude::*,
    };
    use std::sync::Arc;

    use ui::{
        Button, ButtonCommon, ButtonVariant, Clickable, Color, Icon, IconName, IconSize, Label,
        LabelSize, WithScrollbar,
    };

    use crate::notifications::NotificationFrame;

    use super::{Notification, SuppressEvent};

    pub struct MessageNotification {
        focus_handle: FocusHandle,
        build_content: Box<dyn Fn(&mut Window, &mut Context<Self>) -> AnyElement>,
        primary_message: Option<SharedString>,
        primary_icon: Option<IconName>,
        primary_icon_color: Option<Color>,
        primary_on_click: Option<Arc<dyn Fn(&mut Window, &mut Context<Self>)>>,
        secondary_message: Option<SharedString>,
        secondary_icon: Option<IconName>,
        secondary_icon_color: Option<Color>,
        secondary_on_click: Option<Arc<dyn Fn(&mut Window, &mut Context<Self>)>>,
        more_info_message: Option<SharedString>,
        more_info_url: Option<Arc<str>>,
        show_close_button: bool,
        show_suppress_button: bool,
        title: Option<SharedString>,
        scroll_handle: ScrollHandle,
    }

    impl Focusable for MessageNotification {
        fn focus_handle(&self, _: &App) -> FocusHandle {
            self.focus_handle.clone()
        }
    }

    impl EventEmitter<DismissEvent> for MessageNotification {}
    impl EventEmitter<SuppressEvent> for MessageNotification {}

    impl Notification for MessageNotification {}

    impl MessageNotification {
        pub fn new<S>(message: S, cx: &mut App) -> MessageNotification
        where
            S: Into<SharedString>,
        {
            let message = message.into();
            Self::new_from_builder(cx, move |_, _| {
                Label::new(message.clone()).into_any_element()
            })
        }

        pub fn new_from_builder<F>(cx: &mut App, content: F) -> MessageNotification
        where
            F: 'static + Fn(&mut Window, &mut Context<Self>) -> AnyElement,
        {
            Self {
                build_content: Box::new(content),
                primary_message: None,
                primary_icon: None,
                primary_icon_color: None,
                primary_on_click: None,
                secondary_message: None,
                secondary_icon: None,
                secondary_icon_color: None,
                secondary_on_click: None,
                more_info_message: None,
                more_info_url: None,
                show_close_button: true,
                show_suppress_button: true,
                title: None,
                focus_handle: cx.focus_handle(),
                scroll_handle: ScrollHandle::new(),
            }
        }

        pub fn primary_message<S>(mut self, message: S) -> Self
        where
            S: Into<SharedString>,
        {
            self.primary_message = Some(message.into());
            self
        }

        pub fn primary_icon(mut self, icon: IconName) -> Self {
            self.primary_icon = Some(icon);
            self
        }

        pub fn primary_icon_color(mut self, color: Color) -> Self {
            self.primary_icon_color = Some(color);
            self
        }

        pub fn primary_on_click<F>(mut self, on_click: F) -> Self
        where
            F: 'static + Fn(&mut Window, &mut Context<Self>),
        {
            self.primary_on_click = Some(Arc::new(on_click));
            self
        }

        pub fn primary_on_click_arc<F>(mut self, on_click: Arc<F>) -> Self
        where
            F: 'static + Fn(&mut Window, &mut Context<Self>),
        {
            self.primary_on_click = Some(on_click);
            self
        }

        pub fn secondary_message<S>(mut self, message: S) -> Self
        where
            S: Into<SharedString>,
        {
            self.secondary_message = Some(message.into());
            self
        }

        pub fn secondary_icon(mut self, icon: IconName) -> Self {
            self.secondary_icon = Some(icon);
            self
        }

        pub fn secondary_icon_color(mut self, color: Color) -> Self {
            self.secondary_icon_color = Some(color);
            self
        }

        pub fn secondary_on_click<F>(mut self, on_click: F) -> Self
        where
            F: 'static + Fn(&mut Window, &mut Context<Self>),
        {
            self.secondary_on_click = Some(Arc::new(on_click));
            self
        }

        pub fn secondary_on_click_arc<F>(mut self, on_click: Arc<F>) -> Self
        where
            F: 'static + Fn(&mut Window, &mut Context<Self>),
        {
            self.secondary_on_click = Some(on_click);
            self
        }

        pub fn more_info_message<S>(mut self, message: S) -> Self
        where
            S: Into<SharedString>,
        {
            self.more_info_message = Some(message.into());
            self
        }

        pub fn more_info_url<S>(mut self, url: S) -> Self
        where
            S: Into<Arc<str>>,
        {
            self.more_info_url = Some(url.into());
            self
        }

        pub fn dismiss(&mut self, cx: &mut Context<Self>) {
            cx.emit(DismissEvent);
        }

        pub fn show_close_button(mut self, show: bool) -> Self {
            self.show_close_button = show;
            self
        }

        pub fn show_suppress_button(mut self, show: bool) -> Self {
            self.show_suppress_button = show;
            self
        }

        pub fn with_title<S>(mut self, title: S) -> Self
        where
            S: Into<SharedString>,
        {
            self.title = Some(title.into());
            self
        }
    }

    impl Render for MessageNotification {
        fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
            NotificationFrame::new()
                .with_title(self.title.clone())
                .with_content(
                    gpui::div()
                        .child(
                            gpui::div()
                                .id("message-notification-content")
                                .max_h(ui::vh(0.6, window))
                                .overflow_y_scroll()
                                .track_scroll(&self.scroll_handle.clone())
                                .child((self.build_content)(window, cx)),
                        )
                        .vertical_scrollbar_for(&self.scroll_handle, window, cx),
                )
                .show_close_button(self.show_close_button)
                .show_suppress_button(self.show_suppress_button)
                .on_close(cx.listener(|_, suppress, _, cx| {
                    if *suppress {
                        cx.emit(SuppressEvent);
                    } else {
                        cx.emit(DismissEvent);
                    }
                }))
                .with_suffix(
                    ui::h_flex()
                        .gap_1p5()
                        .children(self.primary_message.iter().map(|message| {
                            let mut button = Button::new(message.clone(), message.clone())
                                .label_size(LabelSize::Small)
                                .variant(ButtonVariant::Solid)
                                .on_click(cx.listener(|this, _, window, cx| {
                                    if let Some(on_click) = this.primary_on_click.as_ref() {
                                        (on_click)(window, cx)
                                    };
                                    this.dismiss(cx)
                                }));

                            if let Some(icon) = self.primary_icon {
                                button = button.start_icon(
                                    Icon::new(icon)
                                        .size(IconSize::Small)
                                        .color(self.primary_icon_color.unwrap_or(Color::Muted)),
                                );
                            }

                            button
                        }))
                        .children(self.secondary_message.iter().map(|message| {
                            let mut button = Button::new(message.clone(), message.clone())
                                .label_size(LabelSize::Small)
                                .on_click(cx.listener(|this, _, window, cx| {
                                    if let Some(on_click) = this.secondary_on_click.as_ref() {
                                        (on_click)(window, cx)
                                    };
                                    this.dismiss(cx)
                                }));

                            if let Some(icon) = self.secondary_icon {
                                button = button.start_icon(
                                    Icon::new(icon)
                                        .size(IconSize::Small)
                                        .color(self.secondary_icon_color.unwrap_or(Color::Muted)),
                                );
                            }

                            button
                        }))
                        .child(
                            ui::h_flex().w_full().justify_end().children(
                                self.more_info_message
                                    .iter()
                                    .zip(self.more_info_url.iter())
                                    .map(|(message, url)| {
                                        let url = url.clone();
                                        Button::new(message.clone(), message.clone())
                                            .label_size(LabelSize::Small)
                                            .variant(ButtonVariant::Solid)
                                            .end_icon(
                                                Icon::new(IconName::ArrowUpRight)
                                                    .size(IconSize::Indicator)
                                                    .color(Color::Muted),
                                            )
                                            .on_click(
                                                cx.listener(move |_, _, _, cx| cx.open_url(&url)),
                                            )
                                    }),
                            ),
                        ),
                )
        }
    }
}
