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

//! The security types spoken, and what the two wrapping ones carry: Tight (16) and `VeNCrypt`
//! (19), each with no authentication or VNC Authentication inside, as noVNC 1.5.0 speaks
//! them. Neither encrypts here: the TLS subtypes of `VeNCrypt` are not spoken, nor Plain.

use std::fmt;

/// Security types.
pub(super) const SECURITY_NONE: u8 = 1;
pub(super) const SECURITY_VNC_AUTH: u8 = 2;
pub(super) const SECURITY_TIGHT: u8 = 16;
pub(super) const SECURITY_VENCRYPT: u8 = 19;

/// The types taken, first preferred. A type with VNC Authentication or none at all goes
/// first: what a wrapper carries is known only once it is chosen, and a wrapper chosen
/// cannot be left for another. Tight goes before `VeNCrypt`, which servers offer mostly for
/// TLS.
pub(super) const PREFERENCE: [u8; 4] = [
    SECURITY_VNC_AUTH,
    SECURITY_NONE,
    SECURITY_TIGHT,
    SECURITY_VENCRYPT,
];

/// Bytes of one Tight capability: a code, a vendor and a signature.
pub(super) const TIGHT_CAPABILITY_BYTES: usize = 16;
/// Where the vendor and the signature start in a Tight capability.
const VENDOR_AT: usize = 4;
const SIGNATURE_AT: usize = 8;

/// The Tight tunnel the client takes: none.
pub(super) const TIGHT_NO_TUNNEL: u32 = 0;
const TIGHT_VENDOR: &[u8] = b"TGHT";
const NO_TUNNEL_SIGNATURE: &[u8] = b"NOTUNNEL";
/// Siemens touch panels take no tunnel but do not say so; they say this one instead, and
/// noVNC takes it for no tunnel.
const SIEMENS_TUNNEL: u32 = 1;
const SIEMENS_VENDOR: &[u8] = b"SICR";
const SIEMENS_SIGNATURE: &[u8] = b"SCHANNEL";

/// Tight authentication types, known by vendor and signature as noVNC knows them, and the
/// code the client answers for each.
const STANDARD_VENDOR: &[u8] = b"STDV";
const NO_AUTH_SIGNATURE: &[u8] = b"NOAUTH__";
const VNC_AUTH_SIGNATURE: &[u8] = b"VNCAUTH_";
pub(super) const TIGHT_AUTH_NONE: u32 = 1;
pub(super) const TIGHT_AUTH_VNC: u32 = 2;

/// Most Tight tunnel types read; a server offering more is refused.
pub(super) const MAX_TIGHT_TUNNELS: usize = 64;
/// Most Tight authentication types read; a server offering more is refused.
pub(super) const MAX_TIGHT_AUTH_TYPES: usize = 64;
/// Most entries read in each of Tight's capability lists after `ServerInit`.
pub(super) const MAX_TIGHT_INIT_CAPABILITIES: usize = 256;

/// The `VeNCrypt` version spoken: 0.2, the only one noVNC speaks.
pub(super) const VENCRYPT_VERSION: [u8; 2] = [0, 2];
/// The server's answer when it takes the version.
pub(super) const VENCRYPT_ACCEPTED: u8 = 0;
/// Bytes of one `VeNCrypt` subtype.
pub(super) const VENCRYPT_SUBTYPE_BYTES: usize = 4;
/// Most `VeNCrypt` subtypes read; a server offering more is refused.
pub(super) const MAX_VENCRYPT_SUBTYPES: usize = 64;
/// The `VeNCrypt` subtypes spoken: the standard types, unencrypted.
pub(super) const VENCRYPT_NONE: u32 = 1;
pub(super) const VENCRYPT_VNC_AUTH: u32 = 2;

/// A security type that carries another.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SecurityWrapper {
    /// Tight (16): tunnels, then an authentication.
    Tight,
    /// `VeNCrypt` (19): a version, then a subtype.
    VeNCrypt,
}

impl SecurityWrapper {
    /// The security type's code.
    #[must_use]
    pub fn code(self) -> u8 {
        match self {
            Self::Tight => SECURITY_TIGHT,
            Self::VeNCrypt => SECURITY_VENCRYPT,
        }
    }
}

impl fmt::Display for SecurityWrapper {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Tight => "Tight",
            Self::VeNCrypt => "VeNCrypt",
        })
    }
}

