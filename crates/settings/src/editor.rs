use serde::Deserialize;
use settings_macros::{MergeFrom, with_fallible_options};

#[with_fallible_options]
#[derive(Clone, Default, Deserialize, MergeFrom)]
pub struct GutterContent {
    pub line_numbers: Option<bool>,
    pub min_line_number_digits: Option<usize>,
}
