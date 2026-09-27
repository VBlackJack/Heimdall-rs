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

//! What one connection attempt reports, in order, as cloneable data.

use std::fmt;
use std::sync::Arc;

use heimdall_ssh::{
    KeyboardInteractiveQuestion, PassphraseQuestion, PasswordQuestion, PublicKey, Secret,
    UsernameQuestion,
};

use heimdall_files::RemoteSession;
use heimdall_rdp::{Fingerprint, Framebuffer, Operation};
use heimdall_remote::vnc::{Framebuffer as VncFramebuffer, VncInput};
use tokio::sync::{mpsc, watch};
use zeroize::Zeroizing;

use crate::error::UiError;
use crate::ids::QuestionId;
use crate::sink::InputSink;

/// A question the connection needs answered.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum QuestionKind {
    /// The profile has no user name.
    Username(UsernameQuestion),
    /// Password of the account.
    Password(PasswordQuestion),
    /// Passphrase of the key file.
    Passphrase(PassphraseQuestion),
    /// A keyboard-interactive round; its texts come from the server.
    KeyboardInteractive(KeyboardInteractiveQuestion),
    /// The password of a server with no account, as VNC has.
    ServerPassword(ServerPasswordQuestion),
}

/// The password of a server with no account.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServerPasswordQuestion {
    /// Host.
    pub host: String,
    /// Port.
    pub port: u16,
}

/// The user's answer to a question.
#[derive(Clone)]
pub enum Answer {
    /// A user name.
    Text(String),
    /// A password or passphrase.
    Secret(Secret),
    /// Keyboard-interactive answers, one per prompt.
    Secrets(Vec<Secret>),
}

impl fmt::Debug for Answer {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Text(_) => f.write_str("Answer::Text(..)"),
            Self::Secret(_) => f.write_str("Answer::Secret(..)"),
            Self::Secrets(answers) => write!(f, "Answer::Secrets({} answers)", answers.len()),
        }
    }
}

/// One step of a connection attempt.
#[derive(Clone)]
pub enum ConnectionEvent {
    /// A question for the user; answer it through the registry, with the same identifier.
    Question {
        /// Identifier to answer with.
        question: QuestionId,
        /// What is asked.
        kind: QuestionKind,
    },
    /// The server's key is not recorded: the attempt stopped, the user decides.
    UnknownHostKey {
        /// Host, normalised.
        host: String,
        /// Port.
        port: u16,
        /// SHA-256 fingerprint.
        fingerprint: String,
        /// The key, to record if the user accepts it.
        key: Arc<PublicKey>,
    },
    /// The RDP server's certificate is not recorded: the attempt stopped, the user decides.
    UnknownRdpCertificate {
        /// Host.
        host: String,
        /// Port.
        port: u16,
        /// SHA-256 of the certificate's public key.
        fingerprint: Fingerprint,
    },
    /// The RDP session is open.
    RdpReady {
        /// The desktop, drawn by the UI.
        framebuffer: Framebuffer,
        /// Where keyboard and mouse input goes.
        input: mpsc::UnboundedSender<Vec<Operation>>,
        /// The desktop size the tab wants.
        size: watch::Sender<Option<(u16, u16)>>,
        /// Where this side's clipboard text goes, when the clipboard is shared.
        clipboard: Option<mpsc::UnboundedSender<Zeroizing<String>>>,
    },
    /// The server copied this text: it goes to this side's clipboard.
    RemoteClipboard(Zeroizing<String>),
    /// The VNC session is open.
    VncReady {
        /// The desktop, drawn by the UI.
        framebuffer: VncFramebuffer,
        /// Where keyboard and mouse input goes.
        input: VncInput,
    },
    /// The RDP desktop changed: redraw it.
    DesktopFrame,
    /// The shell is open.
    Connected {
        /// Where input goes.
        input: Arc<dyn InputSink>,
    },
    /// The file session is open (a Files attempt).
    FilesReady {
        /// The session.
        client: RemoteSession,
    },
    /// Output from the shell.
    Output(Vec<u8>),
    /// The shell ended. Last event of the attempt.
    Closed {
        /// Exit status, when reported.
        exit_status: Option<u32>,
    },
    /// The server ended the session and said why, made safe. Last event of the attempt.
    Ended {
        /// Its reason.
        reason: String,
    },
    /// The attempt failed. Last event of the attempt.
    Failed(UiError),
}

impl fmt::Debug for ConnectionEvent {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Question { question, kind } => f
                .debug_struct("Question")
                .field("question", question)
                .field("kind", &std::mem::discriminant(kind))
                .finish(),
            Self::UnknownHostKey {
                host,
                port,
                fingerprint,
                ..
            } => f
                .debug_struct("UnknownHostKey")
                .field("host", host)
                .field("port", port)
                .field("fingerprint", fingerprint)
                .finish(),
            Self::UnknownRdpCertificate {
                host,
                port,
                fingerprint,
            } => f
                .debug_struct("UnknownRdpCertificate")
                .field("host", host)
                .field("port", port)
                .field("fingerprint", &fingerprint.to_string())
                .finish(),
            Self::RdpReady { .. } => f.write_str("RdpReady"),
            // What was copied can be a password: never shown.
            Self::RemoteClipboard(_) => f.write_str("RemoteClipboard(..)"),
            Self::VncReady { .. } => f.write_str("VncReady"),
            Self::DesktopFrame => f.write_str("DesktopFrame"),
            Self::Connected { .. } => f.write_str("Connected"),
            Self::FilesReady { .. } => f.write_str("FilesReady"),
            Self::Output(bytes) => write!(f, "Output({} bytes)", bytes.len()),
            Self::Closed { exit_status } => f
                .debug_struct("Closed")
                .field("exit_status", exit_status)
                .finish(),
            Self::Ended { reason } => f.debug_struct("Ended").field("reason", reason).finish(),
            Self::Failed(error) => f.debug_tuple("Failed").field(error).finish(),
        }
    }
}
