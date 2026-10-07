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

//! A tab detached to a window of its own, as the C# Heimdall's floating window: off the
//! strip and back after its group, refused for a split, closed through the strip,
//! reconnected in place, focused rather than shown, kept in the session snapshot and counted
//! at exit, left out of broadcast input and of docking, its window closed once the tab is
//! gone. A Files tab detaches too, its keys, drops and integrated editor its own there; a
//! split's secondary pane detaches alone, as the C# "Detach Secondary Pane".

use std::path::Path;
use std::sync::{Arc, Mutex};

use heimdall_app::files::{EntryKind, FilesKey, RemoteEntry, Side};
use heimdall_app::local_driver::LocalShell;
use heimdall_app::split::{Axis, Placement, SplitMessage};
use heimdall_app::{
    App, AppConfig, AttemptId, BroadcastMessage, ConnectionEvent, Dialog, Effect, FilesMessage,
    FloatId, FloatMessage, InputSink, KeyInput, Message, Notice, Phase, TabId, TabMenuMessage,
    UiError,
};
use heimdall_core::profile::{ProfileId, RdpProfile, SshProfile};
use heimdall_core::session_snapshot;
use heimdall_core::store::ProfileStore;
use heimdall_files::RemoteSession;
use heimdall_rdp::Framebuffer;
use heimdall_sftp::protocol::{Request, Response, SFTP_VERSION, StatusCode};
use heimdall_sftp::{ClientConfig, RemotePath, SftpClient};
use heimdall_ssh::{AgentSource, SessionClosed, TerminalSize};
use heimdall_term::local::LocalArguments;
use heimdall_term::{GridSize, Key, KeyLocation, Modifiers};
use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};
use tokio::sync::mpsc;

/// The grid every tab opens at until the window says otherwise.
const GRID: GridSize = GridSize { cols: 80, rows: 24 };

/// The bell, as a shell rings it.
const BELL: &[u8] = b"\x07";

/// Records what the application sends to a session.
#[derive(Debug, Default)]
struct RecordingSink {
    written: Mutex<Vec<u8>>,
}

impl RecordingSink {
    fn taken(&self) -> String {
        String::from_utf8(std::mem::take(&mut *self.written.lock().expect("written")))
            .expect("text")
    }
}

impl InputSink for RecordingSink {
    fn write(&self, bytes: Vec<u8>) -> Result<(), SessionClosed> {
        self.written.lock().expect("written").extend(bytes);
        Ok(())
    }
    fn resize(&self, _size: TerminalSize) -> Result<(), SessionClosed> {
        Ok(())
    }
    fn close(&self) {}
}

/// A profile of server `id`; with an account, its shell gets an SFTP pane docked by itself.
fn profile(id: &str, account: bool) -> SshProfile {
    SshProfile {
        id: ProfileId::new(id),
        name: format!("server {id}"),
        group: None,
        host: format!("{id}.lab"),
        port: 22,
        username: account.then(|| "admin".to_owned()),
        key_path: None,
        gateway: None,
        vault_entry: None,
        forwards: heimdall_core::profile::Forwards::default(),
        post_connect: heimdall_core::post_connect::PostConnect::default(),
        forward_agent: false,
        compression: false,
        sftp: false,
        legacy_algorithms: false,
        session_logging: None,
    }
}

/// An RDP profile sharing the clipboard.
fn desktop_profile() -> RdpProfile {
    RdpProfile {
        extras: heimdall_core::profile::RdpExtras::default(),
        id: ProfileId::new("dc"),
        name: "DC".to_owned(),
        group: None,
        host: "dc.lab".to_owned(),
        port: 3389,
        username: Some("admin".to_owned()),
        domain: None,
        allow_tls_only: false,
        gateway: None,
        redirect_clipboard: true,
        redirect_drives: false,
        options: heimdall_core::profile::RdpOptions::default(),
        vault_entry: None,
        forwards: heimdall_core::profile::Forwards::default(),
        follow_defaults: false,
        several_servers: false,
        anti_idle: false,
        auto_reconnect: true,
    }
}

