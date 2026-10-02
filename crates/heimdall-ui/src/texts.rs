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

//! Localised sentences for the values the application core reports.
//!
//! Every user-facing sentence comes from a Fluent key; the `Display` text of an error is
//! never shown on its own, only as the technical detail inside a localised sentence.

use heimdall_app::files::FilesError;
use heimdall_app::profile_draft::DraftError;
use heimdall_app::{KeyProblem, NetworkFailure, StepStatus, UiError, server_text};
use heimdall_core::import::csharp::{Dropped, SkipReason};
use heimdall_core::profile::{FIXED_HEIGHT_MAX, FIXED_SIDE_MIN, FIXED_WIDTH_MAX, display_address};
use heimdall_core::store::RouteError;
use heimdall_files::{LocalNameError, Refusal};
use heimdall_ssh::AuthMethod;

use crate::i18n::fl;

/// Bytes in a kibibyte.
const KIB: f64 = 1024.0;

/// Separator between the items of an inline list.
const LIST_SEPARATOR: &str = ", ";

/// The sentence explaining `error`.
#[must_use]
pub fn error(error: &UiError) -> String {
    match error {
        UiError::InvalidHost => fl!("ui-error-invalid-host"),
        UiError::InvalidUsername => fl!("ui-error-invalid-username"),
        UiError::Network { failure, detail } => match failure {
            NetworkFailure::Refused => fl!("ui-error-network-refused"),
            NetworkFailure::Reset => fl!("ui-error-network-reset"),
            NetworkFailure::TimedOut => fl!("ui-error-network-timed-out"),
            NetworkFailure::Unreachable => fl!("ui-error-network-unreachable"),
            NetworkFailure::Other => fl!("ui-error-network", detail = server_text(detail)),
        },
        UiError::Timeout => fl!("ui-error-timeout"),
        UiError::RdpProtocol { detail } => {
            fl!("ui-error-rdp-protocol", detail = server_text(detail))
        }
        UiError::VncProtocol { detail } => {
            fl!("ui-error-vnc-protocol", detail = server_text(detail))
        }
        UiError::SecurityRefused { detail } => {
            fl!("ui-error-security-refused", detail = server_text(detail))
        }
        UiError::LocalShell { detail } => {
            fl!("ui-error-local-shell", detail = server_text(detail))
        }
        UiError::HostKeyChanged {
            target: None,
            recorded,
            offered,
        } => fl!(
            "ui-error-hostkey-changed",
            recorded = recorded.as_str(),
            offered = offered.as_str()
        ),
        UiError::HostKeyChanged {
            target: Some(target),
            recorded,
            offered,
        } => fl!(
            "ui-error-hostkey-changed-at",
            target = server_text(&target.to_string()),
            recorded = recorded.as_str(),
            offered = offered.as_str()
        ),
        UiError::Route(RouteError::MissingGateway(id)) => {
            fl!("ui-error-gateway-missing", id = server_text(id.as_str()))
        }
        UiError::Route(RouteError::Loop(id)) => {
            fl!("ui-error-gateway-loop", id = server_text(id.as_str()))
        }
        UiError::HostKeyAlgorithmMismatch { recorded } => fl!(
            "ui-error-hostkey-algorithm",
            recorded = recorded.join(LIST_SEPARATOR)
        ),
        UiError::HostCertificateRefused => fl!("ui-error-host-certificate"),
        UiError::KnownHosts { detail } => fl!("ui-error-known-hosts", detail = server_text(detail)),
        UiError::KeyFile { problem, path } => key_problem(problem, path),
        UiError::AuthenticationFailed { tried } if tried.is_empty() => {
            fl!("ui-error-auth-failed-none")
        }
        UiError::AuthenticationFailed { tried } => fl!(
            "ui-error-auth-failed",
            methods = tried
                .iter()
                .copied()
                .map(auth_method)
                .collect::<Vec<_>>()
                .join(LIST_SEPARATOR)
        ),
        UiError::Disconnected {
            server_message: Some(message),
        } if !server_text(message).is_empty() => fl!(
            "ui-error-disconnected-message",
            message = server_text(message)
        ),
        UiError::Disconnected { .. } => fl!("ui-error-disconnected"),
        UiError::ConnectionLost => fl!("ui-error-connection-lost"),
        UiError::Cancelled => fl!("ui-error-cancelled"),
        UiError::PromptTimedOut => fl!("ui-error-prompt-timeout"),
        UiError::PtyRefused => fl!("ui-error-pty-refused"),
        UiError::ShellRefused => fl!("ui-error-shell-refused"),
        UiError::JumpRefused { host, port } => fl!(
            "ui-error-jump-refused",
            target = display_address(&server_text(host), *port)
        ),
        UiError::ProxyPort { port, detail } => {
            fl!(
                "ui-error-proxy-port",
                port = port.to_string(),
                detail = detail.as_str()
            )
        }
        UiError::RemoteForwardRefused { port } => {
            fl!("ui-error-remote-forward", port = port.to_string())
        }
        UiError::SubsystemRefused { name } => {
            fl!("ui-error-subsystem-refused", name = server_text(name))
        }
        UiError::Protocol { detail } => fl!("ui-error-protocol", detail = server_text(detail)),
    }
}

