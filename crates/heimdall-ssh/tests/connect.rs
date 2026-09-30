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

//! End-to-end tests of `heimdall_ssh::connect` against an in-process server.

mod common;

use std::sync::Arc;
use std::time::Duration;

use common::{
    DROP_COMMAND, EXIT_COMMAND, EXIT_THEN_DROP_COMMAND, FIXTURE_PASSPHRASE, KbdRound, LOOPBACK,
    PASSWORD, STEP_TIMEOUT, ScriptedPrompter, Spec, client_public_key, host_public_key,
    options_empty, options_trusting, profile, start,
};
use heimdall_ssh::{
    AuthMethod, ConnectError, KnownHosts, SessionEvent, ShellSession, TerminalSize, connect,
    fingerprint,
};
use russh::MethodKind;
use russh::keys::{Algorithm, HashAlg};
use tokio_util::sync::CancellationToken;

async fn run(
    port: u16,
    key: Option<&str>,
    options: &heimdall_ssh::ConnectOptions,
    prompter: Arc<ScriptedPrompter>,
) -> Result<ShellSession, ConnectError> {
    tokio::time::timeout(
        STEP_TIMEOUT,
        connect(
            &profile(port, key),
            options,
            prompter,
            CancellationToken::new(),
        ),
    )
    .await
    .expect("connect finished in time")
}

/// Reads events until `needle` has been seen in the output.
async fn read_until(session: &mut ShellSession, needle: &[u8]) -> Vec<u8> {
    let mut seen = Vec::new();
    tokio::time::timeout(STEP_TIMEOUT, async {
        while let Some(event) = session.events.recv().await {
            if let SessionEvent::Output(bytes) = event {
                seen.extend_from_slice(&bytes);
                if seen.windows(needle.len()).any(|window| window == needle) {
                    return;
                }
            }
        }
    })
    .await
    .expect("output arrived in time");
    seen
}

async fn closed_status(session: &mut ShellSession) -> Option<u32> {
    tokio::time::timeout(STEP_TIMEOUT, async {
        while let Some(event) = session.events.recv().await {
            if let SessionEvent::Closed { exit_status } = event {
                return exit_status;
            }
        }
        panic!("event stream ended without Closed");
    })
    .await
    .expect("closed in time")
}

// ---- host keys -------------------------------------------------------------------------

#[tokio::test]
async fn first_contact_returns_the_key_without_asking_and_writes_nothing() {
    let server = start(Spec::default()).await;
    let dir = tempfile::tempdir().expect("temp dir");
    let options = options_empty(dir.path());
    let prompter = Arc::new(ScriptedPrompter::passwords(&[PASSWORD]));

    let error = run(server.port, None, &options, prompter.clone())
        .await
        .expect_err("unknown");
    let ConnectError::UnknownHostKey { host, port, key } = error else {
        panic!("expected UnknownHostKey, got {error:?}");
    };
    assert_eq!((host.as_str(), port), (LOOPBACK, server.port));
    assert_eq!(key.key_data(), host_public_key("host-ed25519").key_data());
    assert!(
        prompter.asked().is_empty(),
        "nothing asked: {:?}",
        prompter.asked()
    );
    assert!(
        !options.known_hosts.exists(),
        "nothing written before the user agrees"
    );
}

#[tokio::test]
async fn a_learned_key_is_trusted_on_the_next_connection() {
    let server = start(Spec::default()).await;
    let dir = tempfile::tempdir().expect("temp dir");
    let options = options_empty(dir.path());
    let prompter = Arc::new(ScriptedPrompter::passwords(&[PASSWORD]));

    let Err(ConnectError::UnknownHostKey { host, port, key }) =
        run(server.port, None, &options, prompter.clone()).await
    else {
        panic!("expected UnknownHostKey");
    };
    KnownHosts::new(&options.known_hosts)
        .learn(&host, port, &key)
        .expect("learn");
    let session = run(server.port, None, &options, prompter).await;
    assert!(session.is_ok(), "{:?}", session.err());
}

