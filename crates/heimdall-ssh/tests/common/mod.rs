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

//! In-process SSH server and scripted prompter for the heimdall-ssh tests.

#![allow(dead_code)]
// The test server answers synchronously; `async fn` keeps its handler readable next to the
// trait it implements, where `std::future::ready` wrappers would only add noise.
#![allow(clippy::unused_async_trait_impl)]

use std::borrow::Cow;
use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use heimdall_core::profile::{ProfileId, SshProfile};
use heimdall_ssh::{
    ConnectOptions, KeyboardInteractiveQuestion, PassphraseQuestion, PasswordQuestion, Prompter,
    Secret, UsernameQuestion,
};
use russh::keys::{Algorithm, PrivateKey, PublicKey};
use russh::server::{self, Auth, Msg, Response, Session};
use russh::{Channel, ChannelId, MethodKind, MethodSet, Preferred};
use tokio::net::TcpListener;

pub const USER: &str = "tester";
pub const PASSWORD: &str = "correct horse";
pub const FIXTURE_PASSPHRASE: &str = "fixture-passphrase";
pub const LOOPBACK: &str = "127.0.0.1";
/// Sent by a test to make the server end the shell with its configured exit status.
pub const EXIT_COMMAND: &[u8] = b"exit\n";

/// Subsystem the test server accepts.
pub const SUBSYSTEM_ACCEPTED: &str = "sftp";

/// Subsystem the test server never answers.
pub const SUBSYSTEM_SILENT: &str = "silent";
/// Upper bound for any single test step; generous, it only catches hangs.
pub const STEP_TIMEOUT: Duration = Duration::from_secs(60);
/// Server-side delay after a refused attempt; small so tests stay fast.
const AUTH_REJECTION_TIME: Duration = Duration::from_millis(10);

pub fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
}

pub fn client_key_path(name: &str) -> PathBuf {
    fixtures().join("keys").join(name)
}

pub fn client_public_key(name: &str) -> PublicKey {
    let text = std::fs::read_to_string(client_key_path(&format!("{name}.pub"))).expect("fixture");
    PublicKey::from_openssh(text.trim()).expect("valid public key")
}

pub fn host_private_key(name: &str) -> PrivateKey {
    let text = std::fs::read_to_string(fixtures().join("hostkeys").join(name)).expect("fixture");
    PrivateKey::from_openssh(text).expect("valid host key")
}

pub fn host_public_key(name: &str) -> PublicKey {
    host_private_key(name).public_key().clone()
}

/// One keyboard-interactive round: prompts with their echo flag and the expected answers.
#[derive(Clone, Debug, Default)]
pub struct KbdRound {
    pub prompts: Vec<(String, bool)>,
    pub expected: Vec<String>,
}

/// What the test server accepts and how it behaves.
#[derive(Clone, Debug)]
pub struct Spec {
    pub host_keys: Vec<&'static str>,
    pub methods: Vec<MethodKind>,
    pub password: Option<String>,
    pub kbd_rounds: Vec<KbdRound>,
    pub authorized: Vec<PublicKey>,
    pub max_auth_attempts: usize,
    pub exit_status: u32,
    /// Host key algorithms the server offers; also the signature algorithms it accepts.
    pub key_algorithms: Option<Vec<Algorithm>>,
    /// Closes a connection idle this long, authenticated or not, as OpenSSH's
    /// `LoginGraceTime` does before authentication.
    pub inactivity_timeout: Option<Duration>,
    /// Connects onward when a client asks, as a gateway does; off, as `AllowTcpForwarding no`.
    pub forwarding: bool,
}

impl Default for Spec {
    fn default() -> Self {
        Self {
            host_keys: vec!["host-ed25519"],
            methods: vec![
                MethodKind::PublicKey,
                MethodKind::KeyboardInteractive,
                MethodKind::Password,
            ],
            password: Some(PASSWORD.to_owned()),
            kbd_rounds: Vec::new(),
            authorized: Vec::new(),
            max_auth_attempts: 10,
            exit_status: 0,
            key_algorithms: None,
            inactivity_timeout: None,
            forwarding: false,
        }
    }
}

