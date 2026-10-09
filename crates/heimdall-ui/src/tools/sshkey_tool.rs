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

//! The SSH Key Generator, as the C# `SshKeyGeneratorView` (`SshKeyGeneratorView.xaml.cs`):
//! RSA 2048, RSA 4096 or Ed25519, a comment starting as `user@host`, a passphrase; the pair
//! made off the window's thread, the line under the button saying so; the fingerprint, the
//! public key and the private key, the latter masked until shown; each copied, the public
//! and the private key saved, the private one readable by the user alone.
//!
//! The passphrase and the private key are secrets: held in memory wiped when replaced or
//! when the tab closes, never logged.

use std::fmt;
use std::path::PathBuf;
use std::sync::Arc;

use heimdall_app::TabId;
use heimdall_core::tools::ssh_key_generator::{
    self as engine, GeneratedSshKey, PRIVATE_KEY_EXTENSION, PUBLIC_KEY_EXTENSION, SshKeyAlgorithm,
};
use iced::widget::text_editor::{Action, Content};
use iced::widget::{column, pick_list, row, text_input};
use iced::{Alignment, Element, Length, Task, window};
use zeroize::Zeroizing;

use super::crypto_parts::{self, Tone};
use super::key_parts;
use super::{CopySlot, ToolMessage, ToolPane};
use crate::i18n::fl;
use crate::shell::Message;
use crate::styles;
use crate::tokens::{font_size, spacing};

/// What the hidden private key shows, as the C# `MaskedPlaceholder`.
pub const MASKED_PLACEHOLDER: &str = "********";

/// The extensions offered by the save dialogs, as the C# filters.
const PUBLIC_EXTENSIONS: &[&str] = &[PUBLIC_KEY_EXTENSION];
const PRIVATE_EXTENSIONS: &[&str] = &[PRIVATE_KEY_EXTENSION];

/// Height of a key's box, as the C# `Height="80"` and `Height="160"`.
const PUBLIC_HEIGHT: f32 = 80.0;
const PRIVATE_HEIGHT: f32 = 160.0;

/// Height of the fingerprint's box: one line.
const FINGERPRINT_HEIGHT: f32 = 36.0;

/// An algorithm in the box.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AlgorithmChoice(pub SshKeyAlgorithm);

impl fmt::Display for AlgorithmChoice {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&match self.0 {
            SshKeyAlgorithm::Rsa2048 => fl!("ui-tool-sshkey-rsa-2048"),
            SshKeyAlgorithm::Rsa4096 => fl!("ui-tool-sshkey-rsa-4096"),
            SshKeyAlgorithm::Ed25519 => fl!("ui-tool-sshkey-ed25519"),
        })
    }
}

/// A copy button of the page.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SshCopy {
    /// The fingerprint.
    Fingerprint,
    /// The public key.
    Public,
    /// The private key.
    Private,
}

/// Which key is saved.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SshSave {
    /// The public key, in its `.pub`.
    Public,
    /// The private key.
    Private,
}

/// A box of the page.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SshBox {
    /// The fingerprint's.
    Fingerprint,
    /// The public key's.
    Public,
    /// The private key's.
    Private,
}

/// What the SSH Key Generator is asked.
#[derive(Clone)]
pub enum SshKeyMessage {
    /// The algorithm chosen.
    Algorithm(AlgorithmChoice),
    /// The comment typed.
    Comment(String),
    /// The passphrase typed.
    Passphrase(String),
    /// Make a pair, as the C# Generate button and Enter in the comment.
    Generate,
    /// Generation `.0` made this, or failed saying why; `None` when it did not finish.
    Generated(u64, Option<Result<Arc<GeneratedSshKey>, String>>),
    /// A copy button pressed.
    Copy(SshCopy),
    /// Show or hide the private key.
    TogglePrivate,
    /// Save a key: its place asked first.
    Save(SshSave),
    /// Where to save, picked; `None` when cancelled.
    SaveTo(SshSave, Option<PathBuf>),
    /// The file was written, or why not.
    Saved(Result<(), String>),
    /// Something done in a box, which does not change it.
    Box(SshBox, Action),
}

