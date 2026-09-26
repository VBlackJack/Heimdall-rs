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

//! The application state machine: messages in, state changes and effects out.
//!
//! `update` never touches the network, the clipboard or a timer itself: it returns
//! [`Effect`]s for the UI layer to carry out. It writes to a session's input directly,
//! because that only queues and keeps keystrokes in order.

use std::collections::VecDeque;
use std::fmt;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

use heimdall_core::import::csharp::{self, SkipReason};
use heimdall_core::paths::{LEGACY_SERVERS_FILE_NAME, LEGACY_SETTINGS_FILE_NAME};
use heimdall_core::profile::{ProfileId, RdpProfile, SshProfile, TelnetProfile, VncProfile};
use heimdall_core::store::{MergeReport, ProfileStore};
use heimdall_ssh::{
    AgentSource, ConnectOptions, KeyboardInteractivePrompt, KnownHosts, PublicKey, TerminalSize,
    Verdict, fingerprint, verdict,
};
use heimdall_term::{
    CellPixels, CellPoint, FeedOutput, GridSize, Key, KeyLocation, KeyPress, Modifiers,
    MotionFilter, MouseAction, MouseButton, MouseEvent, SelectionKind, Terminal, TerminalConfig,
    TitleChange, encode_focus, encode_key, encode_mouse, encode_paste, is_reported,
    wheel_as_arrows,
};
use tokio_util::sync::CancellationToken;

use crate::desktop::{DesktopInput, DesktopPane};
use crate::driver::{ConnectRequest, Purpose};
use crate::error::UiError;
use crate::event::{Answer, ConnectionEvent, QuestionKind};
use crate::files::{Direction, FileOperation, FilesPane, Side, TransferId, TransferRequest};
use crate::ids::{AttemptId, QuestionId, TabId};
use crate::profile_draft::{DraftError, ProfileDraft, ProfileField};
use crate::rdp_driver::RdpRequest;
use crate::sink::InputSink;
use crate::telnet_driver::TelnetRequest;
use crate::text::server_text;
use crate::vnc_driver::VncRequest;

mod files_tab;
mod profiles;
mod rdp_tab;
mod telnet_tab;
mod vnc_tab;

pub use files_tab::FilesMessage;
use files_tab::{PendingOperation, PendingTransfer};

/// History lines scrolled per wheel notch when the wheel scrolls locally.
pub const WHEEL_LINES: i32 = 3;

/// Extension of the file profiles are saved to when the profile file is unreadable.
const RECOVERY_EXTENSION: &str = "recovered.toml";

/// Where the application keeps its files, and how it connects.
#[derive(Debug, Clone)]
pub struct AppConfig {
    /// The profile file.
    pub profiles_file: PathBuf,
    /// The `known_hosts` file.
    pub known_hosts: PathBuf,
    /// Data directory of the C# Heimdall, when there is one.
    pub legacy_dir: Option<PathBuf>,
    /// Where to look for an SSH agent.
    pub agent: AgentSource,
    /// Grid size before the first layout is known.
    pub initial_grid: GridSize,
    /// Local folder a Files tab starts in.
    pub files_start: PathBuf,
}

/// A key press, owned, as the UI toolkit reported it.
#[derive(Clone, PartialEq, Eq)]
pub struct KeyInput {
    /// The key.
    pub key: Key,
    /// Text the operating system produced.
    pub text: Option<String>,
    /// Digit printed on the physical key, digit row only.
    pub physical_digit: Option<u8>,
    /// Main block or keypad.
    pub location: KeyLocation,
    /// Modifiers held.
    pub modifiers: Modifiers,
}

impl fmt::Debug for KeyInput {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Typed text can be a password (sudo): never shown.
        f.write_str("KeyInput(..)")
    }
}

/// A pointer event over a terminal.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PointerInput {
    /// The cell under the pointer.
    pub at: CellPoint,
    /// What happened.
    pub action: MouseAction,
    /// Modifiers held.
    pub modifiers: Modifiers,
    /// 1, 2 or 3 for single, double and triple presses.
    pub clicks: u8,
}

/// Every event the application reacts to.
#[derive(Clone)]
pub enum Message {
    /// Open a terminal tab for a saved profile.
    OpenProfile(ProfileId),
    /// Open a Files tab for a saved profile.
    OpenFiles(ProfileId),
    /// Open an RDP tab for a saved RDP profile.
    OpenRdp(ProfileId),
    /// Open a Telnet tab for a saved Telnet profile.
    OpenTelnet(ProfileId),
    /// Open a VNC tab for a saved VNC profile.
    OpenVnc(ProfileId),
    /// Keyboard or mouse input for the remote desktop of a tab.
    DesktopInput {
        /// Tab.
        tab: TabId,
        /// What happened, in order.
        inputs: Vec<DesktopInput>,
    },
    /// Forget the recorded key of the server of a tab whose key changed, and connect again.
    ForgetServer(TabId),
    /// Something in a Files tab.
    Files(FilesMessage),
    /// Show a tab.
    SelectTab(TabId),
    /// Close a tab, asking first when its session is live.
    RequestCloseTab(TabId),
    /// Something happened in a tab's connection attempt.
    Connection {
        /// Tab.
        tab: TabId,
        /// Attempt the event belongs to.
        attempt: AttemptId,
        /// The event.
        event: ConnectionEvent,
    },
    /// The user answered a question; `None` cancels.
    Answer {
        /// Tab.
        tab: TabId,
        /// Question.
        question: QuestionId,
        /// Answer.
        answer: Option<Answer>,
    },
    /// The user decided about an unknown host key.
    HostKeyDecision {
        /// Tab.
        tab: TabId,
        /// Record the key and connect.
        accept: bool,
    },
    /// A key press in a terminal.
    Key {
        /// Tab.
        tab: TabId,
        /// The key.
        input: KeyInput,
    },
    /// A pointer event in a terminal.
    Pointer {
        /// Tab.
        tab: TabId,
        /// The event.
        input: PointerInput,
    },
    /// The terminal area changed size.
    Resize {
        /// Tab shown.
        tab: TabId,
        /// New grid.
        grid: GridSize,
        /// Size of one cell in pixels.
        cell: CellPixels,
    },
    /// Scroll a tab's history; positive goes up.
    ScrollHistory {
        /// Tab.
        tab: TabId,
        /// Lines.
        lines: i32,
    },
    /// Copy the selection.
    Copy(TabId),
    /// Paste the clipboard.
    PasteRequest(TabId),
    /// The clipboard's text arrived for a paste into `tab`.
    ClipboardText {
        /// Tab the paste was asked for.
        tab: TabId,
        /// Text, if the clipboard held any.
        text: Option<String>,
    },
    /// A synchronized update may have reached its deadline.
    SyncDeadline {
        /// Tab.
        tab: TabId,
        /// Deadline generation; stale ones are ignored.
        generation: u64,
    },
    /// The window gained or lost focus.
    WindowFocus(bool),
    /// The window's close button.
    WindowCloseRequested,
    /// Import the profiles of the C# Heimdall.
    ImportLegacy,
    /// Open an empty profile form.
    NewProfile,
    /// Open the form of a saved profile.
    EditProfile(ProfileId),
    /// A field of the profile form changed.
    ProfileField {
        /// Field.
        field: ProfileField,
        /// New text.
        value: String,
    },
    /// From the form of a saved profile, ask to delete it.
    DeleteProfile,
    /// Confirm the open dialog.
    ConfirmDialog,
    /// Dismiss the open dialog.
    DismissDialog,
}

