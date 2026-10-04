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

//! `WinRM` tabs: a local `PowerShell` entering the remote session the profile describes and
//! ending with it, or a tab saying why nothing was started.

use std::path::{Path, PathBuf};
use std::time::Duration;

use heimdall_app::winrm_driver::{WinRmRequest, shell};
use heimdall_app::{App, AppConfig, Effect, Message, Phase, UiError};
use heimdall_core::profile::{DEFAULT_WINRM_HTTP_PORT, ProfileId, WinRmProfile};
use heimdall_core::store::ProfileStore;
use heimdall_core::winrm::{
    POWERSHELL_ARGUMENTS, REMOTE_SESSION_ENDED_EXIT_CODE, REMOTE_SESSION_NOT_ENTERED_EXIT_CODE,
    session_command,
};
use heimdall_ssh::AgentSource;
use heimdall_term::GridSize;
use heimdall_term::local::{self, LocalArguments, LocalConfig, LocalEvent};

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
        gateway: None,
    }
}

fn open(app: &mut App) -> Vec<Effect> {
    app.update(Message::OpenWinRm(ProfileId::new("w")))
}

/// The attempt `effects` start: the server probed, then `PowerShell`.
fn started(effects: &[Effect]) -> Option<WinRmRequest> {
    effects.iter().find_map(|effect| match effect {
        Effect::ConnectWinRm { request, .. } => Some(WinRmRequest::clone(request)),
        _ => None,
    })
}

#[test]
fn a_profile_runs_powershell_entering_its_session() {
    let dir = tempfile::tempdir().expect("dir");
    let saved = profile("dc01.lab", Some("LAB\\admin"));
    let mut app = app(dir.path(), &saved);
    let request = started(&open(&mut app)).expect("an attempt is started");
    assert!(app.dialog.is_none(), "{:?}", app.dialog);
    assert!(request.route.is_empty(), "reached directly");
    assert_eq!(request.profile, saved);
    let shell = shell(request.profile.name.clone(), request.program.clone(), {
        session_command(&request.profile).expect("valid")
    });
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
    expected.push(session_command(&saved).expect("valid"));
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

/// Bound on one launch, paid only when `PowerShell` fails to end: it then sits at a prompt.
const EXIT_DEADLINE: Duration = Duration::from_secs(60);

/// Written by the double that stands for a session entered then ended.
const ENTERED_MARKER: &str = "HEIMDALL-TEST-ENTERED";

/// The parameters `Enter-PSSession` is given, so that a double of the same name binds them.
const DOUBLE_PARAMETERS: &str = "[CmdletBinding()] param($ComputerName, $Port, $Authentication, \
     [switch]$UseSSL, $SessionOption, $Credential)";

/// A refused connection: a NON-terminating error, as the real cmdlet writes it, so the
/// command's own `-ErrorAction Stop` is what ends it. No cmdlet is called, so a module path
/// inherited from another `PowerShell` cannot change the outcome.
fn failing_double() -> String {
    format!(
        "function Enter-PSSession {{ {DOUBLE_PARAMETERS} $PSCmdlet.WriteError(\
         [System.Management.Automation.ErrorRecord]::new(\
         [System.Exception]::new('simulated WinRM connection failure'), 'Simulated', \
         'ConnectionError', $null)) }}"
    )
}

/// A session entered and already ended: nothing is pushed, so the next prompt is local at
/// once, the state a remote `exit` or a dropped connection leaves.
fn returning_double() -> String {
    format!(
        "function Enter-PSSession {{ {DOUBLE_PARAMETERS} $Host.UI.WriteLine('{ENTERED_MARKER}') }}"
    )
}

/// The programs a `WinRM` tab can run here: the one the app picks and, on Windows, the
/// Windows `PowerShell` it falls back to. Empty on a Unix without `pwsh`.
fn powershells(picked: &str) -> Vec<String> {
    let mut programs = Vec::new();
    if local::program_path(Some(picked)).is_ok() {
        programs.push(picked.to_owned());
    }
    #[cfg(windows)]
    {
        let system = local::program_path(None)
            .expect("Windows PowerShell")
            .to_string_lossy()
            .into_owned();
        if !programs.contains(&system) {
            programs.push(system);
        }
    }
    programs
}

/// Runs `program` with the arguments the app gave, the double defined ahead of the command;
/// the output and the exit code.
async fn run_with_double(
    program: &str,
    arguments: &[String],
    double: &str,
) -> (String, Option<i32>) {
    let mut arguments = arguments.to_vec();
    let command = arguments.pop().expect("the command is last");
    arguments.push(format!("{double}; {command}"));
    let mut session = local::spawn(&LocalConfig {
        program: Some(program.to_owned()),
        arguments: LocalArguments::List(arguments),
        working_directory: None,
        environment: Vec::new(),
        columns: 80,
        rows: 24,
    })
    .expect("spawned");
    let mut output = Vec::new();
    let ended = tokio::time::timeout(EXIT_DEADLINE, async {
        while let Some(event) = session.events.recv().await {
            match event {
                LocalEvent::Output(bytes) => output.extend(bytes),
                LocalEvent::Exited(code) => return Some(code),
            }
        }
        None
    })
    .await;
    let output = String::from_utf8_lossy(&output).into_owned();
    match ended {
        Ok(Some(code)) => (output, code),
        Ok(None) => panic!("{program}: the session ended without an exit: {output:?}"),
        Err(_) => {
            session.input.close();
            panic!(
                "{program} still runs {EXIT_DEADLINE:?} after start: it sits at a local \
                 prompt. Output: {output:?}"
            )
        }
    }
}

#[tokio::test]
async fn powershell_ends_at_its_first_local_prompt() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path(), &profile("dc01.lab", Some("LAB\\o'neil")));
    let request = started(&open(&mut app)).expect("an attempt is started");
    let command = session_command(&request.profile).expect("valid");
    let shell = shell(request.profile.name, request.program, command);
    let LocalArguments::List(arguments) = shell.arguments else {
        panic!("arguments one by one");
    };
    let programs = powershells(&shell.program.expect("a program"));
    if programs.is_empty() {
        eprintln!("no PowerShell here: the local prompt guard is not run");
        return;
    }
    for program in programs {
        let (output, code) = run_with_double(&program, &arguments, &failing_double()).await;
        assert_eq!(
            code,
            Some(REMOTE_SESSION_NOT_ENTERED_EXIT_CODE),
            "{program}, never entered: {output:?}"
        );
        assert!(!output.contains(ENTERED_MARKER), "{program}: {output:?}");

        let (output, code) = run_with_double(&program, &arguments, &returning_double()).await;
        assert_eq!(
            code,
            Some(REMOTE_SESSION_ENDED_EXIT_CODE),
            "{program}, entered then ended: {output:?}"
        );
        assert!(output.contains(ENTERED_MARKER), "{program}: {output:?}");
    }
}
