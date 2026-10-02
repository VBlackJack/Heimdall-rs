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

//! The domain's Key Distribution Center, reached from this computer while `CredSSP` logs on
//! with Kerberos.
//!
//! sspi finds the KDC itself (the system's Kerberos configuration, then the realm's DNS
//! records) and hands over each message to send. Its framing is kept as it expects it: over
//! TCP the message carries its 4-byte length and the reply is returned with its own; over UDP
//! neither does, and the reply is returned with its length put in front.

use std::io;
use std::net::SocketAddr;
use std::time::Duration;

use ironrdp::connector::sspi::generator::NetworkRequest;
use ironrdp::connector::sspi::network_client::NetworkProtocol;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpStream, UdpSocket};

/// Port of a KDC whose address names none.
const KDC_PORT: u16 = 88;

/// How long one exchange with a KDC may take, as Windows waits for one.
const KDC_TIMEOUT: Duration = Duration::from_secs(5);

/// Largest reply accepted: a ticket carrying a large group membership stays far below it,
/// and a length read off the wire never sizes an allocation beyond it.
const MAX_REPLY: usize = 1 << 20;

/// Largest UDP datagram.
const MAX_DATAGRAM: usize = 65_535;

/// Bytes of the length a Kerberos message carries over TCP.
const LENGTH_PREFIX: usize = 4;

/// Sends `request` to the KDC it names and returns the reply as sspi reads it.
///
/// # Errors
///
/// The KDC cannot be reached or does not answer in time, its reply is too large, or the
/// request goes to a KDC proxy, which is not offered.
pub(crate) async fn send(request: &NetworkRequest) -> io::Result<Vec<u8>> {
    let exchange = async {
        match request.protocol {
            NetworkProtocol::Tcp => tcp(&addresses(request).await?, &request.data).await,
            NetworkProtocol::Udp => udp(&addresses(request).await?, &request.data).await,
            NetworkProtocol::Http | NetworkProtocol::Https => Err(io::Error::new(
                io::ErrorKind::Unsupported,
                "a KDC proxy is not offered",
            )),
        }
    };
    tokio::time::timeout(KDC_TIMEOUT, exchange)
        .await
        .map_err(|_| io::Error::new(io::ErrorKind::TimedOut, "the KDC did not answer in time"))?
}

/// The addresses of the KDC `request` names.
async fn addresses(request: &NetworkRequest) -> io::Result<Vec<SocketAddr>> {
    let host = request
        .url
        .host_str()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "the KDC has no host"))?;
    // An IPv6 address keeps its brackets in a URL.
    let host = host.trim_start_matches('[').trim_end_matches(']');
    let port = request.url.port().unwrap_or(KDC_PORT);
    Ok(tokio::net::lookup_host((host, port)).await?.collect())
}

/// One exchange over TCP, with the first address that accepts the connection.
async fn tcp(addresses: &[SocketAddr], message: &[u8]) -> io::Result<Vec<u8>> {
    let mut stream = TcpStream::connect(addresses).await?;
    stream.write_all(message).await?;
    let mut prefix = [0; LENGTH_PREFIX];
    stream.read_exact(&mut prefix).await?;
    let length = reply_length(u32::from_be_bytes(prefix))?;
    let mut reply = Vec::with_capacity(LENGTH_PREFIX + length);
    reply.extend_from_slice(&prefix);
    reply.resize(LENGTH_PREFIX + length, 0);
    stream.read_exact(&mut reply[LENGTH_PREFIX..]).await?;
    Ok(reply)
}

/// One exchange over UDP, with the first address.
async fn udp(addresses: &[SocketAddr], message: &[u8]) -> io::Result<Vec<u8>> {
    let address = addresses
        .first()
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "the KDC has no address"))?;
    let local: SocketAddr = if address.is_ipv4() {
        (std::net::Ipv4Addr::UNSPECIFIED, 0).into()
    } else {
        (std::net::Ipv6Addr::UNSPECIFIED, 0).into()
    };
    let socket = UdpSocket::bind(local).await?;
    // Connected: a datagram from anywhere else is not taken for the reply.
    socket.connect(address).await?;
    socket.send(message).await?;
    let mut datagram = vec![0; MAX_DATAGRAM];
    let received = socket.recv(&mut datagram).await?;
    let length = u32::try_from(received)
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "the KDC reply is too large"))?;
    let mut reply = Vec::with_capacity(LENGTH_PREFIX + received);
    reply.extend_from_slice(&length.to_be_bytes());
    reply.extend_from_slice(&datagram[..received]);
    Ok(reply)
}

