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

//! The UUID generator, as the C# `UuidGeneratorView` and `UuidGeneratorViewModel`: one UUID
//! made when the tab opens and on Generate, a batch of 1 to 100 on Generate Batch, version 4
//! or 7, written in upper or lower case, with or without hyphens; a change of format writes
//! the same UUIDs again, a change of version makes new ones.

use std::fmt;

use heimdall_app::TabId;
use heimdall_core::tools::uuid_generator::{self, Uuid, UuidFormat, UuidVersion};
use iced::widget::text_editor::{Action, Content};
use iced::widget::{checkbox, column, container, pick_list, row, text, text_input};
use iced::{Element, Length};

use super::{CopySlot, ToolMessage, ToolPane};
use crate::i18n::fl;
use crate::shell::Message;
use crate::styles;
use crate::tokens::{font_size, spacing};

/// The fewest UUIDs of a batch, as the C# `MinBatchCount`.
pub const MIN_BATCH_COUNT: usize = 1;

/// The most, as the C# `MaxBatchCount`.
pub const MAX_BATCH_COUNT: usize = 100;

/// The count a batch starts at, as the C# `DefaultBatchCount`.
pub const DEFAULT_BATCH_COUNT: usize = 10;

/// Width of the version's list, as the C# combo box's.
const VERSION_WIDTH: f32 = 160.0;

/// Width of the count's box, as the C#'s.
const COUNT_WIDTH: f32 = 60.0;

/// Height of the single UUID's box: one line.
const SINGLE_HEIGHT: f32 = 36.0;

/// Height of the batch's box before it scrolls, as the C# `MaxHeight`.
const BATCH_MAX_HEIGHT: f32 = 300.0;

/// Room between the sections, as the C# `MarginSectionSeparator`.
const SECTION_GAP: f32 = 16.0;

/// What the UUID generator is asked.
#[derive(Debug, Clone)]
pub enum UuidMessage {
    /// The version chosen.
    Version(VersionChoice),
    /// Upper case on or off.
    Uppercase(bool),
    /// Hyphens on or off.
    Hyphens(bool),
    /// Generate one UUID.
    Generate,
    /// The batch's count typed.
    CountEdited(String),
    /// Generate a batch.
    GenerateBatch,
    /// Something done in the single UUID's box, which does not change it.
    Single(Action),
    /// Something done in the batch's box, which does not change it.
    Batch(Action),
    /// Copy the UUID.
    Copy,
    /// Copy the batch.
    CopyBatch,
}

/// A version in the list, named in the language shown.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VersionChoice(pub UuidVersion);

impl VersionChoice {
    /// Both, in the C# order.
    const ALL: [Self; 2] = [Self(UuidVersion::V4), Self(UuidVersion::V7)];
}

impl fmt::Display for VersionChoice {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&match self.0 {
            UuidVersion::V4 => fl!("ui-tool-uuid-v4"),
            UuidVersion::V7 => fl!("ui-tool-uuid-v7"),
        })
    }
}

/// The UUID generator's state, as the C# view model's.
#[derive(Debug)]
pub struct UuidPane {
    version: UuidVersion,
    format: UuidFormat,
    single: Option<Uuid>,
    batch: Vec<Uuid>,
    count: String,
    single_box: Content,
    batch_box: Content,
}

impl UuidPane {
    /// A new tab's state, as the C# `Initialize`: version 4, lower case with hyphens, a
    /// batch of [`DEFAULT_BATCH_COUNT`], one UUID made.
    pub fn new() -> Self {
        let mut pane = Self {
            version: UuidVersion::V4,
            format: UuidFormat::default(),
            single: None,
            batch: Vec::new(),
            count: DEFAULT_BATCH_COUNT.to_string(),
            single_box: Content::new(),
            batch_box: Content::new(),
        };
        pane.generate();
        pane
    }

