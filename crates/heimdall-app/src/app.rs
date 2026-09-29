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
use heimdall_core::profile::{
    LocalProfile, ProfileId, RdpProfile, SshGateway, SshProfile, TelnetProfile, VncProfile,
    WinRmProfile,
};
use heimdall_core::settings::Settings;
use heimdall_core::store::{MergeReport, ProfileStore};
use heimdall_ssh::{
    AgentSource, ConnectOptions, KeyboardInteractivePrompt, KnownHosts, PublicKey, RunTrust,
    Secret, TerminalSize, Verdict, fingerprint, verdict,
};
use heimdall_term::{
    CellPixels, CellPoint, FeedOutput, FindDirection, GridSize, Key, KeyLocation, KeyPress,
    Modifiers, MotionFilter, MouseAction, MouseButton, MouseEvent, Palette, SelectionKind,
    Terminal, TerminalConfig, TitleChange, encode_focus, encode_key, encode_mouse, encode_paste,
    is_reported, wheel_as_arrows,
};
use tokio_util::sync::CancellationToken;

use crate::desktop::{DesktopInput, DesktopPane, SpecialKeys};
use crate::driver::{ConnectRequest, Purpose};
use crate::error::{ServerAddress, UiError};
use crate::event::{Answer, ConnectionEvent, PostConnectProgress, QuestionKind};
use crate::files::{Direction, FileOperation, FilesPane, Side, TransferId, TransferRequest};
use crate::gateway_draft::GatewayDraft;
use crate::ids::{AttemptId, QuestionId, TabId};
use crate::local_driver::{LocalRequest, LocalShell};
use crate::profile_draft::{
    DraftError, DraftProtocol, ProfileChoice, ProfileDraft, ProfileField, ProfileToggle,
};
use crate::rdp_driver::RdpRequest;
use crate::sink::InputSink;
use crate::steps_draft::StepEdit;
use crate::telnet_driver::TelnetRequest;
use crate::text::server_text;
use crate::vnc_driver::VncRequest;

mod appearance;
mod auto_reconnect;
mod broadcast;
mod connect_as;
mod files_tab;
mod folder_menu;
mod folders;
mod gateways;
mod local_tab;
mod pin;
mod post_connect;
mod profile_menu;
mod profiles;
mod provider;
mod provider_connect;
mod quick_connect;
mod rdp_tab;
mod reconnect;
mod selection;
mod status;
mod tab_menu;
mod telnet_tab;
mod transcripts;
mod tree;
mod trusted_keys;
mod vault;
mod vnc_tab;
mod winrm_tab;

use crate::transcript::{Transcript, TranscriptLines};
pub use appearance::SettingsMessage;
pub use auto_reconnect::{RDP_MAX_ATTEMPTS, Retry};
pub use broadcast::BroadcastMessage;
pub use connect_as::ConnectAs;
pub use files_tab::FilesMessage;
use files_tab::{PendingOperation, PendingTransfer};
pub use folder_menu::{FolderMessage, FolderNaming};
pub use folders::{NO_FOLDER, TreeRow};
pub use local_tab::LocalConfirmation;
pub use pin::{PinDialog, PinFailure, PinMessage, PinMode};
pub use post_connect::PostConnectConfirmation;
pub use profile_menu::ProfileMenuMessage;
pub use provider::{ProviderMessage, UNLOCK_SECRET_ENTRY};
pub use provider_connect::{ProviderAnswer, ProviderRequest};
pub use quick_connect::QuickResult;
pub use selection::SelectionMessage;
pub use status::{Notice, SessionStatus};
pub use tab_menu::{TabGroup, TabMenuMessage};
pub use tree::{GatewayBadge, ProfileCopy, ProfileKind, ProfileSummary};
pub use trusted_keys::{TrustedKey, TrustedKeys, TrustedKeysMessage};
use vault::VaultState;
pub use vault::{
    LONG_MASTER_PASSWORD_CHARS, MIN_MASTER_PASSWORD_CHARS, MIN_MASTER_PASSWORD_CLASSES,
    OpenedVault, SystemCredentials, VAULT_FILE_NAME, VaultDialog, VaultJob, VaultMode,
    VaultProblem, VaultStatus, master_password_problem, open_vault,
};

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
    /// Local folder a Files tab and a local shell start in.
    pub files_start: PathBuf,
    /// Where saved passwords go while no master password is set.
    pub system_credentials: SystemCredentials,
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
    /// Open a local shell tab.
    OpenLocal(LocalShell),
    /// Open a saved local profile, asking first unless what it runs is approved.
    OpenLocalProfile(ProfileId),
    /// Open a `WinRM` tab for a saved `WinRM` profile.
    OpenWinRm(ProfileId),
    /// The size a tab shows its remote desktop at, in pixels.
    DesktopResize {
        /// Tab.
        tab: TabId,
        /// Width.
        width: u16,
        /// Height.
        height: u16,
    },
    /// Keyboard or mouse input for the remote desktop of a tab.
    DesktopInput {
        /// Tab.
        tab: TabId,
        /// What happened, in order.
        inputs: Vec<DesktopInput>,
    },
    /// A key combination from the session's menu, for the remote desktop of a tab.
    SendKeys {
        /// Tab.
        tab: TabId,
        /// Which.
        keys: SpecialKeys,
    },
    /// Send this side's clipboard to the remote desktop of a tab, as the C# Heimdall's
    /// noVNC "sync" does: on a click, never by itself over a clear VNC connection.
    SendClipboard(TabId),
    /// Forget the recorded key of the server of a tab whose key changed, and connect again.
    ForgetServer(TabId),
    /// Open the session of a tab again, in its place: a failed or ended one, or, from the
    /// tab's menu, a live one.
    ReconnectTab(TabId),
    /// Something in a Files tab.
    Files(FilesMessage),
    /// Show a tab.
    SelectTab(TabId),
    /// Close a tab, asking first when its session is live.
    RequestCloseTab(TabId),
    /// Something from a tab's menu.
    TabMenu(TabMenuMessage),
    /// The wait before a tab's session opens again by itself is over.
    AutoReconnect {
        /// Tab.
        tab: TabId,
        /// The attempt that failed.
        attempt: AttemptId,
    },
    /// Stop a tab's session from opening again by itself.
    CancelAutoReconnect(TabId),
    /// Open what Quick Connect offered.
    QuickConnect(QuickResult),
    /// Open a profile's host with another protocol, as a session never saved.
    ConnectAs {
        /// Profile.
        id: ProfileId,
        /// Protocol.
        protocol: ConnectAs,
    },
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
    /// Trust an unknown host key or certificate for this run only, as the C# Heimdall's
    /// "Trust this session" and "Just this once", and connect.
    HostKeyTrustOnce(TabId),
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
    /// Look for text in a tab's history, as the C# terminal's search bar.
    FindInTerminal {
        /// Tab.
        tab: TabId,
        /// What to look for, whatever its case.
        query: String,
        /// Which way.
        direction: FindDirection,
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
    /// Open the shell asked about without typing its steps.
    SkipPostConnect,
    /// Stop the post-connect steps of a tab.
    StopPostConnect(TabId),
    /// Dismiss the open dialog.
    DismissDialog,
    /// Save the profile form, with the password typed into it, if any.
    SaveProfile {
        /// The password typed; `None` or empty leaves the saved one as it is.
        password: Option<Secret>,
    },
    /// In the profile form, clear the saved password (done when the form is saved).
    ClearPassword,
    /// In a new profile's form, choose its protocol.
    ChooseProtocol(DraftProtocol),
    /// In the profile form, tick or clear an option.
    ProfileToggle {
        /// Option.
        toggle: ProfileToggle,
        /// Ticked.
        on: bool,
    },
    /// In an RDP profile's form, choose from one of its lists.
    ProfileChoice(ProfileChoice),
    /// Change the post-connect steps of the profile form.
    PostConnectEdit(StepEdit),
    /// Open the gateway dialog for a new gateway.
    NewGateway,
    /// Open the gateway dialog for a saved gateway.
    EditGateway(ProfileId),
    /// A field of the gateway dialog changed.
    GatewayField {
        /// Field.
        field: ProfileField,
        /// New text.
        value: String,
    },
    /// In the gateway dialog, choose the gateway it is reached through.
    ChooseParentGateway(Option<ProfileId>),
    /// In the gateway dialog, clear the saved password (done when it is saved).
    ClearGatewayPassword,
    /// Save the gateway dialog, with the password typed into it, if any.
    SaveGateway {
        /// The password typed; `None` or empty leaves the saved one as it is.
        password: Option<Secret>,
    },
    /// In a session's form, route it through this gateway.
    ChooseGateway(ProfileId),
    /// Open a folder of the tree, or close it.
    ToggleFolder(String),
    /// Something from a folder's menu.
    Folder(FolderMessage),
    /// A profile's Rename or "Move to folder".
    ProfileMenu(ProfileMenuMessage),
    /// Something about the profiles selected together.
    Selection(SelectionMessage),
    /// Select a profile in the tree.
    SelectProfile(ProfileId),
    /// Connect to a profile with its own protocol.
    ConnectProfile(ProfileId),
    /// Save a copy of a profile.
    DuplicateProfile {
        /// Profile.
        id: ProfileId,
        /// Added to the copy's name, in the user's language: " (copy)".
        suffix: String,
    },
    /// Ask to delete a profile, whatever its protocol.
    RequestDeleteProfile(ProfileId),
    /// Put part of a profile on the clipboard.
    CopyProfile {
        /// Profile.
        id: ProfileId,
        /// What.
        what: ProfileCopy,
    },
    /// Open the vault dialog: unlock the vault, or create it.
    ShowVault,
    /// Open the dialog changing the master password.
    ChangeMasterPassword,
    /// Open the dialog removing the master password.
    DisableMasterPassword,
    /// What was typed into the vault dialog.
    SubmitVault {
        /// The first field: the master password, or the new one when creating.
        password: Secret,
        /// The new master password, when changing it.
        new: Option<Secret>,
        /// The new master password typed again, when creating or changing it.
        confirm: Option<Secret>,
    },
    /// The vault was opened, or could not be.
    VaultOpened(Result<OpenedVault, VaultProblem>),
    /// Close the vault.
    LockVault,
    /// The application PIN.
    Pin(PinMessage),
    /// The external credential provider's settings.
    CredentialProvider(ProviderMessage),
    /// The external credential provider answered a tab's question, or could not.
    CredentialProvided(Box<ProviderAnswer>),
    /// A change from the Settings page.
    Settings(SettingsMessage),
    /// A change of broadcast input.
    Broadcast(BroadcastMessage),
}

