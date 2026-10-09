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

//! Citrix profiles in the window, drawn headless: listed in the tree with their protocol,
//! their form asking for a `StoreFront` and an application rather than a server, and the
//! status bar saying how a launch went, and the import from Citrix Workspace's cache, as
//! the C# does.

mod common;

use std::path::Path;

use heimdall_app::citrix::CitrixRefusal;
use heimdall_app::profile_draft::DraftProtocol;
use heimdall_app::{App, AppConfig, Message as AppMessage, Notice, SessionStatus};
use heimdall_core::profile::{CitrixProfile, ProfileId};
use heimdall_core::store::ProfileStore;
use heimdall_ssh::AgentSource;
use heimdall_term::GridSize;
use heimdall_ui::shell::{Message, Shell};
use heimdall_ui::status_bar::status_text;
use heimdall_ui::terminal_view::FONTS;
use iced::{Settings, Size};

const WINDOW: Size = Size::new(1200.0, 720.0);
/// Height of a window showing the whole form.
const TALL_HEIGHT: f32 = 1280.0;

fn app(dir: &Path, citrix: Option<CitrixProfile>) -> App {
    let profiles_file = dir.join("profiles.toml");
    if let Some(profile) = citrix {
        let mut store = ProfileStore::open(&profiles_file).expect("store");
        store.merge_citrix([profile]);
        store.save().expect("save");
    }
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

fn settings() -> Settings {
    Settings {
        fonts: FONTS.iter().map(|face| (*face).into()).collect(),
        ..Settings::default()
    }
}

fn outlook() -> CitrixProfile {
    CitrixProfile {
        id: ProfileId::new("outlook"),
        name: "Outlook".to_owned(),
        group: None,
        store_front_url: Some("https://store.lab/Citrix/Store".to_owned()),
        app_name: Some("Outlook 365".to_owned()),
        ica_file: None,
        seamless: true,
        sso: true,
    }
}

#[test]
fn a_saved_citrix_profile_is_listed_with_its_protocol_and_connects_by_its_id() {
    let dir = tempfile::tempdir().expect("dir");
    let mut shell = Shell::with_app(app(dir.path(), Some(outlook())));
    let mut ui = common::simulator(settings(), WINDOW, shell.view());
    ui.find("Outlook").expect("name");
    drop(ui);
    let messages = common::double_click_messages(
        || common::simulator(settings(), WINDOW, shell.view()),
        "Outlook",
    );
    assert!(
        messages.iter().any(|message| matches!(
            message,
            Message::App(AppMessage::ConnectProfile(id)) if id.as_str() == "outlook"
        )),
        "a double click connects"
    );
    // The tree shows the protocol as its icon; the details of the session selected name it.
    let _ = shell.update(Message::TreeClick(ProfileId::new("outlook")));
    common::simulator(settings(), WINDOW, shell.view())
        .find("Citrix")
        .expect("protocol");
}

#[test]
fn a_citrix_form_asks_for_its_storefront_and_application_not_a_server() {
    let dir = tempfile::tempdir().expect("dir");
    let mut shell = Shell::with_app(app(dir.path(), None));
    let _ = shell.update(Message::App(AppMessage::NewProfile));
    {
        let mut ui = common::simulator(
            settings(),
            Size::new(WINDOW.width, TALL_HEIGHT),
            shell.view(),
        );
        ui.find("Citrix Workspace published app")
            .expect("Citrix card");
    }
    let _ = shell.update(Message::App(AppMessage::ChooseProtocol(
        DraftProtocol::Citrix,
    )));
    let mut ui = common::simulator(
        settings(),
        Size::new(WINDOW.width, TALL_HEIGHT),
        shell.view(),
    );
    for label in [
        "Citrix Workspace",
        "StoreFront URL",
        "Application name",
        "Advanced Citrix options",
        "ICA file path",
        "Provide either a StoreFront URL + application name, or a direct ICA file path.",
        "Seamless mode",
        "Use SSO (Kerberos)",
        "Name the session as the tree lists it.",
    ] {
        ui.find(label).expect(label);
    }
    for absent in ["Server *", "Password", "Username", "Gateway routing"] {
        assert!(ui.find(absent).is_err(), "{absent}");
    }
}

#[test]
fn the_status_bar_says_how_a_launch_went_in_the_csharp_words() {
    let said = |notice: Notice| status_text(&SessionStatus::Ready, Some(&notice), 0);
    assert_eq!(said(Notice::CitrixLaunching), "Launching Citrix session...");
    assert_eq!(
        said(Notice::CitrixRefused(CitrixRefusal::InvalidStoreFront)),
        "Invalid Citrix StoreFront URL. Use an absolute HTTP or HTTPS URL."
    );
    assert_eq!(
        said(Notice::CitrixRefused(CitrixRefusal::WorkspaceNotFound)),
        "Citrix Workspace not found. Install Citrix Workspace App."
    );
    assert!(
        said(Notice::CitrixLaunched("Outlook".to_owned())).contains("Outlook"),
        "the session named"
    );
    assert!(
        said(Notice::CitrixRefused(CitrixRefusal::NotStarted(
            "access denied".to_owned()
        )))
        .contains("access denied")
    );
    assert_eq!(
        said(Notice::CitrixRefused(CitrixRefusal::VaultLocked)),
        "Unlock the vault before launching this Citrix session."
    );
    assert_eq!(
        said(Notice::CitrixRefused(CitrixRefusal::CommandRejected)),
        "The Citrix launch command contains forbidden characters (|, &, ;, `, $, newlines)."
    );
}

#[test]
fn the_more_menu_imports_citrix_apps_after_the_csharp_question() {
    use heimdall_core::import::citrix_cache::{self, CacheScan};
    use heimdall_ui::tree_view::TreeMenu;

    let dir = tempfile::tempdir().expect("dir");
    let mut shell = Shell::with_app(app(dir.path(), None));
    let _ = shell.update(Message::OpenTreeMenu(TreeMenu::More));
    let mut ui = common::simulator(settings(), WINDOW, shell.view());
    ui.click("Import Citrix Apps").expect("the entry");
    assert!(
        ui.into_messages()
            .any(|message| matches!(message, Message::MenuChoice(AppMessage::ImportCitrix)))
    );

    let scan = CacheScan {
        apps: citrix_cache::parse(
            "<resources><resource><FriendlyName>Excel</FriendlyName>             <LaunchCommandLine>-qlaunch Excel</LaunchCommandLine></resource></resources>",
        )
        .expect("parsed"),
        warnings: Vec::new(),
    };
    let _ = shell.update(Message::App(AppMessage::CitrixScanned(scan)));
    {
        let mut ui = common::simulator(settings(), WINDOW, shell.view());
        ui.find("Citrix Applications").expect("title");
        ui.find("Import 1 Citrix application from local Workspace cache?")
            .expect("the C# question");
    }
    let _ = shell.update(Message::App(AppMessage::ConfirmDialog));
    let mut ui = common::simulator(settings(), WINDOW, shell.view());
    ui.find("1 Citrix application imported successfully.")
        .expect("the C# result");
}

#[test]
fn an_empty_cache_says_so_in_the_csharp_words() {
    let dir = tempfile::tempdir().expect("dir");
    let mut shell = Shell::with_app(app(dir.path(), None));
    let _ = shell.update(Message::App(AppMessage::CitrixScanned(
        heimdall_core::import::citrix_cache::CacheScan::default(),
    )));
    let mut ui = common::simulator(settings(), WINDOW, shell.view());
    ui.find(
        "No Citrix applications found in the local cache. Open Citrix Workspace and connect to a store first.",
    )
    .expect("the C# words");
}

/// A launcher standing as it was made.
struct Watch(heimdall_app::citrix_session::LauncherStatus);

impl heimdall_app::citrix_session::LauncherWatch for Watch {
    fn status(&self) -> heimdall_app::citrix_session::LauncherStatus {
        self.0
    }
}

#[test]
fn a_citrix_tab_shows_its_launch_and_its_client_as_the_csharp_info_panel() {
    use heimdall_app::Effect;
    use heimdall_app::citrix_session::{Launched, LauncherStatus, Probe};

    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path(), Some(outlook()));
    // The launch asked is not run: the tab is drawn from what it would answer.
    let effects = app.update(AppMessage::OpenCitrix(ProfileId::new("outlook")));
    let [Effect::LaunchCitrix { tab, .. }] = effects.as_slice() else {
        panic!("{effects:?}");
    };
    let tab = *tab;
    app.update(AppMessage::CitrixLaunched {
        tab,
        name: "Outlook".to_owned(),
        result: Ok(Launched {
            baseline: Ok([5].into_iter().collect()),
            launcher: std::sync::Arc::new(Watch(LauncherStatus::Running)),
            at: std::time::SystemTime::now(),
        }),
    });
    app.update(AppMessage::CitrixProbed {
        tab,
        probe: Probe {
            launcher: LauncherStatus::Exited(Some(0)),
            clients: Ok([5, 4242].into_iter().collect()),
        },
    });
    let shell = Shell::with_app(app);
    let mut ui = common::simulator(settings(), WINDOW, shell.view());
    for shown in [
        "StoreFront: https://store.lab/Citrix/Store",
        "Application: Outlook 365",
        "Mode: StoreFront",
        "Launcher exit code: 0",
        "Connected. Citrix client PID: 4242",
    ] {
        ui.find(shown).expect(shown);
    }
}