impl fmt::Debug for Message {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Input, output, answers and clipboard text are never shown: they carry what the
        // user typed or read, passwords included.
        match self {
            Self::OpenProfile(id) => write!(f, "OpenProfile({id})"),
            Self::OpenFiles(id) => write!(f, "OpenFiles({id})"),
            Self::OpenRdp(id) => write!(f, "OpenRdp({id})"),
            Self::OpenTelnet(id) => write!(f, "OpenTelnet({id})"),
            Self::OpenVnc(id) => write!(f, "OpenVnc({id})"),
            // What was typed is never shown, as for a terminal.
            Self::DesktopInput { tab, inputs } => {
                write!(f, "DesktopInput({}, {} inputs)", tab.value(), inputs.len())
            }
            Self::ForgetServer(tab) => write!(f, "ForgetServer({})", tab.value()),
            Self::Files(message) => write!(f, "Files({message:?})"),
            Self::SelectTab(tab) => write!(f, "SelectTab({})", tab.value()),
            Self::RequestCloseTab(tab) => write!(f, "RequestCloseTab({})", tab.value()),
            Self::Connection {
                tab,
                attempt,
                event,
            } => write!(
                f,
                "Connection({}, {}, {event:?})",
                tab.value(),
                attempt.value()
            ),
            Self::Answer { tab, question, .. } => {
                write!(f, "Answer({}, {}, ..)", tab.value(), question.value())
            }
            Self::HostKeyDecision { tab, accept } => {
                write!(f, "HostKeyDecision({}, {accept})", tab.value())
            }
            Self::Key { tab, .. } => write!(f, "Key({}, ..)", tab.value()),
            Self::Pointer { tab, input } => write!(f, "Pointer({}, {input:?})", tab.value()),
            Self::Resize { tab, grid, .. } => {
                write!(f, "Resize({}, {}x{})", tab.value(), grid.cols, grid.rows)
            }
            Self::ScrollHistory { tab, lines } => {
                write!(f, "ScrollHistory({}, {lines})", tab.value())
            }
            Self::Copy(tab) => write!(f, "Copy({})", tab.value()),
            Self::PasteRequest(tab) => write!(f, "PasteRequest({})", tab.value()),
            Self::ClipboardText { tab, .. } => write!(f, "ClipboardText({}, ..)", tab.value()),
            Self::SyncDeadline { tab, generation } => {
                write!(f, "SyncDeadline({}, {generation})", tab.value())
            }
            Self::WindowFocus(focused) => write!(f, "WindowFocus({focused})"),
            Self::WindowCloseRequested => f.write_str("WindowCloseRequested"),
            Self::ImportLegacy => f.write_str("ImportLegacy"),
            Self::NewProfile => f.write_str("NewProfile"),
            Self::EditProfile(id) => write!(f, "EditProfile({id})"),
            Self::ProfileField { field, .. } => write!(f, "ProfileField({field:?}, ..)"),
            Self::DeleteProfile => f.write_str("DeleteProfile"),
            Self::ConfirmDialog => f.write_str("ConfirmDialog"),
            Self::DismissDialog => f.write_str("DismissDialog"),
        }
    }
}

