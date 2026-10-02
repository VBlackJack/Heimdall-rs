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

//! Import of Remote Desktop `.rdp` files, as the C# Heimdall reads them (`RdpFileParser`,
//! `RdpImportService`).
//!
//! A file is `key:type:value` lines. What a profile can carry is read onto it: the address
//! and port, the account and domain, the administrative session, audio, the clipboard and
//! drives, dynamic resolution, colour depth and Network Level Authentication. A stored
//! password is never read. What the profile has no field for is named, so the preview says
//! the mapping is partial; a file routed through a Remote Desktop Gateway is refused, as the
//! import from the C# settings refuses one.
//!
//! A file is read onto a profile as a [`Patch`]: the settings the file names. A new profile
//! starts from the C# defaults; replacing one keeps every setting the file does not name, as
//! the C# `ReplaceExisting` does.

use std::collections::HashSet;
use std::hash::BuildHasher;

use crate::profile::{
    AudioPlayback, ColorDepth, DEFAULT_RDP_PORT, Forwards, ProfileId, RdpOptions, RdpProfile,
};

/// Biggest file read, as the C# `MaxImportFileSizeBytes`: an oversized file is unreadable.
pub const MAX_FILE_BYTES: u64 = 50 * 1024 * 1024;

/// A file name that says nothing of its server: the address names it instead.
const GENERIC_NAMES: [&str; 3] = ["default", "connection", "remote desktop connection"];

/// `drivestoredirect` naming every drive.
const ALL_DRIVES: &str = "*";

/// The keys read but not carried: named in the preview as a partial mapping.
const NOT_CARRIED: [&str; 12] = [
    "audiocapturemode",
    "redirectprinters",
    "redirectsmartcards",
    "redirectcomports",
    "usbdevicestoredirect",
    "camerastoredirect",
    "compression",
    "bitmapcachepersistenable",
    "autoreconnection enabled",
    "use multimon",
    "screen mode id",
    "authentication level",
];

/// The window size keys, not carried as the C# does not carry them: every file the client
/// saves has them, and a fixed size taken from them would change every imported profile.
const SIZE_KEYS: [&str; 2] = ["desktopwidth", "desktopheight"];

/// What a `.rdp` file says.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RdpFile {
    full_address: Option<String>,
    alternate_full_address: Option<String>,
    username: Option<String>,
    domain: Option<String>,
    audio_mode: Option<i64>,
    redirect_clipboard: Option<bool>,
    redirect_drives: Option<bool>,
    drives_to_redirect: Option<String>,
    administrative_session: Option<bool>,
    dynamic_resolution: Option<bool>,
    session_bpp: Option<i64>,
    enable_credssp: Option<i64>,
    gateway_hostname: Option<String>,
    gateway_usage: Option<i64>,
    /// A stored password was there, and was left out.
    pub has_password: bool,
    /// Keys read but not carried, as written.
    pub not_carried: Vec<String>,
    /// Keys not known at all.
    pub unknown: usize,
}

/// Reads a `.rdp` file; never fails, as the C# parser: a line it cannot read is unknown.
#[must_use]
pub fn parse(text: &str) -> RdpFile {
    let mut file = RdpFile::default();
    for line in text
        .split(['\r', '\n'])
        .map(str::trim)
        .filter(|l| !l.is_empty())
    {
        let Some((key, kind, value)) = split_line(line) else {
            continue;
        };
        let key = key.trim().to_lowercase();
        if key.is_empty() {
            continue;
        }
        if key == "password 51" && kind.eq_ignore_ascii_case("b") {
            file.has_password = true;
            continue;
        }
        read_key(&mut file, &key, kind, value);
    }
    file
}

/// `key:type:value`, the value holding any further colon.
fn split_line(line: &str) -> Option<(&str, &str, &str)> {
    let (key, rest) = line.split_once(':')?;
    let (kind, value) = rest.split_once(':')?;
    (!key.is_empty()).then_some((key, kind, value))
}