/// What the server observed, for assertions.
#[derive(Debug, Default, Clone)]
pub struct Observed {
    pub password_attempts: usize,
    /// Keys offered for public key authentication, accepted or not.
    pub publickey_offers: Vec<PublicKey>,
    /// Keys whose signature the server checked.
    pub publickey_checks: Vec<PublicKey>,
    pub kbd_answers: Vec<Vec<String>>,
    pub pty: Option<(String, u32, u32)>,
    pub resizes: Vec<(u32, u32)>,
    pub bytes_received: usize,
    /// Connections whose session ended, by disconnect or by the socket closing.
    pub connections_ended: usize,
    /// How each ended connection's session finished, as russh reported it.
    pub endings: Vec<String>,
    /// Where clients asked to be connected onward, accepted or not.
    pub forwards: Vec<(String, u32)>,
}

pub struct TestServer {
    pub port: u16,
    pub observed: Arc<Mutex<Observed>>,
}

pub async fn start(spec: Spec) -> TestServer {
    let listener = TcpListener::bind((LOOPBACK, 0)).await.expect("bind");
    let port = listener.local_addr().expect("address").port();
    let observed = Arc::new(Mutex::new(Observed::default()));
    let mut preferred = Preferred::default();
    if let Some(algorithms) = &spec.key_algorithms {
        preferred.key = Cow::Owned(algorithms.clone());
    }
    let config = Arc::new(server::Config {
        keys: spec
            .host_keys
            .iter()
            .map(|name| host_private_key(name))
            .collect(),
        methods: MethodSet::from(spec.methods.as_slice()),
        auth_rejection_time: AUTH_REJECTION_TIME,
        auth_rejection_time_initial: Some(Duration::ZERO),
        max_auth_attempts: spec.max_auth_attempts,
        inactivity_timeout: spec.inactivity_timeout,
        preferred,
        ..server::Config::default()
    });
    let spec = Arc::new(spec);
    let shared = observed.clone();
    tokio::spawn(async move {
        loop {
            let Ok((stream, _)) = listener.accept().await else {
                return;
            };
            let handler = Connection {
                spec: spec.clone(),
                observed: shared.clone(),
                kbd_round: 0,
                relayed: std::collections::HashSet::new(),
            };
            let config = config.clone();
            tokio::spawn(async move {
                let ended = handler.observed.clone();
                let outcome = match server::run_stream(config, stream, handler).await {
                    Ok(session) => format!("{:?}", session.await),
                    Err(error) => format!("setup: {error:?}"),
                };
                let mut observed = ended.lock().expect("observed");
                observed.connections_ended += 1;
                observed.endings.push(outcome);
            });
        }
    });
    TestServer { port, observed }
}

struct Connection {
    spec: Arc<Spec>,
    observed: Arc<Mutex<Observed>>,
    kbd_round: usize,
    /// Channels relayed onward: their data goes to the relay, never to the echo.
    relayed: std::collections::HashSet<ChannelId>,
}

fn reject() -> Auth {
    Auth::Reject {
        proceed_with_methods: None,
        partial_success: false,
    }
}

impl Connection {
    fn observe(&self, change: impl FnOnce(&mut Observed)) {
        change(&mut self.observed.lock().expect("observed"));
    }

    fn round(&self, index: usize) -> Auth {
        let round = &self.spec.kbd_rounds[index];
        Auth::Partial {
            name: Cow::Borrowed(""),
            instructions: Cow::Borrowed(""),
            prompts: Cow::Owned(
                round
                    .prompts
                    .iter()
                    .map(|(text, echo)| (Cow::Owned(text.clone()), *echo))
                    .collect(),
            ),
        }
    }

    /// Compares key material only: russh's `PublicKey` equality includes the comment, which
    /// the fixtures carry and the wire does not.
    fn is_authorized(&self, user: &str, key: &PublicKey) -> bool {
        user == USER
            && self
                .spec
                .authorized
                .iter()
                .any(|authorized| authorized.key_data() == key.key_data())
    }

    fn check_key(&self, user: &str, key: &PublicKey) -> Auth {
        self.observe(|o| o.publickey_checks.push(key.clone()));
        if self.is_authorized(user, key) {
            Auth::Accept
        } else {
            reject()
        }
    }
}

impl server::Handler for Connection {
    type Error = russh::Error;

    async fn auth_none(&mut self, _user: &str) -> Result<Auth, Self::Error> {
        Ok(reject())
    }

    async fn auth_password(&mut self, user: &str, password: &str) -> Result<Auth, Self::Error> {
        self.observe(|o| o.password_attempts += 1);
        let accepted = user == USER && self.spec.password.as_deref() == Some(password);
        if accepted {
            return Ok(Auth::Accept);
        }
        // Behave like OpenSSH: password stays allowed after a wrong one. The russh server,
        // left to itself, removes it from the list (server/encrypted.rs:770 in 0.63.3).
        Ok(Auth::Reject {
            proceed_with_methods: Some(MethodSet::from(self.spec.methods.as_slice())),
            partial_success: false,
        })
    }

