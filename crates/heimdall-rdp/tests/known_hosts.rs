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

//! The file of trusted RDP servers, listed as the Settings page shows it.

use heimdall_rdp::{Fingerprint, KnownRdpHost, KnownRdpHosts};

/// SHA-256 of the fixture's `SubjectPublicKeyInfo`, computed by openssl.
const PIN: &str = include_str!("fixtures/server-spki-sha256.txt");

fn pin() -> Fingerprint {
    format!("SHA256:{}", PIN.trim()).parse().expect("pin")
}

#[test]
fn the_servers_trusted_are_listed_with_their_key_in_the_order_of_the_file() {
    let dir = tempfile::tempdir().expect("dir");
    let known = KnownRdpHosts::new(dir.path().join("known_rdp_hosts"));
    assert_eq!(known.entries().expect("no file"), [], "nothing trusted yet");
    known.record("DC.lab", 3389, &pin()).expect("record");
    known.record("::1", 3390, &pin()).expect("record");
    let text = std::fs::read_to_string(known.path()).expect("file");
    // A comment, a line without a key, one with a key that does not parse, one without a
    // port, two without a host: none trusts anything.
    std::fs::write(
        known.path(),
        format!(
            "# kept\n{text}web.lab:3389\nweb.lab:3389 SHA256:not-a-pin\nweb.lab {pin}\n\
             :3389 {pin}\n[]:3389 {pin}\n",
            pin = pin()
        ),
    )
    .expect("write");
    let entry = |host: &str, port| KnownRdpHost {
        host: host.to_owned(),
        port,
        fingerprint: pin(),
    };
    assert_eq!(
        known.entries().expect("read"),
        [entry("dc.lab", 3389), entry("::1", 3390)],
        "in lower case, an IPv6 host out of its brackets"
    );
    assert!(known.forget("::1", 3390).expect("forget"));
    assert_eq!(known.entries().expect("read"), [entry("dc.lab", 3389)]);
}

#[test]
fn one_certificate_is_forgotten_and_another_trusted_for_the_same_server_stays() {
    let dir = tempfile::tempdir().expect("dir");
    let known = KnownRdpHosts::new(dir.path().join("known_rdp_hosts"));
    let other: Fingerprint = format!("SHA256:{}", "A".repeat(43)).parse().expect("other");
    known.record("dc.lab", 3389, &pin()).expect("record");
    known.record("dc.lab", 3389, &other).expect("record");
    known.record("web.lab", 3389, &pin()).expect("record");
    assert!(
        !known
            .forget_key("dc.lab", 3390, &pin())
            .expect("another port")
    );
    assert!(known.forget_key("DC.lab", 3389, &pin()).expect("forget"));
    let left: Vec<(String, Fingerprint)> = known
        .entries()
        .expect("read")
        .into_iter()
        .map(|entry| (entry.host, entry.fingerprint))
        .collect();
    assert_eq!(
        left,
        [("dc.lab".to_owned(), other), ("web.lab".to_owned(), pin())]
    );
    assert!(!known.forget_key("dc.lab", 3389, &pin()).expect("gone"));
}
