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

//! Every way the application ends by itself says so in the log: a log ending without it
//! tells of a process ended from outside, or of a crash the system reports.

use std::cell::RefCell;
use std::path::Path;
use std::sync::Once;

use heimdall_app::{App, AppConfig, Message as AppMessage, SystemCredentials};
use heimdall_ssh::AgentSource;
use heimdall_term::GridSize;
use heimdall_ui::shell::{Message, Shell};
use log::{LevelFilter, Log, Metadata, Record};

thread_local! {
    /// The lines this thread logged, as `LEVEL message`.
    static LINES: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
}

/// Keeps each line in the logging thread's [`LINES`], so tests side by side do not see
/// each other's.
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
fn start() {
    static INSTALLED: Once = Once::new();
    INSTALLED.call_once(|| {
        let _ = log::set_logger(&CAPTURE);
        log::set_max_level(LevelFilter::Info);
    });
    LINES.with(|lines| lines.borrow_mut().clear());
}

/// Whether a line this thread logged at `level` holds `part`.
fn logged(level: &str, part: &str) -> bool {
    LINES.with(|lines| {
        lines
            .borrow()
            .iter()
            .any(|line| line.starts_with(level) && line.contains(part))
    })
}

/// An application with no session open.
fn shell(dir: &Path) -> Shell {
    Shell::with_app(App::new(AppConfig {
        profiles_file: dir.join("profiles.toml"),
        known_hosts: dir.join("known_hosts"),
        legacy_dir: None,
        agent: AgentSource::Disabled,
        initial_grid: GridSize { cols: 80, rows: 24 },
        files_start: dir.to_owned(),
        system_credentials: SystemCredentials::memory(),
    }))
}

#[test]
fn quitting_as_asked_is_logged() {
    start();
    let dir = tempfile::tempdir().expect("dir");
    let mut shell = shell(dir.path());
    // Nothing open, nothing to lose: the window's close quits at once.
    let _ = shell.update(Message::App(AppMessage::WindowCloseRequested));
    assert!(shell.app().dialog.is_none(), "nothing asked");
    assert!(logged("INFO", "Heimdall quits as asked"));
}

#[test]
fn the_main_window_closing_is_logged() {
    start();
    let dir = tempfile::tempdir().expect("dir");
    let mut shell = shell(dir.path());
    let _ = shell.update(Message::MainWindowClosed);
    assert!(logged("INFO", "the main window closed: Heimdall stops"));
}
