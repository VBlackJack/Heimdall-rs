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

//! A profile as typed into its form, and the checks that turn it into a saved profile.

use std::hash::{BuildHasher as _, RandomState};
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use heimdall_core::post_connect::PostConnect;
use heimdall_core::profile::{
    AudioPlayback, ColorDepth, DEFAULT_FTP_PORT, DEFAULT_RDP_PORT, DEFAULT_TELNET_PORT,
    DEFAULT_VNC_PORT, DEFAULT_WINRM_HTTP_PORT, DEFAULT_WINRM_HTTPS_PORT, Experience,
    FIXED_HEIGHT_MAX, FIXED_SIDE_MIN, FIXED_WIDTH_MAX, Forwards, FtpProfile, LocalCommand,
    LocalProfile, ProfileId, RdpOptions, RdpProfile, Resolution, SshProfile, TelnetProfile,
    VncProfile, WinRmProfile, fixed_desktop,
};

use crate::local_draft;
use crate::steps_draft::StepsDraft;

/// The port of a local shell, which has none.
const NO_PORT: u16 = 0;

/// Port when the field is left empty.
pub const DEFAULT_SSH_PORT: u16 = 22;

/// Prefix of the identifiers this application gives, apart from imported ones.
const ID_PREFIX: &str = "rs-";

/// A field of the profile form.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProfileField {
    /// Name shown in the list.
    Name,
    /// Group it is listed under.
    Group,
    /// Server address.
    Host,
    /// Server port.
    Port,
    /// User name.
    Username,
    /// Private key file.
    KeyPath,
    /// Windows domain of an RDP account.
    Domain,
    /// Width of a fixed RDP desktop.
    FixedWidth,
    /// Height of a fixed RDP desktop.
    FixedHeight,
    /// The profile's entry in the external password manager.
    VaultEntry,
    /// The local port of the SOCKS proxy opened through the gateway.
    SocksPort,
    /// The gateway's port of the remote forward.
    RemoteBindPort,
    /// The local port of the remote forward.
    RemoteLocalPort,
    /// The program a local shell runs; empty is the default shell.
    LocalProgram,
    /// Its arguments, as one line.
    LocalArguments,
    /// The folder it starts in; empty is the current one.
    WorkingDirectory,
}

impl ProfileField {
    /// Every field, in form order.
    pub const ALL: [Self; 16] = [
        Self::Name,
        Self::Group,
        Self::Host,
        Self::Port,
        Self::Username,
        Self::Domain,
        Self::KeyPath,
        Self::FixedWidth,
        Self::FixedHeight,
        Self::VaultEntry,
        Self::SocksPort,
        Self::RemoteBindPort,
        Self::RemoteLocalPort,
        Self::LocalProgram,
        Self::LocalArguments,
        Self::WorkingDirectory,
    ];
}

/// The protocol a form is for: every protocol the editor knows.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum DraftProtocol {
    /// SSH, with SFTP for its files.
    #[default]
    Ssh,
    /// SFTP: the SSH fields, opening the files.
    Sftp,
    /// Remote desktop.
    Rdp,
    /// VNC desktop.
    Vnc,
    /// Remote `PowerShell`.
    WinRm,
    /// Telnet terminal.
    Telnet,
    /// FTP, plain or explicit FTPS: a Files tab.
    Ftp,
    /// A shell on this computer.
    Local,
}

impl DraftProtocol {
    /// Every protocol, in the order the picker shows them.
    pub const ALL: [Self; 8] = [
        Self::Rdp,
        Self::Ssh,
        Self::WinRm,
        Self::Sftp,
        Self::Vnc,
        Self::Telnet,
        Self::Ftp,
        Self::Local,
    ];

    /// Whether it is SSH or SFTP: the same fields, the same profile.
    #[must_use]
    pub fn is_ssh_family(self) -> bool {
        matches!(self, Self::Ssh | Self::Sftp)
    }

    /// Whether a form for this protocol shows `field`.
    #[must_use]
    pub fn shows(self, field: ProfileField) -> bool {
        match field {
            ProfileField::Name | ProfileField::Group => true,
            // A local shell has no server.
            ProfileField::Host | ProfileField::Port => self != Self::Local,
            ProfileField::LocalProgram
            | ProfileField::LocalArguments
            | ProfileField::WorkingDirectory => self == Self::Local,
            ProfileField::Username => {
                matches!(
                    self,
                    Self::Ssh | Self::Sftp | Self::Rdp | Self::WinRm | Self::Ftp
                )
            }
            ProfileField::Domain | ProfileField::FixedWidth | ProfileField::FixedHeight => {
                self == Self::Rdp
            }
            ProfileField::KeyPath => self.is_ssh_family(),
            // The protocols whose password the external credential provider gives.
            ProfileField::VaultEntry => self.saves_password(),
            ProfileField::SocksPort
            | ProfileField::RemoteBindPort
            | ProfileField::RemoteLocalPort => self.routes_through_gateway(),
        }
    }

    /// Whether a password can be saved with this protocol's profiles. A `WinRM` password is
    /// typed into `PowerShell`, which asks for it.
    #[must_use]
    pub fn saves_password(self) -> bool {
        matches!(
            self,
            Self::Ssh | Self::Sftp | Self::Rdp | Self::Vnc | Self::Ftp
        )
    }

    /// Whether this protocol's sessions can go through an SSH gateway.
    #[must_use]
    pub fn routes_through_gateway(self) -> bool {
        matches!(self, Self::Ssh | Self::Sftp | Self::Rdp)
    }

    /// Whether the protocol's profiles name a key file, whose passphrase can be saved.
    #[must_use]
    pub fn has_key_file(self) -> bool {
        matches!(self, Self::Ssh | Self::Sftp)
    }

    /// Whether a saved password belongs to an account, which the form must then name.
    #[must_use]
    pub fn password_needs_username(self) -> bool {
        matches!(self, Self::Ssh | Self::Sftp | Self::Rdp)
    }
}

/// An option of a form, shown as a box to tick.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProfileToggle {
    /// RDP: require Network Level Authentication, as the C# "Enable Network Level
    /// Authentication" box, ticked by default; cleared, a server with TLS alone is accepted.
    Nla,
    /// RDP: share the clipboard.
    RedirectClipboard,
    /// RDP: share this computer's drives.
    RedirectDrives,
    /// `WinRM`: log in with a stored account rather than the current Windows identity.
    StoredCredential,
    /// `WinRM`: over HTTPS.
    UseSsl,
    /// `WinRM` over HTTPS: accept any certificate.
    SkipCertificateCheck,
    /// VNC: watch only.
    ViewOnly,
    /// VNC: connect to a server asking no password.
    AllowNoPassword,
    /// SSH, RDP: connect directly, not through the gateway chosen, as the C# "Connect
    /// directly without an SSH gateway" box.
    DirectConnection,
    /// RDP: open the server's administrative session, as the C# "Run as administrator
    /// session (/admin)" box.
    AdminSession,
    /// SSH: forward this computer's SSH agent to the shell, as the C# "Forward SSH agent".
    ForwardAgent,
    /// SSH: compress the traffic, as the C# "Enable compression".
    Compression,
    /// FTP: passive data connections, as the C# "Passive mode", ticked by default.
    Passive,
    /// FTP: explicit FTPS, as the C# "Enable SSL/TLS (FTPS)".
    Tls,
    /// RDP: take the application's RDP options, as the C# "Use global RDP defaults", ticked
    /// for a new profile.
    FollowDefaults,
    /// SSH, SFTP: also offer the older algorithms old appliances speak, after the current
    /// ones.
    LegacyAlgorithms,
    /// RDP: several machines answer at the address; each new certificate is asked about.
    SeveralServers,
    /// RDP: keep the server from taking the session for idle, as the C# "Enable anti-idle
    /// keepalive".
    AntiIdle,
}

impl ProfileToggle {
    /// The options of `protocol`'s form, in form order.
    #[must_use]
    pub fn of(protocol: DraftProtocol) -> &'static [Self] {
        match protocol {
            DraftProtocol::Rdp => &[
                Self::RedirectClipboard,
                Self::RedirectDrives,
                Self::AntiIdle,
                Self::Nla,
                Self::AdminSession,
                Self::SeveralServers,
            ],
            DraftProtocol::WinRm => &[
                Self::StoredCredential,
                Self::UseSsl,
                Self::SkipCertificateCheck,
            ],
            DraftProtocol::Vnc => &[Self::ViewOnly, Self::AllowNoPassword],
            DraftProtocol::Ssh => &[
                Self::Compression,
                Self::ForwardAgent,
                Self::LegacyAlgorithms,
            ],
            // No shell to forward the agent to.
            DraftProtocol::Sftp => &[Self::Compression, Self::LegacyAlgorithms],
            DraftProtocol::Ftp => &[Self::Passive, Self::Tls],
            DraftProtocol::Telnet | DraftProtocol::Local => &[],
        }
    }
}

