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

//! "Share folder", as the C# `FileShareService`: what the start dialog asks, the server run
//! as a stream of updates, and the URL and `curl` command the user copies.
//!
//! The server itself is `heimdall_files::share`. This is stricter than the C# in what the
//! user decides: the address it listens on is chosen in the dialog (this computer alone by
//! default, never every interface), and a share ends after a time.

use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::path::PathBuf;
use std::time::Duration;

use heimdall_core::settings::{Settings, file_share_minutes_accepted, file_share_port_accepted};
use heimdall_core::tools::network_interfaces::{OperStatus, run_interfaces_command};
use heimdall_files::share::{Share, ShareConfig, ShareError};
use tokio::sync::mpsc;
use tokio_stream::Stream;
use tokio_stream::wrappers::ReceiverStream;
use tokio_util::sync::CancellationToken;

pub use heimdall_files::share::{BindChoice, ListingLabels, RequestRecord, ShareEvent, StopReason};

/// The placeholder the `curl` command leaves for the file to fetch, as the C#.
pub const CURL_FILE_PLACEHOLDER: &str = "<filename>";

/// Updates queued for the window: more are dropped, request reports first.
const EVENT_QUEUE: usize = heimdall_files::share::EVENT_QUEUE;

/// Seconds in a minute.
const SECONDS_PER_MINUTE: u64 = 60;

/// An address of this computer a share may listen on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShareAddress {
    /// The interface's name.
    pub name: String,
    /// Its address.
    pub address: IpAddr,
}

/// The up interfaces of this computer with an IPv4 address that is neither loopback nor
/// link-local, for the user to choose among. Runs a system program: not for the UI thread.
#[must_use]
pub fn local_addresses() -> Vec<ShareAddress> {
    let Ok(interfaces) = run_interfaces_command() else {
        return Vec::new();
    };
    let mut found: Vec<ShareAddress> = interfaces
        .into_iter()
        .filter(|nic| nic.status == OperStatus::Up)
        .filter_map(|nic| {
            let address = nic.ipv4.parse::<Ipv4Addr>().ok()?;
            (!address.is_loopback() && !address.is_link_local() && !address.is_unspecified())
                .then_some(ShareAddress {
                    name: nic.name,
                    address: IpAddr::V4(address),
                })
        })
        .collect();
    found.dedup_by(|a, b| a.address == b.address);
    found
}

/// What the status line says of a share.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ShareNotice {
    /// It listens on this address (the token is not part of it).
    Started(String),
    /// The user stopped it.
    Stopped,
    /// Its time was over.
    Expired,
    /// It did not start.
    Failed(ShareFailure),
    /// The URL was copied.
    UrlCopied,
    /// The `curl` command was copied.
    CommandCopied,
}

/// What stops the start dialog from starting a share.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShareProblem {
    /// No folder is chosen.
    NoFolder,
    /// The port is not a number a share accepts.
    Port,
    /// The time is not a number of minutes a share accepts.
    Minutes,
}

/// A field of the start dialog typed in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShareField {
    /// The port.
    Port,
    /// The minutes the share lasts.
    Minutes,
}

/// The start dialog, as filled. The address is the loopback until the user chooses another.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShareForm {
    /// The folder to share.
    pub folder: Option<PathBuf>,
    /// Where to listen.
    pub bind: BindChoice,
    /// The addresses of this computer, once listed.
    pub addresses: Vec<ShareAddress>,
    /// Whether the addresses are still being listed.
    pub listing: bool,
    /// The port as typed; 0 is a free one.
    pub port: String,
    /// The minutes as typed.
    pub minutes: String,
}

impl ShareForm {
    /// A form with the settings' port and time, and the loopback.
    #[must_use]
    pub fn new(settings: &Settings) -> Self {
        Self {
            folder: None,
            bind: BindChoice::default(),
            addresses: Vec::new(),
            listing: true,
            port: settings.file_share_port.to_string(),
            minutes: settings.file_share_minutes.to_string(),
        }
    }

