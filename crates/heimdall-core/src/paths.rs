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

//! Where Heimdall-rs keeps its files, and where the C# Heimdall keeps its own.

use std::path::PathBuf;

use directories::{BaseDirs, ProjectDirs};

/// Reverse-domain qualifier of the application directories; empty, as for a personal project.
const QUALIFIER: &str = "";

/// Organisation part of the application directories; empty, as for a personal project.
const ORGANIZATION: &str = "";

/// Application name the platform directories are derived from, and the name it goes by.
pub const APPLICATION: &str = "Heimdall-rs";

/// Name of the `known_hosts` file inside the configuration directory; Heimdall-rs keeps
/// its own, so a host accepted here never changes what `ssh` trusts.
pub const KNOWN_HOSTS_FILE_NAME: &str = "known_hosts";

/// Name of the log directory inside the local data directory.
const LOG_DIR_NAME: &str = "logs";

/// Name of the C# Heimdall's folder under the local application data directory.
const LEGACY_APPLICATION_DIR: &str = "Heimdall";

/// Name of the profile file inside the configuration directory.
pub const PROFILES_FILE_NAME: &str = "profiles.toml";

/// Name of the file holding the C# Heimdall's server profiles.
pub const LEGACY_SERVERS_FILE_NAME: &str = "servers.json";

/// Name of the file holding the C# Heimdall's settings, group defaults included.
pub const LEGACY_SETTINGS_FILE_NAME: &str = "settings.json";

/// Configuration directory of Heimdall-rs; `None` when the platform reports no home.
#[must_use]
pub fn config_dir() -> Option<PathBuf> {
    ProjectDirs::from(QUALIFIER, ORGANIZATION, APPLICATION).map(|dirs| dirs.config_dir().to_owned())
}

/// Path of the profile file.
#[must_use]
pub fn profiles_file() -> Option<PathBuf> {
    config_dir().map(|dir| dir.join(PROFILES_FILE_NAME))
}

/// Path of the `known_hosts` file.
#[must_use]
pub fn known_hosts_file() -> Option<PathBuf> {
    config_dir().map(|dir| dir.join(KNOWN_HOSTS_FILE_NAME))
}

/// Local data directory of Heimdall-rs: local to the machine, never roamed. The logs and
/// the files being edited are in it.
#[must_use]
pub fn data_dir() -> Option<PathBuf> {
    ProjectDirs::from(QUALIFIER, ORGANIZATION, APPLICATION)
        .map(|dirs| dirs.data_local_dir().to_owned())
}

/// Directory of the log file and crash reports: local to the machine, never roamed.
#[must_use]
pub fn log_dir() -> Option<PathBuf> {
    data_dir().map(|dir| dir.join(LOG_DIR_NAME))
}

/// Name of the folder a server's files are edited in, inside the local data directory.
const EDIT_DIR_NAME: &str = "edit";

/// Folder a server's file is copied to while edited: local to the machine, the user's own.
#[must_use]
pub fn edit_dir() -> Option<PathBuf> {
    data_dir().map(|dir| dir.join(EDIT_DIR_NAME))
}

/// The user's home folder, where a Files tab starts; `None` when the platform reports none.
#[must_use]
pub fn home_dir() -> Option<PathBuf> {
    BaseDirs::new().map(|dirs| dirs.home_dir().to_owned())
}

/// Data directory of the C# Heimdall on this machine, when the platform has one.
///
/// Only meaningful on Windows, where the C# Heimdall runs; the path is returned whether or
/// not it exists.
#[must_use]
pub fn legacy_data_dir() -> Option<PathBuf> {
    BaseDirs::new().map(|dirs| dirs.data_local_dir().join(LEGACY_APPLICATION_DIR))
}

/// Windows' system folder, as Windows says where it is: never read from the environment,
/// which whoever starts Heimdall sets. `None` when Windows does not say, or says a relative
/// path.
#[cfg(windows)]
#[must_use]
pub fn system_dir() -> Option<PathBuf> {
    winsafe::GetSystemDirectory()
        .ok()
        .map(PathBuf::from)
        .filter(|folder| folder.is_absolute())
}
