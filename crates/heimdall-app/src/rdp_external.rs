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

//! An RDP profile opened in Remote Desktop Connection (`mstsc.exe`), as the C# `RdpHandler`
//! external mode: its options written to an `.rdp` file as the C# `RdpFileGenerator` writes
//! them, and the file handed to `mstsc.exe`. No password is staged: Remote Desktop
//! Connection asks for it, the user name already filled in. The file is removed a few
//! seconds after the client started, which reads it at once; one a run left behind is
//! swept at the next start.
//!
//! Behind an SSH gateway, the file names the loopback forward the gateway carries to the
//! server, as the C# names its tunnel's local end, and the client is waited for: the
//! forward lives as long as it runs.

use std::fmt::Write as _;
use std::future::Future;
use std::io;
use std::net::{Ipv6Addr, SocketAddr};
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::time::{Duration, SystemTime};

use heimdall_core::profile::{
    AudioPlayback, DEFAULT_FIXED_SIZE, Experience, RdpProfile, Resolution, fixed_desktop,
};

use crate::error::UiError;
use crate::external_edit::{private_base, write_new};

/// The folder of the connection files, in the system's temporary folder.
pub const ARTIFACT_FOLDER: &str = "heimdall-rdp";
/// The extension of a connection file.
pub const ARTIFACT_EXTENSION: &str = "rdp";
/// The extension of a connection file still being written, renamed once whole.
const STAGING_EXTENSION: &str = "tmp";
/// How long a connection file is kept once the client started, as the C#
/// `RdpArtifactCleanupDelayMs`.
pub const REMOVE_AFTER: Duration = Duration::from_secs(10);
/// A connection file older than this when the application starts was left by a run that
/// ended before removing it.
pub const STALE_AFTER: Duration = Duration::from_secs(60);

/// Remote Desktop Connection's program, in the system folder.
const MSTSC: &str = "mstsc.exe";

/// `screen mode id` of a window.
const SCREEN_WINDOWED: u8 = 1;
/// `screen mode id` of the full screen.
const SCREEN_FULL: u8 = 2;
/// `authentication level`: connect without checking the server.
const AUTHENTICATION_NONE: u8 = 0;
/// `authentication level`: refuse a server whose identity cannot be checked.
const AUTHENTICATION_REQUIRED: u8 = 1;
/// `authentication level`: warn about a server whose identity cannot be checked.
const AUTHENTICATION_WARN: u8 = 2;
/// `connection type` declared when UDP is not probed: LAN.
const CONNECTION_TYPE_LAN: u8 = 6;
/// `connection type` that lets the client detect the network.
const CONNECTION_TYPE_DETECT: u8 = 7;

/// The visual experience keys, each the box it reads, in the C# order.
const EXPERIENCE_KEYS: [(&str, Experience); 7] = [
    ("disable wallpaper:i", Experience::DisableWallpaper),
    ("disable full window drag:i", Experience::DisableDrag),
    ("disable menu anims:i", Experience::DisableAnimations),
    ("disable themes:i", Experience::DisableThemes),
    ("disable cursor setting:i", Experience::DisableCursorShadow),
    ("allow font smoothing:i", Experience::EnableFontSmoothing),
    ("allow desktop composition:i", Experience::EnableComposition),
];

/// Why a profile was not opened in Remote Desktop Connection.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExternalRefusal {
    /// Remote Desktop Connection is Windows' own.
    NotWindows,
    /// The SSH gateway the profile goes through was not reached, or refused the user: why.
    Gateway(UiError),
    /// No port of this computer's loopback address could be opened for the gateway's
    /// forward: why.
    Forward(String),
    /// `mstsc.exe` is not on this computer.
    NotFound,
    /// The connection file could not be written, and why.
    NotWritten(String),
    /// `mstsc.exe` did not start, and why.
    NotStarted(String),
}

/// How the desktop is shown, as the C# `RdpProfileResolver.ResolveResolution` gives it to
/// the external client.
struct Display {
    size: (u16, u16),
    smart_sizing: bool,
    multi_monitor: bool,
    screen_mode: u8,
    /// `use multimon:i:0` written even without several monitors, as the C# automatic mode.
    single_monitor_said: bool,
}

