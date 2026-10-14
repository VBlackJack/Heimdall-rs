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

//! The SSH Key Auditor tool, as the C# `SshKeyAuditView` and `SshKeyAuditViewModel`:
//! parses and inspects SSH keys (OpenSSH public/private, PEM PKCS#1, PKCS#8, legacy DSA/EC, SPKI),
//! evaluates algorithm security and key size, checks passphrase encryption, computes SHA-256
//! fingerprints, and displays security ratings and granular findings.

use std::path::PathBuf;

use heimdall_app::TabId;
use heimdall_core::tools::ssh_key_audit::{
    self, FindingKind, FindingSeverity, SecurityRating, SshKeyAuditResult,
};
use iced::widget::text_editor::{Action, Content};
use iced::widget::{column, container, row, text};
use iced::{Alignment, Background, Border, Element, Length, Task, Theme, window};

use super::{CopySlot, ToolMessage, ToolPane, key_parts};
use crate::file_dialog::{FileDialog, Pick};
use crate::i18n::fl;
use crate::shell::Message;
use crate::styles;
use crate::tokens::{font_size, radius, spacing};

/// The marks for findings severity.
const PASS_MARK: &str = "\u{2713}";
const WARN_MARK: &str = "!";
const FAIL_MARK: &str = "\u{2717}";

/// Messages dispatched by the SSH Key Auditor.
#[derive(Debug, Clone)]
pub enum SshAuditMessage {
    /// Text editor action performed.
    Input(Action),
    /// Browse button clicked to open file picker.
    Browse,
    /// A file was loaded or failed to load.
    Loaded(Option<LoadedFile>),
    /// Copy the key fingerprint to the clipboard.
    CopyFingerprint,
}

/// The result of reading a key file chosen from the dialog.
#[derive(Debug, Clone)]
pub enum LoadedFile {
    Text(String),
    TooLarge(usize),
    ReadError(String),
}

/// The SSH Key Auditor view state.
#[derive(Default)]
pub struct SshAuditPane {
    input: Content,
    file_error: Option<String>,
    result: Option<SshKeyAuditResult>,
    has_input: bool,
}

impl std::fmt::Debug for SshAuditPane {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SshAuditPane")
            .field("has_input", &self.has_input)
            .field("has_result", &self.result.is_some())
            .field("file_error", &self.file_error)
            .finish_non_exhaustive()
    }
}

/// The outcome of updating the SSH Key Auditor.
#[derive(Debug)]
pub enum Outcome {
    None,
    Copy(CopySlot, String),
    Task(Task<Message>),
}

impl SshAuditPane {
    /// Applies `message` and returns the outcome.
    pub fn update(
        &mut self,
        message: SshAuditMessage,
        tab: TabId,
        main: Option<window::Id>,
    ) -> Outcome {
        match message {
            SshAuditMessage::Input(action) => {
                let is_edit = action.is_edit();
                self.input.perform(action);
                if is_edit {
                    self.file_error = None;
                    let text = super::box_text(&self.input);
                    self.has_input = !text.trim().is_empty();
                    self.result = if self.has_input {
                        ssh_key_audit::audit(&text)
                    } else {
                        None
                    };
                }
                Outcome::None
            }
            SshAuditMessage::Browse => Outcome::Task(pick_file(tab, main)),
            SshAuditMessage::Loaded(Some(LoadedFile::Text(content))) => {
                self.file_error = None;
                self.has_input = !content.trim().is_empty();
                self.result = ssh_key_audit::audit(&content);
                self.input = Content::with_text(&content);
                Outcome::None
            }
            SshAuditMessage::Loaded(Some(LoadedFile::TooLarge(limit))) => {
                self.file_error = Some(fl!("ui-tool-sshaudit-file-too-large", limit = limit));
                Outcome::None
            }
            SshAuditMessage::Loaded(Some(LoadedFile::ReadError(error))) => {
                self.file_error = Some(fl!("ui-tool-sshaudit-file-read-error", error = error.as_str()));
                Outcome::None
            }
            SshAuditMessage::Loaded(None) => Outcome::None,
            SshAuditMessage::CopyFingerprint => {
                if let Some(res) = &self.result {
                    if !res.fingerprint.is_empty() {
                        return Outcome::Copy(
                            CopySlot::SshAuditFingerprint,
                            res.fingerprint.clone(),
                        );
                    }
                }
                Outcome::None
            }
        }
    }