/// A secret saved for a profile or gateway, as its form shows it: its field stays empty
/// whatever is saved.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum SavedSecret {
    /// None is saved.
    #[default]
    Absent,
    /// One is saved: the form says so.
    Saved,
    /// The saved one is to be removed when the form is saved.
    Cleared,
}

impl SavedSecret {
    /// `Saved` when one is, `Absent` otherwise.
    #[must_use]
    pub fn from_saved(saved: bool) -> Self {
        if saved { Self::Saved } else { Self::Absent }
    }
}

/// A value chosen from a list of an RDP profile's form.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProfileChoice {
    /// The colour depth.
    ColorDepth(ColorDepth),
    /// Where the sound goes.
    Audio(AudioPlayback),
    /// How the desktop is sized.
    Resolution(Resolution),
    /// A common size, written into the width and height.
    Preset(u16, u16),
    /// Whether a fixed desktop is scaled into the tab.
    ScaleFixed(bool),
    /// Whether the desktop follows the tab's size.
    DynamicResolution(bool),
    /// A box of the visual experience, ticked or cleared.
    Experience(Experience, bool),
}

/// The profile a form saves, of its protocol.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DraftProfile {
    /// SSH.
    Ssh(SshProfile),
    /// RDP.
    Rdp(RdpProfile),
    /// VNC.
    Vnc(VncProfile),
    /// `WinRM`.
    WinRm(WinRmProfile),
    /// Telnet.
    Telnet(TelnetProfile),
    /// A local shell.
    Local(LocalProfile),
    /// FTP.
    Ftp(FtpProfile),
}

/// What the form holds, as typed.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ProfileDraft {
    /// The profile being edited; `None` for a new one.
    pub editing: Option<ProfileId>,
    /// Name.
    pub name: String,
    /// Group.
    pub group: String,
    /// Address.
    pub host: String,
    /// Port; empty is [`DEFAULT_SSH_PORT`].
    pub port: String,
    /// User name.
    pub username: String,
    /// Key file.
    pub key_path: String,
    /// The SSH gateway of the profile being edited, which the form does not show: kept as it
    /// is, so that saving the form never drops it.
    pub gateway: Option<ProfileId>,
    /// Protocol.
    pub protocol: DraftProtocol,
    /// A new profile's protocol was chosen: the form shows its fields, not the picker.
    pub protocol_chosen: bool,
    /// Windows domain (RDP).
    pub domain: String,
    /// Options ticked.
    pub toggles: Vec<ProfileToggle>,
    /// A password is saved for the profile: the form says so, its field stays empty.
    pub password_saved: bool,
    /// The saved password is to be removed when the form is saved.
    pub clear_password: bool,
    /// The key passphrase saved for the profile, as the form shows it.
    pub passphrase: SavedSecret,
    /// RDP: the options chosen from lists and boxes of their own; the administrative session
    /// is a toggle, the fixed size is typed in `fixed_width` and `fixed_height`.
    pub rdp_options: RdpOptions,
    /// RDP: width of a fixed desktop, as typed.
    pub fixed_width: String,
    /// RDP: height of a fixed desktop, as typed.
    pub fixed_height: String,
    /// The entry in the external password manager; empty uses the name.
    pub vault_entry: String,
    /// The SOCKS proxy's local port, as typed; empty or 0 opens none, as the C# 0.
    pub socks_port: String,
    /// The remote forward's gateway port, as typed; empty or 0 opens none.
    pub remote_bind_port: String,
    /// The remote forward's local port, as typed; empty or 0 is the same port.
    pub remote_local_port: String,
    /// SSH: the post-connect steps and the one selected.
    pub post_connect: StepsDraft,
    /// Local: the program, as typed.
    pub local_program: String,
    /// Local: the arguments, as one line.
    pub local_arguments: String,
    /// Local: the folder it starts in, as typed.
    pub working_directory: String,
}

/// The fields every protocol's profile takes from the form, once checked.
struct Checked<'a> {
    id: ProfileId,
    name: String,
    group: Option<String>,
    host: String,
    port: u16,
    username: &'a str,
    key_path: &'a str,
    domain: &'a str,
    vault_entry: &'a str,
}

/// Why a form cannot be saved yet.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DraftError {
    /// The name is empty.
    NameMissing,
    /// The address is empty.
    HostMissing,
    /// The address holds a space or a control character.
    HostInvalid,
    /// The address holds `user@`: the user has its own field.
    HostHasUser,
    /// The address holds `:port`: the port has its own field.
    HostHasPort,
    /// The port is not a number from 1 to 65535.
    PortInvalid,
    /// The user name holds a space or a control character.
    UsernameInvalid,
    /// A text holds a control character.
    ControlCharacter,
    /// A password is typed but no user name: a password is for an account.
    UsernameForPassword,
    /// A stored `WinRM` credential names no account.
    UsernameMissing,
    /// The domain holds a space or a double quote.
    DomainInvalid,
    /// A fixed width is not a number from 200 to 7680.
    FixedWidthInvalid,
    /// A fixed height is not a number from 200 to 4320.
    FixedHeightInvalid,
    /// A gateway's parents lead back to it.
    GatewayLoop,
    /// The SOCKS port is not a number from 0 to 65535.
    SocksPortInvalid,
    /// The remote forward's gateway port is not a number from 0 to 65535.
    RemoteBindPortInvalid,
    /// The remote forward's local port is not a number from 0 to 65535.
    RemoteLocalPortInvalid,
    /// A local shell's arguments leave a quote open.
    ArgumentsInvalid,
}

impl DraftError {
    /// The field to fix.
    #[must_use]
    pub fn field(self) -> ProfileField {
        match self {
            Self::NameMissing | Self::ControlCharacter | Self::GatewayLoop => ProfileField::Name,
            Self::HostMissing | Self::HostInvalid | Self::HostHasUser | Self::HostHasPort => {
                ProfileField::Host
            }
            Self::PortInvalid => ProfileField::Port,
            Self::UsernameInvalid | Self::UsernameForPassword | Self::UsernameMissing => {
                ProfileField::Username
            }
            Self::DomainInvalid => ProfileField::Domain,
            Self::FixedWidthInvalid => ProfileField::FixedWidth,
            Self::FixedHeightInvalid => ProfileField::FixedHeight,
            Self::SocksPortInvalid => ProfileField::SocksPort,
            Self::RemoteBindPortInvalid => ProfileField::RemoteBindPort,
            Self::RemoteLocalPortInvalid => ProfileField::RemoteLocalPort,
            Self::ArgumentsInvalid => ProfileField::LocalArguments,
        }
    }
}

impl ProfileDraft {
    /// A form filled from a saved profile.
    #[must_use]
    pub fn from_profile(profile: &SshProfile) -> Self {
        Self {
            editing: Some(profile.id.clone()),
            name: profile.name.clone(),
            group: profile.group.clone().unwrap_or_default(),
            host: profile.host.clone(),
            port: profile.port.to_string(),
            username: profile.username.clone().unwrap_or_default(),
            key_path: profile
                .key_path
                .as_ref()
                .map(|path| path.display().to_string())
                .unwrap_or_default(),
            gateway: profile.gateway.clone(),
            vault_entry: profile.vault_entry.clone().unwrap_or_default(),
            socks_port: port_text(profile.forwards.socks_port),
            remote_bind_port: port_text(profile.forwards.remote_bind_port),
            remote_local_port: port_text(profile.forwards.remote_local_port),
            post_connect: StepsDraft::of(&profile.post_connect.steps),
            toggles: [
                (profile.compression, ProfileToggle::Compression),
                (profile.forward_agent, ProfileToggle::ForwardAgent),
                (profile.legacy_algorithms, ProfileToggle::LegacyAlgorithms),
            ]
            .into_iter()
            .filter_map(|(on, toggle)| on.then_some(toggle))
            .collect(),
            protocol: if profile.sftp {
                DraftProtocol::Sftp
            } else {
                DraftProtocol::Ssh
            },
            protocol_chosen: true,
            ..Self::default()
        }
    }

