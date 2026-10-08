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

use heimdall_app::citrix::CitrixRefusal;
use heimdall_app::files::FilesError;
use heimdall_app::profile_draft::DraftError;
use heimdall_app::{KeyProblem, NetworkFailure, StepStatus, UiError, server_text};
use heimdall_core::import::csharp::{Dropped, SkipReason};
use heimdall_core::profile::{FIXED_HEIGHT_MAX, FIXED_SIDE_MIN, FIXED_WIDTH_MAX, display_address};
use heimdall_core::store::RouteError;
use heimdall_files::{LocalNameError, Refusal};
use heimdall_rdp::{Ending, Refusal as RdpRefusal};
use heimdall_ssh::AuthMethod;
use heimdall_tls::ValidationIssue;

use crate::i18n::fl;

/// Bytes in a kibibyte.
const KIB: f64 = 1024.0;

/// Separator between the items of an inline list.
const LIST_SEPARATOR: &str = ", ";

/// Why an RDP server refused a logon, as the C# says it: a warning when the account is the
/// user's to fix, an error otherwise.
#[must_use]
pub fn rdp_refusal(refusal: RdpRefusal) -> String {
    let reason = match refusal {
        RdpRefusal::BadCredentials => fl!("ui-rdp-reason-bad-credentials"),
        RdpRefusal::PasswordExpired => fl!("ui-rdp-reason-password-expired"),
        RdpRefusal::AccountLockedOut => fl!("ui-rdp-reason-account-locked-out"),
        RdpRefusal::AccountDisabled => fl!("ui-rdp-reason-account-disabled"),
        RdpRefusal::AccountExpired => fl!("ui-rdp-reason-account-expired"),
        RdpRefusal::TimeOfDayRestriction => fl!("ui-rdp-reason-time-of-day"),
        RdpRefusal::NoAuthenticationAuthority => fl!("ui-rdp-reason-no-authority"),
        RdpRefusal::ClockSkew => fl!("ui-rdp-reason-clock-skew"),
        RdpRefusal::SecurityError => fl!("ui-rdp-reason-security-error"),
    };
    with_severity(refusal.is_account_issue(), &reason)
}

/// Why an RDP server ended a session, as the C# says it. Nothing for a logoff: the C# says
/// nothing of the session the user ended.
#[must_use]
pub fn rdp_ending(ending: &Ending) -> Option<String> {
    let reason = match ending {
        Ending::Logoff => return None,
        Ending::AdminDisconnect => fl!("ui-rdp-reason-admin-disconnect"),
        Ending::BadCredentials => fl!("ui-rdp-reason-bad-credentials"),
        Ending::License => fl!("ui-rdp-reason-license"),
        Ending::Other(words) => {
            return Some(fl!("ui-session-closed-reason", reason = words.as_str()));
        }
    };
    Some(with_severity(ending.is_account_issue(), &reason))
}

/// `reason` after the C# severity word: a warning for the account, an error otherwise.
fn with_severity(account: bool, reason: &str) -> String {
    let severity = if account {
        fl!("ui-rdp-severity-warning")
    } else {
        fl!("ui-rdp-severity-error")
    };
    fl!(
        "ui-rdp-reason-with-severity",
        severity = severity,
        reason = reason
    )
}

/// What a `WinRM` session's first output said went wrong, as the C# says it.
#[must_use]
pub fn winrm_diagnostic(found: heimdall_core::winrm_diagnostic::Diagnostic) -> String {
    use heimdall_core::winrm_diagnostic::Diagnostic;
    match found {
        Diagnostic::NtlmLoopback => fl!("ui-winrm-diagnostic-ntlm-loopback"),
        Diagnostic::WsmanInvalidResponse => fl!("ui-winrm-diagnostic-wsman-invalid"),
        Diagnostic::LogonFailed => fl!("ui-winrm-diagnostic-logon-failed"),
        Diagnostic::AccessDenied => fl!("ui-winrm-diagnostic-access-denied"),
        Diagnostic::TrustedHosts => fl!("ui-winrm-diagnostic-trusted-hosts"),
        Diagnostic::KerberosPrincipal => fl!("ui-winrm-diagnostic-kerberos-principal"),
        Diagnostic::SessionNotEntered => fl!("ui-winrm-diagnostic-session-not-entered"),
    }
}

