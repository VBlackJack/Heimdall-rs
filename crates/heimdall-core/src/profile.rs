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

//! Server profiles.

use std::fmt;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::post_connect::PostConnect;

/// Port an SSH server listens on unless a profile says otherwise.
pub const DEFAULT_SSH_PORT: u16 = 22;

/// Port an RDP server listens on unless a profile says otherwise.
pub const DEFAULT_RDP_PORT: u16 = 3389;

/// Port a Telnet server listens on unless a profile says otherwise.
pub const DEFAULT_TELNET_PORT: u16 = 23;

/// Port of `WinRM` over HTTP unless a profile says otherwise.
pub const DEFAULT_WINRM_HTTP_PORT: u16 = 5985;

/// Port of `WinRM` over HTTPS unless a profile says otherwise.
pub const DEFAULT_WINRM_HTTPS_PORT: u16 = 5986;

/// Port a VNC server listens on unless a profile says otherwise: display 0.
pub const DEFAULT_VNC_PORT: u16 = 5900;

/// Stable identifier of a profile.
///
/// A profile imported from the C# Heimdall keeps the identifier it had there, so a second
/// import updates it instead of adding a copy.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ProfileId(String);

impl ProfileId {
    /// Wraps an identifier.
    #[must_use]
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }

    /// The identifier as text.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for ProfileId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// A saved SSH destination.
///
/// Holds no secret. A password or a key passphrase is asked for when connecting.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[expect(
    clippy::struct_excessive_bools,
    reason = "one switch per SSH option, each saved on its own"
)]
pub struct SshProfile {
    /// Stable identifier.
    pub id: ProfileId,
    /// Name shown to the user.
    pub name: String,
    /// Folder path, `/`-separated, when the profile is filed in one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub group: Option<String>,
    /// Host name or address.
    pub host: String,
    /// TCP port.
    pub port: u16,
    /// Login name; asked for when connecting if absent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub username: Option<String>,
    /// Private key file, OpenSSH or `PuTTY` format.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub key_path: Option<PathBuf>,
    /// The SSH gateway the server is reached through, if any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gateway: Option<ProfileId>,
    /// The profile's entry in the external password manager, for `{Title}`; `None` uses
    /// its name, as the C# `VaultEntryName`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub vault_entry: Option<String>,
    /// Ports this computer opens through the gateway while the session runs.
    #[serde(default, flatten)]
    pub forwards: Forwards,
    /// Commands the shell types by itself once ready.
    #[serde(default, skip_serializing_if = "PostConnect::is_empty")]
    pub post_connect: PostConnect,
    /// Forward this computer's SSH agent to the shell (`ssh -A`), as the C# "Forward SSH
    /// agent": off unless turned on, written down only when on.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub forward_agent: bool,
    /// Compress the traffic (`ssh -C`), as the C# "Enable compression": off unless turned on,
    /// written down only when on.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub compression: bool,
    /// An SFTP profile, as the C# `connectionType` SFTP: it opens its files rather than a
    /// shell. Written down only when it is one.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub sftp: bool,
    /// Also offer the older key exchanges, ciphers, MACs and the SHA-1 `ssh-rsa` host key that
    /// old appliances still speak, after the current ones, as the C# Heimdall always does.
    /// Off unless turned on, written down only when on.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub legacy_algorithms: bool,
    /// Whether its sessions keep a transcript, as the C# `SessionLoggingOverride`: `None`
    /// follows the Settings page's session logging, `Some` decides for this profile alone.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_logging: Option<bool>,
}

/// Ports a session reached through a gateway opens on this computer's loopback address, as
/// the C# Heimdall's gateway profiles do. Each is used only through a gateway.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Forwards {
    /// A SOCKS5 proxy whose connections leave from the gateway (`ssh -D`); `None` for none,
    /// as the C# `SocksProxyPort` 0.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub socks_port: Option<u16>,
    /// A port the gateway listens on, on its own loopback address, whose connections come
    /// back here (`ssh -R`); `None` for none, as the C# `RemoteBindPort` 0.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub remote_bind_port: Option<u16>,
    /// The local port those connections go to; `None` for the same port, as the C#
    /// `RemoteLocalPort` 0.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub remote_local_port: Option<u16>,
}

impl Forwards {
    /// The remote forward asked for: the gateway's port and the local port it goes to.
    #[must_use]
    pub fn remote(self) -> Option<(u16, u16)> {
        self.remote_bind_port
            .map(|port| (port, self.remote_local_port.unwrap_or(port)))
    }
}

/// An SSH gateway: a server that other connections go through, itself possibly reached
/// through another one, its parent.
///
/// Holds no secret, and no host key: its key is checked against `known_hosts` like any
/// server's.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SshGateway {
    /// Stable identifier.
    pub id: ProfileId,
    /// Name shown to the user.
    pub name: String,
    /// Host name or address.
    pub host: String,
    /// TCP port.
    pub port: u16,
    /// Login name; asked for when connecting if absent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub username: Option<String>,
    /// Private key file, OpenSSH or `PuTTY` format.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub key_path: Option<PathBuf>,
    /// The gateway this one is reached through, if any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent: Option<ProfileId>,
}

impl SshGateway {
    /// The gateway as the one hop it is: a server to log in to, reached directly.
    #[must_use]
    pub fn as_hop(&self) -> SshProfile {
        SshProfile {
            id: self.id.clone(),
            name: self.name.clone(),
            group: None,
            host: self.host.clone(),
            port: self.port,
            username: self.username.clone(),
            key_path: self.key_path.clone(),
            gateway: None,
            vault_entry: None,
            forwards: Forwards::default(),
            post_connect: PostConnect::default(),
            forward_agent: false,
            compression: false,
            sftp: false,
            legacy_algorithms: false,
            session_logging: None,
        }
    }
}

