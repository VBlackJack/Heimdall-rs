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

//! The profile tree as the C# Heimdall draws it: a row per profile with its protocol and
//! name, a click to select, a double click to connect, a right click for its menu at the
//! pointer.

use std::sync::{Arc, Mutex, PoisonError};

use heimdall_app::BulkField;
use heimdall_app::files::{Direction, Side};
use heimdall_app::reachability::{DownReason, Unchecked, Verdict};
use heimdall_app::split::{Axis, Placement, SplitMessage};
use heimdall_app::{
    ConnectAs, FilesMessage, FilterMessage, FloatMessage, FolderMessage, GatewayBadge,
    HostKeysMessage, Message as AppMessage, NO_FOLDER, ProfileCopy, ProfileKind,
    ProfileMenuMessage, ProfileSummary, RdpMessage, ResolutionChoice, SelectionMessage,
    SessionState, SessionsMessage, TabGroup, TabId, TabMenuMessage, TreeFilter,
};
use heimdall_core::folder::FolderColor;
use heimdall_core::profile::{ProfileId, Resolution, fixed_desktop};
use iced::advanced::layout::{self, Layout};
use iced::advanced::widget::{Operation, Tree, Widget};
use iced::advanced::{Clipboard, Shell, mouse, overlay, renderer};
use iced::widget::{
    Column, Container, button, checkbox, column, container, mouse_area, row, rule, text, tooltip,
};
use iced::{Element, Event, Length, Point, Rectangle, Size, Theme, Vector};

use crate::i18n::fl;
use crate::icons::{self, Icon, Tint};
use crate::shell::Message;
use crate::styles;
use crate::tokens::{BORDER_WIDTH, font_size, radius, spacing};
use crate::tree_row::{EDGE_WIDTH, Guided, Mark, RowChrome};

/// Tallest a "Move to folder" list grows before it scrolls.
const MOVE_MENU_HEIGHT: f32 = 360.0;

/// Width of a menu.
const MENU_WIDTH: f32 = 270.0;

/// A menu open in the tree.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TreeMenu {
    /// A profile's menu.
    Profile(ProfileId),
    /// The "Connect as..." entries of a profile.
    ConnectAs(ProfileId),
    /// How a profile opens in a split of the tab shown, as the C# "Open in split".
    OpenInSplit(ProfileId),
    /// The menu of the tree's empty area and of the "+" button.
    Add,
    /// The "..." button's menu.
    More,
    /// A tab's menu, drawn as the tree's are.
    Tab(TabId),
    /// The same menu opened from a pane's header in a split: its Disconnect closes that pane
    /// alone, never the whole tab.
    Pane(TabId),
    /// How a tab is split, as the C# "Split...": each way opens Quick Connect to choose
    /// what goes beside it.
    SplitAxis(TabId),
    /// The tabs a tab can be merged with, as the C# "Merge with...".
    MergeWith(TabId),
    /// How a tab is merged into another, as the C# entries under each tab of "Merge
    /// with...".
    MergeAxis {
        /// The tab split.
        host: TabId,
        /// The tab merged into it.
        tab: TabId,
    },
    /// A folder's menu, [`NO_FOLDER`] included.
    Folder(String),
    /// Where a folder can move to.
    MoveFolder(String),
    /// Which colour a folder takes.
    FolderColor(String),
    /// Which folder a profile can move to.
    MoveProfile(ProfileId),
    /// The tree's filters, as the C# filter button's menu.
    Filter,
    /// The menu of the profiles selected together.
    Selection,
    /// Which folder the profiles selected together can move to.
    MoveSelection,
    /// What can be set on the profiles selected together at once.
    EditSelection,
    /// The route the profiles selected together can be given: direct, or a gateway.
    GatewaySelection,
    /// The server's folders bookmarked in a Files tab.
    FilesBookmarks(TabId),
    /// Which of a Files tab's bookmarks to take off.
    FilesBookmarksRemove(TabId),
    /// The menu of a row of the tunnels panel, as the C# one.
    Tunnel(heimdall_app::tunnel::TunnelId),
    /// An RDP tab's "Resolution" menu, as the C# one.
    Resolution(TabId),
    /// A terminal tab's macros: record, play, stop.
    Macros(TabId),
    /// The notes a session's menu writes about it, as the C# Notes submenu.
    Notes(ProfileId),
    /// The menu of an entry of a Files tab's pane, as the C# Files tab's; `None` beside the
    /// entries, the menu of the folder shown.
    FilesEntry {
        /// The tab.
        tab: TabId,
        /// The pane.
        side: Side,
        /// The entry, or none.
        index: Option<usize>,
    },
}

/// Side of a session's protocol icon, as the C# `GeoIconSmall`.
const PROTOCOL_ICON_SIDE: f32 = 14.0;

/// Side of a folder's icon, as the C# `SessionTreeFolderIconSize`.
const FOLDER_ICON_SIDE: f32 = 13.0;

/// Side of the expander's arrow box, as the C# `TreeExpanderGlyphBoxSize`.
const EXPANDER_SIDE: f32 = 12.0;

/// Space between the expander and its row, as the C# expander's right margin.
const EXPANDER_GAP: f32 = 2.0;

/// Space after a folder's icon, as the C# margin.
const FOLDER_ICON_GAP: f32 = 7.0;

/// Height of a row's content: the C# `SessionTreeServerRowMinHeight` (24) and
/// `SessionTreeFolderRowMinHeight` (26) less their padding.
const ROW_CONTENT_HEIGHT: f32 = 20.0;

/// The space around a state dot the pointer reaches it in, as the C#
/// `SessionTreeStatusHitPadding`.
const DOT_HIT_PADDING: f32 = 3.0;

/// The window's font, semi-bold, as a C# folder's name.
const SEMIBOLD: iced::Font = iced::Font {
    weight: iced::font::Weight::Semibold,
    ..crate::UI_FONT
};

/// Widest a gateway badge grows beside a name, as the C# `SessionTreeGatewayBadgeMaxWidth`.
const BADGE_MAX_WIDTH: f32 = 140.0;

/// Corner radius of a badge, as the C# `SessionTreeBadgeCornerRadius`.
const BADGE_RADIUS: f32 = 3.0;

/// Size of a session's state dot, as the C# tree's and tabs'.
const DOT_SIZE: f32 = 7.0;

/// A session's state as the C# Heimdall's dot shows it: green connected, amber on its way,
/// red failed, grey when none is open.
#[must_use]
pub fn state_dot<'a>(state: Option<SessionState>) -> Element<'a, Message> {
    sized_state_dot(state, DOT_SIZE)
}

/// [`state_dot`] of diameter `side`, as the C# detail panel's larger one.
#[must_use]
pub fn sized_state_dot<'a>(state: Option<SessionState>, side: f32) -> Element<'a, Message> {
    container(iced::widget::space())
        .width(side)
        .height(side)
        .style(move |theme: &Theme| {
            let palette = theme.extended_palette();
            let colour = match state {
                Some(SessionState::Connected) => palette.success.base.color,
                Some(SessionState::Connecting | SessionState::Reconnecting) => {
                    palette.warning.base.color
                }
                Some(SessionState::Failed) => palette.danger.base.color,
                Some(SessionState::Ended) | None => palette.secondary.base.color,
            };
            container::Style {
                background: Some(colour.into()),
                border: iced::Border {
                    radius: (side / 2.0).into(),
                    ..iced::Border::default()
                },
                ..container::Style::default()
            }
        })
        .into()
}

/// Width of the ring that says what the background check found, in logical pixels, as the
/// C# `SessionTreeStatusRingThickness`.
const RING_WIDTH: f32 = 1.5;

/// A server's dot in the tree, as the C# one: its session's state while one is open or
/// failed; else what the background check found, as a ring, so that it is never taken for
/// a session; a grey ring when nothing was found, as the C# unknown verdict.
fn profile_dot<'a>(state: Option<SessionState>, reach: Option<&Verdict>) -> Element<'a, Message> {
    if state.is_some_and(|state| state != SessionState::Ended) {
        return state_dot(state);
    }
    let tone = match reach {
        Some(Verdict::Up(_)) => Tone::Success,
        Some(Verdict::Down(_)) => Tone::Danger,
        Some(Verdict::Checking) => Tone::Warning,
        Some(Verdict::Unchecked(_)) | None => Tone::Unknown,
    };
    container(iced::widget::space())
        .width(DOT_SIZE)
        .height(DOT_SIZE)
        .style(move |theme: &Theme| {
            let palette = theme.extended_palette();
            let colour = match tone {
                Tone::Success => palette.success.base.color,
                Tone::Danger => palette.danger.base.color,
                Tone::Warning => palette.warning.base.color,
                Tone::Unknown => palette.secondary.base.color,
            };
            container::Style {
                border: iced::Border {
                    color: colour,
                    width: RING_WIDTH,
                    radius: (DOT_SIZE / 2.0).into(),
                },
                ..container::Style::default()
            }
        })
        .into()
}

/// The colour of a ring.
#[derive(Debug, Clone, Copy)]
enum Tone {
    Success,
    Danger,
    Warning,
    Unknown,
}

/// What the background check found of a server, as the C# dot's tooltip says it.
fn reach_text(reach: &Verdict) -> String {
    match reach {
        Verdict::Checking => fl!("ui-tree-reachability-checking"),
        Verdict::Up(millis) => fl!("ui-tree-reachability-up", millis = (*millis)),
        Verdict::Down(reason) => fl!(
            "ui-tree-reachability-down",
            reason = match reason {
                DownReason::Timeout => fl!("ui-reachability-reason-timeout"),
                DownReason::Refused => fl!("ui-reachability-reason-refused"),
                DownReason::Unreachable => fl!("ui-reachability-reason-unreachable"),
                DownReason::Dns => fl!("ui-reachability-reason-dns"),
                DownReason::Other(detail) => detail.clone(),
            }
        ),
        Verdict::Unchecked(why) => fl!(
            "ui-tree-reachability-unchecked",
            reason = match why {
                Unchecked::BehindGateway => fl!("ui-reachability-reason-behind-gateway"),
                Unchecked::NoPort => fl!("ui-reachability-reason-no-port"),
                Unchecked::NoHost => fl!("ui-reachability-reason-no-host"),
            }
        ),
    }
}

/// How far a row moves right for each folder it is in, as the C# tree's expander column
/// and the margin of the items under it.
const INDENT: f32 = 16.0;

