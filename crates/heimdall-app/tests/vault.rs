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

//! Saved passwords, as the C# Heimdall keeps them: typed in the profile editor, kept in the
//! system's store or, once a master password is set, in the vault; given back only to the
//! server they are for, once per attempt, and never again in the session once refused.

use std::path::Path;

use heimdall_app::profile_draft::{DraftError, ProfileField};
use heimdall_app::{
    Answer, App, AppConfig, AttemptId, ConnectionEvent, Dialog, Effect, Message, OpenedVault,
    QuestionId, QuestionKind, SystemCredentials, TabId, UiError, VaultJob, VaultMode, VaultProblem,
    VaultStatus, open_vault,
};
use heimdall_core::profile::{ProfileId, SshProfile};
use heimdall_core::store::ProfileStore;
use heimdall_ssh::{AgentSource, AuthMethod, PasswordQuestion, Secret};
use heimdall_term::GridSize;

const MASTER: &str = "correct horse battery staple";
const PASSWORD: &str = "s3cret!";

fn profile(id: &str, host: &str, username: Option<&str>) -> SshProfile {
    SshProfile {
        id: ProfileId::new(id),
        name: format!("server {id}"),
        group: None,
        host: host.to_owned(),
        port: 22,
        username: username.map(str::to_owned),
        key_path: None,
        gateway: None,
    }
}

/// The application over `dir`, with profile `a` at `host` for `admin`, and `system` as the
/// store used without a master password.
fn app(dir: &Path, host: &str, system: &SystemCredentials) -> App {
    app_with(dir, profile("a", host, Some("admin")), system)
}

fn app_with(dir: &Path, profile: SshProfile, system: &SystemCredentials) -> App {
    let profiles_file = dir.join("profiles.toml");
    let mut store = ProfileStore::open(&profiles_file).expect("store");
    store.merge([profile]);
    store.save().expect("save");
    App::new(AppConfig {
        profiles_file,
        known_hosts: dir.join("known_hosts"),
        legacy_dir: None,
        agent: AgentSource::Disabled,
        initial_grid: GridSize { cols: 80, rows: 24 },
        files_start: dir.to_owned(),
        system_credentials: system.clone(),
    })
}

/// Unlocks, or creates, the vault through its dialog, as the UI would.
async fn unlock(app: &mut App, master: &str) {
    let create = app.vault_status() == VaultStatus::Missing;
    app.update(Message::ShowVault);
    let effects = app.update(Message::SubmitVault {
        password: Secret::new(master.to_owned()),
        new: None,
        confirm: create.then(|| Secret::new(master.to_owned())),
    });
    run_vault_job(app, effects).await;
}

/// Runs the vault job `effects` asks for, as the UI would, and hands the result back.
async fn run_vault_job(app: &mut App, effects: Vec<Effect>) {
    let result = open_vault_job(effects).await;
    app.update(Message::VaultOpened(result));
}

/// Runs the vault job `effects` asks for.
async fn open_vault_job(effects: Vec<Effect>) -> Result<OpenedVault, VaultProblem> {
    match <[Effect; 1]>::try_from(effects) {
        Ok(
            [
                Effect::OpenVault {
                    path,
                    password,
                    job,
                },
            ],
        ) => open_vault(path, password, job).await,
        other => panic!("expected OpenVault, got {other:?}"),
    }
}

/// Opens the editor of profile `a` and saves it, `password` typed into it.
fn save_in_editor(app: &mut App, password: Option<&str>) {
    app.update(Message::EditProfile(ProfileId::new("a")));
    app.update(Message::SaveProfile {
        password: password.map(|typed| Secret::new(typed.to_owned())),
    });
}

