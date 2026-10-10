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

use crate::clipboard::{Asked, ClipboardBackend, MAX_IMAGE_BYTES, Offer, Request, offered_formats};
use crate::clipboard_files::{COPY_LIMITS, CopyRefusal, Entry, FileList};
use crate::clipboard_save::{Command, Download, SaveEnd, SaveStep, Writer};
use crate::connect::{ClipboardLink, MAX_DESKTOP_SIDE, RdpConnection, Upgraded};
use crate::frames::FrameReader;
use crate::keep_alive::{self, KeepAlive};
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
    /// The server's clipboard, as an image: a device-independent bitmap the server copied.
    RemoteImage(Vec<u8>),
    /// The files copied on this side were not offered to the server: too many, or too
    /// large.
    FilesRefused(CopyRefusal),
    /// The server's clipboard holds files to save here, or no longer.
    RemoteFiles(bool),
    /// Saving the server's files: this many entries of all are saved so far.
    SaveProgress {
        /// Entries saved.
        saved: usize,
        /// Entries in the copy.
        total: usize,
    },
    /// Saving the server's files ended.
    SaveEnded(SaveEnd),
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
    /// An image, as a device-independent bitmap: offered when no larger than
    /// [`crate::clipboard::MAX_IMAGE_BYTES`].
    Image(Vec<u8>),
    /// Files and folders copied, as Explorer lists them: offered when the server takes
    /// files, walked and read off the session's task.
    Files(Vec<PathBuf>),
    /// Hold this side's offers: the user is about to save the server's files, and
    /// offering would take the server's clipboard back. Released by
    /// [`LocalClipboard::SaveRemoteFiles`] or [`LocalClipboard::CancelSave`].
    HoldOffers,
    /// Save the files the server copied into this folder.
    SaveRemoteFiles(PathBuf),
    /// Stop saving the server's files: what is saved stays.
    CancelSave,
}

/// How long the list of the server's files is waited for.
const FILE_LIST_WAIT: Duration = Duration::from_secs(30);

/// How often saving the server's files says how far it is.
const SAVE_PROGRESS_EVERY: Duration = Duration::from_millis(250);

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
        keep_alive,
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
        keep_alive: KeepAlive::new(keep_alive),
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
    /// The server's files being saved, while the user asked.
    save: Option<Saving>,
    /// Counts the saves: what the writer of an earlier one says is dropped.
    saves: u64,
    /// The number of the last request for the server's bytes, across saves.
    streams: u32,
    /// This side's offers held while the user picks where to save the server's files.
    held_back: bool,
    /// What the writer did, for the save it writes.
    written: (
        mpsc::UnboundedSender<WriterReport>,
        mpsc::UnboundedReceiver<WriterReport>,
    ),
}

/// The writer did, or failed, what it was told: the number of its save, and how it went.
type WriterReport = (u64, bool);

/// The server's files being saved.
enum Saving {
    /// Their list is asked for.
    Listing { folder: PathBuf, since: Instant },
    /// They are fetched, and written by a task of their own.
    Running {
        download: Download,
        writer: std::sync::mpsc::Sender<ToWriter>,
        /// When the progress was last said.
        told: Instant,
    },
}

/// What the writer's task is told.
enum ToWriter {
    Do(Command),
    /// Stop: the file being written is deleted.
    Abandon,
    /// The copy is complete.
    Finish,
}

/// What saving the server's files has to send or say.
#[derive(Default)]
struct SaveOutcome {
    messages: Option<ironrdp::cliprdr::CliprdrSvcMessages<ironrdp::cliprdr::Client>>,
    progress: Option<(usize, usize)>,
    ended: Option<SaveEnd>,
}

/// What the server's clipboard last held from this side, or gave this side: this side's
/// clipboard is offered each time the tab is shown, and offering the same again would take
/// the clipboard back from the server, a copy made there since included.
#[derive(Debug, Default)]
struct Held {
    /// SHA-256 of the text offered, or received from the server.
    text: Option<[u8; SHA256_OUTPUT_LEN]>,
    /// SHA-256 of the image offered, or received from the server.
    image: Option<[u8; SHA256_OUTPUT_LEN]>,
    /// The files offered, or being walked.
    files: Option<Vec<PathBuf>>,
}

impl Held {
    /// Whether `text` is new to the server; remembered when it is.
    fn new_text(&mut self, text: &str) -> bool {
        let seen = fingerprint(text.as_bytes());
        if self.text == Some(seen) {
            return false;
        }
        self.text = Some(seen);
        self.image = None;
        self.files = None;
        true
    }

