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

//! "Test address" under the profile form's address, as the C# dialog's: the button, Cancel
//! while it runs, the sentence saying what it does not check, and the chip with what it found.

use heimdall_app::profile_draft::{AddressTest, ProfileDraft};
use heimdall_app::reachability::Unreached;
use heimdall_app::{Message as AppMessage, server_text};
use heimdall_core::profile::SshGateway;
use iced::widget::{button, column, row, text};
use iced::{Alignment, Element};

use crate::i18n::fl;
use crate::shell::Message;

/// Room between the controls.
const SPACING: f32 = 8.0;

/// Size of the hint and of the chip.
const SMALL_SIZE: f32 = 12.0;

/// How the chip reads: what it found, coloured as the C# chip's states.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tone {
    /// Running, or stopped: no verdict.
    Neutral,
    /// The address answered.
    Answered,
    /// It did not.
    Failed,
}

/// What the chip says of `test`, and how; `None` before any test. `gateway`, the gateway the
/// profile goes through, is named: the test dials directly, which is not the session's way.
#[must_use]
pub fn chip(test: &AddressTest, gateway: Option<&str>) -> Option<(String, Tone)> {
    let scoped = |verdict: String| match gateway {
        None => verdict,
        Some(name) => fl!(
            "ui-address-test-scoped",
            verdict = verdict,
            scope = fl!("ui-address-test-direct-scope", gateway = server_text(name))
        ),
    };
    Some(match test {
        AddressTest::Idle => return None,
        AddressTest::Running => (fl!("ui-address-test-running"), Tone::Neutral),
        AddressTest::Done(Ok(reached)) => {
            let verdict = match &reached.banner {
                Some(banner) => fl!("ui-address-test-success-ssh", banner = server_text(banner)),
                None => fl!(
                    "ui-address-test-success",
                    address = reached.address.as_str(),
                    millis = reached.millis
                ),
            };
            (scoped(verdict), Tone::Answered)
        }
        // Stopped: no verdict, so no limit to name.
        AddressTest::Done(Err(Unreached::Cancelled)) => {
            (fl!("ui-address-test-cancelled"), Tone::Neutral)
        }
        AddressTest::Done(Err(failure)) => (
            scoped(fl!("ui-address-test-failure", reason = reason(failure))),
            Tone::Failed,
        ),
    })
}

/// Why the address did not answer, in the C# words.
fn reason(failure: &Unreached) -> String {
    match failure {
        Unreached::DnsTimeout => fl!("ui-address-test-dns-timeout"),
        Unreached::DnsFailed(detail) => {
            fl!("ui-address-test-dns-failed", reason = server_text(detail))
        }
        Unreached::DnsNoResults => fl!("ui-address-test-dns-no-results"),
        Unreached::TcpTimeout(address) => {
            fl!("ui-address-test-tcp-timeout", address = address.as_str())
        }
        Unreached::TcpFailed { address, detail } => fl!(
            "ui-address-test-tcp-failed",
            address = address.as_str(),
            reason = server_text(detail)
        ),
        Unreached::Cancelled => fl!("ui-address-test-cancelled"),
    }
}

/// The controls under the address of `draft`; `gateways`, to name the one it goes through.
#[must_use]
pub fn view<'a>(draft: &'a ProfileDraft, gateways: &'a [SshGateway]) -> Element<'a, Message> {
    let running = draft.address_test == AddressTest::Running;
    let mut controls = row![
        button(text(fl!("ui-address-test-button")).size(SMALL_SIZE))
            .style(button::secondary)
            .on_press_maybe(
                (!running && draft.test_target().is_some())
                    .then_some(Message::App(AppMessage::TestAddress))
            )
    ]
    .spacing(SPACING)
    .align_y(Alignment::Center);
    if running {
        controls = controls.push(
            button(text(fl!("ui-address-test-cancel")).size(SMALL_SIZE))
                .style(button::text)
                .on_press(Message::App(AppMessage::CancelAddressTest)),
        );
    }
    let gateway = draft.gateway.as_ref().and_then(|id| {
        gateways
            .iter()
            .find(|gateway| &gateway.id == id)
            .map(|gateway| gateway.name.as_str())
    });
    // The hint beside the button, as one line: the form stays as short as it can.
    controls = controls.push(text(fl!("ui-address-test-hint")).size(SMALL_SIZE));
    let mut content = column![controls].spacing(SPACING / 2.0);
    if let Some((said, tone)) = chip(&draft.address_test, gateway) {
        let said = text(said).size(SMALL_SIZE);
        content = content.push(match tone {
            Tone::Neutral => said.style(text::secondary),
            Tone::Answered => said.style(text::success),
            Tone::Failed => said.style(text::danger),
        });
    }
    content.into()
}

#[cfg(test)]
mod tests {
    use heimdall_app::reachability::Reached;

    use super::*;

    fn done(result: Result<Reached, Unreached>) -> AddressTest {
        AddressTest::Done(result)
    }

    #[test]
    fn nothing_is_said_before_a_test_and_running_says_so() {
        assert_eq!(chip(&AddressTest::Idle, None), None);
        assert_eq!(
            chip(&AddressTest::Running, None),
            Some(("Testing the address...".to_owned(), Tone::Neutral))
        );
    }

    #[test]
    fn an_answer_is_said_as_the_csharp_chip_with_the_ssh_banner_when_there_is_one() {
        let reached = |banner: Option<&str>| Reached {
            address: "192.0.2.7".to_owned(),
            millis: 12,
            banner: banner.map(str::to_owned),
        };
        assert_eq!(
            chip(&done(Ok(reached(None))), None),
            Some((
                "Address answers: 192.0.2.7 (12 ms). Credentials were not checked.".to_owned(),
                Tone::Answered
            ))
        );
        assert_eq!(
            chip(&done(Ok(reached(Some("SSH-2.0-OpenSSH_9.6")))), None),
            Some((
                "Address answers and an SSH server replied: SSH-2.0-OpenSSH_9.6. Credentials \
                 were not checked."
                    .to_owned(),
                Tone::Answered
            ))
        );
    }

    #[test]
    fn a_verdict_through_a_gateway_says_it_was_tested_directly_and_a_stop_names_no_limit() {
        let failed = done(Err(Unreached::TcpTimeout("10.0.0.5".to_owned())));
        let (said, tone) = chip(&failed, Some("Bastion")).expect("said");
        assert_eq!(tone, Tone::Failed);
        assert!(
            said.starts_with("The address did not answer: TCP connect timed out (host: 10.0.0.5)."),
            "{said}"
        );
        assert!(
            said.ends_with("Tested directly from this computer, not through Bastion."),
            "{said}"
        );
        assert_eq!(
            chip(&done(Err(Unreached::Cancelled)), Some("Bastion")),
            Some(("Test cancelled.".to_owned(), Tone::Neutral))
        );
    }
}
