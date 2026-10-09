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

//! The X server X11 forwarding draws on, as the C# `X11ServerManager`: one already running is
//! used; otherwise, when the settings allow it, one is started, from the path chosen, the
//! places the C# knows, then the folders of `PATH`. Only a server started here is stopped
//! when the application ends. The application's own `DISPLAY` is never changed: the program
//! forwarding X11 is given one of its own.
//!
//! A server runs when display :0 answers on this computer's TCP port, whatever program it
//! is: the C# looked for process names, which a server under another name escapes.

use std::ffi::OsStr;
use std::net::{Ipv4Addr, SocketAddr, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::{Mutex, PoisonError};
use std::time::{Duration, Instant};

/// The display the forwarding program is given, as the C# `DefaultDisplay`.
pub const DISPLAY: &str = "localhost:0.0";

/// The variable naming the display.
pub const DISPLAY_VARIABLE: &str = "DISPLAY";

/// The TCP port of display :0: X11 listens on 6000 plus the display's number.
pub const DISPLAY_PORT: u16 = 6000;

/// How long display :0 has to answer before no server is said to run.
pub const DETECT_TIMEOUT: Duration = Duration::from_millis(300);

/// How long a server started here has to answer before the launch goes on without it.
pub const READY_TIMEOUT: Duration = Duration::from_secs(3);

/// How often a server just started is asked whether it answers.
const READY_POLL: Duration = Duration::from_millis(100);

/// The places the C# looks for a server, in its order.
pub const KNOWN_INSTALL_PATHS: [&str; 5] = [
    r"C:\Program Files\VcXsrv\vcxsrv.exe",
    r"C:\Program Files (x86)\Xming\Xming.exe",
    r"C:\Program Files\Xming\Xming.exe",
    r"C:\cygwin64\bin\XWin.exe",
    r"C:\cygwin\bin\XWin.exe",
];

/// The servers looked for on `PATH`, by program name, in the C# order.
pub const KNOWN_PROGRAMS: [&str; 4] = ["vcxsrv", "xming", "x410", "xwin"];

/// The program whose arguments are given, as the C# gives them to `VcXsrv` alone.
const VCXSRV: &str = "vcxsrv";

/// `VcXsrv`'s command line, as the C# `VcXsrvArguments`: display :0, a window per X client,
/// the clipboard shared without the primary selection. Access control stays on: `-ac` would
/// let any host that reaches the port read the windows and the clipboard.
pub const VCXSRV_ARGUMENTS: [&str; 4] = [":0", "-multiwindow", "-clipboard", "-noprimary"];

/// What the settings say of the X server.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct X11Settings {
    /// The server chosen; empty for the known places, then `PATH`.
    pub server_path: String,
    /// A server is started when none runs.
    pub auto_start: bool,
}

impl X11Settings {
    /// What `settings` say of the X server.
    #[must_use]
    pub fn of(settings: &heimdall_core::settings::Settings) -> Self {
        Self {
            server_path: settings.x11_server_path.clone(),
            auto_start: settings.x11_auto_start,
        }
    }
}

/// Whether an X server could be counted on, and which.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum X11Outcome {
    /// One was running.
    Running,
    /// This one was started.
    Started(PathBuf),
    /// None runs, and none was started: the settings say not to, none was found, or it
    /// did not start.
    Unavailable,
}

impl X11Outcome {
    /// Whether a display can be given to a program.
    #[must_use]
    pub fn available(&self) -> bool {
        !matches!(self, Self::Unavailable)
    }
}

/// Whether an X server answers on this computer's `port`, within `timeout`.
#[must_use]
pub fn answers(port: u16, timeout: Duration) -> bool {
    let address = SocketAddr::from((Ipv4Addr::LOCALHOST, port));
    TcpStream::connect_timeout(&address, timeout).is_ok()
}

/// The server to start, as the C# `ResolveServerPath`: `configured` when it is a file, then
/// the first known place that is one, then the known programs in the absolute folders of
/// `path_var`, folder by folder. A relative folder, the current one among them, is never
/// searched. `is_file` says whether a candidate exists.
#[must_use]
pub fn resolve(
    configured: &str,
    path_var: Option<&OsStr>,
    is_file: impl Fn(&Path) -> bool,
) -> Option<PathBuf> {
    let configured = configured.trim();
    if !configured.is_empty() && Path::new(configured).is_absolute() {
        let chosen = PathBuf::from(configured);
        if is_file(&chosen) {
            return Some(chosen);
        }
    }
    if let Some(known) = KNOWN_INSTALL_PATHS
        .iter()
        .map(PathBuf::from)
        .find(|path| is_file(path))
    {
        return Some(known);
    }
    let folders = path_var.map(std::env::split_paths).into_iter().flatten();
    for folder in folders.filter(|folder| folder.is_absolute()) {
        for name in KNOWN_PROGRAMS {
            let candidate = folder.join(format!("{name}{}", std::env::consts::EXE_SUFFIX));
            if is_file(&candidate) {
                return Some(candidate);
            }
        }
    }
    None
}

