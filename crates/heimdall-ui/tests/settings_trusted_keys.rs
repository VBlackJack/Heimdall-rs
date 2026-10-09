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

//! The keys trusted for servers on the Settings page, and the way past an FTPS certificate
//! that changed.

mod common;

use std::path::Path;

use heimdall_app::{
    App, AppConfig, ConnectionEvent, Effect, Message as AppMessage, SettingsMessage,
    SystemCredentials, TrustedKey, TrustedKeysMessage, UiError,
};
use heimdall_core::profile::{FtpProfile, ProfileId};
use heimdall_rdp::{CertificateHash, KnownRdpHosts};
use heimdall_ssh::{AgentSource, KnownHosts, PublicKey};
use heimdall_term::GridSize;
use heimdall_ui::shell::{Message, SettingsTab, Shell};
use heimdall_ui::terminal_view::FONTS;
use heimdall_ui::trusted_keys_view::TrustedList;
use iced::{Settings, Size};

/// A window tall enough for the whole Settings page.
const WINDOW: Size = Size::new(1100.0, 1800.0);
/// A window tall enough for the whole SSH tab, the FTPS certificates at its end.
const TALL_WINDOW: Size = Size::new(1100.0, 3200.0);
const ED25519: &str =
    "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIEdv/0kqpfKUkuXCpQIlyU34zlRbf2MM2wBP+uTTnDTR";
const PIN: &str = "SHA256:rgJ0Y04RcqyBpgUaWIbkDqk3bEPjR9jNUxyEc7cvFq0";

fn shell(dir: &Path) -> Shell {
    Shell::with_app(App::new(AppConfig {
        profiles_file: dir.join("profiles.toml"),
        known_hosts: dir.join("known_hosts"),
        legacy_dir: None,
        agent: AgentSource::Disabled,
        initial_grid: GridSize { cols: 80, rows: 24 },
        files_start: dir.to_owned(),
        system_credentials: SystemCredentials::memory(),
    }))
}

fn simulator(shell: &Shell) -> common::Drawn<'_> {
    sized(shell, WINDOW)
}

/// The window of `shell`, at `size`.
fn sized(shell: &Shell, size: Size) -> common::Drawn<'_> {
    let settings = Settings {
        fonts: FONTS.iter().map(|face| (*face).into()).collect(),
        ..Settings::default()
    };
    common::simulator(settings, size, shell.view())
}

fn trust(dir: &Path) {
    let key = PublicKey::from_openssh(ED25519).expect("key");
    let hosts = KnownHosts::new(dir.join("known_hosts"));
    hosts.learn("web.lab", 22, &key).expect("learn");
    hosts.learn("db.lab", 2222, &key).expect("learn");
    KnownRdpHosts::new(dir.join("known_rdp_hosts"))
        .record("dc.lab", 3389, &PIN.parse().expect("pin"))
        .expect("record");
}

/// The Settings page on its `tab`.
fn show(shell: &mut Shell, tab: SettingsTab) {
    let _ = shell.update(Message::ShowSettings);
    let _ = shell.update(Message::SettingsTab(tab));
}

#[test]
fn nothing_trusted_says_so_in_the_csharp_words() {
    let dir = tempfile::tempdir().expect("dir");
    let mut shell = shell(dir.path());
    show(&mut shell, SettingsTab::Ssh);
    {
        let mut ui = simulator(&shell);
        for label in ["Trusted host keys", "No trusted host keys"] {
            ui.find(label).expect(label);
        }
        assert!(
            ui.find("Trusted RDP certificates").is_err(),
            "the RDP tab's"
        );
    }
    show(&mut shell, SettingsTab::Rdp);
    let mut ui = simulator(&shell);
    for label in ["Trusted RDP certificates", "No trusted RDP certificates"] {
        ui.find(label).expect(label);
    }
}

