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

//! Cron job parsing, natural language schedule description, and task scheduler engine.
//!
//! Provides parsing of standard 5-field crontab lines, next run calculations,
//! and Windows Task Scheduler (`schtasks`) enumeration.

use std::process::Command;

/// A parsed crontab entry.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct CrontabEntry {
    /// Minute field (0..=59 or `*`, `*/n`, `a,b`).
    pub minute: String,
    /// Hour field (0..=23 or `*`, `*/n`, `a,b`).
    pub hour: String,
    /// Day of month field (1..=31 or `*`, `*/n`, `a,b`).
    pub day_of_month: String,
    /// Month field (1..=12 or `*`, `*/n`, `a,b`).
    pub month: String,
    /// Day of week field (0..=7 or `*`, `*/n`, `a,b`).
    pub day_of_week: String,
    /// Command line or shell script to execute.
    pub command: String,
    /// Raw unparsed line.
    pub raw_line: String,
    /// Human-readable schedule description.
    pub description: String,
    /// Next upcoming execution timestamps formatted as strings.
    pub next_runs: Vec<String>,
}

/// Parser and schedule descriptor for crontab syntax.
pub struct CrontabParser;

impl CrontabParser {
    /// Generates a natural language description from a 5-field cron expression string.
    #[must_use]
    pub fn describe(expr: &str) -> String {
        let parts: Vec<&str> = expr.split_whitespace().collect();
        if parts.len() >= 5 {
            Self::describe_schedule(parts[0], parts[1], parts[2], parts[3], parts[4])
        } else {
            "Invalid cron expression".to_owned()
        }
    }

    /// Calculates upcoming run timestamps from a 5-field cron expression string.
    #[must_use]
    pub fn next_runs(expr: &str, count: usize) -> Vec<String> {
        let parts: Vec<&str> = expr.split_whitespace().collect();
        if parts.len() >= 5 {
            Self::calculate_next_runs(parts[0], parts[1], parts[2], parts[3], parts[4], count)
        } else {
            Vec::new()
        }
    }

    /// Parses multi-line crontab text into a list of [`CrontabEntry`].
    #[must_use]
    pub fn parse(content: &str) -> Vec<CrontabEntry> {
        let mut entries = Vec::new();

        for raw_line in content.lines() {
            let line = raw_line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }

            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() < 6 {
                continue;
            }

            let minute = parts[0].to_owned();
            let hour = parts[1].to_owned();
            let day_of_month = parts[2].to_owned();
            let month = parts[3].to_owned();
            let day_of_week = parts[4].to_owned();
            let command = parts[5..].join(" ");

            let description =
                Self::describe_schedule(&minute, &hour, &day_of_month, &month, &day_of_week);
            let next_runs =
                Self::calculate_next_runs(&minute, &hour, &day_of_month, &month, &day_of_week, 5);

            entries.push(CrontabEntry {
                minute,
                hour,
                day_of_month,
                month,
                day_of_week,
                command,
                raw_line: raw_line.to_owned(),
                description,
                next_runs,
            });
        }

