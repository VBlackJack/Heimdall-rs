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

//! The portal client's encodings and decodings, then whole requests answered by a fake
//! portal on a private peer-to-peer D-Bus connection, without a bus daemon.

use std::collections::HashMap;
use std::future::Future;
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::time::Duration;

use iced::futures::StreamExt;
use zbus::connection::Builder;
use zbus::message::Type;
use zbus::zvariant::{OwnedObjectPath, OwnedValue, Value};
use zbus::{Connection, Guid, MessageStream};

use super::*;

fn results(uris: &[&str]) -> HashMap<String, OwnedValue> {
    let uris: Vec<String> = uris.iter().map(|uri| (*uri).to_owned()).collect();
    HashMap::from([(
        RESULT_URIS.to_owned(),
        OwnedValue::try_from(Value::from(uris)).expect("an array of strings"),
    )])
}

#[test]
fn a_file_uri_is_decoded_to_its_path() {
    assert_eq!(
        uri_to_path("file:///home/me/notes.txt"),
        Ok(PathBuf::from("/home/me/notes.txt"))
    );
    assert_eq!(
        uri_to_path("file:///home/me/My%20Documents/a%20b.json"),
        Ok(PathBuf::from("/home/me/My Documents/a b.json"))
    );
    assert_eq!(
        uri_to_path("FILE://localhost/tmp/x"),
        Ok(PathBuf::from("/tmp/x"))
    );
}

#[test]
fn unicode_is_decoded_escaped_or_not() {
    assert_eq!(
        uri_to_path("file:///home/me/%C3%A9t%C3%A9/%E6%97%A5.txt"),
        Ok(PathBuf::from("/home/me/été/日.txt"))
    );
    assert_eq!(
        uri_to_path("file:///home/me/été"),
        Ok(PathBuf::from("/home/me/été"))
    );
}

#[test]
fn bytes_that_are_not_utf8_are_kept() {
    let path = uri_to_path("file:///tmp/%FF").expect("a local file");
    assert_eq!(path.as_os_str().as_bytes(), b"/tmp/\xFF");
}

#[test]
fn an_escaped_slash_or_nul_is_refused() {
    for uri in [
        "file:///home/a%2Fb",
        "file:///home/a%2fb",
        "file:///home/a%00b",
    ] {
        assert_eq!(uri_to_path(uri), Err(Unavailable::NotLocal(uri.to_owned())));
    }
}

#[test]
fn a_broken_escape_is_refused() {
    for uri in ["file:///home/a%", "file:///home/a%2", "file:///home/a%zz"] {
        assert_eq!(uri_to_path(uri), Err(Unavailable::NotLocal(uri.to_owned())));
    }
}

#[test]
fn a_uri_that_is_not_a_local_file_is_refused() {
    for uri in [
        "https://example.com/file.txt",
        "smb://server/share/file.txt",
        "file://server/share/file.txt",
        "file:relative",
        "/home/me/plain-path",
        "file:///home/me/a?query",
        "file:///home/me/a#fragment",
    ] {
        assert_eq!(uri_to_path(uri), Err(Unavailable::NotLocal(uri.to_owned())));
    }
}

#[test]
fn filters_become_globs_in_order() {
    let filters = [
        Filter {
            name: "Sessions".to_owned(),
            extensions: vec!["json".to_owned(), "xml".to_owned()],
        },
        Filter {
            name: "Nothing".to_owned(),
            extensions: Vec::new(),
        },
        Filter {
            name: "All".to_owned(),
            extensions: vec![EVERY_FILE.to_owned()],
        },
        Filter {
            name: "Empty".to_owned(),
            extensions: vec![String::new()],
        },
    ];
    assert_eq!(
        encode_filters(&filters),
        vec![
            (
                "Sessions".to_owned(),
                vec![
                    (FILTER_GLOB, "*.json".to_owned()),
                    (FILTER_GLOB, "*.xml".to_owned())
                ]
            ),
            ("All".to_owned(), vec![(FILTER_GLOB, "*".to_owned())]),
            ("Empty".to_owned(), vec![(FILTER_GLOB, "*".to_owned())]),
        ]
    );
}

#[test]
fn filters_are_sent_as_the_portal_signature() {
    let filters = encode_filters(&[Filter {
        name: "Keys".to_owned(),
        extensions: vec!["ppk".to_owned()],
    }]);
    assert_eq!(
        Value::from(filters).value_signature().to_string(),
        "a(sa(us))"
    );
}