#[test]
fn the_page_lists_what_the_files_trust_when_shown() {
    let dir = tempfile::tempdir().expect("dir");
    let mut shell = shell(dir.path());
    trust(dir.path());
    show(&mut shell, SettingsTab::Ssh);
    {
        let mut ui = simulator(&shell);
        for label in ["web.lab:22", "db.lab:2222", "ssh-ed25519"] {
            ui.find(label).expect(label);
        }
        // Fingerprints cut short as the C# lists show them: 16 characters for SSH, 20 for RDP.
        let ssh: String =
            heimdall_ssh::fingerprint(&PublicKey::from_openssh(ED25519).expect("key"))
                .chars()
                .take(16)
                .collect();
        ui.find(format!("{ssh}..."))
            .expect("the SSH key, cut after 16");
        assert!(ui.find("No trusted host keys").is_err());
    }
    show(&mut shell, SettingsTab::Rdp);
    let mut ui = simulator(&shell);
    ui.find("dc.lab:3389").expect("the RDP server");
    ui.find("SHA256:rgJ0Y04RcqyBp...")
        .expect("the RDP key, cut after 20");
}

#[test]
fn a_certificate_shows_its_subject_issuer_and_when_it_was_trusted_as_the_csharp_columns() {
    let dir = tempfile::tempdir().expect("dir");
    // `CN=dc.lab` issued by `CN=Lab Root CA,O=Heimdall Lab`, trusted on 2026-03-15 at noon
    // UTC; then a line from before the details, which shows none.
    std::fs::write(
        dir.path().join("known_rdp_hosts"),
        format!(
            "dc.lab:3389 {PIN} trusted=1773576030 subject=Q049ZGMubGFi \
             issuer=Q049TGFiIFJvb3QgQ0EsTz1IZWltZGFsbCBMYWI\nweb.lab:3389 {PIN}\n"
        ),
    )
    .expect("write");
    let mut shell = shell(dir.path());
    show(&mut shell, SettingsTab::Rdp);
    let since = TrustedKey::Rdp(shell.app().trusted_keys().rdp[0].clone())
        .trusted_since()
        .expect("the time");
    assert!(since.starts_with("2026-03-1"), "{since}");
    {
        let mut ui = simulator(&shell);
        for label in [
            "Subject",
            "Issuer",
            "Trusted since",
            "CN=dc.lab",
            "CN=Lab Root CA,O=Heimdall Lab",
            since.as_str(),
            "web.lab:3389",
        ] {
            ui.find(label).expect(label);
        }
    }
    // As the C# search, the names of the certificate match too.
    let _ = shell.update(Message::TrustedSearch(
        TrustedList::Certificates,
        "root ca".to_owned(),
    ));
    let mut ui = simulator(&shell);
    ui.find("dc.lab:3389").expect("its issuer matches");
    assert!(ui.find("web.lab:3389").is_err(), "no names to match");
}

#[test]
fn a_search_keeps_the_servers_that_match() {
    let dir = tempfile::tempdir().expect("dir");
    let mut shell = shell(dir.path());
    trust(dir.path());
    show(&mut shell, SettingsTab::Ssh);
    let _ = shell.update(Message::TrustedSearch(
        TrustedList::HostKeys,
        " DB ".to_owned(),
    ));
    {
        let mut ui = simulator(&shell);
        ui.find("db.lab:2222").expect("matches");
        assert!(ui.find("web.lab:22").is_err(), "filtered out");
    }
    show(&mut shell, SettingsTab::Rdp);
    {
        let mut ui = simulator(&shell);
        ui.find("dc.lab:3389")
            .expect("the other list keeps its own search");
    }
    let _ = shell.update(Message::TrustedSearch(
        TrustedList::Certificates,
        "rgj0".to_owned(),
    ));
    let mut ui = simulator(&shell);
    ui.find("dc.lab:3389").expect("its fingerprint matches");
}

