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

//! The hash generator, as the C# `HashGeneratorView` and `HashGeneratorViewModel`: the
//! UTF-8 of the text typed hashed by MD5, SHA-1, SHA-256, SHA-384, SHA-512 and SHA3-256 as
//! it is typed; or a file of at most 50 MB, browsed or dropped on the tab, read once away
//! from the window with its progress shown, the text box disabled until the file is
//! cleared; each digest copied or saved; a hash pasted checked against them all.

use std::future::Future;
use std::path::PathBuf;
use std::pin::Pin;

use heimdall_app::TabId;
use heimdall_core::tools::hash_computer::{
    self, Digests, FileDigests, HashAlgorithm, HashFileError, MAX_FILE_BYTES,
};
use iced::futures::SinkExt as _;
use iced::widget::text_editor::{Action, Content};
use iced::widget::{column, container, progress_bar, row, text, text_input};
use iced::{Alignment, Element, Length, Task, window};

use super::crypto_parts::{self, Tone};
use super::{CopySlot, ToolMessage, ToolPane};
use crate::i18n::fl;
use crate::shell::{Message, main_window_task};
use crate::styles;
use crate::tokens::{font_size, spacing};

/// Height of the text's box, as the C# `Height="120"`.
const INPUT_HEIGHT: f32 = 120.0;

/// Height of a digest's box: one line.
const DIGEST_HEIGHT: f32 = 30.0;

/// Width of a digest's name, as the C# `Width="72"`.
const NAME_WIDTH: f32 = 72.0;

/// Height of the file's progress bar, as the C# `ToolLoadingBarStyle`.
const PROGRESS_HEIGHT: f32 = 4.0;

/// Padding of the drop zone and of the results' card, as the C# `PaddingAlertBanner`.
const BANNER_PADDING: [f32; 2] = [8.0, 12.0];

/// Room above the drop zone and the results, as the C# `MarginSectionTop`.
const SECTION_GAP: f32 = 16.0;

/// The whole of a file read, in percent.
const ALL_READ: f32 = 100.0;

/// Progress reports in flight before the reading waits for the window.
const PROGRESS_BUFFER: usize = 16;

/// The pattern of the save dialog's text filter, as the C# `*.txt`.
const TEXT_PATTERN: &str = "*.txt";

/// The extension the save dialog's text filter keeps, and its default name ends with.
const TEXT_EXTENSION: &str = "txt";

/// The extension of the "all files" filters: anything.
const ANY_EXTENSION: &str = "*";

/// The file the user picks in a file dialog, once it closes; `None` when cancelled.
type Pick = Pin<Box<dyn Future<Output = Option<rfd::FileHandle>> + Send>>;

/// What the file line says, as the C# `FileStatusKind` (`HashGeneratorViewModel.cs:543-552`).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
enum FileStatus {
    #[default]
    None,
    Hashing,
    Hashed {
        name: String,
        size: u64,
    },
    TooLarge,
    NotFound,
    AccessDenied,
    Failed,
}

impl FileStatus {
    /// What it says, and whether it is an error, drawn in the error colour.
    fn said(&self) -> Option<(String, bool)> {
        Some(match self {
            Self::None => return None,
            Self::Hashing => (fl!("ui-tool-hash-hashing"), false),
            Self::Hashed { name, size } => (
                fl!(
                    "ui-tool-hash-file-status",
                    name = name.as_str(),
                    size = crate::texts::size(*size)
                ),
                false,
            ),
            Self::TooLarge => (
                fl!(
                    "ui-tool-hash-too-large",
                    size = crate::texts::size(MAX_FILE_BYTES)
                ),
                true,
            ),
            Self::NotFound => (fl!("ui-tool-hash-not-found"), true),
            Self::AccessDenied => (fl!("ui-tool-hash-access-denied"), true),
            Self::Failed => (fl!("ui-tool-hash-error"), true),
        })
    }
}