#[test]
fn a_folder_is_its_bytes_then_a_nul() {
    assert_eq!(
        encode_path(Path::new("/home/me/.ssh")),
        b"/home/me/.ssh\0".to_vec()
    );
    assert_eq!(
        encode_path(Path::new("/home/me/été")),
        b"/home/me/\xC3\xA9t\xC3\xA9\0".to_vec()
    );
    assert_eq!(
        Value::from(encode_path(Path::new("/tmp")))
            .value_signature()
            .to_string(),
        "ay"
    );
}

#[test]
fn the_request_path_follows_the_unique_name_and_token() {
    assert_eq!(
        request_path(":1.42", "heimdall_7_0")
            .expect("a path")
            .as_str(),
        "/org/freedesktop/portal/desktop/request/1_42/heimdall_7_0"
    );
    assert!(request_path(":1.42", "not a token").is_err());
}

#[test]
fn every_token_is_new_and_a_path_element() {
    let (first, second) = (handle_token(), handle_token());
    assert_ne!(first, second);
    for token in [first, second] {
        assert!(
            token
                .chars()
                .all(|character| character.is_ascii_alphanumeric() || character == '_')
        );
    }
}

#[test]
fn an_x11_window_is_named_in_hexadecimal() {
    assert_eq!(x11_parent(0x0420_0007), "x11:4200007");
}

#[test]
fn a_response_is_read_by_its_code() {
    assert_eq!(
        parse_response(RESPONSE_SUCCESS, &results(&["file:///a", "file:///b%20c"])),
        Ok(Some(vec![PathBuf::from("/a"), PathBuf::from("/b c")]))
    );
    assert_eq!(parse_response(RESPONSE_SUCCESS, &results(&[])), Ok(None));
    assert_eq!(
        parse_response(RESPONSE_CANCELLED, &results(&["file:///a"])),
        Ok(None)
    );
    assert_eq!(
        parse_response(RESPONSE_CANCELLED, &HashMap::new()),
        Ok(None)
    );
    assert_eq!(
        parse_response(2, &HashMap::new()),
        Err(Unavailable::Ended(2))
    );
    assert_eq!(
        parse_response(7, &results(&["file:///a"])),
        Err(Unavailable::Ended(7))
    );
}

#[test]
fn a_success_without_uris_names_no_file() {
    assert_eq!(
        parse_response(RESPONSE_SUCCESS, &HashMap::new()),
        Err(Unavailable::NoLocation)
    );
    let wrong_type = HashMap::from([(RESULT_URIS.to_owned(), OwnedValue::from(3_u32))]);
    assert_eq!(
        parse_response(RESPONSE_SUCCESS, &wrong_type),
        Err(Unavailable::NoLocation)
    );
}

#[test]
fn one_place_that_is_not_local_refuses_the_whole_answer() {
    assert_eq!(
        parse_response(RESPONSE_SUCCESS, &results(&["file:///a", "sftp://host/b"])),
        Err(Unavailable::NotLocal("sftp://host/b".to_owned()))
    );
}

/// Longest an end-to-end test may take: each finishes in milliseconds, and one that hangs
/// fails here instead of holding the test run.
const E2E_LIMIT: Duration = Duration::from_secs(5);

/// The unique name the fake portal answers as, when it says one.
const FAKE_PORTAL_NAME: &str = ":1.7";

/// `work`, failing the test when it takes longer than [`E2E_LIMIT`].
async fn bounded<T>(work: impl Future<Output = T>) -> T {
    tokio::time::timeout(E2E_LIMIT, work)
        .await
        .expect("the request finished in time")
}

/// What the fake portal does with the request it takes.
#[derive(Clone, Copy)]
enum Answer {
    /// Answers on the request's path with this code and these URIs, after a decoy answer
    /// on another path, which must be ignored. Peer to peer, without a sender.
    Respond(u32, &'static [&'static str]),
    /// Refuses the call with this D-Bus error.
    Refuse(&'static str),
    /// Takes the request, then closes the connection without answering it.
    CloseAfterTaking,
    /// Closes the connection without even replying to the call.
    CloseBeforeReplying,
    /// Takes the request as [`FAKE_PORTAL_NAME`], as on a bus, says whether that name still
    /// has an owner when asked, then answers first or not and leaves the bus. A forged
    /// departure, not sent by the bus, comes before anything and must be ignored.
    Leave {
        /// Whether the name still has an owner when asked.
        owner: bool,
        /// Whether the dialog answered before the portal left.
        answer_first: bool,
    },
}

