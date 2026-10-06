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

//! What opening a file of this computer does from the local file browser.
//!
//! The C# `LocalFileBrowserView` opens a text file in its editor, and anything else with
//! the system's default program, at once: a program or a script among them, which then
//! runs. Here a text file opens in the external editor set, which shows it and runs
//! nothing; a file that would run is shown, whole, and opens only once agreed; anything
//! else opens with the system's default program at once.
//!
//! What runs: on Windows, the extensions `PATHEXT` names, and the file types Windows
//! itself takes for dangerous to open (programs, installers, shortcuts, script hosts,
//! registry files, help files, interpreters' scripts); elsewhere, a file with an execute
//! bit, and a desktop entry, whose command the desktop's opener starts whatever its mode.

use std::path::Path;

/// The extensions of a text file, as the C# `LocalFileBrowserViewModel.TextExtensions`,
/// compared whatever their case.
pub const TEXT_EXTENSIONS: [&str; 38] = [
    ".txt",
    ".log",
    ".md",
    ".json",
    ".xml",
    ".yaml",
    ".yml",
    ".conf",
    ".cfg",
    ".ini",
    ".ps1",
    ".bat",
    ".cmd",
    ".sh",
    ".bash",
    ".py",
    ".rb",
    ".js",
    ".ts",
    ".css",
    ".html",
    ".htm",
    ".cs",
    ".java",
    ".c",
    ".cpp",
    ".h",
    ".hpp",
    ".sql",
    ".csv",
    ".env",
    ".toml",
    ".properties",
    ".service",
    ".timer",
    ".socket",
    ".gitignore",
    ".editorconfig",
];

/// The file types Windows takes for dangerous to open, beyond those `PATHEXT` names,
/// compared whatever their case: those the Windows Attachment Manager rates high risk, as
/// Microsoft's list of the attachments Outlook blocks gives them. Opened, each runs,
/// installs, or hands its content to a program that acts on it: a script host, an
/// interpreter, a shell handler, a help viewer.
pub const WINDOWS_RUNNABLE_EXTENSIONS: [&str; 117] = [
    ".ade",
    ".adp",
    ".app",
    ".application",
    ".appref-ms",
    ".asp",
    ".bas",
    ".bat",
    ".cer",
    ".chm",
    ".cmd",
    ".cnt",
    ".com",
    ".cpl",
    ".crt",
    ".csh",
    ".der",
    ".diagcab",
    ".exe",
    ".fxp",
    ".gadget",
    ".grp",
    ".hlp",
    ".hpj",
    ".hta",
    ".inf",
    ".ins",
    ".isp",
    ".its",
    ".jar",
    ".jnlp",
    ".js",
    ".jse",
    ".ksh",
    ".library-ms",
    ".lnk",
    ".mad",
    ".maf",
    ".mag",
    ".mam",
    ".maq",
    ".mar",
    ".mas",
    ".mat",
    ".mau",
    ".mav",
    ".maw",
    ".mcf",
    ".mda",
    ".mdb",
    ".mde",
    ".mdt",
    ".mdw",
    ".mdz",
    ".msc",
    ".msh",
    ".msh1",
    ".msh2",
    ".mshxml",
    ".msh1xml",
    ".msh2xml",
    ".msi",
    ".msp",
    ".mst",
    ".msu",
    ".ops",
    ".osd",
    ".pcd",
    ".pif",
    ".pl",
    ".plg",
    ".prf",
    ".prg",
    ".printerexport",
    ".ps1",
    ".ps1xml",
    ".ps2",
    ".ps2xml",
    ".psc1",
    ".psc2",
    ".psd1",
    ".psdm1",
    ".pst",
    ".py",
    ".pyc",
    ".pyo",
    ".pyw",
    ".pyz",
    ".pyzw",
    ".reg",
    ".scf",
    ".scr",
    ".sct",
    ".search-ms",
    ".settingcontent-ms",
    ".shb",
    ".shs",
    ".theme",
    ".tmp",
    ".url",
    ".vb",
    ".vbe",
    ".vbp",
    ".vbs",
    ".vhd",
    ".vhdx",
    ".vsmacros",
    ".vsw",
    ".webpnp",
    ".website",
    ".ws",
    ".wsc",
    ".wsf",
    ".wsh",
    ".xbap",
    ".xll",
    ".xnk",
];