/// The application with profiles `a`, `b`, `c` and `d` (`d` alone with an account), and
/// the desktop `dc`, kept in `dir`.
fn app(dir: &Path) -> App {
    let profiles_file = dir.join("profiles.toml");
    let mut store = ProfileStore::open(&profiles_file).expect("store");
    store.merge(
        ["a", "b", "c"]
            .into_iter()
            .map(|id| profile(id, false))
            .chain([profile("d", true)]),
    );
    store.merge_rdp([desktop_profile()]);
    store.save().expect("save");
    App::new(AppConfig {
        profiles_file,
        known_hosts: dir.join("known_hosts"),
        legacy_dir: None,
        agent: AgentSource::Disabled,
        initial_grid: GRID,
        files_start: dir.to_owned(),
        system_credentials: heimdall_app::SystemCredentials::memory(),
    })
}

fn open_as(app: &mut App, message: Message) -> (TabId, AttemptId) {
    match app.update(message).as_slice() {
        [Effect::Connect { tab, attempt, .. }] => (*tab, *attempt),
        other => panic!("expected one Connect, got {other:?}"),
    }
}

/// A shell tab of profile `id`, still connecting.
fn open(app: &mut App, id: &str) -> (TabId, AttemptId) {
    open_as(app, Message::OpenProfile(ProfileId::new(id)))
}

/// The session of `tab` comes up; what it is sent, and the effects its coming up asked for.
fn connect(app: &mut App, tab: TabId, attempt: AttemptId) -> (Arc<RecordingSink>, Vec<Effect>) {
    let sink = Arc::new(RecordingSink::default());
    let effects = app.update(Message::Connection {
        tab,
        attempt,
        event: ConnectionEvent::Connected {
            input: sink.clone(),
        },
    });
    assert_eq!(app.tab(tab).expect("tab").phase, Phase::Connected);
    (sink, effects)
}

/// Detaches `tab`, which opens a window; its key.
fn detach(app: &mut App, tab: TabId) -> FloatId {
    let effects = app.update(Message::Float(FloatMessage::Detach(tab)));
    let key = app.floating_of(tab).expect("detached");
    assert!(
        matches!(effects.first(), Some(Effect::OpenWindow(opened)) if *opened == key),
        "{effects:?}"
    );
    key
}

fn strip(app: &App) -> Vec<TabId> {
    app.strip().iter().map(|tab| tab.id).collect()
}

fn ids(app: &App) -> Vec<TabId> {
    app.tabs.iter().map(|tab| tab.id).collect()
}

fn closes(effects: &[Effect], key: FloatId) -> bool {
    effects
        .iter()
        .any(|effect| matches!(effect, Effect::CloseWindow(closed) if *closed == key))
}

fn pin(app: &mut App, tab: TabId) {
    app.update(Message::TabMenu(TabMenuMessage::Pin(tab)));
}

#[test]
fn a_detached_tab_leaves_the_strip_and_comes_back_after_its_group() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path());
    let (a, _) = open(&mut app, "a");
    let (b, _) = open(&mut app, "b");
    let (c, _) = open(&mut app, "c");
    let (d, _) = open(&mut app, "d");
    pin(&mut app, a);
    pin(&mut app, b);
    assert_eq!(strip(&app), [a, b, c, d]);

    // The pinned tab shown goes; the keyboard goes to the tab taking its place.
    app.update(Message::SelectTab(a));
    let key_a = detach(&mut app, a);
    assert_eq!(strip(&app), [b, c, d], "off the strip");
    assert_eq!(app.tabs.len(), 4, "still a tab, with its session");
    assert_eq!(app.active, Some(b));
    assert!(
        app.tab(a).expect("a").pinned,
        "its group kept for its return"
    );

    // A tab not shown goes, the keyboard staying; the last one shown goes to the one before.
    let key_c = detach(&mut app, c);
    assert_eq!(app.active, Some(b));
    app.update(Message::SelectTab(d));
    let key_d = detach(&mut app, d);
    assert_eq!(strip(&app), [b]);
    assert_eq!(app.active, Some(b), "the tab before the last one");

    // Pinned, back after the last pinned tab, as the C# `ReintroduceSession`; shown.
    let effects = app.update(Message::Float(FloatMessage::Reattach(key_a)));
    assert!(closes(&effects, key_a), "{effects:?}");
    assert!(
        effects
            .iter()
            .any(|effect| matches!(effect, Effect::FocusMainWindow)),
        "{effects:?}"
    );
    assert_eq!(strip(&app), [b, a]);
    assert_eq!(app.active, Some(a));
    assert!(!app.is_floating(a));

    // Not pinned, back at the end.
    app.update(Message::Float(FloatMessage::Reattach(key_d)));
    app.update(Message::Float(FloatMessage::Reattach(key_c)));
    assert_eq!(strip(&app), [b, a, d, c]);
    assert_eq!(app.active, Some(c));
    assert!(app.floating().is_empty());
    // A window already gone: nothing.
    assert!(
        app.update(Message::Float(FloatMessage::Reattach(key_c)))
            .is_empty()
    );
}

