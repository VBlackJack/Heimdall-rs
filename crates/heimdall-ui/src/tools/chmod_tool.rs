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

//! The chmod calculator, as the C# `ChmodCalculatorView` and `ChmodCalculatorViewModel`:
//! nine permission boxes, the octal mode typed or shown, its `rwx` form, a symbolic
//! notation applied on Enter, the command it makes, and the common presets; each kept in
//! step with the others.

use heimdall_app::TabId;
use heimdall_core::tools::posix_mode::{PosixMode, PosixPermission, PosixRole};
use iced::widget::{Column, Row, button, checkbox, column, container, row, text, text_input};
use iced::{Element, Length};

use super::{CopySlot, ToolMessage, ToolPane};
use crate::i18n::fl;
use crate::shell::Message;
use crate::styles;
use crate::tokens::{font_size, spacing};

/// Width of the roles' column, as the C# grid's 100.
const ROLE_WIDTH: f32 = 100.0;

/// Width of each permission's column, as the C# grid's 80.
const PERMISSION_WIDTH: f32 = 80.0;

/// Width of the octal box, as the C#'s 120.
const OCTAL_WIDTH: f32 = 120.0;

/// Room between the octal box and the symbolic form, as the C#'s column of 20.
const OCTAL_GAP: f32 = 20.0;

/// Characters the octal box takes, as the C# `MaxLength`.
const OCTAL_MAX_LENGTH: usize = 3;

/// Room between the sections, as the C# `MarginSectionBottom`.
const SECTION_GAP: f32 = 12.0;

/// What the chmod calculator is asked.
#[derive(Debug, Clone)]
pub enum ChmodMessage {
    /// A permission box ticked or not.
    Bit(PosixRole, PosixPermission, bool),
    /// The octal box typed.
    OctalEdited(String),
    /// The symbolic notation typed.
    SymbolicEdited(String),
    /// Apply the symbolic notation, as Enter in its box.
    ApplySymbolic,
    /// Apply a preset.
    Preset(PosixMode),
    /// Copy the command.
    CopyCommand,
    /// Copy the octal mode.
    CopyOctal,
    /// Copy the `rwx` form.
    CopySymbolic,
}

/// The chmod calculator's state, as the C# view model's.
#[derive(Debug)]
pub struct ChmodPane {
    mode: PosixMode,
    octal: String,
    symbolic_input: String,
    symbolic_error: bool,
    last_symbolic: Option<String>,
}

impl Default for ChmodPane {
    /// A new tab's state, as the C# `ApplyPrefill` without an argument: 755.
    fn default() -> Self {
        let mut pane = Self {
            mode: PosixMode::EMPTY,
            octal: String::new(),
            symbolic_input: String::new(),
            symbolic_error: false,
            last_symbolic: None,
        };
        pane.apply(PosixMode::DEFAULT);
        pane
    }
}

impl ChmodPane {
    /// Applies `message`; what a copy button copies, when one is pressed.
    pub fn update(&mut self, message: ChmodMessage) -> Option<(CopySlot, String)> {
        match message {
            ChmodMessage::Bit(role, permission, on) => {
                // As the C# `OnBitChanged`: the notation applied is forgotten.
                self.symbolic_error = false;
                self.last_symbolic = None;
                self.mode = self.mode.with_bit(role, permission, on);
                self.octal = self.mode.to_octal();
            }
            ChmodMessage::OctalEdited(typed) => {
                self.octal = typed.chars().take(OCTAL_MAX_LENGTH).collect();
                // As the C# `OnOctalTextChanged`: a mode not read is silently left.
                if let Some(mode) = PosixMode::parse_octal(&self.octal) {
                    self.symbolic_error = false;
                    self.last_symbolic = None;
                    self.mode = mode;
                }
            }
            ChmodMessage::SymbolicEdited(typed) => self.symbolic_input = typed,
            ChmodMessage::ApplySymbolic => self.apply_symbolic(),
            ChmodMessage::Preset(mode) => {
                self.symbolic_error = false;
                self.apply(mode);
            }
            ChmodMessage::CopyCommand => return Some((CopySlot::ChmodCommand, self.command())),
            ChmodMessage::CopyOctal => return Some((CopySlot::ChmodOctal, self.octal.clone())),
            ChmodMessage::CopySymbolic => {
                return Some((CopySlot::ChmodSymbolic, self.mode.to_symbolic()));
            }
        }
        None
    }

