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

//! A delete against a server whose listing calls a link a folder: the delete removes the
//! link and never lists, let alone empties, what it points to.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use heimdall_sftp::protocol::{Attributes, NameEntry, Request, Response, SFTP_VERSION, StatusCode};
use heimdall_sftp::tree::remove_tree;
use heimdall_sftp::{ClientConfig, RemotePath, SftpClient};
use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _, DuplexStream};
use tokio_util::sync::CancellationToken;

const FOLDER: u32 = 0o040_755;
const LINK: u32 = 0o120_777;

fn attributes(mode: u32) -> Attributes {
    Attributes {
        permissions: Some(mode),
        ..Attributes::default()
    }
}

fn ok(id: u32) -> Response {
    Response::Status {
        id,
        code: StatusCode::Ok,
        message: Vec::new(),
    }
}

async fn read_request(server: &mut DuplexStream) -> Option<Request> {
    let mut length = [0; 4];
    server.read_exact(&mut length).await.ok()?;
    let mut body = vec![0; u32::from_be_bytes(length) as usize];
    server.read_exact(&mut body).await.ok()?;
    Some(Request::decode(&body).expect("well formed"))
}

/// Serves `/root` holding `trap`, listed as a folder but a link when asked directly.
/// Records every path the client lists or removes.
async fn serve(mut server: DuplexStream, seen: Arc<Mutex<Vec<String>>>) {
    let _init = read_request(&mut server).await;
    let version = Response::Version {
        version: SFTP_VERSION,
        extensions: Vec::new(),
    };
    server.write_all(&version.encode()).await.expect("version");
    let mut listed = false;
    while let Some(request) = read_request(&mut server).await {
        let note = |what: &str, path: &RemotePath| {
            seen.lock()
                .expect("seen")
                .push(format!("{what} {}", path.display()));
        };
        let response = match request {
            Request::Lstat { id, path } => {
                let mode = if path.as_bytes() == b"/root" {
                    FOLDER
                } else {
                    LINK
                };
                Response::Attrs {
                    id,
                    attributes: attributes(mode),
                }
            }
            Request::Opendir { id, path } => {
                note("list", &path);
                Response::Handle {
                    id,
                    handle: path.as_bytes().to_vec(),
                }
            }
            Request::Readdir { id, .. } if !listed => {
                listed = true;
                Response::Name {
                    id,
                    entries: vec![NameEntry {
                        filename: b"trap".to_vec(),
                        longname: Vec::new(),
                        // The lie: a folder in the listing.
                        attributes: attributes(FOLDER),
                    }],
                }
            }
            Request::Readdir { id, .. } => Response::Status {
                id,
                code: StatusCode::Eof,
                message: Vec::new(),
            },
            Request::Close { id, .. } => ok(id),
            Request::Remove { id, path } => {
                note("remove", &path);
                ok(id)
            }
            Request::Rmdir { id, path } => {
                note("rmdir", &path);
                ok(id)
            }
            other => panic!("unexpected {other:?}"),
        };
        server.write_all(&response.encode()).await.expect("sent");
    }
}

#[tokio::test]
async fn a_link_listed_as_a_folder_is_removed_as_a_link() {
    let (client_end, server_end) = tokio::io::duplex(1 << 16);
    let seen = Arc::new(Mutex::new(Vec::new()));
    tokio::spawn(serve(server_end, seen.clone()));
    let client = SftpClient::start(client_end, ClientConfig::default())
        .await
        .expect("started");
    let removed = tokio::time::timeout(
        Duration::from_secs(30),
        remove_tree(
            &client,
            &RemotePath::from("/root"),
            &CancellationToken::new(),
        ),
    )
    .await
    .expect("in time")
    .expect("removed");
    assert_eq!(removed, 2);
    assert_eq!(
        *seen.lock().expect("seen"),
        ["list /root", "remove /root/trap", "rmdir /root"],
        "the link's target was never listed"
    );
}
