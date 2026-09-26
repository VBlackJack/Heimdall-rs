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

//! SFTP version 3 packets (draft-ietf-secsh-filexfer-02), both directions.
//!
//! Both sides are encoded and decoded: the client sends requests and reads responses, and
//! tests play a server, a hostile one included, with the same code. A frame on the wire is
//! a `uint32` length followed by that many bytes: the packet type and its fields.

use crate::path::RemotePath;
use crate::wire::{Reader, WireError, Writer};

/// The protocol version this client speaks.
pub const SFTP_VERSION: u32 = 3;

/// Extension names and their data, in the order sent.
pub type Extensions = Vec<(Vec<u8>, Vec<u8>)>;

/// Packet type numbers.
mod kind {
    pub const INIT: u8 = 1;
    pub const VERSION: u8 = 2;
    pub const OPEN: u8 = 3;
    pub const CLOSE: u8 = 4;
    pub const READ: u8 = 5;
    pub const WRITE: u8 = 6;
    pub const LSTAT: u8 = 7;
    pub const FSTAT: u8 = 8;
    pub const SETSTAT: u8 = 9;
    pub const FSETSTAT: u8 = 10;
    pub const OPENDIR: u8 = 11;
    pub const READDIR: u8 = 12;
    pub const REMOVE: u8 = 13;
    pub const MKDIR: u8 = 14;
    pub const RMDIR: u8 = 15;
    pub const REALPATH: u8 = 16;
    pub const STAT: u8 = 17;
    pub const RENAME: u8 = 18;
    pub const READLINK: u8 = 19;
    pub const SYMLINK: u8 = 20;
    pub const STATUS: u8 = 101;
    pub const HANDLE: u8 = 102;
    pub const DATA: u8 = 103;
    pub const NAME: u8 = 104;
    pub const ATTRS: u8 = 105;
    pub const EXTENDED: u8 = 200;
    pub const EXTENDED_REPLY: u8 = 201;
}

/// Attribute presence flags.
mod attr {
    pub const SIZE: u32 = 0x0000_0001;
    pub const UIDGID: u32 = 0x0000_0002;
    pub const PERMISSIONS: u32 = 0x0000_0004;
    pub const ACMODTIME: u32 = 0x0000_0008;
    pub const EXTENDED: u32 = 0x8000_0000;
}

/// Flags of an open request.
pub mod open_flags {
    /// Open for reading.
    pub const READ: u32 = 0x0000_0001;
    /// Open for writing.
    pub const WRITE: u32 = 0x0000_0002;
    /// Every write goes to the end of the file.
    pub const APPEND: u32 = 0x0000_0004;
    /// Create the file if it does not exist.
    pub const CREATE: u32 = 0x0000_0008;
    /// Truncate an existing file.
    pub const TRUNCATE: u32 = 0x0000_0010;
    /// Fail if the file exists (with `CREATE`).
    pub const EXCLUSIVE: u32 = 0x0000_0020;
}

/// Status codes of version 3.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StatusCode {
    /// Success.
    Ok,
    /// End of file, or of a directory listing.
    Eof,
    /// No such file.
    NoSuchFile,
    /// Permission denied.
    PermissionDenied,
    /// Failure without a more precise code.
    Failure,
    /// The server could not read the request.
    BadMessage,
    /// No connection (client-side code, never sent by a server).
    NoConnection,
    /// Connection lost (client-side code, never sent by a server).
    ConnectionLost,
    /// The server does not support the operation.
    OpUnsupported,
    /// A code this version does not define.
    Other(u32),
}

impl StatusCode {
    /// The code a number stands for.
    #[must_use]
    pub fn from_u32(code: u32) -> Self {
        match code {
            0 => Self::Ok,
            1 => Self::Eof,
            2 => Self::NoSuchFile,
            3 => Self::PermissionDenied,
            4 => Self::Failure,
            5 => Self::BadMessage,
            6 => Self::NoConnection,
            7 => Self::ConnectionLost,
            8 => Self::OpUnsupported,
            other => Self::Other(other),
        }
    }

