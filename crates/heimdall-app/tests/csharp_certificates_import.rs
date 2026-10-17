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

//! The migration of this computer's C# store carrying over its trusted RDP and FTPS
//! certificates into the trust stores the RDP and FTPS clients use.

use std::path::Path;

use heimdall_app::{
    App, AppConfig, CarriedCertificates, Dialog, ImportSummary, Message, SettingsMessage,
    SystemCredentials, TrustedKey, TrustedKeysMessage,
};
use heimdall_rdp::{CertificateHash, CertificateVerdict, Fingerprint, KnownRdpHosts};
use heimdall_ssh::AgentSource;
use heimdall_term::GridSize;

/// The C# store's profiles: an RDP one, whose certificates the C# keeps by its identifier,
/// its host written in another case than the C# FTPS keys use.
const SERVERS: &str = r#"{"servers":[
    {"id":"r","displayName":"DC","remoteServer":"DC.lab","remotePort":3390,"connectionType":"RDP"},
    {"id":"x","displayName":"X","remoteServer":"x.lab","connectionType":"SSH"}]}"#;

/// A C# thumbprint whose 32 bytes are all `byte`.
fn thumb(byte: u8) -> String {
    let bytes: Vec<String> = (0..32).map(|_| format!("{byte:02X}")).collect();
    format!("SHA256:{}", bytes.join(":"))
}

/// The configuration of an application in `dir`, the C# store in `dir/legacy` holding
/// `SERVERS` and `settings`.
fn config(dir: &Path, settings: &str) -> AppConfig {
    let legacy = dir.join("legacy");
    std::fs::create_dir_all(&legacy).expect("legacy");
    std::fs::write(legacy.join("servers.json"), SERVERS).expect("servers");
    std::fs::write(legacy.join("settings.json"), settings).expect("settings");
    AppConfig {
        profiles_file: dir.join("profiles.toml"),
        known_hosts: dir.join("known_hosts"),
        legacy_dir: Some(legacy),
        agent: AgentSource::Disabled,
        initial_grid: GridSize { cols: 80, rows: 24 },
        files_start: dir.to_owned(),
        system_credentials: SystemCredentials::memory(),
    }
}

/// The report of the import just run.
fn report(app: &App) -> &ImportSummary {
    match &app.dialog {
        Some(Dialog::ImportDone(summary)) => summary,
        other => panic!("expected ImportDone, got {other:?}"),
    }
}

/// A C# `settings.json` trusting certificates of each kind, one of each not to be trusted.
fn settings() -> String {
    format!(
        r#"{{"trustedRdpCertificates": {{"r": [{{"thumbprint": "{rdp}",
                "firstTrusted": "2026-01-01T00:00:01+00:00", "subject": "CN=dc.lab"}}],
                "gone": [{{"thumbprint": "{rdp}"}}]}},
            "trustedFtpsCertificates": {{
                "files.lab:990": {{"fingerprint": "{ftps}", "source": "UserConfirmed"}},
                "[fe80::1]:990": {{"fingerprint": "{ftps}", "source": "UserConfirmed"}},
                "public.lab:990": {{"fingerprint": "{ftps}", "source": "SystemValidated"}},
                "broken.lab:990": {{"fingerprint": "SHA1:AB:CD", "source": "UserConfirmed"}}}}}}"#,
        rdp = thumb(1),
        ftps = thumb(2),
    )
}

/// A key no test certificate has.
fn any_key() -> Fingerprint {
    format!("SHA256:{}", "A".repeat(43)).parse().expect("key")
}

