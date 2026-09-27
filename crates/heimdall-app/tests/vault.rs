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

//! Saved passwords: kept only once accepted, given back only to the server they were
//! accepted by, once per attempt, and never again in the session once refused.

use std::path::Path;

use heimdall_app::{
    Answer, App, AppConfig, AttemptId, ConnectionEvent, Dialog, Effect, Message, QuestionId,
    QuestionKind, TabId, UiError, VaultMode, VaultProblem, VaultStatus, open_vault,
};
use heimdall_core::profile::{ProfileId, SshProfile};
use heimdall_core::store::ProfileStore;
use heimdall_ssh::{AgentSource, AuthMethod, PasswordQuestion, Secret};
use heimdall_term::GridSize;

const MASTER: &str = "correct horse battery staple";
const PASSWORD: &str = "s3cret!";

fn profile(id: &str, host: &str) -> SshProfile {
    SshProfile {
        id: ProfileId::new(id),
        name: format!("server {id}"),
        group: None,
        host: host.to_owned(),
        port: 22,
        username: Some("admin".to_owned()),
        key_path: None,
        gateway: None,
    }
}

/// The application over `dir`, with profile `a` at `host`.
fn app(dir: &Path, host: &str) -> App {
    let profiles_file = dir.join("profiles.toml");
    let mut store = ProfileStore::open(&profiles_file).expect("store");
    store.merge([profile("a", host)]);
    store.save().expect("save");
    App::new(AppConfig {
        profiles_file,
        known_hosts: dir.join("known_hosts"),
        legacy_dir: None,
        agent: AgentSource::Disabled,
        initial_grid: GridSize { cols: 80, rows: 24 },
        files_start: dir.to_owned(),
    })
}

/// Unlocks, or creates, the vault through its dialog, as the UI would.
async fn unlock(app: &mut App, master: &str) {
    let create = app.vault_status() == VaultStatus::Missing;
    app.update(Message::ShowVault);
    let effects = app.update(Message::SubmitVault {
        password: Secret::new(master.to_owned()),
        confirm: create.then(|| Secret::new(master.to_owned())),
    });
    let [
        Effect::OpenVault {
            path,
            password,
            create,
        },
    ] = effects.as_slice()
    else {
        panic!("expected OpenVault, got {effects:?}");
    };
    let result = open_vault(path.clone(), password.clone(), *create).await;
    app.update(Message::VaultOpened(result));
}

fn open(app: &mut App) -> (TabId, AttemptId) {
    let effects = app.update(Message::OpenProfile(ProfileId::new("a")));
    match effects.as_slice() {
        [Effect::Connect { tab, attempt, .. }] => (*tab, *attempt),
        other => panic!("expected one Connect, got {other:?}"),
    }
}

fn event(app: &mut App, tab: TabId, attempt: AttemptId, event: ConnectionEvent) -> Vec<Effect> {
    app.update(Message::Connection {
        tab,
        attempt,
        event,
    })
}

fn password_question(host: &str, attempt: u32) -> QuestionKind {
    QuestionKind::Password(PasswordQuestion {
        host: host.to_owned(),
        port: 22,
        username: "admin".to_owned(),
        attempt,
    })
}

/// Asks a password question; returns it and the password Heimdall answered with by itself,
/// if it did.
fn ask(
    app: &mut App,
    tab: TabId,
    attempt: AttemptId,
    kind: QuestionKind,
) -> (QuestionId, Option<String>) {
    let question = QuestionId::fresh();
    let effects = event(
        app,
        tab,
        attempt,
        ConnectionEvent::Question { question, kind },
    );
    let answered = match effects.as_slice() {
        [] => None,
        [
            Effect::Answer {
                question: answered,
                answer: Some(Answer::Secret(secret)),
            },
        ] if *answered == question => Some(secret.expose().to_owned()),
        other => panic!("unexpected {other:?}"),
    };
    let shown = app
        .tab(tab)
        .expect("tab")
        .prompts
        .iter()
        .any(|prompt| prompt.question == question);
    assert_eq!(shown, answered.is_none(), "shown exactly when not answered");
    (question, answered)
}

