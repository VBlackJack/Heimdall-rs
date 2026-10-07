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

//! A tab split in panes, as the C# Heimdall's: merged, unsplit, swapped, turned, resized;
//! closed whole or a pane at a time, asking as its panes would; reconnected in place; and
//! the strip, the tunnels panel, the notice and the session snapshot following the tab
//! shown rather than the pane with the keyboard.

use std::path::Path;
use std::sync::Arc;

use heimdall_app::files::{EntryKind, RemoteEntry, Side};
use heimdall_app::split::{
    Axis, DEFAULT_RATIO, MAX_PANES, MAX_RATIO, MIN_RATIO, Placement, SplitMessage,
};
use heimdall_app::{
    App, AppConfig, AttemptId, ConnectionEvent, Dialog, Effect, FilesMessage, InputSink, Message,
    Notice, Phase, TabGroup, TabId, TabMenuMessage, TunnelMessage, UiError,
};
use heimdall_core::profile::{ProfileId, SshProfile};
use heimdall_core::store::ProfileStore;
use heimdall_files::RemoteSession;
use heimdall_sftp::protocol::{Request, Response, SFTP_VERSION, StatusCode};
use heimdall_sftp::{ClientConfig, RemotePath, SftpClient};
use heimdall_ssh::{AgentSource, SessionClosed, TerminalSize};
use heimdall_term::{CellPixels, GridSize};
use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};

/// The grid every tab opens at until the window says otherwise.
const GRID: GridSize = GridSize { cols: 80, rows: 24 };

#[derive(Debug, Default)]
struct Sink;

impl InputSink for Sink {
    fn write(&self, _bytes: Vec<u8>) -> Result<(), SessionClosed> {
        Ok(())
    }
    fn resize(&self, _size: TerminalSize) -> Result<(), SessionClosed> {
        Ok(())
    }
    fn close(&self) {}
}

fn profile(id: &str) -> SshProfile {
    SshProfile {
        id: ProfileId::new(id),
        name: format!("server {id}"),
        group: None,
        host: format!("{id}.lab"),
        port: 22,
        username: Some("admin".to_owned()),
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
        ssh_mode: heimdall_core::profile::SshMode::Embedded,
        x11_forwarding: false,
    }
}

fn app(dir: &Path) -> App {
    app_with(dir, &[])
}

/// The application with profiles `a`, `b`, `c` and `more`, kept in `dir`.
fn app_with(dir: &Path, more: &[String]) -> App {
    let profiles_file = dir.join("profiles.toml");
    let mut store = ProfileStore::open(&profiles_file).expect("store");
    store.merge(
        ["a", "b", "c"]
            .into_iter()
            .chain(more.iter().map(String::as_str))
            .map(profile),
    );
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

fn connect(app: &mut App, tab: TabId, attempt: AttemptId) {
    app.update(Message::Connection {
        tab,
        attempt,
        event: ConnectionEvent::Connected {
            input: Arc::new(Sink),
        },
    });
    assert_eq!(app.tab(tab).expect("tab").phase, Phase::Connected);
}

fn fail(app: &mut App, tab: TabId, attempt: AttemptId) {
    app.update(Message::Connection {
        tab,
        attempt,
        event: ConnectionEvent::Failed(UiError::Timeout),
    });
}

fn split(app: &mut App, message: SplitMessage) -> Vec<Effect> {
    app.update(Message::Split(message))
}

/// Tab `b` merged into tab `a`, side by side.
fn merged(app: &mut App) -> ((TabId, AttemptId), (TabId, AttemptId)) {
    let a = open(app, "a");
    let b = open(app, "b");
    split(
        app,
        SplitMessage::Merge {
            host: a.0,
            tab: b.0,
            axis: Axis::SideBySide,
            placement: Placement::Second,
        },
    );
    (a, b)
}

fn strip(app: &App) -> Vec<TabId> {
    app.strip().iter().map(|tab| tab.id).collect()
}

fn layout(app: &App, host: TabId) -> heimdall_app::split::Layout {
    app.tab(host)
        .and_then(|tab| tab.layout.clone())
        .expect("split")
}

#[test]
fn a_merged_tab_leaves_the_strip_for_a_pane_of_its_host() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path());
    let ((a, _), (b, _)) = merged(&mut app);
    assert_eq!(strip(&app), [a], "docked: off the strip");
    assert_eq!(app.tabs.len(), 2, "still a tab, with its session");
    assert!(app.is_docked(b) && !app.is_docked(a));
    assert_eq!(app.host_of(b), Some(a));
    assert_eq!(app.host_of(a), Some(a));
    let split = layout(&app, a);
    assert_eq!(split.leaves(), [a, b], "the merged tab second, as the C#");
    assert_eq!(split.axis(), Some(Axis::SideBySide));
    assert_eq!(split.ratio(), Some(DEFAULT_RATIO));
    assert_eq!(
        app.shown_tab().map(|tab| tab.id),
        Some(a),
        "the split shown"
    );
    assert_eq!(
        app.active,
        Some(b),
        "the merged tab was shown: it keeps the keyboard"
    );
    assert_eq!(app.panes_of(a), [a, b]);
    assert!(app.in_split(a) && app.in_split(b));
}

