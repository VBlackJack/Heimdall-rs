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

//! What the status bar says, as the C# Heimdall's: the state of the session shown, read
//! from it each time rather than kept; and a notice of what was just done, shown while
//! the same session is shown in the same state.

use heimdall_core::profile::ProfileId;
use heimdall_core::settings::BroadcastScope;

use super::reconnect::Reopen;
use super::{App, Phase, Tab};
use crate::error::UiError;
use crate::ids::TabId;

/// The state of the session shown.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SessionStatus {
    /// No session shown.
    Ready,
    /// Connecting, or asking something before it can.
    Connecting(String),
    /// Connected.
    Connected(String),
    /// Waiting to connect again.
    Reconnecting(String),
    /// Ended.
    Disconnected(String),
    /// Failed.
    Error(String),
}

/// What was just done, said while the same session is shown in the same state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Notice {
    /// This text was copied.
    Copied(String),
    /// A tunnel opened on this local port, to this host and port.
    TunnelOpened {
        /// Local port.
        port: u16,
        /// Remote host.
        host: String,
        /// Remote port.
        remote_port: u16,
    },
    /// A tunnel could not be opened, for this reason.
    TunnelFailed(UiError),
    /// The tunnel on this local port closed; by itself when there is an error.
    TunnelClosed {
        /// Local port.
        port: u16,
        /// Why, when it was not closed by the user.
        error: Option<UiError>,
    },
    /// Every tunnel was closed.
    AllTunnelsClosed,
    /// A size typed in "Custom resolution" was not one.
    ResolutionInvalid,
    /// The session's size was kept as its profile's own.
    ResolutionSaved,
    /// The session has no saved profile to keep its size in.
    ResolutionSaveUnavailable,
    /// A profile's address is being tested.
    ReachabilityTesting {
        /// Address.
        host: String,
        /// Port.
        port: u16,
    },
    /// It answered, in this many milliseconds.
    Reachable {
        /// Address.
        host: String,
        /// Port.
        port: u16,
        /// How long the connection took.
        millis: u64,
    },
    /// It did not answer, for this reason.
    Unreachable {
        /// Address.
        host: String,
        /// Port.
        port: u16,
        /// Why.
        failure: crate::reachability::Unreached,
    },
    /// This local port was copied.
    PortCopied(u16),
    /// A favorite could not be saved, as the C# says it.
    FavoriteSaveFailed,
    /// The port of this many profiles was set at once.
    BulkPortUpdated(usize),
    /// The port set at once was every profile's already.
    BulkPortUnchanged,
    /// The account of this many profiles was set at once.
    BulkUsernameUpdated(usize),
    /// The account set at once was every profile's already.
    BulkUsernameUnchanged,
    /// A password was saved for this many profiles at once.
    BulkPasswordUpdated {
        /// Profiles saved.
        count: usize,
        /// `WinRM` profiles selected and left alone.
        winrm_skipped: usize,
    },
    /// A password was saved for some of the profiles only: the system's store failed for
    /// the others.
    BulkPasswordPartial {
        /// Profiles saved.
        count: usize,
        /// Profiles it was to be saved for.
        total: usize,
    },
    /// No password was set at once: of those selected, only this many `WinRM` profiles
    /// could have taken one.
    BulkPasswordWinRmSkipped(usize),
    /// The route of this many profiles changed with the bulk "Set gateway"; none when they
    /// all went that way already.
    BulkGatewayUpdated(usize),
    /// The keys trusted were written into the user's OpenSSH `known_hosts`.
    KnownHostsExported {
        /// Keys written.
        count: usize,
        /// The file written.
        path: String,
        /// Servers trusted by a fingerprint alone, left out.
        skipped: usize,
    },
    /// The keys trusted could not be written there, for this reason: a file and what the
    /// system said, or nothing when the home folder is not known.
    KnownHostsExportFailed(String),
    /// A tab was not split further: it shows this many panes already, the most it can, as
    /// the C# `SplitMaxPanesReached`.
    SplitMaxPanesReached(usize),
    /// A tab split, or a pane docked in a split, was not moved to a window of its own, as
    /// the C# `StatusDetachSplitTabRefused`.
    DetachSplitRefused,
    /// An image of the session shown was copied to the clipboard, as the C# says it.
    ScreenshotCopied,
    /// No image of the session shown could be copied.
    ScreenshotFailed,
    /// This folder was created.
    FolderCreated(String),
    /// The full path of an entry of a Files tab was copied, as the C# says it.
    PathCopied(String),
    /// This many entries of a Files tab were cut.
    FilesCut(usize),
    /// The desktop was connected again at the size asked, the server unable to take it
    /// live.
    ResolutionReconnected,
    /// The size chosen is larger than the tab: the desktop is shown scaled.
    ResolutionScaled,
    /// The files copied were not offered to the RDP server: more files and folders than
    /// one copy takes.
    RdpFilesTooMany,
    /// The files copied were not offered to the RDP server: more bytes than one copy takes.
    RdpFilesTooLarge,
    /// Saving the RDP server's files ended so.
    RdpFilesSaveEnded(heimdall_rdp::SaveEnd),
    /// A server's file is open in the external editor: each save is sent.
    FilesEditing(String),
    /// A save of a file being edited was sent.
    FilesAutoUploaded(String),
    /// A save of a file being edited was sent with sudo.
    FilesSavedWithSudo(String),
    /// A Files tab's sudo mode was turned on, its folders listed as root, or off.
    FilesSudoMode(bool),
    /// A save of a file being edited was not sent, and is not tried again until saved
    /// again.
    FilesAutoUploadRefused {
        /// The file.
        name: String,
        /// Why.
        error: crate::files::FilesError,
    },
    /// "Paste from Explorer" found no files copied.
    ExplorerHoldsNoFiles,
    /// The entries cut or copied were pasted.
    FilesPasted,
    /// This many entries of a Files tab were copied, to be pasted.
    FilesCopied(usize),
    /// This many sessions were dropped into a folder, none for no folder.
    DroppedProfiles {
        /// How many.
        count: usize,
        /// The folder.
        folder: Option<String>,
    },
    /// A folder of this name was dropped into another.
    DroppedFolder(String),
    /// A folder was dropped where one of its name is already.
    DropRefused,
    /// A Files tab's listing on its way was given up with Escape.
    ListingCancelled,
    /// A session was not opened: this many are open already, the most the settings allow,
    /// as the C# "Embedded session limit reached".
    SessionLimitReached(u32),
    /// The SFTP pane opened beside an SSH shell could not connect, for this reason, and
    /// was closed, as the C# `StatusSftpAutoOpenFailed`.
    SftpAutoOpenFailed(UiError),
    /// An SFTP Files tab was not opened: the SFTP browser is off in the settings, as the C#
    /// `ErrorSftpBrowserDisabled`.
    SftpBrowserDisabled,
    /// Sessions were put before or after another, in this folder, none for no folder, as
    /// the C# "Moved ... within ..."; the session's name when it is one.
    Reordered {
        /// How many.
        count: usize,
        /// Its name, when one moved.
        name: Option<String>,
        /// The folder.
        folder: Option<String>,
    },
    /// The last change of the tree's organization was undone.
    MoveUndone,
    /// The last change could not be undone: what it changed has changed since.
    UndoConflict,
    /// No move made by a drop is there to undo.
    NothingToUndo,
    /// The Wake-on-LAN magic packet was sent, or why not, as the C# status says it.
    WakeOnLan(Result<(), String>),
    /// A macro recording ended with nothing typed.
    MacroNothingRecorded,
    /// The macro of this name was kept.
    MacroSaved(String),
    /// The macro of this name was forgotten.
    MacroDeleted(String),
    /// A macro typed into a session ended.
    MacroEnded {
        /// Its name.
        name: String,
        /// How.
        outcome: crate::macro_player::MacroOutcome,
    },
    /// The note of this file name was opened in the editor.
    NoteOpened(String),
    /// A note could not be written or opened, for this reason.
    NoteFailed(String),
    /// The settings were exported.
    SettingsExported,
    /// The settings could not be exported, for this reason.
    SettingsExportFailed(String),
    /// This many settings were imported.
    SettingsImported(usize),
    /// The settings file holds the settings already in use.
    SettingsImportNothing,
    /// The file is not a settings file, or not one this version reads.
    SettingsImportInvalid,
    /// The settings file was written by a newer version.
    SettingsImportNewer,
    /// The settings file could not be read, for this reason.
    SettingsImportFailed(String),
    /// The gateway of this name was deleted.
    GatewayDeleted(String),
    /// This many sessions of a missing gateway were sent through another.
    GatewaysReassigned(usize),
    /// This many sessions of a missing gateway now connect directly.
    GatewaysCleared(usize),
    /// No session needed a change.
    GatewaysUnchanged,
    /// A delete or a change of permissions of entries did not do them all.
    FilesBatch(crate::files::BatchOutcome),
    /// The entries chosen were duplicated.
    FilesDuplicated,
    /// A transcript was started, in this file.
    TranscriptStarted(String),
    /// A transcript was stopped.
    TranscriptStopped,
    /// A transcript could not be written, for this reason, and stopped.
    TranscriptFailed(String),
    /// The server's folder at this path was bookmarked.
    Bookmarked(String),
    /// This folder of the server was taken off the bookmarks.
    BookmarkRemoved(String),
    /// Broadcast input is on, reaching this scope.
    BroadcastOn(BroadcastScope),
    /// Broadcast input is off.
    BroadcastOff,
    /// Broadcast input will reach this scope when on.
    BroadcastScope(BroadcastScope),
    /// The whole fingerprint of this server's key was copied.
    FingerprintCopied(String),
    /// This server's SSH host key was forgotten.
    HostKeyRemoved(String),
    /// This server's RDP or FTPS certificate was forgotten.
    CertificateForgotten(String),
    /// Every certificate trusted for this RDP or FTPS server was forgotten.
    ServerCertificatesForgotten(String),
    /// The external credential provider gave no password for this session: the user is
    /// asked.
    ProviderNoPassword(String),
    /// The external credential provider could not run, for this reason.
    ProviderFailed(String),
    /// The external credential provider took too long.
    ProviderTimedOut,
    /// The link of this name, opened in a Files tab, points at no folder.
    LinkNotAFolder(String),
    /// A `WinRM` session started through an SSH gateway, where Kerberos is out of reach and
    /// the sign-in falls back to NTLM, as the C# Heimdall warns.
    WinRmGatewayNtlm,
    /// A `WinRM` session started with its TLS certificate checks skipped, as the C# warns.
    WinRmCertificateSkipped,
    /// A Citrix application is being launched, as the C# "Launching Citrix session...".
    CitrixLaunching,
    /// The Citrix application of this profile name was launched, outside Heimdall.
    CitrixLaunched(String),
    /// A Citrix application was not launched, for this reason.
    CitrixRefused(crate::citrix::CitrixRefusal),
    /// The RDP profile of this name opened in Remote Desktop Connection; through this RD
    /// Gateway, which sent it there, when it did not ask for it.
    RdpExternalLaunched {
        /// The profile's name.
        name: String,
        /// The RD Gateway the built-in client does not go through.
        gateway: Option<String>,
    },
    /// An RDP profile did not open in Remote Desktop Connection, for this reason.
    RdpExternalRefused(crate::rdp_external::ExternalRefusal),
    /// The SSH profile of this name opened in `PuTTY`.
    PuttyLaunched(String),
    /// An SSH profile opened in `PuTTY` through its SSH gateway.
    PuttyLaunchedThrough {
        /// The profile's name.
        name: String,
        /// The gateway's name.
        gateway: String,
    },
    /// An SSH profile did not open in `PuTTY`, for this reason.
    PuttyRefused(crate::putty::PuttyRefusal),
    /// X11 forwarding was asked and no X server could be found or started: `PuTTY` started
    /// without it, as the C# `X11ServerNotFound` says.
    X11ServerNotFound,
}

