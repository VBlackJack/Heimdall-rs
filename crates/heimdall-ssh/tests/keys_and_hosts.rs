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

//! Key files and the `known_hosts` file, without a server.

mod common;

use common::{FIXTURE_PASSPHRASE, client_key_path, client_public_key, host_public_key};
use std::slice;

use heimdall_ssh::{
    KeyFile, KeyFileError, KeyFormat, KnownHosts, KnownHostsError, PublicKey, Secret, Verdict,
    validate_host, verdict,
};

fn secret(text: &str) -> Secret {
    Secret::new(text.to_owned())
}

/// Key material only: `PublicKey` equality includes the comment, which `known_hosts` drops.
fn key_data(keys: &[PublicKey]) -> Vec<russh::keys::ssh_key::public::KeyData> {
    keys.iter().map(|key| key.key_data().clone()).collect()
}

#[test]
fn formats_and_encryption_are_detected_from_the_text() {
    let cases = [
        ("ed25519-openssh", KeyFormat::OpenSsh, false),
        ("ed25519-openssh-encrypted", KeyFormat::OpenSsh, true),
        ("rsa-openssh", KeyFormat::OpenSsh, false),
        ("ed25519-ppk2.ppk", KeyFormat::Ppk, false),
        ("ed25519-ppk3.ppk", KeyFormat::Ppk, false),
        ("ed25519-ppk3-encrypted.ppk", KeyFormat::Ppk, true),
        ("rsa-ppk3.ppk", KeyFormat::Ppk, false),
    ];
    for (name, format, encrypted) in cases {
        let file = KeyFile::read(client_key_path(name)).expect(name);
        assert_eq!(file.format(), format, "{name}");
        assert_eq!(file.is_encrypted(), encrypted, "{name}");
    }
}

#[test]
fn the_public_key_is_read_without_decrypting() {
    let expected = client_public_key("ed25519-openssh");
    for name in [
        "ed25519-openssh",
        "ed25519-ppk3.ppk",
        "ed25519-ppk3-encrypted.ppk",
    ] {
        let file = KeyFile::read(client_key_path(name)).expect(name);
        assert_eq!(
            file.public_key().map(PublicKey::key_data),
            Some(expected.key_data()),
            "{name}"
        );
    }
    let encrypted = KeyFile::read(client_key_path("ed25519-openssh-encrypted")).expect("read");
    assert_eq!(
        encrypted.public_key().map(PublicKey::key_data),
        Some(client_public_key("ed25519-openssh-encrypted").key_data())
    );
}

#[test]
fn an_encrypted_key_needs_a_passphrase_and_rejects_a_wrong_one_in_both_formats() {
    for name in ["ed25519-openssh-encrypted", "ed25519-ppk3-encrypted.ppk"] {
        let file = KeyFile::read(client_key_path(name)).expect(name);
        assert!(
            matches!(
                file.decrypt(None),
                Err(KeyFileError::NeedsPassphrase { .. })
            ),
            "{name}"
        );
        assert!(
            matches!(
                file.decrypt(Some(&secret("wrong"))),
                Err(KeyFileError::WrongPassphrase { .. })
            ),
            "{name}"
        );
        assert!(
            file.decrypt(Some(&secret(FIXTURE_PASSPHRASE))).is_ok(),
            "{name}"
        );
    }
}

#[test]
fn a_file_that_is_not_a_key_is_refused() {
    let dir = tempfile::tempdir().expect("temp dir");
    let path = dir.path().join("notes.txt");
    std::fs::write(&path, "not a key").expect("write");
    assert!(matches!(
        KeyFile::read(&path),
        Err(KeyFileError::UnknownFormat { .. })
    ));
}