#[test]
fn merging_clears_the_pin_and_takes_a_tab_still_connecting() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path());
    let (a, _) = open(&mut app, "a");
    let (b, _) = open(&mut app, "b");
    app.update(Message::TabMenu(TabMenuMessage::Pin(b)));
    assert!(app.tab(b).expect("b").pinned);
    assert_eq!(app.tab(b).expect("b").phase, Phase::Connecting);
    split(
        &mut app,
        SplitMessage::Merge {
            host: a,
            tab: b,
            axis: Axis::Stacked,
            placement: Placement::Second,
        },
    );
    assert!(
        !app.tab(b).expect("b").pinned,
        "off the strip, never pinned"
    );
    assert_eq!(layout(&app, a).axis(), Some(Axis::Stacked));
}

#[test]
fn a_merge_is_refused_onto_itself_from_a_split_and_beyond_the_most_panes() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path());
    let ((a, _), (b, _)) = merged(&mut app);
    let (c, _) = open(&mut app, "c");
    for (host, tab) in [(c, c), (c, b), (b, c), (c, a)] {
        split(
            &mut app,
            SplitMessage::Merge {
                host,
                tab,
                axis: Axis::Stacked,
                placement: Placement::Second,
            },
        );
        assert!(app.tab(c).expect("c").layout.is_none(), "{host:?} {tab:?}");
        assert_eq!(layout(&app, a).leaves(), [a, b]);
    }
    assert_eq!(app.notice(), None, "refused without a word");
    app.update(Message::SelectTab(a));
    split(
        &mut app,
        SplitMessage::Merge {
            host: a,
            tab: c,
            axis: Axis::Stacked,
            placement: Placement::Second,
        },
    );
    assert_eq!(layout(&app, a).leaves(), [a, b], "two panes at most");
    assert_eq!(strip(&app), [a, c]);
    assert_eq!(app.notice(), Some(&Notice::SplitMaxPanesReached(MAX_PANES)));
    assert!(app.merge_candidates(a).is_empty(), "split: offers none");
    assert_eq!(
        app.merge_candidates(c)
            .iter()
            .map(|tab| tab.id)
            .collect::<Vec<_>>(),
        Vec::<TabId>::new(),
        "the only other tab is split"
    );
}

#[test]
fn unsplit_swap_toggle_and_the_ratio() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path());
    let (c, _) = open(&mut app, "c");
    let ((a, _), (b, _)) = merged(&mut app);
    assert_eq!(strip(&app), [c, a]);

    split(&mut app, SplitMessage::Swap(a));
    assert_eq!(layout(&app, a).leaves(), [b, a]);
    assert_eq!(layout(&app, a).secondary(), Some(a));
    split(&mut app, SplitMessage::ToggleAxis(a));
    assert_eq!(layout(&app, a).axis(), Some(Axis::Stacked));
    split(&mut app, SplitMessage::ToggleAxis(a));
    assert_eq!(layout(&app, a).axis(), Some(Axis::SideBySide));

    split(
        &mut app,
        SplitMessage::Resize {
            host: a,
            ratio: 0.3,
        },
    );
    assert_eq!(layout(&app, a).ratio(), Some(0.3));
    split(
        &mut app,
        SplitMessage::Resize {
            host: a,
            ratio: 0.0,
        },
    );
    assert_eq!(layout(&app, a).ratio(), Some(MIN_RATIO), "the C# clamp");
    split(
        &mut app,
        SplitMessage::Resize {
            host: a,
            ratio: 7.0,
        },
    );
    assert_eq!(layout(&app, a).ratio(), Some(MAX_RATIO));
    split(
        &mut app,
        SplitMessage::Resize {
            host: a,
            ratio: f32::NAN,
        },
    );
    assert_eq!(
        layout(&app, a).ratio(),
        Some(MAX_RATIO),
        "no share is not one"
    );
    split(&mut app, SplitMessage::ResetRatio(a));
    assert_eq!(layout(&app, a).ratio(), Some(DEFAULT_RATIO));

    // The docked tab back on the strip right after its host; the host keeps the keyboard.
    app.update(Message::SelectTab(c));
    app.update(Message::TabMenu(TabMenuMessage::Pin(c)));
    app.update(Message::SelectTab(a));
    split(&mut app, SplitMessage::Focus(b));
    split(&mut app, SplitMessage::Unsplit(a));
    assert!(app.tab(a).expect("a").layout.is_none());
    assert_eq!(
        strip(&app),
        [c, a, b],
        "the pinned tab first, b right after a"
    );
    assert_eq!(app.active, Some(a));
}

