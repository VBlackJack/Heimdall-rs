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

//! The RDP options of a profile's form: the audio mode and colour depth lists, the C# groups
//! and the boxes only Remote Desktop Connection honours, greyed while the global defaults
//! decide them; and the application's RDP options on the settings page.
//!
//! A list draws its value and its entries without a text widget, so they are reached by
//! position: a list lies under its label, which can be found. Each entry is found by
//! opening the list again and clicking lower and lower down, recording the choice each
//! click makes.

mod common;

use heimdall_app::profile_draft::{
    DraftProtocol, ProfileChoice, ProfileDraft, ProfileField, ProfileToggle,
};
use heimdall_app::{Message as AppMessage, SettingsMessage};
use heimdall_core::profile::{
    AudioPlayback, ColorDepth, Experience, RdpDefaults, RdpOptions, RdpSwitch, Resolution,
};
use heimdall_ui::rdp_options::Monitor;
use heimdall_ui::shell::Message;
use heimdall_ui::terminal_view::FONTS;
use iced::{Point, Rectangle, Settings, Size, mouse};
use iced_test::selector::Candidate;
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
    common::simulator(
        settings,
        WINDOW,
        heimdall_ui::rdp_options::view(options, false),
    )
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
    choices_within(label, make, WINDOW.height)
}

/// The choices the list under `label` offers in what `make` draws, its menu tried down to
/// `bottom`.
fn choices_within<'a>(
    label: &str,
    make: &dyn Fn() -> common::Drawn<'a>,
    bottom: f32,
) -> Vec<String> {
    let bounds = make().find(label).expect(label).bounds();
    let list = Point::new(bounds.x + INTO_LIST, bounds.y + bounds.height + INTO_LIST);
    let mut found: Vec<String> = Vec::new();
    let mut y = list.y;
    while y < bottom {
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
        heimdall_ui::rdp_options::resolution(draft, &[], |field| {
            iced::widget::text_input("", draft.value(field)).into()
        }),
    )
}

/// A new RDP form with options of its own: out of the global defaults, every box in reach.
fn own_options() -> ProfileDraft {
    let mut draft = ProfileDraft::new_for(DraftProtocol::Rdp);
    draft.toggle(ProfileToggle::FollowDefaults, false);
    draft
}

/// The fields the resolution card of `draft` asks the form to draw.
fn fields_asked(draft: &ProfileDraft) -> Vec<ProfileField> {
    let asked = std::cell::RefCell::new(Vec::new());
    let _ = heimdall_ui::rdp_options::resolution(draft, &[], |field| {
        asked.borrow_mut().push(field);
        iced::widget::text("").into()
    });
    asked.into_inner()
}

#[test]
fn the_session_mode_offers_the_tab_then_remote_desktop_connection() {
    let draft = ProfileDraft::new_for(DraftProtocol::Rdp);
    let offered: Vec<String> = choices_in("Session mode", &|| resolution(&draft))
        .into_iter()
        .filter(|said| said.starts_with("ProfileChoice(External("))
        .collect();
    assert_eq!(
        offered,
        [false, true].map(|on| format!(
            "{:?}",
            AppMessage::ProfileChoice(ProfileChoice::External(on))
        ))
    );
    let mut external = draft.clone();
    external.choose(ProfileChoice::External(true));
    resolution(&external)
        .find("Launch RDP in a separate mstsc.exe window. Remote Desktop Connection asks for the password itself.")
        .expect("explained");
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
    // Only the list's own: a click below its menu may reach the boxes further down.
    let offered: Vec<String> = choices_in("Resolution mode", &|| resolution(&draft))
        .into_iter()
        .filter(|said| said.starts_with("ProfileChoice(Resolution("))
        .collect();
    assert_eq!(offered, expected);
}

