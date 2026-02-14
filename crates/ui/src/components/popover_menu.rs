use gpui::{
    AnyElement, AnyView, App, Bounds, Corner, DismissEvent, DispatchPhase, ElementId, Entity,
    Focusable, GlobalElementId, HitboxBehavior, HitboxId, InspectorElementId, LayoutId, Length,
    ManagedView, MouseDownEvent, Pixels, Point, Style, Window, prelude::*,
};
use std::{cell::RefCell, rc::Rc};

use crate::prelude::*;

pub trait PopoverTrigger: IntoElement + Clickable + Toggleable + 'static {}

impl<T: IntoElement + Clickable + Toggleable + 'static> PopoverTrigger for T {}

pub struct PopoverMenuHandle<M>(Rc<RefCell<Option<PopoverMenuHandleState<M>>>>);

type MenuBuilder<M> = Rc<dyn Fn(&mut Window, &mut App) -> Option<Entity<M>> + 'static>;
type MenuState<M> = Rc<RefCell<Option<Entity<M>>>>;
type ChildBuilder<M> =
    Box<dyn FnOnce(MenuState<M>, Option<MenuBuilder<M>>) -> AnyElement + 'static>;

impl<M> Clone for PopoverMenuHandle<M> {
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }
}

impl<M> Default for PopoverMenuHandle<M> {
    fn default() -> Self {
        Self(Rc::default())
    }
}

struct PopoverMenuHandleState<M> {
    menu_builder: MenuBuilder<M>,
    menu: MenuState<M>,
    on_open: Option<Rc<dyn Fn(&mut Window, &mut App)>>,
}

impl<M: ManagedView> PopoverMenuHandle<M> {
    pub fn show(&self, window: &mut Window, cx: &mut App) {
        if let Some(state) = self.0.borrow().as_ref() {
            show_menu(
                &state.menu_builder,
                &state.menu,
                state.on_open.clone(),
                window,
                cx,
            );
        }
    }

    pub fn hide(&self, cx: &mut App) {
        if let Some(state) = self.0.borrow().as_ref()
            && let Some(menu) = state.menu.borrow().as_ref()
        {
            menu.update(cx, |_, cx| cx.emit(DismissEvent));
        }
    }

    pub fn toggle(&self, window: &mut Window, cx: &mut App) {
        if let Some(state) = self.0.borrow().as_ref() {
            if state.menu.borrow().is_some() {
                self.hide(cx);
            } else {
                self.show(window, cx);
            }
        }
    }

    pub fn is_deployed(&self) -> bool {
        self.0
            .borrow()
            .as_ref()
            .is_some_and(|state| state.menu.borrow().as_ref().is_some())
    }

    pub fn is_focused(&self, window: &Window, cx: &App) -> bool {
        self.0.borrow().as_ref().is_some_and(|state| {
            state
                .menu
                .borrow()
                .as_ref()
                .is_some_and(|model| model.focus_handle(cx).is_focused(window))
        })
    }

    pub fn refresh_menu(
        &self,
        window: &mut Window,
        cx: &mut App,
        new_menu_builder: MenuBuilder<M>,
    ) {
        let show_menu = if let Some(state) = self.0.borrow_mut().as_mut() {
            state.menu_builder = new_menu_builder;
            state.menu.borrow().is_some()
        } else {
            false
        };

        if show_menu {
            self.show(window, cx);
        }
    }
}

pub struct PopoverMenu<M: ManagedView> {
    id: ElementId,
    child_builder: Option<ChildBuilder<M>>,
    menu_builder: Option<MenuBuilder<M>>,
    anchor: Corner,
    attach: Option<Corner>,
    offset: Option<Point<Pixels>>,
    trigger_handle: Option<PopoverMenuHandle<M>>,
    on_open: Option<Rc<dyn Fn(&mut Window, &mut App)>>,
    full_width: bool,
}

impl<M: ManagedView> PopoverMenu<M> {
    pub fn new(id: impl Into<ElementId>) -> Self {
        Self {
            id: id.into(),
            child_builder: None,
            menu_builder: None,
            anchor: Corner::TopLeft,
            attach: None,
            offset: None,
            trigger_handle: None,
            on_open: None,
            full_width: false,
        }
    }

    pub fn full_width(mut self, full_width: bool) -> Self {
        self.full_width = full_width;
        self
    }

    pub fn menu(
        mut self,
        f: impl Fn(&mut Window, &mut App) -> Option<Entity<M>> + 'static,
    ) -> Self {
        self.menu_builder = Some(Rc::new(f));
        self
    }

