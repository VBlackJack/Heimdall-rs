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
//! in the shell's folder, else the home folder; not when the tab is split already, nor when
//! the settings leave it out; never twice after a reconnect, nor again once the user closed
//! it; closed alone, the shell left; and nothing it does ever typed into the shell. Its
//! "Open in terminal" opens a new default shell in the folder; its files open in the
//! editor, with the system's default program, or once agreed when they would run. Its
//! "Run in Shell" runs a script this platform runs by its interpreter in a new tab, each
//! time once agreed to the command shown whole, Reconnect included.

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use heimdall_app::files::{Direction, EntryKind, FilesError, FilesKey, LocalEntry, Side};
use heimdall_app::local_driver::LocalShell;
use heimdall_app::local_open::{self, LocalOpening};
use heimdall_app::script_shell::{self, ScriptKind, ScriptRefusal};
use heimdall_app::split::{Axis, DEFAULT_RATIO, Placement, SplitMessage};
use heimdall_app::{
    App, AppConfig, AttemptId, ConnectionEvent, Dialog, Effect, FilesMessage, InputSink, Message,
    Phase, Purpose, ScriptConfirmation, SettingsMessage, TabId,
};
use heimdall_core::profile::{
    LocalApproval, LocalArguments as ProfileArguments, LocalCommand, LocalProfile, ProfileId,
};
use heimdall_core::session_snapshot;
use heimdall_core::settings::SftpBrowser;
use heimdall_core::store::ProfileStore;
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

    // Closed by the user, it stays closed when the shell starts again, a reconnect after
    // another.
    app.update(Message::Split(SplitMessage::ClosePane(pane)));
    let mut shell = again;
    for _ in 0..2 {
        shell = match app.update(Message::ReconnectTab(shell.0)).as_slice() {
            [Effect::ConnectLocal { tab, attempt, .. }] => (*tab, *attempt),
            other => panic!("{other:?}"),
        };
        assert!(
            start(&mut app, shell, &sink).is_empty(),
            "closed by the user: not docked again"
        );
        assert!(leaves(&app, shell.0).is_empty());
    }
    assert_eq!(app.tabs.len(), 1);

    // A shell opened anew docks its own, as usual.
    let new = open(&mut app, None);
    let (browser, _) = docked(&start(&mut app, new, &sink));
    assert_eq!(leaves(&app, new.0), [new.0, browser]);
}

#[test]
fn a_browser_taken_out_of_the_split_is_not_docked_again_either() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path());
    let sink = Arc::new(RecordingSink::default());
    let shell = open(&mut app, None);
    let (pane, _) = docked(&start(&mut app, shell, &sink));
    app.update(Message::Split(SplitMessage::Unsplit(shell.0)));
    assert!(app.tab(pane).is_some(), "a tab of its own now");
    let again = match app.update(Message::ReconnectTab(shell.0)).as_slice() {
        [Effect::ConnectLocal { tab, attempt, .. }] => (*tab, *attempt),
        other => panic!("{other:?}"),
    };
    assert!(
        start(&mut app, again, &sink).is_empty(),
        "no second browser"
    );
}

#[test]
fn the_setting_off_docks_no_browser_beside_a_local_shell() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path());
    let sink = Arc::new(RecordingSink::default());
    let off = SftpBrowser {
        dock_local_browser: false,
        ..SftpBrowser::default()
    };
    app.update(Message::Settings(SettingsMessage::SftpBrowser(off)));
    assert_eq!(app.settings().sftp_browser, off);
    let shell = open(&mut app, None);
    assert!(start(&mut app, shell, &sink).is_empty());
    assert!(leaves(&app, shell.0).is_empty());
    assert_eq!(app.tabs.len(), 1);
    // On again: the next shell docks one.
    app.update(Message::Settings(SettingsMessage::SftpBrowser(
        SftpBrowser::default(),
    )));
    let next = open(&mut app, None);
    let (pane, _) = docked(&start(&mut app, next, &sink));
    assert_eq!(leaves(&app, next.0), [next.0, pane]);
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
                Effect::ListLocal { .. }
                    | Effect::WriteClipboard(_)
                    | Effect::OpenFolder { .. }
                    | Effect::LaunchEditor { .. }
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

/// A tab started, and the shell it runs.
type Started = ((TabId, AttemptId), LocalShell);

/// The shell a new tab was started with, and its tab; none when nothing started.
fn started(effects: &[Effect]) -> Option<Started> {
    match effects {
        [
            Effect::ConnectLocal {
                tab,
                attempt,
                request,
            },
        ] => Some(((*tab, *attempt), request.shell.clone())),
        _ => None,
    }
}