    /// `mode` shown everywhere, as the C# `ApplyMode`, the notation applied forgotten.
    fn apply(&mut self, mode: PosixMode) {
        self.last_symbolic = None;
        self.mode = mode;
        self.octal = mode.to_octal();
    }

    /// The symbolic notation applied, as the C# `ApplySymbolic`
    /// (`ChmodCalculatorViewModel.cs:120-145`): nothing typed clears the error, a notation
    /// not read says so and leaves the mode, one read is applied and shown in the command.
    fn apply_symbolic(&mut self) {
        let input = self.symbolic_input.trim().to_owned();
        if input.is_empty() {
            self.symbolic_error = false;
            return;
        }
        match PosixMode::parse_symbolic(&input) {
            Some(mode) => {
                self.symbolic_error = false;
                self.apply(mode);
                self.last_symbolic = Some(input);
            }
            None => self.symbolic_error = true,
        }
    }

    /// The mode.
    #[cfg(test)]
    #[must_use]
    pub fn mode(&self) -> PosixMode {
        self.mode
    }

    /// The octal box's text.
    #[cfg(test)]
    #[must_use]
    pub fn octal_text(&self) -> &str {
        &self.octal
    }

    /// Whether the symbolic notation was not read.
    #[cfg(test)]
    #[must_use]
    pub fn has_symbolic_error(&self) -> bool {
        self.symbolic_error
    }

    /// The command, as the C# `UpdateCommandPreview`: the notation applied, else the mode.
    #[must_use]
    pub fn command(&self) -> String {
        let mode = self
            .last_symbolic
            .clone()
            .unwrap_or_else(|| self.mode.to_octal());
        fl!("ui-tool-chmod-command", mode = mode)
    }