/// What the hash generator is asked.
#[derive(Debug, Clone)]
pub enum HashMessage {
    /// Something done in the text's box.
    Input(Action),
    /// Something done in a digest's box, by its row, which does not change it.
    Digest(usize, Action),
    /// The hash to check typed.
    Verify(String),
    /// Pick a file to hash, as the C# "Browse File".
    Browse,
    /// A file to hash, picked or dropped on the tab; `None` when none was picked.
    Picked(Option<PathBuf>),
    /// The file of reading `.0` is `.1` percent read.
    Progress(u64, f32),
    /// The file of reading `.0` was hashed, or why not.
    Hashed(u64, Result<FileDigests, HashFileError>),
    /// Forget the file, as the C# "Clear file".
    ClearFile,
    /// Copy a digest.
    Copy(HashAlgorithm),
    /// Save a digest to a file.
    Save(HashAlgorithm),
    /// Where a digest goes, picked in the save dialog; `None` when cancelled.
    SaveTo(HashAlgorithm, Option<PathBuf>),
}

/// What an update asks of its tab.
#[derive(Debug, PartialEq, Eq)]
pub enum Outcome {
    /// Nothing more.
    Done,
    /// This digest copied by its copy button.
    Copy(HashAlgorithm, String),
    /// The open dialog for a file to hash.
    Browse,
    /// The file at this path hashed, as reading number `.1`.
    Hash(PathBuf, u64),
    /// The save dialog for a digest.
    AskSave(HashAlgorithm),
    /// This text written at this path.
    Write(PathBuf, String),
}

impl Outcome {
    /// What tab `tab` runs for it, its dialogs over the window `main`; a copy is its tab's.
    pub fn task(self, tab: TabId, main: Option<window::Id>) -> Task<Message> {
        match self {
            Self::Done | Self::Copy(..) => Task::none(),
            Self::Browse => pick_file(tab, main),
            Self::Hash(path, run) => hash_task(tab, path, run),
            Self::AskSave(kind) => pick_save(tab, kind, main),
            Self::Write(path, digest) => Task::future(async move {
                // As the C#, nothing is said of the save; a failure is only logged.
                if let Err(error) = tokio::fs::write(&path, digest).await {
                    log::warn!("hash generator: a digest was not saved: {error}");
                }
            })
            .discard(),
        }
    }
}

/// The hash generator's state, as the C# view model's.
#[derive(Debug)]
pub struct HashPane {
    input: Content,
    /// A file is hashed or was: the text box is disabled until it is cleared.
    file_mode: bool,
    hashing: bool,
    /// The share of the file read, in percent.
    progress: f32,
    /// The reading under way: the reports of another are let go, as the C# cancels it.
    run: u64,
    /// The name of the file read or hashed last.
    file_name: String,
    digests: Digests,
    boxes: Vec<Content>,
    /// The bytes of the text hashed, said under the digests.
    byte_length: Option<usize>,
    file_status: FileStatus,
    /// The digests are shown, rather than the empty state.
    results: bool,
    verify: String,
}

impl Default for HashPane {
    /// A new tab's state: the empty state.
    fn default() -> Self {
        Self {
            input: Content::new(),
            file_mode: false,
            hashing: false,
            progress: 0.0,
            run: 0,
            file_name: String::new(),
            digests: Vec::new(),
            boxes: Vec::new(),
            byte_length: None,
            file_status: FileStatus::None,
            results: false,
            verify: String::new(),
        }
    }
}

