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

//! The clipboard channel (MS-RDPECLIP): text both ways, and files both ways.
//!
//! `IronRDP` calls the backend from inside its channel processing, where the channel cannot
//! be driven again; so the backend only posts what it wants done, and the session loop does
//! it: ask the server for its text, answer the server's request for ours, offer ours, read
//! the files it asks for, and fetch the files it copied when the user saves them.
//!
//! No other format: text and files are what a connection manager needs, and each other
//! format is more that a server could send.

use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex, PoisonError};

use ironrdp::cliprdr::backend::CliprdrBackend;
use ironrdp::cliprdr::pdu::{
    ClipboardFormat, ClipboardFormatId, ClipboardFormatName, ClipboardGeneralCapabilityFlags,
    FileContentsRequest, FileContentsResponse, FileDescriptor, FormatDataRequest,
    FormatDataResponse, LockDataId, OwnedFormatDataResponse,
};
use tokio::sync::mpsc;
use zeroize::Zeroizing;

use crate::clipboard_files::Entry;

/// How long an answer of the server's clipboard is waited for, in milliseconds, before
/// what waits behind it is asked.
const ANSWER_WAIT_MS: u64 = 30_000;

/// How long an image is waited for, in milliseconds: one at its largest takes about a minute
/// on a slow link, and an answer coming after its wait would be taken for the next one's.
const IMAGE_ANSWER_WAIT_MS: u64 = 300_000;

/// Whether the server's images are asked for: on Windows only, where they reach this side's
/// clipboard. Elsewhere one would be fetched for nothing.
const TAKES_IMAGES: bool = cfg!(windows);

/// The sizes of the headers a device-independent bitmap may start with: `BITMAPINFOHEADER`,
/// its versions with colour masks, `BITMAPV4HEADER` and `BITMAPV5HEADER`.
const BITMAP_HEADER_SIZES: [u32; 5] = [40, 52, 56, 108, 124];

/// The bits a pixel of a device-independent bitmap may take.
const BITMAP_BIT_COUNTS: [u16; 7] = [0, 1, 4, 8, 16, 24, 32];

/// Longest text taken from the server, in bytes of UTF-16: a larger one is dropped, not cut.
pub const MAX_REMOTE_TEXT_BYTES: usize = 8 * 1024 * 1024;

/// Largest image copied either way, in bytes of a device-independent bitmap: a 4K screen at
/// 32 bits a pixel and room to spare. A larger one is not offered, nor taken.
pub const MAX_IMAGE_BYTES: usize = 64 * 1024 * 1024;

/// Bytes of the header a device-independent bitmap starts with, at least.
const BITMAP_INFO_HEADER_LEN: usize = 40;

/// What the backend asks the session loop to do.
#[derive(Debug)]
pub(crate) enum Request {
    /// Ask the server for its clipboard, as text.
    Paste,
    /// Ask the server for its clipboard, as an image.
    PasteImage,
    /// Answer the server's request for our clipboard.
    Answer(OwnedFormatDataResponse),
    /// Tell the server what our clipboard holds.
    Offer,
    /// The server's text arrived.
    Received(Zeroizing<String>),
    /// The server's image arrived, as a device-independent bitmap.
    ReceivedImage(Vec<u8>),
    /// The server asks for a file offered, or for its size: `entry` is the file, `None`
    /// when the server's index names none.
    FileContents {
        /// The request.
        request: FileContentsRequest,
        /// The file it names.
        entry: Option<Entry>,
    },
    /// The server copied something: files among it, or not.
    RemoteFiles(bool),
    /// The list of the files the server copied, asked for to save them.
    RemoteFileList {
        /// The files, their names already cleaned of paths by `IronRDP`.
        files: Vec<FileDescriptor>,
        /// The server's lock keeping them, when locks were agreed on.
        lock: Option<u32>,
    },
    /// The list of the server's files did not come.
    FileListFailed,
    /// The list of the server's files, held back while something else was asked, can be
    /// asked for now.
    ListFiles,
    /// The server answered a request for a file's bytes.
    Contents {
        /// The request answered.
        stream: u32,
        /// Its bytes; `None` when the server failed.
        data: Option<Vec<u8>>,
    },
    /// These locks on the server's copies were released.
    LocksCleared(Vec<u32>),
}