impl fmt::Debug for SshKeyMessage {
    /// The passphrase and the keys are never written out.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Algorithm(choice) => write!(f, "Algorithm({:?})", choice.0),
            Self::Passphrase(_) => f.write_str("Passphrase(..)"),
            Self::Generate => f.write_str("Generate"),
            Self::Generated(generation, made) => {
                write!(f, "Generated({generation}, {})", made.is_some())
            }
            Self::Copy(copy) => write!(f, "Copy({copy:?})"),
            Self::Save(save) => write!(f, "Save({save:?})"),
            _ => f.write_str("SshKeyMessage(..)"),
        }
    }
}

/// What an update asks of its tab.
pub enum Outcome {
    /// Nothing more.
    Done,
    /// This copied by its button.
    Copy(CopySlot, String),
    /// A pair made, as generation `.0`.
    Generate(u64, SshKeyAlgorithm, String, Zeroizing<String>),
    /// The save dialog of this key, offering this file's name.
    AskSave(SshSave, String),
    /// This key written at this path.
    Write(SshSave, PathBuf, Zeroizing<String>),
}

impl fmt::Debug for Outcome {
    /// What is copied, the passphrase and the keys are never written out.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Done => f.write_str("Done"),
            Self::Copy(slot, _) => write!(f, "Copy({slot:?}, ..)"),
            Self::Generate(generation, algorithm, ..) => {
                write!(f, "Generate({generation}, {algorithm:?}, ..)")
            }
            Self::AskSave(save, name) => write!(f, "AskSave({save:?}, {name})"),
            Self::Write(save, path, _) => write!(f, "Write({save:?}, {}, ..)", path.display()),
        }
    }
}

impl Outcome {
    /// What tab `tab` runs for it, its dialogs over the window `main`.
    pub fn task(self, tab: TabId, main: Option<window::Id>) -> Task<Message> {
        let send = move |message| Message::Tool(tab, ToolMessage::SshKey(message));
        match self {
            Self::Done | Self::Copy(..) => Task::none(),
            Self::Generate(generation, algorithm, comment, passphrase) => key_parts::off_thread(
                move || {
                    engine::generate(algorithm, &comment, &passphrase)
                        .map(Arc::new)
                        .map_err(|error| error.to_string())
                },
                move |made| send(SshKeyMessage::Generated(generation, made)),
            ),
            Self::AskSave(save, file_name) => {
                let (filter, extensions) = match save {
                    SshSave::Public => (fl!("ui-tool-sshkey-public-filter"), PUBLIC_EXTENSIONS),
                    SshSave::Private => (fl!("ui-tool-sshkey-private-filter"), PRIVATE_EXTENSIONS),
                };
                key_parts::ask_save(
                    main,
                    fl!("ui-tool-sshkey-title"),
                    file_name,
                    filter,
                    extensions,
                    move |path| send(SshKeyMessage::SaveTo(save, path)),
                )
            }
            Self::Write(save, path, content) => key_parts::off_thread(
                move || {
                    match save {
                        SshSave::Public => engine::write_public_key(&path, &content),
                        SshSave::Private => engine::write_private_key(&path, &content),
                    }
                    .map_err(|error| error.to_string())
                },
                move |written| {
                    send(SshKeyMessage::Saved(
                        written.unwrap_or_else(|| Err(String::new())),
                    ))
                },
            ),
        }
    }
}

/// What the line under the button says.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Said {
    /// Making, as the C# `ToolSshKeyGenGenerating`.
    Generating,
    /// The generation failed, saying why.
    Failed(String),
    /// A file could not be written.
    SaveFailed(String),
}

/// The SSH Key Generator's state, as the C# view's fields.
pub struct SshKeyPane {
    algorithm: SshKeyAlgorithm,
    comment: String,
    passphrase: Zeroizing<String>,
    generating: bool,
    generation: u64,
    key: Option<Arc<GeneratedSshKey>>,
    private_visible: bool,
    said: Option<Said>,
    fingerprint_box: Content,
    public_box: Content,
    private_box: Content,
}

impl fmt::Debug for SshKeyPane {
    /// The passphrase and the private key are never written out.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SshKeyPane")
            .field("algorithm", &self.algorithm)
            .field("generating", &self.generating)
            .field("key", &self.key.is_some())
            .finish_non_exhaustive()
    }
}

impl SshKeyPane {
    /// A new tab, as the C# `Initialize`: RSA 2048, the comment `user@host`.
    #[must_use]
    pub fn new() -> Self {
        Self {
            algorithm: SshKeyAlgorithm::Rsa2048,
            comment: engine::default_comment(),
            passphrase: Zeroizing::default(),
            generating: false,
            generation: 0,
            key: None,
            private_visible: false,
            said: None,
            fingerprint_box: Content::new(),
            public_box: Content::new(),
            private_box: Content::new(),
        }
    }

