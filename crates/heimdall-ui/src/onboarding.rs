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

//! The welcome tour, as the C# onboarding overlay (`OnboardingFlowViewModel.cs`,
//! `MainWindow.xaml:5650-5760`): a card over the veiled window that steps through where first
//! users were seen to stumble, with Skip and Next, and Get Started on the last step. Shown at
//! the first start, and again from the Settings page's "Show the tour again".
//!
//! The C# card cuts a hole in its veil around the control a step is about and moves out of its
//! way. iced tells a view nothing of where another widget was laid out, so the card here is
//! always centred, as the C# one is when a step's control cannot be found; each step still
//! opens the page it speaks of first, as the C# `ApplyOnboardingStep` does.

use heimdall_app::tools::ToolId;
use iced::widget::{Row, button, column, container, row, text};
use iced::{Element, Length};

use crate::i18n::fl;
use crate::shell::Message;
use crate::styles;
use crate::tokens::{font_size, spacing};

/// Widest the card is, as the C# `MaxWidth="560"`.
const CARD_MAX_WIDTH: f32 = 560.0;

/// Room inside the card, as the C# `Padding="32,24"`: above and below, then at the sides.
const CARD_PADDING: [f32; 2] = [24.0, 32.0];

/// Side of a dot of the step indicator, as the C# `Ellipse Width="8" Height="8"`.
const DOT_SIZE: f32 = 8.0;

/// Room between two dots, as the C# `Margin="4,0"` on each.
const DOT_GAP: f32 = 8.0;

/// Room under the dots, as the C# `Margin="0,0,0,20"`.
const DOTS_MARGIN: f32 = 20.0;

/// Room under the step's text, as the C# `Margin="0,0,0,24"`.
const BODY_MARGIN: f32 = 24.0;

/// Room inside the Skip button, as the C# `Padding="12,6"`.
const SKIP_PADDING: [f32; 2] = [6.0, 12.0];

/// Room inside the Next button, as the C# `Padding="20,8"`.
const NEXT_PADDING: [f32; 2] = [8.0, 20.0];

/// A step of the tour, in the C# order (`OnboardingFlowViewModel.Steps`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TourStep {
    /// The sessions list: "Connect to Sessions".
    Sessions,
    /// The button that adds a session: "Add your first session".
    Add,
    /// The tree's search: "Find anything quickly".
    Search,
    /// The Tools page: "Built-in Tools".
    Tools,
    /// Quick Connect: "Quick Connect".
    QuickConnect,
    /// The Settings page, where the tour is shown again: "Settings, and this tour".
    Settings,
}

impl TourStep {
    /// Every step, in the order shown.
    pub const ALL: [Self; 6] = [
        Self::Sessions,
        Self::Add,
        Self::Search,
        Self::Tools,
        Self::QuickConnect,
        Self::Settings,
    ];

    /// Its heading.
    #[must_use]
    pub fn title(self) -> String {
        match self {
            Self::Sessions => fl!("ui-onboarding-sessions-title"),
            Self::Add => fl!("ui-onboarding-add-title"),
            Self::Search => fl!("ui-onboarding-search-title"),
            Self::Tools => fl!("ui-onboarding-tools-title"),
            Self::QuickConnect => fl!("ui-onboarding-quick-connect-title"),
            Self::Settings => fl!("ui-onboarding-settings-title"),
        }
    }

    /// What it says under its heading. The C# names its own count of tools; this one names
    /// the tools this version has.
    #[must_use]
    pub fn body(self) -> String {
        match self {
            Self::Sessions => fl!("ui-onboarding-sessions-body"),
            Self::Add => fl!("ui-onboarding-add-body"),
            Self::Search => fl!("ui-onboarding-search-body"),
            Self::Tools => fl!("ui-onboarding-tools-body", count = ToolId::ALL.len()),
            Self::QuickConnect => fl!("ui-onboarding-quick-connect-body"),
            Self::Settings => fl!("ui-onboarding-settings-body"),
        }
    }

    /// Whether the Sessions page is shown before it, as the C# steps whose `ShellTab` is
    /// `Sessions`: what it speaks of is there. The others leave the page as it is.
    #[must_use]
    pub fn shows_sessions(self) -> bool {
        matches!(self, Self::Sessions | Self::Add | Self::Search)
    }
}

