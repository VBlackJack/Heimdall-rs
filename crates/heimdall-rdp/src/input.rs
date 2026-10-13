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

//! What a session is sent of the keyboard and the mouse: `IronRDP`'s operations, and the
//! Pause key, which no scancode of its own says.
//!
//! MS-RDPBCGR 2.2.8.1.1.3.1.1.1 sends a press of Pause as Ctrl down, Num Lock down, Ctrl
//! up, Num Lock up, both Ctrl events flagged `KBDFLAGS_EXTENDED1`, the `E1` prefix of the
//! key's own scancode: what the Windows client sends, whose `ActiveX` control the C# Heimdall
//! embeds. `IronRDP`'s input database knows the `E0` prefix alone, so the sequence is
//! written here, outside it: no key it tracks is left held.

use ironrdp::input::{Database, Operation};
use ironrdp::pdu::input::fast_path::{FastPathInputEvent, KeyboardFlags};

/// Scancode of the Ctrl key, which the Pause sequence sends with `KBDFLAGS_EXTENDED1`.
const CONTROL: u8 = 0x1D;
/// Scancode of the Num Lock key, which the Pause sequence sends after Ctrl.
const NUM_LOCK: u8 = 0x45;

/// Something typed or pointed at, sent to the server in order.
#[derive(Debug, Clone)]
pub enum RdpInput {
    /// A key, a button, a move or the wheel, as `IronRDP` encodes it.
    Operation(Operation),
    /// The Pause key, pressed and released: the server is sent its whole sequence.
    Pause,
}

impl From<Operation> for RdpInput {
    fn from(operation: Operation) -> Self {
        Self::Operation(operation)
    }
}

/// `inputs` as the fast-path events the server is sent, in their order; `keys` keeps what
/// is held between batches.
#[must_use]
pub fn fast_path_events(keys: &mut Database, inputs: Vec<RdpInput>) -> Vec<FastPathInputEvent> {
    let mut events = Vec::new();
    let mut pending = Vec::new();
    for input in inputs {
        match input {
            RdpInput::Operation(operation) => pending.push(operation),
            RdpInput::Pause => {
                events.extend(keys.apply(pending.drain(..)));
                events.extend(pause_events());
            }
        }
    }
    events.extend(keys.apply(pending));
    events
}

/// The Pause key's press and release, as MS-RDPBCGR 2.2.8.1.1.3.1.1.1 writes them.
fn pause_events() -> [FastPathInputEvent; 4] {
    [
        FastPathInputEvent::KeyboardEvent(KeyboardFlags::EXTENDED1, CONTROL),
        FastPathInputEvent::KeyboardEvent(KeyboardFlags::empty(), NUM_LOCK),
        FastPathInputEvent::KeyboardEvent(
            KeyboardFlags::EXTENDED1 | KeyboardFlags::RELEASE,
            CONTROL,
        ),
        FastPathInputEvent::KeyboardEvent(KeyboardFlags::RELEASE, NUM_LOCK),
    ]
}

#[cfg(test)]
mod tests {
    use ironrdp::input::Scancode;

    use super::*;

    const LETTER_A: u8 = 0x1E;

    #[test]
    fn pause_is_sent_as_the_specification_writes_it() {
        let mut keys = Database::new();
        let events = fast_path_events(&mut keys, vec![RdpInput::Pause]);
        assert_eq!(
            events,
            vec![
                FastPathInputEvent::KeyboardEvent(KeyboardFlags::EXTENDED1, 0x1D),
                FastPathInputEvent::KeyboardEvent(KeyboardFlags::empty(), 0x45),
                FastPathInputEvent::KeyboardEvent(
                    KeyboardFlags::EXTENDED1 | KeyboardFlags::RELEASE,
                    0x1D
                ),
                FastPathInputEvent::KeyboardEvent(KeyboardFlags::RELEASE, 0x45),
            ]
        );
    }

    #[test]
    fn pause_keeps_its_place_among_the_keys_and_holds_none() {
        let mut keys = Database::new();
        let letter = Scancode::from_u8(false, LETTER_A);
        let events = fast_path_events(
            &mut keys,
            vec![
                Operation::KeyPressed(letter).into(),
                RdpInput::Pause,
                Operation::KeyReleased(letter).into(),
            ],
        );
        assert_eq!(events.len(), 6);
        assert_eq!(
            events[0],
            FastPathInputEvent::KeyboardEvent(KeyboardFlags::empty(), LETTER_A)
        );
        assert_eq!(
            events[1],
            FastPathInputEvent::KeyboardEvent(KeyboardFlags::EXTENDED1, CONTROL)
        );
        assert_eq!(
            events[5],
            FastPathInputEvent::KeyboardEvent(KeyboardFlags::RELEASE, LETTER_A)
        );
        // Nothing of the sequence is tracked as held: releasing everything sends nothing.
        assert!(keys.release_all().is_empty());
    }
}
