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

use std::path::PathBuf;
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

use ring::digest::{SHA256, SHA256_OUTPUT_LEN, digest};

use ironrdp::cliprdr::CliprdrClient;
use ironrdp::cliprdr::pdu::{ClipboardFormatId, FileContentsRequest, FileContentsResponse};
use zeroize::Zeroizing;

use crate::clipboard::{ClipboardBackend, Request, offered_formats};
use crate::clipboard_files::{COPY_LIMITS, CopyRefusal, Entry, FileList};
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
    /// The files copied on this side were not offered to the server: too many, or too
    /// large.
    FilesRefused(CopyRefusal),
    /// The server cannot change the desktop's size while connected: it has no display
    /// channel, or refused the size. Only a new connection at that size brings it.
    ResizeRefused {
        /// Width asked.
        width: u16,
        /// Height asked.
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
    /// What this side's clipboard holds, to offer the server; `None` when the clipboard is
    /// not shared.
    pub clipboard: Option<mpsc::UnboundedSender<LocalClipboard>>,
}

/// What this side's clipboard holds, offered to the server.
#[derive(Debug)]
pub enum LocalClipboard {
    /// Text.
    Text(Zeroizing<String>),
    /// Files and folders copied, as Explorer lists them: offered when the server takes
    /// files, walked and read off the session's task.
    Files(Vec<PathBuf>),
}

/// How often the clipboard's locks and file requests are looked at, so a lock the server
/// left is released.
const CLIPBOARD_TIMEOUTS: Duration = Duration::from_secs(5);

/// How long a wanted size must hold before the server is asked for it.
pub const RESIZE_SETTLE: Duration = Duration::from_millis(300);

/// How soon a size is asked again when the server's display channel was not open yet.
const RESIZE_RETRY: Duration = Duration::from_millis(500);

/// How long a size is asked again for a display channel that does not open: a server
/// without one never changes size while connected.
const RESIZE_GIVE_UP: Duration = Duration::from_secs(10);

/// Whether a size still waiting for the display channel, first asked at `since`, is asked
/// again at `now`; `since` starts on the first wait and ends on giving up.
fn asks_again(since: &mut Option<Instant>, now: Instant) -> bool {
    let first = *since.get_or_insert(now);
    if now.duration_since(first) < RESIZE_GIVE_UP {
        true
    } else {
        *since = None;
        false
    }
}

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
    let shared = clipboard.map(|link| Shared::new(link, offer_receiver));
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
        waiting_since: None,
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
    /// What this side offers.
    offers: mpsc::UnboundedReceiver<LocalClipboard>,
    /// Files walked off the session's task.
    walked: (
        mpsc::UnboundedSender<Walked>,
        mpsc::UnboundedReceiver<Walked>,
    ),
    /// Answers to the server's file requests, read off the session's task.
    read: (
        mpsc::UnboundedSender<FileContentsResponse<'static>>,
        mpsc::UnboundedReceiver<FileContentsResponse<'static>>,
    ),
    /// What the server's clipboard holds from this side, or gave it.
    held: Held,
    /// Counts what this side offers: a walk that ends after a newer offer is dropped.
    generation: u64,
    timeouts: tokio::time::Interval,
}

/// What the server's clipboard last held from this side, or gave this side: this side's
/// clipboard is offered each time the tab is shown, and offering the same again would take
/// the clipboard back from the server, a copy made there since included.
#[derive(Debug, Default)]
struct Held {
    /// SHA-256 of the text offered, or received from the server.
    text: Option<[u8; SHA256_OUTPUT_LEN]>,
    /// The files offered, or being walked.
    files: Option<Vec<PathBuf>>,
}

impl Held {
    /// Whether `text` is new to the server; remembered when it is.
    fn new_text(&mut self, text: &str) -> bool {
        let seen = fingerprint(text);
        if self.text == Some(seen) {
            return false;
        }
        self.text = Some(seen);
        self.files = None;
        true
    }

    /// Whether `paths` are new to the server; remembered when they are.
    fn new_files(&mut self, paths: &[PathBuf]) -> bool {
        if self.files.as_deref() == Some(paths) {
            return false;
        }
        self.files = Some(paths.to_vec());
        self.text = None;
        true
    }

    /// The server copied `text`: it reaches this side's clipboard, and is not offered back.
    fn received(&mut self, text: &str) {
        self.text = Some(fingerprint(text));
        self.files = None;
    }

    /// The files are no longer offered: the same copied again is offered again.
    fn forget_files(&mut self) {
        self.files = None;
    }
}

/// SHA-256 of `text`, so the text itself is not kept.
fn fingerprint(text: &str) -> [u8; SHA256_OUTPUT_LEN] {
    let mut seen = [0; SHA256_OUTPUT_LEN];
    seen.copy_from_slice(digest(&SHA256, text.as_bytes()).as_ref());
    seen
}

