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

//! The share server over real sockets on the loopback.

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

use tokio::net::TcpStream;
use tokio::sync::mpsc::{UnboundedReceiver, unbounded_channel};
use tokio::task::JoinHandle;

use super::request::{MAX_HEAD_BYTES, MAX_HEADERS, MAX_REQUEST_LINE_BYTES};
use super::*;

const LONG_LIFE: Duration = Duration::from_mins(1);

struct Running {
    addr: SocketAddr,
    token: String,
    cancel: CancellationToken,
    events: UnboundedReceiver<ShareEvent>,
    task: JoinHandle<StopReason>,
    _dir: tempfile::TempDir,
    root: PathBuf,
}

struct Response {
    status: u16,
    head: String,
    body: Vec<u8>,
}

impl Response {
    fn text(&self) -> String {
        String::from_utf8_lossy(&self.body).into_owned()
    }
}

fn labels() -> ListingLabels {
    ListingLabels {
        index_of: "Index of".to_owned(),
        name: "Name".to_owned(),
        size: "Size".to_owned(),
    }
}

fn config(root: PathBuf, lifetime: Duration) -> ShareConfig {
    ShareConfig::new(root, BindChoice::default(), 0, lifetime, labels())
}

async fn start_with(lifetime: Duration, head_timeout: Duration) -> Running {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().to_path_buf();
    std::fs::write(root.join("hello.txt"), "hello world").unwrap();
    std::fs::create_dir(root.join("sub")).unwrap();
    std::fs::write(root.join("sub").join("deep.txt"), "deep").unwrap();
    std::fs::write(root.join("a&b'c.txt"), "x").unwrap();
    let mut config = config(root.clone(), lifetime);
    config.head_timeout = head_timeout;
    let share = Share::bind(config).await.unwrap();
    let (sender, events) = unbounded_channel();
    let cancel = CancellationToken::new();
    let addr = share.local_addr();
    let token = share.token().to_owned();
    let task = tokio::spawn(share.run(cancel.clone(), sender));
    Running {
        addr,
        token,
        cancel,
        events,
        task,
        _dir: dir,
        root,
    }
}

async fn start() -> Running {
    start_with(LONG_LIFE, HEAD_TIMEOUT).await
}

async fn send(addr: SocketAddr, raw: &[u8]) -> Response {
    let mut stream = TcpStream::connect(addr).await.unwrap();
    stream.write_all(raw).await.unwrap();
    let mut bytes = Vec::new();
    let _ = stream.read_to_end(&mut bytes).await;
    let split = bytes
        .windows(4)
        .position(|w| w == b"\r\n\r\n")
        .expect("a head");
    let head = String::from_utf8_lossy(&bytes[..split]).into_owned();
    let status = head
        .split(' ')
        .nth(1)
        .and_then(|code| code.parse().ok())
        .expect("a status");
    Response {
        status,
        head,
        body: bytes[split + 4..].to_vec(),
    }
}

fn get(target: &str, token: Option<&str>) -> Vec<u8> {
    let auth = token.map_or_else(String::new, |t| format!("Authorization: Bearer {t}\r\n"));
    format!("GET {target} HTTP/1.1\r\nHost: x\r\n{auth}\r\n").into_bytes()
}

#[test]
fn token_comparison_accepts_only_the_exact_token() {
    assert!(token_matches("abc123", Some("abc123")));
    assert!(!token_matches("abc123", Some("abc124")));
    assert!(!token_matches("abc123", Some("abc12")));
    assert!(!token_matches("abc123", Some("abc1234")));
    assert!(!token_matches("abc123", Some("")));
    assert!(!token_matches("abc123", None));
}

