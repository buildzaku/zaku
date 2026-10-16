use gpui::{
    Action, Anchor, App, Bounds, DefiniteLength, DismissEvent, ElementId, Entity, EventEmitter,
    FocusHandle, Focusable, MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent, Pixels,
    Point, SharedString, Size, Subscription, Window, prelude::*,
};
use std::{
    cell::Cell,
    rc::Rc,
    time::{Duration, Instant},
};

use theme::ThemeSettings;

use crate::{
    Button, ButtonCommon, ButtonSize, ButtonVariant, Clickable, Color, Disableable, Icon,
    IconAsset, IconButton, IconButtonShape, IconPosition, IconSize, KeyBinding, List, ListItem,
    ListSeparator, ListSubHeader, StyledExt, Text, TextCommon, TextSize, Toggleable, Tooltip,
    VisibleOnHover, utils::WithRemSize,
};

#[derive(Clone, Copy)]
enum SubmenuOpenTrigger {
    Pointer,
    Keyboard,
}

struct OpenSubmenu {
    item_index: usize,
    entity: Entity<ContextMenu>,
    trigger_bounds: Option<Bounds<Pixels>>,
    offset: Option<Pixels>,
    flip_left: bool,
    _dismiss_subscription: Subscription,
}

enum SubmenuState {
    Closed,
    Open(OpenSubmenu),
}

#[derive(Clone, Copy, PartialEq)]
enum HoverTarget {
    MainMenu,
    Submenu,
}

pub enum ContextMenuItem {
    Separator,
    Header(SharedString),
    HeaderWithLink(SharedString, SharedString, SharedString),
    Text(SharedString),
    Entry(ContextMenuEntry),
    Submenu {
        label: SharedString,
        icon: Option<IconAsset>,
        icon_color: Option<Color>,
        builder: Rc<dyn Fn(ContextMenu, &mut Window, &mut Context<ContextMenu>) -> ContextMenu>,
    },
}

impl ContextMenuItem {
    fn is_selectable(&self) -> bool {
        match self {
            ContextMenuItem::Header(_)
            | ContextMenuItem::HeaderWithLink(_, _, _)
            | ContextMenuItem::Separator
            | ContextMenuItem::Text(_) => false,
            ContextMenuItem::Entry(ContextMenuEntry { disabled, .. }) => !disabled,
            ContextMenuItem::Submenu { .. } => true,
        }
    }
}

impl From<ContextMenuEntry> for ContextMenuItem {
    fn from(entry: ContextMenuEntry) -> Self {
        ContextMenuItem::Entry(entry)
    }
}

pub struct ContextMenuEntry {
    toggle: Option<(IconPosition, bool)>,
    text: SharedString,
    icon: Option<IconAsset>,
    icon_position: IconPosition,
    icon_size: IconSize,
    icon_color: Option<Color>,
    handler: Rc<dyn Fn(Option<&FocusHandle>, &mut Window, &mut App)>,
    secondary_handler: Option<Rc<dyn Fn(Option<&FocusHandle>, &mut Window, &mut App)>>,
    action: Option<Box<dyn Action>>,
    disabled: bool,
    end_slot_icon: Option<IconAsset>,
    end_slot_title: Option<SharedString>,
    end_slot_handler: Option<Rc<dyn Fn(Option<&FocusHandle>, &mut Window, &mut App)>>,
    show_end_slot_on_hover: bool,
}