    /// The tool's page, as the C# `ChmodCalculatorView.xaml`: the permissions' grid, the
    /// octal and symbolic forms, the symbolic notation, the command, the copy buttons and
    /// the presets.
    #[expect(clippy::too_many_lines, reason = "one page, in the C# view's order")]
    pub fn view<'a>(&'a self, tab: TabId, state: &ToolPane) -> Element<'a, Message> {
        let send = move |message| Message::Tool(tab, ToolMessage::Chmod(message));
        let permission_label = |permission: PosixPermission| match permission {
            PosixPermission::Read => fl!("ui-tool-chmod-read"),
            PosixPermission::Write => fl!("ui-tool-chmod-write"),
            PosixPermission::Execute => fl!("ui-tool-chmod-execute"),
        };
        let mut grid = Column::new().spacing(spacing::XS).push(
            Row::new()
                .push(iced::widget::space().width(ROLE_WIDTH))
                .extend(PosixPermission::ALL.into_iter().map(|permission| {
                    container(text(permission_label(permission)).size(font_size::BODY))
                        .center_x(PERMISSION_WIDTH)
                        .into()
                })),
        );
        for role in PosixRole::ALL {
            let label = match role {
                PosixRole::Owner => fl!("ui-tool-chmod-owner"),
                PosixRole::Group => fl!("ui-tool-chmod-group"),
                PosixRole::Others => fl!("ui-tool-chmod-others"),
            };
            grid = grid.push(
                Row::new()
                    .push(text(label).size(font_size::BODY).width(ROLE_WIDTH))
                    .extend(PosixPermission::ALL.into_iter().map(|permission| {
                        container(
                            checkbox(self.mode.has(role, permission))
                                .on_toggle(move |on| send(ChmodMessage::Bit(role, permission, on)))
                                .style(styles::checkbox),
                        )
                        .center_x(PERMISSION_WIDTH)
                        .into()
                    }))
                    .align_y(iced::Alignment::Center),
            );
        }
        let forms = card(
            row![
                text(fl!("ui-tool-chmod-octal")).size(font_size::BODY),
                text_input("", &self.octal)
                    .font(super::BOX_FONT)
                    .size(font_size::BODY_LARGE)
                    .padding(super::INPUT_PADDING)
                    .width(OCTAL_WIDTH)
                    .style(styles::text_input)
                    .on_input(move |typed| send(ChmodMessage::OctalEdited(typed))),
                iced::widget::space().width(OCTAL_GAP),
                text(fl!("ui-tool-chmod-symbolic")).size(font_size::BODY),
                text(self.mode.to_symbolic())
                    .font(super::BOX_FONT)
                    .size(font_size::BODY_LARGE),
            ]
            .spacing(spacing::SM)
            .align_y(iced::Alignment::Center),
        );
        let symbolic = card(
            column![
                text(fl!("ui-tool-chmod-symbolic-input")).size(font_size::BODY),
                text_input(
                    &fl!("ui-tool-chmod-symbolic-placeholder"),
                    &self.symbolic_input
                )
                .font(super::BOX_FONT)
                .size(font_size::BODY)
                .padding(super::INPUT_PADDING)
                .style(styles::text_input)
                .on_input(move |typed| send(ChmodMessage::SymbolicEdited(typed)))
                .on_submit(send(ChmodMessage::ApplySymbolic)),
            ]
            .push(self.symbolic_error.then(|| {
                text(fl!("ui-tool-chmod-error-symbolic"))
                    .size(font_size::CAPTION)
                    .style(text::danger)
            }))
            .spacing(spacing::XS),
        );
        let command = card(
            row![
                text(fl!("ui-tool-chmod-command-preview")).size(font_size::BODY),
                text(self.command())
                    .font(super::BOX_FONT)
                    .size(font_size::BODY)
                    .width(Length::Fill),
                super::copy_button(
                    fl!("ui-tool-chmod-copy-command"),
                    state.copied(CopySlot::ChmodCommand),
                    send(ChmodMessage::CopyCommand),
                    super::COPY_PADDING,
                ),
            ]
            .spacing(spacing::SM)
            .align_y(iced::Alignment::Center),
        );
        let copies = row![
            super::copy_button(
                fl!("ui-tool-chmod-copy-octal"),
                state.copied(CopySlot::ChmodOctal),
                send(ChmodMessage::CopyOctal),
                super::COPY_PADDING,
            ),
            super::copy_button(
                fl!("ui-tool-chmod-copy-symbolic"),
                state.copied(CopySlot::ChmodSymbolic),
                send(ChmodMessage::CopySymbolic),
                super::COPY_PADDING,
            ),
        ]
        .spacing(spacing::SM);
        let presets = Row::with_children(PosixMode::PRESETS.into_iter().map(|mode| {
            button(text(mode.to_octal()).size(font_size::BODY))
                .padding(super::PRIMARY_PADDING)
                .style(styles::secondary)
                .on_press(send(ChmodMessage::Preset(mode)))
                .into()
        }))
        .spacing(spacing::SM)
        .wrap();
        super::tool_body(
            column![
                grid,
                forms,
                symbolic,
                command,
                copies,
                super::field_label(fl!("ui-tool-chmod-presets")),
                presets,
            ]
            .spacing(SECTION_GAP),
        )
    }
}

