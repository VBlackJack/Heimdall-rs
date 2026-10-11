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
//!
//! A build with the `test-data-dir` feature, for automated UI tests only, can root every
//! per-user folder in one scratch folder ([`use_test_data_dir`]), and files its credentials in
//! the system's store under a namespace of that folder ([`credential_service`]). The feature
//! never reaches a release build: it does not compile there, and the workspace's guard
//! refuses any manifest or workflow that turns it on.

#[cfg(feature = "test-data-dir")]
use std::path::Path;
use std::path::PathBuf;
#[cfg(feature = "test-data-dir")]
use std::sync::OnceLock;

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
    #[cfg(feature = "test-data-dir")]
    if let Some(folder) = test_folder(test_data::CONFIG) {
        return Some(folder);
    }
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
    #[cfg(feature = "test-data-dir")]
    if let Some(folder) = test_folder(test_data::DATA) {
        return Some(folder);
    }
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
    #[cfg(feature = "test-data-dir")]
    if let Some(folder) = test_folder(test_data::HOME) {
        return Some(folder);
    }
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
    #[cfg(feature = "test-data-dir")]
    if let Some(folder) = test_folder(test_data::LEGACY) {
        return Some(folder);
    }
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
    #[cfg(feature = "test-data-dir")]
    if let Some(folder) = test_folder(test_data::LOCAL_APP_DATA) {
        return Some(folder);
    }
    known_folder(&winsafe::co::KNOWNFOLDERID::LocalAppData)
}

/// The current user's roaming application data folder, as Windows says where it is: never
/// read from `APPDATA`. `None` when Windows does not say.
#[cfg(windows)]
#[must_use]
pub fn roaming_app_data() -> Option<PathBuf> {
    #[cfg(feature = "test-data-dir")]
    if let Some(folder) = test_folder(test_data::ROAMING_APP_DATA) {
        return Some(folder);
    }
    known_folder(&winsafe::co::KNOWNFOLDERID::RoamingAppData)
}

