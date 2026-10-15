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

//! The Service Status Dashboard tool view: Windows background services monitor,
//! running/stopped status inspection, start/stop/restart controls, and CSV export.

use heimdall_app::TabId;
use iced::widget::{button, checkbox, column, container, row, scrollable, text, text_input};
use iced::{Alignment, Element, Length, Task, Theme};

use heimdall_core::tools::service_status::{self, ServiceAction, ServiceEntry, ServiceError};

use super::{CopySlot, ToolPane};
use crate::i18n::fl;
use crate::shell::Message;
use crate::styles;
use crate::tokens::{font_size, spacing};

/// Maximum rows rendered in the table to maintain responsive UI performance.
const MAX_DISPLAY_SERVICES: usize = 300;

/// Messages emitted by the Service Status view.
#[derive(Debug, Clone)]
pub enum ServiceStatusMessage {
    /// Refresh service status list now.
    Refresh,
    /// Refresh completed with service entries.
    Finished(Result<Vec<ServiceEntry>, ServiceError>),
    /// Filter query text changed.
    FilterChanged(String),
    /// Toggle whether to show only running services.
    ToggleRunningOnly(bool),
    /// Trigger a service action (Start, Stop, Restart).
    TriggerAction(String, ServiceAction),
    /// Service action completed.
    ActionResult(Result<(), ServiceError>),
    /// Copy CSV export to clipboard.
    CopyCsv,
}

/// The Service Status tool pane state.
#[derive(Debug, Default)]
pub struct ServiceStatusPane {
    running: bool,
    action_running: bool,
    error: Option<String>,
    status: Option<String>,
    services: Vec<ServiceEntry>,
    filter: String,
    running_only: bool,
}

/// Outcome of updating the Service Status tool pane.
#[derive(Debug)]
pub enum Outcome {
    None,
    Task(Task<Message>),
    Copy(CopySlot, String),
}

impl ServiceStatusPane {
    /// Handles tool messages.
    pub fn update(&mut self, tab: TabId, message: ServiceStatusMessage) -> Outcome {
        match message {
            ServiceStatusMessage::Refresh => {
                self.running = true;
                self.error = None;

                let task = Task::perform(
                    async move {
                        tokio::task::spawn_blocking(service_status::run_services_command)
                            .await
                            .unwrap_or_else(|error| {
                                Err(ServiceError::NotStarted(error.to_string()))
                            })
                    },
                    move |res| {
                        Message::Tool(
                            tab,
                            super::ToolMessage::Services(ServiceStatusMessage::Finished(res)),
                        )
                    },
                );

                Outcome::Task(task)
            }
            ServiceStatusMessage::Finished(Ok(res)) => {
                self.running = false;
                self.services = res;
                Outcome::None
            }
            ServiceStatusMessage::Finished(Err(err)) => {
                self.running = false;
                self.error = Some(service_error_text(&err));
                Outcome::None
            }
            ServiceStatusMessage::FilterChanged(f) => {
                self.filter = f;
                Outcome::None
            }
            ServiceStatusMessage::ToggleRunningOnly(r) => {
                self.running_only = r;
                Outcome::None
            }
            ServiceStatusMessage::TriggerAction(name, action) => {
                // One action at a time, and none while the list is read again: the name is
                // checked against the list as it stands.
                if self.action_running || self.running {
                    return Outcome::None;
                }
                self.action_running = true;
                self.error = None;
                self.status = None;
                let known = self.services.clone();

                let task = Task::perform(
                    async move {
                        tokio::task::spawn_blocking(move || {
                            service_status::run_service_action(action, &name, &known)
                        })
                        .await
                        .unwrap_or_else(|error| Err(ServiceError::NotStarted(error.to_string())))
                    },
                    move |res| {
                        Message::Tool(
                            tab,
                            super::ToolMessage::Services(ServiceStatusMessage::ActionResult(res)),
                        )
                    },
                );

                Outcome::Task(task)
            }
            ServiceStatusMessage::ActionResult(Ok(())) => {
                self.action_running = false;
                self.status = Some(fl!("ui-tool-services-action-success"));
                // Trigger an auto-refresh after an action
                self.update(tab, ServiceStatusMessage::Refresh)
            }
            ServiceStatusMessage::ActionResult(Err(err)) => {
                self.action_running = false;
                self.error = Some(service_error_text(&err));
                Outcome::None
            }
            ServiceStatusMessage::CopyCsv => {
                let matching: Vec<ServiceEntry> = self
                    .services
                    .iter()
                    .filter(|s| self.matches_filter(s))
                    .cloned()
                    .collect();
                if !matching.is_empty() {
                    let csv = service_status::build_services_csv(&matching);
                    Outcome::Copy(CopySlot::ServicesCsv, csv)
                } else {
                    Outcome::None
                }
            }
        }
    }

