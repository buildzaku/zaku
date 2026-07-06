mod file_icons;

pub use file_icons::FileIcons;

use serde::{Deserialize, Serialize};
use std::sync::Arc;
use strum::{EnumIter, EnumString, IntoStaticStr};

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
