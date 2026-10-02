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

//! The external credential provider asked at connect, as the C# one: only when no password
//! is saved, only by the tab's own server, once per attempt; the user asked when it gives
//! nothing.

use std::path::Path;

use heimdall_app::credential_provider::{Provided, ProviderFailure};
use heimdall_app::{
    Answer, App, AppConfig, AttemptId, ConnectionEvent, Effect, Message, Notice, ProviderAnswer,
    ProviderMessage, ProviderRequest, QuestionId, QuestionKind, SystemCredentials, TabId,
};
use heimdall_core::profile::{ProfileId, SshProfile};
use heimdall_core::store::ProfileStore;
use heimdall_ssh::{AgentSource, PasswordQuestion, Secret};
use heimdall_term::GridSize;

fn app(dir: &Path) -> App {
    let profiles_file = dir.join("profiles.toml");
    let mut store = ProfileStore::open(&profiles_file).expect("store");
    store.merge([SshProfile {
        id: ProfileId::new("a"),
        name: "Web server".to_owned(),
        group: None,
        host: "a.lab".to_owned(),
        port: 22,
        username: Some("admin".to_owned()),
        key_path: None,
        gateway: None,
        vault_entry: None,
        forwards: heimdall_core::profile::Forwards::default(),
        post_connect: heimdall_core::post_connect::PostConnect::default(),
        forward_agent: false,
        compression: false,
        sftp: false,
    }]);
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

/// The application with the provider on, running `command`.
fn with_provider(dir: &Path, command: &str) -> App {
    let mut app = app(dir);
    app.update(Message::CredentialProvider(ProviderMessage::Enabled(true)));
    app.update(Message::CredentialProvider(ProviderMessage::Command(
        command.to_owned(),
    )));
    app
}

fn open(app: &mut App) -> (TabId, AttemptId) {
    match app
        .update(Message::OpenProfile(ProfileId::new("a")))
        .as_slice()
    {
        [Effect::Connect { tab, attempt, .. }] => (*tab, *attempt),
        other => panic!("expected one Connect, got {other:?}"),
    }
}

fn password(host: &str, attempt: u32) -> QuestionKind {
    QuestionKind::Password(PasswordQuestion {
        host: host.to_owned(),
        port: 22,
        username: "admin".to_owned(),
        attempt,
    })
}

/// Asks `kind` in the tab; the provider's request when it is asked.
fn ask(
    app: &mut App,
    tab: TabId,
    attempt: AttemptId,
    kind: QuestionKind,
) -> (QuestionId, Option<ProviderRequest>) {
    let question = QuestionId::fresh();
    let effects = app.update(Message::Connection {
        tab,
        attempt,
        event: ConnectionEvent::Question { question, kind },
    });
    match <[Effect; 1]>::try_from(effects) {
        Ok([Effect::AskCredentialProvider(request)]) => (question, Some(*request)),
        Ok(other) => panic!("{other:?}"),
        Err(effects) => {
            assert!(effects.is_empty(), "{effects:?}");
            (question, None)
        }
    }
}

fn shown(app: &App, tab: TabId, question: QuestionId) -> bool {
    app.tab(tab)
        .expect("tab")
        .prompts
        .iter()
        .any(|prompt| prompt.question == question)
}

fn answer(request: &ProviderRequest, result: Result<Provided, ProviderFailure>) -> Message {
    Message::CredentialProvided(Box::new(ProviderAnswer {
        tab: request.tab,
        attempt: request.attempt,
        question: request.question,
        kind: request.kind.clone(),
        result,
    }))
}

fn given(password: &str) -> Provided {
    Provided {
        password: Secret::new(password.to_owned()),
        username: None,
    }
}

#[test]
fn off_the_provider_is_never_asked() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path());
    let (tab, attempt) = open(&mut app);
    let (question, request) = ask(&mut app, tab, attempt, password("a.lab", 1));
    assert!(request.is_none());
    assert!(shown(&app, tab, question), "the user is asked");
}

