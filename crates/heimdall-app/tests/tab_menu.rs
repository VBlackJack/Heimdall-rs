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

//! A tab's menu, as the C# Heimdall's: rename, reconnect a live session, duplicate, close
//! the others or those to the right.

use std::path::Path;
use std::sync::{Arc, Mutex};

use heimdall_app::local_driver::LocalShell;
use heimdall_app::{
    App, AppConfig, AttemptId, ConnectionEvent, Dialog, Effect, InputSink, Message, Phase,
    ProfileKind, TabGroup, TabId, TabMenuMessage, UiError,
};
use heimdall_core::profile::{ProfileId, SshProfile, WinRmProfile};
use heimdall_core::store::ProfileStore;
use heimdall_ssh::{AgentSource, SessionClosed, TerminalSize};
use heimdall_term::GridSize;
use heimdall_term::local::LocalArguments;

#[derive(Debug, Default)]
struct Sink {
    closed: Mutex<bool>,
}

impl InputSink for Sink {
    fn write(&self, _bytes: Vec<u8>) -> Result<(), SessionClosed> {
        Ok(())
    }
    fn resize(&self, _size: TerminalSize) -> Result<(), SessionClosed> {
        Ok(())
    }
    fn close(&self) {
        *self.closed.lock().expect("closed") = true;
    }
}

