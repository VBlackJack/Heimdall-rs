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

//! A running RDP session: the server's graphics decoded into a framebuffer, input sent back.

use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use ironrdp::connector::connection_activation::{
    ConnectionActivationFactory, ConnectionActivationState,
};
use ironrdp::connector::{ConnectionResult, Sequence as _};
use ironrdp::displaycontrol::pdu::MonitorLayoutEntry;
use ironrdp::graphics::image_processing::PixelFormat;
use ironrdp::input::{Database, Operation};
use ironrdp::pdu::Action;
use ironrdp::session::fast_path::ProcessorBuilder;
use ironrdp::session::image::DecodedImage;
use ironrdp::session::{
    ActiveStage, ActiveStageBuilder, ActiveStageOutput, GracefulDisconnectReason,
};
use ironrdp_core::WriteBuf;
use tokio::io::{AsyncWriteExt as _, ReadHalf, WriteHalf};
use tokio::sync::{mpsc, watch};
use tokio::time::Instant;
use tokio_util::sync::CancellationToken;

use ironrdp::cliprdr::CliprdrClient;
use ironrdp::cliprdr::pdu::ClipboardFormatId;
use zeroize::Zeroizing;

use crate::clipboard::{Offered, Request, offered_formats};
use crate::connect::{ClipboardLink, MAX_DESKTOP_SIDE, RdpConnection, Upgraded};
use crate::frames::FrameReader;
use crate::reason::{self, Ending};

/// Events queued before the session waits for the receiver.
const EVENT_QUEUE: usize = 64;

/// The decoded desktop, shared between the session and whoever draws it.
#[derive(Clone)]
pub struct Framebuffer(Arc<Mutex<DecodedImage>>);

impl Framebuffer {
    /// A black desktop of `width` by `height` pixels.
    #[must_use]
    pub fn new(width: u16, height: u16) -> Self {
        Self(Arc::new(Mutex::new(DecodedImage::new(
            PixelFormat::RgbA32,
            width,
            height,
        ))))
    }

    /// Calls `read` with the width, height and RGBA pixels, rows top to bottom.
    pub fn read<T>(&self, read: impl FnOnce(u16, u16, &[u8]) -> T) -> T {
        let image = self.0.lock().unwrap_or_else(PoisonError::into_inner);
        read(image.width(), image.height(), image.data())
    }
}

/// What the session reports.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RdpEvent {
    /// Pixels changed in this rectangle.
    Updated {
        /// Left.
        x: u16,
        /// Top.
        y: u16,
        /// Width.
        width: u16,
        /// Height.
        height: u16,
    },
    /// The desktop changed size; the framebuffer was reallocated.
    Resized {
        /// Width.
        width: u16,
        /// Height.
        height: u16,
    },
    /// The server's clipboard, as text: the server copied it.
    RemoteClipboard(Zeroizing<String>),
    /// The session ended; nothing follows.
    Closed(CloseReason),
}

/// Why a session ended.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CloseReason {
    /// The user logged off or the server ended the session.
    Server,
    /// The server ended the session and said why.
    Disconnected(Ending),
    /// Stopped from this side.
    Local,
    /// The connection failed; a description.
    Failed(String),
}

/// A session running in its own task.
pub struct RdpSession {
    /// The desktop.
    pub framebuffer: Framebuffer,
    /// What the session reports; closes after [`RdpEvent::Closed`].
    pub events: mpsc::Receiver<RdpEvent>,
    /// Keyboard and mouse input. Unbounded, so a key is never dropped while the session
    /// is busy drawing.
    pub input: mpsc::UnboundedSender<Vec<Operation>>,
    /// The desktop size wanted: the session asks the server for it once it has not changed
    /// for [`RESIZE_SETTLE`], so dragging a window edge sends one request, not hundreds.
    pub size: watch::Sender<Option<(u16, u16)>>,
    /// Text this side's clipboard holds, to offer the server; `None` when the clipboard is
    /// not shared.
    pub clipboard: Option<mpsc::UnboundedSender<Zeroizing<String>>>,
}

