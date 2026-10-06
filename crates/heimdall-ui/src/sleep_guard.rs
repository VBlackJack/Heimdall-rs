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

//! The computer kept from sleeping while a session is open, as the C# `SleepPrevention`:
//! the system only, the display left to turn off. Windows ties the request to the thread
//! that made it, so a thread of its own makes it and lets it go.

/// Whether the computer is kept awake, and the thread that keeps it so.
pub struct SleepGuard {
    /// What the keeping thread is told: awake or not. None where there is no such request.
    #[cfg(windows)]
    sender: std::sync::mpsc::Sender<bool>,
    /// What was asked last.
    awake: bool,
}

impl SleepGuard {
    /// Not keeping the computer awake yet.
    #[must_use]
    pub fn new() -> Self {
        Self {
            #[cfg(windows)]
            sender: keeper(),
            awake: false,
        }
    }

    /// Keeps the computer awake, or no longer; nothing is asked again for the same answer.
    pub fn hold(&mut self, awake: bool) {
        if awake == self.awake {
            return;
        }
        self.awake = awake;
        #[cfg(windows)]
        if self.sender.send(awake).is_err() {
            log::warn!("the computer could not be kept awake: its thread is gone");
        }
    }

    /// Whether the computer is asked to stay awake.
    #[must_use]
    pub fn held(&self) -> bool {
        self.awake
    }
}

impl Default for SleepGuard {
    fn default() -> Self {
        Self::new()
    }
}

/// The thread holding the request while it is wanted, told by the sender it returns.
#[cfg(windows)]
fn keeper() -> std::sync::mpsc::Sender<bool> {
    let (sender, receiver) = std::sync::mpsc::channel::<bool>();
    let spawned = std::thread::Builder::new()
        .name("sleep-guard".to_owned())
        .spawn(move || {
            let mut held = None;
            for awake in receiver {
                // Dropped, the request is let go on this same thread.
                held = None;
                if awake {
                    held = keepawake::Builder::default()
                        .idle(true)
                        .create()
                        .inspect_err(|error| {
                            log::warn!("the computer could not be kept awake: {error}");
                        })
                        .ok();
                }
            }
            drop(held);
        });
    if let Err(error) = spawned {
        log::warn!("the computer cannot be kept awake: {error}");
    }
    sender
}
