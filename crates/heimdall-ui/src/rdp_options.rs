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

//! The display and sound choices of an RDP profile's form, as the C# "Display & Audio"
//! card: the audio mode beside the colour depth, then how the desktop is sized.

use heimdall_app::profile_draft::{ProfileChoice, ProfileDraft, ProfileField};
use heimdall_app::{Message as AppMessage, SettingsMessage};
use heimdall_core::profile::{
    Aspect, AudioPlayback, ColorDepth, Experience, RdpDefaults, RdpOptions, Resolution,
};
use iced::widget::{checkbox, column, pick_list, row, text};
use iced::{Element, Length};

use crate::i18n::fl;
use crate::shell::Message;

/// Space between the two lists, and between a label and its list.
const SPACING: f32 = 8.0;
/// Size of a list's label.
const LABEL_SIZE: f32 = 12.0;

/// A colour depth as the list names it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct DepthChoice(ColorDepth);

impl std::fmt::Display for DepthChoice {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&match self.0 {
            ColorDepth::Bpp16 => fl!("ui-profile-color-16"),
            ColorDepth::Bpp24 => fl!("ui-profile-color-24"),
            ColorDepth::Bpp32 => fl!("ui-profile-color-32"),
        })
    }
}

/// An audio mode as the list names it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct AudioChoice(AudioPlayback);

impl std::fmt::Display for AudioChoice {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&match self.0 {
            AudioPlayback::Off => fl!("ui-profile-audio-off"),
            AudioPlayback::Local => fl!("ui-profile-audio-local"),
            AudioPlayback::OnServer => fl!("ui-profile-audio-on-server"),
        })
    }
}

/// The audio mode and colour depth lists of `options`, side by side.
#[must_use]
pub fn view<'a>(options: RdpOptions) -> Element<'a, Message> {
    lists(
        options.audio,
        options.color_depth,
        |audio| Message::App(AppMessage::ProfileChoice(ProfileChoice::Audio(audio))),
        |depth| Message::App(AppMessage::ProfileChoice(ProfileChoice::ColorDepth(depth))),
    )
}

/// The audio mode and colour depth lists, side by side, each choice sent as it says.
fn lists<'a>(
    audio: AudioPlayback,
    depth: ColorDepth,
    on_audio: impl Fn(AudioPlayback) -> Message + 'a,
    on_depth: impl Fn(ColorDepth) -> Message + 'a,
) -> Element<'a, Message> {
    let audio = column![
        text(fl!("ui-profile-audio")).size(LABEL_SIZE),
        pick_list(
            AudioPlayback::ALL.map(AudioChoice),
            Some(AudioChoice(audio)),
            move |choice: AudioChoice| on_audio(choice.0),
        )
        .width(Length::Fill),
    ]
    .spacing(SPACING / 2.0)
    .width(Length::Fill);
    let depth = column![
        text(fl!("ui-profile-color-depth")).size(LABEL_SIZE),
        pick_list(
            ColorDepth::ALL.map(DepthChoice),
            Some(DepthChoice(depth)),
            move |choice: DepthChoice| on_depth(choice.0),
        )
        .width(Length::Fill),
    ]
    .spacing(SPACING / 2.0)
    .width(Length::Fill);
    row![audio, depth].spacing(SPACING).into()
}

/// The C# "Visual experience" card of `options`: one box per performance flag, each change
/// taken at once.
#[must_use]
pub fn experience<'a>(options: RdpOptions) -> Element<'a, Message> {
    let mut card = column![text(fl!("ui-profile-experience")).size(LABEL_SIZE)].spacing(SPACING);
    for experience in Experience::ALL {
        card = card.push(
            checkbox(options.has(experience))
                .label(experience_label(experience))
                .on_toggle(move |on| choice(ProfileChoice::Experience(experience, on))),
        );
    }
    card.into()
}

/// The C# label of `experience`'s box.
fn experience_label(experience: Experience) -> String {
    match experience {
        Experience::DisableWallpaper => fl!("ui-profile-experience-no-wallpaper"),
        Experience::DisableThemes => fl!("ui-profile-experience-no-themes"),
        Experience::DisableAnimations => fl!("ui-profile-experience-no-animations"),
        Experience::DisableDrag => fl!("ui-profile-experience-no-drag"),
        Experience::DisableCursorShadow => fl!("ui-profile-experience-no-cursor-shadow"),
        Experience::EnableFontSmoothing => fl!("ui-profile-experience-font-smoothing"),
        Experience::EnableComposition => fl!("ui-profile-experience-composition"),
    }
}

/// A box of the RDP settings: its label, what it shows and what it changes.
struct Switch {
    /// Its label, in the C# words.
    label: String,
    /// Whether it is ticked.
    get: fn(&RdpDefaults) -> bool,
    /// Ticks or clears it.
    set: fn(&mut RdpDefaults, bool),
    /// Offered only while Network Level Authentication is on, as the C# page greys strict
    /// server authentication out without it.
    needs_nla: bool,
}

impl Switch {
    fn new(label: String, get: fn(&RdpDefaults) -> bool, set: fn(&mut RdpDefaults, bool)) -> Self {
        Self {
            label,
            get,
            set,
            needs_nla: false,
        }
    }
}

