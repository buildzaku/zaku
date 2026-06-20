mod components;
mod graphic;
mod icon;
pub mod prelude;
mod styles;
pub mod traits;
pub mod utils;

pub use components::*;
pub use graphic::*;
pub use icon::*;
pub use icons::IconName;
pub use styles::*;
pub use traits::*;

use std::time::Duration;

pub const TOOLTIP_SHOW_DELAY: Duration = Duration::from_millis(1200);
