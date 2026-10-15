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

//! The cron job manager's two lists, as the C# `CronJobViewModel`
//! (`ViewModels/Tools/CronJobViewModel.cs`) fills them: a crontab pasted, read as the C#
//! `CrontabParser`, its runs and descriptions from the crontab builder's
//! [`cron_builder`](super::cron_builder); and this computer's Windows scheduled tasks, from
//! `schtasks /query /fo CSV /v` as the C# `CronJobService` runs it and its
//! `SchtasksCsvParser` reads it.

use std::time::Duration;

use super::cron_builder::{self, Description, FIELD_COUNT, Minute};
use super::csv_cell;

/// Runs shown for the entry selected, as the C# `NextRunsCount`.
pub const NEXT_RUNS_COUNT: usize = 5;

/// How long `schtasks` is given, as the C# `LoadTimeoutSeconds`.
pub const LOAD_TIMEOUT: Duration = Duration::from_secs(30);

/// Windows' program listing the scheduled tasks, run from the system folder.
const SCHTASKS_PROGRAM: &str = "schtasks.exe";

/// The whole task list as verbose CSV, as the C# `SchtasksQueryArguments`.
const SCHTASKS_ARGUMENTS: [&str; 4] = ["/query", "/fo", "CSV", "/v"];

/// What separates the cells of a row copied, as the C# `BuildClipboardText`.
const CELL_SEPARATOR: char = '\t';

/// The columns of the verbose CSV the C# finds by their English names.
const TASK_NAME_COLUMN: &str = "TaskName";
const STATUS_COLUMN: &str = "Status";
const NEXT_RUN_COLUMN: &str = "Next Run Time";
const LAST_RUN_COLUMN: &str = "Last Run Time";
const LAST_RESULT_COLUMN: &str = "Last Result";

/// Where the verbose CSV puts them, whatever the language of Windows names them: the host,
/// then the task, its next run, its status, the logon mode, its last run and last result.
const TASK_NAME_POSITION: usize = 1;
const NEXT_RUN_POSITION: usize = 2;
const STATUS_POSITION: usize = 3;
const LAST_RUN_POSITION: usize = 5;
const LAST_RESULT_POSITION: usize = 6;

/// One schedule line of a crontab, as the C# `CrontabEntry`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CrontabEntry {
    /// Minute, hour, day of the month, month and day of the week, as written.
    pub fields: [String; FIELD_COUNT],
    /// What it runs: the rest of the line.
    pub command: String,
}

impl CrontabEntry {
    /// Its schedule, the five fields joined by spaces, as the C# `Schedule` column.
    #[must_use]
    pub fn schedule(&self) -> String {
        cron_builder::join(&self.fields)
    }

    /// What its schedule does, as the C# `CronDescriber`; a schedule the builder cannot read
    /// is shown as written.
    #[must_use]
    pub fn description(&self) -> Description {
        if cron_builder::parse(&self.schedule()).is_some() {
            cron_builder::describe(&self.fields)
        } else {
            Description::Custom(self.schedule())
        }
    }

    /// Its next `count` runs after `now`, a day of the week of 7 being Sunday; none when a
    /// field cannot be read, as the C# `CronScheduleCalculator`.
    #[must_use]
    pub fn next_runs(&self, now: Minute, count: usize) -> Vec<String> {
        if cron_builder::parse(&self.schedule()).is_none() {
            return Vec::new();
        }
        cron_builder::crontab_next_runs(&self.fields, now, count)
    }
}

/// The schedule lines of `content`, as the C# `CrontabParser.Parse`: blank lines, comments
/// and environment settings (`NAME=value`) skipped, as are lines of fewer than six words;
/// the command is the rest of the line.
#[must_use]
pub fn parse_crontab(content: &str) -> Vec<CrontabEntry> {
    content.lines().filter_map(parse_crontab_line).collect()
}

