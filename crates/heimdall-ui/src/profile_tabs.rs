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

//! The tabs of the profile form, as the C# server dialog's: General holds the connection
//! basics and the credentials, Options the protocol's options, Network the gateway routing,
//! Info the organization and the metadata.

use heimdall_app::profile_draft::{DraftError, DraftProtocol, ProfileField};

use crate::i18n::fl;

/// A tab of the profile form.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum ProfileTab {
    /// The name, the server and its port, the address test, the credentials; a local
    /// shell's program, a Citrix application's `StoreFront`.
    #[default]
    General,
    /// Session logging, then the protocol's own options.
    Options,
    /// The SSH gateway, its forwards, and for RDP the RD Gateway.
    Network,
    /// The folder, the environment, the favourite mark, the tags, the MAC address and the
    /// password manager's entry.
    Info,
}

impl ProfileTab {
    /// The tabs of a `protocol` form, in the C# order: Network only for the protocols that
    /// can go through a gateway, the others having nothing to show there.
    #[must_use]
    pub fn of(protocol: DraftProtocol) -> Vec<Self> {
        [Self::General, Self::Options, Self::Network, Self::Info]
            .into_iter()
            .filter(|tab| *tab != Self::Network || protocol.routes_through_gateway())
            .collect()
    }

    /// The tab showing `field`, where a form refused for it opens.
    #[must_use]
    pub fn holding(field: ProfileField) -> Self {
        match field {
            ProfileField::FixedWidth
            | ProfileField::FixedHeight
            | ProfileField::ResizeDelay
            | ProfileField::IcaFile
            | ProfileField::WorkingDirectory => Self::Options,
            ProfileField::SocksPort
            | ProfileField::RemoteBindPort
            | ProfileField::RemoteLocalPort
            | ProfileField::LocalTunnelPort
            | ProfileField::RdGateway => Self::Network,
            ProfileField::Group
            | ProfileField::Tags
            | ProfileField::MacAddress
            | ProfileField::VaultEntry => Self::Info,
            ProfileField::Name
            | ProfileField::Host
            | ProfileField::Port
            | ProfileField::StoreFrontUrl
            | ProfileField::AppName
            | ProfileField::WebPath
            | ProfileField::Username
            | ProfileField::Domain
            | ProfileField::KeyPath
            | ProfileField::LocalProgram
            | ProfileField::LocalArguments => Self::General,
        }
    }

    /// How many of `errors` are on this tab, as the C# `GeneralTabErrorCount`,
    /// `OptionsTabErrorCount` and `NetworkTabErrorCount` count them.
    #[must_use]
    pub fn error_count(self, errors: &[DraftError]) -> usize {
        errors
            .iter()
            .filter(|error| Self::holding(error.field()) == self)
            .count()
    }

    /// Its name, as the C# tab's header.
    #[must_use]
    pub fn label(self) -> String {
        match self {
            Self::General => fl!("ui-profile-tab-general"),
            Self::Options => fl!("ui-profile-tab-options"),
            Self::Network => fl!("ui-profile-tab-network"),
            Self::Info => fl!("ui-profile-tab-info"),
        }
    }
}

/// The place after (`forward`) or before `at`, a tab with its pill sub-tab, among `tabs`,
/// the cycle of Ctrl+Tab and Ctrl+Shift+Tab: the pills of the Options tab are visited in
/// turn while `pills` are enabled, then the next tab, wrapping round at the ends.
#[must_use]
pub fn step(
    tabs: &[ProfileTab],
    at: (ProfileTab, RdpSubTab),
    pills: bool,
    forward: bool,
) -> (ProfileTab, RdpSubTab) {
    let (tab, sub) = at;
    if pills && tab == ProfileTab::Options {
        let place = RdpSubTab::ALL.iter().position(|known| *known == sub);
        if let Some(place) = place {
            if forward && place + 1 < RdpSubTab::ALL.len() {
                return (tab, RdpSubTab::ALL[place + 1]);
            }
            if !forward && place > 0 {
                return (tab, RdpSubTab::ALL[place - 1]);
            }
        }
    }
    let Some(index) = tabs.iter().position(|known| *known == tab) else {
        return at;
    };
    let count = tabs.len();
    let next = tabs[if forward {
        (index + 1) % count
    } else {
        (index + count - 1) % count
    }];
    // Entering the options from the left lands on their first pill, from the right on the last.
    let sub = if next == ProfileTab::Options && !forward {
        RdpSubTab::ALL[RdpSubTab::ALL.len() - 1]
    } else if next == ProfileTab::Options {
        RdpSubTab::ALL[0]
    } else {
        sub
    };
    (next, sub)
}

