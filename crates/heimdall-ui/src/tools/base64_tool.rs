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

//! The Base64 tool, as the C# `Base64ToolView` and `Base64ToolViewModel`: text, or a file of
//! at most 5 MB, encoded, standard or URL-safe; Base64 decoded to text, or to a file saved
//! where the user says; the output copied.

use std::path::PathBuf;

use heimdall_app::TabId;
use heimdall_core::tools::base64_codec;
use iced::keyboard::{Key, key::Named};
use iced::widget::text_editor::{Action, Binding, Content, KeyPress, Status};
use iced::widget::{checkbox, column, container, row, text};
use iced::{Element, Length, Task, window};

use super::{CopySlot, ToolMessage, ToolPane};
use crate::file_dialog::{self, FileDialog};
use crate::i18n::fl;
use crate::shell::{Message, main_window_task};
use crate::styles;
use crate::tokens::{font_size, spacing};

/// The largest file encoded, as the C# `MaxFileSizeBytes`: 5 MB.
pub const MAX_FILE_BYTES: u64 = 5 * 1024 * 1024;

/// Room between the input's label row and the boxes, as the C# `MarginStackItem`.
const STACK_GAP: f32 = 8.0;

/// The file the user picks in a file dialog, once it closes; `None` when cancelled.
type Pick = file_dialog::Pick<PathBuf>;

/// What the status line says, as the C# `Base64StatusKind`.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
enum Status64 {
    #[default]
    None,
    Encoded(usize),
    Decoded(usize),
    Saved(String),
    Invalid,
    Error(String),
    TooLarge,
}

impl Status64 {
    /// What it says, and whether it is an error, drawn in the error colour.
    fn said(&self) -> Option<(String, bool)> {
        let said = match self {
            Self::None => return None,
            Self::Encoded(bytes) => {
                let count = *bytes;
                (fl!("ui-tool-base64-status-encoded", count = count), false)
            }
            Self::Decoded(bytes) => {
                let count = *bytes;
                (fl!("ui-tool-base64-status-decoded", count = count), false)
            }
            Self::Saved(path) => {
                let path = path.as_str();
                (fl!("ui-tool-base64-status-saved", path = path), false)
            }
            Self::Invalid => (fl!("ui-tool-base64-status-invalid"), true),
            Self::Error(reason) => {
                let error = reason.as_str();
                (fl!("ui-tool-base64-status-error", error = error), true)
            }
            Self::TooLarge => (fl!("ui-tool-base64-too-large"), true),
        };
        Some(said)
    }
}

/// A file read for file mode, or why not.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Loaded {
    /// Its name and its bytes.
    File(String, Vec<u8>),
    /// Larger than [`MAX_FILE_BYTES`].
    TooLarge,
    /// Why it could not be read.
    Failed(String),
}

/// What the Base64 tool is asked.
#[derive(Debug, Clone)]
pub enum Base64Message {
    /// Something done in the input box.
    Input(Action),
    /// Something done in the output box, which does not change it.
    Output(Action),
    /// URL-safe Base64 on or off.
    UrlSafe(bool),
    /// File mode on or off.
    FileMode(bool),
    /// Encode, as the C# "Encode" button and Ctrl+Enter.
    Encode,
    /// Decode, as the C# "Decode" button and Ctrl+Shift+Enter.
    Decode,
    /// Copy the output.
    Copy,
    /// Pick a file to encode.
    Browse,
    /// The file picked was read, or not; `None` when none was picked.
    Loaded(Option<Loaded>),
    /// Where the decoded bytes go, picked in the save dialog; `None` when cancelled.
    SaveTo(Option<PathBuf>),
    /// The decoded bytes were saved at this path, or why not.
    Saved(Result<String, String>),
}

/// What an update asks of its tab.
#[derive(Debug, PartialEq, Eq)]
pub enum Outcome {
    /// Nothing more.
    Done,
    /// This text copied by the output's copy button.
    Copy(String),
    /// The open dialog for a file to encode.
    Browse,
    /// The save dialog for the decoded bytes.
    AskSave,
    /// These bytes written at this path.
    Save(PathBuf, Vec<u8>),
}

