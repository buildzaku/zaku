use gpui::{Div, Pixels, Rems, Size, Styled, Window};

#[derive(Debug, Clone, Copy)]
pub(crate) struct PositionAndShape {
    pub(crate) left: Pixels,
    pub(crate) right: Pixels,
    pub(crate) top: Pixels,
    pub(crate) bottom: Pixels,
}

macro_rules! relative_size {
    ($name:ident, $accessor:ident) => {
        #[derive(Debug, Clone, Copy, PartialEq)]
        pub struct $name {
            viewport_fraction: f32,
            rems: Rems,
        }

        impl From<Rems> for $name {
            fn from(value: Rems) -> Self {
                Self::rems(value)
            }
        }

        impl $name {
            pub const FULL: Self = Self {
                viewport_fraction: 1.0,
                rems: Rems::ZERO,
            };

            pub const fn viewport(fraction: f32) -> Self {
                debug_assert!(fraction <= 1.0);
                debug_assert!(fraction >= 0.0);
                Self {
                    viewport_fraction: fraction.clamp(0.0, 1.0),
                    rems: Rems::ZERO,
                }
            }

            pub const fn rems(value: Rems) -> Self {
                Self {
                    viewport_fraction: 0.0,
                    rems: value,
                }
            }

            pub fn as_pixels(&self, window: &Window) -> Pixels {
                self.viewport_fraction * window.viewport_size().$accessor
                    + self.rems * window.rem_size()
            }
        }

        impl std::ops::Add for $name {
            type Output = Self;

            fn add(self, rhs: Self) -> Self::Output {
                Self {
                    viewport_fraction: self.viewport_fraction + rhs.viewport_fraction,
                    rems: self.rems + rhs.rems,
                }
            }
        }

        impl std::ops::Sub for $name {
            type Output = Self;

            fn sub(self, rhs: Self) -> Self::Output {
                Self {
                    viewport_fraction: self.viewport_fraction - rhs.viewport_fraction,
                    rems: self.rems - rhs.rems,
                }
            }
        }

        impl std::ops::Sub<Rems> for $name {
            type Output = Self;

            fn sub(self, rhs: Rems) -> Self::Output {
                Self {
                    viewport_fraction: self.viewport_fraction,
                    rems: self.rems - rhs,
                }
            }
        }

        impl std::ops::Div<f32> for $name {
            type Output = Self;

            fn div(mut self, rhs: f32) -> Self::Output {
                self.viewport_fraction /= rhs;
                self.rems = Rems(self.rems.0 / rhs);
                self
            }
        }

        impl std::ops::Mul<f32> for $name {
            type Output = Self;

            fn mul(mut self, rhs: f32) -> Self::Output {
                self.viewport_fraction *= rhs;
                self.rems = Rems(self.rems.0 * rhs);
                self
            }
        }
    };
}

relative_size!(RelativeHeight, height);
relative_size!(RelativeWidth, width);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum VerticalPadding {
    #[default]
    Pad,
    None,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct Centered {
    pub(crate) width: RelativeWidth,
    pub(crate) height: RelativeHeight,
}

impl Default for Centered {
    fn default() -> Self {
        Centered {
            width: RelativeWidth::viewport(0.6),
            height: RelativeHeight::viewport(0.6),
        }
    }
}

#[derive(Debug)]
pub(crate) struct SizeBounds {
    pub(crate) max_width: RelativeWidth,
    pub(crate) max_height: RelativeHeight,
    pub(crate) min_results: Size<Rems>,
}

impl SizeBounds {
    fn clamp_width(&self, width: Pixels, window: &Window) -> Pixels {
        width
            .min(self.max_width.as_pixels(window))
            .max(self.min_results.width * window.rem_size())
    }

    fn clamp_height(&self, height: Pixels, window: &Window) -> Pixels {
        height
            .min(self.max_height.as_pixels(window))
            .max(self.min_results.height * window.rem_size())
    }

    fn clamp_position_and_size(&self, working: &mut PositionAndShape, window: &Window) {
        let target_width = self.clamp_width(working.right - working.left, window);
        let center = (working.left + working.right) / 2.0;
        working.left = center - target_width / 2.0;
        working.right = center + target_width / 2.0;

        let target_height = self.clamp_height(working.bottom - working.top, window);
        working.bottom = working.top + target_height;
    }
}

impl Default for SizeBounds {
    fn default() -> Self {
        Self {
            max_width: RelativeWidth::viewport(0.95),
            max_height: (RelativeHeight::FULL - Rems(10.0)) * 0.95,
            min_results: Size {
                width: Rems(15.0),
                height: Rems(20.0),
            },
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub(crate) enum Shape {
    HorizontallyCentered(Centered),
}

impl Shape {
    pub(crate) fn picker_position_and_size(&self, window: &Window) -> PositionAndShape {
        match self {
            Shape::HorizontallyCentered(Centered { width, height }) => PositionAndShape {
                left: ((RelativeWidth::FULL - *width) / 2.0).as_pixels(window),
                right: (RelativeWidth::FULL / 2.0 + *width / 2.0).as_pixels(window),
                top: Pixels::ZERO,
                bottom: height.as_pixels(window),
            },
        }
    }

    pub(crate) fn clamped_position_and_size(
        &self,
        bounds: &SizeBounds,
        window: &Window,
    ) -> PositionAndShape {
        let mut position = self.picker_position_and_size(window);
        bounds.clamp_position_and_size(&mut position, window);
        position
    }

    pub(crate) fn apply_results_size(
        &self,
        bounds: &SizeBounds,
        vertical_padding: VerticalPadding,
        div: Div,
        window: &Window,
    ) -> Div {
        let position = self.clamped_position_and_size(bounds, window);
        let div = div.w(position.right - position.left);
        match vertical_padding {
            VerticalPadding::None => div,
            VerticalPadding::Pad => div.h(position.bottom - position.top),
        }
    }

    pub(crate) fn results_max_height(
        &self,
        bounds: &SizeBounds,
        vertical_padding: VerticalPadding,
        window: &Window,
    ) -> Option<Pixels> {
        match vertical_padding {
            VerticalPadding::None => Some(self.height(bounds, window)),
            VerticalPadding::Pad => None,
        }
    }

    pub(crate) fn height(&self, bounds: &SizeBounds, window: &Window) -> Pixels {
        let position = self.clamped_position_and_size(bounds, window);
        position.bottom - position.top
    }

    pub(crate) fn set_initial_width(&mut self, width: impl Into<RelativeWidth>) {
        let Shape::HorizontallyCentered(Centered {
            width: current_width,
            ..
        }) = self;
        *current_width = width.into();
    }

    pub(crate) fn set_initial_height(&mut self, height: impl Into<RelativeHeight>) {
        let Shape::HorizontallyCentered(Centered {
            height: current_height,
            ..
        }) = self;
        *current_height = height.into();
    }
}

impl Default for Shape {
    fn default() -> Self {
        Self::HorizontallyCentered(Centered::default())
    }
}