        entries
    }

    /// Generates a natural language description for the 5-field cron schedule.
    #[must_use]
    pub fn describe_schedule(min: &str, hour: &str, dom: &str, mon: &str, dow: &str) -> String {
        if min == "*" && hour == "*" && dom == "*" && mon == "*" && dow == "*" {
            return "Every minute".to_owned();
        }

        if let Some(step) = min.strip_prefix("*/") {
            if hour == "*" && dom == "*" && mon == "*" && dow == "*" {
                return format!("Every {step} minutes");
            }
        }

        if min == "0" && hour == "*" && dom == "*" && mon == "*" && dow == "*" {
            return "Every hour at minute 0".to_owned();
        }

        if let Some(step) = hour.strip_prefix("*/") {
            if min == "0" && dom == "*" && mon == "*" && dow == "*" {
                return format!("Every {step} hours at minute 0");
            }
        }

        if min == "0" && hour == "0" && dom == "*" && mon == "*" && dow == "*" {
            return "Every day at midnight (00:00)".to_owned();
        }

        if min == "0" && hour == "12" && dom == "*" && mon == "*" && dow == "*" {
            return "Every day at noon (12:00)".to_owned();
        }

        if dom == "*" && mon == "*" && dow == "*" {
            if let (Ok(h), Ok(m)) = (hour.parse::<u32>(), min.parse::<u32>()) {
                return format!("Every day at {:02}:{:02}", h, m);
            }
        }

        if dom == "*" && mon == "*" {
            let day_name = match dow {
                "0" | "7" => "Sunday",
                "1" => "Monday",
                "2" => "Tuesday",
                "3" => "Wednesday",
                "4" => "Thursday",
                "5" => "Friday",
                "6" => "Saturday",
                _ => dow,
            };
            if let (Ok(h), Ok(m)) = (hour.parse::<u32>(), min.parse::<u32>()) {
                return format!("Every {day_name} at {:02}:{:02}", h, m);
            }
        }

        format!("At {min} {hour} on day {dom} of month {mon} (weekday {dow})")
    }

    /// Simulates the next upcoming execution times for preview.
    #[must_use]
    pub fn calculate_next_runs(
        min: &str,
        hour: &str,
        _dom: &str,
        _mon: &str,
        _dow: &str,
        count: usize,
    ) -> Vec<String> {
        let mut runs = Vec::new();
        let target_min: u32 = min.parse().unwrap_or(0);
        let target_hour: u32 = hour.parse().unwrap_or(0);

        // Approximate 24h rolling schedule calculation for UI display
        for day_offset in 1..=count {
            runs.push(format!(
                "+{day_offset}d at {:02}:{:02}:00",
                target_hour, target_min
            ));
        }

        runs
    }
}

/// A Windows Task Scheduler entry.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct TaskEntry {
    /// Task name / hierarchy path.
    pub name: String,
    /// Next scheduled run timestamp.
    pub next_run_time: String,
    /// Execution status (e.g. "Ready", "Running", "Disabled").
    pub status: String,
    /// Executable or script command configured to run.
    pub task_to_run: String,
    /// Author or owner account.
    pub author: String,
}

impl TaskEntry {
    /// Formats the task entry as a CSV line.
    #[must_use]
    pub fn to_csv_line(&self) -> String {
        format!(
            "\"{}\",\"{}\",\"{}\",\"{}\",\"{}\"",
            self.name, self.next_run_time, self.status, self.task_to_run, self.author
        )
    }
}

/// Parser for Windows `schtasks.exe /query /fo csv /v` output.
pub struct SchtasksParser;

impl SchtasksParser {
    /// Parses CSV output from `schtasks` into a list of [`TaskEntry`].
    #[must_use]
    pub fn parse(csv: &str) -> Vec<TaskEntry> {
        let mut lines = csv.lines();
        let header_line = match lines.next() {
            Some(h) if !h.trim().is_empty() => h,
            _ => return Vec::new(),
        };

        let headers = Self::split_csv_line(header_line);
        let name_idx = Self::find_column_index(&headers, &["TaskName", "Nom de la tâche", "Tâche"])
            .unwrap_or(0);
        let next_run_idx = Self::find_column_index(
            &headers,
            &[
                "Next Run Time",
                "Prochaine exécution",
                "Heure de la prochaine",
            ],
        )
        .unwrap_or(1);
        let status_idx =
            Self::find_column_index(&headers, &["Status", "Statut", "État"]).unwrap_or(2);
        let task_to_run_idx =
            Self::find_column_index(&headers, &["Task To Run", "Tâche à exécuter", "Action"])
                .or_else(|| Self::find_column_index(&headers, &["Command", "Executable"]));
        let author_idx = Self::find_column_index(&headers, &["Author", "Auteur", "Créateur"])
            .unwrap_or(usize::MAX);

        let mut entries = Vec::new();

        for line in lines {
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }

            let fields = Self::split_csv_line(trimmed);
            let name = fields
                .get(name_idx)
                .map_or("", |s| s.trim_matches('"'))
                .to_owned();
            if name.is_empty() {
                continue;
            }

            let next_run_time = fields
                .get(next_run_idx)
                .map_or("", |s| s.trim_matches('"'))
                .to_owned();
            let status = fields
                .get(status_idx)
                .map_or("", |s| s.trim_matches('"'))
                .to_owned();
            let task_to_run = task_to_run_idx
                .and_then(|idx| fields.get(idx))
                .map_or("", |s| s.trim_matches('"'))
                .to_owned();
            let author = if author_idx < fields.len() {
                fields[author_idx].trim_matches('"').to_owned()
            } else {
                String::new()
            };

            entries.push(TaskEntry {
                name,
                next_run_time,
                status,
                task_to_run,
                author,
            });
        }

