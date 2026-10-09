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
//! question about a gateway's unknown key on the way, the panel listing them under the
//! sessions, and what the status bar says of them.

use heimdall_app::tunnel::{
    LOCAL_PORT_MIN, PORT_MAX, REMOTE_PORT_MIN, SessionRoute, Tunnel, TunnelField, TunnelForm,
    TunnelProblem,
};
use heimdall_app::{Message as AppMessage, Notice, TunnelMessage, server_text};
use heimdall_core::profile::{ProfileId, SshGateway, display_address};
use iced::widget::{
    Column, button, column, container, mouse_area, pick_list, row, rule, space, text, text_input,
    tooltip,
};
use iced::{Alignment, Border, Element, Font, Length, Theme};

use crate::i18n::fl;
use crate::shell::Message;
use crate::styles;
use crate::texts;
use crate::tokens::{font_size, spacing};
use crate::tree_view::TreeMenu;

/// Height of the tunnels panel, within the C# panel's 120 to 300.
const PANEL_HEIGHT: f32 = 160.0;

/// Room around the panel's content.
const PANEL_PADDING: [f32; 2] = [4.0, 8.0];

/// Width of the ring of an interrupted tunnel's health dot.
const RING_WIDTH: f32 = 2.0;
/// Side of a row's health dot, as the C# one.
const DOT_SIDE: f32 = 7.0;

/// Widths of the Label, Local and Port columns, as the C# grid's; Gateway and Remote share
/// what is left.
const LABEL_WIDTH: f32 = 90.0;
const LOCAL_WIDTH: f32 = 60.0;
const PORT_WIDTH: f32 = 50.0;

/// Width of the column holding a row's close button.
const CLOSE_WIDTH: f32 = 28.0;

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
            .style(styles::text_input)
            .on_input(move |value| {
                Message::App(AppMessage::Tunnel(TunnelMessage::Field {
                    field: which,
                    value,
                }))
            })
            .on_submit(Message::App(AppMessage::ConfirmDialog)),
    ]
    .spacing(spacing::XS)
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
        text(fl!("ui-tunnel-new-title")).size(font_size::TITLE),
        text(fl!("ui-tunnel-new-description")),
    ]
    .spacing(spacing::SM);
    let cancel = button(text(fl!("ui-dialog-cancel-button")))
        .style(styles::secondary)
        .on_press(Message::App(AppMessage::DismissDialog));
    if gateways.is_empty() {
        return content
            .push(text(fl!("ui-tunnel-no-gateways")).style(text::warning))
            .push(row![cancel].spacing(spacing::SM))
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
                })
                .style(styles::pick_list)
                .menu_style(styles::menu),
            ]
            .spacing(spacing::XS),
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
    let open = button(text(fl!("ui-tunnel-open-button")))
        .style(styles::primary)
        .on_press_maybe(
            problem
                .is_none()
                .then_some(Message::App(AppMessage::ConfirmDialog)),
        );
    content.push(row![cancel, open].spacing(spacing::SM)).into()
}

/// The question about a gateway's unknown key on a tunnel's way, in the words and with the
/// choices a tab asks it: its algorithm, its fingerprint to copy, and trusting it once.
#[must_use]
pub fn host_key<'a>(
    host: &'a str,
    port: u16,
    fingerprint: &'a str,
    algorithm: &'a str,
) -> Element<'a, Message> {
    let port = port.to_string();
    column![
        text(fl!("ui-hostkey-title")).size(font_size::TITLE),
        text(fl!("ui-hostkey-body", host = host, port = port.as_str())),
        text(fl!("ui-hostkey-algorithm", algorithm = algorithm)).font(Font::MONOSPACE),
        row![
            text(fl!("ui-hostkey-fingerprint", fingerprint = fingerprint))
                .font(Font::MONOSPACE)
                .width(Length::Fill),
            button(text(fl!("ui-hostkey-copy-fingerprint-button")).size(font_size::CAPTION))
                .style(styles::secondary)
                .on_press(tunnel(TunnelMessage::CopyKeyFingerprint)),
        ]
        .spacing(spacing::SM)
        .align_y(Alignment::Center),
        row![
            button(text(fl!("ui-hostkey-reject-button")))
                .style(styles::secondary)
                .on_press(Message::App(AppMessage::DismissDialog)),
            button(text(fl!("ui-hostkey-trust-once-button")))
                .style(styles::secondary)
                .on_press(tunnel(TunnelMessage::TrustKeyOnce)),
            button(text(fl!("ui-hostkey-accept-button")))
                .style(styles::primary)
                .on_press(Message::App(AppMessage::ConfirmDialog)),
        ]
        .spacing(spacing::SM),
    ]
    .spacing(spacing::SM)
    .into()
}

fn tunnel(message: TunnelMessage) -> Message {
    Message::App(AppMessage::Tunnel(message))
}

