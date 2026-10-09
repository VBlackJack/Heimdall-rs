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

//! Citrix profiles: launched outside Heimdall, a tab showing the launch and its client's
//! state, the status bar saying how it went; created, edited and duplicated as the other
//! protocols' profiles; imported from Citrix Workspace's cache, their launch lines kept in
//! the vault.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Instant, SystemTime};

use heimdall_app::citrix::{CitrixLaunch, CitrixRefusal};
use heimdall_app::citrix_session::{
    ClientState, LaunchMethod, Launched, LauncherStatus, LauncherWatch, ListError, Probe,
};
use heimdall_app::citrix_terminate::{TerminateOffer, TerminateResult};
use heimdall_app::profile_draft::{DraftProtocol, ProfileField, ProfileToggle};
use heimdall_app::{
    App, AppConfig, CitrixImportOutcome, Dialog, Effect, Message, Notice, Phase, ProfileKind,
    SystemCredentials, TabId, TabProfile, VaultStatus, open_vault,
};
use heimdall_core::credentials::{citrix_launch_entry, decode_citrix_launch};
use heimdall_core::import::citrix_cache::{self, CacheScan};
use heimdall_core::profile::{CitrixProfile, ProfileId};
use heimdall_core::store::ProfileStore;
use heimdall_ssh::{AgentSource, Secret};
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
fn a_citrix_application_launches_outside_heimdall_and_its_tab_says_how_it_goes() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path(), &[outlook()]);
    // Opened from the tree, as any profile.
    let effects = app.update(Message::ConnectProfile(ProfileId::new("outlook")));
    let [
        Effect::LaunchCitrix {
            tab,
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
    let tab = *tab;
    // Its window is Citrix's own: the tab shows its status.
    let shown = app.tab(tab).expect("its tab");
    assert!(matches!(shown.profile, TabProfile::Citrix(_)));
    assert_eq!(
        shown.citrix.as_deref().map(|pane| pane.method),
        Some(LaunchMethod::StoreFront)
    );
    assert_eq!(shown.phase, Phase::Connecting);
    assert_eq!(app.active, Some(tab));
    assert_eq!(client(&app, tab), ClientState::Launching);
    assert_eq!(app.notice(), Some(&Notice::CitrixLaunching));
    assert!(!app.polls_citrix(), "nothing launched yet");

    let effects = app.update(Message::CitrixLaunched {
        tab,
        name: "Outlook".to_owned(),
        result: Ok(launched_with(&[])),
    });
    assert!(effects.is_empty());
    assert_eq!(
        app.notice(),
        Some(&Notice::CitrixLaunched("Outlook".to_owned()))
    );
    assert!(app.polls_citrix());

    // Every tick, one look at a time.
    let effects = app.update(Message::CitrixTick);
    assert!(
        matches!(
            effects.as_slice(),
            [Effect::ProbeCitrix { tab: probed, lists: true, .. }] if *probed == tab
        ),
        "{effects:?}"
    );
    assert!(app.update(Message::CitrixTick).is_empty(), "one under way");
    app.update(Message::CitrixProbed {
        tab,
        probe: seen(LauncherStatus::Running, &[]),
    });
    assert_eq!(client(&app, tab), ClientState::NotFoundYet);

    app.update(Message::CitrixTick);
    app.update(Message::CitrixProbed {
        tab,
        probe: seen(LauncherStatus::Exited(Some(0)), &[42]),
    });
    assert_eq!(client(&app, tab), ClientState::Running(42));
    let shown = app.tab(tab).expect("its tab");
    assert_eq!(shown.phase, Phase::Connected);
    assert!(
        !shown.is_live(),
        "closing it leaves the Citrix session as it is"
    );
    assert_eq!(
        shown
            .citrix
            .as_deref()
            .and_then(|pane| pane.tracker.exit_code()),
        Some(0)
    );

    app.update(Message::CitrixTick);
    app.update(Message::CitrixProbed {
        tab,
        probe: seen(LauncherStatus::Exited(Some(0)), &[]),
    });
    assert_eq!(client(&app, tab), ClientState::Ended(42));
    assert_eq!(
        app.tab(tab).expect("its tab").phase,
        Phase::Closed { exit_status: None }
    );
    assert!(!app.polls_citrix(), "nothing more to learn");
}

/// A launcher standing as it was made.
struct Watch(LauncherStatus);

impl LauncherWatch for Watch {
    fn status(&self) -> LauncherStatus {
        self.0
    }
}

/// A launch done with the clients `before` running, its launcher still running.
fn launched_with(before: &[u32]) -> Launched {
    Launched {
        baseline: Ok(before.iter().copied().collect()),
        launcher: Arc::new(Watch(LauncherStatus::Running)),
        at: SystemTime::now(),
    }
}

/// What a probe saw: the launcher, and the clients running.
fn seen(launcher: LauncherStatus, clients: &[u32]) -> Probe {
    Probe {
        launcher,
        clients: Ok(clients.iter().copied().collect()),
    }
}

/// Opens `id`: its status tab, and the launch asked.
fn open(app: &mut App, id: &str) -> TabId {
    let effects = app.update(Message::OpenCitrix(ProfileId::new(id)));
    let [Effect::LaunchCitrix { tab, .. }] = effects.as_slice() else {
        panic!("{effects:?}");
    };
    *tab
}

/// Opens `id` and launches it, the clients `before` running.
fn open_launched(app: &mut App, id: &str, before: &[u32]) -> TabId {
    let tab = open(app, id);
    app.update(Message::CitrixLaunched {
        tab,
        name: id.to_owned(),
        result: Ok(launched_with(before)),
    });
    tab
}

/// The client state of Citrix tab `tab`.
fn client(app: &App, tab: TabId) -> ClientState {
    app.tab(tab)
        .and_then(|tab| tab.citrix.as_deref())
        .expect("a Citrix tab")
        .tracker
        .state()
        .clone()
}

#[test]
fn a_launch_that_starts_nothing_says_why_in_its_tab() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path(), &[outlook()]);
    let tab = open(&mut app, "outlook");
    app.update(Message::CitrixLaunched {
        tab,
        name: "Outlook".to_owned(),
        result: Err(CitrixRefusal::WorkspaceNotFound),
    });
    assert_eq!(
        app.notice(),
        Some(&Notice::CitrixRefused(CitrixRefusal::WorkspaceNotFound))
    );
    assert_eq!(
        client(&app, tab),
        ClientState::NotStarted(CitrixRefusal::WorkspaceNotFound)
    );
    assert!(!app.polls_citrix());
    assert!(app.update(Message::CitrixTick).is_empty());
}

