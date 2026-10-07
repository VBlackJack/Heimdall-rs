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

//! The question about an FTPS certificate nobody trusts yet, as the C# FTPS prompt asks it:
//! beside the subject, the issuer, when the certificate holds, and why the system did not
//! vouch for it, the server's names made safe to show.

mod common;

use std::path::Path;

use heimdall_app::{
    App, AppConfig, CertificateDetails, ConnectionEvent, Effect, Message as AppMessage,
    SystemCredentials,
};
use heimdall_core::profile::{FtpProfile, ProfileId};
use heimdall_rdp::{ServerCertificate, Validity};
use heimdall_ssh::AgentSource;
use heimdall_term::GridSize;
use heimdall_tls::ValidationIssue;
use heimdall_ui::shell::Shell;
use heimdall_ui::terminal_view::FONTS;
use iced::{Settings, Size};
use rcgen::{CertificateParams, DistinguishedName, DnType, KeyPair, date_time_ymd};

const WINDOW: Size = Size::new(1200.0, 900.0);

/// A fingerprint the question shows.
const FINGERPRINT: &str = "SHA256:AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA";

/// A self-signed certificate named `common_name`, valid from the first of January of `from`
/// to that of `until`.
fn certificate(common_name: &str, (from, until): (i32, i32)) -> Vec<u8> {
    let mut params = CertificateParams::new(vec!["files.lab".to_owned()]).expect("params");
    let mut name = DistinguishedName::new();
    name.push(DnType::CommonName, common_name);
    params.distinguished_name = name;
    params.not_before = date_time_ymd(from, 1, 1);
    params.not_after = date_time_ymd(until, 1, 1);
    let key = KeyPair::generate().expect("key");
    params
        .self_signed(&key)
        .expect("certificate")
        .der()
        .to_vec()
}

/// What the FTPS driver says of `der`, refused for `issue`.
fn details(der: &[u8], issue: ValidationIssue) -> CertificateDetails {
    CertificateDetails {
        issuer: ServerCertificate::from_der(der).expect("read").issuer,
        validity: Validity::from_der(der).expect("validity"),
        issue,
    }
}

/// The window once the FTPS server of a fresh application presented a certificate with
/// `details`, asked about.
fn asked(dir: &Path, details: CertificateDetails) -> Shell {
    let profiles_file = dir.join("profiles.toml");
    let mut store = heimdall_core::store::ProfileStore::open(&profiles_file).expect("store");
    store.merge_ftp(vec![FtpProfile {
        id: ProfileId::new("files"),
        name: "Files".to_owned(),
        group: None,
        host: "files.lab".to_owned(),
        port: 21,
        username: None,
        passive: true,
        tls: true,
        vault_entry: None,
    }]);
    store.save().expect("save");
    let mut core = App::new(AppConfig {
        profiles_file,
        known_hosts: dir.join("known_hosts"),
        legacy_dir: None,
        agent: AgentSource::Disabled,
        initial_grid: GridSize { cols: 80, rows: 24 },
        files_start: dir.to_owned(),
        system_credentials: SystemCredentials::memory(),
    });
    let effects = core.update(AppMessage::ConnectProfile(ProfileId::new("files")));
    let [Effect::ConnectFtp { tab, attempt, .. }] = effects.as_slice() else {
        panic!("{effects:?}");
    };
    let (tab, attempt) = (*tab, *attempt);
    core.update(AppMessage::Connection {
        tab,
        attempt,
        event: ConnectionEvent::UnknownRdpCertificate {
            host: "files.lab".to_owned(),
            port: 21,
            fingerprint: FINGERPRINT.parse().expect("fingerprint"),
            subject: Some("CN=files.lab".to_owned()),
            details: Some(Box::new(details)),
        },
    });
    Shell::with_app(core)
}

fn simulator(shell: &Shell) -> common::Drawn<'_> {
    common::simulator(
        Settings {
            fonts: FONTS.iter().map(|face| (*face).into()).collect(),
            ..Settings::default()
        },
        WINDOW,
        shell.view(),
    )
}

#[test]
fn an_expired_self_signed_certificate_shows_its_issuer_dates_and_issue_made_safe() {
    let dir = tempfile::tempdir().expect("dir");
    // A name with a character that reverses the text after it: shown without it.
    let der = certificate("Lab \u{202E}Root", (2000, 2001));
    let mut shown = details(&der, ValidationIssue::SelfSigned);
    // A control character the driver would not let through, made safe here too.
    shown.issuer.push('\u{7}');
    let shell = asked(dir.path(), shown);
    let mut ui = simulator(&shell);
    ui.find("Unrecognised Server Certificate").expect("title");
    ui.find("Subject: CN=files.lab").expect("the subject");
    ui.find("Issuer: CN=Lab Root")
        .expect("the issuer, without what could mislead");
    ui.find("Valid from / until: 2000-01-01 00:00 - 2001-01-01 00:00 (expired)")
        .expect("the dates, marked expired");
    ui.find("Validation issue: The certificate is self-signed: no certificate authority vouches for it.")
        .expect("why the system refused it");
}

#[test]
fn a_certificate_not_valid_yet_is_marked_and_a_current_one_is_not() {
    let dir = tempfile::tempdir().expect("dir");
    let der = certificate("Lab Root", (2045, 2046));
    let shell = asked(dir.path(), details(&der, ValidationIssue::NameMismatch));
    let mut ui = simulator(&shell);
    ui.find("Valid from / until: 2045-01-01 00:00 - 2046-01-01 00:00 (not yet valid)")
        .expect("marked not yet valid");
    ui.find("Validation issue: The certificate was issued for another name than this server's.")
        .expect("the name mismatch");
    drop(ui);

    let dir = tempfile::tempdir().expect("dir");
    let der = certificate("Lab Root", (2020, 2045));
    let shell = asked(dir.path(), details(&der, ValidationIssue::UnknownIssuer));
    let mut ui = simulator(&shell);
    ui.find("Valid from / until: 2020-01-01 00:00 - 2045-01-01 00:00")
        .expect("current: no mark");
    ui.find(
        "Validation issue: It was issued by a certificate authority this computer does not trust.",
    )
    .expect("the unknown issuer");
}