/// A ghost button, its text the theme's danger colour, as the C# "Close All".
fn danger_text(theme: &Theme, status: button::Status) -> button::Style {
    button::Style {
        text_color: theme.extended_palette().danger.base.color,
        ..styles::subtle(theme, status)
    }
}

/// The tunnels panel under the sessions, as the C# one: its header with Close All, "+ New"
/// and the chevron that collapses it, then a row per open tunnel, or that there is none.
#[must_use]
pub fn panel<'a>(tunnels: &'a [Tunnel], routes: &[SessionRoute]) -> Element<'a, Message> {
    let header = row![
        text(fl!("ui-tunnels-header", count = tunnels.len())).size(font_size::CAPTION),
        space::horizontal(),
        close_all(
            button(text(fl!("ui-tunnels-close-all")).size(font_size::CAPTION))
                .style(danger_text)
                .on_press_maybe((!tunnels.is_empty()).then(|| tunnel(TunnelMessage::CloseAll)))
        ),
        button(text(fl!("ui-tunnels-new")).size(font_size::CAPTION))
            .style(styles::subtle)
            .on_press(tunnel(TunnelMessage::New)),
        tooltip(
            button(text(fl!("ui-tunnels-collapse-button")).size(font_size::CAPTION))
                .style(styles::subtle)
                .on_press(tunnel(TunnelMessage::TogglePanel)),
            text(fl!("ui-tunnels-collapse-tooltip")).size(font_size::CAPTION),
            tooltip::Position::Top,
        )
        .style(container::rounded_box),
    ]
    .spacing(spacing::SM)
    .align_y(Alignment::Center);
    let body = rows(tunnels, routes, false);
    container(
        column![header, columns(false), rule::horizontal(1), body]
            .spacing(spacing::XS)
            .width(Length::Fill),
    )
    .padding(PANEL_PADDING)
    .height(PANEL_HEIGHT)
    .width(Length::Fill)
    .style(container::bordered_box)
    .into()
}

/// The Tunnels page of the window's navigation, as the C# one: its title, New and Close
/// All, then every tunnel, the whole height.
pub fn page<'a>(tunnels: &'a [Tunnel], routes: &[SessionRoute]) -> Element<'a, Message> {
    let header = row![
        text(fl!("ui-tunnels-page-title")).size(font_size::TITLE),
        space::horizontal(),
        button(text(fl!("ui-tunnels-new")))
            .style(styles::primary)
            .on_press(tunnel(TunnelMessage::New)),
        close_all(
            button(text(fl!("ui-tunnels-close-all")))
                .style(styles::danger)
                .on_press_maybe((!tunnels.is_empty()).then(|| tunnel(TunnelMessage::CloseAll)))
        ),
    ]
    .spacing(spacing::SM)
    .align_y(Alignment::Center);
    let body = rows(tunnels, routes, true);
    // As the C# page's link under the grid.
    let manage = button(text(fl!("ui-tunnels-manage-gateways")))
        .style(styles::subtle)
        .on_press(Message::ManageGateways);
    container(
        column![header, columns(true), rule::horizontal(1), body, manage]
            .spacing(spacing::SM)
            .width(Length::Fill)
            .height(Length::Fill),
    )
    .padding(PAGE_PADDING)
    .width(Length::Fill)
    .height(Length::Fill)
    .into()
}

/// Width of the "Started" column, as the C# grid's.
const STARTED_WIDTH: f32 = 80.0;

/// Room around the Tunnels page.
const PAGE_PADDING: f32 = 16.0;

/// Close All, saying what it leaves: unlike the C# one, it never ends a session, whose
/// route is carried inside Heimdall and goes with its tab.
fn close_all(button: iced::widget::Button<'_, Message>) -> Element<'_, Message> {
    tooltip(
        button,
        text(fl!("ui-tunnels-close-all-tooltip")).size(font_size::CAPTION),
        tooltip::Position::Bottom,
    )
    .style(container::rounded_box)
    .into()
}

/// The tunnels opened by hand, then the sessions' routes under their own title: what Close
/// All closes kept apart from what it leaves.
fn rows<'a>(tunnels: &'a [Tunnel], routes: &[SessionRoute], started: bool) -> Element<'a, Message> {
    let size = if started {
        None
    } else {
        Some(font_size::CAPTION)
    };
    let note = |label: String| {
        let line = text(label).style(text::secondary);
        match size {
            Some(size) => line.size(size),
            None => line,
        }
    };
    let mut list = Column::new().spacing(spacing::XS);
    if tunnels.is_empty() {
        list = list.push(note(fl!("ui-tunnels-empty")));
    }
    list = list.extend(tunnels.iter().map(|open| tunnel_row(open, started)));
    if !routes.is_empty() {
        list = list.push(note(fl!("ui-tunnels-session-routes", count = routes.len())));
        list = list.extend(routes.iter().map(|route| route_row(route, started)));
    }
    styles::scroll(list).height(Length::Fill).into()
}