#[test]
fn a_failed_launcher_and_a_shared_client_are_said() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path(), &[outlook()]);
    let failed = open_launched(&mut app, "outlook", &[]);
    app.update(Message::CitrixProbed {
        tab: failed,
        probe: seen(LauncherStatus::Exited(Some(2)), &[]),
    });
    assert_eq!(client(&app, failed), ClientState::LauncherFailed(2));

    // A client running before the launch, and none of its own.
    let shared = open_launched(&mut app, "outlook", &[7]);
    app.update(Message::CitrixProbed {
        tab: shared,
        probe: seen(LauncherStatus::Exited(Some(0)), &[7]),
    });
    assert_eq!(client(&app, shared), ClientState::Shared);
    assert_eq!(app.tab(shared).expect("tab").phase, Phase::Connected);
}

#[test]
fn a_client_another_tab_follows_is_never_taken() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path(), &[outlook()]);
    let first = open_launched(&mut app, "outlook", &[]);
    let second = open_launched(&mut app, "outlook", &[]);
    for tab in [first, second] {
        app.update(Message::CitrixProbed {
            tab,
            probe: seen(LauncherStatus::Running, &[42]),
        });
    }
    assert_eq!(client(&app, first), ClientState::Running(42));
    assert_eq!(client(&app, second), ClientState::NotFoundYet);
}

#[test]
fn closing_a_citrix_tab_stops_looking_at_its_client() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path(), &[outlook()]);
    let tab = open_launched(&mut app, "outlook", &[]);
    app.update(Message::CitrixTick);
    app.update(Message::CitrixProbed {
        tab,
        probe: seen(LauncherStatus::Running, &[42]),
    });
    assert!(app.polls_citrix());

    // Closed at once: no session of Heimdall's is lost, the Citrix one is left running.
    let effects = app.update(Message::RequestCloseTab(tab));
    assert!(effects.is_empty(), "{effects:?}");
    assert!(app.dialog.is_none(), "not asked");
    assert!(app.tab(tab).is_none());
    assert!(!app.polls_citrix());
    assert!(app.update(Message::CitrixTick).is_empty());
    // A probe under way answering then changes nothing.
    let effects = app.update(Message::CitrixProbed {
        tab,
        probe: seen(LauncherStatus::Running, &[]),
    });
    assert!(effects.is_empty());
    // Nor a launch answering after its tab was closed.
    let closed = open(&mut app, "outlook");
    app.update(Message::RequestCloseTab(closed));
    let effects = app.update(Message::CitrixLaunched {
        tab: closed,
        name: "Outlook".to_owned(),
        result: Ok(launched_with(&[])),
    });
    assert!(effects.is_empty());
    assert!(app.tabs.is_empty());
}

