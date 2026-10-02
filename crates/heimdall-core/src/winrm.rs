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

//! The `PowerShell` command that opens a `WinRM` profile: `Enter-PSSession`, run by a local
//! `PowerShell` in a terminal tab.
//!
//! The tab is a remote session, so the local `PowerShell` never hands the user a prompt of its
//! own: it ends the first time it would show one. See [`session_command`].
//!
//! The command is text `PowerShell` parses, so nothing from the profile reaches it unchecked:
//! the host is a DNS name or an IP address and nothing else, and the account name is a
//! single-quoted literal, every quote `PowerShell` would end it on doubled. Heimdall never
//! holds the password: `-Credential` with a name makes `PowerShell` ask for it.

use std::net::IpAddr;

use thiserror::Error;

use crate::profile::WinRmProfile;

/// Arguments that run [`session_command`] in a `PowerShell` left open once it is done, without
/// the user's profile script: what the session does is what the command says.
pub const POWERSHELL_ARGUMENTS: [&str; 4] = ["-NoLogo", "-NoExit", "-NoProfile", "-Command"];

/// Exit code of the local `PowerShell` when the remote session was entered and has ended.
pub const REMOTE_SESSION_ENDED_EXIT_CODE: i32 = 0;

/// Exit code of the local `PowerShell` when the remote session was never entered.
pub const REMOTE_SESSION_NOT_ENTERED_EXIT_CODE: i32 = 1;

/// Global variable set once `Enter-PSSession` has returned without an error.
const ENTERED_VARIABLE: &str = "$global:HeimdallWinRmEntered";

/// Longest DNS name, in characters.
const MAX_HOST_NAME_LENGTH: usize = 253;

/// Characters `PowerShell` ends a single-quoted string on: the ASCII quote and the four
/// typographic single quotes, as its own `EscapeSingleQuotedStringContent` lists them.
const SINGLE_QUOTES: [char; 5] = ['\'', '\u{2018}', '\u{2019}', '\u{201A}', '\u{201B}'];

/// Why a profile cannot be turned into a command.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum CommandError {
    /// The host is neither a DNS name nor an IP address.
    #[error("the host is not a DNS name or an IP address")]
    InvalidHost,
    /// The account name is empty, or holds a control character or a double quote, which no
    /// Windows account name holds.
    #[error("the account name cannot be used")]
    InvalidUsername,
}

/// The command a `WinRM` tab runs for `profile`: the local prompt guard, then
/// `Enter-PSSession`, then the mark that the session was entered.
///
/// `Enter-PSSession` pushes the remote runspace and returns; the rest of the command still runs
/// locally, and once it is done `PowerShell` evaluates the prompt in the pushed runspace, the
/// remote one. The guard is the LOCAL `prompt` function, so it runs only when no remote
/// runspace is pushed: `Enter-PSSession` failed or was cancelled
/// ([`REMOTE_SESSION_NOT_ENTERED_EXIT_CODE`]), or the remote session ended through a remote
/// `exit` or a dropped connection ([`REMOTE_SESSION_ENDED_EXIT_CODE`]). Either way the tab ends
/// instead of leaving, under the remote host's name, a prompt that runs on this machine.
///
/// A plain `; exit` after `Enter-PSSession` would end `PowerShell` before the user ever reached
/// the remote prompt, and `exit` inside the prompt function is not honoured (`PSReadLine`
/// reports an `ExitException` and the local prompt stays), hence `[Environment]::Exit`.
/// `-ErrorAction Stop` makes a refused connection, a non-terminating error of the cmdlet, end
/// the command before the mark.
///
/// # Errors
///
/// The host or the account name is refused; see [`CommandError`].
pub fn session_command(profile: &WinRmProfile) -> Result<String, CommandError> {
    let enter = enter_session(profile)?;
    Ok(format!(
        "{}; {enter} -ErrorAction Stop; {ENTERED_VARIABLE} = $true",
        local_prompt_guard()
    ))
}

/// Ends the local `PowerShell` the first time it would show a prompt of its own: with
/// [`REMOTE_SESSION_ENDED_EXIT_CODE`] once the session was entered, else with
/// [`REMOTE_SESSION_NOT_ENTERED_EXIT_CODE`].
fn local_prompt_guard() -> String {
    format!(
        "{ENTERED_VARIABLE} = $false; function global:prompt {{ if ({ENTERED_VARIABLE}) \
         {{ [Environment]::Exit({REMOTE_SESSION_ENDED_EXIT_CODE}) }} \
         [Environment]::Exit({REMOTE_SESSION_NOT_ENTERED_EXIT_CODE}) }}"
    )
}

/// The `Enter-PSSession` command for `profile`.
fn enter_session(profile: &WinRmProfile) -> Result<String, CommandError> {
    let host = checked_host(&profile.host)?;
    let mut command = format!(
        "Enter-PSSession -ComputerName '{host}' -Port {} -Authentication Negotiate",
        profile.port
    );
    if profile.use_ssl {
        command.push_str(" -UseSSL");
        if profile.skip_certificate_check {
            command.push_str(
                " -SessionOption (New-PSSessionOption -SkipCACheck -SkipCNCheck \
                 -SkipRevocationCheck)",
            );
        }
    }
    if let Some(username) = &profile.username {
        command.push_str(" -Credential '");
        command.push_str(&quoted_content(checked_username(username)?));
        command.push('\'');
    }
    Ok(command)
}

/// `host` when it is a DNS name, letters, digits, dots and hyphens not starting with a hyphen,
/// or an IP address.
fn checked_host(host: &str) -> Result<&str, CommandError> {
    let is_name = !host.is_empty()
        && host.len() <= MAX_HOST_NAME_LENGTH
        && !host.starts_with('-')
        && host
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '-');
    if is_name || host.parse::<IpAddr>().is_ok() {
        Ok(host)
    } else {
        Err(CommandError::InvalidHost)
    }
}

