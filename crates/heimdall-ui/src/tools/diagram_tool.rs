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

//! The diagram editor: a diagram's source text (Mermaid, Graphviz DOT, PlantUML or ASCII
//! art), started from a template or opened from a file, edited, saved and copied. Unlike
//! the C# `DiagramEditorView`, which embeds the draw.io editor, nothing is drawn here: the
//! source is for whatever renders its format. A template or a file never replaces changes
//! not saved without asking first, as the C# `ToolDiagramUnsavedMessage`.

use std::path::PathBuf;

use heimdall_app::TabId;
use heimdall_core::tools::diagram_editor::{
    self, DIAGRAM_TEMPLATES, DiagramFormat, MAX_FILE_BYTES, TemplateKind,
};
use iced::widget::text_editor::{Action, Content};
use iced::widget::{Row, button, column, row, text};
use iced::{Element, Length, Task, window};

use super::{CopySlot, ToolMessage, ToolPane};
use crate::file_dialog::{self, FileDialog};
use crate::i18n::fl;
use crate::shell::{Message, main_window_task};
use crate::styles;
use crate::tokens::{font_size, spacing};

/// What the diagram editor is asked.
#[derive(Debug, Clone)]
pub enum DiagramMessage {
    /// Start from this template.
    Template(TemplateKind),
    /// Open a file, as the C# File > Open.
    Open,
    /// Replace the changes not saved, as asked.
    ConfirmReplace,
    /// Keep them.
    CancelReplace,
    /// The file picked was read, or not; `None` when none was picked.
    Opened(Option<Result<(PathBuf, String), FileError>>),
    /// Save to the diagram's file, or ask where when it has none, as the C# Save.
    Save,
    /// Ask where to save, as the C# Save As.
    SaveAs,
    /// Where to save, picked; `None` when cancelled.
    SaveTo(Option<PathBuf>),
    /// The text saved to this file, or why not.
    Saved(Result<PathBuf, FileError>),
    /// Something done in the editor.
    Edit(Action),
    /// Copy the source.
    Copy,
}

/// Why a file could not be opened or saved.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FileError {
    /// Larger than [`MAX_FILE_BYTES`].
    TooLarge,
    /// What the system said.
    Failed(String),
}

/// What an update asks of its tab.
#[derive(Debug, PartialEq, Eq)]
pub enum Outcome {
    /// Nothing more.
    Done,
    /// This text copied.
    Copy(String),
    /// The open dialog.
    Open,
    /// The save dialog, a file of this format offered.
    AskSave(DiagramFormat),
    /// This text written to this file.
    Save(PathBuf, String),
}

impl Outcome {
    /// What tab `tab` runs for it, its dialogs over the window `main`.
    pub fn task(self, tab: TabId, main: Option<window::Id>) -> Task<Message> {
        match self {
            Self::Done | Self::Copy(_) => Task::none(),
            Self::Open => pick_file(tab, main),
            Self::AskSave(format) => pick_save(tab, main, format),
            Self::Save(path, source) => Task::perform(write_file(path, source), move |saved| {
                Message::Tool(tab, ToolMessage::Diagram(DiagramMessage::Saved(saved)))
            }),
        }
    }
}

/// What would replace the diagram once confirmed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Replace {
    Template(TemplateKind),
    Open,
}

/// What the status line says.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Status {
    None,
    Saved(PathBuf),
    OpenFailed(FileError),
    SaveFailed(FileError),
}

/// The diagram editor's state.
#[derive(Debug)]
pub struct DiagramPane {
    editor: Content,
    format: DiagramFormat,
    /// The file the diagram was opened from or saved to.
    file: Option<PathBuf>,
    /// Changes not saved.
    dirty: bool,
    /// A replacement waiting for the user to confirm it.
    pending: Option<Replace>,
    status: Status,
}

impl Default for DiagramPane {
    /// A new tab's state: the first template, nothing changed.
    fn default() -> Self {
        let first = &DIAGRAM_TEMPLATES[0];
        Self {
            editor: Content::with_text(first.content),
            format: first.format,
            file: None,
            dirty: false,
            pending: None,
            status: Status::None,
        }
    }
}

