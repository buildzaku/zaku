use gpui::{
    Action, AnyElement, AnyView, App, AppContext, Div, FocusHandle, Length, SharedString, Window,
    prelude::*,
};
use std::{borrow::Borrow, rc::Rc};

use theme::ThemeSettings;

use super::label::{Label, LabelCommon, LabelSize};

use crate::{ActiveTheme, Color, KeyBinding, StyledExt, StyledTypography};

#[derive(Clone, IntoElement)]
enum Title {
    Str(SharedString),
    Callback(Rc<dyn Fn(&mut Window, &mut App) -> AnyElement>),
}

impl Title {
    fn text(title: impl Into<SharedString>) -> Self {
        Title::Str(title.into())
    }
}

impl From<SharedString> for Title {
    fn from(value: SharedString) -> Self {
        Title::Str(value)
    }
}

impl RenderOnce for Title {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        match self {
            Title::Str(title) => title.into_any_element(),
            Title::Callback(element) => element(window, cx),
        }
    }
}

pub struct Tooltip {
    title: Title,
    meta: Option<SharedString>,
    key_binding: Option<KeyBinding>,
    max_w: Option<Length>,
}

impl Tooltip {
    pub fn simple(title: impl Into<SharedString>, cx: &mut App) -> AnyView {
        cx.new(|_| Self {
            title: Title::Str(title.into()),
            meta: None,
            key_binding: None,
            max_w: None,
        })
        .into()
    }

    pub fn text(title: impl Into<SharedString>) -> impl Fn(&mut Window, &mut App) -> AnyView {
        let title = title.into();
        move |_, cx| {
            cx.new(|_| Self {
                title: title.clone().into(),
                meta: None,
                key_binding: None,
                max_w: None,
            })
            .into()
        }
    }

    pub fn for_action_title<T: Into<SharedString>>(
        title: T,
        action: &dyn Action,
    ) -> impl Fn(&mut Window, &mut App) -> AnyView + use<T> {
        let title = title.into();
        let action = action.boxed_clone();
        move |_, cx| {
            cx.new(|cx| Self {
                title: Title::Str(title.clone()),
                meta: None,
                key_binding: Some(KeyBinding::for_action(action.as_ref(), cx)),
                max_w: None,
            })
            .into()
        }
    }

    pub fn for_action_title_in<Str: Into<SharedString>>(
        title: Str,
        action: &dyn Action,
        focus_handle: &FocusHandle,
    ) -> impl Fn(&mut Window, &mut App) -> AnyView + use<Str> {
        let title = title.into();
        let action = action.boxed_clone();
        let focus_handle = focus_handle.clone();
        move |_, cx| {
            cx.new(|cx| Self {
                title: Title::Str(title.clone()),
                meta: None,
                key_binding: Some(KeyBinding::for_action_in(
                    action.as_ref(),
                    &focus_handle,
                    cx,
                )),
                max_w: None,
            })
            .into()
        }
    }

    pub fn for_action(
        title: impl Into<SharedString>,
        action: &dyn Action,
        cx: &mut App,
    ) -> AnyView {
        cx.new(|cx| Self {
            title: Title::Str(title.into()),
            meta: None,
            key_binding: Some(KeyBinding::for_action(action, cx)),
            max_w: None,
        })
        .into()
    }

    pub fn for_action_in(
        title: impl Into<SharedString>,
        action: &dyn Action,
        focus_handle: &FocusHandle,
        cx: &mut App,
    ) -> AnyView {
        cx.new(|cx| Self {
            title: Title::text(title),
            meta: None,
            key_binding: Some(KeyBinding::for_action_in(action, focus_handle, cx)),
            max_w: None,
        })
        .into()
    }

    pub fn with_meta(
        title: impl Into<SharedString>,
        action: Option<&dyn Action>,
        meta: impl Into<SharedString>,
        cx: &mut App,
    ) -> AnyView {
        cx.new(|cx| Self {
            title: Title::text(title),
            meta: Some(meta.into()),
            key_binding: action.map(|action| KeyBinding::for_action(action, cx)),
            max_w: None,
        })
        .into()
    }

