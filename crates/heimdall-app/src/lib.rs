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
mod desktop;
mod driver;
mod error;
mod event;
pub mod files;
pub mod gateway_draft;
mod ids;
pub mod local_driver;
pub mod profile_draft;
pub mod rdp_driver;
mod sink;
pub mod telnet_driver;
mod text;
pub mod vnc_driver;

pub use app::{
    App, AppConfig, ConnectAs, Dialog, Effect, FilesMessage, FolderMessage, FolderNaming,
    GatewayBadge, ImportSummary, KeyInput, LONG_MASTER_PASSWORD_CHARS, LocalConfirmation,
    MIN_MASTER_PASSWORD_CHARS, MIN_MASTER_PASSWORD_CLASSES, Message, NO_FOLDER, NameAction,
    OpenedVault, Phase, PointerInput, ProfileCopy, ProfileKind, ProfileMenuMessage, ProfileSummary,
    Prompt, RDP_MAX_ATTEMPTS, Retry, SelectionMessage, SystemCredentials, Tab, TabGroup,
    TabMenuMessage, TabProfile, TreeRow, VAULT_FILE_NAME, VaultDialog, VaultJob, VaultMode,
    VaultProblem, VaultStatus, WHEEL_LINES, master_password_problem, open_vault,
};
pub use desktop::{DesktopFramebuffer, DesktopInput, DesktopPane, PointerButton, SpecialKeys};
pub use driver::{AnswerRegistry, ConnectRequest, Purpose, connection_events};
pub use error::{KeyProblem, NetworkFailure, ServerAddress, UiError};
pub use event::{Answer, ConnectionEvent, QuestionKind, ServerPasswordQuestion};
pub use ids::{AttemptId, QuestionId, TabId};
pub use sink::InputSink;
pub use text::{MAX_SERVER_TEXT_CHARS, server_text, visible_text};
