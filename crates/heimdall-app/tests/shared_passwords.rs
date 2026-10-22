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

//! The passwords of shared SSH, SFTP and VNC sessions remembered on this computer, as the C#
//! `SharedCredentialApplier` keeps them: opt in when the password is asked, kept once the
//! server accepted it, filed under the server and account rather than the shared session,
//! forgotten when refused, never exported and never logged.

use std::path::Path;
use std::sync::{Arc, Mutex, Once};

use heimdall_app::vnc_driver::{VncRequest, vnc_events};
use heimdall_app::{
    Answer, AnswerRegistry, App, AppConfig, AttemptId, ConnectionEvent, Dialog, Effect, InputSink,
    Message, OpenedVault, QuestionId, QuestionKind, ServerPasswordQuestion, SettingsMessage,
    SystemCredentials, TabId, UiError, VaultTicket, open_vault,
};
use heimdall_core::profile::ProfileId;
use heimdall_core::settings::{Settings, settings_path};
use heimdall_core::shared_sessions::SharedSource;
use heimdall_ssh::{
    AgentSource, AuthMethod, PasswordQuestion, Secret, SessionClosed, TerminalSize,
};
use heimdall_term::GridSize;
use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};
use tokio::net::{TcpListener, TcpStream};
use tokio_stream::StreamExt as _;

/// What the tests type; nothing may carry it anywhere but the credential store.
const PASSWORD: &str = "REMEMBERED-PASSWORD-MARKER";
/// The account and server the shared file names; they are not logged either.
const LOGIN: &str = "remember-login-marker";
const HOST: &str = "remember-host-marker.example.com";
const MASTER: &str = "correct horse battery staple";
/// The password the fake VNC server accepts.
const VNC_PASSWORD: &str = "Secret12";

// ---- Logging -------------------------------------------------------------------------------

struct Capture(Mutex<Vec<String>>);

impl log::Log for Capture {
    fn enabled(&self, _: &log::Metadata<'_>) -> bool {
        true
    }
    fn log(&self, record: &log::Record<'_>) {
        if let Ok(mut lines) = self.0.lock() {
            lines.push(format!("{}", record.args()));
        }
    }
    fn flush(&self) {}
}

static LOGGED: Capture = Capture(Mutex::new(Vec::new()));
static INSTALL: Once = Once::new();

/// Collects every log line of this test binary.
fn capture_logs() {
    INSTALL.call_once(|| {
        log::set_logger(&LOGGED).expect("one logger");
        log::set_max_level(log::LevelFilter::Trace);
    });
}

/// Nothing logged names a password, the server or the account it was remembered for.
fn assert_nothing_logged() {
    for line in LOGGED.0.lock().expect("log").iter() {
        for secret in [PASSWORD, LOGIN, HOST, VNC_PASSWORD] {
            assert!(!line.contains(secret), "logged: {line}");
        }
    }
}

// ---- Files ---------------------------------------------------------------------------------

/// A session line of a `MobaXterm` file: the protocol `code`, and each `(index, value)` of
/// `fields` at its place in the `%` list, the others empty.
fn line(name: &str, code: u32, fields: &[(usize, &str)]) -> String {
    const FIELD_COUNT: usize = 32;
    let mut values = vec![""; FIELD_COUNT];
    for (at, value) in fields {
        values[*at] = value;
    }
    format!("{name}= #{code}#0%{}\r\n", values.join("%"))
}

/// A file with an SSH session `web` at `host`:`port` for `login`, an SFTP session `files` at
/// the same server, a VNC session `desk` at `vnc_port` and an RDP session.
fn file(host: &str, port: u16, login: &str, vnc_port: u16) -> String {
    let port = port.to_string();
    let vnc_port = vnc_port.to_string();
    format!(
        "[Bookmarks]\r\nSubRep=Ops\r\n{}{}{}{}",
        line("web", 109, &[(0, host), (1, &port), (2, login)]),
        line("files", 140, &[(0, host), (1, &port), (2, login)]),
        line("desk", 128, &[(0, "127.0.0.1"), (1, &vnc_port)]),
        "rdp= #91#4%rdp.example.com%3390%admin%0%-1%-1%\r\n",
    )
}

fn source(id: &str, path: &Path) -> SharedSource {
    SharedSource {
        id: id.to_owned(),
        name: id.to_owned(),
        path: path.to_string_lossy().into_owned(),
        enabled: true,
    }
}

/// The application over `dir` reading each `(source, text)`, with `system` as the credential
/// store.
fn app_over(dir: &Path, files: &[(&str, &str)], system: &SystemCredentials) -> App {
    let sources: Vec<SharedSource> = files
        .iter()
        .map(|(id, text)| {
            let path = dir.join(format!("{id}.mxtsessions"));
            std::fs::write(&path, text).expect("write");
            source(id, &path)
        })
        .collect();
    let profiles_file = dir.join("profiles.toml");
    Settings {
        shared_sources: sources,
        ..Settings::default()
    }
    .save(&settings_path(&profiles_file))
    .expect("settings");
    App::new(AppConfig {
        profiles_file,
        known_hosts: dir.join("known_hosts"),
        legacy_dir: None,
        agent: AgentSource::Disabled,
        initial_grid: GridSize { cols: 80, rows: 24 },
        files_start: dir.to_owned(),
        system_credentials: system.clone(),
    })
}

fn id_of(app: &App, name: &str) -> ProfileId {
    app.shared_sessions()
        .sessions()
        .find(|session| session.name() == name)
        .unwrap_or_else(|| panic!("no session {name}"))
        .id
        .clone()
}

// ---- Driving a session ----------------------------------------------------------------------

#[derive(Debug, Default)]
struct Sink;

impl InputSink for Sink {
    fn write(&self, _: Vec<u8>) -> Result<(), SessionClosed> {
        Ok(())
    }
    fn resize(&self, _: TerminalSize) -> Result<(), SessionClosed> {
        Ok(())
    }
    fn close(&self) {}
}

/// Opens shared session `name`: its tab and attempt.
fn open(app: &mut App, name: &str) -> (TabId, AttemptId) {
    let effects = app.update(Message::ConnectShared(id_of(app, name)));
    match effects.as_slice() {
        [
            Effect::Connect { tab, attempt, .. }
            | Effect::ConnectRdp { tab, attempt, .. }
            | Effect::ConnectVnc { tab, attempt, .. },
        ] => (*tab, *attempt),
        other => panic!("expected one connection, got {other:?}"),
    }
}

fn event(app: &mut App, tab: TabId, attempt: AttemptId, event: ConnectionEvent) -> Vec<Effect> {
    app.update(Message::Connection {
        tab,
        attempt,
        event,
    })
}

fn password_question(host: &str, port: u16, login: &str, attempt: u32) -> QuestionKind {
    QuestionKind::Password(PasswordQuestion {
        host: host.to_owned(),
        port,
        username: login.to_owned(),
        attempt,
    })
}

/// What asking `kind` led to: the question, the password Heimdall answered with by itself if
/// it did, and the state of the "Remember" box if the question shows it.
struct Asked {
    question: QuestionId,
    answered: Option<String>,
    box_shown: Option<bool>,
}

fn ask(app: &mut App, tab: TabId, attempt: AttemptId, kind: QuestionKind) -> Asked {
    let question = QuestionId::fresh();
    let effects = event(
        app,
        tab,
        attempt,
        ConnectionEvent::Question { question, kind },
    );
    let answered = match effects.as_slice() {
        [] => None,
        [
            Effect::Answer {
                question: answered,
                answer: Some(Answer::Secret(secret)),
            },
        ] if *answered == question => Some(secret.expose().to_owned()),
        other => panic!("unexpected {other:?}"),
    };
    let box_shown = app
        .tab(tab)
        .expect("tab")
        .prompts
        .iter()
        .find(|prompt| prompt.question == question)
        .and_then(|prompt| prompt.remember);
    Asked {
        question,
        answered,
        box_shown,
    }
}

/// The user types `password` in answer to `question`, with the box ticked or not.
fn type_password(
    app: &mut App,
    tab: TabId,
    question: QuestionId,
    password: &str,
    remember: Option<bool>,
) {
    if let Some(remember) = remember {
        app.update(Message::RememberPassword {
            tab,
            question,
            remember,
        });
    }
    app.update(Message::Answer {
        tab,
        question,
        answer: Some(Answer::Secret(Secret::new(password.to_owned()))),
    });
}

