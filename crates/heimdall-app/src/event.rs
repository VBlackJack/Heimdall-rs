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

use heimdall_sftp::SftpClient;

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
    /// The shell is open.
    Connected {
        /// Where input goes.
        input: Arc<dyn InputSink>,
    },
    /// The SFTP session is open (a Files attempt).
    FilesReady {
        /// The session.
        client: SftpClient,
    },
    /// Output from the shell.
    Output(Vec<u8>),
    /// The shell ended. Last event of the attempt.
    Closed {
        /// Exit status, when reported.
        exit_status: Option<u32>,
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
            Self::Connected { .. } => f.write_str("Connected"),
            Self::FilesReady { .. } => f.write_str("FilesReady"),
            Self::Output(bytes) => write!(f, "Output({} bytes)", bytes.len()),
            Self::Closed { exit_status } => f
                .debug_struct("Closed")
                .field("exit_status", exit_status)
                .finish(),
            Self::Failed(error) => f.debug_tuple("Failed").field(error).finish(),
        }
    }
}
