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

//! A download against a server that answers reads out of order, returns short reads and
//! reports the end of the file while later reads are in flight: OpenSSH's server does none
//! of these, a slow or unusual server may.

use std::time::Duration;

use heimdall_sftp::protocol::{Attributes, Request, Response, SFTP_VERSION, StatusCode};
use heimdall_sftp::transfer::{TransferConfig, download};
use heimdall_sftp::{ClientConfig, RemotePath, SftpClient};
use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _, DuplexStream};
use tokio_util::sync::CancellationToken;

/// Size of the served file: not a multiple of any chunk.
const SIZE: usize = 700 * 1024 + 17;

/// Reads answered together, one batch last first, the next in order.
const BATCH: usize = 7;

/// How long the server waits for more requests before answering those it holds.
const QUIET: Duration = Duration::from_millis(20);

/// Bytes per read in this test: small, so the file takes many requests.
const CHUNK: u32 = 8 * 1024;

fn content() -> Vec<u8> {
    (0..SIZE)
        .map(|n| u8::try_from((n * 7) % 253).expect("below 253"))
        .collect()
}

async fn read_request(server: &mut DuplexStream) -> Option<Request> {
    let mut length = [0; 4];
    server.read_exact(&mut length).await.ok()?;
    let mut body = vec![0; u32::from_be_bytes(length) as usize];
    server.read_exact(&mut body).await.ok()?;
    Some(Request::decode(&body).expect("well formed"))
}

async fn send(server: &mut DuplexStream, response: &Response) {
    server.write_all(&response.encode()).await.expect("sent");
}

/// The answer to one read: short for every third chunk, the end past the file.
fn answer(file: &[u8], id: u32, offset: u64, length: u32) -> Response {
    let offset = usize::try_from(offset).expect("small");
    if offset >= file.len() {
        return Response::Status {
            id,
            code: StatusCode::Eof,
            message: Vec::new(),
        };
    }
    let mut take = (length as usize).min(file.len() - offset);
    if (offset / CHUNK as usize).is_multiple_of(3) && take > 3 {
        take /= 3;
    }
    Response::Data {
        id,
        data: file[offset..offset + take].to_vec(),
    }
}

/// Answers the held reads: last first when `reverse`, which alternates in
/// [`Order::Alternate`] and stays off in [`Order::Forward`].
async fn flush(
    server: &mut DuplexStream,
    held: &mut Vec<Response>,
    reverse: &mut bool,
    order: Order,
) {
    if *reverse {
        held.reverse();
    }
    if matches!(order, Order::Alternate) {
        *reverse = !*reverse;
    }
    for response in held.drain(..) {
        send(server, &response).await;
    }
}

/// How the fake server orders its answers.
#[derive(Clone, Copy)]
enum Order {
    /// One batch last first, the next in order.
    Alternate,
    /// Always in order: ends of file arrive smallest first.
    Forward,
}

async fn serve(server: DuplexStream, file: Vec<u8>) {
    serve_in(server, file, Order::Alternate).await;
}

async fn serve_in(mut server: DuplexStream, file: Vec<u8>, order: Order) {
    let init = read_request(&mut server).await.expect("init");
    assert!(matches!(init, Request::Init { .. }));
    send(
        &mut server,
        &Response::Version {
            version: SFTP_VERSION,
            extensions: Vec::new(),
        },
    )
    .await;
    let mut held: Vec<Response> = Vec::new();
    let mut reverse = matches!(order, Order::Alternate);
    loop {
        let next = tokio::time::timeout(QUIET, read_request(&mut server)).await;
        let request = match next {
            Ok(Some(request)) => request,
            Ok(None) => return,
            Err(_) => {
                // Nothing more is coming for now: answer what is held.
                flush(&mut server, &mut held, &mut reverse, order).await;
                continue;
            }
        };
        let response = match request {
            Request::Stat { id, .. } => Response::Attrs {
                id,
                attributes: Attributes {
                    size: Some(file.len() as u64),
                    permissions: Some(0o100_644),
                    times: Some((1_700_000_000, 1_700_000_000)),
                    ..Attributes::default()
                },
            },
            Request::Open { id, .. } => Response::Handle {
                id,
                handle: b"f".to_vec(),
            },
            Request::Close { id, .. } => Response::Status {
                id,
                code: StatusCode::Ok,
                message: Vec::new(),
            },
            Request::Read {
                id, offset, length, ..
            } => {
                held.push(answer(&file, id, offset, length));
                if held.len() == BATCH {
                    flush(&mut server, &mut held, &mut reverse, order).await;
                }
                continue;
            }
            other => panic!("unexpected {other:?}"),
        };
        send(&mut server, &response).await;
    }
}

