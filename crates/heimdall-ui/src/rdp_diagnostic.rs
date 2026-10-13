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

//! The "Diagnostic details" of a failed RDP connection, as the C# failure card shows them
//! under the error (`SessionPaneControl.xaml`) and copies them with it: the stage it failed
//! at, the protocol's code, and the technical detail.
//!
//! They are made of the failure alone, which carries no credential: a status or a code the
//! server sent, `IronRDP`'s words for it, the system's message. Text that came from the
//! server or the system is made safe to show, as every such text is.

use heimdall_app::{UiError, server_text};
use heimdall_core::profile::display_address;
use heimdall_rdp::{Ending, error_info_description};

use crate::i18n::fl;

/// Where an RDP connection failed: the C# failure card's "Stage", in the C# words where the
/// C# has a stage for it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stage {
    /// The SSH gateways on the way to the server, as the C# `RdpTunnel`.
    Tunnel,
    /// Reaching the server, before it answered.
    Connection,
    /// The security protocol the server and this side agree on.
    Negotiation,
    /// TLS and the server's certificate.
    Certificate,
    /// Network Level Authentication: the logon.
    Authentication,
    /// The server or the network ended it, as the C# `RdpActiveXDisconnect`.
    Disconnect,
}

impl Stage {
    /// What the card says of it.
    #[must_use]
    pub fn label(self) -> String {
        match self {
            Self::Tunnel => fl!("ui-rdp-stage-tunnel"),
            Self::Connection => fl!("ui-rdp-stage-connection"),
            Self::Negotiation => fl!("ui-rdp-stage-negotiation"),
            Self::Certificate => fl!("ui-rdp-stage-certificate"),
            Self::Authentication => fl!("ui-rdp-stage-authentication"),
            Self::Disconnect => fl!("ui-rdp-stage-disconnect"),
        }
    }
}

/// The diagnostic details of a failure: its stage, and the code and the detail when there
/// are some.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Details {
    /// Where it failed.
    pub stage: Stage,
    /// The protocol's code: an NTSTATUS, a Set Error Info code, a negotiation failure code.
    pub code: Option<String>,
    /// What the library or the system said of it.
    pub detail: Option<String>,
}

/// A protocol code as its specifications write it: eight hexadecimal digits.
fn hex(code: u32) -> String {
    format!("0x{code:08X}")
}

/// What the failure of an RDP connection with `error` details; `dropped` when its session
/// was open before. `None` for what is no failure of the connection (a cancel, a refused
/// certificate, a check before connecting) or names no stage.
#[must_use]
pub fn of(error: &UiError, dropped: bool) -> Option<Details> {
    // Reaching the server, or the session it had: a drop is the network ending it.
    let reaching = if dropped {
        Stage::Disconnect
    } else {
        Stage::Connection
    };
    let (stage, code, detail) = match error {
        UiError::Network { detail, .. } | UiError::RdpProtocol { detail } => {
            (reaching, None, Some(server_text(detail)))
        }
        UiError::Timeout => (reaching, None, None),
        UiError::ConnectionLost => (Stage::Disconnect, None, None),
        UiError::SecurityRefused { detail, code } => {
            (Stage::Negotiation, code.map(hex), Some(server_text(detail)))
        }
        UiError::HostKeyChanged { target: None, .. }
        | UiError::PinnedCertificateInvalid { .. }
        | UiError::RdpServerNotAuthenticated => (Stage::Certificate, None, None),
        UiError::KnownHosts { detail } => (Stage::Certificate, None, Some(server_text(detail))),
        UiError::RdpRefused { status, .. } => (Stage::Authentication, status.map(hex), None),
        UiError::RdpEnded { ending, code } => (
            Stage::Disconnect,
            code.map(hex),
            match ending {
                // The server's own words, made safe when they came in.
                Ending::Other(words) => Some(words.clone()),
                _ => code.and_then(error_info_description),
            },
        ),
        UiError::JumpRefused { host, port } => (
            Stage::Tunnel,
            None,
            Some(display_address(&server_text(host), *port)),
        ),
        UiError::HostKeyChanged {
            target: Some(target),
            ..
        } => (Stage::Tunnel, None, Some(server_text(&target.to_string()))),
        UiError::Route(_)
        | UiError::AuthenticationFailed { .. }
        | UiError::KeyFile { .. }
        | UiError::HostKeyAlgorithmMismatch { .. }
        | UiError::Disconnected { .. } => (Stage::Tunnel, None, None),
        _ => return None,
    };
    Some(Details {
        stage,
        code,
        detail: detail.filter(|detail| !detail.trim().is_empty()),
    })
}

impl Details {
    /// The card's rows, label then value: the stage always, the code and the detail when
    /// there are some, as the C# card hides them otherwise.
    #[must_use]
    pub fn rows(&self) -> Vec<(String, String)> {
        let mut rows = vec![(fl!("ui-failure-details-stage"), self.stage.label())];
        if let Some(code) = &self.code {
            rows.push((fl!("ui-failure-details-code"), code.clone()));
        }
        if let Some(detail) = &self.detail {
            rows.push((fl!("ui-failure-details-detail"), detail.clone()));
        }
        rows
    }

