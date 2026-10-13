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

//! Quick Connect's window part, as the C# palette draws it (`MainWindow.xaml:5086-5311`):
//! the mode label in split mode, a search field over what it finds in sections, each line
//! with its protocol's icon, its name with what is typed lit, its address and account, and
//! its protocol's badge, the one chosen lit; the hints under them. The core decides what is
//! found.

use heimdall_app::split::Axis;
use heimdall_app::tools::ToolId;
use heimdall_app::{ProfileKind, QuickGroup, QuickResult, QuickRow, TabId, ToolWords};
use iced::font::Weight;
use iced::widget::text::Span;
use iced::widget::{button, column, container, rich_text, row, span, text, text_input};
use iced::{Alignment, Color, Element, Font, Length, Theme};

use crate::i18n::fl;
use crate::icons::{self, Icon, Tint};
use crate::shell::Message;
use crate::styles;
use crate::tokens::{font_size, radius, spacing};

/// Widest the palette grows, as the C# one.
const PALETTE_WIDTH: f32 = 600.0;

/// Tallest its list grows before it scrolls.
const LIST_HEIGHT: f32 = 420.0;

/// Side of a line's protocol icon, as the C# template's 14 by 14 `Path`.
const ICON_SIDE: f32 = 14.0;

/// Room between the icon and the name, as the C# template's `Margin="0,0,6,0"`.
const ICON_GAP: f32 = 6.0;

/// How far the second line is set in, under the name, as the C# template's
/// `Margin="20,2,0,0"`.
const DETAIL_INDENT: f32 = ICON_SIDE + ICON_GAP;

/// How strongly a badge's colour fills behind it, as the C# template's `Opacity="0.15"`.
const BADGE_ALPHA: f32 = 0.15;

/// Room around a badge's text, as the C# template's `Margin="6,2"`.
const BADGE_PADDING: [f32; 2] = [2.0, 6.0];

/// The window's font, bold: what is typed, lit in a name, as the C# `HighlightTextBehavior`.
const MATCH_FONT: Font = Font {
    weight: Weight::Bold,
    ..crate::UI_FONT
};

/// The search typed so far, and which result is chosen.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Palette {
    /// What is typed.
    pub query: String,
    /// The result chosen, by its place in the list.
    pub chosen: usize,
    /// Opened from a tab's "Split...": the tab what is chosen is merged into, and how the
    /// two are placed, as the C# palette's split mode.
    pub split: Option<(TabId, Axis)>,
}

/// Widget identifier of the palette's field.
#[must_use]
pub fn field_id() -> iced::widget::Id {
    iced::widget::Id::new("quick-connect")
}

/// The names a tool is found by in the language shown, for the core's search.
#[must_use]
pub fn tool_words(tool: ToolId) -> ToolWords {
    ToolWords {
        name: crate::tools::label(tool),
        category: crate::tools::category_label(tool.category()),
    }
}

/// The C# palette's badge of `kind`, as its `ConnectionTypeBadge`.
const fn kind_badge(kind: ProfileKind) -> &'static str {
    match kind {
        ProfileKind::Rdp => "RDP",
        ProfileKind::Ssh => "SSH",
        ProfileKind::WinRm => "WINRM",
        ProfileKind::Sftp => "SFTP",
        ProfileKind::Ftp => "FTP",
        ProfileKind::Vnc => "VNC",
        ProfileKind::Telnet => "TEL",
        ProfileKind::Citrix => "CTX",
        ProfileKind::Local => "SH",
    }
}

/// The header of section `group`.
#[must_use]
pub fn group_label(group: &QuickGroup) -> String {
    match group {
        QuickGroup::Folder(folder) => folder.clone(),
        QuickGroup::Servers => fl!("ui-palette-servers"),
        QuickGroup::QuickConnect => fl!("ui-palette-quick-connect"),
        QuickGroup::ActiveSessions => fl!("ui-palette-active-sessions"),
        QuickGroup::RecentTools => fl!("ui-palette-recent-tools"),
        QuickGroup::Category(category) => crate::tools::category_label(*category),
    }
}

/// What a line shows: its name, and its address and account on a second line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Line {
    /// The name.
    pub title: String,
    /// The address, or what the line does.
    pub detail: Option<String>,
    /// The account.
    pub account: Option<String>,
}

/// What `result` shows, as the C# palette item's `DisplayName`, `Endpoint` and `Username`.
#[must_use]
pub fn line(result: &QuickResult) -> Line {
    match result {
        QuickResult::Profile(profile) => Line {
            title: profile.name.clone(),
            detail: profile
                .endpoint
                .as_ref()
                .map(|(host, port)| format!("{host}:{port}")),
            account: profile
                .username
                .clone()
                .filter(|account| !account.trim().is_empty()),
        },
        QuickResult::Ssh {
            username,
            host,
            port,
        } => {
            let target = match username {
                Some(user) => format!("{user}@{host}:{port}"),
                None => host.clone(),
            };
            Line {
                title: fl!("ui-palette-ssh-to", target = target),
                detail: Some(host.clone()),
                account: None,
            }
        }
        QuickResult::Rdp { host } => Line {
            title: fl!("ui-palette-rdp-to", target = host.as_str()),
            detail: Some(host.clone()),
            account: None,
        },
        QuickResult::Session { title, .. } => Line {
            title: fl!("ui-palette-merge-session", title = title.as_str()),
            detail: Some(fl!("ui-palette-merge-detail")),
            account: None,
        },
        QuickResult::Tool { name, .. } => Line {
            title: name.clone(),
            detail: None,
            account: None,
        },
    }
}