impl DiagramPane {
    /// Applies `message`; what is then to be done.
    pub fn update(&mut self, message: DiagramMessage) -> Outcome {
        match message {
            DiagramMessage::Template(kind) => return self.replace(Replace::Template(kind)),
            DiagramMessage::Open => return self.replace(Replace::Open),
            DiagramMessage::ConfirmReplace => {
                if let Some(replace) = self.pending.take() {
                    return self.apply(replace);
                }
            }
            DiagramMessage::CancelReplace => self.pending = None,
            DiagramMessage::Opened(Some(Ok((path, source)))) => {
                self.editor = Content::with_text(&source);
                self.format = DiagramFormat::of_path(&path);
                self.file = Some(path);
                self.dirty = false;
                self.status = Status::None;
            }
            DiagramMessage::Opened(Some(Err(error))) => self.status = Status::OpenFailed(error),
            DiagramMessage::Opened(None) | DiagramMessage::SaveTo(None) => {}
            DiagramMessage::Save => {
                return match &self.file {
                    Some(path) => Outcome::Save(path.clone(), self.editor.text()),
                    None => Outcome::AskSave(self.format),
                };
            }
            DiagramMessage::SaveAs => return Outcome::AskSave(self.format),
            DiagramMessage::SaveTo(Some(path)) => {
                return Outcome::Save(path, self.editor.text());
            }
            DiagramMessage::Saved(Ok(path)) => {
                self.format = DiagramFormat::of_path(&path);
                self.file = Some(path.clone());
                self.dirty = false;
                self.status = Status::Saved(path);
            }
            DiagramMessage::Saved(Err(error)) => self.status = Status::SaveFailed(error),
            DiagramMessage::Edit(action) => {
                if action.is_edit() {
                    self.dirty = true;
                }
                self.editor.perform(action);
            }
            DiagramMessage::Copy => return Outcome::Copy(self.editor.text()),
        }
        Outcome::Done
    }

    /// `replace` done at once when nothing would be lost, else asked first.
    fn replace(&mut self, replace: Replace) -> Outcome {
        if self.dirty {
            self.pending = Some(replace);
            Outcome::Done
        } else {
            self.apply(replace)
        }
    }

    /// `replace` done.
    fn apply(&mut self, replace: Replace) -> Outcome {
        match replace {
            Replace::Template(kind) => {
                let template = diagram_editor::template(kind);
                self.editor = Content::with_text(template.content);
                self.format = template.format;
                self.file = None;
                self.dirty = false;
                self.status = Status::None;
                Outcome::Done
            }
            Replace::Open => Outcome::Open,
        }
    }

    /// The name the header shows: the file's, or untitled; marked when changed.
    fn document_name(&self) -> String {
        let name = self
            .file
            .as_ref()
            .and_then(|path| path.file_name())
            .map_or_else(
                || fl!("ui-tool-diagram-untitled"),
                |name| name.to_string_lossy().into_owned(),
            );
        if self.dirty {
            fl!("ui-tool-diagram-modified", name = name)
        } else {
            name
        }
    }

    /// What the status line says.
    fn status_text(&self) -> Option<(String, bool)> {
        let file_error = |error: &FileError| match error {
            FileError::TooLarge => fl!("ui-tool-diagram-error-too-large"),
            FileError::Failed(reason) => reason.clone(),
        };
        match &self.status {
            Status::None => None,
            Status::Saved(path) => {
                let path = path.display().to_string();
                Some((fl!("ui-tool-diagram-saved", path = path), false))
            }
            Status::OpenFailed(error) => {
                let error = file_error(error);
                Some((fl!("ui-tool-diagram-error-open", error = error), true))
            }
            Status::SaveFailed(error) => {
                let error = file_error(error);
                Some((fl!("ui-tool-diagram-error-save", error = error), true))
            }
        }
    }