#[test]
fn the_only_tab_detached_leaves_no_tab_shown() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path());
    let (a, _) = open(&mut app, "a");
    detach(&mut app, a);
    assert!(strip(&app).is_empty());
    assert_eq!(app.active, None);
    assert!(app.shown_tab().is_none());
}

#[test]
fn a_split_tab_and_a_docked_pane_are_refused_and_said() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path());
    let (a, _) = open(&mut app, "a");
    let (b, _) = open(&mut app, "b");
    app.update(Message::Split(SplitMessage::Merge {
        host: a,
        tab: b,
        axis: Axis::SideBySide,
        placement: Placement::Second,
    }));
    for tab in [a, b] {
        let effects = app.update(Message::Float(FloatMessage::Detach(tab)));
        assert!(effects.is_empty(), "{effects:?}");
        assert!(!app.is_floating(tab));
        assert_eq!(app.notice(), Some(&Notice::DetachSplitRefused));
    }
    assert!(!app.can_detach(app.tab(a).expect("host")));
    assert!(!app.can_detach(app.tab(b).expect("docked")));
    let (c, _) = open(&mut app, "c");
    assert!(app.can_detach(app.tab(c).expect("shell")));
    detach(&mut app, c);
    assert!(!app.can_detach(app.tab(c).expect("detached")));
    // Detached already: its window is focused.
    let key = app.floating_of(c).expect("detached");
    let effects = app.update(Message::Float(FloatMessage::Detach(c)));
    assert!(
        matches!(effects.as_slice(), [Effect::FocusWindow(focused)] if *focused == key),
        "{effects:?}"
    );
}

#[test]
fn closing_the_window_brings_the_tab_back_then_asks_as_its_tab_would() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path());
    let (a, _) = open(&mut app, "a");
    let (b, attempt) = open(&mut app, "b");
    connect(&mut app, b, attempt);
    let key = detach(&mut app, b);
    assert_eq!(app.active, Some(a));

    // Declined: back on the strip, shown, as the C# leaves it in the main window.
    let effects = app.update(Message::Float(FloatMessage::CloseRequested(key)));
    assert!(closes(&effects, key), "{effects:?}");
    assert!(!app.is_floating(b));
    assert_eq!(strip(&app), [a, b]);
    assert_eq!(app.active, Some(b));
    assert_eq!(app.dialog, Some(Dialog::ConfirmCloseTab(b)));
    app.update(Message::DismissDialog);
    assert!(app.tab(b).is_some(), "kept, in the main window");
    assert!(app.floating().is_empty());

    // Confirmed: closed.
    let key = detach(&mut app, b);
    app.update(Message::Float(FloatMessage::CloseRequested(key)));
    app.update(Message::ConfirmDialog);
    assert!(app.tab(b).is_none());
    assert_eq!(strip(&app), [a]);

    // Nothing to lose: closed without a question.
    let (c, _) = open(&mut app, "c");
    let key = detach(&mut app, c);
    let effects = app.update(Message::Float(FloatMessage::CloseRequested(key)));
    assert!(closes(&effects, key), "{effects:?}");
    assert!(app.tab(c).is_none());
    assert_eq!(app.dialog, None);
}

#[test]
fn a_detached_tab_reconnected_stays_in_its_window_and_the_keyboard_stays() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path());
    let (a, _) = open(&mut app, "a");
    let (b, attempt) = open(&mut app, "b");
    let key = detach(&mut app, b);
    app.update(Message::Connection {
        tab: b,
        attempt,
        event: ConnectionEvent::Failed(UiError::Timeout),
    });
    let (reopened, _) = open_as(&mut app, Message::ReconnectTab(b));
    assert_ne!(reopened, b);
    assert!(app.tab(b).is_none());
    assert_eq!(app.floating_of(reopened), Some(key), "the same window");
    assert_eq!(app.active, Some(a), "the keyboard stays in the main window");
    assert_eq!(strip(&app), [a]);
}

