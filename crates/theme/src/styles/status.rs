use gpui::Hsla;
use refineable::Refineable;
use serde::Deserialize;

use crate::fallback;

#[derive(Debug, Clone, PartialEq, Refineable)]
#[refineable(Debug, Deserialize)]
pub struct StatusColors {
    pub conflict: Hsla,
    pub conflict_background: Hsla,
    pub conflict_border: Hsla,

    pub created: Hsla,
    pub created_background: Hsla,
    pub created_border: Hsla,

    pub deleted: Hsla,
    pub deleted_background: Hsla,
    pub deleted_border: Hsla,

    pub error: Hsla,
    pub error_background: Hsla,
    pub error_border: Hsla,

    pub hidden: Hsla,
    pub hidden_background: Hsla,
    pub hidden_border: Hsla,

    pub hint: Hsla,
    pub hint_background: Hsla,
    pub hint_border: Hsla,

    pub ignored: Hsla,
    pub ignored_background: Hsla,
    pub ignored_border: Hsla,

    pub info: Hsla,
    pub info_background: Hsla,
    pub info_border: Hsla,

    pub modified: Hsla,
    pub modified_background: Hsla,
    pub modified_border: Hsla,

    pub renamed: Hsla,
    pub renamed_background: Hsla,
    pub renamed_border: Hsla,

    pub success: Hsla,
    pub success_background: Hsla,
    pub success_border: Hsla,

    pub unreachable: Hsla,
    pub unreachable_background: Hsla,
    pub unreachable_border: Hsla,

    pub warning: Hsla,
    pub warning_background: Hsla,
    pub warning_border: Hsla,
}

impl StatusColors {
    pub fn dark() -> Self {
        fallback::fallback_dark_theme().styles.status
    }

    pub fn light() -> Self {
        fallback::fallback_light_theme().styles.status
    }
}