/// "Open in terminal" in browser `pane`, `index` selected first when given: the new tab and
/// the shell it runs.
fn open_in_terminal(app: &mut App, pane: TabId, index: Option<usize>) -> Started {
    if let Some(index) = index {
        app.update(Message::Files(FilesMessage::Select {
            tab: pane,
            side: Side::Local,
            index,
        }));
    }
    let effects = app.update(Message::Files(FilesMessage::OpenInTerminal { tab: pane }));
    started(&effects).unwrap_or_else(|| panic!("a new shell: {effects:?}"))
}

#[test]
fn open_in_terminal_opens_a_new_default_shell_in_the_folder_selected_else_the_one_shown() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path());
    let sink = Arc::new(RecordingSink::default());
    let shell = open(&mut app, None);
    let (pane, folder) = docked(&start(&mut app, shell, &sink));
    listed(&mut app, pane, &folder);
    let folder_name = folder
        .file_name()
        .expect("a named folder")
        .to_string_lossy()
        .into_owned();
    for (index, expected, name) in [
        (None, folder.clone(), folder_name.clone()),
        (Some(0), folder.join("sub"), "sub".to_owned()),
        (Some(1), folder.clone(), folder_name.clone()),
    ] {
        let tabs = app.tabs.len();
        let (attempt, started) = open_in_terminal(&mut app, pane, index);
        let tab = attempt.0;
        assert_eq!(app.tabs.len(), tabs + 1, "a new tab");
        assert!(tab != shell.0 && tab != pane);
        assert_eq!(started.program, None, "the user's default shell");
        assert_eq!(started.working_directory.as_ref(), Some(&expected));
        assert!(started.environment.is_empty(), "no HEIMDALL_* variables");
        assert_eq!(started.name, name, "named after the folder");
        let opened = app.tab(tab).expect("tab");
        assert_eq!(opened.title, name);
        assert_eq!(opened.purpose, Purpose::Shell);
        assert!(!app.can_save_as_profile(opened), "the sidebar's own shell");
        assert!(
            sink.take().is_empty(),
            "nothing typed into the shell beside"
        );
        // Once started, it docks a browser of its own, and opens again as itself, in the
        // same folder.
        let (own, _) = docked(&start(&mut app, attempt, &sink));
        assert_eq!(leaves(&app, tab), [tab, own]);
        let (_, again) = started_again(&mut app, tab);
        assert_eq!(again, started);
    }
    // The shell beside the browser is left as it was.
    assert_eq!(app.tab(shell.0).expect("shell").phase, Phase::Connected);
    assert_eq!(leaves(&app, shell.0), [shell.0, pane]);
}

/// Reconnect of `tab`: the shell it runs again.
fn started_again(app: &mut App, tab: TabId) -> Started {
    let effects = app.update(Message::ReconnectTab(tab));
    started(&effects).unwrap_or_else(|| panic!("opened again: {effects:?}"))
}

/// A program found on every machine the tests run on, by its full path.
#[cfg(unix)]
const PROGRAM: &str = "/bin/sh";
#[cfg(windows)]
const PROGRAM: &str = r"C:\Windows\System32\cmd.exe";

#[test]
fn a_profile_running_another_program_still_opens_the_default_shell_and_stays_as_saved() {
    let dir = tempfile::tempdir().expect("dir");
    let profiles = dir.path().join("profiles.toml");
    let command = LocalCommand {
        program: Some(PROGRAM.to_owned()),
        arguments: ProfileArguments::List(Vec::new()),
        working_directory: None,
    };
    let mut store = ProfileStore::open(&profiles).expect("store");
    store.merge_local([LocalProfile {
        id: ProfileId::new("tool"),
        name: "Tool".to_owned(),
        group: Some("Ops".to_owned()),
        command: command.clone(),
        approved: None,
        session_logging: None,
    }]);
    store.approve_local(
        &ProfileId::new("tool"),
        LocalApproval {
            command,
            program_path: PathBuf::from(PROGRAM),
        },
    );
    store.save().expect("save");
    let mut app = app(dir.path());
    let sink = Arc::new(RecordingSink::default());
    let effects = app.update(Message::OpenLocalProfile(ProfileId::new("tool")));
    let Some((host, running)) = started(&effects) else {
        panic!("approved: started at once, {effects:?}");
    };
    assert_eq!(running.program.as_deref(), Some(PROGRAM));
    assert!(
        !running.environment.is_empty(),
        "the profile's HEIMDALL_* variables"
    );
    let (pane, folder) = docked(&start(&mut app, host, &sink));
    listed(&mut app, pane, &folder);
    let saved = std::fs::read(&profiles).expect("saved");
    let (_, opened) = open_in_terminal(&mut app, pane, None);
    assert_eq!(opened.program, None, "never the profile's program");
    assert!(opened.environment.is_empty(), "nor its variables");
    assert!(app.dialog.is_none(), "nothing asked: no profile runs");
    assert_eq!(
        std::fs::read(&profiles).expect("still saved"),
        saved,
        "no profile changed"
    );
    assert!(sink.take().is_empty());
}

