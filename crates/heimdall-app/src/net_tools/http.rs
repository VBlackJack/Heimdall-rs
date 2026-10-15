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

//! The request of the HTTPHEADERS tool, as the C# `HttpHeaderService.FetchDirectAsync`:
//! Heimdall's own HTTP/1.1 client over TCP, and TLS for HTTPS, no program run, no proxy nor
//! configuration file read. A `HEAD`, then a `GET` when the server refuses it with 405; at
//! most [`MAX_RESPONSE_BYTES`] read, up to the end of the header section; the whole within
//! [`FETCH_TIMEOUT_MS`].
//!
//! The C# takes any certificate. Here the system's check runs, as for every other TLS
//! client of Heimdall (`heimdall_tls`): a certificate it refuses is a finding of the
//! analysis, said with why, and the headers are read all the same over a connection that
//! takes that certificate alone, the one just refused. No credential is ever sent.

use std::io;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use heimdall_core::tools::http_headers::{
    self, FETCH_TIMEOUT_MS, HttpResponseInfo, HttpTarget, MAX_RESPONSE_BYTES, METHOD_NOT_ALLOWED,
};
use heimdall_tls::{PresentedSlot, UserVerdict, ValidationIssue};
use tokio::io::{AsyncRead, AsyncReadExt as _, AsyncWrite, AsyncWriteExt as _};
use tokio::net::TcpStream;
use tokio_rustls::rustls::pki_types::ServerName;

use super::{ResolveFailure, resolve, shown_lines};
use crate::text::server_text;

/// Size of one read from the server.
const READ_CHUNK: usize = 4096;

/// What the server's certificate came to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CertificateCheck {
    /// Plain HTTP: no certificate.
    NotTls,
    /// The system trusts it for the server's name.
    Trusted,
    /// The system refused it, for this reason: a finding.
    Untrusted(ValidationIssue),
}

/// The response the analysis reads.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HttpFetch {
    /// Its status and headers, made safe to show.
    pub info: HttpResponseInfo,
    /// The server's certificate.
    pub certificate: CertificateCheck,
}

/// Why no response was read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HttpFetchError {
    /// Past [`FETCH_TIMEOUT_MS`], as the C# `ToolHttpHeadersErrorTimeout`.
    TimedOut,
    /// The host name did not resolve: why, made safe to show.
    Unresolved(String),
    /// No connection, or it failed: why, made safe to show.
    Connection(String),
    /// The TLS handshake failed for another reason than the certificate: why.
    Tls(String),
    /// The certificate the system refused was refused again, taken as it is: another one
    /// came in its place within the same analysis.
    Certificate(ValidationIssue),
    /// The server closed the connection without a byte.
    Empty,
}

/// The headers of `target`, as the module says.
///
/// # Errors
///
/// [`HttpFetchError`].
pub async fn fetch(target: &HttpTarget) -> Result<HttpFetch, HttpFetchError> {
    let timeout = Duration::from_millis(FETCH_TIMEOUT_MS);
    tokio::time::timeout(timeout, fetch_unbounded(target))
        .await
        .unwrap_or(Err(HttpFetchError::TimedOut))
}

async fn fetch_unbounded(target: &HttpTarget) -> Result<HttpFetch, HttpFetchError> {
    let addresses: Vec<SocketAddr> = resolve(&target.host, Duration::from_millis(FETCH_TIMEOUT_MS))
        .await
        .map_err(|failure| {
            HttpFetchError::Unresolved(match failure {
                ResolveFailure::Failed(why) => why,
                ResolveFailure::TimedOut | ResolveFailure::NoAddress => String::new(),
            })
        })?
        .into_iter()
        .map(|address| SocketAddr::new(address, target.port))
        .collect();
    let mut session = Session {
        target,
        addresses: &addresses,
        accepted: None,
        certificate: if target.https {
            CertificateCheck::Trusted
        } else {
            CertificateCheck::NotTls
        },
    };
    let head = session.send("HEAD").await?;
    let info = if head.status_code == METHOD_NOT_ALLOWED {
        session.send("GET").await?
    } else {
        head
    };
    Ok(HttpFetch {
        info,
        certificate: session.certificate,
    })
}