    /// A form filled from a saved FTP profile.
    #[must_use]
    pub fn from_ftp(profile: &FtpProfile) -> Self {
        Self {
            editing: Some(profile.id.clone()),
            name: profile.name.clone(),
            group: profile.group.clone().unwrap_or_default(),
            host: profile.host.clone(),
            port: profile.port.to_string(),
            username: profile.username.clone().unwrap_or_default(),
            vault_entry: profile.vault_entry.clone().unwrap_or_default(),
            protocol: DraftProtocol::Ftp,
            protocol_chosen: true,
            toggles: [
                (profile.passive, ProfileToggle::Passive),
                (profile.tls, ProfileToggle::Tls),
            ]
            .into_iter()
            .filter_map(|(on, toggle)| on.then_some(toggle))
            .collect(),
            ..Self::default()
        }
    }

    /// A form filled from a saved local shell profile.
    #[must_use]
    pub fn from_local(profile: &LocalProfile) -> Self {
        Self {
            editing: Some(profile.id.clone()),
            name: profile.name.clone(),
            group: profile.group.clone().unwrap_or_default(),
            local_program: profile.command.program.clone().unwrap_or_default(),
            local_arguments: local_draft::line_of(&profile.command.arguments),
            working_directory: profile
                .command
                .working_directory
                .as_ref()
                .map(|folder| folder.display().to_string())
                .unwrap_or_default(),
            protocol: DraftProtocol::Local,
            protocol_chosen: true,
            ..Self::default()
        }
    }

    /// The local shell profile this form describes, under `id`: never approved here; saving
    /// from the form approves it, the program found where it is now.
    ///
    /// # Errors
    ///
    /// [`DraftError::ArgumentsInvalid`] for a quote left open.
    fn saved_local(
        &self,
        id: ProfileId,
        name: String,
        group: Option<String>,
    ) -> Result<DraftProfile, DraftError> {
        let optional = |text: &str| Some(text.trim().to_owned()).filter(|text| !text.is_empty());
        let arguments =
            local_draft::arguments_of(&self.local_arguments).ok_or(DraftError::ArgumentsInvalid)?;
        Ok(DraftProfile::Local(LocalProfile {
            id,
            name,
            group,
            command: LocalCommand {
                program: optional(&self.local_program),
                arguments,
                working_directory: optional(&self.working_directory).map(PathBuf::from),
            },
            approved: None,
        }))
    }

    /// A form filled from a saved RDP profile.
    #[must_use]
    pub fn from_rdp(profile: &RdpProfile) -> Self {
        let mut toggles = Vec::new();
        if profile.redirect_clipboard {
            toggles.push(ProfileToggle::RedirectClipboard);
        }
        if profile.redirect_drives {
            toggles.push(ProfileToggle::RedirectDrives);
        }
        if !profile.allow_tls_only {
            toggles.push(ProfileToggle::Nla);
        }
        if profile.options.admin_session {
            toggles.push(ProfileToggle::AdminSession);
        }
        if profile.follow_defaults {
            toggles.push(ProfileToggle::FollowDefaults);
        }
        if profile.several_servers {
            toggles.push(ProfileToggle::SeveralServers);
        }
        if profile.anti_idle {
            toggles.push(ProfileToggle::AntiIdle);
        }
        Self {
            editing: Some(profile.id.clone()),
            name: profile.name.clone(),
            group: profile.group.clone().unwrap_or_default(),
            host: profile.host.clone(),
            port: profile.port.to_string(),
            username: profile.username.clone().unwrap_or_default(),
            domain: profile.domain.clone().unwrap_or_default(),
            gateway: profile.gateway.clone(),
            vault_entry: profile.vault_entry.clone().unwrap_or_default(),
            socks_port: port_text(profile.forwards.socks_port),
            remote_bind_port: port_text(profile.forwards.remote_bind_port),
            remote_local_port: port_text(profile.forwards.remote_local_port),
            protocol: DraftProtocol::Rdp,
            protocol_chosen: true,
            toggles,
            rdp_options: profile.options,
            fixed_width: profile.options.fixed_width.to_string(),
            fixed_height: profile.options.fixed_height.to_string(),
            ..Self::default()
        }
    }

    /// A form filled from a saved VNC profile.
    #[must_use]
    pub fn from_vnc(profile: &VncProfile) -> Self {
        let mut toggles = Vec::new();
        if profile.view_only {
            toggles.push(ProfileToggle::ViewOnly);
        }
        if profile.allow_no_password {
            toggles.push(ProfileToggle::AllowNoPassword);
        }
        Self {
            editing: Some(profile.id.clone()),
            name: profile.name.clone(),
            group: profile.group.clone().unwrap_or_default(),
            host: profile.host.clone(),
            port: profile.port.to_string(),
            vault_entry: profile.vault_entry.clone().unwrap_or_default(),
            protocol: DraftProtocol::Vnc,
            protocol_chosen: true,
            toggles,
            ..Self::default()
        }
    }

    /// A form filled from a saved `WinRM` profile.
    #[must_use]
    pub fn from_winrm(profile: &WinRmProfile) -> Self {
        let mut toggles = Vec::new();
        if profile.username.is_some() {
            toggles.push(ProfileToggle::StoredCredential);
        }
        if profile.use_ssl {
            toggles.push(ProfileToggle::UseSsl);
        }
        if profile.skip_certificate_check {
            toggles.push(ProfileToggle::SkipCertificateCheck);
        }
        Self {
            editing: Some(profile.id.clone()),
            name: profile.name.clone(),
            group: profile.group.clone().unwrap_or_default(),
            host: profile.host.clone(),
            port: profile.port.to_string(),
            username: profile.username.clone().unwrap_or_default(),
            protocol: DraftProtocol::WinRm,
            protocol_chosen: true,
            toggles,
            ..Self::default()
        }
    }

    /// A form filled from a saved Telnet profile.
    #[must_use]
    pub fn from_telnet(profile: &TelnetProfile) -> Self {
        Self {
            editing: Some(profile.id.clone()),
            name: profile.name.clone(),
            group: profile.group.clone().unwrap_or_default(),
            host: profile.host.clone(),
            port: profile.port.to_string(),
            protocol: DraftProtocol::Telnet,
            protocol_chosen: true,
            ..Self::default()
        }
    }

    /// An empty form for `protocol`, as a new C# session starts: its default port written
    /// in, and for RDP the clipboard shared and Network Level Authentication required.
    #[must_use]
    pub fn new_for(protocol: DraftProtocol) -> Self {
        let options = RdpOptions::default();
        let mut draft = Self {
            protocol,
            protocol_chosen: true,
            toggles: match protocol {
                DraftProtocol::Rdp => vec![
                    ProfileToggle::FollowDefaults,
                    ProfileToggle::RedirectClipboard,
                    ProfileToggle::Nla,
                ],
                // Passive by default, as a new C# FTP profile.
                DraftProtocol::Ftp => vec![ProfileToggle::Passive],
                _ => Vec::new(),
            },
            rdp_options: options,
            fixed_width: options.fixed_width.to_string(),
            fixed_height: options.fixed_height.to_string(),
            ..Self::default()
        };
        draft.port = draft.default_port().to_string();
        draft
    }

    /// Whether `toggle` is ticked.
    #[must_use]
    pub fn is_on(&self, toggle: ProfileToggle) -> bool {
        self.toggles.contains(&toggle)
    }

    /// Ticks or clears `toggle`. As in the C# dialog, "Use SSL" moves a `WinRM` port still
    /// on the other transport's default to its own default; a port typed by hand stays.
    pub fn toggle(&mut self, toggle: ProfileToggle, on: bool) {
        let before = self.default_port();
        self.toggles.retain(|ticked| *ticked != toggle);
        if on {
            self.toggles.push(toggle);
        }
        let port = self.port.trim();
        if toggle == ProfileToggle::UseSsl && (port.is_empty() || port == before.to_string()) {
            self.port = self.default_port().to_string();
        }
    }

    /// Takes `choice` from an RDP list.
    pub fn choose(&mut self, choice: ProfileChoice) {
        match choice {
            ProfileChoice::ColorDepth(depth) => self.rdp_options.color_depth = depth,
            ProfileChoice::Audio(audio) => self.rdp_options.audio = audio,
            ProfileChoice::Resolution(resolution) => self.rdp_options.resolution = resolution,
            ProfileChoice::Preset(width, height) => {
                self.fixed_width = width.to_string();
                self.fixed_height = height.to_string();
            }
            ProfileChoice::ScaleFixed(on) => self.rdp_options.scale_fixed = on,
            ProfileChoice::DynamicResolution(on) => self.rdp_options.dynamic_resolution = on,
            ProfileChoice::Experience(experience, on) => self.rdp_options.set(experience, on),
        }
    }