#[test]
fn a_file_opens_by_its_kind_text_first() {
    // Text first, as the C# list: a script is shown, not run.
    for text in [
        "notes.MD",
        "run.sh",
        "deploy.ps1",
        "build.CMD",
        "app.js",
        ".gitignore",
        ".env",
    ] {
        assert!(local_open::is_text(text), "{text}");
        assert_eq!(
            local_open::opening(text, true),
            LocalOpening::Edit,
            "{text}"
        );
    }
    assert_eq!(local_open::extension(".gitignore"), Some(".gitignore"));
    assert_eq!(local_open::extension("archive.tar.gz"), Some(".gz"));
    assert_eq!(local_open::extension("README"), None);
    assert_eq!(local_open::extension("trailing."), None);
    assert_eq!(
        local_open::opening("photo.png", true),
        LocalOpening::Confirm
    );
    assert_eq!(local_open::opening("photo.png", false), LocalOpening::Open);
    assert_eq!(local_open::TEXT_EXTENSIONS.len(), 38, "the C# list, whole");
}

#[test]
fn a_file_runs_by_windows_rule_and_by_unix_rule() {
    // Windows: PATHEXT, else its default, and what the shell runs besides.
    for runs in [
        "setup.EXE",
        "tool.com",
        "update.msi",
        "patch.msp",
        "Desktop.lnk",
        "site.url",
        "page.hta",
        "saver.scr",
        "old.pif",
        "panel.cpl",
        "console.msc",
        "keys.reg",
        "app.jar",
        "macro.vbs",
        "macro.vbe",
        "script.jse",
        "job.wsf",
        "job.wsh",
        "setup.exe.",
        "setup.exe ",
        // High risk to Windows' Attachment Manager though PATHEXT leaves them out.
        "manual.CHM",
        "tool.appref-ms",
        "panel.settingcontent-ms",
        "trouble.diagcab",
        "disk.vhdx",
        "find.search-ms",
        "script.py",
    ] {
        assert!(local_open::windows_runnable(runs, None), "{runs}");
    }
    let mut listed = local_open::WINDOWS_RUNNABLE_EXTENSIONS.to_vec();
    listed.sort_unstable();
    listed.dedup();
    assert_eq!(
        listed.len(),
        local_open::WINDOWS_RUNNABLE_EXTENSIONS.len(),
        "each type once"
    );
    // Text first: a script listed as both is shown in the editor, never run.
    for script in ["deploy.ps1", "build.bat", "tool.py"] {
        assert!(local_open::windows_runnable(script, None), "{script}");
        assert_eq!(
            local_open::opening(script, true),
            LocalOpening::Edit,
            "{script}"
        );
    }
    for stays in ["photo.png", "report.pdf", "README", "setup.exe.txt"] {
        assert!(!local_open::windows_runnable(stays, None), "{stays}");
    }
    assert!(
        local_open::windows_runnable("tool.rexx", Some(".COM;.EXE;.REXX")),
        "PATHEXT as set"
    );
    assert!(
        !local_open::windows_runnable("tool.rexx", None),
        "unknown to the default PATHEXT and to the list"
    );
    assert!(
        local_open::windows_runnable("installer.msi", Some("")),
        "always, whatever PATHEXT says"
    );

    // Unix: an execute bit, any of the three; a mode unknown is asked about.
    for mode in [0o100_755, 0o100_700, 0o100_010, 0o100_001] {
        assert!(local_open::unix_runnable("tool", Some(mode)), "{mode:o}");
    }
    for mode in [0o100_644, 0o100_600, 0o100_000] {
        assert!(
            !local_open::unix_runnable("photo.png", Some(mode)),
            "{mode:o}"
        );
    }
    assert!(local_open::unix_runnable("photo.png", None));
    // A desktop entry, whose command the opener starts, whatever its mode.
    for entry in ["app.desktop", "App.DESKTOP"] {
        assert!(local_open::unix_runnable(entry, Some(0o100_644)), "{entry}");
        assert_eq!(
            local_open::opening(entry, local_open::unix_runnable(entry, Some(0o100_644))),
            LocalOpening::Confirm
        );
    }
}

