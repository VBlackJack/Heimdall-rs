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

//! Whole RDP frames read from a plain tokio stream.
//!
//! `IronRDP`'s own reader goes through generic futures whose `Send` the compiler cannot
//! prove inside a spawned task; this one does the same with `find_size`, over a concrete
//! stream.

use std::io;

use ironrdp::pdu::{Action, find_size};
use ironrdp_tokio::bytes::BytesMut;
use tokio::io::{AsyncRead, AsyncReadExt as _};

/// Reads frames; what arrives beyond one frame waits in the buffer for the next.
pub(crate) struct FrameReader<R> {
    stream: R,
    buffer: BytesMut,
}

impl<R: AsyncRead + Unpin> FrameReader<R> {
    /// A reader over `stream`, starting with `leftover` bytes already read from it.
    pub(crate) fn new(stream: R, leftover: BytesMut) -> Self {
        Self {
            stream,
            buffer: leftover,
        }
    }

    /// The next whole frame. Cancel safe: bytes read before a cancellation stay in the
    /// buffer.
    ///
    /// # Errors
    ///
    /// The stream fails or ends, or the next bytes are not the start of a frame.
    pub(crate) async fn read(&mut self) -> io::Result<(Action, BytesMut)> {
        loop {
            if let Some(info) = find_size(&self.buffer).map_err(io::Error::other)?
                && self.buffer.len() >= info.length
            {
                return Ok((info.action, self.buffer.split_to(info.length)));
            }
            if self.stream.read_buf(&mut self.buffer).await? == 0 {
                return Err(io::Error::new(
                    io::ErrorKind::UnexpectedEof,
                    "the server closed the connection",
                ));
            }
        }
    }
}