/// One line of a crontab, when it is a schedule.
fn parse_crontab_line(line: &str) -> Option<CrontabEntry> {
    let trimmed = line.trim();
    if trimmed.is_empty() || trimmed.starts_with('#') {
        return None;
    }
    // `MAILTO=root`, `PATH=/usr/bin`: a name before `=`, no space in it.
    if let Some(equals) = trimmed.find('=')
        && equals > 0
        && !trimmed[..equals].contains([' ', '\t'])
    {
        return None;
    }
    let mut rest = trimmed;
    let mut fields: [String; FIELD_COUNT] = Default::default();
    for field in &mut fields {
        let word_end = rest.find([' ', '\t'])?;
        *field = rest[..word_end].to_owned();
        rest = rest[word_end..].trim_start_matches([' ', '\t']);
    }
    if rest.is_empty() {
        return None;
    }
    Some(CrontabEntry {
        fields,
        command: rest.to_owned(),
    })
}

/// One Windows scheduled task, as the C# `WindowsTaskEntry`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScheduledTask {
    /// Its path and name.
    pub name: String,
    /// Its status, as Windows words it.
    pub status: String,
    /// When it runs next, as Windows writes it.
    pub next_run: String,
    /// When it last ran.
    pub last_run: String,
    /// What its last run returned.
    pub last_result: String,
}

/// The column of the verbose CSV each value is read from.
struct Columns {
    name: usize,
    status: usize,
    next_run: usize,
    last_run: usize,
    last_result: usize,
}

impl Columns {
    /// The columns of `header`: by their English names as the C# finds them; on a Windows
    /// that names them in its own language, where the verbose layout puts them.
    fn of(header: &[String]) -> Option<Self> {
        let find = |name: &str| {
            let lowered = name.to_lowercase();
            header
                .iter()
                .position(|column| column.to_lowercase().contains(&lowered))
        };
        if let Some(name) = find(TASK_NAME_COLUMN) {
            return Some(Self {
                name,
                status: find(STATUS_COLUMN).unwrap_or(usize::MAX),
                next_run: find(NEXT_RUN_COLUMN).unwrap_or(usize::MAX),
                last_run: find(LAST_RUN_COLUMN).unwrap_or(usize::MAX),
                last_result: find(LAST_RESULT_COLUMN).unwrap_or(usize::MAX),
            });
        }
        (header.len() > LAST_RESULT_POSITION).then_some(Self {
            name: TASK_NAME_POSITION,
            status: STATUS_POSITION,
            next_run: NEXT_RUN_POSITION,
            last_run: LAST_RUN_POSITION,
            last_result: LAST_RESULT_POSITION,
        })
    }
}

/// The tasks of `schtasks /query /fo CSV /v` output, as the C# `SchtasksCsvParser.Parse`:
/// the header read from the first line, rows without a name skipped, and so are the
/// header's repeats, which `schtasks` writes before each folder's tasks.
#[must_use]
pub fn parse_schtasks_csv(csv: &str) -> Vec<ScheduledTask> {
    let mut lines = csv.lines().map(str::trim).filter(|line| !line.is_empty());
    let Some(header_line) = lines.next() else {
        return Vec::new();
    };
    let header = parse_csv_line(header_line);
    let Some(columns) = Columns::of(&header) else {
        return Vec::new();
    };
    let cell = |fields: &[String], index: usize| fields.get(index).cloned().unwrap_or_default();
    lines
        .filter(|line| *line != header_line)
        .filter_map(|line| {
            let fields = parse_csv_line(line);
            let name = cell(&fields, columns.name);
            (!name.is_empty()).then(|| ScheduledTask {
                name,
                status: cell(&fields, columns.status),
                next_run: cell(&fields, columns.next_run),
                last_run: cell(&fields, columns.last_run),
                last_result: cell(&fields, columns.last_result),
            })
        })
        .collect()
}

/// The fields of one CSV line, as the C# `ParseCsvLine`: quotes taken off, a doubled quote
/// inside quotes read as one, each field trimmed.
fn parse_csv_line(line: &str) -> Vec<String> {
    let mut fields = Vec::new();
    let mut current = String::new();
    let mut in_quotes = false;
    let mut chars = line.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '"' if in_quotes && chars.peek() == Some(&'"') => {
                current.push('"');
                chars.next();
            }
            '"' => in_quotes = !in_quotes,
            ',' if !in_quotes => {
                fields.push(current.trim().to_owned());
                current.clear();
            }
            _ => current.push(c),
        }
    }
    fields.push(current.trim().to_owned());
    fields
}

