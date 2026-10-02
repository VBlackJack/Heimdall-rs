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

//! Why a server refused a logon or ended a session, as the C# Heimdall tells them apart.
//!
//! The C# reads both from the codes the Windows client reports. Here they come from the
//! server itself: the status of a refused `CredSSP` logon (MS-CSSP `errorCode`, an NTSTATUS),
//! and the code of a Set Error Info PDU (MS-RDPBCGR 2.2.5.1.1) ending a session.

use ironrdp::connector::sspi;
use ironrdp::pdu::rdp::server_error_info::{ErrorInfo, ServerSetErrorInfoPdu};

/// NTSTATUS of an expired password.
const STATUS_PASSWORD_EXPIRED: u32 = 0xC000_0071;
/// NTSTATUS of a password to change at next logon.
const STATUS_PASSWORD_MUST_CHANGE: u32 = 0xC000_0224;
/// NTSTATUS of a locked-out account.
const STATUS_ACCOUNT_LOCKED_OUT: u32 = 0xC000_0234;
/// NTSTATUS of a disabled account.
const STATUS_ACCOUNT_DISABLED: u32 = 0xC000_0072;
/// NTSTATUS of an expired account.
const STATUS_ACCOUNT_EXPIRED: u32 = 0xC000_0193;
/// NTSTATUS of a logon outside the account's hours.
const STATUS_INVALID_LOGON_HOURS: u32 = 0xC000_006F;
/// NTSTATUS of no domain controller reachable.
const STATUS_NO_LOGON_SERVERS: u32 = 0xC000_005E;
/// NTSTATUS of a domain controller's logon service not started.
const STATUS_NETLOGON_NOT_STARTED: u32 = 0xC000_0192;
/// NTSTATUS of clocks too far apart.
const STATUS_TIME_DIFFERENCE_AT_DC: u32 = 0xC000_0133;

/// Why the server refused the logon, as the C# words it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Refusal {
    /// The account name, password or domain, or the account's right to log on remotely.
    BadCredentials,
    /// The password expired, or must be changed first.
    PasswordExpired,
    /// The account is locked out.
    AccountLockedOut,
    /// The account is disabled.
    AccountDisabled,
    /// The account expired.
    AccountExpired,
    /// The account may not log on at this time of day.
    TimeOfDayRestriction,
    /// No domain controller could check the account.
    NoAuthenticationAuthority,
    /// The two clocks are too far apart.
    ClockSkew,
    /// The security exchange failed for a reason that says nothing of the account.
    SecurityError,
}

impl Refusal {
    /// The refusal `error` stands for: the server's status when it gave one, else what this
    /// side's security package found.
    pub(crate) fn of(error: &sspi::Error) -> Self {
        match error.nstatus {
            Some(status) => Self::of_status(status.0),
            None => match error.error_type {
                sspi::ErrorKind::LogonDenied
                | sspi::ErrorKind::UnknownCredentials
                | sspi::ErrorKind::NoCredentials
                | sspi::ErrorKind::KdcInvalidRequest => Self::BadCredentials,
                sspi::ErrorKind::NoAuthenticatingAuthority => Self::NoAuthenticationAuthority,
                sspi::ErrorKind::TimeSkew => Self::ClockSkew,
                _ => Self::SecurityError,
            },
        }
    }

    /// The refusal of NTSTATUS `status`, which the server sent for a logon it refused: a
    /// status not named here, a wrong name or password among them (`0xC000006D`,
    /// `0xC000006A`, `0xC0000064`), or an account without the remote logon right
    /// (`0xC000006E`, `0xC000015B`), is a refusal of the account.
    fn of_status(status: u32) -> Self {
        match status {
            STATUS_PASSWORD_EXPIRED | STATUS_PASSWORD_MUST_CHANGE => Self::PasswordExpired,
            STATUS_ACCOUNT_LOCKED_OUT => Self::AccountLockedOut,
            STATUS_ACCOUNT_DISABLED => Self::AccountDisabled,
            STATUS_ACCOUNT_EXPIRED => Self::AccountExpired,
            STATUS_INVALID_LOGON_HOURS => Self::TimeOfDayRestriction,
            STATUS_NO_LOGON_SERVERS | STATUS_NETLOGON_NOT_STARTED => {
                Self::NoAuthenticationAuthority
            }
            STATUS_TIME_DIFFERENCE_AT_DC => Self::ClockSkew,
            _ => Self::BadCredentials,
        }
    }