/// Work for the UI layer.
pub enum Effect {
    /// Start a connection attempt and feed its events back as [`Message::Connection`].
    Connect {
        /// Tab.
        tab: TabId,
        /// Attempt.
        attempt: AttemptId,
        /// What to connect to.
        request: Box<ConnectRequest>,
    },
    /// Start an RDP attempt and feed its events back as [`Message::Connection`].
    ConnectRdp {
        /// Tab.
        tab: TabId,
        /// Attempt.
        attempt: AttemptId,
        /// What to connect to.
        request: Box<RdpRequest>,
    },
    /// Start a Telnet attempt and feed its events back as [`Message::Connection`].
    ConnectTelnet {
        /// Tab.
        tab: TabId,
        /// Attempt.
        attempt: AttemptId,
        /// What to connect to.
        request: Box<TelnetRequest>,
    },
    /// Start a VNC attempt and feed its events back as [`Message::Connection`].
    ConnectVnc {
        /// Tab.
        tab: TabId,
        /// Attempt.
        attempt: AttemptId,
        /// What to connect to.
        request: Box<VncRequest>,
    },
    /// Deliver an answer through the registry.
    Answer {
        /// Question.
        question: QuestionId,
        /// Answer; `None` cancels.
        answer: Option<Answer>,
    },
    /// Put text on the clipboard.
    WriteClipboard(String),
    /// Read the clipboard, then send [`Message::ClipboardText`] for `tab`.
    ReadClipboard {
        /// Tab the paste is for.
        tab: TabId,
    },
    /// At `deadline`, send [`Message::SyncDeadline`].
    WakeAt {
        /// Tab.
        tab: TabId,
        /// Generation to send back.
        generation: u64,
        /// When.
        deadline: Instant,
    },
    /// List a remote folder, then send [`FilesMessage::RemoteListed`].
    ListRemote {
        /// Tab.
        tab: TabId,
        /// Session.
        client: heimdall_sftp::SftpClient,
        /// Folder.
        path: heimdall_sftp::RemotePath,
    },
    /// List a local folder, then send [`FilesMessage::LocalListed`].
    ListLocal {
        /// Tab.
        tab: TabId,
        /// Folder.
        path: PathBuf,
    },
    /// Carry out a file operation, then send [`FilesMessage::OperationDone`].
    FileOperation {
        /// Tab.
        tab: TabId,
        /// Pane.
        side: Side,
        /// What to do.
        operation: Box<FileOperation>,
    },
    /// Run a transfer and send its events as [`FilesMessage::TransferEvent`].
    Transfer {
        /// Tab.
        tab: TabId,
        /// Transfer.
        id: TransferId,
        /// What to transfer.
        request: Box<TransferRequest>,
    },
    /// Quit the application.
    Exit,
}

impl fmt::Debug for Effect {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Clipboard text is what the user selected: never shown.
        match self {
            Self::Connect { tab, attempt, .. } => {
                write!(f, "Connect({}, {})", tab.value(), attempt.value())
            }
            Self::ConnectRdp { tab, attempt, .. } => {
                write!(f, "ConnectRdp({}, {})", tab.value(), attempt.value())
            }
            Self::ConnectTelnet { tab, attempt, .. } => {
                write!(f, "ConnectTelnet({}, {})", tab.value(), attempt.value())
            }
            Self::ConnectVnc { tab, attempt, .. } => {
                write!(f, "ConnectVnc({}, {})", tab.value(), attempt.value())
            }
            Self::Answer { question, answer } => {
                write!(f, "Answer({}, {answer:?})", question.value())
            }
            Self::WriteClipboard(_) => f.write_str("WriteClipboard(..)"),
            Self::ReadClipboard { tab } => write!(f, "ReadClipboard({})", tab.value()),
            Self::WakeAt {
                tab, generation, ..
            } => write!(f, "WakeAt({}, {generation})", tab.value()),
            Self::ListRemote { tab, path, .. } => {
                write!(f, "ListRemote({}, {path:?})", tab.value())
            }
            Self::ListLocal { tab, path } => write!(f, "ListLocal({}, {path:?})", tab.value()),
            Self::FileOperation { tab, side, .. } => {
                write!(f, "FileOperation({}, {side:?})", tab.value())
            }
            Self::Transfer { tab, id, request } => write!(
                f,
                "Transfer({}, {}, {:?})",
                tab.value(),
                id.value(),
                request.direction
            ),
            Self::Exit => f.write_str("Exit"),
        }
    }
}

/// Lines a shell would run from pasted text: CR, LF and CR LF all end a line, and empty
/// lines run nothing.
fn command_lines(text: &str) -> usize {
    text.split("\r\n")
        .flat_map(|part| part.split(['\r', '\n']))
        .filter(|line| !line.is_empty())
        .count()
}

/// Where a tab's connection stands.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Phase {
    /// Connecting or authenticating.
    Connecting,
    /// The server's key is unknown; waiting for the user.
    HostKey {
        /// Host.
        host: String,
        /// Port.
        port: u16,
        /// SHA-256 fingerprint.
        fingerprint: String,
    },
    /// The shell is open.
    Connected,
    /// The shell ended.
    Closed {
        /// Exit status, when reported.
        exit_status: Option<u32>,
    },
    /// The attempt failed.
    Failed(UiError),
}

/// A question shown in its tab, server texts already made safe.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Prompt {
    /// Identifier to answer with.
    pub question: QuestionId,
    /// What is asked.
    pub kind: QuestionKind,
}

/// One tab.
pub struct Tab {
    /// Identifier.
    pub id: TabId,
    /// Profile it connects to.
    pub profile: TabProfile,
    /// Title: the profile name, or the one the server set, made safe.
    pub title: String,
    /// Connection state.
    pub phase: Phase,
    /// The terminal.
    pub terminal: Terminal,
    /// Questions waiting, oldest first; only the first is shown.
    pub prompts: VecDeque<Prompt>,
    /// The bell rang since the tab was last shown.
    pub bell: bool,
    /// Shell or files.
    pub purpose: Purpose,
    /// The Files view, for a Files tab.
    pub files: Option<Box<FilesPane>>,
    /// The desktop, for an RDP tab once connected.
    pub desktop: Option<Box<DesktopPane>>,
    pending_rdp_key: Option<heimdall_rdp::Fingerprint>,
    attempt: AttemptId,
    sink: Option<Arc<dyn InputSink>>,
    cancel: CancellationToken,
    pending_host_key: Option<Arc<PublicKey>>,
    connect_grid: GridSize,
    cell: Option<CellPixels>,
    motion: MotionFilter,
    selecting: bool,
    sync_generation: u64,
}

impl fmt::Debug for Tab {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Tab")
            .field("id", &self.id.value())
            .field("phase", &self.phase)
            .field("prompts", &self.prompts.len())
            .finish_non_exhaustive()
    }
}

