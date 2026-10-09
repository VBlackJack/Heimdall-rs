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

use heimdall_core::post_connect::PostConnectStep;
use heimdall_core::profile::{Forwards, SshProfile, display_address};
use heimdall_files::RemoteSession;
use heimdall_sftp::{ClientConfig, SftpClient};
use heimdall_ssh::RemoteForward;
use heimdall_ssh::socks::{self, Proxy};
use heimdall_ssh::{
    ConnectError, ConnectOptions, Connection, KeyboardInteractiveQuestion, PassphraseQuestion,
    PasswordQuestion, Prompter, Routed, Secret, SessionEvent, ShellSession, UsernameQuestion,
    X11Display, establish_via, establish_via_keeping_gateway, fingerprint,
};
use tokio::sync::{mpsc, oneshot};
use tokio_stream::wrappers::ReceiverStream;
use tokio_util::sync::CancellationToken;

use crate::error::UiError;
use crate::event::{Answer, ConnectionEvent, QuestionKind};
use crate::ids::QuestionId;
use crate::post_connect;
use crate::sink::InputSink;
use crate::x11_server::{self, X11Outcome, X11Settings};

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
    /// A Citrix application's status: nothing is connected.
    Citrix,
    /// A built-in tool: nothing is connected.
    Tool,
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
    /// The X server to count on when the shell forwards X11; `None` forwards nothing.
    pub x11: Option<X11Settings>,
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
    let result = match establish_via_keeping_gateway(
        &request.route,
        &request.profile,
        &request.options,
        prompter,
        request.cancel.clone(),
    )
    .await
    {
        Ok(Routed { server, gateway }) => {
            let weak = server.downgrade();
            let options = shell_options(&request, &events).await;
            match open_forwards(request.profile.forwards, gateway.as_ref()).await {
                Ok(forwards) => server
                    .open_shell(&options, request.cancel.clone())
                    .await
                    .map(|session| (session, forwards, gateway, weak)),
                Err(error) => Err(error),
            }
        }
        Err(error) => Err(error),
    };
    // The forwards and the gateway, whose drop would disconnect it and the server carried
    // over it, last as long as the session: they go when this attempt returns.
    let (session, _forwards, _gateway, weak) = match result {
        Ok(opened) => opened,
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
    // The server health panel asks over the session's own connection, never keeping it.
    let _ = events.send(ConnectionEvent::SshConnection(weak)).await;
    // The steps stop with the session: when this attempt returns, or when the tab asks.
    let steps = request.profile.post_connect.to_run();
    let stop = request.cancel.child_token();
    let _stop_with_session = stop.clone().drop_guard();
    if steps.iter().any(PostConnectStep::runs) {
        tokio::spawn(post_connect::run(steps, sink.clone(), events.clone(), stop));
    }
    while let Some(event) = session_events.recv().await {
        let event = session_event(event, &target);
        let last = matches!(
            event,
            ConnectionEvent::Closed { .. } | ConnectionEvent::Failed(_)
        );
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

/// The options the shell opens with: X11 forwarded to a display when its profile asks and
/// one is there; without one, the shell opens without X11, and the tab is told.
async fn shell_options(
    request: &ConnectRequest,
    events: &mpsc::Sender<ConnectionEvent>,
) -> ConnectOptions {
    let mut options = request.options.clone();
    let Some(settings) = request.x11.clone() else {
        return options;
    };
    let own = std::env::var(x11_server::DISPLAY_VARIABLE).ok();
    let mut display = own_display(own.as_deref());
    if display.is_none() {
        // The X server is detected, and perhaps started and waited for: off the runtime.
        let outcome = tokio::task::spawn_blocking(move || {
            x11_server::shared().ensure(&settings, x11_server::DISPLAY_PORT)
        })
        .await
        .unwrap_or(X11Outcome::Unavailable);
        display = display_for(&outcome);
    }
    if display.is_none() {
        log::warn!("X11 forwarding asked but no X server is available: the shell opens without it");
        let _ = events.send(ConnectionEvent::X11ServerNotFound).await;
    }
    options.x11 = display;
    options
}

/// The display this application runs on, as `value` names it: on Unix, where `DISPLAY` is
/// the session's own; never on Windows, where an X server is counted on as for `PuTTY`.
fn own_display(value: Option<&str>) -> Option<X11Display> {
    if cfg!(unix) {
        value.and_then(X11Display::parse)
    } else {
        None
    }
}

/// The display of the X server counted on, as `PuTTY` is given it; none without one.
fn display_for(outcome: &X11Outcome) -> Option<X11Display> {
    if outcome.available() {
        X11Display::parse(x11_server::DISPLAY)
    } else {
        None
    }
}

/// What a shell's session says, for the tab of `target`: a connection lost under it is a
/// failure, which an auto-reconnect may take back.
fn session_event(event: SessionEvent, target: &str) -> ConnectionEvent {
    match event {
        SessionEvent::Output(bytes) => ConnectionEvent::Output(bytes),
        SessionEvent::Closed { exit_status } => {
            log::info!("session to {target} ended, exit status {exit_status:?}");
            ConnectionEvent::Closed { exit_status }
        }
        SessionEvent::Lost => {
            log::info!("connection to {target} lost under the session");
            ConnectionEvent::Failed(UiError::ConnectionLost)
        }
    }
}

/// The ports a session opened through its gateway; each closes when this is dropped.
#[derive(Debug, Default)]
pub(crate) struct OpenForwards {
    _proxy: Option<Proxy>,
    _remote: Option<RemoteForward>,
}

/// Opens the ports of `forwards` through `gateway`, when the profile goes through one: the
/// SOCKS proxy, then the remote forward.
///
/// # Errors
///
/// [`ConnectError::ProxyPort`] when the proxy's port cannot be taken,
/// [`ConnectError::RemoteForwardRefused`] when the gateway will not listen on its port.
pub(crate) async fn open_forwards(
    forwards: Forwards,
    gateway: Option<&Connection>,
) -> Result<OpenForwards, ConnectError> {
    let Some(gateway) = gateway else {
        return Ok(OpenForwards::default());
    };
    let proxy = match forwards.socks_port {
        None => None,
        Some(port) => match socks::start(port, Arc::new(gateway.clone())).await {
            Ok(proxy) => {
                log::info!("SOCKS5 proxy listening on {}", proxy.address());
                Some(proxy)
            }
            Err(source) => return Err(ConnectError::ProxyPort { port, source }),
        },
    };
    let remote = match forwards.remote() {
        None => None,
        Some((port, local)) => {
            let remote = gateway.forward_remote(port, local).await?;
            log::info!("remote forward: gateway port {port} to local port {local}");
            Some(remote)
        }
    };
    Ok(OpenForwards {
        _proxy: proxy,
        _remote: remote,
    })
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
    // The session holds the stream, which holds the connection: the connection ends with
    // the session and the tab's own clone, kept to run commands on the server.
    match SftpClient::start(stream, ClientConfig::default()).await {
        Ok(client) => {
            log::info!("SFTP session open to {target}");
            let client = RemoteSession::Sftp(client);
            let _ = events
                .send(ConnectionEvent::FilesReady {
                    client,
                    shell: Some(connection),
                })
                .await;
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

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use heimdall_ssh::X11Display;

    use super::{display_for, own_display};
    use crate::x11_server::{self, X11Outcome};

    #[test]
    fn without_an_x_server_counted_on_no_display_is_given() {
        assert_eq!(display_for(&X11Outcome::Unavailable), None);
        let expected = X11Display::parse(x11_server::DISPLAY);
        assert!(expected.is_some());
        assert_eq!(display_for(&X11Outcome::Running), expected);
        assert_eq!(
            display_for(&X11Outcome::Started(PathBuf::from("vcxsrv"))),
            expected
        );
    }

    #[test]
    fn the_session_s_own_display_is_used_on_unix_alone() {
        let own = own_display(Some(":1"));
        if cfg!(unix) {
            assert_eq!(own, X11Display::parse(":1"));
        } else {
            assert_eq!(own, None, "an X server is counted on, as for PuTTY");
        }
        assert_eq!(own_display(None), None);
        assert_eq!(own_display(Some("not a display")), None);
    }
}