    /// Whether `image` is new to the server; remembered when it is.
    fn new_image(&mut self, image: &[u8]) -> bool {
        let seen = fingerprint(image);
        if self.image == Some(seen) {
            return false;
        }
        self.image = Some(seen);
        self.text = None;
        self.files = None;
        true
    }

    /// The server copied `image`: it reaches this side's clipboard, and is not offered
    /// back; whether it is new, the same one announced again being written once. The text
    /// remembered stays: should the image not reach this side's clipboard, the text there
    /// is not offered back over the server's image.
    fn received_image(&mut self, image: &[u8]) -> bool {
        let seen = fingerprint(image);
        if self.image == Some(seen) {
            return false;
        }
        self.image = Some(seen);
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
        self.image = None;
        true
    }

    /// The server copied `text`: it reaches this side's clipboard, and is not offered back.
    fn received(&mut self, text: &str) {
        self.text = Some(fingerprint(text.as_bytes()));
        self.image = None;
        self.files = None;
    }

    /// The files are no longer offered: the same copied again is offered again.
    fn forget_files(&mut self) {
        self.files = None;
    }
}

/// SHA-256 of `bytes`, so the text or image itself is not kept.
fn fingerprint(bytes: &[u8]) -> [u8; SHA256_OUTPUT_LEN] {
    let mut seen = [0; SHA256_OUTPUT_LEN];
    seen.copy_from_slice(digest(&SHA256, bytes).as_ref());
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
    /// The writer did, or failed, what it was told for the save numbered so.
    Written(u64, bool),
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
            save: None,
            saves: 0,
            streams: 0,
            held_back: false,
            written: mpsc::unbounded_channel(),
        }
    }

    async fn next(&mut self) -> ClipboardStep {
        tokio::select! {
            Some(request) = self.link.requests.recv() => ClipboardStep::Request(request),
            Some(offer) = self.offers.recv() => ClipboardStep::Offer(offer),
            Some(walked) = self.walked.1.recv() => ClipboardStep::Walked(walked),
            Some(read) = self.read.1.recv() => ClipboardStep::Read(read),
            _ = self.timeouts.tick() => ClipboardStep::Timeouts,
            Some((save, ok)) = self.written.1.recv() => ClipboardStep::Written(save, ok),
        }
    }

    /// Whether this side's offers are held: the server's files are being saved, or about
    /// to be.
    fn holds_offers(&self) -> bool {
        self.save.is_some() || self.held_back
    }

    /// The download of the save running.
    fn running(&mut self) -> Option<&mut Download> {
        match &mut self.save {
            Some(Saving::Running { download, .. }) => Some(download),
            _ => None,
        }
    }

    /// Starts the task writing the save into `folder`, one command at a time, in order.
    fn start_writer(&mut self, folder: PathBuf) -> std::sync::mpsc::Sender<ToWriter> {
        self.saves += 1;
        let save = self.saves;
        let written = self.written.0.clone();
        let (sender, commands) = std::sync::mpsc::channel();
        tokio::task::spawn_blocking(move || {
            let mut writer = Writer::new(folder);
            while let Ok(message) = commands.recv() {
                match message {
                    ToWriter::Do(command) => {
                        let done = writer.apply(command);
                        if let Err(error) = &done {
                            log::warn!("server's files not saved: {error}");
                        }
                        if written.send((save, done.is_ok())).is_err() {
                            break;
                        }
                    }
                    ToWriter::Abandon => {
                        writer.abandon();
                        break;
                    }
                    ToWriter::Finish => {
                        writer.finish();
                        break;
                    }
                }
            }
        });
        sender
    }

    /// Stops the save: the list no longer waited for, or the writer told to abandon. What
    /// was saved, of all, when one was under way.
    fn stop_save(&mut self, channel: &mut CliprdrClient) -> Option<(usize, usize)> {
        // A list still asked for is dropped when it comes: no save is listing any more.
        let _ = channel;
        match self.save.take()? {
            Saving::Listing { .. } => Some((0, 0)),
            Saving::Running {
                download, writer, ..
            } => {
                self.streams = download.stream();
                let _ = writer.send(ToWriter::Abandon);
                Some((download.saved(), download.total()))
            }
        }
    }

    /// The list of the server's files, waited for too long: the save stops.
    fn overdue(&mut self, channel: &mut CliprdrClient) -> Option<SaveEnd> {
        // An answer of the server's clipboard that never came is no longer waited for.
        if let Some(backend) = channel.downcast_backend_mut::<ClipboardBackend>() {
            backend.expire();
        }
        let Some(Saving::Listing { since, .. }) = &self.save else {
            return None;
        };
        if since.elapsed() < FILE_LIST_WAIT {
            return None;
        }
        self.stop_save(channel)
            .map(|(saved, total)| SaveEnd::Failed { saved, total })
    }

    /// Starts saving the server's files into `folder`: their list asked for.
    fn begin_save(&mut self, channel: &mut CliprdrClient, folder: PathBuf) -> SaveOutcome {
        if self.save.is_some() {
            return SaveOutcome::default();
        }
        self.save = Some(Saving::Listing {
            folder,
            since: Instant::now(),
        });
        self.list_files(channel)
    }

    /// Asks for the list of the server's files, unless something else asked of its
    /// clipboard is unanswered: then once it is, as [`Request::ListFiles`].
    fn list_files(&mut self, channel: &mut CliprdrClient) -> SaveOutcome {
        let mut outcome = SaveOutcome::default();
        if !matches!(self.save, Some(Saving::Listing { .. })) {
            return outcome;
        }
        let format = channel
            .downcast_backend::<ClipboardBackend>()
            .and_then(ClipboardBackend::remote_files);
        let Some(format) = format else {
            self.save = None;
            outcome.ended = Some(SaveEnd::Failed { saved: 0, total: 0 });
            return outcome;
        };
        if !ask(channel, Asked::Files) {
            return outcome;
        }
        match channel.initiate_paste(format) {
            Ok(messages) => outcome.messages = Some(messages),
            Err(error) => {
                log::warn!("server's files not listed: {error}");
                if let Some(backend) = channel.downcast_backend_mut::<ClipboardBackend>() {
                    backend.withdraw();
                }
                self.save = None;
                outcome.ended = Some(SaveEnd::Failed { saved: 0, total: 0 });
            }
        }
        outcome
    }

    /// The list of the server's files came: they are fetched, unless refused whole.
    fn listed(
        &mut self,
        channel: &mut CliprdrClient,
        files: &[ironrdp::cliprdr::pdu::FileDescriptor],
        lock: Option<u32>,
    ) -> SaveOutcome {
        let Some(Saving::Listing { folder, .. }) = self.save.take() else {
            return SaveOutcome::default();
        };
        match crate::clipboard_save::plan(files, COPY_LIMITS) {
            Ok(items) => {
                let writer = self.start_writer(folder);
                let mut download = Download::new(items, lock, self.streams);
                let first = download.start();
                self.save = Some(Saving::Running {
                    download,
                    writer,
                    told: Instant::now(),
                });
                self.act(channel, first)
            }
            Err(refusal) => SaveOutcome {
                ended: Some(SaveEnd::Refused(refusal)),
                ..SaveOutcome::default()
            },
        }
    }

    /// Does what the download needs next, and what follows by itself: a request the
    /// channel refuses fails the copy, never the session.
    fn act(&mut self, channel: &mut CliprdrClient, mut action: SaveStep) -> SaveOutcome {
        let mut outcome = SaveOutcome::default();
        loop {
            let Some(Saving::Running {
                download,
                writer,
                told,
            }) = &mut self.save
            else {
                return outcome;
            };
            self.streams = download.stream();
            action = match action {
                SaveStep::Write(command) => {
                    let starts = matches!(command, Command::Open(_) | Command::Folder(_));
                    if starts && told.elapsed() >= SAVE_PROGRESS_EVERY {
                        *told = Instant::now();
                        outcome.progress = Some((download.saved(), download.total()));
                    }
                    if writer.send(ToWriter::Do(command)).is_ok() {
                        return outcome;
                    }
                    download.lost()
                }
                SaveStep::Ask(request) => match channel.request_file_contents(request) {
                    Ok(messages) => {
                        outcome.messages = Some(messages);
                        return outcome;
                    }
                    Err(error) => {
                        log::warn!("server's files not asked for: {error}");
                        download.lost()
                    }
                },
                SaveStep::Done { saved } => {
                    let _ = writer.send(ToWriter::Finish);
                    self.save = None;
                    outcome.ended = Some(SaveEnd::Saved(saved));
                    return outcome;
                }
                SaveStep::Failed { saved } => {
                    let total = download.total();
                    let _ = writer.send(ToWriter::Abandon);
                    self.save = None;
                    outcome.ended = Some(SaveEnd::Failed { saved, total });
                    return outcome;
                }
            };
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

/// Whether `asked` can be asked of the server's clipboard now; else it waits for the
/// answer to what was asked before.
fn ask(channel: &mut CliprdrClient, asked: Asked) -> bool {
    channel
        .downcast_backend_mut::<ClipboardBackend>()
        .is_some_and(|backend| backend.ask(asked))
}

/// This side took the clipboard: whether the server's files were there to save.
fn forget_remote_files(channel: &mut CliprdrClient) -> bool {
    channel
        .downcast_backend_mut::<ClipboardBackend>()
        .is_some_and(|backend| {
            let had = backend.remote_files().is_some();
            backend.forget_remote_files();
            had
        })
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

/// Waits until `deadline`: a size settled, a keep-alive due. The caller polls it only when
/// there is one.
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
    /// When the next keep-alive is due: every frame sent puts it back.
    keep_alive: KeepAlive,
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
        let user_channel_id = result.user_channel_id;
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
        self.keep_alive.activated(Instant::now());
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
                () = sleep_until_settled(self.keep_alive.due()), if self.keep_alive.due().is_some() => {
                    self.send(&keep_alive::frame(&stage, user_channel_id)?).await?;
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
                        self.reactivated(&activation, &mut stage).await?;
                    }
                    _ => {}
                }
            }
        }
    }

    /// Does what the clipboard channel asked, offers what this side copied, or sends the
    /// server what was read for it.
    #[expect(clippy::too_many_lines, reason = "one arm per clipboard step")]
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
            ClipboardStep::Request(Request::ReceivedImage(image)) => {
                if shared.held.received_image(&image) {
                    let _ = self.events.send(RdpEvent::RemoteImage(image)).await;
                }
                return Ok(());
            }
            ClipboardStep::Request(Request::FileContents { request, entry }) => {
                shared.read(request, entry);
                return Ok(());
            }
            ClipboardStep::Request(Request::Paste) => {
                if !ask(channel, Asked::Text) {
                    return Ok(());
                }
                channel.initiate_paste(ClipboardFormatId::CF_UNICODETEXT)
            }
            ClipboardStep::Request(Request::PasteImage) => {
                if !ask(channel, Asked::Image) {
                    return Ok(());
                }
                channel.initiate_paste(ClipboardFormatId::CF_DIB)
            }
            ClipboardStep::Request(Request::Answer(answer)) => channel.submit_format_data(answer),
            ClipboardStep::Request(Request::Offer) => {
                shared.forget_files();
                offer_files(channel, None);
                channel.initiate_copy(&offered_formats(&offered))
            }
            ClipboardStep::Offer(local @ (LocalClipboard::Text(_) | LocalClipboard::Image(_))) => {
                // Held: not remembered, so offered again once the save ends.
                if shared.holds_offers() {
                    return Ok(());
                }
                let offer = match local {
                    LocalClipboard::Text(text) if shared.held.new_text(&text) => Offer::Text(text),
                    LocalClipboard::Image(image)
                        if image.len() <= MAX_IMAGE_BYTES && shared.held.new_image(&image) =>
                    {
                        Offer::Image(image.into())
                    }
                    _ => return Ok(()),
                };
                shared.forget_files();
                offer_files(channel, None);
                *offered.lock().unwrap_or_else(PoisonError::into_inner) = Some(offer);
                let messages = channel.initiate_copy(&offered_formats(&offered));
                if forget_remote_files(channel) {
                    let _ = self.events.send(RdpEvent::RemoteFiles(false)).await;
                }
                messages
            }
            ClipboardStep::Offer(LocalClipboard::Files(paths)) => {
                // Offering files would release the lock on the server's copy being saved.
                if takes_files(channel) && !shared.holds_offers() {
                    shared.walk(paths);
                }
                return Ok(());
            }
            ClipboardStep::Walked(walked) => {
                if walked.generation != shared.generation {
                    return Ok(());
                }
                if shared.holds_offers() {
                    shared.forget_files();
                    return Ok(());
                }
                match walked.result {
                    Ok(list) if list.entries.is_empty() => return Ok(()),
                    Ok(list) => {
                        offer_files(channel, Some(list.entries.into()));
                        match channel.initiate_file_copy(list.descriptors) {
                            Ok(messages) => {
                                *offered.lock().unwrap_or_else(PoisonError::into_inner) = None;
                                if forget_remote_files(channel) {
                                    let _ = self.events.send(RdpEvent::RemoteFiles(false)).await;
                                }
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
            ClipboardStep::Timeouts => {
                if let Some(ended) = shared.overdue(channel) {
                    let _ = self.events.send(RdpEvent::SaveEnded(ended)).await;
                }
                channel.drive_timeouts()
            }
            step => return self.save(stage, step, shared).await,
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

    /// Saves the server's files when the user asks: their list asked for, then their bytes
    /// one request at a time, each written before the next; or stops.
    async fn save(
        &mut self,
        stage: &mut ActiveStage,
        step: ClipboardStep,
        shared: &mut Shared,
    ) -> Result<(), String> {
        let Some(channel) = stage.get_svc_processor_mut::<CliprdrClient>() else {
            return Ok(());
        };
        let mut outcome = SaveOutcome::default();
        match step {
            ClipboardStep::Offer(LocalClipboard::SaveRemoteFiles(folder)) => {
                shared.held_back = false;
                outcome = shared.begin_save(channel, folder);
            }
            ClipboardStep::Request(Request::ListFiles) => outcome = shared.list_files(channel),
            ClipboardStep::Offer(LocalClipboard::HoldOffers) => {
                shared.held_back = true;
                // A walk under way is dropped.
                shared.forget_files();
            }
            ClipboardStep::Offer(LocalClipboard::CancelSave) => {
                shared.held_back = false;
                outcome.ended = shared
                    .stop_save(channel)
                    .map(|(saved, total)| SaveEnd::Cancelled { saved, total });
            }
            ClipboardStep::Request(Request::RemoteFiles(available)) => {
                let _ = self.events.send(RdpEvent::RemoteFiles(available)).await;
                // The server copied again: a server keeping no lock, or not honouring it,
                // would read the new copy in place of the one being saved.
                if let Some(download) = shared.running() {
                    let lost = download.lost();
                    outcome = shared.act(channel, lost);
                }
            }
            ClipboardStep::Request(Request::RemoteFileList { files, lock }) => {
                outcome = shared.listed(channel, &files, lock);
            }
            ClipboardStep::Request(Request::FileListFailed) => {
                if matches!(shared.save, Some(Saving::Listing { .. })) {
                    shared.save = None;
                    outcome.ended = Some(SaveEnd::Failed { saved: 0, total: 0 });
                }
            }
            ClipboardStep::Request(Request::Contents { stream, data }) => {
                if let Some(next) = shared
                    .running()
                    .and_then(|download| download.answered(stream, data))
                {
                    outcome = shared.act(channel, next);
                }
            }
            ClipboardStep::Request(Request::LocksCleared(locks)) => {
                if let Some(download) = shared.running()
                    && download.lock().is_some_and(|lock| locks.contains(&lock))
                {
                    let lost = download.lost();
                    outcome = shared.act(channel, lost);
                }
            }
            ClipboardStep::Written(save, ok) => {
                if save == shared.saves
                    && let Some(next) = shared.running().and_then(|download| download.written(ok))
                {
                    outcome = shared.act(channel, next);
                }
            }
            _ => {}
        }
        if let Some(messages) = outcome.messages {
            let frame = stage
                .process_svc_processor_messages(messages)
                .map_err(|error| described(&error))?;
            if !frame.is_empty() {
                self.send(&frame).await?;
            }
        }
        if let Some((saved, total)) = outcome.progress {
            let _ = self
                .events
                .send(RdpEvent::SaveProgress { saved, total })
                .await;
        }
        if let Some(ended) = outcome.ended {
            let _ = self.events.send(RdpEvent::SaveEnded(ended)).await;
        }
        Ok(())
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
        self.writer
            .flush()
            .await
            .map_err(|error| error.to_string())?;
        self.keep_alive.sent(Instant::now());
        Ok(())
    }

    async fn answer(&mut self, outputs: Vec<ActiveStageOutput>) -> Result<(), String> {
        for output in outputs {
            if let ActiveStageOutput::ResponseFrame(frame) = output {
                self.send(&frame).await?;
            }
        }
        Ok(())
    }

    /// Runs the Deactivation-Reactivation Sequence the server started, without keep-alives
    /// meanwhile, then asks again for the size asked before it.
    async fn reactivated(
        &mut self,
        activation: &ConnectionActivationFactory,
        stage: &mut ActiveStage,
    ) -> Result<(), String> {
        self.keep_alive.deactivated();
        self.reactivate(activation, stage).await?;
        self.keep_alive.activated(Instant::now());
        if self.wanted.is_none() && self.asked.is_some() {
            self.wanted = self.asked;
            self.settle = Some(Instant::now() + RESIZE_SETTLE);
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
    use ironrdp::cliprdr::backend::CliprdrBackend as _;

    use super::*;

    /// A clipboard channel not ready yet, as before the server's Monitor Ready, and the
    /// session's side of it.
    fn unready() -> (CliprdrClient, Shared) {
        let (requests, received) = mpsc::unbounded_channel();
        let offered = crate::clipboard::Offered::default();
        let channel =
            CliprdrClient::new(Box::new(ClipboardBackend::new(requests, offered.clone())));
        let link = ClipboardLink {
            requests: received,
            offered,
        };
        (channel, Shared::new(link, mpsc::unbounded_channel().1))
    }

    #[tokio::test]
    async fn saving_with_nothing_copied_on_the_server_fails_alone() {
        let (mut channel, mut shared) = unready();
        let outcome = shared.begin_save(&mut channel, PathBuf::from("saved"));
        assert_eq!(outcome.ended, Some(SaveEnd::Failed { saved: 0, total: 0 }));
        assert!(outcome.messages.is_none() && shared.save.is_none());
    }

    #[tokio::test]
    async fn a_request_the_channel_refuses_fails_the_copy_not_the_session() {
        let (mut channel, mut shared) = unready();
        let dir = tempfile::tempdir().expect("dir");
        shared.save = Some(Saving::Listing {
            folder: dir.path().to_path_buf(),
            since: Instant::now(),
        });
        let mut file = ironrdp::cliprdr::pdu::FileDescriptor::new("a.bin");
        file.file_size = Some(10);
        let outcome = shared.listed(&mut channel, &[file], Some(1));
        // The file is opened first: its bytes are asked for once the writer says so.
        assert!(outcome.ended.is_none(), "{:?}", outcome.ended);
        let (save, ok) = shared.written.1.recv().await.expect("opened");
        assert!(save == shared.saves && ok);
        let next = shared
            .running()
            .and_then(|download| download.written(ok))
            .expect("a request");
        let outcome = shared.act(&mut channel, next);
        assert_eq!(
            outcome.ended,
            Some(SaveEnd::Failed { saved: 0, total: 1 }),
            "the channel is not ready: refused, the session kept"
        );
        assert!(shared.save.is_none());
    }

    #[tokio::test]
    async fn the_list_waits_for_the_servers_text_asked_before() {
        let (mut channel, mut shared) = unready();
        if let Some(backend) = channel.downcast_backend_mut::<ClipboardBackend>() {
            backend.on_remote_copy(&[ironrdp::cliprdr::pdu::ClipboardFormat::new(
                ClipboardFormatId::new(0xC0A1),
            )
            .with_name(ironrdp::cliprdr::pdu::ClipboardFormatName::FILE_LIST)]);
            assert!(backend.ask(Asked::Text));
        }
        let outcome = shared.begin_save(&mut channel, PathBuf::from("saved"));
        assert!(outcome.messages.is_none() && outcome.ended.is_none());
        assert!(
            matches!(shared.save, Some(Saving::Listing { .. })),
            "asked later"
        );
    }

    #[tokio::test]
    async fn a_list_beyond_the_limits_is_refused_and_nothing_written() {
        let (mut channel, mut shared) = unready();
        let dir = tempfile::tempdir().expect("dir");
        shared.save = Some(Saving::Listing {
            folder: dir.path().to_path_buf(),
            since: Instant::now(),
        });
        let no_size = ironrdp::cliprdr::pdu::FileDescriptor::new("a.bin");
        let outcome = shared.listed(&mut channel, &[no_size], None);
        assert_eq!(
            outcome.ended,
            Some(SaveEnd::Refused(crate::SaveRefusal::UnknownSize))
        );
        assert_eq!(std::fs::read_dir(dir.path()).expect("read").count(), 0);
    }

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
    fn the_servers_image_is_not_offered_back_and_text_and_image_take_each_others_place() {
        let mut held = Held::default();
        let image = [1_u8, 2, 3];
        assert!(held.new_text("copied here"));
        assert!(held.received_image(&image));
        assert!(
            !held.received_image(&image),
            "announced again: written once"
        );
        assert!(
            !held.new_image(&image),
            "written to this side's clipboard, then read when the tab is shown"
        );
        assert!(
            !held.new_text("copied here"),
            "the image not written here: the old text is not offered over it"
        );
        assert!(held.new_text("copied since"));
        assert!(held.new_image(&image), "copied here again since");
        assert!(!held.new_image(&image));
        assert!(held.new_text("copied here"), "the image took its place");
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