    /// The number on the wire.
    #[must_use]
    pub fn as_u32(self) -> u32 {
        match self {
            Self::Ok => 0,
            Self::Eof => 1,
            Self::NoSuchFile => 2,
            Self::PermissionDenied => 3,
            Self::Failure => 4,
            Self::BadMessage => 5,
            Self::NoConnection => 6,
            Self::ConnectionLost => 7,
            Self::OpUnsupported => 8,
            Self::Other(code) => code,
        }
    }
}

/// File attributes; absent fields were not sent.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Attributes {
    /// Size in bytes.
    pub size: Option<u64>,
    /// Owner and group ids.
    pub uid_gid: Option<(u32, u32)>,
    /// POSIX mode bits, file type included.
    pub permissions: Option<u32>,
    /// Access and modification times, seconds since the epoch.
    pub times: Option<(u32, u32)>,
    /// Extended attributes: name and value.
    pub extended: Vec<(Vec<u8>, Vec<u8>)>,
}

impl Attributes {
    fn write(&self, writer: &mut Writer) {
        let mut flags = 0;
        if self.size.is_some() {
            flags |= attr::SIZE;
        }
        if self.uid_gid.is_some() {
            flags |= attr::UIDGID;
        }
        if self.permissions.is_some() {
            flags |= attr::PERMISSIONS;
        }
        if self.times.is_some() {
            flags |= attr::ACMODTIME;
        }
        if !self.extended.is_empty() {
            flags |= attr::EXTENDED;
        }
        writer.u32(flags);
        if let Some(size) = self.size {
            writer.u64(size);
        }
        if let Some((uid, gid)) = self.uid_gid {
            writer.u32(uid).u32(gid);
        }
        if let Some(permissions) = self.permissions {
            writer.u32(permissions);
        }
        if let Some((atime, mtime)) = self.times {
            writer.u32(atime).u32(mtime);
        }
        if !self.extended.is_empty() {
            let count = u32::try_from(self.extended.len()).expect("few extended attributes");
            writer.u32(count);
            for (name, value) in &self.extended {
                writer.string(name).string(value);
            }
        }
    }

    fn read(reader: &mut Reader<'_>) -> Result<Self, WireError> {
        let flags = reader.u32()?;
        let mut attributes = Self::default();
        if flags & attr::SIZE != 0 {
            attributes.size = Some(reader.u64()?);
        }
        if flags & attr::UIDGID != 0 {
            attributes.uid_gid = Some((reader.u32()?, reader.u32()?));
        }
        if flags & attr::PERMISSIONS != 0 {
            attributes.permissions = Some(reader.u32()?);
        }
        if flags & attr::ACMODTIME != 0 {
            attributes.times = Some((reader.u32()?, reader.u32()?));
        }
        if flags & attr::EXTENDED != 0 {
            let count = reader.u32()?;
            // Nothing is allocated from the announced count: pairs are pushed as they are
            // read, and the first missing byte ends the loop with an error.
            for _ in 0..count {
                let name = reader.string()?.to_vec();
                let value = reader.string()?.to_vec();
                attributes.extended.push((name, value));
            }
        }
        Ok(attributes)
    }
}

/// One entry of a directory listing or of a name reply.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NameEntry {
    /// The name, as bytes.
    pub filename: Vec<u8>,
    /// An `ls -l` style line, as bytes; informative only.
    pub longname: Vec<u8>,
    /// Attributes.
    pub attributes: Attributes,
}

