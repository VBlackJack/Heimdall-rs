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

//! A port of this computer whose connections all go to one destination from an SSH gateway,
//! as `ssh -L` opens one: a program that can only dial a host and a port (the `WinRM` client
//! of `PowerShell`, for one) reaches through it what the gateway reaches.
//!
//! [`start`] listens on the loopback address, on a port the system picks, for as long as the
//! [`LocalForward`] it returns lives: every connection it carries ends with it.

use std::io;
use std::net::{Ipv4Addr, SocketAddr};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use tokio::net::TcpListener;
use tokio_util::sync::{CancellationToken, DropGuard};

use crate::socks::Opener;

/// Clients carried at once: a `WinRM` session keeps two or three; more is refused, so a
/// program of this computer cannot open the gateway's way without bound.
pub const MAX_CLIENTS: usize = 16;

/// Wait after a failed accept, so a listener in trouble does not spin.
const ACCEPT_RETRY: Duration = Duration::from_millis(100);

/// A forward that runs until dropped.
#[derive(Debug)]
pub struct LocalForward {
    address: SocketAddr,
    _stop: DropGuard,
}

impl LocalForward {
    /// Where the forward listens.
    #[must_use]
    pub fn address(&self) -> SocketAddr {
        self.address
    }
}

/// Starts a forward on a port of the loopback address the system picks, each connection it
/// takes opened onward by `opener` to `host:port`.
///
/// # Errors
///
/// No port of the loopback address could be taken.
pub async fn start<O: Opener>(opener: Arc<O>, host: String, port: u16) -> io::Result<LocalForward> {
    let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).await?;
    let address = listener.local_addr()?;
    let stop = CancellationToken::new();
    tokio::spawn(serve(listener, opener, (host, port), stop.clone()));
    Ok(LocalForward {
        address,
        _stop: stop.drop_guard(),
    })
}

/// Takes clients on `listener` until `cancel`, each one carried both ways to `destination`.
async fn serve<O: Opener>(
    listener: TcpListener,
    opener: Arc<O>,
    destination: (String, u16),
    cancel: CancellationToken,
) {
    let carried = Arc::new(AtomicUsize::new(0));
    loop {
        let accepted = tokio::select! {
            () = cancel.cancelled() => return,
            accepted = listener.accept() => accepted,
        };
        let (mut client, peer) = match accepted {
            Ok(accepted) => accepted,
            Err(error) => {
                log::warn!("the local forward could not take a client: {error}");
                tokio::time::sleep(ACCEPT_RETRY).await;
                continue;
            }
        };
        if carried.load(Ordering::Acquire) >= MAX_CLIENTS {
            log::warn!("the local forward refused {peer}: {MAX_CLIENTS} clients already");
            continue;
        }
        log::debug!("the local forward took {peer}");
        carried.fetch_add(1, Ordering::AcqRel);
        let opener = Arc::clone(&opener);
        let cancel = cancel.clone();
        let carried = Arc::clone(&carried);
        let (host, port) = destination.clone();
        tokio::spawn(async move {
            let carry = async {
                let mut onward = opener
                    .open(host, port)
                    .await
                    .map_err(|error| io::Error::other(error.to_string()))?;
                tokio::io::copy_bidirectional(&mut client, &mut onward).await
            };
            tokio::select! {
                () = cancel.cancelled() => {}
                result = carry => {
                    if let Err(error) = result {
                        log::debug!("a local forward client ended: {error}");
                    }
                }
            }
            carried.fetch_sub(1, Ordering::AcqRel);
        });
    }
}

#[cfg(test)]
mod tests {
    use std::future::Future;
    use std::sync::Mutex;