fn succeed(app: &mut App, tab: TabId, attempt: AttemptId) {
    #[derive(Debug)]
    struct Nothing;
    impl heimdall_app::InputSink for Nothing {
        fn write(&self, _: Vec<u8>) -> Result<(), heimdall_ssh::SessionClosed> {
            Ok(())
        }
        fn resize(&self, _: heimdall_ssh::TerminalSize) -> Result<(), heimdall_ssh::SessionClosed> {
            Ok(())
        }
        fn close(&self) {}
    }
    event(
        app,
        tab,
        attempt,
        ConnectionEvent::Connected {
            input: std::sync::Arc::new(Nothing),
        },
    );
}

/// Types `PASSWORD` into the question with "remember" ticked.
fn type_remembered(app: &mut App, tab: TabId, question: QuestionId) {
    let prompt = app
        .tab(tab)
        .expect("tab")
        .prompts
        .iter()
        .find(|prompt| prompt.question == question)
        .cloned()
        .expect("prompt");
    assert!(app.can_remember(tab, &prompt));
    app.update(Message::AnswerRemembered {
        tab,
        question,
        password: Secret::new(PASSWORD.to_owned()),
    });
}

/// A vault with `PASSWORD` saved for profile `a` at `a.lab`.
async fn saved(dir: &Path) -> App {
    let mut app = app(dir, "a.lab");
    unlock(&mut app, MASTER).await;
    let (tab, attempt) = open(&mut app);
    let (question, _) = ask(&mut app, tab, attempt, password_question("a.lab", 1));
    type_remembered(&mut app, tab, question);
    succeed(&mut app, tab, attempt);
    app
}

#[test]
fn a_new_master_password_must_be_long_and_typed_twice_alike() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path(), "a.lab");
    assert_eq!(app.vault_status(), VaultStatus::Missing);
    app.update(Message::ShowVault);
    let submit = |app: &mut App, password: &str, confirm: &str| {
        app.update(Message::SubmitVault {
            password: Secret::new(password.to_owned()),
            confirm: Some(Secret::new(confirm.to_owned())),
        })
    };
    let problem = |app: &App| match &app.dialog {
        Some(Dialog::Vault(dialog)) => {
            assert_eq!(dialog.mode, VaultMode::Create);
            dialog.problem.clone()
        }
        other => panic!("{other:?}"),
    };
    assert!(submit(&mut app, "short", "short").is_empty());
    assert_eq!(problem(&app), Some(VaultProblem::TooShort));
    // Characters, not bytes: eleven accented letters are 22 bytes and still too short.
    let eleven = "\u{e9}".repeat(heimdall_app::MIN_MASTER_PASSWORD_CHARS - 1);
    assert!(submit(&mut app, &eleven, &eleven).is_empty());
    assert_eq!(problem(&app), Some(VaultProblem::TooShort));
    assert!(submit(&mut app, MASTER, "correct horse battery stapler").is_empty());
    assert_eq!(problem(&app), Some(VaultProblem::Mismatch));
    let twelve = "\u{e9}".repeat(heimdall_app::MIN_MASTER_PASSWORD_CHARS);
    assert!(matches!(
        submit(&mut app, &twelve, &twelve).as_slice(),
        [Effect::OpenVault { create: true, .. }]
    ));
    assert!(!dir.path().join(heimdall_app::VAULT_FILE_NAME).exists());
}

#[tokio::test]
async fn a_remembered_password_is_kept_only_once_accepted_then_answers_by_itself() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path(), "a.lab");
    unlock(&mut app, MASTER).await;
    assert_eq!(app.vault_status(), VaultStatus::Open);

    // Refused: nothing is kept.
    let (tab, attempt) = open(&mut app);
    let (question, _) = ask(&mut app, tab, attempt, password_question("a.lab", 1));
    type_remembered(&mut app, tab, question);
    event(
        &mut app,
        tab,
        attempt,
        ConnectionEvent::Failed(UiError::AuthenticationFailed {
            tried: vec![AuthMethod::Password],
        }),
    );
    let (tab, attempt) = open(&mut app);
    let (question, answered) = ask(&mut app, tab, attempt, password_question("a.lab", 1));
    assert_eq!(answered, None, "a refused password was not kept");

    // Accepted: kept, and the next connection is answered without asking.
    type_remembered(&mut app, tab, question);
    succeed(&mut app, tab, attempt);
    let (tab, attempt) = open(&mut app);
    let (_, answered) = ask(&mut app, tab, attempt, password_question("A.LAB", 1));
    assert_eq!(answered.as_deref(), Some(PASSWORD));

    // In the file: another start, unlocked, answers too.
    let mut restarted = app_restarted(dir.path());
    assert!(
        matches!(&restarted.dialog, Some(Dialog::Vault(dialog)) if dialog.mode == VaultMode::Unlock),
        "a vault on disk is offered to unlock at start"
    );
    unlock(&mut restarted, MASTER).await;
    let (tab, attempt) = open(&mut restarted);
    let (_, answered) = ask(&mut restarted, tab, attempt, password_question("a.lab", 1));
    assert_eq!(answered.as_deref(), Some(PASSWORD));
}