/// A packet from the client.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Request {
    /// First packet: the version and the client's extensions.
    Init {
        /// Version.
        version: u32,
        /// Extension names and data.
        extensions: Extensions,
    },
    /// Open a file.
    Open {
        /// Request id.
        id: u32,
        /// File.
        path: RemotePath,
        /// [`open_flags`].
        flags: u32,
        /// Attributes of a created file.
        attributes: Attributes,
    },
    /// Close a handle.
    Close {
        /// Request id.
        id: u32,
        /// Handle.
        handle: Vec<u8>,
    },
    /// Read from a file.
    Read {
        /// Request id.
        id: u32,
        /// Handle.
        handle: Vec<u8>,
        /// Offset.
        offset: u64,
        /// Bytes wanted.
        length: u32,
    },
    /// Write to a file.
    Write {
        /// Request id.
        id: u32,
        /// Handle.
        handle: Vec<u8>,
        /// Offset.
        offset: u64,
        /// Bytes.
        data: Vec<u8>,
    },
    /// Attributes, not following a final symbolic link.
    Lstat {
        /// Request id.
        id: u32,
        /// Path.
        path: RemotePath,
    },
    /// Attributes of an open handle.
    Fstat {
        /// Request id.
        id: u32,
        /// Handle.
        handle: Vec<u8>,
    },
    /// Change attributes by path.
    Setstat {
        /// Request id.
        id: u32,
        /// Path.
        path: RemotePath,
        /// Attributes to set.
        attributes: Attributes,
    },
    /// Change attributes of an open handle.
    Fsetstat {
        /// Request id.
        id: u32,
        /// Handle.
        handle: Vec<u8>,
        /// Attributes to set.
        attributes: Attributes,
    },
    /// Open a directory for listing.
    Opendir {
        /// Request id.
        id: u32,
        /// Directory.
        path: RemotePath,
    },
    /// Next page of a listing.
    Readdir {
        /// Request id.
        id: u32,
        /// Handle.
        handle: Vec<u8>,
    },
    /// Delete a file.
    Remove {
        /// Request id.
        id: u32,
        /// File.
        path: RemotePath,
    },
    /// Create a directory.
    Mkdir {
        /// Request id.
        id: u32,
        /// Directory.
        path: RemotePath,
        /// Attributes.
        attributes: Attributes,
    },
    /// Delete an empty directory.
    Rmdir {
        /// Request id.
        id: u32,
        /// Directory.
        path: RemotePath,
    },
    /// Canonical absolute form of a path.
    Realpath {
        /// Request id.
        id: u32,
        /// Path.
        path: RemotePath,
    },
    /// Attributes, following symbolic links.
    Stat {
        /// Request id.
        id: u32,
        /// Path.
        path: RemotePath,
    },
    /// Rename; fails when the target exists.
    Rename {
        /// Request id.
        id: u32,
        /// Current path.
        from: RemotePath,
        /// New path.
        to: RemotePath,
    },
    /// Target of a symbolic link.
    Readlink {
        /// Request id.
        id: u32,
        /// Link.
        path: RemotePath,
    },
    /// Create a symbolic link. The two paths go on the wire in this order, the one OpenSSH
    /// reads (its PROTOCOL file notes it swapped them against the draft).
    Symlink {
        /// Request id.
        id: u32,
        /// What the link points to.
        target: RemotePath,
        /// The link to create.
        link: RemotePath,
    },
    /// A vendor extension.
    Extended {
        /// Request id.
        id: u32,
        /// Extension name, such as `posix-rename@openssh.com`.
        name: Vec<u8>,
        /// Extension-specific fields, already encoded.
        data: Vec<u8>,
    },
}

