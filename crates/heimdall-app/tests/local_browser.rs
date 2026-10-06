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

//! The file browser a local shell gets beside it, as the C# `LocalFileBrowserView`: docked
//! side by side as the second pane once the shell started, the keyboard left on the shell;
//! in the shell's folder, else the home folder; not when the tab is split already; never
//! twice after a reconnect; closed alone, the shell left; and nothing it does ever typed
//! into the shell.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use heimdall_app::files::{Direction, EntryKind, FilesKey, LocalEntry, Side};
use heimdall_app::local_driver::LocalShell;
use heimdall_app::split::{Axis, DEFAULT_RATIO, Placement, SplitMessage};
use heimdall_app::{
    App, AppConfig, AttemptId, ConnectionEvent, Effect, FilesMessage, InputSink, Message, Phase,
    Purpose, TabId,
};
use heimdall_ssh::{AgentSource, SessionClosed, TerminalSize};
use heimdall_term::GridSize;
use heimdall_term::local::LocalArguments;

/// Records what the application sends to a session.
#[derive(Debug, Default)]
struct RecordingSink {
    written: Mutex<Vec<u8>>,
}

impl RecordingSink {
    /// What was written since the last call.
    fn take(&self) -> Vec<u8> {
        std::mem::take(&mut *self.written.lock().expect("written"))
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

fn app(dir: &Path) -> App {
    App::new(AppConfig {
        profiles_file: dir.join("profiles.toml"),
        known_hosts: dir.join("known_hosts"),
        legacy_dir: None,
        agent: AgentSource::Disabled,
        initial_grid: GridSize { cols: 80, rows: 24 },
        files_start: dir.to_owned(),
        system_credentials: heimdall_app::SystemCredentials::memory(),
    })
}

/// A local shell, started in `folder` when one is given.
fn local(folder: Option<PathBuf>) -> LocalShell {
    LocalShell {
        name: "Shell".to_owned(),
        program: None,
        arguments: LocalArguments::List(Vec::new()),
        working_directory: folder,
        environment: Vec::new(),
    }
}

/// A local shell tab, still starting.
fn open(app: &mut App, folder: Option<PathBuf>) -> (TabId, AttemptId) {
    match app.update(Message::OpenLocal(local(folder))).as_slice() {
        [Effect::ConnectLocal { tab, attempt, .. }] => (*tab, *attempt),
        other => panic!("expected one ConnectLocal, got {other:?}"),
    }
}

/// The shell started, writing into `sink`, and what that set off.
fn start(
    app: &mut App,
    (tab, attempt): (TabId, AttemptId),
    sink: &Arc<RecordingSink>,
) -> Vec<Effect> {
    let effects = app.update(Message::Connection {
        tab,
        attempt,
        event: ConnectionEvent::Connected {
            input: Arc::clone(sink) as Arc<dyn InputSink>,
        },
    });
    assert_eq!(app.tab(tab).expect("tab").phase, Phase::Connected);
    effects
}

/// The browser the effects of a shell's start docked, and the folder it lists first.
fn docked(effects: &[Effect]) -> (TabId, PathBuf) {
    match effects {
        [Effect::ListLocal { tab, path }] => (*tab, path.clone()),
        other => panic!("expected the browser's listing, got {other:?}"),
    }
}

fn leaves(app: &App, host: TabId) -> Vec<TabId> {
    app.tab(host)
        .and_then(|tab| tab.layout.as_ref())
        .map(heimdall_app::split::Layout::leaves)
        .unwrap_or_default()
}

/// `folder` listed in browser `pane`, holding folder `sub` and file `run.sh`.
fn listed(app: &mut App, pane: TabId, folder: &Path) -> Vec<Effect> {
    let entry = |name: &str, kind| LocalEntry {
        name: name.into(),
        label: name.to_owned(),
        kind,
        size: None,
        modified: None,
    };
    app.update(Message::Files(FilesMessage::LocalListed {
        tab: pane,
        result: Ok((
            folder.to_owned(),
            vec![
                entry("sub", EntryKind::Directory),
                entry("run.sh", EntryKind::File),
            ],
        )),
    }))
}

#[test]
fn a_local_shell_started_docks_its_browser_beside_it_the_keyboard_on_the_shell() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path());
    let sink = Arc::new(RecordingSink::default());
    let shell = open(&mut app, None);
    let (pane, folder) = docked(&start(&mut app, shell, &sink));
    assert_eq!(folder, dir.path(), "the shell's folder: the home folder");
    let layout = app
        .tab(shell.0)
        .and_then(|tab| tab.layout.clone())
        .expect("split");
    assert_eq!(layout.leaves(), [shell.0, pane], "second, as the C# pane");
    assert_eq!(layout.axis(), Some(Axis::SideBySide), "the C# Vertical");
    assert_eq!(layout.ratio(), Some(DEFAULT_RATIO));
    assert_eq!(layout.focus, shell.0);
    assert_eq!(app.active, Some(shell.0), "the keyboard stays on the shell");
    let strip: Vec<TabId> = app.strip().iter().map(|tab| tab.id).collect();
    assert_eq!(strip, [shell.0]);
    let browser = app.tab(pane).expect("browser");
    assert!(browser.is_local_browser());
    assert_eq!(browser.purpose, Purpose::Files);
    assert_eq!(browser.phase, Phase::Connected, "nothing to connect");
    assert!(!browser.is_live(), "no session");
    assert!(!app.can_restart(browser), "nothing opens it again");
    let files = browser.files.as_deref().expect("files");
    assert!(files.local_only, "this computer's files alone");
    assert!(!files.local_hidden);
    assert_eq!(files.focus, Side::Local);
    assert!(files.client.is_none());
}

