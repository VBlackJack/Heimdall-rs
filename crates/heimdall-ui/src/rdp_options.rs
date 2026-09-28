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
//! card: the audio mode beside the colour depth.

use heimdall_app::Message as AppMessage;
use heimdall_app::profile_draft::ProfileChoice;
use heimdall_core::profile::{AudioPlayback, ColorDepth, RdpOptions};
use iced::widget::{column, pick_list, row, text};
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
