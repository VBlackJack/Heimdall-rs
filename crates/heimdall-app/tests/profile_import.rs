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

//! A Heimdall session document previewed as the C# `ProfileImportService` previews it: a
//! row for each profile with its clash, skip, replace or auto-rename for each, then the rows
//! chosen written in one save.

use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, Mutex};

use heimdall_app::{
    App, AppConfig, Dialog, ImportActions, ImportFile, ImportSummary, Message,
    ProfileImportMessage, ProfileImportPreview, ProfileKind, SessionsMessage, SystemCredentials,
};
use heimdall_core::import::csharp;
use heimdall_core::import::csharp::SkipReason;
use heimdall_core::import::gateways::Reconciliation;
use heimdall_core::import::rdp_file::Conflict;
use heimdall_core::profile::ProfileId;
use heimdall_core::store::ProfileStore;
use heimdall_ssh::AgentSource;
use heimdall_term::GridSize;
use zeroize::Zeroizing;

/// The rename template, as the English locale gives it.
const RENAME: &str = "{name} (Imported {n})";

fn app_with(dir: &Path, credentials: SystemCredentials) -> App {
    App::new(AppConfig {
        profiles_file: dir.join("profiles.toml"),
        known_hosts: dir.join("known_hosts"),
        legacy_dir: None,
        agent: AgentSource::Disabled,
        initial_grid: GridSize { cols: 80, rows: 24 },
        files_start: dir.to_owned(),
        system_credentials: credentials,
    })
}

fn app(dir: &Path) -> App {
    app_with(dir, SystemCredentials::memory())
}

/// The store of `dir` holding the profiles of the C# document `servers`, then what `more`
/// does to it, saved.
fn seed(dir: &Path, servers: &[String], more: impl FnOnce(&mut ProfileStore)) {
    let report = csharp::import(&document(servers), None).expect("document");
    let mut store = ProfileStore::open(dir.join("profiles.toml")).expect("store");
    store.merge(report.profiles);
    store.merge_rdp(report.rdp);
    more(&mut store);
    store.save().expect("save");
}

fn saved(dir: &Path) -> ProfileStore {
    ProfileStore::open(dir.join("profiles.toml")).expect("saved")
}

fn read(app: &mut App, text: &str) {
    app.update(Message::Sessions(SessionsMessage::FileRead(Ok(
        ImportFile {
            name: "export.json".to_owned(),
            text: text.to_owned(),
            settings: None,
            rename: RENAME.to_owned(),
        },
    ))));
}

fn preview(app: &App) -> &ProfileImportPreview {
    match &app.dialog {
        Some(Dialog::ProfileImportPreview(preview)) => preview,
        other => panic!("no preview: {other:?}"),
    }
}

fn done(app: &App) -> &ImportSummary {
    match &app.dialog {
        Some(Dialog::ImportDone(summary)) => summary,
        other => panic!("no report: {other:?}"),
    }
}

fn message(app: &mut App, message: ProfileImportMessage) {
    app.update(Message::ProfileImport(message));
}

fn server(id: &str, name: &str, host: &str, kind: &str) -> String {
    format!(
        r#"{{"id":"{id}","displayName":"{name}","remoteServer":"{host}","connectionType":"{kind}"}}"#
    )
}

fn document(servers: &[String]) -> String {
    format!("[{}]", servers.join(","))
}