    pub fn with_handle(mut self, handle: PopoverMenuHandle<M>) -> Self {
        self.trigger_handle = Some(handle);
        self
    }

    pub fn trigger<T: PopoverTrigger>(mut self, t: T) -> Self {
        let on_open = self.on_open.clone();
        self.child_builder = Some(Box::new(move |menu, builder| {
            let open = menu.borrow().is_some();
            t.toggle_state(open)
                .when_some(builder, |el, builder| {
                    el.on_click(move |_event, window, cx| {
                        show_menu(&builder, &menu, on_open.clone(), window, cx)
                    })
                })
                .into_any_element()
        }));
        self
    }

    pub fn trigger_with_tooltip<T: PopoverTrigger + ButtonCommon>(
        mut self,
        t: T,
        tooltip_builder: impl Fn(&mut Window, &mut App) -> AnyView + 'static,
    ) -> Self {
        let on_open = self.on_open.clone();
        self.child_builder = Some(Box::new(move |menu, builder| {
            let open = menu.borrow().is_some();
            t.toggle_state(open)
                .when_some(builder, |el, builder| {
                    el.on_click(move |_, window, cx| {
                        show_menu(&builder, &menu, on_open.clone(), window, cx)
                    })
                    .when(!open, |t| {
                        t.tooltip(move |window, cx| tooltip_builder(window, cx))
                    })
                })
                .into_any_element()
        }));
        self
    }

    pub fn anchor(mut self, anchor: Corner) -> Self {
        self.anchor = anchor;
        self
    }

    pub fn attach(mut self, attach: Corner) -> Self {
        self.attach = Some(attach);
        self
    }

    pub fn offset(mut self, offset: Point<Pixels>) -> Self {
        self.offset = Some(offset);
        self
    }

    pub fn on_open(mut self, on_open: Rc<dyn Fn(&mut Window, &mut App)>) -> Self {
        self.on_open = Some(on_open);
        self
    }

    fn resolved_attach(&self) -> Corner {
        self.attach.unwrap_or(match self.anchor {
            Corner::TopLeft => Corner::BottomLeft,
            Corner::TopRight => Corner::BottomRight,
            Corner::BottomLeft => Corner::TopLeft,
            Corner::BottomRight => Corner::TopRight,
        })
    }

    fn resolved_offset(&self, window: &mut Window) -> Point<Pixels> {
        self.offset.unwrap_or_else(|| {
            let offset = rems_from_px(5.) * window.rem_size();
            match self.anchor {
                Corner::TopRight | Corner::BottomRight => gpui::point(offset, gpui::px(0.)),
                Corner::TopLeft | Corner::BottomLeft => gpui::point(-offset, gpui::px(0.)),
            }
        })
    }
}

fn show_menu<M: ManagedView>(
    builder: &MenuBuilder<M>,
    menu: &MenuState<M>,
    on_open: Option<Rc<dyn Fn(&mut Window, &mut App)>>,
    window: &mut Window,
    cx: &mut App,
) {
    let previous_focus_handle = window.focused(cx);
    let new_menu = match builder(window, cx) {
        Some(menu) => menu,
        None => return,
    };

    let menu_clone = menu.clone();
    window
        .subscribe(&new_menu, cx, move |modal, _: &DismissEvent, window, cx| {
            if modal.focus_handle(cx).contains_focused(window, cx)
                && let Some(handle) = &previous_focus_handle
            {
                window.focus(handle, cx);
            }
            *menu_clone.borrow_mut() = None;
            window.refresh();
        })
        .detach();

    let focus_handle = new_menu.focus_handle(cx);
    window.on_next_frame(move |window, _cx| {
        window.on_next_frame(move |window, cx| {
            window.focus(&focus_handle, cx);
        });
    });

    *menu.borrow_mut() = Some(new_menu);
    window.refresh();

    if let Some(on_open) = on_open {
        on_open(window, cx);
    }
}

pub struct PopoverMenuElementState<M> {
    menu: MenuState<M>,
    child_bounds: Option<Bounds<Pixels>>,
}

impl<M> Clone for PopoverMenuElementState<M> {
    fn clone(&self) -> Self {
        Self {
            menu: Rc::clone(&self.menu),
            child_bounds: self.child_bounds,
        }
    }
}

impl<M> Default for PopoverMenuElementState<M> {
    fn default() -> Self {
        Self {
            menu: Rc::default(),
            child_bounds: None,
        }
    }
}