/// The output of `schtasks` as text: UTF-8 when it is, else code page 850, the OEM code page
/// a console program writes a French or Western European Windows' text in.
#[must_use]
pub fn decode_console_output(bytes: &[u8]) -> String {
    match std::str::from_utf8(bytes) {
        Ok(text) => text.to_owned(),
        Err(_) => crate::code_page::decode_cp850(bytes),
    }
}

/// Why the scheduled tasks could not be listed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TasksError {
    /// Not on Windows, which alone has them.
    Unsupported,
    /// Windows did not say where its system folder is.
    NoSystemFolder,
    /// `schtasks` did not end within [`LOAD_TIMEOUT`], as the C# timeout.
    TimedOut,
    /// `schtasks` could not be started, or failed: what it or the system said.
    Failed(String),
}

/// This computer's scheduled tasks, as the C# `LoadWindowsTasksAsync`: `schtasks` run from
/// the system folder, never one found through `PATH`, without a console window, and given
/// [`LOAD_TIMEOUT`].
///
/// # Errors
///
/// [`TasksError`] when the tasks cannot be listed.
pub fn load_scheduled_tasks() -> Result<Vec<ScheduledTask>, TasksError> {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt as _;

        /// `CREATE_NO_WINDOW`: no console flashes up while `schtasks` runs.
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;

        let program =
            crate::paths::system_program(SCHTASKS_PROGRAM).ok_or(TasksError::NoSystemFolder)?;
        let mut command = std::process::Command::new(&program);
        command
            .args(SCHTASKS_ARGUMENTS)
            .creation_flags(CREATE_NO_WINDOW);
        if let Some(folder) = program.parent() {
            command.current_dir(folder);
        }
        let output = timed::run(&mut command, LOAD_TIMEOUT)?;
        Ok(parse_schtasks_csv(&decode_console_output(&output.stdout)))
    }
    #[cfg(not(windows))]
    {
        let _ = (SCHTASKS_PROGRAM, SCHTASKS_ARGUMENTS);
        Err(TasksError::Unsupported)
    }
}

/// Programs run with a time limit: `schtasks` on Windows, and the tests.
#[cfg(any(windows, test))]
mod timed {
    use std::io::{self, Read};
    use std::process::{Command, Output, Stdio};
    use std::thread;
    use std::time::{Duration, Instant};

    use super::{TasksError, decode_console_output};

    /// How often a running program is looked at for its end.
    const EXIT_POLL: Duration = Duration::from_millis(50);

    /// `command` run to its end within `timeout`, its outputs read as it runs; killed past it.
    ///
    /// # Errors
    ///
    /// [`TasksError::TimedOut`] past `timeout`, [`TasksError::Failed`] when it cannot start or
    /// ends in failure.
    pub(super) fn run(command: &mut Command, timeout: Duration) -> Result<Output, TasksError> {
        let failed = |error: io::Error| TasksError::Failed(error.to_string());
        let mut child = command
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(failed)?;
        let stdout = child.stdout.take().map(drain);
        let stderr = child.stderr.take().map(drain);
        let deadline = Instant::now() + timeout;
        let status = loop {
            if let Some(status) = child.try_wait().map_err(failed)? {
                break status;
            }
            if Instant::now() >= deadline {
                let _ = child.kill();
                let _ = child.wait();
                return Err(TasksError::TimedOut);
            }
            thread::sleep(EXIT_POLL);
        };
        let collect = |reader: Option<thread::JoinHandle<Vec<u8>>>| {
            reader
                .and_then(|reader| reader.join().ok())
                .unwrap_or_default()
        };
        let output = Output {
            status,
            stdout: collect(stdout),
            stderr: collect(stderr),
        };
        if output.status.success() {
            Ok(output)
        } else {
            let said = decode_console_output(&output.stderr).trim().to_owned();
            Err(TasksError::Failed(if said.is_empty() {
                output.status.to_string()
            } else {
                said
            }))
        }
    }