#[test]
fn showing_a_detached_tab_focuses_its_window_and_leaves_the_keyboard() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path());
    let (a, _) = open(&mut app, "a");
    let (b, _) = open(&mut app, "b");
    let key = detach(&mut app, b);
    assert_eq!(app.active, Some(a));
    for message in [
        Message::SelectTab(b),
        Message::Split(SplitMessage::Focus(b)),
    ] {
        let effects = app.update(message);
        assert!(
            matches!(effects.as_slice(), [Effect::FocusWindow(focused)] if *focused == key),
            "{effects:?}"
        );
        assert_eq!(app.active, Some(a));
    }
}

#[test]
fn the_keyboard_is_never_on_a_detached_tab() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path());
    let (a, _) = open(&mut app, "a");
    let (b, attempt_b) = open(&mut app, "b");
    let (c, _) = open(&mut app, "c");
    let mut keys = Vec::new();
    let held = |app: &App| assert!(app.floating_invariant_holds(), "{:?}", app.active);
    keys.push(detach(&mut app, c));
    held(&app);
    keys.push(detach(&mut app, b));
    held(&app);
    for message in [
        Message::SelectTab(c),
        Message::SelectTab(b),
        Message::Split(SplitMessage::Focus(c)),
        Message::Connection {
            tab: b,
            attempt: attempt_b,
            event: ConnectionEvent::Failed(UiError::Timeout),
        },
        Message::ReconnectTab(b),
        Message::RequestCloseTab(a),
        Message::SelectTab(c),
    ] {
        app.update(message);
        held(&app);
    }
    assert!(app.tab(a).is_none(), "nothing to lose: closed");
    assert!(strip(&app).is_empty());
    assert_eq!(app.active, None, "only detached tabs left");
    for key in keys {
        app.update(Message::Float(FloatMessage::Reattach(key)));
        held(&app);
    }
    assert_eq!(app.floating().len(), 0);
    assert!(app.active.is_some());
}

#[test]
fn the_snapshot_lists_the_detached_tabs_after_the_strip_without_moving_them() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path());
    let (a, _) = open(&mut app, "a");
    open(&mut app, "b");
    open(&mut app, "c");
    let key = detach(&mut app, a);
    let before = ids(&app);
    let effects = app.update(Message::WindowCloseRequested);
    assert!(
        matches!(effects.as_slice(), [Effect::Exit]),
        "nothing live: no question; {effects:?}"
    );
    assert_eq!(ids(&app), before, "the tabs as they were");
    assert_eq!(app.floating_of(a), Some(key), "still detached");
    let path = session_snapshot::snapshot_path(&dir.path().join("profiles.toml"));
    let kept: Vec<String> = session_snapshot::load(&path)
        .expect("snapshot")
        .sessions
        .iter()
        .map(|entry| entry.profile.as_str().to_owned())
        .collect();
    assert_eq!(kept, ["b", "c", "a"], "the strip, then the detached tab");
}

#[test]
fn quitting_counts_the_live_detached_tabs() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path());
    open(&mut app, "a");
    let (b, attempt) = open(&mut app, "b");
    connect(&mut app, b, attempt);
    detach(&mut app, b);
    assert!(app.update(Message::WindowCloseRequested).is_empty());
    assert_eq!(
        app.dialog,
        Some(Dialog::ConfirmExit {
            live: 1,
            unsaved: 0
        })
    );
}

fn typed(app: &mut App, tab: TabId, c: char) {
    app.update(Message::Key {
        tab,
        input: KeyInput {
            key: Key::Character(c),
            text: Some(c.to_string()),
            physical_digit: None,
            location: KeyLocation::Standard,
            modifiers: Modifiers::default(),
        },
    });
}

#[test]
fn broadcast_input_leaves_the_detached_tabs_out() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path());
    let (a, attempt_a) = open(&mut app, "a");
    let (sink_a, _) = connect(&mut app, a, attempt_a);
    let (b, attempt_b) = open(&mut app, "b");
    let (sink_b, _) = connect(&mut app, b, attempt_b);
    let (c, attempt_c) = open(&mut app, "c");
    let (sink_c, _) = connect(&mut app, c, attempt_c);
    detach(&mut app, c);
    app.update(Message::Broadcast(BroadcastMessage::Scope));
    app.update(Message::Broadcast(BroadcastMessage::Toggle));
    app.update(Message::ConfirmDialog);
    assert!(app.broadcasting());
    typed(&mut app, a, 'x');
    assert_eq!(
        [sink_a.taken(), sink_b.taken(), sink_c.taken()],
        ["x", "x", ""],
        "the main strip only, as the C#"
    );
    typed(&mut app, c, 'y');
    assert_eq!(
        [sink_a.taken(), sink_b.taken(), sink_c.taken()],
        ["", "", "y"],
        "typed in its window: itself alone"
    );
}