    /// The RDP options saved: the lists' choices, the administrative session ticked and, in
    /// the fixed mode, the size typed, brought within the limits as the C# dialog does.
    ///
    /// # Errors
    ///
    /// A fixed size that is not a number within the C# limits; checked in the fixed mode
    /// only, where it is shown.
    fn saved_rdp_options(&self) -> Result<RdpOptions, DraftError> {
        let mut options = RdpOptions {
            admin_session: self.is_on(ProfileToggle::AdminSession),
            ..self.rdp_options
        };
        if options.resolution == Resolution::Fixed {
            let side = |typed: &str, max: u16, error: DraftError| {
                typed
                    .trim()
                    .parse::<u16>()
                    .ok()
                    .filter(|side| (FIXED_SIDE_MIN..=max).contains(side))
                    .ok_or(error)
            };
            let width = side(
                &self.fixed_width,
                FIXED_WIDTH_MAX,
                DraftError::FixedWidthInvalid,
            )?;
            let height = side(
                &self.fixed_height,
                FIXED_HEIGHT_MAX,
                DraftError::FixedHeightInvalid,
            )?;
            (options.fixed_width, options.fixed_height) = fixed_desktop(width, height);
        }
        Ok(options)
    }

    /// Whether `field` is shown now: the `WinRM` account only for a stored credential, an RDP
    /// fixed size only in the fixed mode, the SOCKS port only through a gateway.
    #[must_use]
    pub fn shows(&self, field: ProfileField) -> bool {
        let fixed_size = matches!(field, ProfileField::FixedWidth | ProfileField::FixedHeight);
        self.protocol.shows(field)
            && !(self.protocol == DraftProtocol::WinRm
                && field == ProfileField::Username
                && !self.is_on(ProfileToggle::StoredCredential))
            && !(fixed_size && self.rdp_options.resolution != Resolution::Fixed)
            && !(Self::FORWARD_FIELDS.contains(&field) && self.routed_gateway().is_none())
    }

    /// The fields of the ports opened through the gateway, shown only through one.
    const FORWARD_FIELDS: [ProfileField; 3] = [
        ProfileField::SocksPort,
        ProfileField::RemoteBindPort,
        ProfileField::RemoteLocalPort,
    ];

    /// The ports saved, as typed; 0 or empty is none. Checked only where they are shown;
    /// hidden, with no gateway, a value that does not read is dropped, as it could not be
    /// used.
    ///
    /// # Errors
    ///
    /// A shown port that is not a number from 0 to 65535.
    fn saved_forwards(&self) -> Result<Forwards, DraftError> {
        let port = |field: ProfileField, error: DraftError| match self.value(field).trim() {
            "" => Ok(None),
            typed => match typed.parse::<u16>() {
                Ok(port) => Ok(Some(port).filter(|port| *port != 0)),
                Err(_) if self.shows(field) => Err(error),
                Err(_) => Ok(None),
            },
        };
        Ok(Forwards {
            socks_port: port(ProfileField::SocksPort, DraftError::SocksPortInvalid)?,
            remote_bind_port: port(
                ProfileField::RemoteBindPort,
                DraftError::RemoteBindPortInvalid,
            )?,
            remote_local_port: port(
                ProfileField::RemoteLocalPort,
                DraftError::RemoteLocalPortInvalid,
            )?,
        })
    }

    /// The post-connect steps saved, of an SSH form: approved by the store, which the user
    /// who wrote them does when saving, as the C# dialog confirms them.
    fn saved_post_connect(&self) -> PostConnect {
        PostConnect {
            steps: self.post_connect.steps.clone(),
            approved: None,
        }
    }

    /// The user name saved: the one typed where the form shows it, else none.
    ///
    /// # Errors
    ///
    /// [`DraftError::UsernameInvalid`] for a space, a control character or a double quote: a
    /// `WinRM` account is written into a command, and no account name holds one.
    fn checked_username(&self) -> Result<&str, DraftError> {
        let username = if self.shows(ProfileField::Username) {
            self.username.trim()
        } else {
            ""
        };
        if username
            .chars()
            .any(|c| c.is_whitespace() || c.is_control() || c == '"')
        {
            return Err(DraftError::UsernameInvalid);
        }
        Ok(username)
    }

    /// Whether a text of the form, trimmed as it is saved, or a step's command holds a control
    /// character.
    fn has_control_character(&self) -> bool {
        [
            &self.name,
            &self.group,
            &self.key_path,
            &self.domain,
            &self.vault_entry,
            &self.local_program,
            &self.local_arguments,
            &self.working_directory,
        ]
        .iter()
        .any(|text| text.trim().chars().any(char::is_control))
            || self.post_connect_has_control()
    }

    /// Whether a step's command holds a control character, which a line typed into a shell
    /// cannot carry.
    fn post_connect_has_control(&self) -> bool {
        self.protocol == DraftProtocol::Ssh
            && self
                .post_connect
                .steps
                .iter()
                .any(|step| step.input.chars().any(char::is_control))
    }

    /// Whether `toggle` is shown now: skipping certificate checks only over HTTPS.
    #[must_use]
    pub fn shows_toggle(&self, toggle: ProfileToggle) -> bool {
        toggle != ProfileToggle::SkipCertificateCheck || self.is_on(ProfileToggle::UseSsl)
    }

    /// The gateway saved with the profile: none when "Connect directly" is ticked, the
    /// chosen one being kept in the form as the C# combo keeps its selection.
    #[must_use]
    pub fn routed_gateway(&self) -> Option<ProfileId> {
        if self.is_on(ProfileToggle::DirectConnection) {
            None
        } else {
            self.gateway.clone()
        }
    }

    /// The port typed, or the protocol's when the field is empty.
    fn typed_port(&self) -> Result<u16, DraftError> {
        match self.port.trim() {
            "" => Ok(self.default_port()),
            typed => typed
                .parse::<u16>()
                .ok()
                .filter(|port| *port != 0)
                .ok_or(DraftError::PortInvalid),
        }
    }

    /// The port an empty field stands for.
    #[must_use]
    pub fn default_port(&self) -> u16 {
        match self.protocol {
            DraftProtocol::Ssh | DraftProtocol::Sftp => DEFAULT_SSH_PORT,
            DraftProtocol::Rdp => DEFAULT_RDP_PORT,
            DraftProtocol::Vnc => DEFAULT_VNC_PORT,
            DraftProtocol::Telnet => DEFAULT_TELNET_PORT,
            DraftProtocol::WinRm if self.is_on(ProfileToggle::UseSsl) => DEFAULT_WINRM_HTTPS_PORT,
            DraftProtocol::WinRm => DEFAULT_WINRM_HTTP_PORT,
            DraftProtocol::Local => NO_PORT,
            DraftProtocol::Ftp => DEFAULT_FTP_PORT,
        }
    }

    /// The profile this form describes, of its protocol, under `id`.
    ///
    /// # Errors
    ///
    /// The first [`DraftError`] in form order.
    pub fn to_saved(&self, id: ProfileId) -> Result<DraftProfile, DraftError> {
        let name = self.name.trim();
        if name.is_empty() {
            return Err(DraftError::NameMissing);
        }
        let group = self.group.trim();
        let key_path = self.key_path.trim();
        let domain = self.domain.trim();
        let vault_entry = self.vault_entry.trim();
        if self.has_control_character() {
            return Err(DraftError::ControlCharacter);
        }
        // A local shell has no server.
        let local = self.protocol == DraftProtocol::Local;
        let host = if local {
            String::new()
        } else {
            host(&self.host)?
        };
        let port = if local { NO_PORT } else { self.typed_port()? };
        let username = self.checked_username()?;
        if domain.chars().any(|c| c.is_whitespace() || c == '"') {
            return Err(DraftError::DomainInvalid);
        }
        self.build(Checked {
            id,
            name: name.to_owned(),
            group: (!group.is_empty()).then(|| group.to_owned()),
            host,
            port,
            username,
            key_path,
            domain,
            vault_entry,
        })
    }

