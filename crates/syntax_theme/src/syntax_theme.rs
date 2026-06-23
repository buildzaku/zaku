use gpui::HighlightStyle;
#[cfg(any(test, feature = "test-support"))]
use gpui::Hsla;
use std::{
    collections::{BTreeMap, btree_map::Entry},
    sync::Arc,
};

#[derive(Debug, PartialEq, Eq, Clone, Default)]
pub struct SyntaxTheme {
    highlights: Vec<HighlightStyle>,
    capture_name_map: BTreeMap<String, usize>,
}

impl SyntaxTheme {
    pub fn new(highlights: impl IntoIterator<Item = (String, HighlightStyle)>) -> Self {
        let (capture_names, highlights) = highlights.into_iter().unzip();

        Self {
            capture_name_map: Self::create_capture_name_map(capture_names),
            highlights,
        }
    }

    fn create_capture_name_map(highlights: Vec<String>) -> BTreeMap<String, usize> {
        highlights
            .into_iter()
            .enumerate()
            .map(|(index, key)| (key, index))
            .collect()
    }

    #[cfg(any(test, feature = "test-support"))]
    pub fn test_new(colors: impl IntoIterator<Item = (&'static str, Hsla)>) -> Self {
        Self::test_new_styles(colors.into_iter().map(|(key, color)| {
            (
                key,
                HighlightStyle {
                    color: Some(color),
                    ..Default::default()
                },
            )
        }))
    }

    #[cfg(any(test, feature = "test-support"))]
    pub fn test_new_styles(
        colors: impl IntoIterator<Item = (&'static str, HighlightStyle)>,
    ) -> Self {
        Self::new(
            colors
                .into_iter()
                .map(|(key, style)| (key.to_owned(), style)),
        )
    }

    pub fn get(&self, highlight_index: impl Into<usize>) -> Option<&HighlightStyle> {
        self.highlights.get(highlight_index.into())
    }

    pub fn style_for_name(&self, name: &str) -> Option<HighlightStyle> {
        self.capture_name_map.get(name).map(|highlight_index| {
            *self
                .highlights
                .get(*highlight_index)
                .expect("highlight index should be in bounds")
        })
    }

    pub fn get_capture_name(&self, index: impl Into<usize>) -> Option<&str> {
        let index = index.into();
        self.capture_name_map
            .iter()
            .find(|(_, value)| **value == index)
            .map(|(key, _)| key.as_ref())
    }

    pub fn highlight_id(&self, capture_name: &str) -> Option<u32> {
        self.capture_name_map
            .range::<str, _>((
                capture_name.split('.').next().map_or(
                    std::ops::Bound::Included(capture_name),
                    std::ops::Bound::Included,
                ),
                std::ops::Bound::Included(capture_name),
            ))
            .rfind(|(prefix, _)| {
                capture_name
                    .strip_prefix(*prefix)
                    .is_some_and(|remainder| remainder.is_empty() || remainder.starts_with('.'))
            })
            .and_then(|(_, index)| u32::try_from(*index).ok())
    }

    pub fn merge(base: Arc<Self>, user_syntax_styles: Vec<(String, HighlightStyle)>) -> Arc<Self> {
        if user_syntax_styles.is_empty() {
            return base;
        }

        let mut base = Arc::try_unwrap(base).unwrap_or_else(|base| (*base).clone());

        for (name, highlight) in user_syntax_styles {
            match base.capture_name_map.entry(name) {
                Entry::Occupied(entry) => {
                    let existing_highlight = base
                        .highlights
                        .get_mut(*entry.get())
                        .expect("highlight index should be in bounds");
                    existing_highlight.color = highlight.color.or(existing_highlight.color);
                    existing_highlight.font_weight =
                        highlight.font_weight.or(existing_highlight.font_weight);
                    existing_highlight.font_style =
                        highlight.font_style.or(existing_highlight.font_style);
                    existing_highlight.background_color = highlight
                        .background_color
                        .or(existing_highlight.background_color);
                    existing_highlight.underline =
                        highlight.underline.or(existing_highlight.underline);
                    existing_highlight.strikethrough =
                        highlight.strikethrough.or(existing_highlight.strikethrough);
                    existing_highlight.fade_out =
                        highlight.fade_out.or(existing_highlight.fade_out);
                }
                Entry::Vacant(vacant) => {
                    vacant.insert(base.highlights.len());
                    base.highlights.push(highlight);
                }
            }
        }

        Arc::new(base)
    }
}

#[cfg(test)]
mod tests {
    use gpui::FontStyle;

    use super::*;

    #[test]
    fn test_syntax_theme_merge() {
        let syntax_theme = SyntaxTheme::merge(
            Arc::new(SyntaxTheme::test_new([])),
            vec![
                (
                    "foo".to_string(),
                    HighlightStyle {
                        color: Some(gpui::red()),
                        ..Default::default()
                    },
                ),
                (
                    "foo.bar".to_string(),
                    HighlightStyle {
                        color: Some(gpui::green()),
                        ..Default::default()
                    },
                ),
            ],
        );
        assert_eq!(
            syntax_theme,
            Arc::new(SyntaxTheme::test_new([
                ("foo", gpui::red()),
                ("foo.bar", gpui::green())
            ]))
        );

        let syntax_theme = SyntaxTheme::merge(
            Arc::new(SyntaxTheme::test_new([
                ("foo", gpui::blue()),
                ("foo.bar", gpui::red()),
            ])),
            Vec::new(),
        );
        assert_eq!(
            syntax_theme,
            Arc::new(SyntaxTheme::test_new([
                ("foo", gpui::blue()),
                ("foo.bar", gpui::red())
            ]))
        );

        let syntax_theme = SyntaxTheme::merge(
            Arc::new(SyntaxTheme::test_new([
                ("foo", gpui::red()),
                ("foo.bar", gpui::green()),
            ])),
            vec![(
                "foo.bar".to_string(),
                HighlightStyle {
                    color: Some(gpui::yellow()),
                    ..Default::default()
                },
            )],
        );
        assert_eq!(
            syntax_theme,
            Arc::new(SyntaxTheme::test_new([
                ("foo", gpui::red()),
                ("foo.bar", gpui::yellow())
            ]))
        );

        let syntax_theme = SyntaxTheme::merge(
            Arc::new(SyntaxTheme::test_new([
                ("foo", gpui::red()),
                ("foo.bar", gpui::green()),
            ])),
            vec![(
                "foo.bar".to_string(),
                HighlightStyle {
                    font_style: Some(FontStyle::Italic),
                    ..Default::default()
                },
            )],
        );
        assert_eq!(
            syntax_theme,
            Arc::new(SyntaxTheme::test_new_styles([
                (
                    "foo",
                    HighlightStyle {
                        color: Some(gpui::red()),
                        ..Default::default()
                    }
                ),
                (
                    "foo.bar",
                    HighlightStyle {
                        color: Some(gpui::green()),
                        font_style: Some(FontStyle::Italic),
                        ..Default::default()
                    }
                )
            ]))
        );
    }
}
