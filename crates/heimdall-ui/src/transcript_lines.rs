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

//! A transcript's first and last lines in the window's language, as the C# session log
//! words them: when (UTC), which protocol, host and session; when it ended, how long it
//! lasted.

use std::sync::Arc;
use std::time::{Duration, SystemTime};

use heimdall_app::transcript::{TranscriptContext, TranscriptLines};
use heimdall_core::utc::UtcTime;

use crate::i18n::fl;

/// Seconds in a day, an hour, a minute.
const DAY: u64 = 86_400;
const HOUR: u64 = 3_600;
const MINUTE: u64 = 60;

/// The lines, worded now and at each transcript's start and end.
#[must_use]
pub fn lines() -> TranscriptLines {
    TranscriptLines {
        header: Arc::new(header),
        footer: Arc::new(footer),
    }
}

fn header(context: &TranscriptContext) -> String {
    fl!(
        "ui-transcript-header",
        started = UtcTime::of(context.started).iso(),
        protocol = context.protocol.as_str(),
        host = context.host.as_str(),
        title = context.title.as_str()
    )
}

fn footer(ended: SystemTime, lasted: Duration) -> String {
    fl!(
        "ui-transcript-footer",
        ended = UtcTime::of(ended).iso(),
        duration = duration(lasted)
    )
}

/// `lasted` as `01:02:03`, or `2.01:02:03` past a day, the C# constant format to the second.
fn duration(lasted: Duration) -> String {
    let seconds = lasted.as_secs();
    let clock = format!(
        "{:02}:{:02}:{:02}",
        seconds % DAY / HOUR,
        seconds % HOUR / MINUTE,
        seconds % MINUTE
    );
    match seconds / DAY {
        0 => clock,
        days => format!("{days}.{clock}"),
    }
}

#[cfg(test)]
mod tests {
    use std::time::UNIX_EPOCH;

    use super::*;

    #[test]
    fn the_lines_are_the_csharp_ones() {
        let context = TranscriptContext {
            protocol: "SSH".to_owned(),
            host: "web.lab".to_owned(),
            title: "Web".to_owned(),
            started: UNIX_EPOCH + Duration::from_secs(1_790_536_503),
        };
        let lines = lines();
        assert_eq!(
            (lines.header)(&context),
            "===== Session started 2026-09-27T19:15:03Z | SSH | host web.lab | Web ====="
        );
        assert_eq!(
            (lines.footer)(
                UNIX_EPOCH + Duration::from_secs(1_790_540_106),
                Duration::from_secs(3_603)
            ),
            "===== Session ended 2026-09-27T20:15:06Z | duration 01:00:03 ====="
        );
    }

    /// The retention removes only the transcripts opening with the marker: a translation
    /// whose header did not would keep its transcripts forever.
    #[test]
    fn every_translation_of_the_header_opens_with_the_marker_the_retention_reads() {
        let crate_root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        let key = "ui-transcript-header = ";
        for language in heimdall_i18n::SUPPORTED_LANGUAGES {
            let path = heimdall_i18n::ftl_path(crate_root, language, env!("CARGO_PKG_NAME"));
            let source = std::fs::read_to_string(&path).expect("readable");
            let header = source
                .lines()
                .find_map(|line| line.strip_prefix(key))
                .expect("a header");
            assert!(
                header.starts_with(heimdall_app::transcript::HEADER_MARKER),
                "{language}: {header}"
            );
        }
    }

    #[test]
    fn a_duration_is_written_as_the_csharp_constant_format() {
        assert_eq!(duration(Duration::ZERO), "00:00:00");
        assert_eq!(duration(Duration::from_millis(59_999)), "00:00:59");
        assert_eq!(duration(Duration::from_secs(DAY - 1)), "23:59:59");
        assert_eq!(duration(Duration::from_secs(2 * DAY + 3_661)), "2.01:01:01");
    }
}