    /// The tool's page: the templates, the file actions, the document's name and format,
    /// a confirmation when one is asked, and the source.
    pub fn view<'a>(&'a self, tab: TabId, state: &ToolPane) -> Element<'a, Message> {
        let send = move |message| Message::Tool(tab, ToolMessage::Diagram(message));
        let templates = Row::with_children(DIAGRAM_TEMPLATES.iter().map(|template| {
            button(text(template_name(template.kind)).size(font_size::BODY))
                .padding(super::COPY_PADDING)
                .style(styles::secondary)
                .on_press(send(DiagramMessage::Template(template.kind)))
                .into()
        }))
        .spacing(spacing::SM)
        .wrap()
        .vertical_spacing(spacing::SM);
        let files = row![
            super::action_button(
                fl!("ui-tool-diagram-open"),
                false,
                Some(send(DiagramMessage::Open))
            ),
            super::action_button(
                fl!("ui-tool-diagram-save"),
                true,
                Some(send(DiagramMessage::Save))
            ),
            super::action_button(
                fl!("ui-tool-diagram-save-as"),
                false,
                Some(send(DiagramMessage::SaveAs))
            ),
            text(self.document_name()).size(font_size::BODY),
            text(format_name(self.format))
                .size(font_size::CAPTION)
                .style(text::secondary),
            iced::widget::space().width(Length::Fill),
            super::copy_button(
                fl!("ui-tool-diagram-copy"),
                state.copied(CopySlot::DiagramSource),
                send(DiagramMessage::Copy),
                super::COPY_PADDING,
            ),
        ]
        .spacing(spacing::SM)
        .align_y(iced::Alignment::Center);
        let confirm = self.pending.map(|_| {
            row![
                text(fl!("ui-tool-diagram-confirm-replace"))
                    .size(font_size::BODY)
                    .style(text::danger),
                super::action_button(
                    fl!("ui-tool-diagram-replace"),
                    true,
                    Some(send(DiagramMessage::ConfirmReplace))
                ),
                super::action_button(
                    fl!("ui-tool-diagram-keep"),
                    false,
                    Some(send(DiagramMessage::CancelReplace))
                ),
            ]
            .spacing(spacing::SM)
            .align_y(iced::Alignment::Center)
        });
        super::tool_body(
            column![
                super::field_label(fl!("ui-tool-diagram-templates")),
                templates,
                files
            ]
            .push(confirm)
            .push(super::status_line(self.status_text()))
            .push(
                super::text_box(&self.editor, Some(fl!("ui-tool-diagram-placeholder")))
                    .height(Length::Fill)
                    .on_action(move |action| send(DiagramMessage::Edit(action))),
            )
            .spacing(spacing::SM),
        )
    }
}

/// The name of template `kind`, in the window's language.
fn template_name(kind: TemplateKind) -> String {
    match kind {
        TemplateKind::Perimeter => fl!("ui-tool-diagram-template-perimeter"),
        TemplateKind::CloudVpc => fl!("ui-tool-diagram-template-cloud"),
        TemplateKind::AsciiTopology => fl!("ui-tool-diagram-template-ascii"),
        TemplateKind::ActiveDirectory => fl!("ui-tool-diagram-template-ad"),
    }
}

/// The name of `format`.
fn format_name(format: DiagramFormat) -> String {
    match format {
        DiagramFormat::Mermaid => fl!("ui-tool-diagram-format-mermaid"),
        DiagramFormat::Dot => fl!("ui-tool-diagram-format-dot"),
        DiagramFormat::PlantUml => fl!("ui-tool-diagram-format-plantuml"),
        DiagramFormat::Ascii => fl!("ui-tool-diagram-format-ascii"),
    }
}

/// The open dialog over the window `main`, then the file picked read.
fn pick_file(tab: TabId, main: Option<window::Id>) -> Task<Message> {
    let title = fl!("ui-tool-diagram-open-title");
    let done =
        move |opened| Message::Tool(tab, ToolMessage::Diagram(DiagramMessage::Opened(opened)));
    main_window_task(main).then(move |id| {
        let title = title.clone();
        let pick = match id {
            Some(id) => window::run(id, move |window| {
                FileDialog::new()
                    .set_title(title.clone())
                    .set_parent(window)
                    .pick_file()
            }),
            None => Task::done(FileDialog::new().set_title(title).pick_file()),
        };
        pick.then(move |pick: file_dialog::Pick<PathBuf>| {
            Task::perform(
                async move {
                    match pick.await {
                        Ok(Some(path)) => Some(read_file(path).await),
                        Ok(None) => None,
                        Err(unavailable) => Some(Err(FileError::Failed(unavailable.message()))),
                    }
                },
                done,
            )
        })
    })
}

/// The text of the file at `path`, refused past [`MAX_FILE_BYTES`].
pub async fn read_file(path: PathBuf) -> Result<(PathBuf, String), FileError> {
    let failed = |error: std::io::Error| FileError::Failed(error.to_string());
    let metadata = tokio::fs::metadata(&path).await.map_err(failed)?;
    if metadata.len() > MAX_FILE_BYTES {
        return Err(FileError::TooLarge);
    }
    let bytes = tokio::fs::read(&path).await.map_err(failed)?;
    let source = String::from_utf8_lossy(&bytes).into_owned();
    Ok((path, source))
}