#[tokio::test]
async fn a_key_trusted_for_the_run_connects_and_is_never_written() {
    let server = start(Spec::default()).await;
    let dir = tempfile::tempdir().expect("temp dir");
    let mut options = options_empty(dir.path());
    let prompter = Arc::new(ScriptedPrompter::passwords(&[PASSWORD, PASSWORD]));

    let Err(ConnectError::UnknownHostKey { host, port, key }) =
        run(server.port, None, &options, prompter.clone()).await
    else {
        panic!("expected UnknownHostKey");
    };
    let trust = heimdall_ssh::RunTrust::default();
    options.run_trust = trust.clone();
    trust.trust(&host, port, *key);
    let session = run(server.port, None, &options, prompter.clone()).await;
    assert!(session.is_ok(), "{:?}", session.err());
    assert!(
        !options.known_hosts.exists(),
        "trusted for this run only: nothing written"
    );

    // Another port of the same host is another server, asked about.
    let other = start(Spec::default()).await;
    let error = run(other.port, None, &options, prompter)
        .await
        .expect_err("unknown");
    assert!(
        matches!(error, ConnectError::UnknownHostKey { .. }),
        "{error:?}"
    );
}

#[tokio::test]
async fn a_changed_key_is_refused_without_asking() {
    let server = start(Spec::default()).await;
    let dir = tempfile::tempdir().expect("temp dir");
    let options = options_trusting(dir.path(), server.port, "host-ed25519-other");
    let prompter = Arc::new(ScriptedPrompter::passwords(&[PASSWORD]));

    let error = run(server.port, None, &options, prompter.clone())
        .await
        .expect_err("changed");
    let ConnectError::HostKeyChanged {
        recorded, offered, ..
    } = error
    else {
        panic!("expected HostKeyChanged, got {error:?}");
    };
    assert_eq!(
        recorded,
        fingerprint(&host_public_key("host-ed25519-other"))
    );
    assert_eq!(offered, fingerprint(&host_public_key("host-ed25519")));
    assert!(prompter.asked().is_empty());
}

#[tokio::test]
async fn a_server_without_the_recorded_algorithm_is_refused() {
    // The server holds only an ed25519 key; an ECDSA key is recorded for it.
    let server = start(Spec::default()).await;
    let dir = tempfile::tempdir().expect("temp dir");
    let options = options_trusting(dir.path(), server.port, "host-ecdsa");
    let prompter = Arc::new(ScriptedPrompter::passwords(&[PASSWORD]));

    let error = run(server.port, None, &options, prompter.clone())
        .await
        .expect_err("mismatch");
    assert!(
        matches!(error, ConnectError::HostKeyAlgorithmMismatch { .. }),
        "got {error:?}"
    );
    assert!(prompter.asked().is_empty());
}

#[tokio::test]
async fn an_unreadable_known_hosts_refuses_instead_of_reading_as_empty() {
    let server = start(Spec::default()).await;
    let dir = tempfile::tempdir().expect("temp dir");
    let options = options_empty(dir.path());
    // A directory where the file should be: it exists and cannot be read as a file.
    std::fs::create_dir(&options.known_hosts).expect("directory");
    let prompter = Arc::new(ScriptedPrompter::passwords(&[PASSWORD]));

    let error = run(server.port, None, &options, prompter)
        .await
        .expect_err("refused");
    assert!(
        matches!(error, ConnectError::KnownHosts(_)),
        "got {error:?}"
    );
}

#[tokio::test]
async fn a_host_name_that_could_inject_a_known_hosts_entry_is_refused() {
    let dir = tempfile::tempdir().expect("temp dir");
    let options = options_empty(dir.path());
    let mut target = profile(22, None);
    target.host = "a,bank.internal".to_owned();
    let error = connect(
        &target,
        &options,
        Arc::new(ScriptedPrompter::default()),
        CancellationToken::new(),
    )
    .await
    .expect_err("refused");
    assert!(matches!(error, ConnectError::InvalidHost), "got {error:?}");
}

// ---- password --------------------------------------------------------------------------

#[tokio::test]
async fn the_right_password_opens_the_session() {
    let server = start(Spec::default()).await;
    let dir = tempfile::tempdir().expect("temp dir");
    let options = options_trusting(dir.path(), server.port, "host-ed25519");
    let prompter = Arc::new(ScriptedPrompter::passwords(&[PASSWORD]));

    assert!(run(server.port, None, &options, prompter).await.is_ok());
    assert_eq!(
        server.observed.lock().expect("observed").password_attempts,
        1
    );
}

