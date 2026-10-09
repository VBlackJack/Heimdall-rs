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

//! The crontab builder, as the C# `CrontabBuilderView`: presets, a list for each of the
//! five fields, the expression they make with what it does and its next five runs, and the
//! expression typed by hand, checked as it is typed and applied to the lists.

use std::fmt;

use heimdall_app::TabId;
use heimdall_core::tools::cron_builder::{
    self, ANY, CronProblem, Description, FIELD_COUNT, FIELD_RANGES, Minute, NEXT_RUNS_COUNT,
    WeekDay,
};
use heimdall_core::tools::date_time::{Instant, WallClock};
use iced::widget::{Column, Row, button, column, container, pick_list, row, text, text_input};
use iced::{Element, Length};

use super::{CopySlot, ToolMessage, ToolPane};
use crate::i18n::fl;
use crate::shell::Message;
use crate::styles;
use crate::tokens::{font_size, spacing};

/// Width of a field's label, as the C# grid's 120.
const LABEL_WIDTH: f32 = 120.0;

/// The steps the minute's list offers, as the C# `PopulateComboBoxes`.
const MINUTE_STEPS: [&str; 3] = ["*/5", "*/15", "*/30"];

/// The presets, as the C# buttons' tags (`CrontabBuilderView.xaml:72-90`).
const PRESETS: [&str; 6] = [
    "* * * * *",
    "0 * * * *",
    "0 0 * * *",
    "0 9 * * 1-5",
    "0 0 * * 0",
    "0 0 1 * *",
];

/// Where a field is in an expression.
const MINUTE: usize = 0;
const HOUR: usize = 1;
const DAY_OF_MONTH: usize = 2;
const MONTH: usize = 3;
const DAY_OF_WEEK: usize = 4;

/// An entry of a field's list: what it puts in the expression, and what it shows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CronOption {
    /// The field it makes, as the C# item's `Tag`.
    pub value: String,
    /// What the list shows.
    pub label: String,
}

impl fmt::Display for CronOption {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.label)
    }
}

/// What the crontab builder is asked.
#[derive(Debug, Clone)]
pub enum CrontabMessage {
    /// A preset pressed, by its place.
    Preset(usize),
    /// Field `.0` chosen in its list.
    Field(usize, CronOption),
    /// The expression typed by hand.
    ManualEdited(String),
    /// Enter in it.
    ManualSubmitted,
    /// Copy the expression.
    Copy,
}

/// The crontab builder's state, as the C# view's.
#[derive(Debug)]
pub struct CrontabPane {
    selected: [String; FIELD_COUNT],
    expression: String,
    manual: String,
    description: Description,
    next_runs: Vec<String>,
    problem: Option<CronProblem>,
    now: fn() -> WallClock,
}

/// This computer's wall clock now.
fn local_now() -> WallClock {
    let now = Instant::now();
    now.wall_clock(heimdall_app::time_zone::local_offset_at(now))
}

impl CrontabPane {
    /// A new tab's state, as the C# `Initialize` without an argument: every field `*`.
    pub fn new() -> Self {
        Self::with_clock(local_now)
    }

    /// Its state, its runs counted from what `now` says.
    pub fn with_clock(now: fn() -> WallClock) -> Self {
        let mut pane = Self {
            selected: std::array::from_fn(|_| ANY.to_owned()),
            expression: String::new(),
            manual: String::new(),
            description: Description::EveryMinute,
            next_runs: Vec::new(),
            problem: None,
            now,
        };
        pane.apply_selectors();
        pane
    }

    /// Applies `message`; what the copy button copies, when it is pressed.
    pub fn update(&mut self, message: CrontabMessage) -> Option<(CopySlot, String)> {
        match message {
            CrontabMessage::Preset(index) => {
                if let Some(fields) = PRESETS
                    .get(index)
                    .and_then(|preset| cron_builder::parse(preset))
                {
                    // As the C# `OnPresetClick`: the lists, the expression and the box set.
                    self.problem = None;
                    self.manual = cron_builder::join(&fields);
                    self.show(&fields);
                }
            }
            CrontabMessage::Field(index, option) => {
                if let Some(field) = self.selected.get_mut(index) {
                    *field = option.value;
                    self.apply_selectors();
                }
            }
            CrontabMessage::ManualEdited(typed) => {
                self.manual = typed;
                self.check_manual();
            }
            CrontabMessage::ManualSubmitted => {
                // As the C# `OnManualInputKeyDown`: applied when it reads, ranges unchecked.
                let typed = self.manual.trim().to_owned();
                if let Some(fields) = cron_builder::parse(&typed) {
                    self.problem = None;
                    self.show(&fields);
                    self.expression = typed;
                }
            }
            CrontabMessage::Copy => {
                return Some((CopySlot::CrontabExpression, self.expression.clone()));
            }
        }
        None
    }

