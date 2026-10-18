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

//! The "Route via" bar of the network tools, as the C# combo of each view (`CmbRouteVia`,
//! `GatewayRouteSelector`): "Direct" or a saved SSH gateway, Direct by default.
//!
//! The C# starts every tool on Direct, whatever the selected session uses, and so does this.
//! The bar is shown on a tool that can be routed ([`RouteSupport::Tcp`]) and on one that
//! cannot ([`RouteSupport::Refused`]): on the second, choosing a gateway says why the tool
//! will not run, and its run is refused, never made from this computer instead.

use std::fmt;

use heimdall_app::tool_route::{GatewayChoice, RouteSupport, ToolRoute};
use heimdall_app::tools::ToolId;
use heimdall_app::{App, TabId};
use heimdall_core::profile::ProfileId;
use heimdall_ssh::ConnectError;
use iced::widget::{column, container, pick_list, row, text};
use iced::{Alignment, Element, Length};

use super::ToolMessage;
use crate::i18n::fl;
use crate::shell::Message;
use crate::styles;
use crate::tokens::{font_size, spacing};

/// Messages of the bar.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RouteMessage {
    /// The user picked a gateway, or Direct for `None`.
    Chosen(Option<ProfileId>),
}

/// An entry of the drop-down.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Entry {
    id: Option<ProfileId>,
    label: String,
}

impl fmt::Display for Entry {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.label)
    }
}

/// What the bar of a tool tab holds.
#[derive(Debug, Default)]
pub struct RouteBar {
    /// The gateways to choose from, as the core lists them now.
    gateways: Vec<GatewayChoice>,
    /// The gateway chosen; `None` is Direct, the default.
    selected: Option<ProfileId>,
    /// The gateway chosen could not be found when a run was asked.
    unavailable: bool,
}

impl RouteBar {
    /// Follows the core's gateways: the list as it is now. A change is not a choice: the
    /// gateway chosen stays chosen, and if it is gone the run is refused when asked.
    pub fn set_gateways(&mut self, gateways: Vec<GatewayChoice>) {
        if self.gateways != gateways {
            self.gateways = gateways;
        }
    }

    /// The gateway chosen, `None` for Direct.
    #[cfg(test)]
    #[must_use]
    pub fn selected(&self) -> Option<&ProfileId> {
        self.selected.as_ref()
    }

    /// Applies a message of the bar.
    pub fn update(&mut self, message: RouteMessage) {
        match message {
            RouteMessage::Chosen(id) => {
                self.selected = id;
                self.unavailable = false;
            }
        }
    }

    /// What a run of `tool` goes through, asked now: `Ok(None)` for a direct run,
    /// `Ok(Some(route))` through the gateway chosen, and `Err` when the run must not be made
    /// at all: a tool that cannot be routed with a gateway chosen, or a gateway that is no
    /// longer there. Never a direct run in place of a routed one.
    ///
    /// # Errors
    ///
    /// The run is refused; the bar says why.
    pub fn route_for(&mut self, tool: ToolId, app: &App) -> Result<Option<ToolRoute>, Refused> {
        let Some(id) = &self.selected else {
            return Ok(None);
        };
        match tool.route_support() {
            RouteSupport::NotOffered => Ok(None),
            RouteSupport::Refused => Err(Refused),
            RouteSupport::Tcp => {
                if let Some(route) = app.tool_route(id) {
                    self.unavailable = false;
                    Ok(Some(route))
                } else {
                    self.unavailable = true;
                    Err(Refused)
                }
            }
        }
    }

    /// Whether the bar is drawn for a tool of `support`: not on a tool that is not offered a
    /// gateway, and not when there is no gateway to choose.
    #[must_use]
    pub fn shown(&self, support: RouteSupport) -> bool {
        support != RouteSupport::NotOffered && !self.gateways.is_empty()
    }