/// `folder` listed in browser `pane` with `names`, files written there: runnable ones
/// (by name on Windows, by their execute bit on Unix) when `runs` says so.
fn listed_files(app: &mut App, pane: TabId, folder: &Path, names: &[(&str, bool)]) {
    let mut entries = Vec::new();
    for (name, runs) in names {
        let path = folder.join(name);
        std::fs::write(&path, b"x").expect("file");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt as _;
            let mode = if *runs { 0o755 } else { 0o644 };
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(mode)).expect("mode");
        }
        #[cfg(not(unix))]
        let _ = runs;
        entries.push(LocalEntry {
            name: (*name).into(),
            label: (*name).to_owned(),
            kind: EntryKind::File,
            size: Some(1),
            modified: None,
        });
    }
    app.update(Message::Files(FilesMessage::LocalListed {
        tab: pane,
        result: Ok((folder.to_owned(), entries)),
    }));
}

fn opened(app: &mut App, pane: TabId, index: usize) -> Vec<Effect> {
    app.update(Message::Files(FilesMessage::Open {
        tab: pane,
        side: Side::Local,
        index,
    }))
}

#[test]
fn a_text_file_opens_in_the_editor_another_with_its_program_and_one_that_runs_once_agreed() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path());
    let sink = Arc::new(RecordingSink::default());
    let shell = open(&mut app, None);
    let (pane, folder) = docked(&start(&mut app, shell, &sink));
    listed_files(
        &mut app,
        pane,
        &folder,
        &[
            ("notes.md", false),
            ("photo.png", false),
            ("setup.exe", true),
        ],
    );

    // Text: the editor set, on the file itself, nothing watched or sent.
    let effects = opened(&mut app, pane, 0);
    let [Effect::LaunchEditor { tab, editor, file }] = effects.as_slice() else {
        panic!("{effects:?}");
    };
    assert_eq!((*tab, file), (pane, &folder.join("notes.md")));
    assert_eq!(
        *editor,
        heimdall_app::external_edit::editor("").expect("the system's editor")
    );
    let files = app
        .tab(pane)
        .and_then(|tab| tab.files.as_deref())
        .expect("files");
    assert!(files.edits.is_empty() && files.transfers.is_empty());

    // Anything else: the system's default program, at once.
    let effects = opened(&mut app, pane, 1);
    assert!(
        matches!(effects.as_slice(), [Effect::OpenLocalFile { tab, file }] if *tab == pane && *file == folder.join("photo.png")),
        "{effects:?}"
    );
    assert!(app.dialog.is_none());

    // A file that runs: asked first with its full path, nothing opened before.
    let runnable = folder.join("setup.exe");
    assert!(
        opened(&mut app, pane, 2).is_empty(),
        "nothing before agreeing"
    );
    let Some(dialog @ Dialog::ConfirmOpenRunnable { tab, file, shown }) = &app.dialog else {
        panic!("{:?}", app.dialog);
    };
    assert_eq!((*tab, file), (pane, &runnable));
    assert_eq!(*shown, runnable.to_string_lossy());
    assert!(!dialog.confirms_on_enter(), "a click, never an Enter");
    // Dismissed: nothing.
    assert!(app.update(Message::DismissDialog).is_empty());
    assert!(app.dialog.is_none());
    // Agreed: opened with its default program.
    assert!(opened(&mut app, pane, 2).is_empty());
    let effects = app.update(Message::ConfirmDialog);
    assert!(
        matches!(effects.as_slice(), [Effect::OpenLocalFile { tab, file }] if *tab == pane && *file == runnable),
        "{effects:?}"
    );
    assert!(sink.take().is_empty(), "nothing typed into the shell");
}

#[test]
fn a_program_that_does_not_start_is_said_on_this_computers_pane() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path());
    let sink = Arc::new(RecordingSink::default());
    let shell = open(&mut app, None);
    let (pane, folder) = docked(&start(&mut app, shell, &sink));
    listed(&mut app, pane, &folder);
    app.update(Message::Files(FilesMessage::EditorLaunched {
        tab: pane,
        result: Err(heimdall_app::files::FilesError::OpenFailed {
            detail: "no handler".to_owned(),
        }),
    }));
    let files = app
        .tab(pane)
        .and_then(|tab| tab.files.as_deref())
        .expect("files");
    assert!(files.local.error.is_some(), "its one pane");
    assert!(files.remote.error.is_none());
}

/// `folder` listed in browser `pane`: folder `sub`, then files `names`.
fn scripts_listed(app: &mut App, pane: TabId, folder: &Path, names: &[OsString]) {
    let entry = |name: OsString, kind| LocalEntry {
        label: name.to_string_lossy().into_owned(),
        name,
        kind,
        size: None,
        modified: None,
    };
    let mut entries = vec![entry("sub".into(), EntryKind::Directory)];
    entries.extend(
        names
            .iter()
            .map(|name| entry(name.clone(), EntryKind::File)),
    );
    app.update(Message::Files(FilesMessage::LocalListed {
        tab: pane,
        result: Ok((folder.to_owned(), entries)),
    }));
}