/// How `profile`'s desktop is shown. The automatic mode takes the default size, the screen's
/// working area being unknown here; the monitors are spanned in the multi-monitor mode, or
/// when the profile asks for it outside the automatic mode, as the C# reads
/// `RdpMultiMonitor` for a profile saved before its resolution modes.
fn display(profile: &RdpProfile) -> Display {
    let extras = &profile.extras;
    let screen_mode = |smart_sizing: bool| {
        if extras.full_screen || smart_sizing {
            SCREEN_FULL
        } else {
            SCREEN_WINDOWED
        }
    };
    let shown = |size, smart_sizing, multi_monitor| Display {
        size,
        smart_sizing,
        multi_monitor,
        screen_mode: screen_mode(smart_sizing),
        single_monitor_said: false,
    };
    let options = &profile.options;
    match options.resolution {
        Resolution::Auto => Display {
            size: DEFAULT_FIXED_SIZE,
            smart_sizing: true,
            multi_monitor: false,
            screen_mode: SCREEN_WINDOWED,
            single_monitor_said: true,
        },
        Resolution::Fixed => shown(
            fixed_desktop(options.fixed_width, options.fixed_height),
            false,
            extras.multi_monitor,
        ),
        Resolution::FitWindow | Resolution::SmartSizing => {
            shown(DEFAULT_FIXED_SIZE, true, extras.multi_monitor)
        }
        Resolution::MultiMonitor => shown(DEFAULT_FIXED_SIZE, false, true),
    }
}

/// `value` without what would end its line or the file, as the C# `SanitizeValue`.
fn clean(value: &str) -> String {
    value
        .chars()
        .filter(|c| !matches!(c, '\r' | '\n' | '\0'))
        .collect()
}

/// `value` when it holds more than white space, as the C# `IsNullOrWhiteSpace` reads it.
fn present(value: Option<&str>) -> Option<&str> {
    value.filter(|value| !value.trim().is_empty())
}

/// One line of the file: `key` with its type, then `value`.
fn line(file: &mut String, key: &str, value: impl std::fmt::Display) {
    let _ = write!(file, "{key}:{value}\r\n");
}

/// The address `mstsc` is given: an IPv6 address in brackets, then the port.
fn full_address(host: &str, port: u16) -> String {
    if !host.starts_with('[') && host.parse::<Ipv6Addr>().is_ok() {
        format!("[{host}]:{port}")
    } else {
        format!("{host}:{port}")
    }
}

/// The `.rdp` file `profile` opens with in Remote Desktop Connection, as the C#
/// `RdpFileGenerator` writes it for the external client, its lines ended with CR LF: the
/// address, the user name and domain, the display, the redirections, the authentication,
/// the performance options and the RD Gateway. Never a password.
#[must_use]
pub fn rdp_file(profile: &RdpProfile) -> String {
    rdp_file_to(profile, &profile.host, profile.port)
}

/// The `.rdp` file `profile` opens with when its server is reached through the forward
/// listening at `forward`, as the C# writes its tunnel's local end for the address; the RD
/// Gateway, when the profile names one, written as [`rdp_file`] writes it, as the C# does.
#[must_use]
pub fn rdp_file_through(profile: &RdpProfile, forward: SocketAddr) -> String {
    rdp_file_to(profile, &forward.ip().to_string(), forward.port())
}

