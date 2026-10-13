use ironrdp_bulk::BulkCompressor;
use ironrdp_core::{WriteBuf, decode};
use ironrdp_dvc::{DrdynvcClient, DvcProcessor, DynamicVirtualChannel};
use ironrdp_pdu::mcs::{DisconnectProviderUltimatum, DisconnectReason, McsMessage, SendDataIndicationCtx};
use ironrdp_pdu::rdp::autodetect::{AutoDetectReqPdu, AutoDetectRequest, AutoDetectResponse, AutoDetectRspPdu};
use ironrdp_pdu::rdp::headers::ShareDataPdu;
use ironrdp_pdu::rdp::multitransport::MultitransportRequestPdu;
use ironrdp_pdu::rdp::server_error_info::{ErrorInfo, ProtocolIndependentCode, ServerSetErrorInfoPdu};
use ironrdp_pdu::x224::X224;
use ironrdp_svc::{StaticChannelSet, SvcMessage, SvcProcessor, SvcProcessorMessages, client_encode_svc_messages};
use tracing::debug;

use crate::{SessionError, SessionErrorExt as _, SessionResult, reason_err};

/// X224 Processor output
#[derive(Debug, Clone)]
pub enum ProcessorOutput {
    /// A buffer with encoded data to send to the server.
    ResponseFrame(Vec<u8>),
    /// A graceful disconnect notification. Client should close the connection upon receiving this.
    Disconnect(DisconnectDescription),
    /// Received a [`ironrdp_pdu::rdp::headers::ServerDeactivateAll`] PDU. Client should execute the
    /// [Deactivation-Reactivation Sequence].
    ///
    /// [Deactivation-Reactivation Sequence]: https://learn.microsoft.com/en-us/openspecs/windows_protocols/ms-rdpbcgr/dfc234ce-481a-4674-9a5d-2a7bafb14432
    DeactivateAll,
    /// Server Initiate Multitransport Request. The application should establish a
    /// sideband UDP transport using the request ID and security cookie, then send
    /// a [`MultitransportResponsePdu`] back on the IO channel.
    ///
    /// See [\[MS-RDPBCGR\] 2.2.15.1].
    ///
    /// [\[MS-RDPBCGR\] 2.2.15.1]: https://learn.microsoft.com/en-us/openspecs/windows_protocols/ms-rdpbcgr/de783158-8b01-4818-8fb0-62523a5b3490
    /// [`MultitransportResponsePdu`]: ironrdp_pdu::rdp::multitransport::MultitransportResponsePdu
    MultitransportRequest(MultitransportRequestPdu),
    /// Auto-detect network characteristics from server ([\[MS-RDPBCGR\] 2.2.14]).
    ///
    /// Currently only surfaces [`AutoDetectRequest::NetworkCharacteristicsResult`].
    /// RTT requests are handled internally with automatic responses.
    ///
    /// [\[MS-RDPBCGR\] 2.2.14]: https://learn.microsoft.com/en-us/openspecs/windows_protocols/ms-rdpbcgr/dc672839-4f4e-40b1-a71c-cd6a959baa38
    AutoDetect(AutoDetectRequest),
    /// Slow-path graphics update ([MS-RDPBCGR] 2.2.9.1.1.3).
    /// Raw update payload starting with `updateType(u16)`.
    GraphicsUpdate(Vec<u8>),
    /// Slow-path pointer update ([MS-RDPBCGR] 2.2.9.1.1.4).
    /// Raw pointer payload starting with `messageType(u16) + pad(u16)`.
    PointerUpdate(Vec<u8>),
}

#[derive(Debug, Clone)]
pub enum DisconnectDescription {
    /// Includes the reason from the MCS Disconnect Provider Ultimatum.
    /// This is the least-specific disconnect reason and is only used
    /// when a more specific disconnect code is not available.
    McsDisconnect(DisconnectReason),

    /// Includes the error information sent by the RDP server when there
    /// is a connection or disconnection failure.
    ErrorInfo(ErrorInfo),
}

pub struct Processor {
    static_channels: StaticChannelSet,
    user_channel_id: u16,
    io_channel_id: u16,
    message_channel_id: Option<u16>,
    share_id: u32,
}

impl Processor {
    pub fn new(
        static_channels: StaticChannelSet,
        user_channel_id: u16,
        io_channel_id: u16,
        message_channel_id: Option<u16>,
        share_id: u32,
    ) -> Self {
        Self {
            static_channels,
            user_channel_id,
            io_channel_id,
            message_channel_id,
            share_id,
        }
    }

    pub fn set_share_id(&mut self, share_id: u32) {
        self.share_id = share_id;
    }