fn key_problem(problem: &KeyProblem, path: &str) -> String {
    match problem {
        KeyProblem::Unreadable => fl!("ui-error-key-unreadable", path = path),
        KeyProblem::UnknownFormat => fl!("ui-error-key-unknown-format", path = path),
        KeyProblem::NeedsPassphrase => fl!("ui-error-key-needs-passphrase", path = path),
        KeyProblem::WrongPassphrase => fl!("ui-error-key-wrong-passphrase", path = path),
        KeyProblem::Invalid => fl!("ui-error-key-invalid", path = path),
    }
}

/// The name of an authentication method.
#[must_use]
pub fn auth_method(method: AuthMethod) -> String {
    match method {
        AuthMethod::Agent => fl!("ui-auth-method-agent"),
        AuthMethod::KeyFile => fl!("ui-auth-method-key-file"),
        AuthMethod::KeyboardInteractive => fl!("ui-auth-method-keyboard-interactive"),
        AuthMethod::Password => fl!("ui-auth-method-password"),
    }
}

/// A setting an imported profile came without.
#[must_use]
pub fn dropped_setting(dropped: Dropped) -> String {
    match dropped {
        Dropped::ExternalClient => fl!("ui-import-dropped-external-client"),
        Dropped::X11Forwarding => fl!("ui-import-dropped-x11"),
        Dropped::WinRmGateway => fl!("ui-import-dropped-winrm-gateway"),
        Dropped::RdpPrinters => fl!("ui-import-dropped-rdp-printers"),
        Dropped::RdpComPorts => fl!("ui-import-dropped-rdp-com-ports"),
        Dropped::RdpSmartCards => fl!("ui-import-dropped-rdp-smart-cards"),
        Dropped::RdpWebcam => fl!("ui-import-dropped-rdp-webcam"),
        Dropped::RdpUsb => fl!("ui-import-dropped-rdp-usb"),
        Dropped::RdpMicrophone => fl!("ui-import-dropped-rdp-microphone"),
        Dropped::RdpMultiMonitor => fl!("ui-import-dropped-rdp-multi-monitor"),
        Dropped::RdpAntiIdle => fl!("ui-import-dropped-rdp-anti-idle"),
    }
}

/// Why an imported profile was left out.
#[must_use]
pub fn skip_reason(reason: &SkipReason) -> String {
    match reason {
        SkipReason::NotSsh(kind) => fl!("ui-import-skip-not-ssh", kind = server_text(kind)),
        SkipReason::NeedsRdGateway => fl!("ui-import-skip-rd-gateway"),
        SkipReason::MissingHost => fl!("ui-import-skip-missing-host"),
        SkipReason::MissingId => fl!("ui-import-skip-missing-id"),
        SkipReason::InvalidPort(port) => {
            fl!("ui-import-skip-invalid-port", port = port.to_string())
        }
        SkipReason::MissingGateway => fl!("ui-import-skip-missing-gateway"),
        SkipReason::GatewayLoop => fl!("ui-import-skip-gateway-loop"),
        SkipReason::NeedsElevation => fl!("ui-import-skip-elevation"),
        SkipReason::NeedsPostConnectCommands => fl!("ui-import-skip-post-connect"),
        SkipReason::UnsafeLocalCommand => fl!("ui-import-skip-unsafe-local"),
        SkipReason::MissingUsername => fl!("ui-import-skip-missing-username"),
        SkipReason::UnknownIdentityMode => fl!("ui-import-skip-unknown-identity"),
    }
}

