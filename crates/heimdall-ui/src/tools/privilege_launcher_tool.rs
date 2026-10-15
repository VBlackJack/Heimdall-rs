/*
 * Copyright 2026 Julien Bombled
 *
 * Licensed under the Apache License, Version 2.0 (the "License");
 * you may not use this file except in compliance with the License.
 * You may obtain a copy of the License at
 *
 *     http://www.apache.org/licenses/LICENSE-2.0
 *
 * Unless required by applicable law or agreed to in writing, software
 * distributed under the License is distributed on an "AS IS" BASIS,
 * WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
 * See the License for the specific language governing permissions and
 * limitations under the License.
 */

//! The Elevated Privilege Launcher tool view: launch administrative management consoles
//! and custom executables with elevated UAC or root privileges.

use heimdall_app::TabId;
use iced::widget::{button, column, container, row, text, text_input};
use iced::{Alignment, Element, Length, Task, Theme};

use heimdall_core::tools::privilege_launcher::{
    PrivilegeLevel, QUICK_LAUNCH_ITEMS, QuickLaunchItem, is_current_process_elevated,
    launch_elevated,
};

use super::{CopySlot, ToolPane};
use crate::i18n::fl;
use crate::shell::Message;
use crate::styles;
use crate::tokens::{font_size, spacing};

/// Messages emitted by the Privilege Launcher view.
#[derive(Debug, Clone)]
pub enum PrivilegeLauncherMessage {
    /// Target executable path changed.
    TargetExeChanged(String),
    /// Command line arguments changed.
    ArgumentsChanged(String),
    /// Privilege level changed.
    PrivilegeLevelChanged(PrivilegeLevel),
    /// Quick launch button pressed.
    QuickLaunch(QuickLaunchItem),
    /// Launch custom executable.
    LaunchCustom,
    /// Process launch completed.
    LaunchCompleted(Result<(), String>),
}

/// The Privilege Launcher tool pane state.
#[derive(Debug)]
pub struct PrivilegeLauncherPane {
    target_exe: String,
    arguments: String,
    privilege_level: PrivilegeLevel,
    is_elevated: bool,
    launching: bool,
    status: Option<String>,
    error: Option<String>,
}

impl Default for PrivilegeLauncherPane {
    fn default() -> Self {
        Self {
            target_exe: "powershell.exe".to_string(),
            arguments: String::new(),
            privilege_level: PrivilegeLevel::CurrentUserElevated,
            is_elevated: is_current_process_elevated(),
            launching: false,
            status: None,
            error: None,
        }
    }
}

/// Outcome of updating the Privilege Launcher tool pane.
#[derive(Debug)]
#[allow(dead_code)]
pub enum Outcome {
    None,
    Task(Task<Message>),
    Copy(CopySlot, String),
}

impl PrivilegeLauncherPane {
    /// Handles tool messages.
    pub fn update(&mut self, tab: TabId, message: PrivilegeLauncherMessage) -> Outcome {
        match message {
            PrivilegeLauncherMessage::TargetExeChanged(exe) => {
                self.target_exe = exe;
                Outcome::None
            }
            PrivilegeLauncherMessage::ArgumentsChanged(args) => {
                self.arguments = args;
                Outcome::None
            }
            PrivilegeLauncherMessage::PrivilegeLevelChanged(level) => {
                self.privilege_level = level;
                Outcome::None
            }
            PrivilegeLauncherMessage::QuickLaunch(item) => {
                self.target_exe = item.executable.to_string();
                self.arguments = item.arguments.to_string();
                self.launching = true;
                self.error = None;
                self.status = None;

                let exe = item.executable.to_string();
                let args = item.arguments.to_string();
                let priv_level = self.privilege_level;

                let task = Task::perform(
                    async move {
                        tokio::task::spawn_blocking(move || {
                            launch_elevated(&exe, &args, priv_level)
                        })
                        .await
                        .map_err(|e| e.to_string())
                        .and_then(|r| r)
                    },
                    move |res| {
                        Message::Tool(
                            tab,
                            super::ToolMessage::PrivLaunch(
                                PrivilegeLauncherMessage::LaunchCompleted(res),
                            ),
                        )
                    },
                );

                Outcome::Task(task)
            }
            PrivilegeLauncherMessage::LaunchCustom => {
                if self.target_exe.trim().is_empty() {
                    return Outcome::None;
                }
                self.launching = true;
                self.error = None;
                self.status = None;

                let exe = self.target_exe.trim().to_string();
                let args = self.arguments.trim().to_string();
                let priv_level = self.privilege_level;

                let task = Task::perform(
                    async move {
                        tokio::task::spawn_blocking(move || {
                            launch_elevated(&exe, &args, priv_level)
                        })
                        .await
                        .map_err(|e| e.to_string())
                        .and_then(|r| r)
                    },
                    move |res| {
                        Message::Tool(
                            tab,
                            super::ToolMessage::PrivLaunch(
                                PrivilegeLauncherMessage::LaunchCompleted(res),
                            ),
                        )
                    },
                );

                Outcome::Task(task)
            }
            PrivilegeLauncherMessage::LaunchCompleted(Ok(())) => {
                self.launching = false;
                self.status = Some(fl!("ui-tool-privlaunch-success"));
                Outcome::None
            }
            PrivilegeLauncherMessage::LaunchCompleted(Err(err)) => {
                self.launching = false;
                self.error = Some(fl!("ui-tool-privlaunch-failed", error = err));
                Outcome::None
            }
        }
    }

