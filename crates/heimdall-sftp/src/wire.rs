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

//! Primitive SSH wire types (RFC 4251 section 5) used by SFTP packets.
//!
//! The reader never trusts a length it reads: a string longer than what is left fails,
//! and nothing is allocated from an announced size before the bytes are there.

use thiserror::Error;

/// Why bytes could not be read as a packet.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum WireError {
    /// The packet ended before a field did.
    #[error("packet truncated")]
    Truncated,
    /// Bytes were left after the last field.
    #[error("{0} unexpected trailing bytes")]
    Trailing(usize),
    /// The packet type is not one this side reads.
    #[error("unknown packet type {0}")]
    UnknownType(u8),
    /// A packet announced a length above the negotiated maximum.
    #[error("packet of {0} bytes exceeds the limit")]
    TooLong(u32),
}

/// Reads fields from a packet body, front to back.
pub struct Reader<'a> {
    bytes: &'a [u8],
}

impl<'a> Reader<'a> {
    /// A reader over `bytes`.
    #[must_use]
    pub fn new(bytes: &'a [u8]) -> Self {
        Self { bytes }
    }

    fn take(&mut self, count: usize) -> Result<&'a [u8], WireError> {
        if self.bytes.len() < count {
            return Err(WireError::Truncated);
        }
        let (head, rest) = self.bytes.split_at(count);
        self.bytes = rest;
        Ok(head)
    }

    /// One byte.
    ///
    /// # Errors
    ///
    /// [`WireError::Truncated`] at the end of the packet.
    pub fn u8(&mut self) -> Result<u8, WireError> {
        Ok(self.take(1)?[0])
    }

    /// A big-endian `uint32`.
    ///
    /// # Errors
    ///
    /// [`WireError::Truncated`] at the end of the packet.
    pub fn u32(&mut self) -> Result<u32, WireError> {
        let bytes = self.take(4)?;
        Ok(u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
    }

    /// A big-endian `uint64`.
    ///
    /// # Errors
    ///
    /// [`WireError::Truncated`] at the end of the packet.
    pub fn u64(&mut self) -> Result<u64, WireError> {
        Ok((u64::from(self.u32()?) << 32) | u64::from(self.u32()?))
    }

    /// A `string`: a `uint32` length and that many bytes.
    ///
    /// # Errors
    ///
    /// [`WireError::Truncated`] when fewer bytes remain than announced.
    pub fn string(&mut self) -> Result<&'a [u8], WireError> {
        let length = usize::try_from(self.u32()?).map_err(|_| WireError::Truncated)?;
        self.take(length)
    }

    /// Whether every byte was read.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.bytes.is_empty()
    }

    /// Bytes not yet read.
    #[must_use]
    pub fn remaining(&self) -> usize {
        self.bytes.len()
    }

    /// Fails when bytes are left: a well-formed packet is read to its end.
    ///
    /// # Errors
    ///
    /// [`WireError::Trailing`] with the count left.
    pub fn finish(&self) -> Result<(), WireError> {
        if self.bytes.is_empty() {
            Ok(())
        } else {
            Err(WireError::Trailing(self.bytes.len()))
        }
    }
}

/// Appends fields to a packet body.
#[derive(Default)]
pub struct Writer {
    bytes: Vec<u8>,
}

impl Writer {
    /// An empty body.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// One byte.
    pub fn u8(&mut self, value: u8) -> &mut Self {
        self.bytes.push(value);
        self
    }

    /// A big-endian `uint32`.
    pub fn u32(&mut self, value: u32) -> &mut Self {
        self.bytes.extend_from_slice(&value.to_be_bytes());
        self
    }

    /// A big-endian `uint64`.
    pub fn u64(&mut self, value: u64) -> &mut Self {
        self.bytes.extend_from_slice(&value.to_be_bytes());
        self
    }

    /// A `string`.
    ///
    /// # Panics
    ///
    /// When `value` is longer than `u32::MAX`, which no packet this crate builds can be.
    pub fn string(&mut self, value: &[u8]) -> &mut Self {
        let length = u32::try_from(value.len()).expect("SFTP strings fit in 32 bits");
        self.u32(length);
        self.bytes.extend_from_slice(value);
        self
    }

    /// The body.
    #[must_use]
    pub fn into_bytes(self) -> Vec<u8> {
        self.bytes
    }
}

#[cfg(test)]
mod tests {
    use super::{Reader, WireError, Writer};

    #[test]
    fn fields_round_trip() {
        let mut writer = Writer::new();
        writer
            .u8(7)
            .u32(0xDEAD_BEEF)
            .u64(0x0102_0304_0506_0708)
            .string(b"caf\xE9");
        let bytes = writer.into_bytes();
        let mut reader = Reader::new(&bytes);
        assert_eq!(reader.u8(), Ok(7));
        assert_eq!(reader.u32(), Ok(0xDEAD_BEEF));
        assert_eq!(reader.u64(), Ok(0x0102_0304_0506_0708));
        assert_eq!(reader.string(), Ok(&b"caf\xE9"[..]));
        assert_eq!(reader.finish(), Ok(()));
    }

    #[test]
    fn an_announced_length_beyond_the_packet_fails_without_allocating() {
        let bytes = [0xFF, 0xFF, 0xFF, 0xFF, b'a'];
        assert_eq!(Reader::new(&bytes).string(), Err(WireError::Truncated));
        assert_eq!(Reader::new(&[0, 0]).u32(), Err(WireError::Truncated));
    }

    #[test]
    fn trailing_bytes_are_reported() {
        let reader = Reader::new(b"xy");
        assert_eq!(reader.finish(), Err(WireError::Trailing(2)));
    }
}