fn signed_in(app: &mut App, tab: TabId, attempt: AttemptId) {
    event(
        app,
        tab,
        attempt,
        ConnectionEvent::Connected {
            input: Arc::new(Sink),
        },
    );
}

/// The shared SSH session `web` asked for its password: what it was answered with by itself.
fn web_answer(app: &mut App, host: &str, port: u16, login: &str) -> Option<String> {
    let (tab, attempt) = open(app, "web");
    ask(app, tab, attempt, password_question(host, port, login, 1)).answered
}

/// Opens `web`, types `PASSWORD` with the box ticked and lets the server accept it.
fn remember_on_web(app: &mut App, host: &str, port: u16, login: &str) {
    let (tab, attempt) = open(app, "web");
    let asked = ask(app, tab, attempt, password_question(host, port, login, 1));
    assert_eq!(asked.answered, None);
    type_password(app, tab, asked.question, PASSWORD, Some(true));
    signed_in(app, tab, attempt);
}

fn entries(system: &SystemCredentials) -> Vec<(String, Vec<u8>)> {
    let (SystemCredentials::Memory(entries) | SystemCredentials::MemoryFaulty { entries, .. }) =
        system
    else {
        unreachable!("a test store")
    };
    entries
        .lock()
        .expect("entries")
        .iter()
        .map(|(name, bytes)| (name.clone(), bytes.to_vec()))
        .collect()
}

/// Whether anything in the store holds `text`, in a name or a value.
fn store_holds(system: &SystemCredentials, text: &str) -> bool {
    entries(system)
        .iter()
        .any(|(name, bytes)| name.contains(text) || contains(bytes, text.as_bytes()))
}

fn contains(haystack: &[u8], needle: &[u8]) -> bool {
    haystack
        .windows(needle.len())
        .any(|window| window == needle)
}

// ---- Tests ----------------------------------------------------------------------------------

#[test]
fn the_box_is_offered_for_the_password_of_a_shared_ssh_sftp_or_vnc_session_and_starts_cleared() {
    capture_logs();
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app_over(
        dir.path(),
        &[("team", &file(HOST, 22, LOGIN, 5901))],
        &SystemCredentials::memory(),
    );
    for name in ["web", "files"] {
        let (tab, attempt) = open(&mut app, name);
        let asked = ask(
            &mut app,
            tab,
            attempt,
            password_question(HOST, 22, LOGIN, 1),
        );
        assert_eq!(
            asked.box_shown,
            Some(false),
            "{name}: never ticked for the user"
        );
    }
    let (tab, attempt) = open(&mut app, "desk");
    let asked = ask(
        &mut app,
        tab,
        attempt,
        QuestionKind::ServerPassword(ServerPasswordQuestion {
            host: "127.0.0.1".to_owned(),
            port: 5901,
        }),
    );
    assert_eq!(asked.box_shown, Some(false));
    assert_nothing_logged();
}

#[test]
fn a_bastion_a_passphrase_and_an_rdp_session_offer_nothing_to_remember() {
    capture_logs();
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app_over(
        dir.path(),
        &[("team", &file(HOST, 22, LOGIN, 5901))],
        &SystemCredentials::memory(),
    );
    let (tab, attempt) = open(&mut app, "web");
    // Another server on the way asks: its own question, not the session's.
    let asked = ask(
        &mut app,
        tab,
        attempt,
        password_question("bastion.example.com", 22, LOGIN, 1),
    );
    assert_eq!(asked.box_shown, None);
    // RDP asks its password in the RDP client; the C# remembers none either.
    let (tab, attempt) = open(&mut app, "rdp");
    let asked = ask(
        &mut app,
        tab,
        attempt,
        password_question("rdp.example.com", 3390, "admin", 1),
    );
    assert_eq!(asked.box_shown, None);
}

#[test]
fn nothing_is_remembered_unless_the_box_was_ticked() {
    capture_logs();
    let dir = tempfile::tempdir().expect("dir");
    let system = SystemCredentials::memory();
    let mut app = app_over(
        dir.path(),
        &[("team", &file(HOST, 22, LOGIN, 5901))],
        &system,
    );
    let (tab, attempt) = open(&mut app, "web");
    let asked = ask(
        &mut app,
        tab,
        attempt,
        password_question(HOST, 22, LOGIN, 1),
    );
    type_password(&mut app, tab, asked.question, PASSWORD, None);
    signed_in(&mut app, tab, attempt);
    assert!(entries(&system).is_empty());
    // Ticked, then cleared again.
    let (tab, attempt) = open(&mut app, "web");
    let asked = ask(
        &mut app,
        tab,
        attempt,
        password_question(HOST, 22, LOGIN, 1),
    );
    app.update(Message::RememberPassword {
        tab,
        question: asked.question,
        remember: true,
    });
    type_password(&mut app, tab, asked.question, PASSWORD, Some(false));
    signed_in(&mut app, tab, attempt);
    assert!(entries(&system).is_empty());
    assert_eq!(web_answer(&mut app, HOST, 22, LOGIN), None);
    assert_nothing_logged();
}

#[test]
fn a_refused_password_typed_with_the_box_ticked_is_not_kept_for_the_one_typed_after_it() {
    capture_logs();
    let dir = tempfile::tempdir().expect("dir");
    let system = SystemCredentials::memory();
    let mut app = app_over(
        dir.path(),
        &[("team", &file(HOST, 22, LOGIN, 5901))],
        &system,
    );
    let (tab, attempt) = open(&mut app, "web");
    let first = ask(
        &mut app,
        tab,
        attempt,
        password_question(HOST, 22, LOGIN, 1),
    );
    type_password(
        &mut app,
        tab,
        first.question,
        "WRONG-FIRST-PASSWORD",
        Some(true),
    );
    // The server asks again; the right one is typed with the box left cleared.
    let again = ask(
        &mut app,
        tab,
        attempt,
        password_question(HOST, 22, LOGIN, 2),
    );
    type_password(&mut app, tab, again.question, PASSWORD, None);
    signed_in(&mut app, tab, attempt);
    assert!(
        entries(&system).is_empty(),
        "nothing was ticked for the accepted one"
    );
    assert_nothing_logged();
}

#[test]
fn a_ticked_password_is_kept_once_the_server_accepted_it_and_answers_the_next_time() {
    capture_logs();
    let dir = tempfile::tempdir().expect("dir");
    let system = SystemCredentials::memory();
    let mut app = app_over(
        dir.path(),
        &[("team", &file(HOST, 22, LOGIN, 5901))],
        &system,
    );
    let (tab, attempt) = open(&mut app, "web");
    let asked = ask(
        &mut app,
        tab,
        attempt,
        password_question(HOST, 22, LOGIN, 1),
    );
    type_password(&mut app, tab, asked.question, PASSWORD, Some(true));
    assert!(
        entries(&system).is_empty(),
        "not before the server accepted it"
    );
    signed_in(&mut app, tab, attempt);
    assert!(store_holds(&system, PASSWORD));
    assert_eq!(app.shared_password_count("team"), 1);

    assert_eq!(
        web_answer(&mut app, HOST, 22, LOGIN).as_deref(),
        Some(PASSWORD)
    );
    // It survives a restart: the store is the system's, not the process's.
    let mut restarted = app_over(
        dir.path(),
        &[("team", &file(HOST, 22, LOGIN, 5901))],
        &system,
    );
    assert_eq!(
        web_answer(&mut restarted, HOST, 22, LOGIN).as_deref(),
        Some(PASSWORD)
    );
    // The host in any case is the same server.
    assert_eq!(
        web_answer(&mut restarted, &HOST.to_uppercase(), 22, LOGIN).as_deref(),
        Some(PASSWORD)
    );
    assert_nothing_logged();
}

#[test]
fn a_password_the_server_did_not_accept_is_not_kept() {
    capture_logs();
    let dir = tempfile::tempdir().expect("dir");
    let system = SystemCredentials::memory();
    let mut app = app_over(
        dir.path(),
        &[("team", &file(HOST, 22, LOGIN, 5901))],
        &system,
    );
    let (tab, attempt) = open(&mut app, "web");
    let asked = ask(
        &mut app,
        tab,
        attempt,
        password_question(HOST, 22, LOGIN, 1),
    );
    type_password(&mut app, tab, asked.question, PASSWORD, Some(true));
    event(
        &mut app,
        tab,
        attempt,
        ConnectionEvent::Failed(UiError::AuthenticationFailed {
            tried: vec![AuthMethod::Password],
            agent_keys: None,
        }),
    );
    assert!(entries(&system).is_empty());
    // A later success of the same tab's other attempt does not revive it.
    signed_in(&mut app, tab, attempt);
    assert!(entries(&system).is_empty());
    assert_nothing_logged();
}

