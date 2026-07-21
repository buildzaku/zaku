use gpui::{Animation, AnimationElement, AnimationExt, ElementId, Transformation, percentage};
use std::{panic, time::Duration};

use crate::Transformable;

pub trait CommonAnimationExt: AnimationExt {
    #[track_caller]
    fn with_rotate_animation(self, duration: u64) -> AnimationElement<Self>
    where
        Self: Transformable + Sized,
    {
        self.with_keyed_rotate_animation(
            ElementId::CodeLocation(*panic::Location::caller()),
            duration,
        )
    }

    fn with_keyed_rotate_animation(
        self,
        id: impl Into<ElementId>,
        duration: u64,
    ) -> AnimationElement<Self>
    where
        Self: Transformable + Sized,
    {
        self.with_animation(
            id,
            Animation::new(Duration::from_secs(duration)).repeat(),
            |component, delta| component.transform(Transformation::rotate(percentage(delta))),
        )
    }
}

impl<T: AnimationExt> CommonAnimationExt for T {}