/// What this side asked of the server's clipboard, until it answers: one thing at a time,
/// so an answer is never taken for another.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Asked {
    /// Its text.
    Text,
    /// Its image.
    Image,
    /// The list of its files.
    Files,
}

/// What this side's clipboard offers the server, until it asks for it.
#[derive(Debug, Clone)]
pub(crate) enum Offer {
    /// Text.
    Text(Zeroizing<String>),
    /// An image, as a device-independent bitmap.
    Image(Arc<[u8]>),
}

/// The text or image this side offers the server, until it asks for it.
pub(crate) type Offered = Arc<Mutex<Option<Offer>>>;

/// The backend: reads what `IronRDP` reports and posts [`Request`]s.
#[derive(Debug)]
pub(crate) struct ClipboardBackend {
    requests: mpsc::UnboundedSender<Request>,
    offered: Offered,
    /// The server takes files: both sides agreed on file streams.
    takes_files: bool,
    /// The files offered now, at the server's indexes; `None` once something else is.
    files: Option<Arc<[Entry]>>,
    /// The files offered when the server locked the clipboard, by the lock's id: it may
    /// still ask for them after this side copies something else.
    locked: HashMap<u32, Arc<[Entry]>>,
    /// The server's id for its list of files, while its clipboard holds some.
    remote_files: Option<ClipboardFormatId>,
    /// What was asked of the server's clipboard, and when, until it answers: one thing at a
    /// time, as `IronRDP` matches an answer with the last request only.
    asked: Option<(Asked, u64)>,
    /// What waits to be asked once the server answers, in order.
    waiting: VecDeque<Asked>,
}

impl ClipboardBackend {
    pub(crate) fn new(requests: mpsc::UnboundedSender<Request>, offered: Offered) -> Self {
        Self {
            requests,
            offered,
            takes_files: false,
            files: None,
            locked: HashMap::new(),
            remote_files: None,
            asked: None,
            waiting: VecDeque::new(),
        }
    }

    /// The server's id for the list of the files its clipboard holds; `None` when it holds
    /// none.
    pub(crate) fn remote_files(&self) -> Option<ClipboardFormatId> {
        self.remote_files
    }

    /// This side took the clipboard: the server's files are no longer there to save.
    pub(crate) fn forget_remote_files(&mut self) {
        self.remote_files = None;
    }

    /// Whether `asked` can be asked of the server now. While something else is unanswered
    /// it waits, and is posted again once that is answered: [`Request::Paste`] or
    /// [`Request::ListFiles`].
    pub(crate) fn ask(&mut self, asked: Asked) -> bool {
        if self.asked.is_some() {
            if !self.waiting.contains(&asked) {
                self.waiting.push_back(asked);
            }
            return false;
        }
        self.asked = Some((asked, self.now_ms()));
        true
    }

    /// What was asked could not be sent: it is no longer waited for.
    pub(crate) fn withdraw(&mut self) {
        self.answered();
    }

    /// An answer that does not come in time is no longer waited for.
    pub(crate) fn expire(&mut self) {
        self.expire_at(self.now_ms());
    }

    /// As [`Self::expire`], at `now`.
    fn expire_at(&mut self, now: u64) {
        if self.asked.is_some_and(|(asked, since)| {
            let wait = if asked == Asked::Image {
                IMAGE_ANSWER_WAIT_MS
            } else {
                ANSWER_WAIT_MS
            };
            now.saturating_sub(since) >= wait
        }) {
            self.answered();
        }
    }

    /// The server answered: what it answered, and what waited is asked next.
    fn answered(&mut self) -> Option<Asked> {
        let asked = self.asked.take().map(|(asked, _)| asked);
        if let Some(next) = self.waiting.pop_front() {
            self.post(match next {
                Asked::Text => Request::Paste,
                Asked::Image => Request::PasteImage,
                Asked::Files => Request::ListFiles,
            });
        }
        asked
    }

    /// Whether the server takes files.
    pub(crate) fn takes_files(&self) -> bool {
        self.takes_files
    }