    /// The expression the lists make, as the C# `UpdateFromSelectors`: the box typed in
    /// set to it too.
    fn apply_selectors(&mut self) {
        self.expression = cron_builder::join(&self.selected);
        self.manual = self.expression.clone();
        self.description = cron_builder::describe(&self.selected);
        self.next_runs = self.runs(&self.selected.clone());
    }

    /// `fields` shown, as the C#'s `ApplyCronToSelectors` then the expression, its
    /// description and its runs: a list without the field's entry goes back to `*`.
    fn show(&mut self, fields: &[String; FIELD_COUNT]) {
        for (index, field) in fields.iter().enumerate() {
            let listed = options(index).iter().any(|option| option.value == *field);
            self.selected[index] = if listed {
                field.clone()
            } else {
                ANY.to_owned()
            };
        }
        self.expression = cron_builder::join(fields);
        self.description = cron_builder::describe(fields);
        self.next_runs = self.runs(fields);
    }

    /// The next runs of `fields` from now.
    fn runs(&self, fields: &[String; FIELD_COUNT]) -> Vec<String> {
        cron_builder::next_runs(fields, Minute::of(&(self.now)()), NEXT_RUNS_COUNT)
    }

    /// The box typed in checked, as the C# `OnManualInputChanged`
    /// (`CrontabBuilderView.xaml.cs:252-312`): nothing typed hides the error and leaves the
    /// rest; a problem is said; an expression without one is applied.
    fn check_manual(&mut self) {
        let typed = self.manual.trim().to_owned();
        if typed.is_empty() {
            self.problem = None;
            return;
        }
        self.problem = cron_builder::validate(&typed);
        if self.problem.is_none()
            && let Some(fields) = cron_builder::parse(&typed)
        {
            self.show(&fields);
            self.expression = typed;
        }
    }

    /// The expression.
    #[cfg(test)]
    #[must_use]
    pub fn expression(&self) -> &str {
        &self.expression
    }

    /// The box typed in.
    #[cfg(test)]
    #[must_use]
    pub fn manual(&self) -> &str {
        &self.manual
    }

    /// The field each list shows.
    #[cfg(test)]
    #[must_use]
    pub fn selected(&self) -> &[String; FIELD_COUNT] {
        &self.selected
    }

    /// The next runs.
    #[cfg(test)]
    #[must_use]
    pub fn next_runs(&self) -> &[String] {
        &self.next_runs
    }

    /// What the expression does, as the C# `DescribeCron` writes it.
    #[must_use]
    pub fn description_text(&self) -> String {
        match &self.description {
            Description::EveryMinute => fl!("ui-tool-crontab-desc-every-minute"),
            Description::EveryHour => fl!("ui-tool-crontab-desc-every-hour"),
            Description::EveryDay => fl!("ui-tool-crontab-desc-every-day"),
            Description::EveryNMinutes(interval) => {
                fl!(
                    "ui-tool-crontab-desc-every-n-min",
                    interval = interval.as_str()
                )
            }
            Description::DailyAt(time) => {
                fl!("ui-tool-crontab-desc-daily-at", time = time.as_str())
            }
            Description::WeeklyAt(day, time) => {
                let day = match day {
                    WeekDay::Day(day) => day_name(*day),
                    WeekDay::Field(field) => field.clone(),
                };
                fl!(
                    "ui-tool-crontab-desc-weekly-at",
                    day = day,
                    time = time.as_str()
                )
            }
            Description::MonthlyAt(day, time) => {
                let day = *day;
                fl!(
                    "ui-tool-crontab-desc-monthly-at",
                    day = day,
                    time = time.as_str()
                )
            }
            Description::Custom(expression) => {
                fl!(
                    "ui-tool-crontab-desc-custom",
                    expression = expression.as_str()
                )
            }
        }
    }