/// Space between two rows of the tree.
pub const ROW_GAP: f32 = 2.0;

/// Space between the expander's column and an open folder's indent guide, as the C# guide's
/// left margin.
const GUIDE_MARGIN: f32 = 1.0;

/// Where an open folder's indent guide is from its own row's left edge, as the C# draws it:
/// in the column of its items, past the expander and its margin.
const GUIDE_OFFSET: f32 = EXPANDER_SIDE + EXPANDER_GAP + GUIDE_MARGIN;

/// Where the guides beside a row `depth` folders deep are, from the tree's left edge: one
/// for each open folder it is in, the outermost first.
#[must_use]
pub fn guide_offsets(depth: usize) -> Vec<f32> {
    std::iter::successors(Some(GUIDE_OFFSET), |offset| Some(offset + INDENT))
        .take(depth)
        .collect()
}

/// `row` moved right for `depth` folders, the indent guide of each beside it.
#[must_use]
pub fn indented(row: Element<'_, Message>, depth: usize) -> Element<'_, Message> {
    #[allow(
        clippy::cast_precision_loss,
        reason = "a folder depth, far below the 2^24 where f32 loses units"
    )]
    let left = depth as f32 * INDENT;
    Guided::new(
        container(row).padding(iced::Padding {
            left,
            ..iced::Padding::ZERO
        }),
        guide_offsets(depth),
        ROW_GAP,
    )
    .into()
}

/// A folder: open or closed at a click, as the C# tree's; "(No Folder)" for [`NO_FOLDER`].
/// The expander's arrow before it, its icon in its colour, its own or inherited, else the
/// theme's, and the profiles it holds counted at its right, as the C# tree's; the edge of
/// the focus colour while the keyboard is on it.
pub fn folder_row<'a>(
    path: String,
    name: String,
    depth: usize,
    open: bool,
    count: usize,
    color: Option<FolderColor>,
    selected: bool,
) -> Element<'a, Message> {
    let label = if path == NO_FOLDER {
        fl!("ui-sidebar-group-none")
    } else {
        name
    };
    let (arrow, arrow_tint) = if open {
        (Icon::ChevronDown, Tint::Text)
    } else {
        (Icon::ChevronRight, Tint::Secondary)
    };
    let tint = color.map_or(Tint::Info, |color| {
        let (red, green, blue) = color.rgb();
        Tint::Own(iced::Color::from_rgb8(red, green, blue))
    });
    let chrome = RowChrome::new(
        container(
            row![
                container(icons::icon(Icon::Folder, tint, FOLDER_ICON_SIDE))
                    .center_y(ROW_CONTENT_HEIGHT),
                text(label).size(font_size::BODY).font(SEMIBOLD),
                iced::widget::space::horizontal(),
                text(count.to_string())
                    .size(font_size::SMALL_CAPTION)
                    .style(text::secondary),
            ]
            .spacing(FOLDER_ICON_GAP)
            .align_y(iced::Alignment::Center),
        )
        .width(Length::Fill)
        .padding(iced::Padding {
            top: 3.0,
            right: 8.0,
            bottom: 3.0,
            left: EDGE_WIDTH + 1.0,
        }),
        if selected { Mark::Cursor } else { Mark::None },
    );
    let body = container(
        row![icons::icon(arrow, arrow_tint, EXPANDER_SIDE), chrome]
            .spacing(EXPANDER_GAP)
            .align_y(iced::Alignment::Center),
    )
    .padding(iced::Padding {
        top: 3.0,
        bottom: 1.0,
        ..iced::Padding::ZERO
    });
    indented(
        mouse_area(body)
            .on_press(Message::App(AppMessage::ToggleFolder(path.clone())))
            .on_right_press(Message::OpenTreeMenu(TreeMenu::Folder(path)))
            .interaction(mouse::Interaction::Pointer)
            .into(),
        depth,
    )
}

/// Side of a folder colour's swatch, as the C# menu's.
const SWATCH_SIDE: f32 = 10.0;

/// A square of `color`, in a folder's colour menu.
fn swatch<'a>(color: FolderColor) -> Element<'a, Message> {
    let (red, green, blue) = color.rgb();
    container(iced::widget::space().width(SWATCH_SIDE).height(SWATCH_SIDE))
        .style(move |_: &iced::Theme| container::Style {
            background: Some(iced::Color::from_rgb8(red, green, blue).into()),
            border: iced::Border {
                radius: radius::XS.into(),
                ..iced::Border::default()
            },
            ..container::Style::default()
        })
        .into()
}

/// The name of `color`, in the window's language.
fn color_name(color: FolderColor) -> String {
    match color {
        FolderColor::Blue => fl!("ui-folder-color-blue"),
        FolderColor::Green => fl!("ui-folder-color-green"),
        FolderColor::Red => fl!("ui-folder-color-red"),
        FolderColor::Amber => fl!("ui-folder-color-amber"),
        FolderColor::Purple => fl!("ui-folder-color-purple"),
        FolderColor::Pink => fl!("ui-folder-color-pink"),
        FolderColor::Cyan => fl!("ui-folder-color-cyan"),
        FolderColor::Orange => fl!("ui-folder-color-orange"),
    }
}

/// The colours folder `path` can take, its own `current` ticked, then "No colour", ticked
/// when it has none of its own, as the C# "Colour" menu.
#[must_use]
pub fn folder_color_entries<'a>(path: &str, current: Option<FolderColor>) -> Element<'a, Message> {
    let choose = |color| {
        Message::MenuChoice(AppMessage::Folder(FolderMessage::Color {
            path: path.to_owned(),
            color,
        }))
    };
    let item = |label: String, checked: bool, color: Option<FolderColor>| -> Element<'a, Message> {
        button(
            row![
                text(if checked { CHECKED } else { "" })
                    .size(font_size::BODY_LARGE)
                    .width(CHECK_WIDTH),
                color.map(swatch),
                text(label).size(font_size::BODY_LARGE),
            ]
            .spacing(6.0)
            .align_y(iced::Alignment::Center),
        )
        .width(Length::Fill)
        .style(menu_style)
        .on_press(choose(color))
        .into()
    };
    let entries = column![]
        .spacing(0.0)
        .width(MENU_WIDTH)
        .extend(
            FolderColor::ALL
                .into_iter()
                .map(|color| item(color_name(color), current == Some(color), Some(color))),
        )
        .push(separator())
        .push(item(fl!("ui-folder-color-none"), current.is_none(), None));
    menu_card(entries).into()
}

/// Where a found profile is: its folder and host, as the C# tree says under the name while
/// searching; `None` when it has neither.
#[must_use]
pub fn search_context(profile: &ProfileSummary) -> Option<String> {
    let parts: Vec<&str> = [
        profile.group.as_deref(),
        profile.endpoint.as_ref().map(|(host, _)| host.as_str()),
    ]
    .into_iter()
    .flatten()
    .filter(|part| !part.trim().is_empty())
    .collect();
    (!parts.is_empty()).then(|| parts.join("  "))
}

/// One profile: its protocol's icon in its colour, the state of its sessions and name, as
/// the C# tree's row, with where it is under the name while searching; the host, account and
/// protocol in its tooltip. It starts after the expander's column, as a C# leaf does.
pub fn owned_row(
    profile: &ProfileSummary,
    selected: bool,
    (state, reach): (Option<SessionState>, Option<Verdict>),
    context: Option<String>,
) -> Element<'static, Message> {
    let id = profile.id.clone();
    let kind = profile.kind;
    let mut label = row![
        container(icons::icon(
            Icon::of(kind),
            Tint::Protocol(kind),
            PROTOCOL_ICON_SIDE
        ))
        .center_y(ROW_CONTENT_HEIGHT),
        container(profile_dot(state, reach.as_ref())).padding(DOT_HIT_PADDING),
        column![text(profile.name.clone()).wrapping(text::Wrapping::Glyph)].push(context.map(
            |context| {
                text(context)
                    .size(font_size::SMALL_CAPTION)
                    .style(text::secondary)
                    .wrapping(text::Wrapping::Glyph)
            }
        )),
    ]
    .spacing(spacing::XS)
    .align_y(iced::Alignment::Center);
    if profile.favorite {
        label = label.push(
            text(FAVORITE_MARK)
                .size(font_size::SMALL_CAPTION)
                .style(text::warning),
        );
    }
    if let Some(badge) = &profile.gateway {
        label = label.push(gateway_badge(badge));
    }
    if let Some(origin) = profile.metadata.origin {
        label = label.push(origin_badge(origin));
    }
    let body = RowChrome::new(
        container(label).width(Length::Fill).padding(iced::Padding {
            top: 2.0,
            right: 4.0,
            bottom: 2.0,
            left: EDGE_WIDTH + 3.0,
        }),
        if selected { Mark::Selected } else { Mark::None },
    );
    let area = mouse_area(body)
        .on_press(Message::TreeClick(id.clone()))
        .on_double_click(Message::App(AppMessage::ConnectProfile(id.clone())))
        .on_right_press(Message::OpenTreeMenu(TreeMenu::Profile(id)))
        .interaction(mouse::Interaction::Pointer);
    row![
        iced::widget::space().width(EXPANDER_SIDE + EXPANDER_GAP),
        tooltip(
            area,
            text(row_tooltip(profile, reach.as_ref())).size(font_size::CAPTION),
            tooltip::Position::Right,
        )
        .style(container::rounded_box),
    ]
    .into()
}

/// "via name" when a gateway is on the way, "gateway missing" in the warning colour when it
/// is not saved: the C# tree's badge.
fn gateway_badge(badge: &GatewayBadge) -> Element<'static, Message> {
    let missing = matches!(badge, GatewayBadge::Missing);
    let label = match badge {
        GatewayBadge::Via(name) => fl!("ui-tree-gateway-via", name = name.as_str()),
        GatewayBadge::Missing => fl!("ui-tree-gateway-missing"),
    };
    // On one line, cut at its edge: a long gateway name must not fold the row.
    container(
        text(label)
            .size(font_size::SMALL_CAPTION)
            .wrapping(text::Wrapping::None),
    )
    .max_width(BADGE_MAX_WIDTH)
    .clip(true)
    .padding([0.0, 4.0])
    .style(move |theme: &Theme| {
        // As the C# badge: the card's background, the info colour's border and the
        // secondary text; the warning colour when the gateway is missing.
        let palette = theme.extended_palette();
        let (border, text_colour) = if missing {
            (palette.warning.base.color, palette.warning.base.color)
        } else {
            (
                crate::themes::colors_of(theme).cyan,
                palette.secondary.base.color,
            )
        };
        container::Style {
            text_color: Some(text_colour),
            background: Some(palette.background.weak.color.into()),
            border: iced::Border {
                color: border,
                width: BORDER_WIDTH,
                radius: BADGE_RADIUS.into(),
            },
            ..container::Style::default()
        }
    })
    .into()
}