/// A saved RDP destination, reached directly.
///
/// Holds no secret: the password is asked for when connecting.
#[expect(
    clippy::struct_excessive_bools,
    reason = "one switch per C# RDP option, each saved on its own"
)]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RdpProfile {
    /// Stable identifier.
    pub id: ProfileId,
    /// Name shown to the user.
    pub name: String,
    /// Folder path, `/`-separated, when the profile is filed in one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub group: Option<String>,
    /// Host name or address.
    pub host: String,
    /// TCP port.
    pub port: u16,
    /// Login name; asked for when connecting if absent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub username: Option<String>,
    /// Windows domain of the account, if any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub domain: Option<String>,
    /// Also accept a server without Network Level Authentication (xrdp): the password then
    /// travels in the logon packet, inside TLS, once the server's key is trusted.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub allow_tls_only: bool,
    /// The SSH gateway the server is reached through, if any: the RDP connection then runs
    /// in a tunnel the gateway opens to the server.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gateway: Option<ProfileId>,
    /// Share the clipboard with the server, text only. On unless turned off, as in the C#
    /// Heimdall and in the Windows client: written down only when off.
    #[serde(default = "shared", skip_serializing_if = "is_shared")]
    pub redirect_clipboard: bool,
    /// Share this computer's drives with the server, as mstsc does. Off unless turned on,
    /// as in the C# Heimdall: written down only when on.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub redirect_drives: bool,
    /// The profile's entry in the external password manager, for `{Title}`; `None` uses
    /// its name, as the C# `VaultEntryName`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub vault_entry: Option<String>,
    /// Ports this computer opens through the gateway while the session runs.
    #[serde(default, flatten)]
    pub forwards: Forwards,
    /// How the session looks, sounds and which session it opens.
    #[serde(flatten)]
    pub options: RdpOptions,
    /// The session takes the application's [`RdpDefaults`] rather than this profile's own
    /// redirections, authentication, colours, sound and resizing, as the C#
    /// `RdpUseGlobalDefaults`. Off for a profile saved before it existed, so none changes by
    /// itself; the profile's own values are kept either way.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub follow_defaults: bool,
    /// Several machines answer at this address (a pool of domain controllers, a farm of
    /// session hosts): a certificate not trusted yet is asked about and trusted beside the
    /// others, as the C# keeps a set per profile. Off, a new certificate on a trusted server
    /// is refused as changed, the alarm of an intercepted connection.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub several_servers: bool,
    /// Keeps the server from taking the session for idle, as the C# `RdpAntiIdle`: Shift is
    /// pressed and released at the settings' anti-idle interval while the session is open.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub anti_idle: bool,
    /// A desktop that drops for a reason that may pass is opened again by itself, as the C#
    /// `RdpAutoReconnect`: on unless cleared.
    #[serde(default = "shared", skip_serializing_if = "is_shared")]
    pub auto_reconnect: bool,
    /// What the profile asks that the built-in client does not do yet, kept.
    #[serde(flatten)]
    pub extras: RdpExtras,
}

/// What a C# RDP profile asks that the built-in client does not do yet: kept, so that an
/// import loses nothing and the external client can be given it, and shown as not used.
#[expect(
    clippy::struct_excessive_bools,
    reason = "one switch per C# RDP option, each saved on its own"
)]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RdpExtras {
    /// Opened in the Windows client rather than in a tab, as the C# `RdpMode` External.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub external: bool,
    /// The Remote Desktop Gateway the server is reached through, as the C# `RdpGateway`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rd_gateway: Option<String>,
    /// Share this computer's printers, as the C# `RdpRedirectPrinters`.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub redirect_printers: bool,
    /// Share this computer's serial ports, as the C# `RdpRedirectComPorts`.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub redirect_com_ports: bool,
    /// Share this computer's smart cards, as the C# `RdpRedirectSmartCards`.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub redirect_smart_cards: bool,
    /// Share this computer's webcam, as the C# `RdpRedirectWebcam`.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub redirect_webcam: bool,
    /// Share this computer's USB devices, as the C# `RdpRedirectUsb`.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub redirect_usb: bool,
    /// Record from this computer's microphone, as the C# `RdpAudioCapture`.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub microphone: bool,
    /// Span the desktop over several monitors, as the C# `RdpMultiMonitor`.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub multi_monitor: bool,
    /// The monitors spanned, by index, as the C# `RdpSelectedMonitorIndices`; none for all.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub monitors: Vec<u32>,
    /// Refuse a server whose identity cannot be checked, as the C#
    /// `RdpStrictServerAuthentication`.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub strict_server_authentication: bool,
    /// Never use UDP, as the C# `RdpDisableUdp`.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub disable_udp: bool,
    /// Keep bitmaps in a cache, as the C# `RdpBitmapCaching`: on unless turned off.
    #[serde(default = "shared", skip_serializing_if = "is_shared")]
    pub bitmap_caching: bool,
    /// Compress the traffic, as the C# `RdpCompression`: on unless turned off.
    #[serde(default = "shared", skip_serializing_if = "is_shared")]
    pub compression: bool,
    /// Decode through the graphics adapter, as the C# `RdpHardwareAcceleration`.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub hardware_acceleration: bool,
    /// Open in full screen, as the C# `RdpFullScreen`.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub full_screen: bool,
}

impl Default for RdpExtras {
    fn default() -> Self {
        Self {
            external: false,
            rd_gateway: None,
            redirect_printers: false,
            redirect_com_ports: false,
            redirect_smart_cards: false,
            redirect_webcam: false,
            redirect_usb: false,
            microphone: false,
            multi_monitor: false,
            monitors: Vec::new(),
            strict_server_authentication: false,
            disable_udp: false,
            bitmap_caching: true,
            compression: true,
            hardware_acceleration: false,
            full_screen: false,
        }
    }
}

