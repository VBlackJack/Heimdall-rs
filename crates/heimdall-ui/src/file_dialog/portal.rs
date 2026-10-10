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

//! A client of the desktop portal's `FileChooser`, `org.freedesktop.portal.FileChooser`, over
//! the D-Bus session bus, in Rust through `zbus`: the open, save and folder dialogs of Linux.
//!
//! A request goes as the portal specification asks: the `Response` signal of the request is
//! listened to on the path the portal will give it, computed from this connection's unique
//! name and a `handle_token`, before the method is called, so an answer cannot come before
//! it is listened to. A portal older than 0.9 gives another path: it is then listened to
//! once known, as the specification says those portals require. Only an answer from the
//! portal that took the request, on its path, is read.

use std::collections::HashMap;
use std::ffi::OsString;
use std::hash::BuildHasher;
use std::os::raw::c_ulong;
use std::os::unix::ffi::{OsStrExt, OsStringExt};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

use iced::futures::StreamExt;
use iced::window::raw_window_handle::RawWindowHandle;
use zbus::message::Type;
use zbus::zvariant::{OwnedObjectPath, OwnedValue, Value};
use zbus::{Connection, MatchRule, MessageStream};

use super::Unavailable;

/// The bus name of the desktop portal.
pub const PORTAL_DESTINATION: &str = "org.freedesktop.portal.Desktop";

/// The object the portal serves its interfaces on.
pub const PORTAL_PATH: &str = "/org/freedesktop/portal/desktop";

/// The interface of the file dialogs.
pub const FILE_CHOOSER_INTERFACE: &str = "org.freedesktop.portal.FileChooser";

/// The interface of a request in flight, which says its answer.
pub const REQUEST_INTERFACE: &str = "org.freedesktop.portal.Request";

/// The signal carrying a request's answer.
pub const RESPONSE_MEMBER: &str = "Response";

/// The method of the open and folder dialogs.
pub const OPEN_FILE_METHOD: &str = "OpenFile";

/// The method of the save dialog.
pub const SAVE_FILE_METHOD: &str = "SaveFile";

/// Where the portal puts a request: this prefix, the caller's unique name, then its token.
const REQUEST_PATH_PREFIX: &str = "/org/freedesktop/portal/desktop/request";

/// The response code of a dialog that answered.
pub const RESPONSE_SUCCESS: u32 = 0;

/// The response code of a dialog the user cancelled.
pub const RESPONSE_CANCELLED: u32 = 1;

/// The type of a filter's pattern that is a glob, as `*.json`.
pub const FILTER_GLOB: u32 = 0;

/// The pattern of every file.
const EVERY_FILE: &str = "*";

/// The prefix of an X11 window named to the portal; its id follows in hexadecimal.
const X11_PARENT_PREFIX: &str = "x11:";

/// The only scheme of a local file.
const FILE_SCHEME: &str = "file";

/// The host a local file URI may name.
const LOCAL_HOST: &str = "localhost";

/// The start of a handle token, which names this application's requests.
const TOKEN_PREFIX: &str = "heimdall";

/// The options of a request.
const OPTION_HANDLE_TOKEN: &str = "handle_token";
const OPTION_MULTIPLE: &str = "multiple";
const OPTION_DIRECTORY: &str = "directory";
const OPTION_FILTERS: &str = "filters";
const OPTION_CURRENT_FOLDER: &str = "current_folder";
const OPTION_CURRENT_NAME: &str = "current_name";

/// The result of an answer naming the files chosen.
const RESULT_URIS: &str = "uris";

/// The method errors of a bus where no portal, or no `FileChooser`, answers.
const NO_PORTAL_ERRORS: [&str; 5] = [
    "org.freedesktop.DBus.Error.ServiceUnknown",
    "org.freedesktop.DBus.Error.NameHasNoOwner",
    "org.freedesktop.DBus.Error.UnknownMethod",
    "org.freedesktop.DBus.Error.UnknownInterface",
    "org.freedesktop.DBus.Error.UnknownObject",
];