/// A bordered section, as the C# cards of the calculator.
fn card<'a>(content: impl Into<Element<'a, Message>>) -> Element<'a, Message> {
    container(content)
        .padding(spacing::MD)
        .width(Length::Fill)
        .style(styles::card)
        .into()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pane_at(octal: &str) -> ChmodPane {
        let mut pane = ChmodPane::default();
        let _ = pane.update(ChmodMessage::Preset(
            PosixMode::parse_octal(octal).expect("mode"),
        ));
        pane
    }

    #[test]
    fn a_new_tab_shows_755() {
        let pane = ChmodPane::default();
        assert_eq!(pane.octal_text(), "755");
        assert_eq!(pane.mode().to_symbolic(), "rwxr-xr-x");
        assert_eq!(pane.command(), "chmod 755 filename");
    }

    #[test]
    fn ticking_bits_recomputes_the_forms() {
        let mut pane = pane_at("000");
        for (role, permission) in [
            (PosixRole::Owner, PosixPermission::Read),
            (PosixRole::Owner, PosixPermission::Write),
            (PosixRole::Owner, PosixPermission::Execute),
            (PosixRole::Group, PosixPermission::Read),
            (PosixRole::Group, PosixPermission::Execute),
            (PosixRole::Others, PosixPermission::Read),
        ] {
            let _ = pane.update(ChmodMessage::Bit(role, permission, true));
        }
        assert_eq!(pane.octal_text(), "754");
        assert_eq!(pane.mode().to_symbolic(), "rwxr-xr--");
        assert_eq!(pane.command(), "chmod 754 filename");
    }

    #[test]
    fn an_octal_typed_sets_the_bits_and_one_not_read_is_left() {
        let mut pane = ChmodPane::default();
        let _ = pane.update(ChmodMessage::SymbolicEdited("u+rwx".to_owned()));
        let _ = pane.update(ChmodMessage::ApplySymbolic);
        let _ = pane.update(ChmodMessage::OctalEdited("600".to_owned()));
        assert!(pane.mode().has(PosixRole::Owner, PosixPermission::Write));
        assert!(!pane.mode().has(PosixRole::Owner, PosixPermission::Execute));
        assert_eq!(pane.command(), "chmod 600 filename");
        let _ = pane.update(ChmodMessage::OctalEdited("755".to_owned()));
        let _ = pane.update(ChmodMessage::OctalEdited("999".to_owned()));
        assert_eq!(pane.octal_text(), "999");
        assert_eq!(pane.mode().to_symbolic(), "rwxr-xr-x");
        assert_eq!(pane.command(), "chmod 755 filename");
        let _ = pane.update(ChmodMessage::OctalEdited("7777".to_owned()));
        assert_eq!(pane.octal_text(), "777", "three characters at most");
    }

    #[test]
    fn a_symbolic_notation_is_applied_and_shown_in_the_command() {
        let mut pane = ChmodPane::default();
        let _ = pane.update(ChmodMessage::SymbolicEdited("u+rwx,g+rx,o+r".to_owned()));
        let _ = pane.update(ChmodMessage::ApplySymbolic);
        assert!(!pane.has_symbolic_error());
        assert_eq!(pane.octal_text(), "754");
        assert_eq!(pane.command(), "chmod u+rwx,g+rx,o+r filename");
        let _ = pane.update(ChmodMessage::Preset(
            PosixMode::parse_octal("600").expect("mode"),
        ));
        assert_eq!(pane.command(), "chmod 600 filename", "a preset forgets it");
    }

    #[test]
    fn a_wrong_notation_says_so_and_leaves_the_mode_and_nothing_clears_it() {
        let mut pane = ChmodPane::default();
        let _ = pane.update(ChmodMessage::SymbolicEdited("invalid".to_owned()));
        let _ = pane.update(ChmodMessage::ApplySymbolic);
        assert!(pane.has_symbolic_error());
        assert_eq!(pane.octal_text(), "755");
        let _ = pane.update(ChmodMessage::SymbolicEdited(" ".to_owned()));
        let _ = pane.update(ChmodMessage::ApplySymbolic);
        assert!(!pane.has_symbolic_error());
    }

    #[test]
    fn the_copy_buttons_copy_what_is_shown() {
        let mut pane = pane_at("644");
        assert_eq!(
            pane.update(ChmodMessage::CopyOctal),
            Some((CopySlot::ChmodOctal, "644".to_owned()))
        );
        assert_eq!(
            pane.update(ChmodMessage::CopySymbolic),
            Some((CopySlot::ChmodSymbolic, "rw-r--r--".to_owned()))
        );
        assert_eq!(
            pane.update(ChmodMessage::CopyCommand),
            Some((CopySlot::ChmodCommand, "chmod 644 filename".to_owned()))
        );
    }
}