/// A choice of [`RdpExtras`] the built-in client does not honour yet.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RdpExtra {
    /// Opened in the Windows client.
    External,
    /// Through a Remote Desktop Gateway.
    RdGateway,
    /// Printers shared.
    Printers,
    /// Serial ports shared.
    ComPorts,
    /// Smart cards shared.
    SmartCards,
    /// Webcam shared.
    Webcam,
    /// USB devices shared.
    Usb,
    /// Microphone recorded.
    Microphone,
    /// Several monitors spanned.
    MultiMonitor,
    /// Opened in full screen.
    FullScreen,
}

impl RdpExtras {
    /// The choices turned on that the built-in client does not honour yet, in a fixed order;
    /// the external client is not one, the form's session mode showing it.
    #[must_use]
    pub fn unused(&self) -> Vec<RdpExtra> {
        [
            (
                self.rd_gateway
                    .as_deref()
                    .is_some_and(|host| !host.trim().is_empty()),
                RdpExtra::RdGateway,
            ),
            (self.redirect_printers, RdpExtra::Printers),
            (self.redirect_com_ports, RdpExtra::ComPorts),
            (self.redirect_smart_cards, RdpExtra::SmartCards),
            (self.redirect_webcam, RdpExtra::Webcam),
            (self.redirect_usb, RdpExtra::Usb),
            (self.microphone, RdpExtra::Microphone),
            (self.multi_monitor, RdpExtra::MultiMonitor),
            (self.full_screen, RdpExtra::FullScreen),
        ]
        .into_iter()
        .filter_map(|(on, extra)| on.then_some(extra))
        .collect()
    }

    /// The Remote Desktop Gateway the server is reached through, when one is named.
    #[must_use]
    pub fn rd_gateway(&self) -> Option<&str> {
        self.rd_gateway
            .as_deref()
            .map(str::trim)
            .filter(|host| !host.is_empty())
    }
}

impl RdpProfile {
    /// The profile a session opens with: its own values, or, when it follows them, the
    /// application's `defaults` in their place. Derived for each session, never saved.
    #[must_use]
    pub fn effective(self, defaults: &RdpDefaults) -> Self {
        let mut profile = self;
        if profile.follow_defaults {
            profile.redirect_clipboard = defaults.redirect_clipboard;
            profile.redirect_drives = defaults.redirect_drives;
            profile.allow_tls_only = !defaults.nla;
            profile.options.color_depth = defaults.color_depth;
            profile.options.audio = defaults.audio;
            profile.options.dynamic_resolution = defaults.dynamic_resolution;
            profile.auto_reconnect = defaults.auto_reconnect;
        }
        profile
    }
}

/// The RDP options of the application, which a profile following them takes, as the C#
/// `RdpDefault*` settings, with their defaults.
#[expect(
    clippy::struct_excessive_bools,
    reason = "one switch per C# `RdpDefault*` setting"
)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct RdpDefaults {
    /// Share the clipboard, as `RdpDefaultRedirectClipboard`: on.
    #[serde(default = "shared")]
    pub redirect_clipboard: bool,
    /// Share this computer's drives, as `RdpDefaultRedirectDrives`: off.
    #[serde(default)]
    pub redirect_drives: bool,
    /// Require Network Level Authentication, as `RdpDefaultNla`: on.
    #[serde(default = "shared")]
    pub nla: bool,
    /// Colours of the desktop, as `RdpDefaultColorDepth`: 32 bits.
    #[serde(default)]
    pub color_depth: ColorDepth,
    /// Where the sound goes, as `RdpDefaultAudioMode`: not played.
    #[serde(default)]
    pub audio: AudioPlayback,
    /// The desktop follows the tab's size, as `RdpDefaultDynamicResolution`: on.
    #[serde(default = "shared")]
    pub dynamic_resolution: bool,
    /// A dropped desktop is opened again by itself, as `RdpDefaultAutoReconnect`: on.
    #[serde(default = "shared")]
    pub auto_reconnect: bool,
}

impl Default for RdpDefaults {
    fn default() -> Self {
        Self {
            redirect_clipboard: true,
            redirect_drives: false,
            nla: true,
            color_depth: ColorDepth::default(),
            audio: AudioPlayback::default(),
            dynamic_resolution: true,
            auto_reconnect: true,
        }
    }
}

/// How an RDP session is given: colour depth, sound, administrative session, size. Each is
/// written down only when it differs from the C# Heimdall's default.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct RdpOptions {
    /// Colours of the desktop.
    #[serde(default, skip_serializing_if = "ColorDepth::is_default")]
    pub color_depth: ColorDepth,
    /// Where the server's sound goes.
    #[serde(default, skip_serializing_if = "AudioPlayback::is_default")]
    pub audio: AudioPlayback,
    /// Open the server's administrative session, as `mstsc /admin`.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub admin_session: bool,
    /// How the desktop is sized.
    #[serde(default, skip_serializing_if = "Resolution::is_default")]
    pub resolution: Resolution,
    /// Width asked in [`Resolution::Fixed`], as typed; see [`fixed_desktop`].
    #[serde(
        default = "default_fixed_width",
        skip_serializing_if = "is_default_fixed_width"
    )]
    pub fixed_width: u16,
    /// Height asked in [`Resolution::Fixed`], as typed; see [`fixed_desktop`].
    #[serde(
        default = "default_fixed_height",
        skip_serializing_if = "is_default_fixed_height"
    )]
    pub fixed_height: u16,
    /// A fixed desktop is scaled into the tab, as the C# "Scale fixed resolution to fit the
    /// pane", on by default; off, it is drawn pixel for pixel, centred.
    #[serde(default = "shared", skip_serializing_if = "is_shared")]
    pub scale_fixed: bool,
    /// The desktop follows the tab's size, as the C# "Allow dynamic resolution updates", on
    /// by default; off, it gets the tab's size once and is then scaled.
    #[serde(default = "shared", skip_serializing_if = "is_shared")]
    pub dynamic_resolution: bool,
    /// The visual experience asked of the server, as the C# `RdpPerformanceFlags`: the
    /// `TS_PERF_*` bits of the [`Experience`] boxes ticked, 0 when none is.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub performance_flags: u32,
    /// The proportions the desktop is first given, as the C# `RdpAspectRatio`.
    #[serde(default, skip_serializing_if = "Aspect::is_default")]
    pub aspect: Aspect,
}

