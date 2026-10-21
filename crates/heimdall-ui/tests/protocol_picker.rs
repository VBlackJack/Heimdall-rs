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

//! The first step of a new session, drawn headless: the cards of a row share one height,
//! whether their description takes one line or two, as the C# `UniformGrid` gives its cells
//! one (`ServerDialog.xaml`, `ProtocolCards`).

mod common;

use heimdall_app::profile_draft::DraftProtocol;
use heimdall_ui::shell::{protocol_card_id, protocol_picker};
use heimdall_ui::terminal_view::FONTS;
use iced::{Rectangle, Settings, Size};

/// Cards to a row of the picker.
const CARDS_PER_ROW: usize = 4;

/// A window narrow enough that some descriptions wrap to two lines and others do not.
const NARROW_WINDOW: Size = Size::new(640.0, 700.0);

/// A window wide enough for most descriptions on one line.
const WIDE_WINDOW: Size = Size::new(1400.0, 900.0);

/// How far a laid-out edge may stray: a pixel's rounding.
const TOLERANCE: f32 = 1.0;

fn cards(window: Size) -> Vec<Rectangle> {
    let settings = Settings {
        fonts: FONTS.iter().map(|face| (*face).into()).collect(),
        ..Settings::default()
    };
    let mut ui = common::simulator(settings, window, protocol_picker());
    DraftProtocol::ALL
        .iter()
        .map(|protocol| {
            ui.find(protocol_card_id(*protocol))
                .unwrap_or_else(|_| panic!("the card of {protocol:?}"))
                .bounds()
        })
        .collect()
}

fn assert_rows_even(window: Size) {
    let cards = cards(window);
    for row in cards.chunks(CARDS_PER_ROW) {
        let first = row[0];
        for card in row {
            assert!(
                (card.height - first.height).abs() <= TOLERANCE,
                "a row's cards differ in height: {row:?}"
            );
            assert!(
                (card.y - first.y).abs() <= TOLERANCE,
                "a row's cards start at different heights: {row:?}"
            );
        }
        assert!(first.height > 0.0, "the cards have a height: {row:?}");
    }
}

#[test]
fn the_cards_of_a_row_share_one_height_in_a_narrow_window() {
    assert_rows_even(NARROW_WINDOW);
}

#[test]
fn the_cards_of_a_row_share_one_height_in_a_wide_window() {
    assert_rows_even(WIDE_WINDOW);
}