/// Remembers `PASSWORD` for the file as `HOST:22` / `LOGIN` says it, then reads the file as
/// `changed` says it with the same application data, and returns what `web` is answered with.
fn answered_after_the_file_changed(changed: &str) -> Option<String> {
    let dir = tempfile::tempdir().expect("dir");
    let system = SystemCredentials::memory();
    let mut app = app_over(
        dir.path(),
        &[("team", &file(HOST, 22, LOGIN, 5901))],
        &system,
    );
    remember_on_web(&mut app, HOST, 22, LOGIN);
    assert_eq!(
        web_answer(&mut app, HOST, 22, LOGIN).as_deref(),
        Some(PASSWORD)
    );
    drop(app);
    // The same session (same name, same folder: the same shared identifier) in a file that
    // now names another server.
    let (host, port, login) = match changed {
        "host" => ("other-host.example.com", 22, LOGIN),
        "port" => (HOST, 2222, LOGIN),
        "login" => (HOST, 22, "someone-else"),
        other => panic!("{other}"),
    };
    let mut app = app_over(
        dir.path(),
        &[("team", &file(host, port, login, 5901))],
        &system,
    );
    let (tab, attempt) = open(&mut app, "web");
    let asked = ask(
        &mut app,
        tab,
        attempt,
        password_question(host, port, login, 1),
    );
    assert_eq!(asked.box_shown, Some(false), "asked, with the box");
    asked.answered
}

#[test]
fn a_changed_host_in_the_shared_file_never_matches_a_remembered_password() {
    capture_logs();
    assert_eq!(answered_after_the_file_changed("host"), None);
    assert_nothing_logged();
}

#[test]
fn a_changed_port_in_the_shared_file_never_matches_a_remembered_password() {
    capture_logs();
    assert_eq!(answered_after_the_file_changed("port"), None);
}

#[test]
fn a_changed_login_in_the_shared_file_never_matches_a_remembered_password() {
    capture_logs();
    assert_eq!(answered_after_the_file_changed("login"), None);
}

#[test]
fn the_key_is_the_server_and_account_and_the_shared_session_identifier_plays_no_part() {
    capture_logs();
    let dir = tempfile::tempdir().expect("dir");
    let system = SystemCredentials::memory();
    let mut app = app_over(
        dir.path(),
        &[("team", &file(HOST, 22, LOGIN, 5901))],
        &system,
    );
    remember_on_web(&mut app, HOST, 22, LOGIN);
    // The same server and account, reached by the SFTP session of another name: the same
    // SSH account, the same password.
    let (tab, attempt) = open(&mut app, "files");
    let asked = ask(
        &mut app,
        tab,
        attempt,
        password_question(HOST, 22, LOGIN, 1),
    );
    assert_eq!(asked.answered.as_deref(), Some(PASSWORD));
    // The name of the entries carries neither the session identifier nor the server.
    for (name, _) in entries(&system) {
        assert!(!name.contains("shared-team-"), "{name}");
        assert!(!name.contains("remember-"), "{name}");
    }
    assert_nothing_logged();
}

#[test]
fn a_password_the_server_asks_again_is_forgotten_and_never_given_twice() {
    capture_logs();
    let dir = tempfile::tempdir().expect("dir");
    let system = SystemCredentials::memory();
    let mut app = app_over(
        dir.path(),
        &[("team", &file(HOST, 22, LOGIN, 5901))],
        &system,
    );
    remember_on_web(&mut app, HOST, 22, LOGIN);

    let (tab, attempt) = open(&mut app, "web");
    let first = ask(
        &mut app,
        tab,
        attempt,
        password_question(HOST, 22, LOGIN, 1),
    );
    assert_eq!(first.answered.as_deref(), Some(PASSWORD));
    // The server refused it and asks again: not given again, and gone.
    let again = ask(
        &mut app,
        tab,
        attempt,
        password_question(HOST, 22, LOGIN, 2),
    );
    assert_eq!(again.answered, None);
    assert_eq!(
        again.box_shown,
        Some(false),
        "the user is asked, with the box"
    );
    assert!(!store_holds(&system, PASSWORD));
    assert_eq!(app.shared_password_count("team"), 0);
    assert_eq!(web_answer(&mut app, HOST, 22, LOGIN), None);
    assert_nothing_logged();
}

#[test]
fn a_remembered_password_the_connection_refuses_is_forgotten_but_a_network_failure_keeps_it() {
    capture_logs();
    let dir = tempfile::tempdir().expect("dir");
    let system = SystemCredentials::memory();
    let mut app = app_over(
        dir.path(),
        &[("team", &file(HOST, 22, LOGIN, 5901))],
        &system,
    );
    remember_on_web(&mut app, HOST, 22, LOGIN);

    let (tab, attempt) = open(&mut app, "web");
    assert_eq!(
        ask(
            &mut app,
            tab,
            attempt,
            password_question(HOST, 22, LOGIN, 1)
        )
        .answered
        .as_deref(),
        Some(PASSWORD)
    );
    event(
        &mut app,
        tab,
        attempt,
        ConnectionEvent::Failed(UiError::Timeout),
    );
    assert!(
        store_holds(&system, PASSWORD),
        "a timeout says nothing of the password"
    );

    let (tab, attempt) = open(&mut app, "web");
    assert_eq!(
        ask(
            &mut app,
            tab,
            attempt,
            password_question(HOST, 22, LOGIN, 1)
        )
        .answered
        .as_deref(),
        Some(PASSWORD)
    );
    event(
        &mut app,
        tab,
        attempt,
        ConnectionEvent::Failed(UiError::AuthenticationFailed {
            tried: vec![AuthMethod::Password],
            agent_keys: None,
        }),
    );
    assert!(!store_holds(&system, PASSWORD));
    assert_eq!(web_answer(&mut app, HOST, 22, LOGIN), None);
    assert_nothing_logged();
}

#[test]
fn forgetting_a_source_forgets_its_passwords_and_leaves_the_others() {
    capture_logs();
    let dir = tempfile::tempdir().expect("dir");
    let system = SystemCredentials::memory();
    let text = file(HOST, 22, LOGIN, 5901);
    let mut app = app_over(dir.path(), &[("team", &text), ("other", &text)], &system);
    // Two sources naming the same server: each remembers on its own.
    let webs: Vec<ProfileId> = app
        .shared_sessions()
        .sessions()
        .filter(|session| session.name() == "web")
        .map(|session| session.id.clone())
        .collect();
    assert_eq!(webs.len(), 2);
    for session in webs {
        let effects = app.update(Message::ConnectShared(session));
        let [Effect::Connect { tab, attempt, .. }] = effects.as_slice() else {
            panic!("{effects:?}");
        };
        let (tab, attempt) = (*tab, *attempt);
        let asked = ask(
            &mut app,
            tab,
            attempt,
            password_question(HOST, 22, LOGIN, 1),
        );
        assert_eq!(asked.answered, None, "each source asks for itself");
        type_password(&mut app, tab, asked.question, PASSWORD, Some(true));
        signed_in(&mut app, tab, attempt);
    }
    assert_eq!(app.shared_password_count("team"), 1);
    assert_eq!(app.shared_password_count("other"), 1);

    app.update(Message::ForgetSharedPasswords("team".to_owned()));
    assert_eq!(app.shared_password_count("team"), 0);
    assert_eq!(app.shared_password_count("other"), 1);
    assert!(store_holds(&system, PASSWORD), "the other source's stays");
    app.update(Message::ForgetSharedPasswords("other".to_owned()));
    assert!(
        entries(&system).is_empty(),
        "nothing left, indexes included"
    );
    assert_nothing_logged();
}

#[test]
fn a_source_taken_out_of_the_settings_takes_its_remembered_passwords_with_it() {
    capture_logs();
    let dir = tempfile::tempdir().expect("dir");
    let system = SystemCredentials::memory();
    let mut app = app_over(
        dir.path(),
        &[("team", &file(HOST, 22, LOGIN, 5901))],
        &system,
    );
    remember_on_web(&mut app, HOST, 22, LOGIN);
    assert!(store_holds(&system, PASSWORD));
    app.update(Message::Settings(
        SettingsMessage::SharedSources(Vec::new()),
    ));
    app.update(Message::SharedTick);
    assert!(entries(&system).is_empty(), "{:?}", entries(&system));
    assert_nothing_logged();
}

