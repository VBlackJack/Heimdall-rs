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

//! A Citrix application launched as the C# `CitrixHandler` launches it, outside Heimdall:
//! the line Citrix Workspace's cache gave it, an ICA file opened by the program it belongs
//! to, or the published application asked of its `StoreFront` through Citrix Workspace's
//! own launcher. Its window is Citrix's own: none is embedded in a tab, which shows the
//! launch and its client's state instead, as [`crate::citrix_session`] says.

use std::path::{Path, PathBuf};

use heimdall_core::profile::CitrixProfile;
use zeroize::Zeroizing;

/// The extension of the files Citrix Workspace opens.
const ICA_EXTENSION: &str = "ica";

/// Citrix Workspace's `StoreFront` launcher, preferred, as the C#.
const STOREBROWSE: &str = "storebrowse.exe";
/// Citrix Workspace's self-service launcher, the C# fallback.
const SELF_SERVICE: &str = "SelfService.exe";
/// The variable listing the folders a launcher is looked for in last.
const PATH_VARIABLE: &str = "PATH";

/// `storebrowse.exe`'s command with single sign-on, and without.
const STOREBROWSE_SSO: &str = "-S";
const STOREBROWSE_NO_SSO: &str = "-L";
/// `SelfService.exe`'s command and its quiet single sign-on switch.
const SELF_SERVICE_COMMAND: &str = "storebrowse";
const SELF_SERVICE_SSO: &str = "-q";

/// The process gets no console window, as the C# `CreateNoWindow`.
#[cfg(windows)]
pub(crate) const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// What a cache launch line may not hold, as the C# refuses it: what a shell would read.
const FORBIDDEN_IN_LAUNCH_LINE: [char; 7] = ['|', '&', ';', '`', '$', '\n', '\r'];

/// How a Citrix application is launched.
#[derive(Clone, PartialEq, Eq)]
pub enum CitrixLaunch {
    /// The line Citrix Workspace's cache gave it, passed to `SelfService.exe` as it is.
    CacheLine(Zeroizing<String>),
    /// An ICA file, opened by the program Windows gives it.
    IcaFile(PathBuf),
    /// The published application `app`, asked of the `StoreFront` at `url`.
    StoreFront {
        /// The application's name.
        app: String,
        /// The `StoreFront` address, checked.
        url: String,
        /// With this Windows account's Kerberos identity.
        sso: bool,
    },
}

impl std::fmt::Debug for CitrixLaunch {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            // Pre-authenticated: never shown.
            Self::CacheLine(_) => f.write_str("CacheLine(..)"),
            Self::IcaFile(path) => f.debug_tuple("IcaFile").field(path).finish(),
            Self::StoreFront { app, url, sso } => f
                .debug_struct("StoreFront")
                .field("app", app)
                .field("url", url)
                .field("sso", sso)
                .finish(),
        }
    }
}

/// Why a Citrix application is not launched, as the C# says it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CitrixRefusal {
    /// The `StoreFront` address is not an absolute `http` or `https` one.
    InvalidStoreFront,
    /// The `StoreFront` address holds a user name or a password.
    StoreFrontCredentials,
    /// The ICA file is on another computer, or is not one.
    InvalidIcaFile,
    /// Neither an ICA file nor a `StoreFront` and an application name.
    NotConfigured,
    /// No Citrix Workspace launcher on this computer.
    WorkspaceNotFound,
    /// The launcher found cannot ask without single sign-on, nor ask at all.
    Failed,
    /// The launcher could not be started, and why.
    NotStarted(String),
    /// The cache launch line holds what a shell would read.
    CommandRejected,
    /// The vault, which may hold the cache launch line, is locked.
    VaultLocked,
}

