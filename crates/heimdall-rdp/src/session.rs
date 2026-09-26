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

use ironrdp::connector::connection_activation::{
    ConnectionActivationFactory, ConnectionActivationState,
};
use ironrdp::connector::{ConnectionResult, Sequence as _};
use ironrdp::graphics::image_processing::PixelFormat;
use ironrdp::input::{Database, Operation};
use ironrdp::session::fast_path::ProcessorBuilder;
use ironrdp::session::image::DecodedImage;
use ironrdp::session::{
    ActiveStage, ActiveStageBuilder, ActiveStageOutput, GracefulDisconnectReason,
};
use ironrdp_core::WriteBuf;
use tokio::io::{AsyncWriteExt as _, ReadHalf, WriteHalf};
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use crate::connect::{MAX_DESKTOP_SIDE, RdpConnection, Upgraded};
use crate::frames::FrameReader;

/// Events queued before the session waits for the receiver.
const EVENT_QUEUE: usize = 64;

/// Input batches queued before a sender waits.
const INPUT_QUEUE: usize = 64;

/// The decoded desktop, shared between the session and whoever draws it.
#[derive(Clone)]
pub struct Framebuffer(Arc<Mutex<DecodedImage>>);

impl Framebuffer {
    fn new(width: u16, height: u16) -> Self {
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
    /// The session ended; nothing follows.
    Closed(CloseReason),
}

/// Why a session ended.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CloseReason {
    /// The user logged off or the server ended the session.
    Server,
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
    /// Keyboard and mouse input.
    pub input: mpsc::Sender<Vec<Operation>>,
}

/// Starts the session of `connection`; `cancel` ends it.
#[must_use]
pub fn start(connection: RdpConnection, cancel: CancellationToken) -> RdpSession {
    let RdpConnection { framed, result } = connection;
    let framebuffer = Framebuffer::new(result.desktop_size.width, result.desktop_size.height);
    let (events, event_receiver) = mpsc::channel(EVENT_QUEUE);
    let (input, input_receiver) = mpsc::channel(INPUT_QUEUE);
    let (stream, leftover) = framed.into_inner();
    let (read_half, write_half) = tokio::io::split(stream);
    let running = Running {
        framebuffer: framebuffer.clone(),
        events,
        input: input_receiver,
        reader: FrameReader::new(read_half, leftover),
        writer: write_half,
    };
    tokio::spawn(running.run(result, cancel));
    RdpSession {
        framebuffer,
        events: event_receiver,
        input,
    }
}

struct Running {
    framebuffer: Framebuffer,
    events: mpsc::Sender<RdpEvent>,
    input: mpsc::Receiver<Vec<Operation>>,
    reader: FrameReader<ReadHalf<Upgraded>>,
    writer: WriteHalf<Upgraded>,
}

impl Running {
    async fn run(mut self, result: ConnectionResult, cancel: CancellationToken) {
        let reason = match self.serve(result, cancel).await {
            Ok(reason) => reason,
            Err(description) => CloseReason::Failed(description),
        };
        let _ = self.events.send(RdpEvent::Closed(reason)).await;
    }

    async fn serve(
        &mut self,
        result: ConnectionResult,
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
                    let mut image = self.framebuffer.0.lock().unwrap_or_else(PoisonError::into_inner);
                    stage.process(&mut image, action, &frame).map_err(|error| error.to_string())?
                }
                Some(operations) = self.input.recv() => {
                    let events = keys.apply(operations);
                    let mut image = self.framebuffer.0.lock().unwrap_or_else(PoisonError::into_inner);
                    stage
                        .process_fastpath_input(&mut image, &events)
                        .map_err(|error| error.to_string())?
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
                            GracefulDisconnectReason::ServerInitiated
                            | GracefulDisconnectReason::Other(_) => CloseReason::Server,
                        });
                    }
                    ActiveStageOutput::DeactivateAll => {
                        self.reactivate(&activation, &mut stage).await?;
                    }
                    _ => {}
                }
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