    /// What is wrong with the box typed in, as the C# `TxtValidationError`.
    #[must_use]
    pub fn problem_text(&self) -> Option<String> {
        Some(match self.problem.as_ref()? {
            CronProblem::FieldCount => fl!("ui-tool-crontab-error-field-count"),
            CronProblem::InvalidField { field, text } => fl!(
                "ui-tool-crontab-error-invalid-field",
                field = field_label(*field),
                value = text.as_str()
            ),
            CronProblem::OutOfRange { field } => {
                let (min, max) = FIELD_RANGES[*field];
                fl!(
                    "ui-tool-crontab-error-out-of-range",
                    field = field_label(*field),
                    min = min,
                    max = max
                )
            }
        })
    }

    /// The tool's page, as the C# `CrontabBuilderView.xaml`: the presets, the five lists,
    /// the expression and its copy, what it does, the box to type it in, and its next runs.
    pub fn view<'a>(&'a self, tab: TabId, state: &ToolPane) -> Element<'a, Message> {
        let send = move |message| Message::Tool(tab, ToolMessage::Crontab(message));
        let preset_labels = [
            fl!("ui-tool-crontab-preset-every-minute"),
            fl!("ui-tool-crontab-preset-every-hour"),
            fl!("ui-tool-crontab-preset-daily-midnight"),
            fl!("ui-tool-crontab-preset-weekdays-9am"),
            fl!("ui-tool-crontab-preset-weekly-sunday"),
            fl!("ui-tool-crontab-preset-monthly-1st"),
        ];
        let presets =
            Row::with_children(preset_labels.into_iter().enumerate().map(|(index, label)| {
                button(text(label).size(font_size::BODY))
                    .padding(super::COPY_PADDING)
                    .style(styles::secondary)
                    .on_press(send(CrontabMessage::Preset(index)))
                    .into()
            }))
            .spacing(spacing::SM)
            .wrap()
            .vertical_spacing(spacing::SM);
        let mut lists = Column::new().spacing(spacing::SM);
        for index in 0..FIELD_COUNT {
            let choices = options(index);
            let chosen = choices
                .iter()
                .find(|option| option.value == self.selected[index])
                .cloned();
            lists = lists.push(
                row![
                    text(field_label(index))
                        .size(font_size::BODY)
                        .width(LABEL_WIDTH),
                    pick_list(choices, chosen, move |option| send(CrontabMessage::Field(
                        index, option
                    )))
                    .width(Length::Fill)
                    .style(styles::pick_list)
                    .menu_style(styles::menu)
                    .text_size(font_size::BODY),
                ]
                .align_y(iced::Alignment::Center),
            );
        }
        let expression = container(
            row![
                text(&self.expression)
                    .font(super::BOX_FONT)
                    .size(font_size::BODY_LARGE)
                    .style(|theme: &iced::Theme| text::Style {
                        color: Some(theme.palette().primary),
                    })
                    .width(Length::Fill),
                super::copy_button(
                    fl!("ui-tool-crontab-copy"),
                    state.copied(CopySlot::CrontabExpression),
                    send(CrontabMessage::Copy),
                    super::COPY_PADDING,
                ),
            ]
            .align_y(iced::Alignment::Center),
        )
        .padding(spacing::MD)
        .width(Length::Fill)
        .style(styles::card);
        let runs =
            container(Column::with_children(self.next_runs.iter().map(|run| {
                text(run).font(super::BOX_FONT).size(font_size::BODY).into()
            })))
            .padding(spacing::MD)
            .width(Length::Fill)
            .style(styles::card);
        super::content_column(
            column![
                super::field_label(fl!("ui-tool-crontab-presets")),
                presets,
                lists,
                expression,
                text(self.description_text()).size(font_size::BODY),
                super::field_label(fl!("ui-tool-crontab-manual-edit")),
                text_input(&fl!("ui-tool-crontab-placeholder"), &self.manual)
                    .font(super::BOX_FONT)
                    .size(font_size::BODY_LARGE)
                    .padding(super::INPUT_PADDING)
                    .style(styles::text_input)
                    .on_input(move |typed| send(CrontabMessage::ManualEdited(typed)))
                    .on_submit(send(CrontabMessage::ManualSubmitted)),
            ]
            .push(
                self.problem_text()
                    .map(|problem| text(problem).size(font_size::CAPTION).style(text::danger)),
            )
            .push(super::field_label(fl!("ui-tool-crontab-next-runs")))
            .push(runs)
            .spacing(spacing::SM + spacing::XS),
        )
    }
}

/// The name of field `index`, as the C# labels.
fn field_label(index: usize) -> String {
    match index {
        MINUTE => fl!("ui-tool-crontab-minute"),
        HOUR => fl!("ui-tool-crontab-hour"),
        DAY_OF_MONTH => fl!("ui-tool-crontab-day-of-month"),
        MONTH => fl!("ui-tool-crontab-month"),
        _ => fl!("ui-tool-crontab-day-of-week"),
    }
}