    /// A thread reading `pipe` to its end, so a full pipe never stalls the program.
    fn drain(mut pipe: impl Read + Send + 'static) -> thread::JoinHandle<Vec<u8>> {
        thread::spawn(move || {
            let mut bytes = Vec::new();
            let _ = pipe.read_to_end(&mut bytes);
            bytes
        })
    }
}

/// One row copied: its cells sanitized and joined by tabs, as the C# `BuildClipboardText`.
#[must_use]
pub fn clipboard_row(cells: &[&str]) -> String {
    let mut row = cells
        .iter()
        .map(|cell| csv_cell::sanitize(cell))
        .collect::<Vec<_>>()
        .join(&CELL_SEPARATOR.to_string());
    row.push('\n');
    row
}

/// The tasks as the C# copies them: name, status, next run, last run and last result.
#[must_use]
pub fn tasks_clipboard_text(tasks: &[ScheduledTask]) -> String {
    tasks
        .iter()
        .map(|task| {
            clipboard_row(&[
                &task.name,
                &task.status,
                &task.next_run,
                &task.last_run,
                &task.last_result,
            ])
        })
        .collect()
}

#[cfg(test)]
mod tests {
    #[cfg(unix)]
    use std::process::Command;
    #[cfg(unix)]
    use std::time::Instant;

    use super::*;
    use crate::tools::date_time;

    fn minute(seconds: i64) -> Minute {
        Minute::of(&date_time::Instant::from_unix_seconds(seconds).wall_clock(0))
    }

    #[test]
    fn a_crontab_keeps_its_schedules_and_skips_comments_settings_and_short_lines() {
        let sample = "\
# Nightly backup
MAILTO=root
PATH=/usr/bin:/bin
0 2 * * * /usr/local/bin/backup.sh --full  now
*/15\t* * * *\t/usr/bin/healthcheck
@reboot /opt/start.sh
0 0 * * 7 /opt/sunday.sh
";
        let entries = parse_crontab(sample);
        assert_eq!(entries.len(), 3);
        assert_eq!(entries[0].schedule(), "0 2 * * *");
        assert_eq!(entries[0].command, "/usr/local/bin/backup.sh --full  now");
        assert_eq!(entries[1].schedule(), "*/15 * * * *");
        assert_eq!(entries[1].command, "/usr/bin/healthcheck");
        assert_eq!(
            entries[0].description(),
            Description::DailyAt("02:00".to_owned())
        );
        assert_eq!(
            entries[1].description(),
            Description::EveryNMinutes("15".to_owned())
        );
    }

    #[test]
    fn next_runs_are_real_and_seven_is_sunday() {
        // 2026-10-15 12:07 UTC is a Thursday.
        let now = minute(1_792_066_020);
        let entries = parse_crontab("*/5 * * * * a\n0 0 * * 7 b\n0 0 * * 0 c\n0 0 * * MON d\n");
        assert_eq!(
            entries[0].next_runs(now, 3),
            [
                "2026-10-15 12:10 (Thu)",
                "2026-10-15 12:15 (Thu)",
                "2026-10-15 12:20 (Thu)"
            ]
        );
        assert_eq!(entries[1].next_runs(now, 1), ["2026-10-18 00:00 (Sun)"]);
        assert_eq!(entries[1].next_runs(now, 1), entries[2].next_runs(now, 1));
        assert!(
            entries[3].next_runs(now, 1).is_empty(),
            "a name is not read"
        );
        assert_eq!(
            entries[3].description(),
            Description::Custom("0 0 * * MON".to_owned())
        );
    }

