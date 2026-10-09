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

//! The ULID generator, as the C# `UlidGeneratorView` and `UlidGeneratorViewModel`: one ULID
//! made when the tab opens and on Generate, a batch of 1 to 100 on Generate Batch or Enter
//! in its count, each copied.

use heimdall_app::TabId;
use heimdall_core::tools::ulid_generator;
use iced::widget::text_editor::{Action, Content};
use iced::widget::{column, container, row, text, text_input};
use iced::{Element, Length};

use super::{CopySlot, ToolMessage, ToolPane};
use crate::i18n::fl;
use crate::shell::Message;
use crate::styles;
use crate::tokens::{font_size, spacing};

/// The fewest ULIDs of a batch, as the C# `MinBatchCount`.
pub const MIN_BATCH_COUNT: usize = 1;

/// The most, as the C# `MaxBatchCount`.
pub const MAX_BATCH_COUNT: usize = 100;

/// The count a batch starts at, as the C# `DefaultBatchCount`.
pub const DEFAULT_BATCH_COUNT: usize = 10;

/// Width of the count's box, as the C#'s.
const COUNT_WIDTH: f32 = 60.0;

/// Height of the single ULID's box: one line.
const SINGLE_HEIGHT: f32 = 36.0;

/// Height of the batch's box before it scrolls, as the C# `MaxHeight`.
const BATCH_MAX_HEIGHT: f32 = 300.0;

/// What the ULID generator is asked.
#[derive(Debug, Clone)]
pub enum UlidMessage {
    /// Generate one ULID.
    Generate,
    /// The batch's count typed.
    CountEdited(String),
    /// Generate a batch.
    GenerateBatch,
    /// Something done in the single ULID's box, which does not change it.
    Single(Action),
    /// Something done in the batch's box, which does not change it.
    Batch(Action),
    /// Copy the ULID.
    Copy,
    /// Copy the batch.
    CopyBatch,
}

/// The ULID generator's state, as the C# view model's.
#[derive(Debug)]
pub struct UlidPane {
    single: String,
    batch: String,
    count: String,
    single_box: Content,
    batch_box: Content,
    make: fn() -> Option<String>,
}

/// A new ULID, the system giving random bytes.
fn make_ulid() -> Option<String> {
    ulid_generator::generate().ok()
}

impl UlidPane {
    /// A new tab's state, as the C# `Initialize`: a batch of [`DEFAULT_BATCH_COUNT`] offered,
    /// one ULID made.
    pub fn new() -> Self {
        Self::with_maker(make_ulid)
    }

    /// Its state, its ULIDs made by `make`, as the C# view model takes its service.
    pub fn with_maker(make: fn() -> Option<String>) -> Self {
        let mut pane = Self {
            single: String::new(),
            batch: String::new(),
            count: DEFAULT_BATCH_COUNT.to_string(),
            single_box: Content::new(),
            batch_box: Content::new(),
            make,
        };
        pane.generate();
        pane
    }

    /// Applies `message`; what a copy button copies, when one is pressed.
    pub fn update(&mut self, message: UlidMessage) -> Option<(CopySlot, String)> {
        match message {
            UlidMessage::Generate => self.generate(),
            UlidMessage::CountEdited(count) => self.count = count,
            UlidMessage::GenerateBatch => self.generate_batch(),
            UlidMessage::Single(action) => super::read_only(&mut self.single_box, action),
            UlidMessage::Batch(action) => super::read_only(&mut self.batch_box, action),
            UlidMessage::Copy => return Some((CopySlot::UlidSingle, self.single.clone())),
            UlidMessage::CopyBatch => return Some((CopySlot::UlidBatch, self.batch.clone())),
        }
        None
    }

    /// The ULID made.
    #[cfg(test)]
    #[must_use]
    pub fn single_text(&self) -> &str {
        &self.single
    }

    /// The batch, one ULID a line and no line break after the last.
    #[cfg(test)]
    #[must_use]
    pub fn batch_text(&self) -> &str {
        &self.batch
    }

    /// The batch's count as typed.
    #[cfg(test)]
    #[must_use]
    pub fn count_text(&self) -> &str {
        &self.count
    }

    /// A new ULID, as the C# `Generate`; the system giving no random bytes, the last stays.
    fn generate(&mut self) {
        if let Some(ulid) = (self.make)() {
            self.single = ulid;
            self.single_box = Content::with_text(&self.single);
        }
    }

    /// A batch, as the C# `GenerateBatch` (`UlidGeneratorViewModel.cs:154-182`): a count not
    /// read is the fewest, and it is brought within the limits and shown as taken.
    fn generate_batch(&mut self) {
        let count = self
            .count
            .trim()
            .parse::<usize>()
            .unwrap_or(MIN_BATCH_COUNT)
            .clamp(MIN_BATCH_COUNT, MAX_BATCH_COUNT);
        self.count = count.to_string();
        self.batch = (0..count)
            .filter_map(|_| (self.make)())
            .collect::<Vec<_>>()
            .join(super::NEW_LINE);
        self.batch_box = Content::with_text(&self.batch);
    }