#[tokio::test]
async fn a_wrong_password_is_asked_again() {
    let server = start(Spec::default()).await;
    let dir = tempfile::tempdir().expect("temp dir");
    let options = options_trusting(dir.path(), server.port, "host-ed25519");
    let prompter = Arc::new(ScriptedPrompter::passwords(&["wrong", PASSWORD]));

    let result = run(server.port, None, &options, prompter.clone()).await;
    assert!(result.is_ok(), "{:?}", result.err());
    assert_eq!(
        server.observed.lock().expect("observed").password_attempts,
        2
    );
    assert_eq!(prompter.asked(), vec!["password", "password"]);
}

#[tokio::test]
async fn a_cancelled_password_question_cancels_the_connection() {
    let server = start(Spec::default()).await;
    let dir = tempfile::tempdir().expect("temp dir");
    let options = options_trusting(dir.path(), server.port, "host-ed25519");
    let prompter = Arc::new(ScriptedPrompter::default());

    let error = run(server.port, None, &options, prompter)
        .await
        .expect_err("cancelled");
    assert!(matches!(error, ConnectError::Cancelled), "got {error:?}");
}

#[tokio::test]
async fn an_unanswered_question_times_out() {
    let server = start(Spec::default()).await;
    let dir = tempfile::tempdir().expect("temp dir");
    let mut options = options_trusting(dir.path(), server.port, "host-ed25519");
    options.prompt_timeout = Duration::from_millis(200);
    let prompter = Arc::new(ScriptedPrompter {
        hang: true,
        ..ScriptedPrompter::default()
    });

    let error = run(server.port, None, &options, prompter)
        .await
        .expect_err("timed out");
    assert!(
        matches!(error, ConnectError::PromptTimedOut),
        "got {error:?}"
    );
}

#[tokio::test]
async fn cancelling_while_a_question_is_open_ends_the_attempt() {
    let server = start(Spec::default()).await;
    let dir = tempfile::tempdir().expect("temp dir");
    let options = options_trusting(dir.path(), server.port, "host-ed25519");
    let prompter = Arc::new(ScriptedPrompter {
        hang: true,
        ..ScriptedPrompter::default()
    });
    let cancel = CancellationToken::new();
    let trigger = cancel.clone();
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(300)).await;
        trigger.cancel();
    });

    let result = tokio::time::timeout(
        STEP_TIMEOUT,
        connect(&profile(server.port, None), &options, prompter, cancel),
    )
    .await
    .expect("finished in time");
    assert!(
        matches!(result, Err(ConnectError::Cancelled)),
        "got {:?}",
        result.err()
    );
}

#[tokio::test]
async fn too_many_failures_reads_as_the_servers_disconnect_not_as_a_refusal() {
    let server = start(Spec {
        max_auth_attempts: 2,
        ..Spec::default()
    })
    .await;
    let dir = tempfile::tempdir().expect("temp dir");
    let options = options_trusting(dir.path(), server.port, "host-ed25519");
    let prompter = Arc::new(ScriptedPrompter::passwords(&["one", "two", "three"]));

    let error = run(server.port, None, &options, prompter)
        .await
        .expect_err("refused");
    // The server's own words reach the caller; an empty method list after the disconnect
    // must not be reported as "every method refused".
    let ConnectError::Disconnected {
        server_message: Some(message),
    } = error
    else {
        panic!("expected Disconnected with the server's message, got {error:?}");
    };
    assert!(!message.is_empty());
}

#[tokio::test]
async fn a_server_that_gave_up_waiting_for_an_answer_reads_as_a_disconnect() {
    // OpenSSH closes a connection whose authentication takes longer than its
    // LoginGraceTime; the answer then goes nowhere. Measured by hand on 2026-09-26: this
    // surfaced as "Channel send error", a protocol error.
    let server = start(Spec {
        methods: vec![MethodKind::Password],
        inactivity_timeout: Some(Duration::from_millis(300)),
        ..Spec::default()
    })
    .await;
    let dir = tempfile::tempdir().expect("temp dir");
    let options = options_trusting(dir.path(), server.port, "host-ed25519");
    let prompter = Arc::new(ScriptedPrompter {
        delay: Duration::from_millis(1500),
        ..ScriptedPrompter::passwords(&[PASSWORD])
    });

    let error = run(server.port, None, &options, prompter)
        .await
        .expect_err("the server left");
    assert!(
        matches!(error, ConnectError::Disconnected { .. }),
        "got {error:?}"
    );
}