#[test]
fn a_shell_starting_behind_another_tab_leaves_that_tab_shown() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path());
    let sink = Arc::new(RecordingSink::default());
    let shell = open(&mut app, None);
    let (other, _) = open(&mut app, None);
    let (pane, _) = docked(&start(&mut app, shell, &sink));
    assert_eq!(app.active, Some(other), "the tab shown stays shown");
    assert_eq!(leaves(&app, shell.0), [shell.0, pane]);
}

#[test]
fn the_browser_starts_in_the_shells_folder_and_home_goes_back_there() {
    let dir = tempfile::tempdir().expect("dir");
    let work = dir.path().join("work");
    std::fs::create_dir(&work).expect("work");
    let mut app = app(dir.path());
    let sink = Arc::new(RecordingSink::default());
    let shell = open(&mut app, Some(work.clone()));
    let (pane, folder) = docked(&start(&mut app, shell, &sink));
    assert_eq!(folder, work);
    listed(&mut app, pane, &work);
    let effects = app.update(Message::Files(FilesMessage::Open {
        tab: pane,
        side: Side::Local,
        index: 0,
    }));
    assert!(
        matches!(effects.as_slice(), [Effect::ListLocal { path, .. }] if *path == work.join("sub")),
        "{effects:?}"
    );
    listed(&mut app, pane, &work.join("sub"));
    let effects = app.update(Message::Files(FilesMessage::Home {
        tab: pane,
        side: Side::Local,
    }));
    assert!(
        matches!(effects.as_slice(), [Effect::ListLocal { path, .. }] if *path == work),
        "Home is where it started, as the C# session root: {effects:?}"
    );
}

#[test]
fn a_shell_folder_not_there_leaves_the_browser_in_the_home_folder_as_the_csharp() {
    let dir = tempfile::tempdir().expect("dir");
    let gone = dir.path().join("gone");
    let mut app = app(dir.path());
    let sink = Arc::new(RecordingSink::default());
    let shell = open(&mut app, Some(gone.clone()));
    let (pane, folder) = docked(&start(&mut app, shell, &sink));
    assert_eq!(folder, gone);
    let failed = |app: &mut App| {
        app.update(Message::Files(FilesMessage::LocalListed {
            tab: pane,
            result: Err(heimdall_app::files::FilesError::Local {
                detail: "not found".to_owned(),
            }),
        }))
    };
    let effects = failed(&mut app);
    let [Effect::ListLocal { tab, path }] = effects.as_slice() else {
        panic!("{effects:?}");
    };
    assert_eq!(
        (*tab, path.as_path()),
        (pane, dir.path()),
        "the home folder"
    );
    // The home folder failing too is shown, as in any Files tab: no loop.
    assert!(failed(&mut app).is_empty());
    let files = app
        .tab(pane)
        .and_then(|tab| tab.files.as_deref())
        .expect("files");
    assert!(files.local.error.is_some());
}

#[test]
fn a_tab_split_already_or_docked_docks_no_browser() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path());
    let sink = Arc::new(RecordingSink::default());
    let host = open(&mut app, None);
    let docked_shell = open(&mut app, None);
    app.update(Message::Split(SplitMessage::Merge {
        host: host.0,
        tab: docked_shell.0,
        axis: Axis::Stacked,
        placement: Placement::Second,
    }));
    assert!(start(&mut app, host, &sink).is_empty(), "split already");
    assert!(
        start(&mut app, docked_shell, &sink).is_empty(),
        "docked in another tab"
    );
    assert_eq!(leaves(&app, host.0), [host.0, docked_shell.0]);
    assert_eq!(app.tabs.len(), 2);
}

#[test]
fn closing_the_browser_leaves_the_shell_at_once() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path());
    let sink = Arc::new(RecordingSink::default());
    let shell = open(&mut app, None);
    let (pane, _) = docked(&start(&mut app, shell, &sink));
    app.update(Message::Split(SplitMessage::ClosePane(pane)));
    assert!(app.dialog.is_none(), "no session: nothing to confirm");
    assert!(app.tab(pane).is_none());
    let host = app.tab(shell.0).expect("the shell stays");
    assert!(host.layout.is_none());
    assert_eq!(host.phase, Phase::Connected);
    assert_eq!(app.active, Some(shell.0));
}