/// What [`browser_of_scripts`] gives: the app, the shell, its browser and the folder listed.
type ScriptsBrowser = (App, (TabId, AttemptId), TabId, PathBuf);

/// A local shell started in folder "my scripts", made in `dir`, its browser listing
/// `names` there.
fn browser_of_scripts(dir: &Path, names: &[&str], sink: &Arc<RecordingSink>) -> ScriptsBrowser {
    let folder = dir.join("my scripts");
    std::fs::create_dir_all(&folder).expect("folder");
    let mut app = app(dir);
    let shell = open(&mut app, Some(folder.clone()));
    let (pane, _) = docked(&start(&mut app, shell, sink));
    let names: Vec<OsString> = names.iter().map(Into::into).collect();
    scripts_listed(&mut app, pane, &folder, &names);
    (app, shell, pane, folder)
}

/// "Run in Shell" on entry `index` of browser `pane`.
fn run_in_shell(app: &mut App, pane: TabId, index: usize) -> Vec<Effect> {
    app.update(Message::Files(FilesMessage::RunInShell {
        tab: pane,
        index,
    }))
}

/// The question asked, taken as it is.
fn script_question(app: &App) -> ScriptConfirmation {
    match &app.dialog {
        Some(dialog @ Dialog::ConfirmRunScript(confirmation)) => {
            assert!(!dialog.confirms_on_enter(), "a click, never an Enter");
            (**confirmation).clone()
        }
        other => panic!("the script's question: {other:?}"),
    }
}

/// What `name` in `folder` is expected to run, its command as shown, and whether its
/// interpreter reads its line again: on Windows a `PowerShell` script by the default
/// shell's `PowerShell` and a batch script by `cmd.exe`, each by its full system path; on
/// Unix a shell script by `/bin/sh`.
fn expected_run(name: &str, folder: &Path) -> (LocalShell, String, bool) {
    let path = folder.join(name).to_string_lossy().into_owned();
    let (program, arguments, command, rereads) = match ScriptKind::of(name) {
        #[cfg(windows)]
        Some(ScriptKind::PowerShell) => {
            let root = std::env::var("SystemRoot").expect("SystemRoot");
            let program = format!(r"{root}\System32\WindowsPowerShell\v1.0\powershell.exe");
            let command = format!("\"{program}\" -NoLogo -NoExit -File \"{path}\"");
            let arguments = ["-NoLogo", "-NoExit", "-File", path.as_str()]
                .map(str::to_owned)
                .to_vec();
            (program, LocalArguments::List(arguments), command, false)
        }
        #[cfg(windows)]
        Some(ScriptKind::Batch) => {
            let root = std::env::var("SystemRoot").expect("SystemRoot");
            let program = format!(r"{root}\System32\cmd.exe");
            let line = format!("/s /k \"\"{path}\"\"");
            let command = format!("\"{program}\" {line}");
            (program, LocalArguments::WindowsLine(line), command, true)
        }
        #[cfg(unix)]
        Some(ScriptKind::Posix) => {
            let command = format!("/bin/sh '{path}'");
            (
                "/bin/sh".to_owned(),
                LocalArguments::List(vec![path]),
                command,
                false,
            )
        }
        other => panic!("{name} does not run here: {other:?}"),
    };
    let shell = LocalShell {
        name: name.to_owned(),
        program: Some(program),
        arguments,
        working_directory: Some(folder.to_owned()),
        environment: Vec::new(),
    };
    (shell, command, rereads)
}

/// The scripts this platform runs, as the tests list them.
#[cfg(windows)]
const RUN_HERE: [&str; 3] = ["deploy.ps1", "build.BAT", "tool.cmd"];
#[cfg(unix)]
const RUN_HERE: [&str; 1] = ["run.sh"];

#[test]
fn a_script_is_known_by_its_csharp_extension_and_offered_only_where_it_runs() {
    for (name, kind) in [
        ("deploy.ps1", ScriptKind::PowerShell),
        ("Deploy.PS1", ScriptKind::PowerShell),
        ("build.bat", ScriptKind::Batch),
        ("tool.CMD", ScriptKind::Batch),
        ("run.sh", ScriptKind::Posix),
    ] {
        assert_eq!(ScriptKind::of(name), Some(kind), "{name}");
        assert_eq!(
            script_shell::runnable_here(name),
            kind.runs_here().then_some(kind),
            "{name}"
        );
    }
    for other in ["notes.txt", "setup.exe", "run.sh.txt", "ps1", "tool.py"] {
        assert_eq!(ScriptKind::of(other), None, "{other}");
    }
    #[cfg(windows)]
    {
        assert!(ScriptKind::PowerShell.runs_here() && ScriptKind::Batch.runs_here());
        assert!(!ScriptKind::Posix.runs_here(), "no sh on Windows");
    }
    #[cfg(unix)]
    {
        assert!(ScriptKind::Posix.runs_here());
        assert!(!ScriptKind::PowerShell.runs_here() && !ScriptKind::Batch.runs_here());
    }
}

