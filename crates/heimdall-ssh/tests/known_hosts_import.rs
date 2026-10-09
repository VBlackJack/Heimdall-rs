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
    HostKeyNote, HostKeyStatus, HostKeysImported, Malformed, assess, import, parse,
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
            (9, HostKeyNote::Malformed(Malformed::Fields(1))),
            (10, HostKeyNote::UnsupportedKey("ssh-foo".to_owned())),
            (11, HostKeyNote::Malformed(Malformed::BadKey)),
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
        HostKeyNote::Malformed(Malformed::TooLong)
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

/// The fingerprint of a fixture's key.
fn print(name: &str) -> String {
    heimdall_ssh::fingerprint(&heimdall_ssh::PublicKey::from_openssh(&key(name)).expect("key"))
}

#[test]
fn a_key_other_than_the_one_pinned_is_a_conflict_and_neither_file_changes() {
    let dir = tempfile::tempdir().expect("dir");
    let store = KnownHosts::new(dir.path().join("known_hosts"));
    let pins = heimdall_ssh::Pins::beside(store.path());
    // Trusted by its fingerprint alone, as the C# kept it: the ed25519 key, never the other.
    pins.pin("web.lab", 22, &print("host-ed25519"))
        .expect("pinned");
    store
        .learn(
            "seed.lab",
            22,
            &heimdall_ssh::PublicKey::from_openssh(&key("host-ecdsa")).expect("key"),
        )
        .expect("seeded");
    let (hosts_before, pins_before) = (
        std::fs::read(store.path()).expect("read"),
        std::fs::read(pins.path()).expect("read"),
    );

    let parsed = parse(&format!(
        "web.lab {}\nweb.lab {}\n",
        key("host-ed25519-other"),
        key("host-ecdsa")
    ));
    assert_eq!(
        assess(&parsed.candidates, &store).expect("assessed"),
        [HostKeyStatus::Conflict, HostKeyStatus::Conflict],
        "a key of the pinned kind, then one of another kind: neither has the pinned fingerprint"
    );
    let done = import(&parsed.candidates, &store).expect("imported");
    assert_eq!(
        done,
        HostKeysImported {
            imported: 0,
            existing: 0,
            conflicts: 2
        }
    );
    assert!(store.recorded("web.lab", 22).expect("read").is_empty());
    assert_eq!(
        std::fs::read(store.path()).expect("read"),
        hosts_before,
        "known_hosts unchanged"
    );
    assert_eq!(
        std::fs::read(pins.path()).expect("read"),
        pins_before,
        "the pin unchanged"
    );
}

#[test]
fn the_key_pinned_is_recorded_in_full_and_its_pin_dropped_as_a_connection_does() {
    let dir = tempfile::tempdir().expect("dir");
    let store = KnownHosts::new(dir.path().join("known_hosts"));
    let pins = heimdall_ssh::Pins::beside(store.path());
    pins.pin("web.lab", 2222, &print("host-ed25519"))
        .expect("pinned");
    pins.pin("db.lab", 22, &print("host-ed25519"))
        .expect("pinned");

    let parsed = parse(&format!("[web.lab]:2222 {}\n", key("host-ed25519")));
    assert_eq!(
        assess(&parsed.candidates, &store).expect("assessed"),
        [HostKeyStatus::New]
    );
    let done = import(&parsed.candidates, &store).expect("imported");
    assert_eq!(
        done,
        HostKeysImported {
            imported: 1,
            existing: 0,
            conflicts: 0
        }
    );
    let recorded = store.recorded("web.lab", 2222).expect("read");
    assert_eq!(recorded.len(), 1);
    assert_eq!(
        heimdall_ssh::fingerprint(&recorded[0]),
        print("host-ed25519")
    );
    assert!(
        pins.pinned("web.lab", 2222).expect("read").is_empty(),
        "the pin goes once the key is recorded"
    );
    assert_eq!(
        pins.pinned("db.lab", 22).expect("read"),
        [print("host-ed25519")],
        "another server's pin stays"
    );
    // Imported again, the key is trusted already.
    assert_eq!(
        assess(&parsed.candidates, &store).expect("assessed"),
        [HostKeyStatus::Existing]
    );
}

