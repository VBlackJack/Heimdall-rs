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

//! The date and time converter, as the C# `DateTimeConverterView` and
//! `DateTimeConverterViewModel`: a Unix time or a written date converted 150 ms after it is
//! typed, or at once on Enter; shown as a Unix time, in the round-trip form in UTC and in
//! this computer's time, as this computer's long date, in a time zone chosen, and as how
//! far it is from now; each copied.

use std::fmt;
use std::time::Duration;

use heimdall_app::TabId;
use heimdall_core::tools::date_time::{
    self, DetectedFormat, Instant, Parsed, RelativeDuration, RelativeUnit, WallClock,
};
use heimdall_core::tools::time_zone_rules::{self, TimeZoneEntry};
use iced::widget::{column, combo_box, row, text, text_input};
use iced::{Element, Task};

use super::{CopySlot, ToolMessage, ToolPane};
use crate::i18n::fl;
use crate::shell::Message;
use crate::styles;
use crate::tokens::{font_size, spacing};

/// How long typing rests before the input is converted, as the C# `DispatcherTimer`'s
/// 150 ms (`DateTimeConverterView.xaml.cs:63`).
pub const DEBOUNCE: Duration = Duration::from_millis(150);

/// Width of the time zones' list, between the C#'s `MinWidth` of 200 and `MaxWidth` of
/// 400.
const ZONE_WIDTH: f32 = 300.0;

/// A field of the converter a copy button copies.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DateField {
    /// The Unix time.
    Unix,
    /// The round-trip form in UTC.
    IsoUtc,
    /// The round-trip form in this computer's time.
    IsoLocal,
    /// This computer's long date.
    LocalTime,
    /// The long date in the zone chosen.
    Zone,
}

/// What the converter is asked.
#[derive(Debug, Clone)]
pub enum DateTimeMessage {
    /// The input typed.
    InputEdited(String),
    /// Convert now, as Enter.
    Convert,
    /// Typing rested: convert, if `.0` is still the last edit.
    Rested(u64),
    /// Put this moment's Unix time in, as the C# Now.
    Now,
    /// A time zone chosen.
    Zone(ZoneChoice),
    /// Copy a field.
    Copy(DateField),
}

/// A time zone of the list, named as .NET's `DisplayName`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ZoneChoice {
    /// Its identifier.
    pub id: String,
    /// Its name.
    pub name: String,
}

impl fmt::Display for ZoneChoice {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.name)
    }
}

/// Where this computer's clock and zones come from: the system's, or a test's.
#[derive(Debug, Clone, Copy)]
pub struct Clock {
    /// This moment.
    pub now: fn() -> Instant,
    /// This computer's offset east of UTC at a moment, in seconds.
    pub local_offset: fn(Instant) -> i32,
    /// This computer's offset when its wall clock shows a time, in seconds since 1970.
    pub local_offset_of_wall: fn(i64) -> i32,
}

impl Clock {
    /// The system's.
    pub const SYSTEM: Self = Self {
        now: Instant::now,
        local_offset: heimdall_app::time_zone::local_offset_at,
        local_offset_of_wall: heimdall_app::time_zone::local_offset_of_wall_clock,
    };
}

/// What the converter shows under its input.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Shown {
    /// Nothing typed, or typing: the empty state.
    Empty,
    /// What was typed is not a moment.
    Invalid,
    /// A moment read, and how far it was from the moment it was read.
    Result(Parsed, RelativeDuration),
}

/// What an update of the converter leaves to do.
#[derive(Debug)]
pub enum Outcome {
    /// Nothing more.
    Done,
    /// Wait for typing to rest, then send `Rested` with this edit.
    Debounce(u64),
    /// Copy.
    Copy(CopySlot, String),
}

impl Outcome {
    /// The task it asks of tab `tab`.
    pub fn task(self, tab: TabId) -> Task<Message> {
        match self {
            Self::Debounce(edit) => Task::perform(super::wait(DEBOUNCE), move |()| {
                Message::Tool(tab, ToolMessage::DateTime(DateTimeMessage::Rested(edit)))
            }),
            Self::Done | Self::Copy(..) => Task::none(),
        }
    }
}

