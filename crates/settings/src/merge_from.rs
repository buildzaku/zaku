pub trait MergeFrom {
    fn merge_from(&mut self, other: &Self);

    fn merge_from_option(&mut self, other: Option<&Self>) {
        if let Some(other) = other {
            self.merge_from(other);
        }
    }
}

impl MergeFrom for crate::UiDensity {
    fn merge_from(&mut self, other: &Self) {
        *self = *other;
    }
}

impl MergeFrom for gpui::Pixels {
    fn merge_from(&mut self, other: &Self) {
        *self = *other;
    }
}

impl<T: Clone + MergeFrom> MergeFrom for Option<T> {
    fn merge_from(&mut self, other: &Self) {
        let Some(other) = other else {
            return;
        };

        if let Some(this) = self.as_mut() {
            this.merge_from(other);
        } else {
            self.replace(other.clone());
        }
    }
}
