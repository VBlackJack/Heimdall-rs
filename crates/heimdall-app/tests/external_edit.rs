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

//! "Edit with external editor": the editor chosen, the folder of the user's own, and each
//! save sent only while the server's file is the one opened.

use heimdall_app::external_edit::{EditorRefused, editor};

#[test]
fn an_empty_setting_takes_the_system_s_own_editor_and_a_missing_one_is_said() {
    let system = editor("").expect("the system's own");
    let name = system
        .program
        .file_name()
        .map(|name| name.to_string_lossy().to_lowercase())
        .unwrap_or_default();
    if cfg!(windows) {
        assert_eq!(name, "notepad.exe");
    } else if cfg!(target_os = "macos") {
        assert_eq!((name.as_str(), system.arguments.len()), ("open", 1));
    } else {
        assert_eq!(name, "xdg-open");
    }
    assert!(matches!(
        editor("/no/such/editor"),
        Err(EditorRefused::NotFound(_))
    ));
}

#[cfg(unix)]
mod unix {
    use std::os::unix::fs::PermissionsExt as _;
    use std::path::Path;

    use heimdall_app::external_edit::{EditorRefused, edit_folder, editor};

    fn program(dir: &Path, name: &str) -> String {
        let path = dir.join(name);
        std::fs::write(&path, b"#!/bin/true\n").expect("program");
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).expect("mode");
        path.display().to_string()
    }

    #[test]
    fn a_shell_or_an_interpreter_is_never_an_editor_even_under_another_name() {
        let dir = tempfile::tempdir().expect("dir");
        assert!(matches!(editor("/bin/sh"), Err(EditorRefused::Runs(_))));
        for name in ["python3.12", "node18", "pwsh", "perl"] {
            let named = program(dir.path(), name);
            assert!(
                matches!(editor(&named), Err(EditorRefused::Runs(_))),
                "{name}"
            );
        }
        let disguised = dir.path().join("editor");
        std::os::unix::fs::symlink("/bin/sh", &disguised).expect("link");
        assert!(
            matches!(
                editor(&disguised.display().to_string()),
                Err(EditorRefused::Runs(_))
            ),
            "what a link leads to counts"
        );
        let real = program(dir.path(), "kate");
        let chosen = editor(&format!("  \"{real}\"  ")).expect("an editor");
        assert_eq!(chosen.program, std::fs::canonicalize(&real).expect("there"));
    }

    #[test]
    fn edits_go_in_new_folders_of_the_user_s_own_and_never_through_a_link() {
        let dir = tempfile::tempdir().expect("dir");
        let base = dir.path().join("edit");
        std::fs::create_dir(&base).expect("base");
        std::fs::set_permissions(&base, std::fs::Permissions::from_mode(0o755)).expect("open");
        let first = edit_folder(&base).expect("folder");
        let second = edit_folder(&base).expect("folder");
        assert_ne!(first, second, "a new folder each time");
        let mode =
            |path: &Path| std::fs::metadata(path).expect("there").permissions().mode() & 0o777;
        assert_eq!(mode(&base), 0o700, "taken back to the user only");
        assert_eq!(mode(&first), 0o700);

        let elsewhere = dir.path().join("elsewhere");
        std::fs::create_dir(&elsewhere).expect("elsewhere");
        let linked = dir.path().join("linked");
        std::os::unix::fs::symlink(&elsewhere, &linked).expect("link");
        let refused = edit_folder(&linked).expect_err("a link is not taken");
        assert_eq!(refused.kind(), std::io::ErrorKind::PermissionDenied);
        assert_eq!(
            std::fs::read_dir(&elsewhere).expect("listed").count(),
            0,
            "nothing made where it led"
        );
    }
}

#[cfg(unix)]
#[path = "../../heimdall-sftp/tests/common/mod.rs"]
mod sftp;

#[cfg(unix)]
#[tokio::test]
async fn a_save_is_sent_once_it_holds_still_and_never_over_a_change_on_the_server() {
    use std::os::unix::fs::PermissionsExt as _;
    use std::path::PathBuf;

    use heimdall_app::external_edit::{EditCheck, Editor, check_edit, start_edit};
    use heimdall_app::files::FilesError;
    use heimdall_files::RemoteSession;
    use tokio_util::sync::CancellationToken;

    let Some((_server, client)) = sftp::start().await else {
        return;
    };
    let client = RemoteSession::Sftp(client);
    let dir = tempfile::tempdir().expect("dir");
    let file = dir.path().join("nginx.conf");
    std::fs::write(&file, b"listen 80;\n").expect("file");
    let base = dir.path().join("edits");
    let quiet = Editor {
        program: PathBuf::from("true"),
        arguments: Vec::new(),
    };
    let mut session = sftp::step(start_edit(
        client.clone(),
        sftp::remote(&file),
        quiet,
        base,
        CancellationToken::new(),
    ))
    .await
    .expect("opened");
    assert_eq!(
        std::fs::read(&session.local).expect("copy"),
        b"listen 80;\n"
    );
    assert_eq!(
        std::fs::metadata(&session.local)
            .expect("copy")
            .permissions()
            .mode()
            & 0o777,
        0o600,
        "the user's only"
    );
    let look = async |session: &mut heimdall_app::external_edit::EditSession| {
        let check = sftp::step(check_edit(&client, session)).await;
        session.apply(&check);
        check
    };
    assert_eq!(look(&mut session).await, EditCheck::Unchanged);

    // Saved: seen once, sent when seen again unchanged.
    std::thread::sleep(std::time::Duration::from_millis(20));
    std::fs::write(&session.local, b"listen 443;\n").expect("saved");
    assert!(matches!(look(&mut session).await, EditCheck::Saving(_)));
    assert!(matches!(look(&mut session).await, EditCheck::Sent { .. }));
    assert_eq!(std::fs::read(&file).expect("sent"), b"listen 443;\n");
    assert_eq!(look(&mut session).await, EditCheck::Unchanged);

    // Saved with nothing changed: not sent again.
    std::thread::sleep(std::time::Duration::from_millis(20));
    std::fs::write(&session.local, b"listen 443;\n").expect("saved again");
    assert!(matches!(look(&mut session).await, EditCheck::Saving(_)));
    assert!(matches!(look(&mut session).await, EditCheck::Same(_)));

    // Changed on the server meanwhile: the save is kept, the server's file left as it is.
    std::fs::write(&file, b"listen 8080; # by someone else\n").expect("changed");
    std::thread::sleep(std::time::Duration::from_millis(20));
    std::fs::write(&session.local, b"listen 8443;\n").expect("saved");
    assert!(matches!(look(&mut session).await, EditCheck::Saving(_)));
    assert!(matches!(
        look(&mut session).await,
        EditCheck::Refused {
            error: FilesError::ChangedOnServer,
            ..
        }
    ));
    assert_eq!(
        std::fs::read(&file).expect("kept"),
        b"listen 8080; # by someone else\n"
    );
    assert_eq!(
        std::fs::read(&session.local).expect("kept"),
        b"listen 8443;\n"
    );
    assert_eq!(
        look(&mut session).await,
        EditCheck::Unchanged,
        "not tried again"
    );
}
