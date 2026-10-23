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

//! The run panel of a runbook, as the C# `RunbookRunViewModel`. The run is manual, step by
//! step: the variables are asked once when it starts, then each step is shown with its status
//! and its command, and the user sends it to a terminal, types it at the prompt, copies it or
//! skips it. A terminal returns no exit code, so the user reads the output and goes on or
//! stops. Nothing is ever sent by itself: no step advances without a press, and there is no
//! timer.
//!
//! The command of the current step is made again, from the variables and the library as they
//! are now, each time it is shown and each time it is asked for: every value goes through the
//! generator like one typed in the library, so what the editor let through as a `{{variable}}`
//! is judged here. A step that is refused says why and cannot be sent.

use std::collections::BTreeMap;
use std::time::{Duration, Instant};

use heimdall_app::TabId;
use heimdall_core::profile::RemoteOs;
use heimdall_core::tools::command_library;
use heimdall_core::tools::runbooks::{self, ResolvedStep, RunProblem, Runbook, Step, resolve_step};
use heimdall_core::tools::user_commands::{
    Criticality, MAX_COMMAND_LENGTH, UserLibrary, UserParameter,
};
use iced::widget::{Column, Row, column, container, row, text, text_input};
use iced::{Element, Length};

use super::{
    LABEL_WIDTH, RunbookMessage, RunbooksPane, VARIABLE_NAME_WIDTH, action, mode_name, send,
    shown_action,
};
use crate::i18n::fl;
use crate::shell::Message;
use crate::styles;
use crate::tokens::{font_size, spacing};
use crate::tools::command_library_tool::{generate_error, value_error};

/// How long after a step is delivered the next one ignores a press: a double click or a key
/// repeat on the button that just sent a step must not send the one after it, which nobody
/// has read yet.
const RUN_GUARD: Duration = Duration::from_millis(400);

/// Where the steps of the run in progress can go.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RunTarget {
    /// The terminal the run was pinned to when it began.
    Ready(TabId),
    /// There was none then, and none has been picked since.
    NoTerminal,
    /// The terminal the run was pinned to is no longer offered.
    Gone,
    /// The session of the library changed during the run: the user picks a terminal again.
    Changed,
}

/// What became of a step.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::tools) enum StepStatus {
    /// Not reached, or not done yet.
    Pending,
    /// Written into a terminal and submitted.
    Sent,
    /// Typed at the prompt of a terminal and left there.
    Inserted,
    /// Copied.
    Copied,
    /// Passed over.
    Skipped,
}

/// How a step left the run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Delivered {
    /// Written and submitted.
    Sent,
    /// Typed at the prompt.
    Inserted,
    /// Copied.
    Copied,
}

/// What the session the library was opened for gives a run.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(super) struct RunSession {
    /// Its host.
    pub(super) host: Option<String>,
    /// Its account.
    pub(super) user: Option<String>,
    /// The system of its far end.
    pub(super) remote: RemoteOs,
}

/// A run in progress.
#[derive(Debug)]
pub(super) struct Run {
    /// The runbook as it was when the run started: editing it elsewhere does not change a
    /// run.
    runbook: Runbook,
    /// The variables with the value asked for each.
    values: Vec<(String, String)>,
    /// Whether the variables were confirmed and the steps shown.
    begun: bool,
    /// What became of each step.
    statuses: Vec<StepStatus>,
    /// The step the run is at; the number of steps once it is finished.
    current: usize,
    /// Which run it is: a receipt for another one moves nothing.
    id: u64,
    /// Whether the terminal and the system of the far end were pinned, which happens once,
    /// when the steps are first shown.
    bound: bool,
    /// The terminal every step goes to.
    pinned: Option<TabId>,
    /// The system of the far end the templates were chosen for.
    remote: RemoteOs,
    /// The session of the library changed since the run began.
    stale: bool,
    /// When the last step was delivered.
    delivered_at: Option<Instant>,
    /// How long a press is ignored after a delivery.
    guard: Duration,
}

impl Run {
    fn variables(&self) -> BTreeMap<String, String> {
        self.values.iter().cloned().collect()
    }