    /// The files offered from now on, at the indexes the server knows them by; `None` when
    /// something else is offered. Set as the channel is told, so both agree.
    pub(crate) fn offer_files(&mut self, files: Option<Arc<[Entry]>>) {
        self.files = files;
    }

    fn post(&self, request: Request) {
        // The session may be gone: nothing to do then.
        let _ = self.requests.send(request);
    }
}

ironrdp_core::impl_as_any!(ClipboardBackend);

/// The formats this side offers: text, when there is some, or an image.
pub(crate) fn offered_formats(offered: &Offered) -> Vec<ClipboardFormat> {
    match offered
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .as_ref()
    {
        Some(Offer::Text(text)) if !text.is_empty() => {
            vec![ClipboardFormat::new(ClipboardFormatId::CF_UNICODETEXT)]
        }
        Some(Offer::Image(_)) => vec![ClipboardFormat::new(ClipboardFormatId::CF_DIB)],
        _ => Vec::new(),
    }
}

/// The server's image in `response`, when it is one and not too large: a device-independent
/// bitmap whose header reads as one, so that nothing else is handed to this side's
/// clipboard as an image.
pub(crate) fn image_of(response: &FormatDataResponse<'_>) -> Option<Vec<u8>> {
    let data = response.data();
    (!response.is_error()
        && (BITMAP_INFO_HEADER_LEN..=MAX_IMAGE_BYTES).contains(&data.len())
        && is_bitmap(data))
    .then(|| data.to_vec())
}

/// Whether `data` starts as a device-independent bitmap does: a known header size, a width,
/// a height, one plane and a known number of bits a pixel.
fn is_bitmap(data: &[u8]) -> bool {
    let u32_at = |at: usize| {
        data.get(at..at + 4)
            .and_then(|bytes| <[u8; 4]>::try_from(bytes).ok())
            .map(u32::from_le_bytes)
    };
    let u16_at = |at: usize| {
        data.get(at..at + 2)
            .and_then(|bytes| <[u8; 2]>::try_from(bytes).ok())
            .map(u16::from_le_bytes)
    };
    let header = u32_at(0).filter(|size| BITMAP_HEADER_SIZES.contains(size));
    let header_fits = header
        .and_then(|size| usize::try_from(size).ok())
        .is_some_and(|size| size <= data.len());
    let width = u32_at(4).and_then(|width| i32::try_from(width).ok());
    let height = u32_at(8).map(u32::cast_signed);
    header_fits
        && width.is_some_and(|width| width > 0)
        && height.is_some_and(|height| height != 0)
        && u16_at(12) == Some(1)
        && u16_at(14).is_some_and(|bits| BITMAP_BIT_COUNTS.contains(&bits))
}

/// The server's text in `response`, when it is text and not too large.
pub(crate) fn text_of(response: &FormatDataResponse<'_>) -> Option<Zeroizing<String>> {
    if response.is_error() || response.data().len() > MAX_REMOTE_TEXT_BYTES {
        return None;
    }
    response.to_unicode_string().ok().map(Zeroizing::new)
}