#[test]
fn a_pane_given_the_keyboard_is_the_one_shown_when_its_tab_is_selected_again() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path());
    let ((a, _), (b, _)) = merged(&mut app);
    let (c, _) = open(&mut app, "c");
    split(&mut app, SplitMessage::Focus(a));
    assert_eq!(app.active, Some(a));
    app.update(Message::SelectTab(c));
    assert_eq!(app.shown_tab().map(|tab| tab.id), Some(c));
    app.update(Message::SelectTab(a));
    assert_eq!(app.active, Some(a));
    split(&mut app, SplitMessage::Focus(b));
    app.update(Message::SelectTab(c));
    app.update(Message::SelectTab(a));
    assert_eq!(app.active, Some(b), "the split's last focus");
    assert_eq!(layout(&app, a).focus, b);
}

#[test]
fn closing_the_tab_closes_every_pane_and_asks_for_the_live_ones() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path());
    let (c, _) = open(&mut app, "c");
    let ((a, a_attempt), (b, b_attempt)) = merged(&mut app);
    // Only the docked pane is live: its guard is the tab's.
    fail(&mut app, a, a_attempt);
    connect(&mut app, b, b_attempt);
    app.update(Message::RequestCloseTab(a));
    assert_eq!(app.dialog, Some(Dialog::ConfirmCloseTab(a)));
    app.update(Message::DismissDialog);
    assert_eq!(app.tabs.len(), 3, "dismissed: nothing closed");
    app.update(Message::RequestCloseTab(a));
    app.update(Message::ConfirmDialog);
    assert!(app.tab(a).is_none() && app.tab(b).is_none(), "both panes");
    assert_eq!(strip(&app), [c]);
    assert_eq!(app.active, Some(c), "the strip's tab, never a hidden pane");
}

#[test]
fn closing_a_pane_leaves_the_other_in_the_tabs_place() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path());
    let ((a, a_attempt), (b, b_attempt)) = merged(&mut app);
    let (c, _) = open(&mut app, "c");
    connect(&mut app, a, a_attempt);
    connect(&mut app, b, b_attempt);
    app.update(Message::TabMenu(TabMenuMessage::Pin(a)));
    assert_eq!(strip(&app), [a, c]);

    // The host's pane closed: the docked one takes its place on the strip, pinned.
    app.update(Message::SelectTab(a));
    split(&mut app, SplitMessage::Focus(a));
    split(&mut app, SplitMessage::ClosePane(a));
    assert_eq!(app.dialog, Some(Dialog::ConfirmCloseTab(a)), "live: asked");
    app.update(Message::ConfirmDialog);
    assert!(app.tab(a).is_none());
    assert_eq!(strip(&app), [b, c], "b in a's place");
    assert!(app.tab(b).expect("b").pinned, "with its pin");
    assert!(
        app.tab(b).expect("b").layout.is_none(),
        "a single pane: no split"
    );
    assert_eq!(app.active, Some(b));
    let live = app.tabs.iter().filter(|tab| tab.is_live()).count();
    assert_eq!(live, 1, "b's session went on");
}

#[test]
fn closing_the_secondary_pane_keeps_the_host() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path());
    let ((a, _), (b, b_attempt)) = merged(&mut app);
    connect(&mut app, b, b_attempt);
    assert_eq!(app.active, Some(b));
    split(&mut app, SplitMessage::CloseSecondary(a));
    assert_eq!(app.dialog, Some(Dialog::ConfirmCloseTab(b)));
    app.update(Message::ConfirmDialog);
    assert!(app.tab(b).is_none());
    assert!(app.tab(a).expect("a").layout.is_none());
    assert_eq!(strip(&app), [a]);
    assert_eq!(app.active, Some(a), "the pane left");

    // A pane not live closes without a question.
    let (c, _) = open(&mut app, "c");
    split(
        &mut app,
        SplitMessage::Merge {
            host: a,
            tab: c,
            axis: Axis::SideBySide,
            placement: Placement::Second,
        },
    );
    split(&mut app, SplitMessage::ClosePane(c));
    assert_eq!(app.dialog, None);
    assert!(app.tab(c).is_none());
    assert_eq!(app.active, Some(a));
}