impl HashPane {
    /// Applies `message`; what is then to be done.
    pub fn update(&mut self, message: HashMessage) -> Outcome {
        match message {
            HashMessage::Input(action) => {
                // As the C# `IsTextInputEnabled`: no typing while a file is the input.
                if self.file_mode && action.is_edit() {
                    return Outcome::Done;
                }
                let edit = action.is_edit();
                self.input.perform(action);
                if edit {
                    self.hash_text();
                }
            }
            HashMessage::Digest(row, action) => {
                if let Some(content) = self.boxes.get_mut(row) {
                    super::read_only(content, action);
                }
            }
            HashMessage::Verify(typed) => self.verify = typed,
            HashMessage::Browse => {
                if !self.hashing {
                    return Outcome::Browse;
                }
            }
            HashMessage::Picked(Some(path)) => return self.begin_file(path),
            HashMessage::Progress(run, share) => {
                if run == self.run && self.hashing {
                    self.progress = share;
                }
            }
            HashMessage::Hashed(run, hashed) => {
                if run == self.run {
                    self.file_hashed(hashed);
                }
            }
            HashMessage::ClearFile => self.clear_file(),
            HashMessage::Copy(kind) => {
                if let Some(digest) = self.digest(kind) {
                    return Outcome::Copy(kind, digest.to_owned());
                }
            }
            HashMessage::Save(kind) => {
                if self.digest(kind).is_some() {
                    return Outcome::AskSave(kind);
                }
            }
            HashMessage::SaveTo(kind, Some(path)) => {
                if let Some(digest) = self.digest(kind) {
                    return Outcome::Write(path, digest.to_owned());
                }
            }
            HashMessage::Picked(None) | HashMessage::SaveTo(_, None) => {}
        }
        Outcome::Done
    }

    /// The digest of `kind`, when there is one.
    fn digest(&self, kind: HashAlgorithm) -> Option<&str> {
        self.digests
            .iter()
            .find(|(each, digest)| *each == kind && !digest.is_empty())
            .map(|(_, digest)| digest.as_str())
    }

    /// The digests shown: `digests`, none when empty.
    fn show(&mut self, digests: Digests) {
        self.boxes = digests
            .iter()
            .map(|(_, digest)| Content::with_text(digest))
            .collect();
        self.digests = digests;
    }

    /// The text's UTF-8 hashed, as the C# `ComputeTextAsync`
    /// (`HashGeneratorViewModel.cs:313-375`): nothing typed shows the empty state.
    fn hash_text(&mut self) {
        self.file_status = FileStatus::None;
        let typed = super::box_text(&self.input);
        if typed.is_empty() {
            self.show(Vec::new());
            self.byte_length = None;
            self.results = false;
            return;
        }
        self.show(hash_computer::compute_all(typed.as_bytes()));
        self.byte_length = Some(typed.len());
        self.results = true;
    }

    /// A file's hashing begun, as the C# `BeginFileHash` and `HashFileAsync`
    /// (`HashGeneratorViewModel.cs:109-141`): the text cleared, the digests forgotten, the
    /// progress shown; refused while another is read.
    fn begin_file(&mut self, path: PathBuf) -> Outcome {
        if self.hashing {
            return Outcome::Done;
        }
        self.run += 1;
        self.file_name = path
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default();
        self.input = Content::new();
        self.show(Vec::new());
        self.byte_length = None;
        self.file_status = FileStatus::Hashing;
        self.progress = 0.0;
        self.hashing = true;
        self.file_mode = true;
        self.results = false;
        Outcome::Hash(path, self.run)
    }

    /// A file's reading ended, as the C# `HashFileAsync`'s outcomes
    /// (`HashGeneratorViewModel.cs:143-220`): a file missing or too large leaves file mode,
    /// one not readable stays in it, as the C#.
    fn file_hashed(&mut self, hashed: Result<FileDigests, HashFileError>) {
        self.hashing = false;
        self.progress = 0.0;
        match hashed {
            Ok(file) => {
                self.show(file.digests);
                self.file_status = FileStatus::Hashed {
                    name: self.file_name.clone(),
                    size: file.size,
                };
                self.results = true;
                return;
            }
            Err(HashFileError::TooLarge { .. }) => {
                self.file_mode = false;
                self.file_status = FileStatus::TooLarge;
            }
            Err(HashFileError::NotFound) => {
                self.file_mode = false;
                self.file_status = FileStatus::NotFound;
            }
            Err(HashFileError::AccessDenied) => self.file_status = FileStatus::AccessDenied,
            Err(HashFileError::Failed(error)) => {
                log::warn!("hash generator: a file was not hashed: {error}");
                self.file_status = FileStatus::Failed;
            }
        }
        self.show(Vec::new());
        self.results = false;
    }