/// The current user's Documents folder, as Windows says where it is: never one the
/// environment moves. `None` when Windows does not say.
#[cfg(windows)]
#[must_use]
pub fn documents() -> Option<PathBuf> {
    #[cfg(feature = "test-data-dir")]
    if let Some(folder) = test_folder(test_data::DOCUMENTS) {
        return Some(folder);
    }
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

/// The system's temporary folder, where the files handed to another program for a moment
/// are written (Remote Desktop Connection's connection files).
#[must_use]
pub fn temp_dir() -> PathBuf {
    #[cfg(feature = "test-data-dir")]
    if let Some(folder) = test_folder(test_data::TEMP) {
        return folder;
    }
    std::env::temp_dir()
}

/// The folder of this program, where the legacy PowerShell Heimdall is looked for from, as
/// the C# start looks for it; `None` when it cannot be told, and under a test data folder,
/// where no installation of the user's may be read.
#[must_use]
pub fn program_dir() -> Option<PathBuf> {
    #[cfg(feature = "test-data-dir")]
    if test_data_dir().is_some() {
        return None;
    }
    std::env::current_exe()
        .ok()
        .and_then(|program| program.parent().map(std::path::Path::to_path_buf))
}

/// The name Heimdall-rs files its secrets under in the system's credential store, for the
/// application's own name `service`: `service` itself, and under a test data folder
/// `service` followed by that folder's namespace, so that a test never reads, replaces or
/// removes a real entry.
#[must_use]
pub fn credential_service(service: &str) -> String {
    #[cfg(feature = "test-data-dir")]
    if let Some(root) = TEST_ROOT.get() {
        return format!(
            "{service}{}{}",
            test_data::NAMESPACE_DIVIDER,
            root.namespace
        );
    }
    service.to_owned()
}

/// The target name of a generic credential of Windows Credential Manager a profile names:
/// `target` itself, and under a test data folder `target` inside that folder's namespace, so
/// that a test reads only the credentials it made.
#[must_use]
pub fn credential_target(target: &str) -> String {
    #[cfg(feature = "test-data-dir")]
    if let Some(root) = TEST_ROOT.get() {
        return format!(
            "{APPLICATION}{}{}{}{target}",
            test_data::NAMESPACE_DIVIDER,
            root.namespace,
            test_data::TARGET_DIVIDER
        );
    }
    target.to_owned()
}

/// What a test data folder is laid out as, and how its credentials are named.
#[cfg(feature = "test-data-dir")]
pub mod test_data {
    /// The command-line argument naming the test data folder, followed by its full path.
    pub const ARGUMENT: &str = "--data-dir";

    /// The configuration folder inside it: profiles, settings, vault, `known_hosts`.
    pub const CONFIG: &str = "config";

    /// The local data folder inside it: logs, files being edited.
    pub const DATA: &str = "data";

    /// The C# Heimdall's data folder inside it, the migration's source.
    pub const LEGACY: &str = "legacy";

    /// The home folder inside it: `.ssh`, `.Xauthority`, `PuTTY`'s sessions, the Files start.
    pub const HOME: &str = "home";

    /// The local application data folder inside it: the Citrix cache.
    pub const LOCAL_APP_DATA: &str = "local-app-data";

    /// The roaming application data folder inside it.
    pub const ROAMING_APP_DATA: &str = "roaming-app-data";

    /// The Documents folder inside it.
    pub const DOCUMENTS: &str = "documents";

    /// The temporary folder inside it: Remote Desktop Connection's connection files.
    pub const TEMP: &str = "temp";

    /// Every folder inside it, in the order above.
    pub const FOLDERS: [&str; 8] = [
        CONFIG,
        DATA,
        LEGACY,
        HOME,
        LOCAL_APP_DATA,
        ROAMING_APP_DATA,
        DOCUMENTS,
        TEMP,
    ];

    /// What joins the application's name and the namespace of a test data folder.
    pub const NAMESPACE_DIVIDER: &str = "-test-";

    /// What joins a test namespace and a credential target a profile names.
    pub const TARGET_DIVIDER: &str = "/";

    /// Bytes of the SHA-256 of the folder's path the namespace is named after: 48 bits, so
    /// that two scratch folders never share one in practice.
    pub const NAMESPACE_HASH_LEN: usize = 6;
}

/// The test data folder in use, and the namespace of its credentials.
#[cfg(feature = "test-data-dir")]
#[derive(Debug)]
struct TestRoot {
    folder: PathBuf,
    namespace: String,
}

/// Set once, before any folder is asked, by [`use_test_data_dir`].
#[cfg(feature = "test-data-dir")]
static TEST_ROOT: OnceLock<TestRoot> = OnceLock::new();

/// Why a test data folder was refused.
#[cfg(feature = "test-data-dir")]
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum TestDataDirError {
    /// The folder is not a full path.
    #[error("the test data folder must be a full path: {0}")]
    NotAbsolute(PathBuf),
    /// A folder inside it is, holds or sits in one of the user's real Heimdall folders.
    #[error("the test data folder {0} overlaps the user's real folder {1}")]
    Overlaps(PathBuf, PathBuf),
    /// The folder cannot be made or resolved.
    #[error("the test data folder cannot be used: {0}")]
    Unusable(String),
    /// A test data folder is already in use by this process.
    #[error("a test data folder is already in use")]
    AlreadySet,
}

/// Roots every per-user folder of this process in `folder`, made if needed, and files its
/// credentials under a namespace of `folder`: to be called once, before any folder is asked.
///
/// # Errors
///
/// [`TestDataDirError`]: a relative path, a folder whose layout would meet the user's real
/// configuration, data or C# folder, a folder that cannot be made, or a second call.
#[cfg(feature = "test-data-dir")]
pub fn use_test_data_dir(folder: &Path) -> Result<(), TestDataDirError> {
    if !folder.is_absolute() {
        return Err(TestDataDirError::NotAbsolute(folder.to_owned()));
    }
    if TEST_ROOT.get().is_some() {
        return Err(TestDataDirError::AlreadySet);
    }
    let real: Vec<PathBuf> = [config_dir(), data_dir(), legacy_data_dir()]
        .into_iter()
        .flatten()
        .collect();
    for inside in test_data::FOLDERS.map(|name| folder.join(name)) {
        if let Some(met) = real.iter().find(|real| overlap(&inside, real)) {
            return Err(TestDataDirError::Overlaps(folder.to_owned(), met.clone()));
        }
    }
    std::fs::create_dir_all(folder)
        .and_then(|()| std::fs::canonicalize(folder))
        .map_err(|error| TestDataDirError::Unusable(format!("{}: {error}", folder.display())))
        .and_then(|canonical| {
            TEST_ROOT
                .set(TestRoot {
                    folder: folder.to_owned(),
                    namespace: namespace_of(&canonical),
                })
                .map_err(|_| TestDataDirError::AlreadySet)
        })
}

/// The test data folder in use, if one is.
#[cfg(feature = "test-data-dir")]
#[must_use]
pub fn test_data_dir() -> Option<&'static Path> {
    TEST_ROOT.get().map(|root| root.folder.as_path())
}

/// The folder `name` inside the test data folder, when one is in use.
#[cfg(feature = "test-data-dir")]
fn test_folder(name: &str) -> Option<PathBuf> {
    test_data_dir().map(|root| root.join(name))
}

/// The namespace of the folder `canonical`: hex of the start of the SHA-256 of its path,
/// without regard to case on Windows, as Windows compares paths.
#[cfg(feature = "test-data-dir")]
fn namespace_of(canonical: &Path) -> String {
    let text = folded(canonical).to_string_lossy().into_owned();
    let digest = sealvault::hash::sha256(text.as_bytes());
    data_encoding::HEXLOWER.encode(&digest[..test_data::NAMESPACE_HASH_LEN])
}

/// Whether one of `a` and `b` is the other or inside it.
#[cfg(feature = "test-data-dir")]
fn overlap(a: &Path, b: &Path) -> bool {
    let (a, b) = (folded(a), folded(b));
    a.starts_with(&b) || b.starts_with(&a)
}

/// `path` as Windows compares it, without regard to case; as it is elsewhere.
#[cfg(feature = "test-data-dir")]
fn folded(path: &Path) -> PathBuf {
    #[cfg(windows)]
    return PathBuf::from(path.to_string_lossy().to_lowercase());
    #[cfg(not(windows))]
    return path.to_owned();
}