/// Where `query` first is in `text`, whatever the case, as a range of bytes of `text`, as
/// the C# `HighlightTextBehavior.HighlightSplit` finds it; `None` when it is not there or
/// nothing is typed.
#[must_use]
pub fn highlight(text: &str, query: &str) -> Option<(usize, usize)> {
    if query.is_empty() {
        return None;
    }
    text.char_indices().find_map(|(start, _)| {
        let mut wanted = query.chars();
        let mut end = start;
        for found in text[start..].chars() {
            let Some(next) = wanted.clone().next() else {
                break;
            };
            if !found.to_lowercase().eq(next.to_lowercase()) {
                return None;
            }
            wanted.next();
            end += found.len_utf8();
        }
        wanted.next().is_none().then_some((start, end))
    })
}

/// The protocol of `result`; `None` for a tool.
fn kind_of(result: &QuickResult) -> Option<ProfileKind> {
    match result {
        QuickResult::Profile(profile) => Some(profile.kind),
        QuickResult::Ssh { .. } => Some(ProfileKind::Ssh),
        QuickResult::Rdp { .. } => Some(ProfileKind::Rdp),
        QuickResult::Session { kind, .. } => Some(*kind),
        QuickResult::Tool { .. } => None,
    }
}

/// The icon of `result` and its colour: its protocol's, or its tool's.
fn mark(result: &QuickResult) -> (Icon, Tint) {
    match (result, kind_of(result)) {
        (QuickResult::Tool { tool, .. }, _) => {
            (crate::tools::icon(*tool), Tint::Tool(tool.category()))
        }
        (_, kind) => {
            let kind = kind.unwrap_or(ProfileKind::Ssh);
            (Icon::of(kind), Tint::Protocol(kind))
        }
    }
}

/// The badge of `result`, and its colour in `theme`, as the C# `ConnectionTypeBadge`.
fn badge(result: &QuickResult, theme: &Theme) -> (String, Color) {
    match (result, kind_of(result)) {
        (QuickResult::Tool { tool, .. }, _) => (
            fl!("ui-palette-tool-badge"),
            icons::tool_color(theme, tool.category()),
        ),
        (_, kind) => {
            let kind = kind.unwrap_or(ProfileKind::Ssh);
            (
                kind_badge(kind).to_owned(),
                icons::protocol_color(theme, kind),
            )
        }
    }
}

/// `title` with what `query` matches in it bold in `accent`.
fn lit_title<'a>(title: String, query: &str, accent: Color) -> Element<'a, Message> {
    let Some((start, end)) = highlight(&title, query) else {
        return text(title).size(font_size::BODY).into();
    };
    let spans: Vec<Span<'a, (), Font>> = vec![
        span(title[..start].to_owned()),
        span(title[start..end].to_owned())
            .font(MATCH_FONT)
            .color(accent),
        span(title[end..].to_owned()),
    ];
    rich_text(spans).size(font_size::BODY).into()
}

/// A line of the list: `row` at `index`, lit when `chosen`.
fn result_line<'a>(
    row: &QuickRow,
    index: usize,
    chosen: bool,
    query: &str,
    theme: &Theme,
) -> Element<'a, Message> {
    let shown = line(&row.result);
    let (icon, tint) = mark(&row.result);
    let first = row![
        icons::icon(icon, tint, ICON_SIDE),
        lit_title(shown.title, query, theme.palette().primary),
    ]
    .spacing(ICON_GAP)
    .align_y(Alignment::Center);
    let mut second = row![].spacing(spacing::XS);
    if let Some(detail) = shown.detail {
        second = second.push(text(detail).size(font_size::CAPTION));
    }
    if let Some(account) = shown.account {
        second = second.push(
            text(fl!("ui-palette-account", account = account.as_str())).size(font_size::CAPTION),
        );
    }
    let (badge_text, badge_color) = badge(&row.result, theme);
    let badge = container(
        text(badge_text)
            .size(font_size::SMALL_CAPTION)
            .font(styles::SEMIBOLD)
            .color(badge_color),
    )
    .padding(BADGE_PADDING)
    .style(move |_: &Theme| {
        styles::filled(
            Color {
                a: BADGE_ALPHA,
                ..badge_color
            },
            radius::XS,
        )
    });
    button(
        row![
            column![
                first,
                container(second).padding(iced::Padding {
                    left: DETAIL_INDENT,
                    ..iced::Padding::ZERO
                })
            ]
            .spacing(spacing::XS / 2.0)
            .width(Length::Fill),
            badge,
        ]
        .spacing(spacing::SM)
        .align_y(Alignment::Center),
    )
    .width(Length::Fill)
    .style(styles::palette_row(chosen))
    .on_press(Message::PaletteChoose(index))
    .into()
}

