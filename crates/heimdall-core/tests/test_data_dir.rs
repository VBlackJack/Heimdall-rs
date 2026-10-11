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

//! The test data folder: with the `test-data-dir` feature, every per-user folder is rooted
//! in the one named and the credentials are filed under a namespace of it, and nothing
//! reaches a real folder or credential before it is set; without the feature, nothing
//! changes. Its own process: the folder, once set, is the process's.

use heimdall_core::paths;

/// The application's name in the system's credential store, as the application files it.
const SERVICE: &str = paths::APPLICATION;

/// A generic credential's target name a profile could name.
const TARGET: &str = "corp/admin";

#[cfg(feature = "test-data-dir")]
mod with_the_feature {
    use std::path::{Path, PathBuf};

    use heimdall_core::paths::{self, TestDataDirError, test_data};

    use super::{SERVICE, TARGET};

    /// Every per-user folder, by name.
    fn folders() -> Vec<(&'static str, Option<PathBuf>)> {
        let profiles = paths::profiles_file();
        let beside = |make: fn(&Path) -> PathBuf| profiles.as_deref().map(make);
        let folders = vec![
            ("config_dir", paths::config_dir()),
            ("profiles_file", paths::profiles_file()),
            ("known_hosts_file", paths::known_hosts_file()),
            ("data_dir", paths::data_dir()),
            ("log_dir", paths::log_dir()),
            ("edit_dir", paths::edit_dir()),
            ("home_dir", paths::home_dir()),
            ("openssh_known_hosts", paths::openssh_known_hosts()),
            ("legacy_data_dir", paths::legacy_data_dir()),
            ("temp_dir", paths::temp_dir()),
            ("settings", beside(heimdall_core::settings::settings_path)),
            (
                "session_snapshot",
                beside(heimdall_core::session_snapshot::snapshot_path),
            ),
            (
                "window_state",
                beside(heimdall_core::window_state::state_path),
            ),
        ];
        folders.into_iter().chain(windows_folders()).collect()
    }

    /// The per-user folders only Windows has, by name.
    fn windows_folders() -> Vec<(&'static str, Option<PathBuf>)> {
        #[cfg(windows)]
        return vec![
            ("local_app_data", paths::local_app_data()),
            ("roaming_app_data", paths::roaming_app_data()),
            ("documents", paths::documents()),
            (
                "citrix_cache",
                heimdall_core::import::citrix_cache::cache_folder(),
            ),
        ];
        #[cfg(not(windows))]
        return Vec::new();
    }

    #[test]
    fn nothing_is_real_before_and_everything_is_in_the_folder_after() {
        let scratch = tempfile::tempdir().expect("scratch folder");
        let root = scratch.path().join("heimdall-test");

        // Fail closed: before a folder is set, no per-user folder is known, no installation
        // is looked for, and the credentials go to a namespace no real entry has.
        assert_eq!(paths::test_data_dir(), None);
        for (name, folder) in folders() {
            assert_eq!(folder, None, "{name} known before the test folder is set");
        }
        assert_eq!(paths::program_dir(), None);
        let unset = format!(
            "{SERVICE}{}{}",
            test_data::NAMESPACE_DIVIDER,
            test_data::UNSET_NAMESPACE
        );
        assert_eq!(paths::credential_service(SERVICE), unset);
        assert_ne!(paths::credential_service(SERVICE), SERVICE);
        assert_eq!(
            paths::credential_target(TARGET),
            format!("{unset}{}{TARGET}", test_data::TARGET_DIVIDER)
        );

        // Refused: a relative folder, the user's temporary folder itself, a folder holding
        // the home folder, and the real configuration folder.
        assert_eq!(
            paths::use_test_data_dir(Path::new("relative")),
            Err(TestDataDirError::NotAbsolute(PathBuf::from("relative")))
        );
        assert!(matches!(
            paths::use_test_data_dir(&std::env::temp_dir()),
            Err(TestDataDirError::Overlaps(..))
        ));
        #[cfg(not(windows))]
        {
            let home = directories::BaseDirs::new().expect("a home");
            let above = home.home_dir().parent().expect("the home's parent");
            assert!(matches!(
                paths::use_test_data_dir(above),
                Err(TestDataDirError::Overlaps(..))
            ));
            let real = directories::ProjectDirs::from("", "", paths::APPLICATION)
                .expect("the real folders");
            assert!(matches!(
                paths::use_test_data_dir(&real.config_dir().join("inside")),
                Err(TestDataDirError::Overlaps(..))
            ));
            assert!(
                !real.config_dir().join("inside").exists(),
                "nothing is made in a real folder"
            );
        }
        assert_eq!(paths::test_data_dir(), None);

        paths::use_test_data_dir(&root).expect("a scratch folder is accepted");
        assert_eq!(paths::test_data_dir(), Some(root.as_path()));
        assert!(root.is_dir(), "the folder is made");

        for (name, folder) in folders() {
            let folder = folder.unwrap_or_else(|| panic!("{name} unresolved"));
            assert!(folder.starts_with(&root), "{name}: {}", folder.display());
        }
        assert_eq!(paths::config_dir(), Some(root.join(test_data::CONFIG)));
        assert_eq!(paths::data_dir(), Some(root.join(test_data::DATA)));
        assert_eq!(paths::program_dir(), None);

        // The credentials: a namespace of the folder, apart from the real and unset ones.
        let service = paths::credential_service(SERVICE);
        assert_ne!(service, SERVICE);
        assert_ne!(service, unset);
        let namespace = service
            .strip_prefix(&format!("{SERVICE}{}", test_data::NAMESPACE_DIVIDER))
            .expect("the service starts with the application's name");
        assert_eq!(namespace.len(), test_data::NAMESPACE_HASH_LEN * 2);
        assert!(namespace.chars().all(|c| c.is_ascii_hexdigit()));
        assert_eq!(
            paths::credential_target(TARGET),
            format!("{service}{}{TARGET}", test_data::TARGET_DIVIDER)
        );

        // Set once: a second folder is refused, the first kept.
        assert_eq!(
            paths::use_test_data_dir(&scratch.path().join("other")),
            Err(TestDataDirError::AlreadySet)
        );
        assert_eq!(paths::test_data_dir(), Some(root.as_path()));
    }

