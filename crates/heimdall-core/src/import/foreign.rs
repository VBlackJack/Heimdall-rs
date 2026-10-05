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

//! What the `MobaXterm`, `mRemoteNG` and `RDCMan` files give, as the C# Heimdall reads them: each
//! session written as the C# `ServerProfileDto`, then converted by the C# document import, so
//! that ports, hosts and what is not supported are checked in one place.
//!
//! The C# gives each imported session a new identifier; so does the caller, through
//! [`Parsed::report`]: the identifiers written here only tell the sessions apart until then.

use std::collections::HashMap;

use serde_json::{Map, Value, json};

use super::csharp::{self, ImportReport};
use crate::profile::ProfileId;

/// Prefix of the identifier a session carries until it is given its own.
const PLACEHOLDER_ID: &str = "foreign-";

/// Something a whole file says, beyond its sessions.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FileWarning {
    /// The file cannot be read as its format; carries the reader's reason.
    Unreadable(String),
    /// An `mRemoteNG` file encrypted whole: it must be saved without encryption first.
    FullyEncrypted,
}

/// What a file gives.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Parsed {
    /// The sessions, as the C# `ServerProfileDto`, in the file's order.
    pub servers: Vec<Value>,
    /// What the file says as a whole.
    pub warnings: Vec<FileWarning>,
    /// Passwords the file stores, never read: counted to say they must be entered again.
    pub stored_credentials: usize,
}

impl Parsed {
    /// Adds a session: its C# fields, a placeholder identifier, and a direct connection, as
    /// every C# importer of these files sets it.
    pub(super) fn push(&mut self, mut fields: Map<String, Value>) {
        fields.insert(
            "id".to_owned(),
            Value::String(format!("{PLACEHOLDER_ID}{}", self.servers.len())),
        );
        fields.insert("useDirectConnection".to_owned(), Value::Bool(true));
        self.servers.push(Value::Object(fields));
    }

    /// The sessions converted as the C# document import converts them; `new_id` gives each
    /// one that is kept its own identifier, as the C# `Guid.NewGuid()`.
    #[must_use]
    pub fn report(&self, new_id: &mut dyn FnMut() -> String) -> ImportReport {
        let document = json!({ "servers": self.servers }).to_string();
        // The document is written here, of the shape the import reads: it cannot be refused.
        let mut report = csharp::import(&document, None).unwrap_or_default();
        let mut renamed: HashMap<ProfileId, ProfileId> = HashMap::new();
        let mut fresh = |id: &mut ProfileId| {
            let new = ProfileId::new(new_id());
            renamed.insert(std::mem::replace(id, new.clone()), new);
        };
        report.profiles.iter_mut().for_each(|p| fresh(&mut p.id));
        report.rdp.iter_mut().for_each(|p| fresh(&mut p.id));
        report.telnet.iter_mut().for_each(|p| fresh(&mut p.id));
        report.vnc.iter_mut().for_each(|p| fresh(&mut p.id));
        report.ftp.iter_mut().for_each(|p| fresh(&mut p.id));
        report.local.iter_mut().for_each(|p| fresh(&mut p.id));
        report.winrm.iter_mut().for_each(|p| fresh(&mut p.id));
        // What the sessions say of their servers follows them to their new identifiers.
        for (id, _) in &mut report.metadata {
            if let Some(new) = renamed.get(id) {
                id.clone_from(new);
            }
        }
        for id in &mut report.favorites {
            if let Some(new) = renamed.get(id) {
                id.clone_from(new);
            }
        }
        report
    }
}

/// A field of `fields` set to `value` when it says something.
pub(super) fn set_text(fields: &mut Map<String, Value>, key: &str, value: &str) {
    let value = value.trim();
    if !value.is_empty() {
        fields.insert(key.to_owned(), Value::String(value.to_owned()));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::metadata::{Environment, ProfileOrigin};

    #[test]
    fn what_a_session_says_and_its_origin_follow_it_to_its_new_identifier() {
        let mut parsed = Parsed::default();
        for (name, tags) in [("a", "web"), ("b", "")] {
            let mut fields = Map::new();
            fields.insert("displayName".to_owned(), Value::from(name));
            fields.insert(
                "remoteServer".to_owned(),
                Value::from(format!("{name}.lab")),
            );
            fields.insert("connectionType".to_owned(), Value::from("SSH"));
            fields.insert("tags".to_owned(), Value::from(tags));
            fields.insert("environment".to_owned(), Value::from("Lab"));
            fields.insert("isFavorite".to_owned(), Value::from(name == "a"));
            parsed.push(fields);
        }
        let mut next = 0;
        let mut report = parsed.report(&mut || {
            next += 1;
            format!("new-{next}")
        });
        report.stamp_origin(ProfileOrigin::MobaXterm);
        let ids: Vec<&str> = report.profiles.iter().map(|p| p.id.as_str()).collect();
        assert_eq!(ids, ["new-1", "new-2"]);
        assert_eq!(report.favorites, [ProfileId::new("new-1")]);
        let said: Vec<(&str, &str, Option<Environment>, Option<ProfileOrigin>)> = report
            .metadata
            .iter()
            .map(|(id, m)| (id.as_str(), m.tags.as_str(), m.environment, m.origin))
            .collect();
        assert_eq!(
            said,
            [
                (
                    "new-1",
                    "web",
                    Some(Environment::Lab),
                    Some(ProfileOrigin::MobaXterm)
                ),
                (
                    "new-2",
                    "",
                    Some(Environment::Lab),
                    Some(ProfileOrigin::MobaXterm)
                ),
            ]
        );
    }
}
