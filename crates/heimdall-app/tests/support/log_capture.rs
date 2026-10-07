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

//! What a test logs, kept for it to read: each test thread keeps its own lines, so tests
//! running side by side do not see each other's.

#![allow(dead_code, reason = "each test binary uses the part it needs")]

use std::cell::RefCell;
use std::sync::Once;

use log::{LevelFilter, Log, Metadata, Record};

thread_local! {
    /// The lines this thread logged, as `LEVEL message`.
    static LINES: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
}

/// Keeps each line in the logging thread's [`LINES`].
struct Capture;

impl Log for Capture {
    fn enabled(&self, _metadata: &Metadata<'_>) -> bool {
        true
    }

    fn log(&self, record: &Record<'_>) {
        LINES.with(|lines| {
            lines
                .borrow_mut()
                .push(format!("{} {}", record.level(), record.args()));
        });
    }

    fn flush(&self) {}
}

static CAPTURE: Capture = Capture;

/// Starts keeping the lines, once for the test binary, and forgets this thread's so far.
pub fn start() {
    static INSTALLED: Once = Once::new();
    INSTALLED.call_once(|| {
        let _ = log::set_logger(&CAPTURE);
        log::set_max_level(LevelFilter::Info);
    });
    LINES.with(|lines| lines.borrow_mut().clear());
}

/// The lines this thread logged since [`start`].
pub fn lines() -> Vec<String> {
    LINES.with(|lines| lines.borrow().clone())
}

/// Whether a line this thread logged at `level` (`INFO`, `WARN`) holds every one of
/// `parts`.
pub fn has(level: &str, parts: &[&str]) -> bool {
    lines()
        .iter()
        .any(|line| line.starts_with(level) && parts.iter().all(|part| line.contains(part)))
}

/// Whether no line this thread logged holds `secret`.
pub fn never(secret: &str) -> bool {
    lines().iter().all(|line| !line.contains(secret))
}
