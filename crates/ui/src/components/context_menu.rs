use gpui::{
    Action, App, DefiniteLength, DismissEvent, ElementId, Entity, EventEmitter, FocusHandle,
    Focusable, MouseDownEvent, SharedString, Subscription, Window, prelude::*,
};
use std::{rc::Rc, time::Duration};

use menu::{SelectFirst, SelectLast, SelectNext, SelectPrevious};
use theme::ThemeSettings;

use crate::{
    IconButtonShape, KeyBinding, List, ListItem, ListSeparator, ListSubHeader, Tooltip, prelude::*,
    utils::WithRemSize,
};

pub enum ContextMenuItem {
    Separator,
    Header(SharedString),
    HeaderWithLink(SharedString, SharedString, SharedString),
    Label(SharedString),
    Entry(ContextMenuEntry),
}

pub struct ContextMenuEntry {
    toggle: Option<(IconPosition, bool)>,
    label: SharedString,
    icon: Option<IconName>,
    icon_position: IconPosition,
    icon_size: IconSize,
    icon_color: Option<Color>,
    handler: Rc<dyn Fn(Option<&FocusHandle>, &mut Window, &mut App)>,
    secondary_handler: Option<Rc<dyn Fn(Option<&FocusHandle>, &mut Window, &mut App)>>,
    action: Option<Box<dyn Action>>,
    disabled: bool,
    end_slot_icon: Option<IconName>,
    end_slot_title: Option<SharedString>,
    end_slot_handler: Option<Rc<dyn Fn(Option<&FocusHandle>, &mut Window, &mut App)>>,
    show_end_slot_on_hover: bool,
}

impl ContextMenuEntry {
    pub fn new(label: impl Into<SharedString>) -> Self {
        ContextMenuEntry {
            toggle: None,
            label: label.into(),
            icon: None,
            icon_position: IconPosition::Start,
            icon_size: IconSize::Small,
            icon_color: None,
            handler: Rc::new(|_, _, _| {}),
            secondary_handler: None,
            action: None,
            disabled: false,
            end_slot_icon: None,
            end_slot_title: None,
            end_slot_handler: None,
            show_end_slot_on_hover: false,
        }
    }

    pub fn toggleable(mut self, toggle_position: IconPosition, toggled: bool) -> Self {
        self.toggle = Some((toggle_position, toggled));
        self
    }

    pub fn icon(mut self, icon: IconName) -> Self {
        self.icon = Some(icon);
        self
    }

    pub fn icon_position(mut self, position: IconPosition) -> Self {
        self.icon_position = position;
        self
    }

    pub fn icon_size(mut self, icon_size: IconSize) -> Self {
        self.icon_size = icon_size;
        self
    }

    pub fn icon_color(mut self, icon_color: Color) -> Self {
        self.icon_color = Some(icon_color);
        self
    }

    pub fn toggle(mut self, toggle_position: IconPosition, toggled: bool) -> Self {
        self.toggle = Some((toggle_position, toggled));
        self
    }

    pub fn action(mut self, action: Box<dyn Action>) -> Self {
        self.action = Some(action);
        self
    }

    pub fn handler(mut self, handler: impl Fn(&mut Window, &mut App) + 'static) -> Self {
        self.handler = Rc::new(move |_, window, cx| handler(window, cx));
        self
    }

    pub fn secondary_handler(mut self, handler: impl Fn(&mut Window, &mut App) + 'static) -> Self {
        self.secondary_handler = Some(Rc::new(move |_, window, cx| handler(window, cx)));
        self
    }

    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }
}

impl FluentBuilder for ContextMenuEntry {}

impl From<ContextMenuEntry> for ContextMenuItem {
    fn from(entry: ContextMenuEntry) -> Self {
        ContextMenuItem::Entry(entry)
    }
}

pub struct ContextMenu {
    builder: Option<Rc<dyn Fn(Self, &mut Window, &mut Context<Self>) -> Self>>,
    items: Vec<ContextMenuItem>,
    focus_handle: FocusHandle,
    action_context: Option<FocusHandle>,
    selected_index: Option<usize>,
    delayed: bool,
    clicked: bool,
    end_slot_action: Option<Box<dyn Action>>,
    key_context: SharedString,
    _on_blur_subscription: Subscription,
    keep_open_on_confirm: bool,
    fixed_width: Option<DefiniteLength>,
}