/// The sentence explaining a Files error.
#[must_use]
pub fn draft_error(error: DraftError) -> String {
    match error {
        DraftError::NameMissing => fl!("ui-profile-error-name-missing"),
        DraftError::HostMissing => fl!("ui-profile-error-host-missing"),
        DraftError::HostInvalid => fl!("ui-profile-error-host-invalid"),
        DraftError::HostHasUser => fl!("ui-profile-error-host-has-user"),
        DraftError::HostHasPort => fl!("ui-profile-error-host-has-port"),
        DraftError::PortInvalid => fl!("ui-profile-error-port-invalid"),
        DraftError::UsernameInvalid => fl!("ui-profile-error-username-invalid"),
        DraftError::ControlCharacter => fl!("ui-profile-error-control"),
        DraftError::UsernameForPassword => fl!("ui-profile-error-username-for-password"),
        DraftError::UsernameMissing => fl!("ui-profile-error-username-missing"),
        DraftError::DomainInvalid => fl!("ui-profile-error-domain-invalid"),
        DraftError::FixedWidthInvalid => fl!(
            "ui-profile-error-fixed-width",
            min = FIXED_SIDE_MIN,
            max = FIXED_WIDTH_MAX
        ),
        DraftError::FixedHeightInvalid => fl!(
            "ui-profile-error-fixed-height",
            min = FIXED_SIDE_MIN,
            max = FIXED_HEIGHT_MAX
        ),
        DraftError::GatewayLoop => fl!("ui-gateway-error-loop"),
        DraftError::SocksPortInvalid => fl!("ui-profile-error-socks-port"),
        DraftError::ArgumentsInvalid => fl!("ui-profile-error-local-arguments"),
        DraftError::RemoteBindPortInvalid => fl!("ui-profile-error-remote-bind-port"),
        DraftError::RemoteLocalPortInvalid => fl!("ui-profile-error-remote-local-port"),
    }
}

/// What became of a post-connect step, as the C# statuses name it.
pub fn step_status(status: StepStatus) -> String {
    match status {
        StepStatus::Running => fl!("ui-post-connect-running"),
        StepStatus::Completed => fl!("ui-post-connect-completed"),
        StepStatus::Failed => fl!("ui-post-connect-failed"),
        StepStatus::Skipped => fl!("ui-post-connect-skipped"),
        StepStatus::Cancelled => fl!("ui-post-connect-cancelled"),
    }
}

/// Why a Files operation failed, in the user's language.
pub fn files_error(error: &FilesError) -> String {
    match error {
        FilesError::Server { refusal, message } => {
            let message = if message.is_empty() {
                refusal_text(*refusal)
            } else {
                message.clone()
            };
            fl!("ui-files-error-server", message = message)
        }
        FilesError::SessionClosed => fl!("ui-files-error-session"),
        FilesError::Local { detail } => fl!("ui-files-error-local", detail = detail.as_str()),
        FilesError::UnsafeName { name, reason } => fl!(
            "ui-files-error-unsafe-name",
            name = name.as_str(),
            reason = name_reason(reason)
        ),
        FilesError::NotAFile => fl!("ui-files-error-not-a-file"),
        FilesError::TooLarge => fl!("ui-files-error-too-large"),
        FilesError::InvalidName => fl!("ui-files-error-invalid-name"),
        FilesError::Exists => fl!("ui-files-error-exists"),
        FilesError::InvalidPermissions => fl!("ui-files-error-invalid-permissions"),
    }
}

fn refusal_text(refusal: Refusal) -> String {
    match refusal {
        Refusal::NoSuchFile => fl!("ui-files-error-no-such-file"),
        Refusal::PermissionDenied => fl!("ui-files-error-permission-denied"),
        Refusal::Unsupported => fl!("ui-files-error-unsupported"),
        Refusal::Failure => fl!("ui-files-error-failure"),
    }
}

/// Why a server name cannot be a local file name.
#[must_use]
pub fn name_reason(reason: &LocalNameError) -> String {
    match reason {
        LocalNameError::NotAName => fl!("ui-files-name-not-a-name"),
        LocalNameError::Separator => fl!("ui-files-name-separator"),
        LocalNameError::Control => fl!("ui-files-name-control"),
        LocalNameError::Forbidden(character) => {
            fl!("ui-files-name-forbidden", character = character.to_string())
        }
        LocalNameError::Reserved => fl!("ui-files-name-reserved"),
        LocalNameError::TrailingDotOrSpace => fl!("ui-files-name-trailing"),
        LocalNameError::TooLong => fl!("ui-files-name-too-long"),
    }
}

/// A size in the largest binary unit that keeps it at or above 1, one decimal past bytes.
#[must_use]
#[allow(
    clippy::cast_precision_loss,
    reason = "a displayed size needs three significant digits, not all of them"
)]
pub fn size(bytes: u64) -> String {
    let value = bytes as f64;
    if value < KIB {
        return fl!("ui-files-size-bytes", value = bytes.to_string());
    }
    let (value, unit) = if value < KIB * KIB {
        (value / KIB, 1)
    } else if value < KIB * KIB * KIB {
        (value / (KIB * KIB), 2)
    } else {
        (value / (KIB * KIB * KIB), 3)
    };
    let shown = format!("{value:.1}");
    match unit {
        1 => fl!("ui-files-size-kib", value = shown),
        2 => fl!("ui-files-size-mib", value = shown),
        _ => fl!("ui-files-size-gib", value = shown),
    }
}