fn app_restarted(dir: &Path) -> App {
    app(dir, "a.lab")
}

#[tokio::test]
async fn a_saved_password_goes_to_its_own_server_only() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = saved(dir.path()).await;

    // A gateway on the way asks with its own host.
    let (tab, attempt) = open(&mut app);
    let (_, answered) = ask(&mut app, tab, attempt, password_question("gateway.lab", 1));
    assert_eq!(answered, None, "never to a gateway");

    // Another account on the same server.
    let (tab, attempt) = open(&mut app);
    let other_account = QuestionKind::Password(PasswordQuestion {
        host: "a.lab".to_owned(),
        port: 22,
        username: "root".to_owned(),
        attempt: 1,
    });
    let (_, answered) = ask(&mut app, tab, attempt, other_account);
    assert_eq!(answered, None, "never to another account");

    // The profile edited to another host: the saved password stays with the old one.
    drop(app);
    let mut moved = self::app(dir.path(), "b.lab");
    unlock(&mut moved, MASTER).await;
    let (tab, attempt) = open(&mut moved);
    let (_, answered) = ask(&mut moved, tab, attempt, password_question("b.lab", 1));
    assert_eq!(answered, None, "never to the host a profile was changed to");
}

#[tokio::test]
async fn a_saved_password_refused_is_given_once_then_the_user_is_asked() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = saved(dir.path()).await;
    let (tab, attempt) = open(&mut app);
    let (_, answered) = ask(&mut app, tab, attempt, password_question("a.lab", 1));
    assert_eq!(answered.as_deref(), Some(PASSWORD));
    // The server asks again: it refused the saved password.
    let (_, answered) = ask(&mut app, tab, attempt, password_question("a.lab", 2));
    assert_eq!(answered, None);
    // Not given again in this session, not even to a new attempt.
    let (tab, attempt) = open(&mut app);
    let (_, answered) = ask(&mut app, tab, attempt, password_question("a.lab", 1));
    assert_eq!(answered, None);
}

#[tokio::test]
async fn a_saved_password_failing_the_connection_is_not_given_again() {
    // Refused outright, or disconnected by a server tired of wrong passwords.
    for failure in [
        UiError::AuthenticationFailed {
            tried: vec![AuthMethod::Password],
        },
        UiError::Disconnected {
            server_message: Some("Too many authentication failures".to_owned()),
        },
    ] {
        let dir = tempfile::tempdir().expect("dir");
        let mut app = saved(dir.path()).await;
        let (tab, attempt) = open(&mut app);
        let (_, answered) = ask(&mut app, tab, attempt, password_question("a.lab", 1));
        assert_eq!(answered.as_deref(), Some(PASSWORD));
        event(
            &mut app,
            tab,
            attempt,
            ConnectionEvent::Failed(failure.clone()),
        );
        let (tab, attempt) = open(&mut app);
        let (_, answered) = ask(&mut app, tab, attempt, password_question("a.lab", 1));
        assert_eq!(answered, None, "{failure:?}");
    }
}

#[tokio::test]
async fn a_saved_password_answers_one_question_per_attempt() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = saved(dir.path()).await;
    let (tab, attempt) = open(&mut app);
    let (_, answered) = ask(&mut app, tab, attempt, password_question("a.lab", 1));
    assert_eq!(answered.as_deref(), Some(PASSWORD));
    let (_, answered) = ask(&mut app, tab, attempt, password_question("a.lab", 1));
    assert_eq!(answered, None, "a second first try in one attempt is asked");
}

