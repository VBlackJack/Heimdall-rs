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

//! The gateway dialog's "Test route", as the C# card: the unsaved form tested with the
//! gateways it is reached through, each signing in with what is its own, by its place on the
//! route; one test at a time, stopped when the dialog goes.

use std::path::Path;
use std::time::Duration;

use heimdall_app::gateway_draft::{RouteProblem, RouteTest, TargetField};
use heimdall_app::profile_draft::ProfileField;
use heimdall_app::route_test::RouteTestRequest;
use heimdall_app::{App, AppConfig, Dialog, Effect, Message, SystemCredentials};
use heimdall_ssh::{AgentSource, Outcome, Secret, Step, StepOf};
use heimdall_term::GridSize;
use tokio_util::sync::CancellationToken;

const SAVED: &str = "saved pw";
const TYPED: &str = "typed pw";

fn app(dir: &Path) -> App {
    App::new(AppConfig {
        profiles_file: dir.join("profiles.toml"),
        known_hosts: dir.join("known_hosts"),
        legacy_dir: None,
        agent: AgentSource::Disabled,
        initial_grid: GridSize { cols: 80, rows: 24 },
        files_start: dir.to_owned(),
        system_credentials: SystemCredentials::memory(),
    })
}

fn fields(app: &mut App, name: &str, host: &str) {
    for (field, value) in [
        (ProfileField::Name, name),
        (ProfileField::Host, host),
        (ProfileField::Username, "jump"),
    ] {
        app.update(Message::GatewayField {
            field,
            value: value.to_owned(),
        });
    }
}

/// Saves gateway `name` at `host`, with `SAVED` as its password.
fn save(app: &mut App, name: &str, host: &str) {
    app.update(Message::NewGateway);
    fields(app, name, host);
    app.update(Message::SaveGateway {
        password: Some(Secret::new(SAVED.to_owned())),
        passphrase: None,
    });
    assert!(app.dialog.is_none(), "{:?}", app.dialog);
}

fn test(app: &mut App, typed: Option<&str>) -> (u64, RouteTestRequest) {
    let effects = app.update(Message::TestRoute {
        password: typed.map(|typed| Secret::new(typed.to_owned())),
        passphrase: None,
    });
    match effects.into_iter().next() {
        Some(Effect::TestRoute { run, request }) => (run, *request),
        other => panic!("{other:?}"),
    }
}

fn shown(app: &App) -> RouteTest {
    match &app.dialog {
        Some(Dialog::EditGateway { draft, .. }) => draft.route_test.clone(),
        other => panic!("{other:?}"),
    }
}

/// The password each hop of `request` signs in with.
fn passwords(request: &RouteTestRequest) -> Vec<Option<String>> {
    request
        .secrets
        .iter()
        .map(|secrets| {
            secrets
                .password
                .as_ref()
                .map(|secret| secret.expose().to_owned())
        })
        .collect()
}

fn step(index: usize) -> Step {
    Step {
        of: StepOf::Gateway(index),
        outcome: Outcome::Passed,
        elapsed: Duration::from_millis(5),
    }
}

#[test]
fn the_unsaved_form_is_tested_behind_its_parent_and_each_signs_in_with_its_own() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path());
    save(&mut app, "outer", "outer.lab");
    let outer = app.gateways()[0].id.clone();
    app.update(Message::NewGateway);
    fields(&mut app, "edge", "edge.lab");
    app.update(Message::ChooseParentGateway(Some(outer)));
    app.update(Message::RouteTarget {
        field: TargetField::Host,
        value: "wiki.lab".to_owned(),
    });
    app.update(Message::RouteTarget {
        field: TargetField::Port,
        value: "443".to_owned(),
    });
    let (run, request) = test(&mut app, Some(TYPED));
    let hosts: Vec<&str> = request.hops.iter().map(|hop| hop.host.as_str()).collect();
    assert_eq!(
        hosts,
        ["outer.lab", "edge.lab"],
        "nearest first, the form last"
    );
    assert_eq!(
        passwords(&request),
        [Some(SAVED.to_owned()), Some(TYPED.to_owned())]
    );
    assert_eq!(request.destination, Some(("wiki.lab".to_owned(), 443)));
    assert_eq!(shown(&app), RouteTest::Running(Vec::new()));

    // Not saved while it runs: what is tested is what is saved.
    app.update(Message::SaveGateway {
        password: None,
        passphrase: None,
    });
    assert!(matches!(app.dialog, Some(Dialog::EditGateway { .. })));

    app.update(Message::RouteStep { run, step: step(0) });
    app.update(Message::RouteStep { run, step: step(1) });
    assert_eq!(shown(&app), RouteTest::Running(vec![step(0), step(1)]));
    app.update(Message::RouteTestDone { run });
    assert!(matches!(shown(&app), RouteTest::Done { steps, .. } if steps.len() == 2));

    // A password changed: what was found no longer holds.
    app.update(Message::ForgetRouteTest);
    assert_eq!(shown(&app), RouteTest::Idle);
}

#[test]
fn a_saved_password_never_goes_to_an_address_just_typed_nor_once_cleared() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path());
    save(&mut app, "edge", "edge.lab");
    let id = app.gateways()[0].id.clone();

    app.update(Message::EditGateway(id.clone()));
    let (_, request) = test(&mut app, None);
    assert_eq!(
        passwords(&request),
        [Some(SAVED.to_owned())],
        "its own endpoint"
    );
    app.update(Message::DismissDialog);

    app.update(Message::EditGateway(id.clone()));
    app.update(Message::GatewayField {
        field: ProfileField::Host,
        value: "elsewhere.lab".to_owned(),
    });
    let (_, request) = test(&mut app, None);
    assert_eq!(passwords(&request), [None], "an address just typed");
    app.update(Message::DismissDialog);

    app.update(Message::EditGateway(id));
    app.update(Message::ClearGatewayPassword);
    let (_, request) = test(&mut app, None);
    assert_eq!(passwords(&request), [None], "cleared in the dialog");
}

#[test]
fn a_wrong_form_or_destination_is_refused_and_a_closed_dialog_stops_its_test() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path());
    app.update(Message::NewGateway);
    assert!(
        app.update(Message::TestRoute {
            password: None,
            passphrase: None
        })
        .is_empty()
    );
    assert_eq!(
        shown(&app),
        RouteTest::Refused(RouteProblem::Route),
        "no host"
    );

    fields(&mut app, "edge", "edge.lab");
    app.update(Message::RouteTarget {
        field: TargetField::Host,
        value: "wiki.lab".to_owned(),
    });
    app.update(Message::RouteTarget {
        field: TargetField::Port,
        value: "0".to_owned(),
    });
    assert!(
        app.update(Message::TestRoute {
            password: None,
            passphrase: None
        })
        .is_empty()
    );
    assert_eq!(shown(&app), RouteTest::Refused(RouteProblem::Target));

    app.update(Message::RouteTarget {
        field: TargetField::Host,
        value: String::new(),
    });
    let (run, request) = test(&mut app, None);
    let cancel: CancellationToken = request.cancel.clone();
    app.update(Message::DismissDialog);
    // The next message finds the test without its dialog: stopped.
    app.update(Message::RouteStep { run, step: step(0) });
    assert!(cancel.is_cancelled(), "its sessions do not linger");
}