/// The proportions of a desktop that follows its tab, as the C# `AspectRatio` offers them
/// under "Match window" and in the session dialog.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Aspect {
    /// The tab's own: the whole of it.
    #[default]
    #[serde(rename = "stretch")]
    Stretch,
    /// 16:9.
    #[serde(rename = "16:9")]
    Wide,
    /// 4:3.
    #[serde(rename = "4:3")]
    Standard,
    /// 21:9.
    #[serde(rename = "21:9")]
    UltraWide,
}

impl Aspect {
    /// Every choice, in the C# dialog's order.
    pub const ALL: [Self; 4] = [Self::Stretch, Self::Wide, Self::Standard, Self::UltraWide];

    /// The ratios, in the C# menu's order.
    pub const RATIOS: [Self; 3] = [Self::Wide, Self::Standard, Self::UltraWide];

    /// The choice a C# `RdpAspectRatio` stands for: its `Auto`, `Preserve` and `Dynamic`
    /// all fill the tab, as `AspectRatioManager` draws them.
    #[must_use]
    pub fn csharp(name: &str) -> Self {
        match name.trim() {
            "16:9" => Self::Wide,
            "4:3" => Self::Standard,
            "21:9" => Self::UltraWide,
            _ => Self::Stretch,
        }
    }

    /// Its width and height, in parts; `None` for the tab's own.
    #[must_use]
    pub fn ratio(self) -> Option<(u32, u32)> {
        match self {
            Self::Stretch => None,
            Self::Wide => Some((16, 9)),
            Self::Standard => Some((4, 3)),
            Self::UltraWide => Some((21, 9)),
        }
    }

    /// The size asked of the server for a tab of `size`: the tab's own for Stretch; else the
    /// largest of the ratio inside it, as the C# `AspectRatioManager` fits it, kept a size
    /// an RDP server takes.
    #[must_use]
    pub fn fit(self, (width, height): (u16, u16)) -> (u16, u16) {
        let Some((parts_wide, parts_high)) = self.ratio() else {
            return (width, height);
        };
        let (width, height) = (u32::from(width), u32::from(height));
        let (fitted_width, fitted_height) = if width * parts_high > height * parts_wide {
            // Wider than the ratio: as high as the tab, bars at the sides.
            (height * parts_wide / parts_high, height)
        } else {
            (width, width * parts_high / parts_wide)
        };
        let side = |value: u32| u16::try_from(value).unwrap_or(u16::MAX);
        fixed_desktop(side(fitted_width), side(fitted_height))
    }

    #[expect(
        clippy::trivially_copy_pass_by_ref,
        reason = "serde's skip_serializing_if passes a reference"
    )]
    fn is_default(&self) -> bool {
        *self == Self::default()
    }
}

/// Whether `value` is 0: a field written down only when set.
#[expect(
    clippy::trivially_copy_pass_by_ref,
    reason = "serde's skip_serializing_if passes a reference"
)]
fn is_zero(value: &u32) -> bool {
    *value == 0
}

/// A box of the C# "Visual experience" card, each one bit of the performance flags the
/// server is sent, as MS-RDPBCGR names them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Experience {
    /// No desktop wallpaper.
    DisableWallpaper,
    /// No visual styles.
    DisableThemes,
    /// No menu and window animations.
    DisableAnimations,
    /// A window's outline only while it is dragged.
    DisableDrag,
    /// No shadow under the pointer.
    DisableCursorShadow,
    /// `ClearType` text.
    EnableFontSmoothing,
    /// Desktop composition (Aero).
    EnableComposition,
}

impl Experience {
    /// Every box, in the C# card's order.
    pub const ALL: [Self; 7] = [
        Self::DisableWallpaper,
        Self::DisableThemes,
        Self::DisableAnimations,
        Self::DisableDrag,
        Self::DisableCursorShadow,
        Self::EnableFontSmoothing,
        Self::EnableComposition,
    ];

    /// Its `TS_PERF_*` bit.
    #[must_use]
    pub const fn bit(self) -> u32 {
        match self {
            Self::DisableWallpaper => 0x01,
            Self::DisableDrag => 0x02,
            Self::DisableAnimations => 0x04,
            Self::DisableThemes => 0x08,
            Self::DisableCursorShadow => 0x20,
            Self::EnableFontSmoothing => 0x80,
            Self::EnableComposition => 0x100,
        }
    }
}

impl Default for RdpOptions {
    fn default() -> Self {
        Self {
            color_depth: ColorDepth::default(),
            audio: AudioPlayback::default(),
            admin_session: false,
            resolution: Resolution::default(),
            fixed_width: DEFAULT_FIXED_SIZE.0,
            fixed_height: DEFAULT_FIXED_SIZE.1,
            scale_fixed: true,
            dynamic_resolution: true,
            performance_flags: 0,
            aspect: Aspect::Stretch,
        }
    }
}