impl Tab {
    /// Whether a live session would be lost by closing the tab. An attempt still
    /// connecting has nothing to lose: closing it cancels it without asking.
    #[must_use]
    pub fn is_live(&self) -> bool {
        self.phase == Phase::Connected
    }

    fn write(&self, bytes: Vec<u8>) {
        if let Some(sink) = &self.sink
            && !bytes.is_empty()
        {
            // A closed session reports itself through its event stream.
            let _ = sink.write(bytes);
        }
    }

    fn new(
        id: TabId,
        profile: TabProfile,
        purpose: Purpose,
        grid: GridSize,
        attempt: AttemptId,
        cancel: CancellationToken,
    ) -> Self {
        Self {
            id,
            title: profile.name().to_owned(),
            profile,
            phase: Phase::Connecting,
            terminal: Terminal::new(grid, TerminalConfig::default()),
            prompts: VecDeque::new(),
            bell: false,
            purpose,
            files: None,
            desktop: None,
            pending_rdp_key: None,
            attempt,
            sink: None,
            cancel,
            pending_host_key: None,
            connect_grid: grid,
            cell: None,
            motion: MotionFilter::default(),
            selecting: false,
            sync_generation: 0,
        }
    }

    fn stop(&mut self) {
        self.cancel.cancel();
        self.desktop = None;
        if let Some(files) = self.files.as_mut() {
            files.stop();
        }
        if let Some(sink) = self.sink.take() {
            sink.close();
        }
    }
}

/// The profile a tab connects to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TabProfile {
    /// A shell or Files tab.
    Ssh(SshProfile),
    /// A remote desktop tab.
    Rdp(RdpProfile),
    /// A Telnet terminal tab.
    Telnet(TelnetProfile),
    /// A VNC remote desktop tab.
    Vnc(VncProfile),
}

impl TabProfile {
    /// Name shown to the user.
    #[must_use]
    pub fn name(&self) -> &str {
        match self {
            Self::Ssh(profile) => &profile.name,
            Self::Rdp(profile) => &profile.name,
            Self::Telnet(profile) => &profile.name,
            Self::Vnc(profile) => &profile.name,
        }
    }

    /// Host.
    #[must_use]
    pub fn host(&self) -> &str {
        match self {
            Self::Ssh(profile) => &profile.host,
            Self::Rdp(profile) => &profile.host,
            Self::Telnet(profile) => &profile.host,
            Self::Vnc(profile) => &profile.host,
        }
    }

    /// Port.
    #[must_use]
    pub fn port(&self) -> u16 {
        match self {
            Self::Ssh(profile) => profile.port,
            Self::Rdp(profile) => profile.port,
            Self::Telnet(profile) => profile.port,
            Self::Vnc(profile) => profile.port,
        }
    }

    /// Account, when the profile names one.
    #[must_use]
    pub fn username(&self) -> Option<&str> {
        match self {
            Self::Ssh(profile) => profile.username.as_deref(),
            Self::Rdp(profile) => profile.username.as_deref(),
            // Telnet asks for its account in the session; VNC has none.
            Self::Telnet(_) | Self::Vnc(_) => None,
        }
    }
}

/// Counts of an import, and what was left out.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportSummary {
    /// Merge result.
    pub merged: MergeReport,
    /// Profiles left out: display name and reason.
    pub skipped: Vec<(String, SkipReason)>,
}

/// A modal decision that concerns the whole window.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Dialog {
    /// Close a tab whose session is live.
    ConfirmCloseTab(TabId),
    /// Quit with live sessions.
    ConfirmExit {
        /// Live sessions.
        live: usize,
    },
    /// Paste several lines into a shell that would run them one by one.
    ConfirmPaste {
        /// Tab.
        tab: TabId,
        /// Number of lines.
        lines: usize,
    },
    /// A name for a new folder or a renamed entry.
    AskName {
        /// Tab.
        tab: TabId,
        /// Pane.
        side: Side,
        /// What the name is for.
        action: NameAction,
        /// The name typed so far.
        value: String,
    },
    /// Delete an entry, a folder with everything in it.
    ConfirmDelete {
        /// Tab.
        tab: TabId,
        /// Pane.
        side: Side,
        /// The entry's name, made safe.
        name: String,
        /// A folder.
        folder: bool,
    },
    /// Replace an existing file with a transfer.
    ConfirmOverwrite {
        /// Tab.
        tab: TabId,
        /// Direction.
        direction: Direction,
        /// The file's name, made safe.
        name: String,
    },
    /// A profile form: a new profile, or a saved one being edited.
    EditProfile {
        /// What is typed.
        draft: Box<ProfileDraft>,
        /// Why the last save was refused, until the user changes a field.
        error: Option<DraftError>,
    },
    /// Delete a saved profile.
    ConfirmDeleteProfile {
        /// Profile.
        id: ProfileId,
        /// Its name, made safe.
        name: String,
    },
    /// Result of an import.
    ImportDone(ImportSummary),
    /// An import could not run.
    ImportFailed {
        /// Technical detail.
        detail: String,
    },
    /// The profile file could not be read or written.
    StoreError {
        /// Technical detail.
        detail: String,
    },
}

/// What a typed name is for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NameAction {
    /// A new folder.
    NewFolder,
    /// A new name for the selected entry.
    Rename,
}

/// The application.
pub struct App {
    config: AppConfig,
    store: ProfileStore,
    /// Open tabs, in display order.
    pub tabs: Vec<Tab>,
    /// Tab shown.
    pub active: Option<TabId>,
    /// Window-level dialog, if any.
    pub dialog: Option<Dialog>,
    viewport: GridSize,
    pending_paste: Option<(TabId, String)>,
    pending_transfer: Option<PendingTransfer>,
    pending_operation: Option<PendingOperation>,
}

impl fmt::Debug for App {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("App")
            .field("tabs", &self.tabs)
            .field("active", &self.active.map(TabId::value))
            .field("dialog", &self.dialog)
            .finish_non_exhaustive()
    }
}

