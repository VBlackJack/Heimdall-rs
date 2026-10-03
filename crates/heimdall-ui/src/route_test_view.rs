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

//! The gateway dialog's "Test route" card, as the C# one: the route it walks, an optional
//! destination, Test, Stop and Copy, what is running, and the report: each step's label,
//! outcome and time, with no host name, account, key path or raw error in it.

use std::time::SystemTime;

use heimdall_app::gateway_draft::{GatewayDraft, RouteProblem, RouteTest, TargetField};
use heimdall_app::{Message as AppMessage, server_text};
use heimdall_core::profile::{ProfileId, SshGateway, display_address};
use heimdall_core::utc::UtcTime;
use heimdall_ssh::{Outcome, Step, StepOf};
use iced::widget::{Column, button, column, container, row, text, text_input};
use iced::{Element, Font, Length};

use crate::i18n::fl;
use crate::shell::Message;

/// Room between the card's parts.
const SPACING: f32 = 8.0;

/// Size of the card's hints and report.
const SMALL_SIZE: f32 = 12.0;

/// Width of the destination port field.
const PORT_WIDTH: f32 = 90.0;

/// The gateways `parent` is reached through, nearest first, ending with it; a chain that
/// loops stops where it would.
fn chain<'a>(parent: Option<&ProfileId>, gateways: &'a [SshGateway]) -> Vec<&'a SshGateway> {
    let mut chain: Vec<&SshGateway> = Vec::new();
    let mut next = parent;
    while let Some(id) = next {
        let Some(gateway) = gateways.iter().find(|gateway| &gateway.id == id) else {
            break;
        };
        if chain.iter().any(|seen| seen.id == gateway.id) {
            break;
        }
        chain.push(gateway);
        next = gateway.parent.as_ref();
    }
    chain.reverse();
    chain
}

/// The route as the C# card draws it: "This workstation", each gateway, the destination.
#[must_use]
pub fn route_line(draft: &GatewayDraft, gateways: &[SshGateway]) -> String {
    let hop = |name: &str, host: &str, port: &str| {
        fl!(
            "ui-route-test-hop",
            name = server_text(name),
            host = server_text(host),
            port = port
        )
    };
    let mut parts = vec![fl!("ui-route-test-workstation")];
    parts.extend(
        chain(draft.parent.as_ref(), gateways)
            .into_iter()
            .map(|gateway| hop(&gateway.name, &gateway.host, &gateway.port.to_string())),
    );
    parts.push(hop(&draft.name, &draft.host, draft.port.trim()));
    if !draft.target_host.trim().is_empty() {
        parts.push(display_address(
            &server_text(draft.target_host.trim()),
            draft.target_port.trim().parse().unwrap_or_default(),
        ));
    }
    parts.join(&fl!("ui-route-test-separator"))
}

/// What a step is about, as the C# labels it: gateways counted from 1.
fn label(of: StepOf) -> String {
    match of {
        StepOf::Gateway(index) => fl!("ui-route-test-hop-step", number = (index + 1)),
        StepOf::Destination => fl!("ui-route-test-target-step"),
    }
}

/// How a step ended, as the C# says it.
fn outcome(outcome: Outcome) -> String {
    match outcome {
        Outcome::Passed => fl!("ui-route-test-passed"),
        Outcome::TrustRequired => fl!("ui-route-test-trust-required"),
        Outcome::TrustChanged => fl!("ui-route-test-trust-changed"),
        Outcome::Timeout => fl!("ui-route-test-timeout"),
        Outcome::Network => fl!("ui-route-test-network"),
        Outcome::Forwarding => fl!("ui-route-test-forwarding"),
        Outcome::Cancelled => fl!("ui-route-test-cancelled"),
        Outcome::Interactive => fl!("ui-route-test-interactive"),
        Outcome::Auth => fl!("ui-route-test-auth"),
        Outcome::Unavailable => fl!("ui-route-test-unavailable"),
    }
}

/// A step's line: label, outcome, time.
fn step_line(step: &Step) -> String {
    fl!(
        "ui-route-test-step-line",
        step = label(step.of),
        outcome = outcome(step.outcome),
        millis = u64::try_from(step.elapsed.as_millis()).unwrap_or(u64::MAX)
    )
}

/// The report the test gives, as the C# one: no host name, account, key path or raw error;
/// `None` before a test ended. `with_target` says whether a destination was tested.
#[must_use]
pub fn report(steps: &[Step], at: SystemTime, with_target: bool) -> String {
    let at = UtcTime::of(at);
    let mut lines = vec![
        fl!("ui-route-test-report-header"),
        format!(
            "{:04}-{:02}-{:02} {:02}:{:02}:{:02}Z",
            at.year, at.month, at.day, at.hour, at.minute, at.second
        ),
    ];
    lines.extend(steps.iter().map(step_line));
    lines.push(if with_target {
        fl!("ui-route-test-tcp-only")
    } else {
        fl!("ui-route-test-no-target")
    });
    lines.join("\n")
}

/// The report of the test the dialog shows, once it ended.
#[must_use]
pub fn finished_report(draft: &GatewayDraft) -> Option<String> {
    let RouteTest::Done { steps, at } = &draft.route_test else {
        return None;
    };
    let with_target = steps.iter().any(|step| step.of == StepOf::Destination)
        || !draft.target_host.trim().is_empty();
    Some(report(steps, *at, with_target))
}

