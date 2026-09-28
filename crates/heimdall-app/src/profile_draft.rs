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

use heimdall_core::profile::{
    AudioPlayback, ColorDepth, DEFAULT_RDP_PORT, DEFAULT_TELNET_PORT, DEFAULT_VNC_PORT,
    DEFAULT_WINRM_HTTP_PORT, DEFAULT_WINRM_HTTPS_PORT, FIXED_HEIGHT_MAX, FIXED_SIDE_MIN,
    FIXED_WIDTH_MAX, ProfileId, RdpOptions, RdpProfile, Resolution, SshProfile, TelnetProfile,
    VncProfile, WinRmProfile, fixed_desktop,
};

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
}

impl ProfileField {
    /// Every field, in form order.
    pub const ALL: [Self; 9] = [
        Self::Name,
        Self::Group,
        Self::Host,
        Self::Port,
        Self::Username,
        Self::Domain,
        Self::KeyPath,
        Self::FixedWidth,
        Self::FixedHeight,
    ];
}

/// The protocol a form is for: every protocol the editor knows.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum DraftProtocol {
    /// SSH, with SFTP for its files.
    #[default]
    Ssh,
    /// Remote desktop.
    Rdp,
    /// VNC desktop.
    Vnc,
    /// Remote `PowerShell`.
    WinRm,
    /// Telnet terminal.
    Telnet,
}

impl DraftProtocol {
    /// Every protocol, in the order the picker shows them.
    pub const ALL: [Self; 5] = [Self::Rdp, Self::Ssh, Self::WinRm, Self::Vnc, Self::Telnet];

    /// Whether a form for this protocol shows `field`.
    #[must_use]
    pub fn shows(self, field: ProfileField) -> bool {
        match field {
            ProfileField::Name | ProfileField::Group | ProfileField::Host | ProfileField::Port => {
                true
            }
            ProfileField::Username => matches!(self, Self::Ssh | Self::Rdp | Self::WinRm),
            ProfileField::Domain | ProfileField::FixedWidth | ProfileField::FixedHeight => {
                self == Self::Rdp
            }
            ProfileField::KeyPath => self == Self::Ssh,
        }
    }

    /// Whether a password can be saved with this protocol's profiles. A `WinRM` password is
    /// typed into `PowerShell`, which asks for it.
    #[must_use]
    pub fn saves_password(self) -> bool {
        matches!(self, Self::Ssh | Self::Rdp | Self::Vnc)
    }

    /// Whether this protocol's sessions can go through an SSH gateway.
    #[must_use]
    pub fn routes_through_gateway(self) -> bool {
        matches!(self, Self::Ssh | Self::Rdp)
    }

