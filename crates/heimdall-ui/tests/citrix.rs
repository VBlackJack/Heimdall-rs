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
//! status bar saying how a launch went, as the C# does.

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
    let shell = Shell::with_app(app(dir.path(), Some(outlook())));
    let mut ui = common::simulator(settings(), WINDOW, shell.view());
    ui.find("Outlook").expect("name");
    ui.find("Citrix").expect("protocol");
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
}
