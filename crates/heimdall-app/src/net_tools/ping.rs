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

//! One echo request of the PING tool, as the C# `PingService.PingDirectAsync`: the host
//! resolved, then the system's `ping` run on the address, by its full path, for one request
//! within the probe's timeout and a grace after it.

use std::time::Duration;

use heimdall_core::tools::ping::{self, PingDetail, PingProbeResult};

use super::{ProgramFailure, resolve, run_program};

/// How a probe's local time is written, as the C# `DateTime.Now.ToString("HH:mm:ss")`.
pub const TIMESTAMP_FORMAT: &str = "%H:%M:%S";

/// The local time now, as a probe is stamped.
#[must_use]
pub fn timestamp_now() -> String {
    chrono::Local::now().format(TIMESTAMP_FORMAT).to_string()
}

/// Probe `seq` of `host`, waiting `timeout_ms` for the reply.
pub async fn probe(host: &str, seq: usize, timeout_ms: u64) -> PingProbeResult {
    let timestamp = timestamp_now();
    let without_reply =
        |detail| PingProbeResult::without_reply(seq, timestamp.clone(), host.to_owned(), detail);
    // The first address the resolver gives, as the C# `Ping` takes it.
    let Some(address) = resolve(host, Duration::from_millis(timeout_ms))
        .await
        .ok()
        .and_then(|addresses| addresses.first().copied())
    else {
        return without_reply(PingDetail::Unresolved);
    };
    let Some(program) = ping::ping_program() else {
        return without_reply(PingDetail::NoProgram);
    };
    let budget = Duration::from_millis(timeout_ms.saturating_add(ping::PROGRAM_GRACE_MS));
    match run_program(&program, &ping::ping_arguments(address, timeout_ms), budget).await {
        Ok(run) => {
            let output = format!("{}\n{}", run.stdout, run.stderr);
            match ping::classify_run(run.exited_ok, &output) {
                Ok(reply) => {
                    PingProbeResult::replied(seq, timestamp.clone(), host.to_owned(), &reply)
                }
                Err(detail) => without_reply(detail),
            }
        }
        Err(ProgramFailure::TimedOut) => without_reply(PingDetail::TimedOut),
        Err(ProgramFailure::Failed(why)) => without_reply(PingDetail::Failed(why)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use heimdall_core::tools::ping::PingStatus;

    #[test]
    fn a_probe_is_stamped_with_the_local_time() {
        let stamp = timestamp_now();
        let parsed = chrono::NaiveTime::parse_from_str(&stamp, TIMESTAMP_FORMAT).expect("HH:MM:SS");
        let local = chrono::Local::now().time();
        // The same second, or the one before at worst; around midnight, a day apart.
        let apart = (local - parsed).num_seconds().rem_euclid(24 * 60 * 60);
        assert!(apart <= 1, "{stamp} against {local}");
    }

    #[tokio::test]
    async fn a_name_that_does_not_resolve_is_an_error_not_a_loss() {
        let result = probe("nothing.invalid", 1, 2000).await;
        assert_eq!(result.detail, PingDetail::Unresolved);
        assert_eq!(result.status, PingStatus::Error);
        assert_eq!(result.ip_or_host, "nothing.invalid");
    }
}