/// The card, under the gateway dialog's fields.
#[must_use]
pub fn card<'a>(draft: &'a GatewayDraft, gateways: &'a [SshGateway]) -> Element<'a, Message> {
    let running = matches!(draft.route_test, RouteTest::Running(_));
    let target =
        |field: TargetField| move |value| Message::App(AppMessage::RouteTarget { field, value });
    let host = text_input(&fl!("ui-route-test-target-host"), &draft.target_host)
        .on_input_maybe((!running).then_some(target(TargetField::Host)));
    let port = text_input(&fl!("ui-route-test-target-port"), &draft.target_port)
        .on_input_maybe((!running).then_some(target(TargetField::Port)))
        .width(PORT_WIDTH);
    let mut buttons = row![
        button(text(fl!("ui-route-test-test")).size(SMALL_SIZE))
            .style(button::secondary)
            .on_press_maybe((!running).then_some(Message::TestRouteForm))
    ]
    .spacing(SPACING);
    if running {
        buttons = buttons.push(
            button(text(fl!("ui-route-test-stop")).size(SMALL_SIZE))
                .style(button::text)
                .on_press(Message::App(AppMessage::StopRouteTest)),
        );
    }
    if matches!(draft.route_test, RouteTest::Done { .. }) {
        buttons = buttons.push(
            button(text(fl!("ui-route-test-copy")).size(SMALL_SIZE))
                .style(button::text)
                .on_press(Message::CopyRouteReport),
        );
    }
    let mut content: Column<'a, Message> = column![
        text(fl!("ui-route-test-title")),
        text(fl!("ui-route-test-hint")).size(SMALL_SIZE),
        text(route_line(draft, gateways)).size(SMALL_SIZE),
        row![host, port].spacing(SPACING),
        buttons,
    ]
    .spacing(SPACING);
    match &draft.route_test {
        RouteTest::Idle => {}
        RouteTest::Refused(problem) => {
            content = content.push(
                text(match problem {
                    RouteProblem::Route => fl!("ui-route-test-invalid-route"),
                    RouteProblem::Target => fl!("ui-route-test-invalid-target"),
                })
                .size(SMALL_SIZE)
                .style(text::danger),
            );
        }
        RouteTest::Running(steps) => {
            let mut lines = vec![fl!("ui-route-test-running")];
            lines.extend(steps.iter().map(step_line));
            content = content.push(
                text(lines.join("\n"))
                    .size(SMALL_SIZE)
                    .font(Font::MONOSPACE),
            );
        }
        RouteTest::Done { .. } => {
            if let Some(report) = finished_report(draft) {
                content = content.push(text(report).size(SMALL_SIZE).font(Font::MONOSPACE));
            }
        }
    }
    container(content)
        .padding(SPACING)
        .width(Length::Fill)
        .style(container::bordered_box)
        .into()
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, UNIX_EPOCH};

    use super::*;

    fn gateway(id: &str, name: &str, parent: Option<&str>) -> SshGateway {
        SshGateway {
            id: ProfileId::new(id),
            name: name.to_owned(),
            host: format!("{id}.lab"),
            port: 22,
            username: Some("jump".to_owned()),
            key_path: None,
            parent: parent.map(ProfileId::new),
        }
    }

    #[test]
    fn the_route_reads_from_this_workstation_to_the_destination_as_the_csharp_line() {
        let gateways = [
            gateway("outer", "Outer", None),
            gateway("inner", "Inner", Some("outer")),
        ];
        let draft = GatewayDraft {
            name: "Edge".to_owned(),
            host: "edge.lab".to_owned(),
            port: "2222".to_owned(),
            parent: Some(ProfileId::new("inner")),
            target_host: "wiki.lab".to_owned(),
            target_port: "443".to_owned(),
            ..GatewayDraft::default()
        };
        assert_eq!(
            route_line(&draft, &gateways),
            "This workstation \u{2192} Outer (outer.lab:22) \u{2192} Inner (inner.lab:22) \
             \u{2192} Edge (edge.lab:2222) \u{2192} wiki.lab:443"
        );
    }

    #[test]
    fn the_report_names_no_host_and_says_what_was_checked() {
        let steps = [
            Step {
                of: StepOf::Gateway(0),
                outcome: Outcome::Passed,
                elapsed: Duration::from_millis(12),
            },
            Step {
                of: StepOf::Destination,
                outcome: Outcome::Forwarding,
                elapsed: Duration::from_millis(3),
            },
        ];
        let said = report(&steps, UNIX_EPOCH, true);
        let lines: Vec<&str> = said.lines().collect();
        assert!(lines[0].starts_with("Heimdall SSH route diagnostic (anonymized"));
        assert_eq!(lines[1], "1970-01-01 00:00:00Z");
        assert_eq!(
            lines[2],
            "Gateway 1: SSH connection and authentication: Passed (12 ms)"
        );
        assert!(
            lines[3].starts_with("Destination TCP access: Destination access was not confirmed.")
        );
        assert!(lines[4].starts_with("The destination step checks TCP access only."));
        assert!(!said.contains(".lab"), "no host name");
    }
}