#[test]
fn a_tab_closed_elsewhere_closes_its_window() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path());
    let (a, _) = open(&mut app, "a");
    let (b, _) = open(&mut app, "b");
    let key = detach(&mut app, b);
    // The card's Close in the window, or a group closed from the strip: the tab goes.
    let effects = app.update(Message::RequestCloseTab(b));
    assert!(closes(&effects, key), "{effects:?}");
    assert!(app.floating().is_empty());
    assert_eq!(strip(&app), [a]);
    assert_eq!(app.active, Some(a));
}

#[test]
fn a_detached_shell_gets_no_pane_docked_by_itself() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path());
    let (d, attempt) = open(&mut app, "d");
    detach(&mut app, d);
    let (_, effects) = connect(&mut app, d, attempt);
    assert!(effects.is_empty(), "no SFTP pane opened: {effects:?}");
    assert_eq!(app.tabs.len(), 1);
    assert!(app.tab(d).expect("shell").layout.is_none());
    assert!(app.is_floating(d));

    // On the strip, the same shell gets its pane, as the C# auto-open.
    let (shell, attempt) = open(&mut app, "d");
    let (_, effects) = connect(&mut app, shell, attempt);
    assert!(
        effects
            .iter()
            .any(|effect| matches!(effect, Effect::Connect { .. })),
        "{effects:?}"
    );
    assert!(app.tab(shell).expect("shell").layout.is_some());
}

#[test]
fn the_users_merge_brings_a_detached_tab_back_first() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path());
    let (a, _) = open(&mut app, "a");
    let (b, _) = open(&mut app, "b");
    let key = detach(&mut app, b);
    assert!(app.merge_candidates(a).is_empty(), "a detached tab is none");
    assert!(app.merge_candidates(b).is_empty(), "nor a host");
    let effects = app.update(Message::Split(SplitMessage::Merge {
        host: a,
        tab: b,
        axis: Axis::SideBySide,
        placement: Placement::Second,
    }));
    assert!(closes(&effects, key), "{effects:?}");
    assert!(!app.is_floating(b));
    assert!(app.is_docked(b), "merged once back");
    assert_eq!(app.panes_of(a), [a, b]);
    assert!(app.floating_invariant_holds());

    // As the host of a split chosen in Quick Connect, likewise.
    let (c, _) = open(&mut app, "c");
    let key = detach(&mut app, c);
    let effects = app.update(Message::Split(SplitMessage::QuickConnect {
        host: c,
        axis: Axis::Stacked,
        result: heimdall_app::QuickResult::Ssh {
            username: None,
            host: "e.lab".to_owned(),
            port: 22,
        },
    }));
    assert!(closes(&effects, key), "{effects:?}");
    assert!(!app.is_floating(c));
    assert!(app.in_split(c));
}

#[test]
fn a_detached_tab_hears_its_bell_no_more_than_the_tab_shown() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path());
    let (a, attempt_a) = open(&mut app, "a");
    connect(&mut app, a, attempt_a);
    let (b, attempt_b) = open(&mut app, "b");
    connect(&mut app, b, attempt_b);
    detach(&mut app, b);
    app.update(Message::SelectTab(a));
    let (c, attempt_c) = open(&mut app, "c");
    connect(&mut app, c, attempt_c);
    for (tab, attempt) in [(b, attempt_b), (a, attempt_a)] {
        app.update(Message::Connection {
            tab,
            attempt,
            event: ConnectionEvent::Output(BELL.to_vec()),
        });
    }
    assert!(
        !app.tab(b).expect("detached").bell,
        "in sight in its window"
    );
    assert!(app.tab(a).expect("background").bell, "a tab not shown");
}