/// The arguments `program` is started with: `VcXsrv`'s, as the C#, none for another server.
#[must_use]
pub fn arguments(program: &Path) -> &'static [&'static str] {
    let stem = program
        .file_stem()
        .and_then(OsStr::to_str)
        .map(str::to_ascii_lowercase);
    if stem.as_deref() == Some(VCXSRV) {
        &VCXSRV_ARGUMENTS
    } else {
        &[]
    }
}

/// The X server this application started, if it did: it alone is stopped at the end.
#[derive(Debug, Default)]
pub struct X11Server {
    managed: Mutex<Option<Child>>,
}

/// The application's X server, shared by every launch.
static SHARED: X11Server = X11Server::new();

/// The application's X server.
#[must_use]
pub fn shared() -> &'static X11Server {
    &SHARED
}

impl X11Server {
    /// None started yet.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            managed: Mutex::new(None),
        }
    }

    /// An X server to count on, display :0 asked on `port`: the one running, else, when
    /// `settings` allow it, one started, as the C# `EnsureRunningAsync`.
    pub fn ensure(&self, settings: &X11Settings, port: u16) -> X11Outcome {
        if answers(port, DETECT_TIMEOUT) {
            log::info!("an X server answers on display :0");
            return X11Outcome::Running;
        }
        let mut managed = self.managed.lock().unwrap_or_else(PoisonError::into_inner);
        if let Some(child) = managed.as_mut()
            && matches!(child.try_wait(), Ok(None))
        {
            // Started here, still starting: counted on, as the C#.
            return X11Outcome::Running;
        }
        if !settings.auto_start {
            log::info!("no X server answers and starting one is turned off");
            return X11Outcome::Unavailable;
        }
        let path = std::env::var_os("PATH");
        let Some(program) = resolve(&settings.server_path, path.as_deref(), Path::is_file) else {
            log::warn!("no X server answers and none was found to start");
            return X11Outcome::Unavailable;
        };
        match start(&program) {
            Ok(child) => {
                log::info!("X server started: {}", program.display());
                *managed = Some(child);
                drop(managed);
                wait_ready(port);
                X11Outcome::Started(program)
            }
            Err(error) => {
                log::warn!("the X server {} did not start: {error}", program.display());
                X11Outcome::Unavailable
            }
        }
    }

    /// Stops the server started here, if any and still running; one started elsewhere is
    /// left alone.
    pub fn stop(&self) {
        let taken = self
            .managed
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .take();
        let Some(mut child) = taken else {
            return;
        };
        if matches!(child.try_wait(), Ok(None)) {
            match child.kill() {
                Ok(()) => log::info!("the X server started here was stopped"),
                Err(error) => log::warn!("the X server started here was not stopped: {error}"),
            }
        }
        let _ = child.wait();
    }
}

/// Starts `program`, by its whole path, no shell between: no console, nothing read from it.
fn start(program: &Path) -> std::io::Result<Child> {
    let mut command = Command::new(program);
    command
        .args(arguments(program))
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    if let Some(folder) = program.parent() {
        command.current_dir(folder);
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        /// No console window, as the C# `CreateNoWindow`.
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        command.creation_flags(CREATE_NO_WINDOW);
    }
    command.spawn()
}

