use crate::{self as settings, RegisterSetting, Settings, SettingsContent};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GitStatusSettings {
    pub enabled: bool,
    pub project_panel: GitStatusProjectPanelSettings,
    pub tabs: GitStatusTabsSettings,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GitStatusProjectPanelSettings {
    pub colors: bool,
    pub indicators: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GitStatusTabsSettings {
    pub colors: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, RegisterSetting)]
pub struct GitSettings {
    pub enabled: bool,
    pub status: GitStatusSettings,
}

impl GitSettings {
    pub fn is_git_status_enabled(&self) -> bool {
        self.enabled && self.status.enabled
    }
}

impl Settings for GitSettings {
    fn from_settings(content: &SettingsContent) -> Self {
        let git = content.git.as_ref();
        let status = git.and_then(|git| git.status.clone()).unwrap_or_default();
        let project_panel = status.project_panel.clone().unwrap_or_default();
        let tabs = status.tabs.clone().unwrap_or_default();

        Self {
            enabled: git
                .and_then(|git| git.enabled)
                .expect("git enabled should be defaulted"),
            status: GitStatusSettings {
                enabled: status
                    .enabled
                    .expect("git status enabled should be defaulted"),
                project_panel: GitStatusProjectPanelSettings {
                    colors: project_panel
                        .colors
                        .expect("git status project panel colors should be defaulted"),
                    indicators: project_panel
                        .indicators
                        .expect("git status project panel indicators should be defaulted"),
                },
                tabs: GitStatusTabsSettings {
                    colors: tabs
                        .colors
                        .expect("git status tabs colors should be defaulted"),
                },
            },
        }
    }
}
