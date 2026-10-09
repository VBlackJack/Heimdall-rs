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

//! An SSH profile opened in `PuTTY`, as the C# `SshHandler` external mode: the server's host
//! key probed first and checked against the application's own `known_hosts`, then `PuTTY`
//! started with the C# arguments, each one its own, no shell reading them, told to accept
//! that key alone. No password is ever given: `PuTTY` asks for it.
//!
//! With X11 forwarding, an X server is counted on first; `PuTTY` alone is given its display.
//! Without one, it starts without `-X`, as the C# does.
//!
//! Behind an SSH gateway, `PuTTY` is pointed at a forward of this computer's loopback address
//! instead, as the C# points it at its tunnel: see [`crate::putty_driver`].

use std::ffi::{OsStr, OsString};
use std::future::Future;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::process::{Command, Stdio};
use std::sync::Arc;

use heimdall_core::profile::SshProfile;
use heimdall_core::settings::Settings;
use heimdall_ssh::{ConnectError, ConnectOptions, PublicKey, fingerprint, trusted_host_key};
use tokio_util::sync::CancellationToken;

use crate::error::UiError;
use crate::x11_server::{self, X11Outcome, X11Settings};

/// `PuTTY`'s program, looked for in the folders of `PATH` when no path is chosen.
pub const PUTTY_PROGRAM: &str = if cfg!(windows) { "putty.exe" } else { "putty" };

/// The protocol argument.
const SSH_ARGUMENT: &str = "-ssh";
/// The key file argument, followed by its path.
const KEY_ARGUMENT: &str = "-i";
/// The compression argument.
const COMPRESSION_ARGUMENT: &str = "-C";
/// The agent forwarding argument.
const AGENT_ARGUMENT: &str = "-A";
/// The X11 forwarding argument.
const X11_ARGUMENT: &str = "-X";
/// The port argument, followed by the port.
const PORT_ARGUMENT: &str = "-P";
/// The host key argument, followed by the key's fingerprint: `PuTTY` then accepts that key
/// alone, whatever its own cache holds.
const HOST_KEY_ARGUMENT: &str = "-hostkey";

/// Why an SSH profile was not opened in `PuTTY`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PuttyRefusal {
    /// The SSH gateway the profile goes through was not reached, or refused the user: why.
    Gateway(UiError),
    /// No port of this computer's loopback address could be opened for the gateway's
    /// forward: why.
    Forward(String),
    /// The host would be read as an option, or holds a space or a control character, as
    /// the C# input validation refuses it.
    InvalidHost,
    /// The user name would be read as an option, or holds a space or a control character.
    InvalidUsername,
    /// The key file is not named by an absolute path, or is not there.
    KeyFile(PathBuf),
    /// The server's host key is not trusted, or could not be read: why.
    HostKey(UiError),
    /// `PuTTY` was not found.
    NotFound,
    /// `PuTTY` did not start, and why.
    NotStarted(String),
}

/// What the probe found of the server's host key.
#[derive(Debug, Clone)]
pub enum HostKeyProbe {
    /// A key trusted: its SHA-256 fingerprint, `SHA256:` then unpadded base64, as `PuTTY`
    /// reads it.
    Trusted(String),
    /// A key never seen, which the user is asked about.
    Unknown {
        /// The host, normalised.
        host: String,
        /// The port.
        port: u16,
        /// The key's SHA-256 fingerprint.
        fingerprint: String,
        /// The key, recorded if the user accepts it.
        key: Arc<PublicKey>,
    },
    /// The key changed, or the server could not be reached: why.
    Failed(UiError),
}