#[test]
fn reconnecting_a_pane_keeps_the_split() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path());
    let ((a, a_attempt), (b, b_attempt)) = merged(&mut app);
    fail(&mut app, a, a_attempt);
    fail(&mut app, b, b_attempt);
    split(&mut app, SplitMessage::Swap(a));

    // The docked pane, as the C# `ReconnectPaneAsync`: still docked, in its place.
    app.update(Message::ReconnectTab(b));
    let new_b = layout(&app, a).leaves()[0];
    assert_ne!(new_b, b);
    assert!(app.tab(b).is_none());
    assert!(app.is_docked(new_b));
    assert_eq!(strip(&app), [a]);
    assert_eq!(app.active, Some(new_b));
    assert_eq!(layout(&app, a).focus, new_b);

    // The host: its split goes with it.
    app.update(Message::ReconnectTab(a));
    let new_a = strip(&app)[0];
    assert_ne!(new_a, a);
    assert!(app.tab(a).is_none());
    assert_eq!(layout(&app, new_a).leaves(), [new_b, new_a]);
    assert_eq!(app.host_of(new_b), Some(new_a));
    assert_eq!(app.tabs.len(), 2);
}

#[test]
fn reconnecting_keeps_the_pin() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path());
    let (a, attempt) = open(&mut app, "a");
    let (b, _) = open(&mut app, "b");
    fail(&mut app, a, attempt);
    app.update(Message::TabMenu(TabMenuMessage::Pin(a)));
    app.update(Message::ReconnectTab(a));
    let reopened = app.tabs[0].id;
    assert_ne!(reopened, a);
    assert!(app.tabs[0].pinned, "pinned before: pinned after");
    assert_eq!(strip(&app), [reopened, b]);
}

#[test]
fn a_docked_pane_is_neither_moved_nor_pinned_nor_closed_with_the_others() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path());
    let (c, _) = open(&mut app, "c");
    let ((a, _), (b, _)) = merged(&mut app);
    app.update(Message::MoveTab { tab: b, onto: c });
    app.update(Message::MoveTab { tab: c, onto: b });
    assert_eq!(strip(&app), [c, a], "nothing moved");
    app.update(Message::TabMenu(TabMenuMessage::Pin(b)));
    assert!(!app.tab(b).expect("b").pinned);
    assert_eq!(
        app.tab_group(c, TabGroup::Others),
        [a],
        "the split tab, once"
    );
    assert_eq!(app.tab_group(c, TabGroup::Right), [a]);
    assert!(
        app.tab_group(b, TabGroup::Others).is_empty(),
        "a pane is no tab"
    );
    app.update(Message::MoveTab { tab: a, onto: c });
    assert_eq!(strip(&app), [a, c], "the split tab moves, its pane with it");

    // "Close others" closes every pane of the split tab.
    app.update(Message::TabMenu(TabMenuMessage::Close {
        tab: c,
        group: TabGroup::Others,
    }));
    assert!(app.tab(a).is_none() && app.tab(b).is_none());
    assert_eq!(strip(&app), [c]);
}

#[test]
fn the_tunnels_panel_and_the_notice_follow_the_tab_shown_not_the_pane_focused() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path());
    let ((a, _), (b, _)) = merged(&mut app);
    let (c, _) = open(&mut app, "c");
    app.update(Message::SelectTab(a));
    let before = app.tunnels_panel();
    split(&mut app, SplitMessage::Focus(b));
    app.update(Message::Tunnel(TunnelMessage::TogglePanel));
    assert_eq!(app.tunnels_panel(), !before);
    split(&mut app, SplitMessage::Focus(a));
    assert_eq!(
        app.tunnels_panel(),
        !before,
        "the tab's choice, from either pane"
    );
    assert_eq!(
        app.tab(a).expect("a").layout.as_ref().map(|l| l.focus),
        Some(a)
    );

    split(
        &mut app,
        SplitMessage::Merge {
            host: a,
            tab: c,
            axis: Axis::Stacked,
            placement: Placement::Second,
        },
    );
    let said = Some(&Notice::SplitMaxPanesReached(MAX_PANES));
    assert_eq!(app.notice(), said);
    split(&mut app, SplitMessage::Focus(b));
    assert_eq!(app.notice(), said, "the same tab shown");
    app.update(Message::SelectTab(c));
    assert_eq!(app.notice(), None, "another tab shown");
}

