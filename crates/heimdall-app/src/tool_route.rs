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

//! The "Route via" of the network tools: which tools can run through an SSH gateway, the
//! gateways to choose from, and the route ready to open for the one chosen.
//!
//! The C# routes a tool by running programs on the gateway (`bash`, `curl`, `dig`, `ping`)
//! with what they need on its command line. Here only the connections are made from the
//! gateway: a `direct-tcpip` channel to the host and port the tool is pointed at
//! ([`heimdall_ssh::tool_route`]). A tool that needs more than a TCP connection (ICMP, ARP,
//! raw sockets, UDP) is refused when a gateway is chosen, never run from this computer
//! instead.

use heimdall_core::profile::{ProfileId, SshGateway, SshProfile};
use heimdall_ssh::tool_route::{self, ToolGateway};
use heimdall_ssh::{ConnectError, ConnectOptions, HopSecrets};
use tokio_util::sync::CancellationToken;

use crate::tools::ToolId;

/// What a tool can do through a gateway.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RouteSupport {
    /// The tool reaches no host, or is not offered a gateway.
    NotOffered,
    /// Its connections are TCP: a channel of the gateway carries each.
    Tcp,
    /// It sends ICMP, ARP, raw or UDP packets, which a gateway's channels cannot carry: a
    /// gateway chosen is refused.
    Refused,
}

impl ToolId {
    /// What the tool does when a gateway is chosen, as the C# "Route via" of its view.
    #[must_use]
    pub const fn route_support(self) -> RouteSupport {
        match self {
            // The scanning tools of the C# whose probes are TCP connections.
            Self::PortScan
            | Self::BannerGrabber
            | Self::HttpHeaders
            | Self::Cert
            | Self::TcpPing
            // WHOIS (TCP 43), the TLS audit's handshakes, the SMB negotiation (its NetBIOS
            // name query is UDP and is left out through a gateway), the egress tester's
            // connections and the SecNumCloud audit's service probes.
            | Self::Whois
            | Self::TlsAudit
            | Self::Smb
            | Self::FwTest
            | Self::SecNumCloud => RouteSupport::Tcp,
            // ICMP (PING, TCPTRACE's TTL probes), ARP (ARPMON), a broadcast datagram (WOL),
            // UDP (SNMP, DNS) and the ping sweep of NETMAP.
            Self::Ping
            | Self::TcpTrace
            | Self::ArpMon
            | Self::WakeOnLan
            | Self::SnmpWalk
            | Self::Dns
            | Self::DnsBatch
            | Self::DnsSec
            | Self::NetMap => RouteSupport::Refused,
            _ => RouteSupport::NotOffered,
        }
    }
}

/// A gateway the picker offers, as the C# lists it: `Name (host:port)`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GatewayChoice {
    /// The gateway.
    pub id: ProfileId,
    /// What the picker shows.
    pub label: String,
}

impl GatewayChoice {
    /// The choice `gateway` is.
    #[must_use]
    pub fn of(gateway: &SshGateway) -> Self {
        Self {
            id: gateway.id.clone(),
            label: format!("{} ({}:{})", gateway.name, gateway.host, gateway.port),
        }
    }
}

/// A route to a gateway, with what its hops sign in with, ready to open.
#[derive(Clone)]
pub struct ToolRoute {
    hops: Vec<SshProfile>,
    secrets: Vec<HopSecrets>,
    options: ConnectOptions,
}

impl std::fmt::Debug for ToolRoute {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // The secrets are not shown.
        f.debug_struct("ToolRoute")
            .field("hops", &self.hops.len())
            .finish_non_exhaustive()
    }
}

impl ToolRoute {
    /// A route over `hops`, nearest first, each signed in to with its `secrets`.
    #[must_use]
    pub fn new(hops: Vec<SshProfile>, secrets: Vec<HopSecrets>, options: ConnectOptions) -> Self {
        Self {
            hops,
            secrets,
            options,
        }
    }

    /// The gateways of the route, nearest first.
    #[must_use]
    pub fn hops(&self) -> &[SshProfile] {
        &self.hops
    }

    /// Connects to the gateway: the host keys are the sessions' (`known_hosts`), a key never
    /// seen ends the attempt, and no question is asked.
    ///
    /// # Errors
    ///
    /// See [`heimdall_ssh::tool_route::open`].
    pub async fn open(self) -> Result<ToolGateway, ConnectError> {
        // Stopped by dropping the future, as every run of a tool is.
        tool_route::open(
            &self.hops,
            &self.secrets,
            &self.options,
            CancellationToken::new(),
        )
        .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_tools_that_need_more_than_tcp_are_refused_a_gateway() {
        for tool in [
            ToolId::Ping,
            ToolId::TcpTrace,
            ToolId::ArpMon,
            ToolId::WakeOnLan,
            ToolId::SnmpWalk,
            ToolId::NetMap,
            ToolId::Dns,
            ToolId::DnsBatch,
            ToolId::DnsSec,
        ] {
            assert_eq!(tool.route_support(), RouteSupport::Refused, "{tool:?}");
        }
    }

    #[test]
    fn the_tcp_scanning_tools_are_routed() {
        for tool in [
            ToolId::PortScan,
            ToolId::BannerGrabber,
            ToolId::HttpHeaders,
            ToolId::Cert,
            ToolId::TcpPing,
            ToolId::Whois,
            ToolId::TlsAudit,
            ToolId::Smb,
            ToolId::FwTest,
            ToolId::SecNumCloud,
        ] {
            assert_eq!(tool.route_support(), RouteSupport::Tcp, "{tool:?}");
        }
    }

    #[test]
    fn a_local_tool_is_not_offered_a_gateway() {
        for tool in [
            ToolId::Hash,
            ToolId::Uuid,
            ToolId::NetIf,
            ToolId::Notes,
            ToolId::DefaultCreds,
        ] {
            assert_eq!(tool.route_support(), RouteSupport::NotOffered, "{tool:?}");
        }
    }

    #[test]
    fn a_choice_is_named_as_the_csharp_lists_it() {
        let gateway = SshGateway {
            id: ProfileId::new("g"),
            name: "Bastion".to_owned(),
            host: "10.0.0.1".to_owned(),
            port: 2222,
            username: None,
            key_path: None,
            parent: None,
        };
        assert_eq!(GatewayChoice::of(&gateway).label, "Bastion (10.0.0.1:2222)");
    }
}