/// What the fake portal was asked.
#[derive(Debug)]
struct Asked {
    method: String,
    parent_window: String,
    title: String,
    options: HashMap<String, OwnedValue>,
}

impl Asked {
    fn option<T: TryFrom<OwnedValue>>(&self, key: &str) -> Option<T> {
        let value = self.options.get(key)?.try_clone().ok()?;
        T::try_from(value).ok()
    }
}

/// A client and a server connected peer to peer over a socket pair: no bus daemon. The
/// server's messages are listened to from here: a connection drops what arrives while no
/// one listens, so a call sent before the fake portal listens would never be answered.
async fn peers() -> (Connection, Connection, MessageStream) {
    let (client, server) = UnixStream::pair().expect("a socket pair");
    let guid = Guid::generate();
    let server = Builder::async_io_unix_stream(server)
        .server(guid)
        .expect("a server GUID")
        .p2p()
        .build();
    let client = Builder::async_io_unix_stream(client).p2p().build();
    let (client, server) = tokio::join!(client, server);
    let server = server.expect("the server accepts");
    let messages = MessageStream::from(&server);
    (client.expect("the client connects"), server, messages)
}

/// A signal on `path` of `interface`, `member` and `body`, sent as `sender`.
async fn emit_as<B>(
    server: &Connection,
    sender: &str,
    path: &str,
    (interface, member): (&str, &str),
    body: &B,
) where
    B: zbus::export::serde::Serialize + zbus::zvariant::DynamicType,
{
    let signal = zbus::Message::signal(path, interface, member)
        .expect("a signal")
        .sender(sender)
        .expect("a sender")
        .build(body)
        .expect("a body");
    server.send(&signal).await.expect("the signal sent");
}

/// The answer of `code` with `uris` to the request at `handle`, sent as `sender` or as no
/// one.
async fn respond(
    server: &Connection,
    sender: Option<&str>,
    handle: &OwnedObjectPath,
    code: u32,
    uris: &[&str],
) {
    let body = (code, results(uris));
    match sender {
        Some(sender) => {
            emit_as(
                server,
                sender,
                handle.as_str(),
                (REQUEST_INTERFACE, RESPONSE_MEMBER),
                &body,
            )
            .await;
        }
        None => server
            .emit_signal(
                None::<&str>,
                handle,
                REQUEST_INTERFACE,
                RESPONSE_MEMBER,
                &body,
            )
            .await
            .expect("the answer sent"),
    }
}

/// The bus saying `name` left it, as `sender`.
async fn departure(server: &Connection, sender: &str, name: &str) {
    emit_as(
        server,
        sender,
        BUS_PATH,
        (BUS_INTERFACE, NAME_OWNER_CHANGED),
        &(name, name, ""),
    )
    .await;
}

/// The fake portal: takes one call to the `FileChooser` among `messages` of `server`,
/// answers it as `answer` says, and tells what it was asked.
async fn fake_portal(server: Connection, mut messages: MessageStream, answer: Answer) -> Asked {
    let mut asked = None;
    let mut handle = None;
    while let Some(message) = messages.next().await {
        let message = message.expect("a message");
        let header = message.header();
        if message.message_type() != Type::MethodCall {
            continue;
        }
        let interface = header.interface().map(zbus::names::InterfaceName::as_str);
        if interface == Some(BUS_INTERFACE) {
            // Only a portal that leaves is asked whether it is still there.
            let Answer::Leave {
                owner,
                answer_first,
            } = answer
            else {
                panic!("asked about the bus without a bus");
            };
            server.reply(&header, &owner).await.expect("the owner sent");
            let handle: &OwnedObjectPath = handle.as_ref().expect("the request taken");
            if answer_first {
                respond(
                    &server,
                    Some(FAKE_PORTAL_NAME),
                    handle,
                    RESPONSE_SUCCESS,
                    &["file:///kept"],
                )
                .await;
            }
            departure(&server, BUS_NAME, FAKE_PORTAL_NAME).await;
            return asked.expect("the request taken");
        }
        if interface != Some(FILE_CHOOSER_INTERFACE) {
            continue;
        }
        let method = header.member().expect("a method").to_string();
        let (parent_window, title, options): (String, String, HashMap<String, OwnedValue>) =
            message
                .body()
                .deserialize()
                .expect("the FileChooser arguments");
        let call = Asked {
            method,
            parent_window,
            title,
            options,
        };
        let token: String = call.option(OPTION_HANDLE_TOKEN).expect("a handle token");
        let path = request_path(FAKE_PORTAL_NAME, &token).expect("a request path");
        match answer {
            Answer::Refuse(error) => {
                server
                    .reply_error(&header, error, &("no such interface",))
                    .await
                    .expect("the error sent");
                return call;
            }
            Answer::CloseBeforeReplying => {
                server.close().await.expect("closed");
                return call;
            }
            Answer::CloseAfterTaking => {
                server.reply(&header, &path).await.expect("the handle sent");
                server.close().await.expect("closed");
                return call;
            }
            Answer::Respond(code, uris) => {
                server.reply(&header, &path).await.expect("the handle sent");
                let decoy = OwnedObjectPath::try_from(format!("{REQUEST_PATH_PREFIX}/1_7/other"))
                    .expect("a path");
                respond(&server, None, &decoy, RESPONSE_SUCCESS, &["file:///decoy"]).await;
                respond(&server, None, &path, code, uris).await;
                return call;
            }
            Answer::Leave { .. } => {
                let reply = zbus::Message::method_return(&header)
                    .expect("a reply")
                    .sender(FAKE_PORTAL_NAME)
                    .expect("a sender")
                    .build(&path)
                    .expect("a body");
                server.send(&reply).await.expect("the handle sent");
                // Forged: said by the portal, not by the bus.
                departure(&server, FAKE_PORTAL_NAME, FAKE_PORTAL_NAME).await;
                asked = Some(call);
                handle = Some(path);
            }
        }
    }
    panic!("the client went away before calling");
}

