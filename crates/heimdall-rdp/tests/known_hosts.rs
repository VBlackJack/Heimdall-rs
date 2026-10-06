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

use std::time::{Duration, SystemTime, UNIX_EPOCH};

use heimdall_rdp::{Fingerprint, KnownRdpHosts, ServerCertificate, Verdict};

/// SHA-256 of the fixture's `SubjectPublicKeyInfo`, computed by openssl.
const PIN: &str = include_str!("fixtures/server-spki-sha256.txt");

/// A certificate for `CN=dc.lab` issued by `O=Heimdall Lab, CN=Lab Root CA`, made by openssl
/// (`req -x509` for the authority, then `x509 -req` for the server, P-256 keys).
const ISSUED: &[u8] = include_bytes!("fixtures/issued-cert.der");

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
    let entry = |host: &str, port| (host.to_owned(), port, pin());
    let servers = || -> Vec<(String, u16, Fingerprint)> {
        known
            .entries()
            .expect("read")
            .into_iter()
            .map(|entry| (entry.host, entry.port, entry.fingerprint))
            .collect()
    };
    assert_eq!(
        servers(),
        [entry("dc.lab", 3389), entry("::1", 3390)],
        "in lower case, an IPv6 host out of its brackets"
    );
    assert!(known.forget("::1", 3390).expect("forget"));
    assert_eq!(servers(), [entry("dc.lab", 3389)]);
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

/// `time` to the second, as the file keeps it.
fn whole_seconds(time: SystemTime) -> SystemTime {
    let since = time.duration_since(UNIX_EPOCH).expect("after 1970");
    UNIX_EPOCH + Duration::from_secs(since.as_secs())
}

#[test]
fn a_certificate_is_recorded_with_its_names_and_the_time_and_read_back() {
    let dir = tempfile::tempdir().expect("dir");
    let known = KnownRdpHosts::new(dir.path().join("known_rdp_hosts"));
    let certificate = ServerCertificate::from_der(ISSUED).expect("fixture");
    let before = whole_seconds(SystemTime::now());
    known
        .record_certificate("DC.lab", 3389, &certificate)
        .expect("record");
    let after = SystemTime::now();
    let [entry] = known
        .entries()
        .expect("read")
        .try_into()
        .expect("one entry");
    assert_eq!(
        (entry.host.as_str(), entry.port, entry.fingerprint),
        ("dc.lab", 3389, certificate.fingerprint)
    );
    assert_eq!(entry.subject.as_deref(), Some("CN=dc.lab"));
    assert_eq!(
        entry.issuer.as_deref(),
        Some("CN=Lab Root CA,O=Heimdall Lab")
    );
    let trusted = entry.trusted.expect("the time");
    assert!((before..=after).contains(&trusted), "{trusted:?}");
    assert_eq!(
        known
            .verdict("dc.lab", 3389, &certificate.fingerprint)
            .expect("verdict"),
        Verdict::Known
    );

    // A reader of the first two fields alone, as older builds, reads the new line as before.
    let text = std::fs::read_to_string(known.path()).expect("file");
    let mut fields = text.lines().next().expect("a line").split_whitespace();
    assert_eq!(fields.next(), Some("dc.lab:3389"));
    assert_eq!(
        fields.next().expect("the key").parse::<Fingerprint>(),
        Ok(certificate.fingerprint)
    );
}

#[test]
fn a_line_of_two_fields_reads_and_decides_as_before() {
    let dir = tempfile::tempdir().expect("dir");
    let known = KnownRdpHosts::new(dir.path().join("known_rdp_hosts"));
    std::fs::write(known.path(), format!("dc.lab:3389 {}\n", pin())).expect("write");
    let [entry] = known
        .entries()
        .expect("read")
        .try_into()
        .expect("one entry");
    assert_eq!((entry.host.as_str(), entry.fingerprint), ("dc.lab", pin()));
    assert_eq!(
        (entry.subject, entry.issuer, entry.trusted),
        (None, None, None),
        "nothing recorded, nothing shown"
    );
    let other: Fingerprint = format!("SHA256:{}", "A".repeat(43)).parse().expect("other");
    assert!(known.knows("dc.lab", 3389).expect("knows"));
    assert_eq!(
        known.verdict("dc.lab", 3389, &pin()).expect("verdict"),
        Verdict::Known
    );
    assert_eq!(
        known.verdict("dc.lab", 3389, &other).expect("verdict"),
        Verdict::Changed { recorded: pin() }
    );
}

#[test]
fn forgetting_one_key_keeps_every_other_line_and_its_attributes_byte_for_byte() {
    let dir = tempfile::tempdir().expect("dir");
    let known = KnownRdpHosts::new(dir.path().join("known_rdp_hosts"));
    let other: Fingerprint = format!("SHA256:{}", "A".repeat(43)).parse().expect("other");
    let pin = pin();
    // Attributes unknown to this build, one that does not decode, a comment, a line that
    // does not parse: all kept as they are.
    let kept = format!(
        "# kept\ndc.lab:3389 {other} trusted=1767225601 subject=Q049ZGMubGFi colour=blue\n\
         web.lab:3389 {pin} subject=!!! future=x=y\nnot a line\n"
    );
    std::fs::write(
        known.path(),
        format!("dc.lab:3389 {pin} trusted=1767225601 subject=Q049ZGMubGFi\n{kept}"),
    )
    .expect("write");
    let entries = known.entries().expect("read");
    assert_eq!(entries[1].subject.as_deref(), Some("CN=dc.lab"));
    assert_eq!(
        entries[1].trusted,
        Some(UNIX_EPOCH + Duration::from_secs(1_767_225_601))
    );
    assert_eq!(
        (entries[2].subject.as_deref(), entries[2].trusted),
        (None, None),
        "an attribute that does not decode is ignored, the line still trusts"
    );
    assert!(known.forget_key("dc.lab", 3389, &pin).expect("forget"));
    assert_eq!(std::fs::read_to_string(known.path()).expect("file"), kept);
}

#[test]
fn a_name_with_spaces_and_accents_stays_one_field_and_reads_back() {
    let dir = tempfile::tempdir().expect("dir");
    let known = KnownRdpHosts::new(dir.path().join("known_rdp_hosts"));
    let certificate = ServerCertificate {
        fingerprint: pin(),
        public_key: Vec::new(),
        subject: "CN=Soci\u{e9}t\u{e9} G\u{e9}n\u{e9}rale, O=Ville de Paris".to_owned(),
        issuer: "CN=Lab Root CA,O=Heimdall Lab".to_owned(),
    };
    known
        .record_certificate("dc.lab", 3389, &certificate)
        .expect("record");
    let text = std::fs::read_to_string(known.path()).expect("file");
    let fields: Vec<&str> = text.split_whitespace().collect();
    assert_eq!(
        fields.len(),
        5,
        "address, key, time, subject, issuer: {text}"
    );
    assert_eq!(
        fields[3],
        "subject=Q049U29jacOpdMOpIEfDqW7DqXJhbGUsIE89VmlsbGUgZGUgUGFyaXM"
    );
    assert_eq!(fields[4], "issuer=Q049TGFiIFJvb3QgQ0EsTz1IZWltZGFsbCBMYWI");
    let [entry] = known
        .entries()
        .expect("read")
        .try_into()
        .expect("one entry");
    assert_eq!(entry.subject, Some(certificate.subject));
    assert_eq!(entry.issuer, Some(certificate.issuer));
}
