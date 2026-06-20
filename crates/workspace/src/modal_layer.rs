use gpui::{
    AnyView, App, Context, DismissEvent, Entity, EventEmitter, FocusHandle, Focusable, ManagedView,
    MouseButton, Render, Subscription, Window, prelude::*,
};

use theme::ActiveTheme;

#[derive(Debug)]
pub enum DismissDecision {
    Dismiss(bool),
    Pending,
}

pub trait ModalView: ManagedView {
    fn on_before_dismiss(&mut self, _: &mut Window, _: &mut Context<Self>) -> DismissDecision {
        DismissDecision::Dismiss(true)
    }

    fn fade_out_background(&self) -> bool {
        false
    }

    fn render_bare(&self) -> bool {
        false
    }
}

trait ModalViewHandle {
    fn on_before_dismiss(&mut self, window: &mut Window, cx: &mut App) -> DismissDecision;
    fn view(&self) -> AnyView;
    fn fade_out_background(&self, cx: &mut App) -> bool;
    fn render_bare(&self, cx: &mut App) -> bool;
}

impl<V: ModalView> ModalViewHandle for Entity<V> {
    fn on_before_dismiss(&mut self, window: &mut Window, cx: &mut App) -> DismissDecision {
        self.update(cx, |this, cx| this.on_before_dismiss(window, cx))
    }

    fn view(&self) -> AnyView {
        self.clone().into()
    }

    fn fade_out_background(&self, cx: &mut App) -> bool {
        self.read(cx).fade_out_background()
    }

    fn render_bare(&self, cx: &mut App) -> bool {
        self.read(cx).render_bare()
    }
}

struct ActiveModal {
    modal: Box<dyn ModalViewHandle>,
    previous_focus_handle: Option<FocusHandle>,
    focus_handle: FocusHandle,
    _dismiss_subscription: Subscription,
    _focus_out_subscription: Subscription,
}

pub(crate) struct ModalOpenedEvent;

pub struct ModalLayer {
    active_modal: Option<ActiveModal>,
    dismiss_on_focus_lost: bool,
}

impl EventEmitter<ModalOpenedEvent> for ModalLayer {}

impl ModalLayer {
    pub fn new() -> Self {
        Self {
            active_modal: None,
            dismiss_on_focus_lost: false,
        }
    }

    pub fn toggle_modal<V, B>(&mut self, window: &mut Window, cx: &mut Context<Self>, build_view: B)
    where
        V: ModalView,
        B: FnOnce(&mut Window, &mut Context<V>) -> V,
    {
        if let Some(active_modal) = &self.active_modal {
            let should_close = active_modal.modal.view().downcast::<V>().is_ok();
            let did_close = self.hide_modal(window, cx);
            if should_close || !did_close {
                return;
            }
        }
        let new_modal = cx.new(|cx| build_view(window, cx));
        self.show_modal(new_modal, window, cx);
        cx.emit(ModalOpenedEvent);
    }

    fn show_modal<V>(&mut self, new_modal: Entity<V>, window: &mut Window, cx: &mut Context<Self>)
    where
        V: ModalView,
    {
        let focus_handle = cx.focus_handle();
        let dismiss_subscription = cx.subscribe_in(
            &new_modal,
            window,
            |this, _, _: &DismissEvent, window, cx| {
                this.hide_modal(window, cx);
            },
        );
        let focus_out_subscription =
            cx.on_focus_out(&focus_handle, window, |this, _event, window, cx| {
                if this.dismiss_on_focus_lost {
                    this.hide_modal(window, cx);
                }
            });

        self.active_modal = Some(ActiveModal {
            modal: Box::new(new_modal.clone()),
            previous_focus_handle: window.focused(cx),
            focus_handle,
            _dismiss_subscription: dismiss_subscription,
            _focus_out_subscription: focus_out_subscription,
        });
        cx.defer_in(window, move |_, window, cx| {
            window.focus(&new_modal.focus_handle(cx), cx);
        });
        cx.notify();
    }

    pub fn hide_modal(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        let Some(active_modal) = self.active_modal.as_mut() else {
            self.dismiss_on_focus_lost = false;
            return false;
        };

        match active_modal.modal.on_before_dismiss(window, cx) {
            DismissDecision::Dismiss(should_dismiss) => {
                if !should_dismiss {
                    self.dismiss_on_focus_lost = !should_dismiss;
                    return false;
                }
            }
            DismissDecision::Pending => {
                self.dismiss_on_focus_lost = false;
                return false;
            }
        }

        if let Some(active_modal) = self.active_modal.take() {
            if let Some(previous_focus) = active_modal.previous_focus_handle
                && active_modal.focus_handle.contains_focused(window, cx)
            {
                previous_focus.focus(window, cx);
            }
            cx.notify();
        }
        self.dismiss_on_focus_lost = false;
        true
    }

    pub fn active_modal<V>(&self) -> Option<Entity<V>>
    where
        V: 'static,
    {
        let active_modal = self.active_modal.as_ref()?;
        active_modal.modal.view().downcast::<V>().ok()
    }

    pub fn has_active_modal(&self) -> bool {
        self.active_modal.is_some()
    }
}

impl Default for ModalLayer {
    fn default() -> Self {
        Self::new()
    }
}

impl Render for ModalLayer {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let Some(active_modal) = &self.active_modal else {
            return gpui::div().into_any_element();
        };

        if active_modal.modal.render_bare(cx) {
            return active_modal.modal.view().into_any_element();
        }

        gpui::div()
            .absolute()
            .size_full()
            .inset_0()
            .occlude()
            .when(active_modal.modal.fade_out_background(cx), |this| {
                let mut background = cx.theme().colors().elevated_surface_background;
                background.fade_out(0.2);
                this.bg(background)
            })
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _, window, cx| {
                    this.hide_modal(window, cx);
                }),
            )
            .child(
                gpui::div()
                    .flex()
                    .flex_col()
                    .h(gpui::px(0.0))
                    .top_20()
                    .items_center()
                    .track_focus(&active_modal.focus_handle)
                    .child(
                        gpui::div()
                            .flex()
                            .flex_row()
                            .occlude()
                            .child(active_modal.modal.view())
                            .on_mouse_down(MouseButton::Left, |_, _, cx| {
                                cx.stop_propagation();
                            }),
                    ),
            )
            .into_any_element()
    }
}