#[test]
fn a_clash_on_the_id_on_the_name_and_inside_the_file_is_marked_and_auto_renamed_at_first() {
    let dir = tempfile::tempdir().expect("dir");
    seed(
        dir.path(),
        &[
            server("a1", "Web", "web.lab", "SSH"),
            server("b1", "Db", "db.lab", "SSH"),
        ],
        |_| {},
    );
    let mut app = app(dir.path());
    read(
        &mut app,
        &document(&[
            // The id of "Web", another name.
            server("A1", "Front", "front.lab", "SSH"),
            // The name of "Db", without case, another id.
            server("x1", "DB", "db2.lab", "SSH"),
            // Twice the same name in the file.
            server("y1", "Twin", "t1.lab", "SSH"),
            server("y2", " twin ", "t2.lab", "RDP"),
            // Twice the same id in the file.
            server("z1", "One", "o1.lab", "SSH"),
            server("z1", "Other", "o2.lab", "SSH"),
            // Nothing in common.
            server("n1", "New", "new.lab", "SSH"),
        ]),
    );
    let rows = &preview(&app).rows;
    let clashes: Vec<(&str, Option<&str>, Conflict)> = rows
        .iter()
        .map(|row| {
            (
                row.name.as_str(),
                row.conflict_with.as_deref(),
                row.conflict,
            )
        })
        .collect();
    assert_eq!(
        clashes,
        [
            ("Front", Some("Web"), Conflict::AutoRename),
            ("DB", Some("Db"), Conflict::AutoRename),
            ("Twin", Some("Twin"), Conflict::AutoRename),
            ("twin", Some("twin"), Conflict::AutoRename),
            ("One", Some("One"), Conflict::AutoRename),
            ("Other", Some("Other"), Conflict::AutoRename),
            ("New", None, Conflict::Skip),
        ],
        "in the file's order, the name trimmed, as the C# preview"
    );
    assert!(
        rows.iter().all(|row| row.chosen),
        "every row chosen at first"
    );
    assert_eq!(
        rows.iter().map(|row| row.position).collect::<Vec<_>>(),
        (0..7).map(Some).collect::<Vec<_>>(),
        "rows of both protocols in the file's order"
    );
    assert_eq!(rows[3].kind, Some(ProfileKind::Rdp));
    assert_eq!(rows[0].endpoint, Some(("front.lab".to_owned(), 22)));
    assert_eq!(preview(&app).counts(), (7, 7, 6));
    assert!(
        app.profiles().len() == 2,
        "nothing written before the answer"
    );
}

#[test]
fn a_refused_profile_is_a_row_that_cannot_be_chosen() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path());
    read(
        &mut app,
        &document(&[
            r#"{"id":"h1","displayName":"Hostless","connectionType":"SSH"}"#.to_owned(),
            server("k1", "Kept", "kept.lab", "SSH"),
        ]),
    );
    let rows = &preview(&app).rows;
    assert_eq!(rows[0].name, "Hostless");
    assert_eq!(rows[0].refused, Some(SkipReason::MissingHost));
    assert!(!rows[0].chosen && !rows[0].choosable());
    assert_eq!(rows[0].position, Some(0));
    message(&mut app, ProfileImportMessage::Choose(0));
    message(&mut app, ProfileImportMessage::ChooseAll(true));
    assert!(!preview(&app).rows[0].chosen, "never chosen");
    message(&mut app, ProfileImportMessage::ChooseAll(false));
    assert!(!preview(&app).can_import());
    message(&mut app, ProfileImportMessage::Choose(1));
    assert!(preview(&app).can_import());
}

#[test]
fn apply_to_all_changes_the_rows_in_a_clash_only() {
    let dir = tempfile::tempdir().expect("dir");
    seed(dir.path(), &[server("a1", "Web", "web.lab", "SSH")], |_| {});
    let mut app = app(dir.path());
    read(
        &mut app,
        &document(&[
            server("a1", "Web", "web.lab", "SSH"),
            server("n1", "New", "new.lab", "SSH"),
        ]),
    );
    message(
        &mut app,
        ProfileImportMessage::ConflictAll(Conflict::Replace),
    );
    let conflicts: Vec<Conflict> = preview(&app).rows.iter().map(|row| row.conflict).collect();
    assert_eq!(conflicts, [Conflict::Replace, Conflict::Skip]);
    message(&mut app, ProfileImportMessage::Conflict(0, Conflict::Skip));
    assert_eq!(preview(&app).rows[0].conflict, Conflict::Skip);
}

#[test]
fn skip_leaves_the_saved_profile_as_it_is() {
    let dir = tempfile::tempdir().expect("dir");
    seed(
        dir.path(),
        &[server("a1", "Web", "web.lab", "SSH")],
        |store| {
            store.set_favorite(&ProfileId::new("a1"), true);
        },
    );
    let mut app = app(dir.path());
    read(
        &mut app,
        &document(&[
            r#"{"id":"a1","displayName":"Web","remoteServer":"other.lab","connectionType":"SSH","isFavorite":false}"#.to_owned(),
            server("n1", "New", "new.lab", "SSH"),
        ]),
    );
    message(&mut app, ProfileImportMessage::Conflict(0, Conflict::Skip));
    app.update(Message::ConfirmDialog);
    assert_eq!(
        done(&app).actions,
        Some(ImportActions {
            imported: 1,
            replaced: 0,
            renamed: 0,
            skipped: 1
        })
    );
    let store = saved(dir.path());
    let hosts: Vec<(&str, &str)> = store
        .ssh_profiles()
        .iter()
        .map(|p| (p.id.as_str(), p.host.as_str()))
        .collect();
    assert_eq!(hosts, [("a1", "web.lab"), ("n1", "new.lab")]);
    assert!(store.is_favorite(&ProfileId::new("a1")), "untouched");
}

