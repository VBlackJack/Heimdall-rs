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

//! Quick Connect's window part, as the C# palette draws it: a search field over what it
//! finds, the one chosen lit; the core decides what is found.

use heimdall_app::split::Axis;
use heimdall_app::{QuickResult, TabId};
use iced::widget::{button, column, container, row, text, text_input};
use iced::{Element, Length};

use crate::i18n::fl;
use crate::shell::Message;
use crate::styles;
use crate::tokens::{font_size, spacing};

/// Widest the palette grows, as the C# one.
const PALETTE_WIDTH: f32 = 600.0;

/// Tallest its list grows before it scrolls.
const LIST_HEIGHT: f32 = 420.0;

/// The search typed so far, and which result is chosen.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Palette {
    /// What is typed.
    pub query: String,
    /// The result chosen, by its place in the list.
    pub chosen: usize,
    /// Opened from a tab's "Split...": the tab what is chosen is merged into, and how the
    /// two are placed, as the C# palette's split mode.
    pub split: Option<(TabId, Axis)>,
}

/// Widget identifier of the palette's field.
#[must_use]
pub fn field_id() -> iced::widget::Id {
    iced::widget::Id::new("quick-connect")
}

/// What a result says: its title, and a second line.
fn label(result: &QuickResult) -> (String, String) {
    match result {
        QuickResult::Profile(profile) => {
            let detail = profile
                .endpoint
                .as_ref()
                .map_or_else(String::new, |(host, port)| format!("{host}:{port}"));
            (
                format!("{}  {}", profile.kind.label(), profile.name),
                detail,
            )
        }
        QuickResult::Ssh {
            username,
            host,
            port,
        } => {
            let target = match username {
                Some(user) => format!("{user}@{host}:{port}"),
                None => host.clone(),
            };
            (
                fl!("ui-palette-ssh-to", target = target),
                fl!("ui-palette-quick-connect"),
            )
        }
        QuickResult::Rdp { host } => (
            fl!("ui-palette-rdp-to", target = host.as_str()),
            fl!("ui-palette-quick-connect"),
        ),
    }
}

/// The palette: its field, and `results`, the one chosen lit.
pub fn view<'a>(palette: &Palette, results: &[QuickResult]) -> Element<'a, Message> {
    let placeholder = if palette.split.is_some() {
        fl!("ui-split-palette-hint")
    } else {
        fl!("ui-palette-placeholder")
    };
    let list = column(results.iter().enumerate().map(|(index, result)| {
        let (title, detail) = label(result);
        button(column![text(title), text(detail).size(font_size::CAPTION)].spacing(2.0))
            .width(Length::Fill)
            .style(if index == palette.chosen {
                styles::primary
            } else {
                styles::subtle
            })
            .on_press(Message::PaletteChoose(index))
            .into()
    }))
    .spacing(2.0);
    let mut content = column![
        text_input(&placeholder, &palette.query)
            .style(styles::text_input)
            .id(field_id())
            .on_input(Message::PaletteQuery)
            .on_submit(Message::PaletteChoose(palette.chosen)),
    ]
    .spacing(spacing::SM);
    if results.is_empty() {
        content = content.push(text(fl!("ui-palette-nothing")).size(font_size::CAPTION));
    } else {
        content = content.push(styles::scroll(list).height(Length::Shrink));
    }
    container(
        container(row![content].height(Length::Shrink))
            .max_height(LIST_HEIGHT)
            .padding(spacing::SM)
            .width(Length::Fill)
            .style(container::bordered_box),
    )
    .max_width(PALETTE_WIDTH)
    .into()
}
