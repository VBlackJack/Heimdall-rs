use core::mem;

use ironrdp_core::WriteBuf;
use ironrdp_pdu::PduHint;
use ironrdp_pdu::rdp::capability_sets::SERVER_CHANNEL_ID;
use ironrdp_pdu::rdp::headers::ShareDataPdu;
use ironrdp_pdu::rdp::{finalization_messages, server_error_info};
use tracing::{debug, warn};

use crate::{
    ConnectorError, ConnectorErrorExt as _, ConnectorResult, Sequence, State, Written, general_err, reason_err,
};

#[derive(Default, Debug, Copy, Clone)]
#[non_exhaustive]
pub enum ConnectionFinalizationState {
    #[default]
    Consumed,

    SendSynchronize,
    SendControlCooperate,
    SendRequestControl,
    SendFontList,

    WaitForResponse,

    Finished,
}

impl State for ConnectionFinalizationState {
    fn name(&self) -> &'static str {
        match self {
            Self::Consumed => "Consumed",
            Self::SendSynchronize => "SendSynchronize",
            Self::SendControlCooperate => "SendControlCooperate",
            Self::SendRequestControl => "SendRequestControl",
            Self::SendFontList => "SendFontList",
            Self::WaitForResponse => "WaitForResponse",
            Self::Finished => "Finished",
        }
    }

    fn is_terminal(&self) -> bool {
        matches!(self, Self::Finished)
    }

    fn as_any(&self) -> &dyn core::any::Any {
        self
    }
}

#[derive(Debug, Copy, Clone)]
pub struct ConnectionFinalizationSequence {
    pub state: ConnectionFinalizationState,
    pub io_channel_id: u16,
    pub user_channel_id: u16,
    pub share_id: u32,
}

impl ConnectionFinalizationSequence {
    pub fn new(io_channel_id: u16, user_channel_id: u16, share_id: u32) -> Self {
        Self {
            state: ConnectionFinalizationState::SendSynchronize,
            io_channel_id,
            user_channel_id,
            share_id,
        }
    }
}

impl Sequence for ConnectionFinalizationSequence {
    fn next_pdu_hint(&self) -> Option<&dyn PduHint> {
        match self.state {
            ConnectionFinalizationState::Consumed => None,
            ConnectionFinalizationState::SendSynchronize => None,
            ConnectionFinalizationState::SendControlCooperate => None,
            ConnectionFinalizationState::SendRequestControl => None,
            ConnectionFinalizationState::SendFontList => None,
            ConnectionFinalizationState::WaitForResponse => Some(&ironrdp_pdu::X224_HINT),
            ConnectionFinalizationState::Finished => None,
        }
    }

    fn state(&self) -> &dyn State {
        &self.state
    }

