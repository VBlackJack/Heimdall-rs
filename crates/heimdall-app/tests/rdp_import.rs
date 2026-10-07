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

//! "Import .rdp files", as the C# Heimdall's: files picked or dropped, a preview, a choice
//! for each name taken, then the profiles written.

use std::path::{Path, PathBuf};

use heimdall_app::{App, AppConfig, Dialog, Effect, Message, RdpMessage, RdpNames, RdpOutcome};
use heimdall_core::import::rdp_file::Conflict;
use heimdall_core::profile::{Forwards, ProfileId, RdpOptions, RdpProfile, SshProfile};
use heimdall_core::store::ProfileStore;
use heimdall_ssh::AgentSource;
use heimdall_term::GridSize;

fn app(dir: &Path) -> App {
    let profiles_file = dir.join("profiles.toml");
    let mut store = ProfileStore::open(&profiles_file).expect("store");
    store.merge_rdp([RdpProfile {
        extras: heimdall_core::profile::RdpExtras::default(),
        id: ProfileId::new("dc"),
        name: "DC".to_owned(),
        group: Some("Windows".to_owned()),
        host: "old-dc.lab".to_owned(),
        port: 3389,
        username: None,
        domain: Some("OLD".to_owned()),
        allow_tls_only: false,
        gateway: None,
        redirect_clipboard: true,
        redirect_drives: false,
        options: RdpOptions::default(),
        vault_entry: Some("Win/DC".to_owned()),
        forwards: Forwards::default(),
        follow_defaults: false,
        several_servers: false,
        anti_idle: false,
        auto_reconnect: true,
    }]);
    store.merge([SshProfile {
        id: ProfileId::new("web"),
        name: "web".to_owned(),
        group: None,
        host: "web.lab".to_owned(),
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

fn names() -> RdpNames {
    RdpNames {
        rename: "{name} (Imported {n})".to_owned(),
        fallback: "Imported RDP".to_owned(),
    }
}

fn read(app: &mut App, files: &[(&str, Result<&str, &str>)]) {
    app.update(Message::Rdp(RdpMessage::Read {
        files: files
            .iter()
            .map(|(path, text)| {
                (
                    PathBuf::from(path),
                    text.map(str::to_owned).map_err(str::to_owned),
                )
            })
            .collect(),
        names: names(),
    }));
}

fn shown(app: &App) -> &heimdall_app::RdpPreview {
    match &app.dialog {
        Some(Dialog::RdpPreview(preview)) => preview,
        other => panic!("{other:?}"),
    }
}

#[test]
fn picking_asks_the_window_and_only_dropped_rdp_files_are_read() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path());
    assert!(matches!(
        app.update(Message::Rdp(RdpMessage::Start)).as_slice(),
        [Effect::PickRdpFiles]
    ));
    let effects = app.update(Message::Rdp(RdpMessage::Dropped(vec![
        PathBuf::from("a.RDP"),
        PathBuf::from("notes.txt"),
    ])));
    let [Effect::ReadRdpFiles(paths)] = effects.as_slice() else {
        panic!("{effects:?}");
    };
    assert_eq!(paths, &[PathBuf::from("a.RDP")]);
    assert!(
        app.update(Message::Rdp(RdpMessage::Dropped(vec![PathBuf::from(
            "x.txt"
        )])))
        .is_empty()
    );
}

#[test]
fn the_preview_marks_names_taken_and_what_is_not_imported() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path());
    read(
        &mut app,
        &[
            (
                "DC.rdp",
                Ok("full address:s:new-dc.lab\npassword 51:b:0100\n"),
            ),
            (
                "edge.rdp",
                Ok("full address:s:e.lab\ngatewayhostname:s:gw\ngatewayusagemethod:i:1\n"),
            ),
        ],
    );
    let preview = shown(&app);
    let dc = &preview.rows[0];
    assert_eq!(dc.conflict_with.as_deref(), Some("DC"));
    assert_eq!(dc.conflict, Conflict::AutoRename, "the C# default");
    assert!(dc.chosen && dc.password);
    let edge = &preview.rows[1];
    assert!(
        edge.patch.is_err() && !edge.chosen,
        "a gateway in use is refused"
    );
    assert_eq!(preview.counts(), (1, 2, 1, 1));

    // A file dropped later joins the preview; the same file twice once.
    read(
        &mut app,
        &[
            ("DC.rdp", Ok("full address:s:other\n")),
            ("x/DC.rdp", Ok("full address:s:second-dc.lab\n")),
        ],
    );
    let preview = shown(&app);
    assert_eq!(preview.rows.len(), 3);
    assert_eq!(
        preview.rows[2].conflict_with.as_deref(),
        Some("DC"),
        "the saved profile's name"
    );
    app.update(Message::Rdp(RdpMessage::Choose(1)));
    assert!(
        !shown(&app).rows[1].chosen,
        "a refused file cannot be chosen"
    );
}

