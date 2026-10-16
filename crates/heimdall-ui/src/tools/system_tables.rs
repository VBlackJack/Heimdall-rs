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

//! What the Open Ports, Network Interfaces, Route Table and Wi-Fi Networks views share:
//! a failure to read the system's table said in the user's language, and the local time a
//! table was read at.

use heimdall_app::server_text;
use heimdall_core::tools::system_listing::ListingError;

use crate::i18n::fl;

/// How the time a table was read at is written, as the C# `HH:mm:ss`.
const READ_TIME_FORMAT: &str = "%H:%M:%S";

/// The local time now, as the status line says when a table was read: the C#
/// `DateTime.Now`, not the UTC time of day.
pub(super) fn read_time_now() -> String {
    chrono::Local::now().format(READ_TIME_FORMAT).to_string()
}

/// `error` in words of the user's language, a program's own words made safe to show.
pub(super) fn listing_error_text(error: &ListingError) -> String {
    match error {
        ListingError::NotAvailable => fl!("ui-tool-systable-error-not-available"),
        ListingError::NotStarted(detail) => fl!(
            "ui-tool-systable-error-not-started",
            error = server_text(detail)
        ),
        ListingError::TimedOut => fl!("ui-tool-systable-error-timeout"),
        ListingError::Failed { code, detail } if detail.is_empty() => fl!(
            "ui-tool-systable-error-failed",
            code = code.map_or_else(
                || fl!("ui-tool-systable-exit-code-none"),
                |code| code.to_string()
            )
        ),
        ListingError::Failed { detail, .. } => fl!(
            "ui-tool-systable-error-failed-detail",
            error = server_text(detail)
        ),
        ListingError::Unreadable(detail) => fl!(
            "ui-tool-systable-error-unreadable",
            error = server_text(detail)
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_failure_has_its_own_text_and_a_programs_words_are_made_safe() {
        let errors = [
            ListingError::NotAvailable,
            ListingError::NotStarted("x".to_owned()),
            ListingError::TimedOut,
            ListingError::Failed {
                code: Some(1),
                detail: String::new(),
            },
            ListingError::Failed {
                code: None,
                detail: String::new(),
            },
            ListingError::Failed {
                code: Some(1),
                detail: "The Wireless AutoConfig Service (wlansvc) is not running.".to_owned(),
            },
            ListingError::Unreadable("x".to_owned()),
        ];
        let texts: std::collections::HashSet<String> =
            errors.iter().map(listing_error_text).collect();
        assert_eq!(texts.len(), errors.len());
        assert!(texts.iter().all(|text| !text.contains("ui-tool-")));
        assert!(
            texts.iter().any(|text| text.contains("wlansvc")),
            "netsh's message is shown: {texts:?}"
        );

        let hostile = ListingError::Failed {
            code: Some(1),
            detail: "evil\u{202E}txt.exe\u{1b}[2J".to_owned(),
        };
        let text = listing_error_text(&hostile);
        assert!(
            !text.contains('\u{202E}') && !text.contains('\u{1b}'),
            "{text}"
        );
    }

    #[test]
    fn the_read_time_is_a_clock_time() {
        let time = read_time_now();
        assert_eq!(time.len(), "00:00:00".len());
        assert_eq!(time.matches(':').count(), 2);
    }
}