/// The name of day `day`, Sunday being 0, as the C# `GetDayName`.
fn day_name(day: usize) -> String {
    match day {
        0 => fl!("ui-tool-crontab-day-0"),
        1 => fl!("ui-tool-crontab-day-1"),
        2 => fl!("ui-tool-crontab-day-2"),
        3 => fl!("ui-tool-crontab-day-3"),
        4 => fl!("ui-tool-crontab-day-4"),
        5 => fl!("ui-tool-crontab-day-5"),
        _ => fl!("ui-tool-crontab-day-6"),
    }
}

/// The abbreviation of month `month`, 1 to 12.
fn month_abbreviation(month: i64) -> String {
    match month {
        1 => fl!("ui-tool-crontab-month-1"),
        2 => fl!("ui-tool-crontab-month-2"),
        3 => fl!("ui-tool-crontab-month-3"),
        4 => fl!("ui-tool-crontab-month-4"),
        5 => fl!("ui-tool-crontab-month-5"),
        6 => fl!("ui-tool-crontab-month-6"),
        7 => fl!("ui-tool-crontab-month-7"),
        8 => fl!("ui-tool-crontab-month-8"),
        9 => fl!("ui-tool-crontab-month-9"),
        10 => fl!("ui-tool-crontab-month-10"),
        11 => fl!("ui-tool-crontab-month-11"),
        _ => fl!("ui-tool-crontab-month-12"),
    }
}

/// The abbreviation of day `day`, Sunday being 0.
fn day_abbreviation(day: i64) -> String {
    match day {
        0 => fl!("ui-tool-crontab-day-abbr-0"),
        1 => fl!("ui-tool-crontab-day-abbr-1"),
        2 => fl!("ui-tool-crontab-day-abbr-2"),
        3 => fl!("ui-tool-crontab-day-abbr-3"),
        4 => fl!("ui-tool-crontab-day-abbr-4"),
        5 => fl!("ui-tool-crontab-day-abbr-5"),
        _ => fl!("ui-tool-crontab-day-abbr-6"),
    }
}