/// Whether the editor of profile `a` says a password is saved.
fn editor_says_saved(app: &mut App) -> bool {
    app.update(Message::EditProfile(ProfileId::new("a")));
    let saved = match &app.dialog {
        Some(Dialog::EditProfile { draft, .. }) => draft.password_saved,
        other => panic!("{other:?}"),
    };
    app.update(Message::DismissDialog);
    saved
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

/// What a new connection to profile `a` is answered with, asked by `host` on its first try.
fn first_answer(app: &mut App, host: &str) -> Option<String> {
    let (tab, attempt) = open(app);
    ask(app, tab, attempt, password_question(host, 1)).1
}

/// An application with `PASSWORD` saved for profile `a` at `a.lab`, without a master
/// password.
fn saved(dir: &Path, system: &SystemCredentials) -> App {
    let mut app = app(dir, "a.lab", system);
    save_in_editor(&mut app, Some(PASSWORD));
    app
}

#[test]
fn a_password_saved_in_the_editor_answers_by_itself_after_a_restart_too() {
    let dir = tempfile::tempdir().expect("dir");
    let system = SystemCredentials::memory();
    let mut app = app(dir.path(), "a.lab", &system);
    assert!(app.can_save_passwords(), "without a master password");
    assert!(!editor_says_saved(&mut app));
    save_in_editor(&mut app, Some(PASSWORD));
    assert!(editor_says_saved(&mut app));
    assert_eq!(first_answer(&mut app, "A.LAB").as_deref(), Some(PASSWORD));
    assert!(app.dialog.is_none(), "no vault, nothing to unlock");

    let mut restarted = self::app(dir.path(), "a.lab", &system);
    assert!(
        restarted.dialog.is_none(),
        "no master password: no unlock at start"
    );
    assert_eq!(
        first_answer(&mut restarted, "a.lab").as_deref(),
        Some(PASSWORD)
    );
}

#[test]
fn saving_the_editor_without_typing_keeps_the_password_and_clear_removes_it() {
    let dir = tempfile::tempdir().expect("dir");
    let system = SystemCredentials::memory();
    let mut app = saved(dir.path(), &system);
    save_in_editor(&mut app, None);
    save_in_editor(&mut app, Some(""));
    assert_eq!(first_answer(&mut app, "a.lab").as_deref(), Some(PASSWORD));

    app.update(Message::EditProfile(ProfileId::new("a")));
    app.update(Message::ClearPassword);
    let Some(Dialog::EditProfile { draft, .. }) = &app.dialog else {
        panic!("{:?}", app.dialog);
    };
    assert!(!draft.password_saved, "the form stops saying so at once");
    app.update(Message::SaveProfile { password: None });
    assert!(!editor_says_saved(&mut app));
    assert_eq!(first_answer(&mut app, "a.lab"), None);
}

#[test]
fn clearing_then_dismissing_the_editor_keeps_the_password() {
    let dir = tempfile::tempdir().expect("dir");
    let system = SystemCredentials::memory();
    let mut app = saved(dir.path(), &system);
    app.update(Message::EditProfile(ProfileId::new("a")));
    app.update(Message::ClearPassword);
    app.update(Message::DismissDialog);
    assert!(
        editor_says_saved(&mut app),
        "removed only when the form is saved"
    );
}

#[test]
fn a_password_needs_a_user_name() {
    let dir = tempfile::tempdir().expect("dir");
    let system = SystemCredentials::memory();
    let mut app = app_with(dir.path(), profile("a", "a.lab", None), &system);
    save_in_editor(&mut app, Some(PASSWORD));
    let Some(Dialog::EditProfile { error, .. }) = &app.dialog else {
        panic!("{:?}", app.dialog);
    };
    assert_eq!(*error, Some(DraftError::UsernameForPassword));
    assert_eq!(
        DraftError::UsernameForPassword.field(),
        ProfileField::Username
    );
    app.update(Message::DismissDialog);
    assert!(!editor_says_saved(&mut app));
}

#[test]
fn a_new_master_password_must_be_long_and_typed_twice_alike() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path(), "a.lab", &SystemCredentials::memory());
    assert_eq!(app.vault_status(), VaultStatus::Missing);
    app.update(Message::ShowVault);
    let submit = |app: &mut App, password: &str, confirm: &str| {
        app.update(Message::SubmitVault {
            password: Secret::new(password.to_owned()),
            new: None,
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
    // Long enough, but one kind of character: the C# rule asks for three below twenty.
    let twelve = "\u{e9}".repeat(heimdall_app::MIN_MASTER_PASSWORD_CHARS);
    assert!(submit(&mut app, &twelve, &twelve).is_empty());
    assert_eq!(problem(&app), Some(VaultProblem::TooSimple));
    let nineteen = "a".repeat(heimdall_app::LONG_MASTER_PASSWORD_CHARS - 1);
    assert!(submit(&mut app, &nineteen, &nineteen).is_empty());
    assert_eq!(problem(&app), Some(VaultProblem::TooSimple));
    // Three kinds: lower case, upper case, then a digit or anything else.
    for mixed in [
        "\u{e9}\u{e9}\u{e9}\u{e9}\u{e9}\u{c9}\u{c9}\u{c9}\u{c9}\u{c9}12",
        "abcdefghiJK!",
    ] {
        assert_eq!(
            mixed.chars().count(),
            heimdall_app::MIN_MASTER_PASSWORD_CHARS
        );
        assert!(
            matches!(
                submit(&mut app, mixed, mixed).as_slice(),
                [Effect::OpenVault {
                    job: VaultJob::Create,
                    ..
                }]
            ),
            "{mixed}"
        );
        app.update(Message::DismissDialog);
        app.update(Message::ShowVault);
    }
    // Twenty of one kind: a passphrase needs no mix.
    let twenty = "a".repeat(heimdall_app::LONG_MASTER_PASSWORD_CHARS);
    assert!(matches!(
        submit(&mut app, &twenty, &twenty).as_slice(),
        [Effect::OpenVault {
            job: VaultJob::Create,
            ..
        }]
    ));
    assert!(!dir.path().join(heimdall_app::VAULT_FILE_NAME).exists());
}

#[tokio::test]
async fn with_a_master_password_passwords_go_to_the_vault_which_must_be_open() {
    let dir = tempfile::tempdir().expect("dir");
    let system = SystemCredentials::memory();
    let SystemCredentials::Memory(entries) = &system else {
        unreachable!()
    };
    let mut app = app(dir.path(), "a.lab", &system);
    unlock(&mut app, MASTER).await;
    save_in_editor(&mut app, Some(PASSWORD));
    assert!(
        entries.lock().expect("entries").is_empty(),
        "not in the system's store once a master password is set"
    );
    assert_eq!(first_answer(&mut app, "a.lab").as_deref(), Some(PASSWORD));

    app.update(Message::LockVault);
    assert!(!app.can_save_passwords());
    assert!(
        !editor_says_saved(&mut app),
        "nothing can be read, nothing is said"
    );
    // Typed while locked: not saved anywhere, the profile itself still is.
    save_in_editor(&mut app, Some("typed while locked"));
    assert!(app.dialog.is_none(), "{:?}", app.dialog);
    assert!(entries.lock().expect("entries").is_empty());
    assert_eq!(first_answer(&mut app, "a.lab"), None);

    // A restart asks for the master password; unlocked, the vault answers.
    let mut restarted = self::app(dir.path(), "a.lab", &system);
    assert!(
        matches!(&restarted.dialog, Some(Dialog::Vault(dialog)) if dialog.mode == VaultMode::Unlock),
        "a vault on disk is offered to unlock at start"
    );
    unlock(&mut restarted, MASTER).await;
    assert_eq!(
        first_answer(&mut restarted, "a.lab").as_deref(),
        Some(PASSWORD)
    );
}

#[test]
fn a_saved_password_goes_to_its_own_server_only() {
    let dir = tempfile::tempdir().expect("dir");
    let system = SystemCredentials::memory();
    let mut app = saved(dir.path(), &system);

    assert_eq!(
        first_answer(&mut app, "gateway.lab"),
        None,
        "never to a gateway"
    );
    let (tab, attempt) = open(&mut app);
    let other_port = QuestionKind::Password(PasswordQuestion {
        host: "a.lab".to_owned(),
        port: 2222,
        username: "admin".to_owned(),
        attempt: 1,
    });
    assert_eq!(
        ask(&mut app, tab, attempt, other_port).1,
        None,
        "never another port"
    );
    let (tab, attempt) = open(&mut app);
    let other_account = QuestionKind::Password(PasswordQuestion {
        host: "a.lab".to_owned(),
        port: 22,
        username: "root".to_owned(),
        attempt: 1,
    });
    assert_eq!(
        ask(&mut app, tab, attempt, other_account).1,
        None,
        "never another account"
    );

    // The profile changed outside the editor (an import): the password stays behind.
    drop(app);
    let mut moved = self::app(dir.path(), "b.lab", &system);
    assert_eq!(first_answer(&mut moved, "b.lab"), None);
}

#[test]
fn a_profile_changed_in_the_editor_takes_its_password_along() {
    let dir = tempfile::tempdir().expect("dir");
    let system = SystemCredentials::memory();
    let mut app = saved(dir.path(), &system);
    app.update(Message::EditProfile(ProfileId::new("a")));
    app.update(Message::ProfileField {
        field: ProfileField::Host,
        value: "b.lab".to_owned(),
    });
    app.update(Message::SaveProfile { password: None });
    assert_eq!(first_answer(&mut app, "b.lab").as_deref(), Some(PASSWORD));
    assert_eq!(
        first_answer(&mut app, "a.lab"),
        None,
        "the old host no longer gets it"
    );
}

#[test]
fn a_saved_password_refused_is_given_once_then_the_user_is_asked_until_a_new_one() {
    let dir = tempfile::tempdir().expect("dir");
    let system = SystemCredentials::memory();
    let mut app = saved(dir.path(), &system);
    let (tab, attempt) = open(&mut app);
    let (_, answered) = ask(&mut app, tab, attempt, password_question("a.lab", 1));
    assert_eq!(answered.as_deref(), Some(PASSWORD));
    // The server asks again: it refused the saved password.
    let (_, answered) = ask(&mut app, tab, attempt, password_question("a.lab", 2));
    assert_eq!(answered, None);
    assert_eq!(first_answer(&mut app, "a.lab"), None, "not in this session");
    // A new password saved in the editor is given again.
    save_in_editor(&mut app, Some("new password"));
    assert_eq!(
        first_answer(&mut app, "a.lab").as_deref(),
        Some("new password")
    );
}

#[test]
fn a_saved_password_failing_the_connection_is_not_given_again() {
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
        let mut app = saved(dir.path(), &SystemCredentials::memory());
        let (tab, attempt) = open(&mut app);
        let (_, answered) = ask(&mut app, tab, attempt, password_question("a.lab", 1));
        assert_eq!(answered.as_deref(), Some(PASSWORD));
        event(
            &mut app,
            tab,
            attempt,
            ConnectionEvent::Failed(failure.clone()),
        );
        assert_eq!(first_answer(&mut app, "a.lab"), None, "{failure:?}");
    }
}

#[test]
fn a_saved_password_answers_one_question_per_attempt() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = saved(dir.path(), &SystemCredentials::memory());
    let (tab, attempt) = open(&mut app);
    let (_, answered) = ask(&mut app, tab, attempt, password_question("a.lab", 1));
    assert_eq!(answered.as_deref(), Some(PASSWORD));
    let (_, answered) = ask(&mut app, tab, attempt, password_question("a.lab", 1));
    assert_eq!(answered, None, "a second first try in one attempt is asked");
}

