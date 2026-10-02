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

//! The application core of Heimdall, independent of any UI toolkit.
//!
//! [`App::update`] applies a [`Message`] and returns [`Effect`]s: connecting, answering a
//! question, using the clipboard, waking at a deadline, quitting. The UI layer draws the
//! state and carries out the effects; everything the application decides is testable by
//! calling `update`.

mod app;
pub mod credential_provider;
mod desktop;
mod driver;
mod error;
mod event;
pub mod files;
pub mod ftp_driver;
pub mod gateway_draft;
mod ids;
pub mod keyboard_layout;
pub mod local_draft;
pub mod local_driver;
mod paste_guard;
mod post_connect;
pub mod profile_draft;
pub mod putty_store;
pub mod rdp_driver;
mod sink;
pub mod steps_draft;
pub mod telnet_driver;
mod text;
pub mod time_zone;
pub mod transcript;
pub mod tunnel;
pub mod tunnel_driver;
pub mod vnc_driver;
pub mod winrm_driver;
pub mod winrm_preflight;

pub use app::{
    App, AppConfig, BroadcastMessage, ConflictRow, ConnectAs, Dialog, Effect, ExportOutcome,
    FileKind, FilesMessage, FilterMessage, FolderMessage, FolderNaming, GatewayBadge, HostKeyRow,
    HostKeysMessage, HostKeysOutcome, HostKeysPreview, ImportFile, ImportSummary, KeyInput,
    LONG_MASTER_PASSWORD_CHARS, LocalConfirmation, MIN_MASTER_PASSWORD_CHARS,
    MIN_MASTER_PASSWORD_CLASSES, Message, NO_FOLDER, NameAction, Notice, OpenedVault,
    PendingImport, Phase, PinDialog, PinFailure, PinMessage, PinMode, PointerInput,
    PostConnectConfirmation, ProfileCopy, ProfileKind, ProfileMenuMessage, ProfileSummary, Prompt,
    ProviderAnswer, ProviderMessage, ProviderRequest, QuickResult, RDP_EXTENSION, RDP_MAX_ATTEMPTS,
    RdpMessage, RdpNames, RdpOutcome, RdpPreview, RdpRow, Retry, SelectionMessage, SessionState,
    SessionStatus, SessionsCounts, SessionsMessage, SessionsPreview, SessionsRow, SessionsSource,
    SettingsMessage, SystemCredentials, Tab, TabGroup, TabMenuMessage, TabProfile, TreeFilter,
    TreeRow, TrustedKey, TrustedKeys, TrustedKeysMessage, TunnelMessage, UNLOCK_SECRET_ENTRY,
    VAULT_FILE_NAME, VaultDialog, VaultJob, VaultMode, VaultProblem, VaultStatus, WHEEL_LINES,
    master_password_problem, open_vault,
};
pub use desktop::{DesktopFramebuffer, DesktopInput, DesktopPane, PointerButton, SpecialKeys};
pub use driver::{AnswerRegistry, ConnectRequest, Purpose, connection_events};
pub use error::{KeyProblem, NetworkFailure, ServerAddress, UiError};
pub use event::{
    Answer, ConnectionEvent, PostConnectProgress, QuestionKind, ServerPasswordQuestion, StepStatus,
};
pub use ids::{AttemptId, QuestionId, TabId};
pub use sink::InputSink;
pub use text::{MAX_SERVER_TEXT_CHARS, server_text, visible_text};