#[test]
fn an_application_with_no_shared_source_leaves_the_store_alone() {
    capture_logs();
    let dir = tempfile::tempdir().expect("dir");
    let system = SystemCredentials::memory();
    // An entry of another kind is left alone.
    let SystemCredentials::Memory(held) = &system else {
        unreachable!()
    };
    held.lock()
        .expect("held")
        .insert("password/a".to_owned(), zeroize_bytes(b"x"));
    let mut app = app_over(dir.path(), &[], &system);
    app.update(Message::SharedTick);
    assert_eq!(entries(&system).len(), 1);
}

fn zeroize_bytes(bytes: &[u8]) -> zeroize::Zeroizing<Vec<u8>> {
    zeroize::Zeroizing::new(bytes.to_vec())
}

#[test]
fn what_is_remembered_leaves_no_trace_in_the_settings_an_export_or_any_file() {
    capture_logs();
    let dir = tempfile::tempdir().expect("dir");
    let system = SystemCredentials::memory();
    let mut app = app_over(
        dir.path(),
        &[("team", &file(HOST, 22, LOGIN, 5901))],
        &system,
    );
    remember_on_web(&mut app, HOST, 22, LOGIN);
    assert!(store_holds(&system, PASSWORD));

    // The settings export: the document the Export button writes.
    let effects = app.update(Message::SettingsTransfer(
        heimdall_app::SettingsTransferMessage::Export,
    ));
    let documents: Vec<String> = effects
        .into_iter()
        .filter_map(|effect| match effect {
            Effect::SaveSettingsFile { document } => Some(document),
            _ => None,
        })
        .collect();
    assert!(!documents.is_empty(), "an export was produced");
    for document in &documents {
        for secret in [PASSWORD, LOGIN, HOST] {
            assert!(!document.contains(secret), "exported: {secret}");
        }
    }
    // Every file the application wrote beside its data: settings, profiles, the offline copy
    // of the shared file (which holds the sessions, not the password), the known hosts.
    app.update(Message::SharedTick);
    let mut seen = 0;
    for path in walk(dir.path()) {
        let bytes = std::fs::read(&path).expect("read");
        seen += 1;
        assert!(!contains(&bytes, PASSWORD.as_bytes()), "{}", path.display());
    }
    assert!(seen >= 2, "the settings and the shared file at least");
    // The shared report of the source.
    assert!(!format!("{:?}", app.shared_sessions()).contains(PASSWORD));
    assert_nothing_logged();
}

fn walk(dir: &Path) -> Vec<std::path::PathBuf> {
    let mut found = Vec::new();
    for entry in std::fs::read_dir(dir).expect("dir") {
        let path = entry.expect("entry").path();
        if path.is_dir() {
            found.extend(walk(&path));
        } else {
            found.push(path);
        }
    }
    found
}

// ---- With a master password -----------------------------------------------------------------

async fn unlock(app: &mut App, master: &str) {
    let create = app.vault_status() == heimdall_app::VaultStatus::Missing;
    app.update(Message::ShowVault);
    let effects = app.update(Message::SubmitVault {
        password: Secret::new(master.to_owned()),
        new: None,
        confirm: create.then(|| Secret::new(master.to_owned())),
    });
    let (ticket, result) = open_vault_job(effects).await;
    app.update(Message::VaultOpened(ticket, result));
}

async fn open_vault_job(
    effects: Vec<Effect>,
) -> (VaultTicket, Result<OpenedVault, heimdall_app::VaultProblem>) {
    match <[Effect; 1]>::try_from(effects) {
        Ok(
            [
                Effect::OpenVault {
                    path,
                    password,
                    job,
                    ticket,
                },
            ],
        ) => (ticket, open_vault(path, password, job).await),
        other => panic!("expected OpenVault, got {other:?}"),
    }
}

#[tokio::test]
async fn a_master_password_takes_the_remembered_passwords_into_the_vault_and_a_locked_vault_offers_no_box()
 {
    capture_logs();
    let dir = tempfile::tempdir().expect("dir");
    let system = SystemCredentials::memory();
    let mut app = app_over(
        dir.path(),
        &[("team", &file(HOST, 22, LOGIN, 5901))],
        &system,
    );
    remember_on_web(&mut app, HOST, 22, LOGIN);
    assert!(store_holds(&system, PASSWORD));

    unlock(&mut app, MASTER).await;
    assert!(
        entries(&system).is_empty(),
        "moved into the vault: {:?}",
        entries(&system)
            .iter()
            .map(|(name, _)| name)
            .collect::<Vec<_>>()
    );
    assert_eq!(
        web_answer(&mut app, HOST, 22, LOGIN).as_deref(),
        Some(PASSWORD)
    );
    app.update(Message::SharedTick);
    assert_eq!(app.shared_password_count("team"), 1);

    app.update(Message::LockVault);
    app.update(Message::SharedTick);
    assert_eq!(
        app.shared_password_count("team"),
        0,
        "unreadable while locked"
    );
    let (tab, attempt) = open(&mut app, "web");
    let asked = ask(
        &mut app,
        tab,
        attempt,
        password_question(HOST, 22, LOGIN, 1),
    );
    assert_eq!(asked.answered, None);
    assert_eq!(asked.box_shown, None, "nowhere to keep it");

    unlock(&mut app, MASTER).await;
    app.update(Message::SharedTick);
    assert_eq!(app.shared_password_count("team"), 1);
    assert_eq!(
        web_answer(&mut app, HOST, 22, LOGIN).as_deref(),
        Some(PASSWORD)
    );
    assert_nothing_logged();
}

// ---- Reset, revocation and failed deletion -------------------------------------------------

/// Opens `web`, types `PASSWORD` for `login` with the box ticked and leaves the server
/// undecided: the password waits for its answer.
fn type_pending(app: &mut App, login: &str) -> (TabId, AttemptId) {
    let (tab, attempt) = open(app, "web");
    let asked = ask(app, tab, attempt, password_question(HOST, 22, login, 1));
    assert_eq!(asked.answered, None);
    type_password(app, tab, asked.question, PASSWORD, Some(true));
    (tab, attempt)
}

fn reset_to_defaults(app: &mut App) {
    app.update(Message::Settings(SettingsMessage::ResetAllSettings));
    app.update(Message::ConfirmDialog);
}

#[test]
fn reset_to_defaults_forgets_the_remembered_passwords() {
    capture_logs();
    let dir = tempfile::tempdir().expect("dir");
    let system = SystemCredentials::memory();
    let mut app = app_over(
        dir.path(),
        &[("team", &file(HOST, 22, LOGIN, 5901))],
        &system,
    );
    remember_on_web(&mut app, HOST, 22, LOGIN);
    assert_eq!(app.shared_password_count("team"), 1);

    reset_to_defaults(&mut app);
    assert_eq!(app.shared_password_count("team"), 0);
    assert!(entries(&system).is_empty(), "{:?}", entries(&system));
    assert_eq!(web_answer(&mut app, HOST, 22, LOGIN), None);
    assert_nothing_logged();
}

#[test]
fn reset_to_defaults_drops_a_password_still_waiting_for_its_server() {
    capture_logs();
    let dir = tempfile::tempdir().expect("dir");
    let system = SystemCredentials::memory();
    let mut app = app_over(
        dir.path(),
        &[("team", &file(HOST, 22, LOGIN, 5901))],
        &system,
    );
    let (tab, attempt) = type_pending(&mut app, LOGIN);
    reset_to_defaults(&mut app);
    signed_in(&mut app, tab, attempt);
    assert!(entries(&system).is_empty(), "{:?}", entries(&system));
    assert_eq!(web_answer(&mut app, HOST, 22, LOGIN), None);
}

#[tokio::test]
async fn reset_to_defaults_with_a_locked_vault_forgets_them_once_it_opens() {
    capture_logs();
    let dir = tempfile::tempdir().expect("dir");
    let system = SystemCredentials::memory();
    let mut app = app_over(
        dir.path(),
        &[("team", &file(HOST, 22, LOGIN, 5901))],
        &system,
    );
    unlock(&mut app, MASTER).await;
    remember_on_web(&mut app, HOST, 22, LOGIN);
    app.update(Message::SharedTick);
    assert_eq!(app.shared_password_count("team"), 1);

    app.update(Message::LockVault);
    reset_to_defaults(&mut app);
    assert!(app.watches_shared_sources(), "the deletion waits");

    unlock(&mut app, MASTER).await;
    app.update(Message::SharedTick);
    assert_eq!(app.shared_password_count("team"), 0);
    assert_eq!(web_answer(&mut app, HOST, 22, LOGIN), None);
}