    /// The file forgotten, its reading let go, as the C# `ClearFile` and the view's
    /// `OnClearFileClick` (`HashGeneratorViewModel.cs:223-246`).
    fn clear_file(&mut self) {
        self.run += 1;
        self.hashing = false;
        self.file_mode = false;
        self.progress = 0.0;
        self.file_status = FileStatus::None;
        self.byte_length = None;
        self.input = Content::new();
        self.show(Vec::new());
        self.results = false;
    }

    /// What the check of the hash pasted says, and which digest it is, as the C#
    /// `UpdateVerifyResult` (`HashGeneratorViewModel.cs:477-511`): nothing while it or the
    /// digests are missing.
    fn verdict(&self) -> Option<(String, Tone, Option<HashAlgorithm>)> {
        if self.verify.trim().is_empty() || self.digests.is_empty() {
            return None;
        }
        Some(
            match hash_computer::find_match(&self.digests, &self.verify) {
                Some(found) => (
                    fl!("ui-tool-hash-match", algorithm = found.kind.display_name()),
                    Tone::Success,
                    Some(found.kind),
                ),
                None => (fl!("ui-tool-hash-no-match"), Tone::Error, None),
            },
        )
    }

    /// The tool's page, as the C# `HashGeneratorView.xaml`: the text, the drop zone with
    /// Browse, the progress, the file's line with Clear file, the empty state or the
    /// digests with their copy and save buttons, the hash to check and what its check says.
    pub fn view<'a>(&'a self, tab: TabId, state: &ToolPane) -> Element<'a, Message> {
        let send = move |message| Message::Tool(tab, ToolMessage::Hash(message));
        let input = super::text_box(&self.input, Some(fl!("ui-tool-hash-placeholder")))
            .height(INPUT_HEIGHT);
        let input = if self.file_mode {
            input
        } else {
            input.on_action(move |action| send(HashMessage::Input(action)))
        };
        let drop_zone = container(
            row![
                text(fl!("ui-tool-hash-drop-zone"))
                    .size(font_size::BODY)
                    .style(text::secondary)
                    .width(Length::Fill),
                super::action_button(
                    fl!("ui-tool-hash-browse"),
                    false,
                    (!self.hashing).then(|| send(HashMessage::Browse)),
                ),
            ]
            .spacing(spacing::SM)
            .align_y(Alignment::Center),
        )
        .padding(BANNER_PADDING)
        .width(Length::Fill)
        .style(styles::card);
        let progress = self.hashing.then(|| {
            progress_bar(0.0..=ALL_READ, self.progress)
                .length(Length::Fill)
                .girth(PROGRESS_HEIGHT)
        });
        let file_line =
            row![container(super::status_line(self.file_status.said())).width(Length::Fill)]
                .push(self.file_mode.then(|| {
                    super::action_button(
                        fl!("ui-tool-hash-clear-file"),
                        false,
                        Some(send(HashMessage::ClearFile)),
                    )
                }))
                .spacing(spacing::SM)
                .align_y(Alignment::Center);
        let verdict = self.verdict();
        let matched = verdict.as_ref().and_then(|(_, _, kind)| *kind);
        let results: Element<'a, Message> = if self.results {
            self.results_view(send, state, matched)
        } else if self.hashing {
            column![].into()
        } else {
            crypto_parts::empty_state(fl!("ui-tool-hash-empty"))
        };
        super::content_column(
            column![
                super::field_label(fl!("ui-tool-hash-input")),
                input,
                iced::widget::space().height(SECTION_GAP - spacing::SM),
                drop_zone,
            ]
            .push(progress)
            .push(file_line)
            .push(results)
            .push(iced::widget::space().height(SECTION_GAP - spacing::SM))
            .push(super::field_label(fl!("ui-tool-hash-verify")))
            .push(
                text_input(&fl!("ui-tool-hash-verify-placeholder"), &self.verify)
                    .font(super::BOX_FONT)
                    .size(font_size::BODY_LARGE)
                    .padding(super::INPUT_PADDING)
                    .style(styles::text_input)
                    .on_input(move |typed| send(HashMessage::Verify(typed))),
            )
            .push(
                verdict
                    .map(|(said, tone, _)| crypto_parts::said(said, tone, font_size::BODY, true)),
            )
            .spacing(spacing::SM),
        )
    }

    /// The digests' card, the one matching the hash pasted in the success colour, and the
    /// bytes of the text hashed.
    fn results_view<'a>(
        &'a self,
        send: impl Fn(HashMessage) -> Message + Copy + 'a,
        state: &ToolPane,
        matched: Option<HashAlgorithm>,
    ) -> Element<'a, Message> {
        let rows = self.digests.iter().zip(&self.boxes).enumerate().map(
            |(index, ((kind, _), content))| {
                let kind = *kind;
                let digest = if matched == Some(kind) {
                    crypto_parts::tinted_box(content, |theme| {
                        theme.extended_palette().success.base.color
                    })
                } else {
                    super::text_box(content, None)
                };
                row![
                    text(kind.display_name())
                        .size(font_size::CAPTION)
                        .font(iced::Font {
                            weight: iced::font::Weight::Bold,
                            ..super::BOX_FONT
                        })
                        .style(text::secondary)
                        .width(NAME_WIDTH),
                    digest
                        .size(font_size::CAPTION)
                        .height(DIGEST_HEIGHT)
                        .on_action(move |action| send(HashMessage::Digest(index, action))),
                    super::action_button(
                        fl!("ui-tool-hash-save"),
                        false,
                        Some(send(HashMessage::Save(kind))),
                    ),
                    super::copy_button(
                        fl!("ui-tool-hash-copy"),
                        state.copied(CopySlot::HashDigest(kind)),
                        send(HashMessage::Copy(kind)),
                        super::COPY_PADDING,
                    ),
                ]
                .spacing(spacing::XS)
                .align_y(Alignment::Center)
                .into()
            },
        );
        column![
            iced::widget::space().height(SECTION_GAP - spacing::SM),
            super::field_label(fl!("ui-tool-hash-results")),
            container(iced::widget::Column::with_children(rows).spacing(spacing::XS))
                .padding(BANNER_PADDING)
                .width(Length::Fill)
                .style(styles::card),
        ]
        .push(self.byte_length.map(|count| {
            crypto_parts::said(
                fl!("ui-tool-hash-byte-length", count = count),
                Tone::Quiet,
                font_size::CAPTION,
                false,
            )
        }))
        .spacing(spacing::SM)
        .into()
    }
}