    fn is_finished(&self) -> bool {
        self.begun && self.current >= self.runbook.steps.len()
    }

    /// Moves to the next step.
    fn advance(&mut self) {
        if self.current < self.runbook.steps.len() {
            self.current += 1;
        }
    }
}

/// What the library keeps of an action step in its history.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActionRecord {
    /// The identity of the action.
    pub id: String,
    /// Its title.
    pub title: String,
    /// The pattern of the template the step ran, each value a `{name}`.
    pub pattern: String,
    /// The names of its values.
    pub parameters: Vec<String>,
}

/// The current step, made and checked, ready to be sent.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Ready {
    /// The run it belongs to.
    pub run_id: u64,
    /// The place of the step in the runbook.
    pub index: usize,
    /// The command and how dangerous it is.
    pub step: ResolvedStep,
    /// What the history keeps; `None` for a literal step, which no library action stands for
    /// (as the C#, which writes no history row for it).
    pub action: Option<ActionRecord>,
}

impl Ready {
    /// Whether sending it must ask the dangerous-command question: the stored level of the
    /// action, a command the paste guard knows, and always a literal step, which nobody has
    /// judged.
    #[must_use]
    pub fn is_dangerous(&self) -> bool {
        self.step.literal
            || self.step.level == Criticality::Dangerous
            || heimdall_app::dangerous_command(&self.step.command).is_some()
    }
}

impl RunbooksPane {
    /// Tells the panel about the session the library was opened for, which fills the
    /// variables of a run that take its host or its user.
    pub fn set_session(&mut self, host: Option<String>, user: Option<String>, remote: RemoteOs) {
        self.session = RunSession { host, user, remote };
    }

    /// Whether a run is in progress.
    #[cfg(test)]
    #[must_use]
    pub(in crate::tools) const fn is_running(&self) -> bool {
        self.run.is_some()
    }

    /// Starts a run of the runbook of that identity, asking its variables first.
    pub(super) fn start_run(&mut self, id: &str) {
        let Some(runbook) = self.store.get(id).cloned() else {
            return;
        };
        let values = runbook
            .variables
            .iter()
            .map(|(name, default)| {
                let given = command_library::session_value(
                    &UserParameter::new(name),
                    self.session.host.as_deref(),
                    self.session.user.as_deref(),
                );
                (name.clone(), given.unwrap_or_else(|| default.clone()))
            })
            .collect();
        let begun = runbook.variables.is_empty();
        let statuses = vec![StepStatus::Pending; runbook.steps.len()];
        self.runs_started += 1;
        self.run = Some(Run {
            runbook,
            values,
            begun,
            statuses,
            current: 0,
            id: self.runs_started,
            bound: false,
            pinned: None,
            remote: RemoteOs::Auto,
            stale: false,
            delivered_at: None,
            guard: RUN_GUARD,
        });
    }

    /// Pins the run to `tab` and to the system of the session as they are now, once its steps
    /// are shown. A run does not follow the terminal chosen afterwards, nor a session that
    /// changes: it would send the next step somewhere the user did not look.
    pub fn pin(&mut self, tab: Option<TabId>) {
        let remote = self.session.remote;
        if let Some(run) = self.run.as_mut()
            && run.begun
            && !run.bound
        {
            run.bound = true;
            run.pinned = tab;
            run.remote = remote;
        }
    }

    /// The user picked a terminal: the run goes there from now on.
    pub fn repin(&mut self, tab: TabId) {
        if let Some(run) = self.run.as_mut()
            && run.bound
        {
            run.pinned = Some(tab);
            run.stale = false;
        }
    }

    /// The session of the library changed: a run in progress waits for a terminal to be picked.
    pub fn session_changed(&mut self) {
        if let Some(run) = self.run.as_mut()
            && run.bound
        {
            run.stale = true;
        }
    }

    /// The terminal the run is pinned to, and whether the session changed since.
    #[must_use]
    pub fn pinned(&self) -> Option<TabId> {
        self.run.as_ref().and_then(|run| run.pinned)
    }