/// What starts `PuTTY`, the host key already trusted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PuttyLaunch {
    /// The `PuTTY` chosen in the settings; empty looks for it on `PATH`.
    pub putty_path: String,
    /// The server.
    pub host: String,
    /// Its port.
    pub port: u16,
    /// The user name, when the profile names one.
    pub username: Option<String>,
    /// The key file, when the profile names one.
    pub key_path: Option<PathBuf>,
    /// Compress the traffic.
    pub compression: bool,
    /// Forward the SSH agent.
    pub forward_agent: bool,
    /// The fingerprint of the host key trusted.
    pub host_key: String,
    /// The X server to count on, when X11 is forwarded.
    pub x11: Option<X11Settings>,
}

impl PuttyLaunch {
    /// The same launch pointed at `local`, a forward to the server through its gateway: the
    /// host key stays the server's own.
    #[must_use]
    pub fn through(self, local: SocketAddr) -> Self {
        Self {
            host: local.ip().to_string(),
            port: local.port(),
            ..self
        }
    }
}

/// How `PuTTY` started: the X server counted on, when X11 forwarding was asked.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PuttyStarted {
    /// The X server, when X11 forwarding was asked.
    pub x11: Option<X11Outcome>,
}

/// A `PuTTY` started, and its end to wait for.
pub struct Running {
    /// How it started.
    pub started: PuttyStarted,
    /// Completes once the program has exited.
    pub exited: Pin<Box<dyn Future<Output = ()> + Send>>,
}

impl std::fmt::Debug for Running {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Running")
            .field("started", &self.started)
            .finish_non_exhaustive()
    }
}

/// Whether `value` would be read as an option, or holds a space or a control character.
fn unsafe_argument(value: &str) -> bool {
    value.starts_with('-') || value.chars().any(|c| c.is_whitespace() || c.is_control())
}

/// Refuses what `PuTTY` cannot be started on for `profile`, before anything is dialled, as
/// the C# checks it: a host or a user name that is not one, a key file named by a relative
/// path.
///
/// # Errors
///
/// The first [`PuttyRefusal`] found.
pub fn check(profile: &SshProfile) -> Result<(), PuttyRefusal> {
    if profile.host.is_empty() || unsafe_argument(&profile.host) {
        return Err(PuttyRefusal::InvalidHost);
    }
    if profile
        .username
        .as_deref()
        .is_some_and(|user| !user.is_empty() && unsafe_argument(user))
    {
        return Err(PuttyRefusal::InvalidUsername);
    }
    if let Some(key) = profile.key_path.as_ref().filter(|key| !key.is_absolute()) {
        return Err(PuttyRefusal::KeyFile(key.clone()));
    }
    Ok(())
}

/// What starts `PuTTY` for `profile` with `settings`, `host_key` the fingerprint trusted.
#[must_use]
pub fn plan(profile: &SshProfile, settings: &Settings, host_key: String) -> PuttyLaunch {
    PuttyLaunch {
        putty_path: settings.putty_path.clone(),
        host: profile.host.clone(),
        port: profile.port,
        username: profile.username.clone().filter(|user| !user.is_empty()),
        key_path: profile.key_path.clone(),
        compression: profile.compression,
        forward_agent: profile.forward_agent,
        host_key,
        x11: profile.x11_forwarding.then(|| X11Settings::of(settings)),
    }
}

/// `PuTTY`'s arguments, as the C# `BuildPuttyStartInfo` lists them: `-ssh`, the key file,
/// `-C`, `-A`, `-X` when `x11`, the port, the host key, then `user@host`. Never a password.
#[must_use]
pub fn arguments(launch: &PuttyLaunch, x11: bool) -> Vec<OsString> {
    let mut arguments: Vec<OsString> = vec![SSH_ARGUMENT.into()];
    if let Some(key) = &launch.key_path {
        arguments.push(KEY_ARGUMENT.into());
        arguments.push(key.into());
    }
    for (on, flag) in [
        (launch.compression, COMPRESSION_ARGUMENT),
        (launch.forward_agent, AGENT_ARGUMENT),
        (x11, X11_ARGUMENT),
    ] {
        if on {
            arguments.push(flag.into());
        }
    }
    arguments.push(PORT_ARGUMENT.into());
    arguments.push(launch.port.to_string().into());
    arguments.push(HOST_KEY_ARGUMENT.into());
    arguments.push(launch.host_key.clone().into());
    arguments.push(match &launch.username {
        Some(user) => format!("{user}@{}", launch.host).into(),
        None => launch.host.clone().into(),
    });
    arguments
}