    /// Applies `message`; what is then to be done.
    pub fn update(&mut self, message: SshKeyMessage) -> Outcome {
        match message {
            SshKeyMessage::Algorithm(choice) => self.algorithm = choice.0,
            SshKeyMessage::Comment(typed) => self.comment = typed,
            SshKeyMessage::Passphrase(typed) => self.passphrase = Zeroizing::new(typed),
            SshKeyMessage::Generate => {
                if !self.generating {
                    self.generating = true;
                    self.generation += 1;
                    self.said = Some(Said::Generating);
                    return Outcome::Generate(
                        self.generation,
                        self.algorithm,
                        self.comment.trim().to_owned(),
                        self.passphrase.clone(),
                    );
                }
            }
            SshKeyMessage::Generated(generation, made) => {
                if generation == self.generation {
                    self.generated(made);
                }
            }
            SshKeyMessage::Copy(copy) => return self.copy(copy),
            SshKeyMessage::TogglePrivate => {
                self.private_visible = !self.private_visible;
                self.refresh_boxes();
            }
            SshKeyMessage::Save(save) => {
                if let Some(key) = &self.key {
                    let extension = match save {
                        SshSave::Public => PUBLIC_KEY_EXTENSION,
                        SshSave::Private => PRIVATE_KEY_EXTENSION,
                    };
                    let name = format!("{}.{extension}", key.algorithm.file_stem());
                    return Outcome::AskSave(save, name);
                }
            }
            SshKeyMessage::SaveTo(save, Some(path)) => {
                if let Some(key) = &self.key {
                    let content = match save {
                        SshSave::Public => Zeroizing::new(key.public_key.clone()),
                        SshSave::Private => key.private_key_pem.clone(),
                    };
                    return Outcome::Write(save, path, content);
                }
            }
            SshKeyMessage::SaveTo(_, None) | SshKeyMessage::Saved(Ok(())) => {}
            SshKeyMessage::Saved(Err(error)) => self.said = Some(Said::SaveFailed(error)),
            SshKeyMessage::Box(slot, action) => {
                let content = match slot {
                    SshBox::Fingerprint => &mut self.fingerprint_box,
                    SshBox::Public => &mut self.public_box,
                    SshBox::Private => &mut self.private_box,
                };
                super::read_only(content, action);
            }
        }
        Outcome::Done
    }

    /// The pair shown, the private key masked, as the C# `ShowGeneratedKeys`.
    fn generated(&mut self, made: Option<Result<Arc<GeneratedSshKey>, String>>) {
        self.generating = false;
        self.said = None;
        match made {
            Some(Ok(key)) => {
                self.key = Some(key);
                self.private_visible = false;
                self.refresh_boxes();
            }
            Some(Err(error)) => self.said = Some(Said::Failed(error)),
            None => self.said = Some(Said::Failed(String::new())),
        }
    }

    fn refresh_boxes(&mut self) {
        let Some(key) = &self.key else {
            return;
        };
        self.fingerprint_box = Content::with_text(&key.fingerprint);
        self.public_box = Content::with_text(&key.public_key);
        self.private_box = if self.private_visible {
            Content::with_text(&key.private_key_pem)
        } else {
            Content::with_text(MASKED_PLACEHOLDER)
        };
    }

    fn copy(&self, copy: SshCopy) -> Outcome {
        let Some(key) = &self.key else {
            return Outcome::Done;
        };
        match copy {
            SshCopy::Fingerprint => {
                Outcome::Copy(CopySlot::SshFingerprint, key.fingerprint.clone())
            }
            SshCopy::Public => Outcome::Copy(CopySlot::SshPublic, key.public_key.clone()),
            SshCopy::Private => {
                Outcome::Copy(CopySlot::SshPrivate, key.private_key_pem.to_string())
            }
        }
    }