#[test]
fn host_names_are_normalised_and_unsafe_ones_refused() {
    assert_eq!(validate_host("  Web.LAB ").expect("valid"), "web.lab");
    assert_eq!(validate_host("fe80::1").expect("valid"), "fe80::1");
    for unsafe_host in [
        "", "a,b", "#host", "a b", "a\nb", "[h]", "|1|x", "*.lab", "@cert",
    ] {
        assert!(
            matches!(
                validate_host(unsafe_host),
                Err(KnownHostsError::InvalidHost)
            ),
            "{unsafe_host:?}"
        );
    }
}

#[test]
fn keys_are_recorded_per_port_and_case_insensitively() {
    let dir = tempfile::tempdir().expect("temp dir");
    let hosts = KnownHosts::new(dir.path().join("sub").join("known_hosts"));
    let key = host_public_key("host-ed25519");

    assert!(hosts.recorded("Web.Lab", 22).expect("empty").is_empty());
    hosts.learn("Web.Lab", 22, &key).expect("learn 22");
    hosts.learn("web.lab", 2222, &key).expect("learn 2222");

    let expected = key_data(slice::from_ref(&key));
    assert_eq!(
        key_data(&hosts.recorded("WEB.lab", 22).expect("read")),
        expected
    );
    assert_eq!(
        key_data(&hosts.recorded("web.lab", 2222).expect("read")),
        expected
    );
    assert!(hosts.recorded("web.lab", 2200).expect("read").is_empty());

    let text = std::fs::read_to_string(hosts.path()).expect("file");
    assert!(
        text.lines().any(|line| line.starts_with("web.lab ")),
        "{text}"
    );
    assert!(
        text.lines().any(|line| line.starts_with("[web.lab]:2222 ")),
        "{text}"
    );
}

#[test]
fn a_file_edited_with_windows_line_endings_is_still_read_or_refused_never_ignored() {
    let dir = tempfile::tempdir().expect("temp dir");
    let hosts = KnownHosts::new(dir.path().join("known_hosts"));
    let key = host_public_key("host-ed25519");
    hosts.learn("web.lab", 22, &key).expect("learn");
    let text = std::fs::read_to_string(hosts.path()).expect("file");
    std::fs::write(hosts.path(), text.replace('\n', "\r\n")).expect("crlf");

    match hosts.recorded("web.lab", 22) {
        Ok(keys) => assert_eq!(
            key_data(&keys),
            key_data(slice::from_ref(&key)),
            "a CRLF file must still match"
        ),
        Err(KnownHostsError::Corrupt { .. }) => {}
        Err(other) => panic!("unexpected {other:?}"),
    }
}

#[test]
fn the_verdict_table() {
    let ed25519 = host_public_key("host-ed25519");
    let other_ed25519 = host_public_key("host-ed25519-other");
    let ecdsa = host_public_key("host-ecdsa");

    assert_eq!(verdict(&[], &ed25519), Verdict::Unknown);
    assert_eq!(
        verdict(slice::from_ref(&ed25519), &ed25519),
        Verdict::Trusted
    );
    assert_eq!(
        verdict(&[ecdsa.clone(), ed25519.clone()], &ed25519),
        Verdict::Trusted
    );
    assert_eq!(
        verdict(slice::from_ref(&other_ed25519), &ed25519),
        Verdict::Changed {
            recorded: Box::new(other_ed25519)
        }
    );
    assert_eq!(
        verdict(slice::from_ref(&ecdsa), &ed25519),
        Verdict::OtherAlgorithm {
            recorded: vec![ecdsa.algorithm()]
        }
    );
}