/// Opens `id`, launched with the clients `before` running, then sees the clients `now`.
fn open_seen(app: &mut App, id: &str, before: &[u32], now: &[u32]) -> TabId {
    let tab = open_launched(app, id, before);
    app.update(Message::CitrixProbed {
        tab,
        probe: seen(LauncherStatus::Running, now),
    });
    tab
}

/// What Citrix tab `tab` offers now to end its session.
fn offer(app: &App, tab: TabId) -> TerminateOffer {
    app.citrix_terminate_offer(tab, Instant::now())
}

/// Presses Terminate, or Force terminate when `force`, in tab `tab`: whether it asked.
fn press_terminate(app: &mut App, tab: TabId, force: bool) -> bool {
    let effects = app.update(Message::CitrixTerminate { tab, force });
    assert!(
        effects.is_empty(),
        "nothing runs before the answer: {effects:?}"
    );
    matches!(
        app.dialog,
        Some(Dialog::ConfirmCitrixTerminate { tab: asked, force: forced, .. })
            if asked == tab && forced == force
    )
}

#[test]
fn terminate_is_offered_only_for_a_client_the_tab_found_itself() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path(), &[outlook()]);
    let own = open_seen(&mut app, "outlook", &[], &[42]);
    assert_eq!(client(&app, own), ClientState::Running(42));
    assert_eq!(offer(&app, own), TerminateOffer::Terminate);

    // Another tab seeing the same client, already followed: not its own.
    let other = open_seen(&mut app, "outlook", &[], &[42]);
    assert_eq!(client(&app, other), ClientState::NotFoundYet);
    assert_eq!(offer(&app, other), TerminateOffer::Nothing);
    assert!(!press_terminate(&mut app, other, false));

    // A client running before the launch: shared with other sessions.
    let shared = open_seen(&mut app, "outlook", &[7], &[7]);
    assert_eq!(client(&app, shared), ClientState::Shared);
    assert_eq!(offer(&app, shared), TerminateOffer::Nothing);
    assert!(!press_terminate(&mut app, shared, false));

    // Not tracked: no client known.
    let untracked = open(&mut app, "outlook");
    app.update(Message::CitrixLaunched {
        tab: untracked,
        name: "Outlook".to_owned(),
        result: Ok(Launched {
            baseline: Err(ListError::Unsupported),
            launcher: Arc::new(Watch(LauncherStatus::Running)),
            at: SystemTime::now(),
        }),
    });
    assert_eq!(offer(&app, untracked), TerminateOffer::Nothing);
    assert!(!press_terminate(&mut app, untracked, false));

    // Ended: nothing left to end.
    app.update(Message::CitrixProbed {
        tab: own,
        probe: seen(LauncherStatus::Running, &[]),
    });
    assert_eq!(client(&app, own), ClientState::Ended(42));
    assert_eq!(offer(&app, own), TerminateOffer::Nothing);
    assert!(!press_terminate(&mut app, own, false));
    assert!(app.dialog.is_none());
}