    /// Renders the Privilege Launcher tool view.
    pub fn view<'a>(
        &'a self,
        tab: TabId,
        _state: &ToolPane,
        _theme: &Theme,
    ) -> Element<'a, Message> {
        let send = move |msg| Message::Tool(tab, super::ToolMessage::PrivLaunch(msg));

        let elevation_banner = container(
            row![
                text(if self.is_elevated {
                    fl!("ui-tool-privlaunch-banner-elevated")
                } else {
                    fl!("ui-tool-privlaunch-banner-standard")
                })
                .size(font_size::CAPTION)
                .style(if self.is_elevated {
                    text::success
                } else {
                    text::secondary
                }),
            ]
            .spacing(spacing::SM)
            .align_y(Alignment::Center),
        )
        .padding(spacing::SM)
        .style(styles::card)
        .width(Length::Fill);

        let warning_banner = container(
            text(fl!("ui-tool-privlaunch-warning"))
                .size(font_size::SMALL_CAPTION)
                .style(text::secondary),
        )
        .padding(spacing::SM)
        .style(styles::card)
        .width(Length::Fill);

        let mut quick_items_row = row![].spacing(spacing::SM);
        for item in QUICK_LAUNCH_ITEMS {
            let q_item = (*item).clone();
            let btn = button(text(item.label).size(font_size::CAPTION))
                .padding([spacing::XS, spacing::SM])
                .style(styles::secondary)
                .on_press_maybe(if self.launching {
                    None
                } else {
                    Some(send(PrivilegeLauncherMessage::QuickLaunch(q_item)))
                });
            quick_items_row = quick_items_row.push(btn);
        }

        let quick_launch_card = container(
            column![
                text(fl!("ui-tool-privlaunch-quick-title"))
                    .size(font_size::CAPTION)
                    .style(text::secondary),
                quick_items_row,
            ]
            .spacing(spacing::SM),
        )
        .padding(spacing::MD)
        .style(styles::card)
        .width(Length::Fill);

        let exe_input = text_input(&fl!("ui-tool-privlaunch-exe-placeholder"), &self.target_exe)
            .on_input(move |s| send(PrivilegeLauncherMessage::TargetExeChanged(s)))
            .width(Length::FillPortion(2));

        let args_input = text_input(&fl!("ui-tool-privlaunch-args-placeholder"), &self.arguments)
            .on_input(move |s| send(PrivilegeLauncherMessage::ArgumentsChanged(s)))
            .width(Length::FillPortion(2));

        let btn_priv_user =
            button(text(fl!("ui-tool-privlaunch-priv-user")).size(font_size::SMALL_CAPTION))
                .padding([spacing::XS, spacing::XS])
                .style(
                    if self.privilege_level == PrivilegeLevel::CurrentUserElevated {
                        styles::primary
                    } else {
                        styles::secondary
                    },
                )
                .on_press(send(PrivilegeLauncherMessage::PrivilegeLevelChanged(
                    PrivilegeLevel::CurrentUserElevated,
                )));

        let btn_priv_sys =
            button(text(fl!("ui-tool-privlaunch-priv-system")).size(font_size::SMALL_CAPTION))
                .padding([spacing::XS, spacing::XS])
                .style(if self.privilege_level == PrivilegeLevel::System {
                    styles::primary
                } else {
                    styles::secondary
                })
                .on_press(send(PrivilegeLauncherMessage::PrivilegeLevelChanged(
                    PrivilegeLevel::System,
                )));

        let priv_selector = row![
            text(fl!("ui-tool-privlaunch-privilege-label")).size(font_size::SMALL_CAPTION),
            btn_priv_user,
            btn_priv_sys,
        ]
        .spacing(spacing::SM)
        .align_y(Alignment::Center);

        let btn_launch =
            button(text(fl!("ui-tool-privlaunch-btn-launch")).size(font_size::CAPTION))
                .padding([spacing::XS, spacing::MD])
                .style(styles::primary)
                .on_press_maybe(if self.launching || self.target_exe.trim().is_empty() {
                    None
                } else {
                    Some(send(PrivilegeLauncherMessage::LaunchCustom))
                });

        let mut feedback_col = column![].spacing(spacing::XS);
        if let Some(status) = &self.status {
            feedback_col =
                feedback_col.push(text(status).size(font_size::CAPTION).style(text::success));
        }
        if let Some(err) = &self.error {
            feedback_col =
                feedback_col.push(text(err).size(font_size::CAPTION).style(text::danger));
        }

        let custom_launch_card = container(
            column![
                text(fl!("ui-tool-privlaunch-custom-title"))
                    .size(font_size::CAPTION)
                    .style(text::secondary),
                row![exe_input, args_input].spacing(spacing::SM),
                row![priv_selector, btn_launch]
                    .spacing(spacing::MD)
                    .align_y(Alignment::Center),
                feedback_col,
            ]
            .spacing(spacing::MD),
        )
        .padding(spacing::MD)
        .style(styles::card)
        .width(Length::Fill);

        column![
            elevation_banner,
            warning_banner,
            quick_launch_card,
            custom_launch_card
        ]
        .spacing(spacing::MD)
        .padding(spacing::MD)
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
    }
}