/// How the client proved itself.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Authentication {
    /// It did not: the server asked for nothing.
    NoAuthentication,
    /// VNC Authentication: the password answered a challenge.
    VncAuth,
}

impl fmt::Display for Authentication {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::NoAuthentication => "no authentication",
            Self::VncAuth => "VNC Authentication",
        })
    }
}

/// The security a connection agreed on. No secret is in it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Security {
    /// The type that carried the authentication, if one did.
    pub wrapper: Option<SecurityWrapper>,
    /// The authentication.
    pub authentication: Authentication,
}

impl fmt::Display for Security {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.wrapper {
            Some(wrapper) => write!(formatter, "{} inside {wrapper}", self.authentication),
            None => write!(formatter, "{}", self.authentication),
        }
    }
}

/// One Tight capability: a code, a vendor and a signature.
struct Capability<'a> {
    code: u32,
    vendor: &'a [u8],
    signature: &'a [u8],
}

impl Capability<'_> {
    fn is(&self, vendor: &[u8], signature: &[u8]) -> bool {
        self.vendor == vendor && self.signature == signature
    }
}

/// The Tight capabilities in `bytes`, whole ones only.
fn capabilities(bytes: &[u8]) -> impl Iterator<Item = Capability<'_>> {
    bytes
        .as_chunks::<TIGHT_CAPABILITY_BYTES>()
        .0
        .iter()
        .map(|entry| Capability {
            code: u32::from_be_bytes([entry[0], entry[1], entry[2], entry[3]]),
            vendor: &entry[VENDOR_AT..SIGNATURE_AT],
            signature: &entry[SIGNATURE_AT..],
        })
}

/// The codes of the Tight capabilities in `bytes`.
pub(super) fn tight_codes(bytes: &[u8]) -> Vec<u32> {
    capabilities(bytes)
        .map(|capability| capability.code)
        .collect()
}

/// Whether a server offering the Tight tunnels in `bytes` takes no tunnel, as noVNC
/// decides it: the Siemens tunnel is there, or the no-tunnel code is there with Tight's own
/// vendor and signature. Of a code said twice, the last counts.
pub(super) fn tight_takes_no_tunnel(bytes: &[u8]) -> bool {
    let mut no_tunnel = false;
    let mut siemens = false;
    for capability in capabilities(bytes) {
        if capability.code == TIGHT_NO_TUNNEL {
            no_tunnel = capability.is(TIGHT_VENDOR, NO_TUNNEL_SIGNATURE);
        } else if capability.code == SIEMENS_TUNNEL {
            siemens = capability.is(SIEMENS_VENDOR, SIEMENS_SIGNATURE);
        }
    }
    siemens || no_tunnel
}

/// The Tight authentication taken from those in `bytes`: VNC Authentication first, then
/// none when `allow_none`; `None` when neither will do.
pub(super) fn tight_authentication(bytes: &[u8], allow_none: bool) -> Option<Authentication> {
    let offers = |signature: &[u8]| {
        capabilities(bytes).any(|capability| capability.is(STANDARD_VENDOR, signature))
    };
    if offers(VNC_AUTH_SIGNATURE) {
        Some(Authentication::VncAuth)
    } else if allow_none && offers(NO_AUTH_SIGNATURE) {
        Some(Authentication::NoAuthentication)
    } else {
        None
    }
}

/// The code the client answers for `authentication` inside Tight.
pub(super) fn tight_code(authentication: Authentication) -> u32 {
    match authentication {
        Authentication::NoAuthentication => TIGHT_AUTH_NONE,
        Authentication::VncAuth => TIGHT_AUTH_VNC,
    }
}

/// The `VeNCrypt` subtype taken from those offered: VNC Authentication first, then none when
/// `allow_none`; `None` when neither is there.
pub(super) fn vencrypt_subtype(offered: &[u32], allow_none: bool) -> Option<Authentication> {
    if offered.contains(&VENCRYPT_VNC_AUTH) {
        Some(Authentication::VncAuth)
    } else if allow_none && offered.contains(&VENCRYPT_NONE) {
        Some(Authentication::NoAuthentication)
    } else {
        None
    }
}

/// The code the client answers for `authentication` inside `VeNCrypt`.
pub(super) fn vencrypt_code(authentication: Authentication) -> u32 {
    match authentication {
        Authentication::NoAuthentication => VENCRYPT_NONE,
        Authentication::VncAuth => VENCRYPT_VNC_AUTH,
    }
}