/// The `.rdp` file `profile` opens with, its address `host:port`.
fn rdp_file_to(profile: &RdpProfile, host: &str, port: u16) -> String {
    let display = display(profile);
    let options = &profile.options;
    let extras = &profile.extras;
    let mut file = String::new();
    line(
        &mut file,
        "full address:s",
        clean(&full_address(host, port)),
    );
    if let Some(username) = present(profile.username.as_deref()) {
        line(&mut file, "username:s", clean(username));
    }
    if let Some(domain) = present(profile.domain.as_deref()) {
        line(&mut file, "domain:s", clean(domain));
    }
    line(&mut file, "desktopwidth:i", display.size.0);
    line(&mut file, "desktopheight:i", display.size.1);
    line(&mut file, "screen mode id:i", display.screen_mode);
    line(&mut file, "session bpp:i", options.color_depth.bits());
    if options.admin_session {
        line(&mut file, "administrative session:i", 1);
    }
    redirections(&mut file, profile);
    let nla = !profile.allow_tls_only;
    let authentication = match (nla, extras.strict_server_authentication) {
        (false, _) => AUTHENTICATION_NONE,
        (true, true) => AUTHENTICATION_REQUIRED,
        (true, false) => AUTHENTICATION_WARN,
    };
    line(&mut file, "authentication level:i", authentication);
    line(&mut file, "enablecredsspsupport:i", u8::from(nla));
    line(
        &mut file,
        "bitmapcachepersistenable:i",
        u8::from(extras.bitmap_caching),
    );
    line(&mut file, "compression:i", u8::from(extras.compression));
    line(
        &mut file,
        "autoreconnection enabled:i",
        u8::from(profile.auto_reconnect),
    );
    monitors(&mut file, profile, &display);
    if display.smart_sizing || options.dynamic_resolution {
        line(&mut file, "smart sizing:i", 1);
    }
    if options.dynamic_resolution {
        line(&mut file, "dynamic resolution:i", 1);
    }
    if extras.redirect_usb {
        line(&mut file, "usbdevicestoredirect:s", "*");
    }
    if extras.redirect_webcam {
        line(&mut file, "camerastoredirect:s", "*");
    }
    for (key, experience) in EXPERIENCE_KEYS {
        line(&mut file, key, u8::from(options.has(experience)));
    }
    network(&mut file, extras.disable_udp);
    gateway(&mut file, extras.rd_gateway());
    file
}

/// The redirections and the sound. Drives use `redirectdrives`, the key mstsc reads and
/// writes back, never `drivestoredirect`, which would win over a profile that said no.
fn redirections(file: &mut String, profile: &RdpProfile) {
    let extras = &profile.extras;
    line(
        file,
        "redirectclipboard:i",
        u8::from(profile.redirect_clipboard),
    );
    line(file, "redirectdrives:i", u8::from(profile.redirect_drives));
    line(
        file,
        "redirectprinters:i",
        u8::from(extras.redirect_printers),
    );
    line(
        file,
        "redirectcomports:i",
        u8::from(extras.redirect_com_ports),
    );
    line(
        file,
        "redirectsmartcards:i",
        u8::from(extras.redirect_smart_cards),
    );
    // As the C# `MapAudioModeToRdpValue`: 0 plays here, 1 on the server, 2 not at all.
    let audio: u8 = match profile.options.audio {
        AudioPlayback::Local => 0,
        AudioPlayback::OnServer => 1,
        AudioPlayback::Off => 2,
    };
    line(file, "audiomode:i", audio);
    line(file, "audiocapturemode:i", u8::from(extras.microphone));
}

/// The monitors spanned, the ones chosen when the profile names them.
fn monitors(file: &mut String, profile: &RdpProfile, display: &Display) {
    if display.multi_monitor {
        line(file, "use multimon:i", 1);
        let mut chosen: Vec<u32> = Vec::new();
        for index in &profile.extras.monitors {
            if !chosen.contains(index) {
                chosen.push(*index);
            }
        }
        if !chosen.is_empty() {
            let list: Vec<String> = chosen.iter().map(u32::to_string).collect();
            line(file, "selectedmonitors:s", list.join(","));
        }
    } else if display.single_monitor_said {
        line(file, "use multimon:i", 0);
    }
}

/// The network detection: off, with a declared connection type, when UDP is not to be
/// probed, as the C# writes it; this does not force TCP, no client setting can.
fn network(file: &mut String, disable_udp: bool) {
    let detect = u8::from(!disable_udp);
    line(file, "networkautodetect:i", detect);
    line(file, "bandwidthautodetect:i", detect);
    line(
        file,
        "connection type:i",
        if disable_udp {
            CONNECTION_TYPE_LAN
        } else {
            CONNECTION_TYPE_DETECT
        },
    );
}

/// The RD Gateway, the client asked to use it with the credentials it is given.
fn gateway(file: &mut String, gateway: Option<&str>) {
    match gateway {
        Some(host) => {
            line(file, "gatewayusagemethod:i", 1);
            line(file, "gatewayprofileusagemethod:i", 1);
            line(file, "gatewayhostname:s", clean(host));
            line(file, "gatewaycredentialssource:i", 0);
        }
        None => line(file, "gatewayusagemethod:i", 0),
    }
}