    pub fn with_meta_in(
        title: impl Into<SharedString>,
        action: Option<&dyn Action>,
        meta: impl Into<SharedString>,
        focus_handle: &FocusHandle,
        cx: &mut App,
    ) -> AnyView {
        cx.new(|cx| Self {
            title: Title::text(title),
            meta: Some(meta.into()),
            key_binding: action.map(|action| KeyBinding::for_action_in(action, focus_handle, cx)),
            max_w: None,
        })
        .into()
    }

    pub fn new(title: impl Into<SharedString>) -> Self {
        Self {
            title: Title::text(title),
            meta: None,
            key_binding: None,
            max_w: None,
        }
    }

    pub fn new_element(title: impl Fn(&mut Window, &mut App) -> AnyElement + 'static) -> Self {
        Self {
            title: Title::Callback(Rc::new(title)),
            meta: None,
            key_binding: None,
            max_w: None,
        }
    }

    pub fn element(
        title: impl Fn(&mut Window, &mut App) -> AnyElement + 'static,
    ) -> impl Fn(&mut Window, &mut App) -> AnyView {
        let title = Title::Callback(Rc::new(title));
        move |_, cx| {
            let title = title.clone();
            cx.new(|_| Self {
                title,
                meta: None,
                key_binding: None,
                max_w: None,
            })
            .into()
        }
    }

    pub fn meta(mut self, meta: impl Into<SharedString>) -> Self {
        self.meta = Some(meta.into());
        self
    }

    pub fn key_binding(mut self, key_binding: impl Into<Option<KeyBinding>>) -> Self {
        self.key_binding = key_binding.into();
        self
    }

    pub fn max_w(mut self, max_w: impl Into<Length>) -> Self {
        self.max_w = Some(max_w.into());
        self
    }
}

impl Render for Tooltip {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let title = self.title.clone();
        let meta = self.meta.clone();
        let key_binding = self.key_binding.clone();
        let max_w = self.max_w;
        tooltip_container(cx, move |element, _| {
            element
                .child(
                    gpui::div()
                        .flex()
                        .items_center()
                        .gap_4()
                        .child(
                            gpui::div()
                                .map(|this| match max_w {
                                    Some(max_w) => this.max_w(max_w),
                                    None => this.max_w_72(),
                                })
                                .child(title),
                        )
                        .when_some(key_binding, |this, key_binding| {
                            this.justify_between().child(key_binding)
                        }),
                )
                .when_some(meta, |this, meta| {
                    this.child(
                        gpui::div()
                            .map(|this| match max_w {
                                Some(max_w) => this.max_w(max_w),
                                None => this.max_w_72(),
                            })
                            .child(Label::new(meta).size(LabelSize::Small).color(Color::Muted)),
                    )
                })
        })
    }
}

pub fn tooltip_container<C>(
    cx: &mut C,
    builder: impl FnOnce(Div, &mut C) -> Div,
) -> impl IntoElement
where
    C: AppContext + Borrow<App>,
{
    let app = (*cx).borrow();
    let ui_font = ThemeSettings::get_global(app).ui_font.clone();

    gpui::div().pl_2().pt_2p5().child(
        gpui::div()
            .flex()
            .flex_col()
            .elevation_2(app)
            .rounded_sm()
            .font(ui_font)
            .text_ui(app)
            .text_color(app.theme().colors().text)
            .py_0p5()
            .px_1p5()
            .map(|element| builder(element, cx)),
    )
}

pub struct LinkPreview {
    link: SharedString,
}

impl LinkPreview {
    pub fn new(url: &str) -> Self {
        let mut wrapped_url = String::new();
        for (i, ch) in url.chars().enumerate() {
            if i == 500 {
                wrapped_url.push('…');
                break;
            }
            if i % 100 == 0 && i != 0 {
                wrapped_url.push('\n');
            }
            wrapped_url.push(ch);
        }
        Self {
            link: wrapped_url.into(),
        }
    }

    pub fn view(url: &str, cx: &mut App) -> AnyView {
        cx.new(|_| Self::new(url)).into()
    }
}

impl Render for LinkPreview {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        tooltip_container(cx, |element, _| {
            element.child(
                Label::new(self.link.clone())
                    .size(LabelSize::XSmall)
                    .color(Color::Muted),
            )
        })
    }
}