/// The open dialog for a file to hash, over the window `main`, as the C#
/// `OnBrowseFileClick`: the file picked sent to the tab.
fn pick_file(tab: TabId, main: Option<window::Id>) -> Task<Message> {
    let filter = fl!("ui-tool-hash-all-files");
    main_window_task(main).then(move |id| {
        let dialog = rfd::AsyncFileDialog::new().add_filter(filter.clone(), &[ANY_EXTENSION]);
        let pick = match id {
            Some(id) => window::run(id, move |window| {
                Box::pin(dialog.clone().set_parent(&window).pick_file()) as Pick
            }),
            None => Task::done(Box::pin(dialog.pick_file()) as Pick),
        };
        pick.then(move |pick| {
            Task::perform(pick, move |file| {
                Message::Tool(
                    tab,
                    ToolMessage::Hash(HashMessage::Picked(file.map(|file| file.path().to_owned()))),
                )
            })
        })
    })
}

/// The save dialog for a digest of `kind`, over the window `main`, as the C# `OnSaveRow`:
/// named after its algorithm, a text file first.
fn pick_save(tab: TabId, kind: HashAlgorithm, main: Option<window::Id>) -> Task<Message> {
    let text_filter = format!("{} ({TEXT_PATTERN})", fl!("ui-tool-hash-save"));
    let all_filter = fl!("ui-tool-hash-all-files");
    let name = format!("{}.{TEXT_EXTENSION}", kind.display_name());
    main_window_task(main).then(move |id| {
        let dialog = rfd::AsyncFileDialog::new()
            .set_file_name(name.clone())
            .add_filter(text_filter.clone(), &[TEXT_EXTENSION])
            .add_filter(all_filter.clone(), &[ANY_EXTENSION]);
        let pick = match id {
            Some(id) => window::run(id, move |window| {
                Box::pin(dialog.clone().set_parent(&window).save_file()) as Pick
            }),
            None => Task::done(Box::pin(dialog.save_file()) as Pick),
        };
        pick.then(move |pick| {
            Task::perform(pick, move |file| {
                Message::Tool(
                    tab,
                    ToolMessage::Hash(HashMessage::SaveTo(
                        kind,
                        file.map(|file| file.path().to_owned()),
                    )),
                )
            })
        })
    })
}

