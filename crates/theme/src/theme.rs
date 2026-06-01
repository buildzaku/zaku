mod fallback;
mod settings;

use anyhow::{Context, anyhow};
use gpui::{
    App, AssetSource, BorrowAppContext, Global, Hsla, Rgba, SharedString, WindowAppearance,
    WindowBackgroundAppearance,
};
use palette::{FromColor, Hsl, Okhsl};
use parking_lot::RwLock;
use serde::Deserialize;
use std::{collections::HashMap, sync::Arc};
use uuid::Uuid;

pub use settings::*;

pub(crate) const DEFAULT_LIGHT_THEME: &str = "Zaku Light";
pub(crate) const DEFAULT_DARK_THEME: &str = "Zaku Dark";

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
        log::error!("Failed to load bundled themes: {error:?}");
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
    pub fn colors(&self) -> &ThemeColors {
        &self.styles.colors
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

#[derive(Clone, Debug, PartialEq)]
pub struct ThemeStyles {
    pub window_background_appearance: WindowBackgroundAppearance,
    pub colors: ThemeColors,
    pub status: StatusColors,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ThemeColors {
    pub background: Hsla,
    pub surface_background: Hsla,
    pub elevated_surface_background: Hsla,
    pub panel_background: Hsla,
    pub panel_indent_guide: Hsla,
    pub panel_indent_guide_hover: Hsla,
    pub panel_indent_guide_active: Hsla,

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
    pub drop_target_background: Hsla,
    pub drop_target_border: Hsla,

    pub ghost_element_background: Hsla,
    pub ghost_element_hover: Hsla,
    pub ghost_element_active: Hsla,
    pub ghost_element_selected: Hsla,
    pub ghost_element_disabled: Hsla,

    pub title_bar_background: Hsla,
    pub title_bar_inactive_background: Hsla,
    pub status_bar_background: Hsla,
    pub panel_tab_bar_background: Hsla,
    pub panel_tab_inactive_background: Hsla,
    pub panel_tab_active_background: Hsla,
    pub panel_tab_inactive_foreground: Hsla,
    pub panel_tab_active_foreground: Hsla,
    pub tab_bar_background: Hsla,
    pub tab_inactive_background: Hsla,
    pub tab_active_background: Hsla,

    pub editor_background: Hsla,
    pub editor_foreground: Hsla,
    pub editor_active_line_background: Hsla,

    pub scrollbar_track_background: Hsla,
    pub scrollbar_track_border: Hsla,
    pub scrollbar_thumb_background: Hsla,
    pub scrollbar_thumb_hover_background: Hsla,
    pub scrollbar_thumb_active_background: Hsla,
    pub scrollbar_thumb_border: Hsla,
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
    #[serde(rename = "panel.indent_guide")]
    pub panel_indent_guide: Option<String>,
    #[serde(rename = "panel.indent_guide_hover")]
    pub panel_indent_guide_hover: Option<String>,
    #[serde(rename = "panel.indent_guide_active")]
    pub panel_indent_guide_active: Option<String>,

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
    #[serde(rename = "drop_target.background")]
    pub drop_target_background: Option<String>,
    #[serde(rename = "drop_target.border")]
    pub drop_target_border: Option<String>,

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

    #[serde(rename = "title_bar.background")]
    pub title_bar_background: Option<String>,
    #[serde(rename = "title_bar.inactive_background")]
    pub title_bar_inactive_background: Option<String>,
    #[serde(rename = "status_bar.background")]
    pub status_bar_background: Option<String>,
    #[serde(rename = "panel.tab_bar.background")]
    pub panel_tab_bar_background: Option<String>,
    #[serde(rename = "panel.tab.inactive_background")]
    pub panel_tab_inactive_background: Option<String>,
    #[serde(rename = "panel.tab.active_background")]
    pub panel_tab_active_background: Option<String>,
    #[serde(rename = "panel.tab.inactive_foreground")]
    pub panel_tab_inactive_foreground: Option<String>,
    #[serde(rename = "panel.tab.active_foreground")]
    pub panel_tab_active_foreground: Option<String>,
    #[serde(rename = "tab_bar.background")]
    pub tab_bar_background: Option<String>,
    #[serde(rename = "tab.inactive_background")]
    pub tab_inactive_background: Option<String>,
    #[serde(rename = "tab.active_background")]
    pub tab_active_background: Option<String>,

    #[serde(rename = "editor.background")]
    pub editor_background: Option<String>,
    #[serde(rename = "editor.foreground")]
    pub editor_foreground: Option<String>,
    #[serde(rename = "editor.active_line_background")]
    pub editor_active_line_background: Option<String>,

    #[serde(rename = "scrollbar.track.background")]
    pub scrollbar_track_background: Option<String>,
    #[serde(rename = "scrollbar.track.border")]
    pub scrollbar_track_border: Option<String>,
    #[serde(rename = "scrollbar.thumb.background")]
    pub scrollbar_thumb_background: Option<String>,
    #[serde(rename = "scrollbar.thumb.hover_background")]
    pub scrollbar_thumb_hover_background: Option<String>,
    #[serde(rename = "scrollbar.thumb.active_background")]
    pub scrollbar_thumb_active_background: Option<String>,
    #[serde(rename = "scrollbar.thumb.border")]
    pub scrollbar_thumb_border: Option<String>,

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
    fn to_styles(&self) -> anyhow::Result<ThemeStyles> {
        let colors = ThemeColors {
            background: parse_color("background", self.background.as_deref())?,
            surface_background: parse_color(
                "surface.background",
                self.surface_background.as_deref(),
            )?,
            elevated_surface_background: parse_color(
                "elevated_surface.background",
                self.elevated_surface_background.as_deref(),
            )?,
            panel_background: parse_color("panel.background", self.panel_background.as_deref())?,
            panel_indent_guide: parse_color(
                "panel.indent_guide",
                self.panel_indent_guide.as_deref(),
            )?,
            panel_indent_guide_hover: parse_color(
                "panel.indent_guide_hover",
                self.panel_indent_guide_hover.as_deref(),
            )?,
            panel_indent_guide_active: parse_color(
                "panel.indent_guide_active",
                self.panel_indent_guide_active.as_deref(),
            )?,
            border: parse_color("border", self.border.as_deref())?,
            border_variant: parse_color("border.variant", self.border_variant.as_deref())?,
            border_focused: parse_color("border.focused", self.border_focused.as_deref())?,
            border_disabled: parse_color("border.disabled", self.border_disabled.as_deref())?,
            text: parse_color("text", self.text.as_deref())?,
            text_muted: parse_color("text.muted", self.text_muted.as_deref())?,
            text_placeholder: parse_color("text.placeholder", self.text_placeholder.as_deref())?,
            text_disabled: parse_color("text.disabled", self.text_disabled.as_deref())?,
            text_accent: parse_color("text.accent", self.text_accent.as_deref())?,
            icon: parse_color("icon", self.icon.as_deref())?,
            icon_muted: parse_color("icon.muted", self.icon_muted.as_deref())?,
            icon_disabled: parse_color("icon.disabled", self.icon_disabled.as_deref())?,
            icon_accent: parse_color("icon.accent", self.icon_accent.as_deref())?,
            element_background: parse_color(
                "element.background",
                self.element_background.as_deref(),
            )?,
            element_hover: parse_color("element.hover", self.element_hover.as_deref())?,
            element_active: parse_color("element.active", self.element_active.as_deref())?,
            element_selected: parse_color("element.selected", self.element_selected.as_deref())?,
            element_selection_background: parse_color(
                "element.selection_background",
                self.element_selection_background.as_deref(),
            )?,
            element_disabled: parse_color("element.disabled", self.element_disabled.as_deref())?,
            drop_target_background: parse_color(
                "drop_target.background",
                self.drop_target_background.as_deref(),
            )?,
            drop_target_border: parse_color(
                "drop_target.border",
                self.drop_target_border.as_deref(),
            )?,
            ghost_element_background: parse_color(
                "ghost_element.background",
                self.ghost_element_background.as_deref(),
            )?,
            ghost_element_hover: parse_color(
                "ghost_element.hover",
                self.ghost_element_hover.as_deref(),
            )?,
            ghost_element_active: parse_color(
                "ghost_element.active",
                self.ghost_element_active.as_deref(),
            )?,
            ghost_element_selected: parse_color(
                "ghost_element.selected",
                self.ghost_element_selected.as_deref(),
            )?,
            ghost_element_disabled: parse_color(
                "ghost_element.disabled",
                self.ghost_element_disabled.as_deref(),
            )?,
            title_bar_background: parse_color(
                "title_bar.background",
                self.title_bar_background.as_deref(),
            )?,
            title_bar_inactive_background: parse_color(
                "title_bar.inactive_background",
                self.title_bar_inactive_background.as_deref(),
            )?,
            status_bar_background: parse_color(
                "status_bar.background",
                self.status_bar_background.as_deref(),
            )?,
            panel_tab_bar_background: parse_color(
                "panel.tab_bar.background",
                self.panel_tab_bar_background.as_deref(),
            )?,
            panel_tab_inactive_background: parse_color(
                "panel.tab.inactive_background",
                self.panel_tab_inactive_background.as_deref(),
            )?,
            panel_tab_active_background: parse_color(
                "panel.tab.active_background",
                self.panel_tab_active_background.as_deref(),
            )?,
            panel_tab_inactive_foreground: parse_color(
                "panel.tab.inactive_foreground",
                self.panel_tab_inactive_foreground.as_deref(),
            )?,
            panel_tab_active_foreground: parse_color(
                "panel.tab.active_foreground",
                self.panel_tab_active_foreground.as_deref(),
            )?,
            tab_bar_background: parse_color(
                "tab_bar.background",
                self.tab_bar_background.as_deref(),
            )?,
            tab_inactive_background: parse_color(
                "tab.inactive_background",
                self.tab_inactive_background.as_deref(),
            )?,
            tab_active_background: parse_color(
                "tab.active_background",
                self.tab_active_background.as_deref(),
            )?,
            editor_background: parse_color("editor.background", self.editor_background.as_deref())?,
            editor_foreground: parse_color("editor.foreground", self.editor_foreground.as_deref())?,
            editor_active_line_background: parse_color(
                "editor.active_line_background",
                self.editor_active_line_background.as_deref(),
            )?,
            scrollbar_track_background: parse_color(
                "scrollbar.track.background",
                self.scrollbar_track_background.as_deref(),
            )?,
            scrollbar_track_border: parse_color(
                "scrollbar.track.border",
                self.scrollbar_track_border.as_deref(),
            )?,
            scrollbar_thumb_background: parse_color(
                "scrollbar.thumb.background",
                self.scrollbar_thumb_background.as_deref(),
            )?,
            scrollbar_thumb_hover_background: parse_color(
                "scrollbar.thumb.hover_background",
                self.scrollbar_thumb_hover_background.as_deref(),
            )?,
            scrollbar_thumb_active_background: parse_color(
                "scrollbar.thumb.active_background",
                self.scrollbar_thumb_active_background.as_deref(),
            )?,
            scrollbar_thumb_border: parse_color(
                "scrollbar.thumb.border",
                self.scrollbar_thumb_border.as_deref(),
            )?,
        };

        let status = StatusColors {
            conflict: parse_color("conflict", self.conflict.as_deref())?,
            conflict_background: parse_color(
                "conflict.background",
                self.conflict_background.as_deref(),
            )?,
            conflict_border: parse_color("conflict.border", self.conflict_border.as_deref())?,

            created: parse_color("created", self.created.as_deref())?,
            created_background: parse_color(
                "created.background",
                self.created_background.as_deref(),
            )?,
            created_border: parse_color("created.border", self.created_border.as_deref())?,

            deleted: parse_color("deleted", self.deleted.as_deref())?,
            deleted_background: parse_color(
                "deleted.background",
                self.deleted_background.as_deref(),
            )?,
            deleted_border: parse_color("deleted.border", self.deleted_border.as_deref())?,

            error: parse_color("error", self.error.as_deref())?,
            error_background: parse_color("error.background", self.error_background.as_deref())?,
            error_border: parse_color("error.border", self.error_border.as_deref())?,

            hidden: parse_color("hidden", self.hidden.as_deref())?,
            hidden_background: parse_color("hidden.background", self.hidden_background.as_deref())?,
            hidden_border: parse_color("hidden.border", self.hidden_border.as_deref())?,

            hint: parse_color("hint", self.hint.as_deref())?,
            hint_background: parse_color("hint.background", self.hint_background.as_deref())?,
            hint_border: parse_color("hint.border", self.hint_border.as_deref())?,

            ignored: parse_color("ignored", self.ignored.as_deref())?,
            ignored_background: parse_color(
                "ignored.background",
                self.ignored_background.as_deref(),
            )?,
            ignored_border: parse_color("ignored.border", self.ignored_border.as_deref())?,

            info: parse_color("info", self.info.as_deref())?,
            info_background: parse_color("info.background", self.info_background.as_deref())?,
            info_border: parse_color("info.border", self.info_border.as_deref())?,

            modified: parse_color("modified", self.modified.as_deref())?,
            modified_background: parse_color(
                "modified.background",
                self.modified_background.as_deref(),
            )?,
            modified_border: parse_color("modified.border", self.modified_border.as_deref())?,

            renamed: parse_color("renamed", self.renamed.as_deref())?,
            renamed_background: parse_color(
                "renamed.background",
                self.renamed_background.as_deref(),
            )?,
            renamed_border: parse_color("renamed.border", self.renamed_border.as_deref())?,

            success: parse_color("success", self.success.as_deref())?,
            success_background: parse_color(
                "success.background",
                self.success_background.as_deref(),
            )?,
            success_border: parse_color("success.border", self.success_border.as_deref())?,

            unreachable: parse_color("unreachable", self.unreachable.as_deref())?,
            unreachable_background: parse_color(
                "unreachable.background",
                self.unreachable_background.as_deref(),
            )?,
            unreachable_border: parse_color(
                "unreachable.border",
                self.unreachable_border.as_deref(),
            )?,

            warning: parse_color("warning", self.warning.as_deref())?,
            warning_background: parse_color(
                "warning.background",
                self.warning_background.as_deref(),
            )?,
            warning_border: parse_color("warning.border", self.warning_border.as_deref())?,
        };

        Ok(ThemeStyles {
            window_background_appearance: WindowBackgroundAppearance::Opaque,
            colors,
            status,
        })
    }
}

fn parse_color(field: &'static str, value: Option<&str>) -> anyhow::Result<Hsla> {
    let Some(value) = value else {
        return Err(anyhow!("missing required theme style field: {field}"));
    };

    try_parse_color(value)
        .with_context(|| format!("invalid color value for theme style field {field:?}: {value:?}"))
}

fn try_parse_color(color: &str) -> anyhow::Result<Hsla> {
    let rgba = Rgba::try_from(color)?;
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

fn refine_theme_family(content: ThemeFamilyContent) -> anyhow::Result<ThemeFamily> {
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

fn refine_theme(content: &ThemeContent) -> anyhow::Result<Theme> {
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

        registry.insert_theme_family(fallback::zaku_default_themes());

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
        for theme in themes {
            state.themes.insert(theme.name.clone(), Arc::new(theme));
        }
    }

    pub fn load_bundled_themes(&self) -> anyhow::Result<()> {
        let theme_paths = self
            .assets
            .list("themes/")
            .context("listing theme assets")?
            .into_iter()
            .filter(|path| path.ends_with(".json"));

        for path in theme_paths {
            let Some(bytes) = self
                .assets
                .load(&path)
                .with_context(|| format!("loading {path:?}"))?
            else {
                continue;
            };

            let content: ThemeFamilyContent = match serde_json::from_slice(&bytes) {
                Ok(content) => content,
                Err(error) => {
                    log::error!("Failed to parse theme at path {path:?}: {error:?}");
                    continue;
                }
            };

            let family = match refine_theme_family(content) {
                Ok(family) => family,
                Err(error) => {
                    log::error!("Failed to refine theme at path {path:?}: {error:?}");
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