/// How `profile` launches, in the C# order: its `StoreFront` checked first when it names
/// one; `cache_line`, its saved cache launch line, when it has one; an ICA file of this
/// computer that is there; else its `StoreFront` application.
///
/// # Errors
///
/// [`CitrixRefusal`] when it cannot launch.
pub fn plan(
    profile: &CitrixProfile,
    cache_line: Option<&str>,
) -> Result<CitrixLaunch, CitrixRefusal> {
    let url = profile
        .store_front_url
        .as_deref()
        .map(str::trim)
        .filter(|url| !url.is_empty())
        .map(store_front_url)
        .transpose()?;
    if let Some(line) = cache_line.filter(|line| !line.trim().is_empty()) {
        if !cache_line_allowed(line) {
            return Err(CitrixRefusal::CommandRejected);
        }
        return Ok(CitrixLaunch::CacheLine(Zeroizing::new(line.to_owned())));
    }
    if let Some(file) = profile
        .ica_file
        .as_deref()
        .map(str::trim)
        .filter(|file| !file.is_empty())
    {
        let path = ica_file(file)?;
        if path.is_file() {
            return Ok(CitrixLaunch::IcaFile(path));
        }
    }
    match (
        url,
        profile
            .app_name
            .as_deref()
            .map(str::trim)
            .filter(|app| !app.is_empty()),
    ) {
        (Some(url), Some(app)) => Ok(CitrixLaunch::StoreFront {
            app: app.to_owned(),
            url,
            sso: profile.sso,
        }),
        _ => Err(CitrixRefusal::NotConfigured),
    }
}

/// Whether `line` may be passed to `SelfService.exe`: none of the C# forbidden characters.
#[must_use]
pub fn cache_line_allowed(line: &str) -> bool {
    !line.contains(FORBIDDEN_IN_LAUNCH_LINE)
}

/// `url` when it is a `StoreFront` address: absolute, `http` or `https`, naming a host, and
/// no user name or password, as the C# `TryParseStoreFrontUrl` checks it.
///
/// # Errors
///
/// [`CitrixRefusal::InvalidStoreFront`], or [`CitrixRefusal::StoreFrontCredentials`].
pub fn store_front_url(url: &str) -> Result<String, CitrixRefusal> {
    let launchable =
        crate::external_url::launchable_url(url).ok_or(CitrixRefusal::InvalidStoreFront)?;
    let rest = launchable.split_once("://").map_or("", |(_, rest)| rest);
    let authority = rest.split(['/', '?', '#']).next().unwrap_or_default();
    if authority.contains('@') {
        return Err(CitrixRefusal::StoreFrontCredentials);
    }
    Ok(launchable)
}

/// `file` when it may be opened: an `.ica` file of this computer, never one on a share,
/// which a file of profiles could point at.
///
/// # Errors
///
/// [`CitrixRefusal::InvalidIcaFile`].
pub fn ica_file(file: &str) -> Result<PathBuf, CitrixRefusal> {
    let shared = file.starts_with("\\\\") || file.starts_with("//");
    let path = PathBuf::from(file);
    let ica = path
        .extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| extension.eq_ignore_ascii_case(ICA_EXTENSION));
    if shared || !ica {
        return Err(CitrixRefusal::InvalidIcaFile);
    }
    Ok(path)
}

/// Where Citrix Workspace's launchers may be, in the C# probe order: the modern folders
/// first, then the older flat one; `storebrowse.exe` before `SelfService.exe`.
#[must_use]
pub fn launcher_candidates(program_files_x86: &Path, program_files: &Path) -> Vec<PathBuf> {
    let client = |root: &Path| root.join("Citrix").join("ICA Client");
    vec![
        client(program_files_x86)
            .join("AuthManager")
            .join(STOREBROWSE),
        client(program_files).join("AuthManager").join(STOREBROWSE),
        client(program_files_x86).join(STOREBROWSE),
        client(program_files).join(STOREBROWSE),
        client(program_files_x86)
            .join("SelfServicePlugin")
            .join(SELF_SERVICE),
        client(program_files)
            .join("SelfServicePlugin")
            .join(SELF_SERVICE),
        client(program_files_x86).join(SELF_SERVICE),
        client(program_files).join(SELF_SERVICE),
    ]
}

/// Where `SelfService.exe` may be, in the C# probe order of a cache launch.
#[must_use]
pub fn self_service_candidates(program_files_x86: &Path, program_files: &Path) -> Vec<PathBuf> {
    let client = |root: &Path| root.join("Citrix").join("ICA Client");
    vec![
        client(program_files_x86)
            .join("SelfServicePlugin")
            .join(SELF_SERVICE),
        client(program_files_x86).join(SELF_SERVICE),
        client(program_files)
            .join("SelfServicePlugin")
            .join(SELF_SERVICE),
    ]
}