impl RdpOptions {
    /// Whether the box `experience` is ticked.
    #[must_use]
    pub fn has(&self, experience: Experience) -> bool {
        self.performance_flags & experience.bit() != 0
    }

    /// Ticks or clears the box `experience`, the other bits kept.
    pub fn set(&mut self, experience: Experience, on: bool) {
        if on {
            self.performance_flags |= experience.bit();
        } else {
            self.performance_flags &= !experience.bit();
        }
    }

    /// How the desktop's size is decided while the session runs.
    #[must_use]
    pub fn sizing(&self) -> DesktopSizing {
        match self.resolution {
            Resolution::Fixed => {
                let (width, height) = fixed_desktop(self.fixed_width, self.fixed_height);
                DesktopSizing::Fixed { width, height }
            }
            Resolution::FitWindow
            | Resolution::SmartSizing
            | Resolution::Auto
            | Resolution::MultiMonitor
                if self.dynamic_resolution =>
            {
                DesktopSizing::FollowsTab
            }
            Resolution::FitWindow
            | Resolution::SmartSizing
            | Resolution::Auto
            | Resolution::MultiMonitor => DesktopSizing::TabSizeOnce,
        }
    }

    /// Whether the desktop is first shown scaled into its tab, rather than pixel for pixel.
    #[must_use]
    pub fn scaled(&self) -> bool {
        match self.sizing() {
            DesktopSizing::FollowsTab => false,
            DesktopSizing::TabSizeOnce => true,
            DesktopSizing::Fixed { .. } => self.scale_fixed,
        }
    }
}

/// How an RDP desktop is sized, the C# Heimdall's resolution modes an embedded session has.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Resolution {
    /// The tab's size: the C# default.
    #[default]
    FitWindow,
    /// A size of its own, never changed.
    Fixed,
    /// The tab's size, scaled: in an embedded C# session, the same as fitting the window.
    SmartSizing,
    /// Every monitor, as the Windows client spans them; a tab has one: the tab's size.
    MultiMonitor,
    /// The C# default for a new profile: in a tab, the tab's size, scaled, as
    /// `RdpDisplayResolver` gives a windowed session.
    Auto,
}

impl Resolution {
    /// Every mode a tab offers, in the order the C# list shows them; multi-monitor is the
    /// Windows client's.
    pub const ALL: [Self; 4] = [Self::FitWindow, Self::Fixed, Self::SmartSizing, Self::Auto];

    /// The mode of a new profile, as the C# dialog's.
    pub const NEW_PROFILE: Self = Self::Auto;

    #[expect(
        clippy::trivially_copy_pass_by_ref,
        reason = "serde's skip_serializing_if passes a reference"
    )]
    fn is_default(&self) -> bool {
        *self == Self::default()
    }
}

/// How an RDP desktop's size is decided while the session runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DesktopSizing {
    /// Asked of the server each time the tab's size changes.
    FollowsTab,
    /// The tab's size asked once, then kept.
    TabSizeOnce,
    /// This size, asked when connecting and never changed.
    Fixed {
        /// Width, in pixels.
        width: u16,
        /// Height, in pixels.
        height: u16,
    },
}

/// Fixed size of a new profile, as the C# dialog's.
pub const DEFAULT_FIXED_SIZE: (u16, u16) = (1920, 1080);
/// Smallest side of a fixed desktop, as the C# `RdpDisplayLimits`.
pub const FIXED_SIDE_MIN: u16 = 200;
/// Largest width of a fixed desktop.
pub const FIXED_WIDTH_MAX: u16 = 7680;
/// Largest height of a fixed desktop.
pub const FIXED_HEIGHT_MAX: u16 = 4320;
/// Widths are multiples of this, as the C# resolver snaps them.
const WIDTH_STEP: u16 = 4;

/// The fixed size a session is given for `width` by `height` as typed: each side within the
/// C# limits, the width brought down to a multiple of 4.
#[must_use]
pub fn fixed_desktop(width: u16, height: u16) -> (u16, u16) {
    let width = width.clamp(FIXED_SIDE_MIN, FIXED_WIDTH_MAX);
    (
        width - width % WIDTH_STEP,
        height.clamp(FIXED_SIDE_MIN, FIXED_HEIGHT_MAX),
    )
}

/// The sizes the C# resolution menus offer by default, in the C# order.
pub const RESOLUTION_PRESETS: [(u16, u16); 10] = [
    (1920, 1080),
    (1680, 1050),
    (1600, 900),
    (1440, 900),
    (1366, 768),
    (1280, 1024),
    (1280, 720),
    (1024, 768),
    (2560, 1440),
    (3840, 2160),
];

/// A size typed as `WIDTHxHEIGHT`, as the C# "Custom..." reads it: `x` or `X`, spaces around
/// the numbers allowed, each side within the C# limits, the width brought down to a multiple
/// of 4; `None` when it is not one.
#[must_use]
pub fn parse_resolution(typed: &str) -> Option<(u16, u16)> {
    let (width, height) = resolution_preset(typed)?;
    Some(fixed_desktop(width, height))
}

/// A size typed as `WIDTHxHEIGHT`, kept as typed: `x` or `X`, spaces around the numbers
/// allowed, each side within the C# `RdpDisplayLimits`; `None` when it is not one.
#[must_use]
pub fn resolution_preset(typed: &str) -> Option<(u16, u16)> {
    let (width, height) = typed.split_once(['x', 'X'])?;
    let size = (width.trim().parse().ok()?, height.trim().parse().ok()?);
    preset_fits(size).then_some(size)
}

/// Whether a resolution preset is within the C# `RdpDisplayLimits`.
#[must_use]
pub fn preset_fits((width, height): (u16, u16)) -> bool {
    (FIXED_SIDE_MIN..=FIXED_WIDTH_MAX).contains(&width)
        && (FIXED_SIDE_MIN..=FIXED_HEIGHT_MAX).contains(&height)
}

