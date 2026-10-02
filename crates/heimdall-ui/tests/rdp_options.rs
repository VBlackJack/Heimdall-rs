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

mod common;

use heimdall_app::Message as AppMessage;
use heimdall_app::profile_draft::{DraftProtocol, ProfileChoice, ProfileDraft, ProfileField};
use heimdall_core::profile::{AudioPlayback, ColorDepth, Experience, RdpOptions, Resolution};
use heimdall_ui::shell::Message;
use heimdall_ui::terminal_view::FONTS;
use iced::{Point, Settings, Size, mouse};
use iced_test::simulator::Simulator;

/// Size of the simulated form.
const WINDOW: Size = Size::new(600.0, 520.0);
/// From a label's bottom to a point inside its list.
const INTO_LIST: f32 = 14.0;
/// Step between the points tried below an open list.
const STEP: f32 = 4.0;

fn simulator(options: RdpOptions) -> common::Drawn<'static> {
    let settings = Settings {
        fonts: FONTS.iter().map(|face| (*face).into()).collect(),
        ..Settings::default()
    };
    common::simulator(settings, WINDOW, heimdall_ui::rdp_options::view(options))
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

/// The choices the list under `label` offers, top to bottom, as the messages they send.
fn choices(label: &str) -> Vec<String> {
    choices_in(label, &|| simulator(RdpOptions::default()))
}

/// The choices the list under `label` offers in what `make` draws.
fn choices_in<'a>(label: &str, make: &dyn Fn() -> common::Drawn<'a>) -> Vec<String> {
    let bounds = make().find(label).expect(label).bounds();
    let list = Point::new(bounds.x + INTO_LIST, bounds.y + bounds.height + INTO_LIST);
    let mut found: Vec<String> = Vec::new();
    let mut y = list.y;
    while y < WINDOW.height {
        let mut ui = make();
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

/// The resolution card of `draft`, its size fields as plain boxes.
fn resolution(draft: &ProfileDraft) -> common::Drawn<'_> {
    let settings = Settings {
        fonts: FONTS.iter().map(|face| (*face).into()).collect(),
        ..Settings::default()
    };
    common::simulator(
        settings,
        WINDOW,
        heimdall_ui::rdp_options::resolution(draft, |field| {
            iced::widget::text_input("", draft.value(field)).into()
        }),
    )
}

/// The fields the resolution card of `draft` asks the form to draw.
fn fields_asked(draft: &ProfileDraft) -> Vec<ProfileField> {
    let asked = std::cell::RefCell::new(Vec::new());
    let _ = heimdall_ui::rdp_options::resolution(draft, |field| {
        asked.borrow_mut().push(field);
        iced::widget::text("").into()
    });
    asked.into_inner()
}

#[test]
fn the_resolution_modes_are_offered_in_the_csharp_order_without_multi_monitor() {
    let draft = ProfileDraft::new_for(DraftProtocol::Rdp);
    let expected: Vec<String> = Resolution::ALL
        .iter()
        .map(|mode| {
            format!(
                "{:?}",
                AppMessage::ProfileChoice(ProfileChoice::Resolution(*mode))
            )
        })
        .collect();
    assert_eq!(
        choices_in("Resolution mode", &|| resolution(&draft)),
        expected
    );
}

#[test]
fn the_fixed_mode_offers_the_csharp_sizes_and_its_own_fields_only_there() {
    let mut draft = ProfileDraft::new_for(DraftProtocol::Rdp);
    assert_eq!(fields_asked(&draft), [], "fitting the window");
    {
        let mut ui = resolution(&draft);
        assert!(ui.find("Common resolutions").is_err(), "fitting the window");
        assert!(
            ui.find("Scale fixed resolution to fit the pane").is_err(),
            "fitting the window"
        );
        ui.click("Allow dynamic resolution updates")
            .expect("always there");
        assert!(ui.into_messages().any(|message| matches!(
            message,
            Message::App(AppMessage::ProfileChoice(ProfileChoice::DynamicResolution(
                false
            )))
        )));
    }
    draft.choose(ProfileChoice::Resolution(Resolution::Fixed));
    let expected: Vec<String> = [
        (1280, 720),
        (1366, 768),
        (1920, 1080),
        (2560, 1440),
        (3840, 2160),
    ]
    .iter()
    .map(|(width, height)| {
        format!(
            "{:?}",
            AppMessage::ProfileChoice(ProfileChoice::Preset(*width, *height))
        )
    })
    .collect();
    assert_eq!(
        choices_in("Common resolutions", &|| resolution(&draft)),
        expected
    );
    assert_eq!(
        fields_asked(&draft),
        [ProfileField::FixedWidth, ProfileField::FixedHeight]
    );
    let mut ui = resolution(&draft);
    ui.click("Scale fixed resolution to fit the pane")
        .expect("the box");
    assert!(ui.into_messages().any(|message| matches!(
        message,
        Message::App(AppMessage::ProfileChoice(ProfileChoice::ScaleFixed(false)))
    )));
}

#[test]
fn the_visual_experience_boxes_are_the_csharp_ones_each_sending_its_change() {
    let mut options = RdpOptions::default();
    options.set(Experience::EnableFontSmoothing, true);
    let experience = || {
        let settings = Settings {
            fonts: FONTS.iter().map(|face| (*face).into()).collect(),
            ..Settings::default()
        };
        common::simulator(
            settings,
            WINDOW,
            heimdall_ui::rdp_options::experience(options),
        )
    };
    experience()
        .find("Visual experience")
        .expect("the card's title");
    for (label, experience_box, on) in [
        ("Disable wallpaper", Experience::DisableWallpaper, true),
        ("Disable themes", Experience::DisableThemes, true),
        (
            "Disable menu animations",
            Experience::DisableAnimations,
            true,
        ),
        ("Disable full-window drag", Experience::DisableDrag, true),
        (
            "Disable cursor shadow",
            Experience::DisableCursorShadow,
            true,
        ),
        (
            "Enable font smoothing (ClearType)",
            Experience::EnableFontSmoothing,
            false,
        ),
        (
            "Enable desktop composition",
            Experience::EnableComposition,
            true,
        ),
    ] {
        let mut ui = experience();
        ui.click(label).expect(label);
        let sent = format!(
            "{:?}",
            Message::App(AppMessage::ProfileChoice(ProfileChoice::Experience(
                experience_box,
                on,
            )))
        );
        assert!(
            ui.into_messages()
                .any(|message| format!("{message:?}") == sent),
            "{label}"
        );
    }
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