/// The folder of the connection files: the application's own, in the system's temporary
/// folder.
#[must_use]
pub fn artifact_folder() -> PathBuf {
    std::env::temp_dir().join(ARTIFACT_FOLDER)
}

/// Writes `content` as a new connection file in `folder`, made or found the user's own: to
/// a file of its own first, renamed once whole, so that the client never reads half of it.
/// Named after a random token, never after the profile.
///
/// # Errors
///
/// What the file system said, or that no random token could be drawn.
pub fn write_artifact(folder: &Path, content: &str) -> io::Result<PathBuf> {
    std::fs::create_dir_all(folder)?;
    private_base(folder)?;
    let token = heimdall_files::server_copy::random_token()
        .ok_or_else(|| io::Error::other("no random source"))?;
    let name = token.iter().fold(String::new(), |mut name, byte| {
        let _ = write!(name, "{byte:02x}");
        name
    });
    let staging = folder.join(&name).with_extension(STAGING_EXTENSION);
    let file = folder.join(name).with_extension(ARTIFACT_EXTENSION);
    let written =
        write_new(&staging, content.as_bytes()).and_then(|()| std::fs::rename(&staging, &file));
    if let Err(error) = written {
        let _ = std::fs::remove_file(&staging);
        return Err(error);
    }
    Ok(file)
}

/// Removes the connection files in `folder`, that folder only, last changed more than
/// `older_than` before `now`: those a run left behind. How many were removed.
#[must_use]
pub fn sweep(folder: &Path, now: SystemTime, older_than: Duration) -> usize {
    let Ok(entries) = std::fs::read_dir(folder) else {
        return 0;
    };
    let mut removed = 0;
    for entry in entries.flatten() {
        let path = entry.path();
        let ours = path
            .extension()
            .and_then(|extension| extension.to_str())
            .is_some_and(|extension| {
                extension == ARTIFACT_EXTENSION || extension == STAGING_EXTENSION
            });
        let Ok(found) = entry.metadata() else {
            continue;
        };
        let old = found.modified().is_ok_and(|modified| {
            now.duration_since(modified)
                .is_ok_and(|age| age > older_than)
        });
        if ours && found.is_file() && old && std::fs::remove_file(&path).is_ok() {
            removed += 1;
        }
    }
    removed
}

/// Sweeps the connection files earlier runs left in the application's folder.
pub fn sweep_stale() {
    let removed = sweep(&artifact_folder(), SystemTime::now(), STALE_AFTER);
    if removed > 0 {
        log::info!("removed {removed} connection file(s) an earlier run left");
    }
}

/// `mstsc.exe` in the system folder Windows says, never the one the environment names,
/// named by its whole path, never looked for in the current folder, as the C#
/// `MstscRdpExternalClientLauncher`. `None` off Windows.
fn mstsc() -> Option<PathBuf> {
    heimdall_core::paths::system_program(MSTSC).filter(|program| program.is_file())
}

/// Remote Desktop Connection started, and its end to wait for.
pub struct Running {
    /// Completes once the program has exited.
    pub exited: Pin<Box<dyn Future<Output = ()> + Send>>,
}

impl std::fmt::Debug for Running {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Running").finish_non_exhaustive()
    }
}

/// What starts Remote Desktop Connection on connection file `content`: the file written,
/// then `mstsc.exe` with its path as its only argument, no shell between them; the command
/// and the file.
fn prepare(content: &str) -> Result<(std::process::Command, PathBuf), ExternalRefusal> {
    if !cfg!(windows) {
        return Err(ExternalRefusal::NotWindows);
    }
    let program = mstsc().ok_or(ExternalRefusal::NotFound)?;
    let file = write_artifact(&artifact_folder(), content)
        .map_err(|error| ExternalRefusal::NotWritten(error.to_string()))?;
    let mut command = std::process::Command::new(&program);
    if let Some(system) = program.parent() {
        command.current_dir(system);
    }
    command
        .arg(&file)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());
    Ok((command, file))
}