impl Outcome {
    /// What tab `tab` runs for it, its dialogs over the window `main`; a copy is its tab's.
    pub fn task(self, tab: TabId, main: Option<window::Id>) -> Task<Message> {
        match self {
            Self::Done | Self::Copy(_) => Task::none(),
            Self::Browse => pick_file(tab, main),
            Self::AskSave => pick_save(tab, main),
            Self::Save(path, bytes) => save_task(tab, path, bytes),
        }
    }
}

/// The Base64 tool's state, as the C# view model's.
#[derive(Debug, Default)]
pub struct Base64Pane {
    input: Content,
    output: Content,
    output_text: String,
    url_safe: bool,
    file_mode: bool,
    /// The file loaded in file mode: the input then says so and is read-only.
    file: Option<Vec<u8>>,
    /// The bytes of the last decode, saved in file mode.
    decoded: Option<Vec<u8>>,
    status: Status64,
    /// The output is shown, rather than the empty state.
    results: bool,
}

impl Base64Pane {
    /// Applies `message`; what is then to be done.
    pub fn update(&mut self, message: Base64Message) -> Outcome {
        match message {
            Base64Message::Input(action) => self.input_action(action),
            Base64Message::Output(action) => super::read_only(&mut self.output, action),
            Base64Message::UrlSafe(on) => self.url_safe = on,
            Base64Message::FileMode(on) => self.file_mode(on),
            Base64Message::Encode => self.encode(),
            Base64Message::Decode => {
                self.decode();
                // As the C# decode button in file mode: the bytes saved where the user says.
                if self.file_mode && self.decoded.is_some() {
                    return Outcome::AskSave;
                }
            }
            Base64Message::Copy => return Outcome::Copy(self.output_text.clone()),
            Base64Message::Browse => return Outcome::Browse,
            Base64Message::Loaded(Some(loaded)) => self.loaded(loaded),
            Base64Message::SaveTo(Some(path)) => {
                if let Some(bytes) = self.decoded.clone() {
                    return Outcome::Save(path, bytes);
                }
            }
            Base64Message::Saved(Ok(path)) => self.status = Status64::Saved(path),
            Base64Message::Saved(Err(error)) => self.status = Status64::Error(error),
            Base64Message::Loaded(None) | Base64Message::SaveTo(None) => {}
        }
        Outcome::Done
    }

    /// An action in the input: a change clears the output and the status, as the C#
    /// `OnInputTextChangedFromView`; a file loaded leaves it read-only.
    fn input_action(&mut self, action: Action) {
        if action.is_edit() && self.file.is_some() {
            return;
        }
        let edit = action.is_edit();
        self.input.perform(action);
        if edit {
            self.clear_output();
            self.status = Status64::None;
        }
    }

    /// File mode turned on or off; off, everything is cleared, as the C#
    /// `OnIsFileModeChanged`.
    fn file_mode(&mut self, on: bool) {
        self.file_mode = on;
        if on {
            return;
        }
        self.file = None;
        self.input = Content::new();
        self.clear_output();
        self.status = Status64::None;
    }

    /// The output emptied and hidden behind the empty state, the decoded bytes forgotten.
    fn clear_output(&mut self) {
        self.output_text.clear();
        self.output = Content::new();
        self.decoded = None;
        self.results = false;
    }

    /// The output shown: `output`.
    fn show_output(&mut self, output: String) {
        self.output = Content::with_text(&output);
        self.output_text = output;
        self.results = true;
    }

    /// Encodes the file loaded in file mode, else the input's UTF-8, as the C#
    /// `EncodeCoreAsync`.
    fn encode(&mut self) {
        let typed;
        let data: &[u8] = if let (Some(file), true) = (&self.file, self.file_mode) {
            file
        } else {
            typed = super::box_text(&self.input);
            typed.as_bytes()
        };
        let encoded = base64_codec::encode(data, self.url_safe);
        let count = data.len();
        self.decoded = None;
        self.show_output(encoded);
        self.status = Status64::Encoded(count);
    }