#[test]
fn terminate_asks_first_then_asks_the_client_to_close() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path(), &[outlook()]);
    let tab = open_seen(&mut app, "outlook", &[], &[42]);

    // Cancelled: nothing runs, Terminate stays offered.
    assert!(press_terminate(&mut app, tab, false));
    let effects = app.update(Message::DismissDialog);
    assert!(effects.is_empty(), "{effects:?}");
    assert!(app.dialog.is_none());
    assert_eq!(offer(&app, tab), TerminateOffer::Terminate);
    // Forcing is not offered before the client was asked.
    assert!(!press_terminate(&mut app, tab, true));

    assert!(press_terminate(&mut app, tab, false));
    let effects = app.update(Message::ConfirmDialog);
    assert!(
        matches!(
            effects.as_slice(),
            [Effect::TerminateCitrix { tab: asked, pid: 42, force: false }] if *asked == tab
        ),
        "{effects:?}"
    );
    assert_eq!(offer(&app, tab), TerminateOffer::Pending { force: false });
    assert!(
        !press_terminate(&mut app, tab, true),
        "one request at a time"
    );

    // Accepted: the client is given time to close, forcing not offered yet.
    app.update(Message::CitrixTerminated {
        tab,
        pid: 42,
        result: TerminateResult::Requested,
    });
    assert_eq!(offer(&app, tab), TerminateOffer::Asked { force: false });
    assert!(!press_terminate(&mut app, tab, true));
    assert!(!press_terminate(&mut app, tab, false));

    // The next look sees it gone.
    app.update(Message::CitrixProbed {
        tab,
        probe: seen(LauncherStatus::Running, &[]),
    });
    assert_eq!(client(&app, tab), ClientState::Ended(42));
    assert_eq!(
        app.tab(tab).expect("tab").phase,
        Phase::Closed { exit_status: None }
    );
    assert_eq!(offer(&app, tab), TerminateOffer::Nothing);
}

#[test]
fn a_request_that_failed_offers_to_force_it_asked_again() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path(), &[outlook()]);
    let tab = open_seen(&mut app, "outlook", &[], &[42]);
    assert!(press_terminate(&mut app, tab, false));
    app.update(Message::ConfirmDialog);
    app.update(Message::CitrixTerminated {
        tab,
        pid: 42,
        result: TerminateResult::Refused(1),
    });
    assert_eq!(
        offer(&app, tab),
        TerminateOffer::Force(TerminateResult::Refused(1))
    );
    assert!(!press_terminate(&mut app, tab, false), "only forcing now");

    // The second question, cancelled, runs nothing.
    assert!(press_terminate(&mut app, tab, true));
    assert!(app.update(Message::DismissDialog).is_empty());

    assert!(press_terminate(&mut app, tab, true));
    let effects = app.update(Message::ConfirmDialog);
    assert!(
        matches!(
            effects.as_slice(),
            [Effect::TerminateCitrix {
                pid: 42,
                force: true,
                ..
            }]
        ),
        "{effects:?}"
    );
    assert_eq!(offer(&app, tab), TerminateOffer::Pending { force: true });
    // An answer for another client changes nothing.
    app.update(Message::CitrixTerminated {
        tab,
        pid: 43,
        result: TerminateResult::Requested,
    });
    assert_eq!(offer(&app, tab), TerminateOffer::Pending { force: true });
    app.update(Message::CitrixTerminated {
        tab,
        pid: 42,
        result: TerminateResult::TimedOut,
    });
    assert_eq!(
        offer(&app, tab),
        TerminateOffer::Force(TerminateResult::TimedOut)
    );
}