#[test]
fn the_server_s_password_is_asked_with_the_profile_s_values() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = with_provider(dir.path(), "get {Title}");
    app.update(Message::CredentialProvider(
        ProviderMessage::SaveUnlockSecret(Secret::new("db-pass".to_owned())),
    ));
    let (tab, attempt) = open(&mut app);
    let (question, request) = ask(&mut app, tab, attempt, password("a.lab", 1));
    let request = request.expect("asked");
    assert_eq!((request.tab, request.question), (tab, question));
    assert_eq!(request.lookup.host, "a.lab");
    assert_eq!(request.lookup.port, 22);
    assert_eq!(request.lookup.user.as_deref(), Some("admin"));
    assert_eq!(
        request.lookup.title, "Web server",
        "the C# fallback: its name"
    );
    assert_eq!(request.settings.command, "get {Title}");
    assert_eq!(request.unlock.as_ref().map(Secret::expose), Some("db-pass"));
    assert!(
        !shown(&app, tab, question),
        "not asked of the user meanwhile"
    );

    let effects = app.update(answer(&request, Ok(given("s3cret"))));
    let [
        Effect::Answer {
            question: answered,
            answer: Some(Answer::Secret(secret)),
        },
    ] = effects.as_slice()
    else {
        panic!("{effects:?}");
    };
    assert_eq!((*answered, secret.expose()), (question, "s3cret"));
}

#[test]
fn nothing_given_the_user_is_asked_and_told_why() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = with_provider(dir.path(), "get {Title}");
    for (failure, notice) in [
        (
            ProviderFailure::Empty,
            Notice::ProviderNoPassword("Web server".to_owned()),
        ),
        (
            ProviderFailure::Exit(Some(1)),
            Notice::ProviderNoPassword("Web server".to_owned()),
        ),
        (
            ProviderFailure::Launch("not found".to_owned()),
            Notice::ProviderFailed("not found".to_owned()),
        ),
        (ProviderFailure::TimedOut, Notice::ProviderTimedOut),
    ] {
        let (tab, attempt) = open(&mut app);
        let (question, request) = ask(&mut app, tab, attempt, password("a.lab", 1));
        let effects = app.update(answer(&request.expect("asked"), Err(failure)));
        assert!(effects.is_empty(), "{effects:?}");
        assert!(shown(&app, tab, question), "the user is asked");
        assert_eq!(app.notice(), Some(&notice));
    }
}

#[test]
fn a_saved_password_wins_over_the_provider() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = with_provider(dir.path(), "get {Title}");
    app.update(Message::EditProfile(ProfileId::new("a")));
    app.update(Message::SaveProfile {
        password: Some(Secret::new("saved-pw".to_owned())),
        passphrase: None,
    });
    let (tab, attempt) = open(&mut app);
    let question = QuestionId::fresh();
    let effects = app.update(Message::Connection {
        tab,
        attempt,
        event: ConnectionEvent::Question {
            question,
            kind: password("a.lab", 1),
        },
    });
    assert!(
        matches!(
            effects.as_slice(),
            [Effect::Answer { answer: Some(Answer::Secret(secret)), .. }] if secret.expose() == "saved-pw"
        ),
        "{effects:?}"
    );
}

#[test]
fn another_server_or_a_second_try_is_the_user_s() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = with_provider(dir.path(), "get {Title}");
    let (tab, attempt) = open(&mut app);
    // A gateway on the way asks with its own host.
    let (_, request) = ask(&mut app, tab, attempt, password("gw.lab", 1));
    assert!(request.is_none(), "never another server");
    let (_, request) = ask(&mut app, tab, attempt, password("a.lab", 2));
    assert!(request.is_none(), "not on a second try");
}

#[test]
fn a_password_given_and_refused_is_not_asked_again_in_the_attempt() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = with_provider(dir.path(), "get {Title}");
    let (tab, attempt) = open(&mut app);
    let (_, request) = ask(&mut app, tab, attempt, password("a.lab", 1));
    app.update(answer(&request.expect("asked"), Ok(given("wrong"))));
    // The server asks again in the same attempt, as a first try of another round.
    let (question, request) = ask(&mut app, tab, attempt, password("a.lab", 1));
    assert!(request.is_none(), "asked once per attempt");
    assert!(shown(&app, tab, question));
}

#[test]
fn off_while_it_ran_or_the_tab_closed_the_question_is_released() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = with_provider(dir.path(), "get {Title}");
    let (tab, attempt) = open(&mut app);
    let (question, request) = ask(&mut app, tab, attempt, password("a.lab", 1));
    let request = request.expect("asked");
    app.update(Message::RequestCloseTab(tab));
    if app.dialog.is_some() {
        app.update(Message::ConfirmDialog);
    }
    assert!(app.tab(tab).is_none(), "closed");
    let effects = app.update(answer(&request, Ok(given("s3cret"))));
    assert!(
        matches!(
            effects.as_slice(),
            [Effect::Answer { question: released, answer: None }] if *released == question
        ),
        "{effects:?}"
    );
}

