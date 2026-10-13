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

//! The C# server dialog's "Connection path" card (`ServerDialog.xaml:2435-2496`): what the
//! form will do said in a line, then this computer, the gateways a session goes through and
//! its destination, as nodes joined by arrows that name the transport between them.
//!
//! The C# draws the gateway chosen as one node, its whole route said as a line of text under
//! the gateway picker; here every gateway of the route is a node of its own, the one chosen
//! last, a route of one being the C#'s diagram. A session in a tab opens no local port: this
//! computer's node names the local tunnel only for a program started outside the
//! application (`PuTTY`, Remote Desktop Connection, `PowerShell` for `WinRM`), as the tunnel
//! port card says it.

use heimdall_app::profile_draft::{DraftProtocol, ProfileDraft, ProfileField, ProfileToggle};
use heimdall_app::server_text;
use heimdall_core::gateway_parents::MAX_GATEWAY_CHAIN_DEPTH;
use heimdall_core::profile::{ProfileId, SshGateway, SshMode, display_address};
use iced::widget::{Column, Row, column, container, text};
use iced::{Alignment, Element, Length, Padding};

use crate::dialog_parts;
use crate::i18n::fl;
use crate::shell::Message;
use crate::styles;
use crate::tokens::{font_size, spacing};

/// Width of a node, as the C# `DiagramNodeStyle`.
const NODE_WIDTH: f32 = 165.0;
/// Width of an arrow and its caption, as the C# arrow's `StackPanel`.
const ARROW_WIDTH: f32 = 78.0;
/// Room above an arrow, as the C# arrow's top margin: it points at the nodes' titles.
const ARROW_TOP: f32 = 16.0;

/// A node of the diagram: its title, then what it says of itself.
struct Node {
    title: String,
    captions: Vec<String>,
}

/// The card for `draft`, its gateways found among `gateways`.
#[must_use]
pub fn card<'a>(draft: &ProfileDraft, gateways: &[SshGateway]) -> Element<'a, Message> {
    let routed = draft.routed_gateway();
    let headline = if routed.is_some() {
        fl!("ui-profile-path-headline-tunnel")
    } else {
        fl!("ui-profile-path-headline-direct")
    };
    let mut nodes = Row::new().push(node(Node {
        title: fl!("ui-profile-path-your-pc"),
        captions: client_caption(draft).into_iter().collect(),
    }));
    if let Some(id) = routed {
        for gateway in gateway_nodes(&id, gateways) {
            nodes = nodes
                .push(arrow(fl!("ui-profile-path-ssh-tunnel")))
                .push(node(gateway));
        }
    }
    // The last arrow names the session, direct or not, as the C# one.
    nodes = nodes
        .push(arrow(session_kind(draft.protocol)))
        .push(node(Node {
            title: fl!("ui-profile-path-destination"),
            captions: vec![destination_caption(draft)],
        }));
    column![
        dialog_parts::section(fl!("ui-profile-path-title"), Some(headline)),
        container(nodes.wrap().vertical_spacing(spacing::SM))
            .center_x(Length::Fill)
            .padding(Padding::ZERO.top(spacing::LG)),
    ]
    .into()
}

/// The gateways the session goes through, the one reached first first, the route ending at
/// `last`, as the C# `BuildGatewayRouteText` follows the parents; a gateway the list lacks
/// is said not chosen, as the C# says a gateway not found.
fn gateway_nodes(last: &ProfileId, gateways: &[SshGateway]) -> Vec<Node> {
    let mut route: Vec<&SshGateway> = Vec::new();
    let mut next = Some(last);
    while let Some(id) = next {
        if route.len() >= MAX_GATEWAY_CHAIN_DEPTH || route.iter().any(|seen| &seen.id == id) {
            break;
        }
        let Some(found) = gateways.iter().find(|gateway| &gateway.id == id) else {
            break;
        };
        route.push(found);
        next = found.parent.as_ref();
    }
    if route.is_empty() {
        return vec![Node {
            title: fl!("ui-profile-path-gateway"),
            captions: vec![
                fl!("ui-profile-path-no-gateway"),
                fl!("ui-profile-path-no-ssh-gateway"),
            ],
        }];
    }
    route
        .into_iter()
        .rev()
        .map(|gateway| Node {
            title: fl!("ui-profile-path-gateway"),
            captions: vec![
                server_text(&gateway.name),
                display_address(&server_text(&gateway.host), gateway.port),
            ],
        })
        .collect()
}

