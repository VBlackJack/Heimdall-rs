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

//! The Sessions page's empty states, laid out as the C# Launchpad cards
//! (`MainWindow.xaml:2120-2262`): a card centred in the page, a round badge over a title,
//! then its text, its buttons and, for the welcome, the shortcuts in a pill. The C# does not
//! animate them, so there is nothing for reduced motion to stop.

use iced::widget::{column, container, row, space, text};
use iced::{Alignment, Element, Length, Padding};

use crate::icons::{self, Icon, Tint};
use crate::styles;
use crate::tokens::font_size;

/// Width of the welcome card, as its C# `MaxWidth` (`MainWindow.xaml:2124`).
pub const WELCOME_WIDTH: f32 = 560.0;

/// Width of the "select a session" card, as its C# `MaxWidth` (`MainWindow.xaml:2181`).
pub const SELECT_WIDTH: f32 = 460.0;

/// Width of the welcome's subtitle, as its C# `MaxWidth` (`MainWindow.xaml:2152`).
const SUBTITLE_WIDTH: f32 = 440.0;

/// Space inside a card, vertically and horizontally, as its C# `Padding="32,28"`.
const CARD_PADDING: Padding = Padding {
    top: 28.0,
    right: 32.0,
    bottom: 28.0,
    left: 32.0,
};

/// Space inside the "select a session" button, as the C# `Padding="14,6"` (`MainWindow.xaml:2235`).
pub const SELECT_BUTTON_PADDING: [f32; 2] = [6.0, 14.0];

/// Space inside the shortcuts pill, as its C# `Padding="12,6"`.
const PILL_PADDING: [f32; 2] = [6.0, 12.0];

/// Side of the welcome's badge, as its C# `Width="64"`.
const WELCOME_BADGE: f32 = 64.0;

/// Side of the "select a session" badge, as its C# `Width="56"`.
const SELECT_BADGE: f32 = 56.0;

/// Side of the glyph in the welcome's badge, as the C# `IconSizeEmptyState`.
const WELCOME_GLYPH: f32 = 32.0;

/// Side of the glyph in the "select a session" badge, as the C# `IconSizeLarge`.
const SELECT_GLYPH: f32 = 20.0;

/// Space under the welcome's badge, as the C# `Margin="0,0,0,16"`.
const WELCOME_BADGE_GAP: f32 = 16.0;

/// Space under the "select a session" badge, as the C# `Margin="0,0,0,14"`.
const SELECT_BADGE_GAP: f32 = 14.0;

/// Space between the welcome's title and its subtitle, and under the subtitle, as the C#
/// `Margin="0,8,0,24"`.
const SUBTITLE_GAP_ABOVE: f32 = 8.0;
const SUBTITLE_GAP_BELOW: f32 = 24.0;

/// Space above the shortcuts pill, as the C# `Margin="0,20,0,0"`.
const PILL_GAP: f32 = 20.0;

/// Space between the card's buttons: the C#'s `Margin="4"` on each.
const BUTTON_GAP: f32 = 8.0;

/// Space under the "select a session" card's title, as the C# `Margin="0,0,0,16"`.
const SELECT_TITLE_GAP: f32 = 16.0;

/// A round badge of `side` holding `icon` in `glyph`, outlined in the accent if `accented`.
fn badge<'a, Message: 'a>(
    icon: Icon,
    accented: bool,
    side: f32,
    glyph: f32,
) -> Element<'a, Message> {
    container(icons::icon(icon, Tint::Accent, glyph))
        .width(side)
        .height(side)
        .center_x(side)
        .center_y(side)
        .style(styles::launchpad_badge(accented, side))
        .into()
}

/// A card of `width` around `content`, centred in the page.
fn card<'a, Message: 'a>(width: f32, content: Element<'a, Message>) -> Element<'a, Message> {
    container(
        container(content)
            .padding(CARD_PADDING)
            .max_width(width)
            .style(styles::large_card),
    )
    .center_x(Length::Fill)
    .center_y(Length::Fill)
    .into()
}

/// A gap of `height`, for the margins the C# sets part by part.
fn gap<'a, Message: 'a>(height: f32) -> Element<'a, Message> {
    space::vertical().height(height).into()
}

/// The welcome of a window with no session saved, as the C# Launchpad
/// (`MainWindow.xaml:2120-2178`): the badge, the title, the subtitle, the buttons in the
/// order they are given and the shortcuts in a pill.
#[must_use]
pub fn welcome<'a, Message: 'a>(
    title: String,
    subtitle: String,
    shortcuts: String,
    buttons: Vec<Element<'a, Message>>,
) -> Element<'a, Message> {
    let content = column![
        badge(Icon::Web, true, WELCOME_BADGE, WELCOME_GLYPH),
        gap(WELCOME_BADGE_GAP),
        text(title).size(font_size::HEADLINE).font(styles::SEMIBOLD),
        gap(SUBTITLE_GAP_ABOVE),
        container(
            text(subtitle)
                .size(font_size::BODY_LARGE)
                .style(text::secondary)
                .align_x(Alignment::Center)
                .width(Length::Fill),
        )
        .width(Length::Fill)
        .max_width(SUBTITLE_WIDTH),
        gap(SUBTITLE_GAP_BELOW),
        row(buttons).spacing(BUTTON_GAP).wrap(),
        gap(PILL_GAP),
        container(
            text(shortcuts)
                .size(font_size::SMALL_CAPTION)
                .style(text::secondary),
        )
        .padding(PILL_PADDING)
        .style(styles::launchpad_pill),
    ]
    .align_x(Alignment::Center);
    card(WELCOME_WIDTH, content.into())
}

/// The card of a window with sessions saved and none shown, as the C#'s second Launchpad
/// card (`MainWindow.xaml:2180-2253`): a badge, how to open a session and the button that
/// connects quickly.
#[must_use]
pub fn select<'a, Message: 'a>(
    title: String,
    button: Element<'a, Message>,
) -> Element<'a, Message> {
    let content = column![
        badge(Icon::Folder, false, SELECT_BADGE, SELECT_GLYPH),
        gap(SELECT_BADGE_GAP),
        text(title)
            .size(font_size::SUBTITLE)
            .font(styles::SEMIBOLD)
            .align_x(Alignment::Center),
        gap(SELECT_TITLE_GAP),
        button,
    ]
    .align_x(Alignment::Center);
    card(SELECT_WIDTH, content.into())
}
