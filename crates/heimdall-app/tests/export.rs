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

//! "Export Sessions", as the C# Heimdall's: every profile in its session document, then how
//! the file went.

use std::path::Path;

use heimdall_app::{App, AppConfig, Dialog, Effect, ExportOutcome, Message};
use heimdall_core::profile::{LocalArguments, LocalCommand, LocalProfile, ProfileId, SshProfile};
use heimdall_core::store::ProfileStore;
use heimdall_ssh::AgentSource;
use heimdall_term::GridSize;

fn app(dir: &Path) -> App {
    let profiles_file = dir.join("profiles.toml");
    let mut store = ProfileStore::open(&profiles_file).expect("store");
    store.merge([SshProfile {
        id: ProfileId::new("web"),
        name: "Web".to_owned(),
        group: None,
        host: "web.lab".to_owned(),
        port: 22,
        username: None,
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
    // Arguments listed, as a profile made on Linux keeps them.
    store.merge_local([LocalProfile {
        id: ProfileId::new("tool"),
        name: "Tool".to_owned(),
        group: None,
        command: LocalCommand {
            program: Some(r"C:\Tools\tool.exe".to_owned()),
            arguments: LocalArguments::List(vec!["-a".to_owned(), "two words".to_owned()]),
            working_directory: None,
        },
        approved: None,
        session_logging: None,
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

#[test]
fn export_asks_to_save_the_csharp_document_of_every_profile() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path());
    let effects = app.update(Message::ExportSessions);
    let [Effect::SaveExport { document, count }] = effects.as_slice() else {
        panic!("{effects:?}");
    };
    assert_eq!(*count, 2);
    let document: serde_json::Value = serde_json::from_str(document).expect("JSON");
    let servers = document["servers"].as_array().expect("servers");
    assert_eq!(servers.len(), 2);
    let tool = servers
        .iter()
        .find(|server| server["id"] == "tool")
        .expect("the local shell");
    assert_eq!(
        tool["localShellArguments"], r#"-a "two words""#,
        "quoted as the terminal runs them"
    );
}

#[test]
fn the_outcome_is_said_as_the_csharp_says_it() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path());
    app.update(Message::ExportFinished(ExportOutcome::Saved(2)));
    assert!(matches!(app.dialog, Some(Dialog::ExportDone { count: 2 })));
    app.update(Message::DismissDialog);

    app.update(Message::ExportFinished(ExportOutcome::Failed(
        "access denied".to_owned(),
    )));
    assert!(matches!(
        &app.dialog,
        Some(Dialog::ExportFailed { detail }) if detail == "access denied"
    ));
    app.update(Message::DismissDialog);

    app.update(Message::ExportFinished(ExportOutcome::Cancelled));
    assert!(app.dialog.is_none(), "a closed dialog says nothing");
}