/// A packet from the server.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Response {
    /// Answer to `Init`.
    Version {
        /// Version.
        version: u32,
        /// Extension names and data.
        extensions: Extensions,
    },
    /// Outcome of a request without data.
    Status {
        /// Request id.
        id: u32,
        /// Code.
        code: StatusCode,
        /// Server message, untrusted.
        message: Vec<u8>,
    },
    /// An open file or directory.
    Handle {
        /// Request id.
        id: u32,
        /// Handle.
        handle: Vec<u8>,
    },
    /// Bytes read.
    Data {
        /// Request id.
        id: u32,
        /// Bytes.
        data: Vec<u8>,
    },
    /// Names: a listing page, a real path, a link target.
    Name {
        /// Request id.
        id: u32,
        /// Entries.
        entries: Vec<NameEntry>,
    },
    /// Attributes.
    Attrs {
        /// Request id.
        id: u32,
        /// Attributes.
        attributes: Attributes,
    },
    /// Answer to an extension.
    ExtendedReply {
        /// Request id.
        id: u32,
        /// Extension-specific fields.
        data: Vec<u8>,
    },
}

impl Response {
    /// The request this answers; `None` for `Version`.
    #[must_use]
    pub fn id(&self) -> Option<u32> {
        match self {
            Self::Version { .. } => None,
            Self::Status { id, .. }
            | Self::Handle { id, .. }
            | Self::Data { id, .. }
            | Self::Name { id, .. }
            | Self::Attrs { id, .. }
            | Self::ExtendedReply { id, .. } => Some(*id),
        }
    }
}

/// The frame of `body` followed by `tail`: a `uint32` length, then the bytes.
fn frame(body: Writer, tail: &[u8]) -> Vec<u8> {
    let body = body.into_bytes();
    let length = u32::try_from(body.len() + tail.len()).expect("SFTP packets fit in 32 bits");
    let mut framed = Vec::with_capacity(body.len() + tail.len() + 4);
    framed.extend_from_slice(&length.to_be_bytes());
    framed.extend_from_slice(&body);
    framed.extend_from_slice(tail);
    framed
}

fn extensions(reader: &mut Reader<'_>) -> Result<Extensions, WireError> {
    let mut pairs = Vec::new();
    while !reader.is_empty() {
        pairs.push((reader.string()?.to_vec(), reader.string()?.to_vec()));
    }
    Ok(pairs)
}

fn path(reader: &mut Reader<'_>) -> Result<RemotePath, WireError> {
    Ok(RemotePath::from_bytes(reader.string()?))
}

impl Request {
    /// The frame to send: length, type, fields.
    ///
    /// # Panics
    ///
    /// When a field exceeds the 4 GiB a packet length can express; the client never builds
    /// such a packet.
    #[must_use]
    #[allow(clippy::too_many_lines, reason = "one arm per packet type")]
    pub fn encode(&self) -> Vec<u8> {
        let mut w = Writer::new();
        match self {
            Self::Init {
                version,
                extensions,
            } => {
                w.u8(kind::INIT).u32(*version);
                for (name, data) in extensions {
                    w.string(name).string(data);
                }
            }
            Self::Open {
                id,
                path,
                flags,
                attributes,
            } => {
                w.u8(kind::OPEN)
                    .u32(*id)
                    .string(path.as_bytes())
                    .u32(*flags);
                attributes.write(&mut w);
            }
            Self::Close { id, handle } => {
                w.u8(kind::CLOSE).u32(*id).string(handle);
            }
            Self::Read {
                id,
                handle,
                offset,
                length,
            } => {
                w.u8(kind::READ)
                    .u32(*id)
                    .string(handle)
                    .u64(*offset)
                    .u32(*length);
            }
            Self::Write {
                id,
                handle,
                offset,
                data,
            } => {
                w.u8(kind::WRITE)
                    .u32(*id)
                    .string(handle)
                    .u64(*offset)
                    .string(data);
            }
            Self::Lstat { id, path } => {
                w.u8(kind::LSTAT).u32(*id).string(path.as_bytes());
            }
            Self::Fstat { id, handle } => {
                w.u8(kind::FSTAT).u32(*id).string(handle);
            }
            Self::Setstat {
                id,
                path,
                attributes,
            } => {
                w.u8(kind::SETSTAT).u32(*id).string(path.as_bytes());
                attributes.write(&mut w);
            }
            Self::Fsetstat {
                id,
                handle,
                attributes,
            } => {
                w.u8(kind::FSETSTAT).u32(*id).string(handle);
                attributes.write(&mut w);
            }
            Self::Opendir { id, path } => {
                w.u8(kind::OPENDIR).u32(*id).string(path.as_bytes());
            }
            Self::Readdir { id, handle } => {
                w.u8(kind::READDIR).u32(*id).string(handle);
            }
            Self::Remove { id, path } => {
                w.u8(kind::REMOVE).u32(*id).string(path.as_bytes());
            }
            Self::Mkdir {
                id,
                path,
                attributes,
            } => {
                w.u8(kind::MKDIR).u32(*id).string(path.as_bytes());
                attributes.write(&mut w);
            }
            Self::Rmdir { id, path } => {
                w.u8(kind::RMDIR).u32(*id).string(path.as_bytes());
            }
            Self::Realpath { id, path } => {
                w.u8(kind::REALPATH).u32(*id).string(path.as_bytes());
            }
            Self::Stat { id, path } => {
                w.u8(kind::STAT).u32(*id).string(path.as_bytes());
            }
            Self::Rename { id, from, to } => {
                w.u8(kind::RENAME)
                    .u32(*id)
                    .string(from.as_bytes())
                    .string(to.as_bytes());
            }
            Self::Readlink { id, path } => {
                w.u8(kind::READLINK).u32(*id).string(path.as_bytes());
            }
            Self::Symlink { id, target, link } => {
                w.u8(kind::SYMLINK)
                    .u32(*id)
                    .string(target.as_bytes())
                    .string(link.as_bytes());
            }
            Self::Extended { id, name, data } => {
                w.u8(kind::EXTENDED).u32(*id).string(name);
                return frame(w, data);
            }
        }
        frame(w, &[])
    }