#[test]
fn replace_writes_the_file_profile_under_the_saved_id_and_leaves_its_saved_password() {
    let dir = tempfile::tempdir().expect("dir");
    seed(
        dir.path(),
        &[r#"{"id":"a1","displayName":"Web","remoteServer":"web.lab","connectionType":"SSH","group":"Old","vaultEntryName":"web-entry","isFavorite":true}"#.to_owned()],
        |store| {
            store.set_favorite(&ProfileId::new("a1"), true);
        },
    );
    // A password saved for the profile, by its identifier.
    let secrets = Arc::new(Mutex::new(HashMap::from([(
        "saved-for-a1".to_owned(),
        Zeroizing::new(b"secret".to_vec()),
    )])));
    let mut app = app_with(dir.path(), SystemCredentials::Memory(Arc::clone(&secrets)));
    // Found by its name: the saved identifier is kept, the file's is not.
    read(
        &mut app,
        &document(&[
            r#"{"id":"f1","displayName":"web","remoteServer":"new-web.lab","connectionType":"SSH","group":"New"}"#.to_owned(),
        ]),
    );
    message(
        &mut app,
        ProfileImportMessage::Conflict(0, Conflict::Replace),
    );
    app.update(Message::ConfirmDialog);
    assert_eq!(
        done(&app).actions,
        Some(ImportActions {
            imported: 0,
            replaced: 1,
            renamed: 0,
            skipped: 0
        })
    );
    let store = saved(dir.path());
    let [web] = store.ssh_profiles() else {
        panic!("one profile: {:?}", store.ssh_profiles());
    };
    assert_eq!(
        (
            web.id.as_str(),
            web.name.as_str(),
            web.host.as_str(),
            web.group.as_deref(),
            web.vault_entry.as_deref()
        ),
        ("a1", "web", "new-web.lab", Some("New"), None),
        "the file's profile in its place, as the C# writes it"
    );
    assert!(
        !store.is_favorite(&ProfileId::new("a1")),
        "the file's star, as the C# writes it"
    );
    let secrets = secrets.lock().expect("secrets");
    assert_eq!(secrets.len(), 1, "no secret written nor removed");
    assert_eq!(
        secrets.get("saved-for-a1").map(|secret| secret.as_slice()),
        Some(b"secret".as_slice())
    );
}

#[test]
fn replace_of_a_profile_of_another_protocol_takes_its_place() {
    let dir = tempfile::tempdir().expect("dir");
    seed(dir.path(), &[server("dc", "DC", "dc.lab", "RDP")], |_| {});
    let mut app = app(dir.path());
    read(&mut app, &document(&[server("dc", "DC", "dc.lab", "SSH")]));
    message(
        &mut app,
        ProfileImportMessage::ConflictAll(Conflict::Replace),
    );
    app.update(Message::ConfirmDialog);
    let store = saved(dir.path());
    assert!(store.rdp_profiles().is_empty(), "the RDP profile is gone");
    assert_eq!(store.ssh_profiles()[0].id.as_str(), "dc");
}

#[test]
fn auto_rename_takes_the_first_free_name_and_a_fresh_id() {
    let dir = tempfile::tempdir().expect("dir");
    seed(
        dir.path(),
        &[
            server("a1", "Web", "web.lab", "SSH"),
            server("a2", "web (Imported 2)", "web2.lab", "SSH"),
        ],
        |_| {},
    );
    let mut app = app(dir.path());
    read(
        &mut app,
        &document(&[
            r#"{"id":"a1","displayName":"Web","remoteServer":"web3.lab","connectionType":"SSH","isFavorite":true,"tags":"blue"}"#.to_owned(),
        ]),
    );
    app.update(Message::ConfirmDialog);
    assert_eq!(
        done(&app).actions,
        Some(ImportActions {
            imported: 1,
            replaced: 0,
            renamed: 1,
            skipped: 0
        })
    );
    let store = saved(dir.path());
    let renamed = &store.ssh_profiles()[2];
    assert_eq!(renamed.name, "Web (Imported 3)", "the C# rule, from 2 up");
    assert!(!["a1", "a2"].contains(&renamed.id.as_str()), "a fresh id");
    assert_eq!(
        store.ssh_profiles()[0].host,
        "web.lab",
        "the saved one kept"
    );
    assert!(store.is_favorite(&renamed.id), "its star follows it");
    assert!(
        !store.is_favorite(&ProfileId::new("a1")),
        "not the saved one's"
    );
    assert!(
        store
            .metadata(&renamed.id)
            .is_some_and(|metadata| !metadata.tags.is_empty()),
        "its tags follow it"
    );
    assert!(store.metadata(&ProfileId::new("a1")).is_none());
}

#[test]
fn two_profiles_of_the_file_with_one_name_are_both_imported_or_the_second_replaces_the_first() {
    let dir = tempfile::tempdir().expect("dir");
    let text = document(&[
        server("t1", "Twin", "t1.lab", "SSH"),
        server("t2", "Twin", "t2.lab", "SSH"),
    ]);
    let mut app = app(dir.path());
    read(&mut app, &text);
    app.update(Message::ConfirmDialog);
    let names: Vec<String> = saved(dir.path())
        .ssh_profiles()
        .iter()
        .map(|p| p.name.clone())
        .collect();
    assert_eq!(names, ["Twin", "Twin (Imported 2)"]);

    let other = tempfile::tempdir().expect("dir");
    let mut app = self::app(other.path());
    read(&mut app, &text);
    message(
        &mut app,
        ProfileImportMessage::Conflict(1, Conflict::Replace),
    );
    app.update(Message::ConfirmDialog);
    assert_eq!(
        done(&app).actions,
        Some(ImportActions {
            imported: 1,
            replaced: 1,
            renamed: 0,
            skipped: 0
        }),
        "counted as the C# counts it"
    );
    let store = saved(other.path());
    let hosts: Vec<(&str, &str)> = store
        .ssh_profiles()
        .iter()
        .map(|p| (p.id.as_str(), p.host.as_str()))
        .collect();
    assert_eq!(hosts, [("t1", "t2.lab")], "the second in the first's place");
}

#[test]
fn rows_not_chosen_are_left_out_and_nothing_chosen_keeps_the_preview() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path());
    read(
        &mut app,
        &document(&[
            server("a", "A", "a.lab", "SSH"),
            server("b", "B", "b.lab", "SSH"),
        ]),
    );
    message(&mut app, ProfileImportMessage::ChooseAll(false));
    app.update(Message::ConfirmDialog);
    assert!(
        preview(&app).rows.iter().all(|row| !row.chosen),
        "still open"
    );
    assert!(!dir.path().join("profiles.toml").exists(), "nothing saved");
    message(&mut app, ProfileImportMessage::Choose(1));
    app.update(Message::ConfirmDialog);
    assert_eq!(done(&app).actions.map(|actions| actions.imported), Some(1));
    let ids: Vec<String> = saved(dir.path())
        .ssh_profiles()
        .iter()
        .map(|p| p.id.as_str().to_owned())
        .collect();
    assert_eq!(ids, ["b"]);
}