    /// Applies `message`; what a copy button copies, when one is pressed.
    pub fn update(&mut self, message: UuidMessage) -> Option<(CopySlot, String)> {
        match message {
            UuidMessage::Version(VersionChoice(version)) => {
                if version != self.version {
                    // As the C# `OnSelectedVersionChanged`: new ones of that version, the
                    // batch as large as it was.
                    self.version = version;
                    self.generate();
                    if !self.batch.is_empty() {
                        self.generate_batch(self.batch.len());
                    }
                }
            }
            UuidMessage::Uppercase(on) => {
                self.format.uppercase = on;
                self.rewrite();
            }
            UuidMessage::Hyphens(on) => {
                self.format.with_hyphens = on;
                self.rewrite();
            }
            UuidMessage::Generate => self.generate(),
            UuidMessage::CountEdited(count) => self.count = count,
            UuidMessage::GenerateBatch => {
                // As the C# `GenerateBatch`: a count not read is the fewest, and it is
                // brought within the limits and shown as taken.
                let count = self
                    .count
                    .trim()
                    .parse::<usize>()
                    .unwrap_or(MIN_BATCH_COUNT)
                    .clamp(MIN_BATCH_COUNT, MAX_BATCH_COUNT);
                self.count = count.to_string();
                self.generate_batch(count);
            }
            UuidMessage::Single(action) => super::read_only(&mut self.single_box, action),
            UuidMessage::Batch(action) => super::read_only(&mut self.batch_box, action),
            UuidMessage::Copy => return Some((CopySlot::UuidSingle, self.single_text())),
            UuidMessage::CopyBatch => return Some((CopySlot::UuidBatch, self.batch_text())),
        }
        None
    }

    /// The UUID made, written as chosen.
    #[must_use]
    pub fn single_text(&self) -> String {
        self.single
            .map(|uuid| uuid_generator::format(uuid, self.format))
            .unwrap_or_default()
    }

    /// The batch, one UUID a line and no line break after the last, as the C# `FormatBatch`.
    #[must_use]
    pub fn batch_text(&self) -> String {
        self.batch
            .iter()
            .map(|uuid| uuid_generator::format(*uuid, self.format))
            .collect::<Vec<_>>()
            .join(super::NEW_LINE)
    }

    /// A new UUID; the system giving no random bytes, the last one stays.
    fn generate(&mut self) {
        if let Ok(uuid) = uuid_generator::generate(self.version) {
            self.single = Some(uuid);
        }
        self.rewrite();
    }

    /// A batch of `count` new UUIDs.
    fn generate_batch(&mut self, count: usize) {
        self.batch = (0..count)
            .filter_map(|_| uuid_generator::generate(self.version).ok())
            .collect();
        self.rewrite();
    }

    /// The boxes written again, as the format now says.
    fn rewrite(&mut self) {
        self.single_box = Content::with_text(&self.single_text());
        self.batch_box = Content::with_text(&self.batch_text());
    }

