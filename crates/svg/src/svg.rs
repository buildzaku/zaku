use gpui::{App, SharedString};
use serde::{Deserialize, Serialize};
use std::{path::Path, sync::Arc};
use strum::{EnumIter, EnumString, IntoStaticStr};

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, EnumIter, EnumString, IntoStaticStr,
)]
#[strum(serialize_all = "snake_case")]
#[serde(rename_all = "snake_case")]
pub enum SvgAsset {
    Zaku,
    ZakuLogo,
}

impl SvgAsset {
    pub fn path(&self) -> Arc<str> {
        let file_stem: &'static str = self.into();
        format!("svg/{file_stem}.svg").into()
    }

    pub fn aspect_ratio(self) -> f32 {
        match self {
            SvgAsset::Zaku => 70.0 / 32.0,
            SvgAsset::ZakuLogo => 1.0,
        }
    }
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, EnumIter, EnumString, IntoStaticStr,
)]
#[strum(serialize_all = "snake_case")]
#[serde(rename_all = "snake_case")]
pub enum IconAsset {
    ArrowUpRight,
    Backspace,
    CaretDown,
    CaretRight,
    CaretUpDown,
    Check,
    Close,
    Command,
    Control,
    Dash,
    File,
    FileLock,
    FileToml,
    FileTomlPlus,
    Folder,
    FolderClose,
    FolderOpen,
    FolderPlus,
    GitBranch,
    Info,
    LinuxClose,
    LinuxMaximize,
    LinuxMinimize,
    LinuxRestore,
    Menu,
    Minimize,
    Network,
    Option,
    Plus,
    Return,
    Shift,
    SquareDot,
    SquareMinus,
    SquarePlus,
    Trash,
    Tree,
    Warning,
    WarningCircle,
}

impl IconAsset {
    pub fn path(&self) -> Arc<str> {
        let file_stem: &'static str = self.into();
        format!("svg/icons/{file_stem}.svg").into()
    }
}

const FILE_SUFFIXES_BY_ICON_KEY: &[(&str, &[&str])] =
    &[("json", &["json", "jsonc"]), ("log", &["log"])];

const FILE_ICONS: &[(&str, &str)] = &[
    ("default", "svg/icons/file/toml.svg"),
    ("json", "svg/icons/file/code.svg"),
    ("log", "svg/icons/file/info.svg"),
];

#[derive(Debug)]
pub struct FileIcons;

impl FileIcons {
    pub fn get_icon(path: &Path, _: &App) -> Option<SharedString> {
        let get_icon_from_suffix = |suffix: &str| -> Option<SharedString> {
            icon_key_for_suffix(suffix).and_then(icon_for_type)
        };

        if let Some(mut typ) = path.file_name().and_then(|typ| typ.to_str()) {
            let maybe_path = get_icon_from_suffix(typ);
            if maybe_path.is_some() {
                return maybe_path;
            }

            while let Some((_, suffix)) = typ.split_once('.') {
                let maybe_path = get_icon_from_suffix(suffix);
                if maybe_path.is_some() {
                    return maybe_path;
                }
                typ = suffix;
            }
        }

        let extension = path.extension().and_then(|extension| extension.to_str());
        if let Some(extension) = extension {
            let maybe_path = get_icon_from_suffix(extension);
            if maybe_path.is_some() {
                return maybe_path;
            }
        }

        icon_for_type("default")
    }
}

fn icon_key_for_suffix(suffix: &str) -> Option<&'static str> {
    FILE_SUFFIXES_BY_ICON_KEY
        .iter()
        .find_map(|(icon_key, suffixes)| suffixes.contains(&suffix).then_some(*icon_key))
}

fn icon_for_type(typ: &str) -> Option<SharedString> {
    FILE_ICONS
        .iter()
        .find_map(|(icon_type, path)| (*icon_type == typ).then_some((*path).into()))
}