/// A press on the tour's card, or the key that stands for one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TourMessage {
    /// Next, or Get Started on the last step; Enter.
    Next,
    /// Skip; Escape. The tour ends, recorded as seen, as the C# `SkipAsync` and
    /// `EscapeAsync`.
    Skip,
}

/// The tour shown: its step, and whether its end could not be recorded.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Tour {
    /// The step shown, by its place in [`TourStep::ALL`].
    index: usize,
    /// The tour's end could not be saved: said on the card, which stays, as the C#
    /// `CompletionErrorText`.
    save_failed: bool,
}

impl Tour {
    /// The tour from its first step, as the C# `Start`.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// The step shown.
    #[must_use]
    pub fn step(&self) -> TourStep {
        TourStep::ALL[self.index.min(TourStep::ALL.len() - 1)]
    }

    /// Whether the step shown is the last: its button is Get Started.
    #[must_use]
    pub fn is_last(&self) -> bool {
        self.index + 1 >= TourStep::ALL.len()
    }

    /// The next step shown; `false`, and nothing changed, on the last.
    pub fn advance(&mut self) -> bool {
        if self.is_last() {
            return false;
        }
        self.index += 1;
        true
    }

    /// Whether its end could not be recorded.
    #[must_use]
    pub fn save_failed(&self) -> bool {
        self.save_failed
    }

    /// Its end could not be recorded, or was since.
    pub fn set_save_failed(&mut self, failed: bool) {
        self.save_failed = failed;
    }
}

/// The card of `tour`: the step indicator, the step's heading and text, what went wrong
/// recording its end, then Skip and Next.
pub fn card<'a>(tour: &Tour) -> Element<'a, Message> {
    let step = tour.step();
    let dots = Row::with_children(TourStep::ALL.into_iter().map(|each| {
        container(iced::widget::space())
            .width(DOT_SIZE)
            .height(DOT_SIZE)
            .style(styles::step_dot(each == step))
            .into()
    }))
    .spacing(DOT_GAP);
    let next = if tour.is_last() {
        fl!("ui-onboarding-get-started")
    } else {
        fl!("ui-onboarding-next")
    };
    let buttons = row![
        button(text(fl!("ui-onboarding-skip")).style(text::secondary))
            .style(styles::subtle)
            .padding(SKIP_PADDING)
            .on_press(Message::Tour(TourMessage::Skip)),
        iced::widget::space::horizontal(),
        button(text(next))
            .style(styles::primary)
            .padding(NEXT_PADDING)
            .on_press(Message::Tour(TourMessage::Next)),
    ]
    .align_y(iced::Alignment::Center);
    let content = column![
        container(dots)
            .center_x(Length::Fill)
            .padding(iced::Padding::ZERO.bottom(DOTS_MARGIN)),
        text(step.title())
            .size(font_size::HEADLINE)
            .font(crate::detail_view::BOLD)
            .width(Length::Fill)
            .center(),
        container(
            text(step.body())
                .size(font_size::BODY_LARGE)
                .style(text::secondary)
                .width(Length::Fill)
                .center()
        )
        .padding(iced::Padding::ZERO.bottom(BODY_MARGIN)),
    ]
    .spacing(spacing::SM)
    .push(tour.save_failed().then(|| {
        text(fl!("ui-onboarding-save-failed"))
            .style(text::danger)
            .width(Length::Fill)
            .center()
    }))
    .push(buttons);
    container(content)
        .padding(CARD_PADDING)
        .width(Length::Fill)
        .max_width(CARD_MAX_WIDTH)
        .style(styles::dialog)
        .into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_tour_steps_through_the_c_sharp_steps_in_order_and_stops_on_the_last() {
        let mut tour = Tour::new();
        let mut shown = vec![tour.step()];
        while tour.advance() {
            shown.push(tour.step());
        }
        assert_eq!(shown, TourStep::ALL);
        assert!(tour.is_last());
        assert!(!tour.advance(), "nothing after the last");
        assert_eq!(tour.step(), TourStep::Settings);
        // The first three open the Sessions page, as their C# `ShellTab`.
        let sessions: Vec<TourStep> = TourStep::ALL
            .into_iter()
            .filter(|step| step.shows_sessions())
            .collect();
        assert_eq!(
            sessions,
            [TourStep::Sessions, TourStep::Add, TourStep::Search]
        );
    }

    #[test]
    fn every_step_has_its_words() {
        for step in TourStep::ALL {
            assert!(!step.title().is_empty(), "{step:?}");
            assert!(!step.body().is_empty(), "{step:?}");
        }
    }
}
