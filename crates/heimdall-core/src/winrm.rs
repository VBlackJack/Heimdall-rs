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
//! single-quoted literal, every quote `PowerShell` would end it on doubled.
//!
//! The password, as the C# `WinRmCredentialBootstrap` hands it over: a stored one goes to the
//! local `PowerShell` in an environment variable only, [`PASSWORD_VARIABLE`], never on its
//! command line and never on disk. The command reads it into a `SecureString`, removes the
//! variable from the process, then builds the `PSCredential` `Enter-PSSession` is given. With
//! no stored password, or one the server refused, `-Credential` with the account's name makes
//! `PowerShell` ask for it.

use std::net::{IpAddr, SocketAddr};

use thiserror::Error;

use crate::credentials::{CredentialProtocol, Endpoint};
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

/// Environment variable of the local `PowerShell` carrying a stored password, removed by the
/// command once read.
pub const PASSWORD_VARIABLE: &str = "HEIMDALL_WINRM_PASSWORD";

/// Name of the variable holding the stored password as a `SecureString`, until the session is
/// entered.
const SECRET_NAME: &str = "HeimdallWinRmSecret";

/// Longest DNS name, in characters.
const MAX_HOST_NAME_LENGTH: usize = 253;

/// Characters `PowerShell` ends a single-quoted string on: the ASCII quote and the four
/// typographic single quotes, as its own `EscapeSingleQuotedStringContent` lists them.
const SINGLE_QUOTES: [char; 5] = ['\'', '\u{2018}', '\u{2019}', '\u{201A}', '\u{201B}'];

/// Where `Enter-PSSession` takes the account's password from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PasswordSource {
    /// `PowerShell` asks for it in the terminal.
    Prompt,
    /// The stored password, in [`PASSWORD_VARIABLE`]. A profile naming no account has none.
    Environment,
}

/// The server and account a stored password of `profile` is for: its own host and port,
/// never a gateway's forward, and its account. `None` for the current Windows identity,
/// which has no password to store.
#[must_use]
pub fn password_endpoint(profile: &WinRmProfile) -> Option<Endpoint> {
    Some(Endpoint {
        protocol: CredentialProtocol::WinRm,
        host: profile.host.clone(),
        port: profile.port,
        username: Some(profile.username.clone()?),
    })
}

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
    /// HTTPS through an SSH gateway, which the C# Heimdall refuses: the certificate would be
    /// checked against the forward's address, not the server's name.
    #[error("WinRM over an SSH gateway does not support HTTPS")]
    HttpsThroughGateway,
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
/// With [`PasswordSource::Environment`] and an account, the stored password is read from
/// [`PASSWORD_VARIABLE`] and the variable removed before `Enter-PSSession` runs, whatever it
/// does; the `SecureString` is dropped once the session is entered. One try: refused, the
/// session is not entered and the tab ends with [`REMOTE_SESSION_NOT_ENTERED_EXIT_CODE`].
///
/// # Errors
///
/// The host or the account name is refused; see [`CommandError`].
pub fn session_command(
    profile: &WinRmProfile,
    password: PasswordSource,
) -> Result<String, CommandError> {
    if profile.gateway.is_some() && profile.use_ssl {
        return Err(CommandError::HttpsThroughGateway);
    }
    let host = checked_host(&profile.host)?;
    command(profile, host, profile.port, password)
}

/// [`session_command`] for a profile reached through its SSH gateway: `Enter-PSSession` dials
/// `forward`, the loopback port that carries to the profile's host and port, as the C#
/// Heimdall's tunnel does.
///
/// # Errors
///
/// As [`session_command`].
pub fn session_command_through(
    profile: &WinRmProfile,
    forward: SocketAddr,
    password: PasswordSource,
) -> Result<String, CommandError> {
    // The profile's own checks first: its host is still the one the forward reaches.
    session_command(profile, password)?;
    command(profile, &forward.ip().to_string(), forward.port(), password)
}