/// `request` shown as `mode` by a fake portal answering as `answer`: the outcome, and what
/// the portal was asked; both within [`E2E_LIMIT`].
async fn run(
    request: Request,
    mode: Mode,
    answer: Answer,
) -> (Result<Option<Vec<PathBuf>>, Unavailable>, Asked) {
    bounded(async {
        let (client, server, messages) = peers().await;
        let portal = tokio::spawn(fake_portal(server, messages, answer));
        let outcome = choose_on(&client, &request, mode).await;
        let asked = portal.await.expect("the fake portal ran");
        (outcome, asked)
    })
    .await
}

fn open_request() -> Request {
    Request {
        title: "Import".to_owned(),
        parent_window: x11_parent(0x2a),
        filters: vec![Filter {
            name: "JSON".to_owned(),
            extensions: vec!["json".to_owned()],
        }],
        current_folder: Some(PathBuf::from("/home/me/.ssh")),
        current_name: Some("ignored-by-open".to_owned()),
    }
}

#[tokio::test]
async fn open_file_sends_its_options_and_reads_the_answer() {
    let (outcome, asked) = run(
        open_request(),
        Mode::OpenFile,
        Answer::Respond(RESPONSE_SUCCESS, &["file:///home/me/My%20File.json"]),
    )
    .await;
    assert_eq!(
        outcome,
        Ok(Some(vec![PathBuf::from("/home/me/My File.json")]))
    );
    assert_eq!(asked.method, OPEN_FILE_METHOD);
    assert_eq!(asked.title, "Import");
    assert_eq!(asked.parent_window, "x11:2a");
    assert_eq!(asked.option::<bool>(OPTION_MULTIPLE), Some(false));
    assert_eq!(asked.option::<bool>(OPTION_DIRECTORY), None);
    assert_eq!(asked.option::<String>(OPTION_CURRENT_NAME), None);
    assert_eq!(
        asked.option::<Vec<u8>>(OPTION_CURRENT_FOLDER),
        Some(b"/home/me/.ssh\0".to_vec())
    );
    assert_eq!(
        asked.option::<Vec<(String, Vec<(u32, String)>)>>(OPTION_FILTERS),
        Some(vec![(
            "JSON".to_owned(),
            vec![(FILTER_GLOB, "*.json".to_owned())]
        )])
    );
}

#[tokio::test]
async fn open_files_and_folder_ask_for_what_they_pick() {
    let (outcome, asked) = run(
        Request::default(),
        Mode::OpenFiles,
        Answer::Respond(RESPONSE_SUCCESS, &["file:///a.rdp", "file:///b.rdp"]),
    )
    .await;
    assert_eq!(
        outcome,
        Ok(Some(vec![PathBuf::from("/a.rdp"), PathBuf::from("/b.rdp")]))
    );
    assert_eq!(asked.option::<bool>(OPTION_MULTIPLE), Some(true));
    assert_eq!(asked.option::<String>(OPTION_FILTERS), None);
    assert_eq!(asked.parent_window, "");

    let (outcome, asked) = run(
        Request::default(),
        Mode::OpenFolder,
        Answer::Respond(RESPONSE_SUCCESS, &["file:///home/me/logs"]),
    )
    .await;
    assert_eq!(outcome, Ok(Some(vec![PathBuf::from("/home/me/logs")])));
    assert_eq!(asked.method, OPEN_FILE_METHOD);
    assert_eq!(asked.option::<bool>(OPTION_DIRECTORY), Some(true));
}

