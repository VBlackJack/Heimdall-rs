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

//! Notes about a server, as the C# Notes tool's templates write them: a Markdown file in
//! the notes folder beside the profiles, named after when and what, opened in the editor
//! set. The day's note is one file a day, opened again when it is there.

use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use unicode_normalization::UnicodeNormalization as _;
use unicode_normalization::char::is_combining_mark;

/// The folder of the notes, beside the profiles.
pub const NOTES_FOLDER: &str = "notes";

/// The folder of the day's notes, in the notes folder.
const DAILY_FOLDER: &str = "daily";

/// The extension of a note.
const NOTE_EXTENSION: &str = "md";

/// What a note starts as, as the C# templates.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NoteTemplate {
    /// A working note: notes, commands, next.
    Blank,
    /// The day's note, one a day.
    Daily,
    /// An incident report.
    Incident,
    /// A procedure.
    Procedure,
}

impl NoteTemplate {
    /// Every template, in the C# menu's order.
    pub const ALL: [Self; 4] = [Self::Blank, Self::Daily, Self::Incident, Self::Procedure];
}

/// The server a note is about.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct NoteContext {
    /// Its name.
    pub name: String,
    /// Its host.
    pub host: Option<String>,
    /// Its port.
    pub port: Option<u16>,
    /// The account.
    pub user: Option<String>,
    /// Its folder.
    pub group: Option<String>,
    /// Its protocol.
    pub protocol: Option<String>,
}

/// The words of the templates, in the language shown.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
#[expect(missing_docs, reason = "each the C# template word of the same name")]
pub struct NoteLabels {
    pub working_note: String,
    pub notes: String,
    pub commands: String,
    pub next: String,
    pub daily_note: String,
    pub focus: String,
    pub journal: String,
    pub follow_up: String,
    pub incident: String,
    pub incident_report: String,
    pub summary: String,
    pub impact: String,
    pub timeline: String,
    pub incident_started: String,
    pub investigation: String,
    pub actions: String,
    pub resolution: String,
    pub procedure: String,
    pub purpose: String,
    pub scope: String,
    pub preconditions: String,
    pub steps: String,
    pub validation: String,
    pub rollback: String,
    pub references: String,
}

/// A time on this computer's clock.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[expect(missing_docs, reason = "the parts of a date and time")]
pub struct LocalTime {
    pub year: i32,
    pub month: u32,
    pub day: u32,
    pub hour: u32,
    pub minute: u32,
    pub second: u32,
}

impl LocalTime {
    /// Now, on this computer's clock.
    #[must_use]
    pub fn now() -> Self {
        use chrono::{Datelike as _, Timelike as _};
        let now = chrono::Local::now();
        Self {
            year: now.year(),
            month: now.month(),
            day: now.day(),
            hour: now.hour(),
            minute: now.minute(),
            second: now.second(),
        }
    }

    fn date(self) -> String {
        format!("{:04}-{:02}-{:02}", self.year, self.month, self.day)
    }

    fn minutes(self) -> String {
        format!("{} {:02}:{:02}", self.date(), self.hour, self.minute)
    }

    /// As the C# names a note: `20261005-142233`.
    fn stamp(self) -> String {
        format!(
            "{:04}{:02}{:02}-{:02}{:02}{:02}",
            self.year, self.month, self.day, self.hour, self.minute, self.second
        )
    }
}

/// A note to write: where in the notes folder, and what it starts with.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NoteDraft {
    /// Its path, in the notes folder.
    pub path: PathBuf,
    /// Its text.
    pub content: String,
}

/// `value` as a file name's part, as the C# `Slugify`: letters and digits in lower case,
/// accents taken off, anything else a single dash.
#[must_use]
pub fn slug(value: &str) -> String {
    let mut slug = String::with_capacity(value.len());
    let mut dash = false;
    for c in value.nfd().filter(|c| !is_combining_mark(*c)) {
        if c.is_alphanumeric() {
            slug.extend(c.to_lowercase());
            dash = false;
        } else if !dash {
            slug.push('-');
            dash = true;
        }
    }
    slug.trim_matches('-').to_owned()
}

/// What the note is about, in its title: the server's name, else its host.
fn about(context: &NoteContext) -> Option<&str> {
    Some(context.name.trim())
        .filter(|name| !name.is_empty())
        .or(context.host.as_deref())
}