    /// Types `value` in `field`.
    pub fn set(&mut self, field: ShareField, value: String) {
        match field {
            ShareField::Port => self.port = value,
            ShareField::Minutes => self.minutes = value,
        }
    }

    /// The port typed, when a share accepts it.
    #[must_use]
    pub fn port_value(&self) -> Option<u16> {
        let port = self.port.trim().parse::<u32>().ok()?;
        if file_share_port_accepted(port) {
            u16::try_from(port).ok()
        } else {
            None
        }
    }

    /// The time typed, when a share accepts it.
    #[must_use]
    pub fn lifetime(&self) -> Option<Duration> {
        let minutes = self.minutes.trim().parse::<u32>().ok()?;
        file_share_minutes_accepted(minutes)
            .then(|| Duration::from_secs(u64::from(minutes) * SECONDS_PER_MINUTE))
    }

    /// What stops the share from starting, one thing at a time.
    #[must_use]
    pub fn problem(&self) -> Option<ShareProblem> {
        if self.folder.is_none() {
            Some(ShareProblem::NoFolder)
        } else if self.port_value().is_none() {
            Some(ShareProblem::Port)
        } else if self.lifetime().is_none() {
            Some(ShareProblem::Minutes)
        } else {
            None
        }
    }
}

/// What a share is started with.
#[derive(Debug, Clone)]
pub struct ShareRequest {
    /// The folder.
    pub folder: PathBuf,
    /// Where it listens.
    pub bind: BindChoice,
    /// The port; 0 for a free one.
    pub port: u16,
    /// How long it lasts.
    pub lifetime: Duration,
    /// Stops it.
    pub cancel: CancellationToken,
}

/// A share that listens: what the user needs to reach it. The token is a secret: it is
/// shown only by the URL and the command, and not by `Debug`.
#[derive(Clone)]
pub struct ShareInfo {
    /// The folder shared, as the server canonicalised it.
    pub folder: PathBuf,
    /// The address it listens on.
    pub addr: SocketAddr,
    /// How long it lasts.
    pub lifetime: Duration,
    token: String,
}

impl std::fmt::Debug for ShareInfo {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ShareInfo")
            .field("folder", &self.folder)
            .field("addr", &self.addr)
            .field("lifetime", &self.lifetime)
            .finish_non_exhaustive()
    }
}

impl ShareInfo {
    /// The address of the share's root, with the token: the link to open.
    #[must_use]
    pub fn url(&self) -> String {
        format!("http://{}/?token={}", self.addr, self.token)
    }

    /// A `curl` command fetching a file, the token in the header and not in the address, as
    /// the C#.
    #[must_use]
    pub fn curl_command(&self) -> String {
        format!(
            "curl -H \"Authorization: Bearer {}\" -O \"http://{}/{CURL_FILE_PLACEHOLDER}\"",
            self.token, self.addr
        )
    }

    /// The address without the token, for the log and the status line.
    #[must_use]
    pub fn address_text(&self) -> String {
        self.addr.to_string()
    }
}

/// Why a share did not start.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShareFailure {
    /// The folder cannot be read.
    Folder,
    /// The path is not a folder.
    NotAFolder,
    /// The address chosen means every interface.
    EveryInterface,
    /// The address could not be used, the port being taken most often.
    Bind,
    /// No random token could be made.
    Random,
}

impl From<&ShareError> for ShareFailure {
    fn from(error: &ShareError) -> Self {
        match error {
            ShareError::Root(_) => Self::Folder,
            ShareError::NotAFolder => Self::NotAFolder,
            ShareError::EveryInterface => Self::EveryInterface,
            ShareError::Bind(_) => Self::Bind,
            ShareError::Random => Self::Random,
        }
    }
}

/// What a share reports while it runs.
#[derive(Debug, Clone)]
pub enum ShareUpdate {
    /// It listens.
    Started(ShareInfo),
    /// It did not start.
    Failed(ShareFailure),
    /// A request was answered, or the share ended.
    Event(ShareEvent),
}