#[test]
fn the_fixed_mode_offers_the_csharp_sizes_and_its_own_fields_only_there() {
    let mut draft = own_options();
    assert_eq!(
        fields_asked(&draft),
        [ProfileField::ResizeDelay],
        "fitting the window: the C# wait after connecting only"
    );
    let mut once = draft.clone();
    once.choose(ProfileChoice::DynamicResolution(false));
    assert_eq!(fields_asked(&once), [], "the tab's size once: no wait");
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
    // Only the list's own: a click below its menu may reach the boxes further down.
    let offered: Vec<String> = choices_in("Common resolutions", &|| resolution(&draft))
        .into_iter()
        .filter(|said| said.starts_with("ProfileChoice(Preset("))
        .collect();
    assert_eq!(offered, expected);
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

/// A window tall enough for every box of the RDP settings.
const SETTINGS_WINDOW: Size = Size::new(600.0, 1000.0);

/// The application's RDP options, as the settings page shows them.
fn rdp_settings(defaults: RdpDefaults) -> common::Drawn<'static> {
    let settings = Settings {
        fonts: FONTS.iter().map(|face| (*face).into()).collect(),
        ..Settings::default()
    };
    common::simulator(
        settings,
        SETTINGS_WINDOW,
        heimdall_ui::rdp_options::defaults(defaults),
    )
}

/// What ticking or clearing a box of the RDP settings changes.
type Change = fn(&mut RdpDefaults);