fn app(dir: &Path) -> App {
    let profiles_file = dir.join("profiles.toml");
    let mut store = ProfileStore::open(&profiles_file).expect("store");
    store.merge([SshProfile {
        id: ProfileId::new("a"),
        name: "server a".to_owned(),
        group: None,
        host: "a.lab".to_owned(),
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
    }]);
    store.merge_winrm([WinRmProfile {
        id: ProfileId::new("w"),
        name: "w".to_owned(),
        group: None,
        // Refused by the command Heimdall writes: its tab says why.
        host: "-bad".to_owned(),
        port: 5985,
        use_ssl: false,
        skip_certificate_check: false,
        username: None,
        gateway: None,
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
    // The tabs the menus act on alone: no SFTP pane docked beside a shell connected.
    app.update(Message::Settings(
        heimdall_app::SettingsMessage::SftpBrowser(heimdall_core::settings::SftpBrowser {
            auto_open_on_ssh: false,
            ..heimdall_core::settings::SftpBrowser::default()
        }),
    ));
    app
}

fn open(app: &mut App) -> (TabId, AttemptId) {
    match app
        .update(Message::OpenProfile(ProfileId::new("a")))
        .as_slice()
    {
        [Effect::Connect { tab, attempt, .. }] => (*tab, *attempt),
        other => panic!("expected one Connect, got {other:?}"),
    }
}

fn event(app: &mut App, tab: TabId, attempt: AttemptId, event: ConnectionEvent) {
    app.update(Message::Connection {
        tab,
        attempt,
        event,
    });
}

/// A tab whose session is live.
fn live(app: &mut App) -> (TabId, Arc<Sink>) {
    let (tab, attempt) = open(app);
    let sink = Arc::new(Sink::default());
    event(
        app,
        tab,
        attempt,
        ConnectionEvent::Connected {
            input: sink.clone(),
        },
    );
    assert_eq!(app.tab(tab).expect("tab").phase, Phase::Connected);
    (tab, sink)
}

/// A tab whose attempt failed.
fn failed(app: &mut App) -> TabId {
    let (tab, attempt) = open(app);
    event(app, tab, attempt, ConnectionEvent::Failed(UiError::Timeout));
    tab
}

fn menu(app: &mut App, message: TabMenuMessage) -> Vec<Effect> {
    app.update(Message::TabMenu(message))
}

fn ids(app: &App) -> Vec<TabId> {
    app.tabs.iter().map(|tab| tab.id).collect()
}

#[test]
fn a_tab_takes_the_name_typed_and_gives_it_back_when_emptied() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path());
    let (tab, attempt) = open(&mut app);
    menu(&mut app, TabMenuMessage::Rename(tab));
    assert_eq!(
        app.dialog,
        Some(Dialog::RenameTab {
            tab,
            value: "server a".to_owned()
        }),
        "the present name written in"
    );
    menu(
        &mut app,
        TabMenuMessage::NameEdited("  prod \u{202e}db\u{7}  ".to_owned()),
    );
    app.update(Message::ConfirmDialog);
    let renamed = app.tab(tab).expect("tab");
    assert_eq!(renamed.display_title(), "prod db", "trimmed and made safe");
    assert_eq!(renamed.title, "server a", "its own title kept beside");

    // The server's title changes the tab's own, not the name given.
    event(
        &mut app,
        tab,
        attempt,
        ConnectionEvent::Connected {
            input: Arc::new(Sink::default()),
        },
    );
    event(
        &mut app,
        tab,
        attempt,
        ConnectionEvent::Output(b"\x1b]0;shell title\x07".to_vec()),
    );
    let renamed = app.tab(tab).expect("tab");
    assert_eq!(renamed.title, "shell title");
    assert_eq!(renamed.display_title(), "prod db");

    menu(&mut app, TabMenuMessage::ResetTitle(tab));
    assert_eq!(app.tab(tab).expect("tab").display_title(), "shell title");

    menu(&mut app, TabMenuMessage::Rename(tab));
    menu(&mut app, TabMenuMessage::NameEdited("x".to_owned()));
    app.update(Message::DismissDialog);
    assert_eq!(
        app.tab(tab).expect("tab").custom_title,
        None,
        "cancelled: unchanged"
    );
    menu(&mut app, TabMenuMessage::Rename(tab));
    menu(&mut app, TabMenuMessage::NameEdited("named".to_owned()));
    app.update(Message::ConfirmDialog);
    menu(&mut app, TabMenuMessage::Rename(tab));
    menu(&mut app, TabMenuMessage::NameEdited("   ".to_owned()));
    app.update(Message::ConfirmDialog);
    assert_eq!(app.tab(tab).expect("tab").custom_title, None, "emptied");
}

#[test]
fn a_live_session_reconnects_from_the_menu_under_its_name() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path());
    let (connecting, _) = open(&mut app);
    assert!(
        !app.can_restart(app.tab(connecting).expect("tab")),
        "still connecting: nothing to restart"
    );
    assert!(app.update(Message::ReconnectTab(connecting)).is_empty());

    let (tab, sink) = live(&mut app);
    assert!(app.can_restart(app.tab(tab).expect("tab")));
    assert!(
        !app.can_reconnect(app.tab(tab).expect("tab")),
        "not on a card: a live tab shows none"
    );
    menu(&mut app, TabMenuMessage::Rename(tab));
    menu(&mut app, TabMenuMessage::NameEdited("mine".to_owned()));
    app.update(Message::ConfirmDialog);
    let effects = app.update(Message::ReconnectTab(tab));
    let again = match effects.as_slice() {
        [Effect::Connect { tab, .. }] => *tab,
        other => panic!("expected one Connect, got {other:?}"),
    };
    assert!(app.tab(tab).is_none(), "replaced");
    assert_eq!(ids(&app), [connecting, again], "in its place");
    assert_eq!(app.tab(again).expect("again").display_title(), "mine");
    assert!(*sink.closed.lock().expect("closed"), "the old session ends");
}

#[test]
fn a_duplicate_opens_beside_and_leaves_the_tab() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path());
    let (tab, _) = live(&mut app);
    let other = failed(&mut app);
    let effects = menu(&mut app, TabMenuMessage::Duplicate(tab));
    let copy = match effects.as_slice() {
        [Effect::Connect { tab, request, .. }] => {
            assert_eq!(request.profile.host, "a.lab");
            *tab
        }
        other => panic!("expected one Connect, got {other:?}"),
    };
    assert_eq!(ids(&app), [tab, other, copy], "the last");
    assert_eq!(app.tab(tab).expect("tab").phase, Phase::Connected);
    assert_eq!(app.active, Some(copy));

    let shell = LocalShell {
        name: "sh".to_owned(),
        program: Some("sh".to_owned()),
        arguments: LocalArguments::List(Vec::new()),
        working_directory: None,
        environment: Vec::new(),
    };
    let effects = app.update(Message::OpenLocal(shell));
    let local = app.tabs.last().expect("local").id;
    assert!(!effects.is_empty());
    assert!(
        !menu(&mut app, TabMenuMessage::Duplicate(local)).is_empty(),
        "a local shell opens again too"
    );
    assert_eq!(app.tabs.len(), 5);

    app.update(Message::RequestDeleteProfile(ProfileId::new("a")));
    app.update(Message::ConfirmDialog);
    assert!(!app.can_reopen(app.tab(tab).expect("tab")));
    assert!(menu(&mut app, TabMenuMessage::Duplicate(tab)).is_empty());
    assert_eq!(app.tabs.len(), 5, "its profile deleted: nothing opens");
}

