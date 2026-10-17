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

//! A web console's form, as the C# cards: "Web address" on the General tab, plain HTTP, the
//! path and the address they form, then "Opening" on the Options tab, which says that the
//! system's browser opens it, never Heimdall, and, through a gateway, what the browser then
//! names.

use heimdall_app::Message as AppMessage;
use heimdall_app::profile_draft::{ProfileDraft, ProfileField, ProfileToggle};
use iced::Element;
use iced::widget::{checkbox, column, text};

use crate::dialog_parts;
use crate::i18n::fl;
use crate::shell::Message;
use crate::styles;
use crate::tokens::{font_size, spacing};

/// The "Web address" card of the General tab: plain HTTP, the path, then the address they
/// form with the host and port, or what is missing, as the C# preview; `field` draws a field
/// of the form as the other cards do.
pub fn address<'a>(
    draft: &'a ProfileDraft,
    field: impl Fn(ProfileField) -> Element<'a, Message>,
) -> Element<'a, Message> {
    let toggle = ProfileToggle::PlainHttp;
    let preview = match draft.web_address() {
        Some(address) => text(address)
            .size(font_size::CAPTION)
            .style(text::secondary),
        None => text(fl!("ui-profile-web-address-incomplete"))
            .size(font_size::CAPTION)
            .style(text::secondary),
    };
    let mut card = column![
        dialog_parts::section(
            fl!("ui-profile-web-address-title"),
            Some(fl!("ui-profile-web-address-desc"))
        ),
        checkbox(draft.is_on(toggle))
            .style(styles::checkbox)
            .label(fl!("ui-profile-web-use-http"))
            .on_toggle(move |on| Message::App(AppMessage::ProfileToggle { toggle, on })),
        field(ProfileField::WebPath),
        preview,
    ]
    .spacing(spacing::MD);
    if draft.is_on(toggle) {
        card = card.push(
            text(fl!("ui-profile-web-http-warning"))
                .size(font_size::CAPTION)
                .style(text::warning),
        );
    }
    card.into()
}

/// The "Opening" card of the Options tab: the system's browser opens the console, and the
/// browser, not Heimdall, asks for its password; through a gateway, the browser names the
/// tunnel of this computer, not the console.
pub fn opening<'a>(draft: &ProfileDraft) -> Element<'a, Message> {
    let mut card = column![
        dialog_parts::section(
            fl!("ui-profile-web-opening-title"),
            Some(fl!("ui-profile-web-opening-desc"))
        ),
        dialog_parts::hint(fl!("ui-profile-web-password-hint")),
    ]
    .spacing(spacing::MD);
    if draft.routed_gateway().is_some() {
        card = card.push(gateway_hint());
    }
    card.into()
}

/// What a console behind a gateway sees: the browser names the tunnel's loopback address
/// and port, not the console, as the C# SOCKS proxy never made it do.
pub fn gateway_hint<'a>() -> Element<'a, Message> {
    text(fl!("ui-profile-web-gateway-hint"))
        .size(font_size::CAPTION)
        .style(text::warning)
        .into()
}
