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

//! The import report of a shared source as text to copy and as a CSV file to write, as the C#
//! `SharedSessionImportReportFormatter.ToPlainText` and `ToCsv` and the report dialog's "Copy"
//! and "Export CSV..." (`SharedSessionImportReportDialogViewModel`).
//!
//! The names, folders and hosts of a report come from a file other people edit, so every CSV
//! cell is neutralised against formula injection before it is quoted
//! (`InputValidator.SanitizeCsvCell`, as `heimdall_core::tools::csv_cell` writes it for the
//! other CSV exports).

use std::path::Path;

use heimdall_app::shared_report::{ImportReport, ReportEntry};
use heimdall_core::tools::csv_cell;
use iced::{Task, window};

use crate::file_dialog::FileDialog;
use crate::i18n::fl;
use crate::shared_report_view::{check_label, checks, entry_text};
use crate::shell::Message;

/// Ends a CSV record, as the C# `CsvLineEnd` (RFC 4180).
const CSV_LINE_END: &str = "\r\n";

/// Separates the cells of a CSV record.
const CSV_SEPARATOR: &str = ",";

/// Ends a line of the plain text.
const TEXT_LINE_END: &str = "\n";

/// The start of a file's name: the C# `CsvFileNamePrefix`.
const CSV_FILE_NAME_PREFIX: &str = "import-report-";

/// The extension of the file, without its dot: the C# `CsvExtension`.
const CSV_EXTENSION: &str = "csv";

/// The byte order mark of UTF-8, written first so that a spreadsheet opens accented names as
/// UTF-8, as the C# `UTF8Encoding(encoderShouldEmitUTF8Identifier: true)`.
const UTF8_BYTE_ORDER_MARK: &str = "\u{feff}";

/// The folder of an entry as the report shows it; the top of the file has a name of its own.
fn folder_label(folder: &str) -> String {
    if folder.trim().is_empty() {
        fl!("ui-shared-report-top-folder")
    } else {
        folder.to_owned()
    }
}

/// What `entry` found, in one sentence.
fn explain(entry: &ReportEntry) -> String {
    entry_text(entry)
}

/// "3 findings": the total of a report, worded by its number.
fn total_text(report: &ImportReport) -> String {
    fl!("ui-shared-report-total", count = report.entries.len())
}

/// What replaces a control or bidirectional control character in the plain text.
const CONTROL_REPLACEMENT: char = '?';

/// Bidirectional controls, which `char::is_control` does not cover: they reorder what is read.
const BIDI_CONTROLS: [char; 12] = [
    '\u{061c}', '\u{200e}', '\u{200f}', '\u{202a}', '\u{202b}', '\u{202c}', '\u{202d}', '\u{202e}',
    '\u{2066}', '\u{2067}', '\u{2068}', '\u{2069}',
];

/// `text` of the file made safe to paste: each control character (a tab or a line break would
/// start a new spreadsheet cell or row, so a name could put a formula in a cell of its own)
/// and each bidirectional control is replaced by a visible mark.
fn plain(text: &str) -> String {
    text.chars()
        .map(|c| {
            if c.is_control() || BIDI_CONTROLS.contains(&c) {
                CONTROL_REPLACEMENT
            } else {
                c
            }
        })
        .collect()
}

/// `entry` with every field that comes from the file made safe to paste.
fn cleaned(entry: &ReportEntry) -> ReportEntry {
    ReportEntry {
        folder: plain(&entry.folder),
        session: plain(&entry.session),
        detail: plain(&entry.detail),
        context: plain(&entry.context),
        ..entry.clone()
    }
}