#[tokio::test]
async fn a_saved_password_is_not_spent_on_a_later_try() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path(), "a.lab", &SystemCredentials::memory());
    unlock(&mut app, MASTER).await;
    save_in_editor(&mut app, Some(PASSWORD));
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
async fn a_wrong_master_password_says_so_and_opens_nothing() {
    let dir = tempfile::tempdir().expect("dir");
    let system = SystemCredentials::memory();
    let mut app = app(dir.path(), "a.lab", &system);
    unlock(&mut app, MASTER).await;
    drop(app);
    let mut app = self::app(dir.path(), "a.lab", &system);
    unlock(&mut app, "not the master password").await;
    assert_eq!(app.vault_status(), VaultStatus::Locked);
    let Some(Dialog::Vault(dialog)) = &app.dialog else {
        panic!("{:?}", app.dialog);
    };
    assert_eq!(dialog.problem, Some(VaultProblem::Unreadable));
    assert!(!dialog.busy, "the user can try again");
}

#[tokio::test]
async fn cancelling_while_the_key_is_derived_leaves_the_vault_closed() {
    let dir = tempfile::tempdir().expect("dir");
    let system = SystemCredentials::memory();
    let mut app = app(dir.path(), "a.lab", &system);
    unlock(&mut app, MASTER).await;
    drop(app);
    let mut app = self::app(dir.path(), "a.lab", &system);
    let effects = app.update(Message::SubmitVault {
        password: Secret::new(MASTER.to_owned()),
        new: None,
        confirm: None,
    });
    app.update(Message::DismissDialog);
    let result = open_vault_job(effects).await;
    assert!(result.is_ok(), "the right password");
    app.update(Message::VaultOpened(result));
    assert_eq!(app.vault_status(), VaultStatus::Locked);
}

