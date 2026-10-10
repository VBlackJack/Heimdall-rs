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

//! The folders Heimdall trusts or starts programs from come from the system, never from the
//! environment, which whoever starts Heimdall sets.

#[cfg(not(windows))]
#[test]
fn the_citrix_cache_has_no_folder_off_windows() {
    assert_eq!(heimdall_core::import::citrix_cache::cache_folder(), None);
}

#[cfg(not(windows))]
#[test]
fn no_windows_program_is_named_off_windows() {
    use heimdall_core::paths::{EXPLORER_PROGRAM, RUNDLL_PROGRAM, system_program, windows_program};
    assert_eq!(system_program(RUNDLL_PROGRAM), None);
    assert_eq!(windows_program(EXPLORER_PROGRAM), None);
}

#[cfg(windows)]
mod windows {
    use std::path::{Path, PathBuf};
    use std::process::Command;

    use heimdall_core::paths;

    /// Set in the child run only: the probe then prints the folders it resolves.
    const PROBE_VARIABLE: &str = "HEIMDALL_KNOWN_FOLDERS_PROBE";

    /// The probe's full name, as the test harness filters it.
    const PROBE_TEST: &str = "windows::probe_prints_the_folders_when_asked";

    /// What starts each line the probe prints.
    const LINE_PREFIX: &str = "known-folder ";

    /// The variables that name folders on Windows, each planted in the child run.
    const PLANTED_VARIABLES: [&str; 13] = [
        "LOCALAPPDATA",
        "APPDATA",
        "ProgramFiles",
        "ProgramFiles(x86)",
        "ProgramW6432",
        "SystemRoot",
        "windir",
        "USERPROFILE",
        "HOMEDRIVE",
        "HOMEPATH",
        "HOME",
        "TEMP",
        "TMP",
    ];

    /// Every folder resolved from the system, by name.
    fn folders() -> Vec<(&'static str, Option<PathBuf>)> {
        vec![
            ("local_app_data", paths::local_app_data()),
            ("program_files", paths::program_files()),
            ("program_files_x86", paths::program_files_x86()),
            ("system_root", paths::system_root()),
            ("system_dir", paths::system_dir()),
            ("home_dir", paths::home_dir()),
            ("legacy_data_dir", paths::legacy_data_dir()),
            ("rundll", paths::system_program(paths::RUNDLL_PROGRAM)),
            ("explorer", paths::windows_program(paths::EXPLORER_PROGRAM)),
            (
                "citrix_cache",
                heimdall_core::import::citrix_cache::cache_folder(),
            ),
        ]
    }

    /// The folders as the probe prints them, one line each.
    fn lines(folders: &[(&str, Option<PathBuf>)]) -> Vec<String> {
        folders
            .iter()
            .map(|(name, folder)| format!("{LINE_PREFIX}{name}={folder:?}"))
            .collect()
    }

    /// Whether `folder` is `ancestor` or under it, without regard to case, as Windows
    /// compares paths.
    fn under(folder: &Path, ancestor: &Path) -> bool {
        let lower = |path: &Path| path.to_string_lossy().to_lowercase();
        lower(folder).starts_with(&lower(ancestor))
    }

    #[test]
    fn probe_prints_the_folders_when_asked() {
        if std::env::var_os(PROBE_VARIABLE).is_none() {
            return;
        }
        for line in lines(&folders()) {
            println!("{line}");
        }
    }

    #[test]
    fn folders_planted_in_the_environment_change_nothing() {
        let planted = tempfile::tempdir().expect("planted folder");
        // A planted profile with the folders Windows looks for in one, so that a folder
        // expanded from the environment would be found there, not refused.
        std::fs::create_dir_all(planted.path().join("AppData").join("Local"))
            .expect("planted profile");
        let mut command = Command::new(std::env::current_exe().expect("test program"));
        command
            .args([PROBE_TEST, "--exact", "--nocapture", "--test-threads=1"])
            .env(PROBE_VARIABLE, "1");
        for variable in PLANTED_VARIABLES {
            command.env(variable, planted.path());
        }
        let output = command.output().expect("probe run");
        assert!(output.status.success(), "probe failed: {output:?}");
        let printed: Vec<String> = String::from_utf8_lossy(&output.stdout)
            .lines()
            // The harness may print the test's name on the same line as its first output.
            .filter_map(|line| line.find(LINE_PREFIX).map(|start| line[start..].to_owned()))
            .collect();
        let resolved = folders();
        assert_eq!(printed, lines(&resolved));
        for (name, folder) in resolved {
            let folder = folder.unwrap_or_else(|| panic!("{name} unresolved"));
            assert!(
                !under(&folder, planted.path()),
                "{name} follows the environment: {}",
                folder.display()
            );
        }
    }

    #[test]
    fn the_folders_are_full_paths_that_exist() {
        for (name, folder) in folders() {
            if matches!(
                name,
                "citrix_cache" | "legacy_data_dir" | "rundll" | "explorer"
            ) {
                continue;
            }
            let folder = folder.unwrap_or_else(|| panic!("{name} unresolved"));
            assert!(folder.is_absolute(), "{name}: {}", folder.display());
            assert!(folder.is_dir(), "{name}: {}", folder.display());
        }
    }

    #[test]
    fn the_system_folder_is_in_the_windows_folder() {
        let system = paths::system_dir().expect("system folder");
        let root = paths::system_root().expect("Windows folder");
        assert!(
            system
                .parent()
                .is_some_and(|parent| under(parent, &root) && under(&root, parent)),
            "{} not in {}",
            system.display(),
            root.display()
        );
    }

    #[test]
    fn windows_programs_are_named_by_their_whole_path_in_their_folder() {
        let rundll = paths::system_program(paths::RUNDLL_PROGRAM).expect("rundll32");
        assert_eq!(
            rundll,
            paths::system_dir()
                .expect("system folder")
                .join(paths::RUNDLL_PROGRAM)
        );
        assert!(rundll.is_file(), "{}", rundll.display());
        let explorer = paths::windows_program(paths::EXPLORER_PROGRAM).expect("explorer");
        assert_eq!(
            explorer,
            paths::system_root()
                .expect("Windows folder")
                .join(paths::EXPLORER_PROGRAM)
        );
        assert!(explorer.is_file(), "{}", explorer.display());
    }

    #[test]
    fn the_local_application_data_is_the_users_own() {
        let home = paths::home_dir().expect("home");
        let local = paths::local_app_data().expect("local application data");
        assert!(under(&local, &home), "{}", local.display());
    }

    #[test]
    fn the_citrix_cache_is_under_the_local_application_data() {
        let local = paths::local_app_data().expect("local application data");
        let cache = heimdall_core::import::citrix_cache::cache_folder().expect("cache folder");
        assert_eq!(cache, local.join("Citrix").join("SelfService"));
    }
}
