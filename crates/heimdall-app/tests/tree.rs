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

//! The profile tree's actions, as the C# Heimdall's menu offers them: connect with the
//! profile's own protocol, duplicate, delete and copy, whatever the protocol.

use std::path::Path;

use heimdall_app::{
    App, AppConfig, Dialog, Effect, Message, ProfileCopy, ProfileKind, ProfileSummary,
    SystemCredentials,
};
use heimdall_core::profile::{
    LocalApproval, LocalCommand, LocalProfile, ProfileId, RdpProfile, SshProfile, TelnetProfile,
    VncProfile, WinRmProfile,
};
use heimdall_core::store::ProfileStore;
use heimdall_ssh::{AgentSource, PasswordQuestion, Secret};
use heimdall_term::GridSize;

const SUFFIX: &str = " (copy)";

fn id(value: &str) -> ProfileId {
    ProfileId::new(value)
}

/// One profile of each protocol.
fn app(dir: &Path, system: &SystemCredentials) -> App {
    let profiles_file = dir.join("profiles.toml");
    let mut store = ProfileStore::open(&profiles_file).expect("store");
    store.merge([SshProfile {
        id: id("ssh"),
        name: "web".to_owned(),
        group: Some("Prod".to_owned()),
        host: "web.lab".to_owned(),
        port: 2222,
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
    store.merge_rdp([RdpProfile {
        extras: heimdall_core::profile::RdpExtras::default(),
        id: id("rdp"),
        name: "dc".to_owned(),
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
    store.merge_telnet([TelnetProfile {
        id: id("telnet"),
        name: "switch".to_owned(),
        group: None,
        host: "sw.lab".to_owned(),
        port: 23,
        session_logging: None,
    }]);
    store.merge_vnc([VncProfile {
        id: id("vnc"),
        name: "kiosk".to_owned(),
        group: None,
        host: "kiosk.lab".to_owned(),
        port: 5900,
        view_only: false,
        allow_no_password: true,
        username: None,
        vault_entry: None,
    }]);
    // The default shell.
    let command = LocalCommand::default();
    store.merge_local([LocalProfile {
        id: id("local"),
        name: "shell".to_owned(),
        group: None,
        command: command.clone(),
        approved: None,
        session_logging: None,
    }]);
    store.approve_local(
        &id("local"),
        LocalApproval {
            command,
            program_path: dir.join("sh"),
        },
    );
    store.merge_winrm([WinRmProfile {
        id: id("winrm"),
        name: "ps".to_owned(),
        group: None,
        host: "ps.lab".to_owned(),
        port: 5985,
        use_ssl: false,
        skip_certificate_check: false,
        username: None,
        gateway: None,
    }]);
    store.save().expect("save");
    App::new(AppConfig {
        profiles_file,
        known_hosts: dir.join("known_hosts"),
        legacy_dir: None,
        agent: AgentSource::Disabled,
        initial_grid: GridSize { cols: 80, rows: 24 },
        files_start: dir.to_owned(),
        system_credentials: system.clone(),
    })
}

/// What connecting to `profile` asks the UI layer for, by variant name.
fn connect(app: &mut App, profile: &str) -> String {
    let effects = app.update(Message::ConnectProfile(id(profile)));
    match effects.as_slice() {
        [Effect::Connect { .. }] => "Connect".to_owned(),
        [Effect::ConnectRdp { .. }] => "ConnectRdp".to_owned(),
        [Effect::ConnectTelnet { .. }] => "ConnectTelnet".to_owned(),
        [Effect::ConnectVnc { .. }] => "ConnectVnc".to_owned(),
        [Effect::ConnectLocal { .. }] => "ConnectLocal".to_owned(),
        [Effect::ConnectWinRm { .. }] => "ConnectWinRm".to_owned(),
        [] if matches!(app.dialog, Some(Dialog::ConfirmLocalCommand(_))) => {
            "ConfirmLocalCommand".to_owned()
        }
        other => format!("{other:?}"),
    }
}

#[test]
fn connecting_uses_the_profiles_own_protocol() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path(), &SystemCredentials::memory());
    assert_eq!(connect(&mut app, "ssh"), "Connect");
    assert_eq!(connect(&mut app, "rdp"), "ConnectRdp");
    assert_eq!(connect(&mut app, "telnet"), "ConnectTelnet");
    assert_eq!(connect(&mut app, "vnc"), "ConnectVnc");
    // The server probed, then a PowerShell entering the session.
    assert_eq!(connect(&mut app, "winrm"), "ConnectWinRm");
    assert_eq!(
        app.selected_profile,
        Some(id("winrm")),
        "connecting selects"
    );
    assert_eq!(connect(&mut app, "gone"), "[]");
}

#[test]
fn selecting_keeps_only_a_profile_that_exists() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path(), &SystemCredentials::memory());
    app.update(Message::SelectProfile(id("rdp")));
    assert_eq!(app.selected_profile, Some(id("rdp")));
    app.update(Message::SelectProfile(id("gone")));
    assert_eq!(app.selected_profile, None);
}

#[test]
fn every_protocol_is_listed_and_all_but_a_local_program_open_the_editor() {
    let dir = tempfile::tempdir().expect("dir");
    let app = app(dir.path(), &SystemCredentials::memory());
    let mut kinds: Vec<ProfileKind> = app
        .profile_summaries()
        .iter()
        .map(|profile| profile.kind)
        .collect();
    kinds.sort_by_key(|kind| format!("{kind:?}"));
    assert_eq!(
        kinds,
        [
            ProfileKind::Local,
            ProfileKind::Rdp,
            ProfileKind::Ssh,
            ProfileKind::Telnet,
            ProfileKind::Vnc,
            ProfileKind::WinRm
        ]
    );
    for editable in ["ssh", "rdp", "vnc", "winrm", "telnet"] {
        assert!(app.can_edit(&id(editable)), "{editable}");
    }
    assert!(
        !app.can_edit(&id("local")),
        "its command has an editor of its own to come"
    );
}

fn duplicate(app: &mut App, profile: &str) {
    app.update(Message::DuplicateProfile {
        id: id(profile),
        suffix: SUFFIX.to_owned(),
    });
}

fn names(app: &App) -> Vec<String> {
    let mut names: Vec<String> = app
        .profile_summaries()
        .into_iter()
        .map(|profile| profile.name)
        .collect();
    names.sort();
    names
}

#[test]
fn a_duplicate_is_named_and_saved_as_in_the_csharp_app() {
    let dir = tempfile::tempdir().expect("dir");
    let system = SystemCredentials::memory();
    let mut app = app(dir.path(), &system);
    duplicate(&mut app, "ssh");
    duplicate(&mut app, "ssh");
    duplicate(&mut app, "rdp");
    let web: Vec<String> = names(&app)
        .into_iter()
        .filter(|name| name.starts_with("web"))
        .collect();
    assert_eq!(web, ["web", "web (copy)", "web (copy) 2"]);
    let copy = app
        .profile_summaries()
        .into_iter()
        .find(|profile| profile.name == "dc (copy)")
        .expect("rdp copy");
    assert_eq!(copy.kind, ProfileKind::Rdp);
    assert_ne!(copy.id, id("rdp"));
    assert_eq!(
        app.selected_profile,
        Some(copy.id.clone()),
        "the copy is selected"
    );
    assert_eq!(copy.group, None);
    assert_eq!(copy.endpoint, Some(("dc.lab".to_owned(), 3389)));

    // Saved: another start lists them.
    let reopened = self::app(dir.path(), &system);
    assert_eq!(names(&reopened).len(), names(&app).len());
}

#[test]
fn a_duplicated_local_profile_must_be_approved_again() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path(), &SystemCredentials::memory());
    duplicate(&mut app, "local");
    let copy = app
        .local_profiles()
        .iter()
        .find(|profile| profile.name == "shell (copy)")
        .expect("copy");
    assert!(copy.approved.is_none());
    assert!(app.local_profiles().iter().any(|p| p.approved.is_some()));
}