/// The length a TCP reply announces, when it is one to read.
fn reply_length(announced: u32) -> io::Result<usize> {
    usize::try_from(announced)
        .ok()
        .filter(|length| *length <= MAX_REPLY)
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "the KDC reply is too large"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::net::TcpListener;

    fn request(protocol: NetworkProtocol, url: &str, data: &[u8]) -> NetworkRequest {
        NetworkRequest {
            protocol,
            url: url.parse().expect("url"),
            data: data.to_vec(),
        }
    }

    #[tokio::test]
    async fn over_tcp_the_message_goes_as_given_and_the_reply_keeps_its_length() {
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
        let port = listener.local_addr().expect("address").port();
        let kdc = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.expect("accept");
            let mut received = [0; 7];
            stream.read_exact(&mut received).await.expect("message");
            stream
                .write_all(&[0, 0, 0, 2, 0xAB, 0xCD])
                .await
                .expect("reply");
            received
        });
        let reply = send(&request(
            NetworkProtocol::Tcp,
            &format!("tcp://127.0.0.1:{port}"),
            &[0, 0, 0, 3, 1, 2, 3],
        ))
        .await
        .expect("reply");
        assert_eq!(reply, [0, 0, 0, 2, 0xAB, 0xCD]);
        assert_eq!(kdc.await.expect("kdc"), [0, 0, 0, 3, 1, 2, 3]);
    }

    #[tokio::test]
    async fn over_udp_the_reply_gets_its_length_in_front() {
        let kdc = UdpSocket::bind("127.0.0.1:0").await.expect("bind");
        let port = kdc.local_addr().expect("address").port();
        let answer = tokio::spawn(async move {
            let mut received = [0; 16];
            let (length, from) = kdc.recv_from(&mut received).await.expect("message");
            kdc.send_to(&[9, 8, 7], from).await.expect("reply");
            received[..length].to_vec()
        });
        let reply = send(&request(
            NetworkProtocol::Udp,
            &format!("udp://127.0.0.1:{port}"),
            &[1, 2, 3],
        ))
        .await
        .expect("reply");
        assert_eq!(reply, [0, 0, 0, 3, 9, 8, 7]);
        assert_eq!(answer.await.expect("kdc"), [1, 2, 3]);
    }

    #[tokio::test]
    async fn a_tcp_reply_announcing_more_than_the_limit_is_not_read() {
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
        let port = listener.local_addr().expect("address").port();
        tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.expect("accept");
            let mut received = [0; 4];
            stream.read_exact(&mut received).await.expect("message");
            stream
                .write_all(&u32::MAX.to_be_bytes())
                .await
                .expect("reply");
            // Kept open: the refusal must not wait for the bytes announced.
            tokio::time::sleep(KDC_TIMEOUT * 2).await;
        });
        let error = send(&request(
            NetworkProtocol::Tcp,
            &format!("tcp://127.0.0.1:{port}"),
            &[0, 0, 0, 0],
        ))
        .await
        .expect_err("too large");
        assert_eq!(error.kind(), io::ErrorKind::InvalidData);
    }

    #[tokio::test]
    async fn a_kdc_nobody_listens_at_is_an_error_not_a_wait() {
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
        let port = listener.local_addr().expect("address").port();
        drop(listener);
        let error = send(&request(
            NetworkProtocol::Tcp,
            &format!("tcp://127.0.0.1:{port}"),
            &[0, 0, 0, 0],
        ))
        .await
        .expect_err("refused");
        assert_eq!(error.kind(), io::ErrorKind::ConnectionRefused);
    }

    #[tokio::test]
    async fn a_kdc_proxy_is_not_offered() {
        let error = send(&request(
            NetworkProtocol::Https,
            "https://kdc.example.test/KdcProxy",
            &[],
        ))
        .await
        .expect_err("proxy");
        assert_eq!(error.kind(), io::ErrorKind::Unsupported);
    }

    #[test]
    fn the_length_limit_holds_at_its_edge() {
        assert_eq!(
            reply_length(u32::try_from(MAX_REPLY).expect("small")).expect("at"),
            MAX_REPLY
        );
        assert!(reply_length(u32::try_from(MAX_REPLY + 1).expect("small")).is_err());
    }
}
