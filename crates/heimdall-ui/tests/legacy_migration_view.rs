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

//! What the window says of the migration from the legacy PowerShell Heimdall: the offer, with
//! the folder found and what it holds, then the result as the C# `MigrationPresentationPolicy`
//! words it.
//!
//! The texts are one loader for the whole process, and one test reads them in French: every
//! test here holds the same lock and starts in English.

use std::fs;
use std::path::Path;
use std::sync::{Mutex, MutexGuard, PoisonError};

use heimdall_app::rdpmanager::{self, Offer};
use heimdall_app::{
    App, AppConfig, Dialog, LegacyMigrationDone, LegacyMigrationMessage, Message, SystemCredentials,
};
use heimdall_core::import::csharp::SkipReason;
use heimdall_core::import::rdpmanager::{
    LEGACY_APP_FOLDER_NAME, LeftOut, LeftOutBecause, servers_file, settings_file,
};
use heimdall_core::settings::Language;
use heimdall_ssh::AgentSource;
use heimdall_term::GridSize;
use heimdall_ui::legacy_migration_view::{self, MAX_LEFT_OUT_SHOWN};

/// The language the texts are read in, held by one test at a time.
static LANGUAGE: Mutex<()> = Mutex::new(());

/// The texts held in English until dropped, and put back in English then.
struct English {
    _held: MutexGuard<'static, ()>,
}

impl Drop for English {
    fn drop(&mut self) {
        heimdall_ui::i18n::apply(Some(Language::English));
    }
}

fn english() -> English {
    let guard = LANGUAGE.lock().unwrap_or_else(PoisonError::into_inner);
    heimdall_ui::i18n::apply(Some(Language::English));
    English { _held: guard }
}

/// The legacy files in `dir`, offered.
fn offer(dir: &Path, settings: &str, servers: &str) -> Offer {
    let folder = dir.join(LEGACY_APP_FOLDER_NAME);
    fs::create_dir_all(settings_file(&folder).parent().expect("config")).expect("config");
    fs::write(settings_file(&folder), settings).expect("settings");
    fs::write(servers_file(&folder), servers).expect("servers");
    rdpmanager::offer(&folder).expect("offered")
}

/// What the migration of `offer` did, in an application over `dir`.
fn migrated(dir: &Path, offer: Offer) -> LegacyMigrationDone {
    let mut app = App::new(AppConfig {
        profiles_file: dir.join("profiles.toml"),
        known_hosts: dir.join("known_hosts"),
        legacy_dir: None,
        agent: AgentSource::Disabled,
        initial_grid: GridSize { cols: 80, rows: 24 },
        files_start: dir.to_owned(),
        system_credentials: SystemCredentials::memory(),
    });
    app.update(Message::LegacyMigration(LegacyMigrationMessage::Found(
        Some(Box::new(offer)),
    )));
    app.update(Message::ConfirmDialog);
    match app.dialog {
        Some(Dialog::LegacyMigrationDone(done)) => *done,
        other => panic!("{other:?}"),
    }
}

#[test]
fn the_offer_names_the_folder_and_what_it_holds_and_the_result_what_was_left_out() {
    let _english = english();
    let dir = tempfile::tempdir().expect("dir");
    let offer = offer(
        dir.path(),
        r#"{"SshGateways": [{"Id": "gw", "Host": "bastion"}], "Projects": [{"Id": "p"}, {"Id": "q"}]}"#,
        r#"{"Id": "solo", "DisplayName": "Solo", "RemoteServer": "10.0.0.1", "ConnectionType": "SSH"}"#,
    );
    let (title, question, decline, confirm) = legacy_migration_view::offer(&offer);
    assert_eq!(title, "Legacy Migration");
    let folder = dir.path().join(LEGACY_APP_FOLDER_NAME);
    assert_eq!(
        question,
        format!(
            "A legacy Heimdall installation was detected in {}, holding 1 server and 1 SSH gateway. Would you like to import your servers and settings?",
            folder.display()
        )
    );
    assert_eq!(decline, "Do not import");
    assert_eq!(confirm, "Import");

    let done = migrated(dir.path(), offer);
    let (_, lines, warns) = legacy_migration_view::done(&done);
    assert!(!warns, "nothing left out");
    assert_eq!(lines[0], "1 server imported successfully.");
    assert!(
        lines
            .iter()
            .any(|line| line == "2 projects were not imported: Heimdall has no projects.")
    );
    assert!(
        lines
            .iter()
            .any(|line| line.starts_with("Passwords, passphrases, the PIN")),
        "{lines:?}"
    );
}

#[test]
fn the_profiles_left_out_are_named_by_place_five_at_most() {
    let _english = english();
    let dir = tempfile::tempdir().expect("dir");
    let mut done = migrated(dir.path(), offer(dir.path(), "{}", "[]"));
    done.examined = 8;
    done.key_paths_left_out = 2;
    done.imported = 1;
    done.left_out = (2..=8)
        .map(|index| LeftOut {
            index,
            name: (index != 2).then(|| format!("Server {index}")),
            reason: if index == 3 {
                LeftOutBecause::Refused(SkipReason::MissingHost)
            } else {
                LeftOutBecause::InvalidLegacyField
            },
        })
        .collect();
    let (_, lines, warns) = legacy_migration_view::done(&done);
    assert!(warns);
    assert_eq!(
        lines[0],
        "Migration completed with skipped profiles: 8 examined, 1 imported, 7 skipped."
    );
    assert_eq!(lines[1], "#2 - Unnamed profile: invalid legacy field value");
    assert_eq!(lines[2], "#3 - Server 3: has no host");
    assert_eq!(
        lines[MAX_LEFT_OUT_SHOWN],
        "#6 - Server 6: invalid legacy field value"
    );
    assert_eq!(
        lines[MAX_LEFT_OUT_SHOWN + 1],
        "2 additional skipped profiles not shown."
    );
    let secrets = lines
        .iter()
        .position(|line| line.starts_with("Passwords, passphrases, the PIN"))
        .expect("secrets");
    assert_eq!(
        lines[secrets + 1],
        "2 key paths were left out: they were not files on a local drive (network, device or relative paths). Choose the keys again in the profiles or gateways."
    );

    heimdall_ui::i18n::apply(Some(Language::French));
    let (title, lines, _) = legacy_migration_view::done(&done);
    assert_eq!(title, "Migration");
    assert_eq!(
        lines[0],
        "Migration terminée avec des profils ignorés : 8 examinés, 1 importé, 7 ignorés."
    );
    assert_eq!(
        lines[1],
        "# 2 - Profil sans nom : valeur de champ hérité invalide"
    );
}