    pub fn get_svc_processor<T: SvcProcessor + 'static>(&self) -> Option<&T> {
        self.static_channels
            .get_by_type::<T>()
            .and_then(|svc| svc.channel_processor_downcast_ref())
    }

    pub fn get_svc_processor_mut<T: SvcProcessor + 'static>(&mut self) -> Option<&mut T> {
        self.static_channels
            .get_by_type_mut::<T>()
            .and_then(|svc| svc.channel_processor_downcast_mut())
    }

    /// Completes user's SVC request with data, required to sent it over the network and returns
    /// a buffer with encoded data.
    pub fn process_svc_processor_messages<C: SvcProcessor + 'static>(
        &self,
        messages: SvcProcessorMessages<C>,
    ) -> SessionResult<Vec<u8>> {
        let channel_id = self
            .static_channels
            .get_channel_id_by_type::<C>()
            .ok_or_else(|| reason_err!("SVC", "channel not found"))?;

        process_svc_messages(messages.into(), channel_id, self.user_channel_id)
    }

    pub fn get_dvc<T: DvcProcessor + 'static>(&self) -> Option<&DynamicVirtualChannel> {
        self.get_svc_processor::<DrdynvcClient>()?.get_dvc_by_type_id::<T>()
    }

    pub fn get_dvc_by_channel_id(&self, channel_id: u32) -> Option<&DynamicVirtualChannel> {
        self.get_svc_processor::<DrdynvcClient>()?
            .get_dvc_by_channel_id(channel_id)
    }

    /// Processes a received PDU. Returns a vector of [`ProcessorOutput`] that must be processed
    /// in the returned order.
    pub fn process(&mut self, frame: &[u8]) -> SessionResult<Vec<ProcessorOutput>> {
        self.process_with_bulk(frame, None)
    }

    /// [`Processor::process`], a bulk-compressed slow-path data PDU decompressed first by
    /// `bulk`, the decompressor the fast-path updates use: the server compresses both with
    /// one history. Heimdall-rs patch, see `vendor/PATCHES.md`.
    pub fn process_with_bulk(
        &mut self,
        frame: &[u8],
        bulk: Option<&mut BulkCompressor>,
    ) -> SessionResult<Vec<ProcessorOutput>> {
        let data_ctx: SendDataIndicationCtx<'_> =
            ironrdp_pdu::mcs::decode_send_data_indication(frame).map_err(SessionError::decode)?;
        let channel_id = data_ctx.channel_id;

        if channel_id == self.io_channel_id {
            let decompressed;
            let data_ctx = match decompress_share_data(data_ctx.user_data, bulk)? {
                Some(data) => {
                    decompressed = data;
                    SendDataIndicationCtx {
                        user_data: &decompressed,
                        ..data_ctx
                    }
                }
                None => data_ctx,
            };
            self.process_io_channel(data_ctx)
        } else if self.message_channel_id == Some(channel_id) {
            self.process_message_channel(data_ctx)
        } else if let Some(svc) = self.static_channels.get_by_channel_id_mut(channel_id) {
            let response_pdus = svc.process(data_ctx.user_data).map_err(SessionError::pdu)?;
            process_svc_messages(response_pdus, channel_id, data_ctx.initiator_id)
                .map(|data| vec![ProcessorOutput::ResponseFrame(data)])
        } else {
            Err(reason_err!("X224", "unexpected channel received: ID {channel_id}"))
        }
    }

    fn process_io_channel(&self, data_ctx: SendDataIndicationCtx<'_>) -> SessionResult<Vec<ProcessorOutput>> {
        debug_assert_eq!(data_ctx.channel_id, self.io_channel_id);

        let io_channel = ironrdp_pdu::rdp::headers::decode_io_channel(data_ctx).map_err(SessionError::decode)?;

        match io_channel {
            ironrdp_pdu::rdp::headers::IoChannelPdu::Data(ctx) => {
                match ctx.pdu {
                    ShareDataPdu::SaveSessionInfo(session_info) => {
                        debug!("Got Session Save Info PDU: {session_info:?}");
                        Ok(Vec::new())
                    }
                    // FIXME: workaround fix to not terminate the session on "unhandled PDU: Set Keyboard Indicators PDU"
                    ShareDataPdu::SetKeyboardIndicators(data) => {
                        debug!("Got Keyboard Indicators PDU: {data:?}");
                        Ok(Vec::new())
                    }
                    ShareDataPdu::ServerSetErrorInfo(ServerSetErrorInfoPdu(ErrorInfo::ProtocolIndependentCode(
                        ProtocolIndependentCode::None,
                    ))) => {
                        debug!("Received None server error");
                        Ok(Vec::new())
                    }
                    ShareDataPdu::ServerSetErrorInfo(ServerSetErrorInfoPdu(e)) => {
                        // This is a part of server-side graceful disconnect procedure defined
                        // in [MS-RDPBCGR].
                        //
                        // [MS-RDPBCGR]: https://learn.microsoft.com/en-us/openspecs/windows_protocols/ms-rdpbcgr/149070b0-ecec-4c20-af03-934bbc48adb8
                        let desc = DisconnectDescription::ErrorInfo(e);
                        Ok(vec![ProcessorOutput::Disconnect(desc)])
                    }
                    ShareDataPdu::ShutdownDenied => {
                        debug!("ShutdownDenied received, session will be closed");

                        // As defined in [MS-RDPBCGR], when `ShareDataPdu::ShutdownDenied` is received, we
                        // need to send a disconnect ultimatum to the server if we want to proceed with the
                        // session shutdown.
                        //
                        // [MS-RDPBCGR]: https://learn.microsoft.com/en-us/openspecs/windows_protocols/ms-rdpbcgr/27915739-8f77-487e-9927-55008af7fd68
                        let ultimatum = McsMessage::DisconnectProviderUltimatum(
                            DisconnectProviderUltimatum::from_reason(DisconnectReason::UserRequested),
                        );

                        let encoded_pdu = ironrdp_core::encode_vec(&X224(ultimatum)).map_err(SessionError::encode);

                        Ok(vec![
                            ProcessorOutput::ResponseFrame(encoded_pdu?),
                            ProcessorOutput::Disconnect(DisconnectDescription::McsDisconnect(
                                DisconnectReason::UserRequested,
                            )),
                        ])
                    }
                    // Bulk-compressed slow-path payloads arrive here decompressed, by
                    // `process_with_bulk` (Heimdall-rs patch).
                    ShareDataPdu::Update(data) => {
                        debug!("Got slow-path graphics update ({} bytes)", data.len());
                        Ok(vec![ProcessorOutput::GraphicsUpdate(data)])
                    }
                    ShareDataPdu::Pointer(data) => {
                        debug!("Got slow-path pointer update ({} bytes)", data.len());
                        Ok(vec![ProcessorOutput::PointerUpdate(data)])
                    }
                    _ => Err(reason_err!(
                        "IO channel",
                        "unhandled PDU: {:?}",
                        ctx.pdu.as_short_name()
                    )),
                }
            }
            ironrdp_pdu::rdp::headers::IoChannelPdu::MultitransportRequest(pdu) => {
                debug!(
                    "Received Initiate Multitransport Request: request_id={}",
                    pdu.request_id
                );
                Ok(vec![ProcessorOutput::MultitransportRequest(pdu)])
            }
            ironrdp_pdu::rdp::headers::IoChannelPdu::DeactivateAll(_) => Ok(vec![ProcessorOutput::DeactivateAll]),
        }
    }

    /// Process an auto-detect request received on the MCS message channel.
    ///
    /// During continuous auto-detection ([MS-RDPBCGR] 2.2.14) the server sends
    /// RTT (and bandwidth) requests on the message channel; the client answers
    /// RTT requests and surfaces the final Network Characteristics Result.
    fn process_message_channel(&self, data_ctx: SendDataIndicationCtx<'_>) -> SessionResult<Vec<ProcessorOutput>> {
        let Some(message_channel_id) = self.message_channel_id else {
            return Err(reason_err!("message channel", "no message channel negotiated"));
        };

        let req = decode::<AutoDetectReqPdu>(data_ctx.user_data).map_err(SessionError::decode)?;

        match req.request {
            AutoDetectRequest::RttRequest { sequence_number, .. } => {
                let response = AutoDetectRspPdu::new(AutoDetectResponse::RttResponse { sequence_number });
                let mut frame = WriteBuf::new();
                ironrdp_pdu::mcs::encode_send_data_request(
                    self.user_channel_id,
                    message_channel_id,
                    &response,
                    &mut frame,
                )
                .map_err(SessionError::encode)?;
                debug!(sequence_number, "Responded to auto-detect RTT request");
                Ok(vec![ProcessorOutput::ResponseFrame(frame.into_inner())])
            }
            req @ AutoDetectRequest::NetworkCharacteristicsResult { .. } => {
                debug!(?req, "Received network characteristics from server");
                Ok(vec![ProcessorOutput::AutoDetect(req)])
            }
            req => {
                debug!(?req, "Auto-detect request not yet implemented");
                Ok(Vec::new())
            }
        }
    }

    /// Send a pdu on the static global channel. Typically used to send input events
    pub fn encode_static(&self, output: &mut WriteBuf, pdu: ShareDataPdu) -> SessionResult<usize> {
        let written = ironrdp_pdu::rdp::headers::encode_share_data(
            self.user_channel_id,
            self.io_channel_id,
            self.share_id,
            pdu,
            output,
        )
        .map_err(SessionError::encode)?;
        Ok(written)
    }
}

