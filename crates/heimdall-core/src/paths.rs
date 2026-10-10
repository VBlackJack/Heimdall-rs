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

#[cfg(not(windows))]
use directories::{BaseDirs, ProjectDirs};

/// Reverse-domain qualifier of the application directories; empty, as for a personal project.
#[cfg(not(windows))]
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

/// Name of the configuration folder inside the application's roaming folder, as the
/// `directories` crate lays it out on Windows.
#[cfg(windows)]
const CONFIG_DIR_NAME: &str = "config";

/// Name of the data folder inside the application's local folder, as the `directories`
/// crate lays it out on Windows.
#[cfg(windows)]
const DATA_DIR_NAME: &str = "data";

/// The application's folder under an application data folder, as the `directories` crate
/// joins it on Windows: the organisation, then the application.
#[cfg(windows)]
fn application_path() -> PathBuf {
    PathBuf::from_iter([ORGANIZATION, APPLICATION])
}

/// Configuration directory of Heimdall-rs; `None` when the platform reports no home. On
/// Windows, under the roaming application data folder Windows says, never one the
/// environment moves, laid out as the `directories` crate lays it out.
#[must_use]
pub fn config_dir() -> Option<PathBuf> {
    #[cfg(windows)]
    return roaming_app_data().map(|dir| dir.join(application_path()).join(CONFIG_DIR_NAME));
    #[cfg(not(windows))]
    return ProjectDirs::from(QUALIFIER, ORGANIZATION, APPLICATION)
        .map(|dirs| dirs.config_dir().to_owned());
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
/// the files being edited are in it. On Windows, under the local application data folder
/// Windows says, never one the environment moves, laid out as the `directories` crate lays
/// it out.
#[must_use]
pub fn data_dir() -> Option<PathBuf> {
    #[cfg(windows)]
    return local_app_data().map(|dir| dir.join(application_path()).join(DATA_DIR_NAME));
    #[cfg(not(windows))]
    return ProjectDirs::from(QUALIFIER, ORGANIZATION, APPLICATION)
        .map(|dirs| dirs.data_local_dir().to_owned());
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
/// On Windows, the profile folder Windows says, never the one `USERPROFILE` names.
#[must_use]
pub fn home_dir() -> Option<PathBuf> {
    #[cfg(windows)]
    return known_folder(&winsafe::co::KNOWNFOLDERID::Profile);
    #[cfg(not(windows))]
    return BaseDirs::new().map(|dirs| dirs.home_dir().to_owned());
}

/// The folder OpenSSH keeps its files in, under the home folder.
pub const OPENSSH_FOLDER: &str = ".ssh";

/// The file of the keys OpenSSH trusts, in that folder.
pub const OPENSSH_KNOWN_HOSTS: &str = "known_hosts";

/// The user's OpenSSH `known_hosts`, under the home folder Windows says: never read from
/// the environment, which whoever starts Heimdall sets. `None` when the platform reports
/// no home.
#[must_use]
pub fn openssh_known_hosts() -> Option<PathBuf> {
    home_dir().map(|home| home.join(OPENSSH_FOLDER).join(OPENSSH_KNOWN_HOSTS))
}

/// Data directory of the C# Heimdall on this machine, when the platform has one.
///
/// Only meaningful on Windows, where the C# Heimdall runs, under the local application data
/// folder Windows says, never the one `LOCALAPPDATA` names; the path is returned whether or
/// not it exists.
#[must_use]
pub fn legacy_data_dir() -> Option<PathBuf> {
    #[cfg(windows)]
    return local_app_data().map(|dir| dir.join(LEGACY_APPLICATION_DIR));
    #[cfg(not(windows))]
    return BaseDirs::new().map(|dirs| dirs.data_local_dir().join(LEGACY_APPLICATION_DIR));
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

/// Folder `id` as Windows says where it is, for the current user: never read from the
/// environment, which whoever starts Heimdall sets. `None` when Windows does not say, or says
/// a relative path.
///
/// The user's token is named rather than left out: left out, Windows expands the folders
/// under the profile with this process's `USERPROFILE`, so a planted one moved them.
#[cfg(windows)]
fn known_folder(id: &winsafe::co::KNOWNFOLDERID) -> Option<PathBuf> {
    let token = winsafe::HPROCESS::GetCurrentProcess()
        .OpenProcessToken(winsafe::co::TOKEN::QUERY | winsafe::co::TOKEN::IMPERSONATE)
        .ok()?;
    winsafe::SHGetKnownFolderPath(id, winsafe::co::KF::DEFAULT, Some(&token))
        .ok()
        .map(PathBuf::from)
        .filter(|folder| folder.is_absolute())
}

/// The current user's local application data folder, never roamed, as Windows says where it
/// is: never read from `LOCALAPPDATA`. `None` when Windows does not say.
#[cfg(windows)]
#[must_use]
pub fn local_app_data() -> Option<PathBuf> {
    known_folder(&winsafe::co::KNOWNFOLDERID::LocalAppData)
}

/// The current user's roaming application data folder, as Windows says where it is: never
/// read from `APPDATA`. `None` when Windows does not say.
#[cfg(windows)]
#[must_use]
pub fn roaming_app_data() -> Option<PathBuf> {
    known_folder(&winsafe::co::KNOWNFOLDERID::RoamingAppData)
}

/// The current user's Documents folder, as Windows says where it is: never one the
/// environment moves. `None` when Windows does not say.
#[cfg(windows)]
#[must_use]
pub fn documents() -> Option<PathBuf> {
    known_folder(&winsafe::co::KNOWNFOLDERID::Documents)
}

/// The Program Files folder of this process's architecture, as Windows says where it is:
/// never read from `ProgramFiles`. `None` when Windows does not say.
#[cfg(windows)]
#[must_use]
pub fn program_files() -> Option<PathBuf> {
    known_folder(&winsafe::co::KNOWNFOLDERID::ProgramFiles)
}

/// The Program Files folder of 32-bit programs, as Windows says where it is: never read from
/// `ProgramFiles(x86)`. The only Program Files folder on a 32-bit Windows. `None` when
/// Windows does not say.
#[cfg(windows)]
#[must_use]
pub fn program_files_x86() -> Option<PathBuf> {
    known_folder(&winsafe::co::KNOWNFOLDERID::ProgramFilesX86)
}

/// Windows' own folder, the parent of its system folder, as Windows says where it is: never
/// read from `SystemRoot` or `windir`. `None` when Windows does not say.
#[cfg(windows)]
#[must_use]
pub fn system_root() -> Option<PathBuf> {
    known_folder(&winsafe::co::KNOWNFOLDERID::Windows)
}

/// Windows' program that calls a library's entry point, in the system folder.
pub const RUNDLL_PROGRAM: &str = "rundll32.exe";

/// Windows' file manager, in the Windows folder.
pub const EXPLORER_PROGRAM: &str = "explorer.exe";

/// Why a program of Windows was not started: Windows did not say where its system folder is.
pub const SYSTEM_FOLDER_UNKNOWN: &str = "the system folder is unknown";

/// Why a program of Windows was not started: Windows did not say where its own folder is.
pub const WINDOWS_FOLDER_UNKNOWN: &str = "the Windows folder is unknown";

/// Program `name` in Windows' system folder, by its whole path, as Windows says where that
/// is: never one of the same name found first beside Heimdall, in the current folder, on
/// the `PATH` or under a folder the environment names. `None` when Windows does not say,
/// and off Windows.
#[must_use]
pub fn system_program(name: &str) -> Option<PathBuf> {
    #[cfg(windows)]
    return system_dir().map(|folder| folder.join(name));
    #[cfg(not(windows))]
    return {
        let _ = name;
        None
    };
}

/// Program `name` in Windows' own folder, by its whole path, as Windows says where that is,
/// as [`system_program`] for the system folder. `None` when Windows does not say, and off
/// Windows.
#[must_use]
pub fn windows_program(name: &str) -> Option<PathBuf> {
    #[cfg(windows)]
    return system_root().map(|folder| folder.join(name));
    #[cfg(not(windows))]
    return {
        let _ = name;
        None
    };
}