/// A preset as the settings file and the Settings box write it: `WIDTHxHEIGHT`.
#[must_use]
pub fn resolution_text((width, height): (u16, u16)) -> String {
    format!("{width}x{height}")
}

/// The presets typed one per line, as the C# Settings box reads them: blank lines are
/// skipped, and nothing typed at all is the built-in list. The text is read as a whole: the
/// lines that are not a size within the limits, trimmed, when there are any.
///
/// # Errors
///
/// The lines that are not a preset, so none is dropped without a word.
pub fn parse_resolution_presets(typed: &str) -> Result<Vec<(u16, u16)>, Vec<String>> {
    let mut presets = Vec::new();
    let mut invalid = Vec::new();
    for line in typed.lines().map(str::trim).filter(|line| !line.is_empty()) {
        match resolution_preset(line) {
            Some(size) => presets.push(size),
            None => invalid.push(line.to_owned()),
        }
    }
    if invalid.is_empty() {
        Ok(presets)
    } else {
        Err(invalid)
    }
}

fn default_fixed_width() -> u16 {
    DEFAULT_FIXED_SIZE.0
}

fn default_fixed_height() -> u16 {
    DEFAULT_FIXED_SIZE.1
}

#[expect(
    clippy::trivially_copy_pass_by_ref,
    reason = "serde's skip_serializing_if passes a reference"
)]
fn is_default_fixed_width(width: &u16) -> bool {
    *width == DEFAULT_FIXED_SIZE.0
}

#[expect(
    clippy::trivially_copy_pass_by_ref,
    reason = "serde's skip_serializing_if passes a reference"
)]
fn is_default_fixed_height(height: &u16) -> bool {
    *height == DEFAULT_FIXED_SIZE.1
}

/// Bits per pixel of an RDP desktop: the three the C# Heimdall offers.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(try_from = "u32", into = "u32")]
pub enum ColorDepth {
    /// High colour, 16 bits.
    Bpp16,
    /// True colour, 24 bits.
    Bpp24,
    /// True colour with alpha, 32 bits: the C# default.
    #[default]
    Bpp32,
}

impl ColorDepth {
    /// Every depth, in the order the C# list shows them.
    pub const ALL: [Self; 3] = [Self::Bpp16, Self::Bpp24, Self::Bpp32];

    /// Bits per pixel.
    #[must_use]
    pub fn bits(self) -> u32 {
        match self {
            Self::Bpp16 => 16,
            Self::Bpp24 => 24,
            Self::Bpp32 => 32,
        }
    }

    /// The depth a session is given for `bits`, as the C# `NormalizeColorDepth`: 16 or less
    /// is 16, up to 24 is 24, more is 32.
    #[must_use]
    pub fn nearest(bits: i64) -> Self {
        match bits {
            ..=16 => Self::Bpp16,
            17..=24 => Self::Bpp24,
            _ => Self::Bpp32,
        }
    }

    #[expect(
        clippy::trivially_copy_pass_by_ref,
        reason = "serde's skip_serializing_if passes a reference"
    )]
    fn is_default(&self) -> bool {
        *self == Self::default()
    }
}

impl From<ColorDepth> for u32 {
    fn from(depth: ColorDepth) -> Self {
        depth.bits()
    }
}

impl TryFrom<u32> for ColorDepth {
    type Error = String;

    fn try_from(bits: u32) -> Result<Self, Self::Error> {
        Self::ALL
            .into_iter()
            .find(|depth| depth.bits() == bits)
            .ok_or_else(|| format!("colour depth {bits} is not 16, 24 or 32"))
    }
}

/// Where an RDP server's sound goes, as the C# Heimdall's audio modes.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum AudioPlayback {
    /// Not played: the C# default.
    #[default]
    Off,
    /// Played on this computer.
    Local,
    /// Played on the server's own speakers.
    OnServer,
}

impl AudioPlayback {
    /// Every mode, in the order the C# list shows them.
    pub const ALL: [Self; 3] = [Self::Off, Self::Local, Self::OnServer];

    #[expect(
        clippy::trivially_copy_pass_by_ref,
        reason = "serde's skip_serializing_if passes a reference"
    )]
    fn is_default(&self) -> bool {
        *self == Self::default()
    }
}

/// The clipboard is shared unless a profile says otherwise.
fn shared() -> bool {
    true
}

#[expect(
    clippy::trivially_copy_pass_by_ref,
    reason = "serde's skip_serializing_if passes a reference"
)]
fn is_shared(value: &bool) -> bool {
    *value
}

/// A saved Telnet destination, reached directly.
///
/// Holds no account: a Telnet server asks for one in the session itself, as text.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TelnetProfile {
    /// Stable identifier.
    pub id: ProfileId,
    /// Name shown to the user.
    pub name: String,
    /// Folder path, `/`-separated, when the profile is filed in one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub group: Option<String>,
    /// Host name or address.
    pub host: String,
    /// TCP port.
    pub port: u16,
    /// Whether its sessions keep a transcript, as the C# `SessionLoggingOverride`: `None`
    /// follows the Settings page's session logging, `Some` decides for this profile alone.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_logging: Option<bool>,
}