/// What the SSH agents offered a gateway that refused, as the C# says it after the refusal.
fn agent_context(count: usize) -> String {
    match count {
        0 => fl!("ui-error-auth-agent-none"),
        1 => fl!("ui-error-auth-agent-one"),
        _ => fl!("ui-error-auth-agent-many", count = count),
    }
}

/// The sentence explaining `error`.
#[must_use]
#[expect(clippy::too_many_lines, reason = "one arm per error the user can meet")]
pub fn error(error: &UiError) -> String {
    match error {
        UiError::InvalidHost => fl!("ui-error-invalid-host"),
        UiError::InvalidUsername => fl!("ui-error-invalid-username"),
        UiError::WinRmHttpsThroughGateway => fl!("ui-error-winrm-https-gateway"),
        UiError::RdpServerNotAuthenticated => fl!("ui-error-rdp-server-not-authenticated"),
        UiError::NeedsRdGateway(gateway) => {
            fl!("ui-error-rd-gateway", gateway = server_text(gateway))
        }
        UiError::WinRmHostUnresolved { host } => {
            fl!("ui-error-winrm-unresolved", host = host.as_str())
        }
        UiError::WinRmUnreachable { host, port } => {
            fl!(
                "ui-error-winrm-unreachable",
                host = host.as_str(),
                port = (*port)
            )
        }
        UiError::WinRmTlsFailed {
            host,
            port,
            check_skipped: false,
        } => fl!(
            "ui-error-winrm-tls-failed",
            host = host.as_str(),
            port = (*port)
        ),
        // Nothing was checked: the port most likely speaks no TLS, as the C# says it.
        UiError::WinRmTlsFailed {
            host,
            port,
            check_skipped: true,
        } => fl!(
            "ui-error-winrm-tls-no-verify",
            host = host.as_str(),
            port = (*port)
        ),
        UiError::Network { failure, detail } => match failure {
            NetworkFailure::Refused => fl!("ui-error-network-refused"),
            NetworkFailure::Reset => fl!("ui-error-network-reset"),
            NetworkFailure::TimedOut => fl!("ui-error-network-timed-out"),
            NetworkFailure::Unreachable => fl!("ui-error-network-unreachable"),
            NetworkFailure::Other => fl!("ui-error-network", detail = server_text(detail)),
        },
        UiError::Timeout => fl!("ui-error-timeout"),
        UiError::LocalPortUnavailable { port } => {
            fl!("ui-error-local-port-unavailable", port = (*port))
        }
        UiError::RdpRefused { refusal } => rdp_refusal(*refusal),
        UiError::RdpEnded { ending } => {
            rdp_ending(ending).unwrap_or_else(|| fl!("ui-rdp-session-closed"))
        }
        UiError::RdpProtocol { detail } => {
            fl!("ui-error-rdp-protocol", detail = server_text(detail))
        }
        UiError::VncProtocol { detail } => {
            fl!("ui-error-vnc-protocol", detail = server_text(detail))
        }
        UiError::VncSecurityRefused { offered } => {
            fl!(
                "ui-error-vnc-security-refused",
                offered = server_text(offered)
            )
        }
        UiError::VncTlsRequired { offered } => {
            fl!("ui-error-vnc-tls-required", offered = server_text(offered))
        }
        UiError::VncTlsRequiredByProfile { offered } => {
            fl!(
                "ui-error-vnc-tls-required-by-profile",
                offered = server_text(offered)
            )
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
        UiError::AuthenticationFailed { tried, agent_keys } => {
            let refused = if tried.is_empty() {
                fl!("ui-error-auth-failed-none")
            } else {
                fl!(
                    "ui-error-auth-failed",
                    methods = tried
                        .iter()
                        .copied()
                        .map(auth_method)
                        .collect::<Vec<_>>()
                        .join(LIST_SEPARATOR)
                )
            };
            match agent_keys {
                None => refused,
                Some(count) => fl!(
                    "ui-error-auth-with-agent",
                    refused = refused,
                    agent = agent_context(*count)
                ),
            }
        }
        UiError::Disconnected {
            server_message: Some(message),
        } if !server_text(message).is_empty() => fl!(
            "ui-error-disconnected-message",
            message = server_text(message)
        ),
        UiError::Disconnected { .. } => fl!("ui-error-disconnected"),
        UiError::ConnectionLost => fl!("ui-error-connection-lost"),
        UiError::Cancelled => fl!("ui-error-cancelled"),
        UiError::CertificateRefused => fl!("ui-rdp-certificate-refused"),
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
        KeyProblem::NotFound => fl!("ui-error-key-not-found", path = path),
        KeyProblem::NotAbsolute => fl!("ui-error-key-not-absolute", path = path),
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
        Dropped::RdGateway => fl!("ui-import-dropped-rd-gateway"),
        Dropped::X11Forwarding => fl!("ui-import-dropped-x11"),
        Dropped::RdpPrinters => fl!("ui-import-dropped-rdp-printers"),
        Dropped::RdpComPorts => fl!("ui-import-dropped-rdp-com-ports"),
        Dropped::RdpSmartCards => fl!("ui-import-dropped-rdp-smart-cards"),
        Dropped::RdpWebcam => fl!("ui-import-dropped-rdp-webcam"),
        Dropped::RdpUsb => fl!("ui-import-dropped-rdp-usb"),
        Dropped::RdpMicrophone => fl!("ui-import-dropped-rdp-microphone"),
        Dropped::RdpMultiMonitor => fl!("ui-import-dropped-rdp-multi-monitor"),
        Dropped::CitrixCacheLaunch => fl!("ui-import-dropped-citrix-cache-launch"),
        Dropped::LocalPostConnect(count) => {
            fl!("ui-import-dropped-local-post-connect", count = count)
        }
        Dropped::CommandLibraryLinks(count) => {
            fl!("ui-import-dropped-command-library-links", count = count)
        }
    }
}

/// Why a Citrix application was not launched, as the C# error says it.
#[must_use]
pub fn citrix_refusal(refusal: &CitrixRefusal) -> String {
    match refusal {
        CitrixRefusal::InvalidStoreFront => fl!("ui-status-citrix-invalid-storefront"),
        CitrixRefusal::StoreFrontCredentials => fl!("ui-status-citrix-storefront-credentials"),
        CitrixRefusal::InvalidIcaFile => fl!("ui-status-citrix-invalid-ica-file"),
        CitrixRefusal::NotConfigured => fl!("ui-status-citrix-not-configured"),
        CitrixRefusal::WorkspaceNotFound => fl!("ui-status-citrix-workspace-not-found"),
        CitrixRefusal::Failed => fl!("ui-status-citrix-launch-failed"),
        CitrixRefusal::NotStarted(reason) => {
            fl!("ui-status-citrix-not-started", reason = server_text(reason))
        }
        CitrixRefusal::CommandRejected => fl!("ui-status-citrix-command-rejected"),
        CitrixRefusal::VaultLocked => fl!("ui-status-citrix-vault-locked"),
    }
}

/// Why the system did not vouch for an FTPS server's certificate, as the C# prompt's
/// "Validation issue".
#[must_use]
pub fn validation_issue(issue: ValidationIssue) -> String {
    match issue {
        ValidationIssue::SelfSigned => fl!("ui-certificate-issue-self-signed"),
        ValidationIssue::UnknownIssuer => fl!("ui-certificate-issue-unknown-issuer"),
        ValidationIssue::Expired => fl!("ui-certificate-issue-expired"),
        ValidationIssue::NotYetValid => fl!("ui-certificate-issue-not-yet-valid"),
        ValidationIssue::NameMismatch => fl!("ui-certificate-issue-name-mismatch"),
        ValidationIssue::Revoked => fl!("ui-certificate-issue-revoked"),
        ValidationIssue::NoSystemStore => fl!("ui-certificate-issue-no-system-store"),
        ValidationIssue::Other => fl!("ui-certificate-issue-other"),
    }
}

/// Why an imported profile was left out.
#[must_use]
pub fn skip_reason(reason: &SkipReason) -> String {
    match reason {
        SkipReason::NotSsh(kind) => fl!("ui-import-skip-not-ssh", kind = server_text(kind)),
        SkipReason::MissingHost => fl!("ui-import-skip-missing-host"),
        SkipReason::MissingId => fl!("ui-import-skip-missing-id"),
        SkipReason::InvalidPort(port) => {
            fl!("ui-import-skip-invalid-port", port = port.to_string())
        }
        SkipReason::MissingGateway => fl!("ui-import-skip-missing-gateway"),
        SkipReason::GatewayLoop => fl!("ui-import-skip-gateway-loop"),
        SkipReason::NeedsElevation => fl!("ui-import-skip-elevation"),
        SkipReason::UnsafeLocalCommand => fl!("ui-import-skip-unsafe-local"),
        SkipReason::MissingUsername => fl!("ui-import-skip-missing-username"),
        SkipReason::UnknownIdentityMode => fl!("ui-import-skip-unknown-identity"),
    }
}

/// The name of an environment, as the C# list says it; "(None)" for none.
#[must_use]
pub fn environment_name(environment: Option<heimdall_core::metadata::Environment>) -> String {
    use heimdall_core::metadata::Environment;
    match environment {
        None => fl!("ui-profile-environment-none"),
        Some(Environment::Production) => fl!("ui-profile-environment-production"),
        Some(Environment::Staging) => fl!("ui-profile-environment-staging"),
        Some(Environment::Lab) => fl!("ui-profile-environment-lab"),
        Some(Environment::Personal) => fl!("ui-profile-environment-personal"),
    }
}

/// Where an imported profile came from, as the C# `ProfileOriginDisplay.GetDisplayName`.
#[must_use]
pub fn origin_name(origin: heimdall_core::metadata::ProfileOrigin) -> String {
    use heimdall_core::metadata::ProfileOrigin;
    match origin {
        ProfileOrigin::RdpFile => fl!("ui-origin-rdp-file"),
        ProfileOrigin::OpenSsh => fl!("ui-origin-openssh"),
        ProfileOrigin::Putty => fl!("ui-origin-putty"),
        ProfileOrigin::MRemoteNg => fl!("ui-origin-mremoteng"),
        ProfileOrigin::MobaXterm => fl!("ui-origin-mobaxterm"),
        ProfileOrigin::RdcMan => fl!("ui-origin-rdcman"),
    }
}

/// The sentence explaining why the profile form does not save.
#[must_use]
pub fn draft_error(error: DraftError) -> String {
    match error {
        DraftError::MacAddressInvalid => fl!("ui-profile-error-mac-address"),
        DraftError::RdGatewayInvalid => fl!("ui-profile-error-rd-gateway"),
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
        FilesError::Interrupted => fl!("ui-files-error-interrupted"),
        FilesError::Local { detail } => fl!("ui-files-error-local", detail = detail.as_str()),
        FilesError::UnsafeName { name, reason } => fl!(
            "ui-files-error-unsafe-name",
            name = name.as_str(),
            reason = name_reason(reason)
        ),
        FilesError::NotAFile => fl!("ui-files-error-not-a-file"),
        FilesError::IsLink => fl!("ui-files-error-is-link"),
        FilesError::DestinationNotAFile => fl!("ui-files-error-destination-not-a-file"),
        FilesError::ReplaceNotSafe => fl!("ui-files-error-replace-not-safe"),
        FilesError::TooLarge => fl!("ui-files-error-too-large"),
        FilesError::InvalidName => fl!("ui-files-error-invalid-name"),
        FilesError::Exists => fl!("ui-files-error-exists"),
        FilesError::InvalidPermissions => fl!("ui-files-error-invalid-permissions"),
        FilesError::CopyRefused => fl!("ui-files-error-copy-refused"),
        FilesError::ChangedOnServer => fl!("ui-files-error-changed-on-server"),
        FilesError::LooksBinary => fl!("ui-files-error-looks-binary"),
        FilesError::TooLargeForEditor => fl!(
            "ui-files-error-too-large-for-editor",
            size = size(heimdall_app::integrated_edit::INTEGRATED_EDIT_LIMIT)
        ),
        FilesError::NotText => fl!("ui-files-error-not-text"),
        FilesError::FileTooLarge => fl!("ui-files-error-file-too-large"),
        FilesError::WorkingFolderUnprotected => fl!("ui-files-error-working-folder-unprotected"),
        FilesError::EditorFailed { detail } => {
            fl!("ui-files-error-editor-failed", detail = detail.as_str())
        }
        FilesError::EditorRunsFiles => fl!("ui-files-error-editor-runs-files"),
        FilesError::OpenFailed { detail } => {
            fl!("ui-files-error-open-failed", detail = detail.as_str())
        }
        FilesError::ScriptPathCharacter { character } => fl!(
            "ui-files-error-script-character",
            character = character.as_str()
        ),
        FilesError::ScriptPathNotText => fl!("ui-files-error-script-not-text"),
        FilesError::SudoPasswordNeeded => fl!("ui-files-error-sudo-password-needed"),
        FilesError::SudoPasswordRejected => fl!("ui-files-error-sudo-password-rejected"),
        FilesError::SudoNeedsTerminal => fl!("ui-files-error-sudo-needs-terminal"),
        FilesError::SudoUntrusted => fl!("ui-files-error-sudo-untrusted"),
        FilesError::SudoToolingMissing => fl!("ui-files-error-sudo-tooling"),
        FilesError::SudoFailed => fl!("ui-files-error-sudo-failed"),
        FilesError::SudoProtected => fl!("ui-files-error-sudo-protected"),
        FilesError::ChangedSinceConfirmed => fl!("ui-files-error-changed-since-confirmed"),
        FilesError::PasteIntoItself { name } => {
            fl!("ui-files-error-paste-into-itself", name = name.as_str())
        }
        FilesError::PasteLink { name } => {
            fl!("ui-files-error-paste-link", name = name.as_str())
        }
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
    fn a_gateway_s_refusal_says_what_the_agents_offered_as_the_csharp() {
        let refused = |agent_keys| {
            error(&UiError::AuthenticationFailed {
                tried: vec![AuthMethod::Agent],
                agent_keys,
            })
        };
        let plain = refused(None);
        assert!(!plain.contains("loaded in an SSH agent"), "{plain}");
        let none = refused(Some(0));
        assert!(none.starts_with(&plain), "the refusal first: {none}");
        assert!(none.contains("No key was loaded in an SSH agent"), "{none}");
        assert!(refused(Some(1)).contains("One key was loaded in an SSH agent"));
        assert!(refused(Some(3)).contains("3 keys were loaded in an SSH agent"));
    }

    #[test]
    fn an_rdp_refusal_warns_of_the_account_and_errs_on_the_rest_as_the_csharp() {
        use heimdall_rdp::Refusal;

        assert_eq!(
            error(&UiError::RdpRefused {
                refusal: Refusal::PasswordExpired
            }),
            "Warning: The password has expired and must be changed before connecting."
        );
        assert_eq!(
            error(&UiError::RdpRefused {
                refusal: Refusal::AccountDisabled
            }),
            "Error: The account is disabled on the remote computer. Ask your administrator to \
             enable it, then try connecting again."
        );
        assert!(
            error(&UiError::RdpRefused {
                refusal: Refusal::BadCredentials
            })
            .starts_with("Warning: The credentials were not accepted.")
        );
    }

    #[test]
    fn an_rdp_connection_ended_by_a_logoff_says_only_that_it_ended() {
        use heimdall_rdp::Ending;

        assert_eq!(
            error(&UiError::RdpEnded {
                ending: Ending::Logoff
            }),
            "The Remote Desktop session has ended."
        );
        assert!(
            error(&UiError::RdpEnded {
                ending: Ending::License
            })
            .starts_with("Error: A Remote Desktop licensing error blocked the session.")
        );
    }

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
            agent_keys: None,
        });
        assert!(text.contains("SSH agent, password"), "{text}");
        let none = error(&UiError::AuthenticationFailed {
            tried: Vec::new(),
            agent_keys: None,
        });
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
