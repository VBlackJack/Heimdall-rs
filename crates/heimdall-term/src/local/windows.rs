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

//! Windows: `ConPTY`, whose pipes and child watcher post to a poller that one thread waits
//! on. Commands wake it through the poller.

use std::io::{self, Read as _, Write as _};
use std::sync::Arc;
use std::sync::mpsc as std_mpsc;

use alacritty_terminal::event::{OnResize as _, WindowSize};
use alacritty_terminal::tty::windows::PTY_READ_WRITE_TOKEN;
use alacritty_terminal::tty::{
    self, ChildEvent, EventedPty as _, EventedReadWrite as _, Options, Pty,
};
use polling::{Event, Events, PollMode, Poller};
use tokio::sync::mpsc;

use super::{Command, EVENT_QUEUE, LocalEvent, LocalInput, LocalSession, READ_BUFFER, window_size};

/// The command queue of a session, and the poller to wake its thread with.
#[derive(Clone)]
pub(super) struct Commands {
    sender: std_mpsc::Sender<Command>,
    poller: Arc<Poller>,
}

impl Commands {
    pub(super) fn send(&self, command: Command) -> Result<(), std_mpsc::SendError<Command>> {
        self.sender.send(command)?;
        let _ = self.poller.notify();
        Ok(())
    }
}

pub(super) fn spawn(options: &Options, size: WindowSize) -> io::Result<LocalSession> {
    start(tty::new(options, size, 0)?)
}

/// Runs the session of a shell already started: it may have exited by now.
fn start(pty: Pty) -> io::Result<LocalSession> {
    let poller = Arc::new(Poller::new()?);
    let (sender, commands) = std_mpsc::channel();
    let (events_sent, events) = mpsc::channel(EVENT_QUEUE);
    let thread_poller = Arc::clone(&poller);
    std::thread::Builder::new()
        .name("heimdall-local-shell".to_owned())
        .spawn(move || run(pty, &thread_poller, &commands, &events_sent))?;
    Ok(LocalSession {
        input: LocalInput {
            commands: Commands { sender, poller },
        },
        events,
    })
}

fn run(
    mut pty: Pty,
    poller: &Arc<Poller>,
    commands: &std_mpsc::Receiver<Command>,
    events: &mpsc::Sender<LocalEvent>,
) {
    let mut buffer = vec![0; READ_BUFFER];
    let mut pending: Vec<u8> = Vec::new();
    let mut ready = Events::new();
    let code = 'session: loop {
        // Readable always; writable while bytes wait for room in the pipe.
        let interest = if pending.is_empty() {
            Event::readable(PTY_READ_WRITE_TOKEN)
        } else {
            Event::all(PTY_READ_WRITE_TOKEN)
        };
        if pty.reregister(poller, interest, PollMode::Level).is_err() {
            break None;
        }
        // Looked for here, not in what woke the poller: the child watcher wakes it only once
        // registered, and a shell that exits before the first registration never does. Once
        // registered, an exit after this look wakes the wait below, and the next turn sees it.
        if let Some(ChildEvent::Exited(status)) = pty.next_child_event() {
            // What the shell wrote before it exited comes first.
            read_available(&mut pty, &mut buffer, events);
            break status.and_then(|status| status.code());
        }
        ready.clear();
        if poller.wait(&mut ready, None).is_err() {
            break None;
        }
        while let Ok(command) = commands.try_recv() {
            match command {
                Command::Write(bytes) => pending.extend_from_slice(&bytes),
                Command::Resize { columns, rows } => pty.on_resize(window_size(columns, rows)),
                Command::Close => break 'session None,
            }
        }
        if !read_available(&mut pty, &mut buffer, events) {
            break None;
        }
        if !pending.is_empty() {
            match pty.writer().write(&pending) {
                // Zero: the pipe is full; the rest waits for room.
                Ok(written) => {
                    pending.drain(..written);
                }
                Err(_) => break None,
            }
        }
    };
    let _ = events.blocking_send(LocalEvent::Exited(code));
    let _ = pty.deregister(poller);
    drop(pty);
}

/// Reads all the output waiting; `false` once nobody reads the events any more.
fn read_available(pty: &mut Pty, buffer: &mut [u8], events: &mpsc::Sender<LocalEvent>) -> bool {
    loop {
        // `Ok(0)`: nothing waiting now.
        match pty.reader().read(buffer) {
            Ok(read) if read > 0 => {
                if events
                    .blocking_send(LocalEvent::Output(buffer[..read].to_vec()))
                    .is_err()
                {
                    return false;
                }
            }
            _ => return true,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, Instant};

    use super::super::{LocalArguments, LocalConfig, options, window_size};
    use super::*;

    /// Bound on anything the test waits for.
    const WAIT: Duration = Duration::from_secs(15);

    /// Whether a process of that id still runs, as `tasklist` sees it.
    fn running(pid: u32) -> bool {
        let listed = std::process::Command::new("tasklist")
            .args(["/FI", &format!("PID eq {pid}"), "/FO", "CSV", "/NH"])
            .output()
            .expect("tasklist");
        String::from_utf8_lossy(&listed.stdout).contains(&format!("\"{pid}\""))
    }

    /// What a loaded machine does at random, made certain: the shell has exited, as the
    /// system sees it, before the session thread first registers with the poller.
    #[tokio::test]
    async fn a_shell_that_exits_before_its_session_starts_is_still_reported() {
        let options = options(&LocalConfig {
            program: Some("cmd.exe".to_owned()),
            arguments: LocalArguments::List(vec!["/C".to_owned(), "exit 7".to_owned()]),
            ..LocalConfig::default()
        })
        .expect("options");
        let pty = tty::new(&options, window_size(80, 24), 0).expect("started");
        let pid = pty.child_watcher().pid().expect("pid").get();
        let started = Instant::now();
        while running(pid) {
            assert!(started.elapsed() < WAIT, "the shell did not exit");
            std::thread::sleep(Duration::from_millis(50));
        }
        let mut session = start(pty).expect("session");
        let code = loop {
            match tokio::time::timeout(WAIT, session.events.recv())
                .await
                .expect("in time")
                .expect("an event")
            {
                LocalEvent::Output(_) => {}
                LocalEvent::Exited(code) => break code,
            }
        };
        assert_eq!(code, Some(7));
    }
}