/// The code of the program an imported profile came from, as the C# tree's origin badge;
/// the row's tooltip names it in full.
fn origin_badge(origin: heimdall_core::metadata::ProfileOrigin) -> Element<'static, Message> {
    container(
        text(origin.badge())
            .size(font_size::SMALL_CAPTION)
            .style(text::secondary)
            .wrapping(text::Wrapping::None),
    )
    .padding([0.0, 4.0])
    .style(|theme: &Theme| container::Style {
        background: Some(theme.extended_palette().background.weak.color.into()),
        border: iced::Border {
            color: theme.extended_palette().background.strong.color,
            width: BORDER_WIDTH,
            radius: BADGE_RADIUS.into(),
        },
        ..container::Style::default()
    })
    .into()
}

fn row_tooltip(profile: &ProfileSummary, reach: Option<&Verdict>) -> String {
    let mut lines = Vec::new();
    if let Some((host, _)) = &profile.endpoint {
        lines.push(fl!("ui-tree-tooltip-host", host = host.as_str()));
    }
    if let Some(user) = profile.username.as_deref().filter(|user| !user.is_empty()) {
        lines.push(fl!("ui-tree-tooltip-user", user = user));
    }
    lines.push(fl!(
        "ui-tree-tooltip-protocol",
        protocol = profile.kind.label()
    ));
    // What it says of its server, as the C# detail panel.
    if let Some(environment) = profile.metadata.environment {
        lines.push(fl!(
            "ui-tree-tooltip-environment",
            environment = crate::texts::environment_name(Some(environment))
        ));
    }
    if !profile.metadata.tags.is_empty() {
        lines.push(fl!(
            "ui-tree-tooltip-tags",
            tags = heimdall_app::server_text(&profile.metadata.tags)
        ));
    }
    if profile.favorite {
        lines.push(fl!("ui-tree-favorite"));
    }
    if let Some(origin) = profile.metadata.origin {
        lines.push(crate::texts::origin_name(origin));
    }
    // What the background check found, as the C# dot's tooltip.
    if let Some(reach) = reach {
        lines.push(reach_text(reach));
    }
    lines.join("\n")
}

/// Opacity of a menu entry that cannot be chosen.
const DISABLED_ALPHA: f32 = 0.4;

/// A menu entry as the C# menu draws one: plain text, the whole row lit under the pointer,
/// faded when it cannot be chosen.
fn menu_style(theme: &Theme, status: button::Status) -> button::Style {
    let palette = theme.extended_palette();
    let plain = button::Style {
        text_color: palette.background.base.text,
        border: iced::Border {
            radius: radius::XS.into(),
            ..iced::Border::default()
        },
        ..button::Style::default()
    };
    match status {
        button::Status::Active => plain,
        button::Status::Hovered | button::Status::Pressed => button::Style {
            background: Some(palette.primary.weak.color.into()),
            text_color: palette.primary.weak.text,
            ..plain
        },
        button::Status::Disabled => button::Style {
            text_color: palette.background.base.text.scale_alpha(DISABLED_ALPHA),
            ..plain
        },
    }
}

/// The Delete entry: in the error colour, as in the C# menu.
fn danger_style(theme: &Theme, status: button::Status) -> button::Style {
    let palette = theme.extended_palette();
    match status {
        button::Status::Hovered | button::Status::Pressed => button::Style {
            background: Some(palette.danger.base.color.into()),
            text_color: palette.danger.base.text,
            ..menu_style(theme, status)
        },
        _ => button::Style {
            text_color: palette.danger.base.color,
            ..menu_style(theme, status)
        },
    }
}

/// One entry: a label, active when it has a message.
fn entry<'a>(label: String, message: Option<AppMessage>) -> Element<'a, Message> {
    button(text(label).size(font_size::BODY_LARGE))
        .width(Length::Fill)
        .style(menu_style)
        .on_press_maybe(message.map(Message::MenuChoice))
        .into()
}

/// The card every menu is drawn on, so what is under it never shows through.
fn menu_card<'a>(entries: impl Into<Element<'a, Message>>) -> Container<'a, Message> {
    container(entries)
        .padding(spacing::XS)
        .style(container::rounded_box)
}

fn separator<'a>() -> Element<'a, Message> {
    rule::horizontal(1).into()
}

/// The entries of `menu`, as the C# Heimdall orders them, limited to what this version does;
/// `splittable` while a tab is shown that a profile can open in a split of.
pub fn menu_entries<'a>(
    menu: &TreeMenu,
    profile: Option<&ProfileSummary>,
    connect_as: &[ConnectAs],
    (editable, splittable): (bool, bool),
) -> Element<'a, Message> {
    let mut entries = column![].spacing(0.0).width(MENU_WIDTH);
    match (menu, profile) {
        (TreeMenu::Profile(_), Some(profile)) => {
            entries = profile_entries(entries, profile, connect_as, (editable, splittable));
        }
        (TreeMenu::ConnectAs(_), Some(profile)) => {
            // The protocols but the profile's own, as the C# menu lists them.
            for protocol in connect_as {
                entries = entries.push(entry(
                    protocol.label().to_owned(),
                    Some(AppMessage::ConnectAs {
                        id: profile.id.clone(),
                        protocol: *protocol,
                    }),
                ));
            }
        }
        (TreeMenu::Add, _) => entries = add_entries(entries),
        (TreeMenu::More, _) => {
            entries = entries
                .push(entry(
                    fl!("ui-tree-import-sessions"),
                    Some(AppMessage::Sessions(SessionsMessage::File)),
                ))
                .push(entry(
                    fl!("ui-tree-import-openssh"),
                    Some(AppMessage::Sessions(SessionsMessage::Start)),
                ))
                .push(entry(
                    fl!("ui-tree-import-rdp"),
                    Some(AppMessage::Rdp(RdpMessage::Start)),
                ))
                .push(entry(
                    fl!("ui-tree-import-putty"),
                    Some(AppMessage::Sessions(SessionsMessage::Putty)),
                ))
                .push(entry(
                    fl!("ui-tree-import-citrix"),
                    Some(AppMessage::ImportCitrix),
                ))
                .push(entry(
                    fl!("ui-tree-import-known-hosts"),
                    Some(crate::hostkeys_view::app_message(HostKeysMessage::Start)),
                ))
                .push(entry(
                    fl!("ui-tree-export-sessions"),
                    Some(AppMessage::ExportSessions),
                ))
                .push(separator())
                .push(entry(
                    fl!("ui-tree-expand-all"),
                    Some(AppMessage::FoldAll(false)),
                ))
                .push(entry(
                    fl!("ui-tree-collapse-all"),
                    Some(AppMessage::FoldAll(true)),
                ));
        }
        _ => {}
    }
    menu_card(entries).into()
}

/// The tree's filters, as the C# filter button's menu: the protocols, then connected and
/// through a gateway, then the gateway badge. A box ticked leaves the menu open, as the C#
/// entries stay open, so several can be chosen. `badge`: rows show their gateway.
pub fn filter_entries<'a>(filter: &TreeFilter, badge: bool) -> Element<'a, Message> {
    let filter_box = |label: String, on: bool, message: FilterMessage| -> Element<'a, Message> {
        container(
            checkbox(on)
                .style(styles::checkbox)
                .label(label)
                .text_size(font_size::BODY_LARGE)
                .on_toggle(move |_| Message::App(AppMessage::Filter(message))),
        )
        .padding(spacing::XS)
        .into()
    };
    let mut entries = column![
        container(text(fl!("ui-tree-filter-protocols")).size(font_size::BODY_LARGE))
            .padding(spacing::XS)
    ]
    .width(MENU_WIDTH);
    for kind in ProfileKind::ALL {
        entries = entries.push(filter_box(
            kind.label().to_owned(),
            filter.has_protocol(kind),
            FilterMessage::Protocol(kind),
        ));
    }
    entries = entries
        .push(separator())
        .push(filter_box(
            fl!("ui-tree-filter-connected"),
            filter.connected(),
            FilterMessage::Connected,
        ))
        .push(filter_box(
            fl!("ui-tree-filter-gateway"),
            filter.gateway(),
            FilterMessage::Gateway,
        ))
        .push(filter_box(
            fl!("ui-tree-filter-favorites"),
            filter.favorites(),
            FilterMessage::Favorites,
        ))
        .push(separator())
        .push(filter_box(
            fl!("ui-tree-filter-gateway-badge"),
            badge,
            FilterMessage::GatewayBadge,
        ));
    menu_card(entries).into()
}

/// The entries of a session's menu about its server: its address copied, whether it
/// answers, and, when its MAC address is known, Wake-on-LAN.
fn server_entries<'a>(
    mut entries: Column<'a, Message>,
    profile: &ProfileSummary,
) -> Column<'a, Message> {
    let id = profile.id.clone();
    let copy = |what| {
        Some(AppMessage::CopyProfile {
            id: id.clone(),
            what,
        })
    };
    if profile.endpoint.is_some() {
        let has_user = profile
            .username
            .as_deref()
            .is_some_and(|user| !user.is_empty());
        entries = entries
            .push(separator())
            .push(entry(
                fl!("ui-tree-copy-hostname"),
                copy(ProfileCopy::Hostname),
            ))
            .push(entry(
                fl!("ui-tree-copy-username"),
                copy(ProfileCopy::Username).filter(|_| has_user),
            ))
            .push(entry(
                fl!("ui-tree-copy-address"),
                copy(ProfileCopy::Address),
            ));
        if profile.kind == ProfileKind::Ssh {
            entries = entries.push(entry(
                fl!("ui-tree-copy-ssh-command"),
                copy(ProfileCopy::SshCommand),
            ));
        }
        entries = entries.push(entry(
            fl!("ui-tree-test-reachability"),
            Some(AppMessage::ProfileMenu(
                ProfileMenuMessage::TestReachability(id.clone()),
            )),
        ));
    }
    // Only for a server whose MAC address is known, as the C# menu.
    if profile.metadata.mac_address.is_some() {
        entries = entries.push(entry(
            fl!("ui-tree-wake-on-lan"),
            Some(AppMessage::ProfileMenu(ProfileMenuMessage::WakeOnLan(
                id.clone(),
            ))),
        ));
    }
    entries
}