#[test]
fn the_snapshot_keeps_the_strip_tabs_only() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path());
    let _ = merged(&mut app);
    open(&mut app, "c");
    let closing = app.update(Message::WindowCloseRequested);
    assert!(matches!(closing.as_slice(), [Effect::Exit]), "{closing:?}");
    let path = heimdall_core::session_snapshot::snapshot_path(&dir.path().join("profiles.toml"));
    let kept = heimdall_core::session_snapshot::load(&path).expect("kept");
    let profiles: Vec<&str> = kept
        .sessions
        .iter()
        .map(|entry| entry.profile.as_str())
        .collect();
    assert_eq!(profiles, ["a", "c"], "b is a pane of a's tab");
}

#[test]
fn a_pane_size_is_not_the_size_new_tabs_open_at() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path());
    let ((_, _), (b, _)) = merged(&mut app);
    let cell = CellPixels {
        width: 8,
        height: 16,
    };
    let half = GridSize { cols: 40, rows: 20 };
    app.update(Message::Resize {
        tab: b,
        grid: half,
        cell,
    });
    assert_eq!(
        app.tab(b).expect("b").terminal.size(),
        half,
        "the pane's own size"
    );
    let (c, _) = open(&mut app, "c");
    assert_eq!(
        app.tab(c).expect("c").terminal.size(),
        GRID,
        "not the pane's"
    );
    let whole = GridSize {
        cols: 120,
        rows: 40,
    };
    app.update(Message::Resize {
        tab: c,
        grid: whole,
        cell,
    });
    let (d, _) = open(&mut app, "a");
    assert_eq!(
        app.tab(d).expect("d").terminal.size(),
        whole,
        "a whole tab's"
    );
    // Every pane counts as a session.
    assert_eq!(app.tabs.len(), 4);
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

/// A Files tab of profile `b` with `/srv/a.txt` open in its integrated editor, changed.
async fn unsaved_files_tab(app: &mut App) -> TabId {
    let (tab, attempt) = open_as(app, Message::OpenFiles(ProfileId::new("b")));
    app.update(Message::Connection {
        tab,
        attempt,
        event: ConnectionEvent::FilesReady {
            client: idle_client().await,
            shell: None,
        },
    });
    let files = |app: &mut App, message| app.update(Message::Files(message));
    files(
        app,
        FilesMessage::RemoteListed {
            tab,
            result: Ok((
                RemotePath::from("/srv"),
                vec![RemoteEntry {
                    name: b"a.txt".to_vec(),
                    label: "a.txt".to_owned(),
                    kind: EntryKind::File,
                    size: Some(4),
                    modified: None,
                    permissions: None,
                    owner: None,
                    group: None,
                    inode: None,
                }],
            )),
        },
    );
    files(
        app,
        FilesMessage::Select {
            tab,
            side: Side::Remote,
            index: 0,
        },
    );
    let id = match files(app, FilesMessage::EditIntegrated { tab }).as_slice() {
        [Effect::OpenEditor { id, .. }] => *id,
        other => panic!("{other:?}"),
    };
    files(
        app,
        FilesMessage::EditorOpened {
            tab,
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
        },
    );
    files(
        app,
        FilesMessage::EditorChanged {
            tab,
            id,
            dirty: true,
        },
    );
    assert!(app.tab(tab).expect("files").holds_unsaved_text());
    tab
}

#[tokio::test]
async fn a_docked_pane_with_unsaved_text_asks_before_its_tab_closes() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path());
    let (a, _) = open(&mut app, "a");
    let files = unsaved_files_tab(&mut app).await;
    split(
        &mut app,
        SplitMessage::Merge {
            host: a,
            tab: files,
            axis: Axis::SideBySide,
            placement: Placement::Second,
        },
    );
    assert!(app.is_docked(files));
    split(&mut app, SplitMessage::Focus(a));
    app.update(Message::RequestCloseTab(a));
    assert!(
        matches!(
            &app.dialog,
            Some(Dialog::ConfirmCloseEditor { tab, name }) if *tab == a && name == "a.txt"
        ),
        "the docked pane's guard, for the whole tab: {:?}",
        app.dialog
    );
    app.update(Message::ConfirmDialog);
    assert!(app.tab(a).is_none() && app.tab(files).is_none());
    assert!(app.tabs.is_empty());
    assert_eq!(app.active, None);
}