#[cfg(test)]
mod tests {
    use heimdall_app::{KeyProblem, UiError};
    use heimdall_core::import::csharp::SkipReason;
    use heimdall_ssh::AuthMethod;

    use super::{error, skip_reason};

    #[test]
    fn a_network_failure_says_what_the_csharp_one_says() {
        use heimdall_app::NetworkFailure;

        let network = |failure| {
            error(&UiError::Network {
                failure,
                detail: "os detail".to_owned(),
            })
        };
        assert_eq!(network(NetworkFailure::Refused), "Connection refused.");
        assert_eq!(network(NetworkFailure::Reset), "Connection reset.");
        assert_eq!(
            network(NetworkFailure::TimedOut),
            "Connection timed out. Check that the host is reachable."
        );
        assert_eq!(
            network(NetworkFailure::Unreachable),
            "Host or network is unreachable. Check DNS and routing."
        );
        assert!(network(NetworkFailure::Other).contains("os detail"));
    }

    #[test]
    fn values_are_placed_in_the_sentence() {
        let text = error(&UiError::KeyFile {
            problem: KeyProblem::WrongPassphrase,
            path: "/home/u/.ssh/id_ed25519".to_owned(),
        });
        assert!(text.contains("/home/u/.ssh/id_ed25519"), "{text}");
        assert!(text.contains("passphrase"), "{text}");
        assert!(
            !text.contains('\u{2068}'),
            "no bidi isolation marks: {text:?}"
        );
    }

    #[test]
    fn tried_methods_are_named_in_the_language() {
        let text = error(&UiError::AuthenticationFailed {
            tried: vec![AuthMethod::Agent, AuthMethod::Password],
        });
        assert!(text.contains("SSH agent, password"), "{text}");
        let none = error(&UiError::AuthenticationFailed { tried: Vec::new() });
        assert_ne!(none, text);
    }

    #[test]
    fn a_lost_connection_is_said_in_the_csharp_words_not_as_a_server_close() {
        let lost = error(&UiError::ConnectionLost);
        assert_eq!(lost, "Session disconnected unexpectedly.");
        assert_ne!(
            lost,
            error(&UiError::Disconnected {
                server_message: None
            })
        );
    }

    #[test]
    fn a_server_message_is_made_safe_before_it_is_shown() {
        let text = error(&UiError::Disconnected {
            server_message: Some("bye\u{1b}[2J\u{202e}now".to_owned()),
        });
        assert!(
            !text.contains('\u{1b}') && !text.contains('\u{202e}'),
            "{text:?}"
        );
        let blank = error(&UiError::Disconnected {
            server_message: Some("\u{1b}".to_owned()),
        });
        assert_eq!(
            blank,
            error(&UiError::Disconnected {
                server_message: None
            })
        );
    }

    #[test]
    fn skip_reasons_carry_their_value() {
        assert!(skip_reason(&SkipReason::InvalidPort(70000)).contains("70000"));
        assert!(skip_reason(&SkipReason::NotSsh("RDP".to_owned())).contains("RDP"));
    }

    #[test]
    fn technical_details_are_made_safe_too() {
        let text = error(&UiError::Protocol {
            detail: "bad\u{1b}]52;c;AAAA\u{7}name".to_owned(),
        });
        assert!(
            !text.contains('\u{1b}') && !text.contains('\u{7}'),
            "{text:?}"
        );
    }

    #[test]
    fn a_refused_subsystem_is_named() {
        let text = error(&UiError::SubsystemRefused {
            name: "sftp".to_owned(),
        });
        assert!(text.contains("sftp"), "{text}");
    }

    #[test]
    fn sizes_use_binary_units() {
        assert_eq!(super::size(999), "999 B");
        assert_eq!(super::size(1536), "1.5 KiB");
        assert_eq!(super::size(5 * 1024 * 1024 + 99), "5.0 MiB");
        assert_eq!(super::size(3 * 1024 * 1024 * 1024), "3.0 GiB");
    }

    #[test]
    fn a_refused_name_is_explained() {
        let text = super::files_error(&heimdall_app::files::FilesError::UnsafeName {
            name: "C:x".to_owned(),
            reason: heimdall_files::LocalNameError::Forbidden(':'),
        });
        assert!(text.contains("C:x") && text.contains("\":\""), "{text}");
    }
}
