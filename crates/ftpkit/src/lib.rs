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

//! An async FTP client (RFC 959), with explicit FTPS (RFC 4217), passive data connections
//! (RFC 2428 `EPSV`, else `PASV`) and machine listings (RFC 3659 `MLSD`, else `LIST`).

pub mod client;
pub mod listing;
pub mod reply;
mod tls;

pub use client::{Fingerprint, FtpClient, FtpConfig, FtpError, Security};
pub use listing::{Entry, EntryKind};
pub use reply::Reply;