/// The arguments `launcher` takes to ask for `app` at `url`, by its own grammar; `None`
/// when it has none for that: `SelfService.exe` without single sign-on, or another program.
#[must_use]
pub fn launcher_arguments(launcher: &Path, app: &str, url: &str, sso: bool) -> Option<Vec<String>> {
    let name = launcher.file_name()?.to_str()?;
    let mut arguments: Vec<String> = if name.eq_ignore_ascii_case(STOREBROWSE) {
        vec![
            (if sso {
                STOREBROWSE_SSO
            } else {
                STOREBROWSE_NO_SSO
            })
            .to_owned(),
        ]
    } else if name.eq_ignore_ascii_case(SELF_SERVICE) && sso {
        vec![SELF_SERVICE_COMMAND.to_owned(), SELF_SERVICE_SSO.to_owned()]
    } else {
        return None;
    };
    // Each its own argument: a space or a quote reaches the launcher as it is.
    arguments.push(app.to_owned());
    arguments.push(url.to_owned());
    Some(arguments)
}

/// The Program Files folders, 32-bit first, as Windows says where they are: never those the
/// environment names. The 32-bit one is the only one on a 32-bit Windows. `None` off Windows.
fn program_files_folders() -> Option<(PathBuf, PathBuf)> {
    #[cfg(windows)]
    return both_or_either(
        heimdall_core::paths::program_files_x86(),
        heimdall_core::paths::program_files(),
    );
    #[cfg(not(windows))]
    return None;
}

/// The 32-bit and native Program Files folders, each standing for the other when only one is
/// known: one known is still searched, never skipped for the `PATH`.
#[must_use]
pub fn both_or_either(x86: Option<PathBuf>, native: Option<PathBuf>) -> Option<(PathBuf, PathBuf)> {
    match (x86, native) {
        (Some(x86), Some(native)) => Some((x86, native)),
        (Some(one), None) | (None, Some(one)) => Some((one.clone(), one)),
        (None, None) => None,
    }
}

/// The first of `candidates`, given the Program Files folders, that is there; else one of
/// `names` on the `PATH`.
fn find_program(candidates: fn(&Path, &Path) -> Vec<PathBuf>, names: &[&str]) -> Option<PathBuf> {
    if let Some((x86, native)) = program_files_folders()
        && let Some(found) = candidates(&x86, &native)
            .into_iter()
            .find(|path| path.is_file())
    {
        return Some(found);
    }
    let path = std::env::var_os(PATH_VARIABLE)?;
    on_path(names, &path, Path::is_file)
}

/// The first of `names` that `exists` in a folder of `path`, a `PATH` value: a relative
/// entry is skipped, so that nothing is found from the current folder.
pub fn on_path(
    names: &[&str],
    path: &std::ffi::OsStr,
    exists: impl Fn(&Path) -> bool,
) -> Option<PathBuf> {
    names.iter().find_map(|name| {
        std::env::split_paths(path)
            .filter(|folder| folder.is_absolute())
            .map(|folder| folder.join(name))
            .find(|candidate| exists(candidate))
    })
}

/// The Citrix Workspace launcher of this computer: the first candidate there, else one on
/// the `PATH`.
fn find_launcher() -> Option<PathBuf> {
    find_program(launcher_candidates, &[STOREBROWSE, SELF_SERVICE])
}

/// The command starting `SelfService.exe` with cache launch line `line`, passed unchanged as
/// the C# passes its arguments: no shell reads it, the forbidden characters were refused.
#[cfg(windows)]
fn cache_command(line: &str) -> Result<std::process::Command, CitrixRefusal> {
    use std::os::windows::process::CommandExt;
    let launcher = find_program(self_service_candidates, &[SELF_SERVICE])
        .ok_or(CitrixRefusal::WorkspaceNotFound)?;
    let mut command = std::process::Command::new(launcher);
    command.raw_arg(line);
    Ok(command)
}

/// A cache launch line is `SelfService.exe`'s grammar, which only Windows has.
#[cfg(not(windows))]
fn cache_command(_line: &str) -> Result<std::process::Command, CitrixRefusal> {
    Err(CitrixRefusal::Failed)
}

