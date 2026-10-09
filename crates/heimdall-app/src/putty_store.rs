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

//! Where `PuTTY` keeps its saved sessions, read and never written: the registry under
//! `HKEY_CURRENT_USER` on Windows, as the C# `WindowsPuttyRegistrySource` opens it read-only;
//! the files of `~/.putty/sessions` elsewhere. No store at all is no session.

use std::path::Path;

use heimdall_core::import::putty::RawSession;

/// The saved sessions, sorted by name as the C# lists them; or why they could not be read.
///
/// # Errors
///
/// The store's reason when it is there and cannot be read.
pub fn read() -> Result<Vec<RawSession>, String> {
    let mut sessions = read_store()?;
    sessions.sort_by_key(|session| session.encoded_name.to_lowercase());
    Ok(sessions)
}

#[cfg(windows)]
fn read_store() -> Result<Vec<RawSession>, String> {
    use heimdall_core::import::putty::{SESSIONS_KEY, Value};
    use windows_registry::{CURRENT_USER, Type};

    /// `HRESULT_FROM_WIN32(ERROR_FILE_NOT_FOUND)`: no such key.
    const NOT_FOUND: u32 = 0x8007_0002;

    let root = match CURRENT_USER.open(SESSIONS_KEY) {
        Ok(root) => root,
        Err(error) if error.code().0.cast_unsigned() == NOT_FOUND => return Ok(Vec::new()),
        Err(error) => return Err(error.to_string()),
    };
    let mut sessions = Vec::new();
    for name in root.keys().map_err(|error| error.to_string())? {
        let Ok(key) = root.open(&name) else {
            continue;
        };
        let values =
            key.values()
                .map_err(|error| error.to_string())?
                .filter_map(|(value_name, value)| {
                    let read = match value.ty() {
                        Type::U32 => u32::try_from(value).ok().map(Value::Number),
                        Type::String | Type::ExpandString => {
                            String::try_from(value).ok().map(Value::Text)
                        }
                        _ => None,
                    };
                    read.map(|read| (value_name, read))
                });
        sessions.push(RawSession::new(name, values));
    }
    Ok(sessions)
}

#[cfg(not(windows))]
fn read_store() -> Result<Vec<RawSession>, String> {
    use heimdall_core::import::putty::SESSIONS_FOLDER;

    let Some(folder) = heimdall_core::paths::home_dir().map(|home| home.join(SESSIONS_FOLDER))
    else {
        return Ok(Vec::new());
    };
    read_folder(&folder)
}

/// The session files of `folder`, each named as `PuTTY` names it; no folder is no session.
///
/// # Errors
///
/// The reason when the folder is there and cannot be listed. An entry that cannot be read
/// as text, a sub-folder included, is left out.
pub fn read_folder(folder: &Path) -> Result<Vec<RawSession>, String> {
    let entries = match std::fs::read_dir(folder) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(format!("{}: {error}", folder.display())),
    };
    Ok(entries
        .filter_map(Result::ok)
        .filter_map(|entry| {
            let text = std::fs::read_to_string(entry.path()).ok()?;
            Some(RawSession::from_file(
                entry.file_name().to_string_lossy().into_owned(),
                &text,
            ))
        })
        .collect())
}