#[test]
fn the_bearer_scheme_is_case_insensitive_and_the_query_is_the_fallback() {
    let request = |auth: Option<&str>| Request {
        method: Method::Get,
        target: "/".to_owned(),
        authorization: auth.map(str::to_owned),
    };
    assert_eq!(
        presented_token(&request(Some("bearer t1")), None).as_deref(),
        Some("t1")
    );
    assert_eq!(
        presented_token(&request(Some("Basic t1")), Some("token=t2")).as_deref(),
        Some("t2")
    );
    assert_eq!(
        presented_token(&request(None), Some("token=t3")).as_deref(),
        Some("t3")
    );
    assert_eq!(presented_token(&request(None), None), None);
}

#[test]
fn a_share_listens_on_the_loopback_unless_an_address_is_chosen() {
    assert_eq!(BindChoice::default(), BindChoice::Loopback);
    assert_eq!(
        BindChoice::default().address().unwrap(),
        IpAddr::V4(Ipv4Addr::LOCALHOST)
    );
    let lan = IpAddr::V4(Ipv4Addr::new(192, 168, 1, 10));
    assert_eq!(BindChoice::Interface(lan).address().unwrap(), lan);
}

#[test]
fn every_interface_is_refused() {
    for address in [
        IpAddr::V4(Ipv4Addr::UNSPECIFIED),
        IpAddr::V6(Ipv6Addr::UNSPECIFIED),
        IpAddr::V4(Ipv4Addr::new(224, 0, 0, 1)),
    ] {
        assert!(matches!(
            BindChoice::Interface(address).address(),
            Err(ShareError::EveryInterface)
        ));
    }
}

#[tokio::test]
async fn a_default_share_is_bound_to_the_loopback_with_its_own_token() {
    let first = start().await;
    let second = start().await;
    assert!(first.addr.ip().is_loopback());
    assert_eq!(first.token.len(), TOKEN_BYTES * 2);
    assert!(first.token.bytes().all(|b| b.is_ascii_hexdigit()));
    assert_ne!(first.token, second.token);
    let refused = Share::bind(ShareConfig::new(
        first.root.clone(),
        BindChoice::Interface(IpAddr::V4(Ipv4Addr::UNSPECIFIED)),
        0,
        LONG_LIFE,
        labels(),
    ))
    .await;
    assert!(matches!(refused, Err(ShareError::EveryInterface)));
}

#[tokio::test]
async fn a_file_is_not_a_folder_to_share() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("f");
    std::fs::write(&file, "x").unwrap();
    let result = Share::bind(config(file, LONG_LIFE)).await;
    assert!(matches!(result, Err(ShareError::NotAFolder)));
    let missing = Share::bind(config(dir.path().join("none"), LONG_LIFE)).await;
    assert!(matches!(missing, Err(ShareError::Root(_))));
}

#[tokio::test]
async fn a_wrong_or_missing_token_is_refused_with_401() {
    let share = start().await;
    for request in [
        get("/hello.txt", None),
        get("/hello.txt", Some("wrong")),
        get("/hello.txt", Some("")),
        get("/hello.txt?token=wrong", None),
        get("/hello.txt?tokens=x", None),
    ] {
        let response = send(share.addr, &request).await;
        assert_eq!(response.status, 401);
        assert!(response.head.contains("WWW-Authenticate: Bearer"));
        assert!(!response.text().contains("hello"));
    }
}

#[tokio::test]
async fn the_token_is_accepted_as_a_bearer_or_as_a_query() {
    let share = start().await;
    let bearer = send(share.addr, &get("/hello.txt", Some(&share.token))).await;
    assert_eq!(
        (bearer.status, bearer.text()),
        (200, "hello world".to_owned())
    );
    let query = send(
        share.addr,
        &get(&format!("/hello.txt?token={}", share.token), None),
    )
    .await;
    assert_eq!(
        (query.status, query.text()),
        (200, "hello world".to_owned())
    );
}

