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

use heimdall_rdp::{
    CARRIED_SUFFIX, Carried, CarriedCertificate, CertificateHash, CertificateVerdict, Fingerprint,
    KnownRdpHosts, ServerCertificate, Validity, Verdict,
};

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
    let entry = |host: &str, port| (host.to_owned(), port, Some(pin()));
    let servers = || -> Vec<(String, u16, Option<Fingerprint>)> {
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
    let left: Vec<(String, Option<Fingerprint>)> = known
        .entries()
        .expect("read")
        .into_iter()
        .map(|entry| (entry.host, entry.fingerprint))
        .collect();
    assert_eq!(
        left,
        [
            ("dc.lab".to_owned(), Some(other)),
            ("web.lab".to_owned(), Some(pin()))
        ]
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
        ("dc.lab", 3389, Some(certificate.fingerprint))
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
    assert_eq!(
        (entry.host.as_str(), entry.fingerprint),
        ("dc.lab", Some(pin()))
    );
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
        certificate: CertificateHash::of(b"a certificate"),
        validity: Validity {
            not_before: UNIX_EPOCH,
            not_after: UNIX_EPOCH,
        },
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

/// A validity from second `from` to second `until` since 1970.
fn validity(from: u64, until: u64) -> Validity {
    Validity {
        not_before: UNIX_EPOCH + Duration::from_secs(from),
        not_after: UNIX_EPOCH + Duration::from_secs(until),
    }
}

#[test]
fn a_server_pinned_by_its_whole_certificate_is_known_by_that_certificate_only() {
    let dir = tempfile::tempdir().expect("dir");
    let known = KnownRdpHosts::new(dir.path().join("known_ftps_hosts"));
    let certificate = ServerCertificate::from_der(ISSUED).expect("readable");
    let key = certificate.fingerprint;
    let (first, renewed) = (CertificateHash::of(ISSUED), CertificateHash::of(b"renewed"));
    let verdict = |hash: &CertificateHash| {
        known
            .certificate_verdict("Files.lab", 990, &key, hash)
            .expect("read")
    };
    assert_eq!(verdict(&first), CertificateVerdict::Unknown, "no file yet");

    let held = validity(1_767_225_600, 1_798_761_600);
    known
        .record_whole_certificate("files.lab", 990, &certificate, (&first, Some(&held)))
        .expect("record");
    let line = std::fs::read_to_string(known.path()).expect("file");
    assert!(
        line.starts_with(&format!("files.lab:990 {key} trusted="))
            && line.ends_with(&format!(
                " certificate={first} valid_from=1767225600 valid_until=1798761600\n"
            )),
        "{line}"
    );
    assert_eq!(verdict(&first), CertificateVerdict::Known);
    assert_eq!(
        verdict(&renewed),
        CertificateVerdict::Renewed {
            recorded: Some(held)
        },
        "the same key, another certificate"
    );
    let other: Fingerprint = format!("SHA256:{}", "A".repeat(43)).parse().expect("other");
    assert_eq!(
        known
            .certificate_verdict("files.lab", 990, &other, &renewed)
            .expect("read"),
        CertificateVerdict::Changed { recorded: key },
        "another key: the key-changed alarm, as before"
    );
    // A reader of the first two fields reads the key as before.
    assert_eq!(
        known.verdict("files.lab", 990, &key).expect("read"),
        Verdict::Known
    );
    let [entry] = known
        .entries()
        .expect("read")
        .try_into()
        .expect("one entry");
    assert_eq!(
        (entry.certificate, entry.validity),
        (Some(first), Some(held))
    );

    // Trusted again once renewed: it replaces the line of its key, others stay.
    known.record("web.lab", 990, &key).expect("record");
    known
        .record_whole_certificate("files.lab", 990, &certificate, (&renewed, None))
        .expect("record");
    assert_eq!(verdict(&renewed), CertificateVerdict::Known);
    assert_eq!(
        verdict(&first),
        CertificateVerdict::Renewed { recorded: None },
        "the certificate replaced is no longer trusted"
    );
    let entries = known.entries().expect("read");
    assert_eq!(entries.len(), 2, "{entries:?}");
}

#[test]
fn a_server_recorded_by_its_key_alone_adopts_a_certificate_once_keeping_every_other_line() {
    let dir = tempfile::tempdir().expect("dir");
    let known = KnownRdpHosts::new(dir.path().join("known_vnc_hosts"));
    let certificate = ServerCertificate::from_der(ISSUED).expect("readable");
    let key = certificate.fingerprint;
    let hash = CertificateHash::of(ISSUED);
    // As a Heimdall that pinned keys wrote it, beside a comment and another server with an
    // attribute no Heimdall knows.
    let before =
        format!("# kept\nkiosk.lab:5900 {key} trusted=1767225600 future=x\nweb.lab:5900 {key}\n");
    std::fs::write(known.path(), &before).expect("write");
    let verdict = |hash: &CertificateHash| {
        known
            .certificate_verdict("kiosk.lab", 5900, &key, hash)
            .expect("read")
    };
    assert_eq!(verdict(&hash), CertificateVerdict::KeyOnly);
    assert!(
        known
            .adopt_certificate("kiosk.lab", 5900, &key, (&hash, None))
            .expect("adopt")
    );
    assert_eq!(
        std::fs::read_to_string(known.path()).expect("file"),
        format!(
            "# kept\nkiosk.lab:5900 {key} trusted=1767225600 future=x certificate={hash}\n\
             web.lab:5900 {key}\n"
        ),
        "its attributes added at the end, every other line as it was"
    );
    assert_eq!(verdict(&hash), CertificateVerdict::Known);
    assert_eq!(
        verdict(&CertificateHash::of(b"another")),
        CertificateVerdict::Renewed { recorded: None },
        "adopted once: another certificate on the key is asked about"
    );
    assert!(
        !known
            .adopt_certificate("kiosk.lab", 5900, &key, (&hash, None))
            .expect("adopt"),
        "never twice"
    );
}

/// Changes made by each thread of the concurrent test.
const CONCURRENT_CHANGES: usize = 60;

#[test]
fn concurrent_changes_lose_nothing_and_a_pinned_server_never_reads_unknown() {
    let dir = tempfile::tempdir().expect("dir");
    let path = dir.path().join("known_vnc_hosts");
    let certificate = ServerCertificate::from_der(ISSUED).expect("readable");
    let (key, hash) = (certificate.fingerprint, CertificateHash::of(ISSUED));
    KnownRdpHosts::new(&path)
        .record_whole_certificate("pinned.lab", 5900, &certificate, (&hash, None))
        .expect("pinned");
    std::thread::scope(|scope| {
        // Trusted again and again: its line replaced each time.
        scope.spawn(|| {
            let known = KnownRdpHosts::new(&path);
            for _ in 0..CONCURRENT_CHANGES {
                known
                    .record_whole_certificate("pinned.lab", 5900, &certificate, (&hash, None))
                    .expect("record");
            }
        });
        // Other servers trusted by their key.
        scope.spawn(|| {
            let known = KnownRdpHosts::new(&path);
            for index in 0..CONCURRENT_CHANGES {
                known
                    .record(&format!("added{index}.lab"), 5900, &key)
                    .expect("record");
            }
        });
        // Servers recorded by their key alone, then adopting their certificate.
        scope.spawn(|| {
            let known = KnownRdpHosts::new(&path);
            for index in 0..CONCURRENT_CHANGES {
                let host = format!("legacy{index}.lab");
                known.record(&host, 5900, &key).expect("record");
                assert!(
                    known
                        .adopt_certificate(&host, 5900, &key, (&hash, None))
                        .expect("adopt"),
                    "{host}"
                );
            }
        });
        // Read meanwhile: the pinned server is always known.
        scope.spawn(|| {
            let known = KnownRdpHosts::new(&path);
            for _ in 0..CONCURRENT_CHANGES * 3 {
                assert_eq!(
                    known
                        .certificate_verdict("pinned.lab", 5900, &key, &hash)
                        .expect("read"),
                    CertificateVerdict::Known
                );
            }
        });
    });
    let entries = KnownRdpHosts::new(&path).entries().expect("read");
    let count = |prefix: &str| {
        entries
            .iter()
            .filter(|entry| entry.host.starts_with(prefix))
            .count()
    };
    assert_eq!(count("pinned"), 1, "replaced, never doubled");
    assert_eq!(count("added"), CONCURRENT_CHANGES, "no append lost");
    assert_eq!(count("legacy"), CONCURRENT_CHANGES);
    assert!(
        entries
            .iter()
            .filter(|entry| entry.host.starts_with("legacy"))
            .all(|entry| entry.certificate == Some(hash)),
        "every adoption kept"
    );
}

#[test]
fn a_change_that_cannot_be_written_leaves_the_file_whole() {
    let dir = tempfile::tempdir().expect("dir");
    let path = dir.path().join("known_vnc_hosts");
    let known = KnownRdpHosts::new(&path);
    known.record("kiosk.lab", 5900, &pin()).expect("record");
    known.record("web.lab", 5900, &pin()).expect("record");
    let before = std::fs::read(&path).expect("file");
    // The new text cannot be written beside the file: a folder stands where it goes.
    std::fs::create_dir(dir.path().join("known_vnc_hosts.tmp")).expect("in the way");
    assert!(known.forget("kiosk.lab", 5900).is_err());
    let hash = CertificateHash::of(ISSUED);
    assert!(
        known
            .adopt_certificate("web.lab", 5900, &pin(), (&hash, None))
            .is_err()
    );
    assert_eq!(std::fs::read(&path).expect("file"), before, "as it was");
}

#[test]
fn a_change_keeps_the_line_endings_of_the_lines_it_does_not_touch() {
    let dir = tempfile::tempdir().expect("dir");
    let known = KnownRdpHosts::new(dir.path().join("known_vnc_hosts"));
    let key = pin();
    // Edited on Windows, its last line without an ending.
    std::fs::write(
        known.path(),
        format!("# kept\r\nkiosk.lab:5900 {key}\r\nweb.lab:5900 {key}\r\nold.lab:5900 {key}"),
    )
    .expect("write");
    let hash = CertificateHash::of(ISSUED);
    assert!(
        known
            .adopt_certificate("kiosk.lab", 5900, &key, (&hash, None))
            .expect("adopt")
    );
    assert!(known.forget("web.lab", 5900).expect("forget"));
    assert_eq!(
        std::fs::read_to_string(known.path()).expect("file"),
        format!("# kept\r\nkiosk.lab:5900 {key} certificate={hash}\r\nold.lab:5900 {key}"),
    );
}

#[test]
fn a_certificate_that_does_not_read_is_asked_about_never_taken_for_a_key_alone() {
    let dir = tempfile::tempdir().expect("dir");
    let known = KnownRdpHosts::new(dir.path().join("known_vnc_hosts"));
    let key = pin();
    std::fs::write(
        known.path(),
        format!("kiosk.lab:5900 {key} certificate=damaged\n"),
    )
    .expect("write");
    let hash = CertificateHash::of(ISSUED);
    assert_eq!(
        known
            .certificate_verdict("kiosk.lab", 5900, &key, &hash)
            .expect("read"),
        CertificateVerdict::Renewed { recorded: None },
        "asked about again"
    );
    assert!(
        !known
            .adopt_certificate("kiosk.lab", 5900, &key, (&hash, None))
            .expect("adopt"),
        "never adopted"
    );
    let [entry] = known
        .entries()
        .expect("read")
        .try_into()
        .expect("one entry");
    assert!(entry.certificate_unreadable && entry.certificate.is_none());
}

/// A certificate the C# Heimdall trusted for `host:port`, by the hash `certificate`.
fn carried(host: &str, port: u16, certificate: CertificateHash) -> CarriedCertificate {
    CarriedCertificate {
        host: host.to_owned(),
        port,
        certificate,
        trusted: Some(UNIX_EPOCH + Duration::from_secs(1_767_225_601)),
        subject: Some("CN=files.lab".to_owned()),
        issuer: None,
    }
}

/// The file beside `known` that holds the certificates carried over.
fn carried_file(known: &KnownRdpHosts) -> std::path::PathBuf {
    let mut name = known.path().file_name().expect("name").to_owned();
    name.push(CARRIED_SUFFIX);
    known.path().with_file_name(name)
}

#[test]
fn a_certificate_carried_over_is_trusted_by_the_whole_of_it_and_nothing_else() {
    let dir = tempfile::tempdir().expect("dir");
    let known = KnownRdpHosts::new(dir.path().join("known_ftps_hosts"));
    let certificate = ServerCertificate::from_der(ISSUED).expect("readable");
    let hash = CertificateHash::of(ISSUED);
    let done = known
        .carry_over(&[carried("Files.Lab", 990, hash)])
        .expect("carried");
    assert_eq!(
        done,
        Carried {
            carried: 1,
            left: 0
        }
    );
    assert!(
        !known.path().exists(),
        "a key-less line never goes in the file an older reader reads"
    );
    let text = std::fs::read_to_string(carried_file(&known)).expect("file beside it");
    assert!(
        text.starts_with(&format!(
            "files.lab:990 - certificate={hash} trusted=1767225601 "
        )),
        "the host in lower case, no key, the C# date kept: {text}"
    );

    let [entry] = known
        .entries()
        .expect("read")
        .try_into()
        .expect("one entry");
    assert_eq!(entry.fingerprint, None, "the C# never kept the key");
    assert_eq!(entry.certificate, Some(hash));
    assert_eq!(entry.subject.as_deref(), Some("CN=files.lab"));
    assert_eq!(
        entry.pin_text(),
        hash.thumbprint(),
        "shown as the C# showed it"
    );
    assert!(known.knows("files.lab", 990).expect("read"), "TLS required");

    // The very certificate goes through, whatever key it carries.
    assert_eq!(
        known
            .certificate_verdict("FILES.lab", 990, &certificate.fingerprint, &hash)
            .expect("read"),
        CertificateVerdict::Known
    );
    // Another certificate, even on the same key: refused, never asked about.
    let other = CertificateHash::from_sha256([7; 32]);
    assert_eq!(
        known
            .certificate_verdict("files.lab", 990, &certificate.fingerprint, &other)
            .expect("read"),
        CertificateVerdict::OtherCertificate { recorded: hash }
    );
    // A reader of the keys alone has no key to say; a carried server is not unknown.
    assert_eq!(
        known
            .verdict("files.lab", 990, &certificate.fingerprint)
            .expect("read"),
        Verdict::CertificateOnly
    );
    // Another server is not concerned.
    assert_eq!(
        known
            .certificate_verdict("files.lab", 21, &certificate.fingerprint, &hash)
            .expect("read"),
        CertificateVerdict::Unknown
    );
}

#[test]
fn a_certificate_carried_over_beside_a_key_lets_the_key_decide_another_certificate() {
    let dir = tempfile::tempdir().expect("dir");
    let known = KnownRdpHosts::new(dir.path().join("known_rdp_hosts"));
    let hash = CertificateHash::from_sha256([1; 32]);
    known
        .carry_over(&[carried("dc.lab", 3389, hash)])
        .expect("carried");
    // Accepted here later, for another machine of the pool.
    known.record("dc.lab", 3389, &pin()).expect("record");
    let other = CertificateHash::from_sha256([2; 32]);
    let stranger: Fingerprint = "SHA256:BwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwc"
        .parse()
        .expect("key");
    assert_ne!(stranger, pin(), "another key than the one recorded");
    assert_eq!(
        known
            .certificate_verdict("dc.lab", 3389, &stranger, &other)
            .expect("read"),
        CertificateVerdict::Changed { recorded: pin() },
        "the key recorded is the one said"
    );
    assert_eq!(
        known
            .certificate_verdict("dc.lab", 3389, &stranger, &hash)
            .expect("read"),
        CertificateVerdict::Known
    );
}

#[test]
fn a_server_trusted_here_is_never_added_to_by_a_carry_over() {
    let dir = tempfile::tempdir().expect("dir");
    let known = KnownRdpHosts::new(dir.path().join("known_rdp_hosts"));
    known.record("dc.lab", 3389, &pin()).expect("record");
    let (first, second) = (
        CertificateHash::from_sha256([1; 32]),
        CertificateHash::from_sha256([2; 32]),
    );
    let done = known
        .carry_over(&[
            carried("DC.lab", 3389, first),
            // A pool behind one profile: both trusted, the same one given twice once.
            carried("pool.lab", 3389, first),
            carried("pool.lab", 3389, second),
            carried("pool.lab", 3389, second),
            // A host that cannot start a line, and a port that is none.
            carried("bad host", 3389, first),
            carried("#comment", 3389, first),
            carried("zero.lab", 0, first),
        ])
        .expect("carried");
    assert_eq!(
        done,
        Carried {
            carried: 2,
            left: 5
        }
    );
    let pool: Vec<Option<CertificateHash>> = known
        .entries()
        .expect("read")
        .into_iter()
        .filter(|entry| entry.host == "pool.lab")
        .map(|entry| entry.certificate)
        .collect();
    assert_eq!(pool, [Some(first), Some(second)]);
    assert_eq!(
        known
            .certificate_verdict("dc.lab", 3389, &pin(), &first)
            .expect("read"),
        CertificateVerdict::KeyOnly,
        "the trust decided here stands alone"
    );
}

#[test]
fn a_certificate_carried_over_is_forgotten_alone() {
    let dir = tempfile::tempdir().expect("dir");
    let known = KnownRdpHosts::new(dir.path().join("known_rdp_hosts"));
    let (first, second) = (
        CertificateHash::from_sha256([1; 32]),
        CertificateHash::from_sha256([2; 32]),
    );
    known
        .carry_over(&[
            carried("pool.lab", 3389, first),
            carried("pool.lab", 3389, second),
        ])
        .expect("carried");
    assert!(
        known
            .forget_certificate("POOL.lab", 3389, &first)
            .expect("forget")
    );
    let left: Vec<Option<CertificateHash>> = known
        .entries()
        .expect("read")
        .into_iter()
        .map(|entry| entry.certificate)
        .collect();
    assert_eq!(left, [Some(second)]);
    assert!(
        !known
            .forget_certificate("pool.lab", 3389, &first)
            .expect("gone")
    );
}

#[test]
fn a_carried_line_whose_certificate_does_not_read_trusts_nothing() {
    let dir = tempfile::tempdir().expect("dir");
    let known = KnownRdpHosts::new(dir.path().join("known_rdp_hosts"));
    std::fs::write(
        known.path(),
        "dc.lab:3389 - certificate=SHA256:nope\ndc.lab:3389 - trusted=1\n",
    )
    .expect("write");
    assert_eq!(known.entries().expect("read"), []);
    assert!(!known.knows("dc.lab", 3389).expect("read"));
    assert_eq!(
        known
            .certificate_verdict("dc.lab", 3389, &pin(), &CertificateHash::of(ISSUED))
            .expect("read"),
        CertificateVerdict::Unknown
    );
}

#[test]
fn a_thumbprint_read_back_is_the_hash_it_was_shown_from() {
    let hash = CertificateHash::of(ISSUED);
    let shown = hash.thumbprint();
    let bytes: Vec<u8> = shown
        .trim_start_matches("SHA256:")
        .split(':')
        .map(|byte| u8::from_str_radix(byte, 16).expect("hex"))
        .collect();
    let bytes: [u8; 32] = bytes.try_into().expect("32 bytes");
    assert_eq!(CertificateHash::from_sha256(bytes), hash);
}

#[test]
fn an_older_reader_of_the_file_alone_never_finds_a_carried_server_in_it() {
    let dir = tempfile::tempdir().expect("dir");
    let known = KnownRdpHosts::new(dir.path().join("known_ftps_hosts"));
    // A server trusted here, then others carried over, then the first changed again.
    known.record("kept.lab", 21, &pin()).expect("record");
    known
        .carry_over(&[
            carried("files.lab", 990, CertificateHash::from_sha256([1; 32])),
            carried("pool.lab", 21, CertificateHash::from_sha256([2; 32])),
        ])
        .expect("carried");
    known
        .record("later.lab", 21, &pin())
        .expect("record after the carry-over");
    known.forget("later.lab", 21).expect("forget");

    // What an older Heimdall reads: this file alone, its `keys()` and its `knows()` both.
    let main = std::fs::read_to_string(known.path()).expect("file");
    for line in main.lines() {
        let key = line.split_whitespace().nth(1).expect("a key field");
        assert!(
            key.parse::<Fingerprint>().is_ok(),
            "a key-less line in the file: {line}"
        );
    }
    assert!(
        !main.contains("files.lab") && !main.contains("pool.lab"),
        "{main}"
    );
    let older = dir.path().join("older_reader");
    std::fs::create_dir(&older).expect("dir");
    std::fs::write(older.join("known_ftps_hosts"), &main).expect("copy");
    let older = KnownRdpHosts::new(older.join("known_ftps_hosts"));
    for host in ["files.lab", "pool.lab"] {
        assert!(
            !older.knows(host, 990).expect("read") && !older.knows(host, 21).expect("read"),
            "{host} as it was before the import, not a server half-read"
        );
    }
    assert!(older.knows("kept.lab", 21).expect("read"));
    // This reader sees both, and lists them across the two files.
    let hosts: Vec<String> = known
        .entries()
        .expect("read")
        .into_iter()
        .map(|entry| entry.host)
        .collect();
    assert_eq!(hosts, ["kept.lab", "files.lab", "pool.lab"]);
}

#[test]
fn forgetting_a_server_drops_its_carried_lines_too() {
    let dir = tempfile::tempdir().expect("dir");
    let known = KnownRdpHosts::new(dir.path().join("known_rdp_hosts"));
    known.record("kept.lab", 3389, &pin()).expect("record");
    known
        .carry_over(&[
            carried("pool.lab", 3389, CertificateHash::from_sha256([1; 32])),
            carried("pool.lab", 3389, CertificateHash::from_sha256([2; 32])),
            carried("other.lab", 3389, CertificateHash::from_sha256([3; 32])),
        ])
        .expect("carried");
    assert!(known.forget("pool.lab", 3389).expect("forget"));
    assert!(!known.knows("pool.lab", 3389).expect("read"));
    assert!(known.knows("other.lab", 3389).expect("read"));
    assert!(known.knows("kept.lab", 3389).expect("read"));
}

#[test]
fn the_key_verdict_of_a_server_known_by_a_carried_certificate_alone_is_not_unknown() {
    let dir = tempfile::tempdir().expect("dir");
    let known = KnownRdpHosts::new(dir.path().join("known_rdp_hosts"));
    known
        .carry_over(&[carried("dc.lab", 3389, CertificateHash::of(ISSUED))])
        .expect("carried");
    assert_eq!(
        known.verdict("DC.lab", 3389, &pin()).expect("read"),
        Verdict::CertificateOnly
    );
    assert_eq!(
        known.verdict("other.lab", 3389, &pin()).expect("read"),
        Verdict::Unknown
    );
    // A key recorded beside it decides, as before.
    known.record("dc.lab", 3389, &pin()).expect("record");
    assert_eq!(
        known.verdict("dc.lab", 3389, &pin()).expect("read"),
        Verdict::Known
    );
    let stranger: Fingerprint = "SHA256:BwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwc"
        .parse()
        .expect("key");
    assert_eq!(
        known.verdict("dc.lab", 3389, &stranger).expect("read"),
        Verdict::Changed { recorded: pin() }
    );
}
