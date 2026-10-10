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
use std::time::Duration;

use iced::futures::future::{Either, select};
use iced::futures::{FutureExt, StreamExt};
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
    let no_bus = |error: zbus::Error| Unavailable::NoSessionBus(error.to_string());
    let connection = zbus::connection::Builder::session()
        .map_err(no_bus)?
        .method_timeout(CALL_TIMEOUT)
        .build()
        .await
        .map_err(no_bus)?;
    choose_on(&connection, &request, mode).await
}

/// The dialog of `request` shown as `mode` by the portal on `connection`.
///
/// It waits for the user as long as they take, and no longer than the portal lives: the
/// answer stops being waited for when the connection closes or, on a bus, when the portal
/// that took the request leaves it.
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
    let pending = Taken {
        handle,
        portal: portal.clone(),
    };
    // The portal that took the request, watched until it answers: if it leaves the bus, no
    // answer will come. Watched first, then asked about, so that leaving in between is seen.
    let mut departures = match &portal {
        Some(portal) => {
            let departures = departures(connection, portal).await?;
            if !has_owner(connection, portal).await? {
                return pending
                    .queued(&mut responses)
                    .unwrap_or(Err(Unavailable::Closed));
            }
            Some(departures)
        }
        None => None,
    };
    loop {
        let message = match departures.as_mut() {
            None => responses.next().await,
            Some(departures) => match select(responses.next(), departures.next()).await {
                Either::Left((message, _)) => message,
                Either::Right((departure, _)) => {
                    if departure.is_none_or(|departure| has_left(departure, portal.as_deref())) {
                        // An answer sent just before leaving is already queued: it counts.
                        return pending
                            .queued(&mut responses)
                            .unwrap_or(Err(Unavailable::Closed));
                    }
                    continue;
                }
            },
        };
        let Some(message) = message else {
            return Err(Unavailable::Closed);
        };
        if let Some(answer) = pending.answer(message) {
            return answer;
        }
    }
}

/// How long a call to the bus or the portal may take to be answered: the D-Bus default. The
/// dialog's own answer, which waits for the user, is not bounded by it.
const CALL_TIMEOUT: Duration = Duration::from_secs(25);

/// The bus itself, which says when a name leaves it.
const BUS_NAME: &str = "org.freedesktop.DBus";

/// The object and interface of the bus.
const BUS_PATH: &str = "/org/freedesktop/DBus";
const BUS_INTERFACE: &str = "org.freedesktop.DBus";

/// The signal of a name that changed owner, and the method asking whether one has one.
const NAME_OWNER_CHANGED: &str = "NameOwnerChanged";
const NAME_HAS_OWNER: &str = "NameHasOwner";

/// A request taken by the portal: where its answer comes, and from whom.
struct Taken {
    /// The request's object.
    handle: OwnedObjectPath,
    /// The unique name of the portal that took it; none peer to peer.
    portal: Option<String>,
}

impl Taken {
    /// The dialog's answer when `message` is this request's `Response` from its portal;
    /// `None` for any other message.
    fn answer(
        &self,
        message: zbus::Result<zbus::Message>,
    ) -> Option<Result<Option<Vec<PathBuf>>, Unavailable>> {
        // A read that fails is the connection gone: the portal can no longer answer.
        let message = match message {
            Ok(message) => message,
            Err(zbus::Error::InputOutput(_)) => return Some(Err(Unavailable::Closed)),
            Err(other) => return Some(Err(Unavailable::Failed(other.to_string()))),
        };
        let header = message.header();
        let on_request =
            header.path().map(zbus::zvariant::ObjectPath::as_str) == Some(self.handle.as_str());
        let from_portal = header.sender().map(ToString::to_string) == self.portal;
        if !on_request || !from_portal {
            return None;
        }
        Some(
            message
                .body()
                .deserialize::<(u32, HashMap<String, OwnedValue>)>()
                .map_err(|error| Unavailable::Failed(error.to_string()))
                .and_then(|(code, results)| parse_response(code, &results)),
        )
    }

    /// The answer already received in `responses`, without waiting for another.
    fn queued(
        &self,
        responses: &mut MessageStream,
    ) -> Option<Result<Option<Vec<PathBuf>>, Unavailable>> {
        while let Some(Some(message)) = responses.next().now_or_never() {
            if let Some(answer) = self.answer(message) {
                return Some(answer);
            }
        }
        None
    }
}

/// The bus's `NameOwnerChanged` signals about `portal` on `connection`.
async fn departures(connection: &Connection, portal: &str) -> Result<MessageStream, Unavailable> {
    let failed = |error: zbus::Error| Unavailable::Failed(error.to_string());
    let rule = MatchRule::builder()
        .msg_type(Type::Signal)
        .sender(BUS_NAME)
        .map_err(failed)?
        .path(BUS_PATH)
        .map_err(failed)?
        .interface(BUS_INTERFACE)
        .map_err(failed)?
        .member(NAME_OWNER_CHANGED)
        .map_err(failed)?
        .arg(0, portal)
        .map_err(failed)?
        .build();
    MessageStream::for_match_rule(rule, connection, None)
        .await
        .map_err(failed)
}

/// Whether `portal` is still on the bus of `connection`.
async fn has_owner(connection: &Connection, portal: &str) -> Result<bool, Unavailable> {
    let failed = |error: zbus::Error| Unavailable::Failed(error.to_string());
    connection
        .call_method(
            Some(BUS_NAME),
            BUS_PATH,
            Some(BUS_INTERFACE),
            NAME_HAS_OWNER,
            &(portal,),
        )
        .await
        .map_err(failed)?
        .body()
        .deserialize::<bool>()
        .map_err(failed)
}

/// Whether `departure` says, from the bus itself, that `portal` left it. A unique name never
/// changes owner but to none. A failed read is the connection gone, and the portal with it.
fn has_left(departure: zbus::Result<zbus::Message>, portal: Option<&str>) -> bool {
    let Ok(departure) = departure else {
        return true;
    };
    let from_bus = departure
        .header()
        .sender()
        .map(zbus::names::UniqueName::as_str)
        == Some(BUS_NAME);
    let gone = departure
        .body()
        .deserialize::<(String, String, String)>()
        .is_ok_and(|(name, _, new_owner)| Some(name.as_str()) == portal && new_owner.is_empty());
    from_bus && gone
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
