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

//! `WinRM` tabs: a local `PowerShell` entering the remote session the profile describes, or a
//! tab saying why nothing was started.

use std::path::{Path, PathBuf};

use heimdall_app::local_driver::LocalShell;
use heimdall_app::{App, AppConfig, Effect, Message, Phase, UiError};
use heimdall_core::profile::{DEFAULT_WINRM_HTTP_PORT, ProfileId, WinRmProfile};
use heimdall_core::store::ProfileStore;
use heimdall_core::winrm::{POWERSHELL_ARGUMENTS, enter_session};
use heimdall_ssh::AgentSource;
use heimdall_term::GridSize;
use heimdall_term::local::LocalArguments;

fn profiles_file(dir: &Path) -> PathBuf {
    dir.join("profiles.toml")
}

fn app(dir: &Path, profile: &WinRmProfile) -> App {
    let mut store = ProfileStore::open(profiles_file(dir)).expect("store");
    store.merge_winrm([profile.clone()]);
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

fn profile(host: &str, username: Option<&str>) -> WinRmProfile {
    WinRmProfile {
        id: ProfileId::new("w"),
        name: "DC".to_owned(),
        group: None,
        host: host.to_owned(),
        port: DEFAULT_WINRM_HTTP_PORT,
        use_ssl: false,
        skip_certificate_check: false,
        username: username.map(str::to_owned),
    }
}

fn open(app: &mut App) -> Vec<Effect> {
    app.update(Message::OpenWinRm(ProfileId::new("w")))
}

fn started(effects: &[Effect]) -> Option<LocalShell> {
    effects.iter().find_map(|effect| match effect {
        Effect::ConnectLocal { request, .. } => Some(request.shell.clone()),
        _ => None,
    })
}

#[test]
fn a_profile_runs_powershell_entering_its_session() {
    let dir = tempfile::tempdir().expect("dir");
    let saved = profile("dc01.lab", Some("LAB\\admin"));
    let mut app = app(dir.path(), &saved);
    let shell = started(&open(&mut app)).expect("a shell is started");
    assert!(app.dialog.is_none(), "{:?}", app.dialog);
    assert_eq!(shell.name, "DC");
    let program = PathBuf::from(shell.program.expect("a program"));
    let stem = program
        .file_stem()
        .and_then(|stem| stem.to_str())
        .map(str::to_ascii_lowercase);
    assert!(
        matches!(stem.as_deref(), Some("pwsh" | "powershell")),
        "{}",
        program.display()
    );
    let mut expected: Vec<String> = POWERSHELL_ARGUMENTS.map(str::to_owned).to_vec();
    expected.push(enter_session(&saved).expect("valid"));
    assert_eq!(shell.arguments, LocalArguments::List(expected));
    assert_eq!(app.tabs.len(), 1);
    assert_eq!(app.tabs[0].phase, Phase::Connecting);
}

#[test]
fn a_host_that_cannot_be_written_into_the_command_starts_nothing() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path(), &profile("h'; calc; '", None));
    let effects = open(&mut app);
    assert!(started(&effects).is_none(), "{effects:?}");
    assert_eq!(app.tabs.len(), 1, "a tab says why");
    assert_eq!(app.tabs[0].phase, Phase::Failed(UiError::InvalidHost));
    assert_eq!(app.active, Some(app.tabs[0].id));
}

#[test]
fn an_account_name_that_cannot_be_written_into_the_command_starts_nothing() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path(), &profile("h", Some("a\"b")));
    let effects = open(&mut app);
    assert!(started(&effects).is_none(), "{effects:?}");
    assert_eq!(app.tabs[0].phase, Phase::Failed(UiError::InvalidUsername));
}

#[test]
fn an_unknown_profile_opens_nothing() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path(), &profile("h", None));
    let effects = app.update(Message::OpenWinRm(ProfileId::new("gone")));
    assert!(effects.is_empty());
    assert!(app.tabs.is_empty());
}
