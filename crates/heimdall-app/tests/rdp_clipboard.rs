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

//! The clipboard of an RDP tab: what the server copies reaches this side, and this side's
//! text, or the files copied in Explorer, are offered to the server when its desktop comes
//! up or comes back into view.

use std::path::{Path, PathBuf};

use heimdall_app::{
    App, AppConfig, AttemptId, ConnectionEvent, Effect, Message, Notice, SaveState, TabId,
};
use heimdall_core::profile::{ProfileId, RdpProfile};
use heimdall_core::store::ProfileStore;
use heimdall_rdp::{CopyRefusal, Framebuffer, LocalClipboard, SaveEnd};
use heimdall_ssh::AgentSource;
use heimdall_term::GridSize;
use tokio::sync::mpsc;
use zeroize::Zeroizing;

fn app(dir: &Path) -> App {
    let profiles_file = dir.join("profiles.toml");
    let mut store = ProfileStore::open(&profiles_file).expect("store");
    store.merge_rdp([RdpProfile {
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
    }]);
    store.save().expect("save");
    App::new(AppConfig {
        profiles_file,
        known_hosts: dir.join("known_hosts"),
        legacy_dir: None,
        agent: AgentSource::Disabled,
        initial_grid: GridSize { cols: 80, rows: 24 },
        files_start: dir.to_owned(),
        system_credentials: heimdall_app::SystemCredentials::memory(),
    })
}

fn open(app: &mut App) -> (TabId, AttemptId) {
    match app
        .update(Message::OpenRdp(ProfileId::new("dc")))
        .as_slice()
    {
        [Effect::ConnectRdp { tab, attempt, .. }] => (*tab, *attempt),
        other => panic!("{other:?}"),
    }
}

/// The desktop comes up, sharing the clipboard or not; the offers it would receive, and the
/// effects its coming up asked for.
fn ready(
    app: &mut App,
    tab: TabId,
    attempt: AttemptId,
    shared: bool,
) -> (mpsc::UnboundedReceiver<LocalClipboard>, Vec<Effect>) {
    let (input, _) = mpsc::unbounded_channel();
    let (offers, received) = mpsc::unbounded_channel();
    let effects = app.update(Message::Connection {
        tab,
        attempt,
        event: ConnectionEvent::RdpReady {
            framebuffer: Framebuffer::new(64, 48),
            input,
            size: tokio::sync::watch::channel(None).0,
            clipboard: shared.then_some(offers),
        },
    });
    (received, effects)
}

fn reads_for(effects: &[Effect], wanted: TabId) -> bool {
    matches!(effects, [Effect::ReadDesktopClipboard { tab }] if *tab == wanted)
}

#[test]
fn a_desktop_that_comes_up_is_offered_this_sides_clipboard() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path());
    let (tab, attempt) = open(&mut app);
    let (mut offers, effects) = ready(&mut app, tab, attempt, true);
    assert!(reads_for(&effects, tab), "{effects:?}");
    app.update(Message::ClipboardText {
        tab,
        text: Some("copied here".to_owned()),
    });
    assert!(
        matches!(offers.try_recv(), Ok(LocalClipboard::Text(text)) if text.as_str() == "copied here")
    );
    // Nothing to offer: nothing sent.
    app.update(Message::ClipboardText {
        tab,
        text: Some(String::new()),
    });
    assert!(offers.try_recv().is_err());
}

#[test]
fn what_the_server_copies_goes_to_this_sides_clipboard() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path());
    let (tab, attempt) = open(&mut app);
    let _ = ready(&mut app, tab, attempt, true);
    let effects = app.update(Message::Connection {
        tab,
        attempt,
        event: ConnectionEvent::RemoteClipboard(Zeroizing::new("from the server".to_owned())),
    });
    assert!(
        matches!(effects.as_slice(), [Effect::WriteClipboard(text)] if text == "from the server"),
        "{effects:?}"
    );
}

#[test]
fn a_desktop_not_sharing_the_clipboard_is_never_offered_it() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path());
    let (tab, attempt) = open(&mut app);
    let (_, effects) = ready(&mut app, tab, attempt, false);
    assert!(effects.is_empty(), "{effects:?}");
    assert!(app.update(Message::SelectTab(tab)).is_empty());
    assert!(app.update(Message::WindowFocus(true)).is_empty());
}

#[test]
fn coming_back_to_the_desktop_offers_what_was_copied_meanwhile() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path());
    let (tab, attempt) = open(&mut app);
    let _ = ready(&mut app, tab, attempt, true);
    assert!(reads_for(&app.update(Message::SelectTab(tab)), tab));
    assert!(reads_for(&app.update(Message::WindowFocus(true)), tab));
    assert!(
        app.update(Message::WindowFocus(false)).is_empty(),
        "leaving the window offers nothing"
    );
}

#[test]
fn the_files_copied_in_explorer_are_offered_to_the_desktop() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path());
    let (tab, attempt) = open(&mut app);
    let (mut offers, _) = ready(&mut app, tab, attempt, true);
    let copied = vec![PathBuf::from("C:/Users/me/report.docx")];
    assert!(
        app.update(Message::ClipboardFiles {
            tab,
            paths: copied.clone(),
        })
        .is_empty()
    );
    assert!(matches!(offers.try_recv(), Ok(LocalClipboard::Files(paths)) if paths == copied));
    // The button sending the clipboard reads the files first, as coming back does.
    assert!(reads_for(&app.update(Message::SendClipboard(tab)), tab));
}

