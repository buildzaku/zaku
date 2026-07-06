use gpui::{
    Anchor, AnyElement, AnyView, App, ElementId, Entity, Pixels, Point, SharedString, Window,
    prelude::*,
};

use super::{
    PopoverMenuHandle,
    button::{ButtonLike, ButtonSize, ButtonVariant},
};

use crate::{
    Button, ButtonCommon, Color, ContextMenu, Disableable, FixedWidth, Icon, IconAsset,
    IconPosition, IconSize, PopoverMenu,
};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub enum DropdownVariant {
    #[default]
    Solid,
    Outlined,
    OutlinedGhost,
    Subtle,
    Ghost,
}

enum DropdownTitle {
    Text(SharedString),
    Element(AnyElement),
}

#[derive(IntoElement)]
pub struct DropdownMenu {
    id: ElementId,
    title: DropdownTitle,
    trigger_size: ButtonSize,
    trigger_tooltip: Option<Box<dyn Fn(&mut Window, &mut App) -> AnyView + 'static>>,
    trigger_icon: Option<IconAsset>,
    variant: DropdownVariant,
    menu: Entity<ContextMenu>,
    full_width: bool,
    disabled: bool,
    handle: Option<PopoverMenuHandle<ContextMenu>>,
    attach: Option<Anchor>,
    offset: Option<Point<Pixels>>,
    tab_index: Option<isize>,
    caret: bool,
}

impl DropdownMenu {
    pub fn new(
        id: impl Into<ElementId>,
        title: impl Into<SharedString>,
        menu: Entity<ContextMenu>,
    ) -> Self {
        Self {
            id: id.into(),
            title: DropdownTitle::Text(title.into()),
            trigger_size: ButtonSize::Default,
            trigger_tooltip: None,
            trigger_icon: Some(IconAsset::CaretUpDown),
            variant: DropdownVariant::default(),
            menu,
            full_width: false,
            disabled: false,
            handle: None,
            attach: None,
            offset: None,
            tab_index: None,
            caret: true,
        }
    }

    pub fn new_with_element(
        id: impl Into<ElementId>,
        title: AnyElement,
        menu: Entity<ContextMenu>,
    ) -> Self {
        Self {
            id: id.into(),
            title: DropdownTitle::Element(title),
            trigger_size: ButtonSize::Default,
            trigger_tooltip: None,
            trigger_icon: Some(IconAsset::CaretUpDown),
            variant: DropdownVariant::default(),
            menu,
            full_width: false,
            disabled: false,
            handle: None,
            attach: None,
            offset: None,
            tab_index: None,
            caret: true,
        }
    }

    pub fn variant(mut self, variant: DropdownVariant) -> Self {
        self.variant = variant;
        self
    }

    pub fn trigger_size(mut self, size: ButtonSize) -> Self {
        self.trigger_size = size;
        self
    }

    pub fn trigger_tooltip(
        mut self,
        tooltip: impl Fn(&mut Window, &mut App) -> AnyView + 'static,
    ) -> Self {
        self.trigger_tooltip = Some(Box::new(tooltip));
        self
    }

    pub fn trigger_icon(mut self, icon: IconAsset) -> Self {
        self.trigger_icon = Some(icon);
        self
    }

    pub fn full_width(mut self, full_width: bool) -> Self {
        self.full_width = full_width;
        self
    }

    pub fn handle(mut self, handle: PopoverMenuHandle<ContextMenu>) -> Self {
        self.handle = Some(handle);
        self
    }

    pub fn attach(mut self, attach: Anchor) -> Self {
        self.attach = Some(attach);
        self
    }

    pub fn offset(mut self, offset: Point<Pixels>) -> Self {
        self.offset = Some(offset);
        self
    }

    pub fn tab_index(mut self, arg: isize) -> Self {
        self.tab_index = Some(arg);
        self
    }

    pub fn no_caret(mut self) -> Self {
        self.caret = false;
        self
    }
}

impl Disableable for DropdownMenu {
    fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }
}

impl RenderOnce for DropdownMenu {
    fn render(self, _window: &mut Window, _cx: &mut App) -> impl IntoElement {
        let button_variant = match self.variant {
            DropdownVariant::Solid => ButtonVariant::Solid,
            DropdownVariant::Subtle => ButtonVariant::Subtle,
            DropdownVariant::Outlined => ButtonVariant::Outline,
            DropdownVariant::OutlinedGhost => ButtonVariant::OutlinedGhost,
            DropdownVariant::Ghost => ButtonVariant::Ghost,
        };

        let full_width = self.full_width;
        let trigger_size = self.trigger_size;

        let (title_button, element_button) = match self.title {
            DropdownTitle::Text(title) => (
                Some(
                    Button::new(self.id.clone(), title)
                        .variant(button_variant)
                        .when(self.caret, |this| {
                            this.icon(self.trigger_icon)
                                .icon_position(IconPosition::End)
                                .icon_size(IconSize::XSmall)
                                .icon_color(Color::Muted)
                        })
                        .when(full_width, |this| this.full_width())
                        .size(trigger_size)
                        .disabled(self.disabled)
                        .when_some(self.tab_index, |this, tab_index| this.tab_index(tab_index)),
                ),
                None,
            ),
            DropdownTitle::Element(element) => (
                None,
                Some(
                    ButtonLike::new(self.id.clone())
                        .child(element)
                        .variant(button_variant)
                        .when(self.caret, |this| {
                            this.child(
                                Icon::new(IconAsset::CaretUpDown)
                                    .size(IconSize::XSmall)
                                    .color(Color::Muted),
                            )
                        })
                        .when(full_width, |this| this.full_width())
                        .size(trigger_size)
                        .disabled(self.disabled)
                        .when_some(self.tab_index, |this, tab_index| this.tab_index(tab_index)),
                ),
            ),
        };

        let mut popover = PopoverMenu::new((self.id.clone(), "popover"))
            .full_width(self.full_width)
            .menu(move |_window, _cx| Some(self.menu.clone()));

        popover = match (title_button, element_button, self.trigger_tooltip) {
            (Some(title_button), None, Some(tooltip)) => {
                popover.trigger_with_tooltip(title_button, tooltip)
            }
            (Some(title_button), None, None) => popover.trigger(title_button),
            (None, Some(element_button), Some(tooltip)) => {
                popover.trigger_with_tooltip(element_button, tooltip)
            }
            (None, Some(element_button), None) => popover.trigger(element_button),
            _ => popover,
        };

        popover
            .attach(match self.attach {
                Some(attach) => attach,
                None => Anchor::BottomRight,
            })
            .when_some(self.offset, |this, offset| this.offset(offset))
            .when_some(self.handle, |this, handle| this.with_handle(handle))
    }
}
