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

//! The `PSModulePath` a Windows `PowerShell` child gets, as the C# `WindowsPowerShellModulePath`.
//!
//! Heimdall started from `PowerShell` 7 inherits its module path, and Windows `PowerShell` given
//! it verbatim loads `PowerShell` 7's modules: their manifests for `Microsoft.PowerShell.Utility`
//! and `Security` do not load in 5.1, so `Get-FileHash` and `Get-AuthenticodeSignature` are
//! missing, and under an `AllSigned` policy its `PSReadLine` asks about an untrusted publisher
//! before anything runs. `PowerShell` 7 itself removes its personal, shared and `$PSHOME` module
//! folders when it starts `powershell.exe`, keeping every other entry; this does the same.
//!
//! The paths are Windows paths handled as text, so the rule reads the same wherever it is
//! tested.

/// Name of the variable.
pub const VARIABLE: &str = "PSModulePath";

/// Separator of its entries.
const SEPARATOR: char = ';';

/// Leaf name of every module folder.
const MODULES: &str = "Modules";

/// Program file of `PowerShell` 7, found beside its own module folder.
const POWERSHELL_7_PROGRAM: &str = "pwsh.exe";

/// Program file of Windows `PowerShell`.
const WINDOWS_POWERSHELL_PROGRAM: &str = "powershell.exe";

/// The module folders that decide the path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModuleRoots {
    /// `PowerShell` 7's personal and shared module folders.
    pub powershell_7: Vec<String>,
    /// The path Windows `PowerShell` builds for itself when it inherits none.
    pub windows_powershell_defaults: Vec<String>,
}

impl ModuleRoots {
    /// The folders of the current user and machine: under Documents, Program Files and the
    /// Windows folder, all as Windows says where they are, never as the environment names them.
    #[cfg(windows)]
    #[must_use]
    pub fn for_current_user() -> Self {
        let folder =
            |path: Option<std::path::PathBuf>| path.map(|path| path.to_string_lossy().into_owned());
        let documents = folder(heimdall_core::paths::documents());
        let program_files = folder(heimdall_core::paths::program_files());
        let system_root = folder(heimdall_core::paths::system_root());
        let under = |base: &Option<String>, rest: &str| {
            base.as_ref()
                .map(|base| format!("{}\\{rest}", trimmed(base)))
        };
        Self {
            powershell_7: [
                under(&documents, "PowerShell\\Modules"),
                under(&program_files, "PowerShell\\Modules"),
            ]
            .into_iter()
            .flatten()
            .collect(),
            windows_powershell_defaults: [
                under(&documents, "WindowsPowerShell\\Modules"),
                under(&program_files, "WindowsPowerShell\\Modules"),
                under(&system_root, "System32\\WindowsPowerShell\\v1.0\\Modules"),
            ]
            .into_iter()
            .flatten()
            .collect(),
        }
    }
}

/// Whether `program` is Windows `PowerShell`, by its file name.
#[must_use]
pub fn is_windows_powershell(program: &str) -> bool {
    file_name(program).eq_ignore_ascii_case(WINDOWS_POWERSHELL_PROGRAM)
}

/// `inherited` without `PowerShell` 7's entries: the fixed folders of `roots`, and any
/// `...\Modules` folder with `pwsh.exe` beside it, wherever `PowerShell` 7 is installed.
/// Windows `PowerShell`'s own defaults when nothing is left, since it keeps an inherited path
/// verbatim, an empty one included; `None` when nothing was inherited, so it builds its own.
#[must_use]
pub fn for_windows_powershell(
    inherited: Option<&str>,
    roots: &ModuleRoots,
    file_exists: impl Fn(&str) -> bool,
) -> Option<String> {
    let inherited = inherited?;
    let kept: Vec<&str> = inherited
        .split(SEPARATOR)
        .map(str::trim)
        .filter(|entry| {
            let entry = trimmed(entry);
            !entry.is_empty()
                && !roots
                    .powershell_7
                    .iter()
                    .any(|root| trimmed(root).eq_ignore_ascii_case(entry))
                && !is_powershell_7_home(entry, &file_exists)
        })
        .collect();
    let kept = if kept.is_empty() {
        roots
            .windows_powershell_defaults
            .iter()
            .map(String::as_str)
            .collect()
    } else {
        kept
    };
    Some(kept.join(&SEPARATOR.to_string()))
}