#[test]
fn a_host_is_forgotten_on_its_port_only_and_other_lines_stay() {
    let dir = tempfile::tempdir().expect("temp dir");
    let hosts = KnownHosts::new(dir.path().join("known_hosts"));
    let key = host_public_key("host-ed25519");
    hosts.learn("web.lab", 22, &key).expect("learn 22");
    hosts.learn("web.lab", 2222, &key).expect("learn 2222");
    hosts.learn("db.lab", 2222, &key).expect("learn db");
    let comment = "# kept as it is";
    let text = std::fs::read_to_string(hosts.path()).expect("file");
    std::fs::write(hosts.path(), format!("{comment}\n{text}")).expect("comment");

    assert!(hosts.forget("Web.Lab", 2222).expect("forget"));
    assert!(hosts.recorded("web.lab", 2222).expect("read").is_empty());
    assert!(
        !hosts.recorded("web.lab", 22).expect("read").is_empty(),
        "port 22 stays"
    );
    assert!(
        !hosts.recorded("db.lab", 2222).expect("read").is_empty(),
        "another host stays"
    );
    let text = std::fs::read_to_string(hosts.path()).expect("file");
    assert!(text.starts_with(comment), "{text}");
    assert!(
        !hosts.forget("web.lab", 2222).expect("again"),
        "nothing left to remove"
    );
    assert!(hosts.forget("web.lab", 22).expect("forget 22"));
    assert!(hosts.recorded("web.lab", 22).expect("read").is_empty());

    let missing = KnownHosts::new(dir.path().join("none"));
    assert!(!missing.forget("web.lab", 22).expect("no file"));
}

#[test]
fn a_shared_line_loses_the_host_only_and_a_hashed_entry_is_left_to_a_person() {
    let dir = tempfile::tempdir().expect("temp dir");
    let hosts = KnownHosts::new(dir.path().join("known_hosts"));
    let key = host_public_key("host-ed25519");
    hosts.learn("web.lab", 22, &key).expect("learn");
    let text = std::fs::read_to_string(hosts.path()).expect("file");
    std::fs::write(
        hosts.path(),
        text.replacen("web.lab ", "web.lab,db.lab ", 1),
    )
    .expect("share");

    assert!(hosts.forget("web.lab", 22).expect("forget"));
    assert!(hosts.recorded("web.lab", 22).expect("read").is_empty());
    assert!(
        !hosts.recorded("db.lab", 22).expect("read").is_empty(),
        "the other host keeps it"
    );

    // web.lab as OpenSSH writes it with HashKnownHosts (HMAC-SHA1 of the name, salt 0..19):
    // its name cannot be read back, so the line stays and the host is not said forgotten.
    let hashed = "|1|AAECAwQFBgcICQoLDA0ODxAREhM=|wnuEyVNwLIbJJNdcuTFET3xTFkc=";
    let openssh = key.to_openssh().expect("openssh");
    std::fs::write(hosts.path(), format!("{hashed} {openssh}\n")).expect("hashed");
    assert!(
        !hosts.recorded("web.lab", 22).expect("read").is_empty(),
        "the hashed line names web.lab"
    );
    assert!(matches!(
        hosts.forget("web.lab", 22),
        Err(KnownHostsError::NotForgotten { .. })
    ));
}

#[test]
fn the_keys_trusted_are_listed_per_host_and_port_in_the_order_of_the_file() {
    use heimdall_ssh::{KnownHostEntry, fingerprint};

    let dir = tempfile::tempdir().expect("temp dir");
    let hosts = KnownHosts::new(dir.path().join("known_hosts"));
    assert_eq!(hosts.entries().expect("no file"), [], "nothing trusted yet");
    let ed25519 = host_public_key("host-ed25519");
    let ecdsa = host_public_key("host-ecdsa");
    hosts.learn("web.lab", 22, &ed25519).expect("learn");
    hosts.learn("db.lab", 2222, &ecdsa).expect("learn");
    let text = std::fs::read_to_string(hosts.path()).expect("file");
    let openssh = ed25519.to_openssh().expect("openssh");
    // A comment, a marked line, a hashed and a wildcard pattern, a line whose key cannot be
    // read, and a line shared by two hosts.
    std::fs::write(
        hosts.path(),
        format!(
            "# a comment\n{text}@cert-authority *.lab {openssh}\n\
             |1|AAECAwQFBgcICQoLDA0ODxAREhM=|wnuEyVNwLIbJJNdcuTFET3xTFkc= {openssh}\n\
             *.corp {openssh}\nbroken.lab ssh-ed25519 not-base64\n\
             a.lab,[b.lab]:2200 {openssh}\n"
        ),
    )
    .expect("write");
    let entry = |host: &str, port, key: &PublicKey| KnownHostEntry {
        host: host.to_owned(),
        port,
        algorithm: key.algorithm().to_string(),
        fingerprint: fingerprint(key),
        public_key: key
            .to_openssh()
            .expect("openssh")
            .split_whitespace()
            .nth(1)
            .map(str::to_owned),
        details: heimdall_ssh::HostKeyDetails::default(),
    };
    assert_eq!(
        hosts.entries().expect("read"),
        [
            entry("web.lab", 22, &ed25519),
            entry("db.lab", 2222, &ecdsa),
            entry("a.lab", 22, &ed25519),
            entry("b.lab", 2200, &ed25519),
        ]
    );
    assert_eq!(entry("web.lab", 22, &ed25519).algorithm, "ssh-ed25519");
}