#[test]
fn each_client_state_is_said() {
    use heimdall_app::citrix_session::{ClientState, Untracked};
    use heimdall_ui::citrix_view::client_text;

    assert_eq!(
        client_text(&ClientState::Launching),
        "Launching Citrix session..."
    );
    assert_eq!(
        client_text(&ClientState::Ended(12)),
        "Disconnected. The Citrix client (PID: 12) ended."
    );
    assert_eq!(
        client_text(&ClientState::LauncherFailed(3)),
        "The Citrix launcher failed with exit code 3."
    );
    assert!(client_text(&ClientState::Shared).contains("already running"));
    assert!(client_text(&ClientState::NotFoundYet).contains("not found yet"));
    assert_eq!(
        client_text(&ClientState::Untracked(Untracked::WindowsOnly)),
        "Citrix client tracking is available on Windows only."
    );
    assert!(client_text(&ClientState::Untracked(Untracked::Unavailable)).contains("not tracked"));
    assert!(
        client_text(&ClientState::Untracked(Untracked::TimedOut)).contains("not listed in time")
    );
    assert_eq!(
        client_text(&ClientState::NotStarted(CitrixRefusal::WorkspaceNotFound)),
        "Citrix Workspace not found. Install Citrix Workspace App."
    );
}

/// A Citrix tab of `app`, launched with the clients `before` running, then seeing `now`.
fn seen_tab(app: &mut App, before: &[u32], now: &[u32]) -> heimdall_app::TabId {
    use heimdall_app::Effect;
    use heimdall_app::citrix_session::{Launched, LauncherStatus, Probe};

    // The launch asked is not run: the tab is drawn from what it would answer.
    let effects = app.update(AppMessage::OpenCitrix(ProfileId::new("outlook")));
    let [Effect::LaunchCitrix { tab, .. }] = effects.as_slice() else {
        panic!("{effects:?}");
    };
    let tab = *tab;
    app.update(AppMessage::CitrixLaunched {
        tab,
        name: "Outlook".to_owned(),
        result: Ok(Launched {
            baseline: Ok(before.iter().copied().collect()),
            launcher: std::sync::Arc::new(Watch(LauncherStatus::Running)),
            at: std::time::SystemTime::now(),
        }),
    });
    app.update(AppMessage::CitrixProbed {
        tab,
        probe: Probe {
            launcher: LauncherStatus::Running,
            clients: Ok(now.iter().copied().collect()),
        },
    });
    tab
}