fn read_key(file: &mut RdpFile, key: &str, kind: &str, value: &str) {
    let text = || {
        kind.eq_ignore_ascii_case("s")
            .then(|| value.trim().to_owned())
    };
    let number = || {
        kind.eq_ignore_ascii_case("i")
            .then(|| value.trim().parse::<i64>().ok())
            .flatten()
    };
    let flag = || number().map(|value| value != 0);
    let known = match key {
        "full address" => text().map(|v| file.full_address = Some(v)),
        "alternate full address" => text().map(|v| file.alternate_full_address = Some(v)),
        "username" => text().map(|v| file.username = Some(v)),
        "domain" => text().map(|v| file.domain = Some(v)),
        "audiomode" => number().map(|v| file.audio_mode = Some(v)),
        "redirectclipboard" => flag().map(|v| file.redirect_clipboard = Some(v)),
        "redirectdrives" => flag().map(|v| file.redirect_drives = Some(v)),
        "drivestoredirect" => text().map(|v| file.drives_to_redirect = Some(v)),
        "administrative session" => flag().map(|v| file.administrative_session = Some(v)),
        "dynamic resolution" => flag().map(|v| file.dynamic_resolution = Some(v)),
        "session bpp" => number().map(|v| file.session_bpp = Some(v)),
        "enablecredsspsupport" => number().map(|v| file.enable_credssp = Some(v)),
        "gatewayhostname" => text().map(|v| file.gateway_hostname = Some(v)),
        "gatewayusagemethod" => number().map(|v| file.gateway_usage = Some(v)),
        other if NOT_CARRIED.contains(&other) || SIZE_KEYS.contains(&other) => {
            if !file.not_carried.iter().any(|seen| seen == other) {
                file.not_carried.push(other.to_owned());
            }
            Some(())
        }
        _ => None,
    };
    if known.is_none() {
        file.unknown += 1;
    }
}

/// Why a file gives no profile.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Refusal {
    /// No address, or one that is not an address.
    InvalidAddress,
    /// Routed through a Remote Desktop Gateway, which Heimdall-rs does not reach through.
    NeedsRdGateway,
}

/// The settings a file names, to write onto a profile.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Patch {
    /// Host, as the file writes it (an IPv6 address in brackets).
    pub host: String,
    /// Port: the file's, or the RDP default.
    pub port: u16,
    /// The port given was out of range: the default was used.
    pub port_out_of_range: bool,
    username: Option<String>,
    domain: Option<String>,
    administrative_session: Option<bool>,
    audio: Option<AudioPlayback>,
    redirect_clipboard: Option<bool>,
    redirect_drives: Option<bool>,
    /// Some drives only were named: the profile shares all of them.
    pub drives_widened: bool,
    dynamic_resolution: Option<bool>,
    color_depth: Option<ColorDepth>,
    nla: Option<bool>,
}

impl Patch {
    /// The settings of `file`, or why it gives no profile.
    ///
    /// # Errors
    ///
    /// [`Refusal`] when the file has no usable address or goes through a gateway.
    pub fn of(file: &RdpFile) -> Result<Self, Refusal> {
        // As mstsc: no usage method, or 0, is no gateway, whatever the host says; a
        // crafted file must not route the session and its credentials through a third party.
        if file
            .gateway_hostname
            .as_deref()
            .is_some_and(|host| !host.trim().is_empty())
            && file.gateway_usage.is_some_and(|usage| usage != 0)
        {
            return Err(Refusal::NeedsRdGateway);
        }
        let address = file
            .full_address
            .as_deref()
            .filter(|address| !address.trim().is_empty())
            .or(file.alternate_full_address.as_deref());
        let (host, port, port_out_of_range) =
            split_address(address).ok_or(Refusal::InvalidAddress)?;
        let drives = drive_redirection(file.redirect_drives, file.drives_to_redirect.as_deref());
        Ok(Self {
            host,
            port,
            port_out_of_range,
            username: file.username.clone().filter(|user| !user.trim().is_empty()),
            domain: file
                .domain
                .clone()
                .filter(|domain| !domain.trim().is_empty()),
            administrative_session: file.administrative_session,
            audio: file.audio_mode.map(|mode| match mode {
                0 => AudioPlayback::Local,
                1 => AudioPlayback::OnServer,
                _ => AudioPlayback::Off,
            }),
            redirect_clipboard: file.redirect_clipboard,
            redirect_drives: drives,
            drives_widened: drives == Some(true)
                && file
                    .drives_to_redirect
                    .as_deref()
                    .is_some_and(|named| !named.trim().is_empty() && named.trim() != ALL_DRIVES),
            dynamic_resolution: file.dynamic_resolution,
            color_depth: file.session_bpp.map(ColorDepth::nearest),
            // Only 0 and 1 mean something; anything else leaves the profile as it was.
            nla: file
                .enable_credssp
                .filter(|value| matches!(value, 0 | 1))
                .map(|value| value == 1),
        })
    }

    /// Whether settings of the file are not carried: the C# "Partial mapping".
    #[must_use]
    pub fn is_partial(&self, file: &RdpFile) -> bool {
        self.port_out_of_range || self.drives_widened || !file.not_carried.is_empty()
    }