    #[test]
    fn schtasks_rows_are_read_by_their_english_columns() {
        let sample = "\
\"HostName\",\"TaskName\",\"Next Run Time\",\"Status\",\"Logon Mode\",\"Last Run Time\",\"Last Result\",\"Author\"
\"HOST\",\"\\Microsoft\\Backup\",\"10/16/2026 3:00:00 AM\",\"Ready\",\"Interactive/Background\",\"10/15/2026 3:00:00 AM\",\"0\",\"Microsoft\"
\"HostName\",\"TaskName\",\"Next Run Time\",\"Status\",\"Logon Mode\",\"Last Run Time\",\"Last Result\",\"Author\"
\"HOST\",\"\\Say \"\"hi\"\"\",\"N/A\",\"Disabled\",\"Interactive only\",\"N/A\",\"267011\",\"me\"
\"HOST\",\"\",\"N/A\",\"Ready\",\"\",\"\",\"\",\"\"
";
        let tasks = parse_schtasks_csv(sample);
        assert_eq!(
            tasks,
            [
                ScheduledTask {
                    name: "\\Microsoft\\Backup".to_owned(),
                    status: "Ready".to_owned(),
                    next_run: "10/16/2026 3:00:00 AM".to_owned(),
                    last_run: "10/15/2026 3:00:00 AM".to_owned(),
                    last_result: "0".to_owned(),
                },
                ScheduledTask {
                    name: "\\Say \"hi\"".to_owned(),
                    status: "Disabled".to_owned(),
                    next_run: "N/A".to_owned(),
                    last_run: "N/A".to_owned(),
                    last_result: "267011".to_owned(),
                },
            ]
        );
    }

    #[test]
    fn a_french_windows_header_is_read_by_the_verbose_layout() {
        let sample = decode_console_output(
            b"\"Nom de l'h\x93te\",\"Nom de la t\x83che\",\"Prochaine ex\x82cution\",\"Statut\",\"Mode d'ouverture de session\",\"Derni\x8Are ex\x82cution\",\"Dernier r\x82sultat\",\"Auteur\"\r\n\
\"PC\",\"\\T\x83che\",\"16/10/2026 03:00:00\",\"Pr\x88t\",\"Interactif/Arri\x8Are-plan\",\"15/10/2026 03:00:00\",\"0\",\"Moi\"\r\n",
        );
        let tasks = parse_schtasks_csv(&sample);
        assert_eq!(
            tasks,
            [ScheduledTask {
                name: "\\T\u{e2}che".to_owned(),
                status: "Pr\u{ea}t".to_owned(),
                next_run: "16/10/2026 03:00:00".to_owned(),
                last_run: "15/10/2026 03:00:00".to_owned(),
                last_result: "0".to_owned(),
            }]
        );
    }

    #[test]
    fn output_with_too_few_columns_holds_no_task() {
        assert!(parse_schtasks_csv("\"a\",\"b\"\n\"c\",\"d\"\n").is_empty());
        assert!(parse_schtasks_csv("").is_empty());
    }

    #[test]
    fn copied_rows_are_tab_separated_and_sanitized() {
        let tasks = [ScheduledTask {
            name: "=HYPERLINK(\"x\")".to_owned(),
            status: "Ready".to_owned(),
            next_run: "N/A".to_owned(),
            last_run: "-".to_owned(),
            last_result: "0".to_owned(),
        }];
        assert_eq!(
            tasks_clipboard_text(&tasks),
            "'=HYPERLINK(\"x\")\tReady\tN/A\t'-\t0\n"
        );
    }

    #[cfg(unix)]
    #[test]
    fn a_program_past_its_time_is_killed() {
        let mut command = Command::new("sleep");
        command.arg("5");
        let started = Instant::now();
        assert_eq!(
            timed::run(&mut command, Duration::from_millis(100)),
            Err(TasksError::TimedOut)
        );
        assert!(started.elapsed() < Duration::from_secs(4));
    }

    #[cfg(unix)]
    #[test]
    fn a_failing_program_says_why() {
        let mut command = Command::new("sh");
        command.args(["-c", "echo nope >&2; exit 3"]);
        assert_eq!(
            timed::run(&mut command, LOAD_TIMEOUT),
            Err(TasksError::Failed("nope".to_owned()))
        );
        let mut command = Command::new("sh");
        command.args(["-c", "printf ok"]);
        assert_eq!(
            timed::run(&mut command, LOAD_TIMEOUT).map(|output| output.stdout),
            Ok(b"ok".to_vec())
        );
    }
}