    /// Whether the session changed since the run began.
    #[must_use]
    pub fn is_stale(&self) -> bool {
        self.run.as_ref().is_some_and(|run| run.stale)
    }

    /// Whether the run in progress is shown its steps.
    #[must_use]
    pub fn is_bound(&self) -> bool {
        self.run.as_ref().is_some_and(|run| run.bound)
    }

    /// Whether a step was delivered a moment ago: a press now is a double click.
    #[must_use]
    pub fn is_guarded(&self) -> bool {
        self.run
            .as_ref()
            .is_some_and(|run| run.delivered_at.is_some_and(|at| at.elapsed() < run.guard))
    }

    /// How long a press is ignored after a delivery.
    #[cfg(test)]
    pub(in crate::tools) fn set_guard(&mut self, guard: Duration) {
        if let Some(run) = self.run.as_mut() {
            run.guard = guard;
        }
    }

    /// Applies a message of the run that asks nothing of a terminal.
    pub(super) fn run_message(&mut self, message: RunbookMessage) {
        match message {
            RunbookMessage::RunValue(index, value) => {
                if let Some(run) = self.run.as_mut()
                    && !run.begun
                    && let Some(entry) = run.values.get_mut(index)
                {
                    entry.1 = value;
                }
            }
            RunbookMessage::RunBegin => {
                if let Some(run) = self.run.as_mut() {
                    run.begun = true;
                }
            }
            RunbookMessage::RunSkip => {
                if let Some(run) = self.run.as_mut()
                    && run.begun
                    && let Some(status) = run.statuses.get_mut(run.current)
                {
                    *status = StepStatus::Skipped;
                    run.advance();
                }
            }
            RunbookMessage::RunStop => self.run = None,
            _ => {}
        }
    }

    /// The current step, made now from the variables and the library, or why it cannot be.
    fn resolve_current(&self, library: &UserLibrary) -> Option<(usize, Result<Ready, RunProblem>)> {
        let run = self.run.as_ref().filter(|run| run.begun)?;
        let step = run.runbook.steps.get(run.current)?;
        let shown = match step {
            Step::Action { action_id, .. } => shown_action(library, action_id),
            Step::Literal { .. } => None,
        };
        let resolved = resolve_step(
            step,
            &run.variables(),
            shown.as_deref(),
            self.run.as_ref().map_or(self.session.remote, |run| {
                if run.bound {
                    run.remote
                } else {
                    self.session.remote
                }
            }),
        );
        let ready = resolved.map(|resolved| {
            let action = match (step, shown.as_deref()) {
                (Step::Action { action_id, .. }, Some(command)) => Some(ActionRecord {
                    id: action_id.clone(),
                    title: command.title.clone(),
                    pattern: command
                        .template(resolved.platform)
                        .map(|template| template.command_pattern.clone())
                        .unwrap_or_default(),
                    parameters: command
                        .template(resolved.platform)
                        .map(|template| {
                            template
                                .parameters
                                .iter()
                                .map(|parameter| parameter.name.clone())
                                .collect()
                        })
                        .unwrap_or_default(),
                }),
                _ => None,
            };
            Ready {
                run_id: run.id,
                index: run.current,
                step: resolved,
                action,
            }
        });
        Some((run.current, ready))
    }

    /// The current step when it can be sent, judged now: the run asked for it, so the values
    /// are checked again whatever the panel showed a moment ago.
    #[must_use]
    pub fn ready_step(&self, library: &UserLibrary) -> Option<Ready> {
        self.resolve_current(library)?.1.ok()
    }

    /// A step was written, typed or copied: it is marked and the run goes on to the next. A
    /// receipt for a step the run is no longer at (stopped, or moved on) changes nothing.
    pub fn step_delivered(&mut self, run_id: u64, index: usize, how: Delivered) {
        let Some(run) = self.run.as_mut() else {
            return;
        };
        if run.id != run_id || !run.begun || run.current != index {
            return;
        }
        let Some(status) = run.statuses.get_mut(index) else {
            return;
        };
        if *status != StepStatus::Pending {
            return;
        }
        *status = match how {
            Delivered::Sent => StepStatus::Sent,
            Delivered::Inserted => StepStatus::Inserted,
            Delivered::Copied => StepStatus::Copied,
        };
        run.delivered_at = Some(Instant::now());
        run.advance();
    }