#[test]
fn copy_and_remove_ask_the_core_and_the_question_is_the_csharp_one() {
    let dir = tempfile::tempdir().expect("dir");
    let mut shell = shell(dir.path());
    trust(dir.path());
    show(&mut shell, SettingsTab::Ssh);
    let _ = shell.update(Message::TrustedSearch(
        TrustedList::HostKeys,
        "web".to_owned(),
    ));
    let web = TrustedKey::Ssh(shell.app().trusted_keys().ssh[0].clone());
    for (button, wanted) in [
        (
            "Copy fingerprint",
            TrustedKeysMessage::CopyFingerprint(web.clone()),
        ),
        ("Remove", TrustedKeysMessage::RequestForget(web.clone())),
    ] {
        let mut ui = simulator(&shell);
        ui.click(button).expect(button);
        let sent: Vec<TrustedKeysMessage> = ui
            .into_messages()
            .filter_map(|message| match message {
                Message::App(AppMessage::Settings(SettingsMessage::TrustedKeys(sent))) => {
                    Some(sent)
                }
                _ => None,
            })
            .collect();
        assert_eq!(sent, [wanted], "{button}");
    }
    {
        // The question alone: the page under it holds a Remove button too.
        let settings = Settings {
            fonts: FONTS.iter().map(|face| (*face).into()).collect(),
            ..Settings::default()
        };
        let mut ui = common::simulator(
            settings,
            WINDOW,
            heimdall_ui::trusted_keys_view::forget_question(&web),
        );
        ui.click("Remove").expect("the answer");
        assert!(
            ui.into_messages()
                .any(|message| matches!(message, Message::App(AppMessage::ConfirmDialog)))
        );
    }
    let _ = shell.update(Message::App(AppMessage::Settings(
        SettingsMessage::TrustedKeys(TrustedKeysMessage::RequestForget(web)),
    )));
    {
        let mut ui = simulator(&shell);
        ui.find("Remove trusted host key").expect("title");
        let fingerprint =
            heimdall_ssh::fingerprint(&PublicKey::from_openssh(ED25519).expect("key"));
        ui.find(format!(
            "Remove the trusted host key for web.lab:22?\n\nFingerprint: {fingerprint}\n\n\
             Removing this trusted host key will require re-verification on the next \
             connection to web.lab:22."
        ))
        .expect("body");
    }
    let _ = shell.update(Message::App(AppMessage::ConfirmDialog));
    let mut ui = simulator(&shell);
    assert!(ui.find("web.lab:22").is_err(), "forgotten");
    ui.find("Removed trusted host key for web.lab:22.")
        .expect("said");
}

#[test]
fn a_certificate_is_forgotten_after_the_csharp_question_with_keep() {
    let dir = tempfile::tempdir().expect("dir");
    let mut shell = shell(dir.path());
    trust(dir.path());
    show(&mut shell, SettingsTab::Rdp);
    let dc = TrustedKey::Rdp(shell.app().trusted_keys().rdp[0].clone());
    {
        let mut ui = simulator(&shell);
        ui.click("Forget").expect("the button");
        assert!(ui.into_messages().any(|message| matches!(
            message,
            Message::App(AppMessage::Settings(SettingsMessage::TrustedKeys(
                TrustedKeysMessage::RequestForget(ref sent)
            ))) if *sent == dc
        )));
    }
    let _ = shell.update(Message::App(AppMessage::Settings(
        SettingsMessage::TrustedKeys(TrustedKeysMessage::RequestForget(dc)),
    )));
    {
        let mut ui = simulator(&shell);
        ui.find("Forget this certificate?").expect("title");
        ui.find(format!(
            "Heimdall will forget the certificate {PIN} for dc.lab:3389. Only that certificate \
             is affected; any other certificate trusted for the same server stays trusted."
        ))
        .expect("body");
        ui.find("Keep").expect("the other answer");
    }
    let _ = shell.update(Message::App(AppMessage::ConfirmDialog));
    let mut ui = simulator(&shell);
    ui.find("No trusted RDP certificates").expect("forgotten");
    ui.find("Certificate forgotten for dc.lab:3389.")
        .expect("said");
}

#[test]
fn the_host_keys_section_imports_a_known_hosts_file_as_the_csharp_one() {
    let dir = tempfile::tempdir().expect("dir");
    let mut shell = shell(dir.path());
    show(&mut shell, SettingsTab::Ssh);
    let mut ui = simulator(&shell);
    ui.click("Import known_hosts").expect("the button");
    assert!(ui.into_messages().any(|message| matches!(
        message,
        Message::App(AppMessage::Settings(SettingsMessage::TrustedKeys(
            TrustedKeysMessage::Import(heimdall_app::HostKeysMessage::Start)
        )))
    )));
}

