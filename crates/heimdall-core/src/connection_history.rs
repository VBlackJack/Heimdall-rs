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

//! The connection history, as the C# `ConnectionHistory`: one JSON line for each session
//! opened or closed, appended to `connection-history.jsonl` in the log folder, for audit.
//! A line holds a UTC time, the action, the profile's id, its name and its protocol, and
//! nothing else: no host, no account, no password. The file is readable by the user alone
//! before a byte goes in it (on Windows, restricted to the user, the Administrators and
//! SYSTEM as the transcripts are), and past 1 MiB it moves to `connection-history.1.jsonl`,
//! the one before that dropped, so the two together stay under 2 MiB. Writing it must never
//! stop the application: a failure is logged, not raised.

use std::io::{self, Write};
use std::path::Path;
use std::sync::Mutex;
use std::time::SystemTime;

use serde::{Deserialize, Serialize};

use crate::tools::private_file::open_private_log;
use crate::utc::UtcTime;

/// Name of the history file, as the C# one.
pub const FILE_NAME: &str = "connection-history.jsonl";

/// Name the history file takes when it is full, as the C# one.
pub const ROTATED_FILE_NAME: &str = "connection-history.1.jsonl";

/// Size from which the file moves to the rotated name, as the C# 1 MiB.
pub const MAX_FILE_BYTES: u64 = 1_048_576;

/// What a line says happened.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    /// A session was opened.
    Connected,
    /// A session was closed.
    Disconnected,
}

impl Action {
    /// Its word in the file, as the C# writes it.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::Connected => "Connected",
            Self::Disconnected => "Disconnected",
        }
    }
}

/// A line of the file, in the C# camel case and order.
#[derive(Serialize, Deserialize, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Entry {
    /// When, UTC, ISO 8601.
    pub timestamp: String,
    /// `Connected` or `Disconnected`.
    pub action: String,
    /// The profile's id.
    pub server_id: String,
    /// The profile's name.
    pub display_name: String,
    /// The protocol, as the C# names it: SSH, RDP, SFTP...
    pub connection_type: String,
}

/// One writer at a time, so that a rotation cannot cut across a line being added.
static WRITE_LOCK: Mutex<()> = Mutex::new(());

/// Appends a line for `action` to the history in `dir`, made if need be; the file rotated
/// first when it has reached [`MAX_FILE_BYTES`].
///
/// # Errors
///
/// The error of the folder, the restriction, the rotation or the write.
pub fn record_in(
    dir: &Path,
    at: SystemTime,
    action: Action,
    server_id: &str,
    display_name: &str,
    connection_type: &str,
) -> io::Result<()> {
    let entry = Entry {
        timestamp: UtcTime::of(at).iso(),
        action: action.name().to_owned(),
        server_id: server_id.to_owned(),
        display_name: display_name.to_owned(),
        connection_type: connection_type.to_owned(),
    };
    let mut line = serde_json::to_string(&entry).map_err(io::Error::other)?;
    line.push('\n');
    let _held = WRITE_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    std::fs::create_dir_all(dir)?;
    let path = dir.join(FILE_NAME);
    if std::fs::metadata(&path).is_ok_and(|found| found.len() >= MAX_FILE_BYTES) {
        let rotated = dir.join(ROTATED_FILE_NAME);
        match std::fs::remove_file(&rotated) {
            Err(error) if error.kind() != io::ErrorKind::NotFound => return Err(error),
            _ => {}
        }
        std::fs::rename(&path, &rotated)?;
    }
    let mut file = open_private_log(&path)?;
    file.write_all(line.as_bytes())
}

/// [`record_in`] at the current time; a failure is logged and goes no further, as the C#
/// "logging must never crash the application".
pub fn record(
    dir: &Path,
    action: Action,
    server_id: &str,
    display_name: &str,
    connection_type: &str,
) {
    if let Err(error) = record_in(
        dir,
        SystemTime::now(),
        action,
        server_id,
        display_name,
        connection_type,
    ) {
        log::warn!("The connection history could not be written: {error}");
    }
}