/// `PuTTY`: the one chosen when it is a file named by an absolute path, else the one in the
/// first absolute folder of `path_var` that has it, as the C# falls back when the chosen one
/// is missing. A relative folder, the current one among them, is never searched.
#[must_use]
pub fn find_putty(
    configured: &str,
    path_var: Option<&OsStr>,
    is_file: impl Fn(&Path) -> bool,
) -> Option<PathBuf> {
    let configured = Path::new(configured.trim());
    if configured.is_absolute() && is_file(configured) {
        return Some(configured.to_owned());
    }
    heimdall_term::local::program::resolve(PUTTY_PROGRAM, path_var, is_file).ok()
}

/// The command starting `program` for `launch`: its arguments, the display given to it
/// alone when `x11`, nothing read from it and nothing written to it.
#[must_use]
pub fn command(program: &Path, launch: &PuttyLaunch, x11: bool) -> Command {
    let mut command = Command::new(program);
    command
        .args(arguments(launch, x11))
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    if let Some(folder) = program.parent() {
        command.current_dir(folder);
    }
    if x11 {
        command.env(x11_server::DISPLAY_VARIABLE, x11_server::DISPLAY);
    }
    command
}

/// Starts `PuTTY` for `launch`: the X server counted on first when X11 is forwarded, then
/// the program started; a thread waits for it to end, which the log says.
///
/// # Errors
///
/// [`PuttyRefusal`] when `PuTTY` was not started.
pub fn launch(launch: &PuttyLaunch) -> Result<PuttyStarted, PuttyRefusal> {
    let (mut command, x11, target) = prepare(launch)?;
    let mut child = command
        .spawn()
        .map_err(|error| PuttyRefusal::NotStarted(error.to_string()))?;
    log::info!("PuTTY started, process {}, for {target}", child.id());
    std::thread::spawn(move || match child.wait() {
        Ok(status) => log::info!("PuTTY for {target} ended: {status}"),
        Err(error) => log::warn!("PuTTY for {target} could not be waited for: {error}"),
    });
    Ok(PuttyStarted { x11 })
}

/// Starts `PuTTY` for `launch` as [`launch`] does, its end handed back to wait for: a task
/// waits on it, nothing polls. Called within a tokio runtime.
///
/// # Errors
///
/// [`PuttyRefusal`] when `PuTTY` was not started.
pub fn start(launch: &PuttyLaunch) -> Result<Running, PuttyRefusal> {
    let (command, x11, target) = prepare(launch)?;
    let mut child = tokio::process::Command::from(command)
        .spawn()
        .map_err(|error| PuttyRefusal::NotStarted(error.to_string()))?;
    log::info!(
        "PuTTY started, process {}, for {target}",
        child.id().unwrap_or_default()
    );
    let exited = Box::pin(async move {
        match child.wait().await {
            Ok(status) => log::info!("PuTTY for {target} ended: {status}"),
            Err(error) => log::warn!("PuTTY for {target} could not be waited for: {error}"),
        }
    });
    Ok(Running {
        started: PuttyStarted { x11 },
        exited,
    })
}