/// The command starting `launch`.
fn command(launch: &CitrixLaunch) -> Result<std::process::Command, CitrixRefusal> {
    Ok(match launch {
        CitrixLaunch::CacheLine(line) => cache_command(line)?,
        CitrixLaunch::IcaFile(path) => {
            let mut command = if cfg!(windows) {
                // The file's own program, given the path alone: no shell reads it. By its
                // whole path in the system folder, never one of the same name found first
                // elsewhere.
                let rundll =
                    heimdall_core::paths::system_program(heimdall_core::paths::RUNDLL_PROGRAM)
                        .ok_or_else(|| {
                            CitrixRefusal::NotStarted(
                                heimdall_core::paths::SYSTEM_FOLDER_UNKNOWN.to_owned(),
                            )
                        })?;
                let mut command = std::process::Command::new(rundll);
                command.arg(crate::external_edit::FILE_HANDLER_ENTRY);
                command
            } else {
                std::process::Command::new(crate::external_edit::XDG_OPEN_PROGRAM)
            };
            command.arg(path);
            command
        }
        CitrixLaunch::StoreFront { app, url, sso } => {
            let launcher = find_launcher().ok_or(CitrixRefusal::WorkspaceNotFound)?;
            let arguments =
                launcher_arguments(&launcher, app, url, *sso).ok_or(CitrixRefusal::Failed)?;
            let mut command = std::process::Command::new(&launcher);
            command.args(arguments);
            command
        }
    })
}

