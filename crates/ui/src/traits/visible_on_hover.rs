use gpui::{InteractiveElement, SharedString, Styled};

pub trait VisibleOnHover {
    fn visible_on_hover(self, group_name: impl Into<SharedString>) -> Self;
}

impl<E: InteractiveElement + Styled> VisibleOnHover for E {
    fn visible_on_hover(self, group_name: impl Into<SharedString>) -> Self {
        self.invisible()
            .group_hover(group_name, |style| style.visible())
    }
}
