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

use heimdall_app::{KeyProblem, UiError, server_text};
use heimdall_core::import::csharp::SkipReason;
use heimdall_ssh::AuthMethod;

use crate::i18n::fl;

/// Separator between the items of an inline list.
const LIST_SEPARATOR: &str = ", ";

/// The sentence explaining `error`.
#[must_use]
pub fn error(error: &UiError) -> String {
    match error {
        UiError::InvalidHost => fl!("ui-error-invalid-host"),
        UiError::Network { detail } => fl!("ui-error-network", detail = detail.as_str()),
        UiError::Timeout => fl!("ui-error-timeout"),
        UiError::HostKeyChanged { recorded, offered } => fl!(
            "ui-error-hostkey-changed",
            recorded = recorded.as_str(),
            offered = offered.as_str()
        ),
        UiError::HostKeyAlgorithmMismatch { recorded } => fl!(
            "ui-error-hostkey-algorithm",
            recorded = recorded.join(LIST_SEPARATOR)
        ),
        UiError::HostCertificateRefused => fl!("ui-error-host-certificate"),
        UiError::KnownHosts { detail } => fl!("ui-error-known-hosts", detail = detail.as_str()),
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
        UiError::Cancelled => fl!("ui-error-cancelled"),
        UiError::PromptTimedOut => fl!("ui-error-prompt-timeout"),
        UiError::PtyRefused => fl!("ui-error-pty-refused"),
        UiError::ShellRefused => fl!("ui-error-shell-refused"),
        UiError::Protocol { detail } => fl!("ui-error-protocol", detail = detail.as_str()),
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

/// Why an imported profile was left out.
#[must_use]
pub fn skip_reason(reason: &SkipReason) -> String {
    match reason {
        SkipReason::NotSsh(kind) => fl!("ui-import-skip-not-ssh", kind = server_text(kind)),
        SkipReason::NeedsJumpHost => fl!("ui-import-skip-jump-host"),
        SkipReason::MissingHost => fl!("ui-import-skip-missing-host"),
        SkipReason::MissingId => fl!("ui-import-skip-missing-id"),
        SkipReason::InvalidPort(port) => {
            fl!("ui-import-skip-invalid-port", port = port.to_string())
        }
    }
}

#[cfg(test)]
mod tests {
    use heimdall_app::{KeyProblem, UiError};
    use heimdall_core::import::csharp::SkipReason;
    use heimdall_ssh::AuthMethod;

    use super::{error, skip_reason};

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
}
