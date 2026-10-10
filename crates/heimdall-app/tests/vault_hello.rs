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

//! Windows Hello unlocking the vault, as the C# `VaultLifecycleService` and its dialogs: an
//! envelope enrolled while the vault is open, outside the vault file, opens it again from
//! the unlock dialog; anything else leaves the master password to type. Windows Hello is
//! played here by a credential kept in memory: no prompt is ever raised, and the system's
//! store is the one kept in memory.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime};

use heimdall_app::vault_hello::{
    self, CredentialStatus, ENVELOPE_LEN, Envelope, HelloFailure, KeyCredentials,
    master_password_due, reports_tpm2, status_failure,
};
use heimdall_app::{
    App, AppConfig, Dialog, Effect, Message, Notice, OpenedVault, SettingsMessage,
    SystemCredentials, VaultHelloCard, VaultHelloMessage, VaultHelloStatus, VaultMode,
    VaultProblem, VaultStatus, VaultTicket, open_vault,
};
use heimdall_core::settings::{SETTINGS_FILE_NAME, Settings};
use heimdall_core::store::ProfileStore;
use heimdall_ssh::{AgentSource, Secret};
use heimdall_term::GridSize;
use zeroize::Zeroizing;

const MASTER: &str = "correct horse battery staple";

/// A day, for the days the master password is asked again after.
const DAY: Duration = Duration::from_hours(24);

/// Windows Hello as these tests play it.
#[derive(Default)]
struct Hello {
    /// Whether a credential can be enrolled.
    available: bool,
    /// The credentials there, by name: each one's private part.
    credentials: HashMap<String, Vec<u8>>,
    /// Credentials created so far, each given a key of its own.
    created: u8,
    /// What the next prompt answers instead of signing, when set.
    refuse: Option<HelloFailure>,
    /// Each signature differs, as a signature scheme with randomness would.
    randomised: bool,
    /// Prompts raised.
    prompts: u32,
    /// Credentials deleted, by name.
    deleted: Vec<String>,
}

/// A handle on [`Hello`], handed to the core as its platform.
#[derive(Clone, Default)]
struct Fake(Arc<Mutex<Hello>>);

impl Fake {
    fn available() -> Self {
        let fake = Self::default();
        fake.with(|hello| hello.available = true);
        fake
    }

    fn with<T>(&self, change: impl FnOnce(&mut Hello) -> T) -> T {
        change(&mut self.0.lock().expect("hello"))
    }

    fn prompts(&self) -> u32 {
        self.with(|hello| hello.prompts)
    }
}

fn public_key(private: &[u8]) -> Vec<u8> {
    [b"public:".as_slice(), private].concat()
}

impl KeyCredentials for Fake {
    fn enrolment_available(&self) -> bool {
        self.with(|hello| hello.available)
    }

    fn create(&self, name: &str) -> Result<Vec<u8>, HelloFailure> {
        self.with(|hello| {
            hello.prompts += 1;
            if let Some(failure) = hello.refuse {
                return Err(failure);
            }
            hello.created += 1;
            let private = vec![hello.created; 32];
            hello.credentials.insert(name.to_owned(), private.clone());
            Ok(public_key(&private))
        })
    }

    fn open(&self, name: &str) -> Result<Vec<u8>, HelloFailure> {
        self.with(|hello| {
            hello
                .credentials
                .get(name)
                .map(|private| public_key(private))
                .ok_or(HelloFailure::NotFound)
        })
    }

    fn sign(&self, name: &str, challenge: &[u8]) -> Result<Zeroizing<Vec<u8>>, HelloFailure> {
        self.with(|hello| {
            hello.prompts += 1;
            if let Some(failure) = hello.refuse {
                return Err(failure);
            }
            let private = hello.credentials.get(name).ok_or(HelloFailure::NotFound)?;
            let mut signed = [private.as_slice(), challenge].concat();
            if hello.randomised {
                signed.push(u8::try_from(hello.prompts % 256).expect("a byte"));
            }
            Ok(Zeroizing::new(sealvault::hash::sha256(&signed).to_vec()))
        })
    }

    fn delete(&self, name: &str) {
        self.with(|hello| {
            hello.credentials.remove(name);
            hello.deleted.push(name.to_owned());
        });
    }
}