#[tokio::test]
async fn a_locked_vault_answers_nothing_and_saves_nothing() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = saved(dir.path()).await;
    app.update(Message::LockVault);
    assert_eq!(app.vault_status(), VaultStatus::Locked);
    let (tab, attempt) = open(&mut app);
    let (question, answered) = ask(&mut app, tab, attempt, password_question("a.lab", 1));
    assert_eq!(answered, None);
    let prompt = app.tab(tab).expect("tab").prompts[0].clone();
    assert_eq!(prompt.question, question);
    assert!(!app.can_remember(tab, &prompt));
}

#[tokio::test]
async fn a_wrong_master_password_says_so_and_opens_nothing() {
    let dir = tempfile::tempdir().expect("dir");
    drop(saved(dir.path()).await);
    let mut app = app_restarted(dir.path());
    unlock(&mut app, "not the master password").await;
    assert_eq!(app.vault_status(), VaultStatus::Locked);
    let Some(Dialog::Vault(dialog)) = &app.dialog else {
        panic!("{:?}", app.dialog);
    };
    assert_eq!(dialog.problem, Some(VaultProblem::Unreadable));
    assert!(!dialog.busy, "the user can try again");
}

/// Whether the question `kind`, asked in a new tab of `message`'s profile, may be remembered.
fn rememberable(app: &mut App, message: Message, kind: QuestionKind) -> bool {
    let effects = app.update(message);
    let (tab, attempt) = match effects.as_slice() {
        [Effect::ConnectRdp { tab, attempt, .. } | Effect::ConnectVnc { tab, attempt, .. }] => {
            (*tab, *attempt)
        }
        other => panic!("unexpected {other:?}"),
    };
    let (question, answered) = ask(app, tab, attempt, kind);
    assert_eq!(answered, None);
    let prompt = app
        .tab(tab)
        .expect("tab")
        .prompts
        .iter()
        .find(|prompt| prompt.question == question)
        .cloned()
        .expect("prompt");
    app.can_remember(tab, &prompt)
}

#[tokio::test]
async fn rdp_and_vnc_passwords_can_be_remembered_but_not_without_network_level_authentication() {
    use heimdall_app::ServerPasswordQuestion;
    use heimdall_core::profile::{RdpProfile, VncProfile};

    let dir = tempfile::tempdir().expect("dir");
    let rdp = |id: &str, allow_tls_only: bool| RdpProfile {
        id: ProfileId::new(id),
        name: id.to_owned(),
        group: None,
        host: "dc.lab".to_owned(),
        port: 3389,
        username: Some("admin".to_owned()),
        domain: None,
        allow_tls_only,
        gateway: None,
        redirect_clipboard: false,
    };
    let profiles_file = dir.path().join("profiles.toml");
    let mut store = ProfileStore::open(&profiles_file).expect("store");
    store.merge_rdp([rdp("nla", false), rdp("tls", true)]);
    store.merge_vnc([VncProfile {
        id: ProfileId::new("vnc"),
        name: "vnc".to_owned(),
        group: None,
        host: "screen.lab".to_owned(),
        port: 5900,
        view_only: false,
        allow_no_password: false,
    }]);
    store.save().expect("save");
    drop(store);
    let mut app = app(dir.path(), "a.lab");
    unlock(&mut app, MASTER).await;

    let rdp_question = || {
        QuestionKind::Password(PasswordQuestion {
            host: "dc.lab".to_owned(),
            port: 3389,
            username: "admin".to_owned(),
            attempt: 1,
        })
    };
    let vnc_question = |host: &str| {
        QuestionKind::ServerPassword(ServerPasswordQuestion {
            host: host.to_owned(),
            port: 5900,
        })
    };
    assert!(rememberable(
        &mut app,
        Message::OpenRdp(ProfileId::new("nla")),
        rdp_question()
    ));
    assert!(
        !rememberable(
            &mut app,
            Message::OpenRdp(ProfileId::new("tls")),
            rdp_question()
        ),
        "a desktop shown without NLA proves nothing about the password"
    );
    assert!(rememberable(
        &mut app,
        Message::OpenVnc(ProfileId::new("vnc")),
        vnc_question("screen.lab")
    ));
    assert!(!rememberable(
        &mut app,
        Message::OpenVnc(ProfileId::new("vnc")),
        vnc_question("other.lab")
    ));
}