/// The name of the profile tab `id` opened.
fn name(app: &App, id: TabId) -> String {
    app.tab(id).expect("tab").profile.name().to_owned()
}

#[test]
fn a_tab_merged_first_takes_the_left_or_top_side() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path());
    let (a, _) = open(&mut app, "a");
    let (b, _) = open(&mut app, "b");
    app.update(Message::SelectTab(a));
    split(
        &mut app,
        SplitMessage::Merge {
            host: a,
            tab: b,
            axis: Axis::Stacked,
            placement: Placement::First,
        },
    );
    let first = layout(&app, a);
    assert_eq!(first.leaves(), [b, a], "dropped on the top part: above");
    assert_eq!(first.axis(), Some(Axis::Stacked));
    assert_eq!(first.secondary(), Some(a));
    assert_eq!(strip(&app), [a], "the host keeps its place on the strip");
    assert_eq!(app.active, Some(a), "the tab shown keeps the keyboard");

    split(&mut app, SplitMessage::Unsplit(a));
    split(
        &mut app,
        SplitMessage::Merge {
            host: a,
            tab: b,
            axis: Axis::SideBySide,
            placement: Placement::Second,
        },
    );
    assert_eq!(layout(&app, a).leaves(), [a, b], "second, as the C#");
}

#[test]
fn open_in_split_merges_the_new_tab_into_the_tab_shown_up_to_the_most_panes() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path());
    let (a, _) = open(&mut app, "a");
    let (c, _) = open(&mut app, "c");
    app.update(Message::SelectTab(a));
    let effects = split(
        &mut app,
        SplitMessage::OpenInSplit {
            profile: ProfileId::new("b"),
            axis: Axis::SideBySide,
        },
    );
    assert!(
        matches!(effects.as_slice(), [Effect::Connect { .. }, ..]),
        "connecting: {effects:?}"
    );
    let opened = layout(&app, a);
    assert_eq!(opened.axis(), Some(Axis::SideBySide));
    let [host, b] = opened.leaves()[..] else {
        panic!("two panes: {opened:?}");
    };
    assert_eq!(host, a, "the tab shown before, first");
    assert_eq!(name(&app, b), "server b");
    assert_eq!(strip(&app), [a, c], "the new tab never on the strip");
    assert_eq!(app.active, Some(b), "the new pane has the keyboard");

    // Two panes already: said, and nothing opens.
    let before = app.tabs.len();
    let effects = split(
        &mut app,
        SplitMessage::OpenInSplit {
            profile: ProfileId::new("c"),
            axis: Axis::Stacked,
        },
    );
    assert!(effects.is_empty());
    assert_eq!(app.tabs.len(), before, "nothing opened");
    assert_eq!(layout(&app, a).leaves(), [a, b]);
    assert_eq!(app.notice(), Some(&Notice::SplitMaxPanesReached(MAX_PANES)));
}

#[test]
fn open_in_split_merges_nothing_when_the_session_limit_refuses_the_open() {
    use heimdall_app::SettingsMessage;

    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path());
    // No tab shown: nothing to split, nothing opened.
    let effects = split(
        &mut app,
        SplitMessage::OpenInSplit {
            profile: ProfileId::new("b"),
            axis: Axis::SideBySide,
        },
    );
    assert!(effects.is_empty() && app.tabs.is_empty());

    app.update(Message::Settings(SettingsMessage::MaxSessions(1)));
    let (a, _) = open(&mut app, "a");
    let effects = split(
        &mut app,
        SplitMessage::OpenInSplit {
            profile: ProfileId::new("b"),
            axis: Axis::SideBySide,
        },
    );
    assert!(effects.is_empty(), "{effects:?}");
    assert_eq!(app.tabs.len(), 1);
    assert!(app.tab(a).expect("a").layout.is_none(), "nothing merged");
    assert_eq!(app.active, Some(a));
    assert_eq!(app.notice(), Some(&Notice::SessionLimitReached(1)));
}

