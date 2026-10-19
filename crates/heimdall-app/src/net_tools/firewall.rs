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

//! One probe of the FWTEST tool made from an SSH gateway, as the C# `FirewallTesterService`
//! tests egress from the gateway: the channel the gateway opens onward is the TCP handshake,
//! `host` sent as typed for the gateway to resolve. Nothing is connected from this computer.
//!
//! A gateway reports a failed channel without telling a closed port from a name it could not
//! resolve: a refusal is read as closed, as the direct probe reads any socket error.

use std::io::ErrorKind;
use std::time::{Duration, Instant};

use heimdall_core::tools::firewall_tester::{FwProbeResult, FwProbeStatus};
use heimdall_ssh::socks::Opener;
use heimdall_ssh::tool_route::ToolGateway;

/// Port `port` of `host` as the gateway reaches it, within `timeout_ms`: open when the
/// channel opens onward, closed when the gateway is refused, a timeout when nothing answers
/// in time. A gateway that could not be asked at all says nothing of the firewall, so the
/// probe is unresolved.
pub async fn probe_via<O: Opener>(
    gateway: &ToolGateway<O>,
    host: &str,
    port: u16,
    timeout_ms: u64,
) -> FwProbeResult {
    let started = Instant::now();
    let dialed = gateway
        .dial_within(host, port, Some(Duration::from_millis(timeout_ms)))
        .await;
    let elapsed = u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX);
    let (status, latency_ms) = match dialed {
        Ok(_connected) => (FwProbeStatus::Open, Some(elapsed)),
        Err(error) => match error.kind() {
            ErrorKind::TimedOut => (FwProbeStatus::Timeout, None),
            ErrorKind::ConnectionRefused => (FwProbeStatus::Closed, Some(elapsed)),
            _ => (FwProbeStatus::Unresolved, None),
        },
    };
    FwProbeResult {
        host: host.to_owned(),
        port,
        status,
        latency_ms,
    }
}