/// Files walked for the offer numbered `generation`.
struct Walked {
    generation: u64,
    result: Result<FileList, CopyRefusal>,
}

/// What the loop has to do for the clipboard next.
enum ClipboardStep {
    Request(Request),
    Offer(LocalClipboard),
    Walked(Walked),
    Read(FileContentsResponse<'static>),
    Timeouts,
}

impl Shared {
    fn new(link: ClipboardLink, offers: mpsc::UnboundedReceiver<LocalClipboard>) -> Self {
        let mut timeouts = tokio::time::interval(CLIPBOARD_TIMEOUTS);
        timeouts.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        Self {
            link,
            offers,
            walked: mpsc::unbounded_channel(),
            read: mpsc::unbounded_channel(),
            held: Held::default(),
            generation: 0,
            timeouts,
        }
    }

    async fn next(&mut self) -> ClipboardStep {
        tokio::select! {
            Some(request) = self.link.requests.recv() => ClipboardStep::Request(request),
            Some(offer) = self.offers.recv() => ClipboardStep::Offer(offer),
            Some(walked) = self.walked.1.recv() => ClipboardStep::Walked(walked),
            Some(read) = self.read.1.recv() => ClipboardStep::Read(read),
            _ = self.timeouts.tick() => ClipboardStep::Timeouts,
        }
    }

    /// Something other than files is offered: a walk under way is dropped, and the same
    /// files copied again are walked again.
    fn forget_files(&mut self) {
        self.generation += 1;
        self.held.forget_files();
    }

    /// Walks `paths` off the session's task, unless they are the files offered already.
    fn walk(&mut self, paths: Vec<PathBuf>) {
        if !self.held.new_files(&paths) {
            return;
        }
        self.generation += 1;
        let generation = self.generation;
        let walked = self.walked.0.clone();
        tokio::task::spawn_blocking(move || {
            let result = crate::clipboard_files::walk(&paths, COPY_LIMITS);
            let _ = walked.send(Walked { generation, result });
        });
    }