#[test]
fn a_forget_while_a_password_waits_for_its_server_keeps_it_from_being_remembered() {
    capture_logs();
    let dir = tempfile::tempdir().expect("dir");
    let system = SystemCredentials::memory();
    let mut app = app_over(
        dir.path(),
        &[("team", &file(HOST, 22, LOGIN, 5901))],
        &system,
    );
    remember_on_web(&mut app, HOST, 22, LOGIN);
    let (tab, attempt) = type_pending(&mut app, "another-login");
    app.update(Message::ForgetSharedPasswords("team".to_owned()));
    signed_in(&mut app, tab, attempt);
    assert_eq!(app.shared_password_count("team"), 0);
    assert!(entries(&system).is_empty(), "{:?}", entries(&system));
    assert_nothing_logged();
}

#[test]
fn taking_a_source_out_while_a_password_waits_keeps_it_from_being_remembered() {
    capture_logs();
    let dir = tempfile::tempdir().expect("dir");
    let system = SystemCredentials::memory();
    let mut app = app_over(
        dir.path(),
        &[("team", &file(HOST, 22, LOGIN, 5901))],
        &system,
    );
    let (tab, attempt) = type_pending(&mut app, LOGIN);
    app.update(Message::Settings(
        SettingsMessage::SharedSources(Vec::new()),
    ));
    signed_in(&mut app, tab, attempt);
    app.update(Message::SharedTick);
    assert!(entries(&system).is_empty(), "{:?}", entries(&system));
}

#[test]
fn a_password_asked_after_a_forget_is_remembered_again() {
    capture_logs();
    let dir = tempfile::tempdir().expect("dir");
    let system = SystemCredentials::memory();
    let mut app = app_over(
        dir.path(),
        &[("team", &file(HOST, 22, LOGIN, 5901))],
        &system,
    );
    app.update(Message::ForgetSharedPasswords("team".to_owned()));
    remember_on_web(&mut app, HOST, 22, LOGIN);
    assert_eq!(app.shared_password_count("team"), 1);
}

#[test]
fn a_deletion_the_store_refuses_keeps_the_entry_listed_says_so_and_can_be_retried() {
    capture_logs();
    let dir = tempfile::tempdir().expect("dir");
    let (system, faults) = SystemCredentials::memory_faulty();
    let mut app = app_over(
        dir.path(),
        &[("team", &file(HOST, 22, LOGIN, 5901))],
        &system,
    );
    remember_on_web(&mut app, HOST, 22, LOGIN);
    faults.fail_removals(true);

    app.update(Message::ForgetSharedPasswords("team".to_owned()));
    assert_eq!(
        app.shared_password_count("team"),
        1,
        "still there, still shown"
    );
    assert!(matches!(
        app.dialog,
        Some(Dialog::PasswordForgetFailed { remaining: 1 })
    ));
    app.update(Message::DismissDialog);

    faults.fail_removals(false);
    app.update(Message::ForgetSharedPasswords("team".to_owned()));
    assert_eq!(app.shared_password_count("team"), 0);
    assert!(app.dialog.is_none());
    assert!(entries(&system).is_empty(), "{:?}", entries(&system));
    assert_nothing_logged();
}

#[test]
fn a_source_taken_out_whose_deletion_is_refused_is_tried_again_at_the_next_look() {
    capture_logs();
    let dir = tempfile::tempdir().expect("dir");
    let (system, faults) = SystemCredentials::memory_faulty();
    let mut app = app_over(
        dir.path(),
        &[("team", &file(HOST, 22, LOGIN, 5901))],
        &system,
    );
    remember_on_web(&mut app, HOST, 22, LOGIN);
    faults.fail_removals(true);
    app.update(Message::Settings(
        SettingsMessage::SharedSources(Vec::new()),
    ));
    app.update(Message::SharedTick);
    assert!(store_holds(&system, PASSWORD), "refused, so still held");
    assert!(
        app.watches_shared_sources(),
        "no source left, yet the deletion is tried again"
    );

    faults.fail_removals(false);
    app.update(Message::SharedTick);
    assert!(entries(&system).is_empty(), "{:?}", entries(&system));
}

#[test]
fn a_reset_whose_deletion_is_refused_is_tried_again_at_the_next_look() {
    capture_logs();
    let dir = tempfile::tempdir().expect("dir");
    let (system, faults) = SystemCredentials::memory_faulty();
    let mut app = app_over(
        dir.path(),
        &[("team", &file(HOST, 22, LOGIN, 5901))],
        &system,
    );
    remember_on_web(&mut app, HOST, 22, LOGIN);
    faults.fail_removals(true);
    reset_to_defaults(&mut app);
    assert!(store_holds(&system, PASSWORD));
    assert_eq!(app.shared_password_count("team"), 1, "still listed");

    faults.fail_removals(false);
    app.update(Message::SharedTick);
    assert!(entries(&system).is_empty(), "{:?}", entries(&system));
}

#[test]
fn an_eviction_the_store_refuses_remembers_nothing_and_leaves_the_old_entry_listed() {
    capture_logs();
    let dir = tempfile::tempdir().expect("dir");
    let (system, faults) = SystemCredentials::memory_faulty();
    let mut app = app_over(
        dir.path(),
        &[("team", &file(HOST, 22, LOGIN, 5901))],
        &system,
    );
    let capacity = heimdall_core::shared_passwords::MAX_PER_SOURCE;
    let remember = |app: &mut App, login: &str| {
        let (tab, attempt) = type_pending(app, login);
        signed_in(app, tab, attempt);
    };
    for number in 0..capacity {
        remember(&mut app, &format!("login-{number}"));
    }
    assert_eq!(app.shared_password_count("team"), capacity);
    let held = entries(&system).len();

    faults.fail_removals(true);
    remember(&mut app, "one-too-many");
    assert!(matches!(
        app.dialog,
        Some(Dialog::PasswordSaveFailed { .. })
    ));
    app.dialog = None;
    assert_eq!(
        entries(&system).len(),
        held,
        "nothing added, nothing hidden"
    );
    assert_eq!(app.shared_password_count("team"), capacity);

    faults.fail_removals(false);
    remember(&mut app, "one-too-many");
    assert_eq!(app.shared_password_count("team"), capacity);
    assert_eq!(entries(&system).len(), held, "the oldest made room");
}

#[test]
fn a_read_the_store_fails_is_not_an_empty_index_and_forgets_nothing_it_cannot_see() {
    capture_logs();
    let dir = tempfile::tempdir().expect("dir");
    let (system, faults) = SystemCredentials::memory_faulty();
    let mut app = app_over(
        dir.path(),
        &[("team", &file(HOST, 22, LOGIN, 5901))],
        &system,
    );
    remember_on_web(&mut app, HOST, 22, LOGIN);
    faults.fail_reads(true);

    app.update(Message::ForgetSharedPasswords("team".to_owned()));
    assert!(matches!(
        app.dialog,
        Some(Dialog::PasswordForgetFailed { .. })
    ));
    assert_eq!(app.shared_password_count("team"), 1, "still shown");
    app.update(Message::DismissDialog);

    faults.fail_reads(false);
    assert!(store_holds(&system, PASSWORD), "nothing was orphaned");
    app.update(Message::ForgetSharedPasswords("team".to_owned()));
    assert_eq!(app.shared_password_count("team"), 0);
    assert!(entries(&system).is_empty(), "{:?}", entries(&system));
}

#[tokio::test]
async fn a_reset_with_a_locked_vault_is_still_done_after_the_application_restarted() {
    capture_logs();
    let dir = tempfile::tempdir().expect("dir");
    let system = SystemCredentials::memory();
    let files = [("team", file(HOST, 22, LOGIN, 5901))];
    let listed: Vec<(&str, &str)> = files
        .iter()
        .map(|(id, text)| (*id, text.as_str()))
        .collect();
    let mut app = app_over(dir.path(), &listed, &system);
    unlock(&mut app, MASTER).await;
    remember_on_web(&mut app, HOST, 22, LOGIN);
    app.update(Message::LockVault);
    reset_to_defaults(&mut app);
    drop(app);

    let mut app = app_over(dir.path(), &listed, &system);
    assert!(app.watches_shared_sources());
    unlock(&mut app, MASTER).await;
    app.update(Message::SharedTick);
    assert_eq!(app.shared_password_count("team"), 0);
    assert_eq!(web_answer(&mut app, HOST, 22, LOGIN), None);
}

