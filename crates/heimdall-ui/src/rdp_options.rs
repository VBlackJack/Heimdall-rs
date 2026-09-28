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

use heimdall_app::Message as AppMessage;
use heimdall_app::profile_draft::{ProfileChoice, ProfileDraft, ProfileField};
use heimdall_core::profile::{AudioPlayback, ColorDepth, RdpOptions, Resolution};
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
pub fn view<'a>(options: RdpOptions) -> Element<'a, Message> {
    let audio = column![
        text(fl!("ui-profile-audio")).size(LABEL_SIZE),
        pick_list(
            AudioPlayback::ALL.map(AudioChoice),
            Some(AudioChoice(options.audio)),
            |choice: AudioChoice| Message::App(AppMessage::ProfileChoice(ProfileChoice::Audio(
                choice.0
            ))),
        )
        .width(Length::Fill),
    ]
    .spacing(SPACING / 2.0)
    .width(Length::Fill);
    let depth = column![
        text(fl!("ui-profile-color-depth")).size(LABEL_SIZE),
        pick_list(
            ColorDepth::ALL.map(DepthChoice),
            Some(DepthChoice(options.color_depth)),
            |choice: DepthChoice| Message::App(AppMessage::ProfileChoice(
                ProfileChoice::ColorDepth(choice.0,)
            )),
        )
        .width(Length::Fill),
    ]
    .spacing(SPACING / 2.0)
    .width(Length::Fill);
    row![audio, depth].spacing(SPACING).into()
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
        })
    }
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