    /// Ends the run in progress.
    #[cfg(test)]
    pub(in crate::tools) fn stop_for_test(&mut self) {
        self.run = None;
    }

    /// The identity of the run in progress.
    #[cfg(test)]
    pub(in crate::tools) fn run_id(&self) -> u64 {
        self.run.as_ref().map_or(0, |run| run.id)
    }

    /// The status of step `index` of the run in progress.
    #[cfg(test)]
    pub(in crate::tools) fn status_of(&self, index: usize) -> Option<StepStatus> {
        self.run
            .as_ref()
            .and_then(|run| run.statuses.get(index).copied())
    }

    /// The value asked for each variable of the run in progress.
    #[cfg(test)]
    pub(in crate::tools) fn run_values(&self) -> Vec<(String, String)> {
        self.run
            .as_ref()
            .map(|run| run.values.clone())
            .unwrap_or_default()
    }

    /// Whether the run in progress is at its variables, not yet at its steps.
    #[cfg(test)]
    pub(in crate::tools) fn is_asking(&self) -> bool {
        self.run.as_ref().is_some_and(|run| !run.begun)
    }

    /// Whether the run in progress has gone through every step.
    #[cfg(test)]
    pub(in crate::tools) fn is_finished(&self) -> bool {
        self.run.as_ref().is_some_and(Run::is_finished)
    }

    /// The place of the step the run is at.
    #[cfg(test)]
    pub(in crate::tools) fn current_index(&self) -> Option<usize> {
        self.run.as_ref().map(|run| run.current)
    }

    /// Why the current step cannot be sent, when it cannot.
    #[cfg(test)]
    pub(in crate::tools) fn current_problem(&self, library: &UserLibrary) -> Option<RunProblem> {
        self.resolve_current(library)?.1.err()
    }