    /// Checks if a service entry matches the current filter query and running-only toggle.
    fn matches_filter(&self, s: &ServiceEntry) -> bool {
        if self.running_only && !s.status.eq_ignore_ascii_case("Running") {
            return false;
        }
        let q = self.filter.trim().to_lowercase();
        if q.is_empty() {
            true
        } else {
            s.name.to_lowercase().contains(&q)
                || s.display_name.to_lowercase().contains(&q)
                || s.status.to_lowercase().contains(&q)
                || s.start_type.to_lowercase().contains(&q)
        }
    }

    /// Renders the Service Status Dashboard tool view.
    pub fn view<'a>(
        &'a self,
        tab: TabId,
        state: &ToolPane,
        _theme: &Theme,
    ) -> Element<'a, Message> {
        let send = move |msg| Message::Tool(tab, super::ToolMessage::Services(msg));

        let refresh_btn = if self.running {
            super::action_button(fl!("ui-tool-services-scanning"), false, None)
        } else {
            super::action_button(
                fl!("ui-tool-services-refresh"),
                true,
                Some(send(ServiceStatusMessage::Refresh)),
            )
        };

        let copy_csv_btn = super::copy_button(
            fl!("ui-tool-services-copy-csv"),
            state.copied(CopySlot::ServicesCsv),
            send(ServiceStatusMessage::CopyCsv),
            super::COPY_PADDING,
        );

        let filter_input = text_input(
            fl!("ui-tool-services-filter-placeholder").as_str(),
            &self.filter,
        )
        .on_input(move |val| send(ServiceStatusMessage::FilterChanged(val)))
        .padding(super::INPUT_PADDING)
        .size(font_size::BODY)
        .width(Length::FillPortion(2));

        let running_only_chk = checkbox(self.running_only)
            .label(fl!("ui-tool-services-running-only"))
            .on_toggle(move |val| send(ServiceStatusMessage::ToggleRunningOnly(val)));

        let top_row = row![
            filter_input,
            container(running_only_chk).align_y(Alignment::Center),
            refresh_btn,
            copy_csv_btn,
        ]
        .spacing(spacing::SM)
        .align_y(Alignment::Center);

        let header_card = container(top_row).padding(spacing::MD).style(styles::card);

        // Stats summary bar
        let total_count = self.services.len();
        let running_count = self
            .services
            .iter()
            .filter(|s| s.status.eq_ignore_ascii_case("Running"))
            .count();
        let stopped_count = total_count.saturating_sub(running_count);

        let stats_row = row![
            text(fl!("ui-tool-services-total"))
                .size(font_size::CAPTION)
                .style(text::secondary),
            text(total_count.to_string())
                .size(font_size::CAPTION)
                .font(styles::SEMIBOLD),
            text("|").size(font_size::CAPTION).style(text::secondary),
            text(fl!("ui-tool-services-running-label"))
                .size(font_size::CAPTION)
                .style(text::secondary),
            text(running_count.to_string())
                .size(font_size::CAPTION)
                .font(styles::SEMIBOLD)
                .style(text::primary),
            text("|").size(font_size::CAPTION).style(text::secondary),
            text(fl!("ui-tool-services-stopped-label"))
                .size(font_size::CAPTION)
                .style(text::secondary),
            text(stopped_count.to_string())
                .size(font_size::CAPTION)
                .font(styles::SEMIBOLD)
                .style(text::danger),
        ]
        .spacing(spacing::SM)
        .align_y(Alignment::Center);

        let mut body = column![header_card, stats_row].spacing(spacing::MD);

        if let Some(err) = &self.error {
            body = body.push(
                container(text(err.as_str()).size(font_size::BODY).style(text::danger))
                    .padding(spacing::MD)
                    .style(styles::card),
            );
        }

        if let Some(status) = &self.status {
            body = body.push(
                text(status.as_str())
                    .size(font_size::CAPTION)
                    .style(text::secondary),
            );
        }

        let matching: Vec<&'a ServiceEntry> = self
            .services
            .iter()
            .filter(|s| self.matches_filter(s))
            .collect();
        if !matching.is_empty() {
            let table_headers = row![
                text(fl!("ui-tool-services-col-name"))
                    .size(font_size::CAPTION)
                    .style(text::secondary)
                    .width(Length::Fixed(150.0)),
                text(fl!("ui-tool-services-col-display-name"))
                    .size(font_size::CAPTION)
                    .style(text::secondary)
                    .width(Length::FillPortion(2)),
                text(fl!("ui-tool-services-col-status"))
                    .size(font_size::CAPTION)
                    .style(text::secondary)
                    .width(Length::Fixed(90.0)),
                text(fl!("ui-tool-services-col-start-type"))
                    .size(font_size::CAPTION)
                    .style(text::secondary)
                    .width(Length::Fixed(100.0)),
                text(fl!("ui-tool-services-col-actions"))
                    .size(font_size::CAPTION)
                    .style(text::secondary)
                    .width(Length::Fixed(150.0)),
            ]
            .spacing(spacing::SM);

            let mut entry_rows = column![table_headers].spacing(spacing::XS);

            for s in matching.iter().take(MAX_DISPLAY_SERVICES) {
                let is_running = s.status.eq_ignore_ascii_case("Running");
                let status_style = if is_running {
                    text::primary
                } else {
                    text::danger
                };

                let name_clone = s.name.clone();
                let name_clone2 = s.name.clone();
                let name_clone3 = s.name.clone();

                let action_btns = row![
                    button(text(fl!("ui-tool-services-act-start")).size(font_size::CAPTION))
                        .style(styles::subtle)
                        .on_press_maybe((!is_running && !self.action_running).then(|| {
                            send(ServiceStatusMessage::TriggerAction(
                                name_clone,
                                ServiceAction::Start,
                            ))
                        })),
                    button(text(fl!("ui-tool-services-act-stop")).size(font_size::CAPTION))
                        .style(styles::subtle)
                        .on_press_maybe((is_running && !self.action_running).then(|| {
                            send(ServiceStatusMessage::TriggerAction(
                                name_clone2,
                                ServiceAction::Stop,
                            ))
                        })),
                    button(text(fl!("ui-tool-services-act-restart")).size(font_size::CAPTION))
                        .style(styles::subtle)
                        .on_press_maybe((!self.action_running).then(|| {
                            send(ServiceStatusMessage::TriggerAction(
                                name_clone3,
                                ServiceAction::Restart,
                            ))
                        })),
                ]
                .spacing(spacing::XS);

                let r = row![
                    text(&s.name)
                        .size(font_size::BODY)
                        .font(super::BOX_FONT)
                        .width(Length::Fixed(150.0)),
                    text(&s.display_name)
                        .size(font_size::BODY)
                        .width(Length::FillPortion(2)),
                    text(&s.status)
                        .size(font_size::BODY)
                        .font(super::BOX_FONT)
                        .style(status_style)
                        .width(Length::Fixed(90.0)),
                    text(&s.start_type)
                        .size(font_size::BODY)
                        .font(super::BOX_FONT)
                        .style(text::secondary)
                        .width(Length::Fixed(100.0)),
                    action_btns.width(Length::Fixed(150.0)),
                ]
                .spacing(spacing::SM)
                .align_y(Alignment::Center);

                entry_rows = entry_rows.push(r);
            }

            let card = container(scrollable(entry_rows))
                .padding(spacing::MD)
                .style(styles::card);

            body = body.push(card);
        } else if !self.running && self.services.is_empty() {
            body = body.push(super::empty_state(fl!("ui-tool-services-empty-state")));
        }

        super::content_column(body)
    }
}

