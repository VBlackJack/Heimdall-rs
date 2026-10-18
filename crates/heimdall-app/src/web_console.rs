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

//! Web consoles (iLO, iDRAC, a NAS), as the C# WEB profiles: opened in the system's
//! browser, never embedded, on the address their host, port, scheme and path form.
//!
//! The C# opens them in a `WebView2` tab and reaches one behind an SSH gateway through the
//! gateway's SOCKS proxy (`WebHandler.cs:85-133`). Here the system's browser opens them, as
//! the C# "Open with an external program" does with an empty program, and one behind a
//! gateway is reached through a tunnel of this computer's loopback network (`ssh -L`), on an
//! address of its own ([`loopback_for`]) so that the browser keeps each console's cookies
//! apart: the browser then names that address and the tunnel's port, not the console, which
//! a console checking its own name, or a certificate naming it, notices.
//!
//! An address goes to the browser only once built here and passed by
//! [`crate::external_url::launchable_url`]: `http` or `https`, a host and nothing that
//! could be read as a program, a switch or another scheme. No password is ever put in it:
//! the browser asks for one itself.

use std::net::{Ipv4Addr, SocketAddr};

use heimdall_core::profile::{ProfileId, WebProfile};
use heimdall_core::web::{self, WebAddressError};

use crate::tunnel::TunnelBrowse;

/// Why a web console was not opened.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WebRefusal {
    /// Its host, port and path form no address, as the C# `ErrorWebInvalidAddress`.
    InvalidAddress,
    /// A tunnel said to listen elsewhere than on this computer's loopback address: never
    /// handed to the browser.
    NotLoopback,
    /// The browser could not be started, for this reason.
    NotStarted(String),
}

impl From<WebAddressError> for WebRefusal {
    fn from(_: WebAddressError) -> Self {
        Self::InvalidAddress
    }
}

/// The address the browser opens for `profile` reached directly.
///
/// # Errors
///
/// [`WebRefusal::InvalidAddress`] when it forms none the browser may be handed.
pub fn direct_url(profile: &WebProfile) -> Result<String, WebRefusal> {
    launchable(&profile.address()?)
}

/// The host and port a tunnel's gateway dials for `profile`: its host checked, an IPv6
/// address without brackets.
///
/// # Errors
///
/// [`WebRefusal::InvalidAddress`] for a host, or a port, that forms no address.
pub fn forward_target(profile: &WebProfile) -> Result<(String, u16), WebRefusal> {
    // The whole address is checked, its path too, before any gateway is dialled for it.
    direct_url(profile)?;
    Ok((web::forward_host(&profile.host)?, profile.port))
}

/// What the browser opens for `profile` once its tunnel listens.
#[must_use]
pub fn browse_of(profile: &WebProfile) -> TunnelBrowse {
    TunnelBrowse {
        name: profile.name.clone(),
        use_http: profile.use_http,
        path: profile.path.clone(),
        loopback: loopback_for(&profile.id),
    }
}

/// The FNV-1a offset basis, 32 bits.
const FNV_OFFSET: u32 = 0x811C_9DC5;

/// The FNV-1a prime, 32 bits.
const FNV_PRIME: u32 = 0x0100_0193;

/// The loopback address of the console `id`'s tunnel: one of `127.0.0.0/8` of its own, the
/// same on every run, never `127.0.0.1` (the manual tunnels'), nor a network or broadcast
/// address. A browser scopes cookies, storage and saved sign-ins by host, never by port: on
/// one shared address, a console behind a gateway would be sent another's session cookie.
#[must_use]
pub fn loopback_for(id: &ProfileId) -> Ipv4Addr {
    // FNV-1a: stable across runs and builds, unlike the standard library's hasher, so the
    // browser finds the console where it left it.
    let hash = id.as_str().bytes().fold(FNV_OFFSET, |hash, byte| {
        (hash ^ u32::from(byte)).wrapping_mul(FNV_PRIME)
    });
    let [first, second, third, _] = hash.to_be_bytes();
    // 127.1.0.1 to 127.254.255.254: off 127.0.x.x, so never 127.0.0.1.
    Ipv4Addr::new(127, first % 254 + 1, second, third % 254 + 1)
}

/// The address the browser opens for the console `browse` describes, through a tunnel
/// listening at `local`: its loopback address and port, the console's scheme and path.
///
/// # Errors
///
/// [`WebRefusal::NotLoopback`] for a tunnel said to listen anywhere but on the loopback
/// address, [`WebRefusal::InvalidAddress`] for a path that forms no address.
pub fn tunnel_url(browse: &TunnelBrowse, local: SocketAddr) -> Result<String, WebRefusal> {
    if !local.ip().is_loopback() {
        return Err(WebRefusal::NotLoopback);
    }
    launchable(&web::address(
        &local.ip().to_string(),
        local.port(),
        browse.use_http,
        browse.path.as_deref(),
    )?)
}

