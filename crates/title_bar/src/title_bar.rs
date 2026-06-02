mod application_menu;

pub use platform_title_bar::{self, PlatformTitleBar};

use gpui::{
    AnyElement, App, Context, ElementId, Entity, MouseButton, Subscription, WeakEntity, Window,
    WindowButton, prelude::*,
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
    _button_layout_subscription: Subscription,
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
        let button_layout_subscription =
            cx.observe_button_layout_changed(window, |_, _, cx| cx.notify());

        let platform_titlebar = cx.new(|cx| PlatformTitleBar::new(id, cx));

        Self {
            platform_titlebar,
            workspace,
            application_menu,
            _workspace_subscription: workspace_subscription,
            _button_layout_subscription: button_layout_subscription,
        }
    }
}

impl Render for TitleBar {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if self.workspace.upgrade().is_none() {
            self.application_menu = None;
        }

        let colors = cx.theme().colors();
        let mut children = SmallVec::<[AnyElement; 2]>::new();
        let button_layout = cx.button_layout();
        let title_bar_controls_on_left = match PlatformStyle::platform() {
            PlatformStyle::Linux => {
                let supported_controls = window.window_controls();

                button_layout.is_some_and(|button_layout| {
                    button_layout
                        .right
                        .iter()
                        .filter_map(|button| *button)
                        .any(|button| match button {
                            WindowButton::Minimize => supported_controls.minimize,
                            WindowButton::Maximize => supported_controls.maximize,
                            WindowButton::Close => true,
                        })
                })
            }
            PlatformStyle::Mac => false,
            PlatformStyle::Windows => true,
        };

        let zaku = gpui::div()
            .flex()
            .items_center()
            .px(DynamicSpacing::Base12.rems(cx))
            .child(
                Graphic::with_height(GraphicName::Zaku, IconSize::Small.rems())
                    .color(Color::Custom(colors.text)),
            )
            .into_any_element();
        let application_menu = gpui::div()
            .flex()
            .items_center()
            .gap(DynamicSpacing::Base04.rems(cx))
            .on_mouse_down(MouseButton::Left, |_event, _window, cx| {
                cx.stop_propagation();
            })
            .children(self.application_menu.clone())
            .into_any_element();
        let mut title_bar_controls = SmallVec::<[AnyElement; 2]>::new();

        if title_bar_controls_on_left {
            title_bar_controls.push(zaku);
            title_bar_controls.push(application_menu);
        } else {
            title_bar_controls.push(application_menu);
            title_bar_controls.push(zaku);
        }

        let title_bar_controls = h_flex()
            .h_full()
            .items_center()
            .children(title_bar_controls)
            .into_any_element();

        if title_bar_controls_on_left {
            children.push(title_bar_controls);
            children.push(h_flex().h_full().flex_1().into_any_element());
        } else {
            children.push(h_flex().h_full().flex_1().into_any_element());
            children.push(title_bar_controls);
        }

        self.platform_titlebar.update(cx, |titlebar, _cx| {
            titlebar.set_button_layout(button_layout);
            titlebar.set_children(children);
        });

        self.platform_titlebar.clone().into_any_element()
    }
}