/// How long a wanted size must hold before the server is asked for it.
pub const RESIZE_SETTLE: Duration = Duration::from_millis(300);

/// How soon a size is asked again when the server's display channel was not open yet.
const RESIZE_RETRY: Duration = Duration::from_millis(500);

/// Starts the session of `connection`; `cancel` ends it.
#[must_use]
pub fn start(connection: RdpConnection, cancel: CancellationToken) -> RdpSession {
    let RdpConnection {
        framed,
        clipboard,
        result,
        desktop_scale,
    } = connection;
    let framebuffer = Framebuffer::new(result.desktop_size.width, result.desktop_size.height);
    let (events, event_receiver) = mpsc::channel(EVENT_QUEUE);
    let (input, input_receiver) = mpsc::unbounded_channel();
    let (size, size_receiver) = watch::channel(None);
    let (offers, offer_receiver) = mpsc::unbounded_channel();
    let shared = clipboard.map(|link| Shared {
        link,
        offers: offer_receiver,
    });
    let (stream, leftover) = framed.into_inner();
    let (read_half, write_half) = tokio::io::split(stream);
    let running = Running {
        framebuffer: framebuffer.clone(),
        events,
        input: input_receiver,
        size: size_receiver,
        wanted: None,
        asked: None,
        settle: None,
        desktop_scale,
        reader: FrameReader::new(read_half, leftover),
        writer: write_half,
    };
    let offers = shared.is_some().then_some(offers);
    tokio::spawn(running.run(result, shared, cancel));
    RdpSession {
        framebuffer,
        events: event_receiver,
        input,
        size,
        clipboard: offers,
    }
}

/// The session's side of a shared clipboard.
struct Shared {
    link: ClipboardLink,
    /// Text offered from this side.
    offers: mpsc::UnboundedReceiver<Zeroizing<String>>,
}

/// What the loop has to do for the clipboard next.
enum ClipboardStep {
    Request(Request),
    Offer(Zeroizing<String>),
}

impl Shared {
    async fn next(&mut self) -> Option<ClipboardStep> {
        tokio::select! {
            Some(request) = self.link.requests.recv() => Some(ClipboardStep::Request(request)),
            Some(text) = self.offers.recv() => Some(ClipboardStep::Offer(text)),
            else => None,
        }
    }
}

/// Waits for the next clipboard step; forever when the clipboard is not shared.
async fn next_clipboard_step(shared: &mut Option<Shared>) -> Option<ClipboardStep> {
    match shared {
        Some(shared) => shared.next().await,
        None => std::future::pending().await,
    }
}

/// Waits until `deadline`; the caller polls it only when there is one.
async fn sleep_until_settled(deadline: Option<Instant>) {
    if let Some(deadline) = deadline {
        tokio::time::sleep_until(deadline).await;
    }
}

/// X.224 length indicator of a Data TPDU.
const X224_DATA_LENGTH: u8 = 2;

/// X.224 Data TPDU code.
const X224_DATA: u8 = 0xF0;

/// Index of `disconnectProviderUltimatum` among the T.125 `DomainMCSPDU` choices.
const MCS_DISCONNECT_PROVIDER_ULTIMATUM: u8 = 8;

/// Whether `frame` is the server's MCS Disconnect Provider Ultimatum: it closes the
/// session. `IronRDP` 0.11 does not decode the short form xrdp sends
/// (`03 00 00 09 02 f0 80 21 80`), so it is recognised here, by its header alone.
fn is_disconnect_ultimatum(action: Action, frame: &[u8]) -> bool {
    // TPKT (4 bytes), X.224 Data (length indicator, code, end of TSDU), then the MCS PDU,
    // whose choice index takes the top six bits of its first byte.
    action == Action::X224
        && frame.get(4) == Some(&X224_DATA_LENGTH)
        && frame.get(5) == Some(&X224_DATA)
        && frame.get(7).map(|byte| byte >> 2) == Some(MCS_DISCONNECT_PROVIDER_ULTIMATUM)
}