/// What starts `PuTTY` for `launch`: the program found, the key file there, the X server
/// counted on when X11 is forwarded; the command, the X server, and the target it names.
fn prepare(launch: &PuttyLaunch) -> Result<(Command, Option<X11Outcome>, String), PuttyRefusal> {
    let path = std::env::var_os("PATH");
    let program = find_putty(&launch.putty_path, path.as_deref(), Path::is_file)
        .ok_or(PuttyRefusal::NotFound)?;
    if let Some(key) = launch.key_path.as_ref().filter(|key| !key.is_file()) {
        return Err(PuttyRefusal::KeyFile(key.clone()));
    }
    let x11 = launch
        .x11
        .as_ref()
        .map(|settings| x11_server::shared().ensure(settings, x11_server::DISPLAY_PORT));
    let forwarded = x11.as_ref().is_some_and(X11Outcome::available);
    if x11.is_some() && !forwarded {
        log::warn!("X11 forwarding asked but no X server is available: PuTTY starts without it");
    }
    let target = heimdall_core::profile::display_address(&launch.host, launch.port);
    log::info!("launching PuTTY {} for {target}", program.display());
    Ok((command(&program, launch, forwarded), x11, target))
}

/// Probes `profile`'s server for its host key, checked against the application's own
/// `known_hosts`, with `options`; nobody signs in.
pub async fn probe_host_key(profile: SshProfile, options: ConnectOptions) -> HostKeyProbe {
    let probed = trusted_host_key(&profile, &options, &CancellationToken::new()).await;
    host_key_probe(&profile, probed)
}

/// What the probe of `profile`'s server found, said as [`HostKeyProbe`].
pub(crate) fn host_key_probe(
    profile: &SshProfile,
    probed: Result<PublicKey, ConnectError>,
) -> HostKeyProbe {
    let target = heimdall_core::profile::display_address(&profile.host, profile.port);
    match probed {
        Ok(key) => HostKeyProbe::Trusted(fingerprint(&key)),
        Err(ConnectError::UnknownHostKey { host, port, key }) => {
            let fingerprint = fingerprint(&key);
            log::info!("{target} presented an unknown host key {fingerprint}");
            HostKeyProbe::Unknown {
                host,
                port,
                fingerprint,
                key: Arc::from(key),
            }
        }
        Err(error) => {
            log::warn!("the host key of {target} was not probed: {error:?}");
            HostKeyProbe::Failed(UiError::from(error))
        }
    }
}

#[cfg(test)]
mod tests {
    use heimdall_core::post_connect::PostConnect;
    use heimdall_core::profile::{Forwards, ProfileId, SshMode};

    use super::*;

    /// A fingerprint as `PuTTY` reads it.
    const FINGERPRINT: &str = "SHA256:47DEQpj8HBSa+/TImW+5JCeuQeRkm5NMpJWZG3hSuFU";

    fn profile() -> SshProfile {
        SshProfile {
            id: ProfileId::new("web"),
            name: "Web".to_owned(),
            group: None,
            host: "web.lab".to_owned(),
            port: 2222,
            username: Some("ops".to_owned()),
            key_path: None,
            gateway: None,
            local_tunnel_port: None,
            vault_entry: None,
            forwards: Forwards::default(),
            post_connect: PostConnect::default(),
            forward_agent: false,
            compression: false,
            sftp: false,
            legacy_algorithms: false,
            session_logging: None,
            ssh_mode: SshMode::External,
            x11_forwarding: false,
        }
    }

    fn launch_of(profile: &SshProfile) -> PuttyLaunch {
        plan(profile, &Settings::default(), FINGERPRINT.to_owned())
    }

    fn strings(arguments: &[OsString]) -> Vec<String> {
        arguments
            .iter()
            .map(|argument| argument.to_string_lossy().into_owned())
            .collect()
    }

    #[test]
    fn the_least_is_the_protocol_the_port_the_host_key_and_the_target() {
        assert_eq!(
            strings(&arguments(&launch_of(&profile()), false)),
            ["-ssh", "-P", "2222", "-hostkey", FINGERPRINT, "ops@web.lab"]
        );
        let mut anonymous = profile();
        anonymous.username = Some(String::new());
        assert_eq!(
            strings(&arguments(&launch_of(&anonymous), false)).last(),
            Some(&"web.lab".to_owned()),
            "the host alone without a user"
        );
    }