impl App {
    /// The application with the profiles of `config.profiles_file`. A store that cannot
    /// be read starts empty and the problem is shown.
    #[must_use]
    pub fn new(config: AppConfig) -> Self {
        let (store, dialog) = match ProfileStore::open(&config.profiles_file) {
            Ok(store) => (store, None),
            // Start empty, saving beside the unreadable file so it is never overwritten.
            Err(error) => (
                ProfileStore::empty(config.profiles_file.with_extension(RECOVERY_EXTENSION)),
                Some(Dialog::StoreError {
                    detail: error.to_string(),
                }),
            ),
        };
        Self {
            viewport: config.initial_grid,
            config,
            store,
            tabs: Vec::new(),
            active: None,
            dialog,
            pending_paste: None,
            pending_transfer: None,
            pending_operation: None,
        }
    }

    /// Saved SSH profiles.
    #[must_use]
    pub fn profiles(&self) -> &[SshProfile] {
        self.store.ssh_profiles()
    }

    /// Saved RDP profiles.
    #[must_use]
    pub fn rdp_profiles(&self) -> &[RdpProfile] {
        self.store.rdp_profiles()
    }

    /// Saved Telnet profiles.
    #[must_use]
    pub fn telnet_profiles(&self) -> &[TelnetProfile] {
        self.store.telnet_profiles()
    }

    /// Saved VNC profiles.
    #[must_use]
    pub fn vnc_profiles(&self) -> &[VncProfile] {
        self.store.vnc_profiles()
    }

    /// Whether the C# Heimdall's data can be imported.
    #[must_use]
    pub fn can_import(&self) -> bool {
        self.config
            .legacy_dir
            .as_ref()
            .is_some_and(|dir| dir.join(LEGACY_SERVERS_FILE_NAME).is_file())
    }

    /// The tab shown.
    #[must_use]
    pub fn active_tab(&self) -> Option<&Tab> {
        self.active.and_then(|id| self.tab(id))
    }

    /// A tab by identifier.
    #[must_use]
    pub fn tab(&self, id: TabId) -> Option<&Tab> {
        self.tabs.iter().find(|tab| tab.id == id)
    }

    fn tab_mut(&mut self, id: TabId) -> Option<&mut Tab> {
        self.tabs.iter_mut().find(|tab| tab.id == id)
    }

    /// Applies a message.
    pub fn update(&mut self, message: Message) -> Vec<Effect> {
        match message {
            Message::OpenProfile(id) => self.open_profile(&id, Purpose::Shell),
            Message::OpenFiles(id) => self.open_profile(&id, Purpose::Files),
            Message::OpenRdp(id) => self.open_rdp(&id),
            Message::OpenTelnet(id) => self.open_telnet(&id),
            Message::OpenVnc(id) => self.open_vnc(&id),
            Message::DesktopInput { tab, inputs } => {
                self.desktop_input(tab, &inputs);
                Vec::new()
            }
            Message::ForgetServer(tab) => self.forget_server(tab),
            Message::Files(message) => self.files(message),
            Message::SelectTab(tab) => {
                if let Some(found) = self.tab_mut(tab) {
                    found.bell = false;
                    self.active = Some(tab);
                }
                Vec::new()
            }
            Message::RequestCloseTab(tab) => self.request_close(tab),
            Message::Connection {
                tab,
                attempt,
                event,
            } => self.connection(tab, attempt, event),
            Message::Answer {
                tab,
                question,
                answer,
            } => {
                if let Some(found) = self.tab_mut(tab) {
                    found.prompts.retain(|prompt| prompt.question != question);
                }
                vec![Effect::Answer { question, answer }]
            }
            Message::HostKeyDecision { tab, accept } => self.host_key_decision(tab, accept),
            Message::Key { tab, input } => self.key(tab, &input),
            Message::Pointer { tab, input } => self.pointer(tab, input),
            Message::Resize { tab, grid, cell } => self.resize(tab, grid, cell),
            Message::ScrollHistory { tab, lines } => {
                if let Some(found) = self.tab_mut(tab) {
                    found.terminal.scroll(lines);
                }
                Vec::new()
            }
            Message::Copy(tab) => self
                .tab(tab)
                .and_then(|found| found.terminal.selected_text())
                .map(Effect::WriteClipboard)
                .into_iter()
                .collect(),
            Message::PasteRequest(tab) => vec![Effect::ReadClipboard { tab }],
            Message::ClipboardText { tab, text } => self.paste(tab, text),
            Message::SyncDeadline { tab, generation } => self.sync_deadline(tab, generation),
            Message::WindowFocus(focused) => {
                if let Some(found) = self.active.and_then(|id| self.tab(id))
                    && let Some(bytes) = encode_focus(focused, &found.terminal.input_mode())
                {
                    found.write(bytes);
                }
                Vec::new()
            }
            Message::WindowCloseRequested => self.close_window(),
            Message::ImportLegacy => {
                self.import_legacy();
                Vec::new()
            }
            Message::NewProfile => {
                self.new_profile();
                Vec::new()
            }
            Message::EditProfile(id) => {
                self.edit_profile(&id);
                Vec::new()
            }
            Message::ProfileField { field, value } => {
                self.profile_field(field, value);
                Vec::new()
            }
            Message::DeleteProfile => {
                self.ask_delete_profile();
                Vec::new()
            }
            Message::ConfirmDialog => self.confirm_dialog(),
            Message::DismissDialog => {
                self.dialog = None;
                self.pending_paste = None;
                self.pending_transfer = None;
                self.pending_operation = None;
                Vec::new()
            }
        }
    }