    async fn auth_publickey_offered(
        &mut self,
        user: &str,
        key: &PublicKey,
    ) -> Result<Auth, Self::Error> {
        self.observe(|o| o.publickey_offers.push(key.clone()));
        Ok(if self.is_authorized(user, key) {
            Auth::Accept
        } else {
            reject()
        })
    }

    async fn auth_publickey(&mut self, user: &str, key: &PublicKey) -> Result<Auth, Self::Error> {
        Ok(self.check_key(user, key))
    }

    async fn auth_keyboard_interactive<'a>(
        &'a mut self,
        user: &str,
        _submethods: &str,
        response: Option<Response<'a>>,
    ) -> Result<Auth, Self::Error> {
        if user != USER || self.spec.kbd_rounds.is_empty() {
            return Ok(reject());
        }
        let Some(response) = response else {
            self.kbd_round = 0;
            return Ok(self.round(0));
        };
        let answers: Vec<String> = response
            .map(|bytes| String::from_utf8_lossy(&bytes).into_owned())
            .collect();
        self.observe(|o| o.kbd_answers.push(answers.clone()));
        if answers != self.spec.kbd_rounds[self.kbd_round].expected {
            return Ok(reject());
        }
        self.kbd_round += 1;
        if self.kbd_round == self.spec.kbd_rounds.len() {
            Ok(Auth::Accept)
        } else {
            Ok(self.round(self.kbd_round))
        }
    }

    async fn channel_open_session(
        &mut self,
        _channel: Channel<Msg>,
        reply: server::ChannelOpenHandle,
        _session: &mut Session,
    ) -> Result<(), Self::Error> {
        reply.accept().await;
        Ok(())
    }

    /// Connects onward and relays both ways when forwarding is on; refuses otherwise, as a
    /// gateway with forwarding off does.
    async fn channel_open_direct_tcpip(
        &mut self,
        channel: Channel<Msg>,
        host_to_connect: &str,
        port_to_connect: u32,
        _originator_address: &str,
        _originator_port: u32,
        reply: server::ChannelOpenHandle,
        _session: &mut Session,
    ) -> Result<(), Self::Error> {
        self.observe(|o| {
            o.forwards
                .push((host_to_connect.to_owned(), port_to_connect));
        });
        let target = u16::try_from(port_to_connect)
            .ok()
            .filter(|_| self.spec.forwarding);
        let Some(port) = target else {
            reply
                .reject(russh::ChannelOpenFailure::AdministrativelyProhibited)
                .await;
            return Ok(());
        };
        match tokio::net::TcpStream::connect((host_to_connect, port)).await {
            Ok(mut onward) => {
                self.relayed.insert(channel.id());
                reply.accept().await;
                tokio::spawn(async move {
                    let mut stream = channel.into_stream();
                    let _ = tokio::io::copy_bidirectional(&mut stream, &mut onward).await;
                });
            }
            Err(_) => {
                reply.reject(russh::ChannelOpenFailure::ConnectFailed).await;
            }
        }
        Ok(())
    }

    async fn pty_request(
        &mut self,
        channel: ChannelId,
        term: &str,
        cols: u32,
        rows: u32,
        _pixel_width: u32,
        _pixel_height: u32,
        _modes: &[(russh::Pty, u32)],
        session: &mut Session,
    ) -> Result<(), Self::Error> {
        self.observe(|o| o.pty = Some((term.to_owned(), cols, rows)));
        session.channel_success(channel)
    }

    async fn shell_request(
        &mut self,
        channel: ChannelId,
        session: &mut Session,
    ) -> Result<(), Self::Error> {
        session.channel_success(channel)
    }

    /// `sftp` is accepted and echoes like the shell; `silent` gets no answer at all; any
    /// other name is refused.
    async fn subsystem_request(
        &mut self,
        channel: ChannelId,
        name: &str,
        session: &mut Session,
    ) -> Result<(), Self::Error> {
        match name {
            SUBSYSTEM_ACCEPTED => session.channel_success(channel),
            SUBSYSTEM_SILENT => Ok(()),
            _ => session.channel_failure(channel),
        }
    }

    async fn window_change_request(
        &mut self,
        _channel: ChannelId,
        cols: u32,
        rows: u32,
        _pixel_width: u32,
        _pixel_height: u32,
        _session: &mut Session,
    ) -> Result<(), Self::Error> {
        self.observe(|o| o.resizes.push((cols, rows)));
        Ok(())
    }

    async fn data(
        &mut self,
        channel: ChannelId,
        data: &[u8],
        session: &mut Session,
    ) -> Result<(), Self::Error> {
        if self.relayed.contains(&channel) {
            return Ok(());
        }
        self.observe(|o| o.bytes_received += data.len());
        if data == EXIT_COMMAND {
            session.exit_status_request(channel, self.spec.exit_status)?;
            session.close(channel)?;
            return Ok(());
        }
        session.data(channel, data.to_vec())
    }
}