#[test]
fn a_confirmation_for_a_client_gone_meanwhile_runs_nothing() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path(), &[outlook()]);
    let tab = open_seen(&mut app, "outlook", &[], &[42]);
    assert!(press_terminate(&mut app, tab, false));
    app.update(Message::CitrixProbed {
        tab,
        probe: seen(LauncherStatus::Running, &[]),
    });
    assert!(app.update(Message::ConfirmDialog).is_empty());

    // Its tab closed meanwhile: nothing runs, and closing it ended nothing.
    let closed = open_seen(&mut app, "outlook", &[], &[42, 50]);
    assert_eq!(client(&app, closed), ClientState::Running(50));
    assert!(press_terminate(&mut app, closed, false));
    app.dialog = None;
    assert!(app.update(Message::RequestCloseTab(closed)).is_empty());
    app.dialog = Some(Dialog::ConfirmCitrixTerminate {
        tab: closed,
        pid: 50,
        force: false,
    });
    assert!(app.update(Message::ConfirmDialog).is_empty());
    // A request answering after its tab closed changes nothing.
    assert!(
        app.update(Message::CitrixTerminated {
            tab: closed,
            pid: 50,
            result: TerminateResult::Requested,
        })
        .is_empty()
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

/// A Citrix Workspace cache file: two applications of one store.
const CACHE: &str = r#"<resources>
  <resource>
    <FriendlyName>Excel 2024</FriendlyName>
    <Category>Office</Category>
    <LaunchCommandLine>-qlaunch "Excel 2024" -s store-abc</LaunchCommandLine>
    <icaLaunchUrl>https://store.corp.lab/Citrix/Store/resources/launch/ica</icaLaunchUrl>
  </resource>
  <resource>
    <FriendlyName>Notepad</FriendlyName>
    <LaunchCommandLine>-qlaunch "Notepad" -s store-abc</LaunchCommandLine>
  </resource>
</resources>"#;

const MASTER: &str = "correct horse battery staple";

fn app_with(dir: &Path, system: &SystemCredentials) -> App {
    App::new(AppConfig {
        profiles_file: profiles_file(dir),
        known_hosts: dir.join("known_hosts"),
        legacy_dir: None,
        agent: AgentSource::Disabled,
        initial_grid: GridSize { cols: 80, rows: 24 },
        files_start: dir.to_owned(),
        system_credentials: system.clone(),
    })
}

fn scan(text: &str) -> CacheScan {
    CacheScan {
        apps: citrix_cache::parse(text).expect("parsed"),
        warnings: Vec::new(),
    }
}

/// "Import Citrix Apps" run as the window runs it, the scan giving `text`, then confirmed.
fn import(app: &mut App, text: &str) -> CitrixImportOutcome {
    let effects = app.update(Message::ImportCitrix);
    assert!(
        matches!(effects.as_slice(), [Effect::ScanCitrixCache]),
        "{effects:?}"
    );
    app.update(Message::CitrixScanned(scan(text)));
    assert!(
        matches!(&app.dialog, Some(Dialog::ConfirmCitrixImport(scan)) if scan.apps.len() == 2),
        "{:?}",
        app.dialog
    );
    app.update(Message::ConfirmDialog);
    match app.dialog.take() {
        Some(Dialog::CitrixImportDone(outcome)) => outcome,
        other => panic!("{other:?}"),
    }
}

/// The launch line the system's store keeps for `id`.
fn stored_line(system: &SystemCredentials, id: &ProfileId) -> Option<String> {
    let SystemCredentials::Memory(entries) = system else {
        unreachable!()
    };
    let entries = entries.lock().expect("entries");
    let bytes = entries.get(&citrix_launch_entry(id))?;
    decode_citrix_launch(bytes).map(|line| line.to_string())
}

/// How opening `id` launches, or the notice refusing it.
fn launched(app: &mut App, id: &ProfileId) -> Result<CitrixLaunch, Notice> {
    let effects = app.update(Message::OpenCitrix(id.clone()));
    match <[Effect; 1]>::try_from(effects) {
        Ok([Effect::LaunchCitrix { launch, .. }]) => Ok(launch),
        Err(effects) if effects.is_empty() => Err(app.notice().cloned().expect("a notice")),
        other => panic!("{other:?}"),
    }
}

#[test]
fn the_workspace_cache_imports_its_applications_their_launch_lines_in_the_vault() {
    let dir = tempfile::tempdir().expect("dir");
    let system = SystemCredentials::memory();
    let mut app = app_with(dir.path(), &system);
    let outcome = import(&mut app, CACHE);
    assert_eq!((outcome.added, outcome.refreshed), (2, 0));
    assert!(!outcome.without_launch_lines);

    let profiles = saved(dir.path());
    let excel = profiles
        .iter()
        .find(|profile| profile.name == "Excel 2024")
        .expect("excel");
    assert_eq!(excel.app_name.as_deref(), Some("Excel 2024"));
    assert_eq!(excel.group.as_deref(), Some("Citrix/Office"));
    assert_eq!(
        excel.store_front_url.as_deref(),
        Some("https://store.corp.lab")
    );
    assert!(excel.sso);
    let file = std::fs::read_to_string(profiles_file(dir.path())).expect("profiles");
    assert!(!file.contains("qlaunch"), "never in the profiles file");
    assert_eq!(
        stored_line(&system, &excel.id).as_deref(),
        Some("-qlaunch \"Excel 2024\" -s store-abc")
    );

    // The cache line wins over the StoreFront, as in the C# order.
    assert!(matches!(
        launched(&mut app, &excel.id),
        Ok(CitrixLaunch::CacheLine(line)) if line.as_str() == "-qlaunch \"Excel 2024\" -s store-abc"
    ));
}

#[test]
fn scanning_again_refreshes_the_launch_lines_and_adds_nothing() {
    let dir = tempfile::tempdir().expect("dir");
    let system = SystemCredentials::memory();
    let mut app = app_with(dir.path(), &system);
    import(&mut app, CACHE);
    let outcome = import(&mut app, &CACHE.replace("store-abc", "store-new"));
    assert_eq!((outcome.added, outcome.refreshed), (0, 2));
    let profiles = saved(dir.path());
    assert_eq!(profiles.len(), 2, "{profiles:?}");
    for profile in &profiles {
        assert!(
            stored_line(&system, &profile.id).is_some_and(|line| line.ends_with("store-new")),
            "{}",
            profile.name
        );
    }
}

#[test]
fn a_launch_line_a_shell_would_read_is_refused() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app_with(dir.path(), &SystemCredentials::memory());
    import(
        &mut app,
        &CACHE.replace("-s store-abc", "-s store &amp; calc"),
    );
    let id = saved(dir.path())[0].id.clone();
    assert_eq!(
        launched(&mut app, &id),
        Err(Notice::CitrixRefused(CitrixRefusal::CommandRejected))
    );
}

