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

impl ElevationIndex {
    pub fn shadow(self, cx: &App) -> Vec<BoxShadow> {
        let is_light = cx.theme().appearance() == Appearance::Light;

        match self {
            ElevationIndex::ElevatedSurface => vec![
                BoxShadow {
                    color: gpui::hsla(0., 0., 0., 0.12),
                    offset: gpui::point(gpui::px(0.), gpui::px(2.)),
                    blur_radius: gpui::px(3.),
                    spread_radius: gpui::px(0.),
                    inset: false,
                },
                BoxShadow {
                    color: gpui::hsla(0., 0., 0., if is_light { 0.03 } else { 0.06 }),
                    offset: gpui::point(gpui::px(0.), gpui::px(1.)),
                    blur_radius: gpui::px(0.),
                    spread_radius: gpui::px(0.),
                    inset: false,
                },
            ],
            ElevationIndex::ModalSurface => vec![
                BoxShadow {
                    color: gpui::hsla(0., 0., 0., if is_light { 0.06 } else { 0.12 }),
                    offset: gpui::point(gpui::px(0.), gpui::px(2.)),
                    blur_radius: gpui::px(3.),
                    spread_radius: gpui::px(0.),
                    inset: false,
                },
                BoxShadow {
                    color: gpui::hsla(0., 0., 0., if is_light { 0.06 } else { 0.08 }),
                    offset: gpui::point(gpui::px(0.), gpui::px(3.)),
                    blur_radius: gpui::px(6.),
                    spread_radius: gpui::px(0.),
                    inset: false,
                },
                BoxShadow {
                    color: gpui::hsla(0., 0., 0., 0.04),
                    offset: gpui::point(gpui::px(0.), gpui::px(6.)),
                    blur_radius: gpui::px(12.),
                    spread_radius: gpui::px(0.),
                    inset: false,
                },
                BoxShadow {
                    color: gpui::hsla(0., 0., 0., if is_light { 0.04 } else { 0.12 }),
                    offset: gpui::point(gpui::px(0.), gpui::px(1.)),
                    blur_radius: gpui::px(0.),
                    spread_radius: gpui::px(0.),
                    inset: false,
                },
            ],
            ElevationIndex::Background
            | ElevationIndex::Surface
            | ElevationIndex::EditorSurface => {
                vec![]
            }
        }
    }

    pub fn bg(&self, cx: &mut App) -> Hsla {
        match self {
            ElevationIndex::Background => cx.theme().colors().background,
            ElevationIndex::Surface => cx.theme().colors().surface_background,
            ElevationIndex::EditorSurface => cx.theme().colors().editor_background,
            ElevationIndex::ElevatedSurface | ElevationIndex::ModalSurface => {
                cx.theme().colors().elevated_surface_background
            }
        }
    }

    pub fn on_elevation_bg(&self, cx: &App) -> Hsla {
        match self {
            ElevationIndex::Surface => cx.theme().colors().background,
            ElevationIndex::Background | ElevationIndex::EditorSurface => {
                cx.theme().colors().surface_background
            }
            ElevationIndex::ElevatedSurface | ElevationIndex::ModalSurface => {
                cx.theme().colors().background
            }
        }
    }

    /// Attempts to return a darker background color than the current elevation index's background.
    ///
    /// If the current background color is already dark, it will return a lighter color instead.
    pub fn darker_bg(&self, cx: &App) -> Hsla {
        match self {
            ElevationIndex::Background | ElevationIndex::EditorSurface => {
                cx.theme().colors().surface_background
            }
            ElevationIndex::Surface
            | ElevationIndex::ElevatedSurface
            | ElevationIndex::ModalSurface => cx.theme().colors().editor_background,
        }
    }
}

impl Display for ElevationIndex {
    fn fmt(&self, formatter: &mut Formatter) -> fmt::Result {
        match self {
            ElevationIndex::Background => write!(formatter, "Background"),
            ElevationIndex::Surface => write!(formatter, "Surface"),
            ElevationIndex::EditorSurface => write!(formatter, "Editor Surface"),
            ElevationIndex::ElevatedSurface => write!(formatter, "Elevated Surface"),
            ElevationIndex::ModalSurface => write!(formatter, "Modal Surface"),
        }
    }
}
