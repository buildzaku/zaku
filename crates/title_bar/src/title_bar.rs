mod application_menu;

pub use platform_title_bar::{self, PlatformTitleBar};

use gpui::{
    AnyElement, App, Context, ElementId, Entity, MouseButton, Subscription, WeakEntity, Window,
    prelude::*,
};
use smallvec::SmallVec;

use ui::{h_flex, prelude::*};
use workspace::Workspace;

use crate::application_menu::ApplicationMenu;

pub fn init(cx: &mut App) {
    cx.observe_new(|workspace: &mut Workspace, window, cx| {
        let Some(window) = window else {
            return;
        };

        let item = cx.new(|cx| TitleBar::new("title-bar", workspace, window, cx));
        workspace.set_titlebar_item(item.into(), window, cx);
    })
    .detach();
}

pub struct TitleBar {
    platform_titlebar: Entity<PlatformTitleBar>,
    workspace: WeakEntity<Workspace>,
    application_menu: Option<Entity<ApplicationMenu>>,
    _workspace_subscription: Option<Subscription>,
}

impl TitleBar {
    pub fn new(
        id: impl Into<ElementId>,
        workspace: &Workspace,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let workspace = workspace.weak_handle();
        let application_menu = Some(cx.new(|cx| ApplicationMenu::new(window, cx)));
        let workspace_subscription = workspace
            .upgrade()
            .map(|workspace_entity| cx.observe(&workspace_entity, |_, _, cx| cx.notify()));

        let platform_titlebar = cx.new(|cx| PlatformTitleBar::new(id, cx));

        Self {
            platform_titlebar,
            workspace,
            application_menu,
            _workspace_subscription: workspace_subscription,
        }
    }
}

impl Render for TitleBar {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if self.workspace.upgrade().is_none() {
            self.application_menu = None;
        }

        let mut children = SmallVec::<[AnyElement; 2]>::new();

        children.push(h_flex().h_full().flex_1().into_any_element());

        children.extend(self.application_menu.clone().map(|menu| {
            h_flex()
                .h_full()
                .items_center()
                .px(DynamicSpacing::Base08.rems(cx))
                .on_mouse_down(MouseButton::Left, |_event, _window, cx| {
                    cx.stop_propagation();
                })
                .child(menu)
                .into_any_element()
        }));

        self.platform_titlebar.update(cx, |titlebar, _cx| {
            titlebar.set_children(children);
        });

        self.platform_titlebar.clone().into_any_element()
    }
}