#[tokio::test]
async fn a_password_is_not_given_between_a_reset_and_its_deletion() {
    capture_logs();
    let dir = tempfile::tempdir().expect("dir");
    let system = SystemCredentials::memory();
    let mut app = app_over(
        dir.path(),
        &[("team", &file(HOST, 22, LOGIN, 5901))],
        &system,
    );
    unlock(&mut app, MASTER).await;
    remember_on_web(&mut app, HOST, 22, LOGIN);
    app.update(Message::LockVault);
    reset_to_defaults(&mut app);

    // Opened, and connected to at once, before any look at the files.
    unlock(&mut app, MASTER).await;
    assert_eq!(web_answer(&mut app, HOST, 22, LOGIN), None);
}

#[test]
fn a_source_removal_whose_deletion_failed_is_taken_up_again_after_a_restart() {
    capture_logs();
    let dir = tempfile::tempdir().expect("dir");
    let (system, faults) = SystemCredentials::memory_faulty();
    let mut app = app_over(
        dir.path(),
        &[("team", &file(HOST, 22, LOGIN, 5901))],
        &system,
    );
    remember_on_web(&mut app, HOST, 22, LOGIN);
    faults.fail_removals(true);
    app.update(Message::Settings(
        SettingsMessage::SharedSources(Vec::new()),
    ));
    assert!(store_holds(&system, PASSWORD), "refused, so still held");
    drop(app);

    faults.fail_removals(false);
    let app = app_over(dir.path(), &[], &system);
    assert!(entries(&system).is_empty(), "{:?}", entries(&system));
    assert!(!app.watches_shared_sources(), "nothing left to wait for");
}

// ---- Review fixes ---------------------------------------------------------------------------

/// Takes every shared source out of the settings.
fn remove_sources(app: &mut App) {
    app.update(Message::Settings(
        SettingsMessage::SharedSources(Vec::new()),
    ));
}

/// Reconnects `tab`: its replacement and attempt.
fn reconnect(app: &mut App, tab: TabId) -> (TabId, AttemptId) {
    let effects = app.update(Message::ReconnectTab(tab));
    match effects.as_slice() {
        [Effect::Connect { tab, attempt, .. }] => (*tab, *attempt),
        other => panic!("expected one connection, got {other:?}"),
    }
}

#[test]
fn a_tab_of_a_removed_source_reconnected_offers_nothing_and_remembers_nothing() {
    capture_logs();
    let dir = tempfile::tempdir().expect("dir");
    let system = SystemCredentials::memory();
    let mut app = app_over(
        dir.path(),
        &[("team", &file(HOST, 22, LOGIN, 5901))],
        &system,
    );
    let (tab, attempt) = open(&mut app, "web");
    signed_in(&mut app, tab, attempt);
    remove_sources(&mut app);

    let (tab, attempt) = reconnect(&mut app, tab);
    let asked = ask(
        &mut app,
        tab,
        attempt,
        password_question(HOST, 22, LOGIN, 1),
    );
    assert_eq!(asked.box_shown, None, "no box for a source taken out");
    // Ticked anyway, as a stale screen could: nothing is kept.
    type_password(&mut app, tab, asked.question, PASSWORD, Some(true));
    signed_in(&mut app, tab, attempt);
    app.update(Message::SharedTick);
    assert!(entries(&system).is_empty(), "{:?}", entries(&system));
    assert_nothing_logged();
}

#[test]
fn a_tab_of_a_removed_source_reconnected_is_not_answered_from_the_store() {
    capture_logs();
    let dir = tempfile::tempdir().expect("dir");
    let (system, faults) = SystemCredentials::memory_faulty();
    let mut app = app_over(
        dir.path(),
        &[("team", &file(HOST, 22, LOGIN, 5901))],
        &system,
    );
    remember_on_web(&mut app, HOST, 22, LOGIN);
    let (tab, attempt) = open(&mut app, "web");
    signed_in(&mut app, tab, attempt);
    // The store refuses the deletion: the password is still there, but the source is gone.
    faults.fail_removals(true);
    remove_sources(&mut app);
    assert!(store_holds(&system, PASSWORD));

    let (tab, attempt) = reconnect(&mut app, tab);
    let asked = ask(
        &mut app,
        tab,
        attempt,
        password_question(HOST, 22, LOGIN, 1),
    );
    assert_eq!(asked.answered, None);
    assert_eq!(asked.box_shown, None);
}

#[test]
fn a_password_the_server_asked_again_is_not_kept_when_another_means_then_signs_in() {
    capture_logs();
    let dir = tempfile::tempdir().expect("dir");
    let system = SystemCredentials::memory();
    let mut app = app_over(
        dir.path(),
        &[("team", &file(HOST, 22, LOGIN, 5901))],
        &system,
    );
    // Refused: asked again for the second try, then an agent key signs in.
    let (tab, attempt) = type_pending(&mut app, LOGIN);
    ask(
        &mut app,
        tab,
        attempt,
        password_question(HOST, 22, LOGIN, 2),
    );
    signed_in(&mut app, tab, attempt);
    assert!(entries(&system).is_empty(), "{:?}", entries(&system));
    assert_nothing_logged();
}

/// A keyboard-interactive round asking `prompt`, hidden.
fn round(prompt: &str) -> QuestionKind {
    QuestionKind::KeyboardInteractive(heimdall_ssh::KeyboardInteractiveQuestion {
        host: HOST.to_owned(),
        username: LOGIN.to_owned(),
        name: String::new(),
        instructions: String::new(),
        prompts: vec![heimdall_ssh::KeyboardInteractivePrompt {
            text: prompt.to_owned(),
            echo: false,
        }],
    })
}

#[test]
fn a_password_accepted_as_the_first_factor_is_kept_after_a_one_time_code() {
    capture_logs();
    let dir = tempfile::tempdir().expect("dir");
    let system = SystemCredentials::memory();
    let mut app = app_over(
        dir.path(),
        &[("team", &file(HOST, 22, LOGIN, 5901))],
        &system,
    );
    let (tab, attempt) = type_pending(&mut app, LOGIN);
    // The second step: a code, not a refusal.
    ask(&mut app, tab, attempt, round("Verification code: "));
    signed_in(&mut app, tab, attempt);
    assert_eq!(app.shared_password_count("team"), 1);
    assert!(store_holds(&system, PASSWORD));
    assert_eq!(
        web_answer(&mut app, HOST, 22, LOGIN).as_deref(),
        Some(PASSWORD)
    );
}

#[test]
fn a_password_asked_again_in_a_keyboard_interactive_round_is_a_refusal_and_not_kept() {
    capture_logs();
    let dir = tempfile::tempdir().expect("dir");
    let system = SystemCredentials::memory();
    let mut app = app_over(
        dir.path(),
        &[("team", &file(HOST, 22, LOGIN, 5901))],
        &system,
    );
    let (tab, attempt) = type_pending(&mut app, LOGIN);
    ask(&mut app, tab, attempt, round("Password: "));
    signed_in(&mut app, tab, attempt);
    assert!(entries(&system).is_empty(), "{:?}", entries(&system));
}

#[tokio::test]
async fn a_copy_whose_deletion_failed_keeps_its_listing_so_it_cannot_come_back() {
    capture_logs();
    let dir = tempfile::tempdir().expect("dir");
    let (system, faults) = SystemCredentials::memory_faulty();
    let mut app = app_over(
        dir.path(),
        &[("team", &file(HOST, 22, LOGIN, 5901))],
        &system,
    );
    remember_on_web(&mut app, HOST, 22, LOGIN);
    // Only the password's own entry refuses its deletion when the vault is created.
    faults.fail_removals_under(Some("shared-password/"));
    unlock(&mut app, MASTER).await;
    assert!(store_holds(&system, PASSWORD));
    app.update(Message::ForgetSharedPasswords("team".to_owned()));

    faults.fail_removals_under(None);
    app.update(Message::DisableMasterPassword);
    let effects = app.update(Message::SubmitVault {
        password: Secret::new(MASTER.to_owned()),
        new: None,
        confirm: None,
    });
    let (ticket, result) = open_vault_job(effects).await;
    app.update(Message::VaultOpened(ticket, result));
    assert!(entries(&system).is_empty(), "{:?}", entries(&system));
    assert_eq!(web_answer(&mut app, HOST, 22, LOGIN), None);
}