/// What a step edit does, without the text it carries.
fn step_edit_name(edit: &StepEdit) -> &'static str {
    match edit {
        StepEdit::Add => "Add",
        StepEdit::Remove => "Remove",
        StepEdit::MoveUp => "MoveUp",
        StepEdit::MoveDown => "MoveDown",
        StepEdit::Select(_) => "Select",
        StepEdit::Enabled(..) => "Enabled",
        StepEdit::Input(..) => "Input",
        StepEdit::Delay(..) => "Delay",
        StepEdit::OnFailure(..) => "OnFailure",
    }
}

impl fmt::Debug for Message {
    #[allow(clippy::too_many_lines, reason = "one arm per message")]
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Input, output, answers and clipboard text are never shown: they carry what the
        // user typed or read, passwords included.
        match self {
            Self::OpenProfile(id) => write!(f, "OpenProfile({id})"),
            Self::OpenFiles(id) => write!(f, "OpenFiles({id})"),
            Self::OpenRdp(id) => write!(f, "OpenRdp({id})"),
            Self::OpenTelnet(id) => write!(f, "OpenTelnet({id})"),
            // The arguments may carry anything: only the program is shown.
            Self::OpenLocal(shell) => write!(f, "OpenLocal({:?})", shell.program),
            Self::OpenLocalProfile(id) => write!(f, "OpenLocalProfile({id})"),
            Self::OpenWinRm(id) => write!(f, "OpenWinRm({id})"),
            Self::OpenVnc(id) => write!(f, "OpenVnc({id})"),
            // What was typed is never shown, as for a terminal.
            Self::DesktopResize { tab, width, height } => {
                write!(f, "DesktopResize({}, {width}x{height})", tab.value())
            }
            Self::DesktopInput { tab, inputs } => {
                write!(f, "DesktopInput({}, {} inputs)", tab.value(), inputs.len())
            }
            Self::SendKeys { tab, keys } => write!(f, "SendKeys({}, {keys:?})", tab.value()),
            Self::SendClipboard(tab) => write!(f, "SendClipboard({})", tab.value()),
            Self::ForgetServer(tab) => write!(f, "ForgetServer({})", tab.value()),
            Self::ReconnectTab(tab) => write!(f, "ReconnectTab({})", tab.value()),
            Self::Files(message) => write!(f, "Files({message:?})"),
            Self::SelectTab(tab) => write!(f, "SelectTab({})", tab.value()),
            Self::RequestCloseTab(tab) => write!(f, "RequestCloseTab({})", tab.value()),
            Self::TabMenu(message) => write!(f, "TabMenu({message:?})"),
            Self::AutoReconnect { tab, attempt } => {
                write!(f, "AutoReconnect({}, {})", tab.value(), attempt.value())
            }
            Self::CancelAutoReconnect(tab) => write!(f, "CancelAutoReconnect({})", tab.value()),
            Self::ConnectAs { id, protocol } => write!(f, "ConnectAs({id}, {protocol:?})"),
            Self::QuickConnect(result) => write!(f, "QuickConnect({result:?})"),
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
            Self::HostKeyTrustOnce(tab) => write!(f, "HostKeyTrustOnce({})", tab.value()),
            Self::Key { tab, .. } => write!(f, "Key({}, ..)", tab.value()),
            Self::Pointer { tab, input } => write!(f, "Pointer({}, {input:?})", tab.value()),
            Self::Resize { tab, grid, .. } => {
                write!(f, "Resize({}, {}x{})", tab.value(), grid.cols, grid.rows)
            }
            Self::FindInTerminal { tab, direction, .. } => {
                write!(f, "FindInTerminal({}, {direction:?})", tab.value())
            }
            Self::ScrollHistory { tab, lines } => write!(f, "Scroll({}, {lines})", tab.value()),
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
            Self::SkipPostConnect => f.write_str("SkipPostConnect"),
            Self::StopPostConnect(tab) => write!(f, "StopPostConnect({})", tab.value()),
            Self::DismissDialog => f.write_str("DismissDialog"),
            Self::SaveProfile { .. } => f.write_str("SaveProfile(..)"),
            Self::ClearPassword => f.write_str("ClearPassword"),
            Self::ChooseProtocol(protocol) => write!(f, "ChooseProtocol({protocol:?})"),
            Self::ProfileToggle { toggle, on } => write!(f, "ProfileToggle({toggle:?}, {on})"),
            Self::ProfileChoice(choice) => write!(f, "ProfileChoice({choice:?})"),
            // A command may carry anything: only which step changed is shown.
            Self::PostConnectEdit(edit) => write!(f, "PostConnectEdit({})", step_edit_name(edit)),
            Self::NewGateway => f.write_str("NewGateway"),
            Self::EditGateway(id) => write!(f, "EditGateway({id})"),
            Self::GatewayField { field, .. } => write!(f, "GatewayField({field:?}, ..)"),
            Self::ChooseParentGateway(id) => write!(f, "ChooseParentGateway({id:?})"),
            Self::ClearGatewayPassword => f.write_str("ClearGatewayPassword"),
            Self::SaveGateway { .. } => f.write_str("SaveGateway(..)"),
            Self::ChooseGateway(id) => write!(f, "ChooseGateway({id})"),
            Self::ToggleFolder(path) => write!(f, "ToggleFolder({path})"),
            Self::Folder(message) => write!(f, "Folder({message:?})"),
            Self::ProfileMenu(message) => write!(f, "ProfileMenu({message:?})"),
            Self::Selection(message) => write!(f, "Selection({message:?})"),
            Self::SelectProfile(id) => write!(f, "SelectProfile({id})"),
            Self::ConnectProfile(id) => write!(f, "ConnectProfile({id})"),
            Self::DuplicateProfile { id, .. } => write!(f, "DuplicateProfile({id})"),
            Self::RequestDeleteProfile(id) => write!(f, "RequestDeleteProfile({id})"),
            Self::CopyProfile { id, what } => write!(f, "CopyProfile({id}, {what:?})"),
            Self::ShowVault => f.write_str("ShowVault"),
            Self::ChangeMasterPassword => f.write_str("ChangeMasterPassword"),
            Self::DisableMasterPassword => f.write_str("DisableMasterPassword"),
            Self::SubmitVault { .. } => f.write_str("SubmitVault(..)"),
            Self::VaultOpened(result) => write!(f, "VaultOpened({:?})", result.as_ref().err()),
            Self::LockVault => f.write_str("LockVault"),
            Self::Pin(message) => write!(f, "Pin({message:?})"),
            Self::CredentialProvider(message) => write!(f, "CredentialProvider({message:?})"),
            Self::CredentialProvided(answer) => {
                write!(
                    f,
                    "CredentialProvided({}, {})",
                    answer.tab.value(),
                    answer.result.is_ok()
                )
            }
            Self::Settings(message) => write!(f, "Settings({message:?})"),
            Self::Broadcast(message) => write!(f, "Broadcast({message:?})"),
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
    /// Start a local shell and feed its events back as [`Message::Connection`].
    ConnectLocal {
        /// Tab.
        tab: TabId,
        /// Attempt.
        attempt: AttemptId,
        /// What to run.
        request: Box<LocalRequest>,
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
        client: heimdall_files::RemoteSession,
        /// Folder.
        path: heimdall_files::RemotePath,
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
    /// Open, create or seal again the vault; then send [`Message::VaultOpened`].
    OpenVault {
        /// Vault file.
        path: PathBuf,
        /// Master password: the one it is opened with, or created with.
        password: Secret,
        /// What to do.
        job: VaultJob,
    },
    /// Run the external credential provider's password command with test values; then send
    /// [`ProviderMessage::Tested`].
    TestCredentialProvider {
        /// The provider's settings as they are.
        settings: heimdall_core::credential_provider::ProviderSettings,
        /// The unlock secret, when one is kept and can be read.
        unlock: Option<Secret>,
    },
    /// Ask the external credential provider for a tab's password; then send
    /// [`Message::CredentialProvided`].
    AskCredentialProvider(Box<ProviderRequest>),
    /// Quit the application.
    /// Wake the core at `deadline` with [`Message::AutoReconnect`].
    RetryAt {
        /// Tab.
        tab: TabId,
        /// The attempt that failed.
        attempt: AttemptId,
        /// When.
        deadline: Instant,
    },
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
            Self::ConnectLocal { tab, attempt, .. } => {
                write!(f, "ConnectLocal({}, {})", tab.value(), attempt.value())
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
            Self::OpenVault { job, .. } => write!(f, "OpenVault({})", job.name()),
            Self::TestCredentialProvider { .. } => f.write_str("TestCredentialProvider"),
            Self::AskCredentialProvider(request) => {
                write!(f, "AskCredentialProvider({})", request.tab.value())
            }
            Self::RetryAt { tab, attempt, .. } => {
                write!(f, "RetryAt({}, {})", tab.value(), attempt.value())
            }
            Self::Exit => f.write_str("Exit"),
        }
    }
}

/// Reads this side's clipboard for `tab` when it is a desktop sharing the clipboard.
fn clipboard_offer(tab: &Tab) -> Vec<Effect> {
    if tab
        .desktop
        .as_deref()
        .is_some_and(DesktopPane::shares_clipboard)
    {
        vec![Effect::ReadClipboard { tab: tab.id }]
    } else {
        Vec::new()
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
    /// The name the user gave the tab, shown instead of its title until reset.
    pub custom_title: Option<String>,
    /// Why the server ended the session, when it said.
    pub end_reason: Option<String>,
    /// The session waiting to open again by itself, after it dropped.
    pub retry: Option<Retry>,
    /// The last search in its history found nothing.
    pub find_missed: bool,
    /// The transcript it keeps, while it keeps one.
    pub transcript: Option<Transcript>,
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
    /// The saved passwords given in an attempt, each once: the server's, and a gateway's on
    /// the way.
    auto_answered: Vec<(AttemptId, ProfileId)>,
    /// How the tab opens again, for Reconnect.
    reopen: reconnect::Reopen,
    /// The post-connect step running, while the sequence runs.
    pub post_connect: Option<PostConnectProgress>,
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
    /// Whether the question the tab asks is about an RDP server's own certificate, not an
    /// SSH key on the way to it.
    #[must_use]
    pub fn asks_about_certificate(&self) -> bool {
        self.purpose == Purpose::Rdp && self.pending_rdp_key.is_some()
    }

    /// What the tab is called: the name the user gave it, else its title.
    #[must_use]
    pub fn display_title(&self) -> &str {
        self.custom_title.as_deref().unwrap_or(&self.title)
    }

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
        palette: Palette,
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
            custom_title: None,
            end_reason: None,
            retry: None,
            find_missed: false,
            transcript: None,
            reopen: reconnect::Reopen::of(&profile),
            post_connect: None,
            profile,
            phase: Phase::Connecting,
            terminal: Terminal::new(
                grid,
                TerminalConfig {
                    palette,
                    ..TerminalConfig::default()
                },
            ),
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
            auto_answered: Vec::new(),
        }
    }

