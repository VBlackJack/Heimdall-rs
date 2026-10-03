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

//! The profile form's SSH agent chip, as the C# one: the agents asked when an SSH form
//! opens, and again on a click; never for a form whose sign-in no agent serves.

use std::path::Path;

use heimdall_app::profile_draft::DraftProtocol;
use heimdall_app::{AgentChip, App, AppConfig, Effect, Message, SystemCredentials};
use heimdall_ssh::{AgentSource, AgentSurvey};
use heimdall_term::GridSize;

fn app(dir: &Path) -> App {
    App::new(AppConfig {
        profiles_file: dir.join("profiles.toml"),
        known_hosts: dir.join("known_hosts"),
        legacy_dir: None,
        agent: AgentSource::Auto,
        initial_grid: GridSize { cols: 80, rows: 24 },
        files_start: dir.to_owned(),
        system_credentials: SystemCredentials::memory(),
    })
}

fn asks(effects: &[Effect]) -> bool {
    matches!(effects, [Effect::SurveyAgents(AgentSource::Auto)])
}

#[test]
fn an_ssh_form_asks_the_agents_once_then_again_on_a_click_and_an_rdp_one_never() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path());
    app.update(Message::NewProfile);
    assert!(
        asks(&app.update(Message::ChooseProtocol(DraftProtocol::Ssh))),
        "asked when the SSH form shows"
    );
    assert_eq!(app.agent_chip(), &AgentChip::Surveying);
    let found = vec![AgentSurvey {
        name: "Pageant".to_owned(),
        keys: 2,
    }];
    app.update(Message::AgentsSurveyed(found.clone()));
    assert_eq!(app.agent_chip(), &AgentChip::Found(found));
    assert!(
        app.update(Message::ChooseProtocol(DraftProtocol::Sftp))
            .is_empty(),
        "known already"
    );
    assert!(
        asks(&app.update(Message::RefreshAgents)),
        "a click asks again"
    );
    assert!(
        app.update(Message::RefreshAgents).is_empty(),
        "not while asking"
    );

    app.update(Message::NewProfile);
    assert!(
        app.update(Message::ChooseProtocol(DraftProtocol::Rdp))
            .is_empty(),
        "no agent signs an RDP session in"
    );
}
