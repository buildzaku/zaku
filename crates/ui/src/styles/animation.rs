use gpui::{Animation, AnimationElement, AnimationExt, Element, ElementId, Styled};
use std::time::Duration;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AnimationDuration {
    Instant = 50,
    Fast = 150,
    Slow = 300,
}

impl AnimationDuration {
    pub fn duration(&self) -> Duration {
        Duration::from_millis(*self as u64)
    }
}

impl Into<Duration> for AnimationDuration {
    fn into(self) -> Duration {
        self.duration()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AnimationDirection {
    FromBottom,
    FromLeft,
    FromRight,
    FromTop,
}

pub trait DefaultAnimations: Styled + Sized + Element {
    fn animate_in(
        self,
        animation_type: AnimationDirection,
        fade_in: bool,
    ) -> AnimationElement<Self> {
        let animation_name = match animation_type {
            AnimationDirection::FromBottom => "animate_from_bottom",
            AnimationDirection::FromLeft => "animate_from_left",
            AnimationDirection::FromRight => "animate_from_right",
            AnimationDirection::FromTop => "animate_from_top",
        };

        let animation_id = self.id().map_or_else(
            || ElementId::from(animation_name),
            |id| (id, animation_name).into(),
        );

        self.with_animation(
            animation_id,
            Animation::new(AnimationDuration::Fast.into()).with_easing(gpui::ease_out_quint()),
            move |mut this, delta| {
                let start_opacity = 0.4;
                let start_pos = 0.0;
                let end_pos = 40.0;

                if fade_in {
                    this = this.opacity(start_opacity + delta * (1.0 - start_opacity));
                }

                match animation_type {
                    AnimationDirection::FromBottom => {
                        this.bottom(gpui::px(start_pos + delta * (end_pos - start_pos)))
                    }
                    AnimationDirection::FromLeft => {
                        this.left(gpui::px(start_pos + delta * (end_pos - start_pos)))
                    }
                    AnimationDirection::FromRight => {
                        this.right(gpui::px(start_pos + delta * (end_pos - start_pos)))
                    }
                    AnimationDirection::FromTop => {
                        this.top(gpui::px(start_pos + delta * (end_pos - start_pos)))
                    }
                }
            },
        )
    }

    fn animate_in_from_bottom(self, fade: bool) -> AnimationElement<Self> {
        self.animate_in(AnimationDirection::FromBottom, fade)
    }

    fn animate_in_from_left(self, fade: bool) -> AnimationElement<Self> {
        self.animate_in(AnimationDirection::FromLeft, fade)
    }

    fn animate_in_from_right(self, fade: bool) -> AnimationElement<Self> {
        self.animate_in(AnimationDirection::FromRight, fade)
    }

    fn animate_in_from_top(self, fade: bool) -> AnimationElement<Self> {
        self.animate_in(AnimationDirection::FromTop, fade)
    }
}

impl<E: Styled + Element> DefaultAnimations for E {}