/// Starts the share `request` asks for, and reports it. The stream ends with the share.
pub fn share_events(
    request: ShareRequest,
    labels: ListingLabels,
) -> impl Stream<Item = ShareUpdate> + Send {
    let (sender, receiver) = mpsc::channel(EVENT_QUEUE);
    tokio::spawn(async move {
        let config = ShareConfig::new(
            request.folder,
            request.bind,
            request.port,
            request.lifetime,
            labels,
        );
        let share = match Share::bind(config).await {
            Ok(share) => share,
            Err(error) => {
                log::warn!("share: not started: {error}");
                let _ = sender
                    .send(ShareUpdate::Failed(ShareFailure::from(&error)))
                    .await;
                return;
            }
        };
        let info = ShareInfo {
            folder: share.root().to_path_buf(),
            addr: share.local_addr(),
            lifetime: request.lifetime,
            token: share.token().to_owned(),
        };
        // The token is never logged: the address and the folder are.
        log::info!(
            "share: started on {} for {}",
            info.addr,
            info.folder.display()
        );
        let _ = sender.send(ShareUpdate::Started(info)).await;
        let (events, mut reported) = mpsc::channel(EVENT_QUEUE);
        let running = tokio::spawn(share.run(request.cancel, events));
        // Request reports never wait for the window: when its queue is full they are
        // dropped and counted, and the count is sent before the end.
        let mut lost = 0_u64;
        while let Some(event) = reported.recv().await {
            if matches!(event, ShareEvent::Request(_)) {
                if sender.try_send(ShareUpdate::Event(event)).is_err() {
                    lost += 1;
                }
                continue;
            }
            if let ShareEvent::Stopped(reason) = &event {
                log::info!("share: stopped ({reason:?})");
                if lost > 0 {
                    let _ = sender
                        .send(ShareUpdate::Event(ShareEvent::Dropped(lost)))
                        .await;
                }
            }
            let _ = sender.send(ShareUpdate::Event(event)).await;
        }
        let _ = running.await;
    });
    ReceiverStream::new(receiver)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_new_form_listens_on_the_loopback_with_the_settings() {
        let settings = Settings::default();
        let form = ShareForm::new(&settings);
        assert_eq!(form.bind, BindChoice::Loopback);
        assert_eq!(form.port, "8080");
        assert_eq!(form.minutes, "30");
        assert_eq!(form.lifetime(), Some(Duration::from_mins(30)));
        assert_eq!(form.port_value(), Some(8080));
    }

    #[test]
    fn a_form_says_what_stops_it_one_thing_at_a_time() {
        let mut form = ShareForm::new(&Settings::default());
        assert_eq!(form.problem(), Some(ShareProblem::NoFolder));
        form.folder = Some(PathBuf::from("shared"));
        assert_eq!(form.problem(), None);
        form.set(ShareField::Port, "70000".to_owned());
        assert_eq!(form.problem(), Some(ShareProblem::Port));
        form.set(ShareField::Port, "0".to_owned());
        assert_eq!(form.port_value(), Some(0), "0 asks for a free port");
        form.set(ShareField::Minutes, "0".to_owned());
        assert_eq!(form.problem(), Some(ShareProblem::Minutes));
        form.set(ShareField::Minutes, "1441".to_owned());
        assert_eq!(form.problem(), Some(ShareProblem::Minutes));
        form.set(ShareField::Minutes, "abc".to_owned());
        assert_eq!(form.problem(), Some(ShareProblem::Minutes));
    }

    #[test]
    fn the_url_and_the_command_carry_the_token_but_debug_does_not() {
        let info = ShareInfo {
            folder: PathBuf::from("shared"),
            addr: "127.0.0.1:8080".parse().unwrap(),
            lifetime: Duration::from_mins(1),
            token: "secrettoken".to_owned(),
        };
        assert_eq!(info.url(), "http://127.0.0.1:8080/?token=secrettoken");
        assert_eq!(
            info.curl_command(),
            "curl -H \"Authorization: Bearer secrettoken\" -O \"http://127.0.0.1:8080/<filename>\""
        );
        assert!(!format!("{info:?}").contains("secrettoken"));
        assert!(!info.address_text().contains("secrettoken"));
    }
}