/// `url` as the browser is handed it, when [`crate::external_url::launchable_url`] lets it
/// through.
fn launchable(url: &str) -> Result<String, WebRefusal> {
    crate::external_url::launchable_url(url).ok_or(WebRefusal::InvalidAddress)
}

#[cfg(test)]
mod tests {
    use std::net::{Ipv4Addr, Ipv6Addr};

    use heimdall_core::profile::ProfileId;

    use super::*;

    fn console(host: &str, port: u16) -> WebProfile {
        WebProfile {
            id: ProfileId::new("w"),
            name: "iLO".to_owned(),
            group: None,
            host: host.to_owned(),
            port,
            use_http: false,
            path: None,
            username: Some("admin".to_owned()),
            gateway: None,
        }
    }

    #[test]
    fn a_console_reached_directly_opens_its_own_address_without_its_account() {
        let mut profile = console("ilo.lab", 8443);
        profile.path = Some("index.html".to_owned());
        let url = direct_url(&profile).expect("an address");
        assert_eq!(url, "https://ilo.lab:8443/index.html");
        assert!(!url.contains("admin"), "{url}");
    }

    #[test]
    fn a_host_that_would_change_the_scheme_or_the_host_is_never_opened() {
        for host in [
            "javascript:alert(1)",
            "file:///C:/Windows/System32/calc.exe",
            "evil.example@ilo.lab",
            "ilo.lab/../x",
            "ilo.lab\" --switch",
            "",
        ] {
            assert_eq!(
                direct_url(&console(host, 443)),
                Err(WebRefusal::InvalidAddress),
                "{host:?}"
            );
            assert_eq!(
                forward_target(&console(host, 443)),
                Err(WebRefusal::InvalidAddress),
                "{host:?}"
            );
        }
        assert_eq!(
            direct_url(&console("ilo.lab", 0)),
            Err(WebRefusal::InvalidAddress)
        );
    }

    #[test]
    fn each_console_has_a_loopback_address_of_its_own_the_same_on_every_run() {
        let ilo = loopback_for(&ProfileId::new("ilo"));
        assert!(ilo.is_loopback());
        assert_ne!(ilo, Ipv4Addr::LOCALHOST);
        assert_eq!(ilo, loopback_for(&ProfileId::new("ilo")), "stable");
        assert_ne!(ilo, loopback_for(&ProfileId::new("idrac")));
        // Pinned: an address that moved between builds would lose the browser's state.
        assert_eq!(
            loopback_for(&ProfileId::new("")),
            Ipv4Addr::new(127, 130, 28, 158)
        );
        for id in 0..2_000 {
            let address = loopback_for(&ProfileId::new(format!("rs-{id:016x}")));
            let [_, second, _, fourth] = address.octets();
            assert!(address.is_loopback(), "{address}");
            assert!(
                (1..=254).contains(&second) && (1..=254).contains(&fourth),
                "{address}"
            );
        }
    }

    #[test]
    fn a_console_behind_a_gateway_is_dialled_by_its_bare_host() {
        assert_eq!(
            forward_target(&console("[fd00::10]", 443)),
            Ok(("fd00::10".to_owned(), 443))
        );
        assert_eq!(
            forward_target(&console("ilo.lab", 8443)),
            Ok(("ilo.lab".to_owned(), 8443))
        );
    }

    #[test]
    fn the_browser_is_pointed_at_the_tunnel_on_the_loopback_address_only() {
        let mut profile = console("ilo.lab", 443);
        profile.use_http = true;
        profile.path = Some("/login?next=/".to_owned());
        let browse = browse_of(&profile);
        assert_eq!(
            tunnel_url(&browse, SocketAddr::from((Ipv4Addr::LOCALHOST, 50123))).as_deref(),
            Ok("http://127.0.0.1:50123/login?next=/")
        );
        let own = browse.loopback;
        assert_eq!(
            tunnel_url(&browse, SocketAddr::from((own, 50123))),
            Ok(format!("http://{own}:50123/login?next=/"))
        );
        assert_eq!(
            tunnel_url(&browse, SocketAddr::from((Ipv4Addr::UNSPECIFIED, 50123))),
            Err(WebRefusal::NotLoopback)
        );
        assert_eq!(
            tunnel_url(
                &browse,
                SocketAddr::from((Ipv4Addr::new(192, 0, 2, 1), 50123))
            ),
            Err(WebRefusal::NotLoopback)
        );
        assert_eq!(
            tunnel_url(&browse, SocketAddr::from((Ipv6Addr::LOCALHOST, 50123))).as_deref(),
            Ok("http://[::1]:50123/login?next=/")
        );
    }
}