    /// The profile of the form's protocol, from its fields checked.
    ///
    /// # Errors
    ///
    /// What only one protocol checks: an RDP fixed size, a forwarded port, a `WinRM`
    /// account, a local shell's arguments.
    fn build(&self, checked: Checked<'_>) -> Result<DraftProfile, DraftError> {
        let Checked {
            id,
            name,
            group,
            host,
            port,
            username,
            key_path,
            domain,
            vault_entry,
        } = checked;
        let optional = |text: &str| (!text.is_empty()).then(|| text.to_owned());
        Ok(match self.protocol {
            DraftProtocol::Ssh | DraftProtocol::Sftp => DraftProfile::Ssh(SshProfile {
                id,
                name,
                group,
                host,
                port,
                username: optional(username),
                key_path: optional(key_path).map(PathBuf::from),
                gateway: self.routed_gateway(),
                vault_entry: optional(vault_entry),
                forwards: self.saved_forwards()?,
                post_connect: self.saved_post_connect(),
                forward_agent: self.protocol == DraftProtocol::Ssh
                    && self.is_on(ProfileToggle::ForwardAgent),
                compression: self.is_on(ProfileToggle::Compression),
                sftp: self.protocol == DraftProtocol::Sftp,
                legacy_algorithms: self.is_on(ProfileToggle::LegacyAlgorithms),
            }),
            DraftProtocol::Rdp => DraftProfile::Rdp(RdpProfile {
                id,
                name,
                group,
                host,
                port,
                username: optional(username),
                domain: optional(domain),
                allow_tls_only: !self.is_on(ProfileToggle::Nla),
                gateway: self.routed_gateway(),
                redirect_clipboard: self.is_on(ProfileToggle::RedirectClipboard),
                redirect_drives: self.is_on(ProfileToggle::RedirectDrives),
                options: self.saved_rdp_options()?,
                vault_entry: optional(vault_entry),
                forwards: self.saved_forwards()?,
                follow_defaults: self.is_on(ProfileToggle::FollowDefaults),
                several_servers: self.is_on(ProfileToggle::SeveralServers),
                anti_idle: self.is_on(ProfileToggle::AntiIdle),
            }),
            DraftProtocol::Vnc => DraftProfile::Vnc(VncProfile {
                id,
                name,
                group,
                host,
                port,
                view_only: self.is_on(ProfileToggle::ViewOnly),
                allow_no_password: self.is_on(ProfileToggle::AllowNoPassword),
                vault_entry: optional(vault_entry),
            }),
            DraftProtocol::WinRm => {
                let stored = self.is_on(ProfileToggle::StoredCredential);
                if stored && username.is_empty() {
                    return Err(DraftError::UsernameMissing);
                }
                let use_ssl = self.is_on(ProfileToggle::UseSsl);
                DraftProfile::WinRm(WinRmProfile {
                    id,
                    name,
                    group,
                    host,
                    port,
                    use_ssl,
                    skip_certificate_check: use_ssl
                        && self.is_on(ProfileToggle::SkipCertificateCheck),
                    username: stored.then(|| username.to_owned()),
                })
            }
            DraftProtocol::Local => self.saved_local(id, name, group)?,
            DraftProtocol::Ftp => DraftProfile::Ftp(FtpProfile {
                id,
                name,
                group,
                host,
                port,
                username: optional(username),
                passive: self.is_on(ProfileToggle::Passive),
                tls: self.is_on(ProfileToggle::Tls),
                vault_entry: optional(vault_entry),
            }),
            DraftProtocol::Telnet => DraftProfile::Telnet(TelnetProfile {
                id,
                name,
                group,
                host,
                port,
            }),
        })
    }

    /// The text of `field`.
    #[must_use]
    pub fn value(&self, field: ProfileField) -> &str {
        match field {
            ProfileField::Name => &self.name,
            ProfileField::Group => &self.group,
            ProfileField::Host => &self.host,
            ProfileField::Port => &self.port,
            ProfileField::Username => &self.username,
            ProfileField::KeyPath => &self.key_path,
            ProfileField::Domain => &self.domain,
            ProfileField::FixedWidth => &self.fixed_width,
            ProfileField::FixedHeight => &self.fixed_height,
            ProfileField::VaultEntry => &self.vault_entry,
            ProfileField::SocksPort => &self.socks_port,
            ProfileField::RemoteBindPort => &self.remote_bind_port,
            ProfileField::RemoteLocalPort => &self.remote_local_port,
            ProfileField::LocalProgram => &self.local_program,
            ProfileField::LocalArguments => &self.local_arguments,
            ProfileField::WorkingDirectory => &self.working_directory,
        }
    }

    /// Replaces the text of `field`.
    pub fn set(&mut self, field: ProfileField, value: String) {
        *match field {
            ProfileField::Name => &mut self.name,
            ProfileField::Group => &mut self.group,
            ProfileField::Host => &mut self.host,
            ProfileField::Port => &mut self.port,
            ProfileField::Username => &mut self.username,
            ProfileField::KeyPath => &mut self.key_path,
            ProfileField::Domain => &mut self.domain,
            ProfileField::FixedWidth => &mut self.fixed_width,
            ProfileField::FixedHeight => &mut self.fixed_height,
            ProfileField::VaultEntry => &mut self.vault_entry,
            ProfileField::SocksPort => &mut self.socks_port,
            ProfileField::RemoteBindPort => &mut self.remote_bind_port,
            ProfileField::RemoteLocalPort => &mut self.remote_local_port,
            ProfileField::LocalProgram => &mut self.local_program,
            ProfileField::LocalArguments => &mut self.local_arguments,
            ProfileField::WorkingDirectory => &mut self.working_directory,
        } = value;
    }

    /// The profile this form describes, under `id`.
    ///
    /// # Errors
    ///
    /// The first [`DraftError`] in form order.
    pub fn to_profile(&self, id: ProfileId) -> Result<SshProfile, DraftError> {
        let name = self.name.trim();
        if name.is_empty() {
            return Err(DraftError::NameMissing);
        }
        let group = self.group.trim();
        let key_path = self.key_path.trim();
        if [name, group, key_path]
            .iter()
            .any(|text| text.chars().any(char::is_control))
        {
            return Err(DraftError::ControlCharacter);
        }
        let host = host(&self.host)?;
        let port = match self.port.trim() {
            "" => DEFAULT_SSH_PORT,
            typed => typed
                .parse::<u16>()
                .ok()
                .filter(|port| *port != 0)
                .ok_or(DraftError::PortInvalid)?,
        };
        let username = self.username.trim();
        if username
            .chars()
            .any(|c| c.is_whitespace() || c.is_control())
        {
            return Err(DraftError::UsernameInvalid);
        }
        let optional = |text: &str| (!text.is_empty()).then(|| text.to_owned());
        Ok(SshProfile {
            id,
            name: name.to_owned(),
            group: optional(group),
            host,
            port,
            username: optional(username),
            key_path: optional(key_path).map(PathBuf::from),
            gateway: self.gateway.clone(),
            vault_entry: optional(self.vault_entry.trim()),
            forwards: self.saved_forwards()?,
            post_connect: self.saved_post_connect(),
            forward_agent: self.is_on(ProfileToggle::ForwardAgent),
            compression: self.is_on(ProfileToggle::Compression),
            sftp: false,
            legacy_algorithms: self.is_on(ProfileToggle::LegacyAlgorithms),
        })
    }
}

/// A port as the form shows it: empty for none.
fn port_text(port: Option<u16>) -> String {
    port.map(|port| port.to_string()).unwrap_or_default()
}

/// The address as saved: trimmed, and an IPv6 address without the brackets it may have
/// been typed with.
pub(crate) fn host(typed: &str) -> Result<String, DraftError> {
    let typed = typed.trim();
    if typed.is_empty() {
        return Err(DraftError::HostMissing);
    }
    if typed.chars().any(|c| c.is_whitespace() || c.is_control()) {
        return Err(DraftError::HostInvalid);
    }
    if typed.contains('@') {
        return Err(DraftError::HostHasUser);
    }
    let bare = typed
        .strip_prefix('[')
        .and_then(|inner| inner.strip_suffix(']'))
        .unwrap_or(typed);
    // One colon is a port; an IPv6 address has at least two.
    match bare.matches(':').count() {
        0 => Ok(bare.to_owned()),
        1 => Err(DraftError::HostHasPort),
        _ if bare.contains(['[', ']']) => Err(DraftError::HostHasPort),
        _ => Ok(bare.to_owned()),
    }
}