/// The boxes of the RDP settings, in the C# page's order: display, sound, redirections,
/// performance, then security.
fn switches() -> [Switch; 16] {
    [
        Switch::new(
            fl!("ui-profile-resolution-dynamic"),
            |d| d.dynamic_resolution,
            |d, on| d.dynamic_resolution = on,
        ),
        Switch::new(
            fl!("ui-settings-rdp-multi-monitor"),
            |d| d.multi_monitor,
            |d, on| d.multi_monitor = on,
        ),
        Switch::new(
            fl!("ui-settings-rdp-audio-capture"),
            |d| d.microphone,
            |d, on| d.microphone = on,
        ),
        Switch::new(
            fl!("ui-profile-toggle-clipboard"),
            |d| d.redirect_clipboard,
            |d, on| d.redirect_clipboard = on,
        ),
        Switch::new(
            fl!("ui-profile-toggle-drives"),
            |d| d.redirect_drives,
            |d, on| d.redirect_drives = on,
        ),
        Switch::new(
            fl!("ui-settings-rdp-redirect-printers"),
            |d| d.redirect_printers,
            |d, on| d.redirect_printers = on,
        ),
        Switch::new(
            fl!("ui-settings-rdp-redirect-com-ports"),
            |d| d.redirect_com_ports,
            |d, on| d.redirect_com_ports = on,
        ),
        Switch::new(
            fl!("ui-settings-rdp-redirect-smart-cards"),
            |d| d.redirect_smart_cards,
            |d, on| d.redirect_smart_cards = on,
        ),
        Switch::new(
            fl!("ui-settings-rdp-redirect-webcam"),
            |d| d.redirect_webcam,
            |d, on| d.redirect_webcam = on,
        ),
        Switch::new(
            fl!("ui-settings-rdp-redirect-usb"),
            |d| d.redirect_usb,
            |d, on| d.redirect_usb = on,
        ),
        Switch::new(
            fl!("ui-settings-rdp-bitmap-cache"),
            |d| d.bitmap_caching,
            |d, on| d.bitmap_caching = on,
        ),
        Switch::new(
            fl!("ui-settings-rdp-compression"),
            |d| d.compression,
            |d, on| d.compression = on,
        ),
        Switch::new(
            fl!("ui-settings-rdp-hardware-acceleration"),
            |d| d.hardware_acceleration,
            |d, on| d.hardware_acceleration = on,
        ),
        Switch::new(
            fl!("ui-settings-rdp-auto-reconnect"),
            |d| d.auto_reconnect,
            |d, on| d.auto_reconnect = on,
        ),
        Switch::new(fl!("ui-profile-toggle-nla"), |d| d.nla, |d, on| d.nla = on),
        Switch {
            needs_nla: true,
            ..Switch::new(
                fl!("ui-settings-rdp-strict-server-auth"),
                |d| d.strict_server_authentication,
                |d, on| d.strict_server_authentication = on,
            )
        },
    ]
}

/// The application's RDP options, as the C# RDP settings: what a profile following them
/// takes, in the C# page's order, each change applied at once.
#[must_use]
pub fn defaults<'a>(defaults: RdpDefaults) -> Element<'a, Message> {
    let send = |defaults: RdpDefaults| {
        Message::App(AppMessage::Settings(SettingsMessage::RdpDefaults(defaults)))
    };
    let mut page = column![
        text(fl!("ui-settings-rdp-defaults-hint")).size(LABEL_SIZE),
        lists(
            defaults.audio,
            defaults.color_depth,
            move |audio| send(RdpDefaults { audio, ..defaults }),
            move |color_depth| send(RdpDefaults {
                color_depth,
                ..defaults
            }),
        ),
    ]
    .spacing(SPACING);
    for switch in switches() {
        let mut tick = checkbox((switch.get)(&defaults)).label(switch.label);
        if !switch.needs_nla || defaults.nla {
            let set = switch.set;
            tick = tick.on_toggle(move |on| {
                let mut changed = defaults;
                set(&mut changed, on);
                send(changed)
            });
        }
        page = page.push(tick);
    }
    page.into()
}

/// A resolution mode as the list names it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ResolutionChoice(Resolution);

impl std::fmt::Display for ResolutionChoice {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&match self.0 {
            Resolution::FitWindow => fl!("ui-profile-resolution-fit-window"),
            Resolution::Fixed => fl!("ui-profile-resolution-fixed"),
            Resolution::SmartSizing => fl!("ui-profile-resolution-smart-sizing"),
            Resolution::MultiMonitor => fl!("ui-profile-resolution-multi-monitor"),
            Resolution::Auto => fl!("ui-profile-resolution-auto"),
        })
    }
}

/// A proportion as the list names it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct AspectChoice(Aspect);

impl std::fmt::Display for AspectChoice {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&match self.0.ratio() {
            None => fl!("ui-profile-aspect-stretch"),
            Some((wide, high)) => fl!("ui-profile-aspect-ratio-choice", wide = wide, high = high),
        })
    }
}

