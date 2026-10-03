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

//! The clipboard channel (MS-RDPECLIP): text both ways, and files from this side.
//!
//! `IronRDP` calls the backend from inside its channel processing, where the channel cannot
//! be driven again; so the backend only posts what it wants done, and the session loop does
//! it: ask the server for its text, answer the server's request for ours, offer ours, and
//! read the files it asks for.
//!
//! No other format, and no files from the server: text and files copied here are what a
//! connection manager needs, and each other format is more that a server could send.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, PoisonError};

use ironrdp::cliprdr::backend::CliprdrBackend;
use ironrdp::cliprdr::pdu::{
    ClipboardFormat, ClipboardFormatId, ClipboardGeneralCapabilityFlags, FileContentsRequest,
    FileContentsResponse, FormatDataRequest, FormatDataResponse, LockDataId,
    OwnedFormatDataResponse,
};
use tokio::sync::mpsc;
use zeroize::Zeroizing;

use crate::clipboard_files::Entry;

/// Longest text taken from the server, in bytes of UTF-16: a larger one is dropped, not cut.
pub const MAX_REMOTE_TEXT_BYTES: usize = 8 * 1024 * 1024;

/// What the backend asks the session loop to do.
#[derive(Debug)]
pub(crate) enum Request {
    /// Ask the server for its clipboard, as text.
    Paste,
    /// Answer the server's request for our clipboard.
    Answer(OwnedFormatDataResponse),
    /// Tell the server what our clipboard holds.
    Offer,
    /// The server's text arrived.
    Received(Zeroizing<String>),
    /// The server asks for a file offered, or for its size: `entry` is the file, `None`
    /// when the server's index names none.
    FileContents {
        /// The request.
        request: FileContentsRequest,
        /// The file it names.
        entry: Option<Entry>,
    },
}

/// The text this side offers the server, until it asks for it.
pub(crate) type Offered = Arc<Mutex<Option<Zeroizing<String>>>>;

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
}

impl ClipboardBackend {
    pub(crate) fn new(requests: mpsc::UnboundedSender<Request>, offered: Offered) -> Self {
        Self {
            requests,
            offered,
            takes_files: false,
            files: None,
            locked: HashMap::new(),
        }
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

/// The formats this side offers: text, when there is some.
pub(crate) fn offered_formats(offered: &Offered) -> Vec<ClipboardFormat> {
    let has_text = offered
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .as_ref()
        .is_some_and(|text| !text.is_empty());
    if has_text {
        vec![ClipboardFormat::new(ClipboardFormatId::CF_UNICODETEXT)]
    } else {
        Vec::new()
    }
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
        if available_formats
            .iter()
            .any(|format| format.id() == ClipboardFormatId::CF_UNICODETEXT)
        {
            self.post(Request::Paste);
        }
    }

    fn on_format_data_request(&mut self, request: FormatDataRequest) {
        let offered = self
            .offered
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone();
        let answer = match offered {
            Some(text) if request.format == ClipboardFormatId::CF_UNICODETEXT => {
                OwnedFormatDataResponse::new_unicode_string(&text)
            }
            _ => OwnedFormatDataResponse::new_error(),
        };
        self.post(Request::Answer(answer));
    }

    fn on_format_data_response(&mut self, response: FormatDataResponse<'_>) {
        if let Some(text) = text_of(&response) {
            self.post(Request::Received(text));
        }
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

    // A server's files are not taken.
    fn on_file_contents_response(&mut self, _: FileContentsResponse<'_>) {}

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
        FileContentsRequest, FormatDataRequest, FormatDataResponse, LockDataId,
        OwnedFormatDataResponse,
    };
    use tokio::sync::mpsc;
    use zeroize::Zeroizing;

    use super::{
        ClipboardBackend, MAX_REMOTE_TEXT_BYTES, Offered, Request, offered_formats, text_of,
    };
    use crate::clipboard_files::Entry;

    fn backend(
        offered: Option<&str>,
    ) -> (ClipboardBackend, mpsc::UnboundedReceiver<Request>, Offered) {
        let (sender, receiver) = mpsc::unbounded_channel();
        let offered: Offered = Arc::new(Mutex::new(
            offered.map(|text| Zeroizing::new(text.to_owned())),
        ));
        (
            ClipboardBackend::new(sender, offered.clone()),
            receiver,
            offered,
        )
    }

    #[test]
    fn a_remote_copy_with_text_asks_for_it_and_one_without_does_not() {
        let (mut backend, mut requests, _) = backend(None);
        backend.on_remote_copy(&[ClipboardFormat::new(ClipboardFormatId::CF_DIB)]);
        assert!(
            requests.try_recv().is_err(),
            "no text offered: nothing asked"
        );
        backend.on_remote_copy(&[
            ClipboardFormat::new(ClipboardFormatId::CF_DIB),
            ClipboardFormat::new(ClipboardFormatId::CF_UNICODETEXT),
        ]);
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

    #[test]
    fn text_is_offered_only_when_there_is_some() {
        let (_, _, offered) = backend(Some(""));
        assert!(offered_formats(&offered).is_empty());
        *offered.lock().expect("lock") = Some(Zeroizing::new("x".to_owned()));
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
}