    /// Whether a saved password belongs to an account, which the form must then name.
    #[must_use]
    pub fn password_needs_username(self) -> bool {
        matches!(self, Self::Ssh | Self::Rdp)
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
}

impl ProfileToggle {
    /// The options of `protocol`'s form, in form order.
    #[must_use]
    pub fn of(protocol: DraftProtocol) -> &'static [Self] {
        match protocol {
            DraftProtocol::Rdp => &[
                Self::RedirectClipboard,
                Self::RedirectDrives,
                Self::Nla,
                Self::AdminSession,
            ],
            DraftProtocol::WinRm => &[
                Self::StoredCredential,
                Self::UseSsl,
                Self::SkipCertificateCheck,
            ],
            DraftProtocol::Vnc => &[Self::ViewOnly, Self::AllowNoPassword],
            DraftProtocol::Ssh | DraftProtocol::Telnet => &[],
        }
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
    /// RDP: the options chosen from lists and boxes of their own; the administrative session
    /// is a toggle, the fixed size is typed in `fixed_width` and `fixed_height`.
    pub rdp_options: RdpOptions,
    /// RDP: width of a fixed desktop, as typed.
    pub fixed_width: String,
    /// RDP: height of a fixed desktop, as typed.
    pub fixed_height: String,
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
            protocol: DraftProtocol::Ssh,
            protocol_chosen: true,
            ..Self::default()
        }
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
        Self {
            editing: Some(profile.id.clone()),
            name: profile.name.clone(),
            group: profile.group.clone().unwrap_or_default(),
            host: profile.host.clone(),
            port: profile.port.to_string(),
            username: profile.username.clone().unwrap_or_default(),
            domain: profile.domain.clone().unwrap_or_default(),
            gateway: profile.gateway.clone(),
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
                DraftProtocol::Rdp => vec![ProfileToggle::RedirectClipboard, ProfileToggle::Nla],
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
    /// fixed size only in the fixed mode.
    #[must_use]
    pub fn shows(&self, field: ProfileField) -> bool {
        let fixed_size = matches!(field, ProfileField::FixedWidth | ProfileField::FixedHeight);
        self.protocol.shows(field)
            && !(self.protocol == DraftProtocol::WinRm
                && field == ProfileField::Username
                && !self.is_on(ProfileToggle::StoredCredential))
            && !(fixed_size && self.rdp_options.resolution != Resolution::Fixed)
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

    /// The port an empty field stands for.
    #[must_use]
    pub fn default_port(&self) -> u16 {
        match self.protocol {
            DraftProtocol::Ssh => DEFAULT_SSH_PORT,
            DraftProtocol::Rdp => DEFAULT_RDP_PORT,
            DraftProtocol::Vnc => DEFAULT_VNC_PORT,
            DraftProtocol::Telnet => DEFAULT_TELNET_PORT,
            DraftProtocol::WinRm if self.is_on(ProfileToggle::UseSsl) => DEFAULT_WINRM_HTTPS_PORT,
            DraftProtocol::WinRm => DEFAULT_WINRM_HTTP_PORT,
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
        if [name, group, key_path, domain]
            .iter()
            .any(|text| text.chars().any(char::is_control))
        {
            return Err(DraftError::ControlCharacter);
        }
        let host = host(&self.host)?;
        let port = match self.port.trim() {
            "" => self.default_port(),
            typed => typed
                .parse::<u16>()
                .ok()
                .filter(|port| *port != 0)
                .ok_or(DraftError::PortInvalid)?,
        };
        let username = if self.shows(ProfileField::Username) {
            self.username.trim()
        } else {
            ""
        };
        // A double quote is refused too: a WinRM account is written into a command, and no
        // account name holds one.
        if username
            .chars()
            .any(|c| c.is_whitespace() || c.is_control() || c == '"')
        {
            return Err(DraftError::UsernameInvalid);
        }
        if domain.chars().any(|c| c.is_whitespace() || c == '"') {
            return Err(DraftError::DomainInvalid);
        }
        let optional = |text: &str| (!text.is_empty()).then(|| text.to_owned());
        let group = optional(group);
        let name = name.to_owned();
        Ok(match self.protocol {
            DraftProtocol::Ssh => DraftProfile::Ssh(SshProfile {
                id,
                name,
                group,
                host,
                port,
                username: optional(username),
                key_path: optional(key_path).map(PathBuf::from),
                gateway: self.routed_gateway(),
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
            }),
            DraftProtocol::Vnc => DraftProfile::Vnc(VncProfile {
                id,
                name,
                group,
                host,
                port,
                view_only: self.is_on(ProfileToggle::ViewOnly),
                allow_no_password: self.is_on(ProfileToggle::AllowNoPassword),
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
        })
    }
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
        assert_eq!(rdp.rdp_options, RdpOptions::default());
        assert_eq!(
            ProfileToggle::of(DraftProtocol::Rdp),
            [
                ProfileToggle::RedirectClipboard,
                ProfileToggle::RedirectDrives,
                ProfileToggle::Nla,
                ProfileToggle::AdminSession
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
}