/// The report as plain text: the title, the counts per check, then every entry by check, as
/// the C# `ToPlainText`. `source` is the name of the source the report is about. Every text of
/// the file is cleaned of control characters, since the text is pasted into other programs.
#[must_use]
pub fn plain_text(source: &str, report: &ImportReport) -> String {
    let mut lines = vec![
        fl!("ui-shared-report-title", name = plain(source)),
        total_text(report),
    ];
    let summary = |check| {
        fl!(
            "ui-shared-report-summary-line",
            check = check_label(check),
            count = report.count_of(check)
        )
    };
    lines.extend(checks().into_iter().map(summary));
    for check in checks() {
        let entries: Vec<ReportEntry> = report
            .entries
            .iter()
            .filter(|entry| entry.check == check)
            .map(cleaned)
            .collect();
        if entries.is_empty() {
            continue;
        }
        lines.push(String::new());
        lines.push(summary(check));
        lines.extend(entries.iter().map(|entry| {
            fl!(
                "ui-shared-report-plain-entry",
                folder = folder_label(&entry.folder),
                session = entry.session.as_str(),
                explanation = explain(entry)
            )
        }));
    }
    let mut text = lines.join(TEXT_LINE_END);
    text.push_str(TEXT_LINE_END);
    text
}

/// One CSV record: each cell neutralised, quoted, and the cells joined.
fn record(cells: &[String]) -> String {
    let mut line = cells
        .iter()
        .map(|cell| csv_cell::quoted(cell))
        .collect::<Vec<_>>()
        .join(CSV_SEPARATOR);
    line.push_str(CSV_LINE_END);
    line
}

/// The report as CSV, as the C# `ToCsv`: one row per entry with its check, folder, session,
/// detail and explanation, under a header row in the language shown.
#[must_use]
pub fn csv(report: &ImportReport) -> String {
    let mut csv = record(&[
        fl!("ui-shared-report-column-check"),
        fl!("ui-shared-report-column-folder"),
        fl!("ui-shared-report-column-session"),
        fl!("ui-shared-report-column-detail"),
        fl!("ui-shared-report-column-explanation"),
    ]);
    for entry in &report.entries {
        csv.push_str(&record(&[
            check_label(entry.check),
            folder_label(&entry.folder),
            entry.session.clone(),
            entry.detail.clone(),
            explain(entry),
        ]));
    }
    csv
}

/// The name the save dialog proposes for the CSV of `source_id`'s report.
#[must_use]
pub fn file_name(source_id: &str) -> String {
    format!("{CSV_FILE_NAME_PREFIX}{source_id}.{CSV_EXTENSION}")
}

/// `document` as the bytes of the file: UTF-8 with its byte order mark.
fn file_bytes(document: &str) -> Vec<u8> {
    format!("{UTF8_BYTE_ORDER_MARK}{document}").into_bytes()
}

/// Writes `document` at `path`, with its byte order mark.
///
/// # Errors
///
/// The system's reason the file could not be written.
pub async fn write(path: &Path, document: &str) -> Result<(), String> {
    tokio::fs::write(path, file_bytes(document))
        .await
        .map_err(|error| error.to_string())
}

/// Asks where to save the CSV of the report of `source_id`, over the main window `main`, then
/// writes `document` there; nothing when no file is picked. The outcome is a
/// [`Message::SharedReportExported`], the path written or why not.
pub fn save(source_id: &str, document: String, main: Option<window::Id>) -> Task<Message> {
    let (title, filter, name) = (
        fl!("ui-shared-report-export-title"),
        fl!("ui-shared-report-csv-filter"),
        file_name(source_id),
    );
    crate::shell::main_window_task(main)
        .then(move |id| {
            let (title, filter, name) = (title.clone(), filter.clone(), name.clone());
            let dialog = move || {
                FileDialog::new()
                    .set_title(title)
                    .set_file_name(name)
                    .add_filter(filter, &[CSV_EXTENSION])
            };
            match id {
                Some(id) => window::run(id, move |window| dialog().set_parent(window).save_file()),
                None => Task::done(dialog().save_file()),
            }
        })
        .then(move |pick| {
            let document = document.clone();
            Task::future(pick).then(move |picked| match picked {
                Ok(Some(path)) => {
                    let document = document.clone();
                    Task::perform(
                        async move {
                            write(&path, &document)
                                .await
                                .map(|()| path.display().to_string())
                        },
                        Message::SharedReportExported,
                    )
                }
                Ok(None) => Task::none(),
                Err(unavailable) => {
                    Task::done(Message::SharedReportExported(Err(unavailable.message())))
                }
            })
        })
}

