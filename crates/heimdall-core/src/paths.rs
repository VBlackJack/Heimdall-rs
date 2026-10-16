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
//! A build with the `test-data-dir` feature, for automated UI tests only, roots every
//! per-user folder in one scratch folder ([`use_test_data_dir`]), and files its credentials in
//! the system's store under a namespace of that folder ([`credential_service`]). It fails
//! closed: until a scratch folder is set, every per-user folder is unknown and the
//! credentials go to a namespace no real entry has. The feature never reaches a release
//! build: it does not compile there, and the workspace's guard refuses any manifest or
//! workflow that turns it on.

use std::path::PathBuf;
#[cfg(feature = "test-data-dir")]
use std::path::{Component, Path};
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
    return test_folder(test_data::CONFIG);
    #[cfg(not(feature = "test-data-dir"))]
    return platform_config_dir();
}

/// [`config_dir`] as the platform says it.
fn platform_config_dir() -> Option<PathBuf> {
    #[cfg(windows)]
    return platform_roaming_app_data()
        .map(|dir| dir.join(application_path()).join(CONFIG_DIR_NAME));
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
    return test_folder(test_data::DATA);
    #[cfg(not(feature = "test-data-dir"))]
    return platform_data_dir();
}

/// [`data_dir`] as the platform says it.
fn platform_data_dir() -> Option<PathBuf> {
    #[cfg(windows)]
    return platform_local_app_data().map(|dir| dir.join(application_path()).join(DATA_DIR_NAME));
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
    return test_folder(test_data::HOME);
    #[cfg(not(feature = "test-data-dir"))]
    return platform_home_dir();
}

/// [`home_dir`] as the platform says it.
fn platform_home_dir() -> Option<PathBuf> {
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
    return test_folder(test_data::LEGACY);
    #[cfg(not(feature = "test-data-dir"))]
    return platform_legacy_data_dir();
}

/// [`legacy_data_dir`] as the platform says it.
fn platform_legacy_data_dir() -> Option<PathBuf> {
    #[cfg(windows)]
    return platform_local_app_data().map(|dir| dir.join(LEGACY_APPLICATION_DIR));
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
    return test_folder(test_data::LOCAL_APP_DATA);
    #[cfg(not(feature = "test-data-dir"))]
    return platform_local_app_data();
}

/// [`local_app_data`] as Windows says it.
#[cfg(windows)]
fn platform_local_app_data() -> Option<PathBuf> {
    known_folder(&winsafe::co::KNOWNFOLDERID::LocalAppData)
}

/// The current user's roaming application data folder, as Windows says where it is: never
/// read from `APPDATA`. `None` when Windows does not say.
#[cfg(windows)]
#[must_use]
pub fn roaming_app_data() -> Option<PathBuf> {
    #[cfg(feature = "test-data-dir")]
    return test_folder(test_data::ROAMING_APP_DATA);
    #[cfg(not(feature = "test-data-dir"))]
    return platform_roaming_app_data();
}

/// [`roaming_app_data`] as Windows says it.
#[cfg(windows)]
fn platform_roaming_app_data() -> Option<PathBuf> {
    known_folder(&winsafe::co::KNOWNFOLDERID::RoamingAppData)
}

/// The current user's Documents folder, as Windows says where it is: never one the
/// environment moves. `None` when Windows does not say.
#[cfg(windows)]
#[must_use]
pub fn documents() -> Option<PathBuf> {
    #[cfg(feature = "test-data-dir")]
    return test_folder(test_data::DOCUMENTS);
    #[cfg(not(feature = "test-data-dir"))]
    return platform_documents();
}

