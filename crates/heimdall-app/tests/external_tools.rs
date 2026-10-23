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

//! The user's external tools, as the C# "External Tools": kept in the settings, started for
//! a session with its values each inside its own argument, found in Quick Connect, and what
//! the status bar says of a start.

use std::path::{Path, PathBuf};

use heimdall_app::external_tool::{Launched, ToolFailure};
use heimdall_app::tools::ToolId;
use heimdall_app::{
    App, AppConfig, Effect, ExternalToolMessage, Message, Notice, QuickGroup, QuickResult,
    SettingsMessage, ToolWords,
};
use heimdall_core::external_tools::{ExternalTool, Placeholder};
use heimdall_core::profile::{ProfileId, SshGateway, SshProfile};
use heimdall_core::settings::{Settings, settings_path};
use heimdall_core::store::ProfileStore;
use heimdall_ssh::AgentSource;
use heimdall_term::GridSize;

/// A program that is there on every computer the tests run on: the test itself.
fn present() -> String {
    std::env::current_exe()
        .expect("the test itself")
        .to_string_lossy()
        .into_owned()
}

fn ssh(id: &str, name: &str, host: &str, username: &str) -> SshProfile {
    SshProfile {
        id: ProfileId::new(id),
        name: name.to_owned(),
        group: None,
        host: host.to_owned(),
        port: 2222,
        username: Some(username.to_owned()),
        key_path: Some(PathBuf::from(r"C:\Keys\id_ed25519")),
        gateway: Some(ProfileId::new("jump")),
        local_tunnel_port: None,
        vault_entry: None,
        forwards: heimdall_core::profile::Forwards::default(),
        post_connect: heimdall_core::post_connect::PostConnect::default(),
        forward_agent: false,
        compression: false,
        sftp: false,
        legacy_algorithms: false,
        session_logging: None,
        ssh_mode: heimdall_core::profile::SshMode::Embedded,
        x11_forwarding: false,
        remote_os: heimdall_core::profile::RemoteOs::Auto,
    }
}

fn app_with(dir: &Path, profiles: Vec<SshProfile>) -> App {
    let profiles_file = dir.join("profiles.toml");
    let mut store = ProfileStore::open(&profiles_file).expect("store");
    store.merge(profiles);
    store.merge_gateways([SshGateway {
        id: ProfileId::new("jump"),
        name: "Bastion".to_owned(),
        host: "jump.lab".to_owned(),
        port: 22,
        username: Some("ops".to_owned()),
        key_path: None,
        parent: None,
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

fn tool(name: &str, program: &str, arguments: &str) -> ExternalTool {
    ExternalTool {
        name: name.to_owned(),
        executable_path: program.to_owned(),
        arguments: arguments.to_owned(),
        ..ExternalTool::default()
    }
}

/// `app` with `tools` as its external tools, saved.
fn listed(app: &mut App, tools: Vec<ExternalTool>) {
    let _ = app.update(Message::Settings(SettingsMessage::ExternalTools(tools)));
}

fn launch(name: &str, profile: Option<&str>) -> Message {
    Message::ExternalTool(ExternalToolMessage::Launch {
        name: name.to_owned(),
        profile: profile.map(ProfileId::new),
    })
}

/// The program and the arguments the one effect of `effects` starts.
fn started(effects: &[Effect]) -> (PathBuf, Vec<String>) {
    let [Effect::LaunchExternalTool { launch, .. }] = effects else {
        panic!("{effects:?}");
    };
    (launch.program.clone(), launch.arguments.clone())
}

#[test]
fn a_tool_started_for_a_session_gets_each_value_inside_its_own_argument() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app_with(dir.path(), vec![ssh("web", "Web (prod)", "web.lab", "ops")]);
    listed(
        &mut app,
        vec![tool(
            "Probe",
            &present(),
            r#"-P {Port} {User}@{Host} --name "{ServerName}" -i {KeyFile} {Protocol} {Gateway}"#,
        )],
    );
    let (program, arguments) = started(&app.update(launch("Probe", Some("web"))));
    assert_eq!(program, PathBuf::from(present()));
    assert_eq!(
        arguments,
        [
            "-P",
            "2222",
            "ops@web.lab",
            "--name",
            "Web (prod)",
            "-i",
            r"C:\Keys\id_ed25519",
            "SSH",
            "Bastion",
        ]
    );
}

#[test]
fn a_host_or_an_account_can_never_add_an_argument_nor_an_option() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app_with(
        dir.path(),
        vec![
            ssh("spaced", "Spaced", "web.lab --delete \"all\" & calc", "ops"),
            ssh("option", "Option", "-oProxyCommand=calc", "ops"),
            ssh("account", "Account", "web.lab", "-oProxyCommand=calc"),
        ],
    );
    listed(
        &mut app,
        vec![tool("Probe", &present(), "{Host} {User}@{Host}")],
    );
    let (_, arguments) = started(&app.update(launch("Probe", Some("spaced"))));
    assert_eq!(
        arguments,
        [
            "web.lab --delete all  calc",
            "ops@web.lab --delete all  calc"
        ],
        "two arguments, as the template has, the quotes and the & gone"
    );
    for (profile, placeholder) in [
        ("option", Placeholder::Host),
        ("account", Placeholder::User),
    ] {
        assert!(app.update(launch("Probe", Some(profile))).is_empty());
        assert_eq!(
            app.notice(),
            Some(&Notice::ExternalToolFailed {
                name: "Probe".to_owned(),
                failure: ToolFailure::OptionLike(placeholder),
            }),
            "{profile}"
        );
    }
}

#[test]
fn a_shell_is_refused_and_said_and_a_tool_unknown_starts_nothing() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app_with(dir.path(), vec![ssh("web", "Web", "web.lab", "ops")]);
    listed(&mut app, vec![tool("Shell", "cmd.exe", "/c ping {Host}")]);
    assert!(app.update(launch("Shell", Some("web"))).is_empty());
    assert_eq!(
        app.notice(),
        Some(&Notice::ExternalToolFailed {
            name: "Shell".to_owned(),
            failure: ToolFailure::ShellTarget,
        })
    );
    assert!(app.update(launch("Nothing", Some("web"))).is_empty());
    assert!(app.update(launch("Shell", Some("gone"))).is_empty());
}

#[test]
fn with_no_session_and_from_test_the_template_is_left_unfilled() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app_with(dir.path(), Vec::new());
    let probe = tool("Probe", &present(), "/v:{Host} \"{ServerName}\"");
    listed(&mut app, vec![probe.clone()]);
    let (_, arguments) = started(&app.update(launch("Probe", None)));
    assert_eq!(arguments, ["/v:{Host}", "{ServerName}"]);
    let mut edited = probe;
    edited.arguments = "--version".to_owned();
    let (_, arguments) =
        started(&app.update(Message::ExternalTool(ExternalToolMessage::Test(edited))));
    assert_eq!(arguments, ["--version"], "the tool as edited, not as saved");
}