    #[test]
    fn folders_are_compared_as_the_file_system_resolves_them() {
        let scratch = tempfile::tempdir().expect("scratch folder");
        let real = scratch.path().join("real");
        std::fs::create_dir_all(real.join("sub")).expect("real folder");
        let other = scratch.path().join("other");
        std::fs::create_dir_all(&other).expect("other folder");

        // `..`, existing or not.
        assert!(paths::folders_overlap(
            &other.join("..").join("real"),
            &real
        ));
        assert!(paths::folders_overlap(
            &real.join("sub").join("..").join("new"),
            &real
        ));
        assert!(paths::folders_overlap(
            &other.join("missing").join("..").join("..").join("real"),
            &real
        ));
        assert!(!paths::folders_overlap(&other, &real));
        // Either way round, and not across a shared start of a name.
        assert!(paths::folders_overlap(&real, &real.join("sub")));
        assert!(!paths::folders_overlap(
            &scratch.path().join("realm"),
            &real
        ));
        // Without regard to case.
        assert!(paths::folders_overlap(&scratch.path().join("REAL"), &real));
        // Holding is one way only.
        assert!(paths::folder_holds(scratch.path(), &real));
        assert!(paths::folder_holds(&real, &real));
        assert!(!paths::folder_holds(&real.join("sub"), &real));

        // Windows' verbatim prefix, local and network, drops away.
        assert_eq!(
            paths::comparable(Path::new(r"\\?\C:\Users\Someone")),
            paths::comparable(Path::new(r"c:\users\someone"))
        );
        assert_eq!(
            paths::comparable(Path::new(r"\\?\UNC\server\share\x")),
            paths::comparable(Path::new(r"\\server\share\x"))
        );
        #[cfg(windows)]
        assert!(paths::folders_overlap(
            &PathBuf::from(format!(r"\\?\{}", real.join("sub").display())),
            &real
        ));

        // A link to a real folder is that folder, with or without what is under it.
        let link = scratch.path().join("link");
        #[cfg(unix)]
        let linked = std::os::unix::fs::symlink(&real, &link).is_ok();
        #[cfg(windows)]
        let linked = std::os::windows::fs::symlink_dir(&real, &link).is_ok();
        if linked {
            assert!(paths::folders_overlap(&link, &real));
            assert!(paths::folders_overlap(&link.join("sub"), &real));
            assert!(paths::folders_overlap(&link.join("not-yet"), &real));
            assert!(!paths::folders_overlap(&link, &other));
        }
        #[cfg(unix)]
        assert!(linked, "a symbolic link can be made here");
    }
}

#[cfg(not(feature = "test-data-dir"))]
#[test]
fn without_the_feature_the_folders_and_credentials_are_the_platform_ones() {
    assert_eq!(paths::credential_service(SERVICE), SERVICE);
    assert_eq!(paths::credential_target(TARGET), TARGET);
    assert_eq!(paths::temp_dir(), Some(std::env::temp_dir()));
    assert_eq!(
        paths::program_dir(),
        std::env::current_exe()
            .ok()
            .and_then(|program| program.parent().map(std::path::Path::to_path_buf))
    );
    #[cfg(not(windows))]
    {
        let platform = directories::ProjectDirs::from("", "", paths::APPLICATION);
        assert_eq!(
            paths::config_dir(),
            platform.as_ref().map(|dirs| dirs.config_dir().to_owned())
        );
        assert_eq!(
            paths::data_dir(),
            platform
                .as_ref()
                .map(|dirs| dirs.data_local_dir().to_owned())
        );
        assert_eq!(
            paths::home_dir(),
            directories::BaseDirs::new().map(|dirs| dirs.home_dir().to_owned())
        );
    }
}