    /// The bar of a tool of `support`, or nothing when it is not [`shown`](Self::shown).
    pub fn view(&self, tab: TabId, support: RouteSupport) -> Option<Element<'_, Message>> {
        if !self.shown(support) {
            return None;
        }
        let mut entries = vec![Entry {
            id: None,
            label: fl!("ui-tool-route-direct"),
        }];
        entries.extend(self.gateways.iter().map(|gateway| Entry {
            id: Some(gateway.id.clone()),
            label: gateway.label.clone(),
        }));
        let current = entries
            .iter()
            .find(|entry| entry.id == self.selected)
            .cloned();
        let picker = pick_list(entries, current, move |entry: Entry| {
            Message::Tool(tab, ToolMessage::Route(RouteMessage::Chosen(entry.id)))
        })
        .style(styles::pick_list)
        .width(Length::Fill);
        let mut bar = column![
            row![
                text(fl!("ui-tool-route-label")).size(font_size::BODY),
                picker
            ]
            .spacing(spacing::SM)
            .align_y(Alignment::Center)
        ]
        .spacing(spacing::XS);
        if let Some(said) = self.notice(support) {
            bar = bar.push(super::error_line(said));
        } else if self.selected.is_some() {
            bar = bar.push(
                text(fl!("ui-tool-route-hint"))
                    .size(font_size::CAPTION)
                    .style(text::secondary),
            );
        }
        Some(
            container(bar)
                .padding([0.0, spacing::LG])
                .width(Length::Fill)
                .into(),
        )
    }

    /// What is said under the picker, if anything is wrong with the choice.
    fn notice(&self, support: RouteSupport) -> Option<String> {
        self.selected.as_ref()?;
        match support {
            RouteSupport::Refused => Some(fl!("ui-tool-route-refused")),
            RouteSupport::Tcp if self.unavailable => Some(fl!("ui-tool-route-unavailable")),
            RouteSupport::Tcp | RouteSupport::NotOffered => None,
        }
    }
}

/// A run that must not be made: the bar says why.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Refused;

/// Why the gateway could not be opened, in the user's language: a host key not yet trusted
/// says what to do, the rest is the sessions' own wording of the failure.
#[must_use]
pub fn failure_text(error: ConnectError) -> String {
    let detail = match error {
        ConnectError::UnknownHostKey { host, port, .. } => {
            return fl!("ui-tool-route-error-untrusted", host = host, port = port);
        }
        other => crate::texts::error(&heimdall_app::UiError::from(other)),
    };
    fl!("ui-tool-route-error-gateway", detail = detail)
}

/// Stand-ins for a gateway, for the tests of the tools that run through one: a gateway that
/// connects onward to a listener of this computer, one that refuses every channel, and a
/// listener where a direct connection would land.
#[cfg(test)]
pub(super) mod doubles {
    use std::net::SocketAddr;
    use std::time::Duration;

    use heimdall_ssh::ConnectError;
    use heimdall_ssh::socks::Opener;
    use tokio::net::{TcpListener, TcpStream};

    /// A gateway that connects onward to one address of this computer, whatever it is asked.
    pub struct Loopback(pub SocketAddr);

    impl Opener for Loopback {
        type Stream = TcpStream;

        async fn open(&self, _host: String, _port: u16) -> Result<TcpStream, ConnectError> {
            TcpStream::connect(self.0)
                .await
                .map_err(ConnectError::Network)
        }
    }

    /// A gateway that opens no channel.
    pub struct Refusing;

    impl Opener for Refusing {
        type Stream = TcpStream;

        async fn open(&self, host: String, port: u16) -> Result<TcpStream, ConnectError> {
            tokio::task::yield_now().await;
            Err(ConnectError::JumpRefused { host, port })
        }
    }

    /// A listener where a direct connection would land, and whether anyone came to it.
    pub async fn bait() -> (u16, tokio::task::JoinHandle<bool>) {
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
        let port = listener.local_addr().expect("address").port();
        let watch = tokio::spawn(async move {
            tokio::time::timeout(Duration::from_millis(500), listener.accept())
                .await
                .is_ok()
        });
        (port, watch)
    }
}

#[cfg(test)]
mod tests {
    use heimdall_app::tools::ToolContext;
    use heimdall_app::{AppConfig, Message as AppMessage, ToolsMessage};
    use heimdall_core::profile::SshGateway;
    use heimdall_core::store::ProfileStore;
    use heimdall_ssh::AgentSource;
    use heimdall_term::GridSize;

    use super::super::{PingMessage, PortScanMessage, ToolPanes};
    use super::*;

    fn choice(id: &str) -> GatewayChoice {
        GatewayChoice {
            id: ProfileId::new(id),
            label: format!("{id} (host:22)"),
        }
    }

