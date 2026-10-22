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

//! The offline copy of a shared source, as the C# `SharedSessionSourceCache`: every time a
//! source is read, what it gave is kept in a private file of the application's data, so that
//! when the share is not reachable (a laptop off the network, a server down) its sessions can
//! still be opened, with a notice that they come from a copy.
//!
//! The C# keeps the shared file's bytes. Here the copy holds the sessions as they were
//! sanitized, and nothing else: a name, a folder, a protocol, a host, a port and an account
//! per session, and the report's findings. A password the shared file stores is never read,
//! so it cannot reach the copy, and what the copy holds is read back through the very checks
//! a file from the share goes through, as a copy on disk is no more trusted than that file.
//!
//! The copy is written like a transcript (`RestrictedLogFile`, PR #426): to the user alone,
//! atomically, in a folder restricted to the user, the Administrators and SYSTEM on Windows.
//! A copy that cannot be restricted is not written.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use heimdall_core::folder_acl;
use heimdall_core::shared_sessions::{MAX_SOURCE_FILE_BYTES, is_valid_source_id};
use heimdall_core::tools::private_file::write_private;
use serde_json::{Value, json};

use crate::shared_report::ImportReport;

/// The folder of the copies, beside the settings, as the C# `SharedSessionCache`.
pub const DIRECTORY_NAME: &str = "SharedSessionCache";

/// The version of the copy's layout; a copy of another version is ignored, as the C#'s.
/// 3 added the RD Gateway of an RDP session: a copy of version 2 may hold an RDP session that went
/// through one as a direct connection, so it is not read.
const FORMAT_VERSION: u64 = 3;

/// The extension of a copy.
const EXTENSION: &str = "json";

/// What a session of the copy says, before it is checked like one of the file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CachedSession {
    /// The folder the file filed it in.
    pub folder: Option<String>,
    /// Its name.
    pub name: String,
    /// `ssh` or `rdp`.
    pub protocol: String,
    /// Its host.
    pub host: String,
    /// Its port.
    pub port: u16,
    /// Its account, when the file names one.
    pub username: Option<String>,
    /// The SSH gateway it goes through, when the file names one.
    pub gateway: Option<CachedGateway>,
    /// The RD Gateway an RDP session goes through, when the file names one.
    pub rd_gateway: Option<String>,
}

/// A shared session's gateway as the copy keeps it: what the file said of it and nothing
/// else, no key and no password. Its identifier is derived again when the copy is read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CachedGateway {
    /// Its name.
    pub name: String,
    /// Its host.
    pub host: String,
    /// Its port.
    pub port: u16,
    /// The account it logs in as, when the file names one.
    pub username: Option<String>,
}

/// What a source gave when it was last read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CachedSource {
    /// The file it was read from, as the settings name it with its variables expanded.
    pub original_path: String,
    /// When it was read.
    pub taken_at: SystemTime,
    /// Its sessions, sanitized.
    pub sessions: Vec<CachedSession>,
    /// How many sessions of the file were refused.
    pub refused: usize,
    /// How many passwords the file stores, never read.
    pub ignored_passwords: usize,
    /// How many entries were not taken.
    pub skipped_entries: usize,
    /// The file held more than a source may.
    pub truncated: bool,
    /// What the read had to say of the file.
    pub report: ImportReport,
}

/// The folder of the copies.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceCache {
    directory: PathBuf,
}

impl SourceCache {
    /// The cache in `directory`; nothing is made until a copy is written.
    #[must_use]
    pub fn new(directory: PathBuf) -> Self {
        Self { directory }
    }

    /// The file of source `source_id`'s copy, or `None` for an identifier no source has.
    #[must_use]
    pub fn path_for(&self, source_id: &str) -> Option<PathBuf> {
        is_valid_source_id(source_id)
            .then(|| self.directory.join(format!("{source_id}.{EXTENSION}")))
    }

    /// Keeps `copy` as source `source_id`'s offline copy.
    ///
    /// # Errors
    ///
    /// The folder or the file could not be made private or written; the copy is then not kept.
    pub fn store(&self, source_id: &str, copy: &CachedSource) -> io::Result<()> {
        let path = self
            .path_for(source_id)
            .ok_or_else(|| io::Error::from(io::ErrorKind::InvalidInput))?;
        fs::create_dir_all(&self.directory)?;
        folder_acl::restrict(&self.directory).map_err(io::Error::other)?;
        let document = json!({
            "formatVersion": FORMAT_VERSION,
            "sourceId": source_id,
            "originalPath": copy.original_path,
            "takenAt": copy.taken_at.duration_since(UNIX_EPOCH).map_or(0, |age| age.as_secs()),
            "refused": copy.refused,
            "ignoredPasswords": copy.ignored_passwords,
            "skippedEntries": copy.skipped_entries,
            "truncated": copy.truncated,
            "report": copy.report.to_json(),
            "sessions": copy.sessions.iter().map(|session| json!({
                "folder": session.folder,
                "name": session.name,
                "protocol": session.protocol,
                "host": session.host,
                "port": session.port,
                "username": session.username,
                "gateway": session.gateway.as_ref().map(|gateway| json!({
                    "name": gateway.name,
                    "host": gateway.host,
                    "port": gateway.port,
                    "username": gateway.username,
                })),
                "rdGateway": session.rd_gateway,
            })).collect::<Vec<_>>(),
        });
        let bytes = serde_json::to_vec_pretty(&document).map_err(io::Error::other)?;
        write_private(&path, &bytes)
    }