/// A saved `WinRM` destination: a remote `PowerShell` session on a Windows server.
///
/// Holds no password: with a user name, `PowerShell` asks for it when connecting; without,
/// the session runs as the account running Heimdall.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WinRmProfile {
    /// Stable identifier.
    pub id: ProfileId,
    /// Name shown to the user.
    pub name: String,
    /// Folder path, `/`-separated, when the profile is filed in one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub group: Option<String>,
    /// Host name or address.
    pub host: String,
    /// TCP port.
    pub port: u16,
    /// `WinRM` over HTTPS.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub use_ssl: bool,
    /// Over HTTPS, accept whatever certificate the server shows: for test servers only.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub skip_certificate_check: bool,
    /// Account to log in as, `DOMAIN\user` or `user@domain`; `None` for the account running
    /// Heimdall.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub username: Option<String>,
    /// SSH gateway the session goes through, by profile ID, as the C# `SshGatewayId`: HTTP
    /// only, through a forward of this computer's loopback address.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gateway: Option<ProfileId>,
}

/// Port of an FTP server when a profile names none, as the C# `DefaultPorts.Ftp`.
pub const DEFAULT_FTP_PORT: u16 = 21;

/// A saved FTP destination, reached directly, as the C# FTP profile: its files in a Files
/// tab.
///
/// Holds no password: it is asked for when connecting, or saved apart as the others'.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FtpProfile {
    /// Stable identifier.
    pub id: ProfileId,
    /// Name shown to the user.
    pub name: String,
    /// Folder path, `/`-separated, when the profile is filed in one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub group: Option<String>,
    /// Host name or address.
    pub host: String,
    /// TCP port.
    pub port: u16,
    /// Account; `None` logs in as `anonymous`, as the C# with no name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub username: Option<String>,
    /// Passive data connections, as the C# default: on unless turned off, written down only
    /// when off.
    #[serde(default = "passive", skip_serializing_if = "is_passive")]
    pub passive: bool,
    /// Explicit FTPS (`AUTH TLS`), as the C# "Enable SSL/TLS (FTPS)": off unless turned on.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub tls: bool,
    /// The profile's entry in the external password manager, for `{Title}`; `None` uses
    /// its name, as the C# `VaultEntryName`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub vault_entry: Option<String>,
}

/// A Citrix Workspace published application, as the C# Citrix connection type: launched
/// through Citrix Workspace, outside Heimdall, from its `StoreFront` or an ICA file.
///
/// The pre-authenticated launch line the C# keeps from the Workspace cache is not here: it
/// is a secret, and never crosses a file, as the C# import and export drop it too.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CitrixProfile {
    /// Stable identifier.
    pub id: ProfileId,
    /// Name shown to the user.
    pub name: String,
    /// Folder path, `/`-separated, when the profile is filed in one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub group: Option<String>,
    /// The `StoreFront` address the application is published on, as the C#
    /// `CitrixStoreFrontUrl`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub store_front_url: Option<String>,
    /// The published application's name, as the C# `CitrixAppName`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub app_name: Option<String>,
    /// An ICA file launched instead, as the C# `CitrixIcaFilePath`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ica_file: Option<String>,
    /// Seamless windows, as the C# `CitrixSeamlessMode`: on. Kept for the C# file; no
    /// launch reads it, as no C# launch does.
    #[serde(default = "shared")]
    pub seamless: bool,
    /// Single sign-on with this Windows account's Kerberos identity, as the C#
    /// `CitrixUseSso`: on.
    #[serde(default = "shared")]
    pub sso: bool,
}

/// An FTP profile's data connections are passive unless it says otherwise.
fn passive() -> bool {
    true
}

#[expect(
    clippy::trivially_copy_pass_by_ref,
    reason = "serde's skip_serializing_if passes a reference"
)]
fn is_passive(value: &bool) -> bool {
    *value
}

/// A saved VNC destination, reached directly.
///
/// Holds no password: it is asked for when connecting.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VncProfile {
    /// Stable identifier.
    pub id: ProfileId,
    /// Name shown to the user.
    pub name: String,
    /// Folder path, `/`-separated, when the profile is filed in one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub group: Option<String>,
    /// Host name or address.
    pub host: String,
    /// TCP port.
    pub port: u16,
    /// Watch only: no keyboard or pointer goes to the server.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub view_only: bool,
    /// Whether a server asking for no password is accepted. Off by default: for a profile
    /// that expects a password, an impostor offering none would otherwise be let in.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub allow_no_password: bool,
    /// The profile's entry in the external password manager, for `{Title}`; `None` uses
    /// its name, as the C# `VaultEntryName`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub vault_entry: Option<String>,
}

/// The arguments of a local program.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LocalArguments {
    /// One per entry, each quoted as needed when the command line is built.
    List(Vec<String>),
    /// A Windows argument string, put after the program exactly as written: how the C#
    /// Heimdall passed it, and what a profile imported from it keeps. Splitting it would not
    /// give back the same line for every program, `cmd` first.
    WindowsLine(String),
}

impl Default for LocalArguments {
    fn default() -> Self {
        Self::List(Vec::new())
    }
}

impl LocalArguments {
    /// Whether there are none.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        match self {
            Self::List(list) => list.is_empty(),
            Self::WindowsLine(line) => line.is_empty(),
        }
    }
}

/// What a local profile runs.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct LocalCommand {
    /// The program, a full path or a name looked up in `PATH`; `None` for the default shell.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub program: Option<String>,
    /// Its arguments.
    #[serde(default, skip_serializing_if = "LocalArguments::is_empty")]
    pub arguments: LocalArguments,
    /// Folder it starts in; `None` for the home folder.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub working_directory: Option<PathBuf>,
}

impl LocalCommand {
    /// Whether it is the default shell, as the Local shell button opens it: nothing to run
    /// that the user did not choose.
    #[must_use]
    pub fn is_default(&self) -> bool {
        self.program.is_none() && self.arguments.is_empty() && self.working_directory.is_none()
    }
}

/// What the user agreed to run: the command as shown, and the file its program was found
/// at then. It holds for as long as both are still what would run.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LocalApproval {
    /// The command shown.
    pub command: LocalCommand,
    /// The full path its program was found at.
    pub program_path: PathBuf,
}