    #[test]
    fn a_new_bar_is_on_direct_whatever_gateways_there_are() {
        let mut bar = RouteBar::default();
        bar.set_gateways(vec![choice("g")]);
        assert_eq!(bar.selected(), None);
    }

    #[test]
    fn a_choice_stays_when_the_list_changes_and_direct_is_one_pick_away() {
        let mut bar = RouteBar::default();
        bar.set_gateways(vec![choice("g")]);
        bar.update(RouteMessage::Chosen(Some(ProfileId::new("g"))));
        bar.set_gateways(vec![choice("g"), choice("h")]);
        assert_eq!(bar.selected(), Some(&ProfileId::new("g")));
        bar.update(RouteMessage::Chosen(None));
        assert_eq!(bar.selected(), None);
    }

    #[test]
    fn a_gateway_chosen_for_a_tool_that_cannot_be_routed_says_so() {
        let mut bar = RouteBar::default();
        bar.update(RouteMessage::Chosen(Some(ProfileId::new("g"))));
        assert!(bar.notice(RouteSupport::Refused).is_some());
        assert!(bar.notice(RouteSupport::Tcp).is_none());
        bar.update(RouteMessage::Chosen(None));
        assert!(bar.notice(RouteSupport::Refused).is_none());
    }

    #[test]
    fn the_bar_is_shown_only_with_a_gateway_to_choose_and_a_tool_that_is_offered_one() {
        let mut bar = RouteBar::default();
        assert!(!bar.shown(RouteSupport::Tcp), "no gateway saved");
        bar.set_gateways(vec![choice("g")]);
        assert!(bar.shown(RouteSupport::Tcp));
        assert!(bar.shown(RouteSupport::Refused));
        assert!(!bar.shown(RouteSupport::NotOffered));
    }

    /// The tool tabs as the window makes them, over an app that holds gateway `edge`.
    struct Window {
        _dir: tempfile::TempDir,
        app: App,
        panes: ToolPanes,
    }

    impl Window {
        fn with_gateway() -> Self {
            let dir = tempfile::tempdir().expect("dir");
            let profiles_file = dir.path().join("profiles.toml");
            let mut store = ProfileStore::open(&profiles_file).expect("store");
            store.merge_gateways(vec![SshGateway {
                id: ProfileId::new("edge"),
                name: "Edge".to_owned(),
                host: "127.0.0.1".to_owned(),
                port: 2222,
                username: Some("jump".to_owned()),
                key_path: None,
                parent: None,
            }]);
            store.save().expect("save");
            let app = App::new(AppConfig {
                profiles_file,
                known_hosts: dir.path().join("known_hosts"),
                legacy_dir: None,
                agent: AgentSource::Disabled,
                initial_grid: GridSize { cols: 80, rows: 24 },
                files_start: dir.path().to_owned(),
                system_credentials: heimdall_app::SystemCredentials::memory(),
            });
            Self {
                _dir: dir,
                app,
                panes: ToolPanes::default(),
            }
        }

        fn open(&mut self, tool: ToolId) -> TabId {
            let _ = self.app.update(AppMessage::Tools(ToolsMessage::Open {
                tool,
                title: String::new(),
                context: ToolContext::default(),
            }));
            self.panes.sync(&self.app);
            self.app.tabs.last().expect("its tab").id
        }

        fn send(&mut self, tab: TabId, message: ToolMessage) {
            let _ = self.panes.update(tab, message, None, &self.app);
        }

        fn choose(&mut self, tab: TabId, id: Option<&str>) {
            let id = id.map(ProfileId::new);
            self.send(tab, ToolMessage::Route(RouteMessage::Chosen(id)));
        }

        fn running(&self, tab: TabId) -> bool {
            let state = self.panes.get(tab).expect("its state");
            format!("{:?}", state.pane).contains("running: true")
        }

        fn route(&self, tab: TabId) -> &RouteBar {
            &self.panes.get(tab).expect("its state").route
        }
    }

    fn type_host(window: &mut Window, tab: TabId, message: fn(String) -> ToolMessage) {
        window.send(tab, message("db.lab".to_owned()));
    }