#[test]
fn a_scripts_command_line_carries_its_path_as_text_or_is_refused() {
    let batch = |path: &str| script_shell::arguments(ScriptKind::Batch, Path::new(path));
    // Inside the inner quotes, cmd.exe reads & ^ | < > and parentheses as text; /s takes
    // off the outer pair only.
    assert_eq!(
        batch(r"C:\a&b^c|(d)<e>\x y.bat"),
        Ok(LocalArguments::WindowsLine(
            r#"/s /k ""C:\a&b^c|(d)<e>\x y.bat"""#.to_owned()
        ))
    );
    for (path, refused) in [
        (r"C:\100%\x.bat", '%'),
        (r"C:\go!\x.cmd", '!'),
        ("C:\\a\"b\\x.bat", '"'),
        ("C:\\a\nb\\x.bat", '\n'),
    ] {
        assert_eq!(
            batch(path),
            Err(ScriptRefusal::Character(refused)),
            "{path}"
        );
    }
    // PowerShell's -File takes a path as a path: only what the command line cannot carry.
    let powershell = |path: &str| script_shell::arguments(ScriptKind::PowerShell, Path::new(path));
    assert_eq!(
        powershell(r"C:\100% & 'x'\$go!.ps1"),
        Ok(LocalArguments::List(
            ["-NoExit", "-File", r"C:\100% & 'x'\$go!.ps1"]
                .map(str::to_owned)
                .to_vec()
        ))
    );
    assert_eq!(
        powershell("C:\\a\"b.ps1"),
        Err(ScriptRefusal::Character('"'))
    );
    assert_eq!(
        powershell("C:\\a\tb.ps1"),
        Err(ScriptRefusal::Character('\t'))
    );
    // sh gets the path as an argument of its own: nothing in it is read by a shell.
    assert_eq!(
        script_shell::arguments(ScriptKind::Posix, Path::new("/tmp/a 'b'$(c)%!.sh")),
        Ok(LocalArguments::List(vec!["/tmp/a 'b'$(c)%!.sh".to_owned()]))
    );
}

#[test]
fn run_in_shell_is_offered_for_one_script_alone_never_a_folder_or_another_file() {
    let dir = tempfile::tempdir().expect("dir");
    let sink = Arc::new(RecordingSink::default());
    let names = [
        "deploy.ps1",
        "build.bat",
        "tool.cmd",
        "run.sh",
        "notes.txt",
        "setup.exe",
    ];
    let (mut app, shell, pane, _) = browser_of_scripts(dir.path(), &names, &sink);
    let offered: Vec<usize> = (0..=names.len())
        .filter(|index| app.offers_run_in_shell(pane, *index))
        .collect();
    // Entry 0 is the folder; the files follow it.
    #[cfg(windows)]
    assert_eq!(offered, [1, 2, 3], "PowerShell and batch scripts");
    #[cfg(unix)]
    assert_eq!(offered, [4], "a shell script alone");
    assert!(
        !app.offers_run_in_shell(shell.0, 1),
        "not in the shell's own tab"
    );
    // Several chosen: none, and choosing it anyway asks nothing.
    let script = offered[0];
    for message in [
        FilesMessage::Select {
            tab: pane,
            side: Side::Local,
            index: script,
        },
        FilesMessage::Toggle {
            tab: pane,
            side: Side::Local,
            index: 5,
        },
    ] {
        app.update(Message::Files(message));
    }
    assert!(!app.offers_run_in_shell(pane, script));
    assert!(run_in_shell(&mut app, pane, script).is_empty());
    assert!(app.dialog.is_none());
    // Nor a folder or another file.
    app.update(Message::Files(FilesMessage::Select {
        tab: pane,
        side: Side::Local,
        index: 0,
    }));
    for index in [0, 5, 6] {
        assert!(run_in_shell(&mut app, pane, index).is_empty());
        assert!(app.dialog.is_none(), "{index}");
    }
    assert!(sink.take().is_empty());
}