/// A saved local program: a shell, or any command-line program, run in a terminal tab.
///
/// A profile that runs anything other than the default shell runs only once the user has
/// approved exactly what it runs; an imported profile never carries an approval.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LocalProfile {
    /// Stable identifier.
    pub id: ProfileId,
    /// Name shown to the user.
    pub name: String,
    /// Folder path, `/`-separated, when the profile is filed in one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub group: Option<String>,
    /// What it runs.
    pub command: LocalCommand,
    /// What the user approved, if anything.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub approved: Option<LocalApproval>,
    /// Whether its sessions keep a transcript, as the C# `SessionLoggingOverride`: `None`
    /// follows the Settings page's session logging, `Some` decides for this profile alone.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_logging: Option<bool>,
}

impl LocalProfile {
    /// Whether `program_path`, where the program is found now, may run without asking:
    /// the default shell always may; anything else only as approved, program path included.
    #[must_use]
    pub fn may_run(&self, program_path: &std::path::Path) -> bool {
        self.command.is_default()
            || self.approved.as_ref().is_some_and(|approval| {
                approval.command == self.command && approval.program_path == program_path
            })
    }
}

/// `host:port` as people write it: an IPv6 address in brackets, so the port stays
/// apart from it (`[fe80::1]:22`).
#[must_use]
pub fn display_address(host: &str, port: u16) -> String {
    if host.contains(':') {
        format!("[{host}]:{port}")
    } else {
        format!("{host}:{port}")
    }
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};

    use super::{
        LocalApproval, LocalArguments, LocalCommand, LocalProfile, ProfileId, display_address,
    };

    #[test]
    fn an_ipv6_address_is_bracketed_so_its_port_stays_apart() {
        assert_eq!(display_address("fe80::1", 22), "[fe80::1]:22");
        assert_eq!(display_address("srv.lab", 2222), "srv.lab:2222");
    }

    fn tool() -> LocalCommand {
        LocalCommand {
            program: Some("tool".to_owned()),
            arguments: LocalArguments::List(vec!["-x".to_owned()]),
            working_directory: None,
        }
    }

    fn profile(command: LocalCommand, approved: Option<LocalApproval>) -> LocalProfile {
        LocalProfile {
            id: ProfileId::new("p"),
            name: "P".to_owned(),
            group: None,
            command,
            approved,
            session_logging: None,
        }
    }

    const FOUND: &str = "/usr/bin/tool";

    fn approval(command: LocalCommand) -> LocalApproval {
        LocalApproval {
            command,
            program_path: PathBuf::from(FOUND),
        }
    }

    #[test]
    fn the_default_shell_runs_without_approval() {
        assert!(profile(LocalCommand::default(), None).may_run(Path::new("/bin/sh")));
    }

    #[test]
    fn anything_else_runs_only_once_approved() {
        assert!(!profile(tool(), None).may_run(Path::new(FOUND)));
        assert!(profile(tool(), Some(approval(tool()))).may_run(Path::new(FOUND)));
    }

    #[test]
    fn an_approval_lapses_when_the_command_changes() {
        let mut changed = tool();
        changed.arguments = LocalArguments::List(vec!["-y".to_owned()]);
        assert!(!profile(changed.clone(), Some(approval(tool()))).may_run(Path::new(FOUND)));
        changed = tool();
        changed.working_directory = Some(PathBuf::from("/tmp"));
        assert!(!profile(changed, Some(approval(tool()))).may_run(Path::new(FOUND)));
    }

    #[test]
    fn an_approval_lapses_when_the_program_is_found_elsewhere() {
        assert!(!profile(tool(), Some(approval(tool()))).may_run(Path::new("/opt/evil/tool")));
    }

    #[test]
    fn arguments_alone_are_not_the_default_shell() {
        let arguments_only = LocalCommand {
            arguments: LocalArguments::WindowsLine("-NoExit".to_owned()),
            ..LocalCommand::default()
        };
        assert!(!arguments_only.is_default());
        assert!(!profile(arguments_only, None).may_run(Path::new("/bin/sh")));
    }
}

#[cfg(test)]
mod resolution_tests {
    use super::{parse_resolution, parse_resolution_presets, resolution_preset};

    #[test]
    fn a_size_is_read_as_the_csharp_custom_one() {
        assert_eq!(parse_resolution("1920x1080"), Some((1920, 1080)));
        assert_eq!(parse_resolution(" 1600 X 900 "), Some((1600, 900)));
        assert_eq!(
            parse_resolution("1366x768"),
            Some((1364, 768)),
            "width to a multiple of 4"
        );
        assert_eq!(parse_resolution("199x768"), None, "under the limits");
        assert_eq!(parse_resolution("7681x768"), None, "over them");
        assert_eq!(parse_resolution("1920x4321"), None);
        assert_eq!(parse_resolution("1920*1080"), None);
        assert_eq!(parse_resolution("wide x tall"), None);
    }

    #[test]
    fn presets_are_kept_as_typed_and_read_as_a_whole() {
        assert_eq!(
            resolution_preset("1366x768"),
            Some((1366, 768)),
            "a preset is not snapped: the menu snaps the size it gives"
        );
        assert_eq!(
            parse_resolution_presets("1920x1080\n\n  1280 x 720  \n"),
            Ok(vec![(1920, 1080), (1280, 720)]),
            "blank lines skipped"
        );
        assert_eq!(
            parse_resolution_presets("  \n"),
            Ok(Vec::new()),
            "the built-in list"
        );
        assert_eq!(
            parse_resolution_presets("1920x1080\n1920x\n99999x1\n800x600"),
            Err(vec!["1920x".to_owned(), "99999x1".to_owned()]),
            "every bad line named"
        );
    }
}