#[test]
fn one_rule_decides_every_import_against_the_keys_recorded_and_the_pins() {
    use heimdall_ssh::known_hosts_import::{Contradiction, OtherAlgorithm, Trusting, trusting};

    let read = |name: &str| heimdall_ssh::PublicKey::from_openssh(&key(name)).expect("key");
    let (ed, other, ecdsa) = (
        read("host-ed25519"),
        read("host-ed25519-other"),
        read("host-ecdsa"),
    );
    let pin_ed = [print("host-ed25519")];
    let none: [String; 0] = [];
    let cases = [
        // Nothing recorded, nothing pinned.
        (
            vec![],
            &none[..],
            &ed,
            OtherAlgorithm::Adds,
            Trusting::Learn,
        ),
        // Recorded already, whatever the pins say.
        (
            vec![ed.clone()],
            &pin_ed[..],
            &ed,
            OtherAlgorithm::Conflicts,
            Trusting::Recorded,
        ),
        // Another key of its algorithm recorded.
        (
            vec![ed.clone()],
            &none[..],
            &other,
            OtherAlgorithm::Adds,
            Trusting::Conflict(Contradiction::Changed(print("host-ed25519"))),
        ),
        // Another algorithm recorded: as the caller says.
        (
            vec![ed.clone()],
            &none[..],
            &ecdsa,
            OtherAlgorithm::Adds,
            Trusting::Learn,
        ),
        (
            vec![ed.clone()],
            &none[..],
            &ecdsa,
            OtherAlgorithm::Conflicts,
            Trusting::Conflict(Contradiction::OtherAlgorithm(vec![ed.algorithm()])),
        ),
        // Pinned: its key recorded in full, any other refused, of any algorithm.
        (
            vec![],
            &pin_ed[..],
            &ed,
            OtherAlgorithm::Adds,
            Trusting::LearnPinned,
        ),
        (
            vec![],
            &pin_ed[..],
            &other,
            OtherAlgorithm::Adds,
            Trusting::Conflict(Contradiction::Pinned(print("host-ed25519"))),
        ),
        (
            vec![],
            &pin_ed[..],
            &ecdsa,
            OtherAlgorithm::Adds,
            Trusting::Conflict(Contradiction::Pinned(print("host-ed25519"))),
        ),
        // A key of another algorithm recorded beside a pin still answers to the pin.
        (
            vec![ecdsa.clone()],
            &pin_ed[..],
            &other,
            OtherAlgorithm::Adds,
            Trusting::Conflict(Contradiction::Pinned(print("host-ed25519"))),
        ),
    ];
    for (recorded, pinned, offered, rule, expected) in cases {
        assert_eq!(
            trusting(&recorded, pinned, offered, rule),
            expected,
            "{} against {} recorded and {pinned:?}, {rule:?}",
            offered.algorithm(),
            recorded.len()
        );
    }
}

#[test]
fn a_key_trusted_through_the_one_rule_is_written_once_under_the_lock() {
    use heimdall_ssh::known_hosts_import::{Contradiction, OtherAlgorithm, Trusting, trust};

    let dir = tempfile::tempdir().expect("dir");
    let store = KnownHosts::new(dir.path().join("known_hosts"));
    let pins = heimdall_ssh::Pins::beside(store.path());
    let read = |name: &str| heimdall_ssh::PublicKey::from_openssh(&key(name)).expect("key");
    pins.pin("web.lab", 22, &print("host-ed25519"))
        .expect("pinned");
    let rule = OtherAlgorithm::Conflicts;
    let user = heimdall_ssh::HostKeySource::User;
    assert_eq!(
        trust(&store, "web.lab", 22, &read("host-ecdsa"), rule, user).expect("trust"),
        Trusting::Conflict(Contradiction::Pinned(print("host-ed25519")))
    );
    assert_eq!(
        trust(&store, "web.lab", 22, &read("host-ed25519"), rule, user).expect("trust"),
        Trusting::LearnPinned
    );
    assert_eq!(
        trust(&store, "web.lab", 22, &read("host-ed25519"), rule, user).expect("trust"),
        Trusting::Recorded
    );
    assert_eq!(
        trust(&store, "new.lab", 22, &read("host-ecdsa"), rule, user).expect("trust"),
        Trusting::Learn
    );
    assert_eq!(store.recorded("web.lab", 22).expect("read").len(), 1);
    assert_eq!(store.recorded("new.lab", 22).expect("read").len(), 1);
    assert!(pins.pinned("web.lab", 22).expect("read").is_empty());

    // Each key recorded is the user's, first and last seen when it was trusted.
    for entry in store.entries().expect("listed") {
        let details = entry.details;
        assert_eq!(details.source, user, "{}", entry.host);
        assert!(details.first_seen.is_some(), "{}", entry.host);
        assert_eq!(details.first_seen, details.last_seen, "{}", entry.host);
    }
}