#[test]
fn tabs_that_ended_close_at_once() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path());
    let tabs: Vec<TabId> = (0..4).map(|_| failed(&mut app)).collect();
    assert_eq!(app.tab_group(tabs[1], TabGroup::Right), [tabs[2], tabs[3]]);
    assert_eq!(
        app.tab_group(tabs[1], TabGroup::Others),
        [tabs[0], tabs[2], tabs[3]]
    );
    assert!(app.tab_group(tabs[3], TabGroup::Right).is_empty());
    let gone = failed(&mut app);
    app.update(Message::RequestCloseTab(gone));
    assert!(
        app.tab_group(gone, TabGroup::Others).is_empty(),
        "a closed tab"
    );

    menu(
        &mut app,
        TabMenuMessage::Close {
            tab: tabs[1],
            group: TabGroup::Right,
        },
    );
    assert_eq!(app.dialog, None, "nothing live: no question");
    assert_eq!(ids(&app), [tabs[0], tabs[1]]);
    menu(
        &mut app,
        TabMenuMessage::Close {
            tab: tabs[1],
            group: TabGroup::Others,
        },
    );
    assert_eq!(ids(&app), [tabs[1]]);
    assert_eq!(app.active, Some(tabs[1]));
}

#[test]
fn live_tabs_close_together_once_asked() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path());
    let ended = failed(&mut app);
    let (kept, _) = live(&mut app);
    let (live_one, sink) = live(&mut app);
    let ended_last = failed(&mut app);
    let close = TabMenuMessage::Close {
        tab: kept,
        group: TabGroup::Others,
    };
    menu(&mut app, close.clone());
    assert_eq!(
        app.dialog,
        Some(Dialog::ConfirmCloseTabs {
            tabs: vec![ended, live_one, ended_last],
            live: 1,
            unsaved: 0,
        }),
        "asked once for them all"
    );
    app.update(Message::DismissDialog);
    assert_eq!(app.tabs.len(), 4, "declined: all stay");
    menu(&mut app, close);
    app.update(Message::ConfirmDialog);
    assert_eq!(ids(&app), [kept]);
    assert!(*sink.closed.lock().expect("closed"));
}

#[test]
fn a_tab_tells_its_protocol() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path());
    let (tab, _) = open(&mut app);
    assert_eq!(app.tab_kind(app.tab(tab).expect("tab")), ProfileKind::Ssh);
    let profile = app.tab_profile(app.tab(tab).expect("tab")).expect("saved");
    assert_eq!(profile.id, ProfileId::new("a"));
    // A WinRM session runs in a local PowerShell: its tab still says WinRM.
    app.update(Message::OpenWinRm(ProfileId::new("w")));
    let winrm = app.tabs.last().expect("winrm tab");
    assert_eq!(app.tab_kind(winrm), ProfileKind::WinRm);
}

#[test]
fn a_pinned_tab_goes_first_and_is_left_by_close_others_and_close_right() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path());
    let first = failed(&mut app);
    let second = failed(&mut app);
    let third = failed(&mut app);
    app.update(Message::SelectTab(second));
    menu(&mut app, TabMenuMessage::Pin(third));
    assert_eq!(ids(&app), [third, first, second], "the pinned first");
    assert!(app.tab(third).expect("tab").pinned);
    assert_eq!(app.active, Some(second), "the tab shown stays shown");

    assert_eq!(
        app.tab_group(first, TabGroup::Others),
        [second],
        "the pinned left"
    );
    menu(
        &mut app,
        TabMenuMessage::Close {
            tab: first,
            group: TabGroup::Others,
        },
    );
    assert_eq!(ids(&app), [third, first]);

    menu(&mut app, TabMenuMessage::Pin(third));
    assert!(!app.tab(third).expect("tab").pinned, "unpinned");
}