/// The entries of field `index`'s list, as the C# `PopulateComboBoxes`
/// (`CrontabBuilderView.xaml.cs:148-209`): "every" first, then each value.
fn options(index: usize) -> Vec<CronOption> {
    let option = |value: String, label: String| CronOption { value, label };
    let every = match index {
        MINUTE => fl!("ui-tool-crontab-every-minute"),
        HOUR => fl!("ui-tool-crontab-every-hour"),
        DAY_OF_MONTH => fl!("ui-tool-crontab-every-day"),
        MONTH => fl!("ui-tool-crontab-every-month"),
        _ => fl!("ui-tool-crontab-every-day-of-week"),
    };
    let mut options = vec![option(ANY.to_owned(), every)];
    if index == MINUTE {
        let steps = [
            fl!("ui-tool-crontab-every-5-min"),
            fl!("ui-tool-crontab-every-15-min"),
            fl!("ui-tool-crontab-every-30-min"),
        ];
        options.extend(
            MINUTE_STEPS
                .into_iter()
                .zip(steps)
                .map(|(step, label)| option(step.to_owned(), label)),
        );
    }
    let (min, max) = FIELD_RANGES[index.min(DAY_OF_WEEK)];
    options.extend((min..=max).map(|value| {
        let label = match index {
            HOUR => format!("{value:02}:00"),
            MONTH => format!("{value} ({})", month_abbreviation(value)),
            DAY_OF_WEEK => format!("{} ({value})", day_abbreviation(value)),
            _ => value.to_string(),
        };
        option(value.to_string(), label)
    }));
    options
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Friday 2026-10-09 14:05 on the wall clock.
    fn fixed_now() -> WallClock {
        Instant::from_unix_seconds(1_791_554_700).wall_clock(0)
    }

    fn pane() -> CrontabPane {
        CrontabPane::with_clock(fixed_now)
    }

    #[test]
    fn a_new_tab_runs_every_minute() {
        let pane = pane();
        assert_eq!(pane.expression(), "* * * * *");
        assert_eq!(pane.manual(), "* * * * *");
        assert_eq!(pane.description_text(), "Runs every minute");
        assert_eq!(pane.next_runs().len(), NEXT_RUNS_COUNT);
        assert_eq!(pane.next_runs()[0], "2026-10-09 14:06 (Fri)");
    }

    #[test]
    fn a_list_chosen_makes_the_expression() {
        let mut pane = pane();
        let hour = options(HOUR)
            .into_iter()
            .find(|option| option.value == "9")
            .expect("09:00");
        assert_eq!(hour.label, "09:00");
        let _ = pane.update(CrontabMessage::Field(HOUR, hour));
        let minute = options(MINUTE)
            .into_iter()
            .find(|option| option.value == "30")
            .expect("30");
        let _ = pane.update(CrontabMessage::Field(MINUTE, minute));
        assert_eq!(pane.expression(), "30 9 * * *");
        assert_eq!(pane.manual(), "30 9 * * *");
        assert_eq!(pane.description_text(), "Runs daily at 09:30");
        assert_eq!(pane.next_runs()[0], "2026-10-10 09:30 (Sat)");
    }

    #[test]
    fn a_preset_sets_the_lists_and_what_is_not_listed_shows_every() {
        let mut pane = pane();
        let _ = pane.update(CrontabMessage::Preset(3));
        assert_eq!(pane.expression(), "0 9 * * 1-5");
        assert_eq!(pane.manual(), "0 9 * * 1-5");
        assert_eq!(pane.selected()[DAY_OF_WEEK], "*", "1-5 is not listed");
        assert_eq!(pane.selected()[HOUR], "9");
        assert_eq!(pane.description_text(), "Runs every 1-5 at 09:00");
        assert_eq!(pane.next_runs()[0], "2026-10-12 09:00 (Mon)");
        let _ = pane.update(CrontabMessage::Preset(4));
        assert_eq!(pane.description_text(), "Runs every Sunday at 00:00");
    }

    #[test]
    fn a_typed_expression_is_checked_then_applied() {
        let mut pane = pane();
        let _ = pane.update(CrontabMessage::ManualEdited("* * *".to_owned()));
        assert_eq!(
            pane.problem_text().as_deref(),
            Some("A cron expression must have exactly 5 fields separated by spaces")
        );
        assert_eq!(pane.expression(), "* * * * *", "left as it was");
        let _ = pane.update(CrontabMessage::ManualEdited("x * * * *".to_owned()));
        assert_eq!(
            pane.problem_text().as_deref(),
            Some("Invalid characters in field \"Minute\": x")
        );
        let _ = pane.update(CrontabMessage::ManualEdited("* 24 * * *".to_owned()));
        assert_eq!(
            pane.problem_text().as_deref(),
            Some("Field \"Hour\" values must be between 0 and 23")
        );
        let _ = pane.update(CrontabMessage::ManualEdited(" 0 0 1 * * ".to_owned()));
        assert_eq!(pane.problem_text(), None);
        assert_eq!(pane.expression(), "0 0 1 * *");
        assert_eq!(pane.selected()[DAY_OF_MONTH], "1");
        assert_eq!(
            pane.description_text(),
            "Runs on the 1th of every month at 00:00"
        );
        let _ = pane.update(CrontabMessage::ManualEdited(String::new()));
        assert_eq!(pane.problem_text(), None);
        assert_eq!(pane.expression(), "0 0 1 * *", "nothing typed leaves it");
    }

    #[test]
    fn enter_applies_an_expression_that_reads_without_its_ranges() {
        let mut pane = pane();
        let _ = pane.update(CrontabMessage::ManualEdited("99 * * * *".to_owned()));
        assert!(pane.problem_text().is_some());
        let _ = pane.update(CrontabMessage::ManualSubmitted);
        assert_eq!(pane.problem_text(), None);
        assert_eq!(pane.expression(), "99 * * * *");
        assert!(pane.next_runs().is_empty(), "no minute 99");
    }

    #[test]
    fn the_copy_button_copies_the_expression() {
        let mut pane = pane();
        let _ = pane.update(CrontabMessage::Preset(1));
        assert_eq!(
            pane.update(CrontabMessage::Copy),
            Some((CopySlot::CrontabExpression, "0 * * * *".to_owned()))
        );
    }

    #[test]
    fn the_lists_hold_the_csharp_entries() {
        assert_eq!(options(MINUTE).len(), 1 + 3 + 60);
        assert_eq!(options(HOUR).len(), 1 + 24);
        assert_eq!(options(DAY_OF_MONTH).len(), 1 + 31);
        assert_eq!(options(MONTH)[1].label, "1 (Jan)");
        assert_eq!(options(DAY_OF_WEEK)[1].label, "Sun (0)");
    }
}
