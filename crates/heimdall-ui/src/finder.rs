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

//! The terminal's search bar, as the C# one: a field, previous, next and close, at the top
//! right of the terminal; the terminal itself finds. Each message names the tab searched,
//! and the field's identifier is made of it: a tab in its own window has a bar of its own,
//! which an operation meant for the main window's bar never reaches.

use heimdall_app::TabId;
use heimdall_term::{FindDirection, Found};
use iced::widget::{button, container, row, text, text_input};
use iced::{Element, Length};

use crate::i18n::fl;
use crate::shell::Message;

/// Width of the field.
const FIELD_WIDTH: f32 = 220.0;

/// Room around the bar and between its parts.
const SPACING: f32 = 4.0;

/// Size of "No match" and of the count.
const NOTE_SIZE: f32 = 12.0;

/// The search bar of one tab's terminal.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finder {
    /// The tab whose terminal is searched.
    pub tab: TabId,
    /// What is typed.
    pub query: String,
    /// What was last looked for.
    pub searched: Option<String>,
}

impl Finder {
    /// A bar for `tab`, empty.
    #[must_use]
    pub fn new(tab: TabId) -> Self {
        Self {
            tab,
            query: String::new(),
            searched: None,
        }
    }
}

/// Widget identifier of the field of the bar over `tab`'s terminal.
#[must_use]
pub fn field_id(tab: TabId) -> iced::widget::Id {
    iced::widget::Id::from(format!("terminal-find-{}", tab.value()))
}

/// The bar; `found` where what is typed, once looked for, was found, none when it was not.
pub fn view<'a>(finder: &Finder, found: Option<Found>, shift: bool) -> Element<'a, Message> {
    let tab = finder.tab;
    let direction = if shift {
        FindDirection::Up
    } else {
        FindDirection::Down
    };
    let mut bar = row![
        text_input(&fl!("ui-find-placeholder"), &finder.query)
            .id(field_id(tab))
            .width(FIELD_WIDTH)
            .on_input(move |query| Message::FinderQuery { tab, query })
            .on_submit(Message::FinderFind { tab, direction }),
    ]
    .spacing(SPACING)
    .align_y(iced::Alignment::Center);
    // Said only of what was looked for: typing more is a new search.
    if finder.searched.as_deref() == Some(finder.query.as_str()) {
        bar = bar.push(
            text(match found {
                Some(found) => fl!("ui-find-count", index = found.index, total = found.total),
                None => fl!("ui-find-nothing"),
            })
            .size(NOTE_SIZE),
        );
    }
    bar = bar
        .push(
            button(text(fl!("ui-find-previous")))
                .style(button::secondary)
                .on_press(Message::FinderFind {
                    tab,
                    direction: FindDirection::Up,
                }),
        )
        .push(
            button(text(fl!("ui-find-next")))
                .style(button::secondary)
                .on_press(Message::FinderFind {
                    tab,
                    direction: FindDirection::Down,
                }),
        )
        .push(
            button(text(fl!("ui-find-close")))
                .style(button::text)
                .on_press(Message::FinderClose(tab)),
        );
    container(
        container(bar)
            .padding(SPACING)
            .style(container::bordered_box),
    )
    .align_right(Length::Fill)
    .padding(SPACING)
    .into()
}