/// A pill sub-tab of the RDP options, as the C# `SegmentedTabControlStyle` of the Options
/// tab: Display & Audio, Devices, Performance, Behavior.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum RdpSubTab {
    /// The audio mode, colour depth, resolution and monitors.
    #[default]
    DisplayAudio,
    /// What this computer shares with the server.
    Devices,
    /// The connection's boxes, the visual experience and UDP.
    Performance,
    /// The security boxes, the administrative session and full screen.
    Behavior,
}

impl RdpSubTab {
    /// The sub-tabs, in the C# order.
    pub const ALL: [Self; 4] = [
        Self::DisplayAudio,
        Self::Devices,
        Self::Performance,
        Self::Behavior,
    ];

    /// Its name, as the C# sub-tab's header.
    #[must_use]
    pub fn label(self) -> String {
        match self {
            Self::DisplayAudio => fl!("ui-profile-rdp-tab-display-audio"),
            Self::Devices => fl!("ui-profile-rdp-tab-devices"),
            Self::Performance => fl!("ui-profile-rdp-tab-performance"),
            Self::Behavior => fl!("ui-profile-rdp-tab-behavior"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ctrl_tab_visits_the_pills_of_the_options_then_the_next_tab_and_wraps() {
        use ProfileTab::{General, Info, Network, Options};
        let tabs = [General, Options, Network, Info];
        let first = RdpSubTab::DisplayAudio;
        let last = RdpSubTab::Behavior;
        assert_eq!(step(&tabs, (General, first), true, true), (Options, first));
        assert_eq!(
            step(&tabs, (Options, first), true, true),
            (Options, RdpSubTab::Devices)
        );
        assert_eq!(step(&tabs, (Options, last), true, true), (Network, last));
        assert_eq!(step(&tabs, (Network, last), true, false), (Options, last));
        assert_eq!(
            step(&tabs, (Options, last), true, false),
            (Options, RdpSubTab::Performance)
        );
        assert_eq!(step(&tabs, (Options, first), true, false), (General, first));
        // Pills disabled: the options are one stop.
        assert_eq!(step(&tabs, (Options, first), false, true), (Network, first));
        assert_eq!(step(&tabs, (Info, first), false, true).0, General);
        assert_eq!(step(&tabs, (General, first), false, false).0, Info);
    }

    #[test]
    fn the_rdp_sub_tabs_are_the_csharp_four_in_order_each_with_a_name() {
        assert_eq!(RdpSubTab::ALL.len(), 4);
        assert_eq!(RdpSubTab::default(), RdpSubTab::DisplayAudio);
        let labels: std::collections::HashSet<String> =
            RdpSubTab::ALL.iter().map(|tab| tab.label()).collect();
        assert_eq!(labels.len(), 4, "each sub-tab has a name of its own");
    }

    #[test]
    fn network_is_a_tab_only_of_the_protocols_a_gateway_routes() {
        for protocol in DraftProtocol::ALL {
            assert_eq!(
                ProfileTab::of(protocol).contains(&ProfileTab::Network),
                protocol.routes_through_gateway(),
                "{protocol:?}"
            );
            assert_eq!(ProfileTab::of(protocol)[0], ProfileTab::General);
        }
    }

    #[test]
    fn each_tab_counts_the_errors_on_its_own_fields() {
        let errors = [
            DraftError::NameMissing,
            DraftError::PortInvalid,
            DraftError::FixedWidthInvalid,
            DraftError::SocksPortInvalid,
            DraftError::RdGatewayInvalid,
        ];
        assert_eq!(ProfileTab::General.error_count(&errors), 2);
        assert_eq!(ProfileTab::Options.error_count(&errors), 1);
        assert_eq!(ProfileTab::Network.error_count(&errors), 2);
        assert_eq!(ProfileTab::Info.error_count(&errors), 0);
        assert_eq!(ProfileTab::General.error_count(&[]), 0);
    }

    #[test]
    fn a_field_refused_opens_the_tab_holding_it() {
        assert_eq!(ProfileTab::holding(ProfileField::Name), ProfileTab::General);
        assert_eq!(
            ProfileTab::holding(ProfileField::FixedWidth),
            ProfileTab::Options
        );
        assert_eq!(
            ProfileTab::holding(ProfileField::SocksPort),
            ProfileTab::Network
        );
        assert_eq!(
            ProfileTab::holding(ProfileField::MacAddress),
            ProfileTab::Info
        );
    }
}