    /// A request from a frame body (type and fields, without the length).
    ///
    /// # Errors
    ///
    /// [`WireError`] when the body is not a well-formed request.
    #[allow(clippy::too_many_lines, reason = "one arm per packet type")]
    pub fn decode(body: &[u8]) -> Result<Self, WireError> {
        let mut r = Reader::new(body);
        let packet = r.u8()?;
        let request = match packet {
            kind::INIT => {
                let version = r.u32()?;
                Self::Init {
                    version,
                    extensions: extensions(&mut r)?,
                }
            }
            kind::OPEN => Self::Open {
                id: r.u32()?,
                path: path(&mut r)?,
                flags: r.u32()?,
                attributes: Attributes::read(&mut r)?,
            },
            kind::CLOSE => Self::Close {
                id: r.u32()?,
                handle: r.string()?.to_vec(),
            },
            kind::READ => Self::Read {
                id: r.u32()?,
                handle: r.string()?.to_vec(),
                offset: r.u64()?,
                length: r.u32()?,
            },
            kind::WRITE => Self::Write {
                id: r.u32()?,
                handle: r.string()?.to_vec(),
                offset: r.u64()?,
                data: r.string()?.to_vec(),
            },
            kind::LSTAT => Self::Lstat {
                id: r.u32()?,
                path: path(&mut r)?,
            },
            kind::FSTAT => Self::Fstat {
                id: r.u32()?,
                handle: r.string()?.to_vec(),
            },
            kind::SETSTAT => Self::Setstat {
                id: r.u32()?,
                path: path(&mut r)?,
                attributes: Attributes::read(&mut r)?,
            },
            kind::FSETSTAT => Self::Fsetstat {
                id: r.u32()?,
                handle: r.string()?.to_vec(),
                attributes: Attributes::read(&mut r)?,
            },
            kind::OPENDIR => Self::Opendir {
                id: r.u32()?,
                path: path(&mut r)?,
            },
            kind::READDIR => Self::Readdir {
                id: r.u32()?,
                handle: r.string()?.to_vec(),
            },
            kind::REMOVE => Self::Remove {
                id: r.u32()?,
                path: path(&mut r)?,
            },
            kind::MKDIR => Self::Mkdir {
                id: r.u32()?,
                path: path(&mut r)?,
                attributes: Attributes::read(&mut r)?,
            },
            kind::RMDIR => Self::Rmdir {
                id: r.u32()?,
                path: path(&mut r)?,
            },
            kind::REALPATH => Self::Realpath {
                id: r.u32()?,
                path: path(&mut r)?,
            },
            kind::STAT => Self::Stat {
                id: r.u32()?,
                path: path(&mut r)?,
            },
            kind::RENAME => Self::Rename {
                id: r.u32()?,
                from: path(&mut r)?,
                to: path(&mut r)?,
            },
            kind::READLINK => Self::Readlink {
                id: r.u32()?,
                path: path(&mut r)?,
            },
            kind::SYMLINK => Self::Symlink {
                id: r.u32()?,
                target: path(&mut r)?,
                link: path(&mut r)?,
            },
            kind::EXTENDED => {
                let id = r.u32()?;
                let name = r.string()?.to_vec();
                let data = body[body.len() - r.remaining()..].to_vec();
                return Ok(Self::Extended { id, name, data });
            }
            other => return Err(WireError::UnknownType(other)),
        };
        r.finish()?;
        Ok(request)
    }
}

