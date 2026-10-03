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

//! The profile form's "Test address", as the C# dialog's: the address and port the form
//! holds dialled from this computer, one test at a time, its finding shown until the address
//! changes. Nothing is saved, and no credential is checked.

use tokio_util::sync::CancellationToken;

use super::{App, Dialog, Effect, Message};
use crate::profile_draft::AddressTest;

impl App {
    /// Applies a message about the address test.
    pub(super) fn address_test_message(&mut self, message: Message) -> Vec<Effect> {
        match message {
            Message::TestAddress => self.test_address(),
            Message::CancelAddressTest => {
                if let Some((_, cancel)) = &self.address_test {
                    cancel.cancel();
                }
                Vec::new()
            }
            Message::AddressTested { test, result } => {
                // Only the last test's finding, and only while its form is open.
                if self.address_test.as_ref().map(|(running, _)| *running) != Some(test) {
                    return Vec::new();
                }
                self.address_test = None;
                if let Some(Dialog::EditProfile { draft, .. }) = &mut self.dialog
                    && draft.address_test == AddressTest::Running
                {
                    draft.address_test = AddressTest::Done(result);
                }
                Vec::new()
            }
            _ => Vec::new(),
        }
    }

    /// Starts a test of the form's address, when it has one and none is running.
    fn test_address(&mut self) -> Vec<Effect> {
        let Some(Dialog::EditProfile { draft, .. }) = &mut self.dialog else {
            return Vec::new();
        };
        if draft.address_test == AddressTest::Running {
            return Vec::new();
        }
        let Some((host, port)) = draft.test_target() else {
            return Vec::new();
        };
        let ssh = draft.tests_ssh();
        draft.address_test = AddressTest::Running;
        // A test still running for an address left behind is stopped: one at a time.
        if let Some((_, cancel)) = self.address_test.take() {
            cancel.cancel();
        }
        let test = self.next_address_test;
        self.next_address_test += 1;
        let cancel = CancellationToken::new();
        self.address_test = Some((test, cancel.clone()));
        vec![Effect::TestAddress {
            test,
            host,
            port,
            ssh,
            cancel,
        }]
    }
}