    fn stop(&mut self) {
        self.cancel.cancel();
        self.desktop = None;
        // Its footer is written as it goes.
        self.transcript = None;
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
    /// A local shell tab.
    Local(LocalShell),
}

impl TabProfile {
    /// The protocol it connects with; a local shell's, `WinRM` included, is local.
    #[must_use]
    pub fn kind(&self) -> ProfileKind {
        match self {
            Self::Ssh(_) => ProfileKind::Ssh,
            Self::Rdp(_) => ProfileKind::Rdp,
            Self::Telnet(_) => ProfileKind::Telnet,
            Self::Vnc(_) => ProfileKind::Vnc,
            Self::Local(_) => ProfileKind::Local,
        }
    }

    /// Name shown to the user.
    #[must_use]
    pub fn name(&self) -> &str {
        match self {
            Self::Ssh(profile) => &profile.name,
            Self::Rdp(profile) => &profile.name,
            Self::Telnet(profile) => &profile.name,
            Self::Vnc(profile) => &profile.name,
            Self::Local(shell) => &shell.name,
        }
    }

    /// Host and port; `None` for a local shell, which reaches nothing.
    #[must_use]
    pub fn endpoint(&self) -> Option<(&str, u16)> {
        match self {
            Self::Ssh(profile) => Some((&profile.host, profile.port)),
            Self::Rdp(profile) => Some((&profile.host, profile.port)),
            Self::Telnet(profile) => Some((&profile.host, profile.port)),
            Self::Vnc(profile) => Some((&profile.host, profile.port)),
            Self::Local(_) => None,
        }
    }