#[test]
fn a_cancelled_preview_writes_nothing() {
    let dir = tempfile::tempdir().expect("dir");
    seed(dir.path(), &[server("a1", "Web", "web.lab", "SSH")], |_| {});
    let before = std::fs::read(dir.path().join("profiles.toml")).expect("saved");
    let mut app = app(dir.path());
    read(
        &mut app,
        &document(&[
            server("a1", "Web", "other.lab", "SSH"),
            server("n1", "New", "new.lab", "SSH"),
        ]),
    );
    message(
        &mut app,
        ProfileImportMessage::ConflictAll(Conflict::Replace),
    );
    app.update(Message::DismissDialog);
    assert_eq!(app.dialog, None);
    assert_eq!(
        std::fs::read(dir.path().join("profiles.toml")).expect("saved"),
        before
    );
    assert_eq!(app.profiles().len(), 1);
}

#[test]
fn every_choice_is_written_in_one_save_that_a_new_start_reads() {
    let dir = tempfile::tempdir().expect("dir");
    seed(
        dir.path(),
        &[
            server("s1", "Skip me", "s.lab", "SSH"),
            server("r1", "Replace me", "r.lab", "SSH"),
            server("n1", "Rename me", "n.lab", "SSH"),
        ],
        |_| {},
    );
    let mut app = app(dir.path());
    read(
        &mut app,
        &document(&[
            server("s1", "Skip me", "s2.lab", "SSH"),
            server("r1", "Replace me", "r2.lab", "SSH"),
            server("n1", "Rename me", "n2.lab", "SSH"),
            server("f1", "Fresh", "f.lab", "SSH"),
        ]),
    );
    message(&mut app, ProfileImportMessage::Conflict(0, Conflict::Skip));
    message(
        &mut app,
        ProfileImportMessage::Conflict(1, Conflict::Replace),
    );
    app.update(Message::ConfirmDialog);
    assert_eq!(
        done(&app).actions,
        Some(ImportActions {
            imported: 2,
            replaced: 1,
            renamed: 1,
            skipped: 1
        })
    );
    let in_memory: Vec<(String, String)> = app
        .profiles()
        .iter()
        .map(|p| (p.name.clone(), p.host.clone()))
        .collect();
    assert_eq!(
        in_memory,
        [
            ("Skip me".to_owned(), "s.lab".to_owned()),
            ("Replace me".to_owned(), "r2.lab".to_owned()),
            ("Rename me".to_owned(), "n.lab".to_owned()),
            ("Rename me (Imported 2)".to_owned(), "n2.lab".to_owned()),
            ("Fresh".to_owned(), "f.lab".to_owned()),
        ]
    );
    let restarted = self::app(dir.path());
    assert_eq!(restarted.profiles(), app.profiles(), "what was saved");
}

