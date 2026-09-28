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

//! The audio mode and colour depth lists of an RDP profile's form.
//!
//! A list draws its value and its entries without a text widget, so they are reached by
//! position: a list lies under its label, which can be found. Each entry is found by
//! opening the list again and clicking lower and lower down, recording the choice each
//! click makes.

use heimdall_app::Message as AppMessage;
use heimdall_app::profile_draft::ProfileChoice;
use heimdall_core::profile::{AudioPlayback, ColorDepth, RdpOptions};
use heimdall_ui::shell::Message;
use heimdall_ui::terminal_view::FONTS;
use iced::{Point, Settings, Size, mouse};
use iced_test::simulator::Simulator;

/// Size of the simulated form.
const WINDOW: Size = Size::new(600.0, 400.0);
/// From a label's bottom to a point inside its list.
const INTO_LIST: f32 = 14.0;
/// Step between the points tried below an open list.
const STEP: f32 = 4.0;

fn simulator(options: RdpOptions) -> Simulator<'static, Message> {
    let settings = Settings {
        fonts: FONTS.iter().map(|face| (*face).into()).collect(),
        ..Settings::default()
    };
    Simulator::with_size(settings, WINDOW, heimdall_ui::rdp_options::view(options))
}

/// Moves the pointer to `at`, then clicks: a list's menu picks the entry the pointer moved
/// over, not the one under the click.
fn click(ui: &mut Simulator<'_, Message>, at: Point) {
    ui.point_at(at);
    ui.simulate([
        iced::Event::Mouse(mouse::Event::CursorMoved { position: at }),
        iced::Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
        iced::Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)),
    ]);
}

/// A point inside the list under `label`.
fn list_under(label: &str) -> Point {
    let mut ui = simulator(RdpOptions::default());
    let bounds = ui.find(label).expect(label).bounds();
    Point::new(bounds.x + INTO_LIST, bounds.y + bounds.height + INTO_LIST)
}

/// The choices the list under `label` offers, top to bottom, as the messages they send.
fn choices(label: &str) -> Vec<String> {
    let list = list_under(label);
    let mut found: Vec<String> = Vec::new();
    let mut y = list.y;
    while y < WINDOW.height {
        let mut ui = simulator(RdpOptions::default());
        click(&mut ui, list);
        click(&mut ui, Point::new(list.x, y));
        for message in ui.into_messages() {
            let Message::App(app) = message else {
                continue;
            };
            let said = format!("{app:?}");
            if found.last() != Some(&said) {
                found.push(said);
            }
        }
        y += STEP;
    }
    found
}

#[test]
fn the_colour_depths_are_offered_in_the_csharp_order() {
    let expected: Vec<String> = ColorDepth::ALL
        .iter()
        .map(|depth| {
            format!(
                "{:?}",
                AppMessage::ProfileChoice(ProfileChoice::ColorDepth(*depth))
            )
        })
        .collect();
    assert_eq!(choices("Color depth"), expected);
}

#[test]
fn the_audio_modes_are_offered_in_the_csharp_order() {
    let expected: Vec<String> = AudioPlayback::ALL
        .iter()
        .map(|audio| {
            format!(
                "{:?}",
                AppMessage::ProfileChoice(ProfileChoice::Audio(*audio))
            )
        })
        .collect();
    assert_eq!(choices("Audio mode"), expected);
}

/// The RGBA pixels of the form drawn with `options`, and the width of a row in pixels.
fn pixels(options: RdpOptions) -> (Vec<u8>, usize) {
    let dir = tempfile::tempdir().expect("dir");
    simulator(options)
        .snapshot(&iced::Theme::Dark)
        .expect("drawn")
        .matches_image(dir.path().join("form.png"))
        .expect("written");
    let entry = std::fs::read_dir(dir.path())
        .expect("listed")
        .flatten()
        .next()
        .expect("one snapshot");
    let decoder = png::Decoder::new(std::io::BufReader::new(
        std::fs::File::open(entry.path()).expect("opened"),
    ));
    let mut reader = decoder.read_info().expect("header");
    let mut bytes = vec![0; reader.output_buffer_size().expect("size")];
    let info = reader.next_frame(&mut bytes).expect("frame");
    (bytes, usize::try_from(info.width).expect("width"))
}

/// Whether the two drawings differ on the left half, under the audio list's label, and on
/// the right half, under the colour depth's.
fn differ_by_side((one, width): &(Vec<u8>, usize), (other, _): &(Vec<u8>, usize)) -> (bool, bool) {
    let half = width / 2;
    let (mut left, mut right) = (false, false);
    for (index, (a, b)) in one.chunks(4).zip(other.chunks(4)).enumerate() {
        if a != b {
            if index % *width < half {
                left = true;
            } else {
                right = true;
            }
        }
    }
    (left, right)
}

#[test]
fn each_list_shows_the_value_chosen() {
    let defaults = pixels(RdpOptions::default());
    let depth = pixels(RdpOptions {
        color_depth: ColorDepth::Bpp16,
        ..RdpOptions::default()
    });
    let audio = pixels(RdpOptions {
        audio: AudioPlayback::OnServer,
        ..RdpOptions::default()
    });
    assert_eq!(
        differ_by_side(&defaults, &depth),
        (false, true),
        "the depth list, on the right"
    );
    assert_eq!(
        differ_by_side(&defaults, &audio),
        (true, false),
        "the audio list, on the left"
    );
}