/// The variable listing the extensions Windows runs as programs.
pub const PATHEXT_VARIABLE: &str = "PATHEXT";

/// `PATHEXT` as Windows sets it, taken when the variable is not there.
pub const DEFAULT_PATHEXT: &str = ".COM;.EXE;.BAT;.CMD;.VBS;.VBE;.JS;.JSE;.WSF;.WSH;.MSC";

/// The separator of `PATHEXT`'s extensions.
const PATHEXT_SEPARATOR: char = ';';

/// The execute bits of a Unix mode: its owner's, its group's and everyone's.
pub const EXECUTE_BITS: u32 = 0o111;

/// The extensions of the Unix files a desktop's opener (`xdg-open`) starts a command for
/// whatever their mode: a desktop entry, whose `Exec` line it runs.
pub const UNIX_LAUNCHER_EXTENSIONS: [&str; 1] = [".desktop"];

/// What opening a file of this computer does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LocalOpening {
    /// A text file: shown in the external editor, nothing run.
    Edit,
    /// A file that would run: the user is asked first, with its full path.
    Confirm,
    /// Anything else: opened with the system's default program at once.
    Open,
}

/// How file `name` opens, `runnable` as the platform's rule says: a text file in the
/// editor first, since the editor shows a script rather than runs it; then a file that
/// would run, asked first; anything else at once.
#[must_use]
pub fn opening(name: &str, runnable: bool) -> LocalOpening {
    if is_text(name) {
        LocalOpening::Edit
    } else if runnable {
        LocalOpening::Confirm
    } else {
        LocalOpening::Open
    }
}

/// The extension of `name` with its dot, as the C# `Path.GetExtension`: from its last
/// dot, a name that starts with its only dot (`.gitignore`) being all extension; `None`
/// for a name with no dot, or ending with one.
#[must_use]
pub fn extension(name: &str) -> Option<&str> {
    name.rfind('.')
        .map(|at| &name[at..])
        .filter(|extension| extension.len() > 1)
}

/// Whether `name` is a text file, as the C# `IsTextFile`.
#[must_use]
pub fn is_text(name: &str) -> bool {
    extension(name).is_some_and(|extension| {
        TEXT_EXTENSIONS
            .iter()
            .any(|text| extension.eq_ignore_ascii_case(text))
    })
}

/// Whether Windows runs file `name` when it is opened, `pathext` the variable's value when
/// it is set: an extension it names, or one of [`WINDOWS_RUNNABLE_EXTENSIONS`]. Dots and
/// spaces ending the name are left out, as Windows drops them when it opens the file.
#[must_use]
pub fn windows_runnable(name: &str, pathext: Option<&str>) -> bool {
    let Some(extension) = extension(name.trim_end_matches(['.', ' '])) else {
        return false;
    };
    let named = pathext
        .unwrap_or(DEFAULT_PATHEXT)
        .split(PATHEXT_SEPARATOR)
        .map(str::trim)
        .filter(|listed| !listed.is_empty());
    WINDOWS_RUNNABLE_EXTENSIONS
        .iter()
        .copied()
        .chain(named)
        .any(|runs| extension.eq_ignore_ascii_case(runs))
}

/// Whether Unix file `name`, of `mode` when it could be read, runs when opened: a desktop
/// entry whatever its mode, any of its execute bits set, or a mode unknown, asked rather
/// than opened.
#[must_use]
pub fn unix_runnable(name: &str, mode: Option<u32>) -> bool {
    let launcher = extension(name).is_some_and(|extension| {
        UNIX_LAUNCHER_EXTENSIONS
            .iter()
            .any(|launches| extension.eq_ignore_ascii_case(launches))
    });
    launcher || mode.is_none_or(|mode| mode & EXECUTE_BITS != 0)
}

/// Whether file `path`, named `name`, runs when opened, by this platform's rule.
#[must_use]
pub fn runnable(name: &str, path: &Path) -> bool {
    #[cfg(windows)]
    {
        let _ = path;
        let pathext = std::env::var(PATHEXT_VARIABLE).ok();
        windows_runnable(name, pathext.as_deref())
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        let mode = std::fs::metadata(path).map(|found| found.permissions().mode());
        unix_runnable(name, mode.ok())
    }
    #[cfg(not(any(windows, unix)))]
    {
        let _ = (name, path);
        true
    }
}