/// What `error` says to the user.
fn service_error_text(error: &ServiceError) -> String {
    match error {
        ServiceError::InvalidName => fl!("ui-tool-services-error-invalid-name"),
        ServiceError::UnknownName => fl!("ui-tool-services-error-unknown-name"),
        ServiceError::NoPowerShell => fl!("ui-tool-services-error-no-powershell"),
        ServiceError::NoSystemctl => fl!("ui-tool-services-error-no-systemctl"),
        ServiceError::NotStarted(detail) => {
            fl!("ui-tool-services-error-not-started", error = detail.clone())
        }
        ServiceError::TimedOut => fl!("ui-tool-services-error-timeout"),
        ServiceError::NotElevated => fl!("ui-tool-services-error-not-elevated"),
        ServiceError::ActionFailed { code, detail } if detail.is_empty() => fl!(
            "ui-tool-services-error-action-failed",
            code = exit_code_text(*code)
        ),
        ServiceError::ActionFailed { detail, .. } => fl!(
            "ui-tool-services-error-action-failed-detail",
            error = detail.clone()
        ),
        ServiceError::ListFailed { code, detail } => fl!(
            "ui-tool-services-error-list-failed",
            error = if detail.is_empty() {
                exit_code_text(*code)
            } else {
                detail.clone()
            }
        ),
    }
}

