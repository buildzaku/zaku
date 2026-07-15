use gpui::{App, Hsla, IntoElement, PathBuilder, Pixels, RenderOnce, Window, prelude::*};
use std::f32::consts::PI;

use theme::ActiveTheme;

#[derive(IntoElement)]
pub struct CircularProgress {
    value: f32,
    max_value: f32,
    size: Pixels,
    stroke_width: Pixels,
    radius: Option<Pixels>,
    bg_color: Hsla,
    progress_color: Hsla,
}

impl CircularProgress {
    pub fn new(value: f32, max_value: f32, size: Pixels, cx: &App) -> Self {
        Self {
            value,
            max_value,
            size,
            stroke_width: gpui::px(4.0),
            radius: None,
            bg_color: cx.theme().colors().border_variant,
            progress_color: cx.theme().status().info,
        }
    }

    pub fn value(mut self, value: f32) -> Self {
        self.value = value;
        self
    }

    pub fn max_value(mut self, max_value: f32) -> Self {
        self.max_value = max_value;
        self
    }

    pub fn size(mut self, size: Pixels) -> Self {
        self.size = size;
        self
    }

    pub fn stroke_width(mut self, stroke_width: Pixels) -> Self {
        self.stroke_width = stroke_width;
        self
    }

    pub fn radius(mut self, radius: Pixels) -> Self {
        self.radius = Some(radius);
        self
    }

    pub fn bg_color(mut self, color: Hsla) -> Self {
        self.bg_color = color;
        self
    }

    pub fn progress_color(mut self, color: Hsla) -> Self {
        self.progress_color = color;
        self
    }
}

impl RenderOnce for CircularProgress {
    fn render(self, _: &mut Window, _: &mut App) -> impl IntoElement {
        let value = self.value;
        let max_value = self.max_value;
        let size = self.size;
        let stroke_width = self.stroke_width;
        let radius = self.radius.unwrap_or((size / 2.0) - stroke_width);
        let bg_color = self.bg_color;
        let progress_color = self.progress_color;

        gpui::canvas(
            |_, _, _| {},
            move |bounds, (), window, _| {
                let center_x = bounds.origin.x + bounds.size.width / 2.0;
                let center_y = bounds.origin.y + bounds.size.height / 2.0;
                let mut track = PathBuilder::stroke(stroke_width);
                track.move_to(gpui::point(center_x + radius, center_y));
                track.arc_to(
                    gpui::point(radius, radius),
                    gpui::px(0.0),
                    false,
                    true,
                    gpui::point(center_x - radius, center_y),
                );
                track.arc_to(
                    gpui::point(radius, radius),
                    gpui::px(0.0),
                    false,
                    true,
                    gpui::point(center_x + radius, center_y),
                );
                track.close();

                if let Ok(path) = track.build() {
                    window.paint_path(path, bg_color);
                }

                let progress = (value / max_value).clamp(0.0, 1.0);
                if progress <= 0.0 {
                    return;
                }

                let mut progress_arc = PathBuilder::stroke(stroke_width);
                if progress >= 0.999 {
                    progress_arc.move_to(gpui::point(center_x + radius, center_y));
                    progress_arc.arc_to(
                        gpui::point(radius, radius),
                        gpui::px(0.0),
                        false,
                        true,
                        gpui::point(center_x - radius, center_y),
                    );
                    progress_arc.arc_to(
                        gpui::point(radius, radius),
                        gpui::px(0.0),
                        false,
                        true,
                        gpui::point(center_x + radius, center_y),
                    );
                    progress_arc.close();
                } else {
                    let start_x = center_x;
                    let start_y = center_y - radius;
                    progress_arc.move_to(gpui::point(start_x, start_y));

                    let end_angle = -PI / 2.0 + progress * 2.0 * PI;
                    let end_x = center_x + radius * end_angle.cos();
                    let end_y = center_y + radius * end_angle.sin();
                    let use_large_arc = progress > 0.5;

                    progress_arc.arc_to(
                        gpui::point(radius, radius),
                        gpui::px(0.0),
                        use_large_arc,
                        true,
                        gpui::point(end_x, end_y),
                    );
                }

                if let Ok(path) = progress_arc.build() {
                    window.paint_path(path, progress_color);
                }
            },
        )
        .size(size)
    }
}
