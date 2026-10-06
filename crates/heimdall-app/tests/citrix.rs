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

//! Citrix profiles: launched outside Heimdall, with no tab, the status bar saying how it
//! went; created, edited and duplicated as the other protocols' profiles.

use std::path::{Path, PathBuf};

use heimdall_app::citrix::{CitrixLaunch, CitrixRefusal};
use heimdall_app::profile_draft::{DraftProtocol, ProfileField, ProfileToggle};
use heimdall_app::{App, AppConfig, Dialog, Effect, Message, Notice, ProfileKind};
use heimdall_core::profile::{CitrixProfile, ProfileId};
use heimdall_core::store::ProfileStore;
use heimdall_ssh::AgentSource;
use heimdall_term::GridSize;

fn outlook() -> CitrixProfile {
    CitrixProfile {
        id: ProfileId::new("outlook"),
        name: "Outlook".to_owned(),
        group: Some("Apps".to_owned()),
        store_front_url: Some("https://store.lab/Citrix/Store".to_owned()),
        app_name: Some("Outlook 365".to_owned()),
        ica_file: None,
        seamless: true,
        sso: false,
    }
}

fn profiles_file(dir: &Path) -> PathBuf {
    dir.join("profiles.toml")
}

fn app(dir: &Path, profiles: &[CitrixProfile]) -> App {
    let mut store = ProfileStore::open(profiles_file(dir)).expect("store");
    store.merge_citrix(profiles.iter().cloned());
    store.save().expect("save");
    App::new(AppConfig {
        profiles_file: profiles_file(dir),
        known_hosts: dir.join("known_hosts"),
        legacy_dir: None,
        agent: AgentSource::Disabled,
        initial_grid: GridSize { cols: 80, rows: 24 },
        files_start: dir.to_owned(),
        system_credentials: heimdall_app::SystemCredentials::memory(),
    })
}

fn saved(dir: &Path) -> Vec<CitrixProfile> {
    ProfileStore::open(profiles_file(dir))
        .expect("readable")
        .citrix_profiles()
        .to_vec()
}

fn type_in(app: &mut App, field: ProfileField, value: &str) {
    app.update(Message::ProfileField {
        field,
        value: value.to_owned(),
    });
}

#[test]
fn a_citrix_application_launches_outside_heimdall_and_says_so() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path(), &[outlook()]);
    // Opened from the tree, as any profile.
    let effects = app.update(Message::ConnectProfile(ProfileId::new("outlook")));
    let [
        Effect::LaunchCitrix {
            name,
            launch:
                CitrixLaunch::StoreFront {
                    app: application,
                    url,
                    sso,
                },
        },
    ] = effects.as_slice()
    else {
        panic!("{effects:?}");
    };
    assert_eq!(
        (name.as_str(), application.as_str(), url.as_str(), *sso),
        (
            "Outlook",
            "Outlook 365",
            "https://store.lab/Citrix/Store",
            false
        )
    );
    assert!(app.tabs.is_empty(), "no tab: its window is Citrix's own");
    assert_eq!(app.notice(), Some(&Notice::CitrixLaunching));

    let effects = app.update(Message::CitrixLaunched {
        name: "Outlook".to_owned(),
        result: Ok(()),
    });
    assert!(effects.is_empty());
    assert_eq!(
        app.notice(),
        Some(&Notice::CitrixLaunched("Outlook".to_owned()))
    );

    app.update(Message::CitrixLaunched {
        name: "Outlook".to_owned(),
        result: Err(CitrixRefusal::WorkspaceNotFound),
    });
    assert_eq!(
        app.notice(),
        Some(&Notice::CitrixRefused(CitrixRefusal::WorkspaceNotFound))
    );
}

#[test]
fn a_citrix_profile_that_cannot_launch_is_refused_with_the_csharp_reason() {
    let refused = |profile: CitrixProfile| {
        let dir = tempfile::tempdir().expect("dir");
        let mut app = app(dir.path(), std::slice::from_ref(&profile));
        let effects = app.update(Message::OpenCitrix(profile.id));
        assert!(effects.is_empty(), "{effects:?}");
        assert!(app.tabs.is_empty());
        app.notice().cloned()
    };
    assert_eq!(
        refused(CitrixProfile {
            store_front_url: Some("ftp://store.lab/".to_owned()),
            ..outlook()
        }),
        Some(Notice::CitrixRefused(CitrixRefusal::InvalidStoreFront))
    );
    assert_eq!(
        refused(CitrixProfile {
            store_front_url: Some("https://admin:secret@store.lab/".to_owned()),
            ..outlook()
        }),
        Some(Notice::CitrixRefused(CitrixRefusal::StoreFrontCredentials))
    );
    assert_eq!(
        refused(CitrixProfile {
            store_front_url: None,
            app_name: None,
            ..outlook()
        }),
        Some(Notice::CitrixRefused(CitrixRefusal::NotConfigured))
    );
}