    fn connect_request(
        &self,
        profile: &SshProfile,
        grid: GridSize,
        cancel: CancellationToken,
        purpose: Purpose,
    ) -> ConnectRequest {
        let mut options = ConnectOptions::new(self.config.known_hosts.clone());
        options.agent = self.config.agent.clone();
        options.initial_size = terminal_size(grid, None);
        ConnectRequest {
            profile: profile.clone(),
            purpose,
            options,
            cancel,
        }
    }

    fn open_profile(&mut self, id: &ProfileId, purpose: Purpose) -> Vec<Effect> {
        let Some(profile) = self.profiles().iter().find(|p| &p.id == id).cloned() else {
            return Vec::new();
        };
        let grid = self.viewport;
        let tab_id = TabId::fresh();
        let attempt = AttemptId::fresh();
        let cancel = CancellationToken::new();
        let request = self.connect_request(&profile, grid, cancel.clone(), purpose);
        let mut tab = Tab::new(
            tab_id,
            TabProfile::Ssh(profile),
            purpose,
            grid,
            attempt,
            cancel,
        );
        tab.files = (purpose == Purpose::Files)
            .then(|| Box::new(FilesPane::new(self.config.files_start.clone())));
        self.tabs.push(tab);
        self.active = Some(tab_id);
        vec![Effect::Connect {
            tab: tab_id,
            attempt,
            request: Box::new(request),
        }]
    }

    fn connection(
        &mut self,
        tab_id: TabId,
        attempt: AttemptId,
        event: ConnectionEvent,
    ) -> Vec<Effect> {
        let current = self.tab(tab_id).is_some_and(|tab| tab.attempt == attempt);
        if !current {
            // A late event from a closed tab or an abandoned attempt: release what it holds.
            return match event {
                ConnectionEvent::Connected { input } => {
                    input.close();
                    Vec::new()
                }
                ConnectionEvent::Question { question, .. } => vec![Effect::Answer {
                    question,
                    answer: None,
                }],
                // An SFTP session dropped here closes with its connection.
                _ => Vec::new(),
            };
        }
        let active = self.active == Some(tab_id);
        let Some(tab) = self.tab_mut(tab_id) else {
            return Vec::new();
        };
        match event {
            ConnectionEvent::Question { question, kind } => {
                tab.prompts.push_back(Prompt {
                    question,
                    kind: safe_question(kind),
                });
                Vec::new()
            }
            ConnectionEvent::UnknownHostKey {
                host,
                port,
                fingerprint,
                key,
            } => {
                tab.prompts.clear();
                tab.pending_host_key = Some(key);
                tab.phase = Phase::HostKey {
                    host,
                    port,
                    fingerprint,
                };
                Vec::new()
            }
            event @ (ConnectionEvent::UnknownRdpCertificate { .. }
            | ConnectionEvent::RdpReady { .. }
            | ConnectionEvent::DesktopFrame) => {
                rdp_tab::apply(tab, event);
                Vec::new()
            }
            event @ ConnectionEvent::VncReady { .. } => {
                vnc_tab::apply(tab, event);
                Vec::new()
            }
            ConnectionEvent::Connected { input } => {
                tab.phase = Phase::Connected;
                let grid = tab.terminal.size();
                if grid != tab.connect_grid {
                    let _ = input.resize(terminal_size(grid, tab.cell));
                }
                tab.sink = Some(input);
                Vec::new()
            }
            ConnectionEvent::FilesReady { client } => {
                tab.phase = Phase::Connected;
                if let Some(files) = tab.files.as_mut() {
                    files.client = Some(client);
                }
                self.files_ready(tab_id)
            }
            ConnectionEvent::Output(bytes) => {
                let output = tab.terminal.feed(&bytes);
                handle_feed(tab, output, active)
            }
            ConnectionEvent::Closed { exit_status } => {
                tab.phase = Phase::Closed { exit_status };
                tab.sink = None;
                tab.desktop = None;
                tab.prompts.clear();
                Vec::new()
            }
            ConnectionEvent::Failed(error) => {
                tab.phase = Phase::Failed(error);
                tab.sink = None;
                tab.desktop = None;
                tab.prompts.clear();
                Vec::new()
            }
        }
    }

    fn host_key_decision(&mut self, tab_id: TabId, accept: bool) -> Vec<Effect> {
        if self
            .tab(tab_id)
            .is_some_and(|tab| tab.purpose == Purpose::Rdp)
        {
            return self.rdp_certificate_decision(tab_id, accept);
        }
        let known_hosts = KnownHosts::new(&self.config.known_hosts);
        let Some(tab) = self.tab_mut(tab_id) else {
            return Vec::new();
        };
        let (Phase::HostKey { host, port, .. }, Some(key)) =
            (tab.phase.clone(), tab.pending_host_key.take())
        else {
            return Vec::new();
        };
        if !accept {
            tab.phase = Phase::Failed(UiError::Cancelled);
            return Vec::new();
        }
        // Another tab may have recorded a key for this host meanwhile: read again.
        let learned = match known_hosts.recorded(&host, port) {
            Ok(recorded) => match verdict(&recorded, &key) {
                Verdict::Trusted => Ok(()),
                Verdict::Unknown => known_hosts
                    .learn(&host, port, &key)
                    .map_err(|error| UiError::from(&error)),
                Verdict::Changed { recorded } => Err(UiError::HostKeyChanged {
                    recorded: fingerprint(&recorded),
                    offered: fingerprint(&key),
                }),
                Verdict::OtherAlgorithm { recorded } => Err(UiError::HostKeyAlgorithmMismatch {
                    recorded: recorded.iter().map(ToString::to_string).collect(),
                }),
            },
            Err(error) => Err(UiError::from(&error)),
        };
        if let Err(error) = learned {
            tab.phase = Phase::Failed(error);
            return Vec::new();
        }
        let TabProfile::Ssh(profile) = tab.profile.clone() else {
            return Vec::new();
        };
        let grid = tab.terminal.size();
        let attempt = AttemptId::fresh();
        let cancel = CancellationToken::new();
        tab.attempt = attempt;
        tab.cancel = cancel.clone();
        tab.phase = Phase::Connecting;
        tab.connect_grid = grid;
        let purpose = tab.purpose;
        let request = self.connect_request(&profile, grid, cancel, purpose);
        vec![Effect::Connect {
            tab: tab_id,
            attempt,
            request: Box::new(request),
        }]
    }

