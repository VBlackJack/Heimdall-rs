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

//! The keys trusted for servers on the Settings page.

use std::path::Path;

use heimdall_app::{
    App, AppConfig, Message as AppMessage, SettingsMessage, SystemCredentials, TrustedKey,
    TrustedKeysMessage,
};
use heimdall_rdp::KnownRdpHosts;
use heimdall_ssh::{AgentSource, KnownHosts, PublicKey};
use heimdall_term::GridSize;
use heimdall_ui::shell::{Message, Shell};
use heimdall_ui::terminal_view::FONTS;
use heimdall_ui::trusted_keys_view::TrustedList;
use iced::{Settings, Size};
use iced_test::simulator::Simulator;

/// A window tall enough for the whole Settings page.
const WINDOW: Size = Size::new(1100.0, 1800.0);
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

fn simulator(shell: &Shell) -> Simulator<'_, Message> {
    let settings = Settings {
        fonts: FONTS.iter().map(|face| (*face).into()).collect(),
        ..Settings::default()
    };
    Simulator::with_size(settings, WINDOW, shell.view())
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

#[test]
fn nothing_trusted_says_so_in_the_csharp_words() {
    let dir = tempfile::tempdir().expect("dir");
    let mut shell = shell(dir.path());
    let _ = shell.update(Message::ShowSettings);
    let mut ui = simulator(&shell);
    for label in [
        "Trusted host keys",
        "No trusted host keys",
        "Trusted RDP certificates",
        "No trusted RDP certificates",
    ] {
        ui.find(label).expect(label);
    }
}

#[test]
fn the_page_lists_what_the_files_trust_when_shown() {
    let dir = tempfile::tempdir().expect("dir");
    let mut shell = shell(dir.path());
    trust(dir.path());
    let _ = shell.update(Message::ShowSettings);
    let mut ui = simulator(&shell);
    for label in ["web.lab:22", "db.lab:2222", "dc.lab:3389", "ssh-ed25519"] {
        ui.find(label).expect(label);
    }
    // Fingerprints cut short as the C# lists show them: 16 characters for SSH, 20 for RDP.
    let ssh: String = heimdall_ssh::fingerprint(&PublicKey::from_openssh(ED25519).expect("key"))
        .chars()
        .take(16)
        .collect();
    ui.find(format!("{ssh}..."))
        .expect("the SSH key, cut after 16");
    ui.find("SHA256:rgJ0Y04RcqyBp...")
        .expect("the RDP key, cut after 20");
    assert!(ui.find("No trusted host keys").is_err());
}

#[test]
fn a_search_keeps_the_servers_that_match() {
    let dir = tempfile::tempdir().expect("dir");
    let mut shell = shell(dir.path());
    trust(dir.path());
    let _ = shell.update(Message::ShowSettings);
    let _ = shell.update(Message::TrustedSearch(
        TrustedList::HostKeys,
        " DB ".to_owned(),
    ));
    {
        let mut ui = simulator(&shell);
        ui.find("db.lab:2222").expect("matches");
        assert!(ui.find("web.lab:22").is_err(), "filtered out");
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
    let _ = shell.update(Message::ShowSettings);
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
        let mut ui = Simulator::with_size(
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
    let _ = shell.update(Message::ShowSettings);
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