    /// The tool's page, as the C# `UuidGeneratorView.xaml`: the UUID with Generate and Copy,
    /// the version and the format, then the batch.
    pub fn view<'a>(&'a self, tab: TabId, state: &ToolPane) -> Element<'a, Message> {
        let send = move |message| Message::Tool(tab, ToolMessage::Uuid(message));
        let label = if self.version == UuidVersion::V7 {
            fl!("ui-tool-uuid-result-v7")
        } else {
            fl!("ui-tool-uuid-result-v4")
        };
        let single = row![
            super::text_box(&self.single_box, None)
                .height(SINGLE_HEIGHT)
                .on_action(move |action| send(UuidMessage::Single(action))),
            super::action_button(
                fl!("ui-tool-uuid-generate"),
                true,
                Some(send(UuidMessage::Generate))
            ),
            super::copy_button(
                fl!("ui-tool-uuid-copy"),
                state.copied(CopySlot::UuidSingle),
                send(UuidMessage::Copy),
                super::PRIMARY_PADDING,
            ),
        ]
        .spacing(spacing::SM)
        .align_y(iced::Alignment::Center);
        let options = row![
            pick_list(
                VersionChoice::ALL,
                Some(VersionChoice(self.version)),
                move |choice| send(UuidMessage::Version(choice)),
            )
            .width(VERSION_WIDTH)
            .style(styles::pick_list)
            .menu_style(styles::menu)
            .text_size(font_size::BODY),
            checkbox(self.format.uppercase)
                .label(fl!("ui-tool-uuid-uppercase"))
                .on_toggle(move |on| send(UuidMessage::Uppercase(on)))
                .style(styles::checkbox)
                .text_size(font_size::BODY),
            checkbox(self.format.with_hyphens)
                .label(fl!("ui-tool-uuid-hyphens"))
                .on_toggle(move |on| send(UuidMessage::Hyphens(on)))
                .style(styles::checkbox)
                .text_size(font_size::BODY),
        ]
        .spacing(SECTION_GAP)
        .align_y(iced::Alignment::Center);
        let count = row![
            text(fl!("ui-tool-uuid-count")).size(font_size::BODY),
            text_input("", &self.count)
                .width(COUNT_WIDTH)
                .padding(super::INPUT_PADDING)
                .size(font_size::BODY)
                .style(styles::text_input)
                .on_input(move |typed| send(UuidMessage::CountEdited(typed)))
                .on_submit(send(UuidMessage::GenerateBatch)),
            iced::widget::space::horizontal(),
            super::action_button(
                fl!("ui-tool-uuid-generate-batch"),
                false,
                Some(send(UuidMessage::GenerateBatch))
            ),
        ]
        .spacing(spacing::SM)
        .align_y(iced::Alignment::Center);
        let batch = column![
            text(fl!("ui-tool-uuid-batch"))
                .size(font_size::BODY_LARGE)
                .font(styles::SEMIBOLD),
            count,
            super::text_box(&self.batch_box, None)
                .max_height(BATCH_MAX_HEIGHT)
                .on_action(move |action| send(UuidMessage::Batch(action))),
            super::copy_button(
                fl!("ui-tool-uuid-copy-batch"),
                state.copied(CopySlot::UuidBatch),
                send(UuidMessage::CopyBatch),
                super::PRIMARY_PADDING,
            ),
        ]
        .spacing(spacing::SM);
        super::content_column(
            column![
                text(label).size(font_size::BODY).style(text::secondary),
                single,
                options,
                container(iced::widget::space().height(1.0))
                    .width(Length::Fill)
                    .style(styles::divider),
                batch,
            ]
            .spacing(spacing::SM + spacing::XS),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_new_tab_makes_one_version_4_uuid_and_offers_a_batch_of_ten() {
        let pane = UuidPane::new();
        let single = pane.single_text();
        assert_eq!(single.chars().nth(14), Some('4'), "{single}");
        assert_eq!(single, single.to_lowercase());
        assert!(pane.batch.is_empty());
        assert_eq!(pane.count, "10");
    }

    #[test]
    fn a_format_change_writes_the_same_uuids_again() {
        let mut pane = UuidPane::new();
        let uuid = pane.single;
        let _ = pane.update(UuidMessage::Uppercase(true));
        let _ = pane.update(UuidMessage::Hyphens(false));
        assert_eq!(pane.single, uuid, "not made again");
        let text = pane.single_text();
        assert_eq!(text.len(), 32);
        assert_eq!(text, text.to_uppercase());
    }

    #[test]
    fn a_version_change_makes_new_ones_and_the_batch_keeps_its_size() {
        let mut pane = UuidPane::new();
        let _ = pane.update(UuidMessage::CountEdited("3".to_owned()));
        let _ = pane.update(UuidMessage::GenerateBatch);
        let _ = pane.update(UuidMessage::Version(VersionChoice(UuidVersion::V7)));
        assert_eq!(pane.single_text().chars().nth(14), Some('7'));
        assert_eq!(pane.batch.len(), 3);
        assert!(
            pane.batch_text()
                .lines()
                .all(|line| line.chars().nth(14) == Some('7'))
        );
    }

    #[test]
    fn the_batch_count_is_clamped_and_one_not_read_is_the_fewest() {
        let mut pane = UuidPane::new();
        for (typed, taken) in [("0", "1"), ("500", "100"), ("abc", "1"), ("7", "7")] {
            let _ = pane.update(UuidMessage::CountEdited(typed.to_owned()));
            let _ = pane.update(UuidMessage::GenerateBatch);
            assert_eq!(pane.count, taken, "{typed}");
            assert_eq!(pane.batch.len().to_string(), taken);
        }
        let text = pane.batch_text();
        assert_eq!(text.lines().count(), 7);
        assert!(!text.ends_with('\n'), "no line break after the last");
    }

    #[test]
    fn the_copy_buttons_copy_what_is_shown() {
        let mut pane = UuidPane::new();
        let single = pane.single_text();
        assert_eq!(
            pane.update(UuidMessage::Copy),
            Some((CopySlot::UuidSingle, single))
        );
        let _ = pane.update(UuidMessage::GenerateBatch);
        let batch = pane.batch_text();
        assert_eq!(
            pane.update(UuidMessage::CopyBatch),
            Some((CopySlot::UuidBatch, batch))
        );
    }
}