/// An exit code as said: none when the command was ended by a signal.
fn exit_code_text(code: Option<i32>) -> String {
    code.map_or_else(
        || fl!("ui-tool-services-exit-code-none"),
        |code| code.to_string(),
    )
}

#[cfg(test)]
mod tests {
    use heimdall_app::tools::ToolId;
    use heimdall_app::{App, AppConfig, Message as AppMessage, ToolsMessage};
    use heimdall_ssh::AgentSource;
    use heimdall_term::GridSize;

    use super::*;

    /// A tool tab's identifier, from an application of its own in `dir`.
    fn tab(dir: &std::path::Path) -> TabId {
        let mut app = App::new(AppConfig {
            profiles_file: dir.join("profiles.toml"),
            known_hosts: dir.join("known_hosts"),
            legacy_dir: None,
            agent: AgentSource::Disabled,
            initial_grid: GridSize { cols: 80, rows: 24 },
            files_start: dir.to_owned(),
            system_credentials: heimdall_app::SystemCredentials::memory(),
        });
        let _ = app.update(AppMessage::Tools(ToolsMessage::Open {
            tool: ToolId::Services,
            title: String::new(),
        }));
        app.tabs[0].id
    }

    #[test]
    fn one_action_at_a_time_and_its_end_always_frees_the_buttons() {
        let dir = tempfile::tempdir().expect("dir");
        let tab = tab(dir.path());
        let mut pane = ServiceStatusPane::default();
        let action = || ServiceStatusMessage::TriggerAction("x".to_owned(), ServiceAction::Start);
        assert!(matches!(pane.update(tab, action()), Outcome::Task(_)));
        assert!(pane.action_running);
        assert!(
            matches!(pane.update(tab, action()), Outcome::None),
            "a second action waits for the first"
        );

        let _ = pane.update(
            tab,
            ServiceStatusMessage::ActionResult(Err(ServiceError::TimedOut)),
        );
        assert!(!pane.action_running);
        assert_eq!(pane.error, Some(fl!("ui-tool-services-error-timeout")));

        // A refused name is said in words of the user's language, not the command's.
        assert!(matches!(pane.update(tab, action()), Outcome::Task(_)));
        let _ = pane.update(
            tab,
            ServiceStatusMessage::ActionResult(Err(ServiceError::UnknownName)),
        );
        assert!(!pane.action_running);
        assert_eq!(pane.error, Some(fl!("ui-tool-services-error-unknown-name")));
    }

    #[test]
    fn every_error_has_its_own_text() {
        let errors = [
            ServiceError::InvalidName,
            ServiceError::UnknownName,
            ServiceError::NoPowerShell,
            ServiceError::NoSystemctl,
            ServiceError::NotStarted("x".to_owned()),
            ServiceError::TimedOut,
            ServiceError::NotElevated,
            ServiceError::ActionFailed {
                code: Some(1),
                detail: String::new(),
            },
            ServiceError::ActionFailed {
                code: Some(1),
                detail: "denied".to_owned(),
            },
            ServiceError::ListFailed {
                code: None,
                detail: String::new(),
            },
        ];
        let texts: std::collections::HashSet<String> =
            errors.iter().map(service_error_text).collect();
        assert_eq!(texts.len(), errors.len());
        assert!(texts.iter().all(|text| !text.contains("ui-tool-")));
    }
}