/// A session's route: its health, its gateways, its tab, no local port, the server it
/// reaches; nothing to close but its tab, which a click shows.
fn route_row(route: &SessionRoute, started: bool) -> Element<'static, Message> {
    let cell = |value: String| text(value).size(font_size::CAPTION);
    let gateways = route
        .route
        .iter()
        .map(|name| server_text(name))
        .collect::<Vec<_>>()
        .join(&fl!("ui-route-test-separator"));
    let line = row![
        status_dot(route.interrupted),
        cell(gateways).width(Length::Fill),
        cell(server_text(&route.title)).width(LABEL_WIDTH),
        tooltip(
            cell(fl!("ui-tunnels-session-route-local")).width(LOCAL_WIDTH),
            text(fl!("ui-tunnels-session-route-local-tooltip")).size(font_size::CAPTION),
            tooltip::Position::Top,
        )
        .style(container::rounded_box),
        cell(server_text(&route.remote.0)).width(Length::Fill),
        cell(route.remote.1.to_string()).width(PORT_WIDTH),
    ]
    .push(started.then(|| cell(route.started_clock()).width(STARTED_WIDTH)))
    .push(space().width(CLOSE_WIDTH))
    .spacing(spacing::SM)
    .align_y(Alignment::Center);
    mouse_area(line)
        .on_press(Message::App(AppMessage::SelectTab(route.tab)))
        .interaction(iced::mouse::Interaction::Pointer)
        .into()
}

/// The column titles, as the C# grid's; `started` on the page, which has the room.
fn columns<'a>(started: bool) -> Element<'a, Message> {
    let title = |label: String| text(label).size(font_size::CAPTION).style(text::secondary);
    row![
        space().width(DOT_SIDE),
        title(fl!("ui-tunnels-column-gateway")).width(Length::Fill),
        title(fl!("ui-tunnels-column-label")).width(LABEL_WIDTH),
        title(fl!("ui-tunnels-column-local")).width(LOCAL_WIDTH),
        title(fl!("ui-tunnels-column-remote")).width(Length::Fill),
        title(fl!("ui-tunnels-column-port")).width(PORT_WIDTH),
    ]
    .push(started.then(|| title(fl!("ui-tunnels-column-started")).width(STARTED_WIDTH)))
    .push(space().width(CLOSE_WIDTH))
    .spacing(spacing::SM)
    .into()
}

/// A tunnel's row: its health dot, gateway, label, local port, remote host and port, and the
/// button closing it; a right click opens its menu.
fn tunnel_row(open: &Tunnel, started: bool) -> Element<'_, Message> {
    let cell = |value: String| text(value).size(font_size::CAPTION);
    let interrupted = open.interrupted;
    let line = row![
        status_dot(interrupted),
        cell(server_text(&open.gateway_name)).width(Length::Fill),
        cell(
            open.spec
                .label
                .as_deref()
                .map(server_text)
                .unwrap_or_default()
        )
        .width(LABEL_WIDTH),
        cell(open.local.port().to_string()).width(LOCAL_WIDTH),
        cell(server_text(&open.spec.remote_host)).width(Length::Fill),
        cell(open.spec.remote_port.to_string()).width(PORT_WIDTH),
    ]
    .push(started.then(|| cell(open.started_clock()).width(STARTED_WIDTH)))
    .push(
        tooltip(
            button(text(fl!("ui-tab-close-button")).size(font_size::CAPTION))
                .style(styles::subtle)
                .width(CLOSE_WIDTH)
                .on_press(tunnel(TunnelMessage::Close(open.id))),
            text(fl!("ui-tunnels-close-tooltip")).size(font_size::CAPTION),
            tooltip::Position::Left,
        )
        .style(container::rounded_box),
    )
    .spacing(spacing::SM)
    .align_y(Alignment::Center);
    mouse_area(line)
        .on_right_press(Message::OpenTreeMenu(TreeMenu::Tunnel(open.id)))
        .into()
}

/// A row's health: a full green dot while the tunnel listens; an empty red ring once its
/// gateway went, the C# "Interrupted", told apart by its shape as much as by its colour.
fn status_dot<'a>(interrupted: bool) -> Element<'a, Message> {
    let dot = container(space().width(DOT_SIDE).height(DOT_SIDE)).style(move |theme: &Theme| {
        let palette = theme.extended_palette();
        let border = Border {
            radius: (DOT_SIDE / 2.0).into(),
            ..Border::default()
        };
        if interrupted {
            container::Style {
                border: Border {
                    color: palette.danger.base.color,
                    width: RING_WIDTH,
                    ..border
                },
                ..container::Style::default()
            }
        } else {
            container::Style {
                background: Some(palette.success.base.color.into()),
                border,
                ..container::Style::default()
            }
        }
    });
    let label = if interrupted {
        fl!("ui-tunnels-status-interrupted")
    } else {
        fl!("ui-tunnels-status-active")
    };
    tooltip(
        dot,
        text(label).size(font_size::CAPTION),
        tooltip::Position::Right,
    )
    .style(container::rounded_box)
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