#[test]
fn turned_off_with_its_command_set_it_is_not_asked() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = with_provider(dir.path(), "get {Title}");
    app.update(Message::CredentialProvider(ProviderMessage::Enabled(false)));
    let (tab, attempt) = open(&mut app);
    assert!(
        ask(&mut app, tab, attempt, password("a.lab", 1))
            .1
            .is_none()
    );
}

#[test]
fn a_provider_without_a_command_is_not_asked() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = with_provider(dir.path(), "  ");
    let (tab, attempt) = open(&mut app);
    assert!(
        ask(&mut app, tab, attempt, password("a.lab", 1))
            .1
            .is_none()
    );
}

#[cfg(unix)]
#[tokio::test]
async fn the_request_runs_the_command_with_the_profile_s_name() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = with_provider(dir.path(), r#"printf "%s-pw" "{Title}""#);
    let (tab, attempt) = open(&mut app);
    let (_, request) = ask(&mut app, tab, attempt, password("a.lab", 1));
    let answered = request.expect("asked").run().await;
    let provided = answered.result.expect("given");
    assert_eq!(provided.password.expose(), "Web server-pw");
}

/// Types `value` into `field` of profile `a`'s form and saves it.
fn edit_and_save(app: &mut App, field: heimdall_app::profile_draft::ProfileField, value: &str) {
    app.update(Message::EditProfile(ProfileId::new("a")));
    app.update(Message::ProfileField {
        field,
        value: value.to_owned(),
    });
    app.update(Message::SaveProfile {
        password: None,
        passphrase: None,
    });
}

#[test]
fn the_vault_entry_name_is_the_title_and_a_blank_one_is_the_name() {
    use heimdall_app::profile_draft::ProfileField;

    let dir = tempfile::tempdir().expect("dir");
    let mut app = with_provider(dir.path(), "get {Title}");
    edit_and_save(&mut app, ProfileField::VaultEntry, "  Servers/Web  ");
    assert_eq!(app.dialog, None, "saved");
    assert_eq!(
        app.profiles()[0].vault_entry.as_deref(),
        Some("Servers/Web")
    );
    let (tab, attempt) = open(&mut app);
    let (_, request) = ask(&mut app, tab, attempt, password("a.lab", 1));
    assert_eq!(request.expect("asked").lookup.title, "Servers/Web");

    edit_and_save(&mut app, ProfileField::VaultEntry, "   ");
    assert_eq!(app.profiles()[0].vault_entry, None, "blank is none");
    let (tab, attempt) = open(&mut app);
    let (_, request) = ask(&mut app, tab, attempt, password("a.lab", 1));
    assert_eq!(request.expect("asked").lookup.title, "Web server");
}

#[test]
fn renamed_in_its_form_a_profile_keeps_its_entry_under_the_old_name() {
    use heimdall_app::profile_draft::ProfileField;

    let dir = tempfile::tempdir().expect("dir");
    let mut app = with_provider(dir.path(), "get {Title}");
    edit_and_save(&mut app, ProfileField::Name, "Front end");
    assert_eq!(app.profiles()[0].name, "Front end");
    assert_eq!(app.profiles()[0].vault_entry.as_deref(), Some("Web server"));
    let (tab, attempt) = open(&mut app);
    let (_, request) = ask(&mut app, tab, attempt, password("a.lab", 1));
    assert_eq!(request.expect("asked").lookup.title, "Web server");
    // Saved again under the same name, nothing more changes.
    edit_and_save(&mut app, ProfileField::VaultEntry, "Other");
    assert_eq!(app.profiles()[0].vault_entry.as_deref(), Some("Other"));
}

#[test]
fn a_control_character_in_the_vault_entry_name_is_refused() {
    use heimdall_app::profile_draft::{DraftError, ProfileField};

    let dir = tempfile::tempdir().expect("dir");
    let mut app = with_provider(dir.path(), "get {Title}");
    edit_and_save(&mut app, ProfileField::VaultEntry, "a\u{7}b");
    assert!(
        matches!(
            &app.dialog,
            Some(heimdall_app::Dialog::EditProfile {
                error: Some(DraftError::ControlCharacter),
                ..
            })
        ),
        "{:?}",
        app.dialog
    );
}

