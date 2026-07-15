use anyhow::anyhow;
use gpui::{Context, Empty, Entity, IntoElement, PromptLevel, Render, TaskExt, Window};
use semver::Version;
use std::sync::Arc;

use ui::{Text, Tooltip, UpdateButton};
use workspace::{pane::Pane, status_bar::StatusItemView};

use crate::{AutoUpdateStatus, AutoUpdater, UpdateCheckType};

struct ManualUpdateCheck {
    initial_status: AutoUpdateStatus,
    has_started: bool,
}

pub(crate) struct UpdateVersion {
    status: AutoUpdateStatus,
    update_check_type: UpdateCheckType,
    dismissed_status: Option<AutoUpdateStatus>,
    manual_check: Option<ManualUpdateCheck>,
}

impl UpdateVersion {
    pub(crate) fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        if let Some(auto_updater) = AutoUpdater::get(cx) {
            cx.observe_in(&auto_updater, window, |this, auto_updater, window, cx| {
                let auto_updater = auto_updater.read(cx);
                this.update_status(
                    auto_updater.status(),
                    auto_updater.update_check_type(),
                    auto_updater.dismissed_status(),
                    &auto_updater.current_version(),
                    window,
                    cx,
                );
            })
            .detach();

            let auto_updater = auto_updater.read(cx);
            Self {
                status: auto_updater.status(),
                update_check_type: auto_updater.update_check_type(),
                dismissed_status: auto_updater.dismissed_status(),
                manual_check: None,
            }
        } else {
            Self {
                status: AutoUpdateStatus::Idle,
                update_check_type: UpdateCheckType::Automatic,
                dismissed_status: None,
                manual_check: None,
            }
        }
    }

    pub(crate) fn update_simulation(&mut self, cx: &mut Context<Self>) {
        let next_state = match self.status {
            AutoUpdateStatus::Idle => AutoUpdateStatus::Checking,
            AutoUpdateStatus::Checking => AutoUpdateStatus::Downloading {
                version: Version::new(26, 1, 0),
                progress: Some(0.5),
            },
            AutoUpdateStatus::Downloading { .. } => AutoUpdateStatus::Installing {
                version: Version::new(26, 1, 0),
            },
            AutoUpdateStatus::Installing { .. } => AutoUpdateStatus::Updated {
                version: Version::new(26, 1, 0),
            },
            AutoUpdateStatus::Updated { .. } => AutoUpdateStatus::Failed {
                error: Arc::new(anyhow!("network timeout")),
            },
            AutoUpdateStatus::Failed { .. } => AutoUpdateStatus::Idle,
        };

        self.status = next_state;
        self.update_check_type = UpdateCheckType::Manual;
        self.dismissed_status = None;
        self.manual_check = None;
        cx.notify();
    }

    pub(crate) fn start_manual_check(&mut self) {
        self.manual_check = Some(ManualUpdateCheck {
            initial_status: self.status.clone(),
            has_started: matches!(
                self.status,
                AutoUpdateStatus::Checking
                    | AutoUpdateStatus::Downloading { .. }
                    | AutoUpdateStatus::Installing { .. }
            ),
        });
    }

    fn update_status(
        &mut self,
        status: AutoUpdateStatus,
        update_check_type: UpdateCheckType,
        dismissed_status: Option<AutoUpdateStatus>,
        current_version: &Version,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(manual_check) = self.manual_check.as_mut()
            && update_check_type.is_manual()
        {
            match &status {
                AutoUpdateStatus::Checking
                | AutoUpdateStatus::Downloading { .. }
                | AutoUpdateStatus::Installing { .. } => {
                    manual_check.has_started = true;
                }
                AutoUpdateStatus::Idle if manual_check.has_started => {
                    self.manual_check = None;
                    let detail = format!(
                        "Zaku {current_version} is currently the newest version available."
                    );
                    drop(window.prompt(
                        PromptLevel::Info,
                        "You're up to date!",
                        Some(&detail),
                        &["OK"],
                        cx,
                    ));
                }
                AutoUpdateStatus::Failed { .. }
                    if manual_check.has_started
                        || !Self::is_same_status(&manual_check.initial_status, &status) =>
                {
                    self.manual_check = None;
                    Self::show_manual_error_prompt(window, cx);
                }
                AutoUpdateStatus::Updated { .. } if manual_check.has_started => {
                    self.manual_check = None;
                }
                _ => {}
            }
        }

        self.status = status;
        self.update_check_type = update_check_type;
        self.dismissed_status = dismissed_status;
        cx.notify();
    }

    fn is_same_status(status1: &AutoUpdateStatus, status2: &AutoUpdateStatus) -> bool {
        match (status1, status2) {
            (
                AutoUpdateStatus::Failed { error: error1 },
                AutoUpdateStatus::Failed { error: error2 },
            ) => Arc::ptr_eq(error1, error2),
            _ => status1 == status2,
        }
    }

    fn show_manual_error_prompt(window: &mut Window, cx: &mut Context<Self>) {
        let prompt = window.prompt(
            PromptLevel::Warning,
            "Couldn't check for updates",
            Some("Zaku couldn't check for updates. Check your internet connection and try again."),
            &["Open Logs", "OK"],
            cx,
        );
        cx.spawn_in(window, async move |_, cx| {
            if prompt.await == Ok(0) {
                cx.update(|window, cx| {
                    window.dispatch_action(Box::new(actions::zaku::OpenLogs), cx);
                })?;
            }
            anyhow::Ok(())
        })
        .detach_and_log_err(cx);
    }

    fn is_dismissed(&self) -> bool {
        self.dismissed_status.as_ref() == Some(&self.status)
    }

    fn dismiss(&mut self, cx: &mut Context<Self>) {
        self.dismissed_status = Some(self.status.clone());
        if let Some(auto_updater) = AutoUpdater::get(cx) {
            let status = self.status.clone();
            auto_updater.update(cx, |auto_updater, cx| {
                auto_updater.dismiss_status(status, cx);
            });
        }
        cx.notify();
    }

    fn version_tooltip_message(version: &Version) -> String {
        UpdateButton::version_tooltip_message(version)
    }
}