#[tokio::test]
async fn reads_out_of_order_and_short_still_rebuild_the_file() {
    let file = content();
    let (client_end, server_end) = tokio::io::duplex(1 << 20);
    tokio::spawn(serve(server_end, file.clone()));
    let client = SftpClient::start(client_end, ClientConfig::default())
        .await
        .expect("started");
    let dir = tempfile::tempdir().expect("dir");
    let target = dir.path().join("rebuilt.bin");
    let config = TransferConfig {
        chunk: CHUNK,
        in_flight: 16,
        preserve_times: false,
    };
    let report = tokio::time::timeout(
        Duration::from_secs(60),
        download(
            &client,
            &RemotePath::from("/served"),
            &target,
            &config,
            &CancellationToken::new(),
            |_| {},
        ),
    )
    .await
    .expect("in time")
    .expect("downloaded");
    assert_eq!(report.bytes, SIZE as u64);
    let got = std::fs::read(&target).expect("written");
    assert!(got == file, "every byte at its place despite the disorder");
}

#[tokio::test]
async fn a_cancel_amid_disorder_keeps_only_a_whole_prefix() {
    let file = content();
    let (client_end, server_end) = tokio::io::duplex(1 << 20);
    tokio::spawn(serve(server_end, file.clone()));
    let client = SftpClient::start(client_end, ClientConfig::default())
        .await
        .expect("started");
    let dir = tempfile::tempdir().expect("dir");
    let target = dir.path().join("rebuilt.bin");
    let config = TransferConfig {
        chunk: CHUNK,
        in_flight: 16,
        preserve_times: false,
    };
    let cancel = CancellationToken::new();
    let trigger = cancel.clone();
    let stopped = tokio::time::timeout(
        Duration::from_secs(60),
        download(
            &client,
            &RemotePath::from("/served"),
            &target,
            &config,
            &cancel,
            move |mark| {
                if mark >= (SIZE / 3) as u64 {
                    trigger.cancel();
                }
            },
        ),
    )
    .await
    .expect("in time")
    .expect_err("cancelled");
    let heimdall_sftp::transfer::TransferError::Cancelled { kept } = stopped else {
        panic!("expected a cancel, got {stopped:?}");
    };
    let mut part_name = target.as_os_str().to_owned();
    part_name.push(heimdall_sftp::transfer::PART_SUFFIX);
    let part = std::fs::read(std::path::PathBuf::from(part_name)).expect("part kept");
    assert_eq!(
        part.len() as u64,
        kept,
        "bytes written past the mark by later reads are cut away"
    );
    assert!(part[..] == file[..part.len()], "a whole prefix");
}

#[tokio::test]
async fn ends_of_file_arriving_smallest_first_do_not_move_the_end() {
    // A size that is a whole number of chunks: no short read ends exactly at the end, so
    // several reads past it answer "end of file", in order, smallest first.
    let file: Vec<u8> = content()[..(64 * CHUNK) as usize].to_vec();
    let (client_end, server_end) = tokio::io::duplex(1 << 20);
    tokio::spawn(serve_in(server_end, file.clone(), Order::Forward));
    let client = SftpClient::start(client_end, ClientConfig::default())
        .await
        .expect("started");
    let dir = tempfile::tempdir().expect("dir");
    let target = dir.path().join("rebuilt.bin");
    let config = TransferConfig {
        chunk: CHUNK,
        in_flight: 16,
        preserve_times: false,
    };
    let report = tokio::time::timeout(
        Duration::from_secs(60),
        download(
            &client,
            &RemotePath::from("/served"),
            &target,
            &config,
            &CancellationToken::new(),
            |_| {},
        ),
    )
    .await
    .expect("in time")
    .expect("downloaded");
    assert_eq!(report.bytes, file.len() as u64);
    let got = std::fs::read(&target).expect("written");
    assert!(
        got.len() == file.len() && got == file,
        "every byte, no more"
    );
}