#[tokio::test]
async fn get_and_head_are_served_and_everything_else_is_405() {
    let share = start().await;
    let token = &share.token;
    let got = send(share.addr, &get("/hello.txt", Some(token))).await;
    assert_eq!(got.status, 200);
    assert!(got.head.contains("Content-Length: 11"));
    assert!(got.head.contains("attachment"));
    assert!(got.head.contains("X-Content-Type-Options: nosniff"));
    assert!(got.head.contains("Referrer-Policy: no-referrer"));

    let head = send(
        share.addr,
        format!("HEAD /hello.txt HTTP/1.1\r\nAuthorization: Bearer {token}\r\n\r\n").as_bytes(),
    )
    .await;
    assert_eq!(head.status, 200);
    assert!(head.head.contains("Content-Length: 11"));
    assert!(head.body.is_empty());

    for method in ["POST", "PUT", "DELETE", "OPTIONS", "PATCH", "TRACE"] {
        let response = send(
            share.addr,
            format!("{method} /hello.txt HTTP/1.1\r\nAuthorization: Bearer {token}\r\n\r\n")
                .as_bytes(),
        )
        .await;
        assert_eq!(response.status, 405, "{method}");
        assert!(response.head.contains("Allow: GET, HEAD"));
    }
    // Who asks comes before what is asked.
    let anonymous = send(share.addr, b"POST /hello.txt HTTP/1.1\r\n\r\n").await;
    assert_eq!(anonymous.status, 401);
}

#[tokio::test]
async fn a_missing_file_is_404() {
    let share = start().await;
    let response = send(share.addr, &get("/none.txt", Some(&share.token))).await;
    assert_eq!(response.status, 404);
}

#[tokio::test]
async fn paths_that_leave_the_folder_are_refused() {
    let share = start().await;
    let token = &share.token;
    for target in [
        "/..",
        "/../secret",
        "/sub/../hello.txt",
        "/%2e%2e/secret",
        "/%2E%2E%2Fsecret",
        "/sub/%2e%2e/%2e%2e/secret",
        "/hello.txt:stream",
        "/hello.txt%3A%24DATA",
        "/C:/Windows/win.ini",
        "//host/share/file",
        "/%5C%5Chost%5Cshare",
        "/sub%5C..%5Chello.txt",
        "/CON",
        "/a%00b",
    ] {
        let response = send(share.addr, &get(target, Some(token))).await;
        assert_eq!(response.status, 403, "{target}");
        assert!(!response.text().contains("hello world"), "{target}");
    }
    let absolute = send(
        share.addr,
        &get("http://example.com/hello.txt", Some(token)),
    )
    .await;
    assert_eq!(absolute.status, 400);
    let bad_escape = send(share.addr, &get("/%zz", Some(token))).await;
    assert_eq!(bad_escape.status, 400);
}

#[cfg(unix)]
#[tokio::test]
async fn a_symlink_is_refused_and_not_listed() {
    let share = start().await;
    let outside = tempfile::tempdir().unwrap();
    std::fs::write(outside.path().join("secret"), "top secret").unwrap();
    std::os::unix::fs::symlink(outside.path(), share.root.join("out")).unwrap();
    std::os::unix::fs::symlink(share.root.join("hello.txt"), share.root.join("alias")).unwrap();
    let token = &share.token;
    for target in ["/out", "/out/secret", "/alias"] {
        let response = send(share.addr, &get(target, Some(token))).await;
        assert_eq!(response.status, 403, "{target}");
        assert!(!response.text().contains("secret"), "{target}");
    }
    let listing = send(share.addr, &get("/", Some(token))).await;
    assert!(!listing.text().contains("out"));
    assert!(!listing.text().contains("alias"));
}

#[tokio::test]
async fn a_listing_escapes_names_and_stops_at_the_root() {
    let share = start().await;
    let token = &share.token;
    let root = send(share.addr, &get("/", Some(token))).await;
    assert_eq!(root.status, 200);
    assert!(root.head.contains("text/html"));
    let page = root.text();
    assert!(page.contains("a&amp;b&#39;c.txt"), "{page}");
    assert!(page.contains("sub/"));
    assert!(!page.contains(">..<"), "no link above the root");
    assert!(page.contains(&format!("?token={token}")));
    let sub = send(share.addr, &get("/sub/", Some(token))).await;
    assert!(sub.text().contains("deep.txt"));
    assert!(sub.text().contains("<a href=\"/?token="));
}