/// The application over `dir`, with `system` as the system's store.
fn app(dir: &Path, system: &SystemCredentials) -> App {
    let profiles_file = dir.join("profiles.toml");
    ProfileStore::open(&profiles_file)
        .expect("store")
        .save()
        .expect("save");
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

fn vault_path(dir: &Path) -> PathBuf {
    dir.join(heimdall_app::VAULT_FILE_NAME)
}

/// Runs the vault job `effects` asks for, as the UI would: the try it answers, and its
/// result.
async fn vault_job(effects: Vec<Effect>) -> (VaultTicket, Result<OpenedVault, VaultProblem>) {
    match <[Effect; 1]>::try_from(effects) {
        Ok(
            [
                Effect::OpenVault {
                    path,
                    password,
                    job,
                    ticket,
                },
            ],
        ) => (ticket, open_vault(path, password, job).await),
        other => panic!("expected OpenVault, got {other:?}"),
    }
}

/// Runs the vault job `effects` asks for and hands the result back, as the UI would.
async fn run_vault_job(app: &mut App, effects: Vec<Effect>) {
    let (ticket, result) = vault_job(effects).await;
    app.update(Message::VaultOpened(ticket, result));
}

/// A wrong master password typed into the dialog shown, its job answered as a wrong one is
/// without deriving the key; whether it was taken.
fn wrong_try(app: &mut App) -> bool {
    let effects = app.update(Message::SubmitVault {
        password: Secret::new("not the master".to_owned()),
        new: None,
        confirm: None,
    });
    let [Effect::OpenVault { ticket, .. }] = effects.as_slice() else {
        return false;
    };
    let ticket = *ticket;
    app.update(Message::VaultOpened(ticket, Err(VaultProblem::Unreadable)));
    true
}

/// Types `master` into the vault dialog shown, creating the vault when there is none.
async fn type_master(app: &mut App, master: &str) {
    if app.dialog.is_none() {
        app.update(Message::ShowVault);
    }
    let create =
        matches!(&app.dialog, Some(Dialog::Vault(dialog)) if dialog.mode == VaultMode::Create);
    let effects = app.update(Message::SubmitVault {
        password: Secret::new(master.to_owned()),
        new: None,
        confirm: create.then(|| Secret::new(master.to_owned())),
    });
    run_vault_job(app, effects).await;
}

/// Runs what a step of Windows Hello for the vault asks, with `fake` as Windows Hello, and
/// hands each answer back, as the UI would.
async fn run_hello(app: &mut App, fake: &Fake, effects: Vec<Effect>) {
    for effect in effects {
        let answer = match effect {
            Effect::CheckVaultHello => {
                VaultHelloMessage::Checked(vault_hello::check(fake.clone()).await)
            }
            Effect::EnrolVaultHello { data_key, previous } => VaultHelloMessage::Enrolled(
                vault_hello::enrol_away(fake.clone(), data_key, previous).await,
            ),
            Effect::UnlockVaultHello { path, envelope } => VaultHelloMessage::Unlocked(
                vault_hello::unlock_away(fake.clone(), path, envelope).await,
            ),
            Effect::DeleteVaultHelloCredential(name) => {
                vault_hello::delete_away(fake.clone(), name).await;
                continue;
            }
            other => panic!("not a Windows Hello effect: {other:?}"),
        };
        let more = app.update(Message::VaultHello(answer));
        Box::pin(run_hello(app, fake, more)).await;
    }
}

/// Sends `message` and runs what it asks of Windows Hello.
async fn hello(app: &mut App, fake: &Fake, message: VaultHelloMessage) {
    let effects = app.update(Message::VaultHello(message));
    run_hello(app, fake, effects).await;
}

/// A vault created with the master password, open, and Windows Hello asked whether it can
/// be enrolled, as the settings ask when shown.
async fn open_vault_in(dir: &Path, system: &SystemCredentials, fake: &Fake) -> App {
    let mut app = app(dir, system);
    type_master(&mut app, MASTER).await;
    assert_eq!(app.vault_status(), VaultStatus::Open);
    hello(&mut app, fake, VaultHelloMessage::Refresh).await;
    app
}

/// A vault created, Windows Hello enrolled for it, then the application started again: the
/// unlock dialog shown at start.
async fn enrolled_then_restarted(dir: &Path, system: &SystemCredentials, fake: &Fake) -> App {
    let mut app = open_vault_in(dir, system, fake).await;
    hello(&mut app, fake, VaultHelloMessage::Enable).await;
    assert_eq!(
        app.vault_hello_card().status,
        Some(VaultHelloStatus::Enabled)
    );
    drop(app);
    let restarted = self::app(dir, system);
    assert_eq!(dialog_mode(&restarted), Some(VaultMode::Unlock));
    restarted
}

fn dialog_mode(app: &App) -> Option<VaultMode> {
    match &app.dialog {
        Some(Dialog::Vault(dialog)) => Some(dialog.mode),
        _ => None,
    }
}

fn hello_offered(app: &App) -> bool {
    matches!(&app.dialog, Some(Dialog::Vault(dialog)) if dialog.hello)
}

fn vault_problem(app: &App) -> Option<VaultProblem> {
    match &app.dialog {
        Some(Dialog::Vault(dialog)) => dialog.problem.clone(),
        _ => None,
    }
}

/// The envelope the system's store keeps for the vault in `dir`.
fn kept(system: &SystemCredentials, dir: &Path) -> Option<Vec<u8>> {
    let SystemCredentials::Memory(entries) = system else {
        panic!("a store in memory");
    };
    entries
        .lock()
        .expect("entries")
        .get(&vault_hello::entry_name(&vault_path(dir)))
        .map(|bytes| bytes.to_vec())
}

/// Replaces the envelope the system's store keeps for the vault in `dir`.
fn keep(system: &SystemCredentials, dir: &Path, bytes: &[u8]) {
    let SystemCredentials::Memory(entries) = system else {
        panic!("a store in memory");
    };
    entries.lock().expect("entries").insert(
        vault_hello::entry_name(&vault_path(dir)),
        Zeroizing::new(bytes.to_vec()),
    );
}

#[tokio::test]
async fn enrolled_windows_hello_opens_the_vault_at_start_without_the_master_password() {
    let dir = tempfile::tempdir().expect("dir");
    let system = SystemCredentials::memory();
    let fake = Fake::available();
    let vault_before = {
        let mut app = open_vault_in(dir.path(), &system, &fake).await;
        assert_eq!(
            app.vault_hello_card(),
            VaultHelloCard {
                status: Some(VaultHelloStatus::Available),
                can_enable: true,
                can_disable: false,
            }
        );
        let effects = app.update(Message::VaultHello(VaultHelloMessage::Enable));
        assert_eq!(
            app.vault_hello_card().status,
            Some(VaultHelloStatus::Enrolling)
        );
        run_hello(&mut app, &fake, effects).await;
        assert_eq!(
            app.vault_hello_card(),
            VaultHelloCard {
                status: Some(VaultHelloStatus::Enabled),
                can_enable: false,
                can_disable: true,
            }
        );
        assert_eq!(
            fake.prompts(),
            2,
            "the credential created, then the challenge signed"
        );
        std::fs::read(vault_path(dir.path())).expect("vault")
    };
    // Outside the vault file, which enrolling left as it was.
    assert_eq!(
        std::fs::read(vault_path(dir.path())).expect("vault"),
        vault_before
    );
    assert_eq!(
        kept(&system, dir.path()).map(|bytes| bytes.len()),
        Some(ENVELOPE_LEN)
    );

    let mut app = app(dir.path(), &system);
    assert!(hello_offered(&app), "offered at start");
    let effects = app.update(Message::VaultHello(VaultHelloMessage::Unlock));
    assert!(
        matches!(&app.dialog, Some(Dialog::Vault(dialog)) if dialog.busy && dialog.hello_waiting)
    );
    run_hello(&mut app, &fake, effects).await;
    assert_eq!(app.vault_status(), VaultStatus::Open);
    assert!(app.dialog.is_none(), "{:?}", app.dialog);
    assert_eq!(fake.prompts(), 3);
}

#[tokio::test]
async fn the_lock_screen_offers_windows_hello_and_it_unlocks_the_workspace() {
    let dir = tempfile::tempdir().expect("dir");
    let system = SystemCredentials::memory();
    let fake = Fake::available();
    let mut app = open_vault_in(dir.path(), &system, &fake).await;
    hello(&mut app, &fake, VaultHelloMessage::Enable).await;

    app.update(Message::LockVault);
    assert!(app.is_locked());
    assert!(hello_offered(&app));
    hello(&mut app, &fake, VaultHelloMessage::Unlock).await;
    assert!(!app.is_locked(), "{:?}", app.dialog);
    assert_eq!(app.vault_status(), VaultStatus::Open);
}

#[tokio::test]
async fn without_an_enrolment_nothing_is_offered_and_unlock_does_nothing() {
    let dir = tempfile::tempdir().expect("dir");
    let system = SystemCredentials::memory();
    let fake = Fake::available();
    drop(open_vault_in(dir.path(), &system, &fake).await);
    let mut app = app(dir.path(), &system);
    assert!(!hello_offered(&app));
    assert!(
        app.update(Message::VaultHello(VaultHelloMessage::Unlock))
            .is_empty()
    );
    assert_eq!(fake.prompts(), 0);
}

#[tokio::test]
async fn a_prompt_dismissed_says_nothing_and_leaves_the_master_password_which_still_opens() {
    let dir = tempfile::tempdir().expect("dir");
    let system = SystemCredentials::memory();
    let fake = Fake::available();
    let mut app = enrolled_then_restarted(dir.path(), &system, &fake).await;
    for failure in [
        HelloFailure::UserCanceled,
        HelloFailure::UserPrefersPassword,
    ] {
        fake.with(|hello| hello.refuse = Some(failure));
        hello(&mut app, &fake, VaultHelloMessage::Unlock).await;
        assert_eq!(dialog_mode(&app), Some(VaultMode::Unlock), "{failure:?}");
        assert_eq!(vault_problem(&app), None, "silent: {failure:?}");
        assert!(
            matches!(&app.dialog, Some(Dialog::Vault(dialog)) if !dialog.busy && !dialog.hello_waiting)
        );
    }
    assert_eq!(app.vault_status(), VaultStatus::Locked);
    // The fallback: the master password, untouched by any of it.
    type_master(&mut app, MASTER).await;
    assert_eq!(app.vault_status(), VaultStatus::Open);
    assert!(app.dialog.is_none(), "{:?}", app.dialog);
}

#[tokio::test]
async fn each_other_failure_says_what_the_csharp_says_and_counts_no_wrong_master_password() {
    let dir = tempfile::tempdir().expect("dir");
    let system = SystemCredentials::memory();
    let fake = Fake::available();
    let mut app = enrolled_then_restarted(dir.path(), &system, &fake).await;
    for (failure, said) in [
        (
            HelloFailure::SecurityDeviceLocked,
            VaultProblem::HelloLocked,
        ),
        (HelloFailure::Unavailable, VaultProblem::HelloFailed),
        (HelloFailure::CryptoFailure, VaultProblem::HelloFailed),
        // The prompt left unanswered past the time limit, then cancelled.
        (HelloFailure::TimedOut, VaultProblem::HelloFailed),
    ] {
        fake.with(|hello| hello.refuse = Some(failure));
        hello(&mut app, &fake, VaultHelloMessage::Unlock).await;
        assert_eq!(vault_problem(&app), Some(said), "{failure:?}");
        assert_eq!(app.vault_status(), VaultStatus::Locked, "{failure:?}");
    }
    let saved = Settings::load(&dir.path().join(SETTINGS_FILE_NAME)).expect("settings");
    assert_eq!(
        saved.vault_unlock.failures(),
        0,
        "no master password was wrong"
    );
    fake.with(|hello| hello.refuse = None);
    hello(&mut app, &fake, VaultHelloMessage::Unlock).await;
    assert_eq!(app.vault_status(), VaultStatus::Open);
}

#[tokio::test]
async fn a_signature_that_changes_unwraps_nothing_and_the_master_password_is_asked() {
    let dir = tempfile::tempdir().expect("dir");
    let system = SystemCredentials::memory();
    let fake = Fake::available();
    let mut app = enrolled_then_restarted(dir.path(), &system, &fake).await;
    // Windows Hello signs with RSA PKCS#1 v1.5, which is deterministic; were it not, as here,
    // the key derived differs and nothing opens.
    fake.with(|hello| hello.randomised = true);
    hello(&mut app, &fake, VaultHelloMessage::Unlock).await;
    assert_eq!(vault_problem(&app), Some(VaultProblem::HelloFailed));
    assert_eq!(app.vault_status(), VaultStatus::Locked);
}

#[tokio::test]
async fn a_tampered_envelope_opens_nothing_whatever_byte_changed() {
    let dir = tempfile::tempdir().expect("dir");
    let system = SystemCredentials::memory();
    let fake = Fake::available();
    drop(enrolled_then_restarted(dir.path(), &system, &fake).await);
    let good = kept(&system, dir.path()).expect("kept");
    // Each field after the magic and the version: the vault id (its credential is then
    // another one), the public key's hash, the challenge, the salt, the date, the nonce, the
    // wrapped key and its tag.
    for (field, offset) in [
        ("vault id", 5),
        ("public key hash", 21),
        ("challenge", 53),
        ("salt", 85),
        ("enrolment time", 124),
        ("nonce", 125),
        ("wrapped key", 137),
        ("tag", ENVELOPE_LEN - 1),
    ] {
        let mut changed = good.clone();
        changed[offset] ^= 1;
        keep(&system, dir.path(), &changed);
        let mut app = app(dir.path(), &system);
        hello(&mut app, &fake, VaultHelloMessage::Unlock).await;
        assert_eq!(app.vault_status(), VaultStatus::Locked, "{field}");
        assert!(
            matches!(
                vault_problem(&app),
                Some(VaultProblem::HelloFailed | VaultProblem::HelloNotFound)
            ),
            "{field}: {:?}",
            vault_problem(&app)
        );
    }
    // Not an envelope at all: none is read, nothing is offered.
    for broken in [&good[..ENVELOPE_LEN - 1], b"HHVE".as_slice()] {
        keep(&system, dir.path(), broken);
        let app = app(dir.path(), &system);
        assert!(!hello_offered(&app));
    }
}

#[tokio::test]
async fn a_credential_replaced_or_removed_is_not_found_and_enrolling_again_follows_the_master_password()
 {
    let dir = tempfile::tempdir().expect("dir");
    let system = SystemCredentials::memory();
    let fake = Fake::available();
    let mut app = enrolled_then_restarted(dir.path(), &system, &fake).await;
    // Windows Hello reset: the credential made again under its name, another key.
    let name = Envelope::parse(&kept(&system, dir.path()).expect("kept"))
        .expect("an envelope")
        .credential_name();
    fake.with(|hello| {
        hello.credentials.insert(name.clone(), vec![0xEE; 32]);
    });
    let prompts = fake.prompts();
    hello(&mut app, &fake, VaultHelloMessage::Unlock).await;
    assert_eq!(vault_problem(&app), Some(VaultProblem::HelloNotFound));
    assert_eq!(
        fake.prompts(),
        prompts,
        "the public key is checked before any prompt"
    );
    // Removed altogether: the same.
    fake.with(|hello| hello.credentials.clear());
    hello(&mut app, &fake, VaultHelloMessage::Unlock).await;
    assert_eq!(vault_problem(&app), Some(VaultProblem::HelloNotFound));

    type_master(&mut app, MASTER).await;
    assert_eq!(app.vault_status(), VaultStatus::Open);
    assert!(
        matches!(app.dialog, Some(Dialog::ConfirmVaultHelloEnrolAgain)),
        "{:?}",
        app.dialog
    );
    let effects = app.update(Message::ConfirmDialog);
    run_hello(&mut app, &fake, effects).await;
    assert_eq!(
        app.vault_hello_card().status,
        Some(VaultHelloStatus::Enabled)
    );
    let again = Envelope::parse(&kept(&system, dir.path()).expect("kept")).expect("envelope");
    assert_eq!(again.credential_name(), name, "the vault id is kept");

    app.update(Message::LockVault);
    hello(&mut app, &fake, VaultHelloMessage::Unlock).await;
    assert_eq!(
        app.vault_status(),
        VaultStatus::Open,
        "enrolled again, it unlocks"
    );
}

#[tokio::test]
async fn enrolling_again_refused_after_the_master_password_says_so_and_the_vault_stays_open() {
    let dir = tempfile::tempdir().expect("dir");
    let system = SystemCredentials::memory();
    let fake = Fake::available();
    let mut app = enrolled_then_restarted(dir.path(), &system, &fake).await;
    fake.with(|hello| hello.credentials.clear());
    hello(&mut app, &fake, VaultHelloMessage::Unlock).await;
    type_master(&mut app, MASTER).await;
    fake.with(|hello| hello.refuse = Some(HelloFailure::UserCanceled));
    let effects = app.update(Message::ConfirmDialog);
    run_hello(&mut app, &fake, effects).await;
    assert_eq!(app.vault_status(), VaultStatus::Open);
    assert_eq!(app.notice(), Some(&Notice::VaultHelloEnrolAgainFailed));
}

#[tokio::test]
async fn the_master_password_is_asked_again_once_the_days_set_have_passed_since_it_was_typed() {
    let dir = tempfile::tempdir().expect("dir");
    let system = SystemCredentials::memory();
    let fake = Fake::available();
    let mut app = open_vault_in(dir.path(), &system, &fake).await;
    hello(&mut app, &fake, VaultHelloMessage::Enable).await;
    app.update(Message::Settings(SettingsMessage::VaultHelloMaxDays(30)));
    drop(app);

    // Never typed on this computer, as a vault just created: the master password first, as
    // the C# policy says.
    let mut app = self::app(dir.path(), &system);
    assert!(!hello_offered(&app));
    assert!(
        app.update(Message::VaultHello(VaultHelloMessage::Unlock))
            .is_empty()
    );
    type_master(&mut app, MASTER).await;
    let saved = Settings::load(&dir.path().join(SETTINGS_FILE_NAME)).expect("settings");
    let typed = saved.vault_last_master_unlock.expect("noted");
    assert!(SystemTime::now().duration_since(typed).expect("past") < DAY);
    drop(app);

    let mut app = self::app(dir.path(), &system);
    assert!(hello_offered(&app), "typed within the days");
    hello(&mut app, &fake, VaultHelloMessage::Unlock).await;
    assert_eq!(app.vault_status(), VaultStatus::Open);
    let saved = Settings::load(&dir.path().join(SETTINGS_FILE_NAME)).expect("settings");
    assert_eq!(
        saved.vault_last_master_unlock,
        Some(typed),
        "Windows Hello does not count as the master password"
    );
    drop(app);

    // Thirty-one days later.
    let mut settings = Settings::load(&dir.path().join(SETTINGS_FILE_NAME)).expect("settings");
    settings.vault_last_master_unlock = Some(typed - DAY * 31);
    settings
        .save(&dir.path().join(SETTINGS_FILE_NAME))
        .expect("saved");
    let app = self::app(dir.path(), &system);
    assert!(!hello_offered(&app), "due again");
}

#[test]
fn the_master_password_falls_due_as_the_csharp_policy_says() {
    let now = SystemTime::UNIX_EPOCH + DAY * 1000;
    assert!(!master_password_due(None, 0, now), "0 never asks");
    assert!(!master_password_due(Some(now - DAY * 5000), 0, now));
    assert!(master_password_due(None, 7, now), "never typed");
    assert!(
        !master_password_due(Some(now - DAY * 7), 7, now),
        "exactly the days"
    );
    assert!(master_password_due(
        Some(now - DAY * 7 - Duration::from_secs(1)),
        7,
        now
    ));
    // Unlike the C#: a time still to come is not fresh for as long as it lies ahead.
    assert!(
        master_password_due(Some(now + DAY), 7, now),
        "a clock put back, or a file edited"
    );
    assert!(
        !master_password_due(Some(now + DAY), 0, now),
        "0 still never asks"
    );
}

#[tokio::test]
async fn changing_the_master_password_keeps_windows_hello_and_locking_keeps_it_too() {
    let dir = tempfile::tempdir().expect("dir");
    let system = SystemCredentials::memory();
    let fake = Fake::available();
    let mut app = open_vault_in(dir.path(), &system, &fake).await;
    hello(&mut app, &fake, VaultHelloMessage::Enable).await;
    let envelope = kept(&system, dir.path());

    app.update(Message::ChangeMasterPassword);
    let new_master = "another horse battery staple";
    let effects = app.update(Message::SubmitVault {
        password: Secret::new(MASTER.to_owned()),
        new: Some(Secret::new(new_master.to_owned())),
        confirm: Some(Secret::new(new_master.to_owned())),
    });
    run_vault_job(&mut app, effects).await;
    assert!(app.dialog.is_none(), "{:?}", app.dialog);
    assert_eq!(
        kept(&system, dir.path()),
        envelope,
        "the envelope is the same"
    );

    app.update(Message::LockVault);
    assert!(hello_offered(&app));
    hello(&mut app, &fake, VaultHelloMessage::Unlock).await;
    assert_eq!(
        app.vault_status(),
        VaultStatus::Open,
        "the same data key under the new password"
    );
}

#[tokio::test]
async fn disabling_removes_the_envelope_and_its_credential() {
    let dir = tempfile::tempdir().expect("dir");
    let system = SystemCredentials::memory();
    let fake = Fake::available();
    let mut app = open_vault_in(dir.path(), &system, &fake).await;
    hello(&mut app, &fake, VaultHelloMessage::Enable).await;
    let name = Envelope::parse(&kept(&system, dir.path()).expect("kept"))
        .expect("envelope")
        .credential_name();

    hello(&mut app, &fake, VaultHelloMessage::Disable).await;
    assert_eq!(kept(&system, dir.path()), None);
    assert_eq!(fake.with(|hello| hello.deleted.clone()), [name]);
    assert_eq!(
        app.vault_hello_card(),
        VaultHelloCard {
            status: Some(VaultHelloStatus::Available),
            can_enable: true,
            can_disable: false,
        }
    );
    app.update(Message::LockVault);
    assert!(!hello_offered(&app));
}

#[tokio::test]
async fn removing_the_master_password_removes_windows_hello_and_a_new_vault_drops_a_stale_one() {
    let dir = tempfile::tempdir().expect("dir");
    let system = SystemCredentials::memory();
    let fake = Fake::available();
    let mut app = open_vault_in(dir.path(), &system, &fake).await;
    hello(&mut app, &fake, VaultHelloMessage::Enable).await;
    app.update(Message::DisableMasterPassword);
    let effects = app.update(Message::SubmitVault {
        password: Secret::new(MASTER.to_owned()),
        new: None,
        confirm: None,
    });
    let (ticket, result) = vault_job(effects).await;
    let effects = app.update(Message::VaultOpened(ticket, result));
    run_hello(&mut app, &fake, effects).await;
    assert_eq!(app.vault_status(), VaultStatus::Missing);
    assert_eq!(kept(&system, dir.path()), None);
    assert_eq!(fake.with(|hello| hello.deleted.len()), 1);
    assert_eq!(
        app.vault_hello_card().status,
        None,
        "no vault, nothing said"
    );

    // An envelope left behind by a vault deleted outside the application.
    let mut other = open_vault_in(&dir.path().join("other"), &system, &fake).await;
    hello(&mut other, &fake, VaultHelloMessage::Enable).await;
    let stale = kept(&system, &dir.path().join("other")).expect("kept");
    drop(other);
    keep(&system, dir.path(), &stale);
    let effects = {
        app.update(Message::ShowVault);
        app.update(Message::SubmitVault {
            password: Secret::new(MASTER.to_owned()),
            new: None,
            confirm: Some(Secret::new(MASTER.to_owned())),
        })
    };
    let (ticket, result) = vault_job(effects).await;
    let effects = app.update(Message::VaultOpened(ticket, result));
    run_hello(&mut app, &fake, effects).await;
    assert_eq!(app.vault_status(), VaultStatus::Open);
    assert_eq!(kept(&system, dir.path()), None, "the stale envelope went");
    app.update(Message::LockVault);
    assert!(!hello_offered(&app));
}

#[tokio::test]
async fn enrolling_needs_the_vault_open_windows_hello_and_a_tpm_and_any_failure_says_unavailable() {
    let dir = tempfile::tempdir().expect("dir");
    let system = SystemCredentials::memory();
    let fake = Fake::default();
    let mut app = open_vault_in(dir.path(), &system, &fake).await;
    assert_eq!(
        app.vault_hello_card(),
        VaultHelloCard {
            status: Some(VaultHelloStatus::Unavailable),
            can_enable: false,
            can_disable: false,
        }
    );
    // Pressed anyway, no key credential support or no TPM 2.0: nothing is created.
    hello(&mut app, &fake, VaultHelloMessage::Enable).await;
    assert_eq!(fake.prompts(), 0);
    assert_eq!(
        app.vault_hello_card().status,
        Some(VaultHelloStatus::Unavailable)
    );

    fake.with(|hello| {
        hello.available = true;
        hello.refuse = Some(HelloFailure::UserCanceled);
    });
    hello(&mut app, &fake, VaultHelloMessage::Refresh).await;
    hello(&mut app, &fake, VaultHelloMessage::Enable).await;
    assert_eq!(
        app.vault_hello_card().status,
        Some(VaultHelloStatus::Unavailable),
        "as the C# says any enrolment failure"
    );
    assert_eq!(kept(&system, dir.path()), None);

    app.update(Message::LockVault);
    app.update(Message::VaultHello(VaultHelloMessage::Enable));
    assert_eq!(
        app.vault_hello_card().status,
        Some(VaultHelloStatus::UnlockRequired)
    );
}

#[tokio::test]
async fn an_envelope_the_system_store_cannot_keep_is_not_enrolled_and_its_credential_goes() {
    let dir = tempfile::tempdir().expect("dir");
    let fake = Fake::available();
    let system = SystemCredentials::memory();
    let mut app = open_vault_in(dir.path(), &system, &fake).await;
    let effects = app.update(Message::VaultHello(VaultHelloMessage::Enable));
    // The store gone meanwhile: what enrolling made is undone.
    let [Effect::EnrolVaultHello { data_key, previous }] =
        <[Effect; 1]>::try_from(effects).unwrap_or_else(|other| panic!("{other:?}"))
    else {
        panic!("not an enrolment");
    };
    let enrolled = vault_hello::enrol_away(fake.clone(), data_key, previous).await;
    let name = enrolled.as_ref().expect("enrolled").credential_name();
    let SystemCredentials::Memory(entries) = &system else {
        panic!("memory");
    };
    // A lock held elsewhere stands for a store that fails: the write is refused.
    let poisoned = Arc::clone(entries);
    let _ = std::thread::spawn(move || {
        let _guard = poisoned.lock().expect("lock");
        panic!("poisons the store");
    })
    .join();
    let effects = app.update(Message::VaultHello(VaultHelloMessage::Enrolled(enrolled)));
    run_hello(&mut app, &fake, effects).await;
    assert_eq!(fake.with(|hello| hello.deleted.clone()), [name]);
    assert_eq!(
        app.vault_hello_card().status,
        Some(VaultHelloStatus::Unavailable)
    );
}

#[tokio::test]
async fn cancelling_the_dialog_while_windows_hello_is_asked_opens_nothing() {
    let dir = tempfile::tempdir().expect("dir");
    let system = SystemCredentials::memory();
    let fake = Fake::available();
    let mut app = enrolled_then_restarted(dir.path(), &system, &fake).await;
    let effects = app.update(Message::VaultHello(VaultHelloMessage::Unlock));
    assert!(matches!(
        app.update(Message::DismissDialog).as_slice(),
        [Effect::Exit]
    ));
    run_hello(&mut app, &fake, effects).await;
    assert_eq!(app.vault_status(), VaultStatus::Locked);
}

#[tokio::test]
async fn while_the_tries_are_locked_out_windows_hello_is_not_asked_either() {
    use heimdall_core::lockout::MAX_FAILED_ATTEMPTS;

    let dir = tempfile::tempdir().expect("dir");
    let system = SystemCredentials::memory();
    let fake = Fake::available();
    let mut app = enrolled_then_restarted(dir.path(), &system, &fake).await;
    for _ in 0..MAX_FAILED_ATTEMPTS {
        assert!(wrong_try(&mut app));
    }
    assert!(matches!(
        vault_problem(&app),
        Some(VaultProblem::LockedOut { .. })
    ));
    assert!(
        app.update(Message::VaultHello(VaultHelloMessage::Unlock))
            .is_empty()
    );
}

#[test]
fn an_envelope_reads_back_as_written_and_nothing_else_reads_as_one() {
    assert!(Envelope::parse(&[]).is_none());
    assert!(Envelope::parse(&[0; ENVELOPE_LEN]).is_none(), "no magic");
    let mut bytes = vec![0; ENVELOPE_LEN];
    bytes[..4].copy_from_slice(b"HHVE");
    bytes[4] = 1;
    let envelope = Envelope::parse(&bytes).expect("the layout");
    assert_eq!(envelope.to_bytes(), bytes);
    assert_eq!(envelope.enrolled_at(), Some(SystemTime::UNIX_EPOCH));
    // A date past what the clock can say, as the system's store may hand one back: none,
    // rather than a panic.
    let mut far = bytes.clone();
    far[117..125].copy_from_slice(&u64::MAX.to_be_bytes());
    assert_eq!(
        Envelope::parse(&far).expect("the layout").enrolled_at(),
        None
    );
    bytes[4] = 2;
    assert!(Envelope::parse(&bytes).is_none(), "another version");
    bytes[4] = 1;
    bytes.push(0);
    assert!(Envelope::parse(&bytes).is_none(), "longer");
    assert!(
        format!("{envelope:?}").starts_with("Envelope { credential: \"Heimdall.VaultHello."),
        "{envelope:?}"
    );
}

#[test]
fn each_vault_file_has_its_own_entry_in_the_system_store() {
    let one = vault_hello::entry_name(Path::new("/a/vault.hvlt"));
    let two = vault_hello::entry_name(Path::new("/b/vault.hvlt"));
    assert_ne!(one, two);
    assert!(one.starts_with("vault-hello/"), "{one}");
    assert_eq!(one, vault_hello::entry_name(Path::new("/a/vault.hvlt")));
}

#[test]
fn key_credential_statuses_map_as_the_csharp_maps_them() {
    for (status, failure) in [
        (CredentialStatus::Success, None),
        (CredentialStatus::NotFound, Some(HelloFailure::NotFound)),
        (
            CredentialStatus::UserCanceled,
            Some(HelloFailure::UserCanceled),
        ),
        (
            CredentialStatus::UserPrefersPassword,
            Some(HelloFailure::UserPrefersPassword),
        ),
        (
            CredentialStatus::SecurityDeviceLocked,
            Some(HelloFailure::SecurityDeviceLocked),
        ),
        (CredentialStatus::Other, Some(HelloFailure::Unavailable)),
    ] {
        assert_eq!(status_failure(status), failure, "{status:?}");
    }
}

#[test]
fn a_tpm_2_is_read_from_tpmtool_as_the_csharp_reads_it() {
    let present = "TPM Present: True\nTPM Version: 2.0\n";
    assert!(reports_tpm2(true, present));
    assert!(reports_tpm2(
        true,
        "TPM présent : Vrai\nVersion du TPM : 2.0"
    ));
    assert!(!reports_tpm2(false, present), "the tool failed");
    assert!(!reports_tpm2(true, "TPM Present: True\nTPM Version: 1.2"));
    assert!(!reports_tpm2(true, "TPM Present: False\nTPM Version: 2.0"));
}

#[test]
fn elsewhere_nothing_is_enrolled_and_nothing_unlocks() {
    if cfg!(windows) {
        return;
    }
    let system = vault_hello::SystemKeyCredentials;
    assert!(!system.enrolment_available());
    let key = Box::new(Zeroizing::new([0; sealvault::DATA_KEY_LEN]));
    assert_eq!(
        vault_hello::enrol(&system, &key, None, SystemTime::now()),
        Err(HelloFailure::Unavailable)
    );
}

#[tokio::test]
async fn a_master_password_result_arriving_late_opens_neither_the_lock_screen_nor_another_dialog() {
    let dir = tempfile::tempdir().expect("dir");
    let system = SystemCredentials::memory();
    let fake = Fake::available();
    let mut app = open_vault_in(dir.path(), &system, &fake).await;
    hello(&mut app, &fake, VaultHelloMessage::Enable).await;
    // A change of the master password submitted, then its dialog dismissed while the key is
    // derived; the workspace locked, and Windows Hello asked on the lock screen.
    app.update(Message::ChangeMasterPassword);
    let new_master = "another horse battery staple";
    let change = app.update(Message::SubmitVault {
        password: Secret::new(MASTER.to_owned()),
        new: Some(Secret::new(new_master.to_owned())),
        confirm: Some(Secret::new(new_master.to_owned())),
    });
    app.update(Message::DismissDialog);
    app.update(Message::LockVault);
    let asked = app.update(Message::VaultHello(VaultHelloMessage::Unlock));
    assert!(matches!(
        asked.as_slice(),
        [Effect::UnlockVaultHello { .. }]
    ));

    let (ticket, result) = vault_job(change).await;
    assert!(result.is_ok(), "the change itself went through");
    app.update(Message::VaultOpened(ticket, result));
    assert!(app.is_locked(), "{:?}", app.dialog);
    assert_eq!(app.vault_status(), VaultStatus::Locked);
    assert!(
        matches!(&app.dialog, Some(Dialog::Vault(dialog)) if dialog.hello_waiting),
        "still waiting for Windows Hello"
    );

    // Windows Hello's own answer is the one taken.
    run_hello(&mut app, &fake, asked).await;
    assert_eq!(app.vault_status(), VaultStatus::Open);
}

#[tokio::test]
async fn while_windows_hello_is_enrolled_the_master_password_is_neither_changed_nor_removed() {
    let dir = tempfile::tempdir().expect("dir");
    let system = SystemCredentials::memory();
    let fake = Fake::available();
    let mut app = open_vault_in(dir.path(), &system, &fake).await;
    let enrolling = app.update(Message::VaultHello(VaultHelloMessage::Enable));
    for message in [
        Message::ChangeMasterPassword,
        Message::DisableMasterPassword,
    ] {
        app.update(message);
        assert!(app.dialog.is_none(), "{:?}", app.dialog);
    }

    // The vault gone all the same before the enrolment ends: nothing is kept, and the
    // credential made for it goes.
    std::fs::remove_file(vault_path(dir.path())).expect("removed");
    let [Effect::EnrolVaultHello { data_key, previous }] =
        <[Effect; 1]>::try_from(enrolling).unwrap_or_else(|other| panic!("{other:?}"))
    else {
        panic!("not an enrolment");
    };
    let enrolled = vault_hello::enrol_away(fake.clone(), data_key, previous).await;
    let name = enrolled.as_ref().expect("enrolled").credential_name();
    let effects = app.update(Message::VaultHello(VaultHelloMessage::Enrolled(enrolled)));
    run_hello(&mut app, &fake, effects).await;
    assert_eq!(kept(&system, dir.path()), None);
    assert_eq!(fake.with(|hello| hello.deleted.clone()), [name]);
    assert_ne!(
        app.vault_hello_card().status,
        Some(VaultHelloStatus::Enabled)
    );
}

#[tokio::test]
async fn a_lockout_saved_before_a_restart_is_shown_at_once_and_windows_hello_waits_for_its_end() {
    use heimdall_core::lockout::MAX_FAILED_ATTEMPTS;

    let dir = tempfile::tempdir().expect("dir");
    let system = SystemCredentials::memory();
    let fake = Fake::available();
    let mut app = enrolled_then_restarted(dir.path(), &system, &fake).await;
    for _ in 0..MAX_FAILED_ATTEMPTS {
        assert!(wrong_try(&mut app));
    }
    drop(app);

    let mut app = self::app(dir.path(), &system);
    assert!(
        matches!(vault_problem(&app), Some(VaultProblem::LockedOut { .. })),
        "said at once: {:?}",
        vault_problem(&app)
    );
    let prompts = fake.prompts();
    assert!(
        app.update(Message::VaultHello(VaultHelloMessage::Unlock))
            .is_empty()
    );
    assert_eq!(fake.prompts(), prompts, "no prompt during the lockout");
    let saved = Settings::load(&dir.path().join(SETTINGS_FILE_NAME)).expect("settings");
    assert!(saved.vault_unlock.until().is_some(), "the lockout stands");
}

#[tokio::test]
async fn below_the_lockout_windows_hello_forgets_the_wrong_tries_as_the_csharp_resets_them() {
    let dir = tempfile::tempdir().expect("dir");
    let system = SystemCredentials::memory();
    let fake = Fake::available();
    let mut app = enrolled_then_restarted(dir.path(), &system, &fake).await;
    assert!(wrong_try(&mut app));
    assert!(wrong_try(&mut app));
    hello(&mut app, &fake, VaultHelloMessage::Unlock).await;
    assert_eq!(app.vault_status(), VaultStatus::Open);
    let saved = Settings::load(&dir.path().join(SETTINGS_FILE_NAME)).expect("settings");
    assert_eq!(saved.vault_unlock.failures(), 0);
}

#[cfg(unix)]
#[test]
fn a_tool_is_read_within_its_time_and_stopped_past_it() {
    use std::process::Command;

    let mut quick = Command::new("/bin/sh");
    quick.args(["-c", "printf 'TPM Present: True\\nTPM Version: 2.0\\n'"]);
    let (succeeded, text) =
        vault_hello::run_within(&mut quick, Duration::from_secs(10)).expect("answered");
    assert!(succeeded);
    assert!(reports_tpm2(succeeded, &text), "{text}");

    // Its output closed at once, then it lingers: the deadline still holds, and it is
    // stopped rather than waited for.
    let mut lingering = Command::new("/bin/sh");
    lingering.args(["-c", "exec >&-; exec sleep 30"]);
    let started = std::time::Instant::now();
    assert_eq!(
        vault_hello::run_within(&mut lingering, Duration::from_millis(300)),
        None
    );
    assert!(started.elapsed() < Duration::from_secs(10));

    let mut silent = Command::new("/bin/sh");
    silent.args(["-c", "exec sleep 30"]);
    assert_eq!(
        vault_hello::run_within(&mut silent, Duration::from_millis(300)),
        None
    );
}
