use gpui::{App, BoxShadow, Hsla};
use std::fmt::{self, Display, Formatter};

use theme::{ActiveTheme, Appearance};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ElevationIndex {
    Background,
    Surface,
    EditorSurface,
    ElevatedSurface,
    ModalSurface,
}

impl Display for ElevationIndex {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        match self {
            ElevationIndex::Background => write!(f, "Background"),
            ElevationIndex::Surface => write!(f, "Surface"),
            ElevationIndex::EditorSurface => write!(f, "Editor Surface"),
            ElevationIndex::ElevatedSurface => write!(f, "Elevated Surface"),
            ElevationIndex::ModalSurface => write!(f, "Modal Surface"),
        }
    }
}

impl ElevationIndex {
    pub fn shadow(self, cx: &App) -> Vec<BoxShadow> {
        let is_light = cx.theme().appearance() == Appearance::Light;

        match self {
            ElevationIndex::Surface => vec![],
            ElevationIndex::EditorSurface => vec![],

            ElevationIndex::ElevatedSurface => vec![
                BoxShadow {
                    color: gpui::hsla(0., 0., 0., 0.12),
                    offset: gpui::point(gpui::px(0.), gpui::px(2.)),
                    blur_radius: gpui::px(3.),
                    spread_radius: gpui::px(0.),
                },
                BoxShadow {
                    color: gpui::hsla(0., 0., 0., if is_light { 0.03 } else { 0.06 }),
                    offset: gpui::point(gpui::px(0.), gpui::px(1.)),
                    blur_radius: gpui::px(0.),
                    spread_radius: gpui::px(0.),
                },
            ],

            ElevationIndex::ModalSurface => vec![
                BoxShadow {
                    color: gpui::hsla(0., 0., 0., if is_light { 0.06 } else { 0.12 }),
                    offset: gpui::point(gpui::px(0.), gpui::px(2.)),
                    blur_radius: gpui::px(3.),
                    spread_radius: gpui::px(0.),
                },
                BoxShadow {
                    color: gpui::hsla(0., 0., 0., if is_light { 0.06 } else { 0.08 }),
                    offset: gpui::point(gpui::px(0.), gpui::px(3.)),
                    blur_radius: gpui::px(6.),
                    spread_radius: gpui::px(0.),
                },
                BoxShadow {
                    color: gpui::hsla(0., 0., 0., 0.04),
                    offset: gpui::point(gpui::px(0.), gpui::px(6.)),
                    blur_radius: gpui::px(12.),
                    spread_radius: gpui::px(0.),
                },
                BoxShadow {
                    color: gpui::hsla(0., 0., 0., if is_light { 0.04 } else { 0.12 }),
                    offset: gpui::point(gpui::px(0.), gpui::px(1.)),
                    blur_radius: gpui::px(0.),
                    spread_radius: gpui::px(0.),
                },
            ],

            _ => vec![],
        }
    }

    pub fn bg(&self, cx: &mut App) -> Hsla {
        match self {
            ElevationIndex::Background => cx.theme().colors().background,
            ElevationIndex::Surface => cx.theme().colors().surface_background,
            ElevationIndex::EditorSurface => cx.theme().colors().editor_background,
            ElevationIndex::ElevatedSurface => cx.theme().colors().elevated_surface_background,
            ElevationIndex::ModalSurface => cx.theme().colors().elevated_surface_background,
        }
    }

    pub fn on_elevation_bg(&self, cx: &App) -> Hsla {
        match self {
            ElevationIndex::Background => cx.theme().colors().surface_background,
            ElevationIndex::Surface => cx.theme().colors().background,
            ElevationIndex::EditorSurface => cx.theme().colors().surface_background,
            ElevationIndex::ElevatedSurface => cx.theme().colors().background,
            ElevationIndex::ModalSurface => cx.theme().colors().background,
        }
    }

    /// Attempts to return a darker background color than the current elevation index's background.
    ///
    /// If the current background color is already dark, it will return a lighter color instead.
    pub fn darker_bg(&self, cx: &App) -> Hsla {
        match self {
            ElevationIndex::Background => cx.theme().colors().surface_background,
            ElevationIndex::Surface => cx.theme().colors().editor_background,
            ElevationIndex::EditorSurface => cx.theme().colors().surface_background,
            ElevationIndex::ElevatedSurface => cx.theme().colors().editor_background,
            ElevationIndex::ModalSurface => cx.theme().colors().editor_background,
        }
    }
}