/// The converter's state, as the C# view model's.
#[derive(Debug)]
pub struct DateTimePane {
    input: String,
    shown: Shown,
    edits: u64,
    zones: Vec<TimeZoneEntry>,
    zone_list: combo_box::State<ZoneChoice>,
    zone: Option<ZoneChoice>,
    clock: Clock,
}

impl DateTimePane {
    /// A new tab's state, as the C# `Initialize`: this computer's zones, UTC chosen.
    pub fn new() -> Self {
        Self::with(heimdall_app::time_zone::system_zones(), Clock::SYSTEM)
    }

    /// Its state with `zones` and `clock`, as the C# view model takes its zone provider.
    pub fn with(zones: Vec<TimeZoneEntry>, clock: Clock) -> Self {
        let choices: Vec<ZoneChoice> = zones
            .iter()
            .map(|zone| ZoneChoice {
                id: zone.id.clone(),
                name: zone.display_name.clone(),
            })
            .collect();
        // As the C# `LoadTimezones`: UTC when listed, else the first.
        let zone = choices
            .iter()
            .find(|choice| choice.id == time_zone_rules::UTC_ID)
            .or_else(|| choices.first())
            .cloned();
        Self {
            input: String::new(),
            shown: Shown::Empty,
            edits: 0,
            zones,
            zone_list: combo_box::State::new(choices),
            zone,
            clock,
        }
    }

    /// Applies `message`.
    pub fn update(&mut self, message: DateTimeMessage) -> Outcome {
        match message {
            DateTimeMessage::InputEdited(typed) => return self.edit(typed),
            DateTimeMessage::Now => {
                let now = (self.clock.now)().unix_seconds();
                return self.edit(now.to_string());
            }
            DateTimeMessage::Convert => {
                // As Enter: the timer stopped, converted at once.
                self.edits += 1;
                self.convert();
            }
            DateTimeMessage::Rested(edit) => {
                if edit == self.edits {
                    self.convert();
                }
            }
            DateTimeMessage::Zone(choice) => self.zone = Some(choice),
            DateTimeMessage::Copy(field) => {
                return Outcome::Copy(CopySlot::DateTime(field), self.field_text(field));
            }
        }
        Outcome::Done
    }

    /// The input changed, as the C# `OnInputTextChanged`: what was shown cleared, the empty
    /// state shown, a conversion waited for.
    fn edit(&mut self, typed: String) -> Outcome {
        self.input = typed;
        self.shown = Shown::Empty;
        self.edits += 1;
        Outcome::Debounce(self.edits)
    }

    /// The input converted, as the C# `ConvertCurrentInput`
    /// (`DateTimeConverterViewModel.cs:92-114`).
    fn convert(&mut self) {
        if self.input.trim().is_empty() {
            self.shown = Shown::Empty;
            return;
        }
        let local = self.clock.local_offset_of_wall;
        self.shown = match date_time::parse(&self.input, &local) {
            Some(parsed) => Shown::Result(
                parsed,
                date_time::relative(parsed.instant, (self.clock.now)()),
            ),
            None => Shown::Invalid,
        };
    }

    /// The input.
    #[cfg(test)]
    #[must_use]
    pub fn input(&self) -> &str {
        &self.input
    }

    /// Whether the empty state shows.
    #[must_use]
    pub fn shows_empty_state(&self) -> bool {
        self.shown == Shown::Empty
    }

    /// Whether a moment is shown.
    #[must_use]
    pub fn has_result(&self) -> bool {
        matches!(self.shown, Shown::Result(..))
    }

    /// The zones listed.
    #[cfg(test)]
    #[must_use]
    pub fn zones(&self) -> &[TimeZoneEntry] {
        &self.zones
    }

    /// The zone chosen.
    #[cfg(test)]
    #[must_use]
    pub fn zone(&self) -> Option<&ZoneChoice> {
        self.zone.as_ref()
    }

    /// What the line under the input says, as the C# `DetectedFormatText`.
    #[must_use]
    pub fn detected_text(&self) -> String {
        match self.shown {
            Shown::Empty => String::new(),
            Shown::Invalid => fl!("ui-tool-datetime-error-invalid"),
            Shown::Result(parsed, _) => match parsed.detected {
                DetectedFormat::UnixSeconds => fl!("ui-tool-datetime-detected-unix"),
                DetectedFormat::UnixMilliseconds => fl!("ui-tool-datetime-detected-ms"),
                DetectedFormat::Iso8601 => fl!("ui-tool-datetime-detected-iso"),
            },
        }
    }