/// A session error in plain words: its kind, without the source location `IronRDP` adds.
fn described(error: &ironrdp::session::SessionError) -> String {
    error.kind().to_string()
}

struct Running {
    framebuffer: Framebuffer,
    events: mpsc::Sender<RdpEvent>,
    input: mpsc::UnboundedReceiver<Vec<Operation>>,
    size: watch::Receiver<Option<(u16, u16)>>,
    /// A size to ask the server for, once [`Running::settle`] passes.
    wanted: Option<(u32, u32)>,
    /// The desktop scale factor of the connection, in percent, asked again with each size.
    desktop_scale: u32,
    /// The last size asked for, kept: a reactivation (the logon after a login screen, a
    /// reconnection) brings the server's own size back, and it is asked again then.
    asked: Option<(u32, u32)>,
    settle: Option<Instant>,
    reader: FrameReader<ReadHalf<Upgraded>>,
    writer: WriteHalf<Upgraded>,
}

impl Running {
    async fn run(
        mut self,
        result: ConnectionResult,
        shared: Option<Shared>,
        cancel: CancellationToken,
    ) {
        let reason = match self.serve(result, shared, cancel).await {
            Ok(reason) => reason,
            // A Set Error Info ending a reactivation is the server ending the session.
            Err(description) => match reason::ending_in_failure(&description) {
                Some(Ending::Logoff) => CloseReason::Server,
                Some(ending) => CloseReason::Disconnected(ending),
                None => CloseReason::Failed(description),
            },
        };
        let _ = self.events.send(RdpEvent::Closed(reason)).await;
    }

    async fn serve(
        &mut self,
        result: ConnectionResult,
        mut shared: Option<Shared>,
        cancel: CancellationToken,
    ) -> Result<CloseReason, String> {
        let activation = result.activation_factory;
        let mut stage = ActiveStageBuilder {
            static_channels: result.static_channels,
            user_channel_id: result.user_channel_id,
            io_channel_id: result.io_channel_id,
            message_channel_id: result.message_channel_id,
            share_id: result.share_id,
            compression_type: result.compression_type,
            enable_server_pointer: result.enable_server_pointer,
            pointer_software_rendering: result.pointer_software_rendering,
        }
        .build();
        let mut keys = Database::new();
        loop {
            let outputs = tokio::select! {
                () = cancel.cancelled() => {
                    // A polite end first; the connection closes either way.
                    if let Ok(outputs) = stage.graceful_shutdown() {
                        let _ = self.answer(outputs).await;
                    }
                    return Ok(CloseReason::Local);
                }
                frame = self.reader.read() => {
                    let (action, frame) = frame.map_err(|error| error.to_string())?;
                    if is_disconnect_ultimatum(action, &frame) {
                        return Ok(CloseReason::Server);
                    }
                    let mut image = self.framebuffer.0.lock().unwrap_or_else(PoisonError::into_inner);
                    stage.process(&mut image, action, &frame).map_err(|error| described(&error))?
                }
                changed = self.size.changed() => {
                    if changed.is_ok() {
                        if let Some((width, height)) = *self.size.borrow_and_update() {
                            self.wanted = Some((u32::from(width), u32::from(height)));
                            self.settle = Some(Instant::now() + RESIZE_SETTLE);
                        }
                    } else {
                        // Nobody sets a size any more: stop listening for one.
                        self.size = watch::channel(None).1;
                    }
                    Vec::new()
                }
                () = sleep_until_settled(self.settle), if self.settle.is_some() => {
                    self.settle = None;
                    self.ask_for_size(&mut stage).await?;
                    Vec::new()
                }
                Some(operations) = self.input.recv() => {
                    let events = keys.apply(operations);
                    let mut image = self.framebuffer.0.lock().unwrap_or_else(PoisonError::into_inner);
                    stage
                        .process_fastpath_input(&mut image, &events)
                        .map_err(|error| described(&error))?
                }
                Some(step) = next_clipboard_step(&mut shared) => {
                    let offered = shared.as_ref().map(|shared| shared.link.offered.clone());
                    self.clipboard(&mut stage, step, offered).await?;
                    Vec::new()
                }
            };
            for output in outputs {
                match output {
                    ActiveStageOutput::ResponseFrame(frame) => self.send(&frame).await?,
                    ActiveStageOutput::GraphicsUpdate(region) => {
                        let _ = self
                            .events
                            .send(RdpEvent::Updated {
                                x: region.left,
                                y: region.top,
                                width: region.right.saturating_sub(region.left) + 1,
                                height: region.bottom.saturating_sub(region.top) + 1,
                            })
                            .await;
                    }
                    ActiveStageOutput::Terminate(reason) => {
                        return Ok(match reason {
                            GracefulDisconnectReason::UserInitiated => CloseReason::Local,
                            GracefulDisconnectReason::ServerInitiated => CloseReason::Server,
                            GracefulDisconnectReason::Other(reason) => {
                                match reason::ending(&reason) {
                                    Ending::Logoff => CloseReason::Server,
                                    ending => CloseReason::Disconnected(ending),
                                }
                            }
                        });
                    }
                    ActiveStageOutput::DeactivateAll => {
                        self.reactivate(&activation, &mut stage).await?;
                        if self.wanted.is_none() && self.asked.is_some() {
                            self.wanted = self.asked;
                            self.settle = Some(Instant::now() + RESIZE_SETTLE);
                        }
                    }
                    _ => {}
                }
            }
        }
    }