// ---- keyboard-interactive --------------------------------------------------------------

fn two_prompts_then_zero() -> Vec<KbdRound> {
    vec![
        KbdRound {
            prompts: vec![
                ("Password: ".to_owned(), false),
                ("Code: ".to_owned(), true),
            ],
            expected: vec!["pw".to_owned(), "123456".to_owned()],
        },
        KbdRound {
            prompts: Vec::new(),
            expected: Vec::new(),
        },
    ]
}

#[tokio::test]
async fn keyboard_interactive_answers_in_order_and_a_zero_prompt_round_is_answered_alone() {
    let server = start(Spec {
        methods: vec![MethodKind::KeyboardInteractive],
        kbd_rounds: two_prompts_then_zero(),
        ..Spec::default()
    })
    .await;
    let dir = tempfile::tempdir().expect("temp dir");
    let options = options_trusting(dir.path(), server.port, "host-ed25519");
    let prompter = Arc::new(ScriptedPrompter {
        kbd: vec![Some(vec!["pw".to_owned(), "123456".to_owned()])]
            .into_iter()
            .collect::<std::collections::VecDeque<_>>()
            .into(),
        ..ScriptedPrompter::default()
    });

    assert!(
        run(server.port, None, &options, prompter.clone())
            .await
            .is_ok()
    );
    assert_eq!(
        prompter.asked(),
        vec!["kbd"],
        "the empty round is not asked"
    );
    assert_eq!(
        server.observed.lock().expect("observed").kbd_answers,
        vec![vec!["pw".to_owned(), "123456".to_owned()], Vec::new()]
    );
}

#[tokio::test]
async fn cancelling_a_keyboard_interactive_round_ends_promptly() {
    let server = start(Spec {
        kbd_rounds: two_prompts_then_zero(),
        ..Spec::default()
    })
    .await;
    let dir = tempfile::tempdir().expect("temp dir");
    let options = options_trusting(dir.path(), server.port, "host-ed25519");
    // No keyboard-interactive answer queued: the round is cancelled. A password is queued,
    // and must not be used: russh would wait forever for the round's answer.
    let prompter = Arc::new(ScriptedPrompter::passwords(&[PASSWORD]));

    let error = run(server.port, None, &options, prompter.clone())
        .await
        .expect_err("cancelled");
    assert!(matches!(error, ConnectError::Cancelled), "got {error:?}");
    assert_eq!(prompter.asked(), vec!["kbd"]);
    assert_eq!(
        server.observed.lock().expect("observed").password_attempts,
        0
    );
}

// ---- keys ------------------------------------------------------------------------------

async fn key_login(key: &str, public: &str, prompter: ScriptedPrompter) -> Vec<&'static str> {
    let server = start(Spec {
        methods: vec![MethodKind::PublicKey],
        authorized: vec![client_public_key(public)],
        ..Spec::default()
    })
    .await;
    let dir = tempfile::tempdir().expect("temp dir");
    let options = options_trusting(dir.path(), server.port, "host-ed25519");
    let prompter = Arc::new(prompter);
    let result = run(server.port, Some(key), &options, prompter.clone()).await;
    assert!(result.is_ok(), "{key}: {:?}", result.err());
    prompter.asked()
}

#[tokio::test]
async fn an_unencrypted_openssh_key_logs_in_without_a_question() {
    let asked = key_login(
        "ed25519-openssh",
        "ed25519-openssh",
        ScriptedPrompter::default(),
    )
    .await;
    assert!(asked.is_empty(), "{asked:?}");
}

#[tokio::test]
async fn an_encrypted_openssh_key_asks_again_after_a_wrong_passphrase() {
    let asked = key_login(
        "ed25519-openssh-encrypted",
        "ed25519-openssh-encrypted",
        ScriptedPrompter::passphrases(&["wrong", FIXTURE_PASSPHRASE]),
    )
    .await;
    assert_eq!(asked, vec!["passphrase", "passphrase"]);
}