/// What became of starting `mstsc.exe` on `file`: the file removed [`REMOVE_AFTER`] later,
/// on a thread of its own, once started; at once otherwise.
fn started<T>(started: io::Result<T>, file: PathBuf) -> Result<T, ExternalRefusal> {
    match started {
        Ok(running) => {
            std::thread::spawn(move || {
                std::thread::sleep(REMOVE_AFTER);
                let _ = std::fs::remove_file(&file);
            });
            Ok(running)
        }
        Err(error) => {
            let _ = std::fs::remove_file(&file);
            Err(match error.kind() {
                io::ErrorKind::NotFound => ExternalRefusal::NotFound,
                _ => ExternalRefusal::NotStarted(error.to_string()),
            })
        }
    }
}

/// Opens connection file `content` in Remote Desktop Connection: the file written, then
/// `mstsc.exe` started with its path as its only argument, no shell between them; the file
/// removed [`REMOVE_AFTER`] later, on a thread of its own.
///
/// # Errors
///
/// [`ExternalRefusal`] when the client was not started; the file is then removed at once.
pub fn launch(content: &str) -> Result<(), ExternalRefusal> {
    let (mut command, file) = prepare(content)?;
    started(command.spawn().map(drop), file)
}

/// Opens connection file `content` in Remote Desktop Connection as [`launch`] does, its end
/// to wait for: what a forward through an SSH gateway lives as long as, as the C# tunnel
/// is released when `mstsc.exe` exits. Called within the runtime.
///
/// # Errors
///
/// [`ExternalRefusal`] when the client was not started; the file is then removed at once.
pub fn start(content: &str) -> Result<Running, ExternalRefusal> {
    let (command, file) = prepare(content)?;
    let mut child = started(tokio::process::Command::from(command).spawn(), file)?;
    log::info!(
        "mstsc.exe started, process {}",
        child.id().unwrap_or_default()
    );
    Ok(Running {
        exited: Box::pin(async move {
            match child.wait().await {
                Ok(status) => log::info!("mstsc.exe ended: {status}"),
                Err(error) => log::warn!("mstsc.exe could not be waited for: {error}"),
            }
        }),
    })
}

#[cfg(test)]
mod tests {
    use heimdall_core::profile::{ColorDepth, Forwards, ProfileId, RdpExtras, RdpOptions};

    use super::*;

    fn profile() -> RdpProfile {
        RdpProfile {
            id: ProfileId::new("rds"),
            name: "Session host".to_owned(),
            group: None,
            host: "rds.lab".to_owned(),
            port: 3390,
            username: Some("admin".to_owned()),
            domain: Some("LAB".to_owned()),
            allow_tls_only: false,
            gateway: None,
            local_tunnel_port: None,
            redirect_clipboard: true,
            redirect_drives: false,
            vault_entry: None,
            forwards: Forwards::default(),
            options: RdpOptions::default(),
            follow_defaults: false,
            several_servers: false,
            anti_idle: false,
            auto_reconnect: true,
            extras: RdpExtras::default(),
        }
    }

    fn lines(file: &str) -> Vec<&str> {
        assert!(file.ends_with("\r\n"), "every line ended with CR LF");
        file.split_terminator("\r\n").collect()
    }