#[test]
fn quick_connect_in_split_mode_merges_what_is_chosen_into_its_tab() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path());
    let (a, _) = open(&mut app, "a");
    let (c, _) = open(&mut app, "c");
    assert_eq!(app.active, Some(c), "another tab shown");
    let result = app
        .quick_results("server b")
        .into_iter()
        .next()
        .expect("found");
    split(
        &mut app,
        SplitMessage::QuickConnect {
            host: a,
            axis: Axis::Stacked,
            result,
        },
    );
    let opened = layout(&app, a);
    assert_eq!(opened.axis(), Some(Axis::Stacked));
    let [host, b] = opened.leaves()[..] else {
        panic!("two panes: {opened:?}");
    };
    assert_eq!(host, a);
    assert_eq!(name(&app, b), "server b");
    assert_eq!(
        app.shown_tab().map(|tab| tab.id),
        Some(a),
        "the split tab shown"
    );
    assert_eq!(app.active, Some(b));

    // A host typed: a session saved nowhere, merged as well.
    let typed = app
        .quick_results("root@db.lab")
        .into_iter()
        .next()
        .expect("an SSH destination");
    split(
        &mut app,
        SplitMessage::QuickConnect {
            host: c,
            axis: Axis::SideBySide,
            result: typed,
        },
    );
    let leaves = layout(&app, c).leaves();
    assert_eq!(leaves.len(), 2);
    assert_eq!(name(&app, leaves[1]), "db.lab");
    assert_eq!(strip(&app), [a, c]);
}

#[test]
fn the_pane_shortcuts_go_round_the_split_and_leave_a_plain_tab_alone() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path());
    let ((a, _), (b, _)) = merged(&mut app);
    assert_eq!(app.active, Some(b), "the last pane");
    split(&mut app, SplitMessage::FocusNext(a));
    assert_eq!(app.active, Some(a), "round to the first");
    assert_eq!(layout(&app, a).focus, a);
    split(&mut app, SplitMessage::FocusNext(a));
    assert_eq!(app.active, Some(b));
    split(&mut app, SplitMessage::FocusPrevious(a));
    assert_eq!(app.active, Some(a));
    split(&mut app, SplitMessage::FocusPrevious(a));
    assert_eq!(app.active, Some(b), "round to the last");

    let (c, _) = open(&mut app, "c");
    split(&mut app, SplitMessage::FocusNext(c));
    split(&mut app, SplitMessage::FocusPrevious(c));
    assert_eq!(app.active, Some(c), "not split: nothing moves");
}

/// The splits remembered beside the profiles of `dir`, as the next start reads them.
fn remembered(dir: &Path) -> heimdall_core::split_layouts::SplitLayouts {
    heimdall_core::split_layouts::SplitLayouts::load(
        &heimdall_core::split_layouts::split_layouts_path(&dir.join("profiles.toml")),
    )
    .expect("readable")
}

/// Tab `tab` merged into `host`, placed as said.
fn merge(app: &mut App, host: TabId, tab: TabId, axis: Axis, placement: Placement) {
    split(
        app,
        SplitMessage::Merge {
            host,
            tab,
            axis,
            placement,
        },
    );
}

#[test]
fn a_resized_split_of_two_profiles_starts_there_when_merged_again_mirrored_the_other_way() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path());
    let ((a, _), (b, _)) = merged(&mut app);
    assert_eq!(
        layout(&app, a).ratio(),
        Some(DEFAULT_RATIO),
        "nothing known"
    );
    split(
        &mut app,
        SplitMessage::Resize {
            host: a,
            ratio: 0.25,
        },
    );
    let kept = remembered(dir.path());
    let entry = &kept.entries()[0];
    assert_eq!((entry.first.as_str(), entry.second.as_str()), ("a", "b"));
    assert_eq!(
        entry.orientation,
        heimdall_core::split_layouts::Orientation::SideBySide
    );

    // The same pair again starts at that share; the placement is the one asked, as the C#
    // restores the ratio only.
    split(&mut app, SplitMessage::Unsplit(a));
    merge(&mut app, a, b, Axis::Stacked, Placement::Second);
    assert_eq!(layout(&app, a).ratio(), Some(0.25));
    assert_eq!(layout(&app, a).axis(), Some(Axis::Stacked));

    // The other way round, the mirrored share.
    split(&mut app, SplitMessage::Unsplit(a));
    merge(&mut app, b, a, Axis::SideBySide, Placement::Second);
    assert_eq!(layout(&app, b).leaves(), [b, a]);
    assert_eq!(layout(&app, b).ratio(), Some(0.75), "1 - ratio");
    split(&mut app, SplitMessage::Unsplit(b));
    merge(&mut app, a, b, Axis::SideBySide, Placement::First);
    assert_eq!(layout(&app, a).leaves(), [b, a], "dropped first");
    assert_eq!(layout(&app, a).ratio(), Some(0.75));

    // A reset is remembered too; swapping and turning are not, as the C#.
    split(&mut app, SplitMessage::ResetRatio(a));
    split(&mut app, SplitMessage::Swap(a));
    split(&mut app, SplitMessage::ToggleAxis(a));
    let kept = remembered(dir.path());
    assert_eq!(kept.entries().len(), 1, "one pair, in either order");
    assert_eq!(
        kept.ratio(&ProfileId::new("a"), &ProfileId::new("b")),
        Some(DEFAULT_RATIO)
    );
    assert_eq!(
        kept.entries()[0].orientation,
        heimdall_core::split_layouts::Orientation::SideBySide,
        "as merged, not as turned"
    );
}

