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

//! Which sessions keep a transcript, and when it starts and ends, as the C# Heimdall's
//! session log decides.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use heimdall_app::transcript::{TranscriptContext, TranscriptLines};
use heimdall_app::{
    App, AppConfig, AttemptId, ConnectionEvent, Effect, Message, Notice, SettingsMessage, TabId,
    TabMenuMessage, UiError,
};
use heimdall_core::profile::{
    DEFAULT_WINRM_HTTP_PORT, ProfileId, SshProfile, TelnetProfile, WinRmProfile,
};
use heimdall_core::store::ProfileStore;
use heimdall_ssh::AgentSource;
use heimdall_term::GridSize;

fn app(dir: &Path) -> App {
    let profiles_file = dir.join("profiles.toml");
    let mut store = ProfileStore::open(&profiles_file).expect("store");
    store.merge([SshProfile {
        id: ProfileId::new("web"),
        name: "Web".to_owned(),
        group: None,
        host: "web.lab".to_owned(),
        port: 22,
        username: None,
        key_path: None,
        gateway: None,
        vault_entry: None,
        forwards: heimdall_core::profile::Forwards::default(),
        post_connect: heimdall_core::post_connect::PostConnect::default(),
        forward_agent: false,
    }]);
    store.merge_telnet([TelnetProfile {
        id: ProfileId::new("switch"),
        name: "Switch".to_owned(),
        group: None,
        host: "switch.lab".to_owned(),
        port: 23,
    }]);
    store.merge_winrm([WinRmProfile {
        id: ProfileId::new("dc"),
        name: "DC".to_owned(),
        group: None,
        host: "dc.lab".to_owned(),
        port: DEFAULT_WINRM_HTTP_PORT,
        use_ssl: false,
        skip_certificate_check: false,
        username: None,
    }]);
    store.save().expect("save");
    let mut app = App::new(AppConfig {
        profiles_file,
        known_hosts: dir.join("known_hosts"),
        legacy_dir: None,
        agent: AgentSource::Disabled,
        initial_grid: GridSize { cols: 80, rows: 24 },
        files_start: dir.to_owned(),
        system_credentials: heimdall_app::SystemCredentials::memory(),
    });
    app.set_transcript_lines(TranscriptLines {
        header: Arc::new(|context: &TranscriptContext| {
            format!("start {} {}", context.protocol, context.host)
        }),
        footer: Arc::new(|_, _| "end".to_owned()),
    });
    app
}

fn logging(app: &mut App, on: bool) {
    app.update(Message::Settings(SettingsMessage::SessionLogging(on)));
}

/// Opens `message`'s session and connects it.
fn connected(app: &mut App, message: Message) -> (TabId, AttemptId) {
    let (tab, attempt) = match app.update(message).as_slice() {
        [
            Effect::Connect { tab, attempt, .. }
            | Effect::ConnectTelnet { tab, attempt, .. }
            | Effect::ConnectLocal { tab, attempt, .. },
        ] => (*tab, *attempt),
        other => panic!("{other:?}"),
    };
    event(
        app,
        tab,
        attempt,
        ConnectionEvent::Connected {
            input: Arc::new(NullSink),
        },
    );
    (tab, attempt)
}

fn event(app: &mut App, tab: TabId, attempt: AttemptId, event: ConnectionEvent) {
    app.update(Message::Connection {
        tab,
        attempt,
        event,
    });
}

fn recording(app: &App, tab: TabId) -> bool {
    app.tab(tab).expect("tab").transcript.is_some()
}

/// The transcripts in the default folder, by name.
fn transcripts(dir: &Path) -> Vec<PathBuf> {
    let folder = dir.join("logs").join("sessions");
    let mut found: Vec<PathBuf> = std::fs::read_dir(folder)
        .map(|entries| entries.flatten().map(|entry| entry.path()).collect())
        .unwrap_or_default();
    found.sort();
    found
}

fn read(path: &Path) -> String {
    std::fs::read_to_string(path).expect("read")
}

#[test]
fn with_logging_on_ssh_and_telnet_sessions_keep_a_transcript_until_they_end() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path());
    let (off, off_attempt) = connected(&mut app, Message::OpenProfile(ProfileId::new("web")));
    assert!(!recording(&app, off), "off by default, as the C#");
    assert!(transcripts(dir.path()).is_empty());

    logging(&mut app, true);
    event(
        &mut app,
        off,
        off_attempt,
        ConnectionEvent::Output(b"more".to_vec()),
    );
    assert!(
        !recording(&app, off),
        "a session connected before is left, whatever it says after"
    );
    let (tab, attempt) = connected(&mut app, Message::OpenProfile(ProfileId::new("web")));
    assert!(recording(&app, tab));
    let (telnet, _) = connected(&mut app, Message::OpenTelnet(ProfileId::new("switch")));
    assert!(recording(&app, telnet));
    event(
        &mut app,
        tab,
        attempt,
        ConnectionEvent::Output(b"\x1b[32m$\x1b[0m uptime\r\n".to_vec()),
    );
    event(
        &mut app,
        tab,
        attempt,
        ConnectionEvent::Closed { exit_status: None },
    );
    assert!(!recording(&app, tab), "ended with its session");
    assert_eq!(app.notice(), None, "without a word");

    let files = transcripts(dir.path());
    assert_eq!(files.len(), 2, "{files:?}");
    let ssh = files
        .iter()
        .find(|path| path.to_string_lossy().contains("SSH_web.lab_"))
        .expect("the SSH one");
    let newline = if cfg!(windows) { "\r\n" } else { "\n" };
    assert_eq!(
        read(ssh),
        format!("start SSH web.lab{newline}$ uptime\r\n{newline}end{newline}")
    );
    assert!(
        files
            .iter()
            .any(|path| path.to_string_lossy().contains("TELNET_switch.lab_"))
    );
}