/// Processes a vector of [`SvcMessage`] in preparation for sending them to the server on the `channel_id` channel.
///
/// This includes chunkifying the messages, adding MCS, x224, and tpkt headers, and encoding them into a buffer.
/// The messages returned here are ready to be sent to the server.
///
/// The caller is responsible for ensuring that the `channel_id` corresponds to the correct channel.
/// Share Control Header: `totalLength`, `pduType`, `pduSource`, `shareId` ([MS-RDPBCGR] 2.2.8.1.1.1.1).
const SHARE_CONTROL_HEADER_LEN: usize = 10;
/// Share Data Header past the Share Control Header: `pad1`, `streamId`, `uncompressedLength`,
/// `pduType2`, `compressedType`, `compressedLength` ([MS-RDPBCGR] 2.2.8.1.1.1.2).
const SHARE_DATA_HEADER_LEN: usize = 8;
/// Both headers, which a compressed PDU's `compressedLength` counts with its data.
const SHARE_HEADERS_LEN: usize = SHARE_CONTROL_HEADER_LEN + SHARE_DATA_HEADER_LEN;
/// `pduType` of a Data PDU, in its low four bits.
const PDU_TYPE_DATA: u16 = 0x7;
const PDU_TYPE_MASK: u16 = 0xF;
/// Offsets of `compressedType` and `compressedLength` in the user data.
const COMPRESSED_TYPE_OFFSET: usize = SHARE_CONTROL_HEADER_LEN + 5;
const COMPRESSED_LENGTH_OFFSET: usize = SHARE_CONTROL_HEADER_LEN + 6;
/// `PACKET_COMPRESSED`, `PACKET_AT_FRONT` and `PACKET_FLUSHED` in `compressedType`.
const BULK_FLAGS_MASK: u8 = 0xE0;

