use gpui::{App, Hsla, IntoElement, Pixels, RenderOnce, Window, WindowControlArea, prelude::*};
#[cfg(target_os = "windows")]
use windows::{Wdk::System::SystemServices, Win32::System::SystemInformation::OSVERSIONINFOW};

use ui::prelude::*;

#[derive(IntoElement)]
pub struct WindowsWindowControls {
    button_height: Pixels,
}

impl WindowsWindowControls {
    pub fn new(button_height: Pixels) -> Self {
        Self { button_height }
    }

    #[cfg(any(target_os = "linux", target_os = "macos"))]
    fn get_font() -> &'static str {
        "Segoe Fluent Icons"
    }

    #[cfg(target_os = "windows")]
    fn get_font() -> &'static str {
        let mut version = OSVERSIONINFOW::default();
        version.dwOSVersionInfoSize = u32::try_from(std::mem::size_of_val(&version))
            .expect("OSVERSIONINFOW size should fit in u32");

        // SAFETY: RtlGetVersion writes to the provided output buffer, and `version`
        // remains valid for the duration of the call.
        let status = unsafe { SystemServices::RtlGetVersion(&raw mut version) };

        if status.is_ok() && version.dwBuildNumber >= 22000 {
            "Segoe Fluent Icons"
        } else {
            "Segoe MDL2 Assets"
        }
    }
}

impl RenderOnce for WindowsWindowControls {
    fn render(self, window: &mut Window, _: &mut App) -> impl IntoElement {
        gpui::div()
            .id("windows-window-controls")
            .font_family(Self::get_font())
            .flex()
            .flex_row()
            .justify_center()
            .content_stretch()
            .max_h(self.button_height)
            .min_h(self.button_height)
            .child(WindowsCaptionButton::Minimize)
            .map(|this| {
                this.child(if window.is_maximized() {
                    WindowsCaptionButton::Restore
                } else {
                    WindowsCaptionButton::Maximize
                })
            })
            .child(WindowsCaptionButton::Close)
    }
}

#[derive(IntoElement)]
enum WindowsCaptionButton {
    Minimize,
    Restore,
    Maximize,
    Close,
}

impl WindowsCaptionButton {
    #[inline]
    fn id(&self) -> &'static str {
        match self {
            Self::Minimize => "minimize",
            Self::Restore => "restore",
            Self::Maximize => "maximize",
            Self::Close => "close",
        }
    }

    #[inline]
    fn icon(&self) -> &'static str {
        match self {
            Self::Minimize => "\u{e921}",
            Self::Restore => "\u{e923}",
            Self::Maximize => "\u{e922}",
            Self::Close => "\u{e8bb}",
        }
    }

    #[inline]
    fn control_area(&self) -> WindowControlArea {
        match self {
            Self::Close => WindowControlArea::Close,
            Self::Maximize | Self::Restore => WindowControlArea::Max,
            Self::Minimize => WindowControlArea::Min,
        }
    }
}

impl RenderOnce for WindowsCaptionButton {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let (hover_background, hover_foreground, active_background, active_foreground) = match &self
        {
            Self::Close => {
                let color: Hsla = gpui::rgb(0xe81123).into();

                (
                    color,
                    gpui::white(),
                    color.opacity(f32::from(0x98_u8) / 255.0),
                    gpui::white(),
                )
            }
            Self::Minimize | Self::Restore | Self::Maximize => {
                let foreground = cx.theme().colors().text;

                (
                    foreground.opacity(f32::from(0x1a_u8) / 255.0),
                    foreground,
                    foreground.opacity(f32::from(0x33_u8) / 255.0),
                    foreground,
                )
            }
        };

        gpui::div()
            .h_flex()
            .id(self.id())
            .justify_center()
            .content_center()
            .occlude()
            .w_9()
            .h_full()
            .text_size(gpui::rems(0.625))
            .hover(|style| style.bg(hover_background).text_color(hover_foreground))
            .active(|style| style.bg(active_background).text_color(active_foreground))
            .window_control_area(self.control_area())
            .child(self.icon())
    }
}