#[test]
fn deleting_a_profile_forgets_its_saved_password() {
    let dir = tempfile::tempdir().expect("dir");
    let system = SystemCredentials::memory();
    let mut app = saved(dir.path(), &system);
    app.update(Message::EditProfile(ProfileId::new("a")));
    app.update(Message::DeleteProfile);
    app.update(Message::ConfirmDialog);
    assert!(app.profiles().is_empty(), "{:?}", app.dialog);
    drop(app);
    // The same profile made again, on the same server.
    let mut again = self::app(dir.path(), "a.lab", &system);
    assert_eq!(first_answer(&mut again, "a.lab"), None);
}

#[tokio::test]
async fn a_password_that_could_not_be_saved_says_so_and_is_not_used() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path(), "a.lab", &SystemCredentials::memory());
    unlock(&mut app, MASTER).await;
    // The copy kept before each save cannot be written: a folder is in its place.
    let vault = dir.path().join(heimdall_app::VAULT_FILE_NAME);
    let backup = sealvault::backup_path(&vault);
    if backup.exists() {
        std::fs::remove_file(&backup).expect("copy");
    }
    std::fs::create_dir(backup).expect("folder");
    save_in_editor(&mut app, Some(PASSWORD));
    assert!(
        matches!(app.dialog, Some(Dialog::PasswordSaveFailed { .. })),
        "{:?}",
        app.dialog
    );
    app.update(Message::DismissDialog);
    assert_eq!(
        first_answer(&mut app, "a.lab"),
        None,
        "what was not saved is not used"
    );
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
    let mut app = app(dir, "a.lab", &SystemCredentials::memory());
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