    /// The copy of source `source_id`, or `None` when there is none, it is not one this build
    /// reads, or it is not the copy of that source.
    #[must_use]
    pub fn read(&self, source_id: &str) -> Option<CachedSource> {
        let path = self.path_for(source_id)?;
        let meta = fs::metadata(&path).ok()?;
        // Bounded as a shared file is: a copy is never larger than what it was made from.
        if !meta.is_file() || meta.len() > MAX_SOURCE_FILE_BYTES {
            return None;
        }
        let document: Value = serde_json::from_slice(&fs::read(&path).ok()?).ok()?;
        if document.get("formatVersion")?.as_u64()? != FORMAT_VERSION
            || document.get("sourceId")?.as_str()? != source_id
        {
            log::warn!("the offline copy of shared source {source_id} is not one this build reads");
            return None;
        }
        let count = |key: &str| {
            document
                .get(key)
                .and_then(Value::as_u64)
                .and_then(|value| usize::try_from(value).ok())
                .unwrap_or(0)
        };
        let text = |session: &Value, key: &str| {
            session.get(key).and_then(Value::as_str).map(str::to_owned)
        };
        let sessions = document
            .get("sessions")?
            .as_array()?
            .iter()
            .filter_map(|session| {
                // A gateway that cannot be read drops the session: it would otherwise be
                // taken as a direct one.
                let gateway = match session.get("gateway") {
                    None | Some(Value::Null) => None,
                    Some(gateway) => Some(CachedGateway {
                        name: text(gateway, "name")?,
                        host: text(gateway, "host")?,
                        port: u16::try_from(gateway.get("port")?.as_u64()?).ok()?,
                        username: text(gateway, "username"),
                    }),
                };
                // An RD Gateway that cannot be read drops the session, for the same reason. The
                // property is always written (null for a direct session), so a missing one, or
                // an empty text, is a copy that was not written by this build.
                let rd_gateway = match session.get("rdGateway")? {
                    Value::Null => None,
                    Value::String(host) if !host.trim().is_empty() => Some(host.clone()),
                    _ => return None,
                };
                Some(CachedSession {
                    folder: text(session, "folder"),
                    name: text(session, "name")?,
                    protocol: text(session, "protocol")?,
                    host: text(session, "host")?,
                    port: u16::try_from(session.get("port")?.as_u64()?).ok()?,
                    username: text(session, "username"),
                    gateway,
                    rd_gateway,
                })
            })
            .collect();
        Some(CachedSource {
            original_path: document.get("originalPath")?.as_str()?.to_owned(),
            taken_at: UNIX_EPOCH + Duration::from_secs(document.get("takenAt")?.as_u64()?),
            sessions,
            refused: count("refused"),
            ignored_passwords: count("ignoredPasswords"),
            skipped_entries: count("skippedEntries"),
            truncated: document
                .get("truncated")
                .and_then(Value::as_bool)
                .unwrap_or(false),
            report: document
                .get("report")
                .map(ImportReport::from_json)
                .unwrap_or_default(),
        })
    }

    /// Removes the copy of `source_id`; one that is not there is not an error.
    pub fn delete(&self, source_id: &str) {
        let Some(path) = self.path_for(source_id) else {
            return;
        };
        if let Err(error) = fs::remove_file(&path)
            && error.kind() != io::ErrorKind::NotFound
        {
            log::warn!(
                "the offline copy of shared source {source_id} could not be removed: {error}"
            );
        }
    }

    /// Removes the copies of every source but `kept`, as the C# `DeleteAllExcept`: a source
    /// taken out of the settings leaves nothing behind. Only the files this cache names are
    /// touched.
    pub fn delete_all_except(&self, kept: &[&str]) {
        let Ok(entries) = fs::read_dir(&self.directory) else {
            return;
        };
        for entry in entries.flatten() {
            if !entry.file_type().is_ok_and(|kind| kind.is_file()) {
                continue;
            }
            let path = entry.path();
            if path.extension().and_then(|ext| ext.to_str()) != Some(EXTENSION) {
                continue;
            }
            let Some(id) = path.file_stem().and_then(|stem| stem.to_str()) else {
                continue;
            };
            if is_valid_source_id(id) && !kept.contains(&id) {
                self.delete(id);
            }
        }
    }

