use gpui::{FontFeatures, FontStyle, FontWeight, Pixels, SharedString};
use std::sync::Arc;

use settings_content::{
    FontFamilyName, FontFeaturesContent, FontSize, FontStyleContent, FontWeightContent,
};

pub trait IntoGpui {
    type Output;

    fn into_gpui(self) -> Self::Output;
}

impl IntoGpui for FontStyleContent {
    type Output = FontStyle;

    fn into_gpui(self) -> Self::Output {
        match self {
            Self::Normal => FontStyle::Normal,
            Self::Italic => FontStyle::Italic,
            Self::Oblique => FontStyle::Oblique,
        }
    }
}

impl IntoGpui for FontWeightContent {
    type Output = FontWeight;

    fn into_gpui(self) -> Self::Output {
        FontWeight(self.0.clamp(100.0, 950.0))
    }
}

impl IntoGpui for FontSize {
    type Output = Pixels;

    fn into_gpui(self) -> Self::Output {
        gpui::px(self.0)
    }
}

impl IntoGpui for FontFamilyName {
    type Output = SharedString;

    fn into_gpui(self) -> Self::Output {
        SharedString::from(self.0)
    }
}

impl IntoGpui for FontFeaturesContent {
    type Output = FontFeatures;

    fn into_gpui(self) -> Self::Output {
        FontFeatures(Arc::new(self.0.into_iter().collect()))
    }
}

#[cfg(test)]
mod tests {
    use gpui::FontWeight;
    use settings_content::FontWeightContent;

    #[test]
    fn test_font_weight_content_constants_match_gpui() {
        assert_eq!(
            FontWeightContent::THIN,
            FontWeightContent(FontWeight::THIN.0)
        );
        assert_eq!(
            FontWeightContent::EXTRA_LIGHT,
            FontWeightContent(FontWeight::EXTRA_LIGHT.0)
        );
        assert_eq!(
            FontWeightContent::LIGHT,
            FontWeightContent(FontWeight::LIGHT.0)
        );
        assert_eq!(
            FontWeightContent::NORMAL,
            FontWeightContent(FontWeight::NORMAL.0)
        );
        assert_eq!(
            FontWeightContent::MEDIUM,
            FontWeightContent(FontWeight::MEDIUM.0)
        );
        assert_eq!(
            FontWeightContent::SEMIBOLD,
            FontWeightContent(FontWeight::SEMIBOLD.0)
        );
        assert_eq!(
            FontWeightContent::BOLD,
            FontWeightContent(FontWeight::BOLD.0)
        );
        assert_eq!(
            FontWeightContent::EXTRA_BOLD,
            FontWeightContent(FontWeight::EXTRA_BOLD.0)
        );
        assert_eq!(
            FontWeightContent::BLACK,
            FontWeightContent(FontWeight::BLACK.0)
        );
    }
}