#[test]
fn a_detached_desktop_is_offered_the_clipboard_when_its_window_gets_the_focus() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path());
    let (shell, _) = open(&mut app, "a");
    let (tab, attempt) = match app
        .update(Message::OpenRdp(ProfileId::new("dc")))
        .as_slice()
    {
        [Effect::ConnectRdp { tab, attempt, .. }] => (*tab, *attempt),
        other => panic!("{other:?}"),
    };
    let (input, _) = mpsc::unbounded_channel();
    let (offers, _received) = mpsc::unbounded_channel();
    app.update(Message::Connection {
        tab,
        attempt,
        event: ConnectionEvent::RdpReady {
            framebuffer: Framebuffer::new(64, 48),
            input,
            size: tokio::sync::watch::channel(None).0,
            clipboard: Some(offers),
        },
    });
    let key = detach(&mut app, tab);
    assert_eq!(app.active, Some(shell));
    let effects = app.update(Message::Float(FloatMessage::Focused { key, focused: true }));
    assert!(
        matches!(effects.as_slice(), [Effect::ReadDesktopClipboard { tab: read }] if *read == tab),
        "{effects:?}"
    );
    assert!(
        app.update(Message::Float(FloatMessage::Focused {
            key,
            focused: false
        }))
        .is_empty()
    );
    // The main window's focus is the shell's: the desktop is not offered it from there.
    assert!(app.update(Message::WindowFocus(true)).is_empty());
}

#[test]
fn a_files_tab_detaches_and_comes_back() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path());
    let (a, _) = open(&mut app, "a");
    let (files, _) = open_as(&mut app, Message::OpenFiles(ProfileId::new("c")));
    assert!(app.can_detach(app.tab(files).expect("files")));
    let key = detach(&mut app, files);
    assert_eq!(strip(&app), [a]);
    assert_eq!(app.active, Some(a), "the tab taking its place");
    assert!(app.floating_invariant_holds());
    let effects = app.update(Message::Float(FloatMessage::Reattach(key)));
    assert!(closes(&effects, key), "{effects:?}");
    assert_eq!(strip(&app), [a, files]);
    assert_eq!(app.active, Some(files), "shown once back");
}

/// The SFTP pane docked by itself beside shell `d` once connected, and the shell.
fn shell_with_sftp(app: &mut App) -> (TabId, TabId) {
    let (shell, attempt) = open(app, "d");
    connect(app, shell, attempt);
    let pane = app
        .tab(shell)
        .and_then(|tab| tab.layout.as_ref())
        .and_then(heimdall_app::split::Layout::secondary)
        .expect("the SFTP pane docked");
    assert!(app.tab(pane).expect("pane").files.is_some());
    (shell, pane)
}

#[test]
fn detach_secondary_takes_an_ssh_shells_sftp_pane_to_its_own_window() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path());
    let (a, _) = open(&mut app, "a");
    let (shell, pane) = shell_with_sftp(&mut app);
    assert_eq!(app.detachable_secondary(shell), Some(pane));
    assert_eq!(app.detachable_secondary(pane), None, "a docked pane: none");
    assert_eq!(app.detachable_secondary(a), None, "not split: none");
    // The keyboard on the pane: it goes back to the shell.
    app.update(Message::Split(SplitMessage::Focus(pane)));
    assert_eq!(app.active, Some(pane));

    let effects = app.update(Message::Float(FloatMessage::DetachSecondary(shell)));
    let key = app.floating_of(pane).expect("the pane detached");
    assert!(
        matches!(effects.as_slice(), [Effect::OpenWindow(opened)] if *opened == key),
        "{effects:?}"
    );
    assert!(app.tab(shell).expect("shell").layout.is_none(), "unsplit");
    assert!(!app.in_split(shell) && !app.in_split(pane));
    assert_eq!(strip(&app), [a, shell], "the shell alone on the strip");
    assert_eq!(app.active, Some(shell), "the shell keeps the keyboard");
    assert!(app.floating_invariant_holds());
    assert_eq!(app.detachable_secondary(shell), None);

    // Back on the strip, a Files tab of its own.
    app.update(Message::Float(FloatMessage::Reattach(key)));
    assert_eq!(strip(&app), [a, shell, pane]);
    assert!(!app.is_docked(pane));
}