/// "Open in split", as the C#: how the profile opens beside the tab shown; disabled, saying
/// why, while no tab is shown to split.
fn open_in_split_entry<'a>(id: &ProfileId, splittable: bool) -> Element<'a, Message> {
    let label = fl!("ui-split-open-in-split");
    if splittable {
        return submenu(label, TreeMenu::OpenInSplit(id.clone()));
    }
    tooltip(
        button(text(label).size(font_size::BODY_LARGE))
            .width(Length::Fill)
            .style(menu_style),
        text(fl!("ui-split-open-in-split-disabled")).size(font_size::BODY_LARGE),
        tooltip::Position::Right,
    )
    .style(container::rounded_box)
    .into()
}

/// How profile `id` opens in a split of the tab shown, in the C# order: Horizontal,
/// stacked, then Vertical, side by side.
#[must_use]
pub fn open_in_split_entries<'a>(id: &ProfileId) -> Element<'a, Message> {
    let open = |axis| {
        Some(AppMessage::Split(SplitMessage::OpenInSplit {
            profile: id.clone(),
            axis,
        }))
    };
    let entries = column![]
        .spacing(0.0)
        .width(MENU_WIDTH)
        .push(entry(fl!("ui-split-horizontal"), open(Axis::Stacked)))
        .push(entry(fl!("ui-split-vertical"), open(Axis::SideBySide)));
    menu_card(entries).into()
}

/// A profile's menu, in the C# order this version has: Connect, Connect as, Open in split,
/// Rename, Edit, Duplicate, Move to folder, the copies, Delete.
fn profile_entries<'a>(
    mut entries: Column<'a, Message>,
    profile: &ProfileSummary,
    connect_as: &[ConnectAs],
    (editable, splittable): (bool, bool),
) -> Column<'a, Message> {
    let id = profile.id.clone();
    entries = entries.push(entry(
        fl!("ui-tree-connect"),
        Some(AppMessage::ConnectProfile(id.clone())),
    ));
    if !connect_as.is_empty() {
        entries = entries.push(submenu(
            fl!("ui-tree-connect-as"),
            TreeMenu::ConnectAs(id.clone()),
        ));
    }
    entries = entries
        .push(open_in_split_entry(&id, splittable))
        .push(entry(
            fl!("ui-tree-rename"),
            editable.then(|| AppMessage::ProfileMenu(ProfileMenuMessage::Rename(id.clone()))),
        ))
        .push(entry(
            fl!("ui-tree-edit"),
            editable.then(|| AppMessage::EditProfile(id.clone())),
        ))
        .push(entry(
            fl!("ui-tree-duplicate"),
            Some(AppMessage::DuplicateProfile {
                id: id.clone(),
                suffix: fl!("ui-tree-duplicate-suffix"),
            }),
        ))
        .push(favorite_entry(
            profile.favorite,
            AppMessage::ProfileMenu(ProfileMenuMessage::Favorite {
                id: id.clone(),
                favorite: !profile.favorite,
            }),
        ))
        .push(separator())
        .push(
            button(text(fl!("ui-tree-move-to-folder")).size(font_size::BODY_LARGE))
                .width(Length::Fill)
                .style(menu_style)
                .on_press_maybe(
                    editable.then(|| Message::OpenTreeMenu(TreeMenu::MoveProfile(id.clone()))),
                ),
        );
    entries = server_entries(entries, profile);
    entries = entries
        .push(separator())
        .push(submenu(fl!("ui-tree-notes"), TreeMenu::Notes(id.clone())));
    entries = entries.push(separator()).push(
        button(text(fl!("ui-tree-delete")).size(font_size::BODY_LARGE))
            .width(Length::Fill)
            .style(danger_style)
            .on_press(Message::MenuChoice(AppMessage::RequestDeleteProfile(id))),
    );
    entries
}

/// A session's Notes menu, as the C# one: a note about it, written from a template.
#[must_use]
pub fn notes_entries<'a>(id: &ProfileId) -> Element<'a, Message> {
    use heimdall_app::notes::NoteTemplate;
    let mut entries = column![].spacing(0.0).width(MENU_WIDTH);
    for template in NoteTemplate::ALL {
        let label = match template {
            NoteTemplate::Blank => fl!("ui-notes-new"),
            NoteTemplate::Daily => fl!("ui-notes-daily"),
            NoteTemplate::Incident => fl!("ui-notes-incident"),
            NoteTemplate::Procedure => fl!("ui-notes-procedure"),
        };
        entries = entries.push(
            button(text(label).size(font_size::BODY_LARGE))
                .width(Length::Fill)
                .style(menu_style)
                .on_press(Message::NewNote {
                    id: id.clone(),
                    template,
                }),
        );
    }
    entries.into()
}

/// The tree's own menu and the "+" button's, as the C# one: Add Session, Add gateway, New
/// folder.
fn add_entries(entries: Column<'_, Message>) -> Column<'_, Message> {
    entries
        .push(entry(
            fl!("ui-tree-add-session"),
            Some(AppMessage::NewProfile),
        ))
        .push(separator())
        .push(entry(fl!("ui-gateway-add"), Some(AppMessage::NewGateway)))
        .push(entry(
            fl!("ui-folder-new"),
            Some(AppMessage::Folder(FolderMessage::New {
                parent: String::new(),
            })),
        ))
}

/// What the menu of a Files entry needs to know of the entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[expect(
    clippy::struct_excessive_bools,
    reason = "one flag per entry offered or not, each decided on its own"
)]
pub struct FilesEntryFacts {
    /// Where it is listed.
    pub index: usize,
    /// It is chosen alone.
    pub single: bool,
    /// It is a regular file, chosen alone.
    pub one_file: bool,
    /// It is a symbolic link.
    pub link: bool,
    /// It is a script of the local file browser, chosen alone, that this platform runs:
    /// "Run in Shell" is offered.
    pub runs_in_shell: bool,
}

/// The transcript entry of a tab's menu.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TranscriptEntry {
    /// None: the tab shows no session of text.
    Absent,
    /// Start Transcript, offered when it can start now.
    Start(bool),
    /// Stop Transcript.
    Stop,
}

/// What the menu of a Files entry needs to know of its tab.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[expect(
    clippy::struct_excessive_bools,
    reason = "one flag per entry offered or not, each decided on its own"
)]
pub struct FilesTabFacts {
    /// What was cut or copied can be pasted in this tab.
    pub can_paste: bool,
    /// Copy holds its entries: to paste on its server, or on another.
    pub can_copy: bool,
    /// It is connected: its files can be edited.
    pub connected: bool,
    /// It is over SFTP: permissions are changed, and a link never renamed.
    pub sftp: bool,
    /// Its SSH connection: sudo, Duplicate and Open in terminal.
    pub over_ssh: bool,
    /// It is the local file browser docked beside a local shell: no server to send to,
    /// and "Open in Explorer" and "Open in terminal", as the C# browser.
    pub local_only: bool,
}

/// The menu of an entry of a Files tab's pane, in the C# order, limited to what this
/// version does: open it, edit it, send it to the other pane, rename, delete, cut, copy,
/// copy its path; then what applies to the folder shown. As the C#: Edit and Edit with
/// external editor for one regular file whatever the protocol, Change permissions over
/// SFTP only, and what runs on the server (sudo, Duplicate, Open in terminal) over its SSH
/// connection; Paste while what was cut or copied can be pasted in this tab. The local file
/// browser's own entries, as the C# `LocalFileBrowserView`: "Open With" (on Windows, which
/// alone has the chooser) and "Open in Editor" for one regular file, "Copy" and "Paste" of
/// its files, and "Properties" for an entry chosen alone.
pub fn files_entry_menu<'a>(
    (tab, side): (TabId, Side),
    entry_facts: Option<FilesEntryFacts>,
    tab_facts: FilesTabFacts,
) -> Element<'a, Message> {
    // An entry's own actions only on an entry, as the C# list hides them beside it.
    let on_entry = entry_facts.is_some();
    let index = entry_facts.map(|facts| facts.index);
    // Editing is of one regular file: a link, a pipe, a device or a folder is not written
    // back, as in the C#.
    let edits = side == Side::Remote
        && tab_facts.connected
        && entry_facts.is_some_and(|facts| facts.one_file);
    // SFTP renames follow a link to its target: no rename of a link there, as the C#; an
    // FTP rename is by name, a link renamed itself.
    let renames = entry_facts.is_some_and(|facts| facts.single && !(tab_facts.sftp && facts.link));
    // A regular file of the local file browser, chosen alone.
    let local_file = entry_facts
        .filter(|facts| tab_facts.local_only && facts.one_file)
        .map(|facts| facts.index);
    let files = |message| Some(AppMessage::Files(message));
    let server = |entry: Element<'a, Message>| (side == Side::Remote).then_some(entry);
    let (send, direction) = match side {
        Side::Remote => (fl!("ui-files-menu-download"), Direction::Download),
        Side::Local => (fl!("ui-files-menu-upload"), Direction::Upload),
    };
    let entries = column![
        index.map(|index| entry(
            fl!("ui-files-menu-open"),
            files(FilesMessage::Open { tab, side, index })
        )),
        // The system's "Open with" chooser is Windows' alone.
        local_file.filter(|_| cfg!(windows)).map(|index| entry(
            fl!("ui-files-menu-open-with"),
            files(FilesMessage::OpenWith { tab, index })
        )),
        local_file.map(|index| entry(
            fl!("ui-files-menu-open-in-editor"),
            files(FilesMessage::OpenInEditor { tab, index })
        )),
        edits.then(|| entry(
            fl!("ui-files-menu-edit-integrated"),
            files(FilesMessage::EditIntegrated { tab })
        )),
        edits.then(|| entry(
            fl!("ui-files-menu-edit-external"),
            files(FilesMessage::EditExternal { tab })
        )),
        (edits && tab_facts.over_ssh).then(|| entry(
            fl!("ui-files-menu-edit-sudo"),
            files(FilesMessage::EditWithSudo { tab })
        )),
        (on_entry && !tab_facts.local_only)
            .then(|| entry(send, files(FilesMessage::Transfer { tab, direction }))),
        on_entry.then(separator),
        renames.then(|| entry(
            fl!("ui-files-menu-rename"),
            files(FilesMessage::AskRename { tab, side })
        )),
        on_entry.then(|| entry(
            fl!("ui-files-menu-delete"),
            files(FilesMessage::AskDelete { tab, side })
        )),
        // The server's entries only, over SFTP only, as in the C# Files tab.
        (on_entry && tab_facts.sftp)
            .then(|| server(entry(
                fl!("ui-files-menu-permissions"),
                files(FilesMessage::AskPermissions { tab, side })
            )))
            .flatten(),
        on_entry.then(separator),
        clipboard_entries((tab, side), entry_facts, tab_facts),
        on_entry.then(separator),
        entry(
            fl!("ui-files-menu-new-folder"),
            files(FilesMessage::AskNewFolder { tab, side })
        ),
        entry(
            fl!("ui-files-menu-refresh"),
            files(FilesMessage::Refresh { tab, side })
        ),
        outside_entries(
            tab,
            side == Side::Remote && tab_facts.over_ssh,
            tab_facts.local_only,
            entry_facts
        ),
    ]
    .spacing(0.0)
    .width(MENU_WIDTH);
    menu_card(entries).into()
}