/// Launches `launch`: `SelfService.exe` given the cache line, the ICA file opened as a
/// double click opens it, or the launcher started with its arguments; no shell reading
/// them, no console shown. The launcher started is returned, for its exit code: it is not
/// the session, which Citrix Workspace's client runs once the launcher handed it over.
///
/// # Errors
///
/// [`CitrixRefusal`] when nothing was started.
pub fn launch(launch: &CitrixLaunch) -> Result<std::process::Child, CitrixRefusal> {
    let mut command = command(launch)?;
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(CREATE_NO_WINDOW);
    }
    command
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .map_err(|error| CitrixRefusal::NotStarted(error.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn profile() -> CitrixProfile {
        CitrixProfile {
            id: heimdall_core::profile::ProfileId::new("c"),
            name: "Outlook".to_owned(),
            group: None,
            store_front_url: Some("https://store.lab/Citrix/Store".to_owned()),
            app_name: Some("Outlook 365".to_owned()),
            ica_file: None,
            seamless: true,
            sso: true,
        }
    }

    #[test]
    fn a_store_front_application_is_asked_for_by_name() {
        assert_eq!(
            plan(&profile(), None),
            Ok(CitrixLaunch::StoreFront {
                app: "Outlook 365".to_owned(),
                url: "https://store.lab/Citrix/Store".to_owned(),
                sso: true,
            })
        );
        let unnamed = CitrixProfile {
            app_name: Some("  ".to_owned()),
            ..profile()
        };
        assert_eq!(plan(&unnamed, None), Err(CitrixRefusal::NotConfigured));
    }

    #[test]
    fn a_cache_line_launches_first_unless_a_shell_would_read_it() {
        let line = "-qlaunch \"Outlook 365\" -s store";
        assert_eq!(
            plan(&profile(), Some(line)),
            Ok(CitrixLaunch::CacheLine(Zeroizing::new(line.to_owned())))
        );
        assert!(matches!(
            plan(&profile(), Some("  ")),
            Ok(CitrixLaunch::StoreFront { .. })
        ));
        for bad in ['|', '&', ';', '`', '$', '\n', '\r'] {
            assert_eq!(
                plan(&profile(), Some(&format!("-qlaunch a{bad}b"))),
                Err(CitrixRefusal::CommandRejected),
                "{bad:?}"
            );
        }
        let shown = format!(
            "{:?}",
            CitrixLaunch::CacheLine(Zeroizing::new(line.to_owned()))
        );
        assert!(!shown.contains("Outlook"), "{shown}");
        assert_eq!(
            self_service_candidates(Path::new("X86"), Path::new("PF")).first(),
            Some(&PathBuf::from(
                "X86/Citrix/ICA Client/SelfServicePlugin/SelfService.exe"
            ))
        );
    }

    #[test]
    fn the_store_front_is_checked_first_as_the_csharp_checks_it() {
        let with = |url: &str| CitrixProfile {
            store_front_url: Some(url.to_owned()),
            ..profile()
        };
        assert_eq!(
            plan(&with("ftp://store.lab"), None),
            Err(CitrixRefusal::InvalidStoreFront)
        );
        assert_eq!(
            plan(&with("https://admin:secret@store.lab/"), None),
            Err(CitrixRefusal::StoreFrontCredentials)
        );
        assert_eq!(
            plan(&with("store.lab"), None),
            Err(CitrixRefusal::InvalidStoreFront)
        );
    }

    #[test]
    fn an_ica_file_is_opened_only_from_this_computer_and_when_there() {
        assert_eq!(
            ica_file(r"\\share\apps\outlook.ica"),
            Err(CitrixRefusal::InvalidIcaFile)
        );
        assert_eq!(
            ica_file("C:/apps/outlook.exe"),
            Err(CitrixRefusal::InvalidIcaFile)
        );
        assert!(ica_file("C:/apps/Outlook.ICA").is_ok());

        let dir = tempfile::tempdir().expect("dir");
        let file = dir.path().join("outlook.ica");
        std::fs::write(&file, "[ApplicationServers]\n").expect("write");
        let launching = CitrixProfile {
            ica_file: Some(file.display().to_string()),
            ..profile()
        };
        assert_eq!(plan(&launching, None), Ok(CitrixLaunch::IcaFile(file)));
        // Not there: its StoreFront application instead, as the C#.
        let gone = CitrixProfile {
            ica_file: Some(dir.path().join("gone.ica").display().to_string()),
            ..profile()
        };
        assert!(matches!(
            plan(&gone, None),
            Ok(CitrixLaunch::StoreFront { .. })
        ));
    }

    #[test]
    fn each_launcher_is_given_its_own_grammar_or_nothing() {
        let url = "https://store.lab/";
        assert_eq!(
            launcher_arguments(Path::new("storebrowse.exe"), "My app", url, true),
            Some(vec!["-S".to_owned(), "My app".to_owned(), url.to_owned()])
        );
        assert_eq!(
            launcher_arguments(Path::new("storebrowse.exe"), "My app", url, false)
                .map(|a| a[0].clone()),
            Some("-L".to_owned())
        );
        assert_eq!(
            launcher_arguments(Path::new("SelfService.exe"), "My app", url, true),
            Some(vec![
                "storebrowse".to_owned(),
                "-q".to_owned(),
                "My app".to_owned(),
                url.to_owned()
            ])
        );
        assert_eq!(
            launcher_arguments(Path::new("SelfService.exe"), "My app", url, false),
            None,
            "no single sign-on: no documented invocation"
        );
        assert_eq!(
            launcher_arguments(Path::new("other.exe"), "My app", url, true),
            None
        );
        let candidates = launcher_candidates(Path::new("X86"), Path::new("PF"));
        assert_eq!(
            candidates.first(),
            Some(&PathBuf::from(
                "X86/Citrix/ICA Client/AuthManager/storebrowse.exe"
            ))
        );
        assert_eq!(candidates.len(), 8);
    }

    #[test]
    fn one_program_files_folder_known_is_still_searched() {
        let x86 = PathBuf::from("X86");
        let native = PathBuf::from("PF");
        assert_eq!(
            both_or_either(Some(x86.clone()), Some(native.clone())),
            Some((x86.clone(), native.clone()))
        );
        assert_eq!(
            both_or_either(Some(x86.clone()), None),
            Some((x86.clone(), x86))
        );
        assert_eq!(
            both_or_either(None, Some(native.clone())),
            Some((native.clone(), native))
        );
        assert_eq!(both_or_either(None, None), None);
    }

    #[test]
    fn a_relative_path_entry_is_never_searched() {
        let absolute = std::env::temp_dir();
        let path =
            std::env::join_paths([PathBuf::from("relative"), absolute.clone()]).expect("path");
        let everywhere = |_: &Path| true;
        assert_eq!(
            on_path(&[SELF_SERVICE], &path, everywhere),
            Some(absolute.join(SELF_SERVICE))
        );
        let only_relative = std::env::join_paths([PathBuf::from("relative")]).expect("path");
        assert_eq!(on_path(&[SELF_SERVICE], &only_relative, everywhere), None);
    }
}
