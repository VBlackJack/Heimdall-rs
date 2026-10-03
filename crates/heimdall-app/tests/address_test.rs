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

//! The profile form's "Test address", as the C# dialog's: the form's address dialled, one
//! test at a time, its finding kept until the address changes.

use std::path::Path;

use heimdall_app::profile_draft::{AddressTest, DraftProtocol, ProfileField};
use heimdall_app::reachability::{Reached, Unreached};
use heimdall_app::{App, AppConfig, Dialog, Effect, Message, SystemCredentials};
use heimdall_ssh::AgentSource;
use heimdall_term::GridSize;

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

/// A new form for `protocol`, at `host:port`.
fn form(app: &mut App, protocol: DraftProtocol, host: &str, port: &str) {
    app.update(Message::NewProfile);
    app.update(Message::ChooseProtocol(protocol));
    for (field, value) in [(ProfileField::Host, host), (ProfileField::Port, port)] {
        app.update(Message::ProfileField {
            field,
            value: value.to_owned(),
        });
    }
}

fn shown(app: &App) -> AddressTest {
    match &app.dialog {
        Some(Dialog::EditProfile { draft, .. }) => draft.address_test.clone(),
        other => panic!("{other:?}"),
    }
}

/// Starts a test: its number.
fn started(app: &mut App) -> u64 {
    match app.update(Message::TestAddress).as_slice() {
        [Effect::TestAddress { test, .. }] => *test,
        other => panic!("{other:?}"),
    }
}

fn answered() -> Reached {
    Reached {
        address: "192.0.2.7".to_owned(),
        millis: 3,
        banner: None,
    }
}

#[test]
fn the_form_s_address_is_dialled_with_the_ssh_banner_for_ssh_and_its_finding_kept() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path());
    form(&mut app, DraftProtocol::Ssh, " [2001:db8::1] ", "2222");
    let effects = app.update(Message::TestAddress);
    let [
        Effect::TestAddress {
            test,
            host,
            port,
            ssh,
            ..
        },
    ] = effects.as_slice()
    else {
        panic!("{effects:?}");
    };
    assert_eq!((host.as_str(), *port, *ssh), ("2001:db8::1", 2222, true));
    assert_eq!(shown(&app), AddressTest::Running);
    assert!(
        app.update(Message::TestAddress).is_empty(),
        "one test at a time"
    );
    app.update(Message::AddressTested {
        test: *test,
        result: Ok(answered()),
    });
    assert_eq!(shown(&app), AddressTest::Done(Ok(answered())));

    // About another address now: what was found goes.
    app.update(Message::ProfileField {
        field: ProfileField::Port,
        value: "22".to_owned(),
    });
    assert_eq!(shown(&app), AddressTest::Idle);
}

#[test]
fn only_the_last_test_s_finding_is_shown_and_an_rdp_address_gets_no_banner_read() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path());
    form(&mut app, DraftProtocol::Rdp, "dc.lab", "");
    let first = started(&mut app);
    // The address changed while it ran, then tested again.
    app.update(Message::ProfileField {
        field: ProfileField::Host,
        value: "dc2.lab".to_owned(),
    });
    let effects = app.update(Message::TestAddress);
    let [
        Effect::TestAddress {
            test: second,
            host,
            port,
            ssh,
            ..
        },
    ] = effects.as_slice()
    else {
        panic!("{effects:?}");
    };
    assert_eq!((host.as_str(), *port, *ssh), ("dc2.lab", 3389, false));
    app.update(Message::AddressTested {
        test: first,
        result: Ok(answered()),
    });
    assert_eq!(
        shown(&app),
        AddressTest::Running,
        "the first test's finding is stale"
    );
    app.update(Message::AddressTested {
        test: *second,
        result: Err(Unreached::DnsNoResults),
    });
    assert_eq!(shown(&app), AddressTest::Done(Err(Unreached::DnsNoResults)));
}

#[test]
fn a_test_can_be_stopped_and_a_form_without_an_address_offers_none() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path());
    form(&mut app, DraftProtocol::Ssh, "web.lab", "");
    let effects = app.update(Message::TestAddress);
    let [Effect::TestAddress { cancel, .. }] = effects.as_slice() else {
        panic!("{effects:?}");
    };
    app.update(Message::CancelAddressTest);
    assert!(cancel.is_cancelled());

    form(&mut app, DraftProtocol::Ssh, "", "");
    assert!(app.update(Message::TestAddress).is_empty(), "no address");
    form(&mut app, DraftProtocol::Ssh, "web.lab", "0");
    assert!(app.update(Message::TestAddress).is_empty(), "no port");
    app.update(Message::NewProfile);
    app.update(Message::ChooseProtocol(DraftProtocol::Local));
    assert!(app.update(Message::TestAddress).is_empty(), "a local shell");
}

#[test]
fn a_profile_s_address_is_tested_from_its_menu_and_said_in_the_status_bar() {
    use heimdall_app::{Notice, ProfileMenuMessage};
    use heimdall_core::profile::ProfileId;

    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path());
    form(&mut app, DraftProtocol::Ssh, "web.lab", "2200");
    app.update(Message::ProfileField {
        field: ProfileField::Name,
        value: "web".to_owned(),
    });
    app.update(Message::SaveProfile {
        password: None,
        passphrase: None,
    });
    let id: ProfileId = app.profile_summaries()[0].id.clone();
    let effects = app.update(Message::ProfileMenu(ProfileMenuMessage::TestReachability(
        id,
    )));
    assert!(
        matches!(
            effects.as_slice(),
            [Effect::TestReachability { host, port: 2200 }] if host == "web.lab"
        ),
        "{effects:?}"
    );
    assert_eq!(
        app.notice(),
        Some(&Notice::ReachabilityTesting {
            host: "web.lab".to_owned(),
            port: 2200
        })
    );
    app.update(Message::ProfileMenu(ProfileMenuMessage::Tested {
        host: "web.lab".to_owned(),
        port: 2200,
        result: Err(Unreached::DnsNoResults),
    }));
    assert_eq!(
        app.notice(),
        Some(&Notice::Unreachable {
            host: "web.lab".to_owned(),
            port: 2200,
            failure: Unreached::DnsNoResults,
        })
    );
}