    #[test]
    fn a_profile_behind_a_gateway_spanning_monitors_is_written_as_the_csharp_writes_it() {
        let mut profile = profile();
        profile.redirect_drives = true;
        profile.options.color_depth = ColorDepth::Bpp24;
        profile.options.audio = AudioPlayback::Local;
        profile.options.admin_session = true;
        profile.options.resolution = Resolution::MultiMonitor;
        profile.options.dynamic_resolution = false;
        profile.options.set(Experience::DisableWallpaper, true);
        profile.options.set(Experience::EnableFontSmoothing, true);
        profile.extras = RdpExtras {
            external: true,
            rd_gateway: Some(" rdg.lab ".to_owned()),
            redirect_printers: true,
            redirect_com_ports: false,
            redirect_smart_cards: true,
            redirect_webcam: true,
            redirect_usb: true,
            microphone: true,
            multi_monitor: false,
            monitors: vec![0, 2, 0],
            strict_server_authentication: true,
            disable_udp: true,
            bitmap_caching: true,
            compression: false,
            hardware_acceleration: true,
            full_screen: true,
        };
        assert_eq!(
            lines(&rdp_file(&profile)),
            [
                "full address:s:rds.lab:3390",
                "username:s:admin",
                "domain:s:LAB",
                "desktopwidth:i:1920",
                "desktopheight:i:1080",
                "screen mode id:i:2",
                "session bpp:i:24",
                "administrative session:i:1",
                "redirectclipboard:i:1",
                "redirectdrives:i:1",
                "redirectprinters:i:1",
                "redirectcomports:i:0",
                "redirectsmartcards:i:1",
                "audiomode:i:0",
                "audiocapturemode:i:1",
                "authentication level:i:1",
                "enablecredsspsupport:i:1",
                "bitmapcachepersistenable:i:1",
                "compression:i:0",
                "autoreconnection enabled:i:1",
                "use multimon:i:1",
                "selectedmonitors:s:0,2",
                "usbdevicestoredirect:s:*",
                "camerastoredirect:s:*",
                "disable wallpaper:i:1",
                "disable full window drag:i:0",
                "disable menu anims:i:0",
                "disable themes:i:0",
                "disable cursor setting:i:0",
                "allow font smoothing:i:1",
                "allow desktop composition:i:0",
                "networkautodetect:i:0",
                "bandwidthautodetect:i:0",
                "connection type:i:6",
                "gatewayusagemethod:i:1",
                "gatewayprofileusagemethod:i:1",
                "gatewayhostname:s:rdg.lab",
                "gatewaycredentialssource:i:0",
            ]
        );
    }

    #[test]
    fn a_new_profile_opens_windowed_at_the_default_size_on_one_monitor() {
        let mut profile = profile();
        profile.username = None;
        profile.domain = Some("  ".to_owned());
        profile.options.resolution = Resolution::Auto;
        profile.extras.multi_monitor = true;
        let file = rdp_file(&profile);
        let lines = lines(&file);
        for expected in [
            "full address:s:rds.lab:3390",
            "desktopwidth:i:1920",
            "desktopheight:i:1080",
            "screen mode id:i:1",
            "session bpp:i:32",
            "audiomode:i:2",
            "authentication level:i:2",
            "use multimon:i:0",
            "smart sizing:i:1",
            "dynamic resolution:i:1",
            "networkautodetect:i:1",
            "connection type:i:7",
            "gatewayusagemethod:i:0",
        ] {
            assert!(lines.contains(&expected), "{expected} in {lines:?}");
        }
        assert!(
            !lines
                .iter()
                .any(|line| line.starts_with("username") || line.starts_with("domain")),
            "no account named: {lines:?}"
        );
        assert!(!lines.iter().any(|line| line.starts_with("administrative")));
    }

    #[test]
    fn no_password_is_ever_written_nor_a_line_slipped_in_through_a_value() {
        let mut profile = profile();
        profile.allow_tls_only = true;
        profile.username = Some("ops\r\npassword 51:b:0100".to_owned());
        profile.domain = Some("LAB\n\0".to_owned());
        profile.extras.rd_gateway = Some("rdg.lab\r\ngatewaycredentialssource:i:4".to_owned());
        let file = rdp_file(&profile);
        let lines = lines(&file);
        assert!(
            !lines.iter().any(|line| line.starts_with("password")),
            "{lines:?}"
        );
        assert!(lines.contains(&"username:s:opspassword 51:b:0100"));
        assert!(lines.contains(&"domain:s:LAB"));
        assert!(lines.contains(&"gatewayhostname:s:rdg.labgatewaycredentialssource:i:4"));
        assert!(lines.contains(&"authentication level:i:0"), "NLA off");
        assert!(lines.contains(&"enablecredsspsupport:i:0"));
    }

    #[test]
    fn an_ipv6_address_is_written_in_brackets() {
        let mut profile = profile();
        profile.host = "fe80::1".to_owned();
        assert!(rdp_file(&profile).starts_with("full address:s:[fe80::1]:3390\r\n"));
        profile.host = "[fe80::1]".to_owned();
        assert!(rdp_file(&profile).starts_with("full address:s:[fe80::1]:3390\r\n"));
    }