/// The middle entries of a Files entry's menu, about the clipboard and the entry itself:
/// "Upload here..." and "Paste from Explorer" in the server's pane; Cut, Copy, Paste and
/// Duplicate as the C# Files tab offers them, and Copy and Paste of the local file
/// browser's files (`local_only`); "Copy path"; "Properties" of the server's entries, and
/// of the local file browser's entry chosen alone.
fn clipboard_entries<'a>(
    (tab, side): (TabId, Side),
    entry_facts: Option<FilesEntryFacts>,
    tab_facts: FilesTabFacts,
) -> Column<'a, Message> {
    let on_entry = entry_facts.is_some();
    let remote = side == Side::Remote;
    let local_only = tab_facts.local_only;
    let files = |message| Some(AppMessage::Files(message));
    column![
        remote.then(|| entry(
            fl!("ui-files-menu-upload-here"),
            files(FilesMessage::UploadHere { tab })
        )),
        // Explorer's copied files are read on Windows only.
        (remote && cfg!(windows)).then(|| entry(
            fl!("ui-files-menu-paste-explorer"),
            files(FilesMessage::PasteFromExplorer { tab })
        )),
        (remote && on_entry)
            .then(|| entry(fl!("ui-files-menu-cut"), files(FilesMessage::Cut { tab }))),
        (on_entry && ((remote && tab_facts.can_copy) || local_only))
            .then(|| entry(fl!("ui-files-menu-copy"), files(FilesMessage::Copy { tab }))),
        ((remote || local_only) && tab_facts.can_paste).then(|| entry(
            fl!("ui-files-menu-paste"),
            files(FilesMessage::Paste { tab })
        )),
        (remote && tab_facts.over_ssh && on_entry).then(|| entry(
            fl!("ui-files-menu-duplicate"),
            files(FilesMessage::Duplicate { tab })
        )),
        on_entry.then(|| entry(
            fl!("ui-files-menu-copy-path"),
            files(FilesMessage::CopyPath { tab, side })
        )),
        ((remote && on_entry) || (local_only && entry_facts.is_some_and(|facts| facts.single)))
            .then(|| entry(
                fl!("ui-files-menu-properties"),
                files(FilesMessage::ShowProperties { tab, side })
            )),
    ]
    .spacing(0.0)
    .width(Length::Fill)
}

/// The last entries of a Files entry's menu, opening the folder outside the tab: "Open in
/// terminal" in the server's pane over its SSH connection (`over_ssh`); "Open in Explorer"
/// then "Open in terminal", a new local shell there, in the local file browser
/// (`local_only`), as the C# menus; then "Run in Shell", in a new tab too, when the entry
/// (`entry_facts`) is a script this platform runs.
fn outside_entries<'a>(
    tab: TabId,
    over_ssh: bool,
    local_only: bool,
    entry_facts: Option<FilesEntryFacts>,
) -> Column<'a, Message> {
    let files = |message| Some(AppMessage::Files(message));
    let script = entry_facts
        .filter(|facts| local_only && facts.runs_in_shell)
        .map(|facts| facts.index);
    column![
        over_ssh.then(|| entry(
            fl!("ui-files-menu-open-in-terminal"),
            files(FilesMessage::OpenInTerminal { tab })
        )),
        local_only.then(|| entry(
            fl!("ui-files-menu-open-in-explorer"),
            files(FilesMessage::OpenInExplorer { tab })
        )),
        local_only.then(|| entry(
            fl!("ui-files-menu-open-in-terminal"),
            files(FilesMessage::OpenInTerminal { tab })
        )),
        script.map(|index| entry(
            fl!("ui-files-menu-run-in-shell"),
            files(FilesMessage::RunInShell { tab, index })
        )),
    ]
    .spacing(0.0)
    .width(Length::Fill)
}

/// The menu of a Files tab's star, as the C#'s: "Bookmark this path" first, then the
/// server's folders bookmarked, `bookmarks` as shown, each going there; a line saying there
/// are none, as the C# menu.
pub fn files_bookmarks_menu<'a>(tab: TabId, bookmarks: &[String]) -> Element<'a, Message> {
    let mut entries = column![
        entry(
            fl!("ui-files-bookmark-button"),
            Some(AppMessage::Files(FilesMessage::Bookmark { tab })),
        ),
        separator(),
    ]
    .spacing(0.0)
    .width(MENU_WIDTH);
    if bookmarks.is_empty() {
        entries = entries.push(entry(fl!("ui-files-bookmarks-empty"), None));
    }
    for (index, path) in bookmarks.iter().enumerate() {
        entries = entries.push(entry(
            path.clone(),
            Some(AppMessage::Files(FilesMessage::OpenBookmark { tab, index })),
        ));
    }
    if !bookmarks.is_empty() {
        entries = entries.push(separator()).push(submenu(
            fl!("ui-files-bookmark-remove-menu"),
            TreeMenu::FilesBookmarksRemove(tab),
        ));
    }
    menu_card(entries).into()
}

/// Which of a Files tab's bookmarks to take off, as the C# "Remove a bookmark".
#[must_use]
pub fn files_bookmarks_remove_menu<'a>(tab: TabId, bookmarks: &[String]) -> Element<'a, Message> {
    let mut entries = column![].spacing(0.0).width(MENU_WIDTH);
    for (index, path) in bookmarks.iter().enumerate() {
        entries = entries.push(entry(
            path.clone(),
            Some(AppMessage::Files(FilesMessage::RemoveBookmark {
                tab,
                index,
            })),
        ));
    }
    menu_card(entries).into()
}

/// What a tab's menu offers, worked out by the window from the core.
#[derive(Debug, Clone)]
#[expect(
    clippy::struct_excessive_bools,
    reason = "one flag per entry that can be greyed out, each decided on its own"
)]
pub struct TabMenuState {
    /// The tab.
    pub tab: TabId,
    /// It has a name the user gave it.
    pub renamed: bool,
    /// Its session can open again in its place.
    pub can_restart: bool,
    /// Its session can open again in a new tab.
    pub can_reopen: bool,
    /// The saved profile it was opened from, while saved.
    pub profile: Option<ProfileSummary>,
    /// That profile can be edited.
    pub editable: bool,
    /// There are other tabs.
    pub others: bool,
    /// There are tabs after it.
    pub right: bool,
    /// Its transcript entry.
    pub transcript: TranscriptEntry,
    /// It shows a remote desktop whose size can be chosen: an RDP one.
    pub resolution: bool,
    /// Its server health panel, for an SSH shell: shown or not.
    pub health: Option<bool>,
    /// It is pinned.
    pub pinned: bool,
    /// A VNC desktop resized to the tab, or not; `None` for one that cannot be.
    pub vnc_resize: Option<bool>,
    /// Its session is saved nowhere, and can be saved as a profile.
    pub saveable: bool,
    /// It takes terminal macros.
    pub macros: bool,
    /// Opened from a pane's header: Disconnect closes that pane alone.
    pub pane: bool,
    /// It is a pane docked in another tab's split: off the strip, it is never pinned.
    pub docked: bool,
    /// It can be detached to a window of its own: a tab of the strip, not split.
    pub detach: bool,
    /// Its split has a secondary pane "Detach Secondary Pane" can move to a window of its
    /// own, offered in place of "Detach to Window" as the C# offers it on a split tab.
    pub detach_secondary: bool,
    /// What it offers of a split.
    pub split: SplitEntries,
}

/// What a tab's menu offers of a split, as the C# one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SplitEntries {
    /// Not split: "Split...", and "Merge with..." while another tab can be merged into it.
    Merge(bool),
    /// A pane of the split of this tab of the strip: Unsplit, Swap Panes, Toggle Split
    /// Orientation, Close Secondary Pane.
    Split(TabId),
}

/// A terminal tab's Macros menu: recording started or stopped, the macro typed stopped,
/// or one of those kept typed.
#[must_use]
pub fn macro_entries<'a>(tab: TabId, menu: &heimdall_app::MacroMenu) -> Element<'a, Message> {
    let macro_message = |message| Some(AppMessage::Macro(message));
    let mut entries = column![
        container(
            text(fl!("ui-macros-menu"))
                .size(font_size::BODY_LARGE)
                .style(text::secondary)
        )
        .padding(spacing::XS),
        separator(),
    ]
    .spacing(0.0)
    .width(MENU_WIDTH);
    entries = entries.push(match (menu.recording, &menu.playing) {
        (Some(count), _) => entry(
            fl!("ui-macros-stop-recording", count = count),
            macro_message(heimdall_app::MacroMessage::StopRecording(tab)),
        ),
        (None, Some(_)) => entry(fl!("ui-macros-record"), None),
        (None, None) => entry(
            fl!("ui-macros-record"),
            macro_message(heimdall_app::MacroMessage::Record(tab)),
        ),
    });
    if let Some(name) = &menu.playing {
        entries = entries.push(entry(
            fl!("ui-macros-stop", name = heimdall_app::server_text(name)),
            macro_message(heimdall_app::MacroMessage::Stop(tab)),
        ));
    }
    entries = entries.push(separator());
    if menu.macros.is_empty() {
        entries = entries.push(entry(fl!("ui-macros-none"), None));
    }
    // One at a time, and not into what is being recorded.
    let idle = menu.recording.is_none() && menu.playing.is_none();
    for name in &menu.macros {
        entries = entries.push(entry(
            fl!("ui-macros-play", name = heimdall_app::server_text(name)),
            idle.then(|| {
                AppMessage::Macro(heimdall_app::MacroMessage::Play {
                    tab,
                    name: name.clone(),
                })
            }),
        ));
    }
    entries.into()
}