/// `CN=ftp.lab` and `CN=Lab Root CA,O=Heimdall Lab`, as the file writes names.
const FTPS_NAMES: &str = "subject=Q049ZnRwLmxhYg issuer=Q049TGFiIFJvb3QgQ0EsTz1IZWltZGFsbCBMYWI";

/// Trusts two certificates for `ftp.lab:21`, the first with its names and time, and one
/// from before them for `files.lab:990`.
fn trust_ftps(dir: &Path) {
    let other = format!("SHA256:{}", "A".repeat(43));
    std::fs::write(
        dir.join("known_ftps_hosts"),
        format!(
            "ftp.lab:21 {PIN} trusted=1773576030 {FTPS_NAMES}\nftp.lab:21 {other}\n\
             files.lab:990 {PIN}\n"
        ),
    )
    .expect("write");
}

#[test]
fn the_ssh_tab_lists_the_ftps_certificates_in_the_columns_of_the_rdp_ones() {
    let dir = tempfile::tempdir().expect("dir");
    let mut shell = shell(dir.path());
    show(&mut shell, SettingsTab::Ssh);
    {
        let mut ui = sized(&shell, TALL_WINDOW);
        for label in ["Trusted FTPS certificates", "No trusted FTPS certificates"] {
            ui.find(label).expect(label);
        }
    }
    trust_ftps(dir.path());
    show(&mut shell, SettingsTab::Ssh);
    let since = TrustedKey::Ftps(shell.app().trusted_keys().ftps[0].clone())
        .trusted_since()
        .expect("the time");
    {
        let mut ui = sized(&shell, TALL_WINDOW);
        for label in [
            "Trusted FTPS certificates",
            "Server",
            "Subject",
            "Issuer",
            "Trusted since",
            "ftp.lab:21",
            "SHA256:rgJ0Y04RcqyBp...",
            "CN=ftp.lab",
            "CN=Lab Root CA,O=Heimdall Lab",
            since.as_str(),
            // A line from before the names: listed all the same.
            "files.lab:990",
        ] {
            ui.find(label).expect(label);
        }
        assert!(ui.find("No trusted FTPS certificates").is_err());
    }
    // The RDP tab keeps to its own certificates.
    show(&mut shell, SettingsTab::Rdp);
    let mut ui = simulator(&shell);
    ui.find("No trusted RDP certificates").expect("none");
    assert!(ui.find("Trusted FTPS certificates").is_err());
    assert!(ui.find("ftp.lab:21").is_err());
}

#[test]
fn an_ftps_certificate_or_its_whole_server_is_forgotten_after_a_question() {
    let dir = tempfile::tempdir().expect("dir");
    let mut shell = shell(dir.path());
    trust_ftps(dir.path());
    show(&mut shell, SettingsTab::Ssh);
    // The first certificate alone in sight; its server holds another.
    let _ = shell.update(Message::TrustedSearch(
        TrustedList::FtpsCertificates,
        "rgj0".to_owned(),
    ));
    let first = TrustedKey::Ftps(shell.app().trusted_keys().ftps[0].clone());
    for (button, wanted) in [
        ("Forget", TrustedKeysMessage::RequestForget(first.clone())),
        (
            "Forget server",
            TrustedKeysMessage::RequestForgetServer(first.clone()),
        ),
    ] {
        let mut ui = sized(&shell, TALL_WINDOW);
        ui.click(button).expect(button);
        let sent: Vec<TrustedKeysMessage> = ui
            .into_messages()
            .filter_map(|message| match message {
                Message::App(AppMessage::Settings(SettingsMessage::TrustedKeys(sent))) => {
                    Some(sent)
                }
                _ => None,
            })
            .collect();
        assert_eq!(sent, [wanted], "{button}");
    }
    // A server trusted with one certificate offers only to forget it.
    let _ = shell.update(Message::TrustedSearch(
        TrustedList::FtpsCertificates,
        "files.lab".to_owned(),
    ));
    {
        let mut ui = sized(&shell, TALL_WINDOW);
        ui.find("files.lab:990").expect("listed");
        assert!(ui.find("Forget server").is_err());
    }
    let _ = shell.update(Message::App(AppMessage::Settings(
        SettingsMessage::TrustedKeys(TrustedKeysMessage::RequestForgetServer(first)),
    )));
    {
        let mut ui = sized(&shell, TALL_WINDOW);
        ui.find("Forget this server's certificates?")
            .expect("title");
        ui.find(
            "Heimdall will forget the 2 certificates trusted for ftp.lab:21. The next \
             connection to it asks again.",
        )
        .expect("body");
        ui.find("Keep").expect("the other answer");
    }
    let _ = shell.update(Message::App(AppMessage::ConfirmDialog));
    let _ = shell.update(Message::TrustedSearch(
        TrustedList::FtpsCertificates,
        String::new(),
    ));
    let mut ui = sized(&shell, TALL_WINDOW);
    assert!(ui.find("ftp.lab:21").is_err(), "forgotten");
    ui.find("files.lab:990").expect("the other server stays");
    ui.find("Every certificate forgotten for ftp.lab:21.")
        .expect("said");
}