    /// A new profile named `name`, from the C# defaults, with what the file names.
    #[must_use]
    pub fn new_profile(&self, id: ProfileId, name: String) -> RdpProfile {
        let mut profile = RdpProfile {
            id,
            name,
            group: None,
            host: String::new(),
            port: DEFAULT_RDP_PORT,
            username: None,
            domain: None,
            allow_tls_only: false,
            gateway: None,
            redirect_clipboard: true,
            redirect_drives: false,
            options: RdpOptions::default(),
            vault_entry: None,
            forwards: Forwards::default(),
            follow_defaults: false,
            several_servers: false,
        };
        self.apply(&mut profile);
        profile
    }

    /// Writes what the file names onto `profile`, and nothing else: its name, identity and
    /// everything the file does not name stay, as the C# `ReplaceExisting`.
    pub fn apply(&self, profile: &mut RdpProfile) {
        profile.host.clone_from(&self.host);
        profile.port = self.port;
        if let Some(user) = &self.username {
            profile.username = Some(user.clone());
        }
        if let Some(domain) = &self.domain {
            profile.domain = Some(domain.clone());
        }
        if let Some(admin) = self.administrative_session {
            profile.options.admin_session = admin;
        }
        if let Some(audio) = self.audio {
            profile.options.audio = audio;
        }
        if let Some(clipboard) = self.redirect_clipboard {
            profile.redirect_clipboard = clipboard;
        }
        if let Some(drives) = self.redirect_drives {
            profile.redirect_drives = drives;
        }
        if let Some(dynamic) = self.dynamic_resolution {
            profile.options.dynamic_resolution = dynamic;
        }
        if let Some(depth) = self.color_depth {
            profile.options.color_depth = depth;
        }
        if let Some(nla) = self.nla {
            profile.allow_tls_only = !nla;
        }
    }
}

/// The drive redirection a file asks for: `drivestoredirect` naming drives wins, as mstsc
/// reads it; else `redirectdrives`; an empty `drivestoredirect` alone is none.
fn drive_redirection(redirect: Option<bool>, named: Option<&str>) -> Option<bool> {
    if named.is_some_and(|named| !named.trim().is_empty()) {
        return Some(true);
    }
    redirect.or(named.map(|_| false))
}

/// Host and port of `host`, `host:port` or `[v6]:port`; the default port when none is given
/// or it is out of range, saying so.
fn split_address(address: Option<&str>) -> Option<(String, u16, bool)> {
    let address = address?.trim();
    if address.is_empty() {
        return None;
    }
    let port_of = |text: &str| match text.trim().parse::<i64>() {
        Ok(port) => match u16::try_from(port) {
            Ok(port) if port != 0 => (port, false),
            _ => (DEFAULT_RDP_PORT, true),
        },
        Err(_) => (DEFAULT_RDP_PORT, false),
    };
    if let Some(rest) = address.strip_prefix('[')
        && let Some(end) = rest.find(']')
    {
        let host = format!("[{}]", &rest[..end]);
        let after = &rest[end + 1..];
        let (port, out) = after
            .strip_prefix(':')
            .map_or((DEFAULT_RDP_PORT, false), port_of);
        return Some((host, port, out));
    }
    if address.matches(':').count() == 1 {
        let (host, port) = address.split_once(':')?;
        let host = host.trim();
        if !host.is_empty() {
            let (port, out) = port_of(port);
            return Some((host.to_owned(), port, out));
        }
    }
    Some((address.to_owned(), DEFAULT_RDP_PORT, false))
}

/// The name a file proposes: its own, unless generic, else its address, else `fallback`.
#[must_use]
pub fn proposed_name(file_stem: &str, file: &RdpFile, fallback: &str) -> String {
    let stem = file_stem.trim();
    if !stem.is_empty()
        && !GENERIC_NAMES
            .iter()
            .any(|generic| stem.eq_ignore_ascii_case(generic))
    {
        return stem.to_owned();
    }
    [&file.alternate_full_address, &file.full_address]
        .into_iter()
        .flatten()
        .map(|address| address.trim())
        .find(|address| !address.is_empty())
        .map_or_else(|| fallback.to_owned(), str::to_owned)
}

/// What to do with a file whose name a profile already has, as the C# preview offers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Conflict {
    /// Leave the file out.
    Skip,
    /// Write the file onto the profile that has the name.
    Replace,
    /// Import it under a name no profile has: the C# default.
    AutoRename,
}

/// The first `rename(base, n)` from 2 up that `taken` (lowercase) does not hold, as the C#
/// `ImportAutoRename`.
pub fn auto_rename<S: BuildHasher>(
    base: &str,
    taken: &HashSet<String, S>,
    rename: &dyn Fn(&str, u32) -> String,
) -> String {
    let mut suffix = 2;
    loop {
        let name = rename(base, suffix);
        if !taken.contains(&name.to_lowercase()) {
            return name;
        }
        suffix += 1;
    }
}