    /// The text of `field`, as the C# `RebuildOutputs` (`DateTimeConverterViewModel.cs:181-197`);
    /// empty when nothing is shown.
    #[must_use]
    pub fn field_text(&self, field: DateField) -> String {
        let Shown::Result(parsed, _) = self.shown else {
            return String::new();
        };
        let instant = parsed.instant;
        match field {
            DateField::Unix => instant.unix_seconds().to_string(),
            DateField::IsoUtc => date_time::round_trip_utc(instant),
            DateField::IsoLocal => {
                date_time::round_trip_with_offset(instant, (self.clock.local_offset)(instant))
            }
            DateField::LocalTime => {
                long_date(&instant.wall_clock((self.clock.local_offset)(instant)))
            }
            DateField::Zone => self
                .zone
                .as_ref()
                .and_then(|choice| self.zones.iter().find(|zone| zone.id == choice.id))
                .map(|zone| long_date(&instant.wall_clock(zone.rules.offset_at(instant))))
                .unwrap_or_default(),
        }
    }

    /// How far the moment was from the moment it was read, as the C# `FormatRelative`.
    #[must_use]
    pub fn relative_text(&self) -> String {
        let Shown::Result(_, relative) = self.shown else {
            return String::new();
        };
        let count = relative.value;
        let duration = match relative.unit {
            RelativeUnit::Seconds => fl!("ui-tool-datetime-relative-seconds", count = count),
            RelativeUnit::Minutes => fl!("ui-tool-datetime-relative-minutes", count = count),
            RelativeUnit::Hours => fl!("ui-tool-datetime-relative-hours", count = count),
            RelativeUnit::Days => fl!("ui-tool-datetime-relative-days", count = count),
            RelativeUnit::Months => fl!("ui-tool-datetime-relative-months", count = count),
            RelativeUnit::Years => fl!("ui-tool-datetime-relative-years", count = count),
        };
        if relative.is_past {
            fl!("ui-tool-datetime-relative-ago", duration = duration)
        } else {
            fl!("ui-tool-datetime-relative-in", duration = duration)
        }
    }

    /// The tool's page, as the C# `DateTimeConverterView.xaml`: the input with Now, what was
    /// detected, then the empty state or each form with its copy button, the zone's list,
    /// and the relative time.
    pub fn view<'a>(&'a self, tab: TabId, state: &ToolPane) -> Element<'a, Message> {
        let send = move |message| Message::Tool(tab, ToolMessage::DateTime(message));
        let input = row![
            text_input(&fl!("ui-tool-datetime-placeholder"), &self.input)
                .font(super::BOX_FONT)
                .size(font_size::BODY_LARGE)
                .padding(super::INPUT_PADDING)
                .style(styles::text_input)
                .on_input(move |typed| send(DateTimeMessage::InputEdited(typed)))
                .on_submit(send(DateTimeMessage::Convert)),
            super::action_button(
                fl!("ui-tool-datetime-now"),
                false,
                Some(send(DateTimeMessage::Now))
            ),
        ]
        .spacing(spacing::SM)
        .align_y(iced::Alignment::Center);
        let mut page = column![
            super::field_label(fl!("ui-tool-datetime-input")),
            input,
            text(self.detected_text())
                .size(font_size::CAPTION)
                .style(text::secondary),
        ]
        .spacing(spacing::SM);
        if self.shows_empty_state() {
            page = page.push(super::hint(fl!("ui-tool-datetime-empty")));
        }
        if self.has_result() {
            let copy = |field: DateField| {
                super::copy_button(
                    fl!("ui-tool-datetime-copy"),
                    state.copied(CopySlot::DateTime(field)),
                    send(DateTimeMessage::Copy(field)),
                    super::PRIMARY_PADDING,
                )
            };
            for (label, field) in [
                (fl!("ui-tool-datetime-unix"), DateField::Unix),
                (fl!("ui-tool-datetime-iso-utc"), DateField::IsoUtc),
                (fl!("ui-tool-datetime-iso-local"), DateField::IsoLocal),
                (fl!("ui-tool-datetime-local-time"), DateField::LocalTime),
            ] {
                page = page.push(super::field_label(label)).push(
                    row![super::value_box(self.field_text(field)), copy(field)]
                        .spacing(spacing::SM)
                        .align_y(iced::Alignment::Center),
                );
            }
            let zones = combo_box(
                &self.zone_list,
                &fl!("ui-tool-datetime-timezone"),
                self.zone.as_ref(),
                move |choice| send(DateTimeMessage::Zone(choice)),
            )
            .width(ZONE_WIDTH)
            .size(font_size::BODY)
            .padding(super::INPUT_PADDING)
            .input_style(styles::text_input)
            .menu_style(styles::menu);
            page = page
                .push(super::field_label(fl!("ui-tool-datetime-timezone")))
                .push(
                    row![
                        zones,
                        super::value_box(self.field_text(DateField::Zone)),
                        copy(DateField::Zone),
                    ]
                    .spacing(spacing::SM)
                    .align_y(iced::Alignment::Center),
                )
                .push(super::field_label(fl!("ui-tool-datetime-relative")))
                .push(
                    text(self.relative_text())
                        .font(super::BOX_FONT)
                        .size(font_size::BODY_LARGE)
                        .style(|theme: &iced::Theme| text::Style {
                            color: Some(theme.palette().primary),
                        }),
                );
        }
        super::content_column(page)
    }
}