    /// Whether the user can act on it from this side: the account or the password. The
    /// C# warns of these and shows the rest as errors.
    #[must_use]
    pub fn is_account_issue(self) -> bool {
        matches!(
            self,
            Self::BadCredentials
                | Self::PasswordExpired
                | Self::AccountLockedOut
                | Self::AccountExpired
        )
    }
}

/// Why the server ended a session, as the C# words it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Ending {
    /// The user logged off, or disconnected from inside the session: nothing to explain.
    Logoff,
    /// The remote computer ended it: an administrator, the idle or session time limit,
    /// another connection to the same session.
    AdminDisconnect,
    /// The server refused the account the session after its logon.
    BadCredentials,
    /// A Remote Desktop licensing error.
    License,
    /// Another reason, in `IronRDP`'s words.
    Other(String),
}

impl Ending {
    /// Whether the user can act on it from this side, as [`Refusal::is_account_issue`].
    #[must_use]
    pub fn is_account_issue(&self) -> bool {
        matches!(self, Self::BadCredentials)
    }
}

/// The Set Error Info codes worded, each with its ending.
const WORDED: [(u32, Ending); 21] = [
    // Logged off from inside, by the user or an administrative tool the user ran.
    (0x0B, Ending::Logoff),
    (0x0C, Ending::Logoff),
    // Ended by an administrator, the idle limit, the active-session limit (named
    // LOGON_TIMEOUT, which it is not), or another connection taking the session.
    (0x01, Ending::AdminDisconnect),
    (0x02, Ending::AdminDisconnect),
    (0x03, Ending::AdminDisconnect),
    (0x04, Ending::AdminDisconnect),
    (0x05, Ending::AdminDisconnect),
    // Denied, short of privileges, or fresh credentials required.
    (0x07, Ending::BadCredentials),
    (0x09, Ending::BadCredentials),
    (0x0A, Ending::BadCredentials),
    // The licensing block.
    (0x100, Ending::License),
    (0x101, Ending::License),
    (0x102, Ending::License),
    (0x103, Ending::License),
    (0x104, Ending::License),
    (0x105, Ending::License),
    (0x106, Ending::License),
    (0x107, Ending::License),
    (0x108, Ending::License),
    (0x109, Ending::License),
    (0x10A, Ending::License),
];

/// What `IronRDP` prefixes to a Set Error Info it fails a connection step with.
const ERROR_INFO_MARK: &str = "server returned error info: ";

/// The description `IronRDP` gives Set Error Info code `code`, when it knows the code.
fn description(code: u32) -> Option<String> {
    ironrdp_core::decode::<ServerSetErrorInfoPdu>(&code.to_le_bytes())
        .ok()
        .map(|ServerSetErrorInfoPdu(info): ServerSetErrorInfoPdu| ErrorInfo::description(info))
}

/// The ending a Set Error Info `text` stands for: `IronRDP` keeps only its description,
/// so it is matched against the description of each code worded.
pub(crate) fn ending(text: &str) -> Ending {
    WORDED
        .iter()
        .find(|(code, _)| description(*code).is_some_and(|known| text.contains(&known)))
        .map_or_else(
            || Ending::Other(text.to_owned()),
            |(_, ending)| ending.clone(),
        )
}

/// The ending a failed connection step stands for, when its `text` is a Set Error Info.
pub(crate) fn ending_in_failure(text: &str) -> Option<Ending> {
    let start = text.find(ERROR_INFO_MARK)? + ERROR_INFO_MARK.len();
    Some(ending(&text[start..]))
}

#[cfg(test)]
mod tests {
    use super::*;

    const STATUS_LOGON_FAILURE: u32 = 0xC000_006D;
    const STATUS_WRONG_PASSWORD: u32 = 0xC000_006A;
    const STATUS_NO_SUCH_USER: u32 = 0xC000_0064;
    const STATUS_ACCOUNT_RESTRICTION: u32 = 0xC000_006E;
    const STATUS_LOGON_TYPE_NOT_GRANTED: u32 = 0xC000_015B;