/// The tab menu's entry showing or hiding an SSH shell's server health panel.
fn health_entry<'a>(tab: TabId, shown: bool) -> Element<'a, Message> {
    let label = if shown {
        fl!("ui-tab-menu-hide-health")
    } else {
        fl!("ui-tab-menu-show-health")
    };
    entry(
        label,
        Some(AppMessage::TabMenu(TabMenuMessage::ToggleHealth(tab))),
    )
}

/// What an RDP tab's "Resolution" menu shows.
#[derive(Debug, Clone, Copy)]
pub struct ResolutionMenuState {
    /// The tab.
    pub tab: TabId,
    /// The size its desktop keeps; `None` while it follows the tab.
    pub fixed: Option<(u16, u16)>,
    /// It was opened from a saved profile, which can keep the size.
    pub saved: bool,
    /// The mode its desktop is in: chosen for the session, else its profile's.
    pub mode: Resolution,
    /// The tab's size, when known: a size larger than it is shown scaled.
    pub shown: Option<(u16, u16)>,
    /// The proportions kept under "Match window".
    pub aspect: heimdall_app::Aspect,
}

impl ResolutionMenuState {
    /// "Active mode: Fixed (1920x1080)", as the C# menu's header.
    fn header(&self) -> String {
        match self.fixed {
            Some((width, height)) => fl!(
                "ui-resolution-header-size",
                label = fl!("ui-resolution-active-mode"),
                mode = mode_name(self.mode),
                width = width,
                height = height
            ),
            None => fl!(
                "ui-resolution-header",
                label = fl!("ui-resolution-active-mode"),
                mode = mode_name(self.mode)
            ),
        }
    }

    /// The session bar's button tip: "Change resolution - Fixed (1920x1080)", as the C#.
    #[must_use]
    pub fn tooltip(&self) -> String {
        match self.fixed {
            Some((width, height)) => fl!(
                "ui-resolution-tooltip-size",
                mode = mode_name(self.mode),
                width = width,
                height = height
            ),
            None => fl!("ui-resolution-tooltip", mode = mode_name(self.mode)),
        }
    }

    /// Whether `size` is larger than the tab, and so shown scaled.
    fn larger_than_tab(&self, (width, height): (u16, u16)) -> bool {
        self.shown
            .is_some_and(|(shown_width, shown_height)| width > shown_width || height > shown_height)
    }
}

/// A resolution mode's name, as the C# lists it.
fn mode_name(mode: Resolution) -> String {
    match mode {
        Resolution::FitWindow => fl!("ui-resolution-mode-fit-window"),
        Resolution::Fixed => fl!("ui-resolution-mode-fixed"),
        Resolution::SmartSizing => fl!("ui-resolution-mode-smart-sizing"),
        Resolution::MultiMonitor => fl!("ui-resolution-mode-multi-monitor"),
        Resolution::Auto => fl!("ui-resolution-mode-auto"),
    }
}

/// What a checked entry shows before its label, and an unchecked one.
const CHECKED: &str = "\u{2713}";

/// The star a favorite's row carries, as the C# tree's.
const FAVORITE_MARK: &str = "\u{2605}";

/// Width of the column a checked entry's mark is in, so labels line up.
const CHECK_WIDTH: f32 = 18.0;

/// An entry that is the current choice or not, as the C# checked menu items.
fn checked_entry<'a>(label: String, checked: bool, message: AppMessage) -> Element<'a, Message> {
    button(
        row![
            text(if checked { CHECKED } else { "" })
                .size(font_size::BODY_LARGE)
                .width(CHECK_WIDTH),
            text(label).size(font_size::BODY_LARGE),
        ]
        .align_y(iced::Alignment::Center),
    )
    .width(Length::Fill)
    .style(menu_style)
    .on_press(Message::MenuChoice(message))
    .into()
}

/// An RDP tab's "Resolution" menu, as the C# one: the active mode, "Match window", the
/// `presets` the settings offer, "Custom...", then "Save as default for this server".
pub fn resolution_entries<'a>(
    state: &ResolutionMenuState,
    presets: &[(u16, u16)],
) -> Element<'a, Message> {
    let tab = state.tab;
    let choose = |choice| AppMessage::TabMenu(TabMenuMessage::Resolution { tab, choice });
    let header = state.header();
    let mut entries = column![
        container(
            text(header)
                .size(font_size::BODY_LARGE)
                .style(text::secondary)
        )
        .padding(spacing::XS),
        separator(),
        checked_entry(
            fl!("ui-resolution-match-window"),
            state.fixed.is_none() && state.aspect == heimdall_app::Aspect::Stretch,
            choose(ResolutionChoice::MatchWindow),
        ),
    ]
    .spacing(0.0)
    .width(MENU_WIDTH);
    // Under it, as the C# sub-menu: the window followed, fitted to a ratio.
    for aspect in heimdall_app::Aspect::RATIOS {
        let Some((wide, high)) = aspect.ratio() else {
            continue;
        };
        entries = entries.push(checked_entry(
            fl!("ui-resolution-match-aspect", wide = wide, high = high),
            state.fixed.is_none() && state.aspect == aspect,
            choose(ResolutionChoice::MatchAspect(aspect)),
        ));
    }
    for &(width, height) in presets {
        let size = fixed_desktop(width, height);
        let preset = checked_entry(
            format!("{width} x {height}"),
            state.fixed == Some(size),
            choose(ResolutionChoice::Fixed { width, height }),
        );
        // A size the tab cannot show whole says so, as the C# preset tooltip.
        entries = entries.push(if state.larger_than_tab(size) {
            tooltip(
                preset,
                text(fl!("ui-resolution-larger-than-window")).size(font_size::BODY_LARGE),
                tooltip::Position::Left,
            )
            .style(container::rounded_box)
            .into()
        } else {
            preset
        });
    }
    entries = entries
        .push(separator())
        .push(entry(
            fl!("ui-resolution-custom"),
            Some(choose(ResolutionChoice::Custom)),
        ))
        .push(separator())
        .push(entry(
            fl!("ui-resolution-save-default"),
            state.saved.then(|| choose(ResolutionChoice::SaveDefault)),
        ));
    menu_card(entries).into()
}

/// A tab's entries for the saved profile it was opened from: Edit, the copies, and Reveal
/// in tree.
fn profile_tab_entries<'a>(
    entries: Column<'a, Message>,
    tab: TabId,
    profile: &ProfileSummary,
    editable: bool,
) -> Column<'a, Message> {
    let id = profile.id.clone();
    let has_user = profile
        .username
        .as_deref()
        .is_some_and(|user| !user.is_empty());
    entries
        .push(separator())
        .push(entry(
            fl!("ui-tree-edit"),
            editable.then(|| AppMessage::EditProfile(id.clone())),
        ))
        .push(entry(
            fl!("ui-tree-copy-hostname"),
            profile.endpoint.is_some().then(|| AppMessage::CopyProfile {
                id: id.clone(),
                what: ProfileCopy::Hostname,
            }),
        ))
        .push(entry(
            fl!("ui-tree-copy-username"),
            has_user.then(|| AppMessage::CopyProfile {
                id,
                what: ProfileCopy::Username,
            }),
        ))
        .push(entry(
            fl!("ui-tab-menu-reveal-in-tree"),
            Some(AppMessage::TabMenu(TabMenuMessage::RevealInTree(tab))),
        ))
}

/// An entry that opens the menu `menu` beside the one shown.
fn submenu<'a>(label: String, menu: TreeMenu) -> Element<'a, Message> {
    button(text(label).size(font_size::BODY_LARGE))
        .width(Length::Fill)
        .style(menu_style)
        .on_press(Message::OpenTreeMenu(menu))
        .into()
}

/// The entries of a tab's menu, in the C# Heimdall's order, limited to what this version
/// does.
pub fn tab_menu_entries<'a>(state: &TabMenuState) -> Element<'a, Message> {
    let tab = state.tab;
    let menu = |message| Some(AppMessage::TabMenu(message));
    let mut entries = column![]
        .spacing(0.0)
        .width(MENU_WIDTH)
        .push(entry(
            fl!("ui-tab-menu-disconnect"),
            Some(disconnect(state)),
        ))
        .push(entry(
            fl!("ui-tab-menu-rename"),
            menu(TabMenuMessage::Rename(tab)),
        ));
    if state.renamed {
        entries = entries.push(entry(
            fl!("ui-tab-menu-reset-title"),
            menu(TabMenuMessage::ResetTitle(tab)),
        ));
    }
    if !state.docked {
        entries = entries.push(entry(
            if state.pinned {
                fl!("ui-tab-menu-unpin")
            } else {
                fl!("ui-tab-menu-pin")
            },
            menu(TabMenuMessage::Pin(tab)),
        ));
    }
    if let Some(on) = state.vnc_resize {
        entries = entries.push(separator()).push(checked_entry(
            fl!("ui-tab-menu-vnc-remote-resize"),
            on,
            AppMessage::TabMenu(TabMenuMessage::VncRemoteResize(tab)),
        ));
    }
    if state.resolution {
        entries = entries.push(separator()).push(submenu(
            fl!("ui-resolution-menu"),
            TreeMenu::Resolution(tab),
        ));
    }
    if state.macros {
        entries = entries
            .push(separator())
            .push(submenu(fl!("ui-macros-menu"), TreeMenu::Macros(tab)));
    }
    entries = entries
        .push(separator())
        .push(
            button(text(fl!("ui-tab-menu-fullscreen")).size(font_size::BODY_LARGE))
                .width(Length::Fill)
                .style(menu_style)
                .on_press(Message::MenuFullscreen(tab)),
        )
        .push(entry(
            fl!("ui-tab-menu-reconnect"),
            state.can_restart.then_some(AppMessage::ReconnectTab(tab)),
        ))
        .push(entry(
            fl!("ui-tab-menu-duplicate"),
            state
                .can_reopen
                .then(|| AppMessage::TabMenu(TabMenuMessage::Duplicate(tab))),
        ));
    if state.saveable {
        entries = entries.push(entry(
            fl!("ui-tab-menu-save-as-profile"),
            menu(TabMenuMessage::SaveAsProfile(tab)),
        ));
    }
    if let Some(profile) = &state.profile {
        entries = profile_tab_entries(entries, tab, profile, state.editable);
    }
    entries = session_entries(entries, state);
    let close = |group| menu(TabMenuMessage::Close { tab, group });
    entries = entries
        .push(separator())
        .push(entry(
            fl!("ui-tab-menu-close-others"),
            close(TabGroup::Others).filter(|_| state.others),
        ))
        .push(entry(
            fl!("ui-tab-menu-close-right"),
            close(TabGroup::Right).filter(|_| state.right),
        ));
    menu_card(split_entries(entries, tab, state.split)).into()
}

