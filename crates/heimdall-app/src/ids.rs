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

//! Identifiers that are never reused, so a late message can always be told apart from a
//! current one.

use std::sync::atomic::{AtomicU64, Ordering};

macro_rules! identifier {
    ($(#[$doc:meta])* $name:ident) => {
        $(#[$doc])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
        pub struct $name(u64);

        impl $name {
            /// The raw value, for logs.
            #[must_use]
            pub fn value(self) -> u64 {
                self.0
            }
        }
    };
}

identifier!(
    /// A tab. A closed tab's identifier is never given to another.
    TabId
);
identifier!(
    /// One connection attempt of a tab; a reconnection gets a new one.
    AttemptId
);
identifier!(
    /// One question put to the user.
    QuestionId
);
identifier!(
    /// One file opened in a Files tab's integrated editor: kept when the tab reconnects.
    EditorId
);

static NEXT: AtomicU64 = AtomicU64::new(1);

fn next() -> u64 {
    NEXT.fetch_add(1, Ordering::Relaxed)
}

impl TabId {
    pub(crate) fn fresh() -> Self {
        Self(next())
    }
}

impl AttemptId {
    pub(crate) fn fresh() -> Self {
        Self(next())
    }
}

impl EditorId {
    pub(crate) fn fresh() -> Self {
        Self(next())
    }
}

impl QuestionId {
    /// A new identifier. Public because an identifier grants nothing: only a question
    /// actually waiting in the registry can be answered.
    #[must_use]
    pub fn fresh() -> Self {
        Self(next())
    }
}