/// The requests of one analysis, and what its first TLS handshake learnt.
struct Session<'a> {
    target: &'a HttpTarget,
    addresses: &'a [SocketAddr],
    /// The certificate the system refused, taken as it is from then on.
    accepted: Option<Vec<u8>>,
    certificate: CertificateCheck,
}

impl Session<'_> {
    /// One `method` request, on a connection of its own as `Connection: close` asks.
    async fn send(&mut self, method: &str) -> Result<HttpResponseInfo, HttpFetchError> {
        let request = self.target.request(method);
        let stream = self.connect().await?;
        if !self.target.https {
            return exchange(stream, &request).await;
        }
        match self.handshake(stream).await {
            Ok(tls) => exchange(tls, &request).await,
            Err(Refusal::Certificate(der, issue)) if self.accepted.is_none() => {
                self.certificate = CertificateCheck::Untrusted(issue);
                self.accepted = Some(der);
                let stream = self.connect().await?;
                let tls = self.handshake(stream).await.map_err(Refusal::into_error)?;
                exchange(tls, &request).await
            }
            Err(refusal) => Err(refusal.into_error()),
        }
    }

    async fn connect(&self) -> Result<TcpStream, HttpFetchError> {
        heimdall_net::connect_addresses(self.addresses, None)
            .await
            .map(|connected| connected.stream)
            .map_err(|failure| HttpFetchError::Connection(server_text(&failure.to_string())))
    }

    /// The TLS handshake over `stream`: the system's check, else the certificate it refused
    /// earlier in this session, that one alone.
    async fn handshake(
        &self,
        stream: TcpStream,
    ) -> Result<tokio_rustls::client::TlsStream<TcpStream>, Refusal> {
        let name = ServerName::try_from(self.target.host.clone())
            .map_err(|error| Refusal::Other(server_text(&error.to_string())))?;
        let accepted = self.accepted.clone();
        let presented = PresentedSlot::default();
        let connector = heimdall_tls::connector(
            Arc::new(move |der: &[u8], _| match &accepted {
                Some(accepted) if accepted.as_slice() == der => UserVerdict::Decided,
                _ => UserVerdict::Untrusted,
            }),
            Arc::clone(&presented),
        );
        match connector.connect(name, stream).await {
            Ok(tls) => Ok(tls),
            Err(error) => {
                let kept = presented.lock().ok().and_then(|mut slot| slot.take());
                match kept {
                    Some(kept) if !kept.pinned => {
                        let self_issued = heimdall_rdp::ServerCertificate::from_der(&kept.der)
                            .is_ok_and(|certificate| certificate.subject == certificate.issuer);
                        Err(Refusal::Certificate(
                            kept.der,
                            kept.issue.refined(self_issued),
                        ))
                    }
                    _ => Err(Refusal::Other(server_text(&error.to_string()))),
                }
            }
        }
    }
}

/// Why a handshake failed.
enum Refusal {
    /// The system refused the certificate, kept whole (DER), for this reason.
    Certificate(Vec<u8>, ValidationIssue),
    /// Anything else: why, made safe to show.
    Other(String),
}

impl Refusal {
    fn into_error(self) -> HttpFetchError {
        match self {
            Self::Certificate(_, issue) => HttpFetchError::Certificate(issue),
            Self::Other(why) => HttpFetchError::Tls(why),
        }
    }
}

/// `request` written on `stream`, and the response's header section read.
async fn exchange<S: AsyncRead + AsyncWrite + Unpin>(
    mut stream: S,
    request: &str,
) -> Result<HttpResponseInfo, HttpFetchError> {
    let failed = |error: io::Error| HttpFetchError::Connection(server_text(&error.to_string()));
    stream.write_all(request.as_bytes()).await.map_err(failed)?;
    stream.flush().await.map_err(failed)?;
    let received = read_head(&mut stream).await.map_err(failed)?;
    if received.is_empty() {
        return Err(HttpFetchError::Empty);
    }
    Ok(parse_received(&received))
}

