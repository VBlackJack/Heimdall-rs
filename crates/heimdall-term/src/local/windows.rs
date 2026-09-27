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
use alacritty_terminal::tty::windows::{PTY_CHILD_EVENT_TOKEN, PTY_READ_WRITE_TOKEN};
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
    let pty = tty::new(options, size, 0)?;
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
        for event in ready.iter() {
            if event.key == PTY_CHILD_EVENT_TOKEN
                && let Some(ChildEvent::Exited(status)) = pty.next_child_event()
            {
                // What the shell wrote before it exited comes first.
                read_available(&mut pty, &mut buffer, events);
                break 'session status.and_then(|status| status.code());
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