#[test]
fn an_ftp_tab_whose_certificate_changed_offers_to_forget_the_server() {
    let dir = tempfile::tempdir().expect("dir");
    let profiles_file = dir.path().join("profiles.toml");
    let mut store = heimdall_core::store::ProfileStore::open(&profiles_file).expect("store");
    store.merge_ftp(vec![FtpProfile {
        id: ProfileId::new("files"),
        name: "files".to_owned(),
        group: None,
        host: "ftp.lab".to_owned(),
        port: 21,
        username: None,
        passive: true,
        tls: true,
        vault_entry: None,
    }]);
    store.save().expect("save");
    let mut core = App::new(AppConfig {
        profiles_file,
        known_hosts: dir.path().join("known_hosts"),
        legacy_dir: None,
        agent: AgentSource::Disabled,
        initial_grid: GridSize { cols: 80, rows: 24 },
        files_start: dir.path().to_owned(),
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
        event: ConnectionEvent::Failed(UiError::HostKeyChanged {
            target: None,
            recorded: "SHA256:old".to_owned(),
            offered: "SHA256:new".to_owned(),
        }),
    });
    let shell = Shell::with_app(core);
    let mut ui = simulator(&shell);
    ui.click("Forget this server").expect("the way past");
    assert!(ui.into_messages().any(|message| matches!(
        message,
        Message::App(AppMessage::ForgetServer(forgotten)) if forgotten == tab
    )));
}

#[test]
fn a_certificate_pinned_whole_shows_its_thumbprint_as_the_csharp_list() {
    let dir = tempfile::tempdir().expect("dir");
    // SHA-256 of "abc", as the whole certificate's hash; beside a line of its key alone.
    let whole = CertificateHash::of(b"abc");
    std::fs::write(
        dir.path().join("known_ftps_hosts"),
        format!("ftp.lab:21 {PIN} trusted=1773576030 certificate={whole}\nfiles.lab:990 {PIN}\n"),
    )
    .expect("write");
    let mut shell = shell(dir.path());
    show(&mut shell, SettingsTab::Ssh);
    {
        let mut ui = sized(&shell, TALL_WINDOW);
        // The key, as before, and the thumbprint cut after 20 characters.
        ui.find("SHA256:rgJ0Y04RcqyBp...").expect("the key");
        ui.find("SHA256:BA:78:16:BF:8...").expect("the thumbprint");
    }
    // Found by its thumbprint.
    let _ = shell.update(Message::TrustedSearch(
        TrustedList::FtpsCertificates,
        "BA:78:16".to_owned(),
    ));
    let mut ui = sized(&shell, TALL_WINDOW);
    ui.find("ftp.lab:21").expect("found");
    assert!(ui.find("files.lab:990").is_err(), "no thumbprint, no match");
}