/// The requests made by this process, numbered so that no two share a token.
static REQUESTS: AtomicU64 = AtomicU64::new(0);

/// Which dialog.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    /// Open one file.
    OpenFile,
    /// Open one file or more.
    OpenFiles,
    /// Pick one folder.
    OpenFolder,
    /// Save one file.
    Save,
}

impl Mode {
    /// The method of the dialog.
    #[must_use]
    pub fn method(self) -> &'static str {
        match self {
            Self::OpenFile | Self::OpenFiles | Self::OpenFolder => OPEN_FILE_METHOD,
            Self::Save => SAVE_FILE_METHOD,
        }
    }
}

/// Files offered under a name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Filter {
    /// The name shown.
    pub name: String,
    /// The extensions, without their dot; `*` or nothing is every file.
    pub extensions: Vec<String>,
}

/// What a dialog is asked to show.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Request {
    /// Its title; empty for the portal's own.
    pub title: String,
    /// The window holding it, as the portal names windows; empty for none.
    pub parent_window: String,
    /// Its filters, in order.
    pub filters: Vec<Filter>,
    /// The folder it opens in.
    pub current_folder: Option<PathBuf>,
    /// The file name a save dialog offers.
    pub current_name: Option<String>,
}

/// The window `parent` as the portal names it: `x11:` and its id for an X11 window, nothing
/// for any other.
#[must_use]
pub fn parent_window(parent: &dyn iced::window::Window) -> String {
    match parent.window_handle().map(|handle| handle.as_raw()) {
        Ok(RawWindowHandle::Xlib(window)) => x11_parent(window.window),
        Ok(RawWindowHandle::Xcb(window)) => x11_parent(c_ulong::from(window.window.get())),
        _ => String::new(),
    }
}

/// The X11 window `id` as the portal names it.
#[must_use]
pub fn x11_parent(id: c_ulong) -> String {
    format!("{X11_PARENT_PREFIX}{id:x}")
}

/// A token naming a new request: letters, digits and underscores only, as an object path
/// element must be.
#[must_use]
pub fn handle_token() -> String {
    let number = REQUESTS.fetch_add(1, Ordering::Relaxed);
    format!("{TOKEN_PREFIX}_{}_{number}", std::process::id())
}

/// The path the portal gives the request of `token` made by the connection of unique name
/// `sender`: its leading `:` dropped and its dots made underscores.
///
/// # Errors
///
/// When the name or the token cannot make an object path.
pub fn request_path(sender: &str, token: &str) -> Result<OwnedObjectPath, Unavailable> {
    let sender = sender.trim_start_matches(':').replace('.', "_");
    OwnedObjectPath::try_from(format!("{REQUEST_PATH_PREFIX}/{sender}/{token}"))
        .map_err(|error| Unavailable::Failed(error.to_string()))
}

/// The filters as the portal reads them, `a(sa(us))`: each name with its glob patterns. A
/// filter without extensions is left out.
#[must_use]
pub fn encode_filters(filters: &[Filter]) -> Vec<(String, Vec<(u32, String)>)> {
    filters
        .iter()
        .filter(|filter| !filter.extensions.is_empty())
        .map(|filter| {
            let globs = filter
                .extensions
                .iter()
                .map(|extension| {
                    let glob = if extension.is_empty() || extension == EVERY_FILE {
                        EVERY_FILE.to_owned()
                    } else {
                        format!("*.{extension}")
                    };
                    (FILTER_GLOB, glob)
                })
                .collect();
            (filter.name.clone(), globs)
        })
        .collect()
}

/// A folder as the portal reads it, `ay`: its bytes, then a NUL.
#[must_use]
pub fn encode_path(path: &std::path::Path) -> Vec<u8> {
    let mut bytes = path.as_os_str().as_bytes().to_vec();
    bytes.push(0);
    bytes
}