/// Answers questions from queues filled by the test; an empty queue answers "cancel".
#[derive(Default)]
pub struct ScriptedPrompter {
    pub username: Option<String>,
    pub passwords: Mutex<VecDeque<Option<String>>>,
    pub passphrases: Mutex<VecDeque<Option<String>>>,
    pub kbd: Mutex<VecDeque<Option<Vec<String>>>>,
    /// Every question asked, in order: "username", "password", "passphrase", "kbd".
    pub asked: Mutex<Vec<&'static str>>,
    /// Never answer, to test deadlines and cancellation.
    pub hang: bool,
    /// Wait this long before each answer, as a user slow to type.
    pub delay: Duration,
}

impl ScriptedPrompter {
    pub fn passwords(values: &[&str]) -> Self {
        Self {
            passwords: Mutex::new(values.iter().map(|v| Some((*v).to_owned())).collect()),
            ..Self::default()
        }
    }

    pub fn passphrases(values: &[&str]) -> Self {
        Self {
            passphrases: Mutex::new(values.iter().map(|v| Some((*v).to_owned())).collect()),
            ..Self::default()
        }
    }

    pub fn asked(&self) -> Vec<&'static str> {
        self.asked.lock().expect("asked").clone()
    }

    async fn answer<T>(&self, kind: &'static str, next: Option<T>) -> Option<T> {
        self.asked.lock().expect("asked").push(kind);
        if self.hang {
            std::future::pending::<()>().await;
        }
        tokio::time::sleep(self.delay).await;
        next
    }
}

fn pop<T>(queue: &Mutex<VecDeque<Option<T>>>) -> Option<T> {
    queue.lock().expect("queue").pop_front().flatten()
}

impl Prompter for ScriptedPrompter {
    async fn username(&self, _question: UsernameQuestion) -> Option<String> {
        let next = self.username.clone();
        self.answer("username", next).await
    }

    async fn password(&self, _question: PasswordQuestion) -> Option<Secret> {
        let next = pop(&self.passwords).map(Secret::new);
        self.answer("password", next).await
    }

    async fn key_passphrase(&self, _question: PassphraseQuestion) -> Option<Secret> {
        let next = pop(&self.passphrases).map(Secret::new);
        self.answer("passphrase", next).await
    }

    async fn keyboard_interactive(
        &self,
        _question: KeyboardInteractiveQuestion,
    ) -> Option<Vec<Secret>> {
        let next = pop(&self.kbd).map(|answers| answers.into_iter().map(Secret::new).collect());
        self.answer("kbd", next).await
    }
}

pub fn profile(port: u16, key: Option<&str>) -> SshProfile {
    SshProfile {
        id: ProfileId::new("test"),
        name: "test".to_owned(),
        group: None,
        host: LOOPBACK.to_owned(),
        port,
        username: Some(USER.to_owned()),
        key_path: key.map(client_key_path),
        gateway: None,
        vault_entry: None,
        forwards: heimdall_core::profile::Forwards::default(),
    }
}

/// Options with a fresh `known_hosts` that already trusts `host_key` for the server.
pub fn options_trusting(dir: &Path, port: u16, host_key: &str) -> ConnectOptions {
    let options = options_empty(dir);
    heimdall_ssh::KnownHosts::new(&options.known_hosts)
        .learn(LOOPBACK, port, &host_public_key(host_key))
        .expect("learn");
    options
}

/// Options with an empty `known_hosts` and no agent.
pub fn options_empty(dir: &Path) -> ConnectOptions {
    let mut options = ConnectOptions::new(dir.join("known_hosts"));
    options.agent = heimdall_ssh::AgentSource::Disabled;
    options
}
