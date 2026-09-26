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

use heimdall_core::profile::{SshProfile, display_address};
use heimdall_ssh::{
    ConnectError, ConnectOptions, KeyboardInteractiveQuestion, PassphraseQuestion,
    PasswordQuestion, Prompter, Secret, SessionEvent, ShellSession, UsernameQuestion, connect,
    fingerprint,
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

struct ChannelPrompter {
    events: mpsc::Sender<ConnectionEvent>,
    registry: AnswerRegistry,
}

impl ChannelPrompter {
    async fn ask(&self, kind: QuestionKind) -> Option<Answer> {
        let question = QuestionId::fresh();
        let (sender, receiver) = oneshot::channel();
        self.registry.insert(question, sender);
        let _pending = Pending {
            registry: self.registry.clone(),
            question,
        };
        if self
            .events
            .send(ConnectionEvent::Question { question, kind })
            .await
            .is_err()
        {
            return None;
        }
        receiver.await.ok().flatten()
    }
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

/// What an attempt needs.
#[derive(Debug, Clone)]
pub struct ConnectRequest {
    /// Destination.
    pub profile: SshProfile,
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
    let result = connect(
        &request.profile,
        &request.options,
        prompter,
        request.cancel.clone(),
    )
    .await;
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
