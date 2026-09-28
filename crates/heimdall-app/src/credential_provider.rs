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

//! Runs the external credential provider's commands, as the C# `CommandCredentialProvider`:
//! no shell, no window, the unlock secret on standard input, stopped when it takes too long.
//! Standard error is never read: a password manager may print anything there.

use std::process::Stdio;
use std::time::Duration;

use heimdall_core::credential_provider::{
    Lookup, ProviderSettings, TemplateProblem, command_line, password_in, test_lookup,
};
use heimdall_ssh::Secret;
use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};
use zeroize::Zeroizing;

/// Windows: the command opens no console window.
#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// Why a command gave no password.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProviderFailure {
    /// The template cannot run.
    Template(TemplateProblem),
    /// The program could not be started.
    Launch(String),
    /// It ended with this exit code, or was ended by a signal.
    Exit(Option<i32>),
    /// It printed nothing.
    Empty,
    /// It took longer than allowed and was stopped.
    TimedOut,
}

/// What the provider gave.
#[derive(Debug, Clone)]
pub struct Provided {
    /// The password.
    pub password: Secret,
    /// The user name from the user name command, when the profile has none.
    pub username: Option<String>,
}

/// Runs `argv` and returns its standard output; `unlock` is written to its standard input,
/// then a line break, then the input is closed.
async fn run(
    argv: &[String],
    unlock: Option<&Secret>,
    timeout: Duration,
) -> Result<Zeroizing<Vec<u8>>, ProviderFailure> {
    let (program, arguments) = argv
        .split_first()
        .ok_or(ProviderFailure::Template(TemplateProblem::Empty))?;
    let mut command = tokio::process::Command::new(program);
    command
        .args(arguments)
        .stdin(if unlock.is_some() {
            Stdio::piped()
        } else {
            Stdio::null()
        })
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        // Dropped when the time is up: stopped, not left running.
        .kill_on_drop(true);
    #[cfg(windows)]
    command.creation_flags(CREATE_NO_WINDOW);
    let mut child = command
        .spawn()
        .map_err(|error| ProviderFailure::Launch(error.to_string()))?;
    let stdin = child.stdin.take();
    let stdout = child.stdout.take();
    let work = async move {
        if let (Some(mut stdin), Some(secret)) = (stdin, unlock) {
            let mut line = Zeroizing::new(secret.expose().as_bytes().to_vec());
            line.push(b'\n');
            // A program that does not read its input closes it: not a failure.
            let _ = stdin.write_all(&line).await;
        }
        let mut output = Zeroizing::new(Vec::new());
        if let Some(mut stdout) = stdout {
            stdout
                .read_to_end(&mut output)
                .await
                .map_err(|error| ProviderFailure::Launch(error.to_string()))?;
        }
        let status = child
            .wait()
            .await
            .map_err(|error| ProviderFailure::Launch(error.to_string()))?;
        if status.success() {
            Ok(output)
        } else {
            Err(ProviderFailure::Exit(status.code()))
        }
    };
    tokio::time::timeout(timeout, work)
        .await
        .unwrap_or(Err(ProviderFailure::TimedOut))
}

/// The text a command printed, as its password rule reads it.
async fn ask_one(
    template: &str,
    lookup: &Lookup,
    settings: &ProviderSettings,
    unlock: Option<&Secret>,
) -> Result<Zeroizing<String>, ProviderFailure> {
    let argv = command_line(template, lookup, settings).map_err(ProviderFailure::Template)?;
    let output = run(&argv, unlock, settings.timeout).await?;
    let text = Zeroizing::new(String::from_utf8_lossy(&output).into_owned());
    password_in(&text, settings.first_line_only)
        .map(Zeroizing::new)
        .ok_or(ProviderFailure::Empty)
}

/// Asks the provider for `lookup`'s password and, when the profile has no user name and a
/// user name command is set, its user name: a user name that cannot be had is left out, as
/// the C# one falls back.
///
/// # Errors
///
/// [`ProviderFailure`] when no password was given.
pub async fn ask(
    settings: &ProviderSettings,
    lookup: &Lookup,
    unlock: Option<&Secret>,
) -> Result<Provided, ProviderFailure> {
    let password = ask_one(&settings.command, lookup, settings, unlock).await?;
    let wants_user = lookup.user.as_deref().is_none_or(str::is_empty)
        && !settings.username_command.trim().is_empty();
    let username = if wants_user {
        ask_one(&settings.username_command, lookup, settings, unlock)
            .await
            .ok()
            .map(|name| String::clone(&name))
    } else {
        None
    };
    Ok(Provided {
        password: Secret::new(String::clone(&password)),
        username,
    })
}

/// What the Settings page's Test button found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProviderTest {
    /// Running.
    Running,
    /// A password was given; it is never shown.
    Success,
    /// The command printed nothing, or ended in failure.
    NoResult,
    /// The command took longer than allowed.
    TimedOut,
    /// The command cannot run: the reason.
    Failed(ProviderFailure),
}