#[test]
fn a_pane_saved_nowhere_is_not_remembered() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path());
    let (a, _) = open(&mut app, "a");
    let typed = app
        .quick_results("root@db.lab")
        .into_iter()
        .next()
        .expect("an SSH destination");
    split(
        &mut app,
        SplitMessage::QuickConnect {
            host: a,
            axis: Axis::SideBySide,
            result: typed,
        },
    );
    let [_, db] = layout(&app, a).leaves()[..] else {
        panic!("two panes");
    };
    split(
        &mut app,
        SplitMessage::Resize {
            host: a,
            ratio: 0.25,
        },
    );
    assert!(
        remembered(dir.path()).entries().is_empty(),
        "no profile, no key"
    );
    split(&mut app, SplitMessage::Unsplit(a));
    merge(&mut app, a, db, Axis::SideBySide, Placement::Second);
    assert_eq!(layout(&app, a).ratio(), Some(DEFAULT_RATIO));
}

#[test]
fn the_fifty_first_pair_drops_the_oldest() {
    use heimdall_core::split_layouts::MAX_ENTRIES;

    let dir = tempfile::tempdir().expect("dir");
    let others: Vec<String> = (0..=MAX_ENTRIES).map(|n| format!("p{n}")).collect();
    let mut app = app_with(dir.path(), &others);
    let (a, _) = open(&mut app, "a");
    for other in &others {
        let (tab, _) = open(&mut app, other);
        merge(&mut app, a, tab, Axis::SideBySide, Placement::Second);
        split(&mut app, SplitMessage::Unsplit(a));
        app.update(Message::RequestCloseTab(tab));
    }
    let kept = remembered(dir.path());
    assert_eq!(kept.entries().len(), MAX_ENTRIES);
    assert_eq!(
        kept.entries()[0].second.as_str(),
        others[MAX_ENTRIES],
        "newest first"
    );
    let a_id = ProfileId::new("a");
    assert_eq!(
        kept.ratio(&a_id, &ProfileId::new("p0")),
        None,
        "the oldest dropped"
    );
    assert_eq!(
        kept.ratio(&a_id, &ProfileId::new("p1")),
        Some(DEFAULT_RATIO)
    );
}

#[test]
fn the_splits_remembered_are_read_back_at_the_next_start() {
    let dir = tempfile::tempdir().expect("dir");
    let mut first_run = app(dir.path());
    let ((a, _), _) = merged(&mut first_run);
    split(
        &mut first_run,
        SplitMessage::Resize {
            host: a,
            ratio: 0.25,
        },
    );
    drop(first_run);

    let mut app = app(dir.path());
    let (b, _) = open(&mut app, "b");
    let (a, _) = open(&mut app, "a");
    merge(&mut app, b, a, Axis::Stacked, Placement::Second);
    assert_eq!(
        layout(&app, b).ratio(),
        Some(0.75),
        "read at start, mirrored"
    );
}

#[test]
fn split_mode_quick_connect_offers_the_profiles_last_split_with_first() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path());
    let (a, _) = open(&mut app, "a");
    let (c, _) = open(&mut app, "c");
    merge(&mut app, a, c, Axis::SideBySide, Placement::Second);
    split(&mut app, SplitMessage::Unsplit(a));
    let names = |results: Vec<heimdall_app::QuickResult>| -> Vec<String> {
        results
            .into_iter()
            .map(|result| match result {
                heimdall_app::QuickResult::Profile(profile) => profile.id.as_str().to_owned(),
                other => panic!("a profile: {other:?}"),
            })
            .collect()
    };
    assert_eq!(names(app.quick_results_in("", Some(a))), ["c", "a", "b"]);
    assert_eq!(
        names(app.quick_results_in("", None)),
        names(app.quick_results("")),
        "not in split mode: as before"
    );
    assert_eq!(
        names(app.quick_results_in("server b", Some(a))),
        names(app.quick_results("server b")),
        "something typed: as scored"
    );
    let (b, _) = open(&mut app, "b");
    assert_eq!(
        names(app.quick_results_in("", Some(b))),
        names(app.quick_results("")),
        "b was split with nothing"
    );
}