#[tokio::test]
async fn putty_keys_of_both_versions_log_in() {
    assert!(
        key_login(
            "ed25519-ppk3.ppk",
            "ed25519-openssh",
            ScriptedPrompter::default()
        )
        .await
        .is_empty()
    );
    assert!(
        key_login(
            "ed25519-ppk2.ppk",
            "ed25519-openssh",
            ScriptedPrompter::default()
        )
        .await
        .is_empty()
    );
    let asked = key_login(
        "ed25519-ppk3-encrypted.ppk",
        "ed25519-openssh",
        ScriptedPrompter::passphrases(&["wrong", FIXTURE_PASSPHRASE]),
    )
    .await;
    assert_eq!(asked, vec!["passphrase", "passphrase"]);
}

#[tokio::test]
async fn an_rsa_key_signs_with_sha2_for_a_server_that_refuses_sha1() {
    for key in ["rsa-openssh", "rsa-ppk3.ppk"] {
        let server = start(Spec {
            methods: vec![MethodKind::PublicKey],
            authorized: vec![client_public_key("rsa-openssh")],
            key_algorithms: Some(vec![
                Algorithm::Ed25519,
                Algorithm::Rsa {
                    hash: Some(HashAlg::Sha512),
                },
                Algorithm::Rsa {
                    hash: Some(HashAlg::Sha256),
                },
            ]),
            ..Spec::default()
        })
        .await;
        let dir = tempfile::tempdir().expect("temp dir");
        let options = options_trusting(dir.path(), server.port, "host-ed25519");
        let result = run(
            server.port,
            Some(key),
            &options,
            Arc::new(ScriptedPrompter::default()),
        )
        .await;
        assert!(result.is_ok(), "{key}: {:?}", result.err());
    }
}

#[tokio::test]
async fn a_refused_key_falls_back_to_the_password() {
    let server = start(Spec::default()).await;
    let dir = tempfile::tempdir().expect("temp dir");
    let options = options_trusting(dir.path(), server.port, "host-ed25519");
    let prompter = Arc::new(ScriptedPrompter::passwords(&[PASSWORD]));

    let result = run(
        server.port,
        Some("ed25519-openssh"),
        &options,
        prompter.clone(),
    )
    .await;
    assert!(result.is_ok(), "{:?}", result.err());
    assert_eq!(prompter.asked(), vec!["password"]);
}

#[tokio::test]
async fn every_method_refused_lists_what_was_tried() {
    let server = start(Spec {
        methods: vec![MethodKind::PublicKey, MethodKind::Password],
        ..Spec::default()
    })
    .await;
    let dir = tempfile::tempdir().expect("temp dir");
    let options = options_trusting(dir.path(), server.port, "host-ed25519");
    let prompter = Arc::new(ScriptedPrompter::passwords(&["a", "b", "c"]));

    let error = run(server.port, Some("ed25519-openssh"), &options, prompter)
        .await
        .expect_err("refused");
    let ConnectError::AuthenticationFailed { tried } = error else {
        panic!("expected AuthenticationFailed, got {error:?}");
    };
    assert_eq!(tried, vec![AuthMethod::KeyFile, AuthMethod::Password]);
}

// ---- session ---------------------------------------------------------------------------

async fn open_session(spec: Spec) -> (common::TestServer, ShellSession) {
    let server = start(spec).await;
    let dir = tempfile::tempdir().expect("temp dir");
    let mut options = options_trusting(dir.path(), server.port, "host-ed25519");
    options.initial_size = TerminalSize {
        cols: 100,
        rows: 30,
        pixel_width: 0,
        pixel_height: 0,
    };
    let session = run(
        server.port,
        None,
        &options,
        Arc::new(ScriptedPrompter::passwords(&[PASSWORD])),
    )
    .await
    .expect("session");
    (server, session)
}

#[tokio::test]
async fn the_pty_is_requested_with_the_terminal_type_and_size() {
    let (server, _session) = open_session(Spec::default()).await;
    assert_eq!(
        server.observed.lock().expect("observed").pty,
        Some(("xterm-256color".to_owned(), 100, 30))
    );
}

#[tokio::test]
async fn input_reaches_the_shell_and_output_comes_back() {
    let (_server, mut session) = open_session(Spec::default()).await;
    session
        .input
        .write(b"hello heimdall".to_vec())
        .expect("write");
    read_until(&mut session, b"hello heimdall").await;
}

