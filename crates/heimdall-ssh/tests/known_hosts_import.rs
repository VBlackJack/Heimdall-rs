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

//! Another `known_hosts` file imported as the C# Heimdall imports one: plainly named keys
//! read, the rest said, and a key that contradicts one trusted never written.

use std::path::Path;

use heimdall_ssh::KnownHosts;
use heimdall_ssh::known_hosts_import::{
    HostKeyNote, HostKeyStatus, HostKeysImported, assess, import, parse,
};

/// A host key fixture's `type base64` part.
fn key(name: &str) -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/hostkeys")
        .join(format!("{name}.pub"));
    let text = std::fs::read_to_string(path).expect("fixture");
    let mut fields = text.split_whitespace();
    format!(
        "{} {}",
        fields.next().expect("type"),
        fields.next().expect("key")
    )
}

#[test]
fn plain_hosts_and_ports_are_read_and_the_rest_is_said_by_line() {
    let ed = key("host-ed25519");
    let text = format!(
        "# comment\n\nweb.lab,[db.lab]:2222,10.0.0.5,fe80::1 {ed}\n|1|c2FsdA==|aGFzaA== {ed}\n*.lab {ed}\nbad:22 {ed}\n@cert-authority *.lab {ed}\n@revoked old.lab {ed}\nshort-line\nweird.lab ssh-foo AAAA\nnot.lab not-a-type AAAA\n"
    );
    let parsed = parse(&text);
    let hosts: Vec<(&str, u16, usize)> = parsed
        .candidates
        .iter()
        .map(|c| (c.host.as_str(), c.port, c.line))
        .collect();
    assert_eq!(
        hosts,
        [
            ("web.lab", 22, 3),
            ("db.lab", 2222, 3),
            ("10.0.0.5", 22, 3),
            ("fe80::1", 22, 3)
        ]
    );
    let said: Vec<(usize, HostKeyNote)> = parsed
        .diagnostics
        .iter()
        .map(|d| (d.line, d.note.clone()))
        .collect();
    assert_eq!(
        said,
        [
            (4, HostKeyNote::HashedHost),
            (5, HostKeyNote::HostPattern("*.lab".to_owned())),
            (6, HostKeyNote::HostPattern("bad:22".to_owned())),
            (7, HostKeyNote::CertAuthority),
            (8, HostKeyNote::Revoked),
            (9, HostKeyNote::Malformed("1 fields".to_owned())),
            (10, HostKeyNote::UnsupportedKey("ssh-foo".to_owned())),
            (11, HostKeyNote::Malformed("bad key".to_owned())),
        ]
    );
}

#[test]
fn an_overlong_line_is_malformed_and_host_names_are_lowercased() {
    let long = format!("{} {}\n", "a".repeat(70_000), key("host-ed25519"));
    let parsed = parse(&long);
    assert!(parsed.candidates.is_empty());
    assert_eq!(
        parsed.diagnostics[0].note,
        HostKeyNote::Malformed("line too long".to_owned())
    );
    let parsed = parse(&format!("Web.LAB {}\n", key("host-ed25519")));
    assert_eq!(parsed.candidates[0].host, "web.lab");
}

#[test]
fn a_key_is_new_trusted_already_or_in_conflict_and_only_new_ones_are_written() {
    let dir = tempfile::tempdir().expect("dir");
    let store = KnownHosts::new(dir.path().join("known_hosts"));
    let (ed, other, ecdsa) = (
        key("host-ed25519"),
        key("host-ed25519-other"),
        key("host-ecdsa"),
    );
    // Already trusted: web with ed; db with ed.
    let seed = parse(&format!("web.lab {ed}\ndb.lab {ed}\n"));
    import(&seed.candidates, &store).expect("seeded");

    let parsed = parse(&format!(
        "web.lab {ed}\ndb.lab {other}\nnew.lab {ed}\nweb.lab {ecdsa}\ntwice.lab {ed}\ntwice.lab {other}\n"
    ));
    let statuses = assess(&parsed.candidates, &store).expect("assessed");
    assert_eq!(
        statuses,
        [
            HostKeyStatus::Existing,
            HostKeyStatus::Conflict,
            HostKeyStatus::New,
            HostKeyStatus::New,
            HostKeyStatus::Conflict,
            HostKeyStatus::Conflict,
        ],
        "same key; a changed key; unknown; another kind for a known server; two keys in one file"
    );
    let done = import(&parsed.candidates, &store).expect("imported");
    assert_eq!(
        done,
        HostKeysImported {
            imported: 2,
            existing: 1,
            conflicts: 3
        }
    );
    let db = store.recorded("db.lab", 22).expect("read");
    assert_eq!(db.len(), 1, "the trusted key never written over");
    assert!(store.recorded("twice.lab", 22).expect("read").is_empty());
    assert_eq!(store.recorded("new.lab", 22).expect("read").len(), 1);
    assert_eq!(store.recorded("web.lab", 22).expect("read").len(), 2);
}

#[test]
fn the_same_key_twice_in_the_file_is_written_once() {
    let dir = tempfile::tempdir().expect("dir");
    let store = KnownHosts::new(dir.path().join("known_hosts"));
    let ed = key("host-ed25519");
    let parsed = parse(&format!("a.lab {ed}\na.lab {ed}\n"));
    let done = import(&parsed.candidates, &store).expect("imported");
    assert_eq!((done.imported, done.existing), (1, 1));
    assert_eq!(store.recorded("a.lab", 22).expect("read").len(), 1);
}