#[test]
fn its_own_client_offers_terminate_asked_first_in_the_csharp_words() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path(), Some(outlook()));
    let tab = seen_tab(&mut app, &[], &[4242]);
    let mut shell = Shell::with_app(app);
    {
        let mut ui = common::simulator(settings(), WINDOW, shell.view());
        ui.click("Terminate").expect("the button");
        assert!(ui.into_messages().any(|message| matches!(
            message,
            Message::App(AppMessage::CitrixTerminate { tab: asked, force: false }) if asked == tab
        )));
    }
    let _ = shell.update(Message::App(AppMessage::CitrixTerminate {
        tab,
        force: false,
    }));
    let mut ui = common::simulator(settings(), WINDOW, shell.view());
    ui.find("Terminate the Citrix session?")
        .expect("the C# title");
    ui.find(
        "Unsaved work in the remote application will be lost. Are you sure you want to terminate?",
    )
    .expect("the C# question");
}

#[test]
fn a_shared_client_offers_no_terminate() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path(), Some(outlook()));
    seen_tab(&mut app, &[5], &[5]);
    let shell = Shell::with_app(app);
    let mut ui = common::simulator(settings(), WINDOW, shell.view());
    ui.find("The session uses a Citrix client that was already running: it cannot be told apart from the other sessions.")
        .expect("shared");
    assert!(ui.find("Terminate").is_err(), "no button");
}

#[test]
fn each_terminate_step_is_said() {
    use heimdall_app::citrix_terminate::{TerminateOffer, TerminateResult};
    use heimdall_ui::citrix_view::{terminate_question, terminate_text};

    assert_eq!(terminate_text(&TerminateOffer::Nothing), None);
    assert_eq!(terminate_text(&TerminateOffer::Terminate), None);
    assert_eq!(
        terminate_text(&TerminateOffer::Pending { force: false }).as_deref(),
        Some("Asking the Citrix client to close...")
    );
    assert_eq!(
        terminate_text(&TerminateOffer::Asked { force: false }).as_deref(),
        Some("The Citrix client was asked to close.")
    );
    assert_eq!(
        terminate_text(&TerminateOffer::Force(TerminateResult::Requested)).as_deref(),
        Some("The Citrix client is still running after it was asked to close.")
    );
    assert_eq!(
        terminate_text(&TerminateOffer::Force(TerminateResult::Refused(1))).as_deref(),
        Some("The request to end the Citrix client failed: taskkill ended with exit code 1.")
    );
    assert!(
        terminate_text(&TerminateOffer::Force(TerminateResult::TimedOut))
            .is_some_and(|said| said.contains("in time"))
    );
    let (title, _, action) = terminate_question(true);
    assert_eq!(title, "Force terminate the Citrix session?");
    assert_eq!(action, "Force terminate");
}