    /// Does what the clipboard channel asked, or offers this side's new text.
    async fn clipboard(
        &mut self,
        stage: &mut ActiveStage,
        step: ClipboardStep,
        offered: Option<Offered>,
    ) -> Result<(), String> {
        let Some(offered) = offered else {
            return Ok(());
        };
        let Some(channel) = stage.get_svc_processor_mut::<CliprdrClient>() else {
            return Ok(());
        };
        let messages = match step {
            ClipboardStep::Request(Request::Received(text)) => {
                let _ = self.events.send(RdpEvent::RemoteClipboard(text)).await;
                return Ok(());
            }
            ClipboardStep::Request(Request::Paste) => {
                channel.initiate_paste(ClipboardFormatId::CF_UNICODETEXT)
            }
            ClipboardStep::Request(Request::Answer(answer)) => channel.submit_format_data(answer),
            ClipboardStep::Request(Request::Offer) => {
                channel.initiate_copy(&offered_formats(&offered))
            }
            ClipboardStep::Offer(text) => {
                *offered.lock().unwrap_or_else(PoisonError::into_inner) = Some(text);
                channel.initiate_copy(&offered_formats(&offered))
            }
        }
        .map_err(|error| error.to_string())?;
        let frame = stage
            .process_svc_processor_messages(messages)
            .map_err(|error| described(&error))?;
        self.send(&frame).await
    }

    /// Asks the server for the wanted size, unless the desktop has it already; when the
    /// display channel is not open yet, asks again a little later.
    async fn ask_for_size(&mut self, stage: &mut ActiveStage) -> Result<(), String> {
        let Some((width, height)) = self.wanted.take() else {
            return Ok(());
        };
        let (width, height) = MonitorLayoutEntry::adjust_display_size(width, height);
        self.asked = Some((width, height));
        let current = self.framebuffer.read(|current_width, current_height, _| {
            (u32::from(current_width), u32::from(current_height))
        });
        if (width, height) == current {
            return Ok(());
        }
        // The scale asked at the connection, kept: 100 is the server's own, said as none.
        let scale = (self.desktop_scale > 100).then_some(self.desktop_scale);
        match stage.encode_resize(width, height, scale, None) {
            Some(Ok(frame)) => self.send(&frame).await,
            // Not encodable: the size stays as it is.
            Some(Err(_)) => Ok(()),
            None => {
                self.wanted = Some((width, height));
                self.settle = Some(Instant::now() + RESIZE_RETRY);
                Ok(())
            }
        }
    }