/// The line under the title, as the C# writes it.
fn metadata(now: LocalTime, context: &NoteContext) -> String {
    let mut parts = vec![format!("created {}", now.minutes())];
    if !context.name.trim().is_empty() {
        parts.push(format!("display {}", context.name.trim()));
    }
    if let Some(host) = &context.host {
        parts.push(format!("host {host}"));
    }
    if let Some(port) = context.port {
        parts.push(format!("port {port}"));
    }
    if let Some(user) = context.user.as_deref().filter(|user| !user.is_empty()) {
        parts.push(format!("user {user}"));
    }
    if let Some(group) = &context.group {
        parts.push(format!("group {group}"));
    }
    if let Some(protocol) = &context.protocol {
        parts.push(format!("type {protocol}"));
    }
    format!("> {}", parts.join(" | "))
}

/// A Markdown note: its title, the metadata line, then each section with what it starts
/// with.
fn markdown(title: &str, metadata: &str, sections: &[(&str, &str)]) -> String {
    let mut text = format!("# {title}\n\n{metadata}\n");
    for (heading, starts) in sections {
        let _ = write!(text, "\n## {heading}\n\n");
        if !starts.is_empty() {
            let _ = writeln!(text, "{starts}");
        }
    }
    text
}

/// A block of shell commands, empty.
const COMMANDS: &str = "```bash\n\n```";

/// The note `template` writes about `context`, at `now`.
#[must_use]
pub fn draft(
    template: NoteTemplate,
    context: &NoteContext,
    now: LocalTime,
    labels: &NoteLabels,
) -> NoteDraft {
    let about = about(context);
    let named = |label: &str| match about {
        Some(about) => format!("{label} - {about}"),
        None => label.to_owned(),
    };
    let file =
        |title: &str| PathBuf::from(format!("{}-{}.{NOTE_EXTENSION}", now.stamp(), slug(title)));
    let metadata = metadata(now, context);
    match template {
        NoteTemplate::Blank => {
            let title = named(&labels.working_note);
            NoteDraft {
                path: file(&title),
                content: markdown(
                    &title,
                    &metadata,
                    &[
                        (&labels.notes, ""),
                        (&labels.commands, COMMANDS),
                        (&labels.next, "- "),
                    ],
                ),
            }
        }
        NoteTemplate::Daily => {
            let date = now.date();
            let title = match about {
                Some(about) => format!("{} - {date} - {about}", labels.daily_note),
                None => format!("{} - {date}", labels.daily_note),
            };
            NoteDraft {
                path: Path::new(DAILY_FOLDER)
                    .join(format!("{:04}", now.year))
                    .join(format!("{date}.{NOTE_EXTENSION}")),
                content: markdown(
                    &title,
                    &metadata,
                    &[
                        (&labels.focus, "- "),
                        (&labels.journal, ""),
                        (&labels.commands, COMMANDS),
                        (&labels.follow_up, "- "),
                    ],
                ),
            }
        }
        NoteTemplate::Incident => {
            let title = match about {
                Some(about) => format!("{} - {about}", labels.incident),
                None => labels.incident_report.clone(),
            };
            let started = format!(
                "- {:02}:{:02} - {}",
                now.hour, now.minute, labels.incident_started
            );
            NoteDraft {
                path: file(&title),
                content: markdown(
                    &title,
                    &metadata,
                    &[
                        (&labels.summary, ""),
                        (&labels.impact, ""),
                        (&labels.timeline, &started),
                        (&labels.investigation, ""),
                        (&labels.actions, "- "),
                        (&labels.resolution, ""),
                        (&labels.follow_up, "- "),
                    ],
                ),
            }
        }
        NoteTemplate::Procedure => {
            let title = named(&labels.procedure);
            NoteDraft {
                path: file(&title),
                content: markdown(
                    &title,
                    &metadata,
                    &[
                        (&labels.purpose, ""),
                        (&labels.scope, ""),
                        (&labels.preconditions, "- "),
                        (&labels.steps, "1. "),
                        (&labels.validation, ""),
                        (&labels.rollback, ""),
                        (&labels.references, "- "),
                    ],
                ),
            }
        }
    }
}

/// The notes folder beside `profiles_file`.
#[must_use]
pub fn notes_dir(profiles_file: &Path) -> PathBuf {
    profiles_file.with_file_name(NOTES_FOLDER)
}