    /// Decodes the input, trimmed, as the C# `DecodeAsync`: shown as UTF-8 text, or, in file
    /// mode, kept to be saved; what is not Base64 said.
    fn decode(&mut self) {
        let typed = super::box_text(&self.input);
        let Ok(bytes) = base64_codec::decode(typed.trim(), self.url_safe) else {
            self.clear_output();
            self.status = Status64::Invalid;
            return;
        };
        if self.file_mode {
            self.decoded = Some(bytes);
            return;
        }
        let count = bytes.len();
        self.show_output(String::from_utf8_lossy(&bytes).into_owned());
        self.decoded = Some(bytes);
        self.status = Status64::Decoded(count);
    }

    /// A file read for file mode: the input says what was loaded and becomes read-only, as
    /// the C# `LoadFileAsync`.
    fn loaded(&mut self, loaded: Loaded) {
        match loaded {
            Loaded::File(name, bytes) => {
                let said = fl!(
                    "ui-tool-base64-file-loaded",
                    name = name.as_str(),
                    count = bytes.len()
                );
                self.input = Content::with_text(&said);
                self.file = Some(bytes);
            }
            Loaded::TooLarge => self.status = Status64::TooLarge,
            Loaded::Failed(error) => self.status = Status64::Error(error),
        }
    }

    /// The tool's page, as the C# `Base64ToolView.xaml`: the input with its options, Encode
    /// and Decode, the output or its empty state, the status and the copy button.
    pub fn view<'a>(&'a self, tab: TabId, state: &ToolPane) -> Element<'a, Message> {
        let send = move |message| Message::Tool(tab, ToolMessage::Base64(message));
        let options = row![
            text(fl!("ui-tool-base64-input"))
                .size(font_size::BODY)
                .style(text::secondary)
                .width(Length::Fill),
            checkbox(self.url_safe)
                .label(fl!("ui-tool-base64-url-safe"))
                .on_toggle(move |on| send(Base64Message::UrlSafe(on)))
                .style(styles::checkbox)
                .text_size(font_size::BODY),
            checkbox(self.file_mode)
                .label(fl!("ui-tool-base64-file-mode"))
                .on_toggle(move |on| send(Base64Message::FileMode(on)))
                .style(styles::checkbox)
                .text_size(font_size::BODY),
        ]
        .spacing(spacing::MD + spacing::XS)
        .align_y(iced::Alignment::Center);
        let input = super::text_box(&self.input, Some(fl!("ui-tool-base64-placeholder")))
            .height(Length::Fill)
            .on_action(move |action| send(Base64Message::Input(action)))
            .key_binding(move |press| binding(press, send));
        let actions = row![
            super::action_button(
                fl!("ui-tool-base64-encode"),
                true,
                Some(send(Base64Message::Encode))
            ),
            super::action_button(
                fl!("ui-tool-base64-decode"),
                false,
                Some(send(Base64Message::Decode))
            ),
        ]
        .spacing(spacing::SM);
        let output: Element<'a, Message> = if self.results {
            super::text_box(&self.output, None)
                .height(Length::Fill)
                .on_action(move |action| send(Base64Message::Output(action)))
                .into()
        } else {
            super::empty_state(fl!("ui-tool-base64-empty"))
        };
        let status = self.status.said().map(|(said, error)| {
            text(said).size(font_size::CAPTION).style(if error {
                text::danger
            } else {
                text::secondary
            })
        });
        let footer = row![container(status).width(Length::Fill)]
            .push(self.file_mode.then(|| {
                super::action_button(
                    fl!("ui-tool-base64-browse"),
                    false,
                    Some(send(Base64Message::Browse)),
                )
            }))
            .push(super::copy_button(
                fl!("ui-tool-base64-copy"),
                state.copied(CopySlot::Base64Output),
                send(Base64Message::Copy),
                super::COPY_PADDING,
            ))
            .spacing(spacing::SM)
            .align_y(iced::Alignment::Center);
        super::tool_body(
            column![
                options,
                input,
                container(actions).center_x(Length::Fill),
                super::field_label(fl!("ui-tool-base64-output")),
                output,
                footer,
            ]
            .spacing(STACK_GAP),
        )
    }
}