    #[test]
    fn every_option_is_its_own_argument_in_the_csharp_order() {
        let key = if cfg!(windows) {
            r"C:\keys\my key.ppk"
        } else {
            "/keys/my key.ppk"
        };
        let mut full = profile();
        full.key_path = Some(PathBuf::from(key));
        full.compression = true;
        full.forward_agent = true;
        full.x11_forwarding = true;
        assert_eq!(
            strings(&arguments(&launch_of(&full), true)),
            [
                "-ssh",
                "-i",
                key,
                "-C",
                "-A",
                "-X",
                "-P",
                "2222",
                "-hostkey",
                FINGERPRINT,
                "ops@web.lab"
            ]
        );
        // X11 asked, no X server: no -X.
        assert!(!strings(&arguments(&launch_of(&full), false)).contains(&"-X".to_owned()));
    }

    #[test]
    fn each_flag_alone_adds_only_itself() {
        let base = strings(&arguments(&launch_of(&profile()), false));
        let with = |change: fn(&mut SshProfile), x11: bool| {
            let mut changed = profile();
            change(&mut changed);
            strings(&arguments(&launch_of(&changed), x11))
        };
        for (flag, arguments) in [
            ("-C", with(|p| p.compression = true, false)),
            ("-A", with(|p| p.forward_agent = true, false)),
            ("-X", with(|_| {}, true)),
        ] {
            let mut expected = base.clone();
            expected.insert(1, flag.to_owned());
            assert_eq!(arguments, expected, "{flag}");
        }
    }

    #[test]
    fn no_password_is_ever_given() {
        let mut full = profile();
        full.key_path = Some(PathBuf::from("/k"));
        full.compression = true;
        full.forward_agent = true;
        for x11 in [false, true] {
            let arguments = strings(&arguments(&launch_of(&full), x11));
            assert!(
                !arguments
                    .iter()
                    .any(|argument| argument == "-pw" || argument == "-pwfile"),
                "{arguments:?}"
            );
        }
    }

    #[test]
    fn the_host_key_is_the_unpadded_sha256_fingerprint_putty_reads() {
        let arguments = strings(&arguments(&launch_of(&profile()), false));
        let at = arguments
            .iter()
            .position(|argument| argument == "-hostkey")
            .expect("-hostkey");
        let given = &arguments[at + 1];
        let encoded = given.strip_prefix("SHA256:").expect("SHA256: first");
        assert!(!encoded.ends_with('='), "unpadded: {given}");
        assert!(at + 2 < arguments.len(), "before the target");
    }

    #[test]
    fn a_host_or_user_read_as_an_option_or_holding_a_space_is_refused() {
        for host in ["-oProxyCommand=calc", "web lab", "web\tlab", "web\nlab", ""] {
            let mut bad = profile();
            bad.host = host.to_owned();
            assert_eq!(check(&bad), Err(PuttyRefusal::InvalidHost), "{host:?}");
        }
        for user in ["-l", "o ps", "ops\r", "\u{1b}[0m"] {
            let mut bad = profile();
            bad.username = Some(user.to_owned());
            assert_eq!(check(&bad), Err(PuttyRefusal::InvalidUsername), "{user:?}");
        }
        let mut fine = profile();
        fine.username = Some(r"LAB\ops-1".to_owned());
        fine.host = "fe80::1".to_owned();
        assert_eq!(check(&fine), Ok(()));
        fine.username = None;
        assert_eq!(check(&fine), Ok(()));
    }