/// [`documents`] as Windows says it.
#[cfg(windows)]
fn platform_documents() -> Option<PathBuf> {
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

/// Why a file was not handed to another program: the temporary folder is unknown.
pub const TEMP_FOLDER_UNKNOWN: &str = "the temporary folder is unknown";

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
/// are written (Remote Desktop Connection's connection files, a copy between two servers
/// without an edit folder); `None` in a test build without its scratch folder.
#[must_use]
pub fn temp_dir() -> Option<PathBuf> {
    #[cfg(feature = "test-data-dir")]
    return test_folder(test_data::TEMP);
    #[cfg(not(feature = "test-data-dir"))]
    return Some(platform_temp_dir());
}

/// [`temp_dir`] as the system says it.
fn platform_temp_dir() -> PathBuf {
    std::env::temp_dir()
}

/// The folder of this program, where the legacy PowerShell Heimdall is looked for from, as
/// the C# start looks for it; `None` when it cannot be told, and always in a test build,
/// where no installation of the user's may be read.
#[must_use]
pub fn program_dir() -> Option<PathBuf> {
    #[cfg(feature = "test-data-dir")]
    return None;
    #[cfg(not(feature = "test-data-dir"))]
    return std::env::current_exe()
        .ok()
        .and_then(|program| program.parent().map(std::path::Path::to_path_buf));
}

/// The name Heimdall-rs files its secrets under in the system's credential store, for the
/// application's own name `service`: `service` itself; in a test build, `service` followed by
/// the scratch folder's namespace, or by a namespace no folder has while none is set, so
/// that a test never reads, replaces or removes a real entry.
#[must_use]
pub fn credential_service(service: &str) -> String {
    #[cfg(feature = "test-data-dir")]
    return format!(
        "{service}{}{}",
        test_data::NAMESPACE_DIVIDER,
        test_namespace()
    );
    #[cfg(not(feature = "test-data-dir"))]
    return service.to_owned();
}

/// The target name of a generic credential of Windows Credential Manager a profile names:
/// `target` itself; in a test build, `target` inside the namespace of
/// [`credential_service`], so that a test reads only the credentials it made.
#[must_use]
pub fn credential_target(target: &str) -> String {
    #[cfg(feature = "test-data-dir")]
    return format!(
        "{}{}{target}",
        credential_service(APPLICATION),
        test_data::TARGET_DIVIDER
    );
    #[cfg(not(feature = "test-data-dir"))]
    return target.to_owned();
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

    /// What joins the application's name and the namespace of a test data folder.
    pub const NAMESPACE_DIVIDER: &str = "-test-";

    /// The namespace while no test data folder is set: never a folder's, which is hex.
    pub const UNSET_NAMESPACE: &str = "unset";

    /// What joins a test namespace and a credential target a profile names.
    pub const TARGET_DIVIDER: &str = "/";

    /// Bytes of the SHA-256 of the folder's path the namespace is named after: 48 bits, so
    /// that two scratch folders never share one in practice.
    pub const NAMESPACE_HASH_LEN: usize = 6;

    /// The start of a Windows path that bypasses the path parser, as `canonicalize` gives.
    pub const VERBATIM_PREFIX: &str = r"\\?\";

    /// The start of a verbatim network path.
    pub const VERBATIM_UNC_PREFIX: &str = r"\\?\UNC\";

    /// The start of a network path written plainly.
    pub const UNC_PREFIX: &str = r"\\";
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
    /// The folder is, holds or sits in a Heimdall folder of the user's, or is or holds one
    /// of the user's own folders.
    #[error("the test data folder {0} overlaps the user's real folder {1}")]
    Overlaps(PathBuf, PathBuf),
    /// The folder cannot be made or resolved.
    #[error("the test data folder cannot be used: {0}")]
    Unusable(String),
    /// A test data folder is already in use by this process.
    #[error("a test data folder is already in use")]
    AlreadySet,
}

/// Roots every per-user folder of this process in `folder`, made if needed with its home
/// folder ([`test_data::HOME`]), and files its credentials under a namespace of `folder`: to
/// be called once, before any folder is asked.
///
/// The folder is compared with the user's real folders as the file system resolves them
/// (links followed, `..` resolved, without regard to case): it may neither be, hold nor sit
/// in a Heimdall-rs or C# Heimdall folder, nor be or hold the home, Documents, temporary or
/// application data folder. It is checked before it is made, and again once made.
///
/// # Errors
///
/// [`TestDataDirError`]: a relative path, a folder meeting a real one, a folder that cannot
/// be made, or a second call.
#[cfg(feature = "test-data-dir")]
pub fn use_test_data_dir(folder: &Path) -> Result<(), TestDataDirError> {
    if !folder.is_absolute() {
        return Err(TestDataDirError::NotAbsolute(folder.to_owned()));
    }
    if TEST_ROOT.get().is_some() {
        return Err(TestDataDirError::AlreadySet);
    }
    refuse_real(folder)?;
    std::fs::create_dir_all(folder)
        .map_err(|error| TestDataDirError::Unusable(format!("{}: {error}", folder.display())))?;
    refuse_real(folder)?;
    // A real user's home folder is always there: the local pane of the Files tab opens on
    // it, and a missing one showed the system's "path not found" in its place.
    let home = folder.join(test_data::HOME);
    std::fs::create_dir_all(&home)
        .map_err(|error| TestDataDirError::Unusable(format!("{}: {error}", home.display())))?;
    let namespace = namespace_of(&comparable(folder));
    TEST_ROOT
        .set(TestRoot {
            folder: folder.to_owned(),
            namespace,
        })
        .map_err(|_| TestDataDirError::AlreadySet)
}

/// [`TestDataDirError::Overlaps`] when `folder` meets one of the user's real folders, as
/// [`use_test_data_dir`] says.
#[cfg(feature = "test-data-dir")]
fn refuse_real(folder: &Path) -> Result<(), TestDataDirError> {
    let refused = |real: &PathBuf| TestDataDirError::Overlaps(folder.to_owned(), real.clone());
    if let Some(real) = heimdall_folders()
        .iter()
        .find(|real| folders_overlap(folder, real))
    {
        return Err(refused(real));
    }
    if let Some(real) = user_folders()
        .iter()
        .find(|real| folder_holds(folder, real))
    {
        return Err(refused(real));
    }
    Ok(())
}

/// The real folders of Heimdall-rs and the C# Heimdall: no test folder may meet them.
#[cfg(feature = "test-data-dir")]
fn heimdall_folders() -> Vec<PathBuf> {
    let own = [
        platform_config_dir(),
        platform_data_dir(),
        platform_legacy_data_dir(),
    ];
    // On Windows, the application's folders the configuration and data folders are in.
    #[cfg(windows)]
    let own = {
        let parent = |folder: &Option<PathBuf>| {
            folder
                .as_deref()
                .and_then(Path::parent)
                .map(Path::to_path_buf)
        };
        let [config, data, legacy] = own;
        [parent(&config), parent(&data), config, data, legacy]
    };
    own.into_iter().flatten().collect()
}

/// The user's own folders: a test folder may sit in them, never be or hold one.
#[cfg(feature = "test-data-dir")]
fn user_folders() -> Vec<PathBuf> {
    let common = [platform_home_dir(), Some(platform_temp_dir())];
    #[cfg(windows)]
    let common = {
        let [home, temp] = common;
        [
            home,
            temp,
            platform_documents(),
            platform_roaming_app_data(),
            platform_local_app_data(),
        ]
    };
    common.into_iter().flatten().collect()
}

/// The test data folder in use, if one is.
#[cfg(feature = "test-data-dir")]
#[must_use]
pub fn test_data_dir() -> Option<&'static Path> {
    TEST_ROOT.get().map(|root| root.folder.as_path())
}

