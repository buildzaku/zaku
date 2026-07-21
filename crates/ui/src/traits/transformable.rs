use gpui::Transformation;

pub trait Transformable {
    fn transform(self, transformation: Transformation) -> Self;
}