    #[test]
    fn a_tool_opens_on_direct_and_the_gateways_are_offered() {
        let mut window = Window::with_gateway();
        let tab = window.open(ToolId::PortScan);
        assert_eq!(window.route(tab).selected(), None, "Direct by default");
        assert!(
            window.route(tab).shown(RouteSupport::Tcp),
            "the gateway is offered"
        );
    }

    #[test]
    fn a_tool_that_needs_icmp_does_not_run_through_a_gateway_nor_directly() {
        let mut window = Window::with_gateway();
        let tab = window.open(ToolId::Ping);
        type_host(&mut window, tab, |host| {
            ToolMessage::Ping(PingMessage::HostChanged(host))
        });
        window.choose(tab, Some("edge"));
        window.send(tab, ToolMessage::Ping(PingMessage::Start));
        assert!(!window.running(tab), "refused, not run from this computer");
        // Back on Direct it runs.
        window.choose(tab, None);
        window.send(tab, ToolMessage::Ping(PingMessage::Start));
        assert!(window.running(tab));
    }

    #[test]
    fn a_tcp_tool_runs_through_a_gateway_that_is_there() {
        let mut window = Window::with_gateway();
        let tab = window.open(ToolId::PortScan);
        type_host(&mut window, tab, |host| {
            ToolMessage::PortScan(PortScanMessage::HostChanged(host))
        });
        window.choose(tab, Some("edge"));
        window.send(tab, ToolMessage::PortScan(PortScanMessage::Start));
        assert!(window.running(tab));
        assert!(!window.route(tab).unavailable);
    }

    #[test]
    fn a_tcp_tool_whose_gateway_is_gone_does_not_run_directly() {
        let mut window = Window::with_gateway();
        let tab = window.open(ToolId::PortScan);
        type_host(&mut window, tab, |host| {
            ToolMessage::PortScan(PortScanMessage::HostChanged(host))
        });
        window.choose(tab, Some("gone"));
        window.send(tab, ToolMessage::PortScan(PortScanMessage::Start));
        assert!(!window.running(tab), "no fall back to a direct scan");
        assert!(window.route(tab).unavailable);
        window.choose(tab, None);
        assert!(!window.route(tab).unavailable, "choosing again clears it");
        window.send(tab, ToolMessage::PortScan(PortScanMessage::Start));
        assert!(window.running(tab), "Direct runs");
    }

    #[test]
    fn the_newly_routed_tools_are_offered_the_bar_and_do_not_run_without_their_gateway() {
        use super::super::{FirewallTesterMessage, SmbMessage, TlsAuditMessage, WhoisMessage};

        let mut window = Window::with_gateway();
        for tool in [
            ToolId::Whois,
            ToolId::TlsAudit,
            ToolId::Smb,
            ToolId::FwTest,
            ToolId::SecNumCloud,
        ] {
            let tab = window.open(tool);
            assert!(window.route(tab).shown(RouteSupport::Tcp), "{tool:?}");
        }
        let tab = window.open(ToolId::Whois);
        window.send(
            tab,
            ToolMessage::Whois(WhoisMessage::DomainChanged("example.com".to_owned())),
        );
        window.choose(tab, Some("gone"));
        window.send(tab, ToolMessage::Whois(WhoisMessage::Lookup));
        assert!(!window.running(tab), "no fall back to a direct lookup");
        assert!(window.route(tab).unavailable);

        let tab = window.open(ToolId::Smb);
        window.send(
            tab,
            ToolMessage::Smb(SmbMessage::HostChanged("nas.lab".to_owned())),
        );
        window.choose(tab, Some("gone"));
        window.send(tab, ToolMessage::Smb(SmbMessage::Enumerate));
        assert!(window.route(tab).unavailable, "refused, not run directly");

        let tab = window.open(ToolId::TlsAudit);
        window.send(
            tab,
            ToolMessage::TlsAudit(TlsAuditMessage::HostChanged("tls.lab".to_owned())),
        );
        window.choose(tab, Some("gone"));
        window.send(tab, ToolMessage::TlsAudit(TlsAuditMessage::Audit));
        assert!(window.route(tab).unavailable, "refused, not run directly");

        let tab = window.open(ToolId::FwTest);
        window.choose(tab, Some("gone"));
        window.send(tab, ToolMessage::FwTest(FirewallTesterMessage::Start));
        assert!(window.route(tab).unavailable, "refused, not run directly");
    }
}