#[test]
fn a_clean_up_that_cannot_be_noted_for_a_restart_is_reported_once() {
    capture_logs();
    let dir = tempfile::tempdir().expect("dir");
    let (system, faults) = SystemCredentials::memory_faulty();
    let mut app = app_over(
        dir.path(),
        &[("team", &file(HOST, 22, LOGIN, 5901))],
        &system,
    );
    remember_on_web(&mut app, HOST, 22, LOGIN);
    std::fs::create_dir(dir.path().join("shared-passwords-cleanup.pending")).expect("folder");
    faults.fail_removals(true);
    remove_sources(&mut app);
    assert!(
        matches!(app.dialog, Some(Dialog::PasswordPurgeUnrecorded)),
        "{:?}",
        app.dialog
    );
}

#[test]
fn a_marker_that_could_not_be_removed_does_not_wipe_what_was_remembered_since() {
    capture_logs();
    let dir = tempfile::tempdir().expect("dir");
    let system = SystemCredentials::memory();
    // What a removal that failed leaves: the marker emptied of its meaning.
    std::fs::write(dir.path().join("shared-passwords-purge.pending"), b"done").expect("marker");
    let mut app = app_over(
        dir.path(),
        &[("team", &file(HOST, 22, LOGIN, 5901))],
        &system,
    );
    remember_on_web(&mut app, HOST, 22, LOGIN);
    app.update(Message::SharedTick);
    assert_eq!(app.shared_password_count("team"), 1, "not wiped at start");
    assert_eq!(
        web_answer(&mut app, HOST, 22, LOGIN).as_deref(),
        Some(PASSWORD)
    );
}

#[tokio::test]
async fn a_copy_left_in_the_system_store_by_the_vault_does_not_come_back_after_a_forget() {
    capture_logs();
    let dir = tempfile::tempdir().expect("dir");
    let (system, faults) = SystemCredentials::memory_faulty();
    let mut app = app_over(
        dir.path(),
        &[("team", &file(HOST, 22, LOGIN, 5901))],
        &system,
    );
    remember_on_web(&mut app, HOST, 22, LOGIN);
    // The system's store refuses the deletions the vault's creation asks for: copies stay.
    faults.fail_removals(true);
    unlock(&mut app, MASTER).await;
    assert!(store_holds(&system, PASSWORD), "the copy is left behind");
    app.update(Message::ForgetSharedPasswords("team".to_owned()));
    assert_eq!(app.shared_password_count("team"), 0);

    // The master password removed: the vault's passwords go back, and a copy with no
    // counterpart in the vault goes instead of coming back to life.
    faults.fail_removals(false);
    app.update(Message::DisableMasterPassword);
    let effects = app.update(Message::SubmitVault {
        password: Secret::new(MASTER.to_owned()),
        new: None,
        confirm: None,
    });
    let (ticket, result) = open_vault_job(effects).await;
    app.update(Message::VaultOpened(ticket, result));
    assert_eq!(app.vault_status(), heimdall_app::VaultStatus::Missing);
    assert!(entries(&system).is_empty(), "{:?}", entries(&system));
    assert_eq!(web_answer(&mut app, HOST, 22, LOGIN), None);
}

/// Replaces the entry `name` of the memory store with `bytes`.
fn overwrite(system: &SystemCredentials, name: &str, bytes: &[u8]) {
    let (SystemCredentials::Memory(held) | SystemCredentials::MemoryFaulty { entries: held, .. }) =
        system
    else {
        unreachable!("a test store")
    };
    held.lock()
        .expect("held")
        .insert(name.to_owned(), zeroize_bytes(bytes));
}

#[test]
fn an_index_this_version_cannot_read_is_not_an_empty_one() {
    capture_logs();
    let dir = tempfile::tempdir().expect("dir");
    let system = SystemCredentials::memory();
    let mut app = app_over(
        dir.path(),
        &[("team", &file(HOST, 22, LOGIN, 5901))],
        &system,
    );
    remember_on_web(&mut app, HOST, 22, LOGIN);
    let index = "shared-password-index/team";
    let good = entries(&system)
        .into_iter()
        .find(|(name, _)| name == index)
        .expect("an index")
        .1;
    // Written by a newer version, or damaged.
    overwrite(&system, index, b"2\nfrom-the-future");

    app.update(Message::ForgetSharedPasswords("team".to_owned()));
    assert!(matches!(
        app.dialog,
        Some(Dialog::PasswordForgetFailed { .. })
    ));
    app.update(Message::DismissDialog);
    remove_sources(&mut app);
    app.update(Message::SharedTick);
    assert!(store_holds(&system, PASSWORD), "nothing was orphaned");
    assert!(app.watches_shared_sources(), "the clean-up stays pending");

    overwrite(&system, index, &good);
    app.update(Message::SharedTick);
    assert!(entries(&system).is_empty(), "{:?}", entries(&system));
}

#[test]
fn a_password_whose_entry_cannot_be_written_leaves_nothing_behind() {
    capture_logs();
    let dir = tempfile::tempdir().expect("dir");
    let (system, faults) = SystemCredentials::memory_faulty();
    let mut app = app_over(
        dir.path(),
        &[("team", &file(HOST, 22, LOGIN, 5901))],
        &system,
    );
    faults.fail_writes_under(Some("shared-password/"));
    remember_on_web(&mut app, HOST, 22, LOGIN);
    assert!(matches!(
        app.dialog,
        Some(Dialog::PasswordSaveFailed { .. })
    ));
    assert!(entries(&system).is_empty(), "{:?}", entries(&system));
    assert_eq!(app.shared_password_count("team"), 0);
}

#[test]
fn a_password_whose_entry_fails_and_whose_rollback_fails_is_never_stored_unlisted() {
    capture_logs();
    let dir = tempfile::tempdir().expect("dir");
    let (system, faults) = SystemCredentials::memory_faulty();
    let mut app = app_over(
        dir.path(),
        &[("team", &file(HOST, 22, LOGIN, 5901))],
        &system,
    );
    faults.fail_writes_under(Some("shared-password/"));
    faults.fail_removals(true);
    remember_on_web(&mut app, HOST, 22, LOGIN);
    // At worst a name with nothing behind it: no password is held, listed or not.
    assert!(!store_holds(&system, PASSWORD));
    faults.fail_writes_under(None);
    faults.fail_removals(false);
    app.update(Message::ForgetSharedPasswords("team".to_owned()));
    assert!(entries(&system).is_empty(), "{:?}", entries(&system));
}

#[test]
fn a_password_whose_index_cannot_be_written_is_not_stored() {
    capture_logs();
    let dir = tempfile::tempdir().expect("dir");
    let (system, faults) = SystemCredentials::memory_faulty();
    let mut app = app_over(
        dir.path(),
        &[("team", &file(HOST, 22, LOGIN, 5901))],
        &system,
    );
    faults.fail_writes_under(Some("shared-password-index/"));
    remember_on_web(&mut app, HOST, 22, LOGIN);
    assert!(!store_holds(&system, PASSWORD));
    assert!(matches!(
        app.dialog,
        Some(Dialog::PasswordSaveFailed { .. })
    ));
}

#[tokio::test]
async fn a_deferred_reset_that_cannot_be_noted_for_a_restart_is_reported() {
    capture_logs();
    let dir = tempfile::tempdir().expect("dir");
    let system = SystemCredentials::memory();
    let mut app = app_over(
        dir.path(),
        &[("team", &file(HOST, 22, LOGIN, 5901))],
        &system,
    );
    unlock(&mut app, MASTER).await;
    remember_on_web(&mut app, HOST, 22, LOGIN);
    app.update(Message::LockVault);
    // A folder where the note would be written: the write fails.
    std::fs::create_dir(dir.path().join("shared-passwords-purge.pending")).expect("folder");
    reset_to_defaults(&mut app);
    assert!(
        matches!(app.dialog, Some(Dialog::PasswordPurgeUnrecorded)),
        "{:?}",
        app.dialog
    );
}