/// The input's keys, as the C# `OnInputPreviewKeyDown`: Ctrl+Enter encodes, Ctrl+Shift+Enter
/// decodes; the rest as any text box.
fn binding(press: KeyPress, send: impl Fn(Base64Message) -> Message) -> Option<Binding<Message>> {
    if matches!(press.status, Status::Focused { .. })
        && press.key == Key::Named(Named::Enter)
        && press.modifiers.control()
        && !press.modifiers.alt()
    {
        return Some(Binding::Custom(send(if press.modifiers.shift() {
            Base64Message::Decode
        } else {
            Base64Message::Encode
        })));
    }
    Binding::from_key_press(press)
}

/// The open dialog for a file to encode, over the window `main`, then the file read, as the
/// C# `OnBrowseFileClick` and `LoadFileAsync`.
fn pick_file(tab: TabId, main: Option<window::Id>) -> Task<Message> {
    let title = fl!("ui-tool-base64-open-title");
    let done = move |loaded| Message::Tool(tab, ToolMessage::Base64(Base64Message::Loaded(loaded)));
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
        pick.then(move |pick| Task::perform(read_picked(pick), done))
    })
}

/// The file picked read, at most [`MAX_FILE_BYTES`], or why no dialog could be shown;
/// `None` when none was picked.
async fn read_picked(pick: Pick) -> Option<Loaded> {
    match pick.await {
        Ok(Some(path)) => Some(read_file(path).await),
        Ok(None) => None,
        Err(unavailable) => Some(Loaded::Failed(unavailable.message())),
    }
}

/// The file at `path` read for file mode, as the C# `Base64ToolService.LoadFileAsync`: its
/// name and bytes, refused past [`MAX_FILE_BYTES`].
pub async fn read_file(path: PathBuf) -> Loaded {
    let name = path
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default();
    match tokio::fs::metadata(&path).await {
        Ok(metadata) if metadata.len() > MAX_FILE_BYTES => Loaded::TooLarge,
        Ok(_) => match tokio::fs::read(&path).await {
            Ok(bytes) => Loaded::File(name, bytes),
            Err(error) => Loaded::Failed(error.to_string()),
        },
        Err(error) => Loaded::Failed(error.to_string()),
    }
}

/// The save dialog for the decoded bytes, over the window `main`, as the C# decode in file
/// mode: the place picked is sent to the tab, which writes them there.
fn pick_save(tab: TabId, main: Option<window::Id>) -> Task<Message> {
    let title = fl!("ui-tool-base64-save-title");
    main_window_task(main).then(move |id| {
        let title = title.clone();
        let pick = match id {
            Some(id) => window::run(id, move |window| {
                FileDialog::new()
                    .set_title(title.clone())
                    .set_parent(window)
                    .save_file()
            }),
            None => Task::done(FileDialog::new().set_title(title).save_file()),
        };
        pick.then(move |pick: Pick| {
            Task::perform(pick, move |picked| match picked {
                Ok(path) => Message::Tool(tab, ToolMessage::Base64(Base64Message::SaveTo(path))),
                Err(unavailable) => file_dialog::failed(&unavailable),
            })
        })
    })
}

/// Writes `bytes` at `path`, as the C# `SaveFileAsync`: the path written to, or why not.
pub async fn write_file(path: PathBuf, bytes: Vec<u8>) -> Result<String, String> {
    tokio::fs::write(&path, bytes)
        .await
        .map(|()| path.display().to_string())
        .map_err(|error| error.to_string())
}