/// The options of `request` shown as `mode`, under `token`.
fn options<'a>(request: &'a Request, mode: Mode, token: &'a str) -> HashMap<&'a str, Value<'a>> {
    let mut options = HashMap::new();
    options.insert(OPTION_HANDLE_TOKEN, Value::from(token));
    match mode {
        Mode::OpenFile => {
            options.insert(OPTION_MULTIPLE, Value::from(false));
        }
        Mode::OpenFiles => {
            options.insert(OPTION_MULTIPLE, Value::from(true));
        }
        Mode::OpenFolder => {
            options.insert(OPTION_MULTIPLE, Value::from(false));
            options.insert(OPTION_DIRECTORY, Value::from(true));
        }
        Mode::Save => {
            if let Some(name) = &request.current_name {
                options.insert(OPTION_CURRENT_NAME, Value::from(name.as_str()));
            }
        }
    }
    let filters = encode_filters(&request.filters);
    if !filters.is_empty() {
        options.insert(OPTION_FILTERS, Value::from(filters));
    }
    if let Some(folder) = &request.current_folder {
        options.insert(OPTION_CURRENT_FOLDER, Value::from(encode_path(folder)));
    }
    options
}

/// The local path a `file://` URI names, percent-decoded, as `GLib`'s
/// `g_filename_from_uri` reads one: no other scheme, no host but `localhost`, no query or
/// fragment, and no escaped `/` or NUL, which no file name holds.
///
/// # Errors
///
/// [`Unavailable::NotLocal`] for any other URI.
pub fn uri_to_path(uri: &str) -> Result<PathBuf, Unavailable> {
    let not_local = || Unavailable::NotLocal(uri.to_owned());
    let (scheme, rest) = uri.split_once("://").ok_or_else(not_local)?;
    if !scheme.eq_ignore_ascii_case(FILE_SCHEME) {
        return Err(not_local());
    }
    let path = match rest.find('/') {
        Some(0) => rest,
        Some(start) if rest[..start].eq_ignore_ascii_case(LOCAL_HOST) => &rest[start..],
        _ => return Err(not_local()),
    };
    if path.contains(['?', '#']) {
        return Err(not_local());
    }
    let bytes = percent_decode(path.as_bytes()).ok_or_else(not_local)?;
    Ok(PathBuf::from(OsString::from_vec(bytes)))
}

/// `encoded` with its `%XX` escapes decoded; `None` for an escape cut short or not
/// hexadecimal, or one of `/` or NUL.
fn percent_decode(encoded: &[u8]) -> Option<Vec<u8>> {
    let mut decoded = Vec::with_capacity(encoded.len());
    let mut bytes = encoded.iter();
    while let Some(&byte) = bytes.next() {
        if byte != b'%' {
            decoded.push(byte);
            continue;
        }
        let high = hex_digit(*bytes.next()?)?;
        let low = hex_digit(*bytes.next()?)?;
        let escaped = (high << 4) | low;
        if escaped == b'/' || escaped == 0 {
            return None;
        }
        decoded.push(escaped);
    }
    Some(decoded)
}

/// The value of the hexadecimal digit `digit`.
fn hex_digit(digit: u8) -> Option<u8> {
    char::from(digit)
        .to_digit(16)
        .and_then(|value| u8::try_from(value).ok())
}

/// The paths of an answer of `code` with `results`: `None` when the dialog was cancelled.
///
/// # Errors
///
/// Why the answer cannot be used: the dialog ended otherwise, named no file, or named a
/// place that is not a local file.
pub fn parse_response<S: BuildHasher>(
    code: u32,
    results: &HashMap<String, OwnedValue, S>,
) -> Result<Option<Vec<PathBuf>>, Unavailable> {
    match code {
        RESPONSE_SUCCESS => {
            let uris = results
                .get(RESULT_URIS)
                .and_then(|uris| uris.try_clone().ok())
                .and_then(|uris| Vec::<String>::try_from(uris).ok())
                .ok_or(Unavailable::NoLocation)?;
            if uris.is_empty() {
                return Ok(None);
            }
            uris.iter()
                .map(|uri| uri_to_path(uri))
                .collect::<Result<Vec<_>, _>>()
                .map(Some)
        }
        RESPONSE_CANCELLED => Ok(None),
        other => Err(Unavailable::Ended(other)),
    }
}