/// `clock` as this computer's long date, as .NET's "F" in the language shown: the weekday,
/// the day, the month and the year, then the time.
fn long_date(clock: &WallClock) -> String {
    let weekday = match clock.weekday {
        0 => fl!("ui-tool-datetime-weekday-0"),
        1 => fl!("ui-tool-datetime-weekday-1"),
        2 => fl!("ui-tool-datetime-weekday-2"),
        3 => fl!("ui-tool-datetime-weekday-3"),
        4 => fl!("ui-tool-datetime-weekday-4"),
        5 => fl!("ui-tool-datetime-weekday-5"),
        _ => fl!("ui-tool-datetime-weekday-6"),
    };
    let month = match clock.month {
        1 => fl!("ui-tool-datetime-month-1"),
        2 => fl!("ui-tool-datetime-month-2"),
        3 => fl!("ui-tool-datetime-month-3"),
        4 => fl!("ui-tool-datetime-month-4"),
        5 => fl!("ui-tool-datetime-month-5"),
        6 => fl!("ui-tool-datetime-month-6"),
        7 => fl!("ui-tool-datetime-month-7"),
        8 => fl!("ui-tool-datetime-month-8"),
        9 => fl!("ui-tool-datetime-month-9"),
        10 => fl!("ui-tool-datetime-month-10"),
        11 => fl!("ui-tool-datetime-month-11"),
        _ => fl!("ui-tool-datetime-month-12"),
    };
    fl!(
        "ui-tool-datetime-long",
        weekday = weekday,
        d = clock.day.to_string(),
        month = month,
        year = format!("{:04}", clock.year),
        time = clock.time_text()
    )
}

#[cfg(test)]
mod tests {
    use heimdall_core::tools::time_zone_rules::ZoneRules;

    use super::*;

    /// 2026-10-09T12:00:00Z.
    fn fixed_now() -> Instant {
        Instant::from_unix_seconds(1_791_547_200)
    }

    /// A computer two hours east of UTC.
    fn plus_two(_: Instant) -> i32 {
        7_200
    }

    fn plus_two_wall(_: i64) -> i32 {
        7_200
    }

    const CLOCK: Clock = Clock {
        now: fixed_now,
        local_offset: plus_two,
        local_offset_of_wall: plus_two_wall,
    };

    fn zones() -> Vec<TimeZoneEntry> {
        let zone = |id: &str, seconds: i32| TimeZoneEntry {
            id: id.to_owned(),
            display_name: time_zone_rules::display_name(id, seconds),
            rules: ZoneRules::fixed(seconds),
        };
        vec![zone("UTC", 0), zone("Custom/Plus2", 7_200)]
    }

    fn typed(pane: &mut DateTimePane, input: &str) {
        let _ = pane.update(DateTimeMessage::InputEdited(input.to_owned()));
        let _ = pane.update(DateTimeMessage::Convert);
    }