impl Response {
    /// The frame to send: length, type, fields.
    ///
    /// # Panics
    ///
    /// When a field exceeds the 4 GiB a packet length can express; the client never builds
    /// such a packet.
    #[must_use]
    #[allow(clippy::too_many_lines, reason = "one arm per packet type")]
    pub fn encode(&self) -> Vec<u8> {
        let mut w = Writer::new();
        match self {
            Self::Version {
                version,
                extensions,
            } => {
                w.u8(kind::VERSION).u32(*version);
                for (name, data) in extensions {
                    w.string(name).string(data);
                }
            }
            Self::Status { id, code, message } => {
                w.u8(kind::STATUS)
                    .u32(*id)
                    .u32(code.as_u32())
                    .string(message)
                    .string(b"");
            }
            Self::Handle { id, handle } => {
                w.u8(kind::HANDLE).u32(*id).string(handle);
            }
            Self::Data { id, data } => {
                w.u8(kind::DATA).u32(*id).string(data);
            }
            Self::Name { id, entries } => {
                let count = u32::try_from(entries.len()).expect("listing pages are bounded");
                w.u8(kind::NAME).u32(*id).u32(count);
                for entry in entries {
                    w.string(&entry.filename).string(&entry.longname);
                    entry.attributes.write(&mut w);
                }
            }
            Self::Attrs { id, attributes } => {
                w.u8(kind::ATTRS).u32(*id);
                attributes.write(&mut w);
            }
            Self::ExtendedReply { id, data } => {
                w.u8(kind::EXTENDED_REPLY).u32(*id);
                return frame(w, data);
            }
        }
        frame(w, &[])
    }

