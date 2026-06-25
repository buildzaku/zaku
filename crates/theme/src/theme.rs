mod fallback;
mod schema;
mod settings;
mod styles;

pub use fallback::apply_status_color_defaults;
pub use schema::*;
pub use settings::*;
pub use styles::*;

use gpui::{
    App, AssetSource, BorrowAppContext, Global, Hsla, Pixels, SharedString, WindowAppearance,
    WindowBackgroundAppearance,
};
use palette::{FromColor, Hsl, Okhsl};
use parking_lot::RwLock;
use serde::Deserialize;
use std::{collections::HashMap, fmt, sync::Arc};

pub(crate) const DEFAULT_LIGHT_THEME: &str = "Zaku Light";
pub(crate) const DEFAULT_DARK_THEME: &str = "Zaku Dark";
pub const CLIENT_SIDE_DECORATION_ROUNDING: Pixels = gpui::px(10.0);
pub const CLIENT_SIDE_DECORATION_SHADOW: Pixels = gpui::px(10.0);

pub fn default_theme(appearance: Appearance) -> &'static str {
    match appearance {
        Appearance::Light => DEFAULT_LIGHT_THEME,
        Appearance::Dark => DEFAULT_DARK_THEME,
    }
}

#[derive(Debug, PartialEq, Clone, Copy, Deserialize)]
pub enum Appearance {
    Light,
    Dark,
}

impl From<WindowAppearance> for Appearance {
    fn from(value: WindowAppearance) -> Self {
        match value {
            WindowAppearance::Dark | WindowAppearance::VibrantDark => Self::Dark,
            WindowAppearance::Light | WindowAppearance::VibrantLight => Self::Light,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct SystemAppearance(pub Appearance);

impl SystemAppearance {
    pub fn init(cx: &mut App) {
        *cx.default_global::<GlobalSystemAppearance>() =
            GlobalSystemAppearance(SystemAppearance(cx.window_appearance().into()));
    }

    pub fn global(cx: &App) -> Self {
        cx.global::<GlobalSystemAppearance>().0
    }

    pub fn global_mut(cx: &mut App) -> &mut Self {
        &mut cx.global_mut::<GlobalSystemAppearance>().0
    }
}

impl Default for SystemAppearance {
    fn default() -> Self {
        Self(Appearance::Dark)
    }
}

#[derive(Default)]
struct GlobalSystemAppearance(SystemAppearance);

impl Global for GlobalSystemAppearance {}

pub trait ActiveTheme {
    fn theme(&self) -> &Arc<Theme>;
}

impl ActiveTheme for App {
    fn theme(&self) -> &Arc<Theme> {
        GlobalTheme::theme(self)
    }
}

pub enum LoadThemes {
    JustBase,
    All(Box<dyn AssetSource>),
}

pub fn init(themes_to_load: LoadThemes, cx: &mut App) {
    SystemAppearance::init(cx);

    let assets = match themes_to_load {
        LoadThemes::JustBase => Box::new(()) as Box<dyn AssetSource>,
        LoadThemes::All(assets) => assets,
    };

    ThemeRegistry::set_global(assets, cx);

    let registry = ThemeRegistry::global(cx);
    let theme = registry
        .get(DEFAULT_DARK_THEME)
        .unwrap_or_else(|_| Arc::new(fallback::fallback_dark_theme()));
    cx.set_global(GlobalTheme::new(theme));
}

#[derive(Clone, Debug, PartialEq)]
pub struct Theme {
    pub id: String,
    pub name: SharedString,
    pub appearance: Appearance,
    pub styles: ThemeStyles,
}

impl Theme {
    pub fn colors(&self) -> &ThemeColors {
        &self.styles.colors
    }

    pub fn syntax(&self) -> &Arc<SyntaxTheme> {
        &self.styles.syntax
    }

    pub fn appearance(&self) -> Appearance {
        self.appearance
    }

    pub fn window_background_appearance(&self) -> WindowBackgroundAppearance {
        self.styles.window_background_appearance
    }

    pub fn status(&self) -> &StatusColors {
        &self.styles.status
    }

    pub fn darken(&self, color: Hsla, light_amount: f32, dark_amount: f32) -> Hsla {
        let amount = match self.appearance {
            Appearance::Light => light_amount,
            Appearance::Dark => dark_amount,
        };
        let mut okhsl = Okhsl::from_color(Hsl::new_srgb(color.h * 360.0, color.s, color.l));
        okhsl.lightness = (okhsl.lightness - amount).max(0.0);
        let hsla: Hsl = Hsl::from_color(okhsl);

        gpui::hsla(
            hsla.hue.into_positive_degrees() / 360.0,
            hsla.saturation,
            hsla.lightness,
            color.a,
        )
    }
}

pub struct ThemeFamily {
    pub id: String,
    pub name: SharedString,
    pub themes: Vec<Theme>,
}

#[derive(Debug, Clone)]
pub struct ThemeNotFoundError(pub SharedString);

impl fmt::Display for ThemeNotFoundError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "theme not found: {}", self.0)
    }
}

impl std::error::Error for ThemeNotFoundError {}

#[derive(Default)]
struct ThemeRegistryState {
    themes: HashMap<SharedString, Arc<Theme>>,
}

pub struct ThemeRegistry {
    state: RwLock<ThemeRegistryState>,
    assets: Box<dyn AssetSource>,
}

impl ThemeRegistry {
    pub fn global(cx: &App) -> Arc<Self> {
        cx.global::<GlobalThemeRegistry>().0.clone()
    }

    pub(crate) fn set_global(assets: Box<dyn AssetSource>, cx: &mut App) {
        cx.set_global(GlobalThemeRegistry(Arc::new(ThemeRegistry::new(assets))));
    }

    pub fn new(assets: Box<dyn AssetSource>) -> Self {
        let registry = Self {
            state: RwLock::new(ThemeRegistryState {
                themes: HashMap::default(),
            }),
            assets,
        };

        registry.insert_theme_families([fallback::zaku_default_themes()]);

        registry
    }

    pub fn assets(&self) -> &dyn AssetSource {
        self.assets.as_ref()
    }

    pub fn get(&self, name: &str) -> Result<Arc<Theme>, ThemeNotFoundError> {
        self.state
            .read()
            .themes
            .get(name)
            .cloned()
            .ok_or_else(|| ThemeNotFoundError(name.to_string().into()))
    }

    pub fn insert_theme_families(&self, families: impl IntoIterator<Item = ThemeFamily>) {
        for family in families {
            self.insert_themes(family.themes);
        }
    }

    pub fn insert_themes(&self, themes: impl IntoIterator<Item = Theme>) {
        let mut state = self.state.write();
        for theme in themes {
            state.themes.insert(theme.name.clone(), Arc::new(theme));
        }
    }
}

struct GlobalThemeRegistry(Arc<ThemeRegistry>);

impl Global for GlobalThemeRegistry {}

pub struct GlobalTheme {
    theme: Arc<Theme>,
}

impl GlobalTheme {
    pub fn new(theme: Arc<Theme>) -> Self {
        Self { theme }
    }

    pub fn update_theme(cx: &mut App, theme: Arc<Theme>) {
        cx.update_global::<Self, _>(|this, _| this.theme = theme);
    }

    pub fn theme(cx: &App) -> &Arc<Theme> {
        &cx.global::<Self>().theme
    }
}

impl Global for GlobalTheme {}