    /// The tool's page, as the C# `SshKeyGeneratorView.xaml`.
    pub fn view<'a>(&'a self, tab: TabId, state: &ToolPane) -> Element<'a, Message> {
        let send = move |message| Message::Tool(tab, ToolMessage::SshKey(message));
        let settings = key_parts::card(key_parts::stack([
            key_parts::labeled(
                fl!("ui-tool-sshkey-algorithm"),
                pick_list(
                    SshKeyAlgorithm::ALL.map(AlgorithmChoice),
                    Some(AlgorithmChoice(self.algorithm)),
                    move |choice| send(SshKeyMessage::Algorithm(choice)),
                )
                .width(Length::Fill)
                .style(styles::pick_list)
                .menu_style(styles::menu)
                .text_size(font_size::BODY),
            ),
            key_parts::labeled(
                fl!("ui-tool-sshkey-comment"),
                text_input(&fl!("ui-tool-sshkey-comment-placeholder"), &self.comment)
                    .size(font_size::BODY_LARGE)
                    .padding(super::INPUT_PADDING)
                    .style(styles::text_input)
                    .on_input(move |typed| send(SshKeyMessage::Comment(typed)))
                    .on_submit(send(SshKeyMessage::Generate)),
            ),
            key_parts::labeled(
                fl!("ui-tool-sshkey-passphrase"),
                text_input("", &self.passphrase)
                    .secure(true)
                    .size(font_size::BODY_LARGE)
                    .padding(super::INPUT_PADDING)
                    .style(styles::text_input)
                    .on_input(move |typed| send(SshKeyMessage::Passphrase(typed))),
            ),
            key_parts::hint(fl!("ui-tool-sshkey-passphrase-hint")),
        ]));
        let generate = super::action_button(
            fl!("ui-tool-sshkey-generate"),
            true,
            (!self.generating).then_some(send(SshKeyMessage::Generate)),
        );
        let said = self.said.as_ref().map(|said| {
            let (text, tone) = match said {
                Said::Generating => (fl!("ui-tool-sshkey-generating"), Tone::Quiet),
                Said::Failed(error) => (
                    fl!("ui-tool-sshkey-error", error = error.as_str()),
                    Tone::Error,
                ),
                Said::SaveFailed(error) => (
                    fl!("ui-tool-sshkey-save-failed", error = error.as_str()),
                    Tone::Error,
                ),
            };
            crypto_parts::said(text, tone, font_size::BODY, false)
        });
        let mut page = column![settings, generate].push(said).spacing(spacing::MD);
        if self.key.is_some() {
            page = page.push(self.results(send, state));
        }
        super::content_column(page.push(key_parts::hint(fl!("ui-tool-sshkey-ed25519-notice"))))
    }

    /// The fingerprint and the two keys, as the C#'s panels shown once a pair is made.
    fn results<'a>(
        &'a self,
        send: impl Fn(SshKeyMessage) -> Message + Copy + 'a,
        state: &ToolPane,
    ) -> Element<'a, Message> {
        let copy = |slot: CopySlot, copy: SshCopy| {
            super::copy_button(
                fl!("ui-tool-sshkey-copy"),
                state.copied(slot),
                send(SshKeyMessage::Copy(copy)),
                super::COPY_PADDING,
            )
        };
        let save = |key: SshSave| {
            super::action_button(
                fl!("ui-tool-sshkey-save"),
                false,
                Some(send(SshKeyMessage::Save(key))),
            )
        };
        let shown = super::action_button(
            if self.private_visible {
                fl!("ui-tool-sshkey-hide")
            } else {
                fl!("ui-tool-sshkey-show")
            },
            false,
            Some(send(SshKeyMessage::TogglePrivate)),
        );
        let key_box = |content: &'a Content, slot: SshBox, height: f32| {
            super::text_box(content, None)
                .height(height)
                .on_action(move |action| send(SshKeyMessage::Box(slot, action)))
        };
        let heading = |label: String, buttons: Vec<Element<'a, Message>>| {
            row![super::field_label(label)]
                .extend(buttons)
                .spacing(spacing::SM)
                .align_y(Alignment::Center)
        };
        column![
            key_parts::card(key_parts::stack([
                heading(
                    fl!("ui-tool-sshkey-fingerprint"),
                    vec![copy(CopySlot::SshFingerprint, SshCopy::Fingerprint)]
                )
                .into(),
                key_box(
                    &self.fingerprint_box,
                    SshBox::Fingerprint,
                    FINGERPRINT_HEIGHT
                )
                .into(),
            ])),
            key_parts::card(key_parts::stack([
                heading(
                    fl!("ui-tool-sshkey-public-key"),
                    vec![
                        copy(CopySlot::SshPublic, SshCopy::Public),
                        save(SshSave::Public)
                    ]
                )
                .into(),
                key_box(&self.public_box, SshBox::Public, PUBLIC_HEIGHT).into(),
            ])),
            key_parts::card(key_parts::stack([
                heading(
                    fl!("ui-tool-sshkey-private-key"),
                    vec![
                        copy(CopySlot::SshPrivate, SshCopy::Private),
                        shown,
                        save(SshSave::Private)
                    ]
                )
                .into(),
                key_box(&self.private_box, SshBox::Private, PRIVATE_HEIGHT).into(),
            ])),
        ]
        .spacing(spacing::MD)
        .into()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_outcome_never_writes_out_its_secrets() {
        let generate = Outcome::Generate(
            1,
            SshKeyAlgorithm::Ed25519,
            "me".to_owned(),
            Zeroizing::new("s3cret".to_owned()),
        );
        let write = Outcome::Write(
            SshSave::Private,
            PathBuf::from("k"),
            Zeroizing::new("PRIVATE".to_owned()),
        );
        let copy = Outcome::Copy(CopySlot::SshPrivate, "PRIVATE".to_owned());
        let shown = format!("{generate:?} {write:?} {copy:?}");
        assert!(
            !shown.contains("s3cret") && !shown.contains("PRIVATE"),
            "{shown}"
        );
    }

    fn made(pane: &mut SshKeyPane) {
        let Outcome::Generate(generation, algorithm, comment, passphrase) =
            pane.update(SshKeyMessage::Generate)
        else {
            panic!("asked to generate");
        };
        let key = engine::generate_with_rounds(algorithm, &comment, &passphrase, 1000)
            .map(Arc::new)
            .map_err(|error| error.to_string());
        pane.update(SshKeyMessage::Generated(generation, Some(key)));
    }

    #[test]
    fn a_pair_is_made_shown_masked_copied_and_saved_under_its_name() {
        let mut pane = SshKeyPane::new();
        assert!(pane.comment.contains('@'), "user@host");
        pane.update(SshKeyMessage::Algorithm(AlgorithmChoice(
            SshKeyAlgorithm::Ed25519,
        )));
        pane.update(SshKeyMessage::Comment("me@here".to_owned()));
        pane.update(SshKeyMessage::Passphrase("pw".to_owned()));
        made(&mut pane);
        assert!(pane.said.is_none());
        assert!(pane.public_box.text().starts_with("ssh-ed25519 "));
        assert_eq!(pane.private_box.text().trim_end(), MASKED_PLACEHOLDER);
        let Outcome::Copy(CopySlot::SshPrivate, key) =
            pane.update(SshKeyMessage::Copy(SshCopy::Private))
        else {
            panic!("copied");
        };
        assert!(key.starts_with("-----BEGIN ENCRYPTED PRIVATE KEY-----"));
        pane.update(SshKeyMessage::TogglePrivate);
        assert!(pane.private_box.text().starts_with("-----BEGIN ENCRYPTED"));
        let Outcome::AskSave(SshSave::Private, name) =
            pane.update(SshKeyMessage::Save(SshSave::Private))
        else {
            panic!("asked where");
        };
        assert_eq!(name, "id_ed25519.pem");
        let Outcome::AskSave(SshSave::Public, name) =
            pane.update(SshKeyMessage::Save(SshSave::Public))
        else {
            panic!("asked where");
        };
        assert_eq!(name, "id_ed25519.pub");
        assert!(matches!(
            pane.update(SshKeyMessage::SaveTo(
                SshSave::Private,
                Some(PathBuf::from("k"))
            )),
            Outcome::Write(SshSave::Private, _, _)
        ));
        let shown = format!("{pane:?} {:?}", SshKeyMessage::Passphrase("pw".to_owned()));
        assert!(
            !shown.contains("PRIVATE") && !shown.contains("pw\""),
            "{shown}"
        );
    }

    #[test]
    fn one_generation_at_a_time_and_a_late_one_is_ignored() {
        let mut pane = SshKeyPane::new();
        assert!(matches!(
            pane.update(SshKeyMessage::Generate),
            Outcome::Generate(1, ..)
        ));
        assert_eq!(pane.said, Some(Said::Generating));
        assert!(matches!(
            pane.update(SshKeyMessage::Generate),
            Outcome::Done
        ));
        pane.update(SshKeyMessage::Generated(0, Some(Err("old".to_owned()))));
        assert!(pane.generating, "an older generation is let go");
        pane.update(SshKeyMessage::Generated(1, Some(Err("broken".to_owned()))));
        assert_eq!(pane.said, Some(Said::Failed("broken".to_owned())));
    }
}
