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
//! text is offered to the server when its desktop comes up or comes back into view.

use std::path::Path;

use heimdall_app::{App, AppConfig, AttemptId, ConnectionEvent, Effect, Message, TabId};
use heimdall_core::profile::{ProfileId, RdpProfile};
use heimdall_core::store::ProfileStore;
use heimdall_rdp::Framebuffer;
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
) -> (mpsc::UnboundedReceiver<Zeroizing<String>>, Vec<Effect>) {
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
    matches!(effects, [Effect::ReadClipboard { tab }] if *tab == wanted)
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
    assert_eq!(offers.try_recv().expect("offered").as_str(), "copied here");
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