/// The file at `path` hashed as reading `run`, away from the window, its progress and its
/// outcome sent to tab `tab`, as the C# `ComputeFileHashesAsync` reports them.
fn hash_task(tab: TabId, path: PathBuf, run: u64) -> Task<Message> {
    Task::run(reports(path, run), move |message| {
        Message::Tool(tab, ToolMessage::Hash(message))
    })
}

/// What hashing the file at `path` as reading `run` says: its progress, a report a percent,
/// then its outcome.
fn reports(path: PathBuf, run: u64) -> impl iced::futures::Stream<Item = HashMessage> {
    iced::stream::channel(PROGRESS_BUFFER, async move |mut out| {
        let mut progress = out.clone();
        let hashed = tokio::task::spawn_blocking(move || {
            let mut shown = 0_i32;
            hash_computer::compute_file(&path, |share| {
                // One report a percent: the window is not flooded.
                let (percent, share) = percent(share);
                if percent > shown {
                    shown = percent;
                    let _ = progress.try_send(HashMessage::Progress(run, share));
                }
            })
        })
        .await
        .unwrap_or_else(|error| Err(HashFileError::Failed(error.to_string())));
        let _ = out.send(HashMessage::Hashed(run, hashed)).await;
    })
}

/// A share read, from 0 to 100, as its whole percent and as the bar takes it.
#[expect(
    clippy::cast_possible_truncation,
    reason = "a share is from 0 to 100, which both hold"
)]
fn percent(share: f64) -> (i32, f32) {
    (share.floor() as i32, share as f32)
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use iced::widget::text_editor::Edit;

    use super::*;

    fn typed(pane: &mut HashPane, typed: &str) {
        pane.update(HashMessage::Input(Action::Edit(Edit::Paste(Arc::new(
            typed.to_owned(),
        )))));
    }

    fn abc_file() -> FileDigests {
        FileDigests {
            digests: hash_computer::compute_all(b"abc"),
            size: 3,
        }
    }

    #[test]
    fn the_text_is_hashed_as_it_is_typed_and_its_bytes_counted() {
        let mut pane = HashPane::default();
        assert!(!pane.results);
        typed(&mut pane, "h\u{e9}");
        assert!(pane.results);
        assert_eq!(pane.byte_length, Some(3), "UTF-8 bytes, as the C#");
        assert_eq!(pane.digests.len(), HashAlgorithm::ALL.len());
        assert_eq!(
            pane.digest(HashAlgorithm::Sha256),
            Some(hash_computer::compute(HashAlgorithm::Sha256, "h\u{e9}".as_bytes()).as_str())
        );
        assert!(matches!(
            pane.update(HashMessage::Copy(HashAlgorithm::Md5)),
            Outcome::Copy(HashAlgorithm::Md5, _)
        ));
        assert_eq!(
            pane.update(HashMessage::Save(HashAlgorithm::Md5)),
            Outcome::AskSave(HashAlgorithm::Md5)
        );
        let path = PathBuf::from("MD5.txt");
        assert!(matches!(
            pane.update(HashMessage::SaveTo(HashAlgorithm::Md5, Some(path.clone()))),
            Outcome::Write(written, _) if written == path
        ));
    }

    #[test]
    fn a_hash_pasted_is_checked_and_its_digest_named() {
        let mut pane = HashPane::default();
        pane.update(HashMessage::Verify(
            "A9993E364706816ABA3E25717850C26C9CD0D89D".to_owned(),
        ));
        assert_eq!(pane.verdict(), None, "nothing hashed yet");
        typed(&mut pane, "abc");
        let (said, tone, kind) = pane.verdict().expect("checked");
        assert_eq!(tone, Tone::Success);
        assert_eq!(kind, Some(HashAlgorithm::Sha1));
        assert!(said.contains("SHA1"), "{said}");
        pane.update(HashMessage::Verify("00".to_owned()));
        assert_eq!(pane.verdict().map(|(_, tone, _)| tone), Some(Tone::Error));
    }

    #[test]
    fn a_file_is_hashed_in_file_mode_and_cleared_back_to_text() {
        let mut pane = HashPane::default();
        typed(&mut pane, "abc");
        let outcome = pane.update(HashMessage::Picked(Some(PathBuf::from("dir/abc.txt"))));
        assert_eq!(outcome, Outcome::Hash(PathBuf::from("dir/abc.txt"), 1));
        assert!(pane.hashing && pane.file_mode && !pane.results);
        assert!(pane.input.text().trim().is_empty(), "the text cleared");
        assert_eq!(
            pane.update(HashMessage::Browse),
            Outcome::Done,
            "one file at a time"
        );
        pane.update(HashMessage::Progress(1, 50.0));
        assert!((pane.progress - 50.0).abs() < f32::EPSILON);
        typed(&mut pane, "x");
        assert!(
            pane.input.text().trim().is_empty(),
            "no typing in file mode"
        );
        pane.update(HashMessage::Hashed(1, Ok(abc_file())));
        assert!(!pane.hashing && pane.results);
        assert_eq!(
            pane.file_status,
            FileStatus::Hashed {
                name: "abc.txt".to_owned(),
                size: 3
            }
        );
        pane.update(HashMessage::ClearFile);
        assert!(!pane.file_mode && !pane.results && pane.digests.is_empty());
        assert_eq!(pane.file_status, FileStatus::None);
    }

    #[test]
    fn a_reading_cleared_or_failed_is_said_as_the_csharp_says_it() {
        let mut pane = HashPane::default();
        let _ = pane.update(HashMessage::Picked(Some(PathBuf::from("big.bin"))));
        pane.update(HashMessage::ClearFile);
        // The reading let go: its outcome is not shown.
        pane.update(HashMessage::Hashed(1, Ok(abc_file())));
        assert!(!pane.results && pane.digests.is_empty());
        let _ = pane.update(HashMessage::Picked(Some(PathBuf::from("big.bin"))));
        pane.update(HashMessage::Hashed(
            pane.run,
            Err(HashFileError::TooLarge {
                limit: MAX_FILE_BYTES,
            }),
        ));
        assert_eq!(pane.file_status, FileStatus::TooLarge);
        assert!(!pane.file_mode, "a file too large leaves file mode");
        let _ = pane.update(HashMessage::Picked(Some(PathBuf::from("locked.bin"))));
        pane.update(HashMessage::Hashed(
            pane.run,
            Err(HashFileError::AccessDenied),
        ));
        assert_eq!(pane.file_status, FileStatus::AccessDenied);
        assert!(pane.file_mode, "one not readable stays in it, as the C#");
        assert!(pane.file_status.said().is_some_and(|(_, error)| error));
    }

    #[test]
    fn a_file_is_read_away_from_the_window_with_its_progress() {
        let dir = tempfile::tempdir().expect("dir");
        let path = dir.path().join("abc.txt");
        std::fs::write(&path, b"abc").expect("written");
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .expect("runtime");
        let seen: Vec<HashMessage> = runtime.block_on(async {
            use iced::futures::StreamExt as _;
            reports(path, 1).collect().await
        });
        assert!(matches!(seen.last(), Some(HashMessage::Hashed(1, Ok(_)))));
        let mut pane = HashPane::default();
        let _ = pane.update(HashMessage::Picked(Some(dir.path().join("abc.txt"))));
        for report in seen {
            pane.update(report);
        }
        assert_eq!(pane.digests, hash_computer::compute_all(b"abc"));
    }
}