    /// The panel of the run in progress; `has_terminal` says whether a terminal is offered to
    /// send to.
    pub(super) fn run_view<'a>(
        &'a self,
        tab: TabId,
        library: &'a UserLibrary,
        has_terminal: bool,
        warning: Option<String>,
    ) -> Option<Element<'a, Message>> {
        let run = self.run.as_ref()?;
        let send = move |message| send(tab, message);
        let mut panel: Column<'a, Message> = column![
            row![
                text(fl!(
                    "ui-tool-cmdlib-runbooks-run-title",
                    name = run.runbook.name.as_str()
                ))
                .size(font_size::BODY_LARGE)
                .font(styles::SEMIBOLD)
                .width(Length::Fill),
                action(
                    if run.is_finished() {
                        fl!("ui-tool-cmdlib-runbooks-run-close")
                    } else {
                        fl!("ui-tool-cmdlib-runbooks-run-stop")
                    },
                    false,
                    Some(send(RunbookMessage::RunStop)),
                ),
            ]
            .spacing(spacing::SM)
            .align_y(iced::Alignment::Center),
            text(fl!(
                "ui-tool-cmdlib-runbooks-run-mode",
                mode = mode_name(run.runbook.failure_mode)
            ))
            .size(font_size::CAPTION)
            .style(text::secondary),
            text(fl!("ui-tool-cmdlib-runbooks-run-manual"))
                .size(font_size::CAPTION)
                .style(text::secondary),
        ]
        .spacing(spacing::SM);
        if !run.begun {
            panel = panel.push(variables_view(tab, run));
        } else {
            panel = panel.push(self.steps_view(tab, run, library, has_terminal, warning));
        }
        Some(
            container(panel)
                .padding(spacing::MD)
                .width(Length::Fill)
                .style(styles::card)
                .into(),
        )
    }

    fn steps_view<'a>(
        &'a self,
        tab: TabId,
        run: &'a Run,
        library: &'a UserLibrary,
        has_terminal: bool,
        warning: Option<String>,
    ) -> Element<'a, Message> {
        let mut steps: Column<'a, Message> = Column::new().spacing(spacing::SM);
        if run.runbook.steps.is_empty() {
            steps = steps.push(
                text(fl!("ui-tool-cmdlib-runbooks-run-no-steps"))
                    .size(font_size::BODY)
                    .style(text::secondary),
            );
        }
        for (index, step) in run.runbook.steps.iter().enumerate() {
            let status = run
                .statuses
                .get(index)
                .copied()
                .unwrap_or(StepStatus::Pending);
            let is_current = index == run.current && !run.is_finished();
            let number = index + 1;
            let line = row![
                text(fl!(
                    "ui-tool-cmdlib-runbooks-run-step-line",
                    number = number,
                    title = step_title(step, library)
                ))
                .size(font_size::BODY)
                .font(if is_current {
                    styles::SEMIBOLD
                } else {
                    iced::Font::DEFAULT
                })
                .width(Length::Fill),
                text(status_name(status))
                    .size(font_size::CAPTION)
                    .style(text::secondary),
            ]
            .spacing(spacing::SM)
            .align_y(iced::Alignment::Center);
            let mut block: Column<'a, Message> = column![line].spacing(spacing::SM);
            if is_current {
                if let Some(note) = step.description() {
                    block = block.push(text(note).size(font_size::CAPTION).style(text::secondary));
                }
                block = block.push(self.current_view(tab, library, has_terminal, warning.clone()));
            }
            steps = steps.push(
                container(block)
                    .padding(spacing::SM)
                    .width(Length::Fill)
                    .style(styles::field_box),
            );
        }
        if run.is_finished() {
            steps = steps.push(
                text(fl!("ui-tool-cmdlib-runbooks-run-finished"))
                    .size(font_size::BODY)
                    .font(styles::SEMIBOLD),
            );
        }
        steps.into()
    }

    /// The command of the current step with the buttons to send, type, copy or skip it.
    fn current_view<'a>(
        &'a self,
        tab: TabId,
        library: &'a UserLibrary,
        has_terminal: bool,
        warning: Option<String>,
    ) -> Element<'a, Message> {
        let send = move |message| send(tab, message);
        let resolved = self.resolve_current(library).map(|(_, resolved)| resolved);
        let ready = matches!(resolved, Some(Ok(_)));
        let mut body: Column<'a, Message> = Column::new().spacing(spacing::SM);
        match resolved {
            Some(Ok(ready)) => {
                body = body.push(
                    container(
                        text(ready.step.command.clone())
                            .font(crate::tools::BOX_FONT)
                            .size(font_size::BODY),
                    )
                    .padding(spacing::SM)
                    .width(Length::Fill)
                    .style(styles::field_box),
                );
                if ready.step.literal {
                    body = body.push(
                        text(fl!("ui-tool-cmdlib-runbooks-run-literal"))
                            .size(font_size::CAPTION)
                            .style(text::secondary),
                    );
                }
            }
            Some(Err(problem)) => {
                body = body.push(
                    text(problem_text(&problem))
                        .size(font_size::BODY)
                        .style(styles::error_text),
                );
            }
            None => {}
        }
        if let Some(warning) = warning {
            body = body.push(
                text(warning)
                    .size(font_size::BODY)
                    .style(styles::error_text),
            );
        } else if !has_terminal {
            body = body.push(
                text(fl!("ui-tool-cmdlib-send-none"))
                    .size(font_size::CAPTION)
                    .style(text::secondary),
            );
        }
        let can_send = ready && has_terminal;
        body.push(
            row![
                action(
                    fl!("ui-tool-cmdlib-runbooks-run-send"),
                    true,
                    can_send.then(|| send(RunbookMessage::RunSend)),
                ),
                action(
                    fl!("ui-tool-cmdlib-insert"),
                    false,
                    can_send.then(|| send(RunbookMessage::RunInsert)),
                ),
                action(
                    fl!("ui-tool-cmdlib-copy"),
                    false,
                    ready.then(|| send(RunbookMessage::RunCopy)),
                ),
                action(
                    fl!("ui-tool-cmdlib-runbooks-run-skip"),
                    false,
                    Some(send(RunbookMessage::RunSkip)),
                ),
            ]
            .spacing(spacing::SM),
        )
        .into()
    }
}