    /// Account, when the profile names one.
    #[must_use]
    pub fn username(&self) -> Option<&str> {
        match self {
            Self::Ssh(profile) => profile.username.as_deref(),
            Self::Rdp(profile) => profile.username.as_deref(),
            // Telnet asks for its account in the session; VNC has none; a local shell runs
            // as the user running Heimdall.
            Self::Telnet(_) | Self::Vnc(_) | Self::Local(_) => None,
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
    /// Start broadcast input to every tab.
    ConfirmBroadcast,
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
    /// What an entry of the server is, as the C# Properties dialog shows it.
    FileProperties(Box<crate::files::FileProperties>),
    /// Delete an entry, a folder with everything in it.
    ConfirmDelete {
        /// Tab.
        tab: TabId,
        /// Pane.
        side: Side,
        /// The entry's name, made safe; the first one's when several go.
        name: String,
        /// A folder, or one among them.
        folder: bool,
        /// How many entries go.
        count: usize,
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
    /// Run a local profile's command, shown whole, which the user has not approved yet.
    ConfirmLocalCommand(Box<LocalConfirmation>),
    /// Type post-connect steps, shown whole, which the user has not approved yet.
    ConfirmPostConnect(Box<PostConnectConfirmation>),
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
    /// Forget a key trusted for a server?
    ForgetTrustedKey(TrustedKey),
    /// Add or edit an SSH gateway.
    EditGateway {
        /// What is typed.
        draft: Box<GatewayDraft>,
        /// Why the last save was refused.
        error: Option<DraftError>,
        /// The session's form it was opened from, shown again when it closes.
        back: Option<Box<Dialog>>,
    },
    /// Unlock or create the vault.
    Vault(VaultDialog),
    /// The application PIN: asked at start, or set, changed or removed.
    Pin(PinDialog),
    /// A password could not be saved.
    PasswordSaveFailed {
        /// Technical detail.
        detail: String,
    },
    /// A name for a tab.
    RenameTab {
        /// Tab.
        tab: TabId,
        /// The name typed so far.
        value: String,
    },
    /// A name for a folder: a new one, or one renamed.
    FolderName {
        /// What for.
        naming: FolderNaming,
        /// The name typed so far.
        value: String,
        /// Why the last name was refused, until the user types again.
        error: Option<heimdall_core::folder::FolderError>,
    },
    /// Delete several profiles.
    ConfirmDeleteProfiles {
        /// The profiles.
        ids: Vec<ProfileId>,
        /// Their names, sorted, when they are few enough to list; empty otherwise.
        names: Vec<String>,
    },
    /// A new name for a profile.
    RenameProfile {
        /// The profile.
        id: ProfileId,
        /// The name typed so far.
        value: String,
    },
    /// Delete a folder; its profiles go to no folder.
    ConfirmDeleteFolder {
        /// Its path.
        path: String,
        /// Its name.
        name: String,
        /// How many profiles it holds, its folders' included.
        count: usize,
    },
    /// Connect every session a folder holds.
    ConfirmConnectFolder {
        /// Its path.
        path: String,
        /// How many sessions.
        count: usize,
    },
    /// Close several tabs, some of them live.
    ConfirmCloseTabs {
        /// The tabs.
        tabs: Vec<TabId>,
        /// How many are live.
        live: usize,
    },
}

impl Dialog {
    /// Whether Enter may answer it. Not for running a program: a key pressed as the dialog
    /// appears, meant for whatever had the focus, must not be taken for agreement.
    #[must_use]
    pub fn confirms_on_enter(&self) -> bool {
        // The vault's and the PIN's fields submit themselves.
        !matches!(
            self,
            Self::ConfirmLocalCommand(_)
                | Self::ConfirmPostConnect(_)
                | Self::Vault(_)
                | Self::Pin(_)
                | Self::EditGateway { .. }
        )
    }
}

/// The user's answer about an unknown host key or certificate.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum KeyTrust {
    /// Do not connect.
    Refused,
    /// Trusted for this run only, never recorded.
    Once,
    /// Recorded, then trusted.
    Always,
}

impl From<bool> for KeyTrust {
    /// The answer of the Accept and Reject buttons.
    fn from(accept: bool) -> Self {
        if accept { Self::Always } else { Self::Refused }
    }
}

/// What a typed name is for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NameAction {
    /// A new folder.
    NewFolder,
    /// A new name for the selected entry.
    Rename,
    /// New permission bits for the selected entry, typed in octal.
    Permissions,
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
    /// The profile selected in the tree, the last one clicked: where a Shift+click range
    /// starts.
    pub selected_profile: Option<ProfileId>,
    /// The profiles selected together, when more than one is.
    selection: std::collections::BTreeSet<ProfileId>,
    /// What the Settings page changes, and the file it is saved to.
    settings: Settings,
    settings_file: std::path::PathBuf,
    /// The transcripts' first and last lines, as the window words them.
    transcript_lines: Option<TranscriptLines>,
    /// Broadcast input: on or off, and the tabs marked.
    broadcast: broadcast::Broadcast,
    /// What was just done, and the session shown then with its state.
    notice: Option<(Notice, (Option<TabId>, SessionStatus))>,
    /// The keys trusted for servers, as the Settings page last read them.
    trusted_keys: TrustedKeys,
    /// What the Settings page's provider Test found.
    provider_test: Option<crate::credential_provider::ProviderTest>,
    viewport: GridSize,
    pending_paste: Option<(TabId, String)>,
    /// Transfers waiting, one after the other, for the user to confirm they replace a file.
    pending_transfers: std::collections::VecDeque<PendingTransfer>,
    pending_operation: Option<PendingOperation>,
    vault: VaultState,
    /// SSH keys trusted for this run only, shared with every connection.
    run_trust: RunTrust,
    /// RDP certificates trusted for this run only: server, port, key.
    rdp_run_trust: Vec<(String, u16, heimdall_rdp::Fingerprint)>,
    /// The folders of the tree shown closed, by path, [`NO_FOLDER`] included.
    closed_folders: std::collections::HashSet<String>,
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
        let vault = VaultState::beside(&config.profiles_file, config.system_credentials.clone());
        let (settings, settings_file, dialog) = appearance::load_settings(&config, dialog);
        let mut app = Self {
            settings,
            settings_file,
            transcript_lines: None,
            broadcast: broadcast::Broadcast::default(),
            viewport: config.initial_grid,
            config,
            store,
            tabs: Vec::new(),
            active: None,
            dialog,
            selected_profile: None,
            selection: std::collections::BTreeSet::new(),
            notice: None,
            trusted_keys: TrustedKeys::default(),
            provider_test: None,
            pending_paste: None,
            pending_transfers: std::collections::VecDeque::new(),
            pending_operation: None,
            vault,
            run_trust: RunTrust::default(),
            rdp_run_trust: Vec::new(),
            closed_folders: std::collections::HashSet::new(),
        };
        // The PIN when one is set, then a vault on disk is offered to unlock: its passwords
        // are then ready.
        app.show_start_gates();
        app
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

    /// Saved local shell profiles.
    #[must_use]
    pub fn local_profiles(&self) -> &[LocalProfile] {
        self.store.local_profiles()
    }

    /// Saved `WinRM` profiles.
    #[must_use]
    pub fn winrm_profiles(&self) -> &[WinRmProfile] {
        self.store.winrm_profiles()
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

    /// Shows `tab`: its bell is heard, and a desktop's server gets what was copied meanwhile.
    fn select_tab(&mut self, tab: TabId) -> Vec<Effect> {
        let Some(found) = self.tab_mut(tab) else {
            return Vec::new();
        };
        found.bell = false;
        self.active = Some(tab);
        self.tab(tab).map(clipboard_offer).unwrap_or_default()
    }

    /// Applies a message.
    pub fn update(&mut self, message: Message) -> Vec<Effect> {
        self.forget_stale_notice();
        match message {
            message @ (Message::OpenProfile(_)
            | Message::OpenFiles(_)
            | Message::OpenRdp(_)
            | Message::OpenTelnet(_)
            | Message::OpenVnc(_)
            | Message::OpenLocal(_)
            | Message::OpenLocalProfile(_)
            | Message::OpenWinRm(_)
            | Message::ReconnectTab(_)
            | Message::ConnectAs { .. }
            | Message::QuickConnect(_)
            | Message::ForgetServer(_)) => self.open_message(message),
            message @ (Message::DesktopResize { .. }
            | Message::DesktopInput { .. }
            | Message::SendKeys { .. }) => {
                self.desktop_message(message);
                Vec::new()
            }
            Message::Files(message) => self.files(message),
            Message::SelectTab(tab) => self.select_tab(tab),
            Message::RequestCloseTab(tab) => self.request_close(tab),
            Message::TabMenu(message) => self.tab_menu(message),
            message @ (Message::AutoReconnect { .. } | Message::CancelAutoReconnect(_)) => {
                self.retry_message(&message)
            }
            Message::Connection {
                tab,
                attempt,
                event,
            } => self.connection(tab, attempt, event),
            Message::Answer {
                tab,
                question,
                answer,
            } => self.answer(tab, question, answer),
            Message::HostKeyDecision { tab, accept } => self.host_key_decision(tab, accept.into()),
            Message::HostKeyTrustOnce(tab) => self.host_key_decision(tab, KeyTrust::Once),
            Message::Key { tab, input } => self.key(tab, &input),
            Message::Pointer { tab, input } => self.pointer(tab, input),
            Message::Resize { tab, grid, cell } => self.resize(tab, grid, cell),
            message @ (Message::ScrollHistory { .. }
            | Message::FindInTerminal { .. }
            | Message::Copy(_)
            | Message::PasteRequest(_)
            | Message::SendClipboard(_)
            | Message::ClipboardText { .. }) => self.clipboard_message(message),
            Message::SyncDeadline { tab, generation } => self.sync_deadline(tab, generation),
            message @ (Message::WindowFocus(_)
            | Message::WindowCloseRequested
            | Message::ImportLegacy
            | Message::Settings(_)
            | Message::Broadcast(_)) => self.window_message(&message),
            message @ (Message::NewProfile
            | Message::EditProfile(_)
            | Message::ProfileField { .. }
            | Message::DeleteProfile
            | Message::SaveProfile { .. }
            | Message::ClearPassword
            | Message::ChooseProtocol(_)
            | Message::ProfileToggle { .. }
            | Message::PostConnectEdit(_)
            | Message::ProfileChoice(_)
            | Message::NewGateway
            | Message::EditGateway(_)
            | Message::GatewayField { .. }
            | Message::ChooseParentGateway(_)
            | Message::ClearGatewayPassword
            | Message::SaveGateway { .. }
            | Message::ChooseGateway(_)) => {
                self.profile_message(message);
                Vec::new()
            }
            message @ (Message::ConfirmDialog
            | Message::DismissDialog
            | Message::SkipPostConnect
            | Message::StopPostConnect(_)) => self.dialog_message(&message),
            message @ (Message::SelectProfile(_)
            | Message::ToggleFolder(_)
            | Message::Folder(_)
            | Message::ProfileMenu(_)
            | Message::Selection(_)
            | Message::ConnectProfile(_)
            | Message::DuplicateProfile { .. }
            | Message::RequestDeleteProfile(_)
            | Message::CopyProfile { .. }) => self.tree_message(message),
            message @ (Message::ShowVault
            | Message::ChangeMasterPassword
            | Message::DisableMasterPassword
            | Message::SubmitVault { .. }
            | Message::VaultOpened(_)
            | Message::LockVault) => self.vault_message(message),
            Message::Pin(message) => self.pin_message(message),
            Message::CredentialProvider(message) => self.provider_message(message),
            Message::CredentialProvided(answer) => self.provider_answered(*answer),
        }
    }

    /// Answers the open dialog, or stops a tab's post-connect steps.
    fn dialog_message(&mut self, message: &Message) -> Vec<Effect> {
        match message {
            Message::ConfirmDialog => self.confirm_dialog(),
            Message::DismissDialog => self
                .dismiss_vault()
                .or_else(|| self.dismiss_pin())
                .unwrap_or_else(|| {
                    self.dismiss_dialog();
                    Vec::new()
                }),
            _ => self.post_connect_message(message),
        }
    }

    /// Scrolls the history of `tab_id` by `lines`, up when positive.
    /// Scrolls a tab's history, or looks for text in it.
    fn history_message(&mut self, message: Message) -> Vec<Effect> {
        match message {
            Message::ScrollHistory { tab, lines } => {
                if let Some(found) = self.tab_mut(tab) {
                    found.terminal.scroll(lines);
                }
            }
            Message::FindInTerminal {
                tab,
                query,
                direction,
            } => {
                if let Some(found) = self.tab_mut(tab) {
                    found.find_missed = !found.terminal.find(&query, direction);
                }
            }
            _ => {}
        }
        Vec::new()
    }

    /// Applies a message for a remote desktop.
    fn desktop_message(&mut self, message: Message) {
        match message {
            Message::DesktopResize { tab, width, height } => {
                if let Some(pane) = self.tab(tab).and_then(|found| found.desktop.as_ref()) {
                    pane.resize(width, height);
                }
            }
            Message::DesktopInput { tab, inputs } => self.desktop_input(tab, &inputs),
            Message::SendKeys { tab, keys } => self.desktop_input(tab, &keys.inputs()),
            _ => {}
        }
    }

    /// Applies a message opening a session: a new tab, or a tab's session again.
    fn open_message(&mut self, message: Message) -> Vec<Effect> {
        match message {
            Message::OpenProfile(id) => self.open_profile(&id, Purpose::Shell),
            Message::OpenFiles(id) => self.open_profile(&id, Purpose::Files),
            Message::OpenRdp(id) => self.open_rdp(&id),
            Message::OpenTelnet(id) => self.open_telnet(&id),
            Message::OpenVnc(id) => self.open_vnc(&id),
            Message::OpenLocal(shell) => self.open_local(shell),
            Message::OpenLocalProfile(id) => self.open_local_profile(&id),
            Message::OpenWinRm(id) => self.open_winrm(&id),
            Message::ReconnectTab(tab) => self.reconnect_tab(tab),
            Message::ForgetServer(tab) => self.forget_server(tab),
            Message::ConnectAs { id, protocol } => self.connect_as(&id, protocol),
            Message::QuickConnect(result) => self.quick_connect(result),
            _ => Vec::new(),
        }
    }

    /// Closes the open dialog and drops what it held; the gateway dialog returns to the
    /// session's form it was opened from.
    fn dismiss_dialog(&mut self) {
        if self.dismiss_gateway() {
            return;
        }
        self.dialog = None;
        self.pending_paste = None;
        self.pending_transfers.clear();
        self.pending_operation = None;
    }

    /// What connecting to `profile` needs, its gateways included; an error when they cannot
    /// be followed, which no attempt could get past.
    fn connect_request(
        &self,
        profile: &SshProfile,
        grid: GridSize,
        cancel: CancellationToken,
        purpose: Purpose,
    ) -> Result<ConnectRequest, UiError> {
        let route = self
            .store
            .route(profile.gateway.as_ref())
            .map_err(UiError::Route)?;
        let mut options = ConnectOptions::new(self.config.known_hosts.clone());
        options.agent = self.config.agent.clone();
        options.run_trust = self.run_trust.clone();
        options.initial_size = terminal_size(grid, None);
        options.forward_agent = profile.forward_agent;
        Ok(ConnectRequest {
            profile: profile.clone(),
            route: route.iter().map(SshGateway::as_hop).collect(),
            purpose,
            options,
            cancel,
        })
    }

    fn open_profile(&mut self, id: &ProfileId, purpose: Purpose) -> Vec<Effect> {
        let Some(profile) = self.profiles().iter().find(|p| &p.id == id).cloned() else {
            return Vec::new();
        };
        self.open_ssh(profile, purpose)
    }

    /// Opens a tab for `profile`, a shell or its files, without asking about its steps.
    pub(super) fn open_ssh_now(&mut self, profile: SshProfile, purpose: Purpose) -> Vec<Effect> {
        let grid = self.viewport;
        let tab_id = TabId::fresh();
        let attempt = AttemptId::fresh();
        let cancel = CancellationToken::new();
        let request = self.connect_request(&profile, grid, cancel.clone(), purpose);
        let mut tab = Tab::new(
            self.terminal_palette(),
            tab_id,
            TabProfile::Ssh(profile),
            purpose,
            grid,
            attempt,
            cancel,
        );
        tab.files = (purpose == Purpose::Files)
            .then(|| Box::new(FilesPane::new(self.config.files_start.clone())));
        let effects = match request {
            Ok(request) => vec![Effect::Connect {
                tab: tab_id,
                attempt,
                request: Box::new(request),
            }],
            // Shown in its tab, as a failed attempt would be.
            Err(error) => {
                tab.phase = Phase::Failed(error);
                Vec::new()
            }
        };
        self.tabs.push(tab);
        self.active = Some(tab_id);
        effects
    }

    /// The user answered `question`; `None` declines it.
    fn answer(&mut self, tab: TabId, question: QuestionId, answer: Option<Answer>) -> Vec<Effect> {
        if let Some(found) = self.tab_mut(tab) {
            found.prompts.retain(|prompt| prompt.question != question);
        }
        vec![Effect::Answer { question, answer }]
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
        if let ConnectionEvent::Question { question, kind } = &event
            && let Some(answer) = self.saved_answer(tab_id, kind)
        {
            return vec![Effect::Answer {
                question: *question,
                answer: Some(answer),
            }];
        }
        // No password saved: the external credential provider, when it is to be asked.
        if let ConnectionEvent::Question { question, kind } = &event
            && let Some(request) = self.provider_request(tab_id, *question, kind)
        {
            return vec![request];
        }
        if matches!(event, ConnectionEvent::Failed(_)) {
            self.credentials_failed(tab_id);
        }
        if let ConnectionEvent::Output(bytes) = &event {
            self.record(tab_id, bytes);
        }
        let was_connected = self
            .tab(tab_id)
            .is_some_and(|tab| tab.phase == Phase::Connected);
        let failure = match &event {
            ConnectionEvent::Failed(error) => {
                Some((error.clone(), self.tab(tab_id).is_some_and(Tab::is_live)))
            }
            _ => None,
        };
        let mut effects = self.apply_connection_event(tab_id, event);
        self.follow_transcript(tab_id, was_connected);
        if let Some((error, was_live)) = failure {
            effects.extend(self.retry_after(tab_id, &error, was_live));
        }
        effects
    }

    fn apply_connection_event(&mut self, tab_id: TabId, event: ConnectionEvent) -> Vec<Effect> {
        let active = self.active == Some(tab_id);
        let Some(tab) = self.tab_mut(tab_id) else {
            return Vec::new();
        };
        match event {
            ConnectionEvent::Question { question, kind } => {
                tab.retry = None;
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
                tab.retry = None;
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
                // A new desktop is offered this side's clipboard at once.
                clipboard_offer(tab)
            }
            ConnectionEvent::RemoteClipboard(text) => {
                vec![Effect::WriteClipboard(String::clone(&text))]
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
            ConnectionEvent::PostConnect(progress) => {
                tab.post_connect = Some(progress);
                Vec::new()
            }
            ConnectionEvent::PostConnectDone => {
                tab.post_connect = None;
                Vec::new()
            }
            ConnectionEvent::Closed { exit_status } => {
                tab.phase = Phase::Closed { exit_status };
                tab.post_connect = None;
                tab.sink = None;
                tab.desktop = None;
                tab.prompts.clear();
                Vec::new()
            }
            ConnectionEvent::Ended { reason } => {
                tab.phase = Phase::Closed { exit_status: None };
                tab.end_reason = Some(reason);
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

    fn host_key_decision(&mut self, tab_id: TabId, trust: KeyTrust) -> Vec<Effect> {
        // An RDP tab asks about the server's certificate, or, on the way, a gateway's SSH key.
        if self.tab(tab_id).is_some_and(Tab::asks_about_certificate) {
            return self.rdp_certificate_decision(tab_id, trust);
        }
        let known_hosts = KnownHosts::new(&self.config.known_hosts);
        let run_trust = self.run_trust.clone();
        let Some(tab) = self.tab_mut(tab_id) else {
            return Vec::new();
        };
        let (Phase::HostKey { host, port, .. }, Some(key)) =
            (tab.phase.clone(), tab.pending_host_key.take())
        else {
            return Vec::new();
        };
        if trust == KeyTrust::Refused {
            tab.phase = Phase::Failed(UiError::Cancelled);
            return Vec::new();
        }
        // Another tab may have recorded a key for this host meanwhile: read again.
        let learned = match known_hosts.recorded(&host, port) {
            // Held in memory for this run: the file is not written.
            Ok(_) if trust == KeyTrust::Once => {
                run_trust.trust(&host, port, PublicKey::clone(&key));
                Ok(())
            }
            Ok(recorded) => match verdict(&recorded, &key) {
                Verdict::Trusted => Ok(()),
                Verdict::Unknown => known_hosts
                    .learn(&host, port, &key)
                    .map_err(|error| UiError::from(&error)),
                Verdict::Changed { recorded } => Err(UiError::HostKeyChanged {
                    target: Some(ServerAddress {
                        host: host.clone(),
                        port,
                    }),
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
        let profile = match tab.profile.clone() {
            TabProfile::Ssh(profile) => profile,
            // A gateway's key, learnt: the RDP connection starts again through it.
            TabProfile::Rdp(_) => return self.reconnect_rdp(tab_id, None),
            _ => return Vec::new(),
        };
        let grid = tab.terminal.size();
        let attempt = AttemptId::fresh();
        let cancel = CancellationToken::new();
        tab.attempt = attempt;
        tab.cancel = cancel.clone();
        tab.phase = Phase::Connecting;
        tab.connect_grid = grid;
        let purpose = tab.purpose;
        match self.connect_request(&profile, grid, cancel, purpose) {
            Ok(request) => vec![Effect::Connect {
                tab: tab_id,
                attempt,
                request: Box::new(request),
            }],
            Err(error) => {
                if let Some(tab) = self.tab_mut(tab_id) {
                    tab.phase = Phase::Failed(error);
                }
                Vec::new()
            }
        }
    }

    fn key(&mut self, tab_id: TabId, input: &KeyInput) -> Vec<Effect> {
        // A dialog owns the keyboard: Enter meant for it must not run a command.
        if self.dialog.is_some() {
            return Vec::new();
        }
        let typed_into = self
            .tab(tab_id)
            .is_some_and(|tab| tab.phase == Phase::Connected && tab.prompts.is_empty());
        if typed_into {
            for target in self.input_targets(tab_id) {
                self.send_key(target, input);
            }
        }
        Vec::new()
    }

    /// Sends `input` to `tab_id`'s session, encoded for its modes.
    fn send_key(&mut self, tab_id: TabId, input: &KeyInput) {
        let Some(tab) = self.tab_mut(tab_id) else {
            return;
        };
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
            // Selecting copies, as in PuTTY and MobaXterm: no shortcut to learn.
            MouseAction::Release(MouseButton::Left) => {
                let selected = std::mem::take(&mut tab.selecting);
                if selected && let Some(text) = tab.terminal.selected_text() {
                    return vec![Effect::WriteClipboard(text)];
                }
            }
            // A right click pastes, through the same checks as the paste shortcut.
            MouseAction::Press(MouseButton::Right) => {
                return vec![Effect::ReadClipboard { tab: tab_id }];
            }
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

    /// The window gained or lost the focus: the terminal shown is told when it asked to be,
    /// and a desktop shown gets what was copied elsewhere meanwhile.
    fn window_focus(&self, focused: bool) -> Vec<Effect> {
        let Some(found) = self.active.and_then(|id| self.tab(id)) else {
            return Vec::new();
        };
        if let Some(bytes) = encode_focus(focused, &found.terminal.input_mode()) {
            found.write(bytes);
        }
        if focused {
            clipboard_offer(found)
        } else {
            Vec::new()
        }
    }

    /// This side's clipboard, read for `tab_id`: offered to its server when it is a desktop
    /// sharing the clipboard, pasted into it when it is a terminal.
    /// Applies a message about the clipboard: a terminal's selection copied, this side's
    /// clipboard asked for a paste or for a desktop, and what it held; or about a terminal's
    /// history, scrolled or searched.
    fn clipboard_message(&mut self, message: Message) -> Vec<Effect> {
        match message {
            Message::Copy(tab) => self
                .tab(tab)
                .and_then(|found| found.terminal.selected_text())
                .map(Effect::WriteClipboard)
                .into_iter()
                .collect(),
            Message::PasteRequest(tab) => vec![Effect::ReadClipboard { tab }],
            Message::SendClipboard(tab) => self
                .tab(tab)
                .and_then(|found| found.desktop.as_deref())
                .filter(|pane| pane.accepts_clipboard())
                .map(|_| Effect::ReadClipboard { tab })
                .into_iter()
                .collect(),
            Message::ClipboardText { tab, text } => self.clipboard_text(tab, text),
            message => self.history_message(message),
        }
    }

    fn clipboard_text(&mut self, tab_id: TabId, text: Option<String>) -> Vec<Effect> {
        if let Some(pane) = self
            .tab(tab_id)
            .and_then(|tab| tab.desktop.as_deref())
            .filter(|pane| pane.accepts_clipboard())
        {
            if let Some(text) = text.filter(|text| !text.is_empty()) {
                pane.offer_clipboard(text);
            }
            return Vec::new();
        }
        self.paste(tab_id, text)
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
        let targets = self.input_targets(tab_id);
        // Asked when a session reached would run the lines one by one.
        let runs_lines = text.trim_end().contains(['\n', '\r'])
            && targets.iter().any(|target| {
                self.tab(*target)
                    .is_some_and(|tab| !tab.terminal.input_mode().bracketed_paste)
            });
        if runs_lines {
            self.dialog = Some(Dialog::ConfirmPaste {
                tab: tab_id,
                lines: command_lines(&text),
            });
            self.pending_paste = Some((tab_id, text));
            return Vec::new();
        }
        self.paste_to(&targets, &text);
        Vec::new()
    }

    /// Pastes `text` into the sessions of `targets`, each as its modes ask.
    fn paste_to(&self, targets: &[TabId], text: &str) {
        for tab in targets.iter().filter_map(|target| self.tab(*target)) {
            tab.write(encode_paste(text, &tab.terminal.input_mode()));
        }
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
            Some(Dialog::ConfirmCloseTabs { tabs, .. }) => {
                for tab in tabs {
                    self.close_tab(tab);
                }
                Vec::new()
            }
            Some(Dialog::RenameTab { tab, value }) => {
                self.rename_tab(tab, &value);
                Vec::new()
            }
            Some(Dialog::FolderName { naming, value, .. }) => {
                self.confirm_folder_name(naming, value);
                Vec::new()
            }
            Some(Dialog::ConfirmDeleteProfiles { ids, .. }) => {
                self.confirm_delete_profiles(&ids);
                Vec::new()
            }
            Some(Dialog::RenameProfile { id, value }) => {
                self.confirm_rename_profile(&id, &value);
                Vec::new()
            }
            Some(Dialog::ConfirmDeleteFolder { path, .. }) => {
                self.confirm_delete_folder(&path);
                Vec::new()
            }
            Some(Dialog::ConfirmConnectFolder { path, .. }) => self.confirm_connect_folder(&path),
            Some(Dialog::ConfirmBroadcast) => {
                self.confirm_broadcast();
                Vec::new()
            }
            Some(Dialog::ForgetTrustedKey(key)) => {
                self.forget_trusted_key(&key);
                Vec::new()
            }
            Some(Dialog::ConfirmExit { .. }) => {
                for tab in &mut self.tabs {
                    tab.stop();
                }
                vec![Effect::Exit]
            }
            Some(Dialog::ConfirmPaste { .. }) => {
                if let Some((tab_id, text)) = self.pending_paste.take() {
                    self.paste_to(&self.input_targets(tab_id), &text);
                }
                Vec::new()
            }
            Some(Dialog::ConfirmOverwrite { .. }) => self.confirm_overwrite(),
            Some(Dialog::AskName { value, .. }) => self.confirm_operation(Some(&value)),
            Some(Dialog::ConfirmDelete { .. }) => self.confirm_operation(None),
            Some(Dialog::EditProfile { draft, .. }) => {
                self.save_profile(draft, None);
                Vec::new()
            }
            Some(Dialog::ConfirmDeleteProfile { id, .. }) => {
                self.delete_profile(&id);
                Vec::new()
            }
            Some(Dialog::ConfirmLocalCommand(confirmation)) => self.confirm_local(*confirmation),
            Some(Dialog::ConfirmPostConnect(confirmation)) => self.run_post_connect(*confirmation),
            Some(dialog @ (Dialog::Vault(_) | Dialog::Pin(_) | Dialog::EditGateway { .. })) => {
                self.dialog = Some(dialog);
                Vec::new()
            }
            Some(
                Dialog::ImportDone(_)
                | Dialog::FileProperties(_)
                | Dialog::ImportFailed { .. }
                | Dialog::StoreError { .. }
                | Dialog::PasswordSaveFailed { .. },
            )
            | None => Vec::new(),
        }
    }

    /// The window's focus and its close, and what it asks of the application: the import
    /// from the C# Heimdall, a change of the settings or of broadcast input.
    fn window_message(&mut self, message: &Message) -> Vec<Effect> {
        match message {
            Message::WindowFocus(focused) => self.window_focus(*focused),
            Message::WindowCloseRequested => self.close_window(),
            Message::ImportLegacy => {
                self.import_legacy();
                Vec::new()
            }
            Message::Settings(message) => self.settings_message(message),
            Message::Broadcast(message) => self.broadcast_message(*message),
            _ => Vec::new(),
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
            let local = store.merge_local(report.local);
            let winrm = store.merge_winrm(report.winrm);
            let gateways = store.merge_gateways(report.gateways);
            [ssh, rdp, telnet, vnc, local, winrm, gateways]
                .into_iter()
                .fold(MergeReport::default(), |total, one| MergeReport {
                    added: total.added + one.added,
                    updated: total.updated + one.updated,
                    unchanged: total.unchanged + one.unchanged,
                })
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
