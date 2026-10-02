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

//! The tunnels the user opens by hand, in the window: the C# "New tunnel" dialog, the
//! question about a gateway's unknown key on the way, and what the status bar says of them.

use heimdall_app::tunnel::{
    LOCAL_PORT_MIN, PORT_MAX, REMOTE_PORT_MIN, TunnelField, TunnelForm, TunnelProblem,
};
use heimdall_app::{Message as AppMessage, Notice, TunnelMessage, server_text};
use heimdall_core::profile::{ProfileId, SshGateway, display_address};
use iced::widget::{button, column, pick_list, row, text, text_input};
use iced::{Element, Font};

use crate::i18n::fl;
use crate::shell::Message;
use crate::texts;

/// Room between a dialog's parts.
const SPACING: f32 = 8.0;

/// Size of a dialog's title.
const HEADING_SIZE: f32 = 20.0;

/// A gateway in the dialog's list: "Name  user@host:port", as the C# combo shows one.
#[derive(Debug, Clone, PartialEq, Eq)]
struct GatewayChoice {
    id: ProfileId,
    label: String,
}

impl std::fmt::Display for GatewayChoice {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.label)
    }
}

fn choice(gateway: &SshGateway) -> GatewayChoice {
    let address = display_address(&server_text(&gateway.host), gateway.port);
    let account = gateway.username.as_deref().map_or(address.clone(), |user| {
        format!("{}@{address}", server_text(user))
    });
    GatewayChoice {
        id: gateway.id.clone(),
        label: format!("{}  {account}", server_text(&gateway.name)),
    }
}

/// The sentence for `problem`, as the C# check says it.
#[must_use]
pub fn problem_text(problem: TunnelProblem) -> String {
    match problem {
        TunnelProblem::Gateway => fl!("ui-tunnel-problem-gateway"),
        TunnelProblem::RemoteHost => fl!("ui-tunnel-problem-remote-host"),
        TunnelProblem::RemotePort => fl!(
            "ui-tunnel-problem-remote-port",
            min = REMOTE_PORT_MIN,
            max = PORT_MAX
        ),
        TunnelProblem::LocalPort => fl!(
            "ui-tunnel-problem-local-port",
            min = LOCAL_PORT_MIN,
            max = PORT_MAX
        ),
        TunnelProblem::LocalPortInUse(port) => {
            fl!("ui-tunnel-problem-local-port-in-use", port = port)
        }
    }
}

fn field(label: String, value: &str, which: TunnelField) -> Element<'_, Message> {
    column![
        text(label),
        text_input("", value)
            .on_input(move |value| {
                Message::App(AppMessage::Tunnel(TunnelMessage::Field {
                    field: which,
                    value,
                }))
            })
            .on_submit(Message::App(AppMessage::ConfirmDialog)),
    ]
    .spacing(SPACING / 2.0)
    .into()
}