/// An identifier none of `taken` holds.
#[must_use]
pub fn new_id(taken: &[SshProfile]) -> ProfileId {
    // Nanoseconds since the epoch, mixed with a per-process random key: unique across
    // processes in practice, and checked against what is saved here.
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_nanos());
    let mut seed = RandomState::new().hash_one(nanos);
    loop {
        let id = ProfileId::new(format!("{ID_PREFIX}{seed:016x}"));
        if !taken.iter().any(|profile| profile.id == id) {
            return id;
        }
        seed = seed.wrapping_add(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn draft(host: &str, port: &str) -> ProfileDraft {
        ProfileDraft {
            name: "web".to_owned(),
            host: host.to_owned(),
            port: port.to_owned(),
            ..ProfileDraft::default()
        }
    }

    fn id() -> ProfileId {
        ProfileId::new("x")
    }

    #[test]
    fn a_full_form_becomes_a_trimmed_profile() {
        let form = ProfileDraft {
            editing: None,
            name: "  web  ".to_owned(),
            group: " Prod ".to_owned(),
            host: " web.example.org ".to_owned(),
            port: " 2222 ".to_owned(),
            username: " admin ".to_owned(),
            key_path: " /home/me/.ssh/id_ed25519 ".to_owned(),
            ..ProfileDraft::default()
        };
        let profile = form.to_profile(id()).expect("valid");
        assert_eq!(profile.name, "web");
        assert_eq!(profile.group.as_deref(), Some("Prod"));
        assert_eq!(profile.host, "web.example.org");
        assert_eq!(profile.port, 2222);
        assert_eq!(profile.username.as_deref(), Some("admin"));
        assert_eq!(
            profile.key_path,
            Some(PathBuf::from("/home/me/.ssh/id_ed25519"))
        );
        assert_eq!(
            ProfileDraft::from_profile(&profile).to_profile(id()),
            Ok(profile)
        );
    }

    #[test]
    fn empty_optional_fields_are_absent_and_the_port_defaults() {
        let profile = draft("host", "").to_profile(id()).expect("valid");
        assert_eq!(profile.port, DEFAULT_SSH_PORT);
        assert_eq!(
            (profile.group, profile.username, profile.key_path),
            (None, None, None)
        );
    }

    #[test]
    fn addresses_are_checked_and_ipv6_loses_its_brackets() {
        let host = |typed: &str| draft(typed, "").to_profile(id()).map(|p| p.host);
        assert_eq!(host("[2001:db8::1]"), Ok("2001:db8::1".to_owned()));
        assert_eq!(host("2001:db8::1"), Ok("2001:db8::1".to_owned()));
        assert_eq!(host("10.0.0.1"), Ok("10.0.0.1".to_owned()));
        assert_eq!(host("  "), Err(DraftError::HostMissing));
        assert_eq!(host("a b"), Err(DraftError::HostInvalid));
        assert_eq!(host("root@web"), Err(DraftError::HostHasUser));
        assert_eq!(host("web:2222"), Err(DraftError::HostHasPort));
        assert_eq!(host("[2001:db8::1]:22"), Err(DraftError::HostHasPort));
    }

    #[test]
    fn ports_are_one_to_65535() {
        let port = |typed: &str| draft("h", typed).to_profile(id()).map(|p| p.port);
        assert_eq!(port("1"), Ok(1));
        assert_eq!(port("65535"), Ok(65535));
        for bad in ["0", "65536", "-1", "22a", "2 2"] {
            assert_eq!(port(bad), Err(DraftError::PortInvalid), "{bad}");
        }
    }

    #[test]
    fn names_users_and_control_characters() {
        let mut form = draft("h", "");
        form.name = " ".to_owned();
        assert_eq!(form.to_profile(id()), Err(DraftError::NameMissing));
        form.name = "a\tb".to_owned();
        assert_eq!(form.to_profile(id()), Err(DraftError::ControlCharacter));
        form.name = "ok".to_owned();
        form.username = "two words".to_owned();
        assert_eq!(form.to_profile(id()), Err(DraftError::UsernameInvalid));
    }

    #[test]
    fn a_new_id_avoids_every_taken_one() {
        let first = new_id(&[]);
        assert!(first.as_str().starts_with(ID_PREFIX));
        let taken = SshProfile {
            id: first.clone(),
            name: "a".to_owned(),
            group: None,
            host: "h".to_owned(),
            port: 22,
            username: None,
            key_path: None,
            gateway: None,
            vault_entry: None,
            forwards: heimdall_core::profile::Forwards::default(),
            post_connect: heimdall_core::post_connect::PostConnect::default(),
            forward_agent: false,
            compression: false,
            sftp: false,
            legacy_algorithms: false,
        };
        let second = new_id(std::slice::from_ref(&taken));
        assert_ne!(second, first);
    }

    #[test]
    fn a_new_session_starts_as_the_csharp_one() {
        let rdp = ProfileDraft::new_for(DraftProtocol::Rdp);
        assert_eq!(rdp.port, "3389");
        assert!(rdp.is_on(ProfileToggle::RedirectClipboard));
        assert!(
            !rdp.is_on(ProfileToggle::RedirectDrives),
            "drives kept unless shared"
        );
        assert!(rdp.is_on(ProfileToggle::Nla), "NLA required unless cleared");
        assert!(
            !rdp.is_on(ProfileToggle::AdminSession),
            "the ordinary session unless asked"
        );
        assert!(
            !rdp.is_on(ProfileToggle::SeveralServers),
            "one machine per address unless said"
        );
        assert!(!rdp.is_on(ProfileToggle::AntiIdle), "off, as the C#");
        assert_eq!(rdp.rdp_options, RdpOptions::default());
        assert_eq!(
            ProfileToggle::of(DraftProtocol::Rdp),
            [
                ProfileToggle::RedirectClipboard,
                ProfileToggle::RedirectDrives,
                ProfileToggle::AntiIdle,
                ProfileToggle::Nla,
                ProfileToggle::AdminSession,
                // Not in the C# dialog, which always asks: after its boxes.
                ProfileToggle::SeveralServers,
            ],
            "in the C# dialog's order"
        );
        for (protocol, port) in [
            (DraftProtocol::Ssh, "22"),
            (DraftProtocol::WinRm, "5985"),
            (DraftProtocol::Vnc, "5900"),
            (DraftProtocol::Telnet, "23"),
        ] {
            assert_eq!(ProfileDraft::new_for(protocol).port, port, "{protocol:?}");
        }
    }

    #[test]
    fn a_socks_port_is_shown_and_checked_only_through_a_gateway() {
        let socks = |draft: &ProfileDraft, typed: &str| {
            let mut draft = draft.clone();
            draft.set(ProfileField::SocksPort, typed.to_owned());
            draft.to_saved(id()).map(|saved| match saved {
                DraftProfile::Ssh(profile) => profile.forwards.socks_port,
                DraftProfile::Rdp(profile) => profile.forwards.socks_port,
                other => panic!("{other:?}"),
            })
        };
        let mut draft = ProfileDraft::new_for(DraftProtocol::Ssh);
        draft.set(ProfileField::Name, "web".to_owned());
        draft.set(ProfileField::Host, "web.lab".to_owned());
        assert!(!draft.shows(ProfileField::SocksPort), "no gateway");
        // Hidden, a port that reads is kept, as the C# keeps it; one that does not is dropped.
        assert_eq!(socks(&draft, "1080"), Ok(Some(1080)));
        assert_eq!(socks(&draft, "proxy"), Ok(None));

        draft.gateway = Some(ProfileId::new("gw"));
        assert!(draft.shows(ProfileField::SocksPort));
        assert_eq!(socks(&draft, " 1080 "), Ok(Some(1080)));
        assert_eq!(socks(&draft, ""), Ok(None));
        assert_eq!(socks(&draft, "0"), Ok(None), "0 opens none, as in C#");
        assert_eq!(socks(&draft, "65535"), Ok(Some(65535)));
        for typed in ["65536", "-1", "proxy"] {
            assert_eq!(
                socks(&draft, typed),
                Err(DraftError::SocksPortInvalid),
                "{typed}"
            );
        }
        assert_eq!(
            DraftError::SocksPortInvalid.field(),
            ProfileField::SocksPort
        );

        draft.toggle(ProfileToggle::DirectConnection, true);
        assert!(!draft.shows(ProfileField::SocksPort), "connecting directly");

        let mut rdp = ProfileDraft::new_for(DraftProtocol::Rdp);
        rdp.gateway = Some(ProfileId::new("gw"));
        assert!(rdp.shows(ProfileField::SocksPort));
        let mut telnet = ProfileDraft::new_for(DraftProtocol::Telnet);
        telnet.gateway = Some(ProfileId::new("gw"));
        assert!(
            !telnet.shows(ProfileField::SocksPort),
            "never through a gateway"
        );
    }

    #[test]
    fn an_ftp_form_is_passive_by_default_and_reads_back() {
        let mut draft = ProfileDraft::new_for(DraftProtocol::Ftp);
        assert_eq!(draft.port, "21");
        assert!(
            draft.is_on(ProfileToggle::Passive),
            "as a new C# FTP profile"
        );
        assert_eq!(
            ProfileToggle::of(DraftProtocol::Ftp),
            [ProfileToggle::Passive, ProfileToggle::Tls]
        );
        for field in [ProfileField::Username, ProfileField::VaultEntry] {
            assert!(draft.shows(field), "{field:?}");
        }
        for field in [ProfileField::KeyPath, ProfileField::SocksPort] {
            assert!(!draft.shows(field), "{field:?}");
        }
        draft.set(ProfileField::Name, "files".to_owned());
        draft.set(ProfileField::Host, "ftp.lab".to_owned());
        let Ok(DraftProfile::Ftp(anonymous)) = draft.to_saved(id()) else {
            panic!("an FTP profile");
        };
        assert_eq!(anonymous.username, None, "blank: anonymous");
        draft.set(ProfileField::Username, "ops".to_owned());
        draft.toggle(ProfileToggle::Passive, false);
        draft.toggle(ProfileToggle::Tls, true);
        let Ok(DraftProfile::Ftp(profile)) = draft.to_saved(id()) else {
            panic!("an FTP profile");
        };
        assert_eq!(
            (profile.username.as_deref(), profile.passive, profile.tls),
            (Some("ops"), false, true)
        );
        assert_eq!(
            ProfileDraft::from_ftp(&profile).to_saved(id()),
            Ok(DraftProfile::Ftp(profile))
        );
    }

    #[test]
    fn a_local_form_has_no_server_and_saves_its_command() {
        let mut draft = ProfileDraft::new_for(DraftProtocol::Local);
        for field in [
            ProfileField::Name,
            ProfileField::Group,
            ProfileField::LocalProgram,
            ProfileField::LocalArguments,
            ProfileField::WorkingDirectory,
        ] {
            assert!(draft.shows(field), "{field:?}");
        }
        for field in [
            ProfileField::Host,
            ProfileField::Port,
            ProfileField::Username,
            ProfileField::VaultEntry,
        ] {
            assert!(!draft.shows(field), "{field:?}");
        }
        assert!(!DraftProtocol::Ssh.shows(ProfileField::LocalProgram));
        draft.set(ProfileField::Name, "Tool".to_owned());
        draft.set(ProfileField::Group, "Admin".to_owned());
        draft.set(ProfileField::LocalProgram, " bash ".to_owned());
        draft.set(ProfileField::WorkingDirectory, "/srv".to_owned());
        let Ok(DraftProfile::Local(profile)) = draft.to_saved(id()) else {
            panic!("a local profile");
        };
        assert_eq!(profile.command.program.as_deref(), Some("bash"));
        assert_eq!(
            profile.command.working_directory,
            Some(PathBuf::from("/srv"))
        );
        assert_eq!(profile.group.as_deref(), Some("Admin"));
        assert!(
            profile.approved.is_none(),
            "approved by the store, not here"
        );
        let reread = ProfileDraft::from_local(&profile);
        assert_eq!(
            (
                reread.protocol,
                reread.local_program.as_str(),
                reread.working_directory.as_str()
            ),
            (DraftProtocol::Local, "bash", "/srv")
        );
        // Empty: the default shell, in the current folder.
        draft.set(ProfileField::LocalProgram, String::new());
        draft.set(ProfileField::WorkingDirectory, String::new());
        let Ok(DraftProfile::Local(profile)) = draft.to_saved(id()) else {
            panic!("a local profile");
        };
        assert!(profile.command.is_default());
        draft.set(ProfileField::LocalProgram, "ba\u{7}sh".to_owned());
        assert_eq!(draft.to_saved(id()), Err(DraftError::ControlCharacter));
        assert_eq!(
            DraftError::ArgumentsInvalid.field(),
            ProfileField::LocalArguments
        );
    }

    #[cfg(not(windows))]
    #[test]
    fn a_local_argument_line_with_a_quote_left_open_is_refused() {
        let mut draft = ProfileDraft::new_for(DraftProtocol::Local);
        draft.set(ProfileField::Name, "Tool".to_owned());
        draft.set(ProfileField::LocalArguments, "-c 'echo".to_owned());
        assert_eq!(draft.to_saved(id()), Err(DraftError::ArgumentsInvalid));
        draft.set(ProfileField::LocalArguments, "-c 'echo a'".to_owned());
        let Ok(DraftProfile::Local(profile)) = draft.to_saved(id()) else {
            panic!("a local profile");
        };
        assert_eq!(
            profile.command.arguments,
            heimdall_core::profile::LocalArguments::List(vec![
                "-c".to_owned(),
                "echo a".to_owned()
            ])
        );
    }

    #[test]
    fn an_sftp_form_saves_an_ssh_profile_that_opens_its_files() {
        assert_eq!(
            DraftProtocol::ALL,
            [
                DraftProtocol::Rdp,
                DraftProtocol::Ssh,
                DraftProtocol::WinRm,
                DraftProtocol::Sftp,
                DraftProtocol::Vnc,
                DraftProtocol::Telnet,
                DraftProtocol::Ftp,
                DraftProtocol::Local,
            ],
            "the C# picker's order, Citrix aside"
        );
        let mut draft = ProfileDraft::new_for(DraftProtocol::Sftp);
        assert_eq!(draft.port, "22");
        for field in [
            ProfileField::Username,
            ProfileField::KeyPath,
            ProfileField::VaultEntry,
        ] {
            assert!(draft.shows(field), "{field:?}");
        }
        assert_eq!(
            ProfileToggle::of(DraftProtocol::Sftp),
            [ProfileToggle::Compression, ProfileToggle::LegacyAlgorithms],
            "no shell to forward the agent to"
        );
        assert!(DraftProtocol::Sftp.routes_through_gateway());
        draft.set(ProfileField::Name, "files".to_owned());
        draft.set(ProfileField::Host, "files.lab".to_owned());
        draft.set(ProfileField::Username, "ops".to_owned());
        draft.toggle(ProfileToggle::Compression, true);
        draft.toggle(ProfileToggle::ForwardAgent, true);
        let Ok(DraftProfile::Ssh(profile)) = draft.to_saved(id()) else {
            panic!("an SSH profile");
        };
        assert!(profile.sftp && profile.compression);
        assert!(!profile.forward_agent, "never for SFTP, even ticked");
        let reread = ProfileDraft::from_profile(&profile);
        assert_eq!(reread.protocol, DraftProtocol::Sftp);
        assert!(reread.is_on(ProfileToggle::Compression));
        let mut shell = ProfileDraft::new_for(DraftProtocol::Ssh);
        shell.set(ProfileField::Name, "web".to_owned());
        shell.set(ProfileField::Host, "web.lab".to_owned());
        let Ok(DraftProfile::Ssh(profile)) = shell.to_saved(id()) else {
            panic!("an SSH profile");
        };
        assert!(!profile.sftp);
    }

    #[test]
    fn forwarding_the_agent_is_an_ssh_option_that_reads_back() {
        assert_eq!(
            ProfileToggle::of(DraftProtocol::Ssh),
            [
                ProfileToggle::Compression,
                ProfileToggle::ForwardAgent,
                // Not in the C# dialog, which always offers them: after its boxes.
                ProfileToggle::LegacyAlgorithms
            ],
            "in the C# order"
        );
        for protocol in [
            DraftProtocol::Rdp,
            DraftProtocol::Vnc,
            DraftProtocol::WinRm,
            DraftProtocol::Telnet,
        ] {
            assert!(!ProfileToggle::of(protocol).contains(&ProfileToggle::ForwardAgent));
        }
        let mut draft = ProfileDraft::new_for(DraftProtocol::Ssh);
        draft.set(ProfileField::Name, "web".to_owned());
        draft.set(ProfileField::Host, "web.lab".to_owned());
        let saved = |draft: &ProfileDraft| match draft.to_saved(id()) {
            Ok(DraftProfile::Ssh(profile)) => profile,
            other => panic!("{other:?}"),
        };
        assert!(!saved(&draft).forward_agent, "off by default, as in C#");
        draft.toggle(ProfileToggle::ForwardAgent, true);
        let profile = saved(&draft);
        assert!(profile.forward_agent);
        assert!(ProfileDraft::from_profile(&profile).is_on(ProfileToggle::ForwardAgent));
        assert!(draft.to_profile(id()).expect("profile").forward_agent);
        assert!(!profile.compression);
        draft.toggle(ProfileToggle::Compression, true);
        let profile = saved(&draft);
        assert!(profile.compression && profile.forward_agent);
        let reread = ProfileDraft::from_profile(&profile);
        assert!(
            reread.is_on(ProfileToggle::Compression) && reread.is_on(ProfileToggle::ForwardAgent)
        );
        assert!(draft.to_profile(id()).expect("profile").compression);
    }

    #[test]
    fn a_remote_forward_is_read_as_the_csharp_one_and_checked_only_through_a_gateway() {
        let remote = |draft: &ProfileDraft, bind: &str, local: &str| {
            let mut draft = draft.clone();
            draft.set(ProfileField::RemoteBindPort, bind.to_owned());
            draft.set(ProfileField::RemoteLocalPort, local.to_owned());
            draft.to_saved(id()).map(|saved| match saved {
                DraftProfile::Ssh(profile) => profile.forwards.remote(),
                other => panic!("{other:?}"),
            })
        };
        let mut draft = ProfileDraft::new_for(DraftProtocol::Ssh);
        draft.set(ProfileField::Name, "web".to_owned());
        draft.set(ProfileField::Host, "web.lab".to_owned());
        for field in [ProfileField::RemoteBindPort, ProfileField::RemoteLocalPort] {
            assert!(!draft.shows(field), "no gateway");
        }
        assert_eq!(remote(&draft, "8080", "x"), Ok(Some((8080, 8080))));

        draft.gateway = Some(ProfileId::new("gw"));
        for field in [ProfileField::RemoteBindPort, ProfileField::RemoteLocalPort] {
            assert!(draft.shows(field));
        }
        assert_eq!(remote(&draft, "", ""), Ok(None));
        assert_eq!(remote(&draft, "0", "3000"), Ok(None), "0 opens none");
        assert_eq!(remote(&draft, "8080", ""), Ok(Some((8080, 8080))));
        assert_eq!(
            remote(&draft, "8080", "0"),
            Ok(Some((8080, 8080))),
            "0 is the same"
        );
        assert_eq!(remote(&draft, " 8080 ", "3000"), Ok(Some((8080, 3000))));
        assert_eq!(
            remote(&draft, "70000", ""),
            Err(DraftError::RemoteBindPortInvalid)
        );
        assert_eq!(
            remote(&draft, "8080", "local"),
            Err(DraftError::RemoteLocalPortInvalid)
        );
        assert_eq!(
            DraftError::RemoteBindPortInvalid.field(),
            ProfileField::RemoteBindPort
        );
        assert_eq!(
            DraftError::RemoteLocalPortInvalid.field(),
            ProfileField::RemoteLocalPort
        );
        let mut saved = draft.clone();
        saved.set(ProfileField::RemoteBindPort, "8080".to_owned());
        saved.set(ProfileField::RemoteLocalPort, "3000".to_owned());
        let Ok(DraftProfile::Ssh(profile)) = saved.to_saved(id()) else {
            panic!("saved");
        };
        let reread = ProfileDraft::from_profile(&profile);
        assert_eq!(
            (
                reread.remote_bind_port.as_str(),
                reread.remote_local_port.as_str()
            ),
            ("8080", "3000")
        );
    }

    #[test]
    fn every_protocol_reads_back_from_its_form() {
        let rdp = RdpProfile {
            id: id(),
            name: "dc".to_owned(),
            group: Some("Win".to_owned()),
            host: "dc.lab".to_owned(),
            port: 3390,
            username: Some("admin".to_owned()),
            domain: Some("CORP".to_owned()),
            allow_tls_only: true,
            gateway: Some(ProfileId::new("gw")),
            redirect_clipboard: false,
            redirect_drives: false,
            options: heimdall_core::profile::RdpOptions::default(),
            vault_entry: None,
            forwards: Forwards {
                socks_port: Some(1080),
                ..Forwards::default()
            },
            follow_defaults: false,
            several_servers: true,
            anti_idle: true,
        };
        assert_eq!(
            ProfileDraft::from_rdp(&rdp).to_saved(id()),
            Ok(DraftProfile::Rdp(rdp.clone()))
        );
        let nla = RdpProfile {
            allow_tls_only: false,
            redirect_clipboard: true,
            redirect_drives: false,
            options: heimdall_core::profile::RdpOptions::default(),
            ..rdp
        };
        assert_eq!(
            ProfileDraft::from_rdp(&nla).to_saved(id()),
            Ok(DraftProfile::Rdp(nla))
        );
        let vnc = VncProfile {
            id: id(),
            name: "kiosk".to_owned(),
            group: None,
            host: "kiosk.lab".to_owned(),
            port: 5901,
            view_only: true,
            allow_no_password: true,
            vault_entry: None,
        };
        assert_eq!(
            ProfileDraft::from_vnc(&vnc).to_saved(id()),
            Ok(DraftProfile::Vnc(vnc))
        );
        let winrm = WinRmProfile {
            id: id(),
            name: "ps".to_owned(),
            group: None,
            host: "ps.lab".to_owned(),
            port: 5986,
            use_ssl: true,
            skip_certificate_check: true,
            username: Some("LAB\\admin".to_owned()),
        };
        assert_eq!(
            ProfileDraft::from_winrm(&winrm).to_saved(id()),
            Ok(DraftProfile::WinRm(winrm))
        );
        let telnet = TelnetProfile {
            id: id(),
            name: "sw".to_owned(),
            group: None,
            host: "sw.lab".to_owned(),
            port: 2323,
        };
        assert_eq!(
            ProfileDraft::from_telnet(&telnet).to_saved(id()),
            Ok(DraftProfile::Telnet(telnet))
        );
    }

    #[test]
    fn use_ssl_moves_a_default_winrm_port_and_keeps_a_typed_one() {
        let mut form = ProfileDraft::new_for(DraftProtocol::WinRm);
        form.toggle(ProfileToggle::UseSsl, true);
        assert_eq!(form.port, "5986");
        form.toggle(ProfileToggle::UseSsl, false);
        assert_eq!(form.port, "5985");
        form.port = "6000".to_owned();
        form.toggle(ProfileToggle::UseSsl, true);
        assert_eq!(form.port, "6000", "a port typed by hand stays");
    }

    #[test]
    fn a_winrm_account_shows_and_counts_only_for_a_stored_credential() {
        let mut form = ProfileDraft::new_for(DraftProtocol::WinRm);
        form.name = "ps".to_owned();
        form.host = "ps.lab".to_owned();
        form.username = "ignored".to_owned();
        assert!(!form.shows(ProfileField::Username));
        let Ok(DraftProfile::WinRm(current)) = form.to_saved(id()) else {
            panic!("winrm");
        };
        assert_eq!(current.username, None, "the current Windows identity");
        form.toggle(ProfileToggle::StoredCredential, true);
        form.username = String::new();
        assert!(form.shows(ProfileField::Username));
        assert_eq!(form.to_saved(id()), Err(DraftError::UsernameMissing));
        form.username = "a\"b".to_owned();
        assert_eq!(form.to_saved(id()), Err(DraftError::UsernameInvalid));
        // Skipping certificate checks only over HTTPS.
        form.username = "admin".to_owned();
        form.toggle(ProfileToggle::SkipCertificateCheck, true);
        assert!(!form.shows_toggle(ProfileToggle::SkipCertificateCheck));
        let Ok(DraftProfile::WinRm(http)) = form.to_saved(id()) else {
            panic!("winrm");
        };
        assert!(!http.skip_certificate_check);
    }

    #[test]
    fn a_domain_is_checked_and_only_rdp_shows_one() {
        let mut form = ProfileDraft::new_for(DraftProtocol::Rdp);
        form.name = "dc".to_owned();
        form.host = "dc.lab".to_owned();
        form.domain = "CO RP".to_owned();
        assert_eq!(form.to_saved(id()), Err(DraftError::DomainInvalid));
        assert_eq!(DraftError::DomainInvalid.field(), ProfileField::Domain);
        assert!(form.shows(ProfileField::Domain));
        assert!(!ProfileDraft::new_for(DraftProtocol::Ssh).shows(ProfileField::Domain));
        assert!(!ProfileDraft::new_for(DraftProtocol::Vnc).shows(ProfileField::Username));
    }

    #[test]
    fn the_experience_boxes_ticked_are_saved_with_the_flags_no_box_shows() {
        let mut form = ProfileDraft::new_for(DraftProtocol::Rdp);
        form.name = "dc".to_owned();
        form.host = "dc.lab".to_owned();
        // As a C# .rdp import may bring it.
        form.rdp_options.performance_flags = 0x40;
        form.choose(ProfileChoice::Experience(
            Experience::DisableWallpaper,
            true,
        ));
        form.choose(ProfileChoice::Experience(
            Experience::EnableComposition,
            true,
        ));
        form.choose(ProfileChoice::Experience(
            Experience::DisableWallpaper,
            false,
        ));
        let Ok(DraftProfile::Rdp(saved)) = form.to_saved(id()) else {
            panic!("rdp");
        };
        assert_eq!(saved.options.performance_flags, 0x40 | 0x100);
        assert_eq!(
            ProfileDraft::from_rdp(&saved).rdp_options.performance_flags,
            0x140,
            "read back"
        );
    }
}