    #[test]
    fn a_status_the_server_refused_with_is_worded_as_the_csharp_codes() {
        for (status, refusal) in [
            (STATUS_LOGON_FAILURE, Refusal::BadCredentials),
            (STATUS_WRONG_PASSWORD, Refusal::BadCredentials),
            (STATUS_NO_SUCH_USER, Refusal::BadCredentials),
            (STATUS_ACCOUNT_RESTRICTION, Refusal::BadCredentials),
            (STATUS_LOGON_TYPE_NOT_GRANTED, Refusal::BadCredentials),
            (STATUS_PASSWORD_EXPIRED, Refusal::PasswordExpired),
            (STATUS_PASSWORD_MUST_CHANGE, Refusal::PasswordExpired),
            (STATUS_ACCOUNT_LOCKED_OUT, Refusal::AccountLockedOut),
            (STATUS_ACCOUNT_DISABLED, Refusal::AccountDisabled),
            (STATUS_ACCOUNT_EXPIRED, Refusal::AccountExpired),
            (STATUS_INVALID_LOGON_HOURS, Refusal::TimeOfDayRestriction),
            (STATUS_NO_LOGON_SERVERS, Refusal::NoAuthenticationAuthority),
            (
                STATUS_NETLOGON_NOT_STARTED,
                Refusal::NoAuthenticationAuthority,
            ),
            (STATUS_TIME_DIFFERENCE_AT_DC, Refusal::ClockSkew),
            (0xC000_0001, Refusal::BadCredentials),
        ] {
            let error = sspi::Error::new_with_nstatus(
                sspi::ErrorKind::InvalidToken,
                "refused",
                sspi::credssp::NStatusCode(status),
            );
            assert_eq!(Refusal::of(&error), refusal, "{status:#x}");
        }
    }

    #[test]
    fn without_a_status_only_an_account_failure_is_called_one() {
        for (kind, refusal) in [
            (sspi::ErrorKind::LogonDenied, Refusal::BadCredentials),
            (sspi::ErrorKind::UnknownCredentials, Refusal::BadCredentials),
            (sspi::ErrorKind::KdcInvalidRequest, Refusal::BadCredentials),
            (
                sspi::ErrorKind::NoAuthenticatingAuthority,
                Refusal::NoAuthenticationAuthority,
            ),
            (sspi::ErrorKind::TimeSkew, Refusal::ClockSkew),
            (sspi::ErrorKind::MessageAltered, Refusal::SecurityError),
            (sspi::ErrorKind::InternalError, Refusal::SecurityError),
        ] {
            let error = sspi::Error::new(kind, "failed");
            assert_eq!(Refusal::of(&error), refusal, "{kind:?}");
        }
    }

    #[test]
    fn the_csharp_warns_of_an_account_and_errs_on_the_rest() {
        assert!(Refusal::PasswordExpired.is_account_issue());
        assert!(Refusal::AccountLockedOut.is_account_issue());
        assert!(!Refusal::AccountDisabled.is_account_issue());
        assert!(!Refusal::ClockSkew.is_account_issue());
        assert!(Ending::BadCredentials.is_account_issue());
        assert!(!Ending::License.is_account_issue());
    }

    #[test]
    fn each_code_worded_is_found_by_its_own_description_only() {
        for (code, worded) in &WORDED {
            let Some(text) = description(*code) else {
                continue;
            };
            assert_eq!(&ending(&text), worded, "{code:#x}: {text}");
            for (other, _) in &WORDED {
                if other != code {
                    let found = description(*other).is_some_and(|known| text.contains(&known));
                    assert!(!found, "{code:#x} also reads as {other:#x}");
                }
            }
        }
    }

    #[test]
    fn a_logoff_an_admin_disconnect_and_a_licence_error_are_known_to_ironrdp() {
        // The common cases must not fall through to "Other" by a missing code.
        for code in [
            0x0B, 0x0C, 0x01, 0x02, 0x03, 0x04, 0x05, 0x07, 0x09, 0x0A, 0x100,
        ] {
            assert!(description(code).is_some(), "{code:#x}");
        }
    }

    #[test]
    fn an_unknown_ending_keeps_its_words_and_a_failed_step_is_read_after_its_mark() {
        assert_eq!(
            ending("something else"),
            Ending::Other("something else".to_owned())
        );
        let logoff = description(0x0C).expect("known");
        assert_eq!(
            ending_in_failure(&format!(
                "[ServerSetErrorInfo] reason: {ERROR_INFO_MARK}{logoff}"
            )),
            Some(Ending::Logoff)
        );
        assert_eq!(ending_in_failure("unexpected control action"), None);
    }
}