#[cfg(test)]
mod tests {
    use heimdall_app::shared_report::{ReportCheck, SkipReason};

    use super::*;

    fn entry(
        check: ReportCheck,
        folder: &str,
        session: &str,
        detail: &str,
        context: &str,
        reason: Option<SkipReason>,
    ) -> ReportEntry {
        ReportEntry {
            check,
            folder: folder.to_owned(),
            session: session.to_owned(),
            detail: detail.to_owned(),
            context: context.to_owned(),
            reason,
        }
    }

    fn report() -> ImportReport {
        ImportReport {
            entries: vec![
                entry(
                    ReportCheck::SkippedSession,
                    "Prod",
                    "odd box",
                    "12",
                    "",
                    Some(SkipReason::OtherProtocol),
                ),
                entry(
                    ReportCheck::NameHostMismatch,
                    "",
                    "web01",
                    "db01.example.com",
                    "",
                    None,
                ),
                entry(
                    ReportCheck::GatewayOutlier,
                    "Prod",
                    "api",
                    "",
                    "bastion-a",
                    None,
                ),
            ],
        }
    }

    #[test]
    fn the_plain_text_is_the_title_the_counts_then_the_entries_by_check() {
        let text = plain_text("Team", &report());
        let expected = "\
Import report: Team
3 findings
Skipped sessions: 1
Unresolved variables: 0
Name and host mismatch: 1
Gateway outliers: 1
Duplicate hosts: 0

Skipped sessions: 1
Prod / odd box: Session type 12 is not supported, only SSH, SFTP, RDP, Telnet and VNC are taken.

Name and host mismatch: 1
(top level) / web01: The name is not the host db01.example.com nor its first label.

Gateway outliers: 1
Prod / api: It connects through no gateway (direct connection), while most of its folder connects through bastion-a.
";
        assert_eq!(text, expected);
    }

    #[test]
    fn an_empty_report_is_the_title_and_the_zero_counts() {
        let text = plain_text("Team", &ImportReport::default());
        assert!(
            text.starts_with("Import report: Team\n0 findings\n"),
            "{text}"
        );
        assert!(text.ends_with("Duplicate hosts: 0\n"), "{text}");
        let one = ImportReport {
            entries: vec![entry(ReportCheck::DuplicateHost, "A", "x", "h", "y", None)],
        };
        assert!(plain_text("T", &one).contains("\n1 finding\n"));
    }

    #[test]
    fn the_csv_is_a_header_and_a_quoted_row_per_entry_ended_by_crlf() {
        let expected = concat!(
            "\"Check\",\"Folder\",\"Session\",\"Detail\",\"Explanation\"\r\n",
            "\"Skipped sessions\",\"Prod\",\"odd box\",\"12\",",
            "\"Session type 12 is not supported, only SSH, SFTP, RDP, Telnet and VNC are taken.\"\r\n",
            "\"Name and host mismatch\",\"(top level)\",\"web01\",\"db01.example.com\",",
            "\"The name is not the host db01.example.com nor its first label.\"\r\n",
            "\"Gateway outliers\",\"Prod\",\"api\",\"\",",
            "\"It connects through no gateway (direct connection), while most of its folder ",
            "connects through bastion-a.\"\r\n",
        );
        assert_eq!(csv(&report()), expected);
    }