#[test]
fn the_tree_lists_a_citrix_profile_with_no_server() {
    let dir = tempfile::tempdir().expect("dir");
    let app = app(dir.path(), &[outlook()]);
    let summary = app
        .profile_summary(&ProfileId::new("outlook"))
        .expect("listed");
    assert_eq!(summary.kind, ProfileKind::Citrix);
    assert_eq!(summary.kind.label(), "Citrix");
    assert_eq!(
        (&summary.endpoint, &summary.username, &summary.gateway),
        (&None, &None, &None)
    );
    assert_eq!(summary.group.as_deref(), Some("Apps"));
    assert!(summary.matches("citrix apps"));
    assert!(
        app.connects_in_bulk(&summary.id),
        "Connect selected opens it"
    );
}

#[test]
fn a_citrix_profile_is_created_from_the_form_as_the_csharp_dialog_starts_one() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path(), &[]);
    app.update(Message::NewProfile);
    app.update(Message::ChooseProtocol(DraftProtocol::Citrix));
    let Some(Dialog::EditProfile { draft, .. }) = &app.dialog else {
        panic!("{:?}", app.dialog);
    };
    assert!(draft.is_on(ProfileToggle::Seamless) && draft.is_on(ProfileToggle::Sso));
    assert!(!draft.shows(ProfileField::Host) && !draft.shows(ProfileField::Port));
    assert!(!draft.shows(ProfileField::Username) && !draft.shows(ProfileField::VaultEntry));
    assert!(draft.shows(ProfileField::StoreFrontUrl) && draft.shows(ProfileField::IcaFile));
    assert_eq!(draft.test_target(), None, "no address to test");

    type_in(&mut app, ProfileField::Name, "ERP");
    type_in(&mut app, ProfileField::IcaFile, r"  C:\apps\erp.ica  ");
    type_in(&mut app, ProfileField::StoreFrontUrl, "   ");
    app.update(Message::ConfirmDialog);
    assert!(app.dialog.is_none(), "{:?}", app.dialog);
    let on_disk = saved(dir.path());
    assert_eq!(on_disk.len(), 1);
    let erp = &on_disk[0];
    assert_eq!(
        (
            erp.name.as_str(),
            erp.store_front_url.as_deref(),
            erp.app_name.as_deref(),
            erp.ica_file.as_deref(),
            erp.seamless,
            erp.sso
        ),
        ("ERP", None, None, Some(r"C:\apps\erp.ica"), true, true),
        "a blank text saved as none, nothing else required"
    );
}

#[test]
fn a_citrix_profile_is_edited_and_duplicated_in_its_own_protocol() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path(), &[outlook()]);
    app.update(Message::EditProfile(ProfileId::new("outlook")));
    let Some(Dialog::EditProfile { draft, .. }) = &app.dialog else {
        panic!("{:?}", app.dialog);
    };
    assert_eq!(draft.protocol, DraftProtocol::Citrix);
    assert_eq!(draft.value(ProfileField::AppName), "Outlook 365");
    assert!(draft.is_on(ProfileToggle::Seamless) && !draft.is_on(ProfileToggle::Sso));
    app.update(Message::ProfileToggle {
        toggle: ProfileToggle::Sso,
        on: true,
    });
    type_in(&mut app, ProfileField::AppName, "Outlook");
    app.update(Message::ConfirmDialog);
    assert_eq!(
        saved(dir.path()),
        [CitrixProfile {
            app_name: Some("Outlook".to_owned()),
            sso: true,
            ..outlook()
        }]
    );

    app.update(Message::DuplicateProfile {
        id: ProfileId::new("outlook"),
        suffix: " (copy)".to_owned(),
    });
    let copy = app.selected_profile.clone().expect("the copy selected");
    let on_disk = saved(dir.path());
    assert_eq!(on_disk.len(), 2);
    assert_eq!(on_disk[1].id, copy);
    assert_eq!(on_disk[1].name, "Outlook (copy)");
    assert_eq!(on_disk[1].app_name.as_deref(), Some("Outlook"));
}
