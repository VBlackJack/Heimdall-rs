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
use heimdall_rdp::{
    CopyRefusal, Ending, Fingerprint, Framebuffer, LocalClipboard, Operation, SaveEnd,
};
use heimdall_remote::vnc::{Framebuffer as VncFramebuffer, VncInput};
use heimdall_tls::ValidationIssue;
use tokio::sync::{mpsc, watch};
use tokio_util::sync::CancellationToken;
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

/// What the FTPS certificate question shows of the certificate beside its subject, as the C#
/// prompt: its issuer, when it holds, and why the system did not vouch for it; and whether
/// it renews, on the same key, a certificate the user trusted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CertificateDetails {
    /// The issuer, made safe to show.
    pub issuer: String,
    /// When it holds.
    pub validity: heimdall_rdp::Validity,
    /// Why this computer's certificate authorities did not vouch for it.
    pub issue: ValidationIssue,
    /// The hash of the whole certificate: what the user's answer trusts, this very
    /// certificate and no other on the same key.
    pub certificate: heimdall_rdp::CertificateHash,
    /// Set when the server's key is the one trusted, its certificate another: renewed, or
    /// minted again by whoever holds the key.
    pub renewal: Option<Renewal>,
}

/// A certificate presented on a key trusted, with another certificate on record.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Renewal {
    /// When the certificate on record holds, when recorded.
    pub recorded: Option<heimdall_rdp::Validity>,
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
        /// The certificate's subject, made safe, when it was read.
        subject: Option<String>,
        /// Its issuer, validity and validation issue, for an FTPS server, as the C# FTPS
        /// prompt shows them; the RDP question shows none.
        details: Option<Box<CertificateDetails>>,
    },
    /// The RDP session is open.
    RdpReady {
        /// The desktop, drawn by the UI.
        framebuffer: Framebuffer,
        /// Where keyboard and mouse input goes.
        input: mpsc::UnboundedSender<Vec<Operation>>,
        /// The desktop size the tab wants.
        size: watch::Sender<Option<(u16, u16)>>,
        /// Where this side's clipboard goes, when the clipboard is shared.
        clipboard: Option<mpsc::UnboundedSender<LocalClipboard>>,
    },
    /// The server copied this text: it goes to this side's clipboard.
    RemoteClipboard(Zeroizing<String>),
    /// The server copied this image, a device-independent bitmap: it goes to this side's
    /// clipboard.
    RemoteImage(std::sync::Arc<[u8]>),
    /// The SSH shell's connection, held without keeping it open: its server health is
    /// asked over it.
    SshConnection(heimdall_ssh::WeakConnection),
    /// The shell's profile forwards X11 and no X server could be found or started: the
    /// shell opens without X11 forwarding.
    X11ServerNotFound,
    /// The files copied on this side were not offered to the RDP server.
    RdpFilesRefused(CopyRefusal),
    /// The RDP server's clipboard holds files to save here, or no longer.
    RdpRemoteFiles(bool),
    /// Saving the RDP server's files: this many entries of all are saved so far.
    RdpSaveProgress {
        /// Entries saved.
        saved: usize,
        /// Entries in the copy.
        total: usize,
    },
    /// Saving the RDP server's files ended.
    RdpSaveEnded(SaveEnd),
    /// The VNC session is open.
    VncReady {
        /// The desktop's name, as the server gives it: untrusted.
        name: String,
        /// The desktop, drawn by the UI.
        framebuffer: VncFramebuffer,
        /// Where keyboard and mouse input goes.
        input: VncInput,
        /// The TLS version the session is encrypted with, as "TLS 1.3"; `None` in clear.
        tls: Option<&'static str>,
    },
    /// The RDP desktop changed: redraw it.
    DesktopFrame,
    /// The VNC desktop's name changed, as its server says: untrusted text.
    DesktopRenamed(String),
    /// The server cannot change the desktop's size while connected: only a new
    /// connection at that size brings it.
    DesktopResizeRefused {
        /// Width asked.
        width: u16,
        /// Height asked.
        height: u16,
    },
    /// The shell is open.
    Connected {
        /// Where input goes.
        input: Arc<dyn InputSink>,
    },
    /// The file session is open (a Files attempt).
    FilesReady {
        /// The session.
        client: RemoteSession,
        /// The SSH connection under an SFTP session, to run commands on the server; none
        /// for FTP.
        shell: Option<heimdall_ssh::Connection>,
    },
    /// Output from the shell.
    Output(Vec<u8>),
    /// The shell ended. Last event of the attempt.
    Closed {
        /// Exit status, when reported.
        exit_status: Option<u32>,
    },
    /// The server ended the session and said why, its own words made safe. Last event of
    /// the attempt.
    Ended {
        /// Its reason.
        reason: Ending,
    },
    /// The attempt failed. Last event of the attempt.
    Failed(UiError),
    /// A step of the post-connect sequence moved.
    PostConnect(PostConnectProgress),
    /// The post-connect sequence is over, run whole or stopped.
    PostConnectDone,
}