impl CliprdrBackend for ClipboardBackend {
    #[expect(
        clippy::unnecessary_literal_bound,
        reason = "the signature is the trait's"
    )]
    fn temporary_directory(&self) -> &str {
        // Never used: files are offered without paths, and the server's are not taken.
        ""
    }

    fn client_capabilities(&self) -> ClipboardGeneralCapabilityFlags {
        // Files as streams, named without this side's paths; locks keep a copy's files
        // within reach while the server reads them.
        ClipboardGeneralCapabilityFlags::STREAM_FILECLIP_ENABLED
            | ClipboardGeneralCapabilityFlags::FILECLIP_NO_FILE_PATHS
            | ClipboardGeneralCapabilityFlags::CAN_LOCK_CLIPDATA
    }

    fn on_ready(&mut self) {}

    fn on_request_format_list(&mut self) {
        self.post(Request::Offer);
    }

    fn on_process_negotiated_capabilities(
        &mut self,
        capabilities: ClipboardGeneralCapabilityFlags,
    ) {
        self.takes_files =
            capabilities.contains(ClipboardGeneralCapabilityFlags::STREAM_FILECLIP_ENABLED);
    }

    fn on_remote_copy(&mut self, available_formats: &[ClipboardFormat]) {
        // Found by its name, the first, exactly as `IronRDP` finds it.
        self.remote_files = available_formats
            .iter()
            .find(|format| {
                format
                    .name()
                    .is_some_and(|name| name.value() == ClipboardFormatName::FILE_LIST.value())
            })
            .map(ClipboardFormat::id);
        self.post(Request::RemoteFiles(self.remote_files.is_some()));
        // The server took the clipboard: an image this side offered is no longer kept.
        let mut offered = self.offered.lock().unwrap_or_else(PoisonError::into_inner);
        if matches!(*offered, Some(Offer::Image(_))) {
            *offered = None;
        }
        drop(offered);
        let offers = |id| available_formats.iter().any(|format| format.id() == id);
        // Text first, as the clipboard is read here; an image when there is no text.
        if offers(ClipboardFormatId::CF_UNICODETEXT) {
            self.post(Request::Paste);
        } else if TAKES_IMAGES && offers(ClipboardFormatId::CF_DIB) {
            self.post(Request::PasteImage);
        }
    }

    fn on_format_data_request(&mut self, request: FormatDataRequest) {
        let offered = self
            .offered
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone();
        let answer = match offered {
            Some(Offer::Text(text)) if request.format == ClipboardFormatId::CF_UNICODETEXT => {
                OwnedFormatDataResponse::new_unicode_string(&text)
            }
            Some(Offer::Image(image)) if request.format == ClipboardFormatId::CF_DIB => {
                OwnedFormatDataResponse::new_data(image.to_vec())
            }
            _ => OwnedFormatDataResponse::new_error(),
        };
        self.post(Request::Answer(answer));
    }

    fn on_format_data_response(&mut self, response: FormatDataResponse<'_>) {
        match self.answered() {
            Some(Asked::Text) => {
                if let Some(text) = text_of(&response) {
                    self.post(Request::Received(text));
                }
            }
            Some(Asked::Image) => {
                if let Some(image) = image_of(&response) {
                    self.post(Request::ReceivedImage(image));
                }
            }
            // Not a list `IronRDP` could read: never taken for text.
            Some(Asked::Files) => self.post(Request::FileListFailed),
            None => {}
        }
    }

    fn on_remote_file_list(&mut self, files: &[FileDescriptor], clip_data_id: Option<u32>) {
        if !matches!(self.asked, Some((Asked::Files, _))) {
            return;
        }
        self.answered();
        self.post(Request::RemoteFileList {
            files: files.to_vec(),
            lock: clip_data_id,
        });
    }

    // The files of a lock, or those offered now: as `IronRDP` chose the list it checked
    // the index against.
    fn on_file_contents_request(&mut self, request: FileContentsRequest) {
        let files = request
            .data_id
            .and_then(|id| self.locked.get(&id))
            .or(self.files.as_ref());
        let entry = usize::try_from(request.index)
            .ok()
            .and_then(|index| files?.get(index))
            .cloned();
        self.post(Request::FileContents { request, entry });
    }

    fn on_file_contents_response(&mut self, response: FileContentsResponse<'_>) {
        let data = (!response.is_error()).then(|| response.data().to_vec());
        self.post(Request::Contents {
            stream: response.stream_id(),
            data,
        });
    }

    fn on_outgoing_locks_cleared(&mut self, clip_data_ids: &[LockDataId]) {
        self.post(Request::LocksCleared(
            clip_data_ids.iter().map(|id| id.0).collect(),
        ));
    }

    fn on_lock(&mut self, id: LockDataId) {
        // `IronRDP` keeps a hundred locks at most, and tells of one only when it keeps it.
        if let Some(files) = &self.files {
            self.locked.insert(id.0, Arc::clone(files));
        }
    }

    fn on_unlock(&mut self, id: LockDataId) {
        self.locked.remove(&id.0);
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::sync::{Arc, Mutex};

    use ironrdp::cliprdr::backend::CliprdrBackend;
    use ironrdp::cliprdr::pdu::{
        ClipboardFormat, ClipboardFormatId, ClipboardGeneralCapabilityFlags, FileContentsFlags,
        FileContentsRequest, FileContentsResponse, FormatDataRequest, FormatDataResponse,
        LockDataId, OwnedFormatDataResponse,
    };
    use tokio::sync::mpsc;
    use zeroize::Zeroizing;

    use super::{
        Asked, ClipboardBackend, MAX_IMAGE_BYTES, MAX_REMOTE_TEXT_BYTES, Offer, Offered, Request,
        image_of, offered_formats, text_of,
    };
    use crate::clipboard_files::Entry;

    fn backend(
        offered: Option<&str>,
    ) -> (ClipboardBackend, mpsc::UnboundedReceiver<Request>, Offered) {
        let (sender, receiver) = mpsc::unbounded_channel();
        let offered: Offered = Arc::new(Mutex::new(
            offered.map(|text| Offer::Text(Zeroizing::new(text.to_owned()))),
        ));
        (
            ClipboardBackend::new(sender, offered.clone()),
            receiver,
            offered,
        )
    }

    #[test]
    fn a_remote_copy_asks_for_its_text_or_else_its_image_and_for_nothing_else() {
        let (mut backend, mut requests, _) = backend(None);
        backend.on_remote_copy(&[ClipboardFormat::new(ClipboardFormatId::CF_HDROP)]);
        assert!(matches!(
            requests.try_recv(),
            Ok(Request::RemoteFiles(false))
        ));
        assert!(
            requests.try_recv().is_err(),
            "no text nor image offered: nothing asked"
        );
        backend.on_remote_copy(&[ClipboardFormat::new(ClipboardFormatId::CF_DIB)]);
        assert!(matches!(
            requests.try_recv(),
            Ok(Request::RemoteFiles(false))
        ));
        assert_eq!(
            matches!(requests.try_recv(), Ok(Request::PasteImage)),
            cfg!(windows),
            "an image is asked for where it reaches the clipboard"
        );
        backend.on_remote_copy(&[
            ClipboardFormat::new(ClipboardFormatId::CF_DIB),
            ClipboardFormat::new(ClipboardFormatId::CF_UNICODETEXT),
        ]);
        assert!(matches!(
            requests.try_recv(),
            Ok(Request::RemoteFiles(false))
        ));
        assert!(matches!(requests.try_recv(), Ok(Request::Paste)));
    }

    #[test]
    fn the_servers_text_is_passed_on_unless_too_large_or_an_error() {
        let text = OwnedFormatDataResponse::new_unicode_string("héllo");
        assert_eq!(text_of(&text).as_deref().map(String::as_str), Some("héllo"));
        assert!(text_of(&FormatDataResponse::new_error()).is_none());
        let huge = FormatDataResponse::new_data(vec![b'a'; MAX_REMOTE_TEXT_BYTES + 2]);
        assert!(text_of(&huge).is_none(), "dropped, not cut");
    }

    #[test]
    fn the_server_gets_our_text_only_when_there_is_some_and_only_as_text() {
        let (mut backend, mut requests, offered) = backend(Some("secret"));
        backend.on_format_data_request(FormatDataRequest {
            format: ClipboardFormatId::CF_UNICODETEXT,
        });
        let Ok(Request::Answer(answer)) = requests.try_recv() else {
            panic!("an answer");
        };
        assert_eq!(answer.to_unicode_string().expect("text"), "secret");

        backend.on_format_data_request(FormatDataRequest {
            format: ClipboardFormatId::CF_DIB,
        });
        let Ok(Request::Answer(answer)) = requests.try_recv() else {
            panic!("an answer");
        };
        assert!(answer.is_error(), "no other format");

        *offered.lock().expect("lock") = None;
        backend.on_format_data_request(FormatDataRequest {
            format: ClipboardFormatId::CF_UNICODETEXT,
        });
        let Ok(Request::Answer(answer)) = requests.try_recv() else {
            panic!("an answer");
        };
        assert!(answer.is_error(), "nothing offered: nothing given");
    }

    /// A device-independent bitmap of `width` by `height` pixels, 32 bits each.
    fn bitmap(width: u32, height: i32) -> Vec<u8> {
        let mut image = Vec::new();
        image.extend_from_slice(&40_u32.to_le_bytes());
        image.extend_from_slice(&width.to_le_bytes());
        image.extend_from_slice(&height.to_le_bytes());
        image.extend_from_slice(&1_u16.to_le_bytes());
        image.extend_from_slice(&32_u16.to_le_bytes());
        image.resize(40, 0);
        let pixels = width as usize * height.unsigned_abs() as usize;
        image.resize(40 + 4 * pixels, 0x7f);
        image
    }

    #[test]
    fn only_what_reads_as_a_bitmap_is_taken_for_the_servers_image() {
        let image = |data: Vec<u8>| image_of(&FormatDataResponse::new_data(data));
        assert!(image(bitmap(2, 2)).is_some());
        assert!(image(bitmap(2, -2)).is_some(), "top down");
        let mut text: Vec<u8> = "a copied sentence, as long as a header or longer"
            .encode_utf16()
            .flat_map(u16::to_le_bytes)
            .collect();
        text.resize(120, 0);
        assert!(image(text).is_none(), "text answered late is not an image");
        let mut planes = bitmap(2, 2);
        planes[12] = 2;
        assert!(image(planes).is_none());
        let mut bits = bitmap(2, 2);
        bits[14] = 7;
        assert!(image(bits).is_none());
        assert!(image(bitmap(0, 2)).is_none(), "no width");
        assert!(image(bitmap(2, 0)).is_none(), "no height");
    }

    #[test]
    fn an_image_goes_both_ways_as_a_bitmap_within_its_size() {
        let (mut backend, mut requests, offered) = backend(None);
        let image = bitmap(3, 5);
        *offered.lock().expect("lock") = Some(Offer::Image(image.clone().into()));
        assert_eq!(
            offered_formats(&offered),
            [ClipboardFormat::new(ClipboardFormatId::CF_DIB)]
        );
        backend.on_format_data_request(FormatDataRequest {
            format: ClipboardFormatId::CF_DIB,
        });
        let Ok(Request::Answer(answer)) = requests.try_recv() else {
            panic!("an answer");
        };
        assert_eq!(answer.data(), image.as_slice());
        backend.on_format_data_request(FormatDataRequest {
            format: ClipboardFormatId::CF_UNICODETEXT,
        });
        let Ok(Request::Answer(answer)) = requests.try_recv() else {
            panic!("an answer");
        };
        assert!(answer.is_error(), "an image is not text");

        // The server's image, once asked for.
        assert!(backend.ask(Asked::Image));
        backend.on_format_data_response(FormatDataResponse::new_data(image.clone()));
        assert!(matches!(
            requests.try_recv(),
            Ok(Request::ReceivedImage(received)) if received == image
        ));
        assert!(
            image_of(&FormatDataResponse::new_data(vec![0; 39])).is_none(),
            "no header"
        );
        let mut huge = bitmap(1, 1);
        huge.resize(MAX_IMAGE_BYTES + 1, 0);
        assert!(
            image_of(&FormatDataResponse::new_data(huge)).is_none(),
            "too large: dropped"
        );
        assert!(image_of(&FormatDataResponse::new_error()).is_none());

        // The server copies: the image offered is no longer kept.
        backend.on_remote_copy(&[ClipboardFormat::new(ClipboardFormatId::CF_UNICODETEXT)]);
        assert!(offered.lock().expect("lock").is_none());
    }

    #[test]
    fn text_is_offered_only_when_there_is_some() {
        let (_, _, offered) = backend(Some(""));
        assert!(offered_formats(&offered).is_empty());
        *offered.lock().expect("lock") = Some(Offer::Text(Zeroizing::new("x".to_owned())));
        assert_eq!(offered_formats(&offered).len(), 1);
    }

    fn entries(names: &[&str]) -> Arc<[Entry]> {
        names
            .iter()
            .map(|name| Entry {
                path: PathBuf::from(name),
                directory: false,
            })
            .collect()
    }

    fn contents(index: i32, data_id: Option<u32>) -> FileContentsRequest {
        FileContentsRequest {
            stream_id: 1,
            index,
            flags: FileContentsFlags::SIZE,
            position: 0,
            requested_size: 8,
            data_id,
        }
    }

    fn asked(requests: &mut mpsc::UnboundedReceiver<Request>) -> Option<PathBuf> {
        let Ok(Request::FileContents { entry, .. }) = requests.try_recv() else {
            panic!("a file request");
        };
        entry.map(|entry| entry.path)
    }

    #[test]
    fn files_are_taken_only_when_both_sides_agree_on_streams() {
        let (mut backend, _, _) = backend(None);
        assert!(
            backend
                .client_capabilities()
                .contains(ClipboardGeneralCapabilityFlags::STREAM_FILECLIP_ENABLED)
        );
        assert!(!backend.takes_files(), "nothing agreed yet");
        backend.on_process_negotiated_capabilities(
            ClipboardGeneralCapabilityFlags::USE_LONG_FORMAT_NAMES,
        );
        assert!(!backend.takes_files(), "a server without file streams");
        backend.on_process_negotiated_capabilities(
            ClipboardGeneralCapabilityFlags::STREAM_FILECLIP_ENABLED,
        );
        assert!(backend.takes_files());
    }

    #[test]
    fn the_server_reads_the_files_of_its_lock_after_a_new_copy() {
        let (mut backend, mut requests, _) = backend(None);
        backend.offer_files(Some(entries(&["first.txt", "second.txt"])));
        backend.on_file_contents_request(contents(1, None));
        assert_eq!(asked(&mut requests), Some(PathBuf::from("second.txt")));

        // Locked, then something else copied: the lock still names the first copy.
        backend.on_lock(LockDataId(9));
        backend.offer_files(Some(entries(&["other.txt"])));
        backend.on_file_contents_request(contents(1, Some(9)));
        assert_eq!(asked(&mut requests), Some(PathBuf::from("second.txt")));
        backend.on_file_contents_request(contents(0, None));
        assert_eq!(asked(&mut requests), Some(PathBuf::from("other.txt")));

        // Unlocked: as `IronRDP`, the files offered now answer for it.
        backend.on_unlock(LockDataId(9));
        backend.on_file_contents_request(contents(0, Some(9)));
        assert_eq!(asked(&mut requests), Some(PathBuf::from("other.txt")));
        backend.on_file_contents_request(contents(1, Some(9)));
        assert_eq!(asked(&mut requests), None, "no such index");

        // Text copied: no file is offered any more.
        backend.offer_files(None);
        backend.on_file_contents_request(contents(0, None));
        assert_eq!(asked(&mut requests), None);
    }

    #[test]
    fn a_lock_without_files_offered_keeps_nothing() {
        let (mut backend, mut requests, _) = backend(None);
        backend.on_lock(LockDataId(3));
        backend.offer_files(Some(entries(&["late.txt"])));
        backend.on_file_contents_request(contents(0, Some(3)));
        assert_eq!(
            asked(&mut requests),
            Some(PathBuf::from("late.txt")),
            "the files offered now, as `IronRDP` serves it"
        );
    }

    fn file_list(id: u32) -> ClipboardFormat {
        ClipboardFormat::new(ClipboardFormatId::new(id))
            .with_name(ironrdp::cliprdr::pdu::ClipboardFormatName::FILE_LIST)
    }

    #[test]
    fn a_remote_copy_says_whether_it_holds_files_by_their_exact_name() {
        let (mut backend, mut requests, _) = backend(None);
        backend.on_remote_copy(&[file_list(0xC0A1)]);
        assert!(matches!(
            requests.try_recv(),
            Ok(Request::RemoteFiles(true))
        ));
        assert_eq!(backend.remote_files(), Some(ClipboardFormatId::new(0xC0A1)));
        backend.on_remote_copy(&[
            ClipboardFormat::new(ClipboardFormatId::new(0xC0A2)).with_name(
                ironrdp::cliprdr::pdu::ClipboardFormatName::new("filegroupdescriptorw"),
            ),
        ]);
        assert!(matches!(
            requests.try_recv(),
            Ok(Request::RemoteFiles(false))
        ));
        assert_eq!(backend.remote_files(), None);
    }

    #[test]
    fn a_list_of_files_that_does_not_come_is_never_taken_for_text() {
        let (mut backend, mut requests, _) = backend(None);
        assert!(backend.ask(super::Asked::Files));
        // The server copies text meanwhile: asked for once the list has come.
        backend.on_remote_copy(&[ClipboardFormat::new(ClipboardFormatId::CF_UNICODETEXT)]);
        assert!(matches!(
            requests.try_recv(),
            Ok(Request::RemoteFiles(false))
        ));
        assert!(matches!(requests.try_recv(), Ok(Request::Paste)));
        assert!(!backend.ask(super::Asked::Text), "the text waits");

        let raw = OwnedFormatDataResponse::new_unicode_string("not a list");
        backend.on_format_data_response(raw);
        assert!(
            matches!(requests.try_recv(), Ok(Request::Paste)),
            "asked again"
        );
        assert!(matches!(requests.try_recv(), Ok(Request::FileListFailed)));
        assert!(requests.try_recv().is_err(), "nothing taken for text");
        assert!(backend.asked.is_none());
    }

    #[test]
    fn one_question_at_a_time_so_the_servers_newer_text_is_not_lost() {
        let (mut backend, mut requests, _) = backend(None);
        assert!(backend.ask(super::Asked::Text));
        // The server copies again before answering: its new text is asked afterwards.
        assert!(!backend.ask(super::Asked::Text));
        assert!(!backend.ask(super::Asked::Files));
        backend.on_format_data_response(FormatDataResponse::new_error());
        assert!(matches!(requests.try_recv(), Ok(Request::Paste)));
        assert!(
            requests.try_recv().is_err(),
            "the failed answer gives no text"
        );
        assert!(backend.ask(super::Asked::Text));
        backend.on_format_data_response(OwnedFormatDataResponse::new_unicode_string("newer"));
        assert!(matches!(requests.try_recv(), Ok(Request::ListFiles)));
        assert!(
            matches!(requests.try_recv(), Ok(Request::Received(text)) if text.as_str() == "newer")
        );
    }

    #[test]
    fn an_answer_that_never_comes_is_given_up() {
        let (mut backend, mut requests, _) = backend(None);
        assert!(backend.ask(super::Asked::Files));
        assert!(!backend.ask(super::Asked::Text));
        backend.expire();
        assert!(requests.try_recv().is_err(), "not yet");
        backend.expire_at(u64::MAX);
        assert!(matches!(requests.try_recv(), Ok(Request::Paste)));
        assert!(backend.asked.is_none());
    }

    #[test]
    fn the_servers_list_and_bytes_reach_the_session() {
        let (mut backend, mut requests, _) = backend(None);
        let files = [ironrdp::cliprdr::pdu::FileDescriptor::new("a.txt")];
        backend.on_remote_file_list(&files, Some(4));
        assert!(requests.try_recv().is_err(), "not asked for: dropped");

        assert!(backend.ask(super::Asked::Files));
        backend.on_remote_file_list(&files, Some(4));
        let Ok(Request::RemoteFileList { files, lock }) = requests.try_recv() else {
            panic!("the list");
        };
        assert_eq!((files.len(), lock), (1, Some(4)));

        backend.on_file_contents_response(FileContentsResponse::new_data_response(3, vec![7]));
        assert!(matches!(
            requests.try_recv(),
            Ok(Request::Contents { stream: 3, data: Some(data) }) if data == [7]
        ));
        backend.on_file_contents_response(FileContentsResponse::new_error(5));
        assert!(matches!(
            requests.try_recv(),
            Ok(Request::Contents {
                stream: 5,
                data: None
            })
        ));
        backend.on_outgoing_locks_cleared(&[LockDataId(4)]);
        assert!(matches!(requests.try_recv(), Ok(Request::LocksCleared(ids)) if ids == [4]));
    }

    #[test]
    fn the_servers_text_is_taken_only_when_asked_for() {
        let (mut backend, mut requests, _) = backend(None);
        let text = OwnedFormatDataResponse::new_unicode_string("copied there");
        backend.on_format_data_response(text.clone());
        assert!(requests.try_recv().is_err(), "not asked for");
        assert!(backend.ask(super::Asked::Text));
        backend.on_format_data_response(text);
        assert!(
            matches!(requests.try_recv(), Ok(Request::Received(text)) if text.as_str() == "copied there")
        );
    }
}
