use gpui::{App, Global};
use std::env;

use app_version::AppVersion;

pub const ZAKU_NAME: &str = env!("ZAKU_NAME");
pub const ZAKU_DESCRIPTION: &str = env!("ZAKU_DESCRIPTION");
const ZAKU_VERSION: &str = env!("CARGO_PKG_VERSION");
pub const ZAKU_IDENTIFIER: &str = env!("ZAKU_IDENTIFIER");
pub const ZAKU_REPOSITORY: &str = env!("CARGO_PKG_REPOSITORY");
pub const ZAKU_BUILD_ID: Option<&str> = option_env!("ZAKU_BUILD_ID");
pub const ZAKU_COMMIT_SHA: &str = env!("ZAKU_COMMIT_SHA");
pub const ZAKU_SERVER_URL: &str = match option_env!("ZAKU_SERVER_URL") {
    Some(url) => url,
    None => "https://api.zaku.dev",
};

struct GlobalAppVersion(AppVersion);

impl Global for GlobalAppVersion {}

pub fn version(cx: &App) -> AppVersion {
    cx.global::<GlobalAppVersion>().0.clone()
}

pub fn init(cx: &mut App) {
    let version = if let Ok(from_env) = env::var("ZAKU_APP_VERSION") {
        from_env.parse().expect("invalid ZAKU_APP_VERSION")
    } else {
        ZAKU_VERSION.parse().expect("invalid version in Cargo.toml")
    };
    cx.set_global(GlobalAppVersion(version));
}

pub fn init_test(version: AppVersion, cx: &mut App) {
    cx.set_global(GlobalAppVersion(version));
}