/// The RDP settings' boxes in the C# page's order, each with what ticking or clearing it
/// changes.
fn rdp_setting_boxes() -> [(&'static str, Change); 16] {
    [
        ("Allow dynamic resolution updates", |d| {
            d.dynamic_resolution = !d.dynamic_resolution;
        }),
        ("Multi-monitor", |d| {
            d.multi_monitor = !d.multi_monitor;
        }),
        ("Audio capture (microphone)", |d| {
            d.microphone = !d.microphone;
        }),
        ("Redirect clipboard", |d| {
            d.redirect_clipboard = !d.redirect_clipboard;
        }),
        ("Redirect drives", |d| {
            d.redirect_drives = !d.redirect_drives;
        }),
        ("Redirect printers", |d| {
            d.redirect_printers = !d.redirect_printers;
        }),
        ("Redirect COM ports", |d| {
            d.redirect_com_ports = !d.redirect_com_ports;
        }),
        ("Redirect smart cards", |d| {
            d.redirect_smart_cards = !d.redirect_smart_cards;
        }),
        ("Redirect webcam", |d| {
            d.redirect_webcam = !d.redirect_webcam;
        }),
        ("Redirect USB devices", |d| {
            d.redirect_usb = !d.redirect_usb;
        }),
        ("Keep bitmap cache on disk", |d| {
            d.bitmap_caching = !d.bitmap_caching;
        }),
        ("Compression", |d| {
            d.compression = !d.compression;
        }),
        ("Hardware-accelerated rendering", |d| {
            d.hardware_acceleration = !d.hardware_acceleration;
        }),
        ("Auto-reconnect", |d| {
            d.auto_reconnect = !d.auto_reconnect;
        }),
        ("Enable Network Level Authentication", |d| {
            d.nla = !d.nla;
        }),
        ("Strict server authentication", |d| {
            d.strict_server_authentication = !d.strict_server_authentication;
        }),
    ]
}

#[test]
fn the_rdp_settings_show_every_csharp_default_in_its_order_each_sending_its_change() {
    let mut above = f32::MIN;
    for (label, _) in rdp_setting_boxes() {
        let top = rdp_settings(RdpDefaults::default())
            .find(label)
            .expect(label)
            .bounds()
            .y;
        assert!(
            top > above,
            "{label} below the one before, as in the C# page"
        );
        above = top;
    }
    for (label, change) in rdp_setting_boxes() {
        let mut ui = rdp_settings(RdpDefaults::default());
        ui.click(label).expect(label);
        let mut expected = RdpDefaults::default();
        change(&mut expected);
        let sent = format!(
            "{:?}",
            Message::App(AppMessage::Settings(SettingsMessage::RdpDefaults(expected)))
        );
        assert!(
            ui.into_messages()
                .any(|message| format!("{message:?}") == sent),
            "{label}"
        );
    }
}

#[test]
fn strict_server_authentication_is_offered_only_with_network_level_authentication() {
    let without_nla = RdpDefaults {
        nla: false,
        ..RdpDefaults::default()
    };
    let mut ui = rdp_settings(without_nla);
    ui.click("Strict server authentication")
        .expect("shown, greyed out");
    assert!(
        !ui.into_messages().any(|message| matches!(
            message,
            Message::App(AppMessage::Settings(SettingsMessage::RdpDefaults(_)))
        )),
        "as the C# page: not without NLA"
    );
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

/// A window tall enough for every group of the RDP options.
const GROUPS_WINDOW: Size = Size::new(700.0, 2000.0);
/// How far below a list's label its menu's first entries are tried.
const MENU_REACH: f32 = 120.0;
/// How far apart, at most, the middles of a box and of the note beside it are.
const SAME_ROW: f32 = 4.0;

/// Two screens side by side, the second upright.
const SCREENS: [Monitor; 2] = [
    Monitor {
        index: 0,
        width: 1920,
        height: 1080,
        primary: true,
    },
    Monitor {
        index: 1,
        width: 1080,
        height: 1920,
        primary: false,
    },
];

/// The boxes only Remote Desktop Connection honours, by their C# label.
const EXTERNAL_ONLY: [(&str, RdpSwitch); 10] = [
    ("Capture local microphone", RdpSwitch::Microphone),
    ("Redirect printers", RdpSwitch::Printers),
    ("Redirect COM ports", RdpSwitch::ComPorts),
    ("Redirect smart cards", RdpSwitch::SmartCards),
    ("Redirect webcam", RdpSwitch::Webcam),
    ("Redirect USB devices", RdpSwitch::Usb),
    (
        "Keep bitmap cache on disk between sessions",
        RdpSwitch::BitmapCaching,
    ),
    ("Enable RDP compression", RdpSwitch::Compression),
    ("Avoid UDP transport probing", RdpSwitch::DisableUdp),
    ("Open in fullscreen", RdpSwitch::FullScreen),
];

/// What the form says beside an option only Remote Desktop Connection honours.
const EXTERNAL_ONLY_NOTE: &str = "External client (mstsc.exe) only";

/// The box neither client honours yet, and what is said under it whatever the mode.
const HARDWARE_ACCELERATION: &str = "Use hardware-accelerated rendering";
const NOT_SUPPORTED_NOTE: &str = "Not supported yet: the built-in client has no such switch, \
    and Remote Desktop Connection (mstsc.exe) reads no setting for it from its .rdp file.";

/// The boxes the global defaults decide while the profile follows them.
const DECIDED_BY_DEFAULTS: [&str; 18] = [
    "Allow dynamic resolution updates",
    "Enable multi-monitor mode",
    "Capture local microphone",
    "Redirect clipboard",
    "Redirect drives",
    "Redirect printers",
    "Redirect COM ports",
    "Redirect smart cards",
    "Redirect webcam",
    "Redirect USB devices",
    "Keep bitmap cache on disk between sessions",
    "Enable RDP compression",
    "Use hardware-accelerated rendering",
    "Automatically reconnect",
    "Enable Network Level Authentication",
    "Require server identity validation",
    // Twice for the lists, by their labels: tried below.
    "Audio mode",
    "Color depth",
];

/// The boxes the global defaults do not have, the profile's own whatever it follows.
const PROFILE_OWN: [&str; 5] = [
    "Enable anti-idle keepalive",
    "Disable wallpaper",
    "Avoid UDP transport probing",
    "Run as administrator session (/admin)",
    "Open in fullscreen",
];

/// The RDP groups of `draft` as the form shows them, `monitors` the computer's screens.
fn groups<'a>(draft: &'a ProfileDraft, monitors: &[Monitor]) -> common::Drawn<'a> {
    let settings = Settings {
        fonts: FONTS.iter().map(|face| (*face).into()).collect(),
        ..Settings::default()
    };
    common::simulator(
        settings,
        GROUPS_WINDOW,
        iced::widget::column![
            heimdall_ui::rdp_options::display_audio(draft, monitors, |field| {
                iced::widget::text_input("", draft.value(field)).into()
            }),
            heimdall_ui::rdp_options::devices(draft),
            heimdall_ui::rdp_options::performance(draft),
            heimdall_ui::rdp_options::behavior(draft),
        ],
    )
}

