use gpui::{App, Styled};
use theme::ActiveTheme;

use crate::ElevationIndex;

fn elevated<E: Styled>(this: E, cx: &App, index: ElevationIndex) -> E {
    this.bg(cx.theme().colors().elevated_surface_background)
        .rounded_lg()
        .border_1()
        .border_color(cx.theme().colors().border_variant)
        .shadow(index.shadow(cx))
}

fn elevated_borderless<E: Styled>(this: E, cx: &mut App, index: ElevationIndex) -> E {
    this.bg(cx.theme().colors().elevated_surface_background)
        .rounded_lg()
        .shadow(index.shadow(cx))
}

pub trait StyledExt: Styled + Sized {
    fn h_flex(self) -> Self {
        self.flex().flex_row().items_center()
    }

    fn v_flex(self) -> Self {
        self.flex().flex_col()
    }

    /// Located above the app background, is the standard level for all elements
    ///
    /// Example Elements: Title Bar, Panel, Tab Bar, Editor
    fn elevation_1(self, cx: &App) -> Self {
        elevated(self, cx, ElevationIndex::Surface)
    }

    fn elevation_1_borderless(self, cx: &mut App) -> Self {
        elevated_borderless(self, cx, ElevationIndex::Surface)
    }

    /// Non-Modal Elevated Surfaces appear above the [`Surface`](ElevationIndex::Surface) layer and is used for things that should appear above most UI elements like an editor or panel, but not elements like popovers, context menus, modals, etc.
    ///
    /// Examples: Notifications, Palettes, Detached/Floating Windows, Detached/Floating Panels
    fn elevation_2(self, cx: &App) -> Self {
        elevated(self, cx, ElevationIndex::ElevatedSurface)
    }

    fn elevation_2_borderless(self, cx: &mut App) -> Self {
        elevated_borderless(self, cx, ElevationIndex::ElevatedSurface)
    }

    /// Modal Surfaces are used for elements that should appear above all other UI elements and are located above the wash layer. This is the maximum elevation at which UI elements can be rendered in their default state.
    ///
    /// Elements rendered at this layer should have an enforced behavior: Any interaction outside of the modal will either dismiss the modal or prompt an action (Save your progress, etc) then dismiss the modal.
    ///
    /// If the element does not have this behavior, it should be rendered at the [`Elevated Surface`](ElevationIndex::ElevatedSurface) layer.
    ///
    /// Examples: Settings Modal, Setup UI, Dialogs
    fn elevation_3(self, cx: &App) -> Self {
        elevated(self, cx, ElevationIndex::ModalSurface)
    }

    fn elevation_3_borderless(self, cx: &mut App) -> Self {
        elevated_borderless(self, cx, ElevationIndex::ModalSurface)
    }

    fn border_primary(self, cx: &mut App) -> Self {
        self.border_color(cx.theme().colors().border)
    }

    fn border_muted(self, cx: &mut App) -> Self {
        self.border_color(cx.theme().colors().border_variant)
    }

    fn debug_bg_red(self) -> Self {
        self.bg(gpui::hsla(0. / 360., 1., 0.5, 1.))
    }

    fn debug_bg_green(self) -> Self {
        self.bg(gpui::hsla(120. / 360., 1., 0.5, 1.))
    }

    fn debug_bg_blue(self) -> Self {
        self.bg(gpui::hsla(240. / 360., 1., 0.5, 1.))
    }

    fn debug_bg_yellow(self) -> Self {
        self.bg(gpui::hsla(60. / 360., 1., 0.5, 1.))
    }

    fn debug_bg_cyan(self) -> Self {
        self.bg(gpui::hsla(160. / 360., 1., 0.5, 1.))
    }

    fn debug_bg_magenta(self) -> Self {
        self.bg(gpui::hsla(300. / 360., 1., 0.5, 1.))
    }
}

impl<E: Styled> StyledExt for E {}