    fn step(&mut self, input: &[u8], output: &mut WriteBuf) -> ConnectorResult<Written> {
        let (written, next_state) = match mem::take(&mut self.state) {
            ConnectionFinalizationState::Consumed => {
                return Err(general_err!(
                    "connection finalization sequence state is consumed (this is a bug)",
                ));
            }

            ConnectionFinalizationState::SendSynchronize => {
                let message = ShareDataPdu::Synchronize(finalization_messages::SynchronizePdu {
                    target_user_id: self.user_channel_id,
                });

                debug!(?message, "Send");

                let written = ironrdp_pdu::rdp::headers::encode_share_data(
                    self.user_channel_id,
                    self.io_channel_id,
                    self.share_id,
                    message,
                    output,
                )
                .map_err(ConnectorError::encode)?;

                (
                    Written::from_size(written)?,
                    ConnectionFinalizationState::SendControlCooperate,
                )
            }

            ConnectionFinalizationState::SendControlCooperate => {
                let message = ShareDataPdu::Control(finalization_messages::ControlPdu {
                    action: finalization_messages::ControlAction::Cooperate,
                    grant_id: 0,
                    control_id: 0,
                });

                debug!(?message, "Send");

                let written = ironrdp_pdu::rdp::headers::encode_share_data(
                    self.user_channel_id,
                    self.io_channel_id,
                    self.share_id,
                    message,
                    output,
                )
                .map_err(ConnectorError::encode)?;

                (
                    Written::from_size(written)?,
                    ConnectionFinalizationState::SendRequestControl,
                )
            }

            ConnectionFinalizationState::SendRequestControl => {
                let message = ShareDataPdu::Control(finalization_messages::ControlPdu {
                    action: finalization_messages::ControlAction::RequestControl,
                    grant_id: 0,
                    control_id: 0,
                });

                debug!(?message, "Send");

                let written = ironrdp_pdu::rdp::headers::encode_share_data(
                    self.user_channel_id,
                    self.io_channel_id,
                    self.share_id,
                    message,
                    output,
                )
                .map_err(ConnectorError::encode)?;

                (Written::from_size(written)?, ConnectionFinalizationState::SendFontList)
            }

            ConnectionFinalizationState::SendFontList => {
                let message = ShareDataPdu::FontList(finalization_messages::FontPdu::default());

                debug!(?message, "Send");

                let written = ironrdp_pdu::rdp::headers::encode_share_data(
                    self.user_channel_id,
                    self.io_channel_id,
                    self.share_id,
                    message,
                    output,
                )
                .map_err(ConnectorError::encode)?;

                (
                    Written::from_size(written)?,
                    ConnectionFinalizationState::WaitForResponse,
                )
            }

            ConnectionFinalizationState::WaitForResponse => {
                let ctx = ironrdp_pdu::mcs::decode_send_data_indication(input).map_err(ConnectorError::decode)?;
                // Heimdall-rs patch: a bulk-compressed PDU here is refused, never decoded as
                // plain data nor skipped (it would leave the session's history out of step).
                ensure_uncompressed(ctx.user_data)?;
                let ctx = ironrdp_pdu::rdp::headers::decode_share_data(ctx).map_err(ConnectorError::decode)?;

                debug!(message = ?ctx.pdu, "Received");

                let next_state = match ctx.pdu {
                    ShareDataPdu::Synchronize(_) => {
                        debug!("Server Synchronize");
                        ConnectionFinalizationState::WaitForResponse
                    }
                    ShareDataPdu::Control(control_pdu) => match control_pdu.action {
                        finalization_messages::ControlAction::Cooperate => {
                            if control_pdu.grant_id == 0 && control_pdu.control_id == 0 {
                                debug!("Server Control (Cooperate)");
                            } else {
                                warn!(
                                    control_pdu.grant_id,
                                    control_pdu.control_id,
                                    user_channel_id = self.user_channel_id,
                                    "Server Control (Cooperate) has non-zero grant_id or control_id",
                                );
                            }
                            ConnectionFinalizationState::WaitForResponse
                        }
                        finalization_messages::ControlAction::GrantedControl => {
                            debug!(
                                control_pdu.grant_id,
                                control_pdu.control_id,
                                user_channel_id = self.user_channel_id,
                                SERVER_CHANNEL_ID
                            );

                            if control_pdu.grant_id != self.user_channel_id {
                                warn!(
                                    "Server Control (Granted Control) had invalid grant_id, expected {}, but got {}",
                                    self.user_channel_id, control_pdu.grant_id
                                );
                            }

                            if control_pdu.control_id != u32::from(SERVER_CHANNEL_ID) {
                                warn!(
                                    "Server Control (Granted Control) had invalid control_id, expected {}, but got {}",
                                    SERVER_CHANNEL_ID, control_pdu.control_id
                                );
                            }

                            ConnectionFinalizationState::WaitForResponse
                        }
                        _ => return Err(general_err!("unexpected control action")),
                    },
                    ShareDataPdu::ServerSetErrorInfo(server_error_info::ServerSetErrorInfoPdu(error_info)) => {
                        match error_info {
                            server_error_info::ErrorInfo::ProtocolIndependentCode(
                                server_error_info::ProtocolIndependentCode::None,
                            ) => ConnectionFinalizationState::WaitForResponse,
                            _ => {
                                return Err(reason_err!(
                                    "ServerSetErrorInfo",
                                    "server returned error info: {}",
                                    error_info.description()
                                ));
                            }
                        }
                    }
                    ShareDataPdu::FontMap(_) => {
                        // https://learn.microsoft.com/en-us/openspecs/windows_protocols/ms-rdpbcgr/023f1e69-cfe8-4ee6-9ee0-7e759fb4e4ee
                        //
                        // Once the client has sent the Confirm Active PDU, it can start
                        // sending mouse and keyboard input to the server, and upon receipt
                        // of the Font List PDU the server can start sending graphics
                        // output to the client.

                        ConnectionFinalizationState::Finished
                    }
                    _ => return Err(general_err!("unexpected server message")),
                };

                (Written::Nothing, next_state)
            }

            ConnectionFinalizationState::Finished => return Err(general_err!("finalization already finished")),
        };

        self.state = next_state;

        Ok(written)
    }
}

/// Offsets in a Share Control PDU's bytes ([MS-RDPBCGR] 2.2.8.1.1.1.1 and 2.2.8.1.1.1.2):
/// `pduType`, then the Share Data Header's `compressedType`, past the 10 bytes of the Share
/// Control Header and the 5 of `pad1`, `streamId`, `uncompressedLength` and `pduType2`.
const PDU_TYPE_OFFSET: usize = 2;
const COMPRESSED_TYPE_OFFSET: usize = 15;
/// `pduType` of a Data PDU, in its low four bits.
const PDU_TYPE_DATA: u16 = 0x7;
const PDU_TYPE_MASK: u16 = 0xF;
/// `PACKET_COMPRESSED`, `PACKET_AT_FRONT` and `PACKET_FLUSHED`.
const BULK_FLAGS_MASK: u8 = 0xE0;

/// Fails on a Data PDU carrying bulk compression flags. The finalization PDUs (Synchronize,
/// Control, Font Map, Set Error Info) are small and sent uncompressed by the servers met so
/// far; one compressed would need the session's decompressor, which does not exist yet at
/// this point. Heimdall-rs patch, see `vendor/PATCHES.md`.
fn ensure_uncompressed(user_data: &[u8]) -> ConnectorResult<()> {
    let (Some(pdu_type), Some(compressed_type)) = (
        user_data.get(PDU_TYPE_OFFSET..PDU_TYPE_OFFSET + 2),
        user_data.get(COMPRESSED_TYPE_OFFSET),
    ) else {
        // Too short for a Data PDU: the decoder reports it.
        return Ok(());
    };
    let pdu_type = u16::from_le_bytes([pdu_type[0], pdu_type[1]]);
    if pdu_type & PDU_TYPE_MASK == PDU_TYPE_DATA && compressed_type & BULK_FLAGS_MASK != 0 {
        return Err(reason_err!(
            "ConnectionFinalization",
            "bulk-compressed PDU (compressedType {compressed_type:#04x}) during the connection finalization"
        )
        .with_source(CompressedFinalizationPdu));
    }
    Ok(())
}

/// The source of the error [`ensure_uncompressed`] fails with, so that a caller can tell this
/// refusal from any other failure by its type and connect again without compression.
/// Heimdall-rs patch, see `vendor/PATCHES.md`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CompressedFinalizationPdu;

impl core::fmt::Display for CompressedFinalizationPdu {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str("the server bulk-compressed a PDU of the connection finalization")
    }
}

impl core::error::Error for CompressedFinalizationPdu {}
