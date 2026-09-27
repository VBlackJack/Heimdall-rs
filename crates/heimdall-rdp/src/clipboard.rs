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

//! The clipboard channel (MS-RDPECLIP), text only.
//!
//! `IronRDP` calls the backend from inside its channel processing, where the channel cannot
//! be driven again; so the backend only posts what it wants done, and the session loop does
//! it: ask the server for its text, answer the server's request for ours, offer ours.
//!
//! No files, and no other format: text is what a connection manager needs, and each other
//! format is more that a server could send.

use std::sync::{Arc, Mutex, PoisonError};

use ironrdp::cliprdr::backend::CliprdrBackend;
use ironrdp::cliprdr::pdu::{
    ClipboardFormat, ClipboardFormatId, ClipboardGeneralCapabilityFlags, FileContentsRequest,
    FileContentsResponse, FormatDataRequest, FormatDataResponse, LockDataId,
    OwnedFormatDataResponse,
};
use tokio::sync::mpsc;
use zeroize::Zeroizing;

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
}

/// The text this side offers the server, until it asks for it.
pub(crate) type Offered = Arc<Mutex<Option<Zeroizing<String>>>>;

/// The backend: reads what `IronRDP` reports and posts [`Request`]s.
#[derive(Debug)]
pub(crate) struct TextBackend {
    requests: mpsc::UnboundedSender<Request>,
    offered: Offered,
}

impl TextBackend {
    pub(crate) fn new(requests: mpsc::UnboundedSender<Request>, offered: Offered) -> Self {
        Self { requests, offered }
    }

    fn post(&self, request: Request) {
        // The session may be gone: nothing to do then.
        let _ = self.requests.send(request);
    }
}

ironrdp_core::impl_as_any!(TextBackend);

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

impl CliprdrBackend for TextBackend {
    #[expect(
        clippy::unnecessary_literal_bound,
        reason = "the signature is the trait's"
    )]
    fn temporary_directory(&self) -> &str {
        // Never used: no file is ever offered or accepted.
        ""
    }

    fn client_capabilities(&self) -> ClipboardGeneralCapabilityFlags {
        ClipboardGeneralCapabilityFlags::empty()
    }

    fn on_ready(&mut self) {}

    fn on_request_format_list(&mut self) {
        self.post(Request::Offer);
    }

    fn on_process_negotiated_capabilities(&mut self, _: ClipboardGeneralCapabilityFlags) {}

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

    // Files are never offered, and a server's are not taken.
    fn on_file_contents_request(&mut self, _: FileContentsRequest) {}

    fn on_file_contents_response(&mut self, _: FileContentsResponse<'_>) {}

    fn on_lock(&mut self, _: LockDataId) {}

    fn on_unlock(&mut self, _: LockDataId) {}
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use ironrdp::cliprdr::backend::CliprdrBackend;
    use ironrdp::cliprdr::pdu::{
        ClipboardFormat, ClipboardFormatId, FormatDataRequest, FormatDataResponse,
        OwnedFormatDataResponse,
    };
    use tokio::sync::mpsc;
    use zeroize::Zeroizing;

    use super::{MAX_REMOTE_TEXT_BYTES, Offered, Request, TextBackend, offered_formats, text_of};

    fn backend(offered: Option<&str>) -> (TextBackend, mpsc::UnboundedReceiver<Request>, Offered) {
        let (sender, receiver) = mpsc::unbounded_channel();
        let offered: Offered = Arc::new(Mutex::new(
            offered.map(|text| Zeroizing::new(text.to_owned())),
        ));
        (TextBackend::new(sender, offered.clone()), receiver, offered)
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
}