impl Focusable for ContextMenu {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl EventEmitter<DismissEvent> for ContextMenu {}

impl FluentBuilder for ContextMenu {}

impl ContextMenu {
    pub fn new(
        window: &mut Window,
        cx: &mut Context<Self>,
        builder: impl FnOnce(Self, &mut Window, &mut Context<Self>) -> Self,
    ) -> Self {
        let focus_handle = cx.focus_handle();
        let _on_blur_subscription = cx.on_blur(
            &focus_handle,
            window,
            |this: &mut ContextMenu, window, cx| this.cancel(&menu::Cancel, window, cx),
        );

        window.refresh();

        let menu = Self {
            builder: None,
            focus_handle,
            _on_blur_subscription,
            items: Default::default(),
            action_context: None,
            selected_index: None,
            delayed: false,
            clicked: false,
            end_slot_action: None,
            keep_open_on_confirm: false,
            fixed_width: None,
            key_context: "menu".into(),
        };

        (builder)(menu, window, cx)
    }

    pub fn build(
        window: &mut Window,
        cx: &mut App,
        f: impl FnOnce(Self, &mut Window, &mut Context<Self>) -> Self,
    ) -> Entity<Self> {
        cx.new(|cx| Self::new(window, cx, f))
    }

    /// Builds a [`ContextMenu`] that will stay open instead of closing after each confirmation.
    ///
    /// The main difference from [`ContextMenu::build`] is the type of the `builder`, as we
    /// need to be able to hold onto it to call it again.
    pub fn build_persistent(
        window: &mut Window,
        cx: &mut App,
        builder: impl Fn(Self, &mut Window, &mut Context<Self>) -> Self + 'static,
    ) -> Entity<Self> {
        cx.new(|cx| {
            let builder = Rc::new(builder);
            let focus_handle = cx.focus_handle();
            let _on_blur_subscription = cx.on_blur(
                &focus_handle,
                window,
                |this: &mut ContextMenu, window, cx| this.cancel(&menu::Cancel, window, cx),
            );

            window.refresh();

            let menu = Self {
                builder: Some(builder.clone()),
                focus_handle,
                _on_blur_subscription,
                items: Default::default(),
                action_context: None,
                selected_index: None,
                delayed: false,
                clicked: false,
                end_slot_action: None,
                keep_open_on_confirm: true,
                fixed_width: None,
                key_context: "menu".into(),
            };

            (builder)(menu, window, cx)
        })
    }

    /// Used to refresh the menu entries when entries are toggled when the menu is configured with
    /// keep_open_on_confirm = true`.
    ///
    /// This only works if the [`ContextMenu`] was constructed using [`ContextMenu::build_persistent`].
    /// Otherwise it is a no-op.
    pub fn rebuild(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(builder) = self.builder.clone() else {
            return;
        };

        let focus_handle = cx.focus_handle();
        let _on_blur_subscription = cx.on_blur(
            &focus_handle,
            window,
            |this: &mut ContextMenu, window, cx| this.cancel(&menu::Cancel, window, cx),
        );
        let menu = Self {
            builder: Some(builder.clone()),
            focus_handle: focus_handle.clone(),
            _on_blur_subscription,
            items: Default::default(),
            action_context: None,
            selected_index: None,
            delayed: false,
            clicked: false,
            end_slot_action: None,
            keep_open_on_confirm: false,
            fixed_width: None,
            key_context: "menu".into(),
        };

        let new_menu = (builder)(menu, window, cx);
        self.items = new_menu.items;
        cx.notify();
    }

    pub fn context(mut self, focus: FocusHandle) -> Self {
        self.action_context = Some(focus);
        self
    }

    pub fn header(mut self, title: impl Into<SharedString>) -> Self {
        self.items.push(ContextMenuItem::Header(title.into()));
        self
    }

    pub fn header_with_link(
        mut self,
        title: impl Into<SharedString>,
        link_label: impl Into<SharedString>,
        link_url: impl Into<SharedString>,
    ) -> Self {
        self.items.push(ContextMenuItem::HeaderWithLink(
            title.into(),
            link_label.into(),
            link_url.into(),
        ));
        self
    }

    pub fn separator(mut self) -> Self {
        self.items.push(ContextMenuItem::Separator);
        self
    }

    pub fn extend<I: Into<ContextMenuItem>>(mut self, items: impl IntoIterator<Item = I>) -> Self {
        self.items.extend(items.into_iter().map(Into::into));
        self
    }

