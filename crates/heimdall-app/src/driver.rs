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

//! Runs one connection attempt and reports it as a single ordered stream of events.
//!
//! Questions, the connection result, output and the end all travel through one channel, so
//! a question can never be seen after the attempt it belongs to has ended.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use heimdall_core::profile::{SshProfile, display_address};
use heimdall_sftp::{ClientConfig, SftpClient};
use heimdall_ssh::{
    ConnectError, ConnectOptions, KeyboardInteractiveQuestion, PassphraseQuestion,
    PasswordQuestion, Prompter, Secret, SessionEvent, ShellSession, UsernameQuestion,
    establish_via, fingerprint,
};
use tokio::sync::{mpsc, oneshot};
use tokio_stream::wrappers::ReceiverStream;
use tokio_util::sync::CancellationToken;

use crate::error::UiError;
use crate::event::{Answer, ConnectionEvent, QuestionKind};
use crate::ids::QuestionId;
use crate::sink::InputSink;

/// Events buffered before the attempt waits for the UI to read them.
const EVENT_QUEUE_LENGTH: usize = 64;

/// Pending questions, answered by the UI.
#[derive(Clone, Default)]
pub struct AnswerRegistry(Arc<Mutex<HashMap<QuestionId, oneshot::Sender<Option<Answer>>>>>);

impl std::fmt::Debug for AnswerRegistry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let pending = self.0.lock().map_or(0, |map| map.len());
        f.debug_struct("AnswerRegistry")
            .field("pending", &pending)
            .finish()
    }
}

impl AnswerRegistry {
    /// Delivers an answer; `None` cancels. `false` when nobody waits for it any more.
    #[must_use]
    pub fn answer(&self, question: QuestionId, answer: Option<Answer>) -> bool {
        let sender = self.0.lock().ok().and_then(|mut map| map.remove(&question));
        sender.is_some_and(|sender| sender.send(answer).is_ok())
    }

    /// Number of questions waiting.
    #[must_use]
    pub fn pending(&self) -> usize {
        self.0.lock().map_or(0, |map| map.len())
    }

    fn insert(&self, question: QuestionId, sender: oneshot::Sender<Option<Answer>>) {
        if let Ok(mut map) = self.0.lock() {
            map.insert(question, sender);
        }
    }

    fn remove(&self, question: QuestionId) {
        if let Ok(mut map) = self.0.lock() {
            map.remove(&question);
        }
    }
}

/// Removes a question from the registry however the wait ends, so an abandoned question
/// does not linger.
struct Pending {
    registry: AnswerRegistry,
    question: QuestionId,
}

impl Drop for Pending {
    fn drop(&mut self) {
        self.registry.remove(self.question);
    }
}

/// Answers SSH questions by asking the user through an attempt's events.
pub(crate) struct ChannelPrompter {
    pub(crate) events: mpsc::Sender<ConnectionEvent>,
    pub(crate) registry: AnswerRegistry,
}

impl ChannelPrompter {
    async fn ask(&self, kind: QuestionKind) -> Option<Answer> {
        ask(&self.registry, &self.events, kind).await
    }
}

/// Asks the user through `events` and waits for the answer; `None` when cancelled or when
/// nobody listens any more.
pub(crate) async fn ask(
    registry: &AnswerRegistry,
    events: &mpsc::Sender<ConnectionEvent>,
    kind: QuestionKind,
) -> Option<Answer> {
    let question = QuestionId::fresh();
    let (sender, receiver) = oneshot::channel();
    registry.insert(question, sender);
    let _pending = Pending {
        registry: registry.clone(),
        question,
    };
    if events
        .send(ConnectionEvent::Question { question, kind })
        .await
        .is_err()
    {
        return None;
    }
    receiver.await.ok().flatten()
}

impl Prompter for ChannelPrompter {
    async fn username(&self, question: UsernameQuestion) -> Option<String> {
        match self.ask(QuestionKind::Username(question)).await? {
            Answer::Text(text) => Some(text),
            Answer::Secret(_) | Answer::Secrets(_) => None,
        }
    }

    async fn password(&self, question: PasswordQuestion) -> Option<Secret> {
        match self.ask(QuestionKind::Password(question)).await? {
            Answer::Secret(secret) => Some(secret),
            Answer::Text(_) | Answer::Secrets(_) => None,
        }
    }

    async fn key_passphrase(&self, question: PassphraseQuestion) -> Option<Secret> {
        match self.ask(QuestionKind::Passphrase(question)).await? {
            Answer::Secret(secret) => Some(secret),
            Answer::Text(_) | Answer::Secrets(_) => None,
        }
    }

    async fn keyboard_interactive(
        &self,
        question: KeyboardInteractiveQuestion,
    ) -> Option<Vec<Secret>> {
        match self
            .ask(QuestionKind::KeyboardInteractive(question))
            .await?
        {
            Answer::Secrets(secrets) => Some(secrets),
            Answer::Text(_) | Answer::Secret(_) => None,
        }
    }
}

/// What a connection is for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Purpose {
    /// An interactive shell.
    Shell,
    /// An SFTP session.
    Files,
    /// A remote desktop over RDP.
    Rdp,
    /// A remote desktop over VNC.
    Vnc,
}

/// What an attempt needs.
#[derive(Debug, Clone)]
pub struct ConnectRequest {
    /// Destination.
    pub profile: SshProfile,
    /// The SSH gateways it is reached through, nearest first, each as the hop it is.
    pub route: Vec<SshProfile>,
    /// Shell or files.
    pub purpose: Purpose,
    /// Connection settings, initial terminal size included.
    pub options: ConnectOptions,
    /// Cancels the attempt and, once connected, the session.
    pub cancel: CancellationToken,
}