#[test]
fn a_reconnect_keeps_the_browser_and_docks_no_second_one() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path());
    let sink = Arc::new(RecordingSink::default());
    let shell = open(&mut app, None);
    let (pane, _) = docked(&start(&mut app, shell, &sink));
    let again = match app.update(Message::ReconnectTab(shell.0)).as_slice() {
        [Effect::ConnectLocal { tab, attempt, .. }] => (*tab, *attempt),
        other => panic!("{other:?}"),
    };
    assert_eq!(leaves(&app, again.0), [again.0, pane], "the split carried");
    assert!(
        start(&mut app, again, &sink).is_empty(),
        "no second browser"
    );
    assert_eq!(leaves(&app, again.0), [again.0, pane]);
    assert_eq!(app.tabs.len(), 2);

    // The browser's own Reconnect opens nothing: it is no session.
    assert!(app.update(Message::ReconnectTab(pane)).is_empty());
    assert_eq!(app.tabs.len(), 2);

    // Closed by the user, it comes back with the next start, as the C# docks it with
    // every local shell.
    app.update(Message::Split(SplitMessage::ClosePane(pane)));
    let third = match app.update(Message::ReconnectTab(again.0)).as_slice() {
        [Effect::ConnectLocal { tab, attempt, .. }] => (*tab, *attempt),
        other => panic!("{other:?}"),
    };
    let (back, _) = docked(&start(&mut app, third, &sink));
    assert_eq!(leaves(&app, third.0), [third.0, back]);
}

#[test]
fn nothing_the_browser_does_is_typed_into_the_shell() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path());
    let sink = Arc::new(RecordingSink::default());
    let shell = open(&mut app, None);
    let (pane, folder) = docked(&start(&mut app, shell, &sink));
    listed(&mut app, pane, &folder);
    let files = |message| Message::Files(message);
    let key = |key| Message::Files(FilesMessage::Key { tab: pane, key });
    let side = Side::Local;
    // Into a folder and back, as the C# "Open" and "Open in terminal" go: the browser
    // moves, the shell stays where it is.
    for message in [
        files(FilesMessage::Select {
            tab: pane,
            side,
            index: 0,
        }),
        files(FilesMessage::Open {
            tab: pane,
            side,
            index: 0,
        }),
        files(FilesMessage::Up { tab: pane, side }),
        files(FilesMessage::Back { tab: pane, side }),
        files(FilesMessage::Home { tab: pane, side }),
        files(FilesMessage::Refresh { tab: pane, side }),
        files(FilesMessage::CopyPath { tab: pane, side }),
        // A script, as the C# "Run in shell" offers it: opened, sent, nothing runs.
        files(FilesMessage::Select {
            tab: pane,
            side,
            index: 1,
        }),
        files(FilesMessage::Open {
            tab: pane,
            side,
            index: 1,
        }),
        files(FilesMessage::Transfer {
            tab: pane,
            direction: Direction::Upload,
        }),
        key(FilesKey::Open),
        key(FilesKey::Upload),
        key(FilesKey::Parent),
        key(FilesKey::SwitchPane),
        key(FilesKey::Focus(Side::Remote)),
        files(FilesMessage::OpenInExplorer { tab: pane }),
        files(FilesMessage::ToggleLocal { tab: pane }),
    ] {
        let label = format!("{message:?}");
        let effects = app.update(message);
        assert!(
            effects.iter().all(|effect| matches!(
                effect,
                Effect::ListLocal { .. } | Effect::WriteClipboard(_) | Effect::OpenFolder { .. }
            )),
            "{label}: {effects:?}"
        );
        assert!(sink.take().is_empty(), "{label}: typed into the shell");
    }
    let files = app
        .tab(pane)
        .and_then(|tab| tab.files.as_deref())
        .expect("files");
    assert_eq!(files.focus, Side::Local, "its one pane keeps the keys");
    assert!(files.local_only && !files.local_hidden, "never hidden");
    assert!(files.transfers.is_empty(), "nothing sent anywhere");
}

#[test]
fn open_in_explorer_opens_the_folder_selected_else_the_one_shown() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path());
    let sink = Arc::new(RecordingSink::default());
    let shell = open(&mut app, None);
    let (pane, folder) = docked(&start(&mut app, shell, &sink));
    listed(&mut app, pane, &folder);
    let explorer = |app: &mut App, index: Option<usize>| {
        if let Some(index) = index {
            app.update(Message::Files(FilesMessage::Select {
                tab: pane,
                side: Side::Local,
                index,
            }));
        }
        match app
            .update(Message::Files(FilesMessage::OpenInExplorer { tab: pane }))
            .as_slice()
        {
            [Effect::OpenFolder { tab, folder }] if *tab == pane => folder.clone(),
            other => panic!("{other:?}"),
        }
    };
    assert_eq!(explorer(&mut app, None), folder, "nothing selected");
    assert_eq!(explorer(&mut app, Some(0)), folder.join("sub"));
    assert_eq!(explorer(&mut app, Some(1)), folder, "a file: its folder");
}