    pub fn item(mut self, item: impl Into<ContextMenuItem>) -> Self {
        self.items.push(item.into());
        self
    }

    pub fn push_item(&mut self, item: impl Into<ContextMenuItem>) {
        self.items.push(item.into());
    }

    pub fn entry(
        mut self,
        label: impl Into<SharedString>,
        action: Option<Box<dyn Action>>,
        handler: impl Fn(&mut Window, &mut App) + 'static,
    ) -> Self {
        self.items.push(ContextMenuItem::Entry(ContextMenuEntry {
            toggle: None,
            label: label.into(),
            handler: Rc::new(move |_, window, cx| handler(window, cx)),
            secondary_handler: None,
            icon: None,
            icon_position: IconPosition::End,
            icon_size: IconSize::Small,
            icon_color: None,
            action,
            disabled: false,
            end_slot_icon: None,
            end_slot_title: None,
            end_slot_handler: None,
            show_end_slot_on_hover: false,
        }));
        self
    }

    pub fn entry_with_end_slot(
        mut self,
        label: impl Into<SharedString>,
        action: Option<Box<dyn Action>>,
        handler: impl Fn(&mut Window, &mut App) + 'static,
        end_slot_icon: IconName,
        end_slot_title: SharedString,
        end_slot_handler: impl Fn(&mut Window, &mut App) + 'static,
    ) -> Self {
        self.items.push(ContextMenuItem::Entry(ContextMenuEntry {
            toggle: None,
            label: label.into(),
            handler: Rc::new(move |_, window, cx| handler(window, cx)),
            secondary_handler: None,
            icon: None,
            icon_position: IconPosition::End,
            icon_size: IconSize::Small,
            icon_color: None,
            action,
            disabled: false,
            end_slot_icon: Some(end_slot_icon),
            end_slot_title: Some(end_slot_title),
            end_slot_handler: Some(Rc::new(move |_, window, cx| end_slot_handler(window, cx))),
            show_end_slot_on_hover: false,
        }));
        self
    }

    pub fn entry_with_end_slot_on_hover(
        mut self,
        label: impl Into<SharedString>,
        action: Option<Box<dyn Action>>,
        handler: impl Fn(&mut Window, &mut App) + 'static,
        end_slot_icon: IconName,
        end_slot_title: SharedString,
        end_slot_handler: impl Fn(&mut Window, &mut App) + 'static,
    ) -> Self {
        self.items.push(ContextMenuItem::Entry(ContextMenuEntry {
            toggle: None,
            label: label.into(),
            handler: Rc::new(move |_, window, cx| handler(window, cx)),
            secondary_handler: None,
            icon: None,
            icon_position: IconPosition::End,
            icon_size: IconSize::Small,
            icon_color: None,
            action,
            disabled: false,
            end_slot_icon: Some(end_slot_icon),
            end_slot_title: Some(end_slot_title),
            end_slot_handler: Some(Rc::new(move |_, window, cx| end_slot_handler(window, cx))),
            show_end_slot_on_hover: true,
        }));
        self
    }

    pub fn toggleable_entry(
        mut self,
        label: impl Into<SharedString>,
        toggled: bool,
        position: IconPosition,
        action: Option<Box<dyn Action>>,
        handler: impl Fn(&mut Window, &mut App) + 'static,
    ) -> Self {
        self.items.push(ContextMenuItem::Entry(ContextMenuEntry {
            toggle: Some((position, toggled)),
            label: label.into(),
            handler: Rc::new(move |_, window, cx| handler(window, cx)),
            secondary_handler: None,
            icon: None,
            icon_position: position,
            icon_size: IconSize::Small,
            icon_color: None,
            action,
            disabled: false,
            end_slot_icon: None,
            end_slot_title: None,
            end_slot_handler: None,
            show_end_slot_on_hover: false,
        }));
        self
    }

    pub fn label(mut self, label: impl Into<SharedString>) -> Self {
        self.items.push(ContextMenuItem::Label(label.into()));
        self
    }

    pub fn action(self, label: impl Into<SharedString>, action: Box<dyn Action>) -> Self {
        self.action_checked(label, action, false)
    }

