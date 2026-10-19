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

//! The gateways a network tool can be routed through: listed as the C# lists them, and a
//! route found only for a gateway that is there, so that a tool whose gateway is gone fails
//! instead of running direct.

use std::path::Path;

use heimdall_app::{App, AppConfig, SystemCredentials};
use heimdall_core::profile::{ProfileId, SshGateway};
use heimdall_core::store::ProfileStore;
use heimdall_ssh::AgentSource;
use heimdall_term::GridSize;

fn gateway(id: &str, parent: Option<&str>) -> SshGateway {
    SshGateway {
        id: ProfileId::new(id),
        name: format!("{id} gateway"),
        host: format!("{id}.lab"),
        port: 2222,
        username: Some("jump".to_owned()),
        key_path: None,
        parent: parent.map(ProfileId::new),
    }
}

fn app(dir: &Path, gateways: Vec<SshGateway>) -> App {
    let profiles_file = dir.join("profiles.toml");
    let mut store = ProfileStore::open(&profiles_file).expect("store");
    store.merge_gateways(gateways);
    store.save().expect("save");
    App::new(AppConfig {
        profiles_file,
        known_hosts: dir.join("known_hosts"),
        legacy_dir: None,
        agent: AgentSource::Disabled,
        initial_grid: GridSize { cols: 80, rows: 24 },
        files_start: dir.to_owned(),
        system_credentials: SystemCredentials::memory(),
    })
}

#[test]
fn the_gateways_are_listed_as_name_and_endpoint() {
    let dir = tempfile::tempdir().expect("dir");
    let app = app(dir.path(), vec![gateway("edge", None)]);
    let choices = app.tool_gateways();
    assert_eq!(choices.len(), 1);
    assert_eq!(choices[0].id, ProfileId::new("edge"));
    assert_eq!(choices[0].label, "edge gateway (edge.lab:2222)");
}

#[test]
fn the_route_of_a_chained_gateway_is_its_parents_then_it() {
    let dir = tempfile::tempdir().expect("dir");
    let app = app(
        dir.path(),
        vec![gateway("edge", None), gateway("inner", Some("edge"))],
    );
    let route = app.tool_route(&ProfileId::new("inner")).expect("a route");
    let hosts: Vec<&str> = route.hops().iter().map(|hop| hop.host.as_str()).collect();
    assert_eq!(hosts, ["edge.lab", "inner.lab"], "nearest first");
}

#[test]
fn a_gateway_that_is_not_there_has_no_route() {
    let dir = tempfile::tempdir().expect("dir");
    let app = app(dir.path(), vec![gateway("edge", None)]);
    assert!(app.tool_route(&ProfileId::new("gone")).is_none());
}

#[test]
fn a_loop_of_gateways_has_no_route() {
    let dir = tempfile::tempdir().expect("dir");
    let app = app(
        dir.path(),
        vec![gateway("a", Some("b")), gateway("b", Some("a"))],
    );
    assert!(app.tool_route(&ProfileId::new("a")).is_none());
}