/// The variables of a run to be confirmed, each prefilled, and the button that starts it.
fn variables_view<'a>(tab: TabId, run: &'a Run) -> Element<'a, Message> {
    let send = move |message| send(tab, message);
    let mut form: Column<'a, Message> = column![
        text(fl!("ui-tool-cmdlib-runbooks-run-variables"))
            .size(font_size::BODY)
            .font(styles::SEMIBOLD),
        text(fl!("ui-tool-cmdlib-runbooks-run-variables-hint"))
            .size(font_size::CAPTION)
            .style(text::secondary),
    ]
    .spacing(spacing::SM);
    for (index, (name, value)) in run.values.iter().enumerate() {
        form = form.push(
            row![
                text(runbooks::format_reference(name))
                    .size(font_size::BODY)
                    .width(VARIABLE_NAME_WIDTH.max(LABEL_WIDTH)),
                text_input("", value)
                    .size(font_size::BODY)
                    .padding(crate::tools::INPUT_PADDING)
                    .style(styles::text_input)
                    .on_input(move |typed| send(RunbookMessage::RunValue(index, typed))),
            ]
            .spacing(spacing::SM)
            .align_y(iced::Alignment::Center),
        );
    }
    form.push(Row::new().push(action(
        fl!("ui-tool-cmdlib-runbooks-run-start"),
        true,
        Some(send(RunbookMessage::RunBegin)),
    )))
    .into()
}

/// The title of a step in the run's list: the action in the language of the window, else the
/// title it had when the step was made, else its identity; the command for a literal step.
fn step_title(step: &Step, library: &UserLibrary) -> String {
    match step {
        Step::Action {
            action_id,
            action_title,
            ..
        } => shown_action(library, action_id).map_or_else(
            || {
                if action_title.trim().is_empty() {
                    action_id.clone()
                } else {
                    action_title.clone()
                }
            },
            |command| command.title.clone(),
        ),
        Step::Literal { command, .. } => command_library::command_preview(command),
    }
}

/// The name of a status.
fn status_name(status: StepStatus) -> String {
    match status {
        StepStatus::Pending => fl!("ui-tool-cmdlib-runbooks-run-status-pending"),
        StepStatus::Sent => fl!("ui-tool-cmdlib-runbooks-run-status-sent"),
        StepStatus::Inserted => fl!("ui-tool-cmdlib-runbooks-run-status-inserted"),
        StepStatus::Copied => fl!("ui-tool-cmdlib-runbooks-run-status-copied"),
        StepStatus::Skipped => fl!("ui-tool-cmdlib-runbooks-run-status-skipped"),
    }
}

/// Why a step cannot be sent, in the language of the window.
fn problem_text(problem: &RunProblem) -> String {
    match problem {
        RunProblem::CommandMissing => fl!("ui-tool-cmdlib-runbooks-run-err-empty"),
        RunProblem::CommandTooLong => fl!(
            "ui-tool-cmdlib-runbooks-run-err-long",
            max = MAX_COMMAND_LENGTH
        ),
        RunProblem::Hidden => fl!("ui-tool-cmdlib-runbooks-run-err-hidden"),
        RunProblem::ActionMissing => fl!("ui-tool-cmdlib-runbooks-run-err-action-gone"),
        RunProblem::NoTemplate => fl!("ui-tool-cmdlib-runbooks-run-err-no-template"),
        RunProblem::UnknownVariables(names) => fl!(
            "ui-tool-cmdlib-runbooks-run-err-unknown",
            names = names
                .iter()
                .map(|name| runbooks::format_reference(name))
                .collect::<Vec<_>>()
                .join(", ")
        ),
        RunProblem::Values(errors) => fl!(
            "ui-tool-cmdlib-runbooks-run-err-values",
            reason = errors.iter().map(value_error).collect::<Vec<_>>().join(" ")
        ),
        RunProblem::Generate(error) => generate_error(error),
    }
}

#[cfg(test)]
mod tests;