    /// The tool's page, as the C# `UlidGeneratorView.xaml`: the ULID with Generate and Copy,
    /// then the batch, its count, its box and Copy all.
    pub fn view<'a>(&'a self, tab: TabId, state: &ToolPane) -> Element<'a, Message> {
        let send = move |message| Message::Tool(tab, ToolMessage::Ulid(message));
        let single = row![
            super::text_box(&self.single_box, None)
                .height(SINGLE_HEIGHT)
                .on_action(move |action| send(UlidMessage::Single(action))),
            super::action_button(
                fl!("ui-tool-ulid-generate"),
                true,
                Some(send(UlidMessage::Generate))
            ),
            super::copy_button(
                fl!("ui-tool-ulid-copy"),
                state.copied(CopySlot::UlidSingle),
                send(UlidMessage::Copy),
                super::PRIMARY_PADDING,
            ),
        ]
        .spacing(spacing::SM)
        .align_y(iced::Alignment::Center);
        let count = row![
            text(fl!("ui-tool-ulid-count"))
                .size(font_size::BODY)
                .style(text::secondary),
            text_input("", &self.count)
                .width(COUNT_WIDTH)
                .padding(super::INPUT_PADDING)
                .size(font_size::BODY_LARGE)
                .style(styles::text_input)
                .on_input(move |typed| send(UlidMessage::CountEdited(typed)))
                .on_submit(send(UlidMessage::GenerateBatch)),
            iced::widget::space::horizontal(),
            super::action_button(
                fl!("ui-tool-ulid-generate-batch"),
                false,
                Some(send(UlidMessage::GenerateBatch))
            ),
        ]
        .spacing(spacing::SM)
        .align_y(iced::Alignment::Center);
        let batch = column![
            text(fl!("ui-tool-ulid-batch"))
                .size(font_size::BODY)
                .style(text::secondary),
            count,
            super::text_box(&self.batch_box, None)
                .size(font_size::BODY)
                .max_height(BATCH_MAX_HEIGHT)
                .on_action(move |action| send(UlidMessage::Batch(action))),
            super::copy_button(
                fl!("ui-tool-ulid-copy-batch"),
                state.copied(CopySlot::UlidBatch),
                send(UlidMessage::CopyBatch),
                super::PRIMARY_PADDING,
            ),
        ]
        .spacing(spacing::SM);
        super::content_column(
            column![
                text(fl!("ui-tool-ulid-result"))
                    .size(font_size::BODY)
                    .style(text::secondary),
                single,
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
    use std::sync::atomic::{AtomicUsize, Ordering};

    use super::*;

    static MADE: AtomicUsize = AtomicUsize::new(0);

    /// ULIDs counted in turn, as the C# tests' `QueueService`.
    #[expect(clippy::unnecessary_wraps, reason = "the maker's signature")]
    fn counted() -> Option<String> {
        Some(format!("U{}", MADE.fetch_add(1, Ordering::SeqCst)))
    }

    #[test]
    fn a_new_tab_makes_one_ulid_and_offers_a_batch_of_ten() {
        let pane = UlidPane::new();
        assert_eq!(pane.single_text().len(), ulid_generator::TEXT_LENGTH);
        assert!(pane.batch_text().is_empty());
        assert_eq!(pane.count_text(), "10");
    }

    #[test]
    fn generate_makes_another() {
        let mut pane = UlidPane::with_maker(counted);
        let first = pane.single_text().to_owned();
        let _ = pane.update(UlidMessage::Generate);
        assert_ne!(pane.single_text(), first);
    }

    #[test]
    fn the_batch_count_is_clamped_and_one_not_read_is_the_fewest() {
        let mut pane = UlidPane::new();
        for (typed, taken) in [("0", "1"), ("500", "100"), ("abc", "1"), ("5", "5")] {
            let _ = pane.update(UlidMessage::CountEdited(typed.to_owned()));
            let _ = pane.update(UlidMessage::GenerateBatch);
            assert_eq!(pane.count_text(), taken, "{typed}");
            assert_eq!(pane.batch_text().lines().count().to_string(), taken);
        }
        assert!(
            !pane.batch_text().ends_with('\n'),
            "no line break after the last"
        );
        assert!(pane.batch_text().contains(super::super::NEW_LINE));
    }

    #[test]
    fn the_copy_buttons_copy_what_is_shown() {
        let mut pane = UlidPane::new();
        let single = pane.single_text().to_owned();
        assert_eq!(
            pane.update(UlidMessage::Copy),
            Some((CopySlot::UlidSingle, single))
        );
        let _ = pane.update(UlidMessage::GenerateBatch);
        let batch = pane.batch_text().to_owned();
        assert_eq!(
            pane.update(UlidMessage::CopyBatch),
            Some((CopySlot::UlidBatch, batch))
        );
    }
}