    /// The lines copied with the error, one per row, as the C# "Copy error" adds them.
    #[must_use]
    pub fn report_lines(&self) -> Vec<String> {
        self.rows()
            .into_iter()
            .map(|(label, value)| fl!("ui-failure-details-line", label = label, value = value))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use heimdall_app::NetworkFailure;
    use heimdall_rdp::Refusal;

    use super::*;

    /// NTSTATUS of an expired password.
    const STATUS_PASSWORD_EXPIRED: u32 = 0xC000_0071;
    /// Set Error Info `ERRINFO_SERVER_DENIED_CONNECTION`.
    const DENIED: u32 = 0x07;
    /// Set Error Info `ERRINFO_LICENSE_NO_LICENSE_SERVER`.
    const NO_LICENSE_SERVER: u32 = 0x101;

    #[test]
    fn each_cause_has_its_stage_code_and_detail() {
        let refused = of(
            &UiError::RdpRefused {
                refusal: Refusal::PasswordExpired,
                status: Some(STATUS_PASSWORD_EXPIRED),
            },
            false,
        )
        .expect("details");
        assert_eq!(refused.stage, Stage::Authentication);
        assert_eq!(refused.code.as_deref(), Some("0xC0000071"));
        assert_eq!(refused.detail, None);

        let denied = of(
            &UiError::RdpEnded {
                ending: Ending::BadCredentials,
                code: Some(DENIED),
            },
            false,
        )
        .expect("details");
        assert_eq!(denied.stage, Stage::Disconnect);
        assert_eq!(denied.code.as_deref(), Some("0x00000007"));
        assert_eq!(
            denied.detail.as_deref(),
            Some("[Protocol independent error] The server denied the connection")
        );

        let licence = of(
            &UiError::RdpEnded {
                ending: Ending::License,
                code: Some(NO_LICENSE_SERVER),
            },
            false,
        )
        .expect("details");
        assert_eq!(licence.code.as_deref(), Some("0x00000101"));
        assert!(
            licence
                .detail
                .as_deref()
                .is_some_and(|detail| detail.contains("License Server")),
            "{licence:?}"
        );

        let negotiation = of(
            &UiError::SecurityRefused {
                detail: "server requires Enhanced RDP Security with CredSSP".to_owned(),
                code: Some(5),
            },
            false,
        )
        .expect("details");
        assert_eq!(negotiation.stage, Stage::Negotiation);
        assert_eq!(negotiation.code.as_deref(), Some("0x00000005"));

        let network = UiError::Network {
            failure: NetworkFailure::Refused,
            detail: "Connection refused (os error 111)".to_owned(),
        };
        let reaching = of(&network, false).expect("details");
        assert_eq!(reaching.stage, Stage::Connection);
        assert_eq!(reaching.code, None);
        assert_eq!(
            reaching.detail.as_deref(),
            Some("Connection refused (os error 111)")
        );
        assert_eq!(
            of(&network, true).expect("details").stage,
            Stage::Disconnect,
            "a session open before is ended, not reached"
        );

        let tunnel = of(
            &UiError::JumpRefused {
                host: "dc.lab".to_owned(),
                port: 3389,
            },
            false,
        )
        .expect("details");
        assert_eq!(tunnel.stage, Stage::Tunnel);
        assert_eq!(tunnel.detail.as_deref(), Some("dc.lab:3389"));

        assert_eq!(
            of(&UiError::Timeout, false).map(|details| details.stage),
            Some(Stage::Connection)
        );
        assert_eq!(of(&UiError::Cancelled, false), None, "no failure");
        assert_eq!(of(&UiError::CredentialGuardRequired, false), None);
    }

    #[test]
    fn the_rows_and_the_lines_copied_name_each_part_as_the_csharp() {
        let details = Details {
            stage: Stage::Authentication,
            code: Some("0xC0000071".to_owned()),
            detail: None,
        };
        assert_eq!(
            details.rows(),
            [
                (
                    "Stage".to_owned(),
                    "Network Level Authentication".to_owned()
                ),
                ("Code".to_owned(), "0xC0000071".to_owned()),
            ]
        );
        assert_eq!(
            details.report_lines(),
            ["Stage: Network Level Authentication", "Code: 0xC0000071"]
        );
        let disconnect = Details {
            stage: Stage::Disconnect,
            code: None,
            detail: Some("lost".to_owned()),
        };
        assert_eq!(
            disconnect.report_lines(),
            ["Stage: RDP disconnect", "Detail: lost"]
        );
    }

    #[test]
    fn a_detail_from_the_server_is_made_safe() {
        let details = of(
            &UiError::RdpProtocol {
                detail: "bad\u{1b}[31m\u{202e}text".to_owned(),
            },
            false,
        )
        .expect("details");
        let detail = details.detail.expect("detail");
        assert!(!detail.contains('\u{1b}'), "{detail:?}");
        assert!(!detail.contains('\u{202e}'), "{detail:?}");
    }
}