    /// Renders the SSH Key Auditor tool page.
    pub fn view<'a>(&'a self, tab: TabId, state: &ToolPane, theme: &Theme) -> Element<'a, Message> {
        let send = move |msg| Message::Tool(tab, ToolMessage::SshAudit(msg));

        let browse_button = super::action_button(
            fl!("ui-tool-sshaudit-browse"),
            false,
            Some(send(SshAuditMessage::Browse)),
        );

        let input_header = row![
            super::field_label(fl!("ui-tool-sshaudit-input")),
            container(browse_button).align_x(Alignment::End).width(Length::Fill),
        ]
        .align_y(Alignment::Center)
        .width(Length::Fill);

        let editor = super::text_box(&self.input, Some(fl!("ui-tool-sshaudit-input")))
            .height(Length::Fixed(140.0))
            .on_action(move |action| send(SshAuditMessage::Input(action)));

        let mut input_col = column![input_header, editor].spacing(spacing::SM);

        if let Some(err) = &self.file_error {
            input_col = input_col.push(
                text(err.as_str())
                    .size(font_size::CAPTION)
                    .style(text::danger),
            );
        }

        let input_card = key_parts::card(input_col);

        let body: Element<'_, Message> = if !self.has_input {
            super::empty_state(fl!("ui-tool-sshaudit-empty-state"))
        } else if let Some(res) = &self.result {
            let badges = self.badges_row(res);
            let details = self.details_card(res, send, state);
            let findings = self.findings_card(res, theme);
            column![badges, details, findings].spacing(spacing::MD).into()
        } else {
            container(
                text(fl!("ui-tool-sshaudit-parse-error"))
                    .size(font_size::BODY)
                    .style(text::danger),
            )
            .padding(spacing::MD)
            .into()
        };

        super::content_column(column![input_card, body].spacing(spacing::MD))
    }

    fn badges_row<'a>(&'a self, res: &'a SshKeyAuditResult) -> Element<'a, Message> {
        let algo_badge = container(
            text(&res.algorithm)
                .size(font_size::BODY)
                .font(styles::SEMIBOLD)
                .style(text::primary),
        )
        .padding([4.0, 12.0])
        .style(move |t: &Theme| {
            let palette = t.extended_palette();
            container::Style {
                background: Some(Background::Color(palette.primary.weak.color)),
                border: Border {
                    radius: radius::SM.into(),
                    width: 1.0,
                    color: palette.primary.base.color,
                },
                ..container::Style::default()
            }
        });

        let rating_text = match res.rating {
            SecurityRating::Strong => fl!("ui-tool-sshaudit-strong"),
            SecurityRating::Acceptable => fl!("ui-tool-sshaudit-acceptable"),
            SecurityRating::Weak => fl!("ui-tool-sshaudit-weak"),
            SecurityRating::Deprecated => fl!("ui-tool-sshaudit-deprecated"),
        };

        let rating = res.rating;
        let rating_badge = container(
            text(rating_text)
                .size(font_size::BODY)
                .font(styles::SEMIBOLD)
                .style(move |t: &Theme| {
                    let palette = t.extended_palette();
                    match rating {
                        SecurityRating::Strong => text::Style {
                            color: Some(palette.success.base.color),
                        },
                        SecurityRating::Acceptable => text::Style {
                            color: Some(palette.warning.base.color),
                        },
                        SecurityRating::Weak | SecurityRating::Deprecated => text::Style {
                            color: Some(palette.danger.base.color),
                        },
                    }
                }),
        )
        .padding([4.0, 12.0])
        .style(move |t: &Theme| {
            let palette = t.extended_palette();
            let (bg, border_color) = match rating {
                SecurityRating::Strong => (palette.success.weak.color, palette.success.base.color),
                SecurityRating::Acceptable => (palette.warning.weak.color, palette.warning.base.color),
                SecurityRating::Weak | SecurityRating::Deprecated => {
                    (palette.danger.weak.color, palette.danger.base.color)
                }
            };
            container::Style {
                background: Some(Background::Color(bg)),
                border: Border {
                    radius: radius::SM.into(),
                    width: 1.0,
                    color: border_color,
                },
                ..container::Style::default()
            }
        });

        row![algo_badge, rating_badge].spacing(spacing::SM).into()
    }

    fn details_card<'a>(
        &'a self,
        res: &'a SshKeyAuditResult,
        send: impl Fn(SshAuditMessage) -> Message + Copy + 'a,
        state: &ToolPane,
    ) -> Element<'a, Message> {
        let bits_label = if res.key_size > 0 {
            fl!("ui-tool-sshaudit-bits", bits = res.key_size)
        } else {
            String::new()
        };

        let key_type = if res.is_private_key {
            fl!("ui-tool-sshaudit-private")
        } else {
            fl!("ui-tool-sshaudit-public")
        };

        let encrypted = if res.is_encrypted {
            fl!("ui-tool-sshaudit-yes")
        } else {
            fl!("ui-tool-sshaudit-no")
        };

        let copy_btn = super::copy_button(
            fl!("ui-tool-sshaudit-copy-fingerprint"),
            state.copied(CopySlot::SshAuditFingerprint),
            send(SshAuditMessage::CopyFingerprint),
            super::COPY_PADDING,
        );

        let fp_row = row![
            text(&res.fingerprint).size(font_size::BODY),
            copy_btn,
        ]
        .spacing(spacing::SM)
        .align_y(Alignment::Center);

        let content = column![
            key_parts::section_title(fl!("ui-tool-sshaudit-details")),
            key_parts::labeled(fl!("ui-tool-sshaudit-algorithm"), text(&res.algorithm).size(font_size::BODY)),
            key_parts::labeled(fl!("ui-tool-sshaudit-key-size"), text(bits_label).size(font_size::BODY)),
            key_parts::labeled(fl!("ui-tool-sshaudit-fingerprint"), fp_row),
            key_parts::labeled(fl!("ui-tool-sshaudit-format"), text(&res.format).size(font_size::BODY)),
            key_parts::labeled(fl!("ui-tool-sshaudit-type"), text(key_type).size(font_size::BODY)),
            key_parts::labeled(fl!("ui-tool-sshaudit-encrypted"), text(encrypted).size(font_size::BODY)),
        ]
        .spacing(spacing::SM);

        key_parts::card(content)
    }

    fn findings_card<'a>(&'a self, res: &'a SshKeyAuditResult, theme: &Theme) -> Element<'a, Message> {
        let mut list = column![key_parts::section_title(fl!("ui-tool-sshaudit-findings"))]
            .spacing(spacing::SM);

        for finding in &res.findings {
            let (mark, mark_color) = match finding.severity {
                FindingSeverity::Pass => (
                    PASS_MARK,
                    theme.extended_palette().success.base.color,
                ),
                FindingSeverity::Warning => (
                    WARN_MARK,
                    theme.extended_palette().warning.base.color,
                ),
                FindingSeverity::Fail => (
                    FAIL_MARK,
                    theme.extended_palette().danger.base.color,
                ),
            };

            let finding_text = match finding.kind {
                FindingKind::Ed25519Strong => fl!("ui-tool-sshaudit-finding-ed25519"),
                FindingKind::RsaStrong(bits) => fl!("ui-tool-sshaudit-finding-rsa-strong", bits = bits),
                FindingKind::RsaOk => fl!("ui-tool-sshaudit-finding-rsa-ok"),
                FindingKind::RsaWeak(bits) => fl!("ui-tool-sshaudit-finding-rsa-weak", bits = bits),
                FindingKind::DsaDeprecated => fl!("ui-tool-sshaudit-finding-dsa"),
                FindingKind::EcdsaStrong(bits) => fl!("ui-tool-sshaudit-finding-ecdsa-strong", bits = bits),
                FindingKind::EcdsaOk => fl!("ui-tool-sshaudit-finding-ecdsa-ok"),
                FindingKind::UnencryptedPrivateKey => fl!("ui-tool-sshaudit-finding-unencrypted"),
                FindingKind::EncryptedPrivateKey => fl!("ui-tool-sshaudit-finding-encrypted"),
                FindingKind::OldPemFormat => fl!("ui-tool-sshaudit-finding-old-format"),
            };

            let item = row![
                container(
                    text(mark)
                        .size(font_size::BODY_LARGE)
                        .font(styles::SEMIBOLD)
                        .style(move |_| text::Style { color: Some(mark_color) })
                )
                .width(Length::Fixed(24.0)),
                text(finding_text).size(font_size::BODY).width(Length::Fill),
            ]
            .spacing(spacing::SM)
            .align_y(Alignment::Center);

            list = list.push(item);
        }

        key_parts::card(list)
    }
}