#[tokio::test]
async fn a_password_typed_for_an_abandoned_attempt_is_not_saved_by_the_next() {
    use heimdall_ssh::PublicKey;
    const HOST_KEY: &str =
        include_str!("../../heimdall-ssh/tests/fixtures/hostkeys/host-ed25519.pub");

    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path(), "a.lab");
    unlock(&mut app, MASTER).await;
    let (tab, old) = open(&mut app);
    let (question, _) = ask(&mut app, tab, old, password_question("a.lab", 1));
    type_remembered(&mut app, tab, question);
    // The attempt stops on an unknown key; accepting it starts another.
    let key = std::sync::Arc::new(PublicKey::from_openssh(HOST_KEY.trim()).expect("key"));
    event(
        &mut app,
        tab,
        old,
        ConnectionEvent::UnknownHostKey {
            host: "a.lab".to_owned(),
            port: 22,
            fingerprint: "SHA256:x".to_owned(),
            key,
        },
    );
    let effects = app.update(Message::HostKeyDecision { tab, accept: true });
    let [Effect::Connect { attempt: new, .. }] = effects.as_slice() else {
        panic!("expected a new attempt, got {effects:?}");
    };
    // The new attempt succeeds, with whatever password it was given.
    succeed(&mut app, tab, *new);
    let (tab, attempt) = open(&mut app);
    let (_, answered) = ask(&mut app, tab, attempt, password_question("a.lab", 1));
    assert_eq!(answered, None, "the old attempt's password was not saved");
}

#[tokio::test]
async fn a_saved_password_is_not_spent_on_a_later_try() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = saved(dir.path()).await;
    app.update(Message::LockVault);
    let (tab, attempt) = open(&mut app);
    // First try asked of the user, the vault closed; a wrong password typed.
    let (_, answered) = ask(&mut app, tab, attempt, password_question("a.lab", 1));
    assert_eq!(answered, None);
    unlock(&mut app, MASTER).await;
    // The server asks again: every try counts towards a lockout, the user answers.
    let (_, answered) = ask(&mut app, tab, attempt, password_question("a.lab", 2));
    assert_eq!(answered, None);
}

#[tokio::test]
async fn a_remembered_password_refused_then_corrected_is_not_the_one_saved() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path(), "a.lab");
    unlock(&mut app, MASTER).await;
    let (tab, attempt) = open(&mut app);
    // A typo, "remember" ticked.
    let (question, _) = ask(&mut app, tab, attempt, password_question("a.lab", 1));
    type_remembered(&mut app, tab, question);
    // Refused: asked again, the right one typed without ticking.
    let (question, answered) = ask(&mut app, tab, attempt, password_question("a.lab", 2));
    assert_eq!(answered, None);
    app.update(Message::Answer {
        tab,
        question,
        answer: Some(Answer::Secret(Secret::new("the right one".to_owned()))),
    });
    succeed(&mut app, tab, attempt);
    let (tab, attempt) = open(&mut app);
    let (_, answered) = ask(&mut app, tab, attempt, password_question("a.lab", 1));
    assert_eq!(answered, None, "the refused password was not saved");
}

#[tokio::test]
async fn cancelling_while_the_key_is_derived_leaves_the_vault_closed() {
    let dir = tempfile::tempdir().expect("dir");
    drop(saved(dir.path()).await);
    let mut app = app_restarted(dir.path());
    let effects = app.update(Message::SubmitVault {
        password: Secret::new(MASTER.to_owned()),
        confirm: None,
    });
    let [
        Effect::OpenVault {
            path,
            password,
            create,
        },
    ] = effects.as_slice()
    else {
        panic!("expected OpenVault, got {effects:?}");
    };
    app.update(Message::DismissDialog);
    let result = open_vault(path.clone(), password.clone(), *create).await;
    assert!(result.is_ok(), "the right password");
    app.update(Message::VaultOpened(result));
    assert_eq!(app.vault_status(), VaultStatus::Locked);
}

#[tokio::test]
async fn deleting_a_profile_forgets_its_saved_password() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = saved(dir.path()).await;
    app.update(Message::EditProfile(ProfileId::new("a")));
    app.update(Message::DeleteProfile);
    app.update(Message::ConfirmDialog);
    assert!(app.profiles().is_empty(), "{:?}", app.dialog);
    drop(app);
    // The same profile made again, on the same server.
    let mut again = app_restarted(dir.path());
    unlock(&mut again, MASTER).await;
    let (tab, attempt) = open(&mut again);
    let (_, answered) = ask(&mut again, tab, attempt, password_question("a.lab", 1));
    assert_eq!(answered, None);
}

