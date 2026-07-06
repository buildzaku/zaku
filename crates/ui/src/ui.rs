mod components;
mod icon;
mod styles;
mod svg;
pub mod traits;
pub mod utils;

pub use ::svg::{IconAsset, SvgAsset};
pub use components::*;
pub use icon::*;
pub use styles::*;
pub use svg::*;
pub use theme::ActiveTheme;
pub use traits::*;

use std::time::Duration;

pub const TOOLTIP_SHOW_DELAY: Duration = Duration::from_millis(1200);