#[test]
fn detach_secondary_leaves_the_keyboard_where_it_was_outside_the_split() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path());
    let (shell, pane) = shell_with_sftp(&mut app);
    let (a, _) = open(&mut app, "a");
    assert_eq!(app.active, Some(a));
    app.update(Message::Float(FloatMessage::DetachSecondary(shell)));
    assert!(app.is_floating(pane));
    assert_eq!(app.active, Some(a), "the tab shown stays");
    // Not split any more: nothing.
    assert!(
        app.update(Message::Float(FloatMessage::DetachSecondary(shell)))
            .is_empty()
    );
}

/// A local shell started, with the file browser docked beside it; the shell and the browser.
fn shell_with_browser(app: &mut App, folder: &Path) -> (TabId, TabId) {
    let local = LocalShell {
        name: "Shell".to_owned(),
        program: None,
        arguments: LocalArguments::List(Vec::new()),
        working_directory: Some(folder.to_owned()),
        environment: Vec::new(),
    };
    let (shell, attempt) = match app.update(Message::OpenLocal(local)).as_slice() {
        [Effect::ConnectLocal { tab, attempt, .. }] => (*tab, *attempt),
        other => panic!("expected one ConnectLocal, got {other:?}"),
    };
    let (_, effects) = connect(app, shell, attempt);
    let browser = match effects.as_slice() {
        [Effect::ListLocal { tab, .. }] => *tab,
        other => panic!("expected the browser's listing, got {other:?}"),
    };
    assert_eq!(app.panes_of(shell), [shell, browser]);
    (shell, browser)
}

#[test]
fn detach_secondary_takes_a_local_shells_browser_to_its_own_window() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path());
    let (shell, browser) = shell_with_browser(&mut app, dir.path());
    assert_eq!(app.active, Some(shell));
    app.update(Message::Float(FloatMessage::DetachSecondary(shell)));
    assert!(app.is_floating(browser));
    assert!(app.tab(shell).expect("shell").layout.is_none());
    assert_eq!(strip(&app), [shell]);
    assert_eq!(app.active, Some(shell), "the shell keeps the keyboard");
    assert!(
        app.tab(shell).expect("shell").local_browser_closed,
        "taken away by the user: none docked again when the shell restarts"
    );
}

#[test]
fn detach_secondary_of_a_split_whose_host_is_its_secondary_hands_the_split_over() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path());
    let (a, _) = open(&mut app, "a");
    let (b, _) = open(&mut app, "b");
    app.update(Message::Split(SplitMessage::Merge {
        host: a,
        tab: b,
        axis: Axis::SideBySide,
        placement: Placement::Second,
    }));
    // Swapped, the host is the second side's first pane: the secondary.
    app.update(Message::Split(SplitMessage::Swap(a)));
    assert_eq!(app.detachable_secondary(a), Some(a));
    app.update(Message::Float(FloatMessage::DetachSecondary(a)));
    assert!(app.is_floating(a));
    assert_eq!(strip(&app), [b], "the pane left takes the host's place");
    assert!(app.tab(b).expect("b").layout.is_none());
    assert!(app.floating_invariant_holds());
    assert!(app.active.is_some_and(|active| !app.is_floating(active)));
}

/// An SFTP server that answers the start and refuses every request after it.
async fn idle_client() -> RemoteSession {
    let (client_end, mut server) = tokio::io::duplex(4096);
    tokio::spawn(async move {
        let mut length = [0; 4];
        server.read_exact(&mut length).await.expect("init length");
        let mut body = vec![0; u32::from_be_bytes(length) as usize];
        server.read_exact(&mut body).await.expect("init");
        assert!(matches!(Request::decode(&body), Ok(Request::Init { .. })));
        let version = Response::Version {
            version: SFTP_VERSION,
            extensions: Vec::new(),
        };
        server.write_all(&version.encode()).await.expect("version");
        loop {
            if server.read_exact(&mut length).await.is_err() {
                std::future::pending::<()>().await;
            }
            let mut body = vec![0; u32::from_be_bytes(length) as usize];
            server.read_exact(&mut body).await.expect("request");
            let id = body
                .get(1..5)
                .and_then(|id| <[u8; 4]>::try_from(id).ok())
                .map_or(0, u32::from_be_bytes);
            let refused = Response::Status {
                id,
                code: StatusCode::NoSuchFile,
                message: Vec::new(),
            };
            server.write_all(&refused.encode()).await.expect("status");
        }
    });
    RemoteSession::Sftp(
        SftpClient::start(client_end, ClientConfig::default())
            .await
            .expect("started"),
    )
}