/// A tab's entries after its profile's, in the C# order: "Detach to Window", or on a split
/// tab "Detach Secondary Pane", as the C# `AppendDetachItem` places them, then the
/// transcript and the server health panel.
fn session_entries<'a>(
    mut entries: Column<'a, Message>,
    state: &TabMenuState,
) -> Column<'a, Message> {
    let tab = state.tab;
    if state.detach {
        entries = entries.push(separator()).push(entry(
            fl!("ui-tab-menu-detach"),
            Some(AppMessage::Float(FloatMessage::Detach(tab))),
        ));
    } else if let SplitEntries::Split(host) = state.split
        && state.detach_secondary
    {
        entries = entries.push(separator()).push(entry(
            fl!("ui-split-detach-secondary"),
            Some(AppMessage::Float(FloatMessage::DetachSecondary(host))),
        ));
    }
    entries = match state.transcript {
        TranscriptEntry::Absent => entries,
        TranscriptEntry::Start(can) => entries.push(separator()).push(entry(
            fl!("ui-tab-menu-start-transcript"),
            can.then(|| AppMessage::TabMenu(TabMenuMessage::StartTranscript(tab))),
        )),
        TranscriptEntry::Stop => entries.push(separator()).push(entry(
            fl!("ui-tab-menu-stop-transcript"),
            Some(AppMessage::TabMenu(TabMenuMessage::StopTranscript(tab))),
        )),
    };
    if let Some(shown) = state.health {
        entries = entries.push(health_entry(tab, shown));
    }
    entries
}

/// What a tab menu's Disconnect closes: from a pane's header the pane alone, asked as its
/// close button asks; from the strip the whole tab, as the C# `CloseAllPanes`.
fn disconnect(state: &TabMenuState) -> AppMessage {
    if state.pane {
        AppMessage::Split(SplitMessage::ClosePane(state.tab))
    } else {
        AppMessage::RequestCloseTab(state.tab)
    }
}

/// The end of a tab's menu, after a separator as the C#'s: "Split..." and "Merge with..."
/// for a tab not split, else what its split offers.
fn split_entries(
    entries: Column<'_, Message>,
    tab: TabId,
    split: SplitEntries,
) -> Column<'_, Message> {
    let split_message = |message| Some(AppMessage::Split(message));
    match split {
        SplitEntries::Merge(mergeable) => {
            let entries = entries
                .push(separator())
                .push(submenu(fl!("ui-split-menu"), TreeMenu::SplitAxis(tab)));
            if mergeable {
                entries.push(submenu(
                    fl!("ui-split-merge-with"),
                    TreeMenu::MergeWith(tab),
                ))
            } else {
                entries
            }
        }
        SplitEntries::Split(host) => entries
            .push(separator())
            .push(entry(
                fl!("ui-split-unsplit"),
                split_message(SplitMessage::Unsplit(host)),
            ))
            .push(entry(
                fl!("ui-split-swap-panes"),
                split_message(SplitMessage::Swap(host)),
            ))
            .push(entry(
                fl!("ui-split-toggle-orientation"),
                split_message(SplitMessage::ToggleAxis(host)),
            ))
            .push(entry(
                fl!("ui-split-close-secondary"),
                split_message(SplitMessage::CloseSecondary(host)),
            )),
    }
}

/// "Split...": how `host` is split, in the C# order, Horizontal, stacked, then Vertical,
/// side by side; each opens Quick Connect to choose what goes beside it.
#[must_use]
pub fn split_axis_entries<'a>(host: TabId) -> Element<'a, Message> {
    let split = |label: String, axis| -> Element<'a, Message> {
        button(text(label).size(font_size::BODY_LARGE))
            .width(Length::Fill)
            .style(menu_style)
            .on_press(Message::SplitPalette { host, axis })
            .into()
    };
    let entries = column![]
        .spacing(0.0)
        .width(MENU_WIDTH)
        .push(split(fl!("ui-split-horizontal"), Axis::Stacked))
        .push(split(fl!("ui-split-vertical"), Axis::SideBySide));
    menu_card(entries).into()
}

/// "Merge with...": the tabs `host` can be merged with, each by its title, opening how.
#[must_use]
pub fn merge_with_entries<'a>(host: TabId, tabs: &[(TabId, String)]) -> Element<'a, Message> {
    let entries = column![].spacing(0.0).width(MENU_WIDTH).extend(
        tabs.iter()
            .map(|(tab, title)| submenu(title.clone(), TreeMenu::MergeAxis { host, tab: *tab })),
    );
    menu_card(entries).into()
}

/// How `tab` is merged into `host`, in the C# order: Horizontal, stacked, then Vertical,
/// side by side.
#[must_use]
pub fn merge_axis_entries<'a>(host: TabId, tab: TabId) -> Element<'a, Message> {
    let merge = |axis| {
        Some(AppMessage::Split(SplitMessage::Merge {
            host,
            tab,
            axis,
            placement: Placement::Second,
        }))
    };
    let entries = column![]
        .spacing(0.0)
        .width(MENU_WIDTH)
        .push(entry(fl!("ui-split-horizontal"), merge(Axis::Stacked)))
        .push(entry(fl!("ui-split-vertical"), merge(Axis::SideBySide)));
    menu_card(entries).into()
}

/// How tall a tunnel row's menu is, about: it opens above the cursor, the panel being at the
/// window's foot. Three entries of a menu's text and the button's room, a separator, the
/// card's padding.
pub const TUNNEL_MENU_HEIGHT: f32 =
    3.0 * (font_size::BODY_LARGE * 1.3 + 10.0) + 1.0 + 2.0 * spacing::XS;

/// A tunnel row's menu, as the C# one: Close Tunnel, Copy Local Port, then Close All Tunnels.
pub fn tunnel_menu_entries<'a>(
    id: heimdall_app::tunnel::TunnelId,
    interrupted: bool,
) -> Element<'a, Message> {
    let tunnel = |message| Some(AppMessage::Tunnel(message));
    let mut entries = column![].spacing(0.0).width(MENU_WIDTH);
    // An interrupted tunnel is opened again from its row, as it was asked for.
    if interrupted {
        entries = entries.push(entry(
            fl!("ui-tunnels-menu-reopen"),
            tunnel(heimdall_app::TunnelMessage::Reopen(id)),
        ));
    }
    let entries = entries
        .push(entry(
            fl!("ui-tunnels-menu-close"),
            tunnel(heimdall_app::TunnelMessage::Close(id)),
        ))
        .push(entry(
            fl!("ui-tunnels-menu-copy-port"),
            tunnel(heimdall_app::TunnelMessage::CopyPort(id)),
        ))
        .push(separator())
        .push(entry(
            fl!("ui-tunnels-menu-close-all"),
            tunnel(heimdall_app::TunnelMessage::CloseAll),
        ));
    menu_card(entries).into()
}

/// A folder's menu, in the C# order: Connect all, Add Session and New folder in it, then,
/// for a folder of its own, Rename, Move to and Delete folder.
pub fn folder_menu_entries<'a>(path: &str, connectable: usize) -> Element<'a, Message> {
    let folder = |message| Some(AppMessage::Folder(message));
    let mut entries = column![]
        .spacing(0.0)
        .width(MENU_WIDTH)
        .push(entry(
            fl!("ui-folder-connect-all", count = connectable),
            (connectable > 0)
                .then(|| AppMessage::Folder(FolderMessage::RequestConnectAll(path.to_owned()))),
        ))
        .push(separator())
        .push(entry(
            fl!("ui-tree-add-session"),
            folder(FolderMessage::NewProfileIn(path.to_owned())),
        ));
    if path == NO_FOLDER {
        entries = entries.push(entry(
            fl!("ui-folder-new"),
            folder(FolderMessage::New {
                parent: String::new(),
            }),
        ));
    } else {
        entries = entries
            .push(entry(
                fl!("ui-folder-new"),
                folder(FolderMessage::New {
                    parent: path.to_owned(),
                }),
            ))
            .push(separator())
            .push(entry(
                fl!("ui-folder-rename"),
                folder(FolderMessage::Rename(path.to_owned())),
            ))
            .push(submenu(
                fl!("ui-folder-move-to"),
                TreeMenu::MoveFolder(path.to_owned()),
            ))
            .push(
                button(text(fl!("ui-folder-color")).size(font_size::BODY_LARGE))
                    .width(Length::Fill)
                    .style(menu_style)
                    .on_press(Message::OpenTreeMenu(TreeMenu::FolderColor(
                        path.to_owned(),
                    ))),
            )
            .push(
                button(text(fl!("ui-folder-delete")).size(font_size::BODY_LARGE))
                    .width(Length::Fill)
                    .style(danger_style)
                    .on_press(Message::MenuChoice(AppMessage::Folder(
                        FolderMessage::RequestDelete(path.to_owned()),
                    ))),
            );
    }
    menu_card(entries).into()
}

/// Which folder profile `id` can move to: "(No Folder)" first, then every folder, its own
/// greyed, as the C# "Move to folder".
#[must_use]
pub fn move_profile_entries<'a>(
    id: &ProfileId,
    targets: &[(Option<String>, bool)],
) -> Element<'a, Message> {
    let entries = column![]
        .spacing(0.0)
        .width(MENU_WIDTH)
        .extend(targets.iter().map(|(to, other)| {
            let label = to.clone().unwrap_or_else(|| fl!("ui-sidebar-group-none"));
            entry(
                label,
                other.then(|| {
                    AppMessage::ProfileMenu(ProfileMenuMessage::Move {
                        id: id.clone(),
                        to: to.clone(),
                    })
                }),
            )
        }));
    menu_card(styles::scroll(entries).height(Length::Shrink))
        .max_height(MOVE_MENU_HEIGHT)
        .into()
}