/// What a new connection to `profile` is answered with by itself.
fn saved_answer(app: &mut App, profile: &str, host: &str, port: u16) -> Option<String> {
    let effects = app.update(Message::ConnectProfile(id(profile)));
    let (tab, attempt) = match effects.as_slice() {
        [Effect::Connect { tab, attempt, .. } | Effect::ConnectRdp { tab, attempt, .. }] => {
            (*tab, *attempt)
        }
        other => panic!("{other:?}"),
    };
    let question = heimdall_app::QuestionId::fresh();
    let effects = app.update(Message::Connection {
        tab,
        attempt,
        event: heimdall_app::ConnectionEvent::Question {
            question,
            kind: heimdall_app::QuestionKind::Password(PasswordQuestion {
                host: host.to_owned(),
                port,
                username: "admin".to_owned(),
                attempt: 1,
            }),
        },
    });
    match effects.as_slice() {
        [
            Effect::Answer {
                answer: Some(heimdall_app::Answer::Secret(secret)),
                ..
            },
        ] => Some(secret.expose().to_owned()),
        _ => None,
    }
}

#[test]
fn a_duplicate_takes_the_saved_password_along_except_for_rdp() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path(), &SystemCredentials::memory());
    app.update(Message::EditProfile(id("ssh")));
    app.update(Message::SaveProfile {
        password: Some(Secret::new("pw".to_owned())),
        passphrase: None,
    });
    duplicate(&mut app, "ssh");
    let copy = app.selected_profile.clone().expect("copy");
    assert_eq!(
        saved_answer(&mut app, copy.as_str(), "web.lab", 2222).as_deref(),
        Some("pw")
    );
}