#[test]
fn run_in_shell_asks_with_the_exact_command_then_opens_a_new_tab_in_the_scripts_folder() {
    let dir = tempfile::tempdir().expect("dir");
    let sink = Arc::new(RecordingSink::default());
    let (mut app, shell, pane, folder) = browser_of_scripts(dir.path(), &RUN_HERE, &sink);
    for (at, name) in RUN_HERE.iter().enumerate() {
        let index = at + 1;
        let (expected, command, rereads) = expected_run(name, &folder);
        let tabs = app.tabs.len();
        // Asked first, nothing run.
        assert!(run_in_shell(&mut app, pane, index).is_empty(), "{name}");
        let question = script_question(&app);
        assert_eq!(question.command, command, "{name}: the command whole");
        assert_eq!(
            question.folder.as_deref(),
            Some(folder.to_string_lossy().as_ref())
        );
        assert_eq!(question.rereads, rereads, "{name}");
        assert_eq!(question.name, *name);
        assert_eq!(question.replaces, None);
        // Dismissed: nothing.
        assert!(app.update(Message::DismissDialog).is_empty());
        assert!(app.dialog.is_none());
        assert_eq!(app.tabs.len(), tabs);
        // Agreed: a new tab running exactly what was shown, as any local shell.
        assert!(run_in_shell(&mut app, pane, index).is_empty());
        let effects = app.update(Message::ConfirmDialog);
        let (attempt, running) =
            started(&effects).unwrap_or_else(|| panic!("{name}: started, {effects:?}"));
        assert_eq!(running, expected, "{name}");
        assert!(running.environment.is_empty(), "no HEIMDALL_* variables");
        assert_eq!(app.tabs.len(), tabs + 1, "a new tab");
        let tab = app.tab(attempt.0).expect("tab");
        assert_eq!(tab.title, *name, "named after the script");
        assert_eq!(tab.purpose, Purpose::Shell);
        assert!(!app.can_save_as_profile(tab));
        // Once started, its own browser, in the script's folder.
        let (own, listed) = docked(&start(&mut app, attempt, &sink));
        assert_eq!(listed, folder);
        assert_eq!(leaves(&app, attempt.0), [attempt.0, own]);
    }
    // The shell beside the browser is left as it was, nothing typed into it.
    assert_eq!(leaves(&app, shell.0), [shell.0, pane]);
    assert!(sink.take().is_empty());
}

#[cfg(windows)]
#[test]
fn a_scripts_powershell_gets_the_execution_policy_set_once_reconnect_included() {
    let dir = tempfile::tempdir().expect("dir");
    let sink = Arc::new(RecordingSink::default());
    let (mut app, _, pane, folder) = browser_of_scripts(dir.path(), &["deploy.ps1"], &sink);
    app.update(Message::Settings(
        SettingsMessage::PowerShellExecutionPolicy(
            heimdall_core::settings::ExecutionPolicy::RemoteSigned,
        ),
    ));
    run_in_shell(&mut app, pane, 1);
    let path = folder.join("deploy.ps1").to_string_lossy().into_owned();
    let flags = [
        "-ExecutionPolicy",
        "RemoteSigned",
        "-NoLogo",
        "-NoExit",
        "-File",
        path.as_str(),
    ]
    .map(str::to_owned)
    .to_vec();
    assert!(script_question(&app).command.ends_with(&format!(
        "-ExecutionPolicy RemoteSigned -NoLogo -NoExit -File \"{path}\""
    )));
    let (attempt, running) = started(&app.update(Message::ConfirmDialog)).expect("started");
    assert_eq!(running.arguments, LocalArguments::List(flags.clone()));
    start(&mut app, attempt, &sink);
    app.update(Message::ReconnectTab(attempt.0));
    let (_, again) = started(&app.update(Message::ConfirmDialog)).expect("again");
    assert_eq!(again.arguments, LocalArguments::List(flags), "not twice");
}

#[cfg(windows)]
#[test]
fn a_batch_path_cmd_would_read_is_refused_with_a_notice_and_nothing_runs() {
    let dir = tempfile::tempdir().expect("dir");
    let sink = Arc::new(RecordingSink::default());
    let names = ["100%.bat", "go!.cmd", "100%.ps1", "a&b^(c).bat"];
    let (mut app, _, pane, folder) = browser_of_scripts(dir.path(), &names, &sink);
    for (index, refused) in [(1, "%"), (2, "!")] {
        let tabs = app.tabs.len();
        assert!(run_in_shell(&mut app, pane, index).is_empty());
        assert!(app.dialog.is_none(), "nothing asked");
        assert_eq!(app.tabs.len(), tabs, "nothing run");
        let files = app
            .tab(pane)
            .and_then(|tab| tab.files.as_deref())
            .expect("files");
        assert_eq!(
            files.local.error,
            Some(FilesError::ScriptPathCharacter {
                character: refused.to_owned()
            })
        );
    }
    // PowerShell takes % as text; cmd.exe & ^ and parentheses inside the quotes.
    for index in [3, 4] {
        run_in_shell(&mut app, pane, index);
        let (_, command, _) = expected_run(names[index - 1], &folder);
        assert_eq!(script_question(&app).command, command);
        app.update(Message::DismissDialog);
    }
    assert!(sink.take().is_empty());
}

