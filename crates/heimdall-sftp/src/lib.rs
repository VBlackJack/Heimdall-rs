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

//! SFTP, FTP and FTPS file transfer.
//!
//! SFTP is spoken by an in-house version 3 client: paths stay the server's bytes, so a file
//! whose name is not UTF-8 can still be opened, renamed and deleted.

pub mod client;
pub mod local_name;
pub mod path;
pub mod protocol;
pub mod wire;

pub use client::{ClientConfig, Closed, DirEntry, Handle, Limits, SftpClient, SftpError};
pub use path::RemotePath;