/// Where each text reading `wanted` is drawn, in the order of the window's tree.
fn every(ui: &mut Simulator<'_, Message>, wanted: &str) -> Vec<Rectangle> {
    let mut found = Vec::new();
    let _ = ui.find(|candidate: Candidate<'_>| {
        if let Candidate::Text {
            content, bounds, ..
        } = candidate
            && content == wanted
        {
            found.push(bounds);
        }
        None::<()>
    });
    found
}

/// The messages a click on `label` in the groups of `draft` sends.
fn sent_by(draft: &ProfileDraft, label: &str) -> Vec<String> {
    let mut ui = groups(draft, &SCREENS);
    ui.click(label).expect(label);
    ui.into_messages()
        .map(|message| format!("{message:?}"))
        .collect()
}

#[test]
fn each_option_of_the_external_client_is_a_box_said_to_be_its_own_while_the_profile_opens_in_a_tab()
{
    let draft = own_options();
    {
        let mut ui = groups(&draft, &SCREENS);
        let notes = every(&mut ui, EXTERNAL_ONLY_NOTE);
        assert_eq!(
            notes.len(),
            EXTERNAL_ONLY.len() + 1,
            "one beside each box, and the multi-monitor one"
        );
        for label in EXTERNAL_ONLY
            .iter()
            .map(|(label, _)| *label)
            .chain(["Enable multi-monitor mode"])
        {
            let row = ui.find(label).expect(label).bounds();
            assert!(
                notes
                    .iter()
                    .any(|note| (note.center_y() - row.center_y()).abs() < SAME_ROW
                        && note.x > row.x),
                "{label}: the note beside it"
            );
        }
    }
    for (label, switch) in EXTERNAL_ONLY
        .into_iter()
        .chain([(HARDWARE_ACCELERATION, RdpSwitch::HardwareAcceleration)])
    {
        let on = !switch.is_on(&draft.rdp_extras);
        let expected = format!(
            "{:?}",
            Message::App(AppMessage::ProfileChoice(ProfileChoice::Extra(switch, on)))
        );
        assert!(sent_by(&draft, label).contains(&expected), "{label}");
    }
    // Opened in Remote Desktop Connection, or through an RD Gateway, which opens it there:
    // every option applies, nothing is said.
    let mut external = draft.clone();
    external.choose(ProfileChoice::External(true));
    let mut through_gateway = draft.clone();
    through_gateway.set(ProfileField::RdGateway, "rdg.lab".to_owned());
    for opened_outside in [&external, &through_gateway] {
        let mut ui = groups(opened_outside, &SCREENS);
        assert!(every(&mut ui, EXTERNAL_ONLY_NOTE).is_empty());
    }
    // Hardware acceleration, which Remote Desktop Connection reads no key for either: said
    // not supported yet under its box, in a tab or outside, never said to be its own.
    for opened in [&draft, &external, &through_gateway] {
        let mut ui = groups(opened, &SCREENS);
        let tick = ui
            .find(HARDWARE_ACCELERATION)
            .expect("the box, kept")
            .bounds();
        let notes = every(&mut ui, NOT_SUPPORTED_NOTE);
        assert_eq!(notes.len(), 1);
        assert!(notes[0].y > tick.y, "under the box");
        assert!(
            every(&mut ui, EXTERNAL_ONLY_NOTE)
                .iter()
                .all(|note| (note.center_y() - tick.center_y()).abs() >= SAME_ROW),
            "not beside it"
        );
    }
}

#[test]
fn following_the_defaults_greys_what_they_decide_until_left_and_leaves_the_rest_in_reach() {
    let following = ProfileDraft::new_for(DraftProtocol::Rdp);
    assert!(following.is_on(ProfileToggle::FollowDefaults), "as the C#");
    let own = own_options();
    let (boxes, lists) = DECIDED_BY_DEFAULTS.split_at(DECIDED_BY_DEFAULTS.len() - 2);
    for label in boxes {
        assert!(sent_by(&following, label).is_empty(), "{label}: greyed");
        assert!(!sent_by(&own, label).is_empty(), "{label}: in reach, left");
    }
    for (label, prefix) in lists
        .iter()
        .zip(["ProfileChoice(Audio(", "ProfileChoice(ColorDepth("])
    {
        let offered = |draft: &ProfileDraft| -> Vec<String> {
            let top = groups(draft, &SCREENS)
                .find(*label)
                .expect(label)
                .bounds()
                .y;
            choices_within(label, &|| groups(draft, &SCREENS), top + MENU_REACH)
                .into_iter()
                .filter(|said| said.starts_with(prefix))
                .collect()
        };
        assert!(offered(&following).is_empty(), "{label}: veiled");
        assert!(!offered(&own).is_empty(), "{label}: in reach, left");
    }
    for label in PROFILE_OWN {
        assert!(!sent_by(&following, label).is_empty(), "{label}: its own");
    }
}