    /// Reads what the server asked of `entry` off the session's task; the answer comes back
    /// as [`ClipboardStep::Read`].
    fn read(&self, request: FileContentsRequest, entry: Option<Entry>) {
        let read = self.read.0.clone();
        tokio::task::spawn_blocking(move || {
            let _ = read.send(crate::clipboard_files::answer(&request, entry.as_ref()));
        });
    }
}

/// Waits for the next clipboard step; forever when the clipboard is not shared.
async fn next_clipboard_step(shared: &mut Option<Shared>) -> ClipboardStep {
    match shared {
        Some(shared) => shared.next().await,
        None => std::future::pending().await,
    }
}

/// Whether the server of `channel` takes files.
fn takes_files(channel: &CliprdrClient) -> bool {
    channel
        .downcast_backend::<ClipboardBackend>()
        .is_some_and(ClipboardBackend::takes_files)
}

/// Tells the backend of `channel` which files are offered from now on.
fn offer_files(channel: &mut CliprdrClient, files: Option<Arc<[Entry]>>) {
    if let Some(backend) = channel.downcast_backend_mut::<ClipboardBackend>() {
        backend.offer_files(files);
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
    /// Since when a size waits for the display channel to open.
    waiting_since: Option<Instant>,
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
                step = next_clipboard_step(&mut shared) => {
                    if let Some(shared) = shared.as_mut() {
                        self.clipboard(&mut stage, step, shared).await?;
                    }
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

    /// Does what the clipboard channel asked, offers what this side copied, or sends the
    /// server what was read for it.
    async fn clipboard(
        &mut self,
        stage: &mut ActiveStage,
        step: ClipboardStep,
        shared: &mut Shared,
    ) -> Result<(), String> {
        let offered = shared.link.offered.clone();
        let Some(channel) = stage.get_svc_processor_mut::<CliprdrClient>() else {
            return Ok(());
        };
        let messages = match step {
            ClipboardStep::Request(Request::Received(text)) => {
                shared.held.received(&text);
                let _ = self.events.send(RdpEvent::RemoteClipboard(text)).await;
                return Ok(());
            }
            ClipboardStep::Request(Request::FileContents { request, entry }) => {
                shared.read(request, entry);
                return Ok(());
            }
            ClipboardStep::Request(Request::Paste) => {
                channel.initiate_paste(ClipboardFormatId::CF_UNICODETEXT)
            }
            ClipboardStep::Request(Request::Answer(answer)) => channel.submit_format_data(answer),
            ClipboardStep::Request(Request::Offer) => {
                shared.forget_files();
                offer_files(channel, None);
                channel.initiate_copy(&offered_formats(&offered))
            }
            ClipboardStep::Offer(LocalClipboard::Text(text)) => {
                if !shared.held.new_text(&text) {
                    return Ok(());
                }
                shared.forget_files();
                offer_files(channel, None);
                *offered.lock().unwrap_or_else(PoisonError::into_inner) = Some(text);
                channel.initiate_copy(&offered_formats(&offered))
            }
            ClipboardStep::Offer(LocalClipboard::Files(paths)) => {
                if takes_files(channel) {
                    shared.walk(paths);
                }
                return Ok(());
            }
            ClipboardStep::Walked(walked) => {
                if walked.generation != shared.generation {
                    return Ok(());
                }
                match walked.result {
                    Ok(list) if list.entries.is_empty() => return Ok(()),
                    Ok(list) => {
                        offer_files(channel, Some(list.entries.into()));
                        match channel.initiate_file_copy(list.descriptors) {
                            Ok(messages) => {
                                *offered.lock().unwrap_or_else(PoisonError::into_inner) = None;
                                Ok(messages)
                            }
                            // The channel not ready yet: the copy is offered again the next
                            // time the tab is shown, the session kept.
                            Err(error) => {
                                log::warn!("clipboard files not offered: {error}");
                                offer_files(channel, None);
                                shared.forget_files();
                                return Ok(());
                            }
                        }
                    }
                    Err(refusal) => {
                        let _ = self.events.send(RdpEvent::FilesRefused(refusal)).await;
                        return Ok(());
                    }
                }
            }
            ClipboardStep::Read(answer) => channel.submit_file_contents(answer),
            ClipboardStep::Timeouts => channel.drive_timeouts(),
        }
        .map_err(|error| error.to_string())?;
        let frame = stage
            .process_svc_processor_messages(messages)
            .map_err(|error| described(&error))?;
        if frame.is_empty() {
            return Ok(());
        }
        self.send(&frame).await
    }

    /// Asks the server for the wanted size, unless the desktop has it already; when the
    /// display channel is not open yet, asks again a little later, for
    /// [`RESIZE_GIVE_UP`] at most. A size the server cannot take is said.
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
            Some(Ok(frame)) => {
                self.waiting_since = None;
                self.send(&frame).await
            }
            None if asks_again(&mut self.waiting_since, Instant::now()) => {
                self.wanted = Some((width, height));
                self.settle = Some(Instant::now() + RESIZE_RETRY);
                Ok(())
            }
            // Not encodable, or no display channel after all this time.
            Some(Err(_)) | None => {
                self.waiting_since = None;
                let (Ok(width), Ok(height)) = (u16::try_from(width), u16::try_from(height)) else {
                    return Ok(());
                };
                let _ = self
                    .events
                    .send(RdpEvent::ResizeRefused { width, height })
                    .await;
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
mod resize_tests {
    use super::{RESIZE_GIVE_UP, asks_again};
    use std::time::Duration;
    use tokio::time::Instant;

    #[test]
    fn a_size_waits_for_the_display_channel_then_is_given_up_and_waits_afresh() {
        let start = Instant::now();
        let mut since = None;
        assert!(asks_again(&mut since, start));
        assert!(asks_again(
            &mut since,
            start + RESIZE_GIVE_UP - Duration::from_millis(1)
        ));
        assert!(!asks_again(&mut since, start + RESIZE_GIVE_UP), "given up");
        assert_eq!(since, None);
        let later = start + RESIZE_GIVE_UP * 3;
        assert!(
            asks_again(&mut since, later),
            "a new size waits its own time"
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn what_the_server_holds_from_this_side_is_not_offered_again() {
        let mut held = Held::default();
        assert!(held.new_text("copied here"));
        assert!(
            !held.new_text("copied here"),
            "the tab shown again: the server's copy made since is kept"
        );
        assert!(held.new_text("copied again"));

        let copied = vec![PathBuf::from("report.docx")];
        assert!(held.new_files(&copied));
        assert!(!held.new_files(&copied));
        assert!(
            held.new_text("copied again"),
            "files offered since: the same text is new again"
        );
        assert!(
            held.new_files(&copied),
            "text offered since: the files are new again"
        );

        held.forget_files();
        assert!(held.new_files(&copied), "a failed offer is tried again");
    }

    #[test]
    fn the_servers_text_is_not_offered_back_to_it() {
        let mut held = Held::default();
        let copied = vec![PathBuf::from("report.docx")];
        assert!(held.new_files(&copied));
        held.received("from the server");
        assert!(
            !held.new_text("from the server"),
            "written to this side's clipboard, then read when the tab is shown"
        );
        assert!(
            held.new_files(&copied),
            "the server took its clipboard back since"
        );
    }

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