#[test]
fn saving_a_form_that_is_not_open_leaves_the_open_dialog() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path(), "a.lab", &SystemCredentials::memory());
    app.update(Message::ShowVault);
    app.update(Message::SaveProfile {
        password: Some(Secret::new(PASSWORD.to_owned())),
    });
    assert!(
        matches!(app.dialog, Some(Dialog::Vault(_))),
        "{:?}",
        app.dialog
    );
    assert_eq!(first_answer(&mut app, "a.lab"), None);
}

#[test]
fn a_password_typed_while_it_cannot_be_saved_does_not_stop_the_profile_from_saving() {
    let dir = tempfile::tempdir().expect("dir");
    // No master password and no store on this system: nowhere to save a password.
    let mut app = app_with(
        dir.path(),
        profile("a", "a.lab", None),
        &SystemCredentials::Unavailable,
    );
    assert!(!app.can_save_passwords());
    save_in_editor(&mut app, Some(PASSWORD));
    assert!(
        app.dialog.is_none(),
        "saved without the password: {:?}",
        app.dialog
    );
}

const NEW_MASTER: &str = "a new, longer master passphrase";
const GATEWAY_PASSWORD: &str = "jump pw";

/// Adds gateway `bastion` with `GATEWAY_PASSWORD` saved, through its dialog.
fn save_gateway(app: &mut App) {
    app.update(Message::NewGateway);
    for (field, value) in [
        (ProfileField::Name, "bastion"),
        (ProfileField::Host, "bastion.lab"),
        (ProfileField::Username, "jump"),
    ] {
        app.update(Message::GatewayField {
            field,
            value: value.to_owned(),
        });
    }
    app.update(Message::SaveGateway {
        password: Some(Secret::new(GATEWAY_PASSWORD.to_owned())),
    });
    assert!(app.dialog.is_none(), "{:?}", app.dialog);
}

fn system_entries(system: &SystemCredentials) -> usize {
    let SystemCredentials::Memory(entries) = system else {
        unreachable!()
    };
    entries.lock().expect("entries").len()
}

fn vault_problem(app: &App) -> Option<VaultProblem> {
    match &app.dialog {
        Some(Dialog::Vault(dialog)) => dialog.problem.clone(),
        other => panic!("{other:?}"),
    }
}

/// Whether `path` opens with `master`.
async fn opens(path: &Path, master: &str) -> bool {
    open_vault(
        path.to_owned(),
        Secret::new(master.to_owned()),
        VaultJob::Open,
    )
    .await
    .is_ok()
}

