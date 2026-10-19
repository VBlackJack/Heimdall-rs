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

//! "Share folder", as the C# toolbar toggle (`FileShareService`): the start dialog, the share
//! running until it is stopped, its time is over or the application ends, and the copy of
//! its URL and `curl` command.
//!
//! One share at a time. The toggle opens the start dialog when nothing is shared and stops
//! the share when one is; the bar shows the time left meanwhile.

use std::path::PathBuf;
use std::time::{Duration, Instant};

use tokio_util::sync::CancellationToken;

use super::{App, Dialog, Effect, Notice};
use crate::share::{
    BindChoice, ShareAddress, ShareEvent, ShareField, ShareForm, ShareInfo, ShareNotice,
    ShareRequest, ShareUpdate, StopReason,
};

/// What the user does with the share, and what it reports.
#[derive(Debug, Clone)]
pub enum ShareMessage {
    /// The toggle: the start dialog, or the end of the share running.
    Toggle,
    /// A field of the dialog was typed in.
    Field {
        /// Which.
        field: ShareField,
        /// Its text.
        value: String,
    },
    /// An address to listen on was chosen.
    Bind(BindChoice),
    /// The user asked to choose the folder.
    ChooseFolder,
    /// The folder was chosen.
    FolderChosen(PathBuf),
    /// The addresses of this computer were listed.
    Addresses(Vec<ShareAddress>),
    /// What a share started reports; `None` is the end of its stream.
    Update {
        /// The share.
        run: u64,
        /// What it reports.
        update: Option<ShareUpdate>,
    },
    /// Copies the URL, with the token.
    CopyUrl,
    /// Copies the `curl` command, the token in its header.
    CopyCommand,
    /// The bar redraws the time left.
    Tick,
}

/// A share being started or running.
#[derive(Debug)]
pub(super) struct ShareRun {
    run: u64,
    cancel: CancellationToken,
    lifetime: Duration,
    /// Set once the server listens.
    started: Option<(ShareInfo, Instant)>,
    /// Requests answered so far.
    requests: u64,
}

/// What the bar says of the share.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShareStatus {
    /// It is starting.
    Starting,
    /// It runs, for this long still.
    Running {
        /// The time left.
        remaining: Duration,
    },
}

impl App {
    /// Applies a message about "Share folder".
    pub(super) fn share_message(&mut self, message: ShareMessage) -> Vec<Effect> {
        match message {
            ShareMessage::Toggle => {
                if self.share.is_some() {
                    self.stop_share();
                    self.tell(Notice::Share(ShareNotice::Stopped));
                    return Vec::new();
                }
                self.dialog = Some(Dialog::ShareFolder(Box::new(ShareForm::new(
                    &self.settings,
                ))));
                vec![Effect::ListShareAddresses]
            }
            ShareMessage::Field { field, value } => {
                if let Some(Dialog::ShareFolder(form)) = &mut self.dialog {
                    form.set(field, value);
                }
                Vec::new()
            }
            ShareMessage::Bind(choice) => {
                if let Some(Dialog::ShareFolder(form)) = &mut self.dialog {
                    form.bind = choice;
                }
                Vec::new()
            }
            ShareMessage::ChooseFolder => {
                let start = match &self.dialog {
                    Some(Dialog::ShareFolder(form)) => form.folder.clone(),
                    _ => return Vec::new(),
                };
                vec![Effect::PickShareFolder { start }]
            }
            ShareMessage::FolderChosen(folder) => {
                if let Some(Dialog::ShareFolder(form)) = &mut self.dialog {
                    form.folder = Some(folder);
                }
                Vec::new()
            }
            ShareMessage::Addresses(addresses) => {
                if let Some(Dialog::ShareFolder(form)) = &mut self.dialog {
                    form.addresses = addresses;
                    form.listing = false;
                }
                Vec::new()
            }
            ShareMessage::Update { run, update } => self.share_update(run, update),
            ShareMessage::CopyUrl => match self.share_info() {
                Some(info) => {
                    let url = info.url();
                    self.tell(Notice::Share(ShareNotice::UrlCopied));
                    vec![Effect::WriteClipboard(url)]
                }
                None => Vec::new(),
            },
            ShareMessage::CopyCommand => match self.share_info() {
                Some(info) => {
                    let command = info.curl_command();
                    self.tell(Notice::Share(ShareNotice::CommandCopied));
                    vec![Effect::WriteClipboard(command)]
                }
                None => Vec::new(),
            },
            ShareMessage::Tick => Vec::new(),
        }
    }