    pub fn action_checked(
        mut self,
        label: impl Into<SharedString>,
        action: Box<dyn Action>,
        checked: bool,
    ) -> Self {
        self.items.push(ContextMenuItem::Entry(ContextMenuEntry {
            toggle: if checked {
                Some((IconPosition::Start, true))
            } else {
                None
            },
            label: label.into(),
            action: Some(action.boxed_clone()),
            handler: Rc::new(move |context, window, cx| {
                if let Some(context) = &context {
                    window.focus(context, cx);
                }
                window.dispatch_action(action.boxed_clone(), cx);
            }),
            secondary_handler: None,
            icon: None,
            icon_position: IconPosition::End,
            icon_size: IconSize::Small,
            icon_color: None,
            disabled: false,
            end_slot_icon: None,
            end_slot_title: None,
            end_slot_handler: None,
            show_end_slot_on_hover: false,
        }));
        self
    }

    pub fn action_disabled_when(
        mut self,
        disabled: bool,
        label: impl Into<SharedString>,
        action: Box<dyn Action>,
    ) -> Self {
        self.items.push(ContextMenuItem::Entry(ContextMenuEntry {
            toggle: None,
            label: label.into(),
            action: Some(action.boxed_clone()),
            handler: Rc::new(move |context, window, cx| {
                if let Some(context) = &context {
                    window.focus(context, cx);
                }
                window.dispatch_action(action.boxed_clone(), cx);
            }),
            secondary_handler: None,
            icon: None,
            icon_size: IconSize::Small,
            icon_position: IconPosition::End,
            icon_color: None,
            disabled,
            end_slot_icon: None,
            end_slot_title: None,
            end_slot_handler: None,
            show_end_slot_on_hover: false,
        }));
        self
    }

    pub fn link(self, label: impl Into<SharedString>, action: Box<dyn Action>) -> Self {
        self.link_with_handler(label, action, |_, _| {})
    }

    pub fn link_with_handler(
        mut self,
        label: impl Into<SharedString>,
        action: Box<dyn Action>,
        handler: impl Fn(&mut Window, &mut App) + 'static,
    ) -> Self {
        self.items.push(ContextMenuItem::Entry(ContextMenuEntry {
            toggle: None,
            label: label.into(),
            action: Some(action.boxed_clone()),
            handler: Rc::new(move |_, window, cx| {
                handler(window, cx);
                window.dispatch_action(action.boxed_clone(), cx);
            }),
            secondary_handler: None,
            icon: Some(IconName::ArrowUpRight),
            icon_size: IconSize::XSmall,
            icon_position: IconPosition::End,
            icon_color: None,
            disabled: false,
            end_slot_icon: None,
            end_slot_title: None,
            end_slot_handler: None,
            show_end_slot_on_hover: false,
        }));
        self
    }

    pub fn keep_open_on_confirm(mut self, keep_open: bool) -> Self {
        self.keep_open_on_confirm = keep_open;
        self
    }

    pub fn trigger_end_slot_handler(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(entry) = self.selected_index.and_then(|idx| self.items.get(idx)) else {
            return;
        };
        let ContextMenuItem::Entry(entry) = entry else {
            return;
        };
        let Some(handler) = entry.end_slot_handler.as_ref() else {
            return;
        };
        (handler)(None, window, cx);
    }

    pub fn fixed_width(mut self, width: DefiniteLength) -> Self {
        self.fixed_width = Some(width);
        self
    }

    pub fn end_slot_action(mut self, action: Box<dyn Action>) -> Self {
        self.end_slot_action = Some(action);
        self
    }

    pub fn key_context(mut self, context: impl Into<SharedString>) -> Self {
        self.key_context = context.into();
        self
    }

    pub fn selected_index(&self) -> Option<usize> {
        self.selected_index
    }

    pub fn confirm(&mut self, _: &menu::Confirm, window: &mut Window, cx: &mut Context<Self>) {
        let Some(idx) = self.selected_index else {
            return;
        };

        let context = self.action_context.as_ref();

        if let Some(ContextMenuItem::Entry(ContextMenuEntry {
            handler,
            disabled: false,
            ..
        })) = self.items.get(idx)
        {
            (handler)(context, window, cx)
        }

        if self.keep_open_on_confirm {
            self.rebuild(window, cx);
        } else {
            cx.emit(DismissEvent);
        }
    }