impl Render for UpdateVersion {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if self.is_dismissed() {
            return Empty.into_any_element();
        }

        match &self.status {
            AutoUpdateStatus::Checking if self.update_check_type.is_manual() => {
                UpdateButton::checking().into_any_element()
            }
            AutoUpdateStatus::Downloading { version, progress } => {
                let rendered_version = version.clone();
                let tooltip = Tooltip::element(move |_, cx| {
                    let status = AutoUpdater::get(cx).map(|updater| updater.read(cx).status());
                    let message = match &status {
                        Some(AutoUpdateStatus::Downloading { version, progress }) => {
                            UpdateButton::downloading_tooltip_message(version, *progress)
                        }
                        _ => Self::version_tooltip_message(&rendered_version),
                    };
                    Text::new(message).into_any_element()
                });
                UpdateButton::downloading(*progress)
                    .tooltip_fn(tooltip)
                    .into_any_element()
            }
            AutoUpdateStatus::Installing { version } => {
                UpdateButton::installing(Self::version_tooltip_message(version)).into_any_element()
            }
            AutoUpdateStatus::Updated { version } => {
                UpdateButton::updated(Self::version_tooltip_message(version))
                    .on_click(|_, _, cx| workspace::reload(cx))
                    .into_any_element()
            }
            AutoUpdateStatus::Failed { error } => UpdateButton::failed(error.to_string())
                .on_click(|_, window, cx| {
                    window.dispatch_action(Box::new(actions::zaku::OpenLogs), cx);
                })
                .on_dismiss(cx.listener(|this, _, _, cx| this.dismiss(cx)))
                .into_any_element(),
            AutoUpdateStatus::Idle | AutoUpdateStatus::Checking => Empty.into_any_element(),
        }
    }
}

impl StatusItemView for UpdateVersion {
    fn set_active_pane(&mut self, _: &Entity<Pane>, _: &mut Window, _: &mut Context<Self>) {}
}

#[cfg(test)]
mod tests {
    use super::*;

    use gpui::{AppContext, TestAppContext};
    use std::path::PathBuf;

    use http_client::FakeHttpClient;

    #[test]
    fn test_version_tooltip_message() {
        let message = UpdateVersion::version_tooltip_message(&Version::new(26, 1, 0));

        assert_eq!(message, "Update to Zaku 26.1.0");
    }

    #[test]
    fn test_downloading_tooltip_message() {
        let version = Version::new(26, 1, 0);

        let message = UpdateButton::downloading_tooltip_message(&version, None);
        assert_eq!(message, "Update to Zaku 26.1.0");

        let message = UpdateButton::downloading_tooltip_message(&version, Some(0.554));
        assert_eq!(message, "Update to Zaku 26.1.0 (55% downloaded)");

        let message = UpdateButton::downloading_tooltip_message(&version, Some(1.5));
        assert_eq!(message, "Update to Zaku 26.1.0 (100% downloaded)");
    }