/// The save of tab `tab`'s decoded `bytes` at `path`, picked; its outcome sent to the tab.
fn save_task(tab: TabId, path: PathBuf, bytes: Vec<u8>) -> Task<Message> {
    Task::perform(write_file(path, bytes), move |saved| {
        Message::Tool(tab, ToolMessage::Base64(Base64Message::Saved(saved)))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn typed(pane: &mut Base64Pane, typed: &str) {
        pane.update(Base64Message::Input(Action::Edit(
            iced::widget::text_editor::Edit::Paste(std::sync::Arc::new(typed.to_owned())),
        )));
    }

    #[test]
    fn encoding_shows_the_utf8_input_in_base64_and_counts_its_bytes() {
        let mut pane = Base64Pane::default();
        typed(&mut pane, "abc");
        let _ = pane.update(Base64Message::Encode);
        assert_eq!(pane.output_text, "YWJj");
        assert!(pane.results);
        assert_eq!(pane.status, Status64::Encoded(3));
    }

    #[test]
    fn decoding_shows_the_text_and_an_invalid_input_says_so() {
        let mut pane = Base64Pane::default();
        typed(&mut pane, "  Zm9vYmFy\n");
        let _ = pane.update(Base64Message::Decode);
        assert_eq!(pane.output_text, "foobar");
        assert_eq!(pane.status, Status64::Decoded(6));
        typed(&mut pane, "!");
        assert!(!pane.results, "a change clears the output");
        assert_eq!(pane.status, Status64::None);
        let _ = pane.update(Base64Message::Decode);
        assert_eq!(pane.status, Status64::Invalid);
        assert!(pane.output_text.is_empty());
    }

    #[test]
    fn file_mode_encodes_the_file_and_keeps_its_input_read_only_until_turned_off() {
        let mut pane = Base64Pane::default();
        let _ = pane.update(Base64Message::FileMode(true));
        let _ = pane.update(Base64Message::Loaded(Some(Loaded::File(
            "a.bin".to_owned(),
            vec![1, 2, 3],
        ))));
        let said = pane.input.text();
        assert!(said.contains("a.bin"), "{said}");
        typed(&mut pane, "x");
        assert_eq!(pane.input.text(), said, "read-only once loaded");
        let _ = pane.update(Base64Message::Encode);
        assert_eq!(pane.output_text, "AQID");
        let _ = pane.update(Base64Message::FileMode(false));
        assert!(pane.file.is_none() && !pane.results && pane.input.text().trim().is_empty());
    }

    #[test]
    fn decoding_in_file_mode_keeps_the_bytes_without_showing_them() {
        let mut pane = Base64Pane::default();
        let _ = pane.update(Base64Message::FileMode(true));
        typed(&mut pane, "AQID");
        let _ = pane.update(Base64Message::Decode);
        assert_eq!(pane.decoded.as_deref(), Some(&[1_u8, 2, 3][..]));
        assert!(!pane.results);
    }

    #[test]
    fn a_file_too_large_is_refused_and_a_failure_said() {
        let mut pane = Base64Pane::default();
        let _ = pane.update(Base64Message::Loaded(Some(Loaded::TooLarge)));
        assert_eq!(pane.status, Status64::TooLarge);
        let _ = pane.update(Base64Message::Saved(Err("denied".to_owned())));
        assert_eq!(pane.status, Status64::Error("denied".to_owned()));
    }

    #[test]
    fn a_file_is_read_up_to_the_csharp_limit() {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("runtime");
        let dir = tempfile::tempdir().expect("dir");
        let small = dir.path().join("sample.bin");
        std::fs::write(&small, [1, 2, 3]).expect("written");
        assert_eq!(
            runtime.block_on(read_file(small)),
            Loaded::File("sample.bin".to_owned(), vec![1, 2, 3])
        );
        let large = dir.path().join("large.bin");
        let file = std::fs::File::create(&large).expect("created");
        file.set_len(MAX_FILE_BYTES + 1).expect("sized");
        assert_eq!(runtime.block_on(read_file(large)), Loaded::TooLarge);
        assert!(matches!(
            runtime.block_on(read_file(dir.path().join("missing.bin"))),
            Loaded::Failed(_)
        ));
        let saved = dir.path().join("saved.bin");
        assert!(
            runtime
                .block_on(write_file(saved.clone(), vec![9, 8, 7]))
                .is_ok()
        );
        assert_eq!(std::fs::read(saved).expect("read"), vec![9, 8, 7]);
    }
}