#[test]
fn any_profile_is_deleted_from_its_menu_after_confirming() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path(), &SystemCredentials::memory());
    app.update(Message::RequestDeleteProfile(id("rdp")));
    let Some(Dialog::ConfirmDeleteProfile { name, .. }) = &app.dialog else {
        panic!("{:?}", app.dialog);
    };
    assert_eq!(name, "dc");
    assert!(
        app.profile_summary(&id("rdp")).is_some(),
        "not before confirming"
    );
    app.update(Message::ConfirmDialog);
    assert!(app.profile_summary(&id("rdp")).is_none());
    app.update(Message::RequestDeleteProfile(id("gone")));
    assert!(app.dialog.is_none());
}

fn copied(app: &mut App, profile: &str, what: ProfileCopy) -> Option<String> {
    match app
        .update(Message::CopyProfile {
            id: id(profile),
            what,
        })
        .as_slice()
    {
        [Effect::WriteClipboard(text)] => Some(text.clone()),
        [] => None,
        other => panic!("{other:?}"),
    }
}

#[test]
fn the_copy_entries_put_what_they_say_on_the_clipboard() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path(), &SystemCredentials::memory());
    assert_eq!(
        copied(&mut app, "ssh", ProfileCopy::Hostname).as_deref(),
        Some("web.lab")
    );
    assert_eq!(
        copied(&mut app, "ssh", ProfileCopy::Username).as_deref(),
        Some("admin")
    );
    assert_eq!(
        copied(&mut app, "ssh", ProfileCopy::Address).as_deref(),
        Some("web.lab:2222")
    );
    assert_eq!(
        copied(&mut app, "ssh", ProfileCopy::SshCommand).as_deref(),
        Some("ssh admin@web.lab -p 2222")
    );
    assert_eq!(
        copied(&mut app, "winrm", ProfileCopy::Username),
        None,
        "no account"
    );
    assert_eq!(
        copied(&mut app, "local", ProfileCopy::Hostname),
        None,
        "no host"
    );
}

#[test]
fn a_duplicated_rdp_profile_leaves_its_password_behind_as_in_the_csharp_app() {
    use heimdall_core::credentials::{
        CredentialProtocol, Endpoint, SavedPassword, encode, password_entry,
    };

    let dir = tempfile::tempdir().expect("dir");
    let system = SystemCredentials::memory();
    let SystemCredentials::Memory(entries) = &system else {
        unreachable!()
    };
    let saved = SavedPassword {
        endpoint: Endpoint {
            protocol: CredentialProtocol::Rdp,
            host: "dc.lab".to_owned(),
            port: 3389,
            username: Some("admin".to_owned()),
        },
        password: zeroize::Zeroizing::new("pw".to_owned()),
    };
    entries.lock().expect("entries").insert(
        password_entry(&id("rdp")),
        zeroize::Zeroizing::new(encode(&saved).to_vec()),
    );
    let mut app = app(dir.path(), &system);
    duplicate(&mut app, "rdp");
    let copy = app.selected_profile.clone().expect("copy");
    assert!(
        !entries
            .lock()
            .expect("entries")
            .contains_key(&password_entry(&copy))
    );
    assert_eq!(entries.lock().expect("entries").len(), 1);
}