#[test]
fn the_monitors_spanned_are_ticked_among_the_screens_and_those_not_connected_are_kept() {
    let mut draft = own_options();
    {
        let mut ui = groups(&draft, &SCREENS[..1]);
        ui.click("Enable multi-monitor mode")
            .expect("shown, greyed out");
        assert_eq!(
            ui.into_messages().count(),
            0,
            "one screen: nothing to span, as the C#"
        );
    }
    let expected = format!(
        "{:?}",
        Message::App(AppMessage::ProfileChoice(ProfileChoice::MultiMonitor(true)))
    );
    assert!(sent_by(&draft, "Enable multi-monitor mode").contains(&expected));
    draft.choose(ProfileChoice::MultiMonitor(true));
    // Chosen at a desk with more screens than here.
    draft.choose(ProfileChoice::Monitor(5, true));
    let mut ui = groups(&draft, &SCREENS);
    for label in [
        "Multi-monitor uses the selected local displays. Changes require reconnection.",
        "Monitors saved for this session that are not connected right now are kept.",
        "Selected monitors",
        "Choose which monitors the remote session uses. Leaving none checked = all monitors.",
        "Monitor 1: 1920x1080 (primary)",
    ] {
        ui.find(label).expect(label);
    }
    ui.click("Monitor 2: 1080x1920 (vertical)")
        .expect("the upright screen");
    let expected = format!(
        "{:?}",
        Message::App(AppMessage::ProfileChoice(ProfileChoice::Monitor(1, true)))
    );
    assert!(
        ui.into_messages()
            .any(|message| format!("{message:?}") == expected)
    );
}

/// What the form says under the audio list, while it says Local playback, in a build
/// without the `audio` feature.
const LOCAL_PLAYBACK_NOTE: &str =
    "Local playback is not available in this build: the built-in client plays no sound.";

#[test]
fn local_playback_in_a_tab_is_said_unavailable_only_in_a_build_without_sound() {
    let expected = usize::from(!heimdall_rdp::audio::AVAILABLE);
    let mut local = own_options();
    local.choose(ProfileChoice::Audio(AudioPlayback::Local));
    {
        let mut ui = groups(&local, &SCREENS);
        let notes = every(&mut ui, LOCAL_PLAYBACK_NOTE);
        assert_eq!(notes.len(), expected, "in a tab");
        let list = ui.find("Audio mode").expect("the list").bounds();
        assert!(notes.iter().all(|note| note.y > list.y), "under the list");
    }
    // Remote Desktop Connection plays the sound itself; another mode asks nothing here.
    let mut external = local.clone();
    external.choose(ProfileChoice::External(true));
    let mut on_server = own_options();
    on_server.choose(ProfileChoice::Audio(AudioPlayback::OnServer));
    for silent in [&external, &on_server] {
        let mut ui = groups(silent, &SCREENS);
        assert!(every(&mut ui, LOCAL_PLAYBACK_NOTE).is_empty());
    }
    // The settings say it too: a profile following them opens in a tab.
    for (audio, notes) in [
        (AudioPlayback::Local, expected),
        (AudioPlayback::OnServer, 0),
        (AudioPlayback::Off, 0),
    ] {
        let mut ui = rdp_settings(RdpDefaults {
            audio,
            ..RdpDefaults::default()
        });
        assert_eq!(
            every(&mut ui, LOCAL_PLAYBACK_NOTE).len(),
            notes,
            "{audio:?}"
        );
    }
}
