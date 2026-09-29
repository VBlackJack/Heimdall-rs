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

//! Types a shell's post-connect steps, as the C# `PostConnectSequenceRunner`: in order, each
//! after its delay and followed by a new line, reporting each step as it goes.

use std::sync::Arc;
use std::time::Duration;

use heimdall_core::post_connect::{OnFailure, PostConnectStep};
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use crate::event::{ConnectionEvent, PostConnectProgress, StepStatus};
use crate::sink::InputSink;
use crate::text::visible_text;

/// Types `steps` into `sink` until they are done or `stop` is cancelled, reporting on
/// `events`; ends with [`ConnectionEvent::PostConnectDone`].
pub(crate) async fn run(
    steps: Vec<PostConnectStep>,
    sink: Arc<dyn InputSink>,
    events: mpsc::Sender<ConnectionEvent>,
    stop: CancellationToken,
) {
    let total = steps.len();
    log::info!("post-connect: {total} step(s)");
    for (index, step) in steps.iter().enumerate() {
        let report = |status| {
            ConnectionEvent::PostConnect(PostConnectProgress {
                step: index + 1,
                total,
                command: visible_text(&step.input),
                status,
                stop: stop.clone(),
            })
        };
        if !step.runs() {
            let _ = events.send(report(StepStatus::Skipped)).await;
            continue;
        }
        let _ = events.send(report(StepStatus::Running)).await;
        let waited = tokio::select! {
            () = stop.cancelled() => false,
            () = tokio::time::sleep(Duration::from_millis(u64::from(step.delay_ms))) => true,
        };
        if !waited {
            let _ = events.send(report(StepStatus::Cancelled)).await;
            break;
        }
        let mut line = step.input.clone().into_bytes();
        line.push(b'\n');
        if sink.write(line).is_ok() {
            let _ = events.send(report(StepStatus::Completed)).await;
        } else {
            log::warn!(
                "post-connect: step {} of {total} could not be typed",
                index + 1
            );
            let _ = events.send(report(StepStatus::Failed)).await;
            if step.on_failure == OnFailure::Stop {
                break;
            }
        }
    }
    let _ = events.send(ConnectionEvent::PostConnectDone).await;
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use heimdall_core::post_connect::{OnFailure, PostConnectStep};
    use heimdall_ssh::{SessionClosed, TerminalSize};
    use tokio::sync::mpsc;
    use tokio_util::sync::CancellationToken;

    use super::run;
    use crate::event::{ConnectionEvent, StepStatus};
    use crate::sink::InputSink;

    /// Records what is typed; refuses once `closed`.
    #[derive(Debug, Default)]
    struct Shell {
        typed: Mutex<Vec<u8>>,
        closed: bool,
    }

    impl InputSink for Shell {
        fn write(&self, bytes: Vec<u8>) -> Result<(), SessionClosed> {
            if self.closed {
                return Err(SessionClosed);
            }
            self.typed.lock().expect("typed").extend(bytes);
            Ok(())
        }
        fn resize(&self, _size: TerminalSize) -> Result<(), SessionClosed> {
            Ok(())
        }
        fn close(&self) {}
    }

    fn step(input: &str) -> PostConnectStep {
        PostConnectStep {
            delay_ms: 0,
            ..PostConnectStep::new(input)
        }
    }

    /// Runs `steps` to the end; what was typed, and each step's last status in order.
    async fn outcome(
        steps: Vec<PostConnectStep>,
        shell: Arc<Shell>,
        stop: CancellationToken,
    ) -> (String, Vec<(usize, StepStatus)>) {
        let (events, mut received) = mpsc::channel(64);
        run(steps, shell.clone(), events, stop).await;
        let mut statuses = Vec::new();
        let mut done = false;
        while let Ok(event) = received.try_recv() {
            match event {
                ConnectionEvent::PostConnect(progress) => {
                    statuses.push((progress.step, progress.status));
                }
                ConnectionEvent::PostConnectDone => done = true,
                other => panic!("{other:?}"),
            }
        }
        assert!(done, "the sequence says it is over");
        let typed = String::from_utf8(shell.typed.lock().expect("typed").clone()).expect("text");
        (typed, statuses)
    }

    #[tokio::test]
    async fn steps_are_typed_in_order_each_on_its_line_and_the_off_ones_skipped() {
        let off = PostConnectStep {
            enabled: false,
            ..step("rm -rf /tmp/x")
        };
        let (typed, statuses) = outcome(
            vec![step("sudo -i"), off, step("  "), step("cd /srv")],
            Arc::new(Shell::default()),
            CancellationToken::new(),
        )
        .await;
        assert_eq!(typed, "sudo -i\ncd /srv\n");
        assert_eq!(
            statuses,
            [
                (1, StepStatus::Running),
                (1, StepStatus::Completed),
                (2, StepStatus::Skipped),
                (3, StepStatus::Skipped),
                (4, StepStatus::Running),
                (4, StepStatus::Completed),
            ]
        );
    }

    #[tokio::test]
    async fn a_step_waits_its_delay_and_a_stop_during_it_types_nothing_more() {
        let stop = CancellationToken::new();
        let slow = PostConnectStep {
            delay_ms: 60_000,
            ..step("never")
        };
        let shell = Arc::new(Shell::default());
        let (events, mut received) = mpsc::channel(64);
        let running = tokio::spawn(run(
            vec![step("first"), slow, step("after")],
            shell.clone(),
            events,
            stop.clone(),
        ));
        // Past the first step, into the second's delay.
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
        assert_eq!(shell.typed.lock().expect("typed").as_slice(), b"first\n");
        stop.cancel();
        running.await.expect("ran");
        assert_eq!(shell.typed.lock().expect("typed").as_slice(), b"first\n");
        let mut last = None;
        while let Ok(event) = received.try_recv() {
            if let ConnectionEvent::PostConnect(progress) = event {
                last = Some((progress.step, progress.status));
            }
        }
        assert_eq!(last, Some((2, StepStatus::Cancelled)));
    }

    #[tokio::test]
    async fn a_step_that_cannot_be_typed_stops_the_sequence_only_when_it_says_so() {
        let closed = || {
            Arc::new(Shell {
                closed: true,
                ..Shell::default()
            })
        };
        let (_, statuses) = outcome(
            vec![step("a"), step("b")],
            closed(),
            CancellationToken::new(),
        )
        .await;
        assert_eq!(
            statuses,
            [
                (1, StepStatus::Running),
                (1, StepStatus::Failed),
                (2, StepStatus::Running),
                (2, StepStatus::Failed),
            ],
            "continue: the next step is tried"
        );
        let stopping = PostConnectStep {
            on_failure: OnFailure::Stop,
            ..step("a")
        };
        let (_, statuses) = outcome(
            vec![stopping, step("b")],
            closed(),
            CancellationToken::new(),
        )
        .await;
        assert_eq!(
            statuses,
            [(1, StepStatus::Running), (1, StepStatus::Failed)],
            "stop: nothing after"
        );
    }
}