/// "Add to favorites", or "Remove from favorites" once `favorite`, as the C# entry offers
/// it: `message` does it.
fn favorite_entry<'a>(favorite: bool, message: AppMessage) -> Element<'a, Message> {
    let label = if favorite {
        fl!("ui-tree-favorite-remove")
    } else {
        fl!("ui-tree-favorite-add")
    };
    entry(label, Some(message))
}

/// The menu of `count` profiles selected together, as the C# bulk menu: how many, Connect
/// selected, Duplicate selected, the favorites, Move to folder, Delete selected; removing
/// from the favorites once `all_favorites`.
pub fn selection_menu_entries<'a>(
    count: usize,
    connectable: usize,
    all_favorites: bool,
) -> Element<'a, Message> {
    let selection = |message| Some(AppMessage::Selection(message));
    let entries = column![]
        .spacing(0.0)
        .width(MENU_WIDTH)
        .push(entry(fl!("ui-selection-count", count = count), None))
        .push(separator())
        .push(entry(
            fl!("ui-selection-connect", count = connectable),
            (connectable > 0).then_some(AppMessage::Selection(SelectionMessage::Connect)),
        ))
        .push(entry(
            fl!("ui-selection-duplicate"),
            selection(SelectionMessage::Duplicate {
                suffix: fl!("ui-tree-duplicate-suffix"),
            }),
        ))
        .push(favorite_entry(
            all_favorites,
            AppMessage::Selection(SelectionMessage::Favorite(!all_favorites)),
        ))
        .push(separator())
        .push(submenu(fl!("ui-selection-edit"), TreeMenu::EditSelection))
        .push(submenu(
            fl!("ui-tree-move-to-folder"),
            TreeMenu::MoveSelection,
        ))
        .push(separator())
        .push(
            button(text(fl!("ui-selection-delete", count = count)).size(font_size::BODY_LARGE))
                .width(Length::Fill)
                .style(danger_style)
                .on_press(Message::MenuChoice(AppMessage::Selection(
                    SelectionMessage::RequestDelete,
                ))),
        );
    menu_card(entries).into()
}

/// What the profiles selected together can be set at once, as the C# "Edit" menu: their
/// port, the account of the `usernames` among them that take one, and the password of the
/// `passwords` whose password is saved, unless `blocked` says why none can be saved now.
pub fn edit_selection_entries<'a>(
    usernames: usize,
    passwords: usize,
    blocked: Option<String>,
    routed: usize,
) -> Element<'a, Message> {
    let edit = |field| Some(AppMessage::Selection(SelectionMessage::Edit(field)));
    let mut entries = column![]
        .spacing(0.0)
        .width(MENU_WIDTH)
        .push(entry(fl!("ui-selection-edit-port"), edit(BulkField::Port)))
        .push(entry(
            fl!("ui-selection-edit-username", count = usernames),
            edit(BulkField::Username).filter(|_| usernames > 0),
        ))
        .push(entry(
            fl!("ui-selection-edit-password", count = passwords),
            (passwords > 0 && blocked.is_none())
                .then_some(AppMessage::Selection(SelectionMessage::EditPassword)),
        ));
    if let Some(blocked) = blocked {
        entries = entries.push(
            container(text(blocked).size(font_size::CAPTION))
                .padding(spacing::XS)
                .width(Length::Fill),
        );
    }
    let entries = entries.push(
        button(text(fl!("ui-selection-set-gateway", count = routed)).size(font_size::BODY_LARGE))
            .width(Length::Fill)
            .style(menu_style)
            .on_press_maybe(
                (routed > 0).then_some(Message::OpenTreeMenu(TreeMenu::GatewaySelection)),
            ),
    );
    menu_card(entries).into()
}

/// The routes the profiles selected together can be given, as the C# "Set gateway":
/// directly, then through each gateway saved.
#[must_use]
pub fn gateway_selection_entries<'a>(
    gateways: &[heimdall_core::profile::SshGateway],
) -> Element<'a, Message> {
    let set = |gateway| Some(AppMessage::Selection(SelectionMessage::SetGateway(gateway)));
    let entries = column![]
        .spacing(0.0)
        .width(MENU_WIDTH)
        .push(entry(fl!("ui-selection-gateway-direct"), set(None)))
        .push(separator())
        .extend(gateways.iter().map(|gateway| {
            entry(
                heimdall_app::server_text(&gateway.name),
                set(Some(gateway.id.clone())),
            )
        }));
    menu_card(styles::scroll(entries).height(Length::Shrink))
        .max_height(MOVE_MENU_HEIGHT)
        .into()
}

/// Which folder the profiles selected together can move to: "(No Folder)", then every
/// folder.
pub fn move_selection_entries<'a>(folders: &[String]) -> Element<'a, Message> {
    let entries = column![]
        .spacing(0.0)
        .width(MENU_WIDTH)
        .push(entry(
            fl!("ui-sidebar-group-none"),
            Some(AppMessage::Selection(SelectionMessage::Move(None))),
        ))
        .extend(folders.iter().map(|path| {
            entry(
                path.clone(),
                Some(AppMessage::Selection(SelectionMessage::Move(Some(
                    path.clone(),
                )))),
            )
        }));
    menu_card(styles::scroll(entries).height(Length::Shrink))
        .max_height(MOVE_MENU_HEIGHT)
        .into()
}

/// Where folder `path` can move: the top level first, then every folder but itself, those
/// it holds and the one it is in, as the C# "Move to".
#[must_use]
pub fn move_folder_entries<'a>(path: &str, targets: &[String]) -> Element<'a, Message> {
    let entries = column![]
        .spacing(0.0)
        .width(MENU_WIDTH)
        .extend(targets.iter().map(|to| {
            let label = if to.is_empty() {
                fl!("ui-folder-move-top")
            } else {
                to.clone()
            };
            entry(
                label,
                Some(AppMessage::Folder(FolderMessage::Move {
                    path: path.to_owned(),
                    to: to.clone(),
                })),
            )
        }));
    menu_card(entries).into()
}

/// Where the pointer last was in the window, recorded without a message per move.
#[derive(Debug, Clone, Default)]
pub struct CursorSpot(Arc<Mutex<Point>>);

impl CursorSpot {
    /// The last position seen.
    #[must_use]
    pub fn get(&self) -> Point {
        *self.0.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn set(&self, point: Point) {
        *self.0.lock().unwrap_or_else(PoisonError::into_inner) = point;
    }
}

/// Wraps the whole window: records the pointer's position in window coordinates, which a
/// widget deep in a scrolled list does not see.
pub struct CursorTracker<'a> {
    content: Element<'a, Message>,
    spot: CursorSpot,
}

impl<'a> CursorTracker<'a> {
    /// `content`, the pointer recorded in `spot`.
    pub fn new(content: impl Into<Element<'a, Message>>, spot: CursorSpot) -> Self {
        Self {
            content: content.into(),
            spot,
        }
    }
}

impl Widget<Message, Theme, iced::Renderer> for CursorTracker<'_> {
    fn children(&self) -> Vec<Tree> {
        vec![Tree::new(&self.content)]
    }

    fn diff(&self, tree: &mut Tree) {
        tree.diff_children(std::slice::from_ref(&self.content));
    }

    fn size(&self) -> Size<Length> {
        self.content.as_widget().size()
    }

    fn layout(
        &mut self,
        tree: &mut Tree,
        renderer: &iced::Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        self.content
            .as_widget_mut()
            .layout(&mut tree.children[0], renderer, limits)
    }

    fn operate(
        &mut self,
        tree: &mut Tree,
        layout: Layout<'_>,
        renderer: &iced::Renderer,
        operation: &mut dyn Operation,
    ) {
        self.content
            .as_widget_mut()
            .operate(&mut tree.children[0], layout, renderer, operation);
    }

    fn update(
        &mut self,
        tree: &mut Tree,
        event: &Event,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        renderer: &iced::Renderer,
        clipboard: &mut dyn Clipboard,
        shell: &mut Shell<'_, Message>,
        viewport: &Rectangle,
    ) {
        // Before the children: a right click opens a menu where the pointer is now.
        if let Some(position) = cursor.position() {
            self.spot.set(position);
        }
        self.content.as_widget_mut().update(
            &mut tree.children[0],
            event,
            layout,
            cursor,
            renderer,
            clipboard,
            shell,
            viewport,
        );
    }

    fn mouse_interaction(
        &self,
        tree: &Tree,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
        renderer: &iced::Renderer,
    ) -> mouse::Interaction {
        self.content.as_widget().mouse_interaction(
            &tree.children[0],
            layout,
            cursor,
            viewport,
            renderer,
        )
    }

    fn draw(
        &self,
        tree: &Tree,
        renderer: &mut iced::Renderer,
        theme: &Theme,
        style: &renderer::Style,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
    ) {
        self.content.as_widget().draw(
            &tree.children[0],
            renderer,
            theme,
            style,
            layout,
            cursor,
            viewport,
        );
    }

    fn overlay<'b>(
        &'b mut self,
        tree: &'b mut Tree,
        layout: Layout<'b>,
        renderer: &iced::Renderer,
        viewport: &Rectangle,
        translation: Vector,
    ) -> Option<overlay::Element<'b, Message, Theme, iced::Renderer>> {
        self.content.as_widget_mut().overlay(
            &mut tree.children[0],
            layout,
            renderer,
            viewport,
            translation,
        )
    }
}

impl<'a> From<CursorTracker<'a>> for Element<'a, Message> {
    fn from(tracker: CursorTracker<'a>) -> Self {
        Element::new(tracker)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_open_folder_a_row_is_in_draws_its_guide_past_the_folders_expander() {
        assert!(
            guide_offsets(0).is_empty(),
            "a folder at the root is in none"
        );
        assert_eq!(
            guide_offsets(1),
            vec![15.0],
            "12 + 2 + 1, as the C# template"
        );
        assert_eq!(guide_offsets(3), vec![15.0, 31.0, 47.0], "one indent apart");
        // Between the expander of the folder and the items it holds, one indent further in.
        for (offset, items) in guide_offsets(3).into_iter().zip([16.0, 32.0, 48.0]) {
            assert!(
                offset > items - INDENT + EXPANDER_SIDE && offset < items,
                "{offset}"
            );
        }
    }
}