fn pick_file(tab: TabId, main: Option<window::Id>) -> Task<Message> {
    let title = fl!("ui-tool-sshaudit-title");
    let done = move |loaded| Message::Tool(tab, ToolMessage::SshAudit(SshAuditMessage::Loaded(loaded)));
    crate::shell::main_window_task(main).then(move |id| {
        let title = title.clone();
        let pick = match id {
            Some(id) => window::run(id, move |window| {
                FileDialog::new()
                    .set_title(title.clone())
                    .set_parent(window)
                    .add_filter(fl!("ui-tool-sshaudit-title"), &["pem", "pub", "key", "crt"])
                    .pick_file()
            }),
            None => Task::done(
                FileDialog::new()
                    .set_title(title)
                    .add_filter(fl!("ui-tool-sshaudit-title"), &["pem", "pub", "key", "crt"])
                    .pick_file(),
            ),
        };
        pick.then(move |pick| Task::perform(read_picked(pick), done))
    })
}

async fn read_picked(pick: Pick<PathBuf>) -> Option<LoadedFile> {
    match pick.await {
        Ok(Some(path)) => Some(read_file(path).await),
        Ok(None) => None,
        Err(unavailable) => Some(LoadedFile::ReadError(unavailable.message())),
    }
}

async fn read_file(path: PathBuf) -> LoadedFile {
    match tokio::fs::metadata(&path).await {
        Ok(meta) if meta.len() as usize > ssh_key_audit::MAX_KEY_FILE_SIZE => {
            LoadedFile::TooLarge(ssh_key_audit::MAX_KEY_FILE_SIZE)
        }
        Ok(_) => match tokio::fs::read_to_string(&path).await {
            Ok(content) => LoadedFile::Text(content),
            Err(error) => LoadedFile::ReadError(error.to_string()),
        },
        Err(error) => LoadedFile::ReadError(error.to_string()),
    }
}