/// The start of the response, up to the end of its header section or
/// [`MAX_RESPONSE_BYTES`], whichever comes first.
async fn read_head<S: AsyncRead + Unpin>(stream: &mut S) -> io::Result<Vec<u8>> {
    let mut received = Vec::with_capacity(READ_CHUNK);
    let mut chunk = [0u8; READ_CHUNK];
    while received.len() < MAX_RESPONSE_BYTES {
        let room = (MAX_RESPONSE_BYTES - received.len()).min(READ_CHUNK);
        match stream.read(&mut chunk[..room]).await {
            Ok(0) => break,
            Ok(read) => {
                received.extend_from_slice(&chunk[..read]);
                if http_headers::header_end(&received).is_some() {
                    break;
                }
            }
            // A server that closes without TLS's close notice, once it said something.
            Err(error) if error.kind() == io::ErrorKind::UnexpectedEof && !received.is_empty() => {
                break;
            }
            Err(error) => return Err(error),
        }
    }
    Ok(received)
}

/// The header section of `received`, every line made safe to show, parsed.
fn parse_received(received: &[u8]) -> HttpResponseInfo {
    let end = http_headers::header_end(received).unwrap_or(received.len());
    let text = String::from_utf8_lossy(&received[..end]);
    let shown = shown_lines(text.trim_end_matches(['\r', '\n'])).join("\r\n");
    http_headers::parse_http_response(&shown)
}

#[cfg(test)]
mod tests {
    use super::*;
    use heimdall_core::tools::http_headers::parse_target;
    use tokio::net::TcpListener;

    /// A server on the loopback answering each connection with what `answer` gives for the
    /// request line it read; the request lines it saw are sent on `seen`.
    async fn server(
        answer: fn(&str) -> Vec<u8>,
    ) -> (u16, tokio::sync::mpsc::UnboundedReceiver<String>) {
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
        let port = listener.local_addr().expect("address").port();
        let (seen, lines) = tokio::sync::mpsc::unbounded_channel();
        tokio::spawn(async move {
            while let Ok((mut stream, _)) = listener.accept().await {
                let mut request = vec![0u8; 1024];
                let read = stream.read(&mut request).await.unwrap_or(0);
                let text = String::from_utf8_lossy(&request[..read]).into_owned();
                let line = text.lines().next().unwrap_or_default().to_owned();
                let _ = seen.send(line.clone());
                let _ = stream.write_all(&answer(&line)).await;
                let _ = stream.shutdown().await;
            }
        });
        (port, lines)
    }

    #[tokio::test]
    async fn a_head_refused_with_405_is_sent_again_as_a_get() {
        let (port, mut seen) = server(|line| {
            if line.starts_with("HEAD") {
                b"HTTP/1.1 405 Method Not Allowed\r\nAllow: GET\r\n\r\n".to_vec()
            } else {
                b"HTTP/1.1 200 OK\r\nServer: test\r\nX-Frame-Options: DENY\r\n\r\nbody".to_vec()
            }
        })
        .await;
        let target = parse_target(&format!("http://127.0.0.1:{port}/page")).unwrap();
        let fetched = fetch(&target).await.expect("fetched");
        assert_eq!(fetched.info.status_code, 200);
        assert_eq!(fetched.certificate, CertificateCheck::NotTls);
        assert!(
            fetched
                .info
                .headers
                .contains(&("Server".into(), "test".into()))
        );
        assert!(!fetched.info.header_section.contains("body"));
        assert_eq!(seen.recv().await.as_deref(), Some("HEAD /page HTTP/1.1"));
        assert_eq!(seen.recv().await.as_deref(), Some("GET /page HTTP/1.1"));
    }