impl ContextMenuEntry {
    pub fn new(text: impl Into<SharedString>) -> Self {
        ContextMenuEntry {
            toggle: None,
            text: text.into(),
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

    pub fn icon(mut self, icon: IconAsset) -> Self {
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
    on_blur_subscription: Subscription,
    keep_open_on_confirm: bool,
    fixed_width: Option<DefiniteLength>,
    main_menu: Option<Entity<ContextMenu>>,
    main_menu_observed_bounds: Rc<Cell<Option<Bounds<Pixels>>>>,
    submenu_state: SubmenuState,
    hover_target: HoverTarget,
    submenu_safety_threshold_x: Option<Pixels>,
    submenu_trigger_bounds: Rc<Cell<Option<Bounds<Pixels>>>>,
    submenu_trigger_mouse_down: bool,
    ignore_blur_until: Option<Instant>,
}

impl ContextMenu {
    pub fn new(
        window: &mut Window,
        cx: &mut Context<Self>,
        builder: impl FnOnce(Self, &mut Window, &mut Context<Self>) -> Self,
    ) -> Self {
        let focus_handle = cx.focus_handle();
        let on_blur_subscription = cx.on_blur(
            &focus_handle,
            window,
            |this: &mut ContextMenu, window, cx| {
                if let Some(ignore_until) = this.ignore_blur_until {
                    if Instant::now() < ignore_until {
                        return;
                    }
                    this.ignore_blur_until = None;
                }

                if this.main_menu.is_none()
                    && let SubmenuState::Open(open_submenu) = &this.submenu_state
                {
                    let submenu_focus = open_submenu.entity.read(cx).focus_handle.clone();
                    if submenu_focus.contains_focused(window, cx) {
                        return;
                    }
                }

                this.cancel(&actions::menu::Cancel, window, cx);
            },
        );

        window.refresh();

        let menu = Self {
            builder: None,
            focus_handle,
            on_blur_subscription,
            items: Vec::default(),
            action_context: None,
            selected_index: None,
            delayed: false,
            clicked: false,
            end_slot_action: None,
            keep_open_on_confirm: false,
            fixed_width: None,
            key_context: "menu".into(),
            main_menu: None,
            main_menu_observed_bounds: Rc::new(Cell::new(None)),
            submenu_state: SubmenuState::Closed,
            hover_target: HoverTarget::MainMenu,
            submenu_safety_threshold_x: None,
            submenu_trigger_bounds: Rc::new(Cell::new(None)),
            submenu_trigger_mouse_down: false,
            ignore_blur_until: None,
        };

        (builder)(menu, window, cx)
    }

    pub fn build(
        window: &mut Window,
        cx: &mut App,
        builder: impl FnOnce(Self, &mut Window, &mut Context<Self>) -> Self,
    ) -> Entity<Self> {
        cx.new(|cx| Self::new(window, cx, builder))
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
            let on_blur_subscription = cx.on_blur(
                &focus_handle,
                window,
                |this: &mut ContextMenu, window, cx| {
                    if let Some(ignore_until) = this.ignore_blur_until {
                        if Instant::now() < ignore_until {
                            return;
                        }
                        this.ignore_blur_until = None;
                    }

                    if this.main_menu.is_none()
                        && let SubmenuState::Open(open_submenu) = &this.submenu_state
                    {
                        let submenu_focus = open_submenu.entity.read(cx).focus_handle.clone();
                        if submenu_focus.contains_focused(window, cx) {
                            return;
                        }
                    }

                    this.cancel(&actions::menu::Cancel, window, cx);
                },
            );

            window.refresh();

            let menu = Self {
                builder: Some(builder.clone()),
                focus_handle,
                on_blur_subscription,
                items: Vec::default(),
                action_context: None,
                selected_index: None,
                delayed: false,
                clicked: false,
                end_slot_action: None,
                keep_open_on_confirm: true,
                fixed_width: None,
                key_context: "menu".into(),
                main_menu: None,
                main_menu_observed_bounds: Rc::new(Cell::new(None)),
                submenu_state: SubmenuState::Closed,
                hover_target: HoverTarget::MainMenu,
                submenu_safety_threshold_x: None,
                submenu_trigger_bounds: Rc::new(Cell::new(None)),
                submenu_trigger_mouse_down: false,
                ignore_blur_until: None,
            };

            (builder)(menu, window, cx)
        })
    }

    /// Used to refresh the menu entries when entries are toggled when the menu is configured with
    /// `keep_open_on_confirm = true`.
    ///
    /// This only works if the [`ContextMenu`] was constructed using [`ContextMenu::build_persistent`].
    /// Otherwise it is a no-op.
    pub fn rebuild(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(builder) = self.builder.clone() else {
            return;
        };

        let focus_handle = cx.focus_handle();
        let on_blur_subscription = cx.on_blur(
            &focus_handle,
            window,
            |this: &mut ContextMenu, window, cx| {
                if let Some(ignore_until) = this.ignore_blur_until {
                    if Instant::now() < ignore_until {
                        return;
                    }
                    this.ignore_blur_until = None;
                }

                if this.main_menu.is_none()
                    && let SubmenuState::Open(open_submenu) = &this.submenu_state
                {
                    let submenu_focus = open_submenu.entity.read(cx).focus_handle.clone();
                    if submenu_focus.contains_focused(window, cx) {
                        return;
                    }
                }

                this.cancel(&actions::menu::Cancel, window, cx);
            },
        );
        let menu = Self {
            builder: Some(builder.clone()),
            focus_handle: focus_handle.clone(),
            on_blur_subscription,
            items: Vec::default(),
            action_context: None,
            selected_index: None,
            delayed: false,
            clicked: false,
            end_slot_action: None,
            keep_open_on_confirm: false,
            fixed_width: None,
            key_context: "menu".into(),
            main_menu: None,
            main_menu_observed_bounds: Rc::new(Cell::new(None)),
            submenu_state: SubmenuState::Closed,
            hover_target: HoverTarget::MainMenu,
            submenu_safety_threshold_x: None,
            submenu_trigger_bounds: Rc::new(Cell::new(None)),
            submenu_trigger_mouse_down: false,
            ignore_blur_until: None,
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
        link_text: impl Into<SharedString>,
        link_url: impl Into<SharedString>,
    ) -> Self {
        self.items.push(ContextMenuItem::HeaderWithLink(
            title.into(),
            link_text.into(),
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
        text: impl Into<SharedString>,
        action: Option<Box<dyn Action>>,
        handler: impl Fn(&mut Window, &mut App) + 'static,
    ) -> Self {
        self.items.push(ContextMenuItem::Entry(ContextMenuEntry {
            toggle: None,
            text: text.into(),
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
        text: impl Into<SharedString>,
        action: Option<Box<dyn Action>>,
        handler: impl Fn(&mut Window, &mut App) + 'static,
        end_slot_icon: IconAsset,
        end_slot_title: SharedString,
        end_slot_handler: impl Fn(&mut Window, &mut App) + 'static,
    ) -> Self {
        self.items.push(ContextMenuItem::Entry(ContextMenuEntry {
            toggle: None,
            text: text.into(),
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
        text: impl Into<SharedString>,
        action: Option<Box<dyn Action>>,
        handler: impl Fn(&mut Window, &mut App) + 'static,
        end_slot_icon: IconAsset,
        end_slot_title: SharedString,
        end_slot_handler: impl Fn(&mut Window, &mut App) + 'static,
    ) -> Self {
        self.items.push(ContextMenuItem::Entry(ContextMenuEntry {
            toggle: None,
            text: text.into(),
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
        text: impl Into<SharedString>,
        toggled: bool,
        position: IconPosition,
        action: Option<Box<dyn Action>>,
        handler: impl Fn(&mut Window, &mut App) + 'static,
    ) -> Self {
        self.items.push(ContextMenuItem::Entry(ContextMenuEntry {
            toggle: Some((position, toggled)),
            text: text.into(),
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

    pub fn text(mut self, text: impl Into<SharedString>) -> Self {
        self.items.push(ContextMenuItem::Text(text.into()));
        self
    }

    pub fn action(self, text: impl Into<SharedString>, action: Box<dyn Action>) -> Self {
        self.action_checked(text, action, false)
    }

    pub fn action_checked(
        mut self,
        text: impl Into<SharedString>,
        action: Box<dyn Action>,
        checked: bool,
    ) -> Self {
        self.items.push(ContextMenuItem::Entry(ContextMenuEntry {
            toggle: if checked {
                Some((IconPosition::Start, true))
            } else {
                None
            },
            text: text.into(),
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
        text: impl Into<SharedString>,
        action: Box<dyn Action>,
    ) -> Self {
        self.items.push(ContextMenuItem::Entry(ContextMenuEntry {
            toggle: None,
            text: text.into(),
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

    pub fn link(self, text: impl Into<SharedString>, action: Box<dyn Action>) -> Self {
        self.link_with_handler(text, action, |_, _| {})
    }

    pub fn link_with_handler(
        mut self,
        text: impl Into<SharedString>,
        action: Box<dyn Action>,
        handler: impl Fn(&mut Window, &mut App) + 'static,
    ) -> Self {
        self.items.push(ContextMenuItem::Entry(ContextMenuEntry {
            toggle: None,
            text: text.into(),
            action: Some(action.boxed_clone()),
            handler: Rc::new(move |_, window, cx| {
                handler(window, cx);
                window.dispatch_action(action.boxed_clone(), cx);
            }),
            secondary_handler: None,
            icon: Some(IconAsset::ArrowUpRight),
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

    pub fn submenu(
        mut self,
        label: impl Into<SharedString>,
        builder: impl Fn(ContextMenu, &mut Window, &mut Context<ContextMenu>) -> ContextMenu + 'static,
    ) -> Self {
        self.items.push(ContextMenuItem::Submenu {
            label: label.into(),
            icon: None,
            icon_color: None,
            builder: Rc::new(builder),
        });
        self
    }

    pub fn submenu_with_icon(
        mut self,
        label: impl Into<SharedString>,
        icon: IconAsset,
        builder: impl Fn(ContextMenu, &mut Window, &mut Context<ContextMenu>) -> ContextMenu + 'static,
    ) -> Self {
        self.items.push(ContextMenuItem::Submenu {
            label: label.into(),
            icon: Some(icon),
            icon_color: None,
            builder: Rc::new(builder),
        });
        self
    }

    pub fn submenu_with_colored_icon(
        mut self,
        label: impl Into<SharedString>,
        icon: IconAsset,
        icon_color: Color,
        builder: impl Fn(ContextMenu, &mut Window, &mut Context<ContextMenu>) -> ContextMenu + 'static,
    ) -> Self {
        self.items.push(ContextMenuItem::Submenu {
            label: label.into(),
            icon: Some(icon),
            icon_color: Some(icon_color),
            builder: Rc::new(builder),
        });
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

    pub fn confirm(
        &mut self,
        _: &actions::menu::Confirm,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(idx) = self.selected_index else {
            return;
        };

        if let Some(ContextMenuItem::Submenu { builder, .. }) = self.items.get(idx) {
            self.open_submenu(
                idx,
                builder.clone(),
                SubmenuOpenTrigger::Keyboard,
                window,
                cx,
            );

            if let SubmenuState::Open(open_submenu) = &self.submenu_state {
                let focus_handle = open_submenu.entity.read(cx).focus_handle.clone();
                window.focus(&focus_handle, cx);
                open_submenu.entity.update(cx, |submenu, cx| {
                    submenu.select_first(&actions::menu::SelectFirst, window, cx);
                });
            }

            cx.notify();
            return;
        }

        let context = self.action_context.as_ref();

        if let Some(ContextMenuItem::Entry(ContextMenuEntry {
            handler,
            disabled: false,
            ..
        })) = self.items.get(idx)
        {
            (handler)(context, window, cx);
        }

        if self.main_menu.is_some() && !self.keep_open_on_confirm {
            self.clicked = true;
        }

        if self.keep_open_on_confirm {
            self.rebuild(window, cx);
        } else {
            cx.emit(DismissEvent);
        }
    }

    pub fn secondary_confirm(
        &mut self,
        _: &actions::menu::SecondaryConfirm,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(idx) = self.selected_index else {
            return;
        };

        if let Some(ContextMenuItem::Submenu { builder, .. }) = self.items.get(idx) {
            self.open_submenu(
                idx,
                builder.clone(),
                SubmenuOpenTrigger::Keyboard,
                window,
                cx,
            );

            if let SubmenuState::Open(open_submenu) = &self.submenu_state {
                let focus_handle = open_submenu.entity.read(cx).focus_handle.clone();
                window.focus(&focus_handle, cx);
                open_submenu.entity.update(cx, |submenu, cx| {
                    submenu.select_first(&actions::menu::SelectFirst, window, cx);
                });
            }

            cx.notify();
            return;
        }

        let context = self.action_context.as_ref();

        if let Some(ContextMenuItem::Entry(ContextMenuEntry {
            handler,
            secondary_handler,
            disabled: false,
            ..
        })) = self.items.get(idx)
        {
            if let Some(secondary) = secondary_handler {
                (secondary)(context, window, cx);
            } else {
                (handler)(context, window, cx);
            }
        }

        if self.main_menu.is_some() && !self.keep_open_on_confirm {
            self.clicked = true;
        }

        if self.keep_open_on_confirm {
            self.rebuild(window, cx);
        } else {
            cx.emit(DismissEvent);
        }
    }

    pub fn cancel(
        &mut self,
        _: &actions::menu::Cancel,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(parent) = &self.main_menu {
            cx.emit(DismissEvent);

            // Return focus to the parent so keyboard navigation works after the submenu closes.
            let parent_focus = parent.read(cx).focus_handle.clone();
            parent.update(cx, |parent, _| {
                parent.ignore_blur_until = Some(Instant::now() + Duration::from_millis(200));
            });
            window.focus(&parent_focus, cx);
            return;
        }

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

    pub fn select_first(
        &mut self,
        _: &actions::menu::SelectFirst,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
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

    fn handle_select_last(
        &mut self,
        _: &actions::menu::SelectLast,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.select_last(window, cx).is_some() {
            cx.notify();
        }
    }

    pub fn select_next(
        &mut self,
        _: &actions::menu::SelectNext,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(idx) = self.selected_index {
            let next_index = idx + 1;
            if self.items.len() <= next_index {
                self.select_first(&actions::menu::SelectFirst, window, cx);
                return;
            }

            for (idx, item) in self.items.iter().enumerate().skip(next_index) {
                if item.is_selectable() {
                    self.select_index(idx, window, cx);
                    cx.notify();
                    return;
                }
            }
        }
        self.select_first(&actions::menu::SelectFirst, window, cx);
    }

    pub fn select_previous(
        &mut self,
        _: &actions::menu::SelectPrevious,
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
        self.handle_select_last(&actions::menu::SelectLast, window, cx);
    }

    pub fn select_submenu_child(
        &mut self,
        _: &actions::menu::SelectChild,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(idx) = self.selected_index else {
            return;
        };
        let Some(ContextMenuItem::Submenu { builder, .. }) = self.items.get(idx) else {
            return;
        };

        self.open_submenu(
            idx,
            builder.clone(),
            SubmenuOpenTrigger::Keyboard,
            window,
            cx,
        );

        if let SubmenuState::Open(open_submenu) = &self.submenu_state {
            let focus_handle = open_submenu.entity.read(cx).focus_handle.clone();
            window.focus(&focus_handle, cx);
            open_submenu.entity.update(cx, |submenu, cx| {
                submenu.select_first(&actions::menu::SelectFirst, window, cx);
            });
        }

        cx.notify();
    }

    pub fn select_submenu_parent(
        &mut self,
        _: &actions::menu::SelectParent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(parent) = self.main_menu.clone() else {
            return;
        };

        let parent_focus = parent.read(cx).focus_handle.clone();
        window.focus(&parent_focus, cx);

        cx.emit(DismissEvent);

        parent.update(cx, |parent, cx| {
            if let SubmenuState::Open(open_submenu) = &parent.submenu_state {
                let trigger_index = open_submenu.item_index;
                parent.close_submenu(false, cx);
                parent.select_index(trigger_index, window, cx);
                cx.notify();
            }
        });
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

    fn create_submenu(
        builder: Rc<dyn Fn(ContextMenu, &mut Window, &mut Context<ContextMenu>) -> ContextMenu>,
        parent_entity: Entity<ContextMenu>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> (Entity<ContextMenu>, Subscription) {
        let submenu = Self::build_submenu(builder, parent_entity, window, cx);

        let dismiss_subscription = cx.subscribe(&submenu, |this, submenu, _: &DismissEvent, cx| {
            let should_dismiss_parent = submenu.read(cx).clicked;

            this.close_submenu(false, cx);

            if should_dismiss_parent {
                cx.emit(DismissEvent);
            }
        });

        (submenu, dismiss_subscription)
    }

    fn build_submenu(
        builder: Rc<dyn Fn(ContextMenu, &mut Window, &mut Context<ContextMenu>) -> ContextMenu>,
        parent_entity: Entity<ContextMenu>,
        window: &mut Window,
        cx: &mut App,
    ) -> Entity<ContextMenu> {
        cx.new(move |cx| {
            let focus_handle = cx.focus_handle();
            let on_blur_subscription = cx.on_blur(&focus_handle, window, |_, _, _| {});

            let menu = ContextMenu {
                builder: None,
                items: Vec::default(),
                focus_handle,
                action_context: None,
                selected_index: None,
                delayed: false,
                clicked: false,
                end_slot_action: None,
                key_context: "menu".into(),
                on_blur_subscription,
                keep_open_on_confirm: false,
                fixed_width: None,
                main_menu: Some(parent_entity),
                main_menu_observed_bounds: Rc::new(Cell::new(None)),
                submenu_state: SubmenuState::Closed,
                hover_target: HoverTarget::MainMenu,
                submenu_safety_threshold_x: None,
                submenu_trigger_bounds: Rc::new(Cell::new(None)),
                submenu_trigger_mouse_down: false,
                ignore_blur_until: None,
            };

            (builder)(menu, window, cx)
        })
    }

    fn close_submenu(&mut self, clear_selection: bool, cx: &mut Context<Self>) {
        self.submenu_state = SubmenuState::Closed;
        self.hover_target = HoverTarget::MainMenu;
        self.submenu_safety_threshold_x = None;
        self.main_menu_observed_bounds.set(None);
        self.submenu_trigger_bounds.set(None);

        if clear_selection {
            self.selected_index = None;
        }

        cx.notify();
    }

    fn open_submenu(
        &mut self,
        item_index: usize,
        builder: Rc<dyn Fn(ContextMenu, &mut Window, &mut Context<ContextMenu>) -> ContextMenu>,
        trigger: SubmenuOpenTrigger,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if matches!(
            &self.submenu_state,
            SubmenuState::Open(open_submenu) if open_submenu.item_index == item_index
        ) {
            return;
        }

        let (submenu, dismiss_subscription) =
            Self::create_submenu(builder, cx.entity(), window, cx);

        let flip_left = self
            .main_menu_observed_bounds
            .get()
            .is_some_and(|bounds| bounds.right() + gpui::px(200.0) > window.viewport_size().width);

        self.main_menu_observed_bounds.set(None);
        self.submenu_trigger_bounds.set(None);

        self.submenu_safety_threshold_x = None;
        self.hover_target = HoverTarget::MainMenu;

        // Opening with the keyboard briefly moves focus in a way that would otherwise blur and
        // dismiss this menu.
        if matches!(trigger, SubmenuOpenTrigger::Keyboard) {
            self.ignore_blur_until = Some(Instant::now() + Duration::from_millis(150));
        }

        let trigger_bounds = self.submenu_trigger_bounds.get();

        self.submenu_state = SubmenuState::Open(OpenSubmenu {
            item_index,
            entity: submenu,
            trigger_bounds,
            offset: None,
            flip_left,
            _dismiss_subscription: dismiss_subscription,
        });

        cx.notify();
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
                        this.cancel(&actions::menu::Cancel, window, cx);
                        window.dispatch_action(action, cx);
                    })
                })
            })
            .detach_and_log_err(cx);
        } else {
            cx.propagate();
        }
    }

    pub fn on_blur_subscription(mut self, new_subscription: Subscription) -> Self {
        self.on_blur_subscription = new_subscription;
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
            ContextMenuItem::HeaderWithLink(header, text, url) => {
                let url = url.clone();
                let link_id = ElementId::Name(format!("link-{url}").into());
                ListSubHeader::new(header.clone())
                    .inset(true)
                    .end_slot(
                        Button::new(link_id, text.clone())
                            .color(Color::Muted)
                            .text_size(TextSize::Small)
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
            ContextMenuItem::Text(text) => ListItem::new(idx)
                .inset(true)
                .disabled(true)
                .child(Text::new(text.clone()))
                .into_any_element(),
            ContextMenuItem::Entry(entry) => {
                self.render_menu_entry(idx, entry, cx).into_any_element()
            }
            ContextMenuItem::Submenu {
                label,
                icon,
                icon_color,
                ..
            } => self
                .render_submenu_item_trigger(idx, label.clone(), *icon, *icon_color, cx)
                .into_any_element(),
        }
    }

    fn render_submenu_item_trigger(
        &self,
        idx: usize,
        label: SharedString,
        icon: Option<IconAsset>,
        icon_color: Option<Color>,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let toggle_state = Some(idx) == self.selected_index
            || matches!(
                &self.submenu_state,
                SubmenuState::Open(open_submenu) if open_submenu.item_index == idx
            );

        gpui::div()
            .id(("context-menu-submenu-trigger", idx))
            .capture_any_mouse_down(cx.listener(move |this, event: &MouseDownEvent, _, _| {
                // Keeps `on_hover(false)` from closing the submenu while it's being clicked.
                if event.button == MouseButton::Left {
                    this.submenu_trigger_mouse_down = true;
                }
            }))
            .capture_any_mouse_up(cx.listener(move |this, event: &MouseUpEvent, _, _| {
                if event.button == MouseButton::Left {
                    this.submenu_trigger_mouse_down = false;
                }
            }))
            .on_mouse_move(cx.listener(move |this, event: &MouseMoveEvent, _, cx| {
                if matches!(&this.submenu_state, SubmenuState::Open(_))
                    || this.selected_index == Some(idx)
                {
                    this.submenu_safety_threshold_x = Some(event.position.x - gpui::px(100.0));
                }

                cx.notify();
            }))
            .child(
                ListItem::new(idx)
                    .inset(true)
                    .toggle_state(toggle_state)
                    .child(
                        gpui::canvas(
                            {
                                let trigger_bounds_cell = self.submenu_trigger_bounds.clone();
                                move |bounds, _, _| {
                                    if toggle_state {
                                        trigger_bounds_cell.set(Some(bounds));
                                    }
                                }
                            },
                            |_, (), _, _| {},
                        )
                        .size_full()
                        .absolute()
                        .top_0()
                        .left_0(),
                    )
                    .on_hover(cx.listener(move |this, hovered, window, cx| {
                        let mouse_position = window.mouse_position();

                        if *hovered {
                            this.clear_selected();
                            window.focus(&this.focus_handle.clone(), cx);
                            this.hover_target = HoverTarget::MainMenu;
                            this.submenu_safety_threshold_x =
                                Some(mouse_position.x - gpui::px(50.0));

                            if let Some(ContextMenuItem::Submenu { builder, .. }) =
                                this.items.get(idx)
                            {
                                this.open_submenu(
                                    idx,
                                    builder.clone(),
                                    SubmenuOpenTrigger::Pointer,
                                    window,
                                    cx,
                                );
                            }

                            cx.notify();
                        } else {
                            if this.submenu_trigger_mouse_down {
                                return;
                            }

                            let is_open_for_this_item = matches!(
                                &this.submenu_state,
                                SubmenuState::Open(open_submenu) if open_submenu.item_index == idx
                            );
                            let mouse_in_submenu_zone = this
                                .padded_submenu_bounds()
                                .is_some_and(|bounds| bounds.contains(&mouse_position));

                            if is_open_for_this_item
                                && this.hover_target != HoverTarget::Submenu
                                && !mouse_in_submenu_zone
                            {
                                this.close_submenu(false, cx);
                                this.clear_selected();
                                window.focus(&this.focus_handle.clone(), cx);
                                cx.notify();
                            }
                        }
                    }))
                    .on_click(cx.listener(move |this, _, window, cx| {
                        if matches!(
                            &this.submenu_state,
                            SubmenuState::Open(open_submenu) if open_submenu.item_index == idx
                        ) {
                            return;
                        }

                        if let Some(ContextMenuItem::Submenu { builder, .. }) = this.items.get(idx)
                        {
                            this.open_submenu(
                                idx,
                                builder.clone(),
                                SubmenuOpenTrigger::Pointer,
                                window,
                                cx,
                            );
                        }
                    }))
                    .child(
                        gpui::div()
                            .flex()
                            .items_center()
                            .w_full()
                            .gap_2()
                            .justify_between()
                            .child(
                                gpui::div()
                                    .flex()
                                    .items_center()
                                    .gap_1p5()
                                    .when_some(icon, |this, icon| {
                                        this.child(
                                            Icon::new(icon)
                                                .size(IconSize::Small)
                                                .color(icon_color.unwrap_or(Color::Muted)),
                                        )
                                    })
                                    .child(Text::new(label).color(Color::Default)),
                            )
                            .child(
                                Icon::new(IconAsset::CaretRight)
                                    .size(IconSize::Small)
                                    .color(Color::Muted),
                            ),
                    ),
            )
    }

    fn padded_submenu_bounds(&self) -> Option<Bounds<Pixels>> {
        let bounds = self.main_menu_observed_bounds.get()?;
        Some(Bounds {
            origin: Point {
                x: bounds.origin.x - gpui::px(50.0),
                y: bounds.origin.y - gpui::px(50.0),
            },
            size: Size {
                width: bounds.size.width + gpui::px(100.0),
                height: bounds.size.height + gpui::px(100.0),
            },
        })
    }

    fn render_submenu_container(
        &self,
        idx: usize,
        submenu: Entity<ContextMenu>,
        offset: Pixels,
        flip_left: bool,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let bounds_cell = self.main_menu_observed_bounds.clone();
        let canvas = gpui::canvas(
            move |bounds, _, _| {
                bounds_cell.set(Some(bounds));
            },
            |_, (), _, _| {},
        )
        .size_full()
        .absolute()
        .top_0()
        .left_0();

        gpui::div()
            .id(("submenu-container", idx))
            .absolute()
            .top(offset)
            .when(flip_left, |this| this.right_full().mr_neg_0p5())
            .when(!flip_left, |this| this.left_full().ml_neg_0p5())
            .on_hover(cx.listener(|this, hovered, _, _| {
                if *hovered {
                    this.hover_target = HoverTarget::Submenu;
                }
            }))
            .child(
                gpui::anchored()
                    .anchor(if flip_left {
                        Anchor::TopRight
                    } else {
                        Anchor::TopLeft
                    })
                    .snap_to_window_with_margin(gpui::px(8.0))
                    .child(
                        gpui::div()
                            .id(("submenu-hover-zone", idx))
                            .occlude()
                            .child(canvas)
                            .child(submenu),
                    ),
            )
    }

    fn render_menu_entry(
        &self,
        idx: usize,
        entry: &ContextMenuEntry,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let ContextMenuEntry {
            toggle,
            text,
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

        let text_color = if *disabled {
            Color::Disabled
        } else {
            Color::Default
        };

        let content = if let Some(icon_asset) = icon {
            gpui::div()
                .flex()
                .items_center()
                .gap_1p5()
                .when(
                    *icon_position == IconPosition::Start && toggle.is_none(),
                    |flex| flex.child(Icon::new(*icon_asset).size(*icon_size).color(icon_color)),
                )
                .child(Text::new(text.clone()).color(text_color).truncate())
                .when(*icon_position == IconPosition::End, |flex| {
                    flex.child(Icon::new(*icon_asset).size(*icon_size).color(icon_color))
                })
                .into_any_element()
        } else {
            Text::new(text.clone())
                .color(text_color)
                .truncate()
                .into_any_element()
        };

        gpui::div()
            .id(("context-menu-child", idx))
            .child(
                ListItem::new(idx)
                    .group_name("text-container")
                    .inset(true)
                    .disabled(*disabled)
                    .toggle_state(Some(idx) == self.selected_index)
                    .when(self.main_menu.is_none() && !*disabled, |item| {
                        item.on_hover(cx.listener(move |this, hovered, window, cx| {
                            if *hovered {
                                this.clear_selected();
                                window.focus(&this.focus_handle.clone(), cx);

                                if let SubmenuState::Open(open_submenu) = &this.submenu_state
                                    && open_submenu.item_index != idx
                                {
                                    this.close_submenu(false, cx);
                                    cx.notify();
                                }
                            }
                        }))
                    })
                    .when(self.main_menu.is_some(), |item| {
                        item.on_hover(cx.listener(move |this, hovered, window, cx| {
                            if *hovered {
                                this.clear_selected();
                                cx.notify();
                            }

                            if let Some(parent) = &this.main_menu {
                                let mouse_position = window.mouse_position();
                                parent.update(cx, |parent, cx| {
                                    if *hovered {
                                        parent.clear_selected();
                                        parent.hover_target = HoverTarget::Submenu;
                                    } else if matches!(&parent.submenu_state, SubmenuState::Open(_))
                                    {
                                        // Only close the submenu once the mouse is to the left of
                                        // the safety threshold, not on every item exit.
                                        let should_close = parent
                                            .submenu_safety_threshold_x
                                            .is_none_or(|threshold_x| {
                                                mouse_position.x < threshold_x
                                            });

                                        if should_close {
                                            parent.close_submenu(true, cx);
                                        }
                                    }
                                });
                            }
                        }))
                    })
                    .when_some(*toggle, |list_item, (position, toggled)| {
                        let contents = gpui::div()
                            .flex_none()
                            .child(
                                Icon::new(icon.unwrap_or(IconAsset::Check))
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
                        gpui::div()
                            .flex()
                            .items_center()
                            .w_full()
                            .justify_between()
                            .child(content)
                            .debug_selector(|| format!("MENU_ITEM-{text}"))
                            .children(action.as_ref().map(|action| {
                                let binding = self.action_context.as_ref().map_or_else(
                                    || KeyBinding::for_action(action.as_ref(), cx),
                                    |focus| KeyBinding::for_action_in(action.as_ref(), focus, cx),
                                );

                                gpui::div().ml_4().child(binding.disabled(*disabled))
                            })),
                    )
                    .when_some(
                        end_slot_icon
                            .as_ref()
                            .zip(self.end_slot_action.as_ref())
                            .zip(end_slot_title.as_ref())
                            .zip(end_slot_handler.as_ref()),
                        |element, (((icon, action), title), handler)| {
                            element.end_slot({
                                let icon_button = IconButton::new("end-slot-icon", *icon)
                                    .shape(IconButtonShape::Square)
                                    .tooltip({
                                        let action_context = self.action_context.clone();
                                        let title = title.clone();
                                        let action = action.boxed_clone();
                                        move |_window, cx| {
                                            if let Some(focus) = action_context.as_ref() {
                                                Tooltip::for_action_in(
                                                    title.clone(),
                                                    &*action,
                                                    focus,
                                                    cx,
                                                )
                                            } else {
                                                Tooltip::for_action(title.clone(), &*action, cx)
                                            }
                                        }
                                    })
                                    .on_click({
                                        let handler = handler.clone();
                                        move |_, window, cx| {
                                            handler(None, window, cx);
                                            if let Err(error) = this.update(cx, |this, cx| {
                                                this.rebuild(window, cx);
                                                cx.notify();
                                            }) {
                                                log::trace!(
                                                    "Failed to rebuild context menu after slot click: {error:?}"
                                                );
                                            }
                                        }
                                    });

                                if *show_end_slot_on_hover {
                                    gpui::div()
                                        .visible_on_hover("text-container")
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
                            if let Err(error) = menu.update(cx, |menu, cx| {
                                menu.clicked = true;
                                if keep_open_on_confirm {
                                    menu.rebuild(window, cx);
                                } else {
                                    cx.emit(DismissEvent);
                                }
                            }) {
                                log::trace!(
                                    "Failed to update context menu after item click: {error:?}"
                                );
                            }
                        }
                    }),
            )
            .into_any_element()
    }
}

impl Focusable for ContextMenu {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl EventEmitter<DismissEvent> for ContextMenu {}

impl FluentBuilder for ContextMenu {}

impl Render for ContextMenu {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let ui_font_size = ThemeSettings::get_global(cx).ui_font_size(cx);

        let mut focus_submenu = None;
        let submenu_container = match &mut self.submenu_state {
            SubmenuState::Open(open_submenu) => {
                let computed_offset = if open_submenu.offset.is_none() {
                    let menu_bounds = self.main_menu_observed_bounds.get();
                    let trigger_bounds = open_submenu
                        .trigger_bounds
                        .or_else(|| self.submenu_trigger_bounds.get());

                    match (menu_bounds, trigger_bounds) {
                        (Some(menu_bounds), Some(trigger_bounds)) => {
                            Some(trigger_bounds.origin.y - menu_bounds.origin.y)
                        }
                        _ => None,
                    }
                } else {
                    None
                };

                if let Some(offset) = open_submenu.offset.or(computed_offset) {
                    if open_submenu.offset.is_none() {
                        open_submenu.offset = Some(offset);
                    }

                    focus_submenu = Some(open_submenu.entity.read(cx).focus_handle.clone());
                    Some((
                        open_submenu.item_index,
                        open_submenu.entity.clone(),
                        offset,
                        open_submenu.flip_left,
                    ))
                } else {
                    None
                }
            }
            SubmenuState::Closed => None,
        };

        let bounds_cell = self.main_menu_observed_bounds.clone();
        let menu_bounds_measure = gpui::canvas(
            move |bounds, _, _| {
                bounds_cell.set(Some(bounds));
            },
            |_, (), _, _| {},
        )
        .size_full()
        .absolute()
        .top_0()
        .left_0();

        let menu = WithRemSize::new(ui_font_size)
            .occlude()
            .elevation_2(cx)
            .flex()
            .flex_shrink_0()
            .child(
                gpui::div()
                    .id("context-menu")
                    .flex()
                    .flex_col()
                    .max_h(crate::vh(0.75, window))
                    .flex_shrink_0()
                    .child(menu_bounds_measure)
                    .when_some(self.fixed_width, |this, width| {
                        this.w(width).overflow_x_hidden()
                    })
                    .when(self.fixed_width.is_none(), |this| {
                        this.min_w(gpui::px(150.0)).flex_1()
                    })
                    .overflow_y_scroll()
                    .track_focus(&self.focus_handle(cx))
                    .key_context(self.key_context.as_ref())
                    .on_action(cx.listener(ContextMenu::select_first))
                    .on_action(cx.listener(ContextMenu::handle_select_last))
                    .on_action(cx.listener(ContextMenu::select_next))
                    .on_action(cx.listener(ContextMenu::select_previous))
                    .on_action(cx.listener(ContextMenu::select_submenu_child))
                    .on_action(cx.listener(ContextMenu::select_submenu_parent))
                    .on_action(cx.listener(ContextMenu::confirm))
                    .on_action(cx.listener(ContextMenu::secondary_confirm))
                    .on_action(cx.listener(ContextMenu::cancel))
                    .on_hover(cx.listener(|this, hovered: &bool, _, cx| {
                        if *hovered {
                            this.hover_target = HoverTarget::MainMenu;
                            if let Some(parent) = &this.main_menu {
                                parent.update(cx, |parent, _| {
                                    parent.hover_target = HoverTarget::Submenu;
                                });
                            }
                        }
                    }))
                    .on_mouse_down_out(cx.listener(|this, event: &MouseDownEvent, window, cx| {
                        if matches!(&this.submenu_state, SubmenuState::Open(_))
                            && this
                                .padded_submenu_bounds()
                                .is_some_and(|bounds| bounds.contains(&event.position))
                        {
                            return;
                        }

                        if let Some(parent) = &this.main_menu
                            && parent
                                .read(cx)
                                .submenu_trigger_bounds
                                .get()
                                .is_some_and(|bounds| bounds.contains(&event.position))
                        {
                            return;
                        }

                        this.cancel(&actions::menu::Cancel, window, cx);
                    }))
                    .when_some(self.end_slot_action.as_ref(), |element, action| {
                        element.on_boxed_action(action.as_ref(), cx.listener(ContextMenu::end_slot))
                    })
                    .when(!self.delayed, |mut element| {
                        for item in &self.items {
                            if let ContextMenuItem::Entry(ContextMenuEntry {
                                action: Some(action),
                                disabled: false,
                                ..
                            }) = item
                            {
                                element = element.on_boxed_action(
                                    action.as_ref(),
                                    cx.listener(ContextMenu::on_action_dispatch),
                                );
                            }
                        }
                        element
                    })
                    .child(
                        List::new().children(
                            self.items
                                .iter()
                                .enumerate()
                                .map(|(idx, item)| self.render_menu_item(idx, item, window, cx)),
                        ),
                    ),
            );

        if let Some(focus_handle) = focus_submenu.as_ref() {
            window.focus(focus_handle, cx);
        }

        gpui::div().relative().child(menu).when_some(
            submenu_container,
            |this, (idx, submenu, offset, flip_left)| {
                this.child(self.render_submenu_container(idx, submenu, offset, flip_left, cx))
            },
        )
    }
}