/// Writes `draft` in the notes folder `dir`, unless a note is there already (the day's
/// note, written earlier), then opens it in the editor `setting` names; the note's path.
///
/// # Errors
///
/// Why it could not be written or opened.
pub async fn open(dir: PathBuf, draft: NoteDraft, setting: String) -> Result<PathBuf, String> {
    let path = dir.join(&draft.path);
    if let Some(folder) = path.parent() {
        tokio::fs::create_dir_all(folder)
            .await
            .map_err(|error| format!("{}: {error}", folder.display()))?;
    }
    match tokio::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&path)
        .await
    {
        Ok(mut file) => {
            use tokio::io::AsyncWriteExt as _;
            file.write_all(draft.content.as_bytes())
                .await
                .map_err(|error| format!("{}: {error}", path.display()))?;
        }
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
        Err(error) => return Err(format!("{}: {error}", path.display())),
    }
    let editor = crate::external_edit::editor(&setting).map_err(|refused| match refused {
        crate::external_edit::EditorRefused::NotFound(path)
        | crate::external_edit::EditorRefused::Runs(path)
        | crate::external_edit::EditorRefused::NotAProgram(path) => path,
    })?;
    let opened = path.clone();
    tokio::task::spawn_blocking(move || crate::external_edit::launch(&editor, &opened))
        .await
        .map_err(|error| error.to_string())?
        .map_err(|error| error.to_string())?;
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn labels() -> NoteLabels {
        NoteLabels {
            working_note: "Working Note".to_owned(),
            notes: "Notes".to_owned(),
            commands: "Commands".to_owned(),
            next: "Next".to_owned(),
            daily_note: "Daily Note".to_owned(),
            focus: "Focus".to_owned(),
            journal: "Journal".to_owned(),
            follow_up: "Follow-up".to_owned(),
            incident: "Incident".to_owned(),
            incident_report: "Incident Report".to_owned(),
            incident_started: "Incident started".to_owned(),
            timeline: "Timeline".to_owned(),
            procedure: "Procedure".to_owned(),
            steps: "Steps".to_owned(),
            ..NoteLabels::default()
        }
    }

    const NOW: LocalTime = LocalTime {
        year: 2026,
        month: 10,
        day: 5,
        hour: 14,
        minute: 7,
        second: 3,
    };

    fn web() -> NoteContext {
        NoteContext {
            name: "Web Prod é".to_owned(),
            host: Some("web.lab".to_owned()),
            port: Some(22),
            user: Some("admin".to_owned()),
            group: Some("Prod".to_owned()),
            protocol: Some("SSH".to_owned()),
        }
    }

    #[test]
    fn a_name_becomes_a_file_name_as_the_csharp_slugifies_it() {
        assert_eq!(
            slug("Working Note - Web Prod é!"),
            "working-note-web-prod-e"
        );
        assert_eq!(slug("  --  "), "");
    }

    #[test]
    fn a_working_note_is_named_after_when_and_what_with_the_server_said() {
        let note = draft(NoteTemplate::Blank, &web(), NOW, &labels());
        assert_eq!(
            note.path,
            PathBuf::from("20261005-140703-working-note-web-prod-e.md")
        );
        assert!(note.content.starts_with(
            "# Working Note - Web Prod é\n\n> created 2026-10-05 14:07 | display Web Prod é | \
             host web.lab | port 22 | user admin | group Prod | type SSH\n"
        ));
        assert!(note.content.contains("## Commands\n\n```bash\n\n```\n"));
    }

    #[test]
    fn the_days_note_is_one_file_a_day_and_an_incident_starts_its_timeline() {
        let daily = draft(NoteTemplate::Daily, &web(), NOW, &labels());
        assert_eq!(
            daily.path,
            Path::new("daily").join("2026").join("2026-10-05.md")
        );
        assert!(
            daily
                .content
                .starts_with("# Daily Note - 2026-10-05 - Web Prod é\n")
        );
        let incident = draft(
            NoteTemplate::Incident,
            &NoteContext::default(),
            NOW,
            &labels(),
        );
        assert!(incident.content.starts_with("# Incident Report\n"));
        assert!(
            incident
                .content
                .contains("## Timeline\n\n- 14:07 - Incident started\n")
        );
        let procedure = draft(NoteTemplate::Procedure, &web(), NOW, &labels());
        assert!(procedure.content.contains("## Steps\n\n1. \n"));
    }

    #[tokio::test]
    async fn a_note_there_already_is_not_written_over() {
        let dir = tempfile::tempdir().expect("dir");
        let daily = draft(NoteTemplate::Daily, &web(), NOW, &labels());
        let path = dir.path().join(&daily.path);
        std::fs::create_dir_all(path.parent().expect("folder")).expect("folder");
        std::fs::write(&path, "kept").expect("written");
        // No editor to open it with here: what matters is the file.
        let _ = open(dir.path().to_owned(), daily, "/no/such/editor".to_owned()).await;
        assert_eq!(std::fs::read_to_string(&path).expect("read"), "kept");
    }
}