    fn key(&mut self, tab_id: TabId, input: &KeyInput) -> Vec<Effect> {
        // A dialog owns the keyboard: Enter meant for it must not run a command.
        if self.dialog.is_some() {
            return Vec::new();
        }
        let Some(tab) = self.tab_mut(tab_id) else {
            return Vec::new();
        };
        if tab.phase != Phase::Connected || !tab.prompts.is_empty() {
            return Vec::new();
        }
        let press = KeyPress {
            key: input.key,
            text: input.text.as_deref(),
            physical_digit: input.physical_digit,
            location: input.location,
            modifiers: input.modifiers,
        };
        if let Some(bytes) = encode_key(&press, &tab.terminal.input_mode()) {
            tab.terminal.scroll_to_bottom();
            tab.write(bytes);
        }
        Vec::new()
    }

    fn pointer(&mut self, tab_id: TabId, input: PointerInput) -> Vec<Effect> {
        if self.dialog.is_some() {
            return Vec::new();
        }
        let Some(tab) = self.tab_mut(tab_id) else {
            return Vec::new();
        };
        let mode = tab.terminal.input_mode();
        if tab.phase == Phase::Connected && is_reported(&mode, input.modifiers) {
            let event = MouseEvent {
                action: input.action,
                col: input.at.col,
                row: input.at.row,
                modifiers: input.modifiers,
            };
            if let Some(bytes) = encode_mouse(&event, &mode, &mut tab.motion) {
                tab.write(bytes);
            }
            return Vec::new();
        }
        match input.action {
            MouseAction::Press(MouseButton::Left) => {
                let kind = match input.clicks {
                    2 => SelectionKind::Word,
                    3.. => SelectionKind::Line,
                    _ => SelectionKind::Simple,
                };
                tab.terminal.begin_selection(input.at, kind);
                tab.selecting = true;
            }
            MouseAction::Motion {
                held: Some(MouseButton::Left),
            } if tab.selecting => tab.terminal.extend_selection(input.at),
            MouseAction::Release(MouseButton::Left) => tab.selecting = false,
            MouseAction::WheelUp | MouseAction::WheelDown => {
                let up = input.action == MouseAction::WheelUp;
                if tab.phase == Phase::Connected
                    && let Some(bytes) = wheel_as_arrows(up, 1, &mode)
                {
                    tab.write(bytes);
                } else {
                    tab.terminal
                        .scroll(if up { WHEEL_LINES } else { -WHEEL_LINES });
                }
            }
            MouseAction::Press(_) | MouseAction::Release(_) | MouseAction::Motion { .. } => {}
        }
        Vec::new()
    }

    fn resize(&mut self, tab_id: TabId, grid: GridSize, cell: CellPixels) -> Vec<Effect> {
        self.viewport = grid.clamped();
        let Some(tab) = self.tab_mut(tab_id) else {
            return Vec::new();
        };
        tab.terminal.set_cell_pixels(cell);
        tab.cell = Some(cell);
        if tab.terminal.size() == grid.clamped() {
            return Vec::new();
        }
        // The grid follows the drawing at once; the server is told after, coalesced.
        let grid = tab.terminal.resize(grid);
        if let Some(sink) = &tab.sink {
            let _ = sink.resize(terminal_size(grid, Some(cell)));
        }
        Vec::new()
    }

    fn paste(&mut self, tab_id: TabId, text: Option<String>) -> Vec<Effect> {
        let Some(text) = text.filter(|text| !text.is_empty()) else {
            return Vec::new();
        };
        let Some(tab) = self.tab(tab_id) else {
            return Vec::new();
        };
        if tab.phase != Phase::Connected {
            return Vec::new();
        }
        let mode = tab.terminal.input_mode();
        let lines = command_lines(&text);
        let runs_lines = !mode.bracketed_paste && text.trim_end().contains(['\n', '\r']);
        if runs_lines {
            self.dialog = Some(Dialog::ConfirmPaste { tab: tab_id, lines });
            self.pending_paste = Some((tab_id, text));
            return Vec::new();
        }
        tab.write(encode_paste(&text, &mode));
        Vec::new()
    }

    fn sync_deadline(&mut self, tab_id: TabId, generation: u64) -> Vec<Effect> {
        let active = self.active == Some(tab_id);
        let Some(tab) = self.tab_mut(tab_id) else {
            return Vec::new();
        };
        if tab.sync_generation != generation {
            return Vec::new();
        }
        match tab.terminal.sync_deadline() {
            Some(deadline) if deadline > Instant::now() => {
                tab.sync_generation += 1;
                vec![Effect::WakeAt {
                    tab: tab_id,
                    generation: tab.sync_generation,
                    deadline,
                }]
            }
            Some(_) => {
                let output = tab.terminal.flush_sync();
                handle_feed(tab, output, active)
            }
            None => Vec::new(),
        }
    }

    fn live_tabs(&self) -> usize {
        self.tabs.iter().filter(|tab| tab.is_live()).count()
    }

    fn request_close(&mut self, tab_id: TabId) -> Vec<Effect> {
        match self.tab(tab_id) {
            Some(tab) if tab.is_live() => {
                self.dialog = Some(Dialog::ConfirmCloseTab(tab_id));
            }
            Some(_) => self.close_tab(tab_id),
            None => {}
        }
        Vec::new()
    }