#[tokio::test]
async fn enabling_a_master_password_moves_the_saved_passwords_into_the_vault() {
    let dir = tempfile::tempdir().expect("dir");
    let system = SystemCredentials::memory();
    let mut app = saved(dir.path(), &system);
    save_gateway(&mut app);
    assert_eq!(system_entries(&system), 2, "the server's and the gateway's");

    unlock(&mut app, MASTER).await;
    assert_eq!(app.vault_status(), VaultStatus::Open);
    assert!(app.dialog.is_none(), "{:?}", app.dialog);
    assert_eq!(
        system_entries(&system),
        0,
        "moved, not copied: the system's store keeps nothing"
    );
    assert_eq!(first_answer(&mut app, "a.lab").as_deref(), Some(PASSWORD));

    // What moved is in the vault on disk: a restart unlocked answers with it.
    let mut restarted = self::app(dir.path(), "a.lab", &system);
    unlock(&mut restarted, MASTER).await;
    assert_eq!(
        first_answer(&mut restarted, "a.lab").as_deref(),
        Some(PASSWORD)
    );
}

#[tokio::test]
async fn changing_the_master_password_seals_the_vault_and_its_copy_with_the_new_one() {
    let dir = tempfile::tempdir().expect("dir");
    let system = SystemCredentials::memory();
    let mut app = saved(dir.path(), &system);
    unlock(&mut app, MASTER).await;
    let path = dir.path().join(heimdall_app::VAULT_FILE_NAME);

    app.update(Message::ChangeMasterPassword);
    let change = |current: &str, new: &str, confirm: &str| Message::SubmitVault {
        password: Secret::new(current.to_owned()),
        new: Some(Secret::new(new.to_owned())),
        confirm: Some(Secret::new(confirm.to_owned())),
    };
    assert!(app.update(change(MASTER, "short", "short")).is_empty());
    assert_eq!(vault_problem(&app), Some(VaultProblem::TooShort));
    assert!(
        app.update(change(MASTER, NEW_MASTER, "another one"))
            .is_empty()
    );
    assert_eq!(vault_problem(&app), Some(VaultProblem::Mismatch));
    let effects = app.update(change("not the master", NEW_MASTER, NEW_MASTER));
    run_vault_job(&mut app, effects).await;
    assert_eq!(vault_problem(&app), Some(VaultProblem::Unreadable));
    assert!(
        opens(&path, MASTER).await,
        "a wrong current one changes nothing"
    );

    let effects = app.update(change(MASTER, NEW_MASTER, NEW_MASTER));
    run_vault_job(&mut app, effects).await;
    assert!(app.dialog.is_none(), "{:?}", app.dialog);
    assert_eq!(app.vault_status(), VaultStatus::Open);
    assert_eq!(first_answer(&mut app, "a.lab").as_deref(), Some(PASSWORD));
    assert!(opens(&path, NEW_MASTER).await);
    assert!(!opens(&path, MASTER).await, "the old one opens nothing");
    let backup = sealvault::backup_path(&path);
    assert!(backup.is_file(), "a copy is kept beside the vault");
    assert!(!opens(&backup, MASTER).await, "nor the copy kept beside it");
}

#[tokio::test]
async fn disabling_the_master_password_puts_the_passwords_back_in_the_system_store() {
    let dir = tempfile::tempdir().expect("dir");
    let system = SystemCredentials::memory();
    let mut app = saved(dir.path(), &system);
    save_gateway(&mut app);
    unlock(&mut app, MASTER).await;
    let path = dir.path().join(heimdall_app::VAULT_FILE_NAME);

    app.update(Message::DisableMasterPassword);
    let disable = |master: &str| Message::SubmitVault {
        password: Secret::new(master.to_owned()),
        new: None,
        confirm: None,
    };
    let effects = app.update(disable("not the master"));
    run_vault_job(&mut app, effects).await;
    assert_eq!(vault_problem(&app), Some(VaultProblem::Unreadable));
    assert!(path.is_file(), "a wrong master password removes nothing");
    assert_eq!(system_entries(&system), 0);

    let effects = app.update(disable(MASTER));
    run_vault_job(&mut app, effects).await;
    assert!(app.dialog.is_none(), "{:?}", app.dialog);
    assert_eq!(app.vault_status(), VaultStatus::Missing);
    assert!(!path.exists(), "the vault is deleted");
    assert!(
        !sealvault::backup_path(&path).exists(),
        "and the copy kept beside it"
    );
    assert_eq!(system_entries(&system), 2, "the server's and the gateway's");
    assert_eq!(first_answer(&mut app, "a.lab").as_deref(), Some(PASSWORD));

    let mut restarted = self::app(dir.path(), "a.lab", &system);
    assert!(restarted.dialog.is_none(), "no master password to ask for");
    assert_eq!(
        first_answer(&mut restarted, "a.lab").as_deref(),
        Some(PASSWORD)
    );
}