#[test]
fn without_a_store_for_secrets_the_applications_come_without_their_launch_lines() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app_with(dir.path(), &SystemCredentials::Unavailable);
    let outcome = import(&mut app, CACHE);
    assert_eq!(outcome.added, 2);
    assert!(outcome.without_launch_lines);
    let excel = saved(dir.path())
        .into_iter()
        .find(|profile| profile.name == "Excel 2024")
        .expect("excel");
    assert!(matches!(
        launched(&mut app, &excel.id),
        Ok(CitrixLaunch::StoreFront { .. })
    ));
}

#[test]
fn deleting_a_citrix_profile_deletes_its_launch_line() {
    let dir = tempfile::tempdir().expect("dir");
    let system = SystemCredentials::memory();
    let mut app = app_with(dir.path(), &system);
    import(&mut app, CACHE);
    let id = saved(dir.path())[0].id.clone();
    assert!(stored_line(&system, &id).is_some());
    app.update(Message::EditProfile(id.clone()));
    app.update(Message::DeleteProfile);
    app.update(Message::ConfirmDialog);
    assert_eq!(saved(dir.path()).len(), 1, "{:?}", app.dialog);
    assert_eq!(stored_line(&system, &id), None);
}

/// Runs the vault job `effects` asks for, as the window would.
async fn vault_job(app: &mut App, effects: Vec<Effect>) {
    let Ok(
        [
            Effect::OpenVault {
                path,
                password,
                job,
                ticket,
            },
        ],
    ) = <[Effect; 1]>::try_from(effects)
    else {
        panic!("expected OpenVault");
    };
    app.update(Message::VaultOpened(
        ticket,
        open_vault(path, password, job).await,
    ));
}

#[tokio::test]
async fn a_master_password_takes_the_launch_lines_and_a_locked_vault_refuses_the_launch() {
    let submit = |confirm: bool| Message::SubmitVault {
        password: Secret::new(MASTER.to_owned()),
        new: None,
        confirm: confirm.then(|| Secret::new(MASTER.to_owned())),
    };
    let dir = tempfile::tempdir().expect("dir");
    let system = SystemCredentials::memory();
    // A profile with no other way to launch: neither StoreFront nor ICA file.
    let bare = CitrixProfile {
        id: ProfileId::new("bare"),
        store_front_url: None,
        ..outlook()
    };
    let mut store = ProfileStore::open(profiles_file(dir.path())).expect("store");
    store.merge_citrix([bare.clone()]);
    store.save().expect("save");
    let mut app = app_with(dir.path(), &system);
    import(&mut app, CACHE);
    let id = saved(dir.path())
        .into_iter()
        .find(|profile| profile.id != bare.id)
        .expect("imported")
        .id;

    app.update(Message::ShowVault);
    let effects = app.update(submit(true));
    vault_job(&mut app, effects).await;
    assert_eq!(app.vault_status(), VaultStatus::Open);
    assert_eq!(stored_line(&system, &id), None, "moved into the vault");
    assert!(matches!(
        launched(&mut app, &id),
        Ok(CitrixLaunch::CacheLine(_))
    ));

    app.update(Message::LockVault);
    // Locked, its line cannot be read: launched through its StoreFront instead.
    assert!(matches!(
        launched(&mut app, &id),
        Ok(CitrixLaunch::StoreFront { .. })
    ));
    // No other way: the vault is to be unlocked, as the C# says.
    assert_eq!(
        launched(&mut app, &bare.id),
        Err(Notice::CitrixRefused(CitrixRefusal::VaultLocked))
    );
    let effects = app.update(submit(false));
    vault_job(&mut app, effects).await;
    assert!(matches!(
        launched(&mut app, &id),
        Ok(CitrixLaunch::CacheLine(_))
    ));
}