    pub fn secondary_confirm(
        &mut self,
        _: &menu::SecondaryConfirm,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(idx) = self.selected_index else {
            return;
        };

        let context = self.action_context.as_ref();

        if let Some(ContextMenuItem::Entry(ContextMenuEntry {
            handler,
            secondary_handler,
            disabled: false,
            ..
        })) = self.items.get(idx)
        {
            if let Some(secondary) = secondary_handler {
                (secondary)(context, window, cx)
            } else {
                (handler)(context, window, cx)
            }
        }

        if self.keep_open_on_confirm {
            self.rebuild(window, cx);
        } else {
            cx.emit(DismissEvent);
        }
    }

    pub fn cancel(&mut self, _: &menu::Cancel, _window: &mut Window, cx: &mut Context<Self>) {
        cx.emit(DismissEvent);
    }

    pub fn end_slot(&mut self, _: &dyn Action, window: &mut Window, cx: &mut Context<Self>) {
        let Some(item) = self.selected_index.and_then(|idx| self.items.get(idx)) else {
            return;
        };
        let ContextMenuItem::Entry(entry) = item else {
            return;
        };
        let Some(handler) = entry.end_slot_handler.as_ref() else {
            return;
        };
        handler(None, window, cx);
        self.rebuild(window, cx);
        cx.notify();
    }

    pub fn clear_selected(&mut self) {
        self.selected_index = None;
    }

    pub fn select_first(&mut self, _: &SelectFirst, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(idx) = self.items.iter().position(|item| item.is_selectable()) {
            self.select_index(idx, window, cx);
        }
        cx.notify();
    }

    pub fn select_last(&mut self, window: &mut Window, cx: &mut Context<Self>) -> Option<usize> {
        for (idx, item) in self.items.iter().enumerate().rev() {
            if item.is_selectable() {
                return self.select_index(idx, window, cx);
            }
        }
        None
    }

    fn handle_select_last(&mut self, _: &SelectLast, window: &mut Window, cx: &mut Context<Self>) {
        if self.select_last(window, cx).is_some() {
            cx.notify();
        }
    }

    pub fn select_next(&mut self, _: &SelectNext, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(idx) = self.selected_index {
            let next_index = idx + 1;
            if self.items.len() <= next_index {
                self.select_first(&SelectFirst, window, cx);
                return;
            } else {
                for (idx, item) in self.items.iter().enumerate().skip(next_index) {
                    if item.is_selectable() {
                        self.select_index(idx, window, cx);
                        cx.notify();
                        return;
                    }
                }
            }
        }
        self.select_first(&SelectFirst, window, cx);
    }

    pub fn select_previous(
        &mut self,
        _: &SelectPrevious,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(idx) = self.selected_index {
            for (idx, item) in self.items.iter().enumerate().take(idx).rev() {
                if item.is_selectable() {
                    self.select_index(idx, window, cx);
                    cx.notify();
                    return;
                }
            }
        }
        self.handle_select_last(&SelectLast, window, cx);
    }