#[test]
fn a_key_imported_from_a_file_is_said_imported_and_a_key_already_there_keeps_its_details() {
    use heimdall_ssh::HostKeySource;

    let dir = tempfile::tempdir().expect("dir");
    let store = KnownHosts::new(dir.path().join("known_hosts"));
    let ed = key("host-ed25519");
    // Trusted before the details were kept: unknown, never given invented dates.
    store
        .learn(
            "old.lab",
            22,
            &heimdall_ssh::PublicKey::from_openssh(&ed).expect("key"),
        )
        .expect("learn");
    let parsed = parse(&format!(
        "old.lab {ed}
new.lab {ed}
"
    ));
    import(&parsed.candidates, &store).expect("imported");
    let listed = store.entries().expect("listed");
    let of = |host: &str| {
        listed
            .iter()
            .find(|entry| entry.host == host)
            .map(|entry| entry.details)
            .expect(host)
    };
    assert_eq!(of("old.lab"), heimdall_ssh::HostKeyDetails::default());
    assert_eq!(of("new.lab").source, HostKeySource::Imported);
    assert!(of("new.lab").first_seen.is_some());
    assert!(
        listed.iter().all(|entry| entry.public_key.is_some()),
        "each key's base64 read from known_hosts"
    );
}

/// A fixture's key, read.
fn read(name: &str) -> heimdall_ssh::PublicKey {
    heimdall_ssh::PublicKey::from_openssh(&key(name)).expect("key")
}

/// The fingerprints `store` records for `host` on 22.
fn recorded(store: &KnownHosts, host: &str) -> Vec<String> {
    store
        .recorded(host, 22)
        .expect("read")
        .iter()
        .map(heimdall_ssh::fingerprint)
        .collect()
}

#[test]
fn at_startup_a_new_server_learns_every_algorithm_said_imported() {
    use heimdall_ssh::HostKeySource;
    use heimdall_ssh::known_hosts_import::{HostKeysSynced, sync};

    let dir = tempfile::tempdir().expect("dir");
    let store = KnownHosts::new(dir.path().join("known_hosts"));
    let parsed = parse(&format!(
        "web.lab {}\nweb.lab {}\n|1|c2FsdA==|aGFzaA== {}\nweb.lab {}\n",
        key("host-ed25519"),
        key("host-ecdsa"),
        key("host-ed25519-other"),
        key("host-ed25519"),
    ));
    assert_eq!(
        parsed.diagnostics.first().map(|d| &d.note),
        Some(&HostKeyNote::HashedHost),
        "a hashed line is left out"
    );
    let done = sync(&parsed.candidates, &store).expect("synced");
    assert_eq!(
        done,
        HostKeysSynced {
            imported: 2,
            matched: 1,
            conflicts: Vec::new(),
        },
        "the same key twice is learnt once"
    );
    assert_eq!(
        recorded(&store, "web.lab"),
        [print("host-ed25519"), print("host-ecdsa")]
    );
    for entry in store.entries().expect("listed") {
        assert_eq!(entry.details.source, HostKeySource::Imported, "{entry:?}");
        assert!(entry.details.first_seen.is_some());
    }
}

#[test]
fn at_startup_a_file_contradicting_itself_teaches_nothing_of_that_server() {
    use heimdall_ssh::known_hosts_import::sync;

    let dir = tempfile::tempdir().expect("dir");
    let store = KnownHosts::new(dir.path().join("known_hosts"));
    let parsed = parse(&format!(
        "web.lab {}\nweb.lab {}\nweb.lab {}\ndb.lab {}\n",
        key("host-ed25519"),
        key("host-ecdsa"),
        key("host-ed25519-other"),
        key("host-ed25519"),
    ));
    let done = sync(&parsed.candidates, &store).expect("synced");
    assert_eq!((done.imported, done.matched), (1, 0), "db.lab alone");
    assert!(
        recorded(&store, "web.lab").is_empty(),
        "web.lab refused whole"
    );
    assert_eq!(recorded(&store, "db.lab"), [print("host-ed25519")]);
    let said: Vec<(usize, String, String, bool)> = done
        .conflicts
        .iter()
        .map(|c| {
            (
                c.line,
                c.existing.clone(),
                c.imported.clone(),
                c.within_file,
            )
        })
        .collect();
    assert_eq!(
        said,
        [
            (1, print("host-ed25519-other"), print("host-ed25519"), true),
            (2, print("host-ed25519"), print("host-ecdsa"), true),
            (3, print("host-ed25519"), print("host-ed25519-other"), true),
        ],
        "each key with what contradicts it"
    );
}