    /// A response from a frame body (type and fields, without the length).
    ///
    /// # Errors
    ///
    /// [`WireError`] when the body is not a well-formed response.
    #[allow(clippy::too_many_lines, reason = "one arm per packet type")]
    pub fn decode(body: &[u8]) -> Result<Self, WireError> {
        let mut r = Reader::new(body);
        let packet = r.u8()?;
        let response = match packet {
            kind::VERSION => {
                let version = r.u32()?;
                Self::Version {
                    version,
                    extensions: extensions(&mut r)?,
                }
            }
            kind::STATUS => {
                let id = r.u32()?;
                let code = StatusCode::from_u32(r.u32()?);
                // Version 3 servers before the draft's revision omit message and language.
                let message = if r.is_empty() {
                    Vec::new()
                } else {
                    r.string()?.to_vec()
                };
                if !r.is_empty() {
                    r.string()?;
                }
                Self::Status { id, code, message }
            }
            kind::HANDLE => Self::Handle {
                id: r.u32()?,
                handle: r.string()?.to_vec(),
            },
            kind::DATA => Self::Data {
                id: r.u32()?,
                data: r.string()?.to_vec(),
            },
            kind::NAME => {
                let id = r.u32()?;
                let count = r.u32()?;
                // Nothing is allocated from the announced count: entries are pushed as they
                // are read, and the first missing byte ends the loop with an error.
                let mut entries = Vec::new();
                for _ in 0..count {
                    entries.push(NameEntry {
                        filename: r.string()?.to_vec(),
                        longname: r.string()?.to_vec(),
                        attributes: Attributes::read(&mut r)?,
                    });
                }
                Self::Name { id, entries }
            }
            kind::ATTRS => Self::Attrs {
                id: r.u32()?,
                attributes: Attributes::read(&mut r)?,
            },
            kind::EXTENDED_REPLY => {
                let id = r.u32()?;
                let data = body[body.len() - r.remaining()..].to_vec();
                return Ok(Self::ExtendedReply { id, data });
            }
            other => return Err(WireError::UnknownType(other)),
        };
        r.finish()?;
        Ok(response)
    }
}

#[cfg(test)]
mod tests {
    use super::{Attributes, NameEntry, Request, Response, SFTP_VERSION, StatusCode, open_flags};
    use crate::path::RemotePath;
    use crate::wire::WireError;

    fn body(frame: &[u8]) -> &[u8] {
        let length = u32::from_be_bytes([frame[0], frame[1], frame[2], frame[3]]) as usize;
        assert_eq!(
            length,
            frame.len() - 4,
            "the length covers the rest of the frame"
        );
        &frame[4..]
    }

    fn full_attributes() -> Attributes {
        Attributes {
            size: Some(1 << 40),
            uid_gid: Some((1000, 100)),
            permissions: Some(0o100_644),
            times: Some((1_700_000_000, 1_700_000_001)),
            extended: vec![(b"acl@x".to_vec(), b"\x00\xFF".to_vec())],
        }
    }

    #[test]
    #[allow(clippy::too_many_lines, reason = "one request of each type")]
    fn every_request_round_trips() {
        let latin1 = RemotePath::from_bytes(b"/srv/caf\xE9".to_vec());
        let handle = b"h\x00\x01".to_vec();
        let requests = vec![
            Request::Init {
                version: SFTP_VERSION,
                extensions: vec![(b"x@y".to_vec(), b"1".to_vec())],
            },
            Request::Open {
                id: 1,
                path: latin1.clone(),
                flags: open_flags::WRITE | open_flags::CREATE | open_flags::TRUNCATE,
                attributes: full_attributes(),
            },
            Request::Close {
                id: 2,
                handle: handle.clone(),
            },
            Request::Read {
                id: 3,
                handle: handle.clone(),
                offset: u64::MAX - 1,
                length: 32_768,
            },
            Request::Write {
                id: 4,
                handle: handle.clone(),
                offset: 7,
                data: vec![0, 1, 2],
            },
            Request::Lstat {
                id: 5,
                path: latin1.clone(),
            },
            Request::Fstat {
                id: 6,
                handle: handle.clone(),
            },
            Request::Setstat {
                id: 7,
                path: latin1.clone(),
                attributes: Attributes {
                    permissions: Some(0o755),
                    ..Attributes::default()
                },
            },
            Request::Fsetstat {
                id: 8,
                handle: handle.clone(),
                attributes: Attributes::default(),
            },
            Request::Opendir {
                id: 9,
                path: latin1.clone(),
            },
            Request::Readdir {
                id: 10,
                handle: handle.clone(),
            },
            Request::Remove {
                id: 11,
                path: latin1.clone(),
            },
            Request::Mkdir {
                id: 12,
                path: latin1.clone(),
                attributes: Attributes::default(),
            },
            Request::Rmdir {
                id: 13,
                path: latin1.clone(),
            },
            Request::Realpath {
                id: 14,
                path: RemotePath::from("."),
            },
            Request::Stat {
                id: 15,
                path: latin1.clone(),
            },
            Request::Rename {
                id: 16,
                from: latin1.clone(),
                to: RemotePath::from("/srv/b"),
            },
            Request::Readlink {
                id: 17,
                path: latin1.clone(),
            },
            Request::Symlink {
                id: 18,
                target: RemotePath::from("/t"),
                link: RemotePath::from("/l"),
            },
            Request::Extended {
                id: 19,
                name: b"posix-rename@openssh.com".to_vec(),
                data: b"\x00\x00\x00\x01a\x00\x00\x00\x01b".to_vec(),
            },
        ];
        for request in requests {
            let frame = request.encode();
            assert_eq!(
                Request::decode(body(&frame)),
                Ok(request.clone()),
                "{request:?}"
            );
        }
    }

