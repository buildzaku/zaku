use std::ops::Range;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CodeLabel {
    pub text: String,
    pub filter_range: Range<usize>,
}

impl CodeLabel {
    pub fn plain(text: String, filter_text: Option<&str>) -> Self {
        let filter_range = filter_text
            .and_then(|filter| text.find(filter).map(|index| index..index + filter.len()))
            .unwrap_or(0..text.len());
        Self { text, filter_range }
    }

    pub fn filter_text(&self) -> &str {
        self.text
            .get(self.filter_range.clone())
            .expect("filter range should be on char boundary")
    }
}
