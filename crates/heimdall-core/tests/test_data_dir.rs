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
//! in the one named and the credentials are filed under a namespace of it; without the
//! feature, nothing changes. Its own process: the folder, once set, is the process's.

use heimdall_core::paths;

/// The application's name in the system's credential store, as the application files it.
const SERVICE: &str = paths::APPLICATION;

/// A generic credential's target name a profile could name.
const TARGET: &str = "corp/admin";

#[cfg(feature = "test-data-dir")]
#[test]
fn every_per_user_folder_and_credential_is_rooted_in_the_test_data_folder() {
    use std::path::{Path, PathBuf};

    use paths::{TestDataDirError, test_data};

    let scratch = tempfile::tempdir().expect("scratch folder");
    let root = scratch.path().join("heimdall-test");

    // Refused before anything is set: a relative folder, and one whose layout meets the
    // user's real configuration folder.
    assert_eq!(
        paths::use_test_data_dir(Path::new("relative")),
        Err(TestDataDirError::NotAbsolute(PathBuf::from("relative")))
    );
    if let Some(real) = paths::config_dir() {
        assert!(matches!(
            paths::use_test_data_dir(&real),
            Err(TestDataDirError::Overlaps(..))
        ));
    }
    assert_eq!(paths::test_data_dir(), None);
    let real_service = paths::credential_service(SERVICE);
    assert_eq!(real_service, SERVICE);

    paths::use_test_data_dir(&root).expect("a scratch folder is accepted");
    assert_eq!(paths::test_data_dir(), Some(root.as_path()));
    assert!(root.is_dir(), "the folder is made");

    let under = |name: &str, folder: Option<PathBuf>| {
        let folder = folder.unwrap_or_else(|| panic!("{name} unresolved"));
        assert!(folder.starts_with(&root), "{name}: {}", folder.display());
    };
    under("config_dir", paths::config_dir());
    under("profiles_file", paths::profiles_file());
    under("known_hosts_file", paths::known_hosts_file());
    under("data_dir", paths::data_dir());
    under("log_dir", paths::log_dir());
    under("edit_dir", paths::edit_dir());
    under("home_dir", paths::home_dir());
    under("openssh_known_hosts", paths::openssh_known_hosts());
    under("legacy_data_dir", paths::legacy_data_dir());
    under("temp_dir", Some(paths::temp_dir()));
    #[cfg(windows)]
    {
        under("local_app_data", paths::local_app_data());
        under("roaming_app_data", paths::roaming_app_data());
        under("documents", paths::documents());
        under(
            "citrix_cache",
            heimdall_core::import::citrix_cache::cache_folder(),
        );
    }
    // The files kept beside the profiles follow them.
    let profiles = paths::profiles_file().expect("profiles");
    under(
        "settings",
        Some(heimdall_core::settings::settings_path(&profiles)),
    );
    under(
        "session_snapshot",
        Some(heimdall_core::session_snapshot::snapshot_path(&profiles)),
    );
    under(
        "window_state",
        Some(heimdall_core::window_state::state_path(&profiles)),
    );
    // Each in its own folder, as the layout says.
    assert_eq!(
        paths::config_dir(),
        Some(root.join(test_data::CONFIG)),
        "config"
    );
    assert_eq!(paths::data_dir(), Some(root.join(test_data::DATA)), "data");
    // No installation of the user's is looked for.
    assert_eq!(paths::program_dir(), None);

    // The credentials: a namespace of the folder, apart from the real one.
    let service = paths::credential_service(SERVICE);
    assert_ne!(service, real_service);
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
    let other = scratch.path().join("other");
    assert_eq!(
        paths::use_test_data_dir(&other),
        Err(TestDataDirError::AlreadySet)
    );
    assert_eq!(paths::test_data_dir(), Some(root.as_path()));
}

#[cfg(not(feature = "test-data-dir"))]
#[test]
fn without_the_feature_the_folders_and_credentials_are_the_platform_ones() {
    assert_eq!(paths::credential_service(SERVICE), SERVICE);
    assert_eq!(paths::credential_target(TARGET), TARGET);
    assert_eq!(paths::temp_dir(), std::env::temp_dir());
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