    fn close_tab(&mut self, tab_id: TabId) {
        let Some(index) = self.tabs.iter().position(|tab| tab.id == tab_id) else {
            return;
        };
        // Cancelling the attempt drops its pending questions from the registry.
        let mut tab = self.tabs.remove(index);
        tab.stop();
        if self.active == Some(tab_id) {
            self.active = self
                .tabs
                .get(index.min(self.tabs.len().saturating_sub(1)))
                .map(|tab| tab.id);
        }
    }

    fn close_window(&mut self) -> Vec<Effect> {
        let live = self.live_tabs();
        if live > 0 {
            self.dialog = Some(Dialog::ConfirmExit { live });
            return Vec::new();
        }
        vec![Effect::Exit]
    }

    fn confirm_dialog(&mut self) -> Vec<Effect> {
        match self.dialog.take() {
            Some(Dialog::ConfirmCloseTab(tab)) => {
                self.close_tab(tab);
                Vec::new()
            }
            Some(Dialog::ConfirmExit { .. }) => {
                for tab in &mut self.tabs {
                    tab.stop();
                }
                vec![Effect::Exit]
            }
            Some(Dialog::ConfirmPaste { .. }) => {
                if let Some((tab_id, text)) = self.pending_paste.take()
                    && let Some(tab) = self.tab(tab_id)
                {
                    tab.write(encode_paste(&text, &tab.terminal.input_mode()));
                }
                Vec::new()
            }
            Some(Dialog::ConfirmOverwrite { .. }) => self.confirm_overwrite(),
            Some(Dialog::AskName { value, .. }) => self.confirm_operation(Some(&value)),
            Some(Dialog::ConfirmDelete { .. }) => self.confirm_operation(None),
            Some(Dialog::EditProfile { draft, .. }) => {
                self.save_profile(draft);
                Vec::new()
            }
            Some(Dialog::ConfirmDeleteProfile { id, .. }) => {
                self.delete_profile(&id);
                Vec::new()
            }
            Some(
                Dialog::ImportDone(_) | Dialog::ImportFailed { .. } | Dialog::StoreError { .. },
            )
            | None => Vec::new(),
        }
    }

    fn import_legacy(&mut self) {
        let Some(dir) = self.config.legacy_dir.clone() else {
            return;
        };
        let servers = match std::fs::read_to_string(dir.join(LEGACY_SERVERS_FILE_NAME)) {
            Ok(text) => text,
            Err(error) => {
                self.dialog = Some(Dialog::ImportFailed {
                    detail: error.to_string(),
                });
                return;
            }
        };
        let settings = std::fs::read_to_string(dir.join(LEGACY_SETTINGS_FILE_NAME)).ok();
        let report = match csharp::import(&servers, settings.as_deref()) {
            Ok(report) => report,
            Err(error) => {
                self.dialog = Some(Dialog::ImportFailed {
                    detail: error.to_string(),
                });
                return;
            }
        };
        // Saved before it is kept: a failed save leaves the list as its file is.
        let merged = match self.store.apply(|store| {
            let ssh = store.merge(report.profiles);
            let rdp = store.merge_rdp(report.rdp);
            let telnet = store.merge_telnet(report.telnet);
            let vnc = store.merge_vnc(report.vnc);
            MergeReport {
                added: ssh.added + rdp.added + telnet.added + vnc.added,
                updated: ssh.updated + rdp.updated + telnet.updated + vnc.updated,
                unchanged: ssh.unchanged + rdp.unchanged + telnet.unchanged + vnc.unchanged,
            }
        }) {
            Ok(merged) => merged,
            Err(error) => {
                self.dialog = Some(Dialog::StoreError {
                    detail: error.to_string(),
                });
                return;
            }
        };
        self.dialog = Some(Dialog::ImportDone(ImportSummary {
            merged,
            skipped: report
                .skipped
                .into_iter()
                .map(|skipped| (server_text(&skipped.name), skipped.reason))
                .collect(),
        }));
    }
}

fn terminal_size(grid: GridSize, cell: Option<CellPixels>) -> TerminalSize {
    let pixels = |count: usize, size: Option<u16>| {
        size.and_then(|size| {
            u32::try_from(count)
                .ok()
                .map(|count| count * u32::from(size))
        })
        .unwrap_or(0)
    };
    TerminalSize {
        cols: u32::try_from(grid.cols).unwrap_or(u32::MAX),
        rows: u32::try_from(grid.rows).unwrap_or(u32::MAX),
        pixel_width: pixels(grid.cols, cell.map(|c| c.width)),
        pixel_height: pixels(grid.rows, cell.map(|c| c.height)),
    }
}

fn handle_feed(tab: &mut Tab, output: FeedOutput, active: bool) -> Vec<Effect> {
    let mut effects = Vec::new();
    tab.write(output.replies);
    match output.title {
        Some(TitleChange::Set(title)) => {
            let title = server_text(&title);
            tab.title = if title.is_empty() {
                tab.profile.name().to_owned()
            } else {
                title
            };
        }
        Some(TitleChange::Reset) => tab.title = tab.profile.name().to_owned(),
        None => {}
    }
    if output.bell && !active {
        tab.bell = true;
    }
    if let Some(text) = output.clipboard {
        effects.push(Effect::WriteClipboard(text));
    }
    if let Some(deadline) = tab.terminal.sync_deadline() {
        tab.sync_generation += 1;
        effects.push(Effect::WakeAt {
            tab: tab.id,
            generation: tab.sync_generation,
            deadline,
        });
    }
    effects
}

fn safe_question(kind: QuestionKind) -> QuestionKind {
    match kind {
        QuestionKind::KeyboardInteractive(mut question) => {
            question.name = server_text(&question.name);
            question.instructions = server_text(&question.instructions);
            question.prompts = question
                .prompts
                .into_iter()
                .map(|prompt| KeyboardInteractivePrompt {
                    text: server_text(&prompt.text),
                    echo: prompt.echo,
                })
                .collect();
            QuestionKind::KeyboardInteractive(question)
        }
        other => other,
    }
}