    #[test]
    fn a_cell_a_spreadsheet_would_read_as_a_formula_is_neutralised_and_quotes_are_doubled() {
        let hostile = ImportReport {
            entries: vec![
                entry(
                    ReportCheck::NameHostMismatch,
                    "=HYPERLINK(\"http://evil\")",
                    "+cmd|' /C calc'!A0",
                    "-2+3",
                    "",
                    None,
                ),
                entry(
                    ReportCheck::UnresolvedVariable,
                    "@SUM(A1)",
                    "say \"hi\", ok",
                    "\tTAB",
                    "",
                    None,
                ),
            ],
        };
        let text = csv(&hostile);
        // Neutralised first (an apostrophe before the formula character), then quoted.
        assert!(
            text.contains("\"'=HYPERLINK(\"\"http://evil\"\")\""),
            "{text}"
        );
        assert!(text.contains("\"'+cmd|' /C calc'!A0\""), "{text}");
        assert!(text.contains("\"'-2+3\""), "{text}");
        assert!(text.contains("\"'@SUM(A1)\""), "{text}");
        assert!(text.contains("\"say \"\"hi\"\", ok\""), "{text}");
        assert!(text.contains("\"'\tTAB\""), "{text}");
        // No cell starts with a formula character once quoted.
        for line in text
            .split(CSV_LINE_END)
            .skip(1)
            .filter(|line| !line.is_empty())
        {
            for cell in ["\"=", "\"+", "\"-", "\"@"] {
                assert!(!line.contains(&format!(",{cell}")), "{line}");
                assert!(!line.starts_with(cell), "{line}");
            }
        }
    }

    #[test]
    fn the_plain_text_holds_no_control_character_from_the_file() {
        let hostile = ImportReport {
            entries: vec![
                entry(
                    ReportCheck::UnresolvedVariable,
                    "Pr\tod",
                    "a\t+1+1\tb\r\n=2\u{202e}x",
                    "GH\tOST",
                    "",
                    None,
                ),
                entry(
                    ReportCheck::DuplicateHost,
                    "F\u{85}",
                    "x",
                    "h\tost",
                    "o\nther",
                    None,
                ),
            ],
        };
        let text = plain_text("So\turce", &hostile);
        // Only the line ends this export writes remain.
        assert!(
            !text.contains(['\t', '\r', '\u{85}', '\u{202e}']),
            "{text:?}"
        );
        assert!(text.contains("a?+1+1?b??=2?x"), "{text:?}");
        assert!(text.contains("GH?OST"), "{text:?}");
        assert!(text.contains("So?urce"), "{text:?}");
        assert!(text.contains("h?ost is also used by o?ther"), "{text:?}");
    }

    #[test]
    fn a_bookmark_named_to_become_a_formula_cell_cannot_reach_the_clipboard_text() {
        use heimdall_app::shared_sessions::{SharedSessions, Variables};
        use heimdall_core::shared_sessions::SharedSource;
        let dir = tempfile::tempdir().expect("dir");
        let file = dir.path().join("team.mxtsessions");
        std::fs::write(
            &file,
            "[Bookmarks]\r\na\t+1+1\tb= #109#0%web.example.com%22%[UNDEFINED]%\r\n",
        )
        .expect("write");
        let mut shared = SharedSessions::default();
        shared.load(
            &[SharedSource {
                id: "team".to_owned(),
                name: "Team".to_owned(),
                path: file.to_string_lossy().into_owned(),
                enabled: true,
            }],
            &Variables::default(),
        );
        let report = shared.report_of("team").expect("a report");
        assert!(
            report
                .entries
                .iter()
                .any(|entry| entry.check == ReportCheck::UnresolvedVariable),
            "{report:?}"
        );
        let text = plain_text("Team", report);
        assert!(!text.contains('\t'), "{text:?}");
        assert!(
            csv(report)
                .find('\t')
                .is_none_or(|at| csv(report)[..at].ends_with('\''))
        );
    }

    #[test]
    fn the_file_starts_with_a_byte_order_mark_and_is_named_after_its_source() {
        assert_eq!(file_name("team"), "import-report-team.csv");
        let bytes = file_bytes("a\r\n");
        assert_eq!(&bytes[..3], &[0xEF, 0xBB, 0xBF]);
        assert_eq!(&bytes[3..], b"a\r\n");
    }
}
