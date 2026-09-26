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

//! Where a tab's input goes. The SSH session in production, a recorder in tests.

use std::fmt::Debug;

use heimdall_ssh::{SessionClosed, SessionInput, TerminalSize};

/// Input side of a session. Every call only queues: none may wait on the network.
pub trait InputSink: Send + Sync + Debug {
    /// Sends bytes to the remote shell.
    ///
    /// # Errors
    ///
    /// [`SessionClosed`] once the session has ended.
    fn write(&self, bytes: Vec<u8>) -> Result<(), SessionClosed>;

    /// Reports a new terminal size.
    ///
    /// # Errors
    ///
    /// [`SessionClosed`] once the session has ended.
    fn resize(&self, size: TerminalSize) -> Result<(), SessionClosed>;

    /// Ends the session.
    fn close(&self);
}

impl InputSink for SessionInput {
    fn write(&self, bytes: Vec<u8>) -> Result<(), SessionClosed> {
        SessionInput::write(self, bytes)
    }

    fn resize(&self, size: TerminalSize) -> Result<(), SessionClosed> {
        SessionInput::resize(self, size)
    }

    fn close(&self) {
        SessionInput::close(self);
    }
}