/// What this computer's node says, as the C# `ClientNodeCaption`: an outbound connection
/// when direct; through a gateway, the local tunnel's port when a program started outside
/// the application listens on one, else nothing.
fn client_caption(draft: &ProfileDraft) -> Option<String> {
    if draft.routed_gateway().is_none() {
        return Some(fl!("ui-profile-path-client-direct"));
    }
    if !listens_locally(draft) {
        return None;
    }
    let port = draft.value(ProfileField::LocalTunnelPort).to_owned();
    let shown = if draft.is_on(ProfileToggle::AutoTunnelPort) {
        fl!("ui-profile-tunnel-port-auto", port = port)
    } else {
        port
    };
    Some(fl!("ui-profile-path-client-tunnel", port = shown))
}

/// Whether the session, through a gateway, is opened by a program listening on a local
/// port: `PuTTY`, Remote Desktop Connection, or `PowerShell` for `WinRM`.
fn listens_locally(draft: &ProfileDraft) -> bool {
    match draft.protocol {
        DraftProtocol::Ssh => draft.ssh_mode == SshMode::External,
        DraftProtocol::Rdp => draft.rdp_extras.external,
        DraftProtocol::WinRm => true,
        DraftProtocol::Sftp
        | DraftProtocol::Vnc
        | DraftProtocol::Telnet
        | DraftProtocol::Ftp
        | DraftProtocol::Citrix
        | DraftProtocol::Local => false,
    }
}

/// What the destination's node says, as the C# `DestinationNodeCaption`: its address and
/// port, or that it is the destination server while no host is typed.
fn destination_caption(draft: &ProfileDraft) -> String {
    let host = draft.host.trim();
    if host.is_empty() {
        return fl!("ui-profile-path-destination-server");
    }
    let port = draft
        .port
        .trim()
        .parse::<u16>()
        .ok()
        .filter(|port| *port != 0)
        .unwrap_or_else(|| draft.default_port());
    display_address(&server_text(host), port)
}

/// The session the destination is reached with, as the C# `SessionKindLabel`.
fn session_kind(protocol: DraftProtocol) -> String {
    match protocol {
        DraftProtocol::Rdp => fl!("ui-profile-path-session-rdp"),
        DraftProtocol::Ftp => fl!("ui-profile-path-session-ftp"),
        DraftProtocol::Sftp => fl!("ui-profile-path-session-sftp"),
        DraftProtocol::Ssh => fl!("ui-profile-path-session-ssh"),
        DraftProtocol::WinRm => fl!("ui-profile-path-session-winrm"),
        DraftProtocol::Vnc => fl!("ui-profile-path-session-vnc"),
        DraftProtocol::Telnet => fl!("ui-profile-path-session-telnet"),
        DraftProtocol::Citrix => fl!("ui-profile-path-session-citrix"),
        DraftProtocol::Local => fl!("ui-profile-path-session-local"),
    }
}

/// A node, as the C# `DiagramNodeStyle`: its title semi-bold, its captions as hints.
fn node<'a>(node: Node) -> Element<'a, Message> {
    let mut content = Column::new()
        .spacing(spacing::XS)
        .push(text(node.title).font(styles::SEMIBOLD));
    for caption in node.captions {
        content = content.push(dialog_parts::hint(caption));
    }
    container(content)
        .padding(spacing::MD)
        .width(NODE_WIDTH)
        .style(styles::card)
        .into()
}

/// An arrow between two nodes, `caption` under it, as the C# `ArrowCaptionStyle`.
fn arrow<'a>(caption: String) -> Element<'a, Message> {
    container(
        column![
            text(fl!("ui-profile-path-arrow")).size(font_size::TITLE),
            text(caption)
                .size(font_size::CAPTION)
                .style(text::secondary)
                .align_x(iced::alignment::Horizontal::Center),
        ]
        .spacing(spacing::XS)
        .align_x(Alignment::Center),
    )
    .padding(Padding::ZERO.top(ARROW_TOP))
    .center_x(ARROW_WIDTH)
    .into()
}