/// A session's state: the one the status bar names, its tab's dot shows and its profile's
/// row in the tree, as the C# Heimdall's connection state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionState {
    /// Connecting, or asking something before it can.
    Connecting,
    /// Waiting to connect again.
    Reconnecting,
    /// Connected.
    Connected,
    /// Ended.
    Ended,
    /// Failed.
    Failed,
}

impl SessionState {
    /// The state of `tab`.
    #[must_use]
    pub fn of(tab: &Tab) -> Self {
        if tab.retry.is_some() {
            return Self::Reconnecting;
        }
        match tab.phase {
            Phase::Connected => Self::Connected,
            Phase::Closed { .. } => Self::Ended,
            Phase::Failed(_) => Self::Failed,
            _ => Self::Connecting,
        }
    }

    /// What a profile's row says of its sessions: an open one before one on its way,
    /// before one that failed. An ended session says nothing: the row is as if none were
    /// open.
    fn rank(self) -> Option<u8> {
        match self {
            Self::Connected => Some(3),
            Self::Connecting | Self::Reconnecting => Some(2),
            Self::Failed => Some(1),
            Self::Ended => None,
        }
    }
}

/// The state of `tab`, named by its title.
fn tab_status(tab: &Tab) -> SessionStatus {
    let title = tab.display_title().to_owned();
    match SessionState::of(tab) {
        SessionState::Reconnecting => SessionStatus::Reconnecting(title),
        SessionState::Connected => SessionStatus::Connected(title),
        SessionState::Ended => SessionStatus::Disconnected(title),
        SessionState::Failed => SessionStatus::Error(title),
        SessionState::Connecting => SessionStatus::Connecting(title),
    }
}