#[tokio::test]
async fn a_resize_reaches_the_server_with_columns_and_rows_in_order() {
    let (server, session) = open_session(Spec::default()).await;
    session
        .input
        .resize(TerminalSize {
            cols: 132,
            rows: 43,
            pixel_width: 0,
            pixel_height: 0,
        })
        .expect("resize");
    tokio::time::timeout(STEP_TIMEOUT, async {
        loop {
            if server
                .observed
                .lock()
                .expect("observed")
                .resizes
                .contains(&(132, 43))
            {
                return;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .expect("resize observed");
}

#[tokio::test]
async fn the_exit_status_is_delivered_with_the_close() {
    let (_server, mut session) = open_session(Spec {
        exit_status: 42,
        ..Spec::default()
    })
    .await;
    session.input.write(EXIT_COMMAND.to_vec()).expect("write");
    assert_eq!(closed_status(&mut session).await, Some(42));
}

/// The event that ended `session`, after `command`.
async fn ending_after(session: &mut ShellSession, command: &[u8]) -> Option<SessionEvent> {
    session.input.write(command.to_vec()).expect("write");
    tokio::time::timeout(STEP_TIMEOUT, async {
        let mut last = None;
        while let Some(event) = session.events.recv().await {
            if !matches!(event, SessionEvent::Output(_)) {
                last = Some(event);
            }
        }
        last
    })
    .await
    .expect("ended in time")
}

#[tokio::test]
async fn a_connection_dropped_under_the_shell_is_lost_not_closed() {
    let (_server, mut session) = open_session(Spec::default()).await;
    assert_eq!(
        ending_after(&mut session, DROP_COMMAND).await,
        Some(SessionEvent::Lost),
        "the server never closed the session: its connection was lost"
    );
}

#[tokio::test]
async fn a_shell_that_said_how_it_ended_is_closed_whatever_happens_to_the_connection() {
    let (_server, mut session) = open_session(Spec {
        exit_status: 7,
        ..Spec::default()
    })
    .await;
    assert_eq!(
        ending_after(&mut session, EXIT_THEN_DROP_COMMAND).await,
        Some(SessionEvent::Closed {
            exit_status: Some(7)
        }),
        "the shell ended: nothing to reconnect"
    );
}

#[tokio::test]
async fn closing_from_the_client_ends_the_event_stream() {
    let (_server, mut session) = open_session(Spec::default()).await;
    session.input.close();
    assert_eq!(closed_status(&mut session).await, None);
    assert!(session.input.write(b"late".to_vec()).is_err());
}

#[tokio::test]
async fn a_large_paste_echoed_back_with_resizes_and_a_slow_reader_does_not_deadlock() {
    // Four megabytes, twice the server's 2 MiB window, written in 1 KiB pieces the server
    // echoes one by one, so the echoes arrive as thousands of small packets, far beyond
    // russh's 100-message channel buffer. The reader pauses and resizes at every megabyte.
    // When `resize` awaited russh, this hung at exactly 2 MiB in 2 runs out of 3
    // (2026-09-26): the resize waited for the russh loop, which waited for unread output.
    const PASTE_BYTES: usize = 4 * 1024 * 1024;
    const CHUNK_BYTES: usize = 1024;
    const PAUSE_EVERY_BYTES: usize = 1024 * 1024;
    let (server, mut session) = open_session(Spec::default()).await;
    for _ in 0..PASTE_BYTES / CHUNK_BYTES {
        session.input.write(vec![b'x'; CHUNK_BYTES]).expect("write");
    }
    let mut received = 0;
    let finished = tokio::time::timeout(STEP_TIMEOUT, async {
        while received < PASTE_BYTES {
            let Some(SessionEvent::Output(bytes)) = session.events.recv().await else {
                panic!("session ended early");
            };
            received += bytes.len();
            if received % PAUSE_EVERY_BYTES < bytes.len() {
                tokio::time::sleep(Duration::from_millis(50)).await;
                session
                    .input
                    .resize(TerminalSize {
                        cols: 90,
                        rows: 20,
                        pixel_width: 0,
                        pixel_height: 0,
                    })
                    .expect("resize during the paste");
            }
        }
    })
    .await;
    let server_received = server.observed.lock().expect("observed").bytes_received;
    assert!(
        finished.is_ok(),
        "stalled: client received {received}, server received {server_received}"
    );
    assert_eq!(server_received, PASTE_BYTES);
}
