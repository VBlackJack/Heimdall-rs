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

//! Unix: the PTY master, non-blocking, read and written through tokio's `AsyncFd`; the exit
//! learnt from `SIGCHLD`.

use std::fs::File;
use std::io::{self, ErrorKind, Read as _, Write as _};
use std::time::Duration;

use alacritty_terminal::event::{OnResize as _, WindowSize};
use alacritty_terminal::tty::{self, ChildEvent, EventedPty as _, Options, Pty};
use rustix::process::{Pid, Signal, kill_process_group};
use tokio::io::unix::AsyncFd;
use tokio::signal::unix::{SignalKind, signal};
use tokio::sync::mpsc;
use tokio::time::Instant;

use super::{Command, EVENT_QUEUE, LocalEvent, LocalInput, LocalSession, READ_BUFFER, window_size};

/// `EIO`: the master reads it once no process holds the slave any more.
const EIO: i32 = 5;

/// How long a hung-up shell has to exit before it is killed.
const HANG_UP_GRACE: Duration = Duration::from_secs(2);

pub(super) fn spawn(options: &Options, size: WindowSize) -> io::Result<LocalSession> {
    let pty = tty::new(options, size, 0)?;
    // Clones of the master: the same open file, non-blocking as alacritty set it.
    let reader = AsyncFd::new(pty.file().try_clone()?)?;
    let writer = AsyncFd::new(pty.file().try_clone()?)?;
    let child_exited = signal(SignalKind::child())?;
    let (commands, commands_received) = mpsc::unbounded_channel();
    let (events_sent, events) = mpsc::channel(EVENT_QUEUE);
    tokio::spawn(run(
        pty,
        reader,
        writer,
        child_exited,
        commands_received,
        events_sent,
    ));
    Ok(LocalSession {
        input: LocalInput { commands },
        events,
    })
}

async fn run(
    mut pty: Pty,
    reader: AsyncFd<File>,
    writer: AsyncFd<File>,
    mut child_exited: tokio::signal::unix::Signal,
    mut commands: mpsc::UnboundedReceiver<Command>,
    events: mpsc::Sender<LocalEvent>,
) {
    let mut buffer = vec![0; READ_BUFFER];
    // The slave closed (EIO): nothing more to read, the exit is on its way.
    let mut drained = false;
    // When a hung-up shell that has not exited gets killed.
    let mut kill_at: Option<Instant> = None;
    let status = loop {
        // A SIGCHLD may have come before the signal stream was listening.
        if let Some(ChildEvent::Exited(status)) = pty.next_child_event() {
            break Some(status);
        }
        tokio::select! {
            ready = reader.readable(), if !drained => {
                let Ok(mut guard) = ready else { drained = true; continue };
                match guard.try_io(|file| file.get_ref().read(&mut buffer)) {
                    Ok(Ok(0)) => drained = true,
                    Ok(Ok(read)) => {
                        if events.send(LocalEvent::Output(buffer[..read].to_vec())).await.is_err() {
                            break None;
                        }
                    }
                    Ok(Err(error)) if error.raw_os_error() == Some(EIO) => drained = true,
                    Ok(Err(error)) if error.kind() == ErrorKind::Interrupted => {}
                    Ok(Err(_)) => drained = true,
                    // Would block: the readiness was stale.
                    Err(_) => {}
                }
            }
            _ = child_exited.recv() => {}
            () = sleep_until(kill_at), if kill_at.is_some() => {
                kill_at = None;
                signal_group(&pty, Signal::KILL);
            }
            command = commands.recv() => match command {
                Some(Command::Write(bytes)) => {
                    if write_all(&writer, &bytes).await.is_err() {
                        drained = true;
                    }
                }
                Some(Command::Resize { columns, rows }) => pty.on_resize(window_size(columns, rows)),
                // Hung up as a terminal closing would; the exit is reported as any other.
                Some(Command::Close) if kill_at.is_none() => {
                    signal_group(&pty, Signal::HUP);
                    kill_at = Some(Instant::now() + HANG_UP_GRACE);
                }
                Some(Command::Close) => {}
                None => break None,
            },
        }
    };
    let code = if let Some(status) = status {
        // What the shell wrote before it exited comes first.
        drain(&reader, &mut buffer, &events).await;
        status.and_then(|status| status.code())
    } else {
        // Nobody will read what the shell does next: it goes now.
        signal_group(&pty, Signal::KILL);
        None
    };
    let _ = events.send(LocalEvent::Exited(code)).await;
    // Dropping the PTY hangs the shell up and waits for it: never on a runtime thread.
    tokio::task::spawn_blocking(move || drop(pty));
}

/// Sends `signal` to the shell's process group: it leads its own session, so its foreground
/// jobs go with it.
fn signal_group(pty: &Pty, signal: Signal) {
    let id = pty.child().id();
    if let Some(pid) = i32::try_from(id).ok().and_then(Pid::from_raw) {
        let _ = kill_process_group(pid, signal);
    }
}

/// Waits until `deadline`; the caller polls it only when there is one.
async fn sleep_until(deadline: Option<Instant>) {
    if let Some(deadline) = deadline {
        tokio::time::sleep_until(deadline).await;
    }
}

/// Reads what is left in the master without waiting for more.
async fn drain(reader: &AsyncFd<File>, buffer: &mut [u8], events: &mpsc::Sender<LocalEvent>) {
    loop {
        match reader.get_ref().read(buffer) {
            Ok(read) if read > 0 => {
                if events
                    .send(LocalEvent::Output(buffer[..read].to_vec()))
                    .await
                    .is_err()
                {
                    return;
                }
            }
            _ => return,
        }
    }
}

async fn write_all(writer: &AsyncFd<File>, mut bytes: &[u8]) -> io::Result<()> {
    while !bytes.is_empty() {
        let mut guard = writer.writable().await?;
        match guard.try_io(|file| file.get_ref().write(bytes)) {
            Ok(Ok(0)) => return Err(ErrorKind::WriteZero.into()),
            Ok(Ok(written)) => bytes = &bytes[written..],
            Ok(Err(error)) if error.kind() == ErrorKind::Interrupted => {}
            Ok(Err(error)) => return Err(error),
            Err(_) => {}
        }
    }
    Ok(())
}