/// The save dialog over the window `main`, a file of `format` offered.
fn pick_save(tab: TabId, main: Option<window::Id>, format: DiagramFormat) -> Task<Message> {
    let title = fl!("ui-tool-diagram-save-title");
    let name = format!(
        "{}.{}",
        fl!("ui-tool-diagram-default-file-name"),
        format.extension()
    );
    main_window_task(main).then(move |id| {
        let dialog = FileDialog::new()
            .set_title(title.clone())
            .set_file_name(name.clone());
        let pick = match id {
            Some(id) => window::run(id, move |window| dialog.set_parent(window).save_file()),
            None => Task::done(dialog.save_file()),
        };
        pick.then(move |pick: file_dialog::Pick<PathBuf>| {
            Task::perform(pick, move |picked| match picked {
                Ok(path) => Message::Tool(tab, ToolMessage::Diagram(DiagramMessage::SaveTo(path))),
                Err(unavailable) => file_dialog::failed(&unavailable),
            })
        })
    })
}

/// Writes `source` to `path`: the path written, or why not.
pub async fn write_file(path: PathBuf, source: String) -> Result<PathBuf, FileError> {
    tokio::fs::write(&path, source)
        .await
        .map(|()| path)
        .map_err(|error| FileError::Failed(error.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn typed(pane: &mut DiagramPane, typed: &str) {
        pane.update(DiagramMessage::Edit(Action::Edit(
            iced::widget::text_editor::Edit::Paste(std::sync::Arc::new(typed.to_owned())),
        )));
    }

    #[test]
    fn a_template_replaces_an_untouched_diagram_at_once() {
        let mut pane = DiagramPane::default();
        assert_eq!(
            pane.update(DiagramMessage::Template(TemplateKind::AsciiTopology)),
            Outcome::Done
        );
        assert_eq!(pane.format, DiagramFormat::Ascii);
        assert_eq!(
            pane.editor.text(),
            diagram_editor::template(TemplateKind::AsciiTopology).content
        );
        assert_eq!(pane.pending, None);
    }

    #[test]
    fn a_template_or_a_file_never_replaces_edits_without_asking() {
        let mut pane = DiagramPane::default();
        typed(&mut pane, "mine ");
        let edited = pane.editor.text();
        pane.update(DiagramMessage::Template(TemplateKind::CloudVpc));
        assert_eq!(
            pane.pending,
            Some(Replace::Template(TemplateKind::CloudVpc))
        );
        assert_eq!(pane.editor.text(), edited, "kept until confirmed");
        pane.update(DiagramMessage::CancelReplace);
        assert_eq!(pane.editor.text(), edited);
        assert_eq!(pane.update(DiagramMessage::Open), Outcome::Done);
        assert_eq!(pane.update(DiagramMessage::ConfirmReplace), Outcome::Open);
        pane.update(DiagramMessage::Template(TemplateKind::CloudVpc));
        pane.update(DiagramMessage::ConfirmReplace);
        assert_eq!(
            pane.editor.text(),
            diagram_editor::template(TemplateKind::CloudVpc).content
        );
        assert!(!pane.dirty);
    }

    #[test]
    fn save_asks_where_once_then_writes_to_the_same_file() {
        let mut pane = DiagramPane::default();
        typed(&mut pane, "x");
        assert_eq!(
            pane.update(DiagramMessage::Save),
            Outcome::AskSave(DiagramFormat::Mermaid)
        );
        let path = PathBuf::from("net.dot");
        let Outcome::Save(to, source) = pane.update(DiagramMessage::SaveTo(Some(path.clone())))
        else {
            panic!("a save");
        };
        assert_eq!(to, path);
        assert!(source.starts_with('x'));
        pane.update(DiagramMessage::Saved(Ok(path.clone())));
        assert!(!pane.dirty);
        assert_eq!(pane.format, DiagramFormat::Dot);
        assert!(matches!(pane.update(DiagramMessage::Save), Outcome::Save(to, _) if to == path));
    }

    #[test]
    fn an_opened_file_is_shown_with_its_format() {
        let mut pane = DiagramPane::default();
        pane.update(DiagramMessage::Opened(Some(Ok((
            PathBuf::from("a.puml"),
            "@startuml\n@enduml".to_owned(),
        )))));
        assert_eq!(pane.format, DiagramFormat::PlantUml);
        assert_eq!(pane.editor.text(), "@startuml\n@enduml");
        pane.update(DiagramMessage::Opened(Some(Err(FileError::TooLarge))));
        assert_eq!(pane.status, Status::OpenFailed(FileError::TooLarge));
    }

    #[tokio::test]
    async fn a_file_is_written_then_read_back() {
        let dir = tempfile::tempdir().expect("temp dir");
        let path = dir.path().join("d.mmd");
        assert_eq!(
            write_file(path.clone(), "graph TD".to_owned()).await,
            Ok(path.clone())
        );
        assert_eq!(
            read_file(path.clone()).await,
            Ok((path, "graph TD".to_owned()))
        );
    }
}