#[tokio::test]
async fn the_master_password_is_changed_or_removed_only_while_the_vault_is_open() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path(), "a.lab", &SystemCredentials::memory());
    for message in [
        Message::ChangeMasterPassword,
        Message::DisableMasterPassword,
    ] {
        app.update(message);
        assert!(app.dialog.is_none(), "no master password: {:?}", app.dialog);
    }
    unlock(&mut app, MASTER).await;
    app.update(Message::LockVault);
    for message in [
        Message::ChangeMasterPassword,
        Message::DisableMasterPassword,
    ] {
        app.update(message);
        assert!(app.is_locked(), "the lock screen stays: {:?}", app.dialog);
    }
}

#[tokio::test]
async fn without_a_system_store_the_master_password_cannot_be_removed() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path(), "a.lab", &SystemCredentials::Unavailable);
    unlock(&mut app, MASTER).await;
    assert_eq!(app.vault_status(), VaultStatus::Open);
    app.update(Message::DisableMasterPassword);
    let effects = app.update(Message::SubmitVault {
        password: Secret::new(MASTER.to_owned()),
        new: None,
        confirm: None,
    });
    assert!(effects.is_empty(), "{effects:?}");
    assert_eq!(vault_problem(&app), Some(VaultProblem::NoSystemStore));
    assert!(dir.path().join(heimdall_app::VAULT_FILE_NAME).is_file());
}

#[tokio::test]
async fn a_locked_workspace_stays_locked_until_the_master_password_is_typed() {
    let dir = tempfile::tempdir().expect("dir");
    let system = SystemCredentials::memory();
    let mut app = saved(dir.path(), &system);
    unlock(&mut app, MASTER).await;
    assert!(!app.is_locked());

    app.update(Message::LockVault);
    assert!(app.is_locked());
    assert_eq!(app.vault_status(), VaultStatus::Locked);
    assert!(
        app.update(Message::DismissDialog).is_empty(),
        "the lock screen has no Cancel"
    );
    assert!(app.is_locked(), "still locked: {:?}", app.dialog);

    let submit = |master: &str| Message::SubmitVault {
        password: Secret::new(master.to_owned()),
        new: None,
        confirm: None,
    };
    let effects = app.update(submit("not the master"));
    run_vault_job(&mut app, effects).await;
    assert!(app.is_locked());
    assert_eq!(vault_problem(&app), Some(VaultProblem::Unreadable));
    let effects = app.update(submit(MASTER));
    run_vault_job(&mut app, effects).await;
    assert!(!app.is_locked(), "{:?}", app.dialog);
    assert_eq!(app.vault_status(), VaultStatus::Open);
    assert_eq!(first_answer(&mut app, "a.lab").as_deref(), Some(PASSWORD));
}

#[test]
fn without_a_master_password_there_is_nothing_to_lock() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path(), "a.lab", &SystemCredentials::memory());
    app.update(Message::LockVault);
    assert!(!app.is_locked());
    assert!(app.dialog.is_none(), "{:?}", app.dialog);
}

#[tokio::test]
async fn cancelling_the_master_password_at_start_quits() {
    let dir = tempfile::tempdir().expect("dir");
    let system = SystemCredentials::memory();
    let mut app = app(dir.path(), "a.lab", &system);
    unlock(&mut app, MASTER).await;
    drop(app);
    let mut restarted = self::app(dir.path(), "a.lab", &system);
    assert!(
        matches!(&restarted.dialog, Some(Dialog::Vault(dialog)) if dialog.mode == VaultMode::Unlock)
    );
    assert!(matches!(
        restarted.update(Message::DismissDialog).as_slice(),
        [Effect::Exit]
    ));
}