#[test]
fn the_search_finds_every_word_each_in_name_host_folder_account_or_protocol() {
    let dir = tempfile::tempdir().expect("dir");
    let app = app(dir.path(), &SystemCredentials::memory());
    let web = app.profile_summary(&id("ssh")).expect("web");
    for found in [
        "",
        "  ",
        "web",
        "WEB",
        "eb.la",
        "prod",
        "admin",
        "ssh",
        " Web ",
        // As the C#: each word on its own, in any field, in any order.
        "web prod",
        "prod web",
        "lab prod",
        "web  admin ssh",
        "web web",
        // Accents folded on the search side.
        "wéb",
        "PRÔD",
    ] {
        assert!(web.matches(found), "{found:?} finds it");
    }
    // A word is never matched across two fields; every word must be found.
    for missed in ["webprod", "web dc", "2222", "dc", "rdp", "web rdp"] {
        assert!(!web.matches(missed), "{missed:?} does not");
    }
    let dc = app.profile_summary(&id("rdp")).expect("dc");
    assert!(dc.matches("rdp"), "by the protocol the tree shows");
    assert!(!dc.matches("prod"), "no folder");
    let found: Vec<_> = app
        .profile_summaries()
        .into_iter()
        .filter(|profile| profile.matches("lab"))
        .map(|profile| profile.id)
        .collect();
    assert!(
        found.contains(&id("telnet")) && found.contains(&id("vnc")),
        "{found:?}"
    );
}

#[test]
fn the_search_folds_the_accents_of_the_profile_too() {
    let summary = ProfileSummary {
        id: ProfileId::new("r"),
        name: "Contrôleur Élysée".to_owned(),
        group: Some("Réseau".to_owned()),
        kind: ProfileKind::Ssh,
        endpoint: Some(("dc.lab".to_owned(), 22)),
        username: Some("hélène".to_owned()),
        gateway: None,
        favorite: false,
        metadata: heimdall_core::metadata::ProfileMetadata::default(),
    };
    for found in [
        "reseau",
        "RESEAU",
        "réseau",
        "controleur elysee",
        "helene",
        "Hélène reseau",
    ] {
        assert!(summary.matches(found), "{found:?} finds it");
    }
    assert!(!summary.matches("reseaux"), "folded, not loosened");
}

/// The profiles the tree shows, by identifier.
fn shown(app: &App) -> Vec<String> {
    app.tree_rows("")
        .into_iter()
        .filter_map(|row| match row {
            heimdall_app::TreeRow::Profile { profile, .. } => Some(profile.id.as_str().to_owned()),
            heimdall_app::TreeRow::Folder { .. } => None,
        })
        .collect()
}

#[test]
fn favorites_are_marked_from_the_menu_one_or_several_and_filtered_as_the_csharp() {
    use heimdall_app::{FilterMessage, ProfileMenuMessage, SelectionMessage};

    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path(), &SystemCredentials::memory());
    app.update(Message::ProfileMenu(ProfileMenuMessage::Favorite {
        id: id("ssh"),
        favorite: true,
    }));
    assert!(app.is_favorite(&id("ssh")));
    assert!(
        app.profile_summary(&id("ssh"))
            .is_some_and(|profile| profile.favorite),
        "the tree's row carries it"
    );
    // Saved: a new run sees it.
    let reread = self::app(dir.path(), &SystemCredentials::memory());
    assert!(reread.is_favorite(&id("ssh")));

    app.update(Message::Filter(FilterMessage::Favorites));
    assert!(app.tree_filter().is_active());
    assert_eq!(shown(&app), ["ssh"]);

    // Several at once: added while one is not, removed once all are.
    app.update(Message::Filter(FilterMessage::Favorites));
    app.update(Message::SelectProfile(id("ssh")));
    app.update(Message::Selection(SelectionMessage::Toggle(id("rdp"))));
    let selected = app.selected_profiles();
    assert!(!app.all_favorites(&selected));
    app.update(Message::Selection(SelectionMessage::Favorite(true)));
    assert!(app.all_favorites(&selected));
    app.update(Message::Selection(SelectionMessage::Favorite(false)));
    assert!(!app.is_favorite(&id("ssh")) && !app.is_favorite(&id("rdp")));
}

