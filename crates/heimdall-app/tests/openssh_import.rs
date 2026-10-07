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

//! "Import OpenSSH config", as the C# Heimdall's: the file picked, a preview to choose from,
//! then the servers imported through their gateway chains.

use std::path::Path;

use heimdall_app::{
    App, AppConfig, Dialog, Effect, Message, ProfileKind, SessionsCounts, SessionsMessage,
    SessionsSource,
};
use heimdall_core::import::openssh::Status;
use heimdall_core::profile::{Forwards, ProfileId, SshProfile};
use heimdall_core::store::ProfileStore;
use heimdall_ssh::AgentSource;
use heimdall_term::GridSize;

const CONFIG: &str = "Host web\n    HostName web.lab\n    User ops\n    IdentityFile /nowhere/key\n    ProxyJump alice@edge.lab\nHost db\n    HostName db.lab\n    UnknownThing yes\n";

fn app(dir: &Path) -> App {
    let profiles_file = dir.join("profiles.toml");
    let mut store = ProfileStore::open(&profiles_file).expect("store");
    store.merge([SshProfile {
        id: ProfileId::new("mine"),
        name: "DB".to_owned(),
        group: None,
        host: "old.lab".to_owned(),
        port: 22,
        username: None,
        key_path: None,
        gateway: None,
        vault_entry: None,
        forwards: Forwards::default(),
        post_connect: heimdall_core::post_connect::PostConnect::default(),
        forward_agent: false,
        compression: false,
        sftp: false,
        legacy_algorithms: false,
        session_logging: None,
        ssh_mode: heimdall_core::profile::SshMode::Embedded,
        x11_forwarding: false,
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

fn read(app: &mut App, text: &str) {
    app.update(Message::Sessions(SessionsMessage::Read(
        Ok(text.to_owned()),
    )));
}

#[test]
fn the_menu_entry_asks_for_the_file() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path());
    let effects = app.update(Message::Sessions(SessionsMessage::Start));
    assert!(
        matches!(effects.as_slice(), [Effect::PickOpenSshConfig]),
        "{effects:?}"
    );
}

#[test]
fn the_preview_chooses_the_new_servers_and_says_what_was_left_out() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path());
    read(&mut app, CONFIG);
    let Some(Dialog::SessionsPreview(preview)) = &app.dialog else {
        panic!("{:?}", app.dialog);
    };
    let rows: Vec<(&str, Status, bool)> = preview
        .rows
        .iter()
        .map(|row| {
            (
                row.assessment.candidate.alias.as_str(),
                row.assessment.status,
                row.chosen,
            )
        })
        .collect();
    assert_eq!(
        rows,
        [("web", Status::New, true), ("db", Status::Duplicate, false)],
        "a profile named DB already"
    );
    assert_eq!(preview.counts(), (2, 1, 1, 0));
    assert_eq!(preview.diagnostics.len(), 1, "UnknownThing");
    assert!(!preview.all_chosen());

    app.update(Message::Sessions(SessionsMessage::ChooseAll(false)));
    let Some(Dialog::SessionsPreview(preview)) = &app.dialog else {
        panic!();
    };
    assert!(!preview.can_import(), "nothing chosen, nothing to do");
    app.update(Message::Sessions(SessionsMessage::Choose(0)));
    let Some(Dialog::SessionsPreview(preview)) = &app.dialog else {
        panic!();
    };
    assert!(preview.rows[0].chosen && !preview.rows[1].chosen);
    app.update(Message::Sessions(SessionsMessage::ChooseAll(true)));
    let Some(Dialog::SessionsPreview(preview)) = &app.dialog else {
        panic!();
    };
    assert!(preview.all_chosen());
}

#[test]
fn the_chosen_servers_are_imported_through_their_gateway() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path());
    read(&mut app, CONFIG);
    // "db" chosen too: a profile has its name, so it is left out all the same.
    app.update(Message::Sessions(SessionsMessage::ChooseAll(true)));
    app.update(Message::ConfirmDialog);
    assert!(
        matches!(
            app.dialog,
            Some(Dialog::SessionsDone {
                source: SessionsSource::OpenSsh,
                counts: SessionsCounts {
                    imported: 1,
                    gateways: 1,
                    duplicates: 1,
                    invalid: 0,
                    warnings: 1,
                },
            })
        ),
        "{:?}",
        app.dialog
    );
    let web = app
        .profile_summaries()
        .into_iter()
        .find(|profile| profile.name == "web")
        .expect("imported");
    assert_eq!(web.kind, ProfileKind::Ssh);
    assert_eq!(web.endpoint, Some(("web.lab".to_owned(), 22)));
    assert!(web.gateway.is_some(), "through edge.lab");
    let reopened = ProfileStore::open(dir.path().join("profiles.toml")).expect("saved");
    assert_eq!(reopened.gateways().len(), 1);
    assert_eq!(reopened.gateways()[0].host, "edge.lab");
    assert_eq!(
        reopened
            .ssh_profiles()
            .iter()
            .find(|profile| profile.name == "web")
            .and_then(|profile| profile.gateway.clone()),
        Some(reopened.gateways()[0].id.clone())
    );
}

#[test]
fn an_unreadable_or_empty_file_is_said() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path());
    app.update(Message::Sessions(SessionsMessage::Read(Err(
        "config: denied".to_owned(),
    ))));
    assert!(matches!(
        &app.dialog,
        Some(Dialog::SessionsUnreadable { detail, source: SessionsSource::OpenSsh }) if detail == "config: denied"
    ));
    for text in ["", "# nothing here\n\n"] {
        read(&mut app, text);
        assert!(
            matches!(
                app.dialog,
                Some(Dialog::SessionsEmpty {
                    source: SessionsSource::OpenSsh
                })
            ),
            "{text:?}"
        );
    }
}

#[test]
fn the_files_text_is_never_shown_in_a_log() {
    let message = Message::Sessions(SessionsMessage::Read(Ok("Host secret-name\n".to_owned())));
    assert!(!format!("{message:?}").contains("secret-name"));
}

#[test]
fn a_file_that_only_says_what_it_left_out_is_previewed() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path());
    read(&mut app, "Include other\n");
    let Some(Dialog::SessionsPreview(preview)) = &app.dialog else {
        panic!("{:?}", app.dialog);
    };
    assert!(preview.rows.is_empty() && preview.diagnostics.len() == 1);
}