#[test]
fn a_failed_session_and_a_closed_tab_end_their_transcript() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path());
    logging(&mut app, true);
    let (failed, attempt) = connected(&mut app, Message::OpenProfile(ProfileId::new("web")));
    event(
        &mut app,
        failed,
        attempt,
        ConnectionEvent::Failed(UiError::Timeout),
    );
    assert!(!recording(&app, failed));
    let (closed, _) = connected(&mut app, Message::OpenTelnet(ProfileId::new("switch")));
    app.update(Message::RequestCloseTab(closed));
    app.update(Message::ConfirmDialog);
    assert!(app.tab(closed).is_none());
    for path in transcripts(dir.path()) {
        assert!(
            read(&path).ends_with(&format!("end{}", if cfg!(windows) { "\r\n" } else { "\n" })),
            "{path:?}"
        );
    }
    assert_eq!(transcripts(dir.path()).len(), 2);
}

#[test]
fn leaving_the_application_ends_the_transcripts_with_their_footer() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path());
    logging(&mut app, true);
    let (tab, _) = connected(&mut app, Message::OpenProfile(ProfileId::new("web")));
    app.update(Message::WindowCloseRequested);
    let effects = app.update(Message::ConfirmDialog);
    assert!(matches!(effects.as_slice(), [Effect::Exit]), "{effects:?}");
    assert!(!recording(&app, tab), "ended before the window goes");
    let newline = if cfg!(windows) { "\r\n" } else { "\n" };
    let path = transcripts(dir.path()).remove(0);
    assert!(read(&path).ends_with(&format!("end{newline}")));
}

#[test]
fn a_winrm_session_keeps_one_only_when_asked() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path());
    logging(&mut app, true);
    let (tab, _) = connected(&mut app, Message::OpenWinRm(ProfileId::new("dc")));
    assert!(!recording(&app, tab), "its host could echo what sets it up");
    app.update(Message::TabMenu(TabMenuMessage::StartTranscript(tab)));
    assert!(recording(&app, tab), "by hand, it may");
}

#[test]
fn by_hand_a_transcript_starts_and_stops_and_says_so() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path());
    let (tab, _) = connected(&mut app, Message::OpenProfile(ProfileId::new("web")));
    assert!(app.can_start_transcript(app.tab(tab).expect("tab")));
    app.update(Message::TabMenu(TabMenuMessage::StartTranscript(tab)));
    assert!(recording(&app, tab));
    let path = transcripts(dir.path()).remove(0);
    assert_eq!(
        app.notice(),
        Some(&Notice::TranscriptStarted(path.display().to_string()))
    );
    assert!(
        !app.can_start_transcript(app.tab(tab).expect("tab")),
        "one at a time"
    );
    app.update(Message::TabMenu(TabMenuMessage::StartTranscript(tab)));
    assert_eq!(transcripts(dir.path()).len(), 1);

    app.update(Message::TabMenu(TabMenuMessage::StopTranscript(tab)));
    assert!(!recording(&app, tab));
    assert_eq!(app.notice(), Some(&Notice::TranscriptStopped));
    assert!(read(&path).contains("end"));

    // Not for a session still connecting.
    let opened = match app
        .update(Message::OpenProfile(ProfileId::new("web")))
        .as_slice()
    {
        [Effect::Connect { tab, .. }] => *tab,
        other => panic!("{other:?}"),
    };
    app.update(Message::TabMenu(TabMenuMessage::StartTranscript(opened)));
    assert!(!recording(&app, opened));
}

#[test]
fn a_folder_that_cannot_be_made_is_said_and_nothing_is_kept() {
    let dir = tempfile::tempdir().expect("dir");
    // A file where the folder would go.
    std::fs::write(dir.path().join("logs"), "not a folder").expect("written");
    let mut app = app(dir.path());
    logging(&mut app, true);
    let (tab, _) = connected(&mut app, Message::OpenProfile(ProfileId::new("web")));
    assert!(!recording(&app, tab));
    assert!(matches!(app.notice(), Some(Notice::TranscriptFailed(_))));
}

#[test]
fn the_folder_is_the_one_chosen_trimmed_and_kept() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path());
    app.update(Message::Settings(SettingsMessage::SessionLogDirectory(
        "  records  ".to_owned(),
    )));
    assert_eq!(app.settings().session_log_directory, "records");
    logging(&mut app, true);
    let (tab, _) = connected(&mut app, Message::OpenProfile(ProfileId::new("web")));
    assert!(recording(&app, tab));
    assert_eq!(
        std::fs::read_dir(dir.path().join("records"))
            .expect("made")
            .count(),
        1
    );
    let again = self::app(dir.path());
    assert!(again.settings().session_logging, "kept for the next run");
    assert_eq!(again.settings().session_log_directory, "records");
}

#[derive(Debug)]
struct NullSink;

impl heimdall_app::InputSink for NullSink {
    fn write(&self, _bytes: Vec<u8>) -> Result<(), heimdall_ssh::SessionClosed> {
        Ok(())
    }
    fn resize(&self, _size: heimdall_ssh::TerminalSize) -> Result<(), heimdall_ssh::SessionClosed> {
        Ok(())
    }
    fn close(&self) {}
}