#[tokio::test]
async fn save_file_offers_its_name() {
    let (outcome, asked) = run(
        open_request(),
        Mode::Save,
        Answer::Respond(RESPONSE_SUCCESS, &["file:///home/me/export.json"]),
    )
    .await;
    assert_eq!(
        outcome,
        Ok(Some(vec![PathBuf::from("/home/me/export.json")]))
    );
    assert_eq!(asked.method, SAVE_FILE_METHOD);
    assert_eq!(
        asked.option::<String>(OPTION_CURRENT_NAME),
        Some("ignored-by-open".to_owned())
    );
    assert_eq!(asked.option::<bool>(OPTION_MULTIPLE), None);
}

#[tokio::test]
async fn a_cancelled_dialog_picks_nothing() {
    let (outcome, _) = run(
        open_request(),
        Mode::Save,
        Answer::Respond(RESPONSE_CANCELLED, &[]),
    )
    .await;
    assert_eq!(outcome, Ok(None));
}

#[tokio::test]
async fn a_dialog_ended_otherwise_says_so() {
    let (outcome, _) = run(open_request(), Mode::OpenFile, Answer::Respond(2, &[])).await;
    assert_eq!(outcome, Err(Unavailable::Ended(2)));
}

#[tokio::test]
async fn a_remote_place_is_refused() {
    let (outcome, _) = run(
        open_request(),
        Mode::OpenFile,
        Answer::Respond(RESPONSE_SUCCESS, &["sftp://host/etc/passwd"]),
    )
    .await;
    assert_eq!(
        outcome,
        Err(Unavailable::NotLocal("sftp://host/etc/passwd".to_owned()))
    );
}

#[tokio::test]
async fn no_file_chooser_is_no_portal() {
    let (outcome, _) = run(
        open_request(),
        Mode::OpenFile,
        Answer::Refuse("org.freedesktop.DBus.Error.UnknownInterface"),
    )
    .await;
    assert!(
        matches!(outcome, Err(Unavailable::NoPortal(_))),
        "{outcome:?}"
    );
}

#[tokio::test]
async fn another_refusal_is_a_failure() {
    let (outcome, _) = run(
        open_request(),
        Mode::OpenFile,
        Answer::Refuse("org.freedesktop.DBus.Error.AccessDenied"),
    )
    .await;
    assert!(
        matches!(outcome, Err(Unavailable::Failed(_))),
        "{outcome:?}"
    );
}

#[tokio::test]
async fn a_portal_gone_before_answering_is_closed() {
    let (outcome, _) = run(open_request(), Mode::OpenFile, Answer::CloseAfterTaking).await;
    assert_eq!(outcome, Err(Unavailable::Closed));
}

#[tokio::test]
async fn a_portal_gone_before_replying_fails_the_call() {
    let (outcome, _) = run(open_request(), Mode::OpenFile, Answer::CloseBeforeReplying).await;
    assert!(
        matches!(outcome, Err(Unavailable::Failed(_))),
        "{outcome:?}"
    );
}

#[tokio::test]
async fn a_portal_leaving_the_bus_before_answering_is_closed() {
    let (outcome, _) = run(
        open_request(),
        Mode::OpenFile,
        Answer::Leave {
            owner: true,
            answer_first: false,
        },
    )
    .await;
    assert_eq!(outcome, Err(Unavailable::Closed));
}

#[tokio::test]
async fn a_portal_gone_before_it_is_watched_is_closed() {
    let (outcome, _) = run(
        open_request(),
        Mode::OpenFile,
        Answer::Leave {
            owner: false,
            answer_first: false,
        },
    )
    .await;
    assert_eq!(outcome, Err(Unavailable::Closed));
}

#[tokio::test]
async fn an_answer_sent_before_leaving_the_bus_is_read() {
    let (outcome, _) = run(
        open_request(),
        Mode::OpenFile,
        Answer::Leave {
            owner: true,
            answer_first: true,
        },
    )
    .await;
    assert_eq!(outcome, Ok(Some(vec![PathBuf::from("/kept")])));
}
