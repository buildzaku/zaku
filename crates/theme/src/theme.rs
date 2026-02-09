mod fallback;
mod settings;

use anyhow::{Context, Result, anyhow};
use gpui::{App, AssetSource, BorrowAppContext, Global, Hsla, SharedString, WindowAppearance};
use palette::FromColor;
use parking_lot::RwLock;
use serde::Deserialize;
use std::{collections::HashMap, sync::Arc};
use uuid::Uuid;

pub use settings::*;

pub(crate) const DEFAULT_LIGHT_THEME: &str = "Comet Light";
pub(crate) const DEFAULT_DARK_THEME: &str = "Comet Dark";

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

impl Default for SystemAppearance {
    fn default() -> Self {
        Self(Appearance::Dark)
    }
}

#[derive(Default)]
struct GlobalSystemAppearance(SystemAppearance);

impl Global for GlobalSystemAppearance {}

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
    ThemeSettings::init(cx);

    let (assets, load_bundled_themes) = match themes_to_load {
        LoadThemes::JustBase => (Box::new(()) as Box<dyn AssetSource>, false),
        LoadThemes::All(assets) => (assets, true),
    };

    ThemeRegistry::set_global(assets, cx);

    if load_bundled_themes && let Err(error) = ThemeRegistry::global(cx).load_bundled_themes() {
        eprintln!("failed to load bundled themes: {error:?}");
    }

    let theme = GlobalTheme::configured_theme(cx);
    cx.set_global(GlobalTheme { theme });
}

#[derive(Clone, Debug, PartialEq)]
pub struct Theme {
    pub id: String,
    pub name: SharedString,
    pub appearance: Appearance,
    pub styles: ThemeStyles,
}

impl Theme {
    #[inline(always)]
    pub fn colors(&self) -> &ThemeColors {
        &self.styles.colors
    }

