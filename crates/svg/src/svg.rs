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

#[derive(Debug)]
pub struct FileIcon;

impl FileIcon {
    const EXTENSIONS_BY_KIND: &[(&str, &[&str])] = &[
        ("json", &["json", "jsonc"]),
        ("log", &["log"]),
        ("toml", &["toml"]),
    ];

    const PATHS_BY_KIND: &[(&str, &str)] = &[
        ("json", "svg/icons/file/code.svg"),
        ("log", "svg/icons/file/info.svg"),
        ("toml", "svg/icons/file/toml.svg"),
    ];

    pub fn for_path(path: &Path) -> Arc<str> {
        let icon_path_for_extension = |extension: &str| -> Option<Arc<str>> {
            Self::EXTENSIONS_BY_KIND
                .iter()
                .find_map(|(kind, extensions)| extensions.contains(&extension).then_some(*kind))
                .and_then(|kind| {
                    Self::PATHS_BY_KIND.iter().find_map(|(path_kind, path)| {
                        (*path_kind == kind).then_some(Arc::from(*path))
                    })
                })
        };

        if let Some(mut file_name) = path.file_name().and_then(|file_name| file_name.to_str()) {
            if let Some(icon_path) = icon_path_for_extension(file_name) {
                return icon_path;
            }

            while let Some((_, extension)) = file_name.split_once('.') {
                if let Some(icon_path) = icon_path_for_extension(extension) {
                    return icon_path;
                }
                file_name = extension;
            }
        }

        let extension = path.extension().and_then(|extension| extension.to_str());
        if let Some(extension) = extension
            && let Some(icon_path) = icon_path_for_extension(extension)
        {
            return icon_path;
        }

        IconAsset::File.path()
    }
}