fn checked_username(username: &str) -> Result<&str, CommandError> {
    let username = username.trim();
    if username.is_empty() || username.chars().any(|c| c.is_control() || c == '"') {
        Err(CommandError::InvalidUsername)
    } else {
        Ok(username)
    }
}

/// `text` as the inside of a single-quoted `PowerShell` string: each quote doubled.
fn quoted_content(text: &str) -> String {
    let mut quoted = String::with_capacity(text.len());
    for c in text.chars() {
        if SINGLE_QUOTES.contains(&c) {
            quoted.push(c);
        }
        quoted.push(c);
    }
    quoted
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::profile::{DEFAULT_WINRM_HTTP_PORT, DEFAULT_WINRM_HTTPS_PORT, ProfileId};

    fn profile(host: &str) -> WinRmProfile {
        WinRmProfile {
            id: ProfileId::new("w"),
            name: "W".to_owned(),
            group: None,
            host: host.to_owned(),
            port: DEFAULT_WINRM_HTTP_PORT,
            use_ssl: false,
            skip_certificate_check: false,
            username: None,
        }
    }

    #[test]
    fn the_guard_ends_powershell_at_its_first_local_prompt() {
        assert_eq!(
            local_prompt_guard(),
            "$global:HeimdallWinRmEntered = $false; function global:prompt { if \
             ($global:HeimdallWinRmEntered) { [Environment]::Exit(0) } [Environment]::Exit(1) }"
        );
    }

    #[test]
    fn the_session_is_entered_between_the_guard_and_the_mark() {
        let profile = WinRmProfile {
            username: Some("LAB\\admin".to_owned()),
            ..profile("dc01.lab.local")
        };
        assert_eq!(
            session_command(&profile),
            Ok(format!(
                "{}; {} -ErrorAction Stop; $global:HeimdallWinRmEntered = $true",
                local_prompt_guard(),
                enter_session(&profile).expect("valid")
            ))
        );
    }

    #[test]
    fn a_refused_profile_gives_no_command_at_all() {
        assert_eq!(
            session_command(&profile("h$(calc)")),
            Err(CommandError::InvalidHost)
        );
        let profile = WinRmProfile {
            username: Some("a\"b".to_owned()),
            ..profile("h")
        };
        assert_eq!(
            session_command(&profile),
            Err(CommandError::InvalidUsername)
        );
    }

    #[test]
    fn the_current_user_connects_over_http_with_no_credential() {
        assert_eq!(
            enter_session(&profile("dc01.lab.local")).as_deref(),
            Ok(
                "Enter-PSSession -ComputerName 'dc01.lab.local' -Port 5985 -Authentication Negotiate"
            )
        );
    }

    #[test]
    fn https_skipping_checks_and_an_account_are_all_said() {
        let profile = WinRmProfile {
            port: DEFAULT_WINRM_HTTPS_PORT,
            use_ssl: true,
            skip_certificate_check: true,
            username: Some(" LAB\\admin ".to_owned()),
            ..profile("192.168.31.136")
        };
        assert_eq!(
            enter_session(&profile).as_deref(),
            Ok("Enter-PSSession -ComputerName '192.168.31.136' -Port 5986 \
                -Authentication Negotiate -UseSSL -SessionOption (New-PSSessionOption \
                -SkipCACheck -SkipCNCheck -SkipRevocationCheck) -Credential 'LAB\\admin'")
        );
    }

    #[test]
    fn skipping_certificate_checks_is_never_said_over_http() {
        let profile = WinRmProfile {
            skip_certificate_check: true,
            ..profile("h")
        };
        let command = enter_session(&profile).expect("valid");
        assert!(!command.contains("-SessionOption"), "{command}");
        assert!(!command.contains("-UseSSL"), "{command}");
    }

    #[test]
    fn a_host_that_is_not_a_name_or_an_address_is_refused() {
        for host in [
            "",
            "h'; Remove-Item x; '",
            "h$(calc)",
            "h name",
            "-h",
            "h\u{2019}",
            "h\n",
            &"a".repeat(MAX_HOST_NAME_LENGTH + 1),
        ] {
            assert_eq!(
                enter_session(&profile(host)),
                Err(CommandError::InvalidHost),
                "{host:?}"
            );
        }
    }

    #[test]
    fn names_and_addresses_of_both_families_are_accepted() {
        for host in ["h", "web-01.lab", "10.0.0.1", "fe80::1", "2001:db8::5"] {
            let command = enter_session(&profile(host)).expect("valid");
            assert!(
                command.contains(&format!("-ComputerName '{host}' ")),
                "{command}"
            );
        }
    }

    #[test]
    fn every_quote_powershell_ends_a_literal_on_is_doubled() {
        let profile = WinRmProfile {
            username: Some("o'b\u{2018}c\u{2019}d\u{201A}e\u{201B}f".to_owned()),
            ..profile("h")
        };
        let command = enter_session(&profile).expect("valid");
        assert!(
            command.ends_with(
                " -Credential 'o''b\u{2018}\u{2018}c\u{2019}\u{2019}d\u{201A}\u{201A}e\u{201B}\u{201B}f'"
            ),
            "{command}"
        );
    }

    #[test]
    fn an_account_name_no_windows_account_has_is_refused() {
        for username in ["", "  ", "a\"b", "a\nb", "a\u{0}b"] {
            let profile = WinRmProfile {
                username: Some(username.to_owned()),
                ..profile("h")
            };
            assert_eq!(
                enter_session(&profile),
                Err(CommandError::InvalidUsername),
                "{username:?}"
            );
        }
    }
}
