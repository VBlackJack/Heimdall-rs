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
use std::time::{Duration, Instant};

use heimdall_core::import::csharp::{self, SkipReason};
use heimdall_core::import::foreign::FileWarning;
use heimdall_core::paths::{LEGACY_SERVERS_FILE_NAME, LEGACY_SETTINGS_FILE_NAME};
use heimdall_core::profile::{
    FtpProfile, LocalProfile, ProfileId, RdpProfile, SshGateway, SshProfile, TelnetProfile,
    VncProfile, WinRmProfile,
};
use heimdall_core::settings::Settings;
use heimdall_core::store::{MergeReport, ProfileStore, StoreError};
use heimdall_core::winrm_diagnostic::{Diagnostic, EarlyOutput};
use heimdall_ssh::known_hosts_import::{self, OtherAlgorithm, Trusting};
use heimdall_ssh::{
    AgentSource, ConnectOptions, HostKeySource, KeyboardInteractivePrompt, KnownHosts, PublicKey,
    RunTrust, Secret, TerminalSize, fingerprint,
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
use crate::error::UiError;
use crate::event::{Answer, ConnectionEvent, PostConnectProgress, QuestionKind};
use crate::files::{FileOperation, FilesPane, Side, TransferId, TransferRequest};
use crate::ftp_driver::FtpRequest;
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
use crate::text::{server_prompt_text, server_text};
use crate::vnc_driver::VncRequest;
use crate::winrm_driver::WinRmRequest;
use crate::x11_server::X11Settings;

mod address_test;
mod agent_chip;
mod appearance;
mod auto_reconnect;
mod broadcast;
mod bulk_edit;
mod bulk_password;
mod citrix_import;
mod citrix_launch;
mod connect_as;
mod credential_guard_gate;
mod detail;
mod docked_sftp;
mod file_import;
mod files_clipboard;
mod files_edit;
mod files_editor;
mod files_sudo;
mod files_tab;
mod files_terminal;
mod floating;
mod folder_menu;
mod folders;
mod ftp_tab;
mod gateway_overview;
mod gateways;
mod health_tab;
mod hello_gate;
mod hostkeys_import;
mod idle_lock;
mod keep_alive;
mod legacy_migration;
mod local_browser;
mod local_menu;
mod local_tab;
mod macro_editor;
mod macros;
mod mstsc_launch;
mod pin;
mod post_connect;
mod profile_import;
mod profile_menu;
mod profiles;
mod provider;
mod provider_connect;
mod putty_launch;
mod quick_connect;
mod rdp_import;
mod rdp_tab;
mod reachability_monitor;
mod reconnect;
mod resolution;
mod route_test;
mod run_in_shell;
mod selection;
mod session_events;
mod session_restore;
mod sessions_import;
mod settings_transfer;
mod shell_directory;
pub mod split;
mod status;
mod tab_menu;
mod telnet_tab;
mod tools;
mod transcripts;
mod tree;
mod tree_drag;
mod tree_filter;
mod trusted_keys;
mod tunnels;
mod updates;
mod vault;
mod vault_hello;
mod vnc_tab;
mod winrm_tab;

use crate::transcript::{Transcript, TranscriptLines};
pub use agent_chip::AgentChip;
pub use appearance::SettingsMessage;
pub use auto_reconnect::Retry;
pub use broadcast::BroadcastMessage;
pub use bulk_edit::{BulkField, BulkRefusal};
pub use bulk_password::{BulkPasswordRefusal, BulkPasswordSkips, bulk_password_refusal};
pub use citrix_import::CitrixImportOutcome;
pub use connect_as::ConnectAs;
pub use detail::SavedCredentials;
pub use file_import::{FileKind, ImportFile, PendingImport};
pub use files_clipboard::{ClipMode, FilesClipboard};
pub use files_edit::SudoAction;
pub use files_tab::FilesMessage;
use files_tab::{PendingOperation, PendingPlan};
pub use floating::{FloatMessage, Floating};
pub use folder_menu::{FolderMessage, FolderNaming};
pub use folders::{NO_FOLDER, TreeRow};
pub use gateway_overview::{
    GatewayEntry, GatewayOverview, GatewaysMessage, MissingGateway, RoutedSession,
};
pub use hostkeys_import::{HostKeyRow, HostKeysMessage, HostKeysOutcome, HostKeysPreview};
pub use idle_lock::{IDLE_POLL, should_auto_lock};
pub use legacy_migration::{LegacyMigrationDone, LegacyMigrationMessage};
pub use local_tab::{ElevatedPane, ElevatedState, LocalConfirmation};
pub use macro_editor::{EntryDraft, EntryField, EntryProblem, MacroDraft, MacroEdit, MacroProblem};
pub use macros::{MacroMenu, MacroMessage, MacroPlaying, MacroRecording};
pub use pin::{PinDialog, PinFailure, PinMessage, PinMode};
pub use post_connect::PostConnectConfirmation;
pub use profile_import::{
    ImportActions, ProfileImportMessage, ProfileImportPreview, ProfileImportRow,
};
pub use profile_menu::ProfileMenuMessage;
pub use provider::{ProviderMessage, UNLOCK_SECRET_ENTRY};
pub use provider_connect::{ProviderAnswer, ProviderRequest};
pub use quick_connect::QuickResult;
pub use rdp_import::{RDP_EXTENSION, RdpMessage, RdpNames, RdpOutcome, RdpPreview, RdpRow};
use rdp_tab::ResizeFallback;
pub use resolution::ResolutionChoice;
pub use run_in_shell::ScriptConfirmation;
pub use selection::SelectionMessage;
pub use session_restore::{RestoreDialog, RestoreRow};
pub use sessions_import::{
    SessionsCounts, SessionsMessage, SessionsPreview, SessionsRow, SessionsSource,
};
pub use settings_transfer::SettingsTransferMessage;
pub use status::{
    ANNOUNCEMENTS_KEPT, Announced, Announcement, Notice, SessionState, SessionStatus,
};
pub use tab_menu::{TabGroup, TabMenuMessage};
pub use tools::ToolsMessage;
pub use tree::{GatewayBadge, ProfileCopy, ProfileKind, ProfileSummary, search_folded};
pub use tree_drag::{DropTarget, OrganizationChange};
pub use tree_filter::{FilterMessage, TreeFilter};
pub use trusted_keys::{TrustedKey, TrustedKeys, TrustedKeysMessage, local_date_time};
pub use tunnels::TunnelMessage;
pub use updates::{UpdateMessage, UpdateStatus};
use vault::VaultState;
pub use vault::{
    LONG_MASTER_PASSWORD_CHARS, MIN_MASTER_PASSWORD_CHARS, MIN_MASTER_PASSWORD_CLASSES,
    OpenedVault, SystemCredentials, VAULT_FILE_NAME, VaultDialog, VaultJob, VaultMode,
    VaultProblem, VaultStatus, VaultTicket, master_password_problem, open_vault,
};
pub use vault_hello::{VaultHelloCard, VaultHelloMessage, VaultHelloStatus};

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
    /// Open a saved RDP profile in `mode` this once, as the C# profile menu's "Connect
    /// with" (`ContextMenuFactory.cs:266-283`): the profile is not changed.
    OpenRdpWith {
        /// The profile.
        id: ProfileId,
        /// Where its desktop opens this time.
        mode: heimdall_core::profile::RdpMode,
    },
    /// Open a Telnet tab for a saved Telnet profile.
    OpenTelnet(ProfileId),
    /// Open a VNC tab for a saved VNC profile.
    OpenVnc(ProfileId),
    /// Open a Files tab for a saved FTP profile.
    OpenFtp(ProfileId),
    /// Open a local shell tab.
    OpenLocal(LocalShell),
    /// Open a saved local profile, asking first unless what it runs is approved.
    OpenLocalProfile(ProfileId),
    /// A local shell run as administrator was started in a window of its own, or why not.
    ElevatedLaunched {
        /// The tab that says so.
        tab: TabId,
        /// How the start went.
        outcome: crate::elevated_shell::ElevatedOutcome,
    },
    /// Open a `WinRM` tab for a saved `WinRM` profile.
    OpenWinRm(ProfileId),
    /// Launch a saved Citrix profile's application, outside Heimdall, and open its status
    /// tab.
    OpenCitrix(ProfileId),
    /// A Citrix application was launched, or why not.
    CitrixLaunched {
        /// Its status tab.
        tab: TabId,
        /// The profile's name.
        name: String,
        /// Launched, or why not.
        result: Result<crate::citrix_session::Launched, crate::citrix::CitrixRefusal>,
    },
    /// Time to look at the Citrix tabs' clients.
    CitrixTick,
    /// A message about the Tools area: a tool opened, pinned, the sidebar's tab.
    Tools(ToolsMessage),
    /// What a Citrix tab's probe saw.
    CitrixProbed {
        /// The tab.
        tab: TabId,
        /// What it saw.
        probe: crate::citrix_session::Probe,
    },
    /// End a Citrix tab's session, as the C# "Terminate", asked first: its client asked to
    /// close, or forced once it did not.
    CitrixTerminate {
        /// The tab.
        tab: TabId,
        /// Whether the client is forced.
        force: bool,
    },
    /// What a request to end a Citrix tab's client came to.
    CitrixTerminated {
        /// The tab.
        tab: TabId,
        /// The client process asked.
        pid: u32,
        /// What it came to.
        result: crate::citrix_terminate::TerminateResult,
    },
    /// An RDP profile was opened in Remote Desktop Connection, or why not.
    RdpExternalLaunched {
        /// The profile's name.
        name: String,
        /// The RD Gateway that sent it there, when the profile did not ask for it.
        gateway: Option<String>,
        /// Started, or why not.
        result: Result<(), crate::rdp_external::ExternalRefusal>,
    },
    /// What a launch of Remote Desktop Connection through an SSH gateway reports.
    MstscRoute {
        /// The launch.
        id: crate::mstsc_driver::MstscRouteId,
        /// What happened.
        event: crate::mstsc_driver::MstscRouteEvent,
    },
    /// The host key of an SSH profile to open in `PuTTY` was probed.
    PuttyHostKey {
        /// The profile, as it was to open.
        profile: Box<SshProfile>,
        /// What the probe found.
        probe: crate::putty::HostKeyProbe,
    },
    /// `PuTTY` was started for an SSH profile, or why not.
    PuttyLaunched {
        /// The profile's name.
        name: String,
        /// Started, or why not.
        result: Result<crate::putty::PuttyStarted, crate::putty::PuttyRefusal>,
    },
    /// What a launch of `PuTTY` through a gateway reports.
    PuttyRoute {
        /// The launch.
        id: crate::putty_driver::PuttyRouteId,
        /// What happened.
        event: crate::putty_driver::PuttyRouteEvent,
    },
    /// The size a tab shows its remote desktop at, in pixels.
    DesktopResize {
        /// Tab.
        tab: TabId,
        /// Width.
        width: u16,
        /// Height.
        height: u16,
    },
    /// The size a tab shows its remote desktop in, in pixels, kept without asking the server
    /// for it: the desktop keeps a size of its own, or is shown scaled.
    DesktopShown {
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
    /// The quality a tab's VNC desktop is asked at, from its toolbar's "Quality" menu.
    VncQuality {
        /// Tab.
        tab: TabId,
        /// Which.
        quality: heimdall_remote::vnc::Quality,
    },
    /// A key combination from the session's menu, for the remote desktop of a tab.
    SendKeys {
        /// Tab.
        tab: TabId,
        /// Which.
        keys: SpecialKeys,
    },
    /// Time for the anti-idle keys of the sessions asking for them.
    AntiIdleTick,
    /// The window's screen draws this many physical pixels per logical one: RDP desktops
    /// opened from now on ask for that scale.
    DisplayScale(f32),
    /// Time to look at the idle SSH shells for their `TMOUT` reset.
    TmoutResetTick,
    /// Time to ask the servers whose health panel is shown.
    HealthTick,
    /// Time for the background check of every server.
    ReachabilityTick,
    /// The look for a newer release: its ticks, its answer, "Check now" and the banner.
    Update(UpdateMessage),
    /// A server answered the background check, or did not.
    ReachabilityChecked {
        /// The profile.
        id: ProfileId,
        /// What was found.
        verdict: crate::reachability::Verdict,
    },
    /// A server said how it is.
    HealthRead {
        /// Tab.
        tab: TabId,
        /// What it said.
        health: Box<crate::server_health::ServerHealth>,
    },
    /// Stop the anti-idle keys of a tab's session, until it connects again.
    StopAntiIdle(TabId),
    /// A second of the RDP desktops settling after connecting, at this instant: those whose
    /// wait is over follow their tab again.
    StabilizationTick(Instant),
    /// The C# Resolution menu's "Skip stabilization": the tab's desktop follows its tab now.
    SkipStabilization(TabId),
    /// Send this side's clipboard to the remote desktop of a tab, as the C# Heimdall's
    /// noVNC "sync" does: on a click, never by itself over a clear VNC connection.
    SendClipboard(TabId),
    /// Save the files the RDP server of a tab copied: a folder is asked for.
    SaveRemoteFiles(TabId),
    /// The folder picked to save the RDP server's files into; `None` when the dialog was
    /// closed.
    SaveFolderPicked {
        /// Tab.
        tab: TabId,
        /// The folder.
        folder: Option<PathBuf>,
    },
    /// Stop saving the RDP server's files: what was saved stays.
    CancelSave(TabId),
    /// Forget the recorded key of the server of a tab whose key changed, and connect again.
    ForgetServer(TabId),
    /// Open the session of a tab again, in its place: a failed or ended one, or, from the
    /// tab's menu, a live one.
    ReconnectTab(TabId),
    /// Something in a Files tab.
    Files(FilesMessage),
    /// Something about the tunnels the user opens by hand.
    Tunnel(TunnelMessage),
    /// Show a tab.
    SelectTab(TabId),
    /// A tab dragged onto another: it takes that one's place, within its own group, pinned
    /// or not, as the C# `MoveSession`.
    MoveTab {
        /// The tab dragged.
        tab: TabId,
        /// The tab it was let go over.
        onto: TabId,
    },
    /// Close a tab, asking first when its session is live.
    RequestCloseTab(TabId),
    /// End a tab's remote desktop from its bar, the tab kept to reconnect.
    DisconnectDesktop(TabId),
    /// Something from a tab's menu.
    TabMenu(TabMenuMessage),
    /// Something done to a split tab or to one of its panes.
    Split(split::SplitMessage),
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
    /// Copy the fingerprint the tab's key question shows, as the C# prompts' Copy button.
    CopyHostKeyFingerprint(TabId),
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
    /// The session shown was copied to the clipboard as an image, or could not be.
    ScreenshotTaken {
        /// Whether it is on the clipboard.
        copied: bool,
    },
    /// The clipboard's image arrived for the desktop of `tab`, a device-independent bitmap:
    /// it held no files and no text.
    ClipboardImage {
        /// Tab whose desktop shares the clipboard.
        tab: TabId,
        /// The image.
        image: Vec<u8>,
    },
    /// The files copied in Explorer arrived for the desktop of `tab`.
    ClipboardFiles {
        /// Tab whose desktop shares the clipboard.
        tab: TabId,
        /// The files and folders copied.
        paths: Vec<PathBuf>,
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
    /// "Import Citrix Apps": scan Citrix Workspace's local cache.
    ImportCitrix,
    /// Citrix Workspace's local cache scanned.
    CitrixScanned(heimdall_core::import::citrix_cache::CacheScan),
    /// Export every profile and gateway in the C# Heimdall's session file.
    ExportSessions,
    /// The import of an OpenSSH configuration.
    Sessions(SessionsMessage),
    /// The import of `.rdp` files.
    Rdp(RdpMessage),
    /// The preview of a Heimdall session document's profiles.
    ProfileImport(ProfileImportMessage),
    /// How the export's file went.
    ExportFinished(ExportOutcome),
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
    /// Sessions dragged onto the tree's `onto`, as the C# tree drops them.
    DropProfiles {
        /// The sessions.
        ids: Vec<ProfileId>,
        /// Where.
        onto: DropTarget,
    },
    /// A folder dragged onto the tree's `onto`.
    DropFolder {
        /// The folder.
        path: String,
        /// Where.
        onto: DropTarget,
    },
    /// Undo the last change of the tree's organization, as the C# tree's Undo bar and
    /// Ctrl+Z.
    UndoMove,
    /// A session moved one place up, or down, in its folder, as the C# Alt+Up and Alt+Down.
    NudgeProfile {
        /// The session.
        id: ProfileId,
        /// Down, rather than up.
        down: bool,
    },
    /// A session of the restore dialog ticked or not; every one for `None`, its
    /// "Select all".
    RestoreChoose {
        /// The session, by its place; every one for `None`.
        index: Option<usize>,
        /// Ticked.
        chosen: bool,
    },
    /// Show the keyboard shortcuts, unless a dialog is open.
    ShowShortcuts,
    /// Test the gateway dialog's route, signed in with the password and passphrase typed in
    /// it.
    TestRoute {
        /// Password typed, if any.
        password: Option<Secret>,
        /// Key passphrase typed, if any.
        passphrase: Option<Secret>,
    },
    /// Stop the route test running.
    StopRouteTest,
    /// A field of the "Test route" card changed.
    RouteTarget {
        /// Which.
        field: crate::gateway_draft::TargetField,
        /// Its text.
        value: String,
    },
    /// What the route test found no longer holds: a password of the dialog changed.
    ForgetRouteTest,
    /// A step of a route test ended.
    RouteStep {
        /// Which test.
        run: u64,
        /// The step.
        step: heimdall_ssh::Step,
    },
    /// A route test ended.
    RouteTestDone {
        /// Which test.
        run: u64,
    },
    /// Ask the SSH agents again what they hold, for the profile form's chip.
    RefreshAgents,
    /// What the SSH agents answered.
    AgentsSurveyed(Vec<heimdall_ssh::AgentSurvey>),
    /// Test whether the profile form's address answers.
    TestAddress,
    /// Stop the address test running.
    CancelAddressTest,
    /// What an address test found.
    AddressTested {
        /// Which test.
        test: u64,
        /// What it found.
        result: Result<crate::reachability::Reached, crate::reachability::Unreached>,
    },
    /// Save the profile form, with the password and key passphrase typed into it, if any.
    SaveProfile {
        /// The password typed; `None` or empty leaves the saved one as it is.
        password: Option<Secret>,
        /// The key passphrase typed; `None` or empty leaves the saved one as it is.
        passphrase: Option<Secret>,
    },
    /// In the profile form, clear the saved password (done when the form is saved).
    ClearPassword,
    /// In the profile form, clear the saved key passphrase (done when the form is saved).
    ClearPassphrase,
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
    /// In the gateway dialog, clear the saved key passphrase (done when it is saved).
    ClearGatewayPassphrase,
    /// Save the gateway dialog, with the password and key passphrase typed into it, if any.
    SaveGateway {
        /// The password typed; `None` or empty leaves the saved one as it is.
        password: Option<Secret>,
        /// The key passphrase typed; `None` or empty leaves the saved one as it is.
        passphrase: Option<Secret>,
    },
    /// In a session's form, route it through this gateway.
    ChooseGateway(ProfileId),
    /// Open a folder of the tree, or close it.
    ToggleFolder(String),
    /// A change to the tree's filters.
    Filter(FilterMessage),
    /// Something from a folder's menu.
    Folder(FolderMessage),
    /// A profile's Rename or "Move to folder".
    ProfileMenu(ProfileMenuMessage),
    /// Something about the profiles selected together.
    Selection(SelectionMessage),
    /// Select a profile in the tree.
    SelectProfile(ProfileId),
    /// Fold every folder of the tree, or unfold them all, as the C# "Collapse all" and
    /// "Expand all".
    FoldAll(bool),
    /// Select a folder in the tree, by its path, as the C# tree's keyboard reaches folders.
    SelectFolder(String),
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
    VaultOpened(VaultTicket, Result<OpenedVault, VaultProblem>),
    /// What was typed into the bulk password dialog: saved for its profiles when the two
    /// are alike.
    SetBulkPassword {
        /// The password.
        password: Secret,
        /// The password typed again.
        confirm: Secret,
    },
    /// Close the vault.
    LockVault,
    /// Windows Hello unlocking the vault.
    VaultHello(VaultHelloMessage),
    /// How long the computer has had no input, as the window measured it: the workspace
    /// locks once that reaches the idle auto-lock threshold.
    Idle(Duration),
    /// The application PIN.
    Pin(PinMessage),
    /// The external credential provider's settings.
    CredentialProvider(ProviderMessage),
    /// The external credential provider answered a tab's question, or could not.
    CredentialProvided(Box<ProviderAnswer>),
    /// Windows Hello answered [`Effect::VerifyWindowsHello`]: the sessions waiting open, or
    /// why they do not.
    WindowsHello(Result<(), crate::windows_hello::HelloRefusal>),
    /// The Credential Guard check of [`Effect::CheckCredentialGuard`] answered: the embedded
    /// RDP sessions waiting open, or are refused.
    CredentialGuard(crate::credential_guard::Status),
    /// About the migration from the legacy PowerShell Heimdall.
    LegacyMigration(LegacyMigrationMessage),
    /// A change from the Settings page.
    Settings(SettingsMessage),
    /// A step of the terminal macros.
    Macro(MacroMessage),
    /// A note about a server was written and opened in the editor set, or why not.
    NoteOpened(Result<PathBuf, String>),
    /// The settings carried to or from another computer.
    SettingsTransfer(SettingsTransferMessage),
    /// A step of the Settings page's Gateways tab.
    Gateways(GatewaysMessage),
    /// A change of broadcast input.
    Broadcast(BroadcastMessage),
    /// Something about a tab's own window.
    Float(FloatMessage),
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
            Self::OpenRdpWith { id, mode } => write!(f, "OpenRdpWith({id}, {mode:?})"),
            Self::OpenTelnet(id) => write!(f, "OpenTelnet({id})"),
            // The arguments may carry anything: only the program is shown.
            Self::OpenLocal(shell) => write!(f, "OpenLocal({:?})", shell.program),
            Self::OpenLocalProfile(id) => write!(f, "OpenLocalProfile({id})"),
            Self::ElevatedLaunched { tab, outcome } => {
                write!(f, "ElevatedLaunched({}, {outcome:?})", tab.value())
            }
            Self::OpenWinRm(id) => write!(f, "OpenWinRm({id})"),
            Self::Tools(message) => write!(f, "Tools({message:?})"),
            Self::OpenVnc(id) => write!(f, "OpenVnc({id})"),
            Self::OpenFtp(id) => write!(f, "OpenFtp({id})"),
            Self::OpenCitrix(id) => write!(f, "OpenCitrix({id})"),
            Self::CitrixLaunched { tab, result, .. } => {
                write!(f, "CitrixLaunched({}, {})", tab.value(), result.is_ok())
            }
            Self::CitrixTick => f.write_str("CitrixTick"),
            Self::CitrixProbed { tab, probe } => {
                write!(f, "CitrixProbed({}, {probe:?})", tab.value())
            }
            Self::CitrixTerminate { tab, force } => {
                write!(f, "CitrixTerminate({}, {force})", tab.value())
            }
            Self::CitrixTerminated { tab, pid, result } => {
                write!(f, "CitrixTerminated({}, {pid}, {result:?})", tab.value())
            }
            Self::RdpExternalLaunched { result, .. } => {
                write!(f, "RdpExternalLaunched({})", result.is_ok())
            }
            Self::MstscRoute { id, .. } => write!(f, "MstscRoute({})", id.value()),
            Self::PuttyHostKey { profile, .. } => write!(f, "PuttyHostKey({})", profile.id),
            Self::PuttyLaunched { result, .. } => {
                write!(f, "PuttyLaunched({})", result.is_ok())
            }
            Self::PuttyRoute { id, .. } => write!(f, "PuttyRoute({})", id.value()),
            // What was typed is never shown, as for a terminal.
            Self::DesktopResize { tab, width, height } => {
                write!(f, "DesktopResize({}, {width}x{height})", tab.value())
            }
            Self::DesktopShown { tab, width, height } => {
                write!(f, "DesktopShown({}, {width}x{height})", tab.value())
            }
            Self::DesktopInput { tab, inputs } => {
                write!(f, "DesktopInput({}, {} inputs)", tab.value(), inputs.len())
            }
            Self::SendKeys { tab, keys } => write!(f, "SendKeys({}, {keys:?})", tab.value()),
            Self::VncQuality { tab, quality } => {
                write!(f, "VncQuality({}, {quality:?})", tab.value())
            }
            Self::AntiIdleTick => f.write_str("AntiIdleTick"),
            Self::DisplayScale(scale) => write!(f, "DisplayScale({scale})"),
            Self::TmoutResetTick => f.write_str("TmoutResetTick"),
            Self::HealthTick => f.write_str("HealthTick"),
            Self::ReachabilityTick => f.write_str("ReachabilityTick"),
            Self::Update(message) => write!(f, "Update({message:?})"),
            Self::ReachabilityChecked { id, verdict } => {
                write!(f, "ReachabilityChecked({id}, {verdict:?})")
            }
            Self::HealthRead { tab, .. } => write!(f, "HealthRead({})", tab.value()),
            Self::StopAntiIdle(tab) => write!(f, "StopAntiIdle({})", tab.value()),
            Self::StabilizationTick(_) => f.write_str("StabilizationTick"),
            Self::SkipStabilization(tab) => write!(f, "SkipStabilization({})", tab.value()),
            Self::SendClipboard(tab) => write!(f, "SendClipboard({})", tab.value()),
            Self::SaveRemoteFiles(tab) => write!(f, "SaveRemoteFiles({})", tab.value()),
            Self::SaveFolderPicked { tab, folder } => {
                write!(f, "SaveFolderPicked({}, {})", tab.value(), folder.is_some())
            }
            Self::CancelSave(tab) => write!(f, "CancelSave({})", tab.value()),
            Self::ForgetServer(tab) => write!(f, "ForgetServer({})", tab.value()),
            Self::ReconnectTab(tab) => write!(f, "ReconnectTab({})", tab.value()),
            Self::Files(message) => write!(f, "Files({message:?})"),
            Self::Tunnel(message) => write!(f, "Tunnel({message:?})"),
            Self::SelectTab(tab) => write!(f, "SelectTab({})", tab.value()),
            Self::MoveTab { tab, onto } => {
                write!(f, "MoveTab({} onto {})", tab.value(), onto.value())
            }
            Self::RequestCloseTab(tab) => write!(f, "RequestCloseTab({})", tab.value()),
            Self::DisconnectDesktop(tab) => write!(f, "DisconnectDesktop({})", tab.value()),
            Self::TabMenu(message) => write!(f, "TabMenu({message:?})"),
            Self::Split(message) => write!(f, "Split({message:?})"),
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
            Self::CopyHostKeyFingerprint(tab) => {
                write!(f, "CopyHostKeyFingerprint({})", tab.value())
            }
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
            Self::ScreenshotTaken { copied } => write!(f, "ScreenshotTaken({copied})"),
            Self::ClipboardFiles { tab, paths } => {
                write!(f, "ClipboardFiles({}, {})", tab.value(), paths.len())
            }
            Self::ClipboardImage { tab, image } => {
                write!(f, "ClipboardImage({}, {})", tab.value(), image.len())
            }
            Self::SyncDeadline { tab, generation } => {
                write!(f, "SyncDeadline({}, {generation})", tab.value())
            }
            Self::WindowFocus(focused) => write!(f, "WindowFocus({focused})"),
            Self::WindowCloseRequested => f.write_str("WindowCloseRequested"),
            Self::ImportLegacy => f.write_str("ImportLegacy"),
            Self::ImportCitrix => f.write_str("ImportCitrix"),
            // Each application carries its launch line: only counted.
            Self::CitrixScanned(scan) => write!(f, "CitrixScanned({} apps)", scan.apps.len()),
            Self::ExportSessions => f.write_str("ExportSessions"),
            // The file's text is the user's configuration: never shown.
            Self::Sessions(SessionsMessage::Read(_)) => f.write_str("Sessions(Read(..))"),
            Self::Sessions(SessionsMessage::PuttyRead(_)) => f.write_str("Sessions(PuttyRead(..))"),
            Self::Sessions(message) => write!(f, "Sessions({message:?})"),
            // A file's text may name servers and accounts: never shown.
            Self::Rdp(RdpMessage::Read { files, .. }) => {
                write!(f, "Rdp(Read({} files))", files.len())
            }
            Self::Rdp(message) => write!(f, "Rdp({message:?})"),
            Self::ProfileImport(message) => write!(f, "ProfileImport({message:?})"),
            Self::ExportFinished(outcome) => write!(f, "ExportFinished({outcome:?})"),
            Self::NewProfile => f.write_str("NewProfile"),
            Self::EditProfile(id) => write!(f, "EditProfile({id})"),
            Self::ProfileField { field, .. } => write!(f, "ProfileField({field:?}, ..)"),
            Self::DeleteProfile => f.write_str("DeleteProfile"),
            Self::ConfirmDialog => f.write_str("ConfirmDialog"),
            Self::SkipPostConnect => f.write_str("SkipPostConnect"),
            Self::StopPostConnect(tab) => write!(f, "StopPostConnect({})", tab.value()),
            Self::DismissDialog => f.write_str("DismissDialog"),
            Self::DropProfiles { ids, onto } => write!(f, "DropProfiles({}, {onto:?})", ids.len()),
            Self::DropFolder { onto, .. } => write!(f, "DropFolder({onto:?})"),
            Self::UndoMove => f.write_str("UndoMove"),
            Self::NudgeProfile { id, down } => write!(f, "NudgeProfile({}, {down})", id.as_str()),
            Self::RestoreChoose { index, chosen } => {
                write!(f, "RestoreChoose({index:?}, {chosen})")
            }
            Self::ShowShortcuts => f.write_str("ShowShortcuts"),
            Self::TestAddress => f.write_str("TestAddress"),
            Self::TestRoute { .. } => f.write_str("TestRoute(..)"),
            Self::StopRouteTest => f.write_str("StopRouteTest"),
            Self::RouteTarget { field, .. } => write!(f, "RouteTarget({field:?})"),
            Self::ForgetRouteTest => f.write_str("ForgetRouteTest"),
            Self::RouteStep { run, step } => write!(f, "RouteStep({run}, {step:?})"),
            Self::RouteTestDone { run } => write!(f, "RouteTestDone({run})"),
            Self::RefreshAgents => f.write_str("RefreshAgents"),
            Self::AgentsSurveyed(found) => write!(f, "AgentsSurveyed({})", found.len()),
            Self::CancelAddressTest => f.write_str("CancelAddressTest"),
            Self::AddressTested { test, .. } => write!(f, "AddressTested({test})"),
            Self::SaveProfile { .. } => f.write_str("SaveProfile(..)"),
            Self::ClearPassword => f.write_str("ClearPassword"),
            Self::ClearPassphrase => f.write_str("ClearPassphrase"),
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
            Self::ClearGatewayPassphrase => f.write_str("ClearGatewayPassphrase"),
            Self::SaveGateway { .. } => f.write_str("SaveGateway(..)"),
            Self::ChooseGateway(id) => write!(f, "ChooseGateway({id})"),
            Self::ToggleFolder(path) => write!(f, "ToggleFolder({path})"),
            Self::Filter(message) => write!(f, "Filter({message:?})"),
            Self::Folder(message) => write!(f, "Folder({message:?})"),
            Self::ProfileMenu(message) => write!(f, "ProfileMenu({message:?})"),
            Self::Selection(message) => write!(f, "Selection({message:?})"),
            Self::SelectProfile(id) => write!(f, "SelectProfile({id})"),
            Self::FoldAll(fold) => write!(f, "FoldAll({fold})"),
            Self::SelectFolder(path) => write!(f, "SelectFolder({path})"),
            Self::ConnectProfile(id) => write!(f, "ConnectProfile({id})"),
            Self::DuplicateProfile { id, .. } => write!(f, "DuplicateProfile({id})"),
            Self::RequestDeleteProfile(id) => write!(f, "RequestDeleteProfile({id})"),
            Self::CopyProfile { id, what } => write!(f, "CopyProfile({id}, {what:?})"),
            Self::ShowVault => f.write_str("ShowVault"),
            Self::ChangeMasterPassword => f.write_str("ChangeMasterPassword"),
            Self::DisableMasterPassword => f.write_str("DisableMasterPassword"),
            Self::SubmitVault { .. } => f.write_str("SubmitVault(..)"),
            Self::SetBulkPassword { .. } => f.write_str("SetBulkPassword(..)"),
            Self::VaultOpened(_, result) => {
                write!(f, "VaultOpened({:?})", result.as_ref().err())
            }
            Self::LockVault => f.write_str("LockVault"),
            Self::VaultHello(message) => write!(f, "VaultHello({message:?})"),
            Self::Idle(idle) => write!(f, "Idle({idle:?})"),
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
            Self::WindowsHello(answer) => write!(f, "WindowsHello({answer:?})"),
            Self::CredentialGuard(status) => write!(f, "CredentialGuard({status:?})"),
            Self::LegacyMigration(LegacyMigrationMessage::Found(offer)) => {
                write!(f, "LegacyMigration(Found({}))", offer.is_some())
            }
            Self::LegacyMigration(LegacyMigrationMessage::OfferAgain) => {
                f.write_str("LegacyMigration(OfferAgain)")
            }
            Self::Settings(message) => write!(f, "Settings({message:?})"),
            // What a macro types is not logged.
            Self::Macro(MacroMessage::NameEdited(_)) => f.write_str("Macro(NameEdited)"),
            Self::Macro(MacroMessage::Draft(_)) => f.write_str("Macro(Draft)"),
            Self::Macro(message) => write!(f, "Macro({message:?})"),
            Self::NoteOpened(result) => write!(f, "NoteOpened({})", result.is_ok()),
            Self::SettingsTransfer(message) => match message {
                // What the file says is not logged.
                SettingsTransferMessage::Read(result) => {
                    write!(f, "SettingsTransfer(Read({}))", result.is_ok())
                }
                other => write!(f, "SettingsTransfer({other:?})"),
            },
            Self::Gateways(message) => write!(f, "Gateways({message:?})"),
            Self::Broadcast(message) => write!(f, "Broadcast({message:?})"),
            Self::Float(message) => write!(f, "Float({message:?})"),
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
    /// Start an FTP attempt and feed its events back as [`Message::Connection`].
    ConnectFtp {
        /// Tab.
        tab: TabId,
        /// Attempt.
        attempt: AttemptId,
        /// What to connect to.
        request: Box<FtpRequest>,
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
    /// Start a local shell as administrator in a window of its own, off the UI thread, as
    /// [`crate::elevated_shell::launch`] does; answered with [`Message::ElevatedLaunched`].
    LaunchElevated {
        /// The tab that says how it went.
        tab: TabId,
        /// What Windows is asked to start.
        request: Box<crate::elevated_shell::ElevatedLaunch>,
    },
    /// Test a gateway route, and say each step as [`Message::RouteStep`], then
    /// [`Message::RouteTestDone`].
    TestRoute {
        /// Which test.
        run: u64,
        /// What to test.
        request: Box<crate::route_test::RouteTestRequest>,
    },
    /// Ask the SSH agents of `0` what they hold, and say it as [`Message::AgentsSurveyed`].
    SurveyAgents(AgentSource),
    /// Send the Wake-on-LAN magic packet for this card, and say how it went as
    /// [`ProfileMenuMessage::WakeOnLanSent`].
    WakeOnLan(heimdall_core::metadata::MacAddress),
    /// Type a macro into `tab`, then say how it ended as [`MacroMessage::Finished`].
    PlayMacro {
        /// The tab.
        tab: TabId,
        /// The typing.
        run: std::pin::Pin<
            Box<dyn std::future::Future<Output = crate::macro_player::MacroOutcome> + Send>,
        >,
    },
    /// Dial these servers, `at_once` at a time, each with `timeout` to answer, and say each
    /// as [`Message::ReachabilityChecked`].
    CheckReachability {
        /// The servers.
        probes: Vec<crate::reachability::Probe>,
        /// The time each has.
        timeout: std::time::Duration,
        /// How many are dialled at once.
        at_once: usize,
    },
    /// Test whether a profile's address answers, from the tree, and say it as
    /// [`ProfileMenuMessage::Tested`].
    TestReachability {
        /// Address.
        host: String,
        /// Port.
        port: u16,
    },
    /// Test whether an address answers, and say it as [`Message::AddressTested`].
    TestAddress {
        /// Which test.
        test: u64,
        /// Address.
        host: String,
        /// Port.
        port: u16,
        /// Whether to read an SSH server's banner.
        ssh: bool,
        /// Stops it.
        cancel: CancellationToken,
    },
    /// Open a tunnel the user asked for and feed its events back as [`TunnelMessage::Event`].
    OpenTunnel {
        /// Tunnel.
        id: crate::tunnel::TunnelId,
        /// What to open.
        request: Box<crate::tunnel_driver::TunnelRequest>,
    },
    /// Start a `WinRM` session through an SSH gateway and feed its events back as
    /// [`Message::Connection`].
    ConnectWinRm {
        /// Tab.
        tab: TabId,
        /// Attempt.
        attempt: AttemptId,
        /// What to connect to.
        request: Box<WinRmRequest>,
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
    /// Put files of this computer on the system's clipboard as copied files, each by its
    /// full path, as Explorer's Copy does: the local file browser's "Copy". Windows only:
    /// elsewhere they stay with Heimdall.
    WriteFileList(Vec<PathBuf>),
    /// Open this web address in the system's browser: Ctrl+click on one in a terminal.
    OpenUrl(String),
    /// Ask GitHub for the latest release, off the UI thread, and say what it found as
    /// [`UpdateMessage::Checked`].
    CheckForUpdate,
    /// Ask Windows whether Windows Hello can be enrolled for the vault, off the UI thread,
    /// and say it as [`VaultHelloMessage::Checked`].
    CheckVaultHello,
    /// Enrol Windows Hello for the vault, off the UI thread, prompting; then send
    /// [`VaultHelloMessage::Enrolled`].
    EnrolVaultHello {
        /// A copy of the open vault's data key, on the heap so that each move carries only
        /// its pointer, zeroed when dropped.
        data_key: Box<zeroize::Zeroizing<[u8; sealvault::DATA_KEY_LEN]>>,
        /// The envelope enrolled before, whose vault id is kept.
        previous: Option<crate::vault_hello::Envelope>,
    },
    /// Open the vault with Windows Hello, off the UI thread, prompting; then send
    /// [`VaultHelloMessage::Unlocked`].
    UnlockVaultHello {
        /// Vault file.
        path: PathBuf,
        /// What enrolling left.
        envelope: crate::vault_hello::Envelope,
    },
    /// Delete the Windows Hello credential of this name, off the UI thread.
    DeleteVaultHelloCredential(String),
    /// Ask Windows Hello for the user's verification, off the UI thread, and say what it
    /// came to as [`Message::WindowsHello`].
    VerifyWindowsHello,
    /// Find whether Credential Guard runs with this detection, off the UI thread, and say
    /// it as [`Message::CredentialGuard`].
    CheckCredentialGuard(std::sync::Arc<crate::credential_guard::Detector>),
    /// Import the keys of the user's OpenSSH `known_hosts` into Heimdall-rs's own, off the
    /// UI thread, with [`crate::known_hosts_sync::run`]; nothing is answered, as the C#
    /// startup import says what it did in the log only.
    SyncKnownHosts {
        /// The user's OpenSSH `known_hosts`.
        source: PathBuf,
        /// Heimdall-rs's own.
        store: PathBuf,
    },
    /// Look for the legacy PowerShell Heimdall walking up from this folder, the program's,
    /// off the UI thread, with [`crate::rdpmanager::detect`]; answered with
    /// [`LegacyMigrationMessage::Found`].
    FindLegacyInstallation(PathBuf),
    /// Launch a Citrix application outside Heimdall, off the UI thread, the client's
    /// processes listed first; answered with [`Message::CitrixLaunched`].
    LaunchCitrix {
        /// Its status tab.
        tab: TabId,
        /// The profile's name.
        name: String,
        /// How it launches.
        launch: crate::citrix::CitrixLaunch,
    },
    /// Look at a Citrix tab's launcher and, when `lists`, list the client's processes, off
    /// the UI thread; answered with [`Message::CitrixProbed`].
    ProbeCitrix {
        /// The tab.
        tab: TabId,
        /// Its launcher.
        launcher: std::sync::Arc<dyn crate::citrix_session::LauncherWatch>,
        /// Whether the client's processes are listed.
        lists: bool,
    },
    /// Ask a Citrix tab's client to close, or force it, by `taskkill.exe`, off the UI
    /// thread; answered with [`Message::CitrixTerminated`].
    TerminateCitrix {
        /// The tab.
        tab: TabId,
        /// The client process.
        pid: u32,
        /// Whether it is forced.
        force: bool,
    },
    /// Open an RDP profile in Remote Desktop Connection, off the UI thread: its connection
    /// file written and `mstsc.exe` started on it; answered with
    /// [`Message::RdpExternalLaunched`].
    LaunchRdpExternal {
        /// The profile's name.
        name: String,
        /// The RD Gateway that sent it there, when the profile did not ask for it.
        gateway: Option<String>,
        /// The `.rdp` file, as [`crate::rdp_external::rdp_file`] writes it.
        content: String,
    },
    /// Open an RDP profile in Remote Desktop Connection through its SSH gateway, as
    /// [`mstsc_route_events`](crate::mstsc_driver::mstsc_route_events) does; answered with
    /// [`Message::MstscRoute`].
    OpenMstscRoute {
        /// The launch.
        id: crate::mstsc_driver::MstscRouteId,
        /// What it needs.
        request: Box<crate::mstsc_driver::MstscRouteRequest>,
    },
    /// Probe an SSH profile's host key before it opens in `PuTTY`, off the UI thread;
    /// answered with [`Message::PuttyHostKey`].
    ProbePuttyHostKey {
        /// The profile.
        profile: Box<SshProfile>,
        /// What every SSH connection shares, `known_hosts` among it.
        options: Box<ConnectOptions>,
    },
    /// Start `PuTTY`, off the UI thread; answered with [`Message::PuttyLaunched`].
    LaunchPutty {
        /// The profile's name.
        name: String,
        /// How it starts.
        launch: Box<crate::putty::PuttyLaunch>,
    },
    /// Start `PuTTY` through a gateway, as
    /// [`putty_route_events`](crate::putty_driver::putty_route_events) does; answered with
    /// [`Message::PuttyRoute`].
    OpenPuttyRoute {
        /// The launch.
        id: crate::putty_driver::PuttyRouteId,
        /// What it needs.
        request: Box<crate::putty_driver::PuttyRouteRequest>,
    },
    /// Put an image on the clipboard, a device-independent bitmap: what an RDP server
    /// copied.
    WriteClipboardImage(std::sync::Arc<[u8]>),
    /// Ask which OpenSSH configuration to import, as the C# open dialog, then read it;
    /// answered with [`SessionsMessage::Read`], or nothing when no file is picked.
    PickOpenSshConfig,
    /// Read `PuTTY`'s saved sessions: the registry on Windows, `~/.putty/sessions`
    /// elsewhere; answered with [`SessionsMessage::PuttyRead`].
    ReadPuttySessions,
    /// Scan Citrix Workspace's local cache, off the window's thread; answered with
    /// [`Message::CitrixScanned`].
    ScanCitrixCache,
    /// Ask which `.rdp` files to import, then read them; answered with
    /// [`RdpMessage::Read`], or nothing when none is picked.
    PickRdpFiles,
    /// Ask which file "Import Sessions" imports, then read it; answered with
    /// [`SessionsMessage::FileRead`], or nothing when none is picked.
    PickSessionsFile,
    /// Ask where to save the settings file, then write `document` there; answered with
    /// [`SettingsTransferMessage::Written`], or nothing when no file is picked.
    SaveSettingsFile {
        /// The settings file.
        document: String,
    },
    /// Ask which settings file to import, then read it; answered with
    /// [`SettingsTransferMessage::Read`], or nothing when none is picked.
    PickSettingsFile,
    /// Every setting was put back to its default: what the Settings page holds typed and
    /// not applied yet goes with the values it was typed over.
    SettingsReset,
    /// Ask which `known_hosts` file to import, then read it; answered with
    /// [`HostKeysMessage::Read`], or nothing when none is picked.
    PickKnownHosts,
    /// Read these `.rdp` files, dropped on the window; answered with [`RdpMessage::Read`].
    ReadRdpFiles(Vec<PathBuf>),
    /// Ask where to save the exported sessions, as the C# save dialog, then write them
    /// there; answered with [`Message::ExportFinished`].
    SaveExport {
        /// The C# session document.
        document: String,
        /// Profiles it holds.
        count: usize,
    },
    /// Read the clipboard, then send [`Message::ClipboardText`] for `tab`.
    ReadClipboard {
        /// Tab the paste is for.
        tab: TabId,
    },
    /// Read the clipboard for the desktop of `tab`, which shares it: the files copied in
    /// Explorer, sent as [`Message::ClipboardFiles`], or else its text, sent as
    /// [`Message::ClipboardText`], or else its image, sent as [`Message::ClipboardImage`].
    ReadDesktopClipboard {
        /// Tab.
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
    /// Carry out the entry of a delete or a change of permissions being worked on, then send
    /// [`FilesMessage::BatchStepDone`].
    FileBatchStep {
        /// Tab.
        tab: TabId,
        /// Pane.
        side: Side,
        /// What to do.
        operation: Box<FileOperation>,
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
    /// Move entries of the server one after another, then send [`FilesMessage::Moved`].
    MoveRemote {
        /// Tab.
        tab: TabId,
        /// Session.
        client: heimdall_files::RemoteSession,
        /// Each entry and its new path.
        moves: Vec<(heimdall_files::RemotePath, heimdall_files::RemotePath)>,
    },
    /// Read the files copied in Explorer, then send [`FilesMessage::ExplorerFilesRead`].
    ReadExplorerFiles {
        /// Tab.
        tab: TabId,
    },
    /// Ask the user for a folder to save the RDP server's files into, then send
    /// [`Message::SaveFolderPicked`].
    PickSaveFolder {
        /// Tab.
        tab: TabId,
    },
    /// Ask the user for files of this computer to upload, then send
    /// [`FilesMessage::UploadPicked`].
    PickUploads {
        /// Tab.
        tab: TabId,
    },
    /// Ask the server of a shell tab how it is, then send [`Message::HealthRead`].
    ReadHealth {
        /// Tab.
        tab: TabId,
        /// Its session's connection.
        connection: heimdall_ssh::Connection,
    },
    /// Read a server's file for the integrated editor, then send
    /// [`FilesMessage::EditorOpened`]; its text goes to the window.
    OpenEditor {
        /// Tab.
        tab: TabId,
        /// The editor.
        id: crate::ids::EditorId,
        /// Session.
        client: heimdall_files::RemoteSession,
        /// The server's file.
        remote: heimdall_files::RemotePath,
    },
    /// Save the integrated editor's text, then send [`FilesMessage::EditorSaved`].
    SaveEditor {
        /// Tab.
        tab: TabId,
        /// The editor.
        id: crate::ids::EditorId,
        /// Session.
        client: heimdall_files::RemoteSession,
        /// The server's file.
        remote: heimdall_files::RemotePath,
        /// The text, stored as the file was.
        bytes: Vec<u8>,
        /// The server's file it may replace; `None` to write over whatever is there.
        expected: Option<heimdall_files::Fingerprint>,
    },
    /// Copy the server's file into a folder of the user's own and start the editor on it,
    /// then send [`FilesMessage::EditStarted`].
    StartEdit {
        /// Tab.
        tab: TabId,
        /// Session.
        client: heimdall_files::RemoteSession,
        /// The server's file.
        remote: heimdall_files::RemotePath,
        /// The editor.
        editor: crate::external_edit::Editor,
        /// The folder edits go under.
        base: PathBuf,
        /// The folders of the edits open, kept when old ones are removed.
        keep: Vec<PathBuf>,
        /// Stops the copy.
        cancel: tokio_util::sync::CancellationToken,
    },
    /// Start the editor again on a file being edited, then send
    /// [`FilesMessage::EditorLaunched`].
    LaunchEditor {
        /// Tab.
        tab: TabId,
        /// The editor.
        editor: crate::external_edit::Editor,
        /// The local copy.
        file: PathBuf,
    },
    /// Send an edit's refused save over the server's file as it is now, then send
    /// [`FilesMessage::EditSentAnyway`].
    SendEditAnyway {
        /// Tab.
        tab: TabId,
        /// Session.
        client: heimdall_files::RemoteSession,
        /// The edit.
        edit: Box<crate::external_edit::EditSession>,
    },
    /// Open a folder in the system's file manager; a failure sends
    /// [`FilesMessage::EditorLaunched`].
    OpenFolder {
        /// Tab.
        tab: TabId,
        /// The folder.
        folder: PathBuf,
    },
    /// Open a file of this computer with the system's default program, as
    /// [`crate::external_edit::open_with_default`] does; a failure sends
    /// [`FilesMessage::EditorLaunched`] with [`crate::files::FilesError::OpenFailed`].
    OpenLocalFile {
        /// The local file browser's tab.
        tab: TabId,
        /// The file, an absolute path.
        file: PathBuf,
    },
    /// Show the system's "Open with" chooser for a file of this computer, as
    /// [`crate::external_edit::open_with_chooser`] does; a failure sends
    /// [`FilesMessage::EditorLaunched`] with [`crate::files::FilesError::OpenFailed`].
    OpenWithChooser {
        /// The local file browser's tab.
        tab: TabId,
        /// The file, an absolute path.
        file: PathBuf,
    },
    /// Look at the files being edited, send their saves, then send
    /// [`FilesMessage::EditsChecked`].
    CheckEdits {
        /// Tab.
        tab: TabId,
        /// Session.
        client: heimdall_files::RemoteSession,
        /// The SSH connection, for the saves sent with sudo.
        shell: Option<heimdall_ssh::Connection>,
        /// The password sudo took for the tab.
        password: Option<crate::sudo_edit::SudoPassword>,
        /// The files, as they were.
        edits: Vec<crate::external_edit::EditSession>,
    },
    /// Open the server's file with sudo, then send [`FilesMessage::SudoOpened`].
    SudoOpen {
        /// Tab.
        tab: TabId,
        /// The SSH connection.
        shell: heimdall_ssh::Connection,
        /// The server's file.
        remote: heimdall_files::RemotePath,
        /// The editor.
        editor: crate::external_edit::Editor,
        /// The folder edits go under, and those kept.
        folders: (PathBuf, Vec<PathBuf>),
        /// The password sudo took for the tab.
        password: Option<crate::sudo_edit::SudoPassword>,
    },
    /// Send an edit's save with sudo, then send [`FilesMessage::SudoSaved`].
    SudoSave {
        /// Tab.
        tab: TabId,
        /// The SSH connection.
        shell: heimdall_ssh::Connection,
        /// The edit.
        edit: Box<crate::external_edit::EditSession>,
        /// The password sudo took for the tab.
        password: Option<crate::sudo_edit::SudoPassword>,
    },
    /// List a remote folder as root, the tab's sudo mode being on, then send
    /// [`FilesMessage::SudoListed`].
    SudoListRemote {
        /// Tab.
        tab: TabId,
        /// The SSH connection.
        shell: heimdall_ssh::Connection,
        /// Folder.
        path: heimdall_files::RemotePath,
        /// The password sudo took for the tab.
        password: Option<crate::sudo_edit::SudoPassword>,
    },
    /// Copy entries of the server on the server, one after another, then send
    /// [`FilesMessage::Copied`].
    CopyRemote {
        /// Tab.
        tab: TabId,
        /// Session.
        client: heimdall_files::RemoteSession,
        /// The SSH connection the copies run on.
        shell: heimdall_ssh::Connection,
        /// What to copy.
        sources: Vec<crate::files::CopySource>,
        /// Into this folder.
        folder: heimdall_files::RemotePath,
        /// Stops the copy running.
        cancel: tokio_util::sync::CancellationToken,
        /// A duplicate, rather than a paste.
        duplicate: bool,
    },
    /// Copy entries of another server's tab into a folder of this one, through this
    /// computer, then send [`FilesMessage::Copied`].
    CopyAcross {
        /// Tab pasted in.
        tab: TabId,
        /// The session they are read from.
        from: heimdall_files::RemoteSession,
        /// The session they are written to.
        to: heimdall_files::RemoteSession,
        /// What to copy.
        sources: Vec<crate::files::CopySource>,
        /// Into this folder.
        folder: heimdall_files::RemotePath,
        /// Where a file is kept on this computer between the two.
        staging: PathBuf,
        /// Stops the copy running.
        cancel: tokio_util::sync::CancellationToken,
    },
    /// Plan a transfer whole, then send [`FilesMessage::Planned`].
    PlanTransfer {
        /// Tab.
        tab: TabId,
        /// What to plan.
        request: Box<crate::files::PlanRequest>,
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
        /// The try it answers, handed back with the result.
        ticket: VaultTicket,
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
    /// Open a window of its own for the tab detached to `0`, and give it the focus.
    OpenWindow(crate::ids::FloatId),
    /// Close the window `0`: its tab went back on the strip, or is gone.
    CloseWindow(crate::ids::FloatId),
    /// Give the window `0` the focus, restored when minimized: its tab was asked for.
    FocusWindow(crate::ids::FloatId),
    /// Give the main window the focus, restored when minimized: a tab came back to it.
    FocusMainWindow,
    /// Wake the core at `deadline` with [`Message::AutoReconnect`].
    RetryAt {
        /// Tab.
        tab: TabId,
        /// The attempt that failed.
        attempt: AttemptId,
        /// When.
        deadline: Instant,
    },
    /// Quit the application.
    Exit,
}

impl fmt::Debug for Effect {
    #[expect(clippy::too_many_lines, reason = "one arm per effect")]
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
            Self::ConnectFtp { tab, attempt, .. } => {
                write!(f, "ConnectFtp({}, {})", tab.value(), attempt.value())
            }
            Self::ConnectVnc { tab, attempt, .. } => {
                write!(f, "ConnectVnc({}, {})", tab.value(), attempt.value())
            }
            Self::ConnectLocal { tab, attempt, .. } => {
                write!(f, "ConnectLocal({}, {})", tab.value(), attempt.value())
            }
            Self::LaunchElevated { tab, .. } => write!(f, "LaunchElevated({})", tab.value()),
            Self::OpenTunnel { id, .. } => write!(f, "OpenTunnel({})", id.value()),
            Self::TestAddress { test, .. } => write!(f, "TestAddress({test})"),
            Self::TestRoute { run, .. } => write!(f, "TestRoute({run})"),
            Self::TestReachability { port, .. } => write!(f, "TestReachability(port {port})"),
            Self::WakeOnLan(_) => f.write_str("WakeOnLan"),
            Self::PlayMacro { tab, .. } => write!(f, "PlayMacro({})", tab.value()),
            Self::CheckReachability { probes, .. } => {
                write!(f, "CheckReachability({})", probes.len())
            }
            Self::SurveyAgents(_) => f.write_str("SurveyAgents"),
            Self::ConnectWinRm { tab, attempt, .. } => {
                write!(f, "ConnectWinRm({}, {})", tab.value(), attempt.value())
            }
            Self::Answer { question, answer } => {
                write!(f, "Answer({}, {answer:?})", question.value())
            }
            Self::WriteClipboard(_) => f.write_str("WriteClipboard(..)"),
            Self::WriteFileList(paths) => write!(f, "WriteFileList({})", paths.len()),
            Self::OpenUrl(_) => f.write_str("OpenUrl(..)"),
            Self::CheckForUpdate => f.write_str("CheckForUpdate"),
            Self::CheckVaultHello => f.write_str("CheckVaultHello"),
            // Never the data key.
            Self::EnrolVaultHello { previous, .. } => {
                write!(f, "EnrolVaultHello(again: {})", previous.is_some())
            }
            Self::UnlockVaultHello { envelope, .. } => {
                write!(f, "UnlockVaultHello({envelope:?})")
            }
            Self::DeleteVaultHelloCredential(name) => {
                write!(f, "DeleteVaultHelloCredential({name})")
            }
            Self::VerifyWindowsHello => f.write_str("VerifyWindowsHello"),
            Self::CheckCredentialGuard(_) => f.write_str("CheckCredentialGuard"),
            Self::SyncKnownHosts { .. } => f.write_str("SyncKnownHosts(..)"),
            Self::FindLegacyInstallation(_) => f.write_str("FindLegacyInstallation(..)"),
            Self::LaunchCitrix { tab, .. } => write!(f, "LaunchCitrix({}, ..)", tab.value()),
            Self::ProbeCitrix { tab, lists, .. } => {
                write!(f, "ProbeCitrix({}, {lists})", tab.value())
            }
            Self::TerminateCitrix { tab, pid, force } => {
                write!(f, "TerminateCitrix({}, {pid}, {force})", tab.value())
            }
            Self::LaunchRdpExternal { .. } => f.write_str("LaunchRdpExternal(..)"),
            Self::OpenMstscRoute { id, .. } => write!(f, "OpenMstscRoute({})", id.value()),
            Self::ProbePuttyHostKey { profile, .. } => {
                write!(f, "ProbePuttyHostKey({})", profile.id)
            }
            Self::LaunchPutty { .. } => f.write_str("LaunchPutty(..)"),
            Self::OpenPuttyRoute { id, .. } => write!(f, "OpenPuttyRoute({})", id.value()),
            Self::WriteClipboardImage(image) => write!(f, "WriteClipboardImage({})", image.len()),
            Self::SaveExport { count, .. } => write!(f, "SaveExport({count})"),
            Self::PickOpenSshConfig => f.write_str("PickOpenSshConfig"),
            Self::ReadPuttySessions => f.write_str("ReadPuttySessions"),
            Self::ScanCitrixCache => f.write_str("ScanCitrixCache"),
            Self::PickRdpFiles => f.write_str("PickRdpFiles"),
            Self::PickSessionsFile => f.write_str("PickSessionsFile"),
            Self::SaveSettingsFile { .. } => f.write_str("SaveSettingsFile"),
            Self::PickSettingsFile => f.write_str("PickSettingsFile"),
            Self::SettingsReset => f.write_str("SettingsReset"),
            Self::PickKnownHosts => f.write_str("PickKnownHosts"),
            Self::ReadRdpFiles(paths) => write!(f, "ReadRdpFiles({})", paths.len()),
            Self::ReadClipboard { tab } => write!(f, "ReadClipboard({})", tab.value()),
            Self::ReadDesktopClipboard { tab } => {
                write!(f, "ReadDesktopClipboard({})", tab.value())
            }
            Self::WakeAt {
                tab, generation, ..
            } => write!(f, "WakeAt({}, {generation})", tab.value()),
            Self::ListRemote { tab, path, .. } => {
                write!(f, "ListRemote({}, {path:?})", tab.value())
            }
            Self::ListLocal { tab, path } => write!(f, "ListLocal({}, {path:?})", tab.value()),
            Self::FileBatchStep { tab, side, .. } => {
                write!(f, "FileBatchStep({}, {side:?})", tab.value())
            }
            Self::FileOperation { tab, side, .. } => {
                write!(f, "FileOperation({}, {side:?})", tab.value())
            }
            Self::PickUploads { tab } => write!(f, "PickUploads({})", tab.value()),
            Self::PickSaveFolder { tab } => write!(f, "PickSaveFolder({})", tab.value()),
            Self::ReadExplorerFiles { tab } => write!(f, "ReadExplorerFiles({})", tab.value()),
            Self::StartEdit { tab, remote, .. } => {
                write!(f, "StartEdit({}, {remote:?})", tab.value())
            }
            Self::ReadHealth { tab, .. } => write!(f, "ReadHealth({})", tab.value()),
            Self::OpenEditor { tab, remote, .. } => {
                write!(f, "OpenEditor({}, {remote:?})", tab.value())
            }
            Self::SaveEditor {
                tab,
                bytes,
                expected,
                ..
            } => write!(
                f,
                "SaveEditor({}, {} bytes, {})",
                tab.value(),
                bytes.len(),
                expected.is_some()
            ),
            Self::LaunchEditor { tab, .. } => write!(f, "LaunchEditor({})", tab.value()),
            Self::SendEditAnyway { tab, .. } => write!(f, "SendEditAnyway({})", tab.value()),
            Self::OpenFolder { tab, .. } => write!(f, "OpenFolder({})", tab.value()),
            Self::OpenLocalFile { tab, .. } => write!(f, "OpenLocalFile({})", tab.value()),
            Self::OpenWithChooser { tab, .. } => write!(f, "OpenWithChooser({})", tab.value()),
            Self::CheckEdits { tab, edits, .. } => {
                write!(f, "CheckEdits({}, {})", tab.value(), edits.len())
            }
            Self::SudoOpen { tab, remote, .. } => {
                write!(f, "SudoOpen({}, {remote:?})", tab.value())
            }
            Self::SudoSave { tab, .. } => write!(f, "SudoSave({})", tab.value()),
            Self::SudoListRemote { tab, path, .. } => {
                write!(f, "SudoListRemote({}, {path:?})", tab.value())
            }
            Self::CopyRemote { tab, sources, .. } => {
                write!(f, "CopyRemote({}, {})", tab.value(), sources.len())
            }
            Self::CopyAcross { tab, sources, .. } => {
                write!(f, "CopyAcross({}, {})", tab.value(), sources.len())
            }
            Self::MoveRemote { tab, moves, .. } => {
                write!(f, "MoveRemote({}, {})", tab.value(), moves.len())
            }
            Self::PlanTransfer { tab, request } => write!(
                f,
                "PlanTransfer({}, {:?}, {})",
                tab.value(),
                request.direction,
                request.roots.len()
            ),
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
            Self::OpenWindow(key) => write!(f, "OpenWindow({})", key.value()),
            Self::CloseWindow(key) => write!(f, "CloseWindow({})", key.value()),
            Self::FocusWindow(key) => write!(f, "FocusWindow({})", key.value()),
            Self::FocusMainWindow => f.write_str("FocusMainWindow"),
            Self::Exit => f.write_str("Exit"),
        }
    }
}

/// Reads this side's clipboard for `tab` when it is a desktop sharing the clipboard.
fn clipboard_offer(tab: &Tab) -> Vec<Effect> {
    if tab
        .desktop
        .as_deref()
        // Not while the server's files are saved: offering would take its clipboard back.
        .is_some_and(|pane| pane.shares_clipboard() && pane.save_state().is_none())
    {
        vec![Effect::ReadDesktopClipboard { tab: tab.id }]
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

/// How writing the exported sessions went.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExportOutcome {
    /// Written: this many profiles.
    Saved(usize),
    /// The save dialog was closed without a file.
    Cancelled,
    /// The file could not be written.
    Failed(String),
}

/// One tab.
#[expect(
    clippy::struct_excessive_bools,
    reason = "a tab's independent states: bell, search, input, pinned"
)]
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
    pub end_reason: Option<heimdall_rdp::Ending>,
    /// How far an RDP connection on its way has come, as it last said.
    pub rdp_step: Option<heimdall_rdp::Step>,
    /// The session waiting to open again by itself, after it dropped.
    pub retry: Option<Retry>,
    /// The desktop size the user chose from the tab's "Resolution" menu, kept for the
    /// session's reconnections; `None`, as its profile says.
    pub(crate) desktop_sizing: Option<heimdall_core::profile::DesktopSizing>,
    /// The proportions chosen under "Match window", kept for the session's reconnections.
    pub(crate) desktop_aspect: crate::desktop::Aspect,
    /// The desktop size the session connected again for, the server unable to take it
    /// live: asked at the next connection, then kept so the same refusal never loops.
    pub(crate) resize_fallback: Option<ResizeFallback>,
    /// When the user's input last reached the session: a TMOUT reset waits for an idle shell.
    last_input: std::sync::Mutex<Option<Instant>>,
    /// A `WinRM` session's first output, read for what it says went wrong.
    early_output: Option<EarlyOutput>,
    /// What a `WinRM` session's first output said went wrong, as the C# says it.
    pub winrm_diagnostic: Option<Diagnostic>,
    /// The attempt a `WinRM` session was given its stored password in: that session never
    /// entered, the password is taken as refused.
    winrm_password_given: Option<AttemptId>,
    /// The last search in its history found nothing.
    pub find_missed: bool,
    /// Where the last search's match is among all of them, as the C# bar counts them.
    pub find_found: Option<heimdall_term::Found>,
    /// The transcript it keeps, while it keeps one.
    pub transcript: Option<Transcript>,
    /// Its server health panel, for an SSH shell.
    pub health: crate::server_health::HealthPane,
    /// Connection state.
    pub phase: Phase,
    /// Its session failed once open: what it showed stays in sight. See [`Tab::dropped`].
    dropped: bool,
    /// When its session last connected, for how long it lasted.
    connected_at: Option<Instant>,
    /// How long its session was connected before it failed. See [`Tab::session_lasted`].
    lasted: Option<Duration>,
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
    /// The launch and its client, for a Citrix tab.
    pub citrix: Option<Box<crate::citrix_session::CitrixPane>>,
    /// What the certificate question says beside the fingerprint, while it is asked.
    pub certificate_context: Option<CertificateContext>,
    /// The key and the whole certificate the certificate question asks about, while it is
    /// asked.
    pending_rdp_key: Option<(heimdall_rdp::Fingerprint, heimdall_rdp::CertificateHash)>,
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
    /// Whether the tunnels panel was opened or closed while this tab was shown, as the C#
    /// `TunnelsPanelManualOverride`: it wins over its profile's choice.
    pub(crate) tunnels_panel: Option<bool>,
    /// When it opened: a session through a gateway lists it as its route's start.
    pub(crate) opened: std::time::SystemTime,
    /// Pinned, as the C# tab: before every tab not pinned, and left by "Close others" and
    /// "Close to the right".
    pub pinned: bool,
    /// The post-connect step running, while the sequence runs.
    pub post_connect: Option<PostConnectProgress>,
    /// The macro being recorded from what is typed into it.
    pub macro_recording: Option<MacroRecording>,
    /// The macro being typed into it.
    pub macro_playing: Option<MacroPlaying>,
    /// How the tab is split, while it shows other tabs' sessions beside its own; only a tab
    /// of the strip is.
    pub layout: Option<split::Layout>,
    /// The working folder its shell last reported (OSC 7), an absolute path, the host left
    /// aside, or an absolute Windows path (OSC 9;9), never starting with `/`: the server's
    /// word, untrusted. Kept for any terminal tab, SSH or local, for the
    /// panes that follow the shell; none until the shell reports one, and none after a
    /// reconnect, which opens a tab of its own.
    pub working_directory: Option<String>,
    /// The user closed the file browser docked beside this local shell, or took it out of
    /// the split: none is docked again when the shell starts again. Carried across a
    /// reconnect; a shell opened anew docks one as usual.
    pub local_browser_closed: bool,
    /// A local shell run as administrator in a window of its own: what the tab shows in
    /// place of a terminal.
    pub elevated: Option<ElevatedPane>,
    /// The RDP mode the profile menu's "Connect with" chose for this session alone, as the
    /// C# tab's `RdpModeOverride`: its title says so, and Reconnect keeps it
    /// (`SessionCoordinator.cs:1014-1027`, 1481); `None`, as its profile says.
    pub rdp_mode_override: Option<heimdall_core::profile::RdpMode>,
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
    /// Whether the question the tab asks is about a server's own certificate, an RDP, FTPS
    /// or VNC one, not an SSH key on the way to it.
    #[must_use]
    pub fn asks_about_certificate(&self) -> bool {
        (matches!(self.purpose, Purpose::Rdp | Purpose::Vnc)
            || matches!(self.profile, TabProfile::Ftp(_)))
            && self.pending_rdp_key.is_some()
    }

    /// What the tab is called: the name the user gave it, else its title.
    #[must_use]
    pub fn display_title(&self) -> &str {
        self.custom_title.as_deref().unwrap_or(&self.title)
    }

    /// The resolution mode an RDP tab's desktop is in, as its "Resolution" menu names it:
    /// a size chosen for the session, else its profile's mode.
    #[must_use]
    pub fn resolution_mode(&self) -> Option<heimdall_core::profile::Resolution> {
        use heimdall_core::profile::{DesktopSizing, Resolution};
        let TabProfile::Rdp(profile) = &self.profile else {
            return None;
        };
        Some(match self.desktop_sizing {
            Some(DesktopSizing::Fixed { .. }) => Resolution::Fixed,
            Some(DesktopSizing::FollowsTab | DesktopSizing::TabSizeOnce) => Resolution::FitWindow,
            None => profile.options.resolution,
        })
    }

    /// The algorithm of the key the tab asks about, as the C# prompt's "Algorithm" row.
    #[must_use]
    pub fn host_key_algorithm(&self) -> Option<String> {
        self.pending_host_key
            .as_ref()
            .map(|key| key.algorithm().to_string())
    }

    /// Whether the tab's session failed after it was open, a connection lost under it: its
    /// terminal or its listing stays in sight, read-only, as the C# "View output" leaves the
    /// output readable after a drop. A session that never opened has nothing to show.
    #[must_use]
    pub fn dropped(&self) -> bool {
        self.dropped && matches!(self.phase, Phase::Failed(_))
    }

    /// How long the tab's failed session had been connected when it failed, as the C# RDP
    /// error report's "Session" line says it; `None` unless it failed after it connected.
    #[must_use]
    pub fn session_lasted(&self) -> Option<Duration> {
        self.lasted
            .filter(|_| matches!(self.phase, Phase::Failed(_)))
    }

    /// Times the session from `was_connected`, its state before the event just applied, at
    /// `now`: when it connects, and how long it lasted when it fails after that; a failure
    /// before it connected lasted nothing.
    fn time_session(&mut self, was_connected: bool, now: Instant) {
        match (&self.phase, was_connected) {
            (Phase::Connected, false) => {
                self.connected_at = Some(now);
                self.lasted = None;
            }
            (Phase::Failed(_), true) => {
                self.lasted = self
                    .connected_at
                    .map(|at| now.saturating_duration_since(at));
            }
            (Phase::Failed(_), false) => self.lasted = None,
            _ => {}
        }
    }

    /// Whether a live session would be lost by closing the tab. An attempt still
    /// connecting has nothing to lose: closing it cancels it without asking.
    #[must_use]
    pub fn is_live(&self) -> bool {
        // The local file browser is no session: nothing is lost when it closes. Nor is a
        // Citrix tab's: closing it leaves the Citrix session as it is. Nor a tool's.
        self.phase == Phase::Connected
            && !self.is_local_browser()
            && self.citrix.is_none()
            && self.tool().is_none()
    }

    fn write(&self, bytes: Vec<u8>) {
        if let Some(sink) = &self.sink
            && !bytes.is_empty()
        {
            if let Ok(mut last) = self.last_input.lock() {
                *last = Some(Instant::now());
            }
            // A closed session reports itself through its event stream.
            let _ = sink.write(bytes);
        }
    }

    /// Whether no input of the user's reached the session for `interval`.
    fn idle_for(&self, interval: Duration) -> bool {
        self.last_input
            .lock()
            .map_or(true, |last| last.is_none_or(|at| at.elapsed() >= interval))
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
            rdp_step: None,
            retry: None,
            desktop_sizing: None,
            desktop_aspect: match &profile {
                TabProfile::Rdp(rdp) => rdp.options.aspect,
                _ => crate::desktop::Aspect::Stretch,
            },
            resize_fallback: None,
            last_input: std::sync::Mutex::new(None),
            early_output: None,
            winrm_diagnostic: None,
            winrm_password_given: None,
            find_missed: false,
            find_found: None,
            transcript: None,
            health: crate::server_health::HealthPane::default(),
            pinned: false,
            tunnels_panel: None,
            opened: std::time::SystemTime::now(),
            reopen: reconnect::Reopen::of(&profile),
            post_connect: None,
            macro_recording: None,
            macro_playing: None,
            layout: None,
            working_directory: None,
            local_browser_closed: false,
            elevated: None,
            rdp_mode_override: None,
            dropped: false,
            connected_at: None,
            lasted: None,
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
            citrix: None,
            certificate_context: None,
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

    /// Whether the tab's integrated editor holds text not saved, or being saved.
    #[must_use]
    pub fn holds_unsaved_text(&self) -> bool {
        self.files
            .as_ref()
            .and_then(|files| files.editor.as_ref())
            .is_some_and(|edit| edit.dirty || edit.saving)
    }

    fn stop(&mut self) {
        self.cancel.cancel();
        self.macro_recording = None;
        if let Some(playing) = self.macro_playing.take() {
            playing.stop();
        }
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

/// What a server certificate question says beside the fingerprint, as the C# one.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CertificateContext {
    /// Certificates this profile already trusts at the same address: several usually mean
    /// several machines answer to the name.
    pub others: usize,
    /// The gateways the tab reaches the server through, nearest to this machine first.
    pub route: Vec<String>,
    /// The certificate's subject, as the C# prompt shows it, when it was read.
    pub subject: Option<String>,
    /// Its issuer, validity and validation issue, as the C# FTPS prompt shows them, for an
    /// FTPS or VNC server; for an RDP server only when its certificate is renewed.
    pub details: Option<crate::event::CertificateDetails>,
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
    /// A Files tab on an FTP server.
    Ftp(FtpProfile),
    /// A local shell tab.
    Local(LocalShell),
    /// A `WinRM` session: a local `PowerShell` entering it, directly or through an SSH
    /// gateway.
    WinRm(WinRmProfile),
    /// A Citrix application launched outside Heimdall: its status alone.
    Citrix(heimdall_core::profile::CitrixProfile),
    /// A built-in tool: nothing connects.
    Tool(crate::tools::ToolId),
}

impl TabProfile {
    /// The SSH gateway its connection goes through, when it goes through one.
    #[must_use]
    pub fn gateway(&self) -> Option<&ProfileId> {
        match self {
            Self::Ssh(profile) => profile.gateway.as_ref(),
            Self::Rdp(profile) => profile.gateway.as_ref(),
            Self::WinRm(profile) => profile.gateway.as_ref(),
            Self::Telnet(_)
            | Self::Vnc(_)
            | Self::Ftp(_)
            | Self::Local(_)
            | Self::Citrix(_)
            | Self::Tool(_) => None,
        }
    }

    /// The protocol it connects with; a local shell's, `WinRM` included, is local.
    #[must_use]
    pub fn kind(&self) -> ProfileKind {
        match self {
            Self::Ssh(profile) if profile.sftp => ProfileKind::Sftp,
            Self::Ssh(_) => ProfileKind::Ssh,
            Self::Rdp(_) => ProfileKind::Rdp,
            Self::Telnet(_) => ProfileKind::Telnet,
            Self::Vnc(_) => ProfileKind::Vnc,
            Self::Ftp(_) => ProfileKind::Ftp,
            // A tool runs on this computer, as a local shell: no protocol of its own.
            Self::Local(_) | Self::Tool(_) => ProfileKind::Local,
            Self::WinRm(_) => ProfileKind::WinRm,
            Self::Citrix(_) => ProfileKind::Citrix,
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
            Self::Ftp(profile) => &profile.name,
            Self::Local(shell) => &shell.name,
            Self::WinRm(profile) => &profile.name,
            Self::Citrix(profile) => &profile.name,
            // Its tab is named in the language shown when it opens.
            Self::Tool(tool) => tool.code(),
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
            Self::Ftp(profile) => Some((&profile.host, profile.port)),
            // A Citrix application's server is its StoreFront's to choose.
            Self::Local(_) | Self::Citrix(_) | Self::Tool(_) => None,
            Self::WinRm(profile) => Some((&profile.host, profile.port)),
        }
    }

    /// Account, when the profile names one.
    #[must_use]
    pub fn username(&self) -> Option<&str> {
        match self {
            Self::Ssh(profile) => profile.username.as_deref(),
            Self::Rdp(profile) => profile.username.as_deref(),
            Self::Ftp(profile) => profile.username.as_deref(),
            Self::WinRm(profile) => profile.username.as_deref(),
            // Telnet asks for its account in the session; VNC has none; a local shell runs
            // as the user running Heimdall; Citrix Workspace signs in by itself.
            Self::Telnet(_) | Self::Vnc(_) | Self::Local(_) | Self::Citrix(_) | Self::Tool(_) => {
                None
            }
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
    /// What the file said as a whole.
    pub warnings: Vec<FileWarning>,
    /// For a `MobaXterm` file, the passwords it stores, which must be entered again.
    pub stored_credentials: Option<usize>,
    /// Profiles imported without settings Heimdall-rs does not have: the name, made safe,
    /// and what was left out.
    pub dropped: Vec<(String, Vec<heimdall_core::import::csharp::Dropped>)>,
    /// For the migration of this computer's C# store, the trusted SSH servers carried over;
    /// or why they could not be.
    pub host_keys: Option<Result<heimdall_ssh::Carried, String>>,
    /// The SSH gateways the file brought, created or found already saved, as the C# summary's
    /// gateway line counts them.
    pub gateways: heimdall_core::import::gateways::Reconciliation,
    /// For a Heimdall session document previewed, what each choice did, as the C# summary
    /// counts it.
    pub actions: Option<ImportActions>,
}

/// One destination in a transfer's way, and the answer picked for it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConflictRow {
    /// The planned entry it is.
    pub index: usize,
    /// Its path below the destination folder, made safe.
    pub target: String,
    /// A folder: skipping it skips everything planned inside.
    pub folder: bool,
    /// The answers it allows.
    pub allowed: heimdall_files::conflict::Allowed,
    /// The answer picked.
    pub choice: heimdall_files::conflict::Choice,
    /// The size and time of what would be written, as the C# dialog shows them.
    pub incoming: heimdall_files::Stamp,
    /// The size and time of what is there, when known.
    pub existing: Option<heimdall_files::Stamp>,
}

/// A modal decision that concerns the whole window.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Dialog {
    /// The C# "Custom resolution" of an RDP tab: the size typed, as `WIDTHxHEIGHT`.
    CustomResolution {
        /// The tab.
        tab: TabId,
        /// What is typed.
        value: String,
    },
    /// The C# "New tunnel" dialog, as filled.
    NewTunnel(crate::tunnel::TunnelForm),
    /// A gateway on a tunnel's way, or a server `PuTTY` is to open, presented a key never
    /// seen: trusted, or neither opened.
    TunnelHostKey {
        /// The gateway's host.
        host: String,
        /// Its port.
        port: u16,
        /// The key's SHA-256 fingerprint.
        fingerprint: String,
        /// The key's algorithm, as the C# prompt's "Algorithm" row.
        algorithm: String,
    },
    /// Close a tab whose session is live.
    ConfirmCloseTab(TabId),
    /// Close a Files tab whose transfers are running, which closing cancels, as the C#
    /// "Transfer In Progress" question.
    ConfirmCloseTransfers {
        /// Tab.
        tab: TabId,
        /// Its name, as the question says it.
        name: String,
    },
    /// The password sudo asks for on a Files tab's server, to open or save a file with it;
    /// typed in the window, never held here.
    SudoPassword {
        /// Tab.
        tab: TabId,
        /// The file, as the question names it.
        name: String,
        /// What the password is for.
        action: SudoAction,
    },
    /// Close a Files tab whose integrated editor's text is not saved, as the C# close
    /// guard asks.
    ConfirmCloseEditor {
        /// Tab.
        tab: TabId,
        /// The file, as the question names it.
        name: String,
    },
    /// Open the address an application tied to a terminal's text with OSC 8: what the
    /// text says may not be where it leads, so the address itself is shown first.
    ConfirmOpenLink {
        /// The address, http or https only.
        url: String,
    },
    /// Open a file of this computer that would run, a program, an installer, a shortcut or
    /// a script, from the local file browser: its full path shown first, opened with the
    /// system's default program only once agreed.
    ConfirmOpenRunnable {
        /// The local file browser's tab.
        tab: TabId,
        /// The file, as opened.
        file: PathBuf,
        /// Its full path, every invisible character written out.
        shown: String,
        /// Asked for "Open With": the system's chooser is shown once agreed, which can
        /// run it too, rather than its default program.
        chooser: bool,
    },
    /// Download a server's file Open found not to be text, as the C# "Binary file"
    /// question offers.
    ConfirmDownloadBinary {
        /// Tab.
        tab: TabId,
        /// The file, as the question names it.
        name: String,
        /// The server's file.
        remote: heimdall_files::RemotePath,
    },
    /// Close the integrated editor and lose its text not saved, as the C# "Unsaved
    /// Changes".
    ConfirmDiscardEditor {
        /// Tab.
        tab: TabId,
        /// The file, as the question names it.
        name: String,
    },
    /// Close a Files tab with files open in an external editor, whose next saves would
    /// no longer be sent, as the C# close guard asks.
    ConfirmCloseEdits {
        /// Tab.
        tab: TabId,
        /// Its name, as the question says it.
        name: String,
    },
    /// Start broadcast input to every tab.
    ConfirmBroadcast,
    /// Which of the previous run's sessions to reopen, as the C# restore dialog asks.
    RestoreSessions(RestoreDialog),
    /// The keyboard shortcuts, as the C# F1 help.
    Shortcuts,
    /// Turn session transcripts on, which keep what is typed.
    ConfirmSessionLogging,
    /// The RDP settings back to their own values, as the C# "Reset RDP defaults" asks.
    ConfirmResetRdpDefaults,
    /// Every setting back to its default, as the C# "Reset defaults" asks.
    ConfirmResetAllSettings,
    /// Enrol Windows Hello again once the master password opened the vault, its credential
    /// having been found gone, as the C# "Re-enable Windows Hello unlock?".
    ConfirmVaultHelloEnrolAgain,
    /// Migrate the legacy PowerShell Heimdall found at start, as the C# asks: declined, not
    /// asked again for the same files.
    LegacyMigrationOffer(Box<crate::rdpmanager::Offer>),
    /// What the migration from the legacy PowerShell Heimdall did.
    LegacyMigrationDone(Box<LegacyMigrationDone>),
    /// The default SSH mode written into every SSH profile, as the C# "Apply to all saved
    /// sessions" asks, with the size of the rewrite.
    ConfirmApplySshMode {
        /// The mode written.
        mode: heimdall_core::profile::SshMode,
        /// SSH profiles that change.
        changes: usize,
        /// SSH profiles there are.
        total: usize,
    },
    /// The default RDP mode written into every RDP profile, as the C# "Apply to all saved
    /// sessions" of the RDP tab asks, with the size of the rewrite.
    ConfirmApplyRdpMode {
        /// The mode written.
        mode: heimdall_core::profile::RdpMode,
        /// RDP profiles that change.
        changes: usize,
        /// RDP profiles there are.
        total: usize,
    },
    /// A macro's name and inputs, edited, as the C# macro editor.
    EditMacro(Box<macro_editor::MacroDraft>),
    /// Forget the macro of this name, as the C# asks.
    ConfirmDeleteMacro(String),
    /// The name of the macro just recorded, asked before it is kept.
    SaveMacro {
        /// The name typed so far.
        name: String,
        /// What was recorded.
        entries: Vec<heimdall_core::macros::MacroEntry>,
    },
    /// Whether the settings exported take the paths under this computer's user's folder,
    /// as the C# asks: this many.
    ConfirmSettingsExportPaths {
        /// Settings naming such a path.
        count: usize,
    },
    /// The settings read from a file, and what they change, taken once agreed to.
    ConfirmSettingsImport(Box<heimdall_core::settings::SettingsImport>),
    /// Delete an SSH gateway, its references cleared, as the C# asks with what it clears.
    ConfirmDeleteGateway {
        /// The gateway.
        id: ProfileId,
        /// Its name.
        name: String,
        /// Servers going through it.
        servers: usize,
        /// Gateways reached through it.
        gateways: usize,
    },
    /// Quit with live sessions, or text not saved in an integrated editor.
    ConfirmExit {
        /// Live sessions.
        live: usize,
        /// Integrated editors whose text is not saved.
        unsaved: usize,
    },
    /// Paste several lines into a shell that would run them one by one, or a command that
    /// can destroy data or stop the machine.
    ConfirmPaste {
        /// Tab.
        tab: TabId,
        /// Number of lines.
        lines: usize,
        /// The destructive command the text holds, as the C# names it.
        command: Option<&'static str>,
        /// The text as it is shown before it is pasted.
        preview: crate::paste_guard::PastePreview,
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
    /// What an entry of the local file browser is, as the C# local Properties shows it.
    LocalFileProperties(Box<crate::local_properties::LocalProperties>),
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
    /// Delete entries of the server as root, the tab's sudo mode being on: a danger
    /// question naming them, as the C# "Delete as root?".
    ConfirmSudoDelete {
        /// Tab.
        tab: TabId,
        /// The first entries' names, made safe, in order.
        names: Vec<String>,
        /// How many more go, not named.
        more: usize,
    },
    /// What to do with each destination already taken, for a whole transfer, before it
    /// starts, as the C# file conflict dialog.
    FileConflicts {
        /// Tab.
        tab: TabId,
        /// One per destination in the way.
        rows: Vec<ConflictRow>,
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
    /// Run a script from the local file browser by its interpreter, the command shown
    /// whole: asked each time, nothing recorded.
    ConfirmRunScript(Box<ScriptConfirmation>),
    /// Type post-connect steps, shown whole, which the user has not approved yet.
    ConfirmPostConnect(Box<PostConnectConfirmation>),
    /// Result of an import.
    ImportDone(ImportSummary),
    /// An import could not run.
    ImportFailed {
        /// Technical detail.
        detail: String,
    },
    /// The sessions were exported: how many, as the C# says it.
    ExportDone {
        /// Profiles written.
        count: usize,
    },
    /// The export's file could not be written.
    ExportFailed {
        /// Technical detail.
        detail: String,
    },
    /// What an OpenSSH configuration gives, to choose from.
    SessionsPreview(Box<SessionsPreview>),
    /// The OpenSSH configuration picked could not be read.
    SessionsUnreadable {
        /// OpenSSH or `PuTTY`.
        source: SessionsSource,
        /// Technical detail.
        detail: String,
    },
    /// The OpenSSH file picked, or `PuTTY`'s store, gives nothing to import.
    SessionsEmpty {
        /// OpenSSH or `PuTTY`.
        source: SessionsSource,
    },
    /// What `.rdp` files give, to choose from.
    RdpPreview(Box<RdpPreview>),
    /// No `.rdp` file could be read.
    RdpNothing {
        /// Why each could not.
        unreadable: Vec<String>,
    },
    /// What the `.rdp` import did.
    RdpDone(RdpOutcome),
    /// How many sessions a picked file gives, asked before they are imported.
    ConfirmImportFile(Box<PendingImport>),
    /// The profiles of a Heimdall session document, each clash with a choice.
    ProfileImportPreview(Box<ProfileImportPreview>),
    /// How many applications Citrix Workspace's cache gives, asked before they are imported.
    ConfirmCitrixImport(Box<heimdall_core::import::citrix_cache::CacheScan>),
    /// Citrix Workspace's cache gives no application: what the scan said.
    CitrixImportNothing {
        /// What the scan said.
        warnings: Vec<heimdall_core::import::citrix_cache::CacheWarning>,
    },
    /// What the Citrix import did.
    CitrixImportDone(CitrixImportOutcome),
    /// A picked file gives no session: those left out, and what it said.
    ImportNothing {
        /// Sessions left out: display name and reason.
        skipped: Vec<(String, SkipReason)>,
        /// What the file said as a whole.
        warnings: Vec<FileWarning>,
    },
    /// What another `known_hosts` file gives, to choose from.
    HostKeysPreview(Box<HostKeysPreview>),
    /// The `known_hosts` file picked, or Heimdall's own, could not be read or written.
    HostKeysUnreadable {
        /// Technical detail.
        detail: String,
    },
    /// The `known_hosts` file picked gives no key.
    HostKeysEmpty,
    /// What the `known_hosts` import did.
    HostKeysDone {
        /// Keys imported, trusted already, in conflict.
        done: HostKeysOutcome,
        /// Lines the file could not be read at.
        warnings: usize,
    },
    /// What the OpenSSH or `PuTTY` import added.
    SessionsDone {
        /// OpenSSH or `PuTTY`.
        source: SessionsSource,
        /// What it did.
        counts: SessionsCounts,
    },
    /// The profile or settings file could not be read at start: started without it, saving
    /// beside it.
    StoreUnreadable {
        /// Technical detail.
        detail: String,
    },
    /// A change could not be saved, as the C# `EditorSaveErrorMessage` says.
    StoreError {
        /// Technical detail.
        detail: String,
    },
    /// A change was not saved: the profile file was changed by another program or instance
    /// since it was read, and is kept as it is.
    StoreChanged {
        /// Technical detail.
        detail: String,
    },
    /// Forget a key trusted for a server?
    ForgetTrustedKey(TrustedKey),
    /// All that is known of an SSH host key, as the C# "Trusted host key details".
    TrustedHostKeyDetails(Box<heimdall_ssh::KnownHostEntry>),
    /// Forget every certificate trusted for the server of this one?
    ForgetTrustedServer {
        /// A certificate of the server.
        key: TrustedKey,
        /// How many certificates are trusted for it.
        count: usize,
    },
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
    /// One value for several profiles at once, as the C# bulk edit asks it.
    BulkEdit {
        /// What is set.
        field: BulkField,
        /// The profiles selected that take it.
        ids: Vec<ProfileId>,
        /// The value typed, written in when they all shared it.
        value: String,
        /// They did not share one: the field says so, empty.
        mixed: bool,
        /// Why the value confirmed was refused.
        refused: Option<BulkRefusal>,
    },
    /// One password for several profiles at once, as the C# bulk edit asks it, typed
    /// twice. What is typed stays in the window until it is confirmed.
    BulkPassword {
        /// The profiles selected whose password is saved.
        ids: Vec<ProfileId>,
        /// The profiles selected left alone, counted by why.
        skipped: BulkPasswordSkips,
        /// Why the password confirmed was refused.
        refused: Option<BulkPasswordRefusal>,
    },
    /// End a Remote Desktop session from its bar, the tab kept to reconnect, as the C#
    /// asks first.
    ConfirmDisconnectDesktop {
        /// The tab.
        tab: TabId,
        /// What it is connected to.
        name: String,
    },
    /// End a Citrix tab's session, as the C# "Terminate the Citrix session?" asks, or force
    /// its client once it did not close.
    ConfirmCitrixTerminate {
        /// The tab.
        tab: TabId,
        /// Its client process.
        pid: u32,
        /// Whether the client is forced.
        force: bool,
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
        /// How many hold an integrated editor whose text is not saved.
        unsaved: usize,
    },
}

impl Dialog {
    /// What a save that failed with `error` shows: the file changed outside told apart from
    /// any other failure.
    #[must_use]
    pub fn save_failed(error: &StoreError) -> Self {
        let detail = error.to_string();
        if matches!(error, StoreError::ChangedOutside { .. }) {
            Self::StoreChanged { detail }
        } else {
            Self::StoreError { detail }
        }
    }

    /// Whether Enter may answer it. Not for running a program: a key pressed as the dialog
    /// appears, meant for whatever had the focus, must not be taken for agreement.
    #[must_use]
    pub fn confirms_on_enter(&self) -> bool {
        // The vault's and the PIN's fields submit themselves.
        !matches!(
            self,
            Self::ConfirmLocalCommand(_)
                | Self::ConfirmRunScript(_)
                | Self::ConfirmPostConnect(_)
                | Self::Vault(_)
                | Self::Pin(_)
                | Self::EditGateway { .. }
                // A link is opened by a click on its button, never by an Enter meant for the
                // terminal.
                | Self::ConfirmOpenLink { .. }
                // A file that runs is opened by a click too, never by an Enter meant for
                // the browser's list.
                | Self::ConfirmOpenRunnable { .. }
                // A key trusted for good is a click too, as a session's own key card.
                | Self::TunnelHostKey { .. }
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
    /// The pane with the keyboard: the tab shown, or one of the panes of the split shown.
    pub active: Option<TabId>,
    /// The pane the open close question is about alone, not its whole split.
    closing_pane: Option<TabId>,
    /// The tabs detached to windows of their own, off the strip.
    floating: Vec<Floating>,
    /// Window-level dialog, if any.
    pub dialog: Option<Dialog>,
    /// The route test running in the gateway dialog, and what stops it.
    route_test: Option<(u64, CancellationToken)>,
    /// The number of the next route test.
    next_route_test: u64,
    /// What the profile form's SSH agent chip knows.
    agent_chip: AgentChip,
    /// The entries cut in a Files tab, waiting to be pasted.
    files_clipboard: Option<FilesClipboard>,
    /// The files of this computer copied in the local file browser, waiting to be pasted,
    /// where the system's clipboard holds no copied files: everywhere but Windows.
    local_copied: Vec<PathBuf>,
    /// Where a server's files are edited: the user's own local folder.
    edit_dir: Option<PathBuf>,
    /// The address test running in the profile form, and what stops it.
    address_test: Option<(u64, CancellationToken)>,
    /// The number of the next address test.
    next_address_test: u64,
    /// Tunnels the user opened by hand, open: the rows of the tunnels panel.
    pub tunnels: Vec<crate::tunnel::Tunnel>,
    /// A session opens in a tab's place, as Reconnect opens it: it takes no more room.
    replacing: bool,
    /// The SFTP panes opened beside an SSH shell by themselves, until they connect: one
    /// that fails before is closed and said, as the C# auto-open.
    docking_sftp: Vec<TabId>,
    /// Whether the tunnels panel is shown under the sessions while no session is: as the
    /// settings say at start, then as it is toggled.
    tunnels_panel: bool,
    /// The last move a drop in the tree made, to undo.
    last_move: Option<(tree_drag::UndoMove, Instant)>,
    /// The previous run's sessions, until they are offered.
    pending_restore: Option<heimdall_core::session_snapshot::SessionSnapshot>,
    /// The hosts connected to, newest first, with the protocol, as the C#
    /// `RecentConnectionTracker` keeps them: for this run only.
    recent_hosts: Vec<(String, ProfileKind)>,
    /// The tools used lately, as the C# `RecentToolList`: for this run only.
    recent_tools: crate::tools::RecentTools,
    /// The credentials kept for the session selected, as last read.
    detail: detail::DetailCache,
    /// The terminal macros kept.
    macros: heimdall_core::macros::Macros,
    /// The background check of every server.
    monitor: reachability_monitor::Monitor,
    /// The look for a newer release.
    updates: updates::Updates,
    /// Credential Guard required before an embedded RDP session, asked before Windows
    /// Hello.
    credential_guard: credential_guard_gate::CredentialGuardGate,
    /// The user's OpenSSH `known_hosts` was looked at for an import at startup: never
    /// again in this run.
    known_hosts_synced: bool,
    /// The migration from the legacy PowerShell Heimdall offered at start.
    legacy_migration: legacy_migration::LegacyMigrationState,
    /// Windows Hello asked before a connection.
    hello: hello_gate::HelloGate,
    /// Tunnels being opened or open, with what stops them.
    tunnel_runs: Vec<tunnels::TunnelRun>,
    /// The identifier of the next tunnel.
    next_tunnel: crate::tunnel::TunnelId,
    /// The gateway key the user is asked about for a tunnel.
    pending_tunnel_key: Option<tunnels::PendingTunnelKey>,
    /// Launches of `PuTTY` through gateways, until their forwards are released.
    putty_routes: Vec<putty_launch::PuttyRoute>,
    /// The identifier of the next launch of `PuTTY` through a gateway.
    next_putty_route: crate::putty_driver::PuttyRouteId,
    /// Launches of Remote Desktop Connection through gateways, until their forwards are
    /// released.
    mstsc_routes: mstsc_launch::MstscRoutes,
    /// The profile selected in the tree, the last one clicked: where a Shift+click range
    /// starts.
    pub selected_profile: Option<ProfileId>,
    /// The folder selected in the tree, by its path; a profile selected unselects it.
    pub selected_folder: Option<String>,
    /// The profiles selected together, when more than one is.
    selection: std::collections::BTreeSet<ProfileId>,
    /// What the Settings page changes, and the file it is saved to.
    settings: Settings,
    settings_file: std::path::PathBuf,
    /// What the Files tabs keep between runs: bookmarks, the last download folder.
    files_state: heimdall_core::files_state::FilesState,
    /// The splits made between saved profiles, as the C# `SplitLayoutMemory`: the share a
    /// new split of a pair starts at.
    split_layouts: heimdall_core::split_layouts::SplitLayouts,
    /// The transcripts' first and last lines, as the window words them.
    transcript_lines: Option<TranscriptLines>,
    /// Broadcast input: on or off, and the tabs marked.
    broadcast: broadcast::Broadcast,
    /// What was just done, and the session shown then with its state.
    notice: Option<(Notice, (Option<TabId>, SessionStatus))>,
    /// What the status bar said lately, newest first.
    announcements: status::Announcements,
    /// The keys trusted for servers, as the Settings page last read them.
    trusted_keys: TrustedKeys,
    /// What the Settings page's provider Test found.
    provider_test: Option<crate::credential_provider::ProviderTest>,
    viewport: GridSize,
    /// Physical pixels per logical one on the window's screen: RDP desktops ask for that
    /// scale, as mstsc does on a high-density screen.
    display_scale: f32,
    pending_paste: Option<(TabId, String)>,
    /// Planned transfers waiting, one after the other, for the user's answers to the
    /// destinations in their way.
    pending_plans: std::collections::VecDeque<PendingPlan>,
    pending_operation: Option<PendingOperation>,
    vault: VaultState,
    /// Windows Hello unlocking the vault.
    vault_hello: vault_hello::VaultHelloState,
    /// The auto-reconnects due while the workspace was locked, by tab and failed attempt:
    /// attempted once it is unlocked.
    deferred_reconnects: Vec<(TabId, AttemptId)>,
    /// SSH keys trusted for this run only, shared with every connection.
    run_trust: RunTrust,
    /// RDP, FTPS and VNC certificates trusted for this run only: server, port, and the hash
    /// of the whole certificate.
    rdp_run_trust: Vec<(String, u16, heimdall_rdp::CertificateHash)>,
    /// The shared session logs beside the transcripts: desktops' events, Files changes.
    session_logs: crate::session_log::SessionLogs,
    /// The desktops connected, for the events log.
    desktop_sessions: session_events::DesktopSessions,
    /// The folders of the tree shown closed, by path, [`NO_FOLDER`] included.
    closed_folders: std::collections::HashSet<String>,
    /// The tree's filters, beyond its search.
    tree_filter: TreeFilter,
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

/// The files kept beside the profiles that are not settings: the macros, the Files tabs'
/// state and the split layouts; what cannot be read starts empty and is logged.
fn side_files(
    profiles_file: &std::path::Path,
) -> (
    heimdall_core::macros::Macros,
    heimdall_core::files_state::FilesState,
    heimdall_core::split_layouts::SplitLayouts,
) {
    let macros =
        heimdall_core::macros::Macros::load(&heimdall_core::macros::macros_path(profiles_file))
            .unwrap_or_else(|error| {
                log::warn!("macros not read: {error}");
                heimdall_core::macros::Macros::default()
            });
    let files_state = heimdall_core::files_state::FilesState::open(
        profiles_file.with_file_name(heimdall_core::files_state::FILES_STATE_FILE_NAME),
    );
    let (split_layouts, unread) = heimdall_core::split_layouts::SplitLayouts::open(
        &heimdall_core::split_layouts::split_layouts_path(profiles_file),
    );
    if let Some(error) = unread {
        log::warn!("split layouts not read: {error}");
    }
    (macros, files_state, split_layouts)
}

impl App {
    /// The file the profiles are kept in; the settings and the trusted keys are beside it.
    #[must_use]
    pub fn profiles_file(&self) -> &std::path::Path {
        &self.config.profiles_file
    }

    /// The application with the profiles of `config.profiles_file`. A store that cannot
    /// be read starts empty and the problem is shown.
    #[must_use]
    pub fn new(config: AppConfig) -> Self {
        let pending_restore = heimdall_core::session_snapshot::load(
            &heimdall_core::session_snapshot::snapshot_path(&config.profiles_file),
        );
        let (store, dialog) = match ProfileStore::open(&config.profiles_file) {
            Ok(store) => (store, None),
            // Start empty, saving beside the unreadable file so it is never overwritten.
            Err(error) => (
                ProfileStore::empty(config.profiles_file.with_extension(RECOVERY_EXTENSION)),
                Some(Dialog::StoreUnreadable {
                    detail: error.to_string(),
                }),
            ),
        };
        let vault = VaultState::beside(&config.profiles_file, config.system_credentials.clone());
        let (settings, settings_file, dialog) = appearance::load_settings(&config, dialog);
        let tunnels_panel = !settings.collapse_tunnels_panel;
        let (macros, files_state, split_layouts) = side_files(&config.profiles_file);
        let mut app = Self {
            settings,
            settings_file,
            files_state,
            split_layouts,
            transcript_lines: None,
            broadcast: broadcast::Broadcast::default(),
            viewport: config.initial_grid,
            display_scale: 1.0,
            config,
            store,
            tabs: Vec::new(),
            active: None,
            closing_pane: None,
            floating: Vec::new(),
            dialog,
            route_test: None,
            next_route_test: 0,
            agent_chip: AgentChip::Unknown,
            files_clipboard: None,
            local_copied: Vec::new(),
            edit_dir: heimdall_core::paths::edit_dir(),
            address_test: None,
            next_address_test: 0,
            tunnels: Vec::new(),
            // As the settings say it starts, the C# `CollapseTunnelsPanelByDefault`.
            tunnels_panel,
            last_move: None,
            replacing: false,
            docking_sftp: Vec::new(),
            pending_restore,
            recent_hosts: Vec::new(),
            recent_tools: crate::tools::RecentTools::default(),
            detail: detail::DetailCache::default(),
            macros,
            monitor: reachability_monitor::Monitor::default(),
            updates: updates::Updates::new(crate::update_check::running_release()),
            credential_guard: credential_guard_gate::CredentialGuardGate::default(),
            known_hosts_synced: false,
            legacy_migration: legacy_migration::LegacyMigrationState::default(),
            hello: hello_gate::HelloGate::default(),
            tunnel_runs: Vec::new(),
            next_tunnel: crate::tunnel::TunnelId::default(),
            pending_tunnel_key: None,
            putty_routes: Vec::new(),
            next_putty_route: crate::putty_driver::PuttyRouteId::default(),
            mstsc_routes: mstsc_launch::MstscRoutes::default(),
            selected_profile: None,
            selected_folder: None,
            selection: std::collections::BTreeSet::new(),
            notice: None,
            announcements: status::Announcements::default(),
            trusted_keys: TrustedKeys::default(),
            provider_test: None,
            pending_paste: None,
            pending_plans: std::collections::VecDeque::new(),
            pending_operation: None,
            vault_hello: vault_hello::VaultHelloState::with(vault.hello_record()),
            vault,
            deferred_reconnects: Vec::new(),
            run_trust: RunTrust::default(),
            rdp_run_trust: Vec::new(),
            session_logs: crate::session_log::SessionLogs::default(),
            desktop_sessions: session_events::DesktopSessions::default(),
            closed_folders: std::collections::HashSet::new(),
            tree_filter: TreeFilter::default(),
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

    /// Whether the tunnels panel is shown under the sessions, resolved as the C# does it:
    /// the choice made while the tab shown was, else the one its profile keeps, else the
    /// window's.
    #[must_use]
    pub fn tunnels_panel(&self) -> bool {
        let Some(tab) = self.shown_tab() else {
            return self.tunnels_panel;
        };
        tab.tunnels_panel
            .or_else(|| {
                tab.saved_profile()
                    .and_then(|id| self.store.metadata(id))
                    .and_then(|metadata| metadata.tunnels_expanded)
            })
            .unwrap_or(self.tunnels_panel)
    }

    /// The pane with the keyboard: the tab shown, or a pane of its split.
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

    /// Shows `tab`, the pane of its split last given the keyboard when it is split: its bell
    /// is heard, and a desktop's server gets what was copied meanwhile.
    fn select_tab(&mut self, tab: TabId) -> Vec<Effect> {
        if self.tab(tab).is_none() {
            return Vec::new();
        }
        let pane = self.focus_of(tab);
        self.focus_pane(pane)
    }

    /// Applies a message; the windows of the tabs it closed close with them, and the
    /// desktops it connected or ended go to the session events log.
    pub fn update(&mut self, message: Message) -> Vec<Effect> {
        let outermost = self.begin_batch();
        let mut effects = self.apply(message);
        effects.extend(self.prune_floating());
        self.follow_desktop_sessions();
        self.note_status();
        if effects.iter().any(|effect| matches!(effect, Effect::Exit)) {
            self.close_session_logs();
            self.release_putty_routes();
            self.release_mstsc_routes();
        }
        debug_assert!(
            self.floating_invariant_holds(),
            "the keyboard's pane is a detached tab"
        );
        self.end_batch(outermost);
        effects
    }

    /// Applies a message, the windows of the tabs it closed left open.
    #[expect(clippy::too_many_lines, reason = "one arm per family of messages")]
    fn apply(&mut self, message: Message) -> Vec<Effect> {
        self.forget_stale_notice();
        self.stop_orphan_route_test();
        // Credential Guard first, as the C# checks it before Windows Hello: a session it
        // refuses never raises the prompt.
        if self.waits_for_credential_guard(&message) {
            return self.wait_for_credential_guard(message);
        }
        if self.waits_for_hello(&message) {
            return self.wait_for_hello(message);
        }
        match message {
            message @ (Message::OpenProfile(_)
            | Message::OpenFiles(_)
            | Message::OpenRdp(_)
            | Message::OpenRdpWith { .. }
            | Message::OpenTelnet(_)
            | Message::OpenVnc(_)
            | Message::OpenFtp(_)
            | Message::OpenLocal(_)
            | Message::OpenLocalProfile(_)
            | Message::ElevatedLaunched { .. }
            | Message::OpenWinRm(_)
            | Message::OpenCitrix(_)
            | Message::CitrixLaunched { .. }
            | Message::RdpExternalLaunched { .. }
            | Message::MstscRoute { .. }
            | Message::PuttyHostKey { .. }
            | Message::PuttyLaunched { .. }
            | Message::PuttyRoute { .. }
            | Message::ReconnectTab(_)
            | Message::ConnectAs { .. }
            | Message::QuickConnect(_)
            | Message::ForgetServer(_)) => self.open_message(message),
            message @ (Message::DesktopResize { .. }
            | Message::DesktopShown { .. }
            | Message::DesktopInput { .. }
            | Message::SendKeys { .. }
            | Message::VncQuality { .. }
            | Message::AntiIdleTick
            | Message::DisplayScale(_)
            | Message::TmoutResetTick
            | Message::StopAntiIdle(_)
            | Message::StabilizationTick(_)
            | Message::SkipStabilization(_)) => self.desktop_message(message),
            message @ (Message::HealthTick | Message::HealthRead { .. }) => {
                self.health_message(message)
            }
            Message::CitrixTick => self.citrix_tick(),
            Message::Tools(message) => self.tools_message(message),
            Message::CitrixProbed { tab, probe } => {
                self.citrix_probed(tab, &probe);
                Vec::new()
            }
            Message::CitrixTerminate { tab, force } => {
                self.request_citrix_terminate(tab, force, Instant::now());
                Vec::new()
            }
            Message::CitrixTerminated { tab, pid, result } => {
                self.citrix_terminated(tab, pid, result, Instant::now());
                Vec::new()
            }
            Message::ReachabilityTick => self.reachability_round(),
            Message::Update(message) => self.update_message(message),
            Message::ReachabilityChecked { id, verdict } => {
                self.reachability_checked(&id, verdict);
                Vec::new()
            }
            Message::Files(message) => self.files(message),
            Message::Tunnel(message) => self.tunnel_message(message),
            message @ (Message::TestRoute { .. }
            | Message::StopRouteTest
            | Message::RouteTarget { .. }
            | Message::ForgetRouteTest
            | Message::RouteStep { .. }
            | Message::RouteTestDone { .. }) => self.route_test_message(message),
            message @ (Message::TestAddress
            | Message::CancelAddressTest
            | Message::AddressTested { .. }) => self.address_test_message(message),
            Message::SelectTab(tab) => self.select_tab(tab),
            Message::MoveTab { tab, onto } => {
                self.move_tab(tab, onto);
                Vec::new()
            }
            Message::RequestCloseTab(tab) => self.request_close(tab),
            Message::DisconnectDesktop(tab) => {
                self.request_disconnect_desktop(tab);
                Vec::new()
            }
            Message::TabMenu(message) => self.tab_menu(message),
            Message::Split(message) => {
                // The user's split of a detached tab brings it back first; the docking the
                // application does by itself never reaches a detached tab.
                let mut effects = self.reattach_for_split(&message.tabs());
                effects.extend(self.split_message(message));
                effects
            }
            Message::Float(message) => self.float_message(message),
            message @ (Message::AutoReconnect { .. } | Message::CancelAutoReconnect(_)) => {
                self.retry_message(&message)
            }
            message @ (Message::Connection { .. } | Message::Answer { .. }) => {
                self.attempt_message(message)
            }
            Message::HostKeyDecision { tab, accept } => self.host_key_decision(tab, accept.into()),
            Message::HostKeyTrustOnce(tab) => self.host_key_decision(tab, KeyTrust::Once),
            Message::CopyHostKeyFingerprint(tab) => self.copy_host_key_fingerprint(tab),
            Message::Key { tab, input } => self.key(tab, &input),
            Message::Pointer { tab, input } => self.pointer(tab, input),
            Message::Resize { tab, grid, cell } => self.resize(tab, grid, cell),
            Message::ScreenshotTaken { copied } => {
                self.tell(if copied {
                    Notice::ScreenshotCopied
                } else {
                    Notice::ScreenshotFailed
                });
                Vec::new()
            }
            message @ (Message::ScrollHistory { .. }
            | Message::FindInTerminal { .. }
            | Message::Copy(_)
            | Message::PasteRequest(_)
            | Message::SendClipboard(_)
            | Message::ClipboardText { .. }
            | Message::ClipboardImage { .. }
            | Message::ClipboardFiles { .. }) => self.clipboard_message(message),
            message @ (Message::SaveRemoteFiles(_)
            | Message::SaveFolderPicked { .. }
            | Message::CancelSave(_)) => self.save_message(message),
            Message::SyncDeadline { tab, generation } => self.sync_deadline(tab, generation),
            message @ (Message::WindowFocus(_)
            | Message::WindowCloseRequested
            | Message::ImportLegacy
            | Message::ImportCitrix
            | Message::CitrixScanned(_)
            | Message::ExportSessions
            | Message::ExportFinished(_)
            | Message::Sessions(_)
            | Message::Rdp(_)
            | Message::ProfileImport(_)
            | Message::Settings(_)
            | Message::Macro(_)
            | Message::NoteOpened(_)
            | Message::SettingsTransfer(_)
            | Message::Gateways(_)
            | Message::Broadcast(_)) => self.window_message(&message),
            message @ (Message::NewProfile
            | Message::EditProfile(_)
            | Message::ProfileField { .. }
            | Message::DeleteProfile
            | Message::SaveProfile { .. }
            | Message::ClearPassword
            | Message::ClearPassphrase
            | Message::ChooseProtocol(_)
            | Message::ProfileToggle { .. }
            | Message::PostConnectEdit(_)
            | Message::ProfileChoice(_)
            | Message::NewGateway
            | Message::EditGateway(_)
            | Message::GatewayField { .. }
            | Message::ChooseParentGateway(_)
            | Message::ClearGatewayPassword
            | Message::ClearGatewayPassphrase
            | Message::SaveGateway { .. }
            | Message::ChooseGateway(_)) => self.profile_form_message(message),
            message @ (Message::RefreshAgents | Message::AgentsSurveyed(_)) => {
                self.agent_chip_message(message)
            }
            Message::DropProfiles { ids, onto } => {
                self.drop_profiles(&ids, &onto);
                Vec::new()
            }
            Message::DropFolder { path, onto } => {
                self.drop_folder_on(&path, &onto);
                Vec::new()
            }
            Message::UndoMove => {
                self.undo_move();
                Vec::new()
            }
            Message::NudgeProfile { id, down } => {
                self.nudge_profile(&id, down);
                Vec::new()
            }
            Message::RestoreChoose { index, chosen } => {
                self.choose_restored(index, chosen);
                Vec::new()
            }
            message @ (Message::ConfirmDialog
            | Message::DismissDialog
            | Message::ShowShortcuts
            | Message::SkipPostConnect
            | Message::StopPostConnect(_)) => self.dialog_message(&message),
            message @ (Message::SelectProfile(_)
            | Message::FoldAll(_)
            | Message::SelectFolder(_)
            | Message::ToggleFolder(_)
            | Message::Filter(_)
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
            | Message::VaultOpened(..)
            | Message::LockVault
            | Message::Idle(_)) => self.vault_message(message),
            Message::VaultHello(message) => self.vault_hello_message(message),
            Message::SetBulkPassword { password, confirm } => {
                self.set_bulk_password(&password, &confirm);
                Vec::new()
            }
            Message::Pin(message) => self.pin_message(message),
            Message::CredentialProvider(message) => self.provider_message(message),
            Message::CredentialProvided(answer) => self.provider_answered(*answer),
            Message::WindowsHello(answer) => self.hello_answered(answer),
            Message::CredentialGuard(status) => self.credential_guard_answered(status),
            Message::LegacyMigration(message) => self.legacy_migration_message(message),
        }
    }

    /// An event of a connection attempt, or the user's answer to its question.
    fn attempt_message(&mut self, message: Message) -> Vec<Effect> {
        match message {
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
            _ => Vec::new(),
        }
    }

    /// Answers the open dialog, or stops a tab's post-connect steps.
    fn dialog_message(&mut self, message: &Message) -> Vec<Effect> {
        match message {
            Message::ConfirmDialog => self.confirm_dialog(),
            Message::ShowShortcuts => {
                if self.dialog.is_none() {
                    self.dialog = Some(Dialog::Shortcuts);
                }
                Vec::new()
            }
            Message::DismissDialog => self
                .dismiss_vault()
                .or_else(|| self.dismiss_pin())
                .or_else(|| self.dismiss_tunnel_key())
                .or_else(|| self.dismiss_settings_export())
                .unwrap_or_else(|| self.dismiss_dialog()),
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
                    found.find_found = found.terminal.find(&query, direction);
                    found.find_missed = found.find_found.is_none();
                }
            }
            _ => {}
        }
        Vec::new()
    }

    /// Applies a message for a remote desktop, or a session timer's.
    fn desktop_message(&mut self, message: Message) -> Vec<Effect> {
        match message {
            Message::DesktopResize { tab, width, height } => {
                if let Some(pane) = self.tab(tab).and_then(|found| found.desktop.as_ref()) {
                    pane.resize(width, height);
                }
            }
            Message::DesktopShown { tab, width, height } => {
                if let Some(pane) = self.tab(tab).and_then(|found| found.desktop.as_ref()) {
                    pane.shown_at(width, height);
                }
            }
            Message::DesktopInput { tab, inputs } => self.desktop_input(tab, &inputs),
            Message::SendKeys { tab, keys } => self.send_keys(tab, keys),
            Message::VncQuality { tab, quality } => {
                if let Some(pane) = self
                    .tab_mut(tab)
                    .and_then(|found| found.desktop.as_deref_mut())
                {
                    pane.set_vnc_quality(quality);
                }
            }
            Message::AntiIdleTick => self.anti_idle_tick(),
            // A scale no screen has is not taken.
            Message::DisplayScale(scale) if scale.is_finite() && scale > 0.0 => {
                self.display_scale = scale;
            }
            // Not a desktop's, but a session timer's as anti-idle is.
            Message::TmoutResetTick => self.tmout_reset_tick(),
            Message::StopAntiIdle(tab) => self.stop_anti_idle(tab),
            Message::StabilizationTick(now) => self.stabilization_tick(now),
            Message::SkipStabilization(tab) => self.skip_stabilization(tab),
            _ => {}
        }
        Vec::new()
    }

    /// Applies a message opening a session: a new tab, or a tab's session again.
    fn open_message(&mut self, message: Message) -> Vec<Effect> {
        match message {
            Message::OpenProfile(id) => self.open_profile(&id, Purpose::Shell),
            Message::OpenFiles(id) => self.open_profile(&id, Purpose::Files),
            Message::OpenRdp(id) => self.open_rdp(&id),
            Message::OpenRdpWith { id, mode } => self.open_rdp_with(&id, mode),
            Message::OpenTelnet(id) => self.open_telnet(&id),
            Message::OpenVnc(id) => self.open_vnc(&id),
            Message::OpenFtp(id) => self.open_ftp(&id),
            Message::OpenLocal(shell) => self.open_local(shell),
            Message::OpenLocalProfile(id) => self.open_local_profile(&id),
            Message::ElevatedLaunched { tab, outcome } => self.elevated_launched(tab, &outcome),
            Message::OpenWinRm(id) => self.open_winrm(&id),
            Message::OpenCitrix(id) => self.open_citrix(&id),
            Message::CitrixLaunched { tab, name, result } => {
                self.citrix_launched(tab, &name, result)
            }
            Message::RdpExternalLaunched {
                name,
                gateway,
                result,
            } => self.rdp_external_launched(name, gateway, result),
            Message::MstscRoute { id, event } => self.mstsc_route_event(id, event),
            Message::PuttyHostKey { profile, probe } => self.putty_host_key(*profile, probe),
            Message::PuttyLaunched { name, result } => self.putty_launched(name, result),
            Message::PuttyRoute { id, event } => self.putty_route_event(id, event),
            Message::ReconnectTab(tab) => self.reconnect_tab(tab),
            Message::ForgetServer(tab) => self.forget_server(tab),
            Message::ConnectAs { id, protocol } => self.connect_as(&id, protocol),
            Message::QuickConnect(result) => self.quick_connect(result),
            _ => Vec::new(),
        }
    }

    /// Closes the open dialog and drops what it held; the gateway dialog returns to the
    /// session's form it was opened from.
    fn dismiss_dialog(&mut self) -> Vec<Effect> {
        if self.dismiss_gateway() {
            return Vec::new();
        }
        let mut effects = Vec::new();
        match self.dialog.take() {
            Some(Dialog::FileConflicts { .. }) => effects = self.cancel_conflicts(),
            // The password asked to list as root not given: the sudo mode is not turned on.
            Some(Dialog::SudoPassword {
                tab,
                action: SudoAction::List(_),
                ..
            }) => effects = self.sudo_mode_off(tab),
            // "Don't restore": answered, the snapshot goes.
            Some(Dialog::RestoreSessions(_)) => self.forget_snapshot(),
            // "Do not import": not asked again for the same files.
            Some(Dialog::LegacyMigrationOffer(offer)) => self.decline_legacy_migration(&offer),
            _ => {}
        }
        self.pending_paste = None;
        self.pending_operation = None;
        self.closing_pane = None;
        self.ask_next_conflicts();
        effects
    }

    /// What connecting to `profile` needs, its gateways included; an error when they cannot
    /// be followed, which no attempt could get past.
    ///
    /// See [`App::ssh_options`] for what every SSH connection shares.
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
        let mut options = self.ssh_options();
        options.initial_size = terminal_size(grid, None);
        options.forward_agent = profile.forward_agent;
        options.compression = profile.compression;
        // X11 for a shell alone: its files and a desktop tunnelled through it have no
        // X11 programs.
        let x11 = (purpose == Purpose::Shell && profile.x11_forwarding)
            .then(|| X11Settings::of(&self.settings));
        Ok(ConnectRequest {
            profile: profile.clone(),
            route: route.iter().map(SshGateway::as_hop).collect(),
            purpose,
            options,
            x11,
            cancel,
        })
    }

    fn open_profile(&mut self, id: &ProfileId, purpose: Purpose) -> Vec<Effect> {
        let Some(profile) = self.profiles().iter().find(|p| &p.id == id).cloned() else {
            return Vec::new();
        };
        self.open_ssh(profile, purpose)
    }

    /// Whether one more session may not open, as the C# `MaxEmbeddedSessions`, which is
    /// said: no limit at 0; this computer's own shells are not counted, nor a session
    /// opening in a tab's place.
    pub(super) fn session_limit_reached(&mut self) -> bool {
        let max = self.settings.max_sessions;
        if max == 0 || self.replacing {
            return false;
        }
        let open = self
            .tabs
            .iter()
            .filter(|tab| !matches!(tab.profile, TabProfile::Local(_) | TabProfile::Tool(_)))
            .count();
        let reached = open >= usize::try_from(max).unwrap_or(usize::MAX);
        if reached {
            self.tell(Notice::SessionLimitReached(max));
        }
        reached
    }

    /// Opens a tab for `profile`, a shell or its files, without asking about its steps:
    /// within the session limit, and its files only while the SFTP browser is on, as the C#
    /// SFTP handler refuses them.
    pub(super) fn open_ssh_now(&mut self, profile: SshProfile, purpose: Purpose) -> Vec<Effect> {
        if purpose == Purpose::Files && !self.settings.sftp_browser.enabled {
            self.tell(Notice::SftpBrowserDisabled);
            return Vec::new();
        }
        if self.session_limit_reached() {
            return Vec::new();
        }
        self.open_ssh_tab(profile, purpose).1
    }

    /// Opens a tab for `profile`, a shell or its files, the last of the tabs and the active
    /// one, whatever the session limit; the tab, and what connects it.
    pub(super) fn open_ssh_tab(
        &mut self,
        profile: SshProfile,
        purpose: Purpose,
    ) -> (TabId, Vec<Effect>) {
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
        let start = self.files_start();
        let follow = self.settings.sftp_browser.follow_ssh_directory;
        tab.files = (purpose == Purpose::Files).then(|| {
            let mut files = FilesPane::new(start);
            // As the C# seeds every SFTP pane's "cwd" toggle when it opens.
            files.follow = Some(crate::files::ShellFollow::seeded(follow));
            Box::new(files)
        });
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
        (tab_id, effects)
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
            self.read_winrm_output(tab_id, bytes);
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
        let shell_up = matches!(event, ConnectionEvent::Connected { .. });
        if matches!(event, ConnectionEvent::FilesReady { .. }) {
            self.docking_sftp.retain(|docking| *docking != tab_id);
        }
        let mut effects = self.apply_connection_event(tab_id, event);
        if let Some(tab) = self.tab_mut(tab_id) {
            tab.time_session(was_connected, Instant::now());
        }
        self.follow_transcript(tab_id, was_connected);
        if !was_connected {
            self.note_recent(tab_id);
        }
        if !was_connected && self.active == Some(tab_id) {
            self.warn_winrm(tab_id);
            self.warn_ftp_cleartext(tab_id);
        }
        if shell_up {
            effects.extend(self.dock_sftp(tab_id));
            effects.extend(self.dock_local_browser(tab_id));
        }
        if let Some((error, was_live)) = failure
            && !self.docked_sftp_failed(tab_id, error.clone())
        {
            effects.extend(self.retry_after(tab_id, &error, was_live));
        }
        effects
    }

    #[expect(clippy::too_many_lines, reason = "one arm per connection event")]
    fn apply_connection_event(&mut self, tab_id: TabId, event: ConnectionEvent) -> Vec<Effect> {
        let active = self.in_sight(tab_id);
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
            ConnectionEvent::DesktopResizeRefused { width, height } => {
                self.resize_refused(tab_id, (width, height))
            }
            ConnectionEvent::RdpStep(step) => {
                tab.rdp_step = Some(step);
                Vec::new()
            }
            event @ (ConnectionEvent::UnknownRdpCertificate { .. }
            | ConnectionEvent::RdpReady { .. }
            | ConnectionEvent::DesktopFrame) => self.rdp_event(tab_id, event),
            ConnectionEvent::RemoteClipboard(text) => {
                vec![Effect::WriteClipboard(String::clone(&text))]
            }
            ConnectionEvent::RemoteImage(image) => vec![Effect::WriteClipboardImage(image)],
            ConnectionEvent::SshConnection(connection) => self.shell_connection(tab_id, connection),
            ConnectionEvent::X11ServerNotFound => {
                self.tell(Notice::X11ServerNotFound);
                Vec::new()
            }
            event @ (ConnectionEvent::RdpFilesRefused(_)
            | ConnectionEvent::RdpRemoteFiles(_)
            | ConnectionEvent::RdpSaveProgress { .. }
            | ConnectionEvent::RdpSaveEnded(_)) => self.clipboard_files_event(tab_id, event),
            event @ (ConnectionEvent::VncReady { .. } | ConnectionEvent::DesktopRenamed(_)) => {
                vnc_tab::apply(tab, event);
                Vec::new()
            }
            ConnectionEvent::Connected { input } => {
                // Back: the attempts stop, and a later loss starts a chain of its own.
                tab.retry = None;
                tab.phase = Phase::Connected;
                let grid = tab.terminal.size();
                if grid != tab.connect_grid {
                    let _ = input.resize(terminal_size(grid, tab.cell));
                }
                tab.sink = Some(input);
                Vec::new()
            }
            ConnectionEvent::FilesReady { client, shell } => {
                tab.phase = Phase::Connected;
                if let Some(files) = tab.files.as_mut() {
                    files.client = Some(client);
                    files.shell = shell;
                }
                self.files_ready(tab_id)
            }
            ConnectionEvent::Output(bytes) => {
                if let Some(playing) = &tab.macro_playing {
                    playing.saw(&bytes);
                }
                let mut output = tab.terminal.feed(&bytes);
                let directory = output.working_directory.take();
                let mut effects = handle_feed(tab, output, active);
                if let Some(directory) = directory {
                    effects.extend(self.working_directory_reported(tab_id, directory));
                }
                effects
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
                // What waits its turn has no session left to run on.
                if let Some(files) = tab.files.as_deref_mut() {
                    files.cancel_waiting();
                }
                tab.post_connect = None;
                tab.sink = None;
                tab.desktop = None;
                tab.prompts.clear();
                self.winrm_ended(tab_id, exit_status);
                Vec::new()
            }
            ConnectionEvent::Ended { reason } => {
                tab.phase = Phase::Closed { exit_status: None };
                if let Some(files) = tab.files.as_deref_mut() {
                    files.cancel_waiting();
                }
                tab.end_reason = Some(reason);
                tab.sink = None;
                tab.desktop = None;
                tab.prompts.clear();
                Vec::new()
            }
            ConnectionEvent::Failed(error) => {
                tab.dropped = tab.phase == Phase::Connected;
                tab.phase = Phase::Failed(error);
                if let Some(files) = tab.files.as_deref_mut() {
                    files.cancel_waiting();
                    // A listing kept in sight after a drop is read only: nothing it offers
                    // reaches the session gone.
                    files.client = None;
                    files.shell = None;
                }
                tab.sink = None;
                tab.desktop = None;
                tab.prompts.clear();
                Vec::new()
            }
        }
    }

    /// Copies the fingerprint the tab's key question shows, an SSH key's or a certificate's.
    fn copy_host_key_fingerprint(&mut self, tab_id: TabId) -> Vec<Effect> {
        let Some(Phase::HostKey {
            host,
            port,
            fingerprint,
        }) = self.tab(tab_id).map(|tab| &tab.phase)
        else {
            return Vec::new();
        };
        let address = heimdall_core::profile::display_address(host, *port);
        let fingerprint = fingerprint.clone();
        self.tell(Notice::FingerprintCopied(address));
        vec![Effect::WriteClipboard(fingerprint)]
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
        let target = heimdall_core::profile::display_address(&host, port);
        if trust == KeyTrust::Refused {
            log::info!("the host key of {target} was refused by the user");
            tab.phase = Phase::Failed(UiError::Cancelled);
            return Vec::new();
        }
        let learned = match known_hosts.recorded(&host, port) {
            // Held in memory for this run: the file is not written.
            Ok(_) if trust == KeyTrust::Once => {
                log::info!(
                    "the host key of {target} is trusted by the user for this run: {}",
                    fingerprint(&key)
                );
                run_trust.trust(&host, port, PublicKey::clone(&key));
                Ok(())
            }
            // Another tab may have recorded a key for this host meanwhile, or a pin been
            // carried over: checked again, and written, under the lock of the trust files.
            // A key of another algorithm is refused, as a connection refuses it.
            Ok(_) => match known_hosts_import::trust(
                &known_hosts,
                &host,
                port,
                &key,
                OtherAlgorithm::Conflicts,
                HostKeySource::User,
            ) {
                Ok(Trusting::Recorded) => Ok(()),
                Ok(Trusting::Learn | Trusting::LearnPinned) => {
                    log::info!(
                        "the host key of {target} is trusted by the user and recorded: {}",
                        fingerprint(&key)
                    );
                    Ok(())
                }
                Ok(Trusting::Conflict(contradiction)) => Err(UiError::host_key_contradicted(
                    contradiction,
                    &host,
                    port,
                    &key,
                )),
                Err(error) => Err(UiError::from(&error)),
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
            // So does the WinRM one.
            TabProfile::WinRm(_) => return self.reconnect_winrm(tab_id),
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
            if let Some(recording) = tab.macro_recording.as_mut() {
                recording.note(&bytes);
            }
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
        // Ctrl+click on an OSC 8 link: its address is asked about first, as the text may
        // hide it; an address neither http nor https is not opened, as the C# policy.
        if matches!(input.action, MouseAction::Press(MouseButton::Left))
            && input.modifiers.ctrl
            && let Some(link) = tab.terminal.hyperlink_at(input.at)
        {
            if let Some(url) = crate::external_url::launchable_url(&link) {
                self.dialog = Some(Dialog::ConfirmOpenLink { url });
            }
            return Vec::new();
        }
        // Ctrl+click on a web address opens it, as the C# terminal does, rather than select.
        if matches!(input.action, MouseAction::Press(MouseButton::Left))
            && input.modifiers.ctrl
            && let Some(url) = tab
                .terminal
                .url_at(input.at)
                .and_then(|url| crate::external_url::launchable_url(&url))
        {
            return vec![Effect::OpenUrl(url)];
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
        // A pane is a share of the window, a tab's own window another window: the next tab
        // opens at the size of a whole tab of the main window.
        if !self.in_split(tab_id) && !self.is_floating(tab_id) {
            self.viewport = grid.clamped();
        }
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
        self.active
            .map(|tab| self.focus_report(tab, focused))
            .unwrap_or_default()
    }

    /// The window showing `tab` gained or lost the focus: its terminal is told when it
    /// asked to be, and its desktop gets what was copied elsewhere meanwhile.
    fn focus_report(&self, tab: TabId, focused: bool) -> Vec<Effect> {
        let Some(found) = self.tab(tab) else {
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
                .map(|pane| {
                    if pane.shares_clipboard() {
                        Effect::ReadDesktopClipboard { tab }
                    } else {
                        Effect::ReadClipboard { tab }
                    }
                })
                .into_iter()
                .collect(),
            Message::ClipboardText { tab, text } => self.clipboard_text(tab, text),
            Message::ClipboardFiles { tab, paths } => {
                if let Some(pane) = self
                    .tab(tab)
                    .and_then(|found| found.desktop.as_deref())
                    .filter(|pane| pane.shares_clipboard())
                {
                    pane.offer_files(paths);
                }
                Vec::new()
            }
            Message::ClipboardImage { tab, image } => {
                if let Some(pane) = self
                    .tab(tab)
                    .and_then(|found| found.desktop.as_deref())
                    .filter(|pane| pane.shares_clipboard())
                {
                    pane.offer_image(image);
                }
                Vec::new()
            }
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
        // A destructive command is asked about whatever the shell does with the lines, as
        // the C# smart paste guard.
        let command = crate::paste_guard::dangerous_command(&text);
        if runs_lines || command.is_some() {
            self.dialog = Some(Dialog::ConfirmPaste {
                tab: tab_id,
                lines: command_lines(&text),
                command,
                preview: crate::paste_guard::PastePreview::of(&text),
            });
            self.pending_paste = Some((tab_id, text));
            return Vec::new();
        }
        self.paste_to(&targets, &text);
        Vec::new()
    }

    /// Pastes `text` into the sessions of `targets`, each as its modes ask.
    fn paste_to(&mut self, targets: &[TabId], text: &str) {
        for target in targets {
            if let Some(tab) = self.tab_mut(*target) {
                let bytes = encode_paste(text, &tab.terminal.input_mode());
                if let Some(recording) = tab.macro_recording.as_mut() {
                    recording.note(&bytes);
                }
                tab.write(bytes);
            }
        }
    }

    fn sync_deadline(&mut self, tab_id: TabId, generation: u64) -> Vec<Effect> {
        let active = self.in_sight(tab_id);
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

    /// Whether `tab` is in sight for its bell: the pane with the keyboard, or a tab in a
    /// window of its own.
    fn in_sight(&self, tab: TabId) -> bool {
        self.active == Some(tab) || self.is_floating(tab)
    }

    fn live_tabs(&self) -> usize {
        self.tabs.iter().filter(|tab| tab.is_live()).count()
    }

    /// Tabs whose integrated editor's text is not saved, or still being saved.
    pub(crate) fn unsaved_tabs(&self, tabs: &[TabId]) -> usize {
        tabs.iter()
            .filter_map(|id| self.tab(*id))
            .filter(|tab| tab.holds_unsaved_text())
            .count()
    }

    /// The session bar's Disconnect, as the C# one: an RDP desktop asks first, as the C#
    /// `RdpConfirmDisconnect` does by default; a VNC one ends at once.
    fn request_disconnect_desktop(&mut self, tab_id: TabId) {
        let Some(tab) = self.tab(tab_id) else {
            return;
        };
        if tab.phase != Phase::Connected || tab.desktop.is_none() {
            return;
        }
        if tab.purpose == Purpose::Rdp {
            self.dialog = Some(Dialog::ConfirmDisconnectDesktop {
                tab: tab_id,
                name: tab.display_title().to_owned(),
            });
        } else {
            self.disconnect_desktop(tab_id);
        }
    }

    /// Ends `tab_id`'s remote desktop, the tab kept and Reconnect offered: the user ended it,
    /// so nothing reconnects by itself, and whatever the old session still reports is
    /// another attempt's.
    fn disconnect_desktop(&mut self, tab_id: TabId) {
        self.desktop_disconnected_by_user(tab_id);
        let Some(tab) = self.tab_mut(tab_id) else {
            return;
        };
        tab.attempt = AttemptId::fresh();
        tab.retry = None;
        tab.stop();
        tab.end_reason = None;
        tab.phase = Phase::Closed { exit_status: None };
    }

    /// Closes `tab_id`, every pane of its split with it, once asked when one of them would
    /// lose something: the guards of all its panes, as one.
    fn request_close(&mut self, tab_id: TabId) -> Vec<Effect> {
        self.closing_pane = None;
        if self.tab(tab_id).is_none() {
            return Vec::new();
        }
        let panes = self.panes_of(tab_id);
        if !self.ask_before_closing(tab_id, &panes) {
            self.close_tab(tab_id);
        }
        Vec::new()
    }

    /// Closes `tab_id` and, for a split tab, every pane of it, as the C# `CloseAllPanes`.
    fn close_tab(&mut self, tab_id: TabId) {
        for pane in self.panes_of(tab_id) {
            if pane != tab_id {
                self.close_pane(pane);
            }
        }
        self.close_pane(tab_id);
    }

    fn close_window(&mut self) -> Vec<Effect> {
        let live = self.live_tabs();
        let all: Vec<TabId> = self.tabs.iter().map(|tab| tab.id).collect();
        let unsaved = self.unsaved_tabs(&all);
        if live > 0 || unsaved > 0 {
            self.dialog = Some(Dialog::ConfirmExit { live, unsaved });
            return Vec::new();
        }
        self.keep_snapshot();
        vec![Effect::Exit]
    }

    fn confirm_dialog(&mut self) -> Vec<Effect> {
        if let Some(Dialog::NewTunnel(_) | Dialog::TunnelHostKey { .. }) = &self.dialog {
            return self.confirm_tunnel_dialog();
        }
        let effects = self.confirm_open_dialog();
        // A transfer's question waits for any other to be answered.
        self.ask_next_conflicts();
        effects
    }

    #[expect(clippy::too_many_lines, reason = "one arm per dialog")]
    fn confirm_open_dialog(&mut self) -> Vec<Effect> {
        match self.dialog.take() {
            Some(
                dialog @ (Dialog::SessionsPreview(_)
                | Dialog::RdpPreview(_)
                | Dialog::HostKeysPreview(_)
                | Dialog::ConfirmImportFile(_)
                | Dialog::ProfileImportPreview(_)),
            ) => {
                self.confirm_import(dialog);
                Vec::new()
            }
            Some(Dialog::ConfirmCitrixImport(scan)) => {
                self.import_citrix(*scan);
                Vec::new()
            }
            Some(Dialog::CustomResolution { tab, value }) => {
                self.confirm_custom_resolution(tab, &value);
                Vec::new()
            }
            Some(
                Dialog::ConfirmCloseTab(tab)
                | Dialog::ConfirmCloseTransfers { tab, .. }
                | Dialog::ConfirmCloseEditor { tab, .. }
                | Dialog::ConfirmCloseEdits { tab, .. },
            ) => {
                // Asked about one pane alone: its split stays.
                if self.closing_pane.take() == Some(tab) {
                    self.close_pane(tab);
                } else {
                    self.close_tab(tab);
                }
                Vec::new()
            }
            Some(Dialog::ConfirmDownloadBinary { tab, remote, .. }) => {
                self.download_remote(tab, &remote)
            }
            Some(Dialog::ConfirmOpenLink { url }) => vec![Effect::OpenUrl(url)],
            Some(Dialog::ConfirmOpenRunnable {
                tab, file, chooser, ..
            }) => {
                if chooser {
                    vec![Effect::OpenWithChooser { tab, file }]
                } else {
                    vec![Effect::OpenLocalFile { tab, file }]
                }
            }
            Some(Dialog::ConfirmDiscardEditor { tab, .. }) => {
                if let Some(files) = self.files_mut(tab) {
                    files.editor = None;
                }
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
            Some(Dialog::ConfirmDisconnectDesktop { tab, .. }) => {
                self.disconnect_desktop(tab);
                Vec::new()
            }
            Some(Dialog::ConfirmCitrixTerminate { tab, pid, force }) => {
                self.confirm_citrix_terminate(tab, pid, force, Instant::now())
            }
            Some(Dialog::BulkEdit {
                field,
                ids,
                value,
                mixed,
                ..
            }) => {
                self.confirm_bulk_edit(field, &ids, &value, mixed);
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
            Some(Dialog::ConfirmSessionLogging) => self.confirm_session_logging(),
            Some(Dialog::ConfirmResetRdpDefaults) => self.confirm_reset_rdp_defaults(),
            Some(Dialog::ConfirmResetAllSettings) => self.confirm_reset_all_settings(),
            Some(Dialog::ConfirmVaultHelloEnrolAgain) => self.enrol_vault_hello_again(),
            Some(Dialog::LegacyMigrationOffer(offer)) => {
                self.migrate_legacy(*offer);
                Vec::new()
            }
            Some(Dialog::ConfirmApplySshMode { mode, .. }) => self.confirm_apply_ssh_mode(mode),
            Some(Dialog::ConfirmApplyRdpMode { mode, .. }) => self.confirm_apply_rdp_mode(mode),
            Some(Dialog::EditMacro(draft)) => {
                self.save_edited_macro(*draft);
                Vec::new()
            }
            Some(Dialog::ConfirmDeleteMacro(name)) => {
                self.delete_macro(&name);
                Vec::new()
            }
            Some(Dialog::SaveMacro { name, entries }) => {
                self.save_macro(&name, entries);
                Vec::new()
            }
            Some(Dialog::ConfirmSettingsExportPaths { .. }) => self.export_settings(true),
            Some(Dialog::ConfirmSettingsImport(read)) => self.apply_imported_settings(*read),
            Some(Dialog::ConfirmDeleteGateway { id, name, .. }) => {
                self.confirm_delete_gateway(&id, &name);
                Vec::new()
            }
            Some(Dialog::ForgetTrustedKey(key)) => {
                self.forget_trusted_key(&key);
                Vec::new()
            }
            Some(Dialog::ForgetTrustedServer { key, .. }) => {
                self.forget_server_of(&key);
                Vec::new()
            }
            Some(Dialog::ConfirmExit { .. }) => {
                self.keep_snapshot();
                for tab in &mut self.tabs {
                    tab.stop();
                }
                vec![Effect::Exit]
            }
            Some(Dialog::RestoreSessions(dialog)) => self.restore_sessions(dialog),
            Some(Dialog::ConfirmPaste { .. }) => {
                if let Some((tab_id, text)) = self.pending_paste.take() {
                    let targets = self.input_targets(tab_id);
                    self.paste_to(&targets, &text);
                }
                Vec::new()
            }
            Some(Dialog::FileConflicts { rows, .. }) => self.confirm_conflicts(&rows),
            Some(Dialog::AskName { value, .. }) => self.confirm_operation(Some(&value)),
            Some(Dialog::ConfirmDelete { .. } | Dialog::ConfirmSudoDelete { .. }) => {
                self.confirm_operation(None)
            }
            Some(Dialog::EditProfile { draft, .. }) => {
                self.save_profile(draft, None, None);
                Vec::new()
            }
            Some(Dialog::ConfirmDeleteProfile { id, .. }) => {
                self.delete_profile(&id);
                Vec::new()
            }
            Some(Dialog::ConfirmLocalCommand(confirmation)) => self.confirm_local(*confirmation),
            Some(Dialog::ConfirmRunScript(confirmation)) => self.confirm_script(*confirmation),
            Some(Dialog::ConfirmPostConnect(confirmation)) => self.run_post_connect(*confirmation),
            // sudo's question is answered with the password the window holds, never by a
            // bare confirm.
            Some(
                dialog @ (Dialog::Vault(_)
                | Dialog::Pin(_)
                | Dialog::EditGateway { .. }
                | Dialog::SudoPassword { .. }
                | Dialog::BulkPassword { .. }),
            ) => {
                self.dialog = Some(dialog);
                Vec::new()
            }
            Some(
                Dialog::ImportDone(_)
                | Dialog::LegacyMigrationDone(_)
                | Dialog::TrustedHostKeyDetails(_)
                | Dialog::FileProperties(_)
                | Dialog::LocalFileProperties(_)
                | Dialog::ImportFailed { .. }
                | Dialog::ExportDone { .. }
                | Dialog::ExportFailed { .. }
                | Dialog::SessionsUnreadable { .. }
                | Dialog::SessionsEmpty { .. }
                | Dialog::SessionsDone { .. }
                | Dialog::RdpNothing { .. }
                | Dialog::RdpDone(_)
                | Dialog::ImportNothing { .. }
                | Dialog::CitrixImportNothing { .. }
                | Dialog::CitrixImportDone(_)
                | Dialog::HostKeysUnreadable { .. }
                | Dialog::HostKeysEmpty
                | Dialog::HostKeysDone { .. }
                | Dialog::StoreUnreadable { .. }
                | Dialog::StoreError { .. }
                | Dialog::StoreChanged { .. }
                | Dialog::PasswordSaveFailed { .. }
                | Dialog::Shortcuts
                // Confirmed before: see `confirm_dialog`.
                | Dialog::NewTunnel(_)
                | Dialog::TunnelHostKey { .. },
            )
            | None => Vec::new(),
        }
    }

    /// The window's focus and its close, and what it asks of the application: the import
    /// from the C# Heimdall and the export for it, a change of the settings or of broadcast
    /// input.
    fn window_message(&mut self, message: &Message) -> Vec<Effect> {
        match message {
            Message::WindowFocus(focused) => self.window_focus(*focused),
            Message::WindowCloseRequested => self.close_window(),
            Message::ImportLegacy => {
                self.import_legacy();
                Vec::new()
            }
            Message::ExportSessions => vec![self.export_sessions()],
            Message::ImportCitrix => Self::scan_citrix(),
            Message::CitrixScanned(scan) => {
                self.citrix_scanned(scan.clone());
                Vec::new()
            }
            Message::Sessions(message) => self.sessions_message(message.clone()),
            Message::Rdp(message) => self.rdp_message(message.clone()),
            Message::ProfileImport(message) => {
                self.profile_import_message(*message);
                Vec::new()
            }
            Message::ExportFinished(outcome) => {
                self.dialog = match outcome {
                    ExportOutcome::Saved(count) => Some(Dialog::ExportDone { count: *count }),
                    ExportOutcome::Failed(detail) => Some(Dialog::ExportFailed {
                        detail: detail.clone(),
                    }),
                    ExportOutcome::Cancelled => self.dialog.take(),
                };
                Vec::new()
            }
            Message::Settings(message) => self.settings_message(message),
            Message::Macro(message) => self.macro_message(message.clone()),
            Message::NoteOpened(result) => {
                self.tell(match result {
                    Ok(path) => Notice::NoteOpened(
                        path.file_name()
                            .map(|name| name.to_string_lossy().into_owned())
                            .unwrap_or_default(),
                    ),
                    Err(reason) => Notice::NoteFailed(reason.clone()),
                });
                Vec::new()
            }
            Message::SettingsTransfer(message) => self.settings_transfer(message.clone()),
            Message::Gateways(message) => {
                self.gateways_message(message.clone());
                Vec::new()
            }
            Message::Broadcast(message) => self.broadcast_message(*message),
            _ => Vec::new(),
        }
    }

    /// The profiles and gateways as the C# session document, a local shell's arguments
    /// quoted as the terminal runs them.
    fn export_sessions(&self) -> Effect {
        let store = &self.store;
        Effect::SaveExport {
            document: heimdall_core::export::csharp(
                store,
                &|arguments| {
                    heimdall_term::local::windows_arguments(&local_tab::term_arguments(arguments))
                },
                &self.settings.rdp_defaults,
            ),
            count: heimdall_core::export::session_count(store),
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
        let mut report = match csharp::import(&servers, settings.as_deref()) {
            Ok(report) => report,
            Err(error) => {
                self.dialog = Some(Dialog::ImportFailed {
                    detail: error.to_string(),
                });
                return;
            }
        };
        let host_keys = std::mem::take(&mut report.host_keys);
        // Saved before it is kept: a failed save leaves the list as its file is.
        if let Some(mut summary) = self.merge_import(report) {
            // This computer's own C# store: its trusted servers are trusted here too.
            summary.host_keys = Some(
                heimdall_ssh::carry_over(
                    &heimdall_ssh::KnownHosts::new(&self.config.known_hosts),
                    &host_keys,
                )
                .map_err(|error| error.to_string()),
            );
            self.dialog = Some(Dialog::ImportDone(summary));
        }
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
            question.name = server_prompt_text(&question.name);
            question.instructions = server_prompt_text(&question.instructions);
            question.prompts = question
                .prompts
                .into_iter()
                .map(|prompt| KeyboardInteractivePrompt {
                    text: server_prompt_text(&prompt.text),
                    echo: prompt.echo,
                })
                .collect();
            QuestionKind::KeyboardInteractive(question)
        }
        other => other,
    }
}