    #[inline(always)]
    pub fn status(&self) -> &StatusColors {
        &self.styles.status
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct ThemeStyles {
    pub colors: ThemeColors,
    pub status: StatusColors,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ThemeColors {
    pub background: Hsla,
    pub surface_background: Hsla,
    pub elevated_surface_background: Hsla,
    pub panel_background: Hsla,

    pub border: Hsla,
    pub border_variant: Hsla,
    pub border_focused: Hsla,
    pub border_disabled: Hsla,

    pub text: Hsla,
    pub text_muted: Hsla,
    pub text_placeholder: Hsla,
    pub text_disabled: Hsla,
    pub text_accent: Hsla,

    pub icon: Hsla,
    pub icon_muted: Hsla,
    pub icon_disabled: Hsla,
    pub icon_accent: Hsla,

    pub element_background: Hsla,
    pub element_hover: Hsla,
    pub element_active: Hsla,
    pub element_selected: Hsla,
    pub element_selection_background: Hsla,
    pub element_disabled: Hsla,

    pub ghost_element_background: Hsla,
    pub ghost_element_hover: Hsla,
    pub ghost_element_active: Hsla,
    pub ghost_element_selected: Hsla,
    pub ghost_element_disabled: Hsla,

    pub status_bar_background: Hsla,

    pub editor_background: Hsla,
    pub editor_foreground: Hsla,
}

#[derive(Clone, Debug, PartialEq)]
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

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AppearanceContent {
    Light,
    Dark,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ThemeFamilyContent {
    pub name: String,
    pub themes: Vec<ThemeContent>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ThemeContent {
    pub name: String,
    pub appearance: AppearanceContent,
    pub style: ThemeStyleContent,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct ThemeStyleContent {
    #[serde(rename = "background")]
    pub background: Option<String>,
    #[serde(rename = "surface.background")]
    pub surface_background: Option<String>,
    #[serde(rename = "elevated_surface.background")]
    pub elevated_surface_background: Option<String>,
    #[serde(rename = "panel.background")]
    pub panel_background: Option<String>,

    #[serde(rename = "border")]
    pub border: Option<String>,
    #[serde(rename = "border.variant")]
    pub border_variant: Option<String>,
    #[serde(rename = "border.focused")]
    pub border_focused: Option<String>,
    #[serde(rename = "border.disabled")]
    pub border_disabled: Option<String>,

    #[serde(rename = "text")]
    pub text: Option<String>,
    #[serde(rename = "text.muted")]
    pub text_muted: Option<String>,
    #[serde(rename = "text.placeholder")]
    pub text_placeholder: Option<String>,
    #[serde(rename = "text.disabled")]
    pub text_disabled: Option<String>,
    #[serde(rename = "text.accent")]
    pub text_accent: Option<String>,

    #[serde(rename = "icon")]
    pub icon: Option<String>,
    #[serde(rename = "icon.muted")]
    pub icon_muted: Option<String>,
    #[serde(rename = "icon.disabled")]
    pub icon_disabled: Option<String>,
    #[serde(rename = "icon.accent")]
    pub icon_accent: Option<String>,

    #[serde(rename = "element.background")]
    pub element_background: Option<String>,
    #[serde(rename = "element.hover")]
    pub element_hover: Option<String>,
    #[serde(rename = "element.active")]
    pub element_active: Option<String>,
    #[serde(rename = "element.selected")]
    pub element_selected: Option<String>,
    #[serde(rename = "element.selection_background")]
    pub element_selection_background: Option<String>,
    #[serde(rename = "element.disabled")]
    pub element_disabled: Option<String>,

    #[serde(rename = "ghost_element.background")]
    pub ghost_element_background: Option<String>,
    #[serde(rename = "ghost_element.hover")]
    pub ghost_element_hover: Option<String>,
    #[serde(rename = "ghost_element.active")]
    pub ghost_element_active: Option<String>,
    #[serde(rename = "ghost_element.selected")]
    pub ghost_element_selected: Option<String>,
    #[serde(rename = "ghost_element.disabled")]
    pub ghost_element_disabled: Option<String>,

    #[serde(rename = "status_bar.background")]
    pub status_bar_background: Option<String>,

    #[serde(rename = "editor.background")]
    pub editor_background: Option<String>,
    #[serde(rename = "editor.foreground")]
    pub editor_foreground: Option<String>,

    #[serde(rename = "conflict")]
    pub conflict: Option<String>,
    #[serde(rename = "conflict.background")]
    pub conflict_background: Option<String>,
    #[serde(rename = "conflict.border")]
    pub conflict_border: Option<String>,

    #[serde(rename = "created")]
    pub created: Option<String>,
    #[serde(rename = "created.background")]
    pub created_background: Option<String>,
    #[serde(rename = "created.border")]
    pub created_border: Option<String>,

    #[serde(rename = "deleted")]
    pub deleted: Option<String>,
    #[serde(rename = "deleted.background")]
    pub deleted_background: Option<String>,
    #[serde(rename = "deleted.border")]
    pub deleted_border: Option<String>,

    #[serde(rename = "error")]
    pub error: Option<String>,
    #[serde(rename = "error.background")]
    pub error_background: Option<String>,
    #[serde(rename = "error.border")]
    pub error_border: Option<String>,

    #[serde(rename = "hidden")]
    pub hidden: Option<String>,
    #[serde(rename = "hidden.background")]
    pub hidden_background: Option<String>,
    #[serde(rename = "hidden.border")]
    pub hidden_border: Option<String>,

    #[serde(rename = "hint")]
    pub hint: Option<String>,
    #[serde(rename = "hint.background")]
    pub hint_background: Option<String>,
    #[serde(rename = "hint.border")]
    pub hint_border: Option<String>,

    #[serde(rename = "ignored")]
    pub ignored: Option<String>,
    #[serde(rename = "ignored.background")]
    pub ignored_background: Option<String>,
    #[serde(rename = "ignored.border")]
    pub ignored_border: Option<String>,

    #[serde(rename = "info")]
    pub info: Option<String>,
    #[serde(rename = "info.background")]
    pub info_background: Option<String>,
    #[serde(rename = "info.border")]
    pub info_border: Option<String>,

    #[serde(rename = "modified")]
    pub modified: Option<String>,
    #[serde(rename = "modified.background")]
    pub modified_background: Option<String>,
    #[serde(rename = "modified.border")]
    pub modified_border: Option<String>,

    #[serde(rename = "renamed")]
    pub renamed: Option<String>,
    #[serde(rename = "renamed.background")]
    pub renamed_background: Option<String>,
    #[serde(rename = "renamed.border")]
    pub renamed_border: Option<String>,

    #[serde(rename = "success")]
    pub success: Option<String>,
    #[serde(rename = "success.background")]
    pub success_background: Option<String>,
    #[serde(rename = "success.border")]
    pub success_border: Option<String>,

    #[serde(rename = "unreachable")]
    pub unreachable: Option<String>,
    #[serde(rename = "unreachable.background")]
    pub unreachable_background: Option<String>,
    #[serde(rename = "unreachable.border")]
    pub unreachable_border: Option<String>,

    #[serde(rename = "warning")]
    pub warning: Option<String>,
    #[serde(rename = "warning.background")]
    pub warning_background: Option<String>,
    #[serde(rename = "warning.border")]
    pub warning_border: Option<String>,
}

impl ThemeStyleContent {
    fn to_styles(&self) -> Result<ThemeStyles> {
        let colors = ThemeColors {
            background: required_color("background", &self.background)?,
            surface_background: required_color("surface.background", &self.surface_background)?,
            elevated_surface_background: required_color(
                "elevated_surface.background",
                &self.elevated_surface_background,
            )?,
            panel_background: required_color("panel.background", &self.panel_background)?,
            border: required_color("border", &self.border)?,
            border_variant: required_color("border.variant", &self.border_variant)?,
            border_focused: required_color("border.focused", &self.border_focused)?,
            border_disabled: required_color("border.disabled", &self.border_disabled)?,
            text: required_color("text", &self.text)?,
            text_muted: required_color("text.muted", &self.text_muted)?,
            text_placeholder: required_color("text.placeholder", &self.text_placeholder)?,
            text_disabled: required_color("text.disabled", &self.text_disabled)?,
            text_accent: required_color("text.accent", &self.text_accent)?,
            icon: required_color("icon", &self.icon)?,
            icon_muted: required_color("icon.muted", &self.icon_muted)?,
            icon_disabled: required_color("icon.disabled", &self.icon_disabled)?,
            icon_accent: required_color("icon.accent", &self.icon_accent)?,
            element_background: required_color("element.background", &self.element_background)?,
            element_hover: required_color("element.hover", &self.element_hover)?,
            element_active: required_color("element.active", &self.element_active)?,
            element_selected: required_color("element.selected", &self.element_selected)?,
            element_selection_background: required_color(
                "element.selection_background",
                &self.element_selection_background,
            )?,
            element_disabled: required_color("element.disabled", &self.element_disabled)?,
            ghost_element_background: required_color(
                "ghost_element.background",
                &self.ghost_element_background,
            )?,
            ghost_element_hover: required_color("ghost_element.hover", &self.ghost_element_hover)?,
            ghost_element_active: required_color(
                "ghost_element.active",
                &self.ghost_element_active,
            )?,
            ghost_element_selected: required_color(
                "ghost_element.selected",
                &self.ghost_element_selected,
            )?,
            ghost_element_disabled: required_color(
                "ghost_element.disabled",
                &self.ghost_element_disabled,
            )?,
            status_bar_background: required_color(
                "status_bar.background",
                &self.status_bar_background,
            )?,
            editor_background: required_color("editor.background", &self.editor_background)?,
            editor_foreground: required_color("editor.foreground", &self.editor_foreground)?,
        };

        let status = StatusColors {
            conflict: required_color("conflict", &self.conflict)?,
            conflict_background: required_color("conflict.background", &self.conflict_background)?,
            conflict_border: required_color("conflict.border", &self.conflict_border)?,

            created: required_color("created", &self.created)?,
            created_background: required_color("created.background", &self.created_background)?,
            created_border: required_color("created.border", &self.created_border)?,

            deleted: required_color("deleted", &self.deleted)?,
            deleted_background: required_color("deleted.background", &self.deleted_background)?,
            deleted_border: required_color("deleted.border", &self.deleted_border)?,

            error: required_color("error", &self.error)?,
            error_background: required_color("error.background", &self.error_background)?,
            error_border: required_color("error.border", &self.error_border)?,

            hidden: required_color("hidden", &self.hidden)?,
            hidden_background: required_color("hidden.background", &self.hidden_background)?,
            hidden_border: required_color("hidden.border", &self.hidden_border)?,

            hint: required_color("hint", &self.hint)?,
            hint_background: required_color("hint.background", &self.hint_background)?,
            hint_border: required_color("hint.border", &self.hint_border)?,

            ignored: required_color("ignored", &self.ignored)?,
            ignored_background: required_color("ignored.background", &self.ignored_background)?,
            ignored_border: required_color("ignored.border", &self.ignored_border)?,

            info: required_color("info", &self.info)?,
            info_background: required_color("info.background", &self.info_background)?,
            info_border: required_color("info.border", &self.info_border)?,

            modified: required_color("modified", &self.modified)?,
            modified_background: required_color("modified.background", &self.modified_background)?,
            modified_border: required_color("modified.border", &self.modified_border)?,

            renamed: required_color("renamed", &self.renamed)?,
            renamed_background: required_color("renamed.background", &self.renamed_background)?,
            renamed_border: required_color("renamed.border", &self.renamed_border)?,

            success: required_color("success", &self.success)?,
            success_background: required_color("success.background", &self.success_background)?,
            success_border: required_color("success.border", &self.success_border)?,

            unreachable: required_color("unreachable", &self.unreachable)?,
            unreachable_background: required_color(
                "unreachable.background",
                &self.unreachable_background,
            )?,
            unreachable_border: required_color("unreachable.border", &self.unreachable_border)?,

            warning: required_color("warning", &self.warning)?,
            warning_background: required_color("warning.background", &self.warning_background)?,
            warning_border: required_color("warning.border", &self.warning_border)?,
        };

        Ok(ThemeStyles { colors, status })
    }
}

fn required_color(field: &'static str, value: &Option<String>) -> Result<Hsla> {
    let Some(value) = value.as_ref() else {
        return Err(anyhow!("missing required theme style field: {field}"));
    };

    try_parse_color(value)
        .with_context(|| format!("invalid color value for theme style field {field:?}: {value:?}"))
}

fn try_parse_color(color: &str) -> Result<Hsla> {
    let rgba = gpui::Rgba::try_from(color)?;
    let rgba = palette::rgb::Srgba::from_components((rgba.r, rgba.g, rgba.b, rgba.a));
    let hsla = palette::Hsla::from_color(rgba);

    Ok(gpui::hsla(
        hsla.hue.into_positive_degrees() / 360.0,
        hsla.saturation,
        hsla.lightness,
        hsla.alpha,
    ))
}

pub struct ThemeFamily {
    pub id: String,
    pub name: SharedString,
    pub themes: Vec<Theme>,
}

fn refine_theme_family(content: ThemeFamilyContent) -> Result<ThemeFamily> {
    let mut themes = Vec::with_capacity(content.themes.len());
    for theme in &content.themes {
        themes.push(refine_theme(theme)?);
    }

    Ok(ThemeFamily {
        id: Uuid::new_v4().to_string(),
        name: content.name.into(),
        themes,
    })
}

fn refine_theme(content: &ThemeContent) -> Result<Theme> {
    let appearance = match content.appearance {
        AppearanceContent::Light => Appearance::Light,
        AppearanceContent::Dark => Appearance::Dark,
    };

    Ok(Theme {
        id: Uuid::new_v4().to_string(),
        name: content.name.clone().into(),
        appearance,
        styles: content.style.to_styles()?,
    })
}

#[derive(Default)]
struct ThemeRegistryState {
    themes: HashMap<SharedString, Arc<Theme>>,
}

pub struct ThemeRegistry {
    state: RwLock<ThemeRegistryState>,
    assets: Box<dyn AssetSource>,
}

struct GlobalThemeRegistry(Arc<ThemeRegistry>);

impl Global for GlobalThemeRegistry {}

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

        registry.insert_theme_family(fallback::comet_default_themes());

        registry
    }

    pub fn get(&self, name: &str) -> Option<Arc<Theme>> {
        self.state.read().themes.get(name).cloned()
    }

    fn insert_theme_family(&self, family: ThemeFamily) {
        self.insert_themes(family.themes);
    }

    fn insert_themes(&self, themes: impl IntoIterator<Item = Theme>) {
        let mut state = self.state.write();
        for theme in themes.into_iter() {
            state.themes.insert(theme.name.clone(), Arc::new(theme));
        }
    }

    pub fn load_bundled_themes(&self) -> Result<()> {
        let theme_paths = self
            .assets
            .list("themes/")
            .context("listing theme assets")?
            .into_iter()
            .filter(|path| path.ends_with(".json"));

        for path in theme_paths {
            let bytes = match self
                .assets
                .load(&path)
                .with_context(|| format!("loading {path:?}"))?
            {
                Some(bytes) => bytes,
                None => continue,
            };

            let content: ThemeFamilyContent = match serde_json::from_slice(&bytes) {
                Ok(content) => content,
                Err(error) => {
                    eprintln!("failed to parse theme at path {path:?}: {error:?}");
                    continue;
                }
            };

            let family = match refine_theme_family(content) {
                Ok(family) => family,
                Err(error) => {
                    eprintln!("failed to refine theme at path {path:?}: {error:?}");
                    continue;
                }
            };

            self.insert_theme_family(family);
        }

        Ok(())
    }
}

pub struct GlobalTheme {
    theme: Arc<Theme>,
}

impl Global for GlobalTheme {}

impl GlobalTheme {
    fn configured_theme(cx: &mut App) -> Arc<Theme> {
        let registry = ThemeRegistry::global(cx);
        let system_appearance = SystemAppearance::global(cx).0;
        let theme_name = match system_appearance {
            Appearance::Light => DEFAULT_LIGHT_THEME,
            Appearance::Dark => DEFAULT_DARK_THEME,
        };

        registry
            .get(theme_name)
            .or_else(|| registry.get(DEFAULT_DARK_THEME))
            .unwrap_or_else(|| Arc::new(fallback::fallback_dark_theme()))
    }

    pub fn reload_theme(cx: &mut App) {
        let theme = Self::configured_theme(cx);
        cx.update_global::<Self, _>(|this, _| this.theme = theme);
        cx.refresh_windows();
    }

    pub fn theme(cx: &App) -> &Arc<Theme> {
        &cx.global::<Self>().theme
    }
}