    use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _, DuplexStream};
    use tokio::net::TcpStream;

    use super::*;
    use crate::error::ConnectError;

    /// Opens onward into pipes whose other ends the test holds; says where it was asked to go.
    #[derive(Default)]
    struct FakeGateway {
        asked: Mutex<Vec<(String, u16)>>,
        far: Mutex<Vec<DuplexStream>>,
        refuse: bool,
    }

    impl Opener for FakeGateway {
        type Stream = DuplexStream;

        fn open(
            &self,
            host: String,
            port: u16,
        ) -> impl Future<Output = Result<DuplexStream, ConnectError>> + Send {
            self.asked.lock().expect("asked").push((host.clone(), port));
            let result = if self.refuse {
                Err(ConnectError::JumpRefused { host, port })
            } else {
                let (near, far) = tokio::io::duplex(1024);
                self.far.lock().expect("far").push(far);
                Ok(near)
            };
            async move { result }
        }
    }

    /// The far end of the oldest connection onward not taken yet, once opened.
    async fn far(gateway: &FakeGateway) -> DuplexStream {
        loop {
            if let Some(far) = {
                let mut far = gateway.far.lock().expect("far");
                (!far.is_empty()).then(|| far.remove(0))
            } {
                return far;
            }
            tokio::task::yield_now().await;
        }
    }

    #[tokio::test]
    async fn it_listens_on_the_loopback_on_a_port_the_system_picks() {
        let forward = start(Arc::new(FakeGateway::default()), "dc.lab".to_owned(), 5985)
            .await
            .expect("start");
        assert!(forward.address().ip().is_loopback());
        assert_ne!(forward.address().port(), 0);
    }

    #[tokio::test]
    async fn each_client_reaches_the_one_destination_both_ways() {
        let gateway = Arc::new(FakeGateway::default());
        let forward = start(Arc::clone(&gateway), "dc.lab".to_owned(), 5985)
            .await
            .expect("start");
        let mut first = TcpStream::connect(forward.address()).await.expect("first");
        let mut second = TcpStream::connect(forward.address()).await.expect("second");
        first.write_all(b"one").await.expect("write");
        let mut far_first = far(&gateway).await;
        let mut read = [0; 3];
        far_first.read_exact(&mut read).await.expect("read");
        assert_eq!(&read, b"one");
        far_first.write_all(b"back").await.expect("write back");
        let mut back = [0; 4];
        first.read_exact(&mut back).await.expect("read back");
        assert_eq!(&back, b"back");
        second.write_all(b"two").await.expect("write");
        let mut far_second = far(&gateway).await;
        far_second.read_exact(&mut read).await.expect("read");
        assert_eq!(&read, b"two");
        assert_eq!(
            *gateway.asked.lock().expect("asked"),
            [("dc.lab".to_owned(), 5985), ("dc.lab".to_owned(), 5985)]
        );
    }

    #[tokio::test]
    async fn a_client_the_gateway_cannot_carry_is_closed() {
        let gateway = Arc::new(FakeGateway {
            refuse: true,
            ..FakeGateway::default()
        });
        let forward = start(gateway, "dc.lab".to_owned(), 5985)
            .await
            .expect("start");
        let mut client = TcpStream::connect(forward.address())
            .await
            .expect("connect");
        let mut read = [0; 1];
        let closed = client.read(&mut read).await;
        assert!(matches!(closed, Ok(0) | Err(_)), "{closed:?}");
    }

    #[tokio::test]
    async fn once_dropped_it_takes_no_client_and_ends_those_it_carried() {
        let gateway = Arc::new(FakeGateway::default());
        let forward = start(Arc::clone(&gateway), "dc.lab".to_owned(), 5985)
            .await
            .expect("start");
        let address = forward.address();
        let mut client = TcpStream::connect(address).await.expect("connect");
        client.write_all(b"x").await.expect("write");
        let _far = far(&gateway).await;
        drop(forward);
        let mut read = [0; 1];
        let ended = tokio::time::timeout(Duration::from_secs(5), client.read(&mut read))
            .await
            .expect("ended in time");
        assert!(matches!(ended, Ok(0) | Err(_)), "{ended:?}");
        // The listener is gone with it: a new client is refused, or closed at once.
        tokio::task::yield_now().await;
        if let Ok(mut late) = TcpStream::connect(address).await {
            let late = tokio::time::timeout(Duration::from_secs(5), late.read(&mut read))
                .await
                .expect("closed in time");
            assert!(matches!(late, Ok(0) | Err(_)), "{late:?}");
        }
    }

    #[tokio::test]
    async fn clients_beyond_the_limit_are_refused() {
        let gateway = Arc::new(FakeGateway::default());
        let forward = start(Arc::clone(&gateway), "dc.lab".to_owned(), 5985)
            .await
            .expect("start");
        let (mut held, mut clients) = (Vec::new(), Vec::new());
        for _ in 0..MAX_CLIENTS {
            let mut client = TcpStream::connect(forward.address())
                .await
                .expect("connect");
            client.write_all(b"x").await.expect("write");
            held.push(far(&gateway).await);
            clients.push(client);
        }
        let mut over = TcpStream::connect(forward.address())
            .await
            .expect("connect");
        let mut read = [0; 1];
        let refused = tokio::time::timeout(Duration::from_secs(5), over.read(&mut read))
            .await
            .expect("closed in time");
        assert!(matches!(refused, Ok(0) | Err(_)), "{refused:?}");
        assert_eq!(gateway.asked.lock().expect("asked").len(), MAX_CLIENTS);
    }
}