/// Where the post-connect sequence stands.
#[derive(Debug, Clone)]
pub struct PostConnectProgress {
    /// The step, from 1.
    pub step: usize,
    /// How many steps there are.
    pub total: usize,
    /// What the step types, made safe.
    pub command: String,
    /// What became of it.
    pub status: StepStatus,
    /// Stops the sequence.
    pub stop: CancellationToken,
}

/// What became of a post-connect step, as the C# statuses.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StepStatus {
    /// Waiting for its delay, then typed.
    Running,
    /// Typed.
    Completed,
    /// Could not be typed: the session had ended.
    Failed,
    /// Off, or nothing to type.
    Skipped,
    /// The sequence was stopped before it.
    Cancelled,
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
                ..
            } => f
                .debug_struct("UnknownRdpCertificate")
                .field("host", host)
                .field("port", port)
                .field("fingerprint", &fingerprint.to_string())
                .finish(),
            Self::RdpReady { .. } => f.write_str("RdpReady"),
            // What was copied can be a password: never shown.
            Self::RemoteClipboard(_) => f.write_str("RemoteClipboard(..)"),
            Self::RemoteImage(image) => write!(f, "RemoteImage({})", image.len()),
            Self::SshConnection(_) => f.write_str("SshConnection"),
            Self::X11ServerNotFound => f.write_str("X11ServerNotFound"),
            Self::RdpFilesRefused(refusal) => write!(f, "RdpFilesRefused({refusal:?})"),
            Self::RdpRemoteFiles(available) => write!(f, "RdpRemoteFiles({available})"),
            Self::RdpSaveProgress { saved, total } => {
                write!(f, "RdpSaveProgress({saved}/{total})")
            }
            Self::RdpSaveEnded(end) => write!(f, "RdpSaveEnded({end:?})"),
            Self::VncReady { .. } => f.write_str("VncReady"),
            Self::DesktopFrame => f.write_str("DesktopFrame"),
            Self::DesktopRenamed(_) => f.write_str("DesktopRenamed(..)"),
            Self::DesktopResizeRefused { width, height } => {
                write!(f, "DesktopResizeRefused({width}x{height})")
            }
            Self::Connected { .. } => f.write_str("Connected"),
            Self::FilesReady { .. } => f.write_str("FilesReady"),
            Self::Output(bytes) => write!(f, "Output({} bytes)", bytes.len()),
            Self::Closed { exit_status } => f
                .debug_struct("Closed")
                .field("exit_status", exit_status)
                .finish(),
            Self::Ended { reason } => f.debug_struct("Ended").field("reason", reason).finish(),
            Self::Failed(error) => f.debug_tuple("Failed").field(error).finish(),
            Self::PostConnect(progress) => f
                .debug_struct("PostConnect")
                .field("step", &progress.step)
                .field("total", &progress.total)
                .field("status", &progress.status)
                .finish_non_exhaustive(),
            Self::PostConnectDone => f.write_str("PostConnectDone"),
        }
    }
}