    fn select_index(
        &mut self,
        idx: usize,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<usize> {
        let item = self.items.get(idx)?;
        if item.is_selectable() {
            self.selected_index = Some(idx);
        }
        Some(idx)
    }

    pub fn on_action_dispatch(
        &mut self,
        dispatched: &dyn Action,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.clicked {
            cx.propagate();
            return;
        }

        if let Some(idx) = self.items.iter().position(|item| {
            if let ContextMenuItem::Entry(ContextMenuEntry {
                action: Some(action),
                disabled: false,
                ..
            }) = item
            {
                action.partial_eq(dispatched)
            } else {
                false
            }
        }) {
            self.select_index(idx, window, cx);
            self.delayed = true;
            cx.notify();
            let action = dispatched.boxed_clone();
            cx.spawn_in(window, async move |this, cx| {
                cx.background_executor()
                    .timer(Duration::from_millis(50))
                    .await;
                cx.update(|window, cx| {
                    this.update(cx, |this, cx| {
                        this.cancel(&menu::Cancel, window, cx);
                        window.dispatch_action(action, cx);
                    })
                })
            })
            .detach_and_log_err(cx);
        } else {
            cx.propagate()
        }
    }

    pub fn on_blur_subscription(mut self, new_subscription: Subscription) -> Self {
        self._on_blur_subscription = new_subscription;
        self
    }

    fn render_menu_item(
        &self,
        idx: usize,
        item: &ContextMenuItem,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        match item {
            ContextMenuItem::Separator => ListSeparator.into_any_element(),
            ContextMenuItem::Header(header) => ListSubHeader::new(header.clone())
                .inset(true)
                .into_any_element(),
            ContextMenuItem::HeaderWithLink(header, label, url) => {
                let url = url.clone();
                let link_id = ElementId::Name(format!("link-{}", url).into());
                ListSubHeader::new(header.clone())
                    .inset(true)
                    .end_slot(
                        Button::new(link_id, label.clone())
                            .color(Color::Muted)
                            .label_size(LabelSize::Small)
                            .size(ButtonSize::None)
                            .variant(ButtonVariant::Ghost)
                            .on_click(move |_, _, cx| {
                                let url = url.clone();
                                cx.open_url(&url);
                            })
                            .into_any_element(),
                    )
                    .into_any_element()
            }
            ContextMenuItem::Label(label) => ListItem::new(idx)
                .inset(true)
                .disabled(true)
                .child(Label::new(label.clone()))
                .into_any_element(),
            ContextMenuItem::Entry(entry) => {
                self.render_menu_entry(idx, entry, cx).into_any_element()
            }
        }
    }

    fn render_menu_entry(
        &self,
        idx: usize,
        entry: &ContextMenuEntry,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let ContextMenuEntry {
            toggle,
            label,
            handler,
            icon,
            icon_position,
            icon_size,
            icon_color,
            action,
            disabled,
            end_slot_icon,
            end_slot_title,
            end_slot_handler,
            show_end_slot_on_hover,
            secondary_handler: _,
        } = entry;

        let this = cx.weak_entity();
        let handler = handler.clone();
        let menu = cx.entity().downgrade();

        let icon_color = if *disabled {
            Color::Muted
        } else if toggle.is_some() {
            icon_color.unwrap_or(Color::Accent)
        } else {
            icon_color.unwrap_or(Color::Default)
        };

        let label_color = if *disabled {
            Color::Disabled
        } else {
            Color::Default
        };

        let label_element = if let Some(icon_name) = icon {
            h_flex()
                .gap_1p5()
                .when(
                    *icon_position == IconPosition::Start && toggle.is_none(),
                    |flex| flex.child(Icon::new(*icon_name).size(*icon_size).color(icon_color)),
                )
                .child(Label::new(label.clone()).color(label_color).truncate())
                .when(*icon_position == IconPosition::End, |flex| {
                    flex.child(Icon::new(*icon_name).size(*icon_size).color(icon_color))
                })
                .into_any_element()
        } else {
            Label::new(label.clone())
                .color(label_color)
                .truncate()
                .into_any_element()
        };

        gpui::div()
            .id(("context-menu-child", idx))
            .child(
                ListItem::new(idx)
                    .group_name("label-container")
                    .inset(true)
                    .disabled(*disabled)
                    .toggle_state(Some(idx) == self.selected_index)
                    .when(!*disabled, |item| {
                        item.on_hover(cx.listener(|this, hovered, window, cx| {
                            if *hovered {
                                this.clear_selected();
                                window.focus(&this.focus_handle.clone(), cx);
                            }
                        }))
                    })
                    .when_some(*toggle, |list_item, (position, toggled)| {
                        let contents = gpui::div()
                            .flex_none()
                            .child(
                                Icon::new(icon.unwrap_or(IconName::Check))
                                    .color(icon_color)
                                    .size(*icon_size),
                            )
                            .when(!toggled, |contents| contents.invisible());

                        match position {
                            IconPosition::Start => list_item.start_slot(contents),
                            IconPosition::End => list_item.end_slot(contents),
                        }
                    })
                    .child(
                        h_flex()
                            .w_full()
                            .justify_between()
                            .child(label_element)
                            .debug_selector(|| format!("MENU_ITEM-{}", label))
                            .children(action.as_ref().map(|action| {
                                let binding = self
                                    .action_context
                                    .as_ref()
                                    .map(|focus| KeyBinding::for_action_in(&**action, focus, cx))
                                    .unwrap_or_else(|| KeyBinding::for_action(&**action, cx));

                                gpui::div().ml_4().child(binding.disabled(*disabled))
                            })),
                    )
                    .when_some(
                        end_slot_icon
                            .as_ref()
                            .zip(self.end_slot_action.as_ref())
                            .zip(end_slot_title.as_ref())
                            .zip(end_slot_handler.as_ref()),
                        |el, (((icon, action), title), handler)| {
                            el.end_slot({
                                let icon_button = IconButton::new("end-slot-icon", *icon)
                                    .shape(IconButtonShape::Square)
                                    .tooltip({
                                        let action_context = self.action_context.clone();
                                        let title = title.clone();
                                        let action = action.boxed_clone();
                                        move |_window, cx| {
                                            action_context
                                                .as_ref()
                                                .map(|focus| {
                                                    Tooltip::for_action_in(
                                                        title.clone(),
                                                        &*action,
                                                        focus,
                                                        cx,
                                                    )
                                                })
                                                .unwrap_or_else(|| {
                                                    Tooltip::for_action(title.clone(), &*action, cx)
                                                })
                                        }
                                    })
                                    .on_click({
                                        let handler = handler.clone();
                                        move |_, window, cx| {
                                            handler(None, window, cx);
                                            this.update(cx, |this, cx| {
                                                this.rebuild(window, cx);
                                                cx.notify();
                                            })
                                            .ok();
                                        }
                                    });

                                if *show_end_slot_on_hover {
                                    gpui::div()
                                        .visible_on_hover("label-container")
                                        .child(icon_button)
                                        .into_any_element()
                                } else {
                                    icon_button.into_any_element()
                                }
                            })
                        },
                    )
                    .on_click({
                        let context = self.action_context.clone();
                        let keep_open_on_confirm = self.keep_open_on_confirm;
                        move |_, window, cx| {
                            handler(context.as_ref(), window, cx);
                            menu.update(cx, |menu, cx| {
                                menu.clicked = true;
                                if keep_open_on_confirm {
                                    menu.rebuild(window, cx);
                                } else {
                                    cx.emit(DismissEvent);
                                }
                            })
                            .ok();
                        }
                    }),
            )
            .into_any_element()
    }
}

impl ContextMenuItem {
    fn is_selectable(&self) -> bool {
        match self {
            ContextMenuItem::Header(_)
            | ContextMenuItem::HeaderWithLink(_, _, _)
            | ContextMenuItem::Separator
            | ContextMenuItem::Label(_) => false,
            ContextMenuItem::Entry(ContextMenuEntry { disabled, .. }) => !disabled,
        }
    }
}

impl Render for ContextMenu {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let ui_font_size = ThemeSettings::get_global(cx).ui_font_size(cx);
        WithRemSize::new(ui_font_size)
            .occlude()
            .elevation_2(cx)
            .flex()
            .flex_row()
            .flex_shrink_0()
            .child(
                v_flex()
                    .id("context-menu")
                    .max_h(vh(0.75, window))
                    .flex_shrink_0()
                    .when_some(self.fixed_width, |this, width| {
                        this.w(width).overflow_x_hidden()
                    })
                    .when(self.fixed_width.is_none(), |this| {
                        this.min_w(gpui::px(200.)).flex_1()
                    })
                    .overflow_y_scroll()
                    .track_focus(&self.focus_handle(cx))
                    .key_context(self.key_context.as_ref())
                    .on_action(cx.listener(ContextMenu::select_first))
                    .on_action(cx.listener(ContextMenu::handle_select_last))
                    .on_action(cx.listener(ContextMenu::select_next))
                    .on_action(cx.listener(ContextMenu::select_previous))
                    .on_action(cx.listener(ContextMenu::confirm))
                    .on_action(cx.listener(ContextMenu::secondary_confirm))
                    .on_action(cx.listener(ContextMenu::cancel))
                    .on_mouse_down_out(cx.listener(|this, _event: &MouseDownEvent, window, cx| {
                        this.cancel(&menu::Cancel, window, cx)
                    }))
                    .when_some(self.end_slot_action.as_ref(), |el, action| {
                        el.on_boxed_action(&**action, cx.listener(ContextMenu::end_slot))
                    })
                    .when(!self.delayed, |mut el| {
                        for item in self.items.iter() {
                            if let ContextMenuItem::Entry(ContextMenuEntry {
                                action: Some(action),
                                disabled: false,
                                ..
                            }) = item
                            {
                                el = el.on_boxed_action(
                                    &**action,
                                    cx.listener(ContextMenu::on_action_dispatch),
                                );
                            }
                        }
                        el
                    })
                    .child(
                        List::new().children(
                            self.items
                                .iter()
                                .enumerate()
                                .map(|(idx, item)| self.render_menu_item(idx, item, window, cx)),
                        ),
                    ),
            )
    }
}