/// The "New tunnel" dialog, filled as `form`, its gateways chosen among `gateways`; what
/// stops the tunnel from opening, `problem`, said under the fields as the C# says it, one at
/// a time, and "Open tunnel" offered only once there is none.
#[must_use]
pub fn new_tunnel<'a>(
    form: &'a TunnelForm,
    gateways: &'a [SshGateway],
    problem: Option<TunnelProblem>,
) -> Element<'a, Message> {
    let mut content = column![
        text(fl!("ui-tunnel-new-title")).size(HEADING_SIZE),
        text(fl!("ui-tunnel-new-description")),
    ]
    .spacing(SPACING);
    let cancel = button(text(fl!("ui-dialog-cancel-button")))
        .style(button::secondary)
        .on_press(Message::App(AppMessage::DismissDialog));
    if gateways.is_empty() {
        return content
            .push(text(fl!("ui-tunnel-no-gateways")).style(text::warning))
            .push(row![cancel].spacing(SPACING))
            .into();
    }
    let choices: Vec<GatewayChoice> = gateways.iter().map(choice).collect();
    let selected = choices
        .iter()
        .find(|choice| Some(&choice.id) == form.gateway.as_ref())
        .cloned();
    content = content
        .push(
            column![
                text(fl!("ui-tunnel-gateway-label")),
                pick_list(choices, selected, |chosen: GatewayChoice| {
                    Message::App(AppMessage::Tunnel(TunnelMessage::Gateway(chosen.id)))
                }),
            ]
            .spacing(SPACING / 2.0),
        )
        .push(field(
            fl!("ui-tunnel-remote-host-label"),
            &form.remote_host,
            TunnelField::RemoteHost,
        ))
        .push(field(
            fl!("ui-tunnel-remote-port-label"),
            &form.remote_port,
            TunnelField::RemotePort,
        ))
        .push(field(
            fl!("ui-tunnel-local-port-label"),
            &form.local_port,
            TunnelField::LocalPort,
        ))
        .push(field(
            fl!("ui-tunnel-label-label"),
            &form.label,
            TunnelField::Label,
        ));
    if let Some(problem) = problem {
        content = content.push(text(problem_text(problem)).style(text::danger));
    }
    let open = button(text(fl!("ui-tunnel-open-button"))).on_press_maybe(
        problem
            .is_none()
            .then_some(Message::App(AppMessage::ConfirmDialog)),
    );
    content.push(row![cancel, open].spacing(SPACING)).into()
}

/// The question about a gateway's unknown key on a tunnel's way, in the words a tab asks it.
#[must_use]
pub fn host_key<'a>(host: &'a str, port: u16, fingerprint: &'a str) -> Element<'a, Message> {
    let port = port.to_string();
    column![
        text(fl!("ui-hostkey-title")).size(HEADING_SIZE),
        text(fl!("ui-hostkey-body", host = host, port = port.as_str())),
        text(fl!("ui-hostkey-fingerprint", fingerprint = fingerprint)).font(Font::MONOSPACE),
        row![
            button(text(fl!("ui-hostkey-reject-button")))
                .style(button::secondary)
                .on_press(Message::App(AppMessage::DismissDialog)),
            button(text(fl!("ui-hostkey-accept-button")))
                .on_press(Message::App(AppMessage::ConfirmDialog)),
        ]
        .spacing(SPACING),
    ]
    .spacing(SPACING)
    .into()
}

/// What the status bar says of a tunnel `notice`; `None` when it is not one.
#[must_use]
pub fn notice_text(notice: &Notice) -> Option<String> {
    Some(match notice {
        Notice::TunnelOpened {
            port,
            host,
            remote_port,
        } => fl!(
            "ui-tunnel-opened",
            port = (*port),
            host = server_text(host),
            remote = (*remote_port)
        ),
        Notice::TunnelFailed(error) => fl!("ui-tunnel-failed", reason = texts::error(error)),
        Notice::TunnelClosed { port, error } => {
            let closed = fl!("ui-tunnel-closed", port = (*port));
            match error {
                None => closed,
                Some(error) => fl!(
                    "ui-tunnel-closed-reason",
                    closed = closed,
                    reason = texts::error(error)
                ),
            }
        }
        Notice::AllTunnelsClosed => fl!("ui-tunnels-all-closed"),
        Notice::PortCopied(port) => fl!("ui-tunnel-port-copied", port = (*port)),
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn gateway(username: Option<&str>) -> SshGateway {
        SshGateway {
            id: ProfileId::new("bastion"),
            name: "Bastion".to_owned(),
            host: "bastion.lab".to_owned(),
            port: 2222,
            username: username.map(str::to_owned),
            key_path: None,
            parent: None,
        }
    }

    #[test]
    fn a_gateway_is_listed_as_the_csharp_combo_shows_it() {
        assert_eq!(
            choice(&gateway(Some("jump"))).to_string(),
            "Bastion  jump@bastion.lab:2222"
        );
        assert_eq!(
            choice(&gateway(None)).to_string(),
            "Bastion  bastion.lab:2222"
        );
    }
}