    async fn send(&mut self, frame: &[u8]) -> Result<(), String> {
        self.writer
            .write_all(frame)
            .await
            .map_err(|error| error.to_string())?;
        self.writer.flush().await.map_err(|error| error.to_string())
    }

    async fn answer(&mut self, outputs: Vec<ActiveStageOutput>) -> Result<(), String> {
        for output in outputs {
            if let ActiveStageOutput::ResponseFrame(frame) = output {
                self.send(&frame).await?;
            }
        }
        Ok(())
    }

    /// Runs the Deactivation-Reactivation Sequence the server started: a logon, a
    /// resolution change. The desktop may change size.
    async fn reactivate(
        &mut self,
        activation: &ConnectionActivationFactory,
        stage: &mut ActiveStage,
    ) -> Result<(), String> {
        let mut sequence = activation.create();
        let mut buffer = WriteBuf::new();
        loop {
            buffer.clear();
            let written = if sequence.next_pdu_hint().is_some() {
                let (_, frame) = self
                    .reader
                    .read()
                    .await
                    .map_err(|error| error.to_string())?;
                sequence.step(&frame, &mut buffer)
            } else {
                sequence.step_no_input(&mut buffer)
            }
            .map_err(|error| error.to_string())?;
            if let Some(length) = written.size() {
                let frame = buffer.filled()[..length].to_vec();
                self.send(&frame).await?;
            }
            if sequence.state().is_terminal() {
                break;
            }
        }
        let ConnectionActivationState::Finalized {
            desktop_size,
            share_id,
            enable_server_pointer,
            pointer_software_rendering,
        } = sequence.connection_activation_state()
        else {
            return Err("the reactivation did not finish".to_owned());
        };
        if desktop_size.width > MAX_DESKTOP_SIDE || desktop_size.height > MAX_DESKTOP_SIDE {
            return Err(format!(
                "the server asked for a {}x{} desktop",
                desktop_size.width, desktop_size.height
            ));
        }
        stage.set_share_id(share_id);
        stage.set_enable_server_pointer(enable_server_pointer);
        stage.set_fastpath_processor(
            ProcessorBuilder {
                io_channel_id: activation.io_channel_id(),
                user_channel_id: activation.user_channel_id(),
                share_id,
                enable_server_pointer,
                pointer_software_rendering,
                bulk_decompressor: None,
            }
            .build(),
        );
        *self
            .framebuffer
            .0
            .lock()
            .unwrap_or_else(PoisonError::into_inner) =
            DecodedImage::new(PixelFormat::RgbA32, desktop_size.width, desktop_size.height);
        let _ = self
            .events
            .send(RdpEvent::Resized {
                width: desktop_size.width,
                height: desktop_size.height,
            })
            .await;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_server_s_disconnect_ultimatum_is_a_close_and_data_is_not() {
        // As xrdp sends it when its login window is cancelled.
        let ultimatum = [0x03, 0x00, 0x00, 0x09, 0x02, 0xF0, 0x80, 0x21, 0x80];
        assert!(is_disconnect_ultimatum(Action::X224, &ultimatum));
        // An MCS Send Data Indication (choice 26): ordinary traffic.
        let data = [
            0x03, 0x00, 0x00, 0x0C, 0x02, 0xF0, 0x80, 0x68, 0x00, 0x01, 0x03, 0xEB,
        ];
        assert!(!is_disconnect_ultimatum(Action::X224, &data));
        assert!(!is_disconnect_ultimatum(Action::FastPath, &ultimatum));
        assert!(!is_disconnect_ultimatum(Action::X224, &ultimatum[..7]));
    }
}