#[test]
fn at_startup_a_known_server_matches_its_key_and_takes_no_other_of_any_algorithm() {
    use heimdall_ssh::HostKeySource;
    use heimdall_ssh::known_hosts_import::sync;

    let dir = tempfile::tempdir().expect("dir");
    let store = KnownHosts::new(dir.path().join("known_hosts"));
    // Trusted before the details were kept: no source, no dates.
    store
        .learn("web.lab", 22, &read("host-ed25519"))
        .expect("learn");
    let before = store.entries().expect("listed")[0].details;
    assert_eq!(before.last_seen, None);

    let parsed = parse(&format!(
        "web.lab {}\nweb.lab {}\nweb.lab {}\n",
        key("host-ed25519"),
        key("host-ecdsa"),
        key("host-ed25519-other"),
    ));
    let done = sync(&parsed.candidates, &store).expect("synced");
    assert_eq!((done.imported, done.matched), (0, 1));
    assert_eq!(
        recorded(&store, "web.lab"),
        [print("host-ed25519")],
        "never added: the ECDSA key contradicts the server's trust"
    );
    let said: Vec<(usize, String, String, bool)> = done
        .conflicts
        .iter()
        .map(|c| {
            (
                c.line,
                c.existing.clone(),
                c.imported.clone(),
                c.within_file,
            )
        })
        .collect();
    assert_eq!(
        said,
        [
            (2, print("host-ed25519"), print("host-ecdsa"), false),
            (3, print("host-ed25519"), print("host-ed25519-other"), false),
        ]
    );
    let after = store.entries().expect("listed")[0].details;
    assert!(after.last_seen.is_some(), "the match raises its last seen");
    assert_eq!(
        after.source,
        HostKeySource::Unknown,
        "a match is not said imported"
    );
    assert_eq!(after.first_seen, None, "never an invented first date");
}

#[test]
fn at_startup_a_pinned_server_takes_its_pinned_key_in_full_and_nothing_else() {
    use heimdall_ssh::known_hosts_import::sync;

    let dir = tempfile::tempdir().expect("dir");
    let store = KnownHosts::new(dir.path().join("known_hosts"));
    let pins = heimdall_ssh::Pins::beside(store.path());
    pins.pin("web.lab", 22, &print("host-ed25519"))
        .expect("pinned");
    pins.pin("db.lab", 22, &print("host-ed25519"))
        .expect("pinned");

    let parsed = parse(&format!(
        "web.lab {}\nweb.lab {}\nweb.lab {}\ndb.lab {}\n",
        key("host-ecdsa"),
        key("host-ed25519"),
        key("host-ed25519-other"),
        key("host-ed25519-other"),
    ));
    let done = sync(&parsed.candidates, &store).expect("synced");
    assert_eq!((done.imported, done.matched), (1, 0));
    assert_eq!(recorded(&store, "web.lab"), [print("host-ed25519")]);
    assert!(
        pins.pinned("web.lab", 22).expect("read").is_empty(),
        "recorded in full, unpinned"
    );
    assert!(recorded(&store, "db.lab").is_empty());
    assert_eq!(
        pins.pinned("db.lab", 22).expect("read"),
        [print("host-ed25519")],
        "another key: the pin stays"
    );
    let said: Vec<(usize, String, bool)> = done
        .conflicts
        .iter()
        .map(|c| (c.line, c.existing.clone(), c.within_file))
        .collect();
    assert_eq!(
        said,
        [
            (1, print("host-ed25519"), false),
            (3, print("host-ed25519"), false),
            (4, print("host-ed25519"), false),
        ],
        "the pin, then the key recorded from it, contradict the others"
    );
}