    #[test]
    fn every_response_round_trips() {
        let responses = vec![
            Response::Version {
                version: SFTP_VERSION,
                extensions: vec![(b"limits@openssh.com".to_vec(), b"1".to_vec())],
            },
            Response::Status {
                id: 1,
                code: StatusCode::Eof,
                message: b"end".to_vec(),
            },
            Response::Handle {
                id: 2,
                handle: b"\x00".to_vec(),
            },
            Response::Data {
                id: 3,
                data: vec![9; 100],
            },
            Response::Name {
                id: 4,
                entries: vec![NameEntry {
                    filename: b"caf\xE9".to_vec(),
                    longname: b"-rw-r--r-- 1 u g 0 Jan 1 caf\xE9".to_vec(),
                    attributes: full_attributes(),
                }],
            },
            Response::Attrs {
                id: 5,
                attributes: full_attributes(),
            },
            Response::ExtendedReply {
                id: 6,
                data: vec![1, 2, 3],
            },
        ];
        for response in responses {
            let frame = response.encode();
            assert_eq!(
                Response::decode(body(&frame)),
                Ok(response.clone()),
                "{response:?}"
            );
        }
    }

    #[test]
    fn a_status_without_message_is_accepted() {
        let body = [101, 0, 0, 0, 7, 0, 0, 0, 2];
        assert_eq!(
            Response::decode(&body),
            Ok(Response::Status {
                id: 7,
                code: StatusCode::NoSuchFile,
                message: Vec::new()
            })
        );
    }

    #[test]
    fn a_listing_announcing_more_entries_than_it_holds_is_refused() {
        // NAME, id 1, count 4 billion, nothing else.
        let body = [104, 0, 0, 0, 1, 0xFF, 0xFF, 0xFF, 0xFF];
        assert_eq!(Response::decode(&body), Err(WireError::Truncated));
    }

    #[test]
    fn attributes_announcing_more_extensions_than_they_hold_are_refused() {
        // ATTRS, id 1, flags EXTENDED, count 4 billion.
        let body = [105, 0, 0, 0, 1, 0x80, 0, 0, 0, 0xFF, 0xFF, 0xFF, 0xFF];
        assert_eq!(Response::decode(&body), Err(WireError::Truncated));
    }

    #[test]
    fn unknown_types_and_trailing_bytes_are_refused() {
        assert_eq!(Response::decode(&[99]), Err(WireError::UnknownType(99)));
        assert_eq!(
            Response::decode(&[103, 0, 0, 0, 1, 0, 0, 0, 0, 0xAA]),
            Err(WireError::Trailing(1))
        );
        assert_eq!(Response::decode(&[]), Err(WireError::Truncated));
    }

    #[test]
    fn status_codes_map_both_ways() {
        for code in 0..=9 {
            assert_eq!(StatusCode::from_u32(code).as_u32(), code);
        }
        assert_eq!(StatusCode::from_u32(9), StatusCode::Other(9));
    }
}
