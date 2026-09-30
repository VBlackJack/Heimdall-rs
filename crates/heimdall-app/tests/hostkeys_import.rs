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

//! "Trusted SSH hosts...": another `known_hosts` file previewed against the keys already
//! trusted, only new keys chosen, then trusted, as the C# Heimdall's.

use std::path::Path;

use heimdall_app::{
    App, AppConfig, Dialog, Effect, HostKeysMessage, HostKeysOutcome, Message, SettingsMessage,
    SystemCredentials, TrustedKeysMessage,
};
use heimdall_ssh::known_hosts_import::HostKeyStatus;
use heimdall_ssh::{AgentSource, KnownHosts, PublicKey};
use heimdall_term::GridSize;

/// A host key fixture's `type base64` part.
fn key(name: &str) -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../heimdall-ssh/tests/fixtures/hostkeys")
        .join(format!("{name}.pub"));
    let text = std::fs::read_to_string(path).expect("fixture");
    let mut fields = text.split_whitespace();
    format!(
        "{} {}",
        fields.next().expect("type"),
        fields.next().expect("key")
    )
}

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

fn import(app: &mut App, message: HostKeysMessage) -> Vec<Effect> {
    app.update(Message::Settings(SettingsMessage::TrustedKeys(
        TrustedKeysMessage::Import(message),
    )))
}

fn store(dir: &Path) -> KnownHosts {
    KnownHosts::new(dir.join("known_hosts"))
}

/// An application trusting `web.lab` with the first key, shown a file giving `web.lab`
/// again, `db.lab` with a key that contradicts none, a changed key for `web.lab`'s kind
/// on another server trusted, and a line it cannot read.
fn previewing(dir: &Path) -> App {
    let (ed, other) = (key("host-ed25519"), key("host-ed25519-other"));
    store(dir)
        .learn(
            "changed.lab",
            22,
            &PublicKey::from_openssh(&ed).expect("key"),
        )
        .expect("learn");
    store(dir)
        .learn("web.lab", 22, &PublicKey::from_openssh(&ed).expect("key"))
        .expect("learn");
    let mut app = app(dir);
    let text = format!(
        "web.lab {ed}\n[db.lab]:2222 {ed}\nchanged.lab {other}\nshort-line\n|1|c2FsdA==|aGFzaA== {ed}\n"
    );
    import(&mut app, HostKeysMessage::Read(Ok(text)));
    app
}

fn preview(app: &App) -> &heimdall_app::HostKeysPreview {
    match &app.dialog {
        Some(Dialog::HostKeysPreview(preview)) => preview,
        other => panic!("not a preview: {other:?}"),
    }
}

#[test]
fn the_import_starts_by_asking_for_the_file_from_the_settings_family() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path());
    let effects = import(&mut app, HostKeysMessage::Start);
    assert!(matches!(effects.as_slice(), [Effect::PickKnownHosts]));
    assert!(app.dialog.is_none());
}

#[test]
fn the_preview_says_each_key_new_trusted_or_in_conflict_and_ticks_only_new_ones() {
    let dir = tempfile::tempdir().expect("dir");
    let app = previewing(dir.path());
    let preview = preview(&app);
    let rows: Vec<(&str, u16, HostKeyStatus, bool)> = preview
        .rows
        .iter()
        .map(|row| {
            (
                row.candidate.host.as_str(),
                row.candidate.port,
                row.status,
                row.chosen,
            )
        })
        .collect();
    assert_eq!(
        rows,
        [
            ("web.lab", 22, HostKeyStatus::Existing, false),
            ("db.lab", 2222, HostKeyStatus::New, true),
            ("changed.lab", 22, HostKeyStatus::Conflict, false),
        ]
    );
    assert!(
        preview.rows[1].fingerprint.starts_with("SHA256:"),
        "{}",
        preview.rows[1].fingerprint
    );
    assert_eq!(preview.counts(), (3, 1, 1, 1));
    assert_eq!(
        preview.diagnostics.len(),
        2,
        "the short line and the hashed host"
    );
    assert!(preview.can_import());
    assert!(preview.all_chosen());
}

#[test]
fn only_a_new_key_can_be_chosen_and_choosing_none_leaves_nothing_to_import() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = previewing(dir.path());
    import(&mut app, HostKeysMessage::Choose(0));
    import(&mut app, HostKeysMessage::Choose(2));
    import(&mut app, HostKeysMessage::Choose(9));
    let chosen: Vec<bool> = preview(&app).rows.iter().map(|row| row.chosen).collect();
    assert_eq!(
        chosen,
        [false, true, false],
        "trusted and conflict stay unticked"
    );

    import(&mut app, HostKeysMessage::Choose(1));
    assert!(!preview(&app).can_import());
    import(&mut app, HostKeysMessage::ChooseAll(true));
    let chosen: Vec<bool> = preview(&app).rows.iter().map(|row| row.chosen).collect();
    assert_eq!(
        chosen,
        [false, true, false],
        "all means every new key, only"
    );
    import(&mut app, HostKeysMessage::ChooseAll(false));
    assert!(!preview(&app).can_import());
    assert!(!preview(&app).all_chosen());
}

#[test]
fn confirming_trusts_the_chosen_keys_and_says_what_it_did_as_the_csharp() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = previewing(dir.path());
    app.update(Message::Settings(SettingsMessage::TrustedKeys(
        TrustedKeysMessage::Refresh,
    )));
    let before = app.trusted_keys().ssh.len();
    app.update(Message::ConfirmDialog);
    assert_eq!(
        app.dialog,
        Some(Dialog::HostKeysDone {
            done: HostKeysOutcome {
                imported: 1,
                existing: 1,
                conflicts: 1
            },
            warnings: 1,
        }),
        "the short line is a warning; the hashed host is information"
    );
    assert_eq!(
        store(dir.path())
            .recorded("db.lab", 2222)
            .expect("read")
            .len(),
        1
    );
    assert_eq!(
        store(dir.path())
            .recorded("changed.lab", 22)
            .expect("read")
            .len(),
        1,
        "the conflicting key never written"
    );
    assert_eq!(
        app.trusted_keys().ssh.len(),
        before + 1,
        "the Settings list shows the key just trusted"
    );
}

#[test]
fn an_empty_file_an_unreadable_one_and_one_that_gives_only_diagnostics() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path());
    import(
        &mut app,
        HostKeysMessage::Read(Ok("# nothing\n\n".to_owned())),
    );
    assert_eq!(app.dialog, Some(Dialog::HostKeysEmpty));

    import(&mut app, HostKeysMessage::Read(Err("denied".to_owned())));
    assert_eq!(
        app.dialog,
        Some(Dialog::HostKeysUnreadable {
            detail: "denied".to_owned()
        })
    );

    // As the C#: diagnostics alone are still shown, with nothing to import.
    import(
        &mut app,
        HostKeysMessage::Read(Ok("short-line\n".to_owned())),
    );
    let preview = preview(&app);
    assert!(preview.rows.is_empty());
    assert_eq!(preview.diagnostics.len(), 1);
    assert!(!preview.can_import());
    assert!(
        !preview.all_chosen(),
        "nothing new: \"Import all\" is not ticked"
    );
}

#[test]
fn the_file_read_never_appears_in_a_log() {
    let message = HostKeysMessage::Read(Ok("secret.lab ssh-ed25519 AAAA".to_owned()));
    assert_eq!(format!("{message:?}"), "Read(..)");
    let wrapped = Message::Settings(SettingsMessage::TrustedKeys(TrustedKeysMessage::Import(
        message,
    )));
    assert!(!format!("{wrapped:?}").contains("secret.lab"));
}