    #[test]
    fn a_new_tab_lists_the_zones_and_starts_on_utc() {
        let pane = DateTimePane::with(zones(), CLOCK);
        assert_eq!(pane.zones().len(), 2);
        assert_eq!(pane.zone().map(|zone| zone.id.as_str()), Some("UTC"));
        assert!(pane.shows_empty_state());
        let system = DateTimePane::new();
        if !system.zones().is_empty() {
            assert!(system.zone().is_some());
        }
    }

    #[test]
    fn a_unix_time_fills_every_form() {
        let mut pane = DateTimePane::with(zones(), CLOCK);
        typed(&mut pane, "1712345678");
        assert!(pane.has_result());
        assert!(!pane.shows_empty_state());
        assert_eq!(pane.field_text(DateField::Unix), "1712345678");
        assert_eq!(
            pane.field_text(DateField::IsoUtc),
            "2024-04-05T19:34:38.0000000Z"
        );
        assert_eq!(
            pane.field_text(DateField::IsoLocal),
            "2024-04-05T21:34:38.0000000+02:00"
        );
        assert_eq!(
            pane.field_text(DateField::LocalTime),
            "Friday, 5 April 2024 21:34:38"
        );
        assert!(pane.detected_text().starts_with("Detected:"));
        assert!(
            pane.relative_text().ends_with("ago"),
            "{}",
            pane.relative_text()
        );
    }

    #[test]
    fn a_zone_chosen_shows_the_moment_there_without_reading_it_again() {
        let mut pane = DateTimePane::with(zones(), CLOCK);
        typed(&mut pane, "2024-12-25T10:30:45Z");
        let _ = pane.update(DateTimeMessage::Zone(ZoneChoice {
            id: "Custom/Plus2".to_owned(),
            name: String::new(),
        }));
        assert_eq!(
            pane.field_text(DateField::Zone),
            "Wednesday, 25 December 2024 12:30:45"
        );
    }

    #[test]
    fn a_wrong_input_says_so_and_hides_the_forms() {
        let mut pane = DateTimePane::with(zones(), CLOCK);
        typed(&mut pane, "not-a-date");
        assert!(!pane.has_result());
        assert!(!pane.shows_empty_state());
        assert_eq!(pane.field_text(DateField::Unix), "");
        assert_eq!(
            pane.detected_text(),
            "Invalid input. Enter a Unix timestamp or ISO 8601 datetime."
        );
        typed(&mut pane, "   ");
        assert!(pane.shows_empty_state());
        assert_eq!(pane.detected_text(), "");
    }

    #[test]
    fn typing_clears_what_was_shown_and_converts_once_it_rests() {
        let mut pane = DateTimePane::with(zones(), CLOCK);
        typed(&mut pane, "1712345678");
        let Outcome::Debounce(first) =
            pane.update(DateTimeMessage::InputEdited("17123456".to_owned()))
        else {
            panic!("a conversion waited for");
        };
        assert!(pane.shows_empty_state());
        assert_eq!(pane.field_text(DateField::Unix), "");
        let Outcome::Debounce(second) =
            pane.update(DateTimeMessage::InputEdited("171234567".to_owned()))
        else {
            panic!("a conversion waited for");
        };
        let _ = pane.update(DateTimeMessage::Rested(first));
        assert!(pane.shows_empty_state(), "a stale rest does nothing");
        let _ = pane.update(DateTimeMessage::Rested(second));
        assert_eq!(pane.field_text(DateField::Unix), "171234567");
    }

    #[test]
    fn now_puts_this_moment_in() {
        let mut pane = DateTimePane::with(zones(), CLOCK);
        let _ = pane.update(DateTimeMessage::Now);
        assert_eq!(pane.input(), "1791547200");
    }

    #[test]
    fn a_copy_button_copies_its_field() {
        let mut pane = DateTimePane::with(zones(), CLOCK);
        typed(&mut pane, "0");
        let Outcome::Copy(slot, copied) = pane.update(DateTimeMessage::Copy(DateField::IsoUtc))
        else {
            panic!("a copy");
        };
        assert_eq!(slot, CopySlot::DateTime(DateField::IsoUtc));
        assert_eq!(copied, "1970-01-01T00:00:00.0000000Z");
    }
}
