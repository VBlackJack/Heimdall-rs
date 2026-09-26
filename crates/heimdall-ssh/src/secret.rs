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

//! A password, passphrase or one-time code typed by the user.
//!
//! What this type guarantees: its text never appears in `Debug` output, and its own buffer
//! is zeroed when dropped. What it does not: russh and ssh-key make their own copies while
//! authenticating (the outgoing packet buffer, keyboard-interactive answers, the `PuTTY`
//! passphrase), and the UI field the user typed into holds another. Those copies are outside
//! this crate's reach.

use std::fmt;

use zeroize::Zeroizing;

/// Shown in place of the text in `Debug` output.
const REDACTED: &str = "<redacted>";

/// Text typed by the user that must not be logged or kept.
#[derive(Clone)]
pub struct Secret(Zeroizing<String>);

impl Secret {
    /// Takes ownership of `text`, without copying it.
    #[must_use]
    pub fn new(text: String) -> Self {
        Self(Zeroizing::new(text))
    }

    /// The text, for the one call that needs it.
    #[must_use]
    pub fn expose(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for Secret {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("Secret").field(&REDACTED).finish()
    }
}

#[cfg(test)]
mod tests {
    use super::Secret;

    #[test]
    fn debug_output_never_contains_the_text() {
        let secret = Secret::new("hunter2".to_owned());
        let shown = format!("{secret:?} {:?}", vec![secret.clone()]);
        assert!(!shown.contains("hunter2"), "{shown}");
        assert_eq!(secret.expose(), "hunter2");
    }
}
