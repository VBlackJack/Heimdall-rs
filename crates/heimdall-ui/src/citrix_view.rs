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

//! A Citrix application's tab, as the info panel of the C# `EmbeddedCitrixView` in its
//! external mode: its name, its protocol and its state; its `StoreFront`, its application
//! and how it launched; when it launched, the launcher's exit code once it exited, and its
//! client's state; and its Terminate, as the C# one, for a client it found itself. And the
//! work behind it, off the UI thread: the launch, each look at the client, and ending it.

use std::sync::Arc;

use heimdall_app::citrix::{self, CitrixLaunch, CitrixRefusal};
use heimdall_app::citrix_session::{
    self, ChildWatch, CitrixPane, ClientLister, ClientState, LaunchMethod, LauncherStatus,
    LauncherWatch, ListError, Probe, Tasklist, Untracked,
};
use heimdall_app::citrix_terminate::{ClientTerminator, Taskkill, TerminateOffer, TerminateResult};
use heimdall_app::{Message as AppMessage, ProfileKind, SessionState, TabId, server_text};
use heimdall_core::profile::CitrixProfile;
use iced::widget::{Column, button, center, column, container, row, text};
use iced::{Alignment, Element, Task};

use crate::detail_view;
use crate::i18n::fl;
use crate::shell::Message;
use crate::styles;
use crate::tokens::{font_size, spacing};

/// The card's widest, as the C# panel's `MaxWidth`.
const CARD_MAX_WIDTH: f32 = 460.0;

/// What the line of the launch method says.
fn method_text(method: LaunchMethod) -> String {
    match method {
        LaunchMethod::CacheLine => fl!("ui-citrix-tab-mode-cache"),
        LaunchMethod::IcaFile => fl!("ui-citrix-tab-mode-ica-file"),
        LaunchMethod::StoreFront => fl!("ui-citrix-tab-mode-storefront"),
    }
}

/// What the tab says of its client in state `state`.
#[must_use]
pub fn client_text(state: &ClientState) -> String {
    match state {
        ClientState::Launching => fl!("ui-citrix-tab-launching"),
        ClientState::Running(pid) => fl!("ui-citrix-tab-running", pid = pid.to_string()),
        ClientState::NotFoundYet => fl!("ui-citrix-tab-not-found"),
        ClientState::Ended(pid) => fl!("ui-citrix-tab-ended", pid = pid.to_string()),
        ClientState::Shared => fl!("ui-citrix-tab-shared"),
        ClientState::LauncherFailed(code) => {
            fl!("ui-citrix-tab-launcher-failed", code = code.to_string())
        }
        ClientState::NotStarted(refusal) => crate::texts::citrix_refusal(refusal),
        ClientState::Untracked(Untracked::WindowsOnly) => fl!("ui-citrix-tab-windows-only"),
        ClientState::Untracked(Untracked::Unavailable) => fl!("ui-citrix-tab-unavailable"),
        ClientState::Untracked(Untracked::TimedOut) => fl!("ui-citrix-tab-timed-out"),
    }
}

/// The lines under the state, each in the secondary colour: the `StoreFront` and the
/// application when the profile names them, the launch method, when it launched, and the
/// launcher's exit code once it exited with one.
#[must_use]
pub fn detail_lines(profile: &CitrixProfile, pane: &CitrixPane) -> Vec<String> {
    let named = |value: Option<&str>| {
        value
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(server_text)
    };
    let mut lines = Vec::new();
    if let Some(url) = named(profile.store_front_url.as_deref()) {
        lines.push(fl!("ui-citrix-tab-storefront", url = url));
    }
    if let Some(name) = named(profile.app_name.as_deref()) {
        lines.push(fl!("ui-citrix-tab-application", name = name));
    }
    lines.push(method_text(pane.method));
    if let Some(time) = pane.launched_clock() {
        lines.push(fl!("ui-citrix-tab-launched-at", time = time));
    }
    if let Some(code) = pane.tracker.exit_code() {
        lines.push(fl!("ui-citrix-tab-launcher-exit", code = code.to_string()));
    }
    lines
}

/// What the tab says of the last request to end its client, while there is one to say.
#[must_use]
pub fn terminate_text(offer: &TerminateOffer) -> Option<String> {
    Some(match offer {
        TerminateOffer::Nothing | TerminateOffer::Terminate => return None,
        TerminateOffer::Pending { force: false } => fl!("ui-citrix-tab-terminating"),
        TerminateOffer::Pending { force: true } => fl!("ui-citrix-tab-force-terminating"),
        TerminateOffer::Asked { force: false } => fl!("ui-citrix-tab-terminate-asked"),
        TerminateOffer::Asked { force: true } => fl!("ui-citrix-tab-terminate-forced"),
        TerminateOffer::Force(TerminateResult::Requested | TerminateResult::Gone) => {
            fl!("ui-citrix-tab-terminate-still-running")
        }
        TerminateOffer::Force(TerminateResult::Refused(code)) => {
            fl!("ui-citrix-tab-terminate-refused", code = code.to_string())
        }
        TerminateOffer::Force(TerminateResult::TimedOut) => {
            fl!("ui-citrix-tab-terminate-timed-out")
        }
        TerminateOffer::Force(TerminateResult::NotRun(_)) => fl!("ui-citrix-tab-terminate-not-run"),
    })
}