    #[test]
    fn behind_a_gateway_putty_goes_to_the_forward_with_the_server_s_own_key() {
        let mut routed = profile();
        routed.gateway = Some(ProfileId::new("bastion"));
        assert_eq!(check(&routed), Ok(()), "a gateway is gone through");
        let local = SocketAddr::from(([127, 0, 0, 1], 50123));
        let launch = launch_of(&routed).through(local);
        assert_eq!(
            strings(&arguments(&launch, false)),
            [
                "-ssh",
                "-P",
                "50123",
                "-hostkey",
                FINGERPRINT,
                "ops@127.0.0.1"
            ]
        );
        let mut anonymous = routed;
        anonymous.username = None;
        assert_eq!(
            strings(&arguments(&launch_of(&anonymous).through(local), false)).last(),
            Some(&"127.0.0.1".to_owned())
        );
    }

    #[test]
    fn a_profile_with_a_relative_key_is_refused() {
        let mut relative = profile();
        relative.key_path = Some(PathBuf::from("keys/id"));
        assert_eq!(
            check(&relative),
            Err(PuttyRefusal::KeyFile(PathBuf::from("keys/id")))
        );
    }

    #[test]
    fn x11_brings_the_x_server_settings_along() {
        let settings = Settings {
            x11_server_path: "/opt/x/vcxsrv".to_owned(),
            x11_auto_start: false,
            putty_path: "/opt/putty".to_owned(),
            ..Settings::default()
        };
        let mut forwarding = profile();
        assert_eq!(plan(&forwarding, &settings, String::new()).x11, None);
        forwarding.x11_forwarding = true;
        let launch = plan(&forwarding, &settings, String::new());
        assert_eq!(
            launch.x11,
            Some(X11Settings {
                server_path: "/opt/x/vcxsrv".to_owned(),
                auto_start: false,
            })
        );
        assert_eq!(launch.putty_path, "/opt/putty");
    }

    #[test]
    fn the_display_is_given_to_putty_alone_and_only_with_x11() {
        let program = Path::new("/opt/putty/putty");
        let launch = launch_of(&profile());
        let display = |command: &Command| {
            command
                .get_envs()
                .find(|(name, _)| *name == OsStr::new(x11_server::DISPLAY_VARIABLE))
                .and_then(|(_, value)| value.map(OsStr::to_owned))
        };
        assert_eq!(display(&command(program, &launch, false)), None);
        assert_eq!(
            display(&command(program, &launch, true)),
            Some(OsString::from("localhost:0.0"))
        );
        assert_eq!(command(program, &launch, true).get_program(), program);
    }

    #[test]
    fn putty_chosen_comes_first_then_path_never_a_relative_folder() {
        let dir = tempfile::tempdir().expect("dir");
        let chosen = dir.path().join("tools").join(PUTTY_PROGRAM);
        let on_path = dir.path().join("bin").join(PUTTY_PROGRAM);
        let search = std::env::join_paths([dir.path().join("bin")]).expect("joined");
        let present = |paths: Vec<PathBuf>| move |path: &Path| paths.iter().any(|p| p == path);
        let configured = chosen.to_string_lossy().into_owned();
        assert_eq!(
            find_putty(
                &configured,
                Some(&search),
                present(vec![chosen.clone(), on_path.clone()])
            ),
            Some(chosen.clone())
        );
        // The chosen one missing: PATH, as the C# falls back.
        assert_eq!(
            find_putty(&configured, Some(&search), present(vec![on_path.clone()])),
            Some(on_path.clone())
        );
        assert_eq!(
            find_putty("", Some(&search), present(vec![on_path.clone()])),
            Some(on_path)
        );
        let relative = std::env::join_paths([Path::new("bin")]).expect("joined");
        let in_relative = Path::new("bin").join(PUTTY_PROGRAM);
        assert_eq!(
            find_putty("", Some(&relative), present(vec![in_relative])),
            None
        );
        assert_eq!(
            find_putty(
                PUTTY_PROGRAM,
                None,
                present(vec![PathBuf::from(PUTTY_PROGRAM)])
            ),
            None,
            "a relative path chosen is not taken"
        );
    }
}
