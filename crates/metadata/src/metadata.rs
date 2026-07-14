use gpui::{App, Global};
use semver::Version;
use std::env;

pub const ZAKU_NAME: &str = env!("ZAKU_NAME");
pub const ZAKU_DESCRIPTION: &str = env!("ZAKU_DESCRIPTION");
pub const ZAKU_VERSION: &str = env!("CARGO_PKG_VERSION");
pub const ZAKU_IDENTIFIER: &str = env!("ZAKU_IDENTIFIER");
pub const ZAKU_REPOSITORY: &str = env!("CARGO_PKG_REPOSITORY");
pub const ZAKU_COMMIT_SHA: &str = env!("ZAKU_COMMIT_SHA");
pub const ZAKU_SERVER_URL: &str = match option_env!("ZAKU_SERVER_URL") {
    Some(url) => url,
    None => "https://zaku.dev",
};

struct GlobalAppVersion(Version);

impl Global for GlobalAppVersion {}

pub struct AppVersion;

impl AppVersion {
    pub fn load(package_version: &str) -> Version {
        if let Ok(from_env) = env::var("ZAKU_APP_VERSION") {
            from_env.parse().expect("invalid ZAKU_APP_VERSION")
        } else {
            package_version
                .parse()
                .expect("invalid version in Cargo.toml")
        }
    }

    pub fn global(cx: &App) -> Version {
        cx.global::<GlobalAppVersion>().0.clone()
    }
}

pub fn init(cx: &mut App) {
    cx.set_global(GlobalAppVersion(AppVersion::load(ZAKU_VERSION)));
}

pub fn init_test(app_version: Version, cx: &mut App) {
    cx.set_global(GlobalAppVersion(app_version));
}
