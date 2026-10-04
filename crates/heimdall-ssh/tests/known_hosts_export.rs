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

//! The keys trusted, written into the user's OpenSSH `known_hosts`, as the C# export.

mod common;

use common::host_public_key;

use heimdall_ssh::{KnownHosts, KnownHostsExport, Pins, PublicKey, fingerprint};

/// A key as a `known_hosts` line writes it: algorithm and base64, no comment.
fn written(key: &PublicKey) -> String {
    let openssh = key.to_openssh().expect("openssh");
    let mut parts = openssh.split_whitespace();
    format!(
        "{} {}",
        parts.next().expect("algorithm"),
        parts.next().expect("base64")
    )
}

#[test]
fn the_keys_trusted_replace_their_lines_in_place_and_every_other_line_stays() {
    let dir = tempfile::tempdir().expect("temp dir");
    let hosts = KnownHosts::new(dir.path().join("heimdall").join("known_hosts"));
    let ed25519 = host_public_key("host-ed25519");
    let other = host_public_key("host-ed25519-other");
    let ecdsa = host_public_key("host-ecdsa");
    hosts.learn("web.lab", 22, &ed25519).expect("learn");
    hosts.learn("db.lab", 2222, &ecdsa).expect("learn");
    hosts.learn("new.lab", 22, &other).expect("learn");
    // Trusted by a fingerprint alone: no key to write.
    Pins::beside(hosts.path())
        .pin("pinned.lab", 22, &fingerprint(&ecdsa))
        .expect("pinned");

    let target = dir.path().join(".ssh").join("known_hosts");
    std::fs::create_dir_all(target.parent().expect("folder")).expect("folder");
    let stale = written(&other);
    let theirs = written(&ed25519);
    let before = format!(
        "# mine\n\
         web.lab {stale}\n\
         |1|AAECAwQFBgcICQoLDA0ODxAREhM=|wnuEyVNwLIbJJNdcuTFET3xTFkc= {stale}\n\
         *.corp {stale}\n\
         web.lab,elsewhere.lab {stale}\n\
         @cert-authority *.lab {stale}\n\
         [db.lab]:2222 {theirs} a comment\n\
         broken.lab ssh-ed25519 not-base64\n"
    );
    std::fs::write(&target, &before).expect("write");

    let report = hosts.export_to(&target).expect("exported");
    assert_eq!(
        report,
        KnownHostsExport {
            written: 3,
            preserved: 6,
            skipped: 1,
        }
    );
    let after = std::fs::read_to_string(&target).expect("read");
    assert_eq!(
        after,
        format!(
            "# mine\n\
             web.lab {}\n\
             |1|AAECAwQFBgcICQoLDA0ODxAREhM=|wnuEyVNwLIbJJNdcuTFET3xTFkc= {stale}\n\
             *.corp {stale}\n\
             web.lab,elsewhere.lab {stale}\n\
             @cert-authority *.lab {stale}\n\
             [db.lab]:2222 {}\n\
             broken.lab ssh-ed25519 not-base64\n\
             new.lab {}\n",
            written(&ed25519),
            written(&ecdsa),
            written(&other),
        ),
        "a line naming a host not trusted here, hashed, wildcard, marked or unreadable stays"
    );
    // Once more: nothing moves.
    let again = hosts.export_to(&target).expect("again");
    assert_eq!(again.written, 3);
    assert_eq!(std::fs::read_to_string(&target).expect("read"), after);
    let leftovers: Vec<_> = std::fs::read_dir(target.parent().expect("folder"))
        .expect("listed")
        .filter_map(Result::ok)
        .filter(|entry| entry.file_name() != "known_hosts")
        .collect();
    assert!(
        leftovers.is_empty(),
        "no file left beside it: {leftovers:?}"
    );
}

#[test]
fn a_missing_file_and_folder_are_created_and_nothing_trusted_writes_nothing() {
    let dir = tempfile::tempdir().expect("temp dir");
    let hosts = KnownHosts::new(dir.path().join("known_hosts"));
    let target = dir.path().join("home").join(".ssh").join("known_hosts");
    assert_eq!(
        hosts.export_to(&target).expect("nothing"),
        KnownHostsExport::default()
    );
    assert!(!target.exists(), "nothing trusted: the file is not touched");

    let key = host_public_key("host-ed25519");
    hosts.learn("web.lab", 2200, &key).expect("learn");
    let report = hosts.export_to(&target).expect("exported");
    assert_eq!((report.written, report.preserved), (1, 0));
    assert_eq!(
        std::fs::read_to_string(&target).expect("read"),
        format!("[web.lab]:2200 {}\n", written(&key))
    );
}