#[test]
fn the_vault_entry_name_is_offered_where_the_provider_gives_the_password() {
    use heimdall_app::profile_draft::{DraftProtocol, ProfileField};

    for (protocol, shown) in [
        (DraftProtocol::Ssh, true),
        (DraftProtocol::Rdp, true),
        (DraftProtocol::Vnc, true),
        (DraftProtocol::WinRm, false),
        (DraftProtocol::Telnet, false),
    ] {
        assert_eq!(
            protocol.shows(ProfileField::VaultEntry),
            shown,
            "{protocol:?}"
        );
    }
}

#[test]
fn edited_for_something_else_a_profile_keeps_its_vault_entry_name() {
    use heimdall_app::profile_draft::ProfileField;

    let dir = tempfile::tempdir().expect("dir");
    let mut app = with_provider(dir.path(), "get {Title}");
    edit_and_save(&mut app, ProfileField::VaultEntry, "Servers/Web");
    edit_and_save(&mut app, ProfileField::Host, "b.lab");
    assert_eq!(app.profiles()[0].host, "b.lab");
    assert_eq!(
        app.profiles()[0].vault_entry.as_deref(),
        Some("Servers/Web")
    );
}

#[test]
fn a_blank_vault_entry_name_written_in_the_file_is_the_name() {
    let dir = tempfile::tempdir().expect("dir");
    let profiles_file = dir.path().join("profiles.toml");
    let mut store = ProfileStore::open(&profiles_file).expect("store");
    store.merge([SshProfile {
        id: ProfileId::new("a"),
        name: "Web server".to_owned(),
        group: None,
        host: "a.lab".to_owned(),
        port: 22,
        username: Some("admin".to_owned()),
        key_path: None,
        gateway: None,
        vault_entry: Some("   ".to_owned()),
        forwards: heimdall_core::profile::Forwards::default(),
        post_connect: heimdall_core::post_connect::PostConnect::default(),
        forward_agent: false,
        compression: false,
        sftp: false,
    }]);
    store.save().expect("save");
    let mut app = App::new(AppConfig {
        profiles_file,
        known_hosts: dir.path().join("known_hosts"),
        legacy_dir: None,
        agent: AgentSource::Disabled,
        initial_grid: GridSize { cols: 80, rows: 24 },
        files_start: dir.path().to_owned(),
        system_credentials: SystemCredentials::memory(),
    });
    app.update(Message::CredentialProvider(ProviderMessage::Enabled(true)));
    app.update(Message::CredentialProvider(ProviderMessage::Command(
        "get {Title}".to_owned(),
    )));
    let (tab, attempt) = open(&mut app);
    let (_, request) = ask(&mut app, tab, attempt, password("a.lab", 1));
    assert_eq!(request.expect("asked").lookup.title, "Web server");
}

#[test]
fn windows_credential_manager_needs_no_command_and_looks_up_the_entry() {
    use heimdall_core::credential_provider::ProviderKind;

    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path());
    app.update(Message::CredentialProvider(ProviderMessage::Enabled(true)));
    app.update(Message::CredentialProvider(ProviderMessage::Kind(
        ProviderKind::WindowsCredentialManager,
    )));
    let (tab, attempt) = open(&mut app);
    let (_, request) = ask(&mut app, tab, attempt, password("a.lab", 1));
    let request = request.expect("asked without a command");
    assert_eq!(
        request.settings.kind,
        ProviderKind::WindowsCredentialManager
    );
    assert_eq!(request.lookup.title, "Web server");
}

#[cfg(not(windows))]
#[tokio::test]
async fn away_from_windows_the_credential_manager_says_it_is_not_there() {
    use heimdall_core::credential_provider::ProviderKind;

    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path());
    app.update(Message::CredentialProvider(ProviderMessage::Enabled(true)));
    app.update(Message::CredentialProvider(ProviderMessage::Kind(
        ProviderKind::WindowsCredentialManager,
    )));
    let (tab, attempt) = open(&mut app);
    let (question, request) = ask(&mut app, tab, attempt, password("a.lab", 1));
    let answered = request.expect("asked").run().await;
    assert!(matches!(answered.result, Err(ProviderFailure::Launch(_))));
    app.update(Message::CredentialProvided(Box::new(answered)));
    assert!(shown(&app, tab, question), "the user is asked");
    assert!(matches!(app.notice(), Some(Notice::ProviderFailed(_))));
}