impl App {
    /// What the row of profile `id` shows of the sessions opened from it: the most alive
    /// of their states, nothing when none is open or all have ended.
    #[must_use]
    pub fn profile_state(&self, id: &ProfileId) -> Option<SessionState> {
        self.tabs
            .iter()
            .filter(|tab| matches!(&tab.reopen, Reopen::Profile(opened) if opened == id))
            .map(SessionState::of)
            .filter_map(|state| state.rank().map(|rank| (rank, state)))
            .max_by_key(|(rank, _)| *rank)
            .map(|(_, state)| state)
    }

    /// The state of the session shown.
    #[must_use]
    pub fn session_status(&self) -> SessionStatus {
        self.active_tab().map_or(SessionStatus::Ready, tab_status)
    }

    /// The notice of what was just done, while the session shown is as it was then.
    #[must_use]
    pub fn notice(&self) -> Option<&Notice> {
        self.notice
            .as_ref()
            .filter(|(_, then)| *then == self.shown())
            .map(|(notice, _)| notice)
    }

    /// Forgets a notice once the session shown or its state changed: it is not said again
    /// on coming back.
    pub(super) fn forget_stale_notice(&mut self) {
        if self.notice.is_some() && self.notice().is_none() {
            self.notice = None;
        }
    }

    /// Says `notice` while the same session is shown in the same state.
    pub(super) fn tell(&mut self, notice: Notice) {
        self.notice = Some((notice, self.shown()));
    }

    /// The tab of the strip shown, and its state: a notice stays while the keyboard moves
    /// between the panes of its split.
    fn shown(&self) -> (Option<TabId>, SessionStatus) {
        let shown = self.shown_tab();
        (
            shown.map(|tab| tab.id),
            shown.map_or(SessionStatus::Ready, tab_status),
        )
    }
}
