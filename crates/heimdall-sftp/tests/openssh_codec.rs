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

//! The codec against the reference implementation: OpenSSH's `sftp-server`, spoken to on
//! its standard input and output, with no SSH around it.
//!
//! The server is looked up at `HEIMDALL_SFTP_SERVER`, then at the usual Linux paths; the
//! test says so and passes when there is none (Windows runners, minimal systems), unless
//! `HEIMDALL_REQUIRE_SFTP_SERVER` is set, as it is on the Linux CI job.

#![cfg(unix)]

use std::io::{Read, Write};
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::MetadataExt;
use std::path::PathBuf;
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};

use heimdall_sftp::RemotePath;
use heimdall_sftp::protocol::{
    Attributes, Request, Response, SFTP_VERSION, StatusCode, open_flags,
};

/// Environment variable naming the `sftp-server` binary.
const SERVER_VARIABLE: &str = "HEIMDALL_SFTP_SERVER";

/// Set in CI where the server is installed: its absence then fails instead of skipping.
const REQUIRE_VARIABLE: &str = "HEIMDALL_REQUIRE_SFTP_SERVER";

/// Where distributions install it.
const KNOWN_PATHS: [&str; 2] = [
    "/usr/libexec/openssh/sftp-server",
    "/usr/lib/openssh/sftp-server",
];

struct Server {
    child: Child,
    input: ChildStdin,
    output: ChildStdout,
}

impl Server {
    fn start() -> Option<Self> {
        let binary = std::env::var_os(SERVER_VARIABLE)
            .map(PathBuf::from)
            .or_else(|| {
                KNOWN_PATHS
                    .iter()
                    .map(PathBuf::from)
                    .find(|path| path.is_file())
            });
        let Some(binary) = binary else {
            assert!(
                std::env::var_os(REQUIRE_VARIABLE).is_none(),
                "{REQUIRE_VARIABLE} is set and no sftp-server was found"
            );
            eprintln!("no sftp-server found; set {SERVER_VARIABLE} to run this test");
            return None;
        };
        let mut child = Command::new(binary)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .expect("sftp-server starts");
        let input = child.stdin.take().expect("stdin");
        let output = child.stdout.take().expect("stdout");
        Some(Self {
            child,
            input,
            output,
        })
    }

    fn send(&mut self, request: &Request) -> Response {
        self.input.write_all(&request.encode()).expect("sent");
        let mut length = [0; 4];
        self.output.read_exact(&mut length).expect("length");
        let mut body = vec![0; u32::from_be_bytes(length) as usize];
        self.output.read_exact(&mut body).expect("body");
        Response::decode(&body).expect("a well-formed response")
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn status(response: &Response) -> StatusCode {
    match response {
        Response::Status { code, .. } => *code,
        other => panic!("expected a status, got {other:?}"),
    }
}

fn handle(response: Response) -> Vec<u8> {
    match response {
        Response::Handle { handle, .. } => handle,
        other => panic!("expected a handle, got {other:?}"),
    }
}

#[test]
fn a_latin1_name_is_created_listed_and_deleted_byte_for_byte() {
    let Some(mut server) = Server::start() else {
        return;
    };
    let version = server.send(&Request::Init {
        version: SFTP_VERSION,
        extensions: Vec::new(),
    });
    assert!(
        matches!(version, Response::Version { version: 3, .. }),
        "{version:?}"
    );

    let dir = tempfile::tempdir().expect("dir");
    let root = RemotePath::from_bytes(dir.path().as_os_str().as_bytes());
    let name = b"caf\xE9 d\xE9j\xE0.txt";
    let file = root.join(name);

    let opened = server.send(&Request::Open {
        id: 1,
        path: file.clone(),
        flags: open_flags::WRITE | open_flags::CREATE | open_flags::EXCLUSIVE,
        attributes: Attributes::default(),
    });
    let file_handle = handle(opened);
    let written = server.send(&Request::Write {
        id: 2,
        handle: file_handle.clone(),
        offset: 0,
        data: b"bonjour".to_vec(),
    });
    assert_eq!(status(&written), StatusCode::Ok);
    assert_eq!(
        status(&server.send(&Request::Close {
            id: 3,
            handle: file_handle,
        })),
        StatusCode::Ok
    );

    let listing = handle(server.send(&Request::Opendir {
        id: 4,
        path: root.clone(),
    }));
    let mut names = Vec::new();
    for id in 5.. {
        match server.send(&Request::Readdir {
            id,
            handle: listing.clone(),
        }) {
            Response::Name { entries, .. } => {
                names.extend(entries.into_iter().map(|entry| entry.filename));
            }
            eof => {
                assert_eq!(status(&eof), StatusCode::Eof);
                break;
            }
        }
    }
    assert!(names.iter().any(|listed| listed == name), "{names:?}");

    let attributes = match server.send(&Request::Stat {
        id: 100,
        path: file.clone(),
    }) {
        Response::Attrs { attributes, .. } => attributes,
        other => panic!("expected attributes, got {other:?}"),
    };
    // Every field against the file system: a field order wrong in both the encoder and the
    // decoder round-trips, but cannot read the reference server's bytes right.
    let local = std::fs::metadata(dir.path().join(std::ffi::OsStr::from_bytes(name)))
        .expect("the file exists locally");
    assert_eq!(attributes.size, Some(7));
    assert_eq!(attributes.uid_gid, Some((local.uid(), local.gid())));
    assert_eq!(attributes.permissions, Some(local.mode()));
    let mtime = u32::try_from(local.mtime()).expect("a recent time");
    assert_eq!(attributes.times.map(|(_, modified)| modified), Some(mtime));

    assert_eq!(
        status(&server.send(&Request::Remove {
            id: 101,
            path: file
        })),
        StatusCode::Ok
    );
    assert!(
        std::fs::read_dir(dir.path())
            .expect("readable")
            .next()
            .is_none(),
        "the file with the Latin-1 name is gone"
    );
}

#[test]
fn symlink_takes_the_target_first_as_openssh_reads_it() {
    let Some(mut server) = Server::start() else {
        return;
    };
    server.send(&Request::Init {
        version: SFTP_VERSION,
        extensions: Vec::new(),
    });
    let dir = tempfile::tempdir().expect("dir");
    let link = RemotePath::from_bytes(dir.path().join("link").as_os_str().as_bytes());
    let created = server.send(&Request::Symlink {
        id: 1,
        target: RemotePath::from("/heimdall/target"),
        link: link.clone(),
    });
    assert_eq!(status(&created), StatusCode::Ok);
    assert_eq!(
        std::fs::read_link(dir.path().join("link")).expect("a link"),
        PathBuf::from("/heimdall/target")
    );
}

#[test]
fn a_missing_file_answers_no_such_file() {
    let Some(mut server) = Server::start() else {
        return;
    };
    server.send(&Request::Init {
        version: SFTP_VERSION,
        extensions: Vec::new(),
    });
    let response = server.send(&Request::Stat {
        id: 1,
        path: RemotePath::from("/nonexistent/heimdall"),
    });
    assert_eq!(status(&response), StatusCode::NoSuchFile);
}