/// A Files tab of profile `id`, connected, its server's folder `/srv` listing `a.txt` and
/// `b.txt`, its server's pane with the keyboard.
async fn files_tab(app: &mut App, id: &str) -> TabId {
    let (tab, attempt) = open_as(app, Message::OpenFiles(ProfileId::new(id)));
    app.update(Message::Connection {
        tab,
        attempt,
        event: ConnectionEvent::FilesReady {
            client: idle_client().await,
            shell: None,
        },
    });
    let entry = |name: &str| RemoteEntry {
        name: name.as_bytes().to_vec(),
        label: name.to_owned(),
        kind: EntryKind::File,
        size: Some(4),
        modified: None,
        permissions: None,
        owner: None,
        group: None,
        inode: None,
    };
    app.update(Message::Files(FilesMessage::RemoteListed {
        tab,
        result: Ok((
            RemotePath::from("/srv"),
            vec![entry("a.txt"), entry("b.txt")],
        )),
    }));
    app.update(Message::Files(FilesMessage::Key {
        tab,
        key: FilesKey::Focus(Side::Remote),
    }));
    tab
}

fn remote_selected(app: &App, tab: TabId) -> Option<usize> {
    app.tab(tab)
        .and_then(|found| found.files.as_deref())
        .and_then(|files| files.remote.selected)
}

#[tokio::test]
async fn a_detached_files_tabs_keys_and_drops_reach_it_alone() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path());
    let floating = files_tab(&mut app, "a").await;
    let shown = files_tab(&mut app, "b").await;
    detach(&mut app, floating);
    assert_eq!(app.active, Some(shown), "another Files tab shown in main");
    let (before_floating, before_shown) = (
        remote_selected(&app, floating),
        remote_selected(&app, shown),
    );

    app.update(Message::Files(FilesMessage::Key {
        tab: floating,
        key: FilesKey::Last,
    }));
    assert_eq!(remote_selected(&app, floating), Some(1), "its own list");
    assert_ne!(before_floating, Some(1));
    assert_eq!(
        remote_selected(&app, shown),
        before_shown,
        "the main one's untouched"
    );
    assert_eq!(app.active, Some(shown));

    let outside = tempfile::tempdir().expect("dir");
    let file = outside.path().join("report.pdf");
    std::fs::write(&file, b"12345").expect("written");
    let effects = app.update(Message::Files(FilesMessage::Dropped {
        tab: floating,
        path: file,
    }));
    assert!(
        matches!(effects.as_slice(), [Effect::PlanTransfer { tab, .. }] if *tab == floating),
        "{effects:?}"
    );
    assert_eq!(app.active, Some(shown));
    assert!(app.floating_invariant_holds());
}

#[tokio::test]
async fn the_integrated_editor_of_a_detached_files_tab_saves_its_own_file() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path());
    let floating = files_tab(&mut app, "a").await;
    let shown = files_tab(&mut app, "b").await;
    app.update(Message::Files(FilesMessage::Select {
        tab: floating,
        side: Side::Remote,
        index: 0,
    }));
    let id = match app
        .update(Message::Files(FilesMessage::EditIntegrated {
            tab: floating,
        }))
        .as_slice()
    {
        [Effect::OpenEditor { id, .. }] => *id,
        other => panic!("{other:?}"),
    };
    app.update(Message::Files(FilesMessage::EditorOpened {
        tab: floating,
        id,
        result: Ok((
            heimdall_app::text_codec::TextEncoding::Utf8 { bom: false },
            heimdall_files::Fingerprint {
                size: Some(4),
                modified: Some(1),
                permissions: Some(0o100_644),
                uid_gid: Some((1000, 1000)),
            },
        )),
    }));
    detach(&mut app, floating);
    app.update(Message::SelectTab(shown));
    let effects = app.update(Message::Files(FilesMessage::EditorSave {
        tab: floating,
        id,
        text: "text".to_owned(),
        overwrite: false,
    }));
    assert!(
        matches!(
            effects.as_slice(),
            [Effect::SaveEditor { tab, id: saved, remote, .. }]
                if *tab == floating && *saved == id && remote.as_bytes() == b"/srv/a.txt"
        ),
        "{effects:?}"
    );
    assert!(app.is_floating(floating));
    assert_eq!(app.active, Some(shown));
}
