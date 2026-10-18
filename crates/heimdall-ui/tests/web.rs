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

//! Web consoles in the window, drawn headless: their card among the protocols, their form
//! asking for the address's parts and an account but no password, and the status bar saying
//! that the browser, not Heimdall, opened them.

mod common;

use std::path::Path;

use heimdall_app::profile_draft::{DraftProtocol, ProfileField};
use heimdall_app::web_console::WebRefusal;
use heimdall_app::{App, AppConfig, Message as AppMessage, Notice, SessionStatus};
use heimdall_ssh::AgentSource;
use heimdall_term::GridSize;
use heimdall_ui::shell::{Message, ProfileTab, Shell};
use heimdall_ui::status_bar::status_text;
use heimdall_ui::terminal_view::FONTS;
use iced::{Settings, Size};

const WINDOW: Size = Size::new(1200.0, 720.0);
/// Height of a window showing the whole form.
const TALL_HEIGHT: f32 = 1280.0;

fn app(dir: &Path) -> App {
    App::new(AppConfig {
        profiles_file: dir.join("profiles.toml"),
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

#[test]
fn a_web_form_asks_for_the_address_and_an_account_never_a_password() {
    let dir = tempfile::tempdir().expect("dir");
    let mut shell = Shell::with_app(app(dir.path()));
    let _ = shell.update(Message::App(AppMessage::NewProfile));
    {
        let mut ui = common::simulator(
            settings(),
            Size::new(WINDOW.width, TALL_HEIGHT),
            shell.view(),
        );
        ui.find("Web console (iLO, iDRAC, switch)")
            .expect("Web card");
    }
    let _ = shell.update(Message::App(AppMessage::ChooseProtocol(DraftProtocol::Web)));
    let _ = shell.update(Message::App(AppMessage::ProfileField {
        field: ProfileField::Host,
        value: "ilo.lab".to_owned(),
    }));
    let mut ui = common::simulator(
        settings(),
        Size::new(WINDOW.width, TALL_HEIGHT),
        shell.view(),
    );
    for label in [
        "Web address",
        "Use plain HTTP (unencrypted)",
        "Path (optional)",
        "https://ilo.lab/",
        "HTTP sign-in",
        "Network",
    ] {
        ui.find(label).expect(label);
    }
    assert!(ui.find("Password").is_err(), "the browser asks for it");
    drop(ui);
    let _ = shell.update(Message::ProfileTab(ProfileTab::Options));
    let mut ui = common::simulator(
        settings(),
        Size::new(WINDOW.width, TALL_HEIGHT),
        shell.view(),
    );
    ui.find("The console opens in this computer's default browser, never inside Heimdall.")
        .expect("opening card");
}

#[test]
fn the_status_bar_says_the_browser_opened_the_console_and_what_it_names() {
    let said = |notice: Notice| status_text(&SessionStatus::Ready, Some(&notice), 0);
    assert_eq!(
        said(Notice::WebOpened("iLO".to_owned())),
        "Web console iLO opened in the browser."
    );
    let through = said(Notice::WebOpenedThrough {
        name: "iLO".to_owned(),
        port: 50_123,
    });
    assert!(through.contains("port 50123"), "{through}");
    assert!(through.contains("certificate"), "{through}");
    assert!(
        said(Notice::WebRefused(WebRefusal::InvalidAddress)).contains("do not form a web address")
    );
}