/// The folder `name` inside the test data folder; `None` while none is set, so that a test
/// build never falls back to a real folder.
#[cfg(feature = "test-data-dir")]
fn test_folder(name: &str) -> Option<PathBuf> {
    test_data_dir().map(|root| root.join(name))
}

/// The namespace of the test data folder in use, or [`test_data::UNSET_NAMESPACE`].
#[cfg(feature = "test-data-dir")]
fn test_namespace() -> &'static str {
    TEST_ROOT
        .get()
        .map_or(test_data::UNSET_NAMESPACE, |root| root.namespace.as_str())
}

/// The namespace of the folder `comparable`: hex of the start of the SHA-256 of its path.
#[cfg(feature = "test-data-dir")]
fn namespace_of(comparable: &Path) -> String {
    let text = comparable.to_string_lossy().into_owned();
    let digest = sealvault::hash::sha256(text.as_bytes());
    data_encoding::HEXLOWER.encode(&digest[..test_data::NAMESPACE_HASH_LEN])
}

/// Whether one of the folders `a` and `b` is the other or inside it, as the file system
/// resolves them.
#[cfg(feature = "test-data-dir")]
#[must_use]
pub fn folders_overlap(a: &Path, b: &Path) -> bool {
    let (a, b) = (comparable(a), comparable(b));
    a.starts_with(&b) || b.starts_with(&a)
}

/// Whether the folder `outer` is the folder `inner` or holds it, as the file system resolves
/// them.
#[cfg(feature = "test-data-dir")]
#[must_use]
pub fn folder_holds(outer: &Path, inner: &Path) -> bool {
    comparable(inner).starts_with(comparable(outer))
}

/// `path` as it is compared: its longest part that exists resolved by the file system (links
/// followed, `..` resolved), the rest resolved by its words, Windows' verbatim prefix
/// dropped, without regard to case.
#[cfg(feature = "test-data-dir")]
#[must_use]
pub fn comparable(path: &Path) -> PathBuf {
    let components: Vec<Component<'_>> = path.components().collect();
    let resolved = (1..=components.len())
        .rev()
        .find_map(|split| {
            let head: PathBuf = components[..split].iter().collect();
            std::fs::canonicalize(head)
                .ok()
                .map(|canonical| append_lexically(canonical, &components[split..]))
        })
        .unwrap_or_else(|| append_lexically(PathBuf::new(), &components));
    let text = resolved.to_string_lossy();
    let plain = if let Some(rest) = text.strip_prefix(test_data::VERBATIM_UNC_PREFIX) {
        format!("{}{rest}", test_data::UNC_PREFIX)
    } else if let Some(rest) = text.strip_prefix(test_data::VERBATIM_PREFIX) {
        rest.to_owned()
    } else {
        text.into_owned()
    };
    PathBuf::from(plain.to_lowercase())
}

/// `base` followed by `rest`, a `..` taking the last part away and a `.` nothing.
#[cfg(feature = "test-data-dir")]
fn append_lexically(mut base: PathBuf, rest: &[Component<'_>]) -> PathBuf {
    for component in rest {
        match component {
            Component::ParentDir => {
                base.pop();
            }
            Component::CurDir => {}
            other => base.push(other),
        }
    }
    base
}