/// The IO channel `user_data` of a bulk-compressed Data PDU, rewritten uncompressed: its
/// data decompressed by `bulk`, its `compressedType` and `compressedLength` zeroed and its
/// `totalLength` set to the new size, as FreeRDP's `rdp_recv_data_pdu` decompresses before
/// it reads the PDU. `None` when the PDU carries no compression flags. A PDU with compression
/// flags and no decompressor (compression not negotiated) is an error: its bytes are never
/// decoded as plain data. Bytes past `compressedLength` are dropped, as FreeRDP drops them.
fn decompress_share_data(user_data: &[u8], bulk: Option<&mut BulkCompressor>) -> SessionResult<Option<Vec<u8>>> {
    if user_data.len() < SHARE_HEADERS_LEN {
        return Ok(None);
    }
    let pdu_type = u16::from_le_bytes([user_data[2], user_data[3]]);
    if pdu_type & PDU_TYPE_MASK != PDU_TYPE_DATA {
        return Ok(None);
    }
    let compressed_type = user_data[COMPRESSED_TYPE_OFFSET];
    if compressed_type & BULK_FLAGS_MASK == 0 {
        return Ok(None);
    }
    let Some(bulk) = bulk else {
        return Err(reason_err!(
            "IO channel",
            "compressed slow-path PDU (compressedType {compressed_type:#04x}) without negotiated compression"
        ));
    };
    let compressed_length = usize::from(u16::from_le_bytes([
        user_data[COMPRESSED_LENGTH_OFFSET],
        user_data[COMPRESSED_LENGTH_OFFSET + 1],
    ]));
    let end = Some(compressed_length)
        .filter(|end| (SHARE_HEADERS_LEN..=user_data.len()).contains(end))
        .ok_or_else(|| {
            reason_err!(
                "IO channel",
                "compressedLength {compressed_length} out of the {} bytes received",
                user_data.len()
            )
        })?;
    let data = bulk
        .decompress(&user_data[SHARE_HEADERS_LEN..end], u32::from(compressed_type))
        .map_err(|e| reason_err!("IO channel", "bulk decompression failed: {e}"))?;
    let mut pdu = Vec::with_capacity(SHARE_HEADERS_LEN + data.len());
    pdu.extend_from_slice(&user_data[..SHARE_HEADERS_LEN]);
    pdu.extend_from_slice(data);
    let total_length =
        u16::try_from(pdu.len()).map_err(|_| reason_err!("IO channel", "decompressed PDU of {} bytes", pdu.len()))?;
    pdu[..2].copy_from_slice(&total_length.to_le_bytes());
    pdu[COMPRESSED_TYPE_OFFSET] = 0;
    pdu[COMPRESSED_LENGTH_OFFSET..SHARE_HEADERS_LEN].copy_from_slice(&0u16.to_le_bytes());
    Ok(Some(pdu))
}

fn process_svc_messages(messages: Vec<SvcMessage>, channel_id: u16, initiator_id: u16) -> SessionResult<Vec<u8>> {
    client_encode_svc_messages(messages, channel_id, initiator_id).map_err(SessionError::encode)
}
