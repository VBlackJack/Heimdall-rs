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

//! A Citrix profile's form, as the C# cards: "Citrix Workspace" on the General tab, the
//! `StoreFront` address and the application's name, then "Advanced Citrix options" on the
//! Options tab, the ICA file and what it takes; its boxes follow, drawn as every protocol's.

use heimdall_app::profile_draft::ProfileField;
use iced::Element;
use iced::widget::column;

use crate::dialog_parts;
use crate::i18n::fl;
use crate::shell::Message;
use crate::tokens::spacing;

/// The "Citrix Workspace" card; `field` draws a field of the form as the other cards do.
pub fn basics<'a>(field: impl Fn(ProfileField) -> Element<'a, Message>) -> Element<'a, Message> {
    column![
        dialog_parts::section(
            fl!("ui-profile-citrix-title"),
            Some(fl!("ui-profile-citrix-desc"))
        ),
        field(ProfileField::StoreFrontUrl),
        field(ProfileField::AppName),
    ]
    .spacing(spacing::MD)
    .into()
}

/// The "Advanced Citrix options" card: the ICA file, then the C# hint of what to fill.
pub fn advanced<'a>(field: impl Fn(ProfileField) -> Element<'a, Message>) -> Element<'a, Message> {
    column![
        dialog_parts::section(
            fl!("ui-profile-citrix-advanced-title"),
            Some(fl!("ui-profile-citrix-advanced-desc"))
        ),
        field(ProfileField::IcaFile),
        dialog_parts::hint(fl!("ui-profile-citrix-hint")),
    ]
    .spacing(spacing::MD)
    .into()
}