/// Whether `entry` is the module folder of a `PowerShell` 7 home: a full path ending in
/// `Modules` with `pwsh.exe` beside it.
fn is_powershell_7_home(entry: &str, file_exists: &impl Fn(&str) -> bool) -> bool {
    let Some((home, leaf)) = entry.rsplit_once(['\\', '/']) else {
        return false;
    };
    is_full_path(entry)
        && leaf.eq_ignore_ascii_case(MODULES)
        && !home.is_empty()
        && file_exists(&format!("{home}\\{POWERSHELL_7_PROGRAM}"))
}

/// A drive path (`C:\...`) or a network one (`\\server\...`).
fn is_full_path(path: &str) -> bool {
    let bytes = path.as_bytes();
    path.starts_with("\\\\")
        || (bytes.len() >= 3
            && bytes[0].is_ascii_alphabetic()
            && bytes[1] == b':'
            && matches!(bytes[2], b'\\' | b'/'))
}

/// `path` without the separators it ends with.
fn trimmed(path: &str) -> &str {
    path.trim().trim_end_matches(['\\', '/'])
}

/// The last component of `path`, whichever separator it uses.
fn file_name(path: &str) -> &str {
    trimmed(path).rsplit(['\\', '/']).next().unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn roots() -> ModuleRoots {
        ModuleRoots {
            powershell_7: vec![
                r"C:\Users\u\Documents\PowerShell\Modules".to_owned(),
                r"C:\Program Files\PowerShell\Modules".to_owned(),
            ],
            windows_powershell_defaults: vec![
                r"C:\Users\u\Documents\WindowsPowerShell\Modules".to_owned(),
                r"C:\Program Files\WindowsPowerShell\Modules".to_owned(),
                r"C:\WINDOWS\System32\WindowsPowerShell\v1.0\Modules".to_owned(),
            ],
        }
    }

    fn pwsh_at(home: &'static str) -> impl Fn(&str) -> bool {
        move |file: &str| file.eq_ignore_ascii_case(&format!("{home}\\pwsh.exe"))
    }

    #[test]
    fn powershell_7_entries_go_and_every_other_stays_in_order() {
        // As pwsh 7 sets it, with a module folder of the user's own at the end.
        let inherited = r"C:\Users\u\Documents\PowerShell\Modules;C:\Program Files\PowerShell\Modules;c:\program files\powershell\7\Modules;C:\Program Files\WindowsPowerShell\Modules;C:\WINDOWS\system32\WindowsPowerShell\v1.0\Modules;D:\Tools\Modules\";
        assert_eq!(
            for_windows_powershell(
                Some(inherited),
                &roots(),
                pwsh_at(r"C:\Program Files\PowerShell\7")
            )
            .as_deref(),
            Some(
                r"C:\Program Files\WindowsPowerShell\Modules;C:\WINDOWS\system32\WindowsPowerShell\v1.0\Modules;D:\Tools\Modules\"
            )
        );
    }

    #[test]
    fn a_powershell_7_home_is_told_by_pwsh_beside_it_wherever_installed() {
        let inherited = r"E:\Apps\pwsh\Modules;E:\Apps\other\Modules";
        assert_eq!(
            for_windows_powershell(Some(inherited), &roots(), pwsh_at(r"E:\Apps\pwsh")).as_deref(),
            Some(r"E:\Apps\other\Modules")
        );
        assert_eq!(
            for_windows_powershell(Some(r"Modules"), &roots(), |_| true).as_deref(),
            Some("Modules"),
            "a relative entry is never a home"
        );
    }

    #[test]
    fn nothing_inherited_is_left_to_windows_powershell_and_nothing_left_gets_its_defaults() {
        assert_eq!(for_windows_powershell(None, &roots(), |_| false), None);
        let defaults = roots().windows_powershell_defaults.join(";");
        for inherited in ["", " ; ", r"C:\Program Files\PowerShell\Modules\"] {
            assert_eq!(
                for_windows_powershell(Some(inherited), &roots(), |_| false).as_deref(),
                Some(defaults.as_str()),
                "{inherited:?}"
            );
        }
    }

    #[test]
    fn windows_powershell_is_told_by_its_file_name() {
        assert!(is_windows_powershell(
            r"C:\WINDOWS\System32\WindowsPowerShell\v1.0\PowerShell.exe"
        ));
        assert!(is_windows_powershell("powershell.exe"));
        assert!(!is_windows_powershell(
            r"C:\Program Files\PowerShell\7\pwsh.exe"
        ));
        assert!(!is_windows_powershell(r"C:\tools\powershell.exe.bak"));
    }
}
