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

//! The RDP keep-alive of a running session, as mstsc's `KeepAliveInterval`: when nothing
//! was sent to the server for the interval, the client sends it a PDU that changes nothing,
//! so a firewall or a NAT on the way does not drop an idle connection, and a dead one fails
//! the next write.
//!
//! MS-RDPBCGR names no client keep-alive PDU. The one sent is the Client Synchronize PDU
//! (MS-RDPBCGR 2.2.1.14, `TS_SYNCHRONIZE_PDU`, `PDUTYPE2_SYNCHRONIZE`, `messageType`
//! `SYNCMSGTYPE_SYNC`), as the connection finalization sent it on this connection already:
//!
//! - it carries no state: no input, so the server's idle-session limit still runs (an
//!   input event would reset it); no toggle keys, unlike the Synchronize input event
//!   (2.2.8.1.1.3.1.1.5); no redraw, unlike the Refresh Rect (2.2.11.2) and Suppress
//!   Output (2.2.11.3) PDUs;
//! - the server does not answer it: its own Synchronize PDU (2.2.1.19) answers the Confirm
//!   Active PDU (3.3.5.3.13.2), not this one;
//! - xrdp takes it as a no-op at any time (`xrdp_rdp_process_data_sync`), and the `FreeRDP`
//!   server reads it in any state of an active connection;
//! - the Persistent Key List PDU (2.2.1.17) and the Font List PDU (2.2.1.18) belong to the
//!   connection finalization only.
//!
//! It is RDP-level, so it keeps the way the same through an SSH tunnel.

use std::time::Duration;

use ironrdp::pdu::rdp::finalization_messages::SynchronizePdu;
use ironrdp::pdu::rdp::headers::ShareDataPdu;
use ironrdp::session::ActiveStage;
use ironrdp_core::WriteBuf;
use tokio::time::Instant;

/// Time without anything sent before a keep-alive by default, as mstsc's
/// `KeepAliveInterval` the C# Heimdall sets (`RdpSessionState.DefaultKeepAliveIntervalMs`).
pub const DEFAULT_KEEP_ALIVE: Duration = Duration::from_secs(60);

/// When the session sends its next keep-alive: `every` after the last thing it sent, and
/// only while the connection is active.
#[derive(Debug, Clone, Copy)]
pub(crate) struct KeepAlive {
    /// Time without anything sent before a keep-alive; zero for none.
    every: Duration,
    /// When something was last sent while active; `None` before the activation and during
    /// a reactivation.
    since: Option<Instant>,
}

impl KeepAlive {
    /// A keep-alive every `every` of silence, not yet active.
    pub(crate) fn new(every: Duration) -> Self {
        Self { every, since: None }
    }

    /// The connection is active from `now`: after its activation, or a reactivation.
    pub(crate) fn activated(&mut self, now: Instant) {
        self.since = Some(now);
    }

    /// A Deactivation-Reactivation Sequence starts: no keep-alive until it ends.
    pub(crate) fn deactivated(&mut self) {
        self.since = None;
    }

    /// Something was sent to the server at `now`: input, a response, a keep-alive.
    pub(crate) fn sent(&mut self, now: Instant) {
        if let Some(since) = self.since.as_mut() {
            *since = now;
        }
    }

    /// When the next keep-alive is due; `None` while not active, or with no interval.
    pub(crate) fn due(&self) -> Option<Instant> {
        if self.every.is_zero() {
            return None;
        }
        self.since?.checked_add(self.every)
    }

    /// Whether a keep-alive is due at `now`.
    #[cfg(test)]
    fn is_due(&self, now: Instant) -> bool {
        self.due().is_some_and(|due| now >= due)
    }
}

/// The keep-alive frame for `stage`: a Client Synchronize PDU on the I/O channel, from and
/// targeting `user_channel_id`, as the connection finalization sends it, under the share
/// the stage is in now.
///
/// # Errors
///
/// The PDU could not be encoded.
pub(crate) fn frame(stage: &ActiveStage, user_channel_id: u16) -> Result<Vec<u8>, String> {
    let mut buffer = WriteBuf::new();
    stage
        .encode_static(
            &mut buffer,
            ShareDataPdu::Synchronize(SynchronizePdu {
                target_user_id: user_channel_id,
            }),
        )
        .map_err(|error| error.kind().to_string())?;
    Ok(buffer.into_inner())
}

#[cfg(test)]
mod tests {
    use ironrdp::session::ActiveStageBuilder;
    use ironrdp::svc::StaticChannelSet;

    use super::*;

    const EVERY: Duration = Duration::from_secs(60);