/// A session mode as the list names it: in a tab, or in Remote Desktop Connection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ModeChoice(bool);

impl std::fmt::Display for ModeChoice {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&if self.0 {
            fl!("ui-profile-rdp-mode-external")
        } else {
            fl!("ui-profile-rdp-mode-embedded")
        })
    }
}

/// The C# "Session mode" list of `draft`: in a tab, or in Remote Desktop Connection, which
/// is then explained.
fn session_mode<'a>(draft: &ProfileDraft) -> Element<'a, Message> {
    let external = draft.rdp_extras.external;
    let mut mode = column![
        text(fl!("ui-profile-rdp-session-mode")).size(LABEL_SIZE),
        pick_list(
            [ModeChoice(false), ModeChoice(true)],
            Some(ModeChoice(external)),
            |picked: ModeChoice| choice(ProfileChoice::External(picked.0)),
        )
        .width(Length::Fill),
    ]
    .spacing(SPACING / 2.0);
    if external {
        mode = mode.push(text(fl!("ui-profile-rdp-mode-external-desc")).size(LABEL_SIZE));
    }
    mode.into()
}

/// The common sizes the C# dialog offers for a fixed desktop.
const PRESETS: [(u16, u16); 5] = [
    (1280, 720),
    (1366, 768),
    (1920, 1080),
    (2560, 1440),
    (3840, 2160),
];

/// A common size as the list names it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct PresetChoice(u16, u16);

impl std::fmt::Display for PresetChoice {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&fl!(
            "ui-profile-resolution-preset",
            width = self.0,
            height = self.1
        ))
    }
}

fn choice(choice: ProfileChoice) -> Message {
    Message::App(AppMessage::ProfileChoice(choice))
}

/// The C# "Resolution profile" card of `draft`: the mode, and in the fixed mode its common
/// sizes, the width and height drawn by `field`, and whether it is scaled; then whether the
/// desktop follows the tab.
pub fn resolution<'a>(
    draft: &'a ProfileDraft,
    field: impl Fn(ProfileField) -> Element<'a, Message>,
) -> Element<'a, Message> {
    let options = draft.rdp_options;
    let mut card = column![
        text(fl!("ui-profile-resolution-title")),
        text(fl!("ui-profile-resolution-desc")).size(LABEL_SIZE),
        text(fl!("ui-profile-resolution-mode")).size(LABEL_SIZE),
        pick_list(
            Resolution::ALL.map(ResolutionChoice),
            Some(ResolutionChoice(options.resolution)),
            |picked: ResolutionChoice| choice(ProfileChoice::Resolution(picked.0)),
        )
        .width(Length::Fill),
    ]
    .spacing(SPACING / 2.0);
    if options.resolution == Resolution::Auto {
        card = card.push(text(fl!("ui-profile-resolution-auto-desc")).size(LABEL_SIZE));
    }
    card = card
        .push(session_mode(draft))
        .push(text(fl!("ui-profile-aspect-ratio")).size(LABEL_SIZE))
        .push(
            pick_list(
                Aspect::ALL.map(AspectChoice),
                Some(AspectChoice(options.aspect)),
                |picked: AspectChoice| choice(ProfileChoice::Aspect(picked.0)),
            )
            .width(Length::Fill),
        );
    // What an imported profile asks that the built-in client does not do: said, not lost.
    let unused = draft.rdp_extras.unused();
    if !unused.is_empty() {
        let names: Vec<String> = unused.into_iter().map(crate::texts::rdp_extra).collect();
        card = card.push(
            text(fl!(
                "ui-profile-rdp-extras",
                extras = names.join(&fl!("ui-dialog-import-dropped-separator"))
            ))
            .size(LABEL_SIZE)
            .style(text::secondary),
        );
    }
    if draft.shows(ProfileField::FixedWidth) {
        // The size typed, when it is one of the list's.
        let typed = (
            draft.fixed_width.trim().parse(),
            draft.fixed_height.trim().parse(),
        );
        let current = match typed {
            (Ok(width), Ok(height)) => PRESETS
                .contains(&(width, height))
                .then_some(PresetChoice(width, height)),
            _ => None,
        };
        card = card
            .push(text(fl!("ui-profile-resolution-presets")).size(LABEL_SIZE))
            .push(
                pick_list(
                    PRESETS.map(|(width, height)| PresetChoice(width, height)),
                    current,
                    |PresetChoice(width, height)| choice(ProfileChoice::Preset(width, height)),
                )
                .placeholder(fl!("ui-profile-resolution-custom"))
                .width(Length::Fill),
            )
            .push(
                row![
                    field(ProfileField::FixedWidth),
                    field(ProfileField::FixedHeight)
                ]
                .spacing(SPACING),
            )
            .push(
                checkbox(options.scale_fixed)
                    .label(fl!("ui-profile-resolution-scale-fixed"))
                    .on_toggle(|on| choice(ProfileChoice::ScaleFixed(on))),
            );
    }
    card.push(
        checkbox(options.dynamic_resolution)
            .label(fl!("ui-profile-resolution-dynamic"))
            .on_toggle(|on| choice(ProfileChoice::DynamicResolution(on))),
    )
    .into()
}