pub struct PopoverMenuFrameState<M: ManagedView> {
    child_layout_id: Option<LayoutId>,
    child_element: Option<AnyElement>,
    menu_element: Option<AnyElement>,
    menu_handle: MenuState<M>,
}

impl<M: ManagedView> Element for PopoverMenu<M> {
    type RequestLayoutState = PopoverMenuFrameState<M>;
    type PrepaintState = Option<HitboxId>;

    fn id(&self) -> Option<ElementId> {
        Some(self.id.clone())
    }

    fn source_location(&self) -> Option<&'static core::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        global_id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, Self::RequestLayoutState) {
        window.with_element_state(
            global_id.unwrap(),
            |element_state: Option<PopoverMenuElementState<M>>, window| {
                let element_state = element_state.unwrap_or_default();
                let mut menu_layout_id = None;

                let menu_element = element_state.menu.borrow_mut().as_mut().map(|menu| {
                    let offset = self.resolved_offset(window);
                    let mut anchored = gpui::anchored()
                        .snap_to_window_with_margin(gpui::px(8.))
                        .anchor(self.anchor)
                        .offset(offset);
                    if let Some(child_bounds) = element_state.child_bounds {
                        anchored =
                            anchored.position(child_bounds.corner(self.resolved_attach()) + offset);
                    }
                    let mut element =
                        gpui::deferred(anchored.child(gpui::div().occlude().child(menu.clone())))
                            .with_priority(1)
                            .into_any();

                    menu_layout_id = Some(element.request_layout(window, cx));
                    element
                });

                let mut child_element = self.child_builder.take().map(|child_builder| {
                    (child_builder)(element_state.menu.clone(), self.menu_builder.clone())
                });

                if let Some(trigger_handle) = self.trigger_handle.take()
                    && let Some(menu_builder) = self.menu_builder.clone()
                {
                    *trigger_handle.0.borrow_mut() = Some(PopoverMenuHandleState {
                        menu_builder,
                        menu: element_state.menu.clone(),
                        on_open: self.on_open.clone(),
                    });
                }

                let child_layout_id = child_element
                    .as_mut()
                    .map(|child_element| child_element.request_layout(window, cx));

                let mut style = Style::default();
                if self.full_width {
                    style.size = gpui::size(gpui::relative(1.).into(), Length::Auto);
                }

                let layout_id = window.request_layout(
                    style,
                    menu_layout_id.into_iter().chain(child_layout_id),
                    cx,
                );

                (
                    (
                        layout_id,
                        PopoverMenuFrameState {
                            child_element,
                            child_layout_id,
                            menu_element,
                            menu_handle: element_state.menu.clone(),
                        },
                    ),
                    element_state,
                )
            },
        )
    }

    fn prepaint(
        &mut self,
        global_id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        _bounds: Bounds<Pixels>,
        request_layout: &mut Self::RequestLayoutState,
        window: &mut Window,
        cx: &mut App,
    ) -> Option<HitboxId> {
        if let Some(child) = request_layout.child_element.as_mut() {
            child.prepaint(window, cx);
        }

        if let Some(menu) = request_layout.menu_element.as_mut() {
            menu.prepaint(window, cx);
        }

        request_layout.child_layout_id.map(|layout_id| {
            let bounds = window.layout_bounds(layout_id);
            window.with_element_state(global_id.unwrap(), |element_state, _cx| {
                let mut element_state: PopoverMenuElementState<M> = element_state.unwrap();
                element_state.child_bounds = Some(bounds);
                ((), element_state)
            });

            window.insert_hitbox(bounds, HitboxBehavior::Normal).id
        })
    }

    fn paint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        _: Bounds<Pixels>,
        request_layout: &mut Self::RequestLayoutState,
        child_hitbox: &mut Option<HitboxId>,
        window: &mut Window,
        cx: &mut App,
    ) {
        if let Some(mut child) = request_layout.child_element.take() {
            child.paint(window, cx);
        }

        if let Some(mut menu) = request_layout.menu_element.take() {
            menu.paint(window, cx);

            if let Some(child_hitbox) = *child_hitbox {
                let menu_handle = request_layout.menu_handle.clone();
                window.on_mouse_event(move |_: &MouseDownEvent, phase, window, cx| {
                    if phase == DispatchPhase::Bubble && child_hitbox.is_hovered(window) {
                        if let Some(menu) = menu_handle.borrow().as_ref() {
                            menu.update(cx, |_, cx| {
                                cx.emit(DismissEvent);
                            });
                        }
                        cx.stop_propagation();
                    }
                })
            }
        }
    }
}

impl<M: ManagedView> IntoElement for PopoverMenu<M> {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}