    #[tokio::test]
    async fn at_most_sixteen_kilobytes_are_read_and_controls_never_shown() {
        let (port, _seen) = server(|_| {
            let mut answer = b"HTTP/1.1 200 OK\r\nX-Evil: a\x1b[2Jb\r\nX-Long: ".to_vec();
            answer.extend(std::iter::repeat_n(b'a', 4 * MAX_RESPONSE_BYTES));
            answer
        })
        .await;
        let target = parse_target(&format!("http://127.0.0.1:{port}/")).unwrap();
        let fetched = fetch(&target).await.expect("fetched");
        assert!(fetched.info.header_section.len() <= MAX_RESPONSE_BYTES + 64);
        assert!(
            fetched
                .info
                .headers
                .contains(&("X-Evil".into(), "a\\u{001B}[2Jb".into()))
        );
        assert!(!fetched.info.header_section.contains('\u{1b}'));
    }

    #[tokio::test]
    async fn a_server_that_says_nothing_is_said_as_such() {
        let (port, _seen) = server(|_| Vec::new()).await;
        let target = parse_target(&format!("http://127.0.0.1:{port}/")).unwrap();
        assert_eq!(fetch(&target).await, Err(HttpFetchError::Empty));
    }

    /// A TLS server on the loopback with a certificate for `localhost` no system trusts,
    /// answering every request with headers.
    async fn self_signed_server() -> u16 {
        use tokio_rustls::TlsAcceptor;
        use tokio_rustls::rustls::ServerConfig;
        use tokio_rustls::rustls::crypto::ring::default_provider;
        use tokio_rustls::rustls::pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer};

        let issued =
            rcgen::generate_simple_self_signed(vec!["localhost".to_owned()]).expect("cert");
        let certificate = CertificateDer::from(issued.cert.der().to_vec());
        let key =
            PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(issued.signing_key.serialize_der()));
        let config = ServerConfig::builder_with_provider(Arc::new(default_provider()))
            .with_safe_default_protocol_versions()
            .expect("versions")
            .with_no_client_auth()
            .with_single_cert(vec![certificate], key)
            .expect("server config");
        let acceptor = TlsAcceptor::from(Arc::new(config));
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
        let port = listener.local_addr().expect("address").port();
        tokio::spawn(async move {
            while let Ok((stream, _)) = listener.accept().await {
                let acceptor = acceptor.clone();
                tokio::spawn(async move {
                    if let Ok(mut tls) = acceptor.accept(stream).await {
                        let mut request = vec![0u8; 1024];
                        let _ = tls.read(&mut request).await;
                        let _ = tls
                            .write_all(
                                b"HTTP/1.1 200 OK\r\nStrict-Transport-Security: max-age=1\r\n\r\n",
                            )
                            .await;
                        let _ = tls.shutdown().await;
                    }
                });
            }
        });
        port
    }

    #[tokio::test]
    async fn a_certificate_the_system_refuses_is_a_finding_and_the_headers_are_read() {
        let port = self_signed_server().await;
        let target = parse_target(&format!("https://localhost:{port}/")).unwrap();
        let fetched = fetch(&target).await.expect("fetched");
        assert!(
            matches!(
                fetched.certificate,
                CertificateCheck::Untrusted(
                    ValidationIssue::SelfSigned | ValidationIssue::NoSystemStore
                )
            ),
            "{:?}",
            fetched.certificate
        );
        assert_eq!(fetched.info.status_code, 200);
        assert!(
            fetched
                .info
                .headers
                .iter()
                .any(|(name, _)| name == "Strict-Transport-Security")
        );
    }

    #[tokio::test]
    async fn a_port_that_speaks_no_tls_fails_the_handshake_not_the_certificate() {
        let (port, _seen) = server(|_| b"HTTP/1.1 400 Bad Request\r\n\r\n".to_vec()).await;
        let target = parse_target(&format!("https://127.0.0.1:{port}/")).unwrap();
        assert!(matches!(fetch(&target).await, Err(HttpFetchError::Tls(_))));
    }
}