#[test]
fn the_form_marks_a_favorite_and_shows_one() {
    use heimdall_app::profile_draft::ProfileToggle;

    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path(), &SystemCredentials::memory());
    app.update(Message::EditProfile(id("vnc")));
    let Some(Dialog::EditProfile { draft, .. }) = &app.dialog else {
        panic!("{:?}", app.dialog);
    };
    assert!(!draft.is_on(ProfileToggle::Favorite));
    app.update(Message::ProfileToggle {
        toggle: ProfileToggle::Favorite,
        on: true,
    });
    app.update(Message::SaveProfile {
        password: None,
        passphrase: None,
    });
    assert!(app.dialog.is_none(), "{:?}", app.dialog);
    assert!(app.is_favorite(&id("vnc")));
    app.update(Message::EditProfile(id("vnc")));
    let Some(Dialog::EditProfile { draft, .. }) = &app.dialog else {
        panic!("{:?}", app.dialog);
    };
    assert!(draft.is_on(ProfileToggle::Favorite), "shown ticked");
}

#[test]
fn the_port_and_the_account_of_several_profiles_are_set_at_once_as_the_csharp() {
    use heimdall_app::{BulkField, BulkRefusal, Notice, SelectionMessage};

    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path(), &SystemCredentials::memory());
    let select = |app: &mut App, ids: &[&str]| {
        app.update(Message::SelectProfile(id(ids[0])));
        for other in &ids[1..] {
            app.update(Message::Selection(SelectionMessage::Toggle(id(other))));
        }
    };
    let bulk = |app: &mut App, message| app.update(Message::Selection(message));
    let port = |app: &App, profile: &str| {
        app.profile_summary(&id(profile))
            .and_then(|profile| profile.endpoint)
            .map(|(_, port)| port)
    };

    // A local shell has no port: left out, the others' ports differ.
    select(&mut app, &["ssh", "rdp", "local"]);
    bulk(&mut app, SelectionMessage::Edit(BulkField::Port));
    let Some(Dialog::BulkEdit {
        ids, value, mixed, ..
    }) = &app.dialog
    else {
        panic!("{:?}", app.dialog);
    };
    let mut ids = ids.clone();
    ids.sort();
    assert_eq!(ids, [id("rdp"), id("ssh")]);
    assert!(value.is_empty() && *mixed, "mixed values");
    bulk(&mut app, SelectionMessage::BulkEdited("0".to_owned()));
    app.update(Message::ConfirmDialog);
    assert!(
        matches!(
            app.dialog,
            Some(Dialog::BulkEdit {
                refused: Some(BulkRefusal::Port),
                ..
            })
        ),
        "between 1 and 65535"
    );
    bulk(&mut app, SelectionMessage::BulkEdited("2200".to_owned()));
    app.update(Message::ConfirmDialog);
    assert!(app.dialog.is_none());
    assert_eq!(
        (port(&app, "ssh"), port(&app, "rdp")),
        (Some(2200), Some(2200))
    );
    assert_eq!(app.notice(), Some(&Notice::BulkPortUpdated(2)));
    // The same again changes nothing, and says so.
    bulk(&mut app, SelectionMessage::Edit(BulkField::Port));
    let Some(Dialog::BulkEdit { value, mixed, .. }) = &app.dialog else {
        panic!("{:?}", app.dialog);
    };
    assert_eq!(
        (value.as_str(), *mixed),
        ("2200", false),
        "shared: written in"
    );
    app.update(Message::ConfirmDialog);
    assert_eq!(app.notice(), Some(&Notice::BulkPortUnchanged));

    // VNC names no account: the menu counts the others, the dialog sets theirs.
    select(&mut app, &["ssh", "vnc", "winrm"]);
    let selected = app.selected_profiles();
    assert_eq!(app.bulk_targets(&selected, BulkField::Username), 2);
    bulk(&mut app, SelectionMessage::Edit(BulkField::Username));
    bulk(
        &mut app,
        SelectionMessage::BulkEdited("ops\tteam".to_owned()),
    );
    app.update(Message::ConfirmDialog);
    assert!(matches!(
        app.dialog,
        Some(Dialog::BulkEdit {
            refused: Some(BulkRefusal::Username),
            ..
        })
    ));
    bulk(&mut app, SelectionMessage::BulkEdited("ops".to_owned()));
    app.update(Message::ConfirmDialog);
    for profile in ["ssh", "winrm"] {
        assert_eq!(
            app.profile_summary(&id(profile))
                .and_then(|profile| profile.username)
                .as_deref(),
            Some("ops"),
            "{profile}"
        );
    }
    assert_eq!(app.notice(), Some(&Notice::BulkUsernameUpdated(2)));

    // One profile alone is not a bulk edit.
    app.update(Message::SelectProfile(id("ssh")));
    bulk(&mut app, SelectionMessage::Edit(BulkField::Port));
    assert!(app.dialog.is_none());
}