/// Runs the password command with the C# Test button's values.
pub async fn test(settings: ProviderSettings, unlock: Option<Secret>) -> ProviderTest {
    match ask(&settings, &test_lookup(), unlock.as_ref()).await {
        Ok(_) => ProviderTest::Success,
        Err(ProviderFailure::Empty | ProviderFailure::Exit(_)) => ProviderTest::NoResult,
        Err(ProviderFailure::TimedOut) => ProviderTest::TimedOut,
        Err(failure) => ProviderTest::Failed(failure),
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    fn settings(command: &str) -> ProviderSettings {
        ProviderSettings {
            enabled: true,
            command: command.to_owned(),
            ..ProviderSettings::default()
        }
    }

    fn lookup(user: Option<&str>) -> Lookup {
        Lookup {
            host: "web.lab".to_owned(),
            port: 22,
            user: user.map(str::to_owned),
            title: "Web server".to_owned(),
        }
    }

    #[tokio::test]
    async fn the_output_is_the_password() {
        let provided = ask(
            &settings(r#"printf "  %s-pw \n" "{Title}""#),
            &lookup(Some("admin")),
            None,
        )
        .await
        .expect("given");
        assert_eq!(provided.password.expose(), "Web server-pw");
        assert_eq!(provided.username, None);
    }

    #[tokio::test]
    async fn the_unlock_secret_is_the_first_line_of_its_input() {
        let provided = ask(
            &settings(r#"sh -c "read secret; printf %s-open $secret""#),
            &lookup(Some("admin")),
            Some(&Secret::new("hunter2".to_owned())),
        )
        .await
        .expect("given");
        assert_eq!(provided.password.expose(), "hunter2-open");
    }

    #[tokio::test]
    async fn without_a_secret_the_input_is_empty() {
        let provided = ask(
            &settings(r#"sh -c "cat; printf done""#),
            &lookup(Some("admin")),
            None,
        )
        .await
        .expect("given");
        assert_eq!(provided.password.expose(), "done");
    }

    #[tokio::test]
    async fn the_user_name_is_asked_only_for_a_profile_without_one() {
        let with_user = ProviderSettings {
            username_command: "printf deploy".to_owned(),
            ..settings("printf pw")
        };
        let provided = ask(&with_user, &lookup(None), None).await.expect("given");
        assert_eq!(provided.username.as_deref(), Some("deploy"));
        let provided = ask(&with_user, &lookup(Some("admin")), None)
            .await
            .expect("given");
        assert_eq!(provided.username, None, "the profile's name stays");
        let failing = ProviderSettings {
            username_command: "false".to_owned(),
            ..settings("printf pw")
        };
        let provided = ask(&failing, &lookup(None), None).await.expect("given");
        assert_eq!(provided.username, None, "a failure leaves it out");
    }

    #[tokio::test]
    async fn a_failure_gives_no_password() {
        assert_eq!(
            ask(&settings("false"), &lookup(None), None).await.err(),
            Some(ProviderFailure::Exit(Some(1)))
        );
        assert_eq!(
            // Only blanks and a line break.
            ask(&settings(r#"printf "  \n ""#), &lookup(None), None)
                .await
                .err(),
            Some(ProviderFailure::Empty)
        );
        assert!(matches!(
            ask(&settings("no-such-program-heimdall"), &lookup(None), None)
                .await
                .err(),
            Some(ProviderFailure::Launch(_))
        ));
        assert_eq!(
            ask(&settings(r#"tool "{Title}"#), &lookup(None), None)
                .await
                .err(),
            Some(ProviderFailure::Template(TemplateProblem::UnclosedQuote))
        );
    }

    #[tokio::test]
    async fn a_command_too_slow_is_stopped() {
        let slow = ProviderSettings {
            timeout: Duration::from_millis(200),
            ..settings("sleep 5")
        };
        let started = std::time::Instant::now();
        assert_eq!(
            ask(&slow, &lookup(None), None).await.err(),
            Some(ProviderFailure::TimedOut)
        );
        assert!(started.elapsed() < Duration::from_secs(3));
    }

    #[tokio::test]
    async fn the_test_button_sorts_the_outcomes() {
        assert_eq!(
            test(settings("printf pw"), None).await,
            ProviderTest::Success
        );
        assert_eq!(test(settings("false"), None).await, ProviderTest::NoResult);
        assert_eq!(test(settings("true"), None).await, ProviderTest::NoResult);
        let slow = ProviderSettings {
            timeout: Duration::from_millis(100),
            ..settings("sleep 5")
        };
        assert_eq!(test(slow, None).await, ProviderTest::TimedOut);
        assert_eq!(
            test(settings(" "), None).await,
            ProviderTest::Failed(ProviderFailure::Template(TemplateProblem::Empty))
        );
    }
}
