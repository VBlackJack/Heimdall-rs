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

//! The `--data-dir` data root of a normal build, as the C#
//! `ApplicationDataPathResolver.UseDataRoot`: it roots the application's own configuration
//! and data folders in the one given and files the saved passwords under a namespace of it,
//! and leaves the home and temporary folders the user's own. Its own process, where the root
//! is set once.

#[cfg(not(feature = "test-data-dir"))]
mod normal_build {
    use std::path::{Path, PathBuf};

    use heimdall_core::paths::{self, APPLICATION, DataRootError};

    /// A generic credential's target name a profile could name: only read, never moved.
    const TARGET: &str = "corp/admin";

    /// What a data root's credential namespace follows, after the application's name.
    const NAMESPACE_DIVIDER: &str = "-data-";

    /// Hex digits of a credential namespace: 6 bytes of a SHA-256.
    const NAMESPACE_HEX_DIGITS: usize = 12;

    #[test]
    fn the_data_root_moves_the_application_folders_and_passwords_and_is_set_once() {
        // Refused, nothing set: a relative folder, the user's temporary folder itself.
        assert_eq!(paths::data_root(), None);
        assert_eq!(
            paths::use_data_root(Path::new("relative")),
            Err(DataRootError::NotAbsolute(PathBuf::from("relative")))
        );
        assert!(matches!(
            paths::use_data_root(&std::env::temp_dir()),
            Err(DataRootError::Overlaps(..))
        ));
        // A folder holding the home folder, and one inside the real configuration folder, which
        // is never made.
        #[cfg(not(windows))]
        {
            let home = directories::BaseDirs::new().expect("a home");
            let above = home.home_dir().parent().expect("the home's parent");
            assert!(matches!(
                paths::use_data_root(above),
                Err(DataRootError::Overlaps(..))
            ));
            let real =
                directories::ProjectDirs::from("", "", APPLICATION).expect("the real folders");
            let inside = real.config_dir().join("inside");
            assert!(matches!(
                paths::use_data_root(&inside),
                Err(DataRootError::Overlaps(..))
            ));
            assert!(!inside.exists(), "nothing is made in a real folder");
        }
        assert_eq!(paths::data_root(), None);

        // The user's own folders and credentials, before the root.
        let real_temp = paths::temp_dir();
        let real_home = paths::home_dir();
        assert_eq!(paths::credential_service(APPLICATION), APPLICATION);

        let scratch = tempfile::tempdir().expect("scratch folder");
        let root = scratch.path().join("heimdall-demo");
        paths::use_data_root(&root).expect("a folder of its own is accepted");
        assert_eq!(paths::data_root(), Some(root.as_path()));
        assert!(root.is_dir(), "the folder is made");

        // The application's own folders are under the root.
        let config = paths::config_dir().expect("a configuration folder");
        let data = paths::data_dir().expect("a data folder");
        assert_eq!(config, root.join("config"));
        assert_eq!(data, root.join("data"));
        for (name, file) in [
            ("profiles", paths::profiles_file()),
            ("known_hosts", paths::known_hosts_file()),
        ] {
            assert!(file.expect(name).starts_with(&config), "{name}");
        }
        for (name, folder) in [("logs", paths::log_dir()), ("edit", paths::edit_dir())] {
            assert!(folder.expect(name).starts_with(&data), "{name}");
        }

        // The home and temporary folders stay the user's own.
        assert_eq!(paths::temp_dir(), real_temp);
        assert_eq!(paths::home_dir(), real_home);

        // The saved passwords are the root's own: never the user's real entries.
        let service = paths::credential_service(APPLICATION);
        let namespace = service
            .strip_prefix(&format!("{APPLICATION}{NAMESPACE_DIVIDER}"))
            .expect("the service is the application's, in the root's namespace");
        assert_eq!(namespace.len(), NAMESPACE_HEX_DIGITS);
        assert!(namespace.chars().all(|c| c.is_ascii_hexdigit()));
        // A Windows Credential Manager entry a profile names is only read, as the C# reads it.
        assert_eq!(paths::credential_target(TARGET), TARGET);

        // Set once: a second folder is refused, the first kept.
        assert_eq!(
            paths::use_data_root(&scratch.path().join("other")),
            Err(DataRootError::AlreadySet)
        );
        assert_eq!(paths::data_root(), Some(root.as_path()));
    }
}

#[cfg(feature = "test-data-dir")]
mod test_build {
    use heimdall_core::paths::{self, DataRootError};

    #[test]
    fn a_test_build_refuses_a_data_root() {
        let scratch = tempfile::tempdir().expect("scratch folder");
        assert_eq!(
            paths::use_data_root(&scratch.path().join("demo")),
            Err(DataRootError::TestBuild)
        );
        assert_eq!(paths::data_root(), None);
    }
}