#[test]
fn files_the_session_could_not_offer_are_said() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path());
    let (tab, attempt) = open(&mut app);
    let _ = ready(&mut app, tab, attempt, true);
    for (refusal, notice) in [
        (CopyRefusal::TooManyEntries, Notice::RdpFilesTooMany),
        (CopyRefusal::TooLarge, Notice::RdpFilesTooLarge),
    ] {
        let effects = app.update(Message::Connection {
            tab,
            attempt,
            event: ConnectionEvent::RdpFilesRefused(refusal),
        });
        assert!(effects.is_empty(), "{effects:?}");
        assert_eq!(app.notice(), Some(&notice));
    }
}

fn save_state(app: &App, tab: TabId) -> (bool, Option<SaveState>) {
    let pane = app
        .tab(tab)
        .and_then(|found| found.desktop.as_deref())
        .expect("a desktop");
    (pane.can_save_files(), pane.save_state())
}

fn event(app: &mut App, tab: TabId, attempt: AttemptId, event: ConnectionEvent) {
    let effects = app.update(Message::Connection {
        tab,
        attempt,
        event,
    });
    assert!(effects.is_empty(), "{effects:?}");
}

#[test]
fn the_servers_copied_files_are_saved_into_the_folder_picked() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path());
    let (tab, attempt) = open(&mut app);
    let (mut offers, _) = ready(&mut app, tab, attempt, true);
    assert!(
        app.update(Message::SaveRemoteFiles(tab)).is_empty(),
        "nothing copied on the server"
    );
    event(
        &mut app,
        tab,
        attempt,
        ConnectionEvent::RdpRemoteFiles(true),
    );
    assert_eq!(save_state(&app, tab), (true, None));

    let effects = app.update(Message::SaveRemoteFiles(tab));
    assert!(
        matches!(effects.as_slice(), [Effect::PickSaveFolder { tab: asked }] if *asked == tab),
        "{effects:?}"
    );
    assert_eq!(save_state(&app, tab), (false, Some(SaveState::Picking)));
    assert!(matches!(offers.try_recv(), Ok(LocalClipboard::HoldOffers)));
    // The dialog gives the focus back: offering this side's clipboard then would take the
    // server's away, and what was read before is not offered either.
    assert!(app.update(Message::WindowFocus(true)).is_empty());
    app.update(Message::ClipboardText {
        tab,
        text: Some("read before".to_owned()),
    });
    assert!(offers.try_recv().is_err());

    let folder = dir.path().join("saved");
    app.update(Message::SaveFolderPicked {
        tab,
        folder: Some(folder.clone()),
    });
    assert!(
        matches!(offers.try_recv(), Ok(LocalClipboard::SaveRemoteFiles(asked)) if asked == folder)
    );
    event(
        &mut app,
        tab,
        attempt,
        ConnectionEvent::RdpSaveProgress { saved: 1, total: 3 },
    );
    assert_eq!(
        save_state(&app, tab),
        (false, Some(SaveState::Running { saved: 1, total: 3 }))
    );

    app.update(Message::CancelSave(tab));
    assert!(matches!(offers.try_recv(), Ok(LocalClipboard::CancelSave)));
    let ended = SaveEnd::Cancelled { saved: 1, total: 3 };
    let effects = app.update(Message::Connection {
        tab,
        attempt,
        event: ConnectionEvent::RdpSaveEnded(ended),
    });
    assert!(
        reads_for(&effects, tab),
        "what was copied meanwhile is offered: {effects:?}"
    );
    assert_eq!(app.notice(), Some(&Notice::RdpFilesSaveEnded(ended)));
    assert_eq!(
        save_state(&app, tab),
        (true, None),
        "the files can be saved again"
    );
}

#[test]
fn closing_the_folder_dialog_saves_nothing() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path());
    let (tab, attempt) = open(&mut app);
    let (mut offers, _) = ready(&mut app, tab, attempt, true);
    event(
        &mut app,
        tab,
        attempt,
        ConnectionEvent::RdpRemoteFiles(true),
    );
    app.update(Message::SaveRemoteFiles(tab));
    assert!(matches!(offers.try_recv(), Ok(LocalClipboard::HoldOffers)));
    let effects = app.update(Message::SaveFolderPicked { tab, folder: None });
    assert!(
        matches!(offers.try_recv(), Ok(LocalClipboard::CancelSave)),
        "the offers held are released"
    );
    assert!(reads_for(&effects, tab), "{effects:?}");
    assert_eq!(save_state(&app, tab), (true, None));

    // The server copies something else: nothing to save any more.
    event(
        &mut app,
        tab,
        attempt,
        ConnectionEvent::RdpRemoteFiles(false),
    );
    assert_eq!(save_state(&app, tab), (false, None));
}

#[test]
fn an_image_goes_both_ways_between_the_clipboards() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path());
    let (tab, attempt) = open(&mut app);
    let (mut offers, _) = ready(&mut app, tab, attempt, true);
    let image: Vec<u8> = (0..64_u8).collect();
    assert!(
        app.update(Message::ClipboardImage {
            tab,
            image: image.clone(),
        })
        .is_empty()
    );
    assert!(matches!(offers.try_recv(), Ok(LocalClipboard::Image(offered)) if offered == image));

    let effects = app.update(Message::Connection {
        tab,
        attempt,
        event: ConnectionEvent::RemoteImage(image.clone().into()),
    });
    assert!(
        matches!(effects.as_slice(), [Effect::WriteClipboardImage(written)] if **written == *image),
        "{effects:?}"
    );
}
