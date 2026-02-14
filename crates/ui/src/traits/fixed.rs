use gpui::DefiniteLength;

pub trait FixedWidth {
    fn width(self, width: impl Into<DefiniteLength>) -> Self;
    fn full_width(self) -> Self;
}