#[test]
fn concurrent_writers_lose_no_key_and_no_pin_while_another_host_is_forgotten_over_and_over() {
    const WRITERS: usize = 6;
    const SERVERS_PER_WRITER: usize = 15;
    const FORGOTTEN_ROUNDS: usize = 40;

    let dir = tempfile::tempdir().expect("temp dir");
    let path = dir.path().join("known_hosts");
    let key = host_public_key("host-ed25519");
    let print = heimdall_ssh::fingerprint(&key);
    std::thread::scope(|scope| {
        for writer in 0..WRITERS {
            let (path, key, print) = (&path, &key, &print);
            scope.spawn(move || {
                let hosts = KnownHosts::new(path);
                let pins = heimdall_ssh::Pins::beside(path);
                for server in 0..SERVERS_PER_WRITER {
                    hosts
                        .learn(&format!("key-{writer}-{server}.lab"), 22, key)
                        .expect("learn");
                    assert!(
                        pins.pin(&format!("pin-{writer}-{server}.lab"), 22, print)
                            .expect("pin")
                    );
                }
            });
        }
        // Each forget rewrites both files whole: a line written between its read and its
        // write would be lost without the lock.
        let (path, key) = (&path, &key);
        scope.spawn(move || {
            let hosts = KnownHosts::new(path);
            let pins = heimdall_ssh::Pins::beside(path);
            for _ in 0..FORGOTTEN_ROUNDS {
                hosts.learn("gone.lab", 22, key).expect("learn");
                pins.pin("gone.lab", 2222, &heimdall_ssh::fingerprint(key))
                    .expect("pin");
                hosts.forget("gone.lab", 22).expect("forget");
                hosts.forget("gone.lab", 2222).expect("forget");
            }
        });
    });

    let hosts = KnownHosts::new(&path);
    let pins = heimdall_ssh::Pins::beside(&path);
    for writer in 0..WRITERS {
        for server in 0..SERVERS_PER_WRITER {
            let name = format!("key-{writer}-{server}.lab");
            assert_eq!(
                hosts.recorded(&name, 22).expect("read").len(),
                1,
                "{name} recorded once"
            );
            let name = format!("pin-{writer}-{server}.lab");
            assert_eq!(
                pins.pinned(&name, 22).expect("read"),
                slice::from_ref(&print),
                "{name} pinned"
            );
        }
    }
    assert!(hosts.recorded("gone.lab", 22).expect("read").is_empty());
    assert!(pins.pinned("gone.lab", 2222).expect("read").is_empty());
    let left: Vec<_> = std::fs::read_dir(dir.path())
        .expect("listed")
        .map(|entry| entry.expect("entry").file_name())
        .filter(|name| name.to_string_lossy().ends_with(".tmp"))
        .collect();
    assert!(left.is_empty(), "no file left beside them: {left:?}");
}