/// The guard, the stored password read when there is one, `Enter-PSSession` to `host`:`port`,
/// then the mark.
fn command(
    profile: &WinRmProfile,
    host: &str,
    port: u16,
    password: PasswordSource,
) -> Result<String, CommandError> {
    // The current Windows identity has no password to give.
    let password = if profile.username.is_some() {
        password
    } else {
        PasswordSource::Prompt
    };
    let enter = enter_session(profile, host, port, password)?;
    let guard = local_prompt_guard();
    Ok(match password {
        PasswordSource::Prompt => {
            format!("{guard}; {enter} -ErrorAction Stop; {ENTERED_VARIABLE} = $true")
        }
        PasswordSource::Environment => format!(
            "{guard}; ${SECRET_NAME} = ConvertTo-SecureString $env:{PASSWORD_VARIABLE} \
             -AsPlainText -Force; Remove-Item Env:{PASSWORD_VARIABLE}; {enter} -ErrorAction \
             Stop; {ENTERED_VARIABLE} = $true; Remove-Variable {SECRET_NAME}"
        ),
    })
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

/// The `Enter-PSSession` command for `profile`, to `host`:`port`, a host already checked.
fn enter_session(
    profile: &WinRmProfile,
    host: &str,
    port: u16,
    password: PasswordSource,
) -> Result<String, CommandError> {
    let mut command =
        format!("Enter-PSSession -ComputerName '{host}' -Port {port} -Authentication Negotiate");
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
        let name = quoted_content(checked_username(username)?);
        let credential = match password {
            PasswordSource::Prompt => format!(" -Credential '{name}'"),
            PasswordSource::Environment => {
                format!(" -Credential ([pscredential]::new('{name}', ${SECRET_NAME}))")
            }
        };
        command.push_str(&credential);
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
            gateway: None,
            local_tunnel_port: None,
        }
    }

    /// `Enter-PSSession` for `profile`, to its own host and port.
    fn enter(profile: &WinRmProfile) -> Result<String, CommandError> {
        enter_session(
            profile,
            checked_host(&profile.host)?,
            profile.port,
            PasswordSource::Prompt,
        )
    }

    #[test]
    fn through_a_gateway_the_session_goes_to_the_forward_over_http_only() {
        let routed = WinRmProfile {
            gateway: Some(ProfileId::new("bastion")),
            username: Some("LAB\\admin".to_owned()),
            ..profile("dc01.lab.local")
        };
        let forward: SocketAddr = "127.0.0.1:50123".parse().expect("address");
        let command =
            session_command_through(&routed, forward, PasswordSource::Prompt).expect("valid");
        assert!(
            command.contains(
                "Enter-PSSession -ComputerName '127.0.0.1' -Port 50123 -Authentication Negotiate \
                 -Credential 'LAB\\admin' -ErrorAction Stop"
            ),
            "{command}"
        );
        assert!(
            !command.contains("dc01"),
            "the forward is dialled: {command}"
        );
        let https = WinRmProfile {
            use_ssl: true,
            port: DEFAULT_WINRM_HTTPS_PORT,
            ..routed.clone()
        };
        assert_eq!(
            session_command_through(&https, forward, PasswordSource::Prompt),
            Err(CommandError::HttpsThroughGateway)
        );
        assert_eq!(
            session_command(&https, PasswordSource::Prompt),
            Err(CommandError::HttpsThroughGateway),
            "refused before any forward is opened"
        );
        // The profile's host is still checked: it is where the forward leads.
        let bad = WinRmProfile {
            host: "h$(calc)".to_owned(),
            ..routed
        };
        assert_eq!(
            session_command_through(&bad, forward, PasswordSource::Prompt),
            Err(CommandError::InvalidHost)
        );
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
            session_command(&profile, PasswordSource::Prompt),
            Ok(format!(
                "{}; {} -ErrorAction Stop; $global:HeimdallWinRmEntered = $true",
                local_prompt_guard(),
                enter(&profile).expect("valid")
            ))
        );
    }

    #[test]
    fn a_stored_password_is_read_from_the_environment_then_removed() {
        let profile = WinRmProfile {
            username: Some(" LAB\\o'neil ".to_owned()),
            ..profile("dc01.lab.local")
        };
        let command = session_command(&profile, PasswordSource::Environment).expect("valid");
        assert_eq!(
            command,
            format!(
                "{}; $HeimdallWinRmSecret = ConvertTo-SecureString $env:HEIMDALL_WINRM_PASSWORD \
                 -AsPlainText -Force; Remove-Item Env:HEIMDALL_WINRM_PASSWORD; Enter-PSSession \
                 -ComputerName 'dc01.lab.local' -Port 5985 -Authentication Negotiate \
                 -Credential ([pscredential]::new('LAB\\o''neil', $HeimdallWinRmSecret)) \
                 -ErrorAction Stop; $global:HeimdallWinRmEntered = $true; Remove-Variable \
                 HeimdallWinRmSecret",
                local_prompt_guard()
            )
        );
        // Read, then the variable removed, before the session is tried.
        let read = command.find("$env:HEIMDALL_WINRM_PASSWORD").expect("read");
        let removed = command
            .find("Remove-Item Env:HEIMDALL_WINRM_PASSWORD")
            .expect("removed");
        let entered = command.find("Enter-PSSession").expect("entered");
        assert!(read < removed && removed < entered, "{command}");
    }

    #[test]
    fn the_current_identity_reads_no_password_whatever_is_asked() {
        let current = profile("dc01.lab.local");
        assert_eq!(
            session_command(&current, PasswordSource::Environment),
            session_command(&current, PasswordSource::Prompt)
        );
        let command = session_command(&current, PasswordSource::Environment).expect("valid");
        assert!(!command.contains(PASSWORD_VARIABLE), "{command}");
        assert!(!command.contains("-Credential"), "{command}");
    }

    #[test]
    fn through_a_gateway_the_stored_password_is_still_the_servers() {
        let routed = WinRmProfile {
            gateway: Some(ProfileId::new("bastion")),
            username: Some("LAB\\admin".to_owned()),
            ..profile("dc01.lab.local")
        };
        let forward: SocketAddr = "127.0.0.1:50123".parse().expect("address");
        let command =
            session_command_through(&routed, forward, PasswordSource::Environment).expect("valid");
        assert!(
            command.contains(
                "Enter-PSSession -ComputerName '127.0.0.1' -Port 50123 -Authentication Negotiate \
                 -Credential ([pscredential]::new('LAB\\admin', $HeimdallWinRmSecret))"
            ),
            "{command}"
        );
        // The password is the server's, not the forward's.
        assert_eq!(
            password_endpoint(&routed),
            Some(Endpoint {
                protocol: CredentialProtocol::WinRm,
                host: "dc01.lab.local".to_owned(),
                port: DEFAULT_WINRM_HTTP_PORT,
                username: Some("LAB\\admin".to_owned()),
            })
        );
        assert_eq!(password_endpoint(&profile("dc01.lab.local")), None);
    }

    #[test]
    fn a_refused_profile_gives_no_command_at_all() {
        assert_eq!(
            session_command(&profile("h$(calc)"), PasswordSource::Prompt),
            Err(CommandError::InvalidHost)
        );
        let profile = WinRmProfile {
            username: Some("a\"b".to_owned()),
            ..profile("h")
        };
        assert_eq!(
            session_command(&profile, PasswordSource::Prompt),
            Err(CommandError::InvalidUsername)
        );
    }

    #[test]
    fn the_current_user_connects_over_http_with_no_credential() {
        assert_eq!(
            enter(&profile("dc01.lab.local")).as_deref(),
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
            enter(&profile).as_deref(),
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
        let command = enter(&profile).expect("valid");
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
                enter(&profile(host)),
                Err(CommandError::InvalidHost),
                "{host:?}"
            );
        }
    }

    #[test]
    fn names_and_addresses_of_both_families_are_accepted() {
        for host in ["h", "web-01.lab", "10.0.0.1", "fe80::1", "2001:db8::5"] {
            let command = enter(&profile(host)).expect("valid");
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
        let command = enter(&profile).expect("valid");
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
                enter(&profile),
                Err(CommandError::InvalidUsername),
                "{username:?}"
            );
        }
    }
}