    #[test]
    fn a_fixed_desktop_keeps_its_size_within_the_limits_unscaled() {
        let mut profile = profile();
        profile.options.resolution = Resolution::Fixed;
        profile.options.fixed_width = 1366;
        profile.options.fixed_height = 768;
        profile.options.dynamic_resolution = false;
        let file = rdp_file(&profile);
        let lines = lines(&file);
        assert!(lines.contains(&"desktopwidth:i:1364"), "a multiple of 4");
        assert!(lines.contains(&"desktopheight:i:768"));
        assert!(lines.contains(&"screen mode id:i:1"));
        assert!(!lines.iter().any(|line| line.starts_with("smart sizing")));
        assert!(!lines.iter().any(|line| line.starts_with("use multimon")));
    }

    #[test]
    fn a_connection_file_is_written_whole_under_a_random_name() {
        let dir = tempfile::tempdir().expect("dir");
        let folder = dir.path().join(ARTIFACT_FOLDER);
        let first = write_artifact(&folder, "full address:s:a\r\n").expect("written");
        let second = write_artifact(&folder, "full address:s:b\r\n").expect("written");
        assert_ne!(first, second);
        assert_eq!(
            std::fs::read_to_string(&first).expect("read"),
            "full address:s:a\r\n"
        );
        assert_eq!(
            first.extension().and_then(|extension| extension.to_str()),
            Some(ARTIFACT_EXTENSION)
        );
        let names: Vec<_> = std::fs::read_dir(&folder)
            .expect("folder")
            .flatten()
            .map(|entry| entry.file_name())
            .collect();
        assert_eq!(names.len(), 2, "nothing staged left: {names:?}");
    }

    #[test]
    fn the_sweep_removes_only_old_connection_files_of_its_folder() {
        let dir = tempfile::tempdir().expect("dir");
        let folder = dir.path().join(ARTIFACT_FOLDER);
        std::fs::create_dir_all(folder.join("sub")).expect("folders");
        let now = SystemTime::now();
        let hour_ago = now - Duration::from_secs(3600);
        let make = |path: &Path, modified: SystemTime| {
            std::fs::write(path, "x").expect("write");
            std::fs::File::options()
                .write(true)
                .open(path)
                .and_then(|file| file.set_modified(modified))
                .expect("dated");
        };
        let old = folder.join("old.rdp");
        let staged = folder.join("old.tmp");
        let fresh = folder.join("fresh.rdp");
        let other = folder.join("notes.txt");
        let nested = folder.join("sub").join("old.rdp");
        let outside = dir.path().join("old.rdp");
        for path in [&old, &staged, &other, &nested, &outside] {
            make(path, hour_ago);
        }
        make(&fresh, now);
        assert_eq!(sweep(&folder, now, STALE_AFTER), 2);
        assert!(!old.exists() && !staged.exists());
        for kept in [&fresh, &other, &nested, &outside] {
            assert!(kept.exists(), "{} kept", kept.display());
        }
        assert!(folder.join("sub").is_dir());
        assert_eq!(sweep(&dir.path().join("none"), now, STALE_AFTER), 0);
    }

    #[test]
    fn remote_desktop_connection_is_refused_outside_windows() {
        if !cfg!(windows) {
            assert_eq!(
                launch("full address:s:a\r\n"),
                Err(ExternalRefusal::NotWindows)
            );
            assert!(matches!(
                start("full address:s:a\r\n"),
                Err(ExternalRefusal::NotWindows)
            ));
        }
    }

    #[test]
    fn through_a_forward_the_file_names_the_forward_and_keeps_the_rest() {
        let mut profile = profile();
        profile.extras.rd_gateway = Some("rdg.lab".to_owned());
        let forward = SocketAddr::from(([127, 0, 0, 1], 50123));
        let through = rdp_file_through(&profile, forward);
        assert!(
            through.starts_with("full address:s:127.0.0.1:50123\r\n"),
            "{through}"
        );
        assert!(!through.contains("rds.lab"), "{through}");
        // Every other line as the file the server's own address opens with.
        assert_eq!(lines(&through)[1..], lines(&rdp_file(&profile))[1..]);
        assert!(through.contains("\r\ngatewayhostname:s:rdg.lab\r\n"));
    }
}
