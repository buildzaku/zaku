mod schema;
mod settings;
mod styles;

pub use schema::{
    AppearanceContent, FontStyleContent, FontWeightContent, HighlightStyleContent,
    ThemeColorsContent, ThemeContent, ThemeFamilyContent, ThemeStyleContent, parse_color,
};
pub use settings::*;
pub use styles::*;

use gpui::{
    App, AssetSource, BorrowAppContext, Global, Hsla, Pixels, SharedString, WindowAppearance,
};
use palette::{FromColor, Hsl, Okhsl};
use parking_lot::RwLock;
use serde::Deserialize;
use std::{
    borrow::Cow,
    collections::HashMap,
    fmt,
    sync::{Arc, LazyLock},
};

pub(crate) const DEFAULT_LIGHT_THEME: &str = "Zaku Light";
pub(crate) const DEFAULT_DARK_THEME: &str = "Zaku Dark";
pub const CLIENT_SIDE_DECORATION_ROUNDING: Pixels = gpui::px(10.0);
pub const CLIENT_SIDE_DECORATION_SHADOW: Pixels = gpui::px(10.0);

const ZAKU_THEME_FAMILY: &[u8] = include_bytes!("../../../assets/themes/zaku/zaku.json");
const ZAKU_DARK_THEME: &[u8] = include_bytes!("../../../assets/themes/zaku/zaku-dark.json");
const ZAKU_LIGHT_THEME: &[u8] = include_bytes!("../../../assets/themes/zaku/zaku-light.json");

static ZAKU_DEFAULT_THEMES: LazyLock<ZakuDefaultThemes> = LazyLock::new(|| {
    let family = ThemeFamily::from_bytes(ZAKU_THEME_FAMILY, |path| match path {
        "zaku-dark.json" => Ok(Some(Cow::Borrowed(ZAKU_DARK_THEME))),
        "zaku-light.json" => Ok(Some(Cow::Borrowed(ZAKU_LIGHT_THEME))),
        _ => Ok(None),
    })
    .expect("bundled Zaku theme should parse");
    let dark_theme = family
        .themes
        .iter()
        .find(|theme| theme.name.as_ref() == DEFAULT_DARK_THEME)
        .cloned()
        .expect("bundled Zaku theme should include Zaku Dark");
    let light_theme = family
        .themes
        .iter()
        .find(|theme| theme.name.as_ref() == DEFAULT_LIGHT_THEME)
        .cloned()
        .expect("bundled Zaku theme should include Zaku Light");

    ZakuDefaultThemes {
        family,
        dark_theme,
        light_theme,
    }
});

struct ZakuDefaultThemes {
    family: ThemeFamily,
    dark_theme: Theme,
    light_theme: Theme,
}

pub fn default_theme(appearance: Appearance) -> &'static str {
    match appearance {
        Appearance::Light => DEFAULT_LIGHT_THEME,
        Appearance::Dark => DEFAULT_DARK_THEME,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Deserialize)]
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
        .unwrap_or_else(|_| Arc::new(Theme::default_dark()));
    cx.set_global(GlobalTheme::new(theme));
}

#[derive(Debug, Clone, PartialEq)]
pub struct Theme {
    pub id: String,
    pub name: SharedString,
    pub appearance: Appearance,
    pub styles: ThemeStyles,
}

impl Theme {
    pub(crate) fn default_dark() -> Self {
        ZAKU_DEFAULT_THEMES.dark_theme.clone()
    }

    pub(crate) fn default_light() -> Self {
        ZAKU_DEFAULT_THEMES.light_theme.clone()
    }

    pub fn colors(&self) -> &ThemeColors {
        &self.styles.colors
    }

    pub fn syntax(&self) -> &Arc<SyntaxTheme> {
        &self.styles.syntax
    }

    pub fn appearance(&self) -> Appearance {
        self.appearance
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

#[derive(Debug, Clone)]
pub struct ThemeFamily {
    pub id: String,
    pub name: SharedString,
    pub themes: Vec<Theme>,
}

impl ThemeFamily {
    pub fn from_bytes(
        bytes: &[u8],
        loader: impl FnMut(&str) -> anyhow::Result<Option<Cow<'static, [u8]>>>,
    ) -> anyhow::Result<Self> {
        let content: ThemeFamilyContent = serde_json::from_slice(bytes)?;
        content.into_theme_family(loader)
    }

    pub(crate) fn zaku_default() -> Self {
        ZAKU_DEFAULT_THEMES.family.clone()
    }
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

        registry.insert_theme_families([ThemeFamily::zaku_default()]);

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