#[cfg(unix)]
#[test]
fn a_script_path_that_is_not_text_is_refused_with_a_notice_and_nothing_runs() {
    use std::os::unix::ffi::OsStringExt as _;
    let dir = tempfile::tempdir().expect("dir");
    let sink = Arc::new(RecordingSink::default());
    let (mut app, _, pane, folder) = browser_of_scripts(dir.path(), &[], &sink);
    let name = OsString::from_vec(b"r\xffn.sh".to_vec());
    scripts_listed(&mut app, pane, &folder, &[name]);
    let tabs = app.tabs.len();
    assert!(app.offers_run_in_shell(pane, 1), "a shell script still");
    assert!(run_in_shell(&mut app, pane, 1).is_empty());
    assert!(app.dialog.is_none(), "nothing asked");
    assert_eq!(app.tabs.len(), tabs, "nothing run");
    let files = app
        .tab(pane)
        .and_then(|tab| tab.files.as_deref())
        .expect("files");
    assert_eq!(files.local.error, Some(FilesError::ScriptPathNotText));
    assert!(sink.take().is_empty());
}

#[test]
fn reconnecting_a_scripts_tab_asks_again_then_runs_it_in_its_place() {
    let dir = tempfile::tempdir().expect("dir");
    let sink = Arc::new(RecordingSink::default());
    let (mut app, _, pane, folder) = browser_of_scripts(dir.path(), &RUN_HERE[..1], &sink);
    let (expected, command, _) = expected_run(RUN_HERE[0], &folder);
    run_in_shell(&mut app, pane, 1);
    let (attempt, _) = started(&app.update(Message::ConfirmDialog)).expect("started");
    let tab = attempt.0;
    let (own, _) = docked(&start(&mut app, attempt, &sink));
    app.update(Message::Connection {
        tab,
        attempt: attempt.1,
        event: ConnectionEvent::Closed {
            exit_status: Some(0),
        },
    });
    assert!(app.can_reconnect(app.tab(tab).expect("tab")));
    let place = app.tabs.iter().position(|found| found.id == tab);
    let tabs = app.tabs.len();
    // Asked again, the same command, nothing run.
    assert!(app.update(Message::ReconnectTab(tab)).is_empty());
    let question = script_question(&app);
    assert_eq!(question.command, command);
    assert_eq!(question.replaces, Some(tab));
    // Dismissed: the tab stays as it was.
    assert!(app.update(Message::DismissDialog).is_empty());
    assert!(matches!(
        app.tab(tab).expect("still there").phase,
        Phase::Closed { .. }
    ));
    // Agreed: the same run, in its place, its browser beside it.
    app.update(Message::ReconnectTab(tab));
    let ((again, attempt_again), running) =
        started(&app.update(Message::ConfirmDialog)).expect("run again");
    assert_eq!(running, expected);
    assert!(app.tab(tab).is_none(), "replaced");
    assert_eq!(app.tabs.len(), tabs);
    assert_eq!(app.tabs.iter().position(|found| found.id == again), place);
    assert_eq!(leaves(&app, again), [again, own]);
    // And asked again the next time too, once it runs.
    start(&mut app, (again, attempt_again), &sink);
    assert!(app.update(Message::ReconnectTab(again)).is_empty());
    assert_eq!(script_question(&app).replaces, Some(again));
    app.update(Message::DismissDialog);
    assert!(sink.take().is_empty(), "nothing typed into any shell");
}

#[test]
fn a_scripts_tab_is_not_offered_again_at_the_next_start() {
    let dir = tempfile::tempdir().expect("dir");
    let sink = Arc::new(RecordingSink::default());
    let (mut app, _, pane, _) = browser_of_scripts(dir.path(), &RUN_HERE[..1], &sink);
    run_in_shell(&mut app, pane, 1);
    let (attempt, _) = started(&app.update(Message::ConfirmDialog)).expect("started");
    start(&mut app, attempt, &sink);
    let mut effects = app.update(Message::WindowCloseRequested);
    if matches!(app.dialog, Some(Dialog::ConfirmExit { .. })) {
        effects = app.update(Message::ConfirmDialog);
    }
    assert!(matches!(effects.as_slice(), [Effect::Exit]), "{effects:?}");
    let snapshot = session_snapshot::snapshot_path(&dir.path().join("profiles.toml"));
    assert!(
        session_snapshot::load(&snapshot).is_none(),
        "no session to reopen: the script runs again only once asked"
    );
}