#[cfg(unix)]
#[tokio::test]
async fn a_password_that_could_not_be_saved_is_not_used_and_an_open_form_stays() {
    use std::os::unix::fs::PermissionsExt as _;

    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path(), "a.lab");
    unlock(&mut app, MASTER).await;
    let (tab, attempt) = open(&mut app);
    let (question, _) = ask(&mut app, tab, attempt, password_question("a.lab", 1));
    type_remembered(&mut app, tab, question);
    // The folder cannot be written: the save fails.
    std::fs::set_permissions(dir.path(), std::fs::Permissions::from_mode(0o500)).expect("chmod");
    app.update(Message::NewProfile);
    succeed(&mut app, tab, attempt);
    std::fs::set_permissions(dir.path(), std::fs::Permissions::from_mode(0o700)).expect("chmod");
    assert!(
        matches!(app.dialog, Some(Dialog::EditProfile { .. })),
        "the form being filled is kept: {:?}",
        app.dialog
    );
    app.update(Message::DismissDialog);
    let (tab, attempt) = open(&mut app);
    let (_, answered) = ask(&mut app, tab, attempt, password_question("a.lab", 1));
    assert_eq!(answered, None, "what was not saved is not used");
}

/// A vault holding a password for RDP profile `nla` as `CORP\admin` on `dc.lab:3389`, and
/// the application over it with RDP profiles `nla` and `tls` (without NLA) on that server,
/// both in domain `domain`.
async fn rdp_vault(dir: &Path, domain: &str) -> App {
    use heimdall_core::credentials::{
        CredentialProtocol, Endpoint, SavedPassword, encode, password_entry,
    };
    use heimdall_core::profile::RdpProfile;

    let mut vault =
        sealvault::Vault::create(dir.join(heimdall_app::VAULT_FILE_NAME), MASTER.as_bytes())
            .expect("vault");
    for id in ["nla", "tls"] {
        let saved = SavedPassword {
            endpoint: Endpoint {
                protocol: CredentialProtocol::Rdp,
                host: "dc.lab".to_owned(),
                port: 3389,
                username: Some("CORP\\admin".to_owned()),
            },
            password: zeroize::Zeroizing::new("rdp password".to_owned()),
        };
        vault.set(password_entry(&ProfileId::new(id)), encode(&saved).to_vec());
    }
    vault.save().expect("save");
    let rdp = |id: &str, allow_tls_only: bool| RdpProfile {
        id: ProfileId::new(id),
        name: id.to_owned(),
        group: None,
        host: "dc.lab".to_owned(),
        port: 3389,
        username: Some("admin".to_owned()),
        domain: Some(domain.to_owned()),
        allow_tls_only,
        gateway: None,
        redirect_clipboard: false,
    };
    let mut store = ProfileStore::open(dir.join("profiles.toml")).expect("store");
    store.merge_rdp([rdp("nla", false), rdp("tls", true)]);
    store.save().expect("save");
    drop(store);
    let mut app = app(dir, "a.lab");
    unlock(&mut app, MASTER).await;
    app
}

fn rdp_answer(app: &mut App, id: &str) -> Option<String> {
    let effects = app.update(Message::OpenRdp(ProfileId::new(id)));
    let [Effect::ConnectRdp { tab, attempt, .. }] = effects.as_slice() else {
        panic!("expected ConnectRdp, got {effects:?}");
    };
    let kind = QuestionKind::Password(PasswordQuestion {
        host: "dc.lab".to_owned(),
        port: 3389,
        username: "admin".to_owned(),
        attempt: 1,
    });
    ask(app, *tab, *attempt, kind).1
}

#[tokio::test]
async fn an_rdp_password_goes_to_its_domain_account_and_never_without_nla() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = rdp_vault(dir.path(), "CORP").await;
    assert_eq!(rdp_answer(&mut app, "nla").as_deref(), Some("rdp password"));
    assert_eq!(
        rdp_answer(&mut app, "tls"),
        None,
        "without NLA a wrong password is never refused, so none is sent"
    );

    let dir = tempfile::tempdir().expect("dir");
    let mut app = rdp_vault(dir.path(), "LAB").await;
    assert_eq!(
        rdp_answer(&mut app, "nla"),
        None,
        "the profile moved to another domain: another account"
    );
}