/// The dialog of `request` shown as `mode` by the portal of the session bus: the paths
/// picked, or `None` when it was cancelled.
///
/// # Errors
///
/// Why no dialog was shown, or why its answer cannot be used.
pub async fn choose(request: Request, mode: Mode) -> Result<Option<Vec<PathBuf>>, Unavailable> {
    let connection = Connection::session()
        .await
        .map_err(|error| Unavailable::NoSessionBus(error.to_string()))?;
    choose_on(&connection, &request, mode).await
}

/// The dialog of `request` shown as `mode` by the portal on `connection`.
///
/// # Errors
///
/// Why no dialog was shown, or why its answer cannot be used.
pub async fn choose_on(
    connection: &Connection,
    request: &Request,
    mode: Mode,
) -> Result<Option<Vec<PathBuf>>, Unavailable> {
    let token = handle_token();
    // Listened to before the call. A connection without a bus, peer to peer, has no unique
    // name: every answer is listened to, and only the request's is read.
    let expected = connection
        .unique_name()
        .map(|name| request_path(name.as_str(), &token))
        .transpose()?;
    let mut responses = responses(connection, expected.as_ref()).await?;
    let reply = connection
        .call_method(
            Some(PORTAL_DESTINATION),
            PORTAL_PATH,
            Some(FILE_CHOOSER_INTERFACE),
            mode.method(),
            &(
                request.parent_window.as_str(),
                request.title.as_str(),
                options(request, mode, &token),
            ),
        )
        .await
        .map_err(|error| call_error(&error))?;
    let handle: OwnedObjectPath = reply
        .body()
        .deserialize()
        .map_err(|error| Unavailable::Failed(error.to_string()))?;
    let portal = reply.header().sender().map(ToString::to_string);
    if expected
        .as_ref()
        .is_some_and(|expected| *expected != handle)
    {
        // A portal older than 0.9, which does not name requests after their token.
        responses = self::responses(connection, Some(&handle)).await?;
    }
    while let Some(message) = responses.next().await {
        // A read that fails is the bus gone: the portal can no longer answer.
        let message = message.map_err(|error| match error {
            zbus::Error::InputOutput(_) => Unavailable::Closed,
            other => Unavailable::Failed(other.to_string()),
        })?;
        let header = message.header();
        let on_request =
            header.path().map(zbus::zvariant::ObjectPath::as_str) == Some(handle.as_str());
        let from_portal = header.sender().map(ToString::to_string) == portal;
        if !on_request || !from_portal {
            continue;
        }
        let (code, results): (u32, HashMap<String, OwnedValue>) = message
            .body()
            .deserialize()
            .map_err(|error| Unavailable::Failed(error.to_string()))?;
        return parse_response(code, &results);
    }
    Err(Unavailable::Closed)
}

/// The `Response` signals of requests on `connection`, of the request at `path` when known.
async fn responses(
    connection: &Connection,
    path: Option<&OwnedObjectPath>,
) -> Result<MessageStream, Unavailable> {
    let failed = |error: zbus::Error| Unavailable::Failed(error.to_string());
    let mut rule = MatchRule::builder()
        .msg_type(Type::Signal)
        .interface(REQUEST_INTERFACE)
        .map_err(failed)?
        .member(RESPONSE_MEMBER)
        .map_err(failed)?;
    if let Some(path) = path {
        rule = rule.path(path.as_ref()).map_err(failed)?;
    }
    MessageStream::for_match_rule(rule.build(), connection, None)
        .await
        .map_err(failed)
}

/// Why a call to the portal failed: no portal, or another reason.
fn call_error(error: &zbus::Error) -> Unavailable {
    match error {
        zbus::Error::MethodError(name, _, _) if NO_PORTAL_ERRORS.contains(&name.as_str()) => {
            Unavailable::NoPortal(error.to_string())
        }
        _ => Unavailable::Failed(error.to_string()),
    }
}

#[cfg(test)]
mod tests;
