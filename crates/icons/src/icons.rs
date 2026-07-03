mod file_icons;

pub use file_icons::FileIcons;

use std::sync::Arc;

use serde::{Deserialize, Serialize};
use strum::{EnumIter, EnumString, IntoStaticStr};

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, EnumIter, EnumString, IntoStaticStr,
)]
#[strum(serialize_all = "snake_case")]
#[serde(rename_all = "snake_case")]
pub enum IconName {
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

impl IconName {
    pub fn path(&self) -> Arc<str> {
        let file_stem: &'static str = self.into();
        format!("icons/{file_stem}.svg").into()
    }
}