#[test]
fn a_reset_done_at_once_does_not_need_its_note() {
    capture_logs();
    let dir = tempfile::tempdir().expect("dir");
    let system = SystemCredentials::memory();
    let mut app = app_over(
        dir.path(),
        &[("team", &file(HOST, 22, LOGIN, 5901))],
        &system,
    );
    remember_on_web(&mut app, HOST, 22, LOGIN);
    std::fs::create_dir(dir.path().join("shared-passwords-purge.pending")).expect("folder");
    reset_to_defaults(&mut app);
    assert!(app.dialog.is_none(), "{:?}", app.dialog);
    assert!(entries(&system).is_empty(), "{:?}", entries(&system));
}

#[test]
fn the_log_does_not_say_that_passwords_are_remembered() {
    capture_logs();
    let dir = tempfile::tempdir().expect("dir");
    let (system, faults) = SystemCredentials::memory_faulty();
    let mut app = app_over(
        dir.path(),
        &[("team", &file(HOST, 22, LOGIN, 5901))],
        &system,
    );
    remember_on_web(&mut app, HOST, 22, LOGIN);
    faults.fail_removals(true);
    app.update(Message::ForgetSharedPasswords("team".to_owned()));
    faults.fail_reads(true);
    app.update(Message::ForgetSharedPasswords("team".to_owned()));
    remove_sources(&mut app);
    app.update(Message::SharedTick);
    for line in LOGGED.0.lock().expect("log").iter() {
        let line = line.to_lowercase();
        assert!(
            !line.contains("remember") && !line.contains("shared password"),
            "logged: {line}"
        );
    }
}

// ---- VNC ------------------------------------------------------------------------------------

const CHALLENGE: [u8; 16] = [
    0x00, 0x11, 0x22, 0x33, 0x44, 0x55, 0x66, 0x77, 0x88, 0x99, 0xaa, 0xbb, 0xcc, 0xdd, 0xee, 0xff,
];
/// Client messages between the server's init and the session.
const OPENING_REQUESTS: usize = 20 + 4 + 16 * 4 + 10;
const WAIT: std::time::Duration = std::time::Duration::from_secs(10);

async fn read_exactly(stream: &mut TcpStream, count: usize) -> Vec<u8> {
    let mut bytes = vec![0; count];
    tokio::time::timeout(WAIT, stream.read_exact(&mut bytes))
        .await
        .expect("in time")
        .expect("read");
    bytes
}

/// A VNC Authentication server that accepts a session or, for `accept` false, refuses the
/// password; an accepted session is then closed.
async fn serve(listener: &TcpListener, accept: bool) {
    let (mut stream, _) = listener.accept().await.expect("accepted");
    stream.write_all(b"RFB 003.008\n").await.expect("version");
    let _ = read_exactly(&mut stream, 12).await;
    stream.write_all(&[1, 2]).await.expect("types");
    let _ = read_exactly(&mut stream, 1).await;
    stream.write_all(&CHALLENGE).await.expect("challenge");
    let _ = read_exactly(&mut stream, 16).await;
    if !accept {
        let reason = b"Authentication failed";
        stream.write_all(&[0, 0, 0, 1]).await.expect("result");
        stream
            .write_all(&u32::try_from(reason.len()).expect("short").to_be_bytes())
            .await
            .expect("length");
        stream.write_all(reason).await.expect("reason");
        return;
    }
    stream.write_all(&[0, 0, 0, 0]).await.expect("result");
    let _ = read_exactly(&mut stream, 1).await;
    let mut init = vec![0, 4, 0, 2];
    init.extend_from_slice(&[32, 24, 0, 1, 0, 255, 0, 255, 0, 255, 16, 8, 0, 0, 0, 0]);
    init.extend_from_slice(&8_u32.to_be_bytes());
    init.extend_from_slice(b"Lab desk");
    stream.write_all(&init).await.expect("init");
    let _ = read_exactly(&mut stream, OPENING_REQUESTS).await;
}

/// What a VNC attempt asked and was told.
struct VncRun {
    /// The box of the password question, if one was shown to the user.
    box_shown: Option<bool>,
    /// The password Heimdall answered with by itself, if it did.
    answered: Option<String>,
}

/// Runs the VNC attempt of `request` against the server in the background: the password
/// question is answered by Heimdall when it remembers one, else by the user typing
/// `VNC_PASSWORD` with the box `remember` says.
async fn vnc_attempt(
    app: &mut App,
    tab: TabId,
    attempt: AttemptId,
    request: VncRequest,
    remember: Option<bool>,
) -> VncRun {
    let registry = AnswerRegistry::default();
    let mut events = vnc_events(request, registry.clone());
    let mut run = VncRun {
        box_shown: None,
        answered: None,
    };
    while let Some(event) = tokio::time::timeout(WAIT, events.next())
        .await
        .expect("in time")
    {
        let question = match &event {
            ConnectionEvent::Question { question, .. } => Some(*question),
            _ => None,
        };
        let effects = event_effects(app, tab, attempt, event);
        for effect in effects {
            if let Effect::Answer { question, answer } = effect {
                if let Some(Answer::Secret(secret)) = &answer {
                    run.answered = Some(secret.expose().to_owned());
                }
                assert!(registry.answer(question, answer));
            }
        }
        if let Some(question) = question
            && run.answered.is_none()
        {
            run.box_shown = app
                .tab(tab)
                .and_then(|found| found.prompts.iter().find(|p| p.question == question))
                .and_then(|prompt| prompt.remember);
            type_password(app, tab, question, VNC_PASSWORD, remember);
            assert!(registry.answer(
                question,
                Some(Answer::Secret(Secret::new(VNC_PASSWORD.to_owned())))
            ));
        }
    }
    run
}

fn event_effects(
    app: &mut App,
    tab: TabId,
    attempt: AttemptId,
    event: ConnectionEvent,
) -> Vec<Effect> {
    app.update(Message::Connection {
        tab,
        attempt,
        event,
    })
}

fn open_vnc(app: &mut App) -> (TabId, AttemptId, VncRequest) {
    let effects = app.update(Message::ConnectShared(id_of(app, "desk")));
    let Some(Effect::ConnectVnc {
        tab,
        attempt,
        request,
    }) = effects.into_iter().next()
    else {
        panic!("no VNC attempt");
    };
    (tab, attempt, *request)
}

#[tokio::test]
async fn a_shared_vnc_password_is_remembered_once_accepted_given_again_and_forgotten_when_refused()
{
    capture_logs();
    let dir = tempfile::tempdir().expect("dir");
    let system = SystemCredentials::memory();
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("listener");
    let port = listener.local_addr().expect("addr").port();
    let mut app = app_over(
        dir.path(),
        &[("team", &file(HOST, 22, LOGIN, port))],
        &system,
    );

    // Asked, with the box cleared; ticked; accepted: kept.
    let (tab, attempt, request) = open_vnc(&mut app);
    let (run, ()) = tokio::join!(
        vnc_attempt(&mut app, tab, attempt, request, Some(true)),
        serve(&listener, true)
    );
    assert_eq!(run.box_shown, Some(false));
    assert_eq!(run.answered, None);
    assert!(store_holds(&system, VNC_PASSWORD));

    // Next time Heimdall answers by itself, and the server takes it.
    let (tab, attempt, request) = open_vnc(&mut app);
    let (run, ()) = tokio::join!(
        vnc_attempt(&mut app, tab, attempt, request, None),
        serve(&listener, true)
    );
    assert_eq!(run.answered.as_deref(), Some(VNC_PASSWORD));
    assert!(store_holds(&system, VNC_PASSWORD));

    // The server now refuses it: forgotten, and the user is asked again.
    let (tab, attempt, request) = open_vnc(&mut app);
    let (run, ()) = tokio::join!(
        vnc_attempt(&mut app, tab, attempt, request, None),
        serve(&listener, false)
    );
    assert_eq!(run.answered.as_deref(), Some(VNC_PASSWORD));
    assert!(!store_holds(&system, VNC_PASSWORD));
    assert!(entries(&system).is_empty(), "{:?}", entries(&system));

    let (tab, attempt, request) = open_vnc(&mut app);
    let (run, ()) = tokio::join!(
        vnc_attempt(&mut app, tab, attempt, request, None),
        serve(&listener, true)
    );
    assert_eq!(run.answered, None);
    assert_eq!(run.box_shown, Some(false));
    assert!(!store_holds(&system, VNC_PASSWORD), "unticked: not kept");
    assert_nothing_logged();
}