#[test]
fn how_a_start_went_is_said_and_a_prompt_declined_is_not() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app_with(dir.path(), Vec::new());
    let launched = |result| {
        Message::ExternalTool(ExternalToolMessage::Launched {
            name: "Probe".to_owned(),
            result,
        })
    };
    let _ = app.update(launched(Ok(Launched::Started)));
    assert_eq!(
        app.notice(),
        Some(&Notice::ExternalToolStarted("Probe".to_owned()))
    );
    let _ = app.update(launched(Err(ToolFailure::NotStarted("denied".to_owned()))));
    assert_eq!(
        app.notice(),
        Some(&Notice::ExternalToolFailed {
            name: "Probe".to_owned(),
            failure: ToolFailure::NotStarted("denied".to_owned()),
        })
    );
    let mut quiet = app_with(dir.path(), Vec::new());
    let _ = quiet.update(launched(Ok(Launched::Declined)));
    assert_eq!(quiet.notice(), None);
}

#[test]
fn the_list_is_saved_trimmed_and_refused_as_the_csharp_save_refuses_it() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app_with(dir.path(), Vec::new());
    listed(&mut app, vec![tool(" WinSCP ", " winscp.exe ", " {Host} ")]);
    let saved = vec![tool("WinSCP", "winscp.exe", " {Host} ")];
    assert_eq!(app.external_tools(), saved.as_slice());
    let on_disk = Settings::load(&settings_path(&dir.path().join("profiles.toml"))).expect("read");
    assert_eq!(on_disk.external_tools, saved);
    // Two of one name, or one without a program: nothing changes.
    listed(
        &mut app,
        vec![tool("A", "a.exe", ""), tool("a", "b.exe", "")],
    );
    listed(&mut app, vec![tool("A", " ", "")]);
    assert_eq!(app.external_tools(), saved.as_slice());
}

/// The names of the tools, as the window gives them: here their identifiers.
fn words(tool: ToolId, argument: Option<&str>) -> ToolWords {
    ToolWords {
        name: argument.map_or_else(|| tool.code().to_owned(), str::to_owned),
        category: format!("{:?}", tool.category()),
    }
}

#[test]
fn quick_connect_finds_a_tool_by_its_name_and_starts_it_for_the_session_selected() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app_with(dir.path(), vec![ssh("web", "Web", "web.lab", "ops")]);
    listed(&mut app, vec![tool("WinSCP", &present(), "{Host}")]);
    let found: Vec<(QuickGroup, QuickResult)> = app
        .quick_results_in("winsc", None, &words)
        .into_iter()
        .map(|row| (row.group, row.result))
        .collect();
    let external = QuickResult::ExternalTool {
        name: "WinSCP".to_owned(),
    };
    assert_eq!(found, [(QuickGroup::ExternalTools, external.clone())]);
    // No session selected: the template as written, as the C# palette.
    assert_eq!(
        launched_for(app.quick_open(external.clone(), None, false)),
        ("WinSCP".to_owned(), None)
    );
    let _ = app.update(Message::SelectProfile(ProfileId::new("web")));
    // Ctrl+Enter starts it too: a tool is never split.
    let open = app.quick_open(external, None, true);
    assert_eq!(
        launched_for(open.clone()),
        ("WinSCP".to_owned(), Some(ProfileId::new("web")))
    );
    let (_, arguments) = started(&app.update(open));
    assert_eq!(arguments, ["web.lab"]);
}

/// The tool and the session `message` starts it for.
fn launched_for(message: Message) -> (String, Option<ProfileId>) {
    match message {
        Message::ExternalTool(ExternalToolMessage::Launch { name, profile }) => (name, profile),
        other => panic!("{other:?}"),
    }
}