    #[gpui::test]
    fn test_manual_check_with_multiple_windows(cx: &mut TestAppContext) {
        let auto_updater = cx.new(|cx| {
            AutoUpdater::new(
                Version::new(26, 1, 0),
                FakeHttpClient::create(|_| async { panic!("http client should not be used") }),
                PathBuf::new(),
                cx,
            )
        });
        cx.set_global(crate::GlobalAutoUpdate(Some(auto_updater.clone())));

        let window1 = cx.add_window(|_, _| Empty);
        let window2 = cx.add_window(|_, _| Empty);
        let update_version1 = window1
            .update(cx, |_, window, cx| {
                cx.new(|cx| UpdateVersion::new(window, cx))
            })
            .unwrap();
        let update_version2 = window2
            .update(cx, |_, window, cx| {
                cx.new(|cx| UpdateVersion::new(window, cx))
            })
            .unwrap();

        update_version1.update(cx, |update_version, _| {
            update_version.start_manual_check();
        });
        auto_updater.update(cx, |auto_updater, cx| {
            auto_updater.status = AutoUpdateStatus::Checking;
            auto_updater.update_check_type = UpdateCheckType::Manual;
            cx.notify();
        });
        cx.run_until_parked();

        assert_eq!(
            update_version2.read_with(cx, |update_version, _| update_version.status.clone()),
            AutoUpdateStatus::Checking,
            "window 2 should observe the active check"
        );
        auto_updater.update(cx, |auto_updater, cx| {
            auto_updater.status = AutoUpdateStatus::Idle;
            cx.notify();
        });
        cx.run_until_parked();

        assert_eq!(
            cx.pending_prompt(),
            Some((
                "You're up to date!".to_string(),
                "Zaku 26.1.0 is currently the newest version available.".to_string(),
            )),
            "window 1 should show up-to-date prompt"
        );
        cx.simulate_prompt_answer("OK");

        assert!(
            !cx.has_pending_prompt(),
            "only window 1 should show a prompt"
        );
        assert_eq!(
            update_version2.read_with(cx, |update_version, _| update_version.status.clone()),
            AutoUpdateStatus::Idle,
            "window 2 should observe the completed check"
        );

        update_version1.update(cx, |update_version, _| {
            update_version.start_manual_check();
        });
        auto_updater.update(cx, |auto_updater, cx| {
            auto_updater.status = AutoUpdateStatus::Failed {
                error: Arc::new(anyhow!("network timeout")),
            };
            cx.notify();
        });
        cx.run_until_parked();

        assert_eq!(
            cx.pending_prompt(),
            Some((
                "Couldn't check for updates".to_string(),
                "Zaku couldn't check for updates. Check your internet connection and try again."
                    .to_string(),
            )),
            "manual failure should show error prompt"
        );
        cx.simulate_prompt_answer("OK");

        cx.run_until_parked();
        assert!(
            !cx.has_pending_prompt(),
            "only window 1 should show an error prompt"
        );
    }

    #[gpui::test]
    fn test_manual_check_distinguishes_repeated_errors(cx: &mut TestAppContext) {
        let (_, cx) = cx.add_window_view(|_, _| Empty);
        let existing_error = Arc::new(anyhow!("network timeout"));
        let update_version = cx.new(|_| UpdateVersion {
            status: AutoUpdateStatus::Failed {
                error: Arc::clone(&existing_error),
            },
            update_check_type: UpdateCheckType::Automatic,
            dismissed_status: None,
            manual_check: None,
        });

        update_version.update_in(cx, |update_version, window, cx| {
            update_version.start_manual_check();
            update_version.update_status(
                AutoUpdateStatus::Failed {
                    error: Arc::clone(&existing_error),
                },
                UpdateCheckType::Manual,
                None,
                &Version::new(26, 1, 0),
                window,
                cx,
            );
        });
        assert!(
            !cx.has_pending_prompt(),
            "the current error should not complete a new manual check"
        );

        update_version.update_in(cx, |update_version, window, cx| {
            update_version.update_status(
                AutoUpdateStatus::Failed {
                    error: Arc::new(anyhow!("network timeout")),
                },
                UpdateCheckType::Manual,
                None,
                &Version::new(26, 1, 0),
                window,
                cx,
            );
        });
        assert_eq!(
            cx.pending_prompt(),
            Some((
                "Couldn't check for updates".to_string(),
                "Zaku couldn't check for updates. Check your internet connection and try again."
                    .to_string(),
            )),
            "a repeated error should complete the new manual check"
        );
        cx.simulate_prompt_answer("OK");
        cx.run_until_parked();
    }
}