/// The question asked before the client is asked to close, or forced when `force`: its
/// title, its text and its action, as the C# confirmation.
#[must_use]
pub fn terminate_question(force: bool) -> (String, String, String) {
    if force {
        (
            fl!("ui-citrix-force-terminate-title"),
            fl!("ui-citrix-force-terminate-body"),
            fl!("ui-citrix-tab-force-terminate"),
        )
    } else {
        (
            fl!("ui-citrix-terminate-title"),
            fl!("ui-citrix-terminate-body"),
            fl!("ui-citrix-tab-terminate"),
        )
    }
}

/// The button ending tab `tab`'s session, in the danger colour, when `offer` offers one.
fn terminate_button<'a>(tab: TabId, offer: &TerminateOffer) -> Option<Element<'a, Message>> {
    let (label, force) = match offer {
        TerminateOffer::Terminate => (fl!("ui-citrix-tab-terminate"), false),
        TerminateOffer::Force(_) => (fl!("ui-citrix-tab-force-terminate"), true),
        TerminateOffer::Nothing | TerminateOffer::Pending { .. } | TerminateOffer::Asked { .. } => {
            return None;
        }
    };
    Some(
        button(text(label))
            .style(styles::danger)
            .on_press(Message::App(AppMessage::CitrixTerminate { tab, force }))
            .into(),
    )
}

/// Tab `tab` of `profile`, launched as `pane` says, its state `state`, offering `offer` to
/// end its session.
pub fn view<'a>(
    profile: &CitrixProfile,
    pane: &CitrixPane,
    state: SessionState,
    tab: TabId,
    offer: &TerminateOffer,
) -> Element<'a, Message> {
    let client = pane.tracker.state();
    let failed = matches!(
        client,
        ClientState::LauncherFailed(_) | ClientState::NotStarted(_)
    );
    let mut card = column![
        text(server_text(&profile.name))
            .size(font_size::DISPLAY)
            .font(detail_view::BOLD),
        row![
            detail_view::pill(ProfileKind::Citrix),
            detail_view::status(Some(state))
        ]
        .spacing(spacing::SM)
        .align_y(Alignment::Center),
    ]
    .spacing(spacing::SM);
    let details = detail_lines(profile, pane)
        .into_iter()
        .fold(Column::new().spacing(spacing::XS), |lines, line| {
            lines.push(text(line).size(font_size::BODY).style(text::secondary))
        });
    card = card.push(details).push(
        text(client_text(client))
            .size(font_size::BODY)
            .style(if failed { text::danger } else { text::default }),
    );
    if let Some(said) = terminate_text(offer) {
        let refused = matches!(offer, TerminateOffer::Force(result) if !result.accepted());
        card = card.push(text(said).size(font_size::BODY).style(if refused {
            text::danger
        } else {
            text::secondary
        }));
    }
    if let Some(button) = terminate_button(tab, offer) {
        card = card.push(button);
    }
    center(
        container(card)
            .padding(spacing::XL)
            .max_width(CARD_MAX_WIDTH)
            .style(container::bordered_box),
    )
    .into()
}

/// Launches `launch` for tab `tab`, the client's processes listed first.
pub(crate) fn launch(tab: TabId, name: String, launch: CitrixLaunch) -> Task<Message> {
    Task::future(async move {
        // Listing processes and starting one wait on the system: off the UI thread.
        let result = tokio::task::spawn_blocking(move || {
            citrix_session::launch_tracked(
                Arc::new(Tasklist),
                citrix_session::LIST_TIME_LIMIT,
                || citrix::launch(&launch).map(ChildWatch::shared),
            )
        })
        .await
        .unwrap_or_else(|error| Err(CitrixRefusal::NotStarted(error.to_string())));
        Message::App(AppMessage::CitrixLaunched { tab, name, result })
    })
}

/// Looks at tab `tab`'s launcher and, when `lists`, lists the client's processes.
pub(crate) fn probe(tab: TabId, launcher: Arc<dyn LauncherWatch>, lists: bool) -> Task<Message> {
    Task::future(async move {
        let probe = tokio::task::spawn_blocking(move || {
            let lister = lists.then(|| Arc::new(Tasklist) as Arc<dyn ClientLister>);
            citrix_session::probe(lister, launcher.as_ref(), citrix_session::LIST_TIME_LIMIT)
        })
        .await
        .unwrap_or_else(|error| Probe {
            launcher: LauncherStatus::Unknown,
            clients: Err(ListError::Failed(error.to_string())),
        });
        Message::App(AppMessage::CitrixProbed { tab, probe })
    })
}

/// Asks tab `tab`'s client `pid` to close, or forces it when `force`, by `taskkill.exe`.
pub(crate) fn terminate(tab: TabId, pid: u32, force: bool) -> Task<Message> {
    Task::future(async move {
        // `taskkill.exe` waits on the client: off the UI thread.
        let result = tokio::task::spawn_blocking(move || Taskkill.terminate(pid, force))
            .await
            .unwrap_or_else(|error| TerminateResult::NotRun(error.to_string()));
        Message::App(AppMessage::CitrixTerminated { tab, pid, result })
    })
}