#[tokio::test]
async fn requests_over_the_limits_are_refused() {
    let share = start().await;
    let token = &share.token;
    let long = format!("/{}", "a".repeat(MAX_REQUEST_LINE_BYTES));
    assert_eq!(send(share.addr, &get(&long, Some(token))).await.status, 414);

    let many = "X-n: v\r\n".repeat(MAX_HEADERS + 1);
    let crowded = format!("GET / HTTP/1.1\r\n{many}\r\n");
    assert_eq!(send(share.addr, crowded.as_bytes()).await.status, 431);

    let big = format!(
        "GET / HTTP/1.1\r\nX: {}\r\n\r\n",
        "v".repeat(MAX_HEAD_BYTES)
    );
    assert_eq!(send(share.addr, big.as_bytes()).await.status, 431);

    let body =
        format!("GET / HTTP/1.1\r\nAuthorization: Bearer {token}\r\nContent-Length: 4\r\n\r\nabcd");
    assert_eq!(send(share.addr, body.as_bytes()).await.status, 400);
    assert_eq!(
        send(share.addr, b"GET / HTTP/2.0\r\n\r\n").await.status,
        505
    );
    assert_eq!(send(share.addr, b"nonsense\r\n\r\n").await.status, 400);
}

#[tokio::test]
async fn a_client_too_slow_to_send_its_head_is_dropped() {
    let share = start_with(LONG_LIFE, Duration::from_millis(200)).await;
    let mut stream = TcpStream::connect(share.addr).await.unwrap();
    stream.write_all(b"GET / HT").await.unwrap();
    let mut bytes = Vec::new();
    let _ = stream.read_to_end(&mut bytes).await;
    assert!(String::from_utf8_lossy(&bytes).starts_with("HTTP/1.1 408"));
}

#[tokio::test]
async fn requests_are_reported_without_the_token() {
    let mut share = start().await;
    let target = format!("/hello.txt?token={}", share.token);
    let _ = send(share.addr, &get(&target, None)).await;
    let _ = send(share.addr, &get("/hello.txt?token=wrong", None)).await;
    let mut seen = Vec::new();
    while seen.len() < 2 {
        match share.events.recv().await {
            Some(ShareEvent::Request(record)) => seen.push(record),
            other => panic!("unexpected {other:?}"),
        }
    }
    assert_eq!(seen[0].path, "/hello.txt");
    assert_eq!((seen[0].status, seen[1].status), (200, 401));
    assert_eq!(seen[0].method, "GET");
    assert!(seen[0].client.is_loopback());
    assert!(
        seen.iter()
            .all(|r| !r.path.contains("token") && !r.path.contains(&share.token))
    );
}

#[tokio::test]
async fn a_share_stops_when_told_to() {
    let mut share = start().await;
    share.cancel.cancel();
    assert_eq!(share.task.await.unwrap(), StopReason::Stopped);
    assert_eq!(
        share.events.recv().await,
        Some(ShareEvent::Stopped(StopReason::Stopped))
    );
    assert!(TcpStream::connect(share.addr).await.is_err());
}

#[tokio::test]
async fn a_share_stops_by_itself_when_its_lifetime_is_over() {
    let mut share = start_with(Duration::from_millis(250), HEAD_TIMEOUT).await;
    assert_eq!(share.task.await.unwrap(), StopReason::Expired);
    assert_eq!(
        share.events.recv().await,
        Some(ShareEvent::Stopped(StopReason::Expired))
    );
    assert!(TcpStream::connect(share.addr).await.is_err());
}