        entries
    }

    fn find_column_index(headers: &[String], candidates: &[&str]) -> Option<usize> {
        for (idx, header) in headers.iter().enumerate() {
            let lower = header.to_lowercase();
            for candidate in candidates {
                if lower.contains(&candidate.to_lowercase()) {
                    return Some(idx);
                }
            }
        }
        None
    }

    fn split_csv_line(line: &str) -> Vec<String> {
        let mut fields = Vec::new();
        let mut current = String::new();
        let mut in_quotes = false;

        for ch in line.chars() {
            match ch {
                '"' => {
                    in_quotes = !in_quotes;
                    current.push(ch);
                }
                ',' if !in_quotes => {
                    fields.push(current.trim().to_owned());
                    current.clear();
                }
                _ => current.push(ch),
            }
        }
        fields.push(current.trim().to_owned());
        fields
    }
}

/// Executes the system task query command and returns scheduled task entries.
///
/// # Errors
///
/// Returns an error message if the query process fails.
pub fn run_tasks_command() -> Result<Vec<TaskEntry>, String> {
    #[cfg(windows)]
    {
        let output = Command::new("schtasks")
            .args(["/query", "/fo", "csv", "/v"])
            .output()
            .map_err(|err| format!("Failed to run schtasks: {err}"))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(if stderr.trim().is_empty() {
                format!(
                    "schtasks failed with exit code {}",
                    output.status.code().unwrap_or(-1)
                )
            } else {
                stderr.into_owned()
            });
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        Ok(SchtasksParser::parse(&stdout))
    }

    #[cfg(not(windows))]
    {
        let output = Command::new("crontab")
            .args(["-l"])
            .output()
            .map_err(|err| format!("Failed to run crontab -l: {err}"))?;

        let stdout = String::from_utf8_lossy(&output.stdout);
        let crons = CrontabParser::parse(&stdout);
        let tasks = crons
            .into_iter()
            .map(|c| TaskEntry {
                name: c.command.clone(),
                next_run_time: c.next_runs.first().cloned().unwrap_or_default(),
                status: "Enabled".to_owned(),
                task_to_run: c.command,
                author: "current_user".to_owned(),
            })
            .collect();
        Ok(tasks)
    }
}

/// Builds a CSV string from a slice of [`TaskEntry`].
#[must_use]
pub fn build_tasks_csv(entries: &[TaskEntry]) -> String {
    let mut csv = String::from("TaskName,NextRunTime,Status,TaskToRun,Author\n");
    for t in entries {
        csv.push_str(&t.to_csv_line());
        csv.push('\n');
    }
    csv
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_crontab_parser_and_descriptions() {
        let sample = "\
# Nightly backup
0 2 * * * /usr/local/bin/backup.sh
*/15 * * * * /usr/bin/healthcheck
0 0 * * 1 /opt/cleanup.sh
";
        let parsed = CrontabParser::parse(sample);
        assert_eq!(parsed.len(), 3);

        assert_eq!(parsed[0].command, "/usr/local/bin/backup.sh");
        assert_eq!(parsed[0].description, "Every day at 02:00");

        assert_eq!(parsed[1].command, "/usr/bin/healthcheck");
        assert_eq!(parsed[1].description, "Every 15 minutes");

        assert_eq!(parsed[2].command, "/opt/cleanup.sh");
        assert_eq!(parsed[2].description, "Every Monday at 00:00");
    }

    #[test]
    fn test_schtasks_csv_parser() {
        let sample = "\
\"Folder\",\"HostName\",\"TaskName\",\"Next Run Time\",\"Status\",\"Logon Mode\",\"Last Run Time\",\"Last Result\",\"Author\",\"Task To Run\"
\"\\\",\"HOST\",\"\\Microsoft\\Windows\\UpdateOrchestrator\\Backup\",\"10/16/2026 03:00:00 AM\",\"Ready\",\"Interactive/Background\",\"10/15/2026 03:00:00 AM\",\"0\",\"Microsoft\",\"cmd.exe /c echo update\"
";
        let tasks = SchtasksParser::parse(sample);
        assert_eq!(tasks.len(), 1);
        assert_eq!(
            tasks[0].name,
            "\\Microsoft\\Windows\\UpdateOrchestrator\\Backup"
        );
        assert_eq!(tasks[0].status, "Ready");
    }
}