/// Waits, at most [`READY_TIMEOUT`], for a server just started to answer on `port`.
fn wait_ready(port: u16) {
    let deadline = Instant::now() + READY_TIMEOUT;
    while Instant::now() < deadline {
        if answers(port, DETECT_TIMEOUT) {
            return;
        }
        std::thread::sleep(READY_POLL);
    }
    log::warn!("the X server started does not answer yet: the launch goes on");
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;
    use std::ffi::OsString;
    use std::net::TcpListener;

    use super::*;

    /// A test's files: the paths that exist.
    fn files(paths: &[&Path]) -> impl Fn(&Path) -> bool + use<> {
        let present: HashSet<PathBuf> = paths.iter().map(|path| path.to_path_buf()).collect();
        move |path| present.contains(path)
    }

    fn path_var(folders: &[&Path]) -> OsString {
        std::env::join_paths(folders).expect("joined")
    }

    fn exe(name: &str) -> String {
        format!("{name}{}", std::env::consts::EXE_SUFFIX)
    }

    /// A port nothing can listen on. A port freed by a test is not one: another test running
    /// at the same time can be given it and listen there.
    const NO_LISTENER: u16 = 0;

    #[test]
    fn a_server_listening_on_the_port_is_detected_and_none_otherwise() {
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).expect("bound");
        let port = listener.local_addr().expect("address").port();
        assert!(answers(port, DETECT_TIMEOUT));
        assert!(!answers(NO_LISTENER, DETECT_TIMEOUT), "nothing listens");
    }

    #[test]
    fn a_running_server_is_used_and_nothing_is_started() {
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).expect("bound");
        let port = listener.local_addr().expect("address").port();
        let server = X11Server::new();
        let settings = X11Settings {
            server_path: String::new(),
            auto_start: false,
        };
        assert_eq!(server.ensure(&settings, port), X11Outcome::Running);
        assert!(server.managed.lock().expect("lock").is_none());
    }

    #[test]
    fn with_starting_turned_off_and_none_running_none_is_counted_on() {
        let settings = X11Settings {
            server_path: "/nowhere/vcxsrv".to_owned(),
            auto_start: false,
        };
        let outcome = X11Server::new().ensure(&settings, NO_LISTENER);
        assert_eq!(outcome, X11Outcome::Unavailable);
        assert!(!outcome.available());
    }

    #[test]
    fn the_chosen_server_comes_first_then_the_known_places_then_path() {
        let dir = tempfile::tempdir().expect("dir");
        let chosen = dir.path().join("my-x.exe");
        let known = Path::new(KNOWN_INSTALL_PATHS[1]);
        let on_path = dir.path().join("bin").join(exe("xwin"));
        let configured = chosen.to_string_lossy().into_owned();
        let all = files(&[&chosen, known, &on_path]);
        let search = path_var(&[&dir.path().join("bin")]);
        assert_eq!(
            resolve(&configured, Some(&search), &all),
            Some(chosen.clone())
        );
        // The chosen one missing: the known places, in the C# order.
        let without_chosen = files(&[known, &on_path]);
        assert_eq!(
            resolve(&configured, Some(&search), &without_chosen),
            Some(known.to_path_buf())
        );
        let earlier = Path::new(KNOWN_INSTALL_PATHS[0]);
        let both = files(&[known, earlier]);
        assert_eq!(resolve("", None, &both), Some(earlier.to_path_buf()));
        // None of those: PATH.
        assert_eq!(
            resolve(" ", Some(&search), files(&[&on_path])),
            Some(on_path.clone())
        );
        assert_eq!(resolve("", Some(&search), files(&[])), None);
    }

    #[test]
    fn path_is_searched_folder_by_folder_and_never_in_a_relative_folder() {
        let dir = tempfile::tempdir().expect("dir");
        let first = dir.path().join("first");
        let second = dir.path().join("second");
        let xming_second = second.join(exe("xming"));
        let vcxsrv_second = second.join(exe("vcxsrv"));
        let x410_first = first.join(exe("x410"));
        let search = path_var(&[&first, &second]);
        // The first folder wins over a program named earlier, as the C# loops.
        assert_eq!(
            resolve(
                "",
                Some(&search),
                files(&[&xming_second, &vcxsrv_second, &x410_first])
            ),
            Some(x410_first)
        );
        let relative = Path::new("bin");
        let in_relative = relative.join(exe("vcxsrv"));
        let search = path_var(&[relative]);
        assert_eq!(resolve("", Some(&search), files(&[&in_relative])), None);
        // A relative path chosen is not taken either.
        let chosen = Path::new("vcxsrv.exe");
        assert_eq!(resolve("vcxsrv.exe", None, files(&[chosen])), None);
    }

    #[test]
    fn vcxsrv_alone_gets_the_csharp_arguments() {
        assert_eq!(
            arguments(&Path::new("VcXsrv").join("VcXsrv.exe")),
            [":0", "-multiwindow", "-clipboard", "-noprimary"]
        );
        assert_eq!(arguments(Path::new("/opt/x/vcxsrv")), VCXSRV_ARGUMENTS);
        assert!(arguments(&Path::new("Xming").join("Xming.exe")).is_empty());
        assert!(
            !VCXSRV_ARGUMENTS.contains(&"-ac"),
            "access control stays on"
        );
    }

    #[test]
    fn stopping_with_nothing_started_does_nothing() {
        let server = X11Server::new();
        server.stop();
        assert!(server.managed.lock().expect("lock").is_none());
    }
}