#[test]
fn each_choice_for_a_name_taken_is_honoured() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path());
    read(
        &mut app,
        &[
            (
                "DC.rdp",
                Ok("full address:s:new-dc.lab:3390\nusername:s:ops\n"),
            ),
            ("x/DC.rdp", Ok("full address:s:second-dc.lab\n")),
            ("y/DC.rdp", Ok("full address:s:third-dc.lab\n")),
            ("web.rdp", Ok("full address:s:web-desktop.lab\n")),
            (
                "fresh.rdp",
                Ok("full address:s:fresh.lab\npassword 51:b:01\n"),
            ),
        ],
    );
    app.update(Message::Rdp(RdpMessage::Conflict(0, Conflict::Replace)));
    app.update(Message::Rdp(RdpMessage::Conflict(1, Conflict::AutoRename)));
    app.update(Message::Rdp(RdpMessage::Conflict(2, Conflict::Skip)));
    app.update(Message::Rdp(RdpMessage::Conflict(3, Conflict::Replace)));
    app.update(Message::ConfirmDialog);
    assert_eq!(
        app.dialog,
        Some(Dialog::RdpDone(RdpOutcome {
            imported: 3,
            replaced: 1,
            renamed: 2,
            skipped: 1,
            passwords: 1,
        })),
    );
    let store = ProfileStore::open(dir.path().join("profiles.toml")).expect("saved");
    let dc = store
        .rdp_profiles()
        .iter()
        .find(|profile| profile.id.as_str() == "dc")
        .expect("kept");
    assert_eq!(
        (dc.host.as_str(), dc.port, dc.username.as_deref()),
        ("new-dc.lab", 3390, Some("ops")),
        "written onto"
    );
    assert_eq!(
        dc.domain.as_deref(),
        Some("OLD"),
        "what the file does not name stays"
    );
    assert_eq!(dc.vault_entry.as_deref(), Some("Win/DC"));
    let names: Vec<&str> = store
        .rdp_profiles()
        .iter()
        .map(|p| p.name.as_str())
        .collect();
    assert!(names.contains(&"DC (Imported 2)"), "{names:?}");
    assert!(!names.iter().any(|name| name.contains("third")));
    assert!(
        names.contains(&"web (Imported 2)"),
        "an SSH profile named web is not turned into RDP: {names:?}"
    );
    assert!(names.contains(&"fresh"));
    assert_eq!(store.ssh_profiles().len(), 1, "the SSH profile untouched");
}

#[test]
fn choose_all_and_conflict_all_apply_to_every_row_they_can() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path());
    read(
        &mut app,
        &[
            ("DC.rdp", Ok("full address:s:a\n")),
            ("b.rdp", Ok("full address:s:b\n")),
            ("bad.rdp", Ok("username:s:x\n")),
        ],
    );
    app.update(Message::Rdp(RdpMessage::ChooseAll(false)));
    assert!(!shown(&app).can_import());
    app.update(Message::Rdp(RdpMessage::ChooseAll(true)));
    let chosen: Vec<bool> = shown(&app).rows.iter().map(|row| row.chosen).collect();
    assert_eq!(chosen, [true, true, false], "a refused file stays out");
    app.update(Message::Rdp(RdpMessage::ConflictAll(Conflict::Skip)));
    let conflicts: Vec<Conflict> = shown(&app).rows.iter().map(|row| row.conflict).collect();
    assert_eq!(conflicts[0], Conflict::Skip);
    assert_eq!(
        conflicts[1],
        Conflict::AutoRename,
        "not in conflict: unchanged"
    );
}

#[test]
fn files_that_cannot_be_read_are_counted_and_nothing_left_says_so() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path());
    read(
        &mut app,
        &[
            ("a.rdp", Err("a.rdp: denied")),
            ("b.rdp", Err("b.rdp: gone")),
        ],
    );
    assert!(
        matches!(&app.dialog, Some(Dialog::RdpNothing { unreadable }) if unreadable.len() == 2),
        "{:?}",
        app.dialog
    );
}

#[test]
fn a_files_text_is_never_shown_in_a_log() {
    let message = Message::Rdp(RdpMessage::Read {
        files: vec![(
            PathBuf::from("a.rdp"),
            Ok("full address:s:secret-host\n".to_owned()),
        )],
        names: names(),
    });
    assert!(!format!("{message:?}").contains("secret-host"));
}

#[test]
fn two_new_files_with_one_name_are_both_in_conflict_and_the_second_is_renamed() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path());
    read(
        &mut app,
        &[
            ("a/b.rdp", Ok("full address:s:one\n")),
            ("c/b.rdp", Ok("full address:s:two\n")),
        ],
    );
    let marked: Vec<Option<&str>> = shown(&app)
        .rows
        .iter()
        .map(|row| row.conflict_with.as_deref())
        .collect();
    assert_eq!(marked, [Some("b"), Some("b")], "the batch's own duplicate");
    app.update(Message::ConfirmDialog);
    let store = ProfileStore::open(dir.path().join("profiles.toml")).expect("saved");
    let mut names: Vec<&str> = store
        .rdp_profiles()
        .iter()
        .map(|p| p.name.as_str())
        .collect();
    names.sort_unstable();
    assert_eq!(names, ["DC", "b", "b (Imported 2)"]);
}