    #[test]
    fn none_is_due_before_the_activation() {
        let mut keep_alive = KeepAlive::new(EVERY);
        let start = Instant::now();
        keep_alive.sent(start);
        assert_eq!(keep_alive.due(), None);
        assert!(!keep_alive.is_due(start + EVERY * 10));
    }

    #[test]
    fn one_is_due_after_the_interval_of_silence() {
        let mut keep_alive = KeepAlive::new(EVERY);
        let start = Instant::now();
        keep_alive.activated(start);
        assert_eq!(keep_alive.due(), Some(start + EVERY));
        assert!(!keep_alive.is_due(start + EVERY - Duration::from_millis(1)));
        assert!(keep_alive.is_due(start + EVERY));
    }

    #[test]
    fn anything_sent_puts_it_back_by_the_interval() {
        let mut keep_alive = KeepAlive::new(EVERY);
        let start = Instant::now();
        keep_alive.activated(start);
        let typed = start + Duration::from_secs(45);
        keep_alive.sent(typed);
        assert!(!keep_alive.is_due(start + EVERY));
        assert_eq!(keep_alive.due(), Some(typed + EVERY));
        // The keep-alive itself counts: the next one is an interval later.
        let kept = typed + EVERY;
        keep_alive.sent(kept);
        assert_eq!(keep_alive.due(), Some(kept + EVERY));
    }

    #[test]
    fn none_is_due_during_a_reactivation_and_the_interval_restarts_after() {
        let mut keep_alive = KeepAlive::new(EVERY);
        let start = Instant::now();
        keep_alive.activated(start);
        keep_alive.deactivated();
        assert_eq!(keep_alive.due(), None);
        // The reactivation's own PDUs do not start it again.
        keep_alive.sent(start + EVERY);
        assert!(!keep_alive.is_due(start + EVERY * 3));
        let back = start + EVERY * 3;
        keep_alive.activated(back);
        assert_eq!(keep_alive.due(), Some(back + EVERY));
    }

    #[test]
    fn a_zero_interval_sends_none() {
        let mut keep_alive = KeepAlive::new(Duration::ZERO);
        let start = Instant::now();
        keep_alive.activated(start);
        assert_eq!(keep_alive.due(), None);
    }

    #[test]
    fn the_frame_is_a_client_synchronize_pdu_on_the_io_channel() {
        let stage = ActiveStageBuilder {
            static_channels: StaticChannelSet::new(),
            user_channel_id: 1007,
            io_channel_id: 1003,
            message_channel_id: None,
            share_id: 0x0001_03EA,
            compression_type: None,
            enable_server_pointer: false,
            pointer_software_rendering: false,
        }
        .build();
        let expected: [u8; 36] = [
            // TPKT: version 3, 36 bytes.
            0x03, 0x00, 0x00, 0x24, //
            // X.224 Data TPDU.
            0x02, 0xF0, 0x80, //
            // MCS Send Data Request: initiator 1007 (6 above 1001), channel 1003, high
            // priority, whole, 22 bytes of user data.
            0x64, 0x00, 0x06, 0x03, 0xEB, 0x70, 0x16, //
            // TS_SHARECONTROLHEADER: 22 bytes, PDUTYPE_DATAPDU of version 1, from 1007.
            0x16, 0x00, 0x17, 0x00, 0xEF, 0x03, //
            // TS_SHAREDATAHEADER: the share, padding, STREAM_MED, 4 bytes uncompressed,
            // PDUTYPE2_SYNCHRONIZE, no compression.
            0xEA, 0x03, 0x01, 0x00, 0x00, 0x02, 0x04, 0x00, 0x1F, 0x00, 0x00, 0x00, //
            // TS_SYNCHRONIZE_PDU: SYNCMSGTYPE_SYNC, target user 1007.
            0x01, 0x00, 0xEF, 0x03,
        ];
        assert_eq!(frame(&stage, 1007).expect("encoded"), expected);
    }

    #[test]
    fn the_frame_follows_the_share_of_a_reactivation() {
        let mut stage = ActiveStageBuilder {
            static_channels: StaticChannelSet::new(),
            user_channel_id: 1007,
            io_channel_id: 1003,
            message_channel_id: None,
            share_id: 1,
            compression_type: None,
            enable_server_pointer: false,
            pointer_software_rendering: false,
        }
        .build();
        stage.set_share_id(0x0A0B_0C0D);
        let frame = frame(&stage, 1007).expect("encoded");
        assert_eq!(frame[20..24], [0x0D, 0x0C, 0x0B, 0x0A]);
    }
}