#[test]
fn the_csharp_certificates_are_trusted_as_the_csharp_bound_them() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = App::new(config(dir.path(), &settings()));
    app.update(Message::ImportLegacy);
    assert_eq!(
        report(&app).certificates,
        Some(Ok(CarriedCertificates {
            rdp: 1,
            ftps: 2,
            skipped: 2,
            invalid: 1,
        })),
        "the profile not imported and the certificate the system validated skipped"
    );
    let rdp = KnownRdpHosts::new(dir.path().join("known_rdp_hosts"));
    let rdp_hash = CertificateHash::from_sha256([1; 32]);
    assert_eq!(
        rdp.certificate_verdict("dc.lab", 3390, &any_key(), &rdp_hash)
            .expect("read"),
        CertificateVerdict::Known,
        "bound to the server of the profile that trusted it, whatever its case"
    );
    let ftps = KnownRdpHosts::new(dir.path().join("known_ftps_hosts"));
    let ftps_hash = CertificateHash::from_sha256([2; 32]);
    for (host, port) in [("files.lab", 990), ("fe80::1", 990)] {
        assert_eq!(
            ftps.certificate_verdict(host, port, &any_key(), &ftps_hash)
                .expect("read"),
            CertificateVerdict::Known,
            "{host}"
        );
    }
    assert!(
        !ftps.knows("public.lab", 990).expect("read"),
        "never trusted by the user"
    );
}

#[test]
fn a_certificate_carried_over_is_listed_and_forgotten_as_any() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = App::new(config(dir.path(), &settings()));
    app.update(Message::ImportLegacy);
    app.update(Message::DismissDialog);
    app.update(Message::Settings(SettingsMessage::TrustedKeys(
        TrustedKeysMessage::Refresh,
    )));
    let entry = app.trusted_keys().rdp[0].clone();
    assert_eq!(entry.fingerprint, None);
    assert_eq!(entry.subject.as_deref(), Some("CN=dc.lab"));
    let key = TrustedKey::Rdp(entry);
    assert_eq!(key.fingerprint(), thumb(1), "the C# thumbprint shown");
    app.update(Message::Settings(SettingsMessage::TrustedKeys(
        TrustedKeysMessage::RequestForget(key),
    )));
    app.update(Message::ConfirmDialog);
    assert!(app.trusted_keys().rdp.is_empty());
    let rdp = KnownRdpHosts::new(dir.path().join("known_rdp_hosts"));
    assert!(!rdp.knows("dc.lab", 3390).expect("read"));
}

#[test]
fn a_server_trusted_here_is_left_as_it_is_by_a_second_import() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = App::new(config(dir.path(), &settings()));
    let ftps = KnownRdpHosts::new(dir.path().join("known_ftps_hosts"));
    ftps.record("files.lab", 990, &any_key()).expect("record");
    app.update(Message::ImportLegacy);
    assert_eq!(
        report(&app).certificates,
        Some(Ok(CarriedCertificates {
            rdp: 1,
            ftps: 1,
            skipped: 3,
            invalid: 1,
        }))
    );
    assert_eq!(
        ftps.certificate_verdict(
            "files.lab",
            990,
            &any_key(),
            &CertificateHash::from_sha256([2; 32])
        )
        .expect("read"),
        CertificateVerdict::KeyOnly,
        "the trust decided here stands alone"
    );
}

#[test]
fn certificates_of_another_shape_fail_on_their_own_line() {
    let dir = tempfile::tempdir().expect("dir");
    let settings = r#"{"defaultTheme": "Vesper", "trustedFtpsCertificates": "everything"}"#;
    let mut app = App::new(config(dir.path(), settings));
    app.update(Message::ImportLegacy);
    let summary = report(&app);
    assert_eq!(summary.merged.added, 2);
    assert_eq!(
        summary.certificates,
        Some(Ok(CarriedCertificates {
            invalid: 1,
            ..CarriedCertificates::default()
        }))
    );
    assert!(
        matches!(&summary.preferences, Some(Ok(preferences)) if preferences.taken == 1),
        "the preferences read all the same"
    );
}

#[test]
fn a_part_the_profiles_cannot_read_leaves_the_certificates_read() {
    let dir = tempfile::tempdir().expect("dir");
    let settings = format!(
        r#"{{"groupDefaults": 3, "trustedFtpsCertificates": {{
            "files.lab:990": {{"fingerprint": "{}", "source": "UserConfirmed"}}}}}}"#,
        thumb(2)
    );
    let mut app = App::new(config(dir.path(), &settings));
    app.update(Message::ImportLegacy);
    let summary = report(&app);
    assert!(summary.settings_unreadable.is_some());
    assert_eq!(
        summary.certificates,
        Some(Ok(CarriedCertificates {
            ftps: 1,
            ..CarriedCertificates::default()
        }))
    );
}