/// Starts an attempt on the current tokio runtime; its events arrive on the stream, which
/// ends after [`ConnectionEvent::Closed`] or [`ConnectionEvent::Failed`] (or after
/// [`ConnectionEvent::UnknownHostKey`], which also ends the attempt).
#[must_use]
pub fn connection_events(
    request: ConnectRequest,
    registry: AnswerRegistry,
) -> ReceiverStream<ConnectionEvent> {
    let (events, receiver) = mpsc::channel(EVENT_QUEUE_LENGTH);
    tokio::spawn(run(request, registry, events));
    ReceiverStream::new(receiver)
}

async fn run(
    request: ConnectRequest,
    registry: AnswerRegistry,
    events: mpsc::Sender<ConnectionEvent>,
) {
    // Destinations come from the user's own profiles; errors are logged in their `Debug`
    // form, which escapes any control character a server message may carry.
    let target = display_address(&request.profile.host, request.profile.port);
    log::info!("connecting to {target}");
    let prompter = Arc::new(ChannelPrompter {
        events: events.clone(),
        registry,
    });
    if request.purpose == Purpose::Files {
        open_files(&request, prompter, &events, &target).await;
        return;
    }
    let result = match establish_via(
        &request.route,
        &request.profile,
        &request.options,
        prompter,
        request.cancel.clone(),
    )
    .await
    {
        Ok(connection) => {
            connection
                .open_shell(&request.options, request.cancel.clone())
                .await
        }
        Err(error) => Err(error),
    };
    let session = match result {
        Ok(session) => session,
        Err(ConnectError::UnknownHostKey { host, port, key }) => {
            let fingerprint = fingerprint(&key);
            log::info!("{target} presented an unknown host key {fingerprint}");
            let _ = events
                .send(ConnectionEvent::UnknownHostKey {
                    host,
                    port,
                    fingerprint,
                    key: Arc::from(key),
                })
                .await;
            return;
        }
        Err(error) => {
            let error = UiError::from(error);
            if error == UiError::Cancelled {
                log::info!("connection to {target} cancelled");
            } else {
                log::warn!("connection to {target} failed: {error:?}");
            }
            let _ = events.send(ConnectionEvent::Failed(error)).await;
            return;
        }
    };
    let ShellSession {
        input,
        events: mut session_events,
    } = session;
    let sink: Arc<dyn InputSink> = Arc::new(input);
    log::info!("session open to {target}");
    if events
        .send(ConnectionEvent::Connected {
            input: sink.clone(),
        })
        .await
        .is_err()
    {
        sink.close();
        return;
    }
    while let Some(event) = session_events.recv().await {
        let event = match event {
            SessionEvent::Output(bytes) => ConnectionEvent::Output(bytes),
            SessionEvent::Closed { exit_status } => {
                log::info!("session to {target} ended, exit status {exit_status:?}");
                ConnectionEvent::Closed { exit_status }
            }
        };
        let last = matches!(event, ConnectionEvent::Closed { .. });
        if events.send(event).await.is_err() {
            // Nobody listens any more: the tab is gone.
            sink.close();
            return;
        }
        if last {
            return;
        }
    }
}

/// Name of the SFTP subsystem.
const SFTP_SUBSYSTEM: &str = "sftp";

/// Longest wait for the server to accept the SFTP subsystem.
const SUBSYSTEM_TIMEOUT: Duration = Duration::from_secs(20);

/// Reports how an attempt ended before its session opened.
pub(crate) async fn report_failure(
    error: ConnectError,
    events: &mpsc::Sender<ConnectionEvent>,
    target: &str,
) {
    if let ConnectError::UnknownHostKey { host, port, key } = error {
        let fingerprint = fingerprint(&key);
        log::info!("{target} presented an unknown host key {fingerprint}");
        let _ = events
            .send(ConnectionEvent::UnknownHostKey {
                host,
                port,
                fingerprint,
                key: Arc::from(key),
            })
            .await;
        return;
    }
    let error = UiError::from(error);
    if error == UiError::Cancelled {
        log::info!("connection to {target} cancelled");
    } else {
        log::warn!("connection to {target} failed: {error:?}");
    }
    let _ = events.send(ConnectionEvent::Failed(error)).await;
}

/// Authenticates, opens the SFTP subsystem and starts a session on it.
async fn open_files(
    request: &ConnectRequest,
    prompter: Arc<ChannelPrompter>,
    events: &mpsc::Sender<ConnectionEvent>,
    target: &str,
) {
    let connection = match establish_via(
        &request.route,
        &request.profile,
        &request.options,
        prompter,
        request.cancel.clone(),
    )
    .await
    {
        Ok(connection) => connection,
        Err(error) => return report_failure(error, events, target).await,
    };
    let stream = match connection
        .open_subsystem(SFTP_SUBSYSTEM, SUBSYSTEM_TIMEOUT)
        .await
    {
        Ok(stream) => stream,
        Err(error) => return report_failure(error, events, target).await,
    };
    // The session holds the stream, which holds the connection: dropping the session
    // closes both.
    drop(connection);
    match SftpClient::start(stream, ClientConfig::default()).await {
        Ok(client) => {
            log::info!("SFTP session open to {target}");
            let _ = events.send(ConnectionEvent::FilesReady { client }).await;
        }
        Err(error) => {
            log::warn!("SFTP session to {target} failed: {error:?}");
            let _ = events
                .send(ConnectionEvent::Failed(UiError::Protocol {
                    detail: error.to_string(),
                }))
                .await;
        }
    }
}