#[test]
fn a_session_saved_nowhere_opens_the_form_of_a_new_profile_filled_from_it() {
    use heimdall_app::QuickResult;

    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path());
    let saved = failed(&mut app);
    assert!(!app.can_save_as_profile(app.tab(saved).expect("tab")));
    app.update(Message::QuickConnect(QuickResult::Ssh {
        username: Some("root".to_owned()),
        host: "jump.lab".to_owned(),
        port: 2222,
    }));
    let typed = *ids(&app).last().expect("the typed one");
    assert!(app.can_save_as_profile(app.tab(typed).expect("tab")));
    menu(&mut app, TabMenuMessage::SaveAsProfile(typed));
    assert!(
        matches!(&app.dialog, Some(Dialog::EditProfile { draft, .. })
            if draft.editing.is_none()
                && draft.host == "jump.lab"
                && draft.port == "2222"
                && draft.username == "root"),
        "{:?}",
        app.dialog
    );
}

#[test]
fn reveal_in_tree_selects_the_profile_its_folders_opened() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path());
    let tab = failed(&mut app);
    // Its folder closed: none, so "(No Folder)".
    app.update(Message::ToggleFolder(heimdall_app::NO_FOLDER.to_owned()));
    menu(&mut app, TabMenuMessage::RevealInTree(tab));
    assert_eq!(app.selected_profile, Some(ProfileId::new("a")));
    assert!(
        app.tree_rows("")
            .iter()
            .any(|row| matches!(row, heimdall_app::TreeRow::Profile { profile, .. } if profile.id.as_str() == "a")),
        "shown, its folder opened"
    );
}

#[test]
fn the_sessions_limit_refuses_one_more_but_not_a_reconnect_nor_a_local_shell() {
    use heimdall_app::{Notice, SettingsMessage};
    use heimdall_term::local::LocalArguments;

    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path());
    assert_eq!(app.settings().max_sessions, 0, "no limit by default");
    app.update(Message::Settings(SettingsMessage::MaxSessions(21)));
    assert_eq!(app.settings().max_sessions, 0, "out of the C# range: kept");
    app.update(Message::Settings(SettingsMessage::MaxSessions(1)));

    let first = failed(&mut app);
    assert!(
        app.update(Message::OpenProfile(ProfileId::new("a")))
            .is_empty(),
        "a second one refused"
    );
    assert_eq!(app.tabs.len(), 1);
    assert_eq!(app.notice(), Some(&Notice::SessionLimitReached(1)));

    // In the first one's place: no more room taken.
    assert!(
        !app.update(Message::ReconnectTab(first)).is_empty(),
        "reconnected"
    );
    assert_eq!(app.tabs.len(), 1);

    // This computer's own shells are not counted.
    let effects = app.update(Message::OpenLocal(heimdall_app::local_driver::LocalShell {
        name: "shell".to_owned(),
        program: None,
        arguments: LocalArguments::List(Vec::new()),
        working_directory: None,
        environment: Vec::new(),
    }));
    assert!(!effects.is_empty());
    assert_eq!(app.tabs.len(), 2);
}

#[test]
fn a_tab_dragged_onto_another_takes_its_place_within_its_own_group() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path());
    let first = failed(&mut app);
    let second = failed(&mut app);
    let third = failed(&mut app);
    let moved = |app: &mut App, tab, onto| {
        app.update(Message::MoveTab { tab, onto });
    };
    moved(&mut app, third, first);
    assert_eq!(ids(&app), [third, first, second], "to the left");
    moved(&mut app, third, second);
    assert_eq!(ids(&app), [first, second, third], "to the right");
    moved(&mut app, second, second);
    assert_eq!(ids(&app), [first, second, third], "onto itself");

    // Pinned first, always: an unpinned tab dragged onto a pinned one stays after it, a
    // pinned one dragged onto an unpinned one stays before.
    menu(&mut app, TabMenuMessage::Pin(first));
    moved(&mut app, third, first);
    assert_eq!(ids(&app), [first, third, second]);
    moved(&mut app, first, second);
    assert_eq!(ids(&app), [first, third, second]);
}