    /// The folder of the copies.
    #[must_use]
    pub fn directory(&self) -> &Path {
        &self.directory
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn copy() -> CachedSource {
        CachedSource {
            original_path: "/srv/team.mxtsessions".to_owned(),
            taken_at: UNIX_EPOCH + Duration::from_secs(1_700_000_000),
            sessions: vec![CachedSession {
                folder: Some("Prod".to_owned()),
                name: "web01".to_owned(),
                protocol: "ssh".to_owned(),
                host: "web01.example.com".to_owned(),
                port: 22,
                username: Some("admin".to_owned()),
                gateway: Some(CachedGateway {
                    name: "jump@bastion".to_owned(),
                    host: "bastion.example.com".to_owned(),
                    port: 2222,
                    username: Some("jump".to_owned()),
                }),
                rd_gateway: Some("rdg.example.com".to_owned()),
            }],
            refused: 2,
            ignored_passwords: 1,
            skipped_entries: 3,
            truncated: true,
            report: ImportReport::default(),
        }
    }

    #[test]
    fn a_copy_is_read_back_as_it_was_stored() {
        let dir = tempfile::tempdir().expect("dir");
        let cache = SourceCache::new(dir.path().join(DIRECTORY_NAME));
        cache.store("team", &copy()).expect("stored");
        assert_eq!(cache.read("team"), Some(copy()));
    }

    #[test]
    fn a_copy_of_another_source_or_version_is_ignored() {
        let dir = tempfile::tempdir().expect("dir");
        let cache = SourceCache::new(dir.path().to_path_buf());
        cache.store("team", &copy()).expect("stored");
        let path = cache.path_for("team").expect("path");
        // Another source's copy under this one's name.
        fs::copy(&path, cache.path_for("other").expect("path")).expect("copied");
        assert_eq!(cache.read("other"), None);
        let text = fs::read_to_string(&path).expect("read");
        fs::write(
            &path,
            text.replace("\"formatVersion\": 3", "\"formatVersion\": 9"),
        )
        .expect("written");
        assert_eq!(cache.read("team"), None);
        // A version-1 copy was made when the gateways of a file were dropped: a session of it
        // may have crossed one, so it is not served.
        fs::write(
            &path,
            text.replace("\"formatVersion\": 3", "\"formatVersion\": 1"),
        )
        .expect("written");
        assert_eq!(cache.read("team"), None);
        fs::write(&path, "not json").expect("written");
        assert_eq!(cache.read("team"), None);
        assert_eq!(cache.read("missing"), None);
    }

    #[test]
    fn a_copy_with_a_missing_or_empty_rd_gateway_is_not_read_as_a_direct_session() {
        let dir = tempfile::tempdir().expect("dir");
        let cache = SourceCache::new(dir.path().join(DIRECTORY_NAME));
        cache.store("team", &copy()).expect("stored");
        let path = cache.path_for("team").expect("path");
        let text = fs::read_to_string(&path).expect("read");
        let key = "\"rdGateway\": \"rdg.example.com\"";
        assert!(text.contains(key), "{text}");
        for replacement in ["\"other\": 1", "\"rdGateway\": \"\"", "\"rdGateway\": 7"] {
            fs::write(&path, text.replace(key, replacement)).expect("written");
            let read = cache.read("team").expect("the copy itself");
            assert!(read.sessions.is_empty(), "{replacement}: session kept");
        }
        fs::write(&path, text.replace(key, "\"rdGateway\": null")).expect("written");
        assert_eq!(
            cache.read("team").expect("copy").sessions[0].rd_gateway,
            None
        );
        // A version-2 copy, written before the RD Gateway was kept, is not read.
        fs::write(
            &path,
            text.replace("\"formatVersion\": 3", "\"formatVersion\": 2"),
        )
        .expect("written");
        assert_eq!(cache.read("team"), None);
    }

    #[test]
    fn only_a_valid_source_identifier_names_a_file() {
        let cache = SourceCache::new(PathBuf::from("cache"));
        assert!(cache.path_for("team01").is_some());
        assert!(cache.path_for("../team").is_none());
        assert!(cache.path_for("Team").is_none());
        assert!(cache.path_for("").is_none());
    }

    #[test]
    fn a_source_taken_out_leaves_no_copy() {
        let dir = tempfile::tempdir().expect("dir");
        let cache = SourceCache::new(dir.path().to_path_buf());
        cache.store("keep", &copy()).expect("stored");
        cache.store("drop", &copy()).expect("stored");
        fs::write(dir.path().join("notes.txt"), "mine").expect("written");
        cache.delete_all_except(&["keep"]);
        assert!(cache.read("keep").is_some());
        assert!(cache.read("drop").is_none());
        assert!(dir.path().join("notes.txt").exists());
    }

    #[cfg(unix)]
    #[test]
    fn a_copy_is_readable_by_its_owner_alone() {
        use std::os::unix::fs::PermissionsExt as _;
        let dir = tempfile::tempdir().expect("dir");
        let cache = SourceCache::new(dir.path().to_path_buf());
        cache.store("team", &copy()).expect("stored");
        let mode = fs::metadata(cache.path_for("team").expect("path"))
            .expect("meta")
            .permissions()
            .mode();
        assert_eq!(mode & 0o077, 0);
    }
}