/// A document with a gateway, a profile going through it and one naming a gateway it does
/// not hold.
const WITH_GATEWAYS: &str = r#"{"servers":[
    {"id":"web","displayName":"Web","remoteServer":"web.internal","connectionType":"SSH","sshGatewayId":"gw"},
    {"id":"dc","displayName":"DC","remoteServer":"dc.internal","connectionType":"RDP","sshGatewayId":"gone"}],
  "gateways":[{"id":"gw","name":"Bastion","host":"bastion.lab","port":22,"username":"ops"}]}"#;

#[test]
fn the_gateways_are_not_rows_and_are_reconciled_only_when_a_profile_is_written() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path());
    read(&mut app, WITH_GATEWAYS);
    let names: Vec<&str> = preview(&app)
        .rows
        .iter()
        .map(|row| row.name.as_str())
        .collect();
    assert_eq!(names, ["Web", "DC"], "a row for each profile only");
    message(&mut app, ProfileImportMessage::Choose(0));
    message(&mut app, ProfileImportMessage::Choose(1));
    message(&mut app, ProfileImportMessage::Choose(1));
    app.update(Message::ConfirmDialog);
    assert_eq!(
        done(&app).gateways,
        Reconciliation {
            created: 1,
            merged: 0,
            orphans: 1
        },
        "as the C#, the file's gateways come with any profile written; the orphan of the          profile written counted"
    );
    assert!(
        saved(dir.path()).ssh_profiles().is_empty(),
        "the profile not chosen left out"
    );

    // Nothing written, the one row chosen skipped for its clash: no gateway either.
    let none = tempfile::tempdir().expect("dir");
    seed(
        none.path(),
        &[server("web", "Web", "web.lab", "SSH")],
        |_| {},
    );
    let mut app = self::app(none.path());
    read(&mut app, WITH_GATEWAYS);
    message(&mut app, ProfileImportMessage::ConflictAll(Conflict::Skip));
    message(&mut app, ProfileImportMessage::Choose(1));
    app.update(Message::ConfirmDialog);
    assert_eq!(done(&app).gateways, Reconciliation::default());
    assert_eq!(done(&app).actions.map(|actions| actions.skipped), Some(1));
    assert!(saved(none.path()).gateways().is_empty());

    let other = tempfile::tempdir().expect("dir");
    let mut app = self::app(other.path());
    read(&mut app, WITH_GATEWAYS);
    app.update(Message::ConfirmDialog);
    assert_eq!(
        done(&app).gateways,
        Reconciliation {
            created: 1,
            merged: 0,
            orphans: 1
        }
    );
    let store = saved(other.path());
    assert_eq!(store.gateways().len(), 1);
    assert_eq!(
        store.rdp_profiles()[0].gateway,
        Some(ProfileId::new("gone")),
        "kept naming it, as since #321"
    );
}