    /// Starts the share the dialog asks for, once nothing stops it.
    pub(super) fn confirm_share_dialog(&mut self) -> Vec<Effect> {
        let Some(Dialog::ShareFolder(form)) = &self.dialog else {
            return Vec::new();
        };
        if self.share.is_some() || form.problem().is_some() {
            return Vec::new();
        }
        let (Some(folder), Some(port), Some(lifetime)) =
            (form.folder.clone(), form.port_value(), form.lifetime())
        else {
            return Vec::new();
        };
        let bind = form.bind;
        self.dialog = None;
        self.next_share += 1;
        let run = self.next_share;
        let cancel = CancellationToken::new();
        self.share = Some(ShareRun {
            run,
            cancel: cancel.clone(),
            lifetime,
            started: None,
            requests: 0,
        });
        vec![Effect::StartShare {
            run,
            request: Box::new(ShareRequest {
                folder,
                bind,
                port,
                lifetime,
                cancel,
            }),
        }]
    }

    /// What a share reports. A share that was stopped meanwhile is not heard again.
    fn share_update(&mut self, run: u64, update: Option<ShareUpdate>) -> Vec<Effect> {
        let Some(current) = self.share.as_mut().filter(|share| share.run == run) else {
            return Vec::new();
        };
        match update {
            Some(ShareUpdate::Started(info)) => {
                let address = info.address_text();
                current.lifetime = info.lifetime;
                current.started = Some((info, Instant::now()));
                self.tell(Notice::Share(ShareNotice::Started(address)));
                // The URL and the command are offered at once, with the warning.
                self.dialog = Some(Dialog::ShareStarted);
            }
            Some(ShareUpdate::Failed(failure)) => {
                self.share = None;
                self.tell(Notice::Share(ShareNotice::Failed(failure)));
            }
            Some(ShareUpdate::Event(ShareEvent::Request(_))) => current.requests += 1,
            Some(ShareUpdate::Event(ShareEvent::Dropped(lost))) => current.requests += lost,
            Some(ShareUpdate::Event(ShareEvent::Stopped(reason))) => {
                self.share = None;
                let notice = match reason {
                    StopReason::Expired => ShareNotice::Expired,
                    StopReason::Stopped => ShareNotice::Stopped,
                };
                self.tell(Notice::Share(notice));
            }
            // The stream ended without saying why: the share is gone all the same.
            None => self.share = None,
        }
        if self.share.is_none() && matches!(self.dialog, Some(Dialog::ShareStarted)) {
            self.dialog = None;
        }
        Vec::new()
    }

    /// Ends the share, if there is one: its server stops and its connections drop. Also
    /// done when the application ends.
    pub(super) fn stop_share(&mut self) {
        if let Some(share) = self.share.take() {
            share.cancel.cancel();
        }
        if matches!(self.dialog, Some(Dialog::ShareStarted)) {
            self.dialog = None;
        }
    }

    /// The share that listens, for the dialog and the copy buttons.
    #[must_use]
    pub fn share_info(&self) -> Option<&ShareInfo> {
        self.share
            .as_ref()
            .and_then(|share| share.started.as_ref().map(|(info, _)| info))
    }

    /// What the bar says of the share, `None` when nothing is shared.
    #[must_use]
    pub fn share_status(&self) -> Option<ShareStatus> {
        let share = self.share.as_ref()?;
        Some(match &share.started {
            None => ShareStatus::Starting,
            Some((_, since)) => ShareStatus::Running {
                remaining: share.lifetime.saturating_sub(since.elapsed()),
            },
        })
    }

    /// Requests the running share has answered.
    #[must_use]
    pub fn share_requests(&self) -> u64 {
        self.share.as_ref().map_or(0, |share| share.requests)
    }
}