/// A section's header, as the C# palette's `GroupStyle.HeaderTemplate`.
fn header<'a>(group: &QuickGroup) -> Element<'a, Message> {
    container(
        text(group_label(group))
            .size(font_size::SMALL_CAPTION)
            .font(styles::SEMIBOLD)
            .style(text::secondary),
    )
    .padding(iced::Padding {
        top: spacing::SM,
        left: spacing::SM,
        ..iced::Padding::ZERO
    })
    .into()
}

/// The palette: the mode label in split mode, its field, `rows` in sections, the one
/// chosen lit, and the hints, as the C# palette; colours from `theme`.
pub fn view<'a>(palette: &Palette, rows: &[QuickRow], theme: &Theme) -> Element<'a, Message> {
    let placeholder = if palette.split.is_some() {
        fl!("ui-split-palette-hint")
    } else {
        fl!("ui-palette-placeholder")
    };
    let mut list = column![].spacing(spacing::XS / 2.0);
    let mut section: Option<&QuickGroup> = None;
    for (index, row) in rows.iter().enumerate() {
        if section != Some(&row.group) {
            list = list.push(header(&row.group));
            section = Some(&row.group);
        }
        list = list.push(result_line(
            row,
            index,
            index == palette.chosen,
            &palette.query,
            theme,
        ));
    }
    let mut content = column![].spacing(spacing::SM);
    if palette.split.is_some() {
        // As the C# `PaletteModeLabel`: in the accent, semi-bold, over the field.
        content = content.push(
            text(fl!("ui-palette-mode-split"))
                .size(font_size::CAPTION)
                .font(styles::SEMIBOLD)
                .style(text::primary),
        );
    }
    content = content.push(
        text_input(&placeholder, &palette.query)
            .style(styles::text_input)
            .id(field_id())
            .on_input(Message::PaletteQuery)
            .on_submit(Message::PaletteSubmit),
    );
    if rows.is_empty() {
        content = content.push(text(fl!("ui-palette-nothing")).size(font_size::CAPTION));
    } else {
        content = content.push(styles::scroll(list).height(Length::Shrink));
    }
    content = content.push(
        text(fl!("ui-palette-hints"))
            .size(font_size::CAPTION)
            .style(text::secondary),
    );
    container(
        container(row![content].height(Length::Shrink))
            .max_height(LIST_HEIGHT)
            .padding(spacing::SM)
            .width(Length::Fill)
            .style(container::bordered_box),
    )
    .max_width(PALETTE_WIDTH)
    .into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn what_is_typed_is_found_first_whatever_the_case() {
        assert_eq!(highlight("WebServer", "web"), Some((0, 3)));
        assert_eq!(highlight("my-web-web", "WEB"), Some((3, 6)), "the first");
        assert_eq!(highlight("webserver", ""), None, "nothing typed");
        assert_eq!(highlight("webserver", "xyz"), None);
        assert_eq!(highlight("w-e-b", "web"), None, "letters apart are not lit");
        assert_eq!(highlight("web", "webs"), None, "longer than the name");
        assert_eq!(highlight("Hôte ÉCOLE", "école"), Some((6, 12)), "accents");
        assert_eq!(highlight("[SSH] Connect to db", "db"), Some((17, 19)));
    }

    #[test]
    fn each_protocol_has_the_csharp_badge() {
        assert_eq!(kind_badge(ProfileKind::Telnet), "TEL");
        assert_eq!(kind_badge(ProfileKind::Citrix), "CTX");
        assert_eq!(kind_badge(ProfileKind::Local), "SH");
        assert_eq!(kind_badge(ProfileKind::WinRm), "WINRM");
    }

    #[test]
    fn a_line_shows_the_address_and_the_account() {
        let profile = heimdall_app::ProfileSummary {
            id: heimdall_core::profile::ProfileId::new("p"),
            name: "alpha".to_owned(),
            group: None,
            kind: ProfileKind::Ssh,
            endpoint: Some(("web.lab".to_owned(), 22)),
            username: Some("admin".to_owned()),
            gateway: None,
            favorite: false,
            metadata: heimdall_core::metadata::ProfileMetadata::default(),
        };
        let shown = line(&QuickResult::Profile(profile));
        assert_eq!(shown.title, "alpha");
        assert_eq!(shown.detail.as_deref(), Some("web.lab:22"));
        assert_eq!(shown.account.as_deref(), Some("admin"));
        let typed = line(&QuickResult::Rdp {
            host: "dc01".to_owned(),
        });
        assert_eq!(typed.detail.as_deref(), Some("dc01"), "its host, as the C#");
        assert!(typed.account.is_none());
    }
}
