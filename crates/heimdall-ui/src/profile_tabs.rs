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

#[cfg(test)]
mod tests {
    use super::*;

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
