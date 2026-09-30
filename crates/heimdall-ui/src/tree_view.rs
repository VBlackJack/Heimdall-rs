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

use heimdall_app::files::{Direction, Side};
use heimdall_app::{
    ConnectAs, FilesMessage, FilterMessage, FolderMessage, GatewayBadge, HostKeysMessage,
    Message as AppMessage, NO_FOLDER, ProfileCopy, ProfileKind, ProfileMenuMessage, ProfileSummary,
    RdpMessage, SelectionMessage, SessionState, SessionsMessage, TabGroup, TabId, TabMenuMessage,
    TreeFilter,
};
use heimdall_core::profile::ProfileId;
use iced::advanced::layout::{self, Layout};
use iced::advanced::widget::{Operation, Tree, Widget};
use iced::advanced::{Clipboard, Shell, mouse, overlay, renderer};
use iced::widget::{
    Column, Container, button, checkbox, column, container, mouse_area, row, rule, scrollable,
    text, tooltip,
};
use iced::{Element, Event, Length, Point, Rectangle, Size, Theme, Vector};

use crate::i18n::fl;
use crate::shell::Message;

/// Size of a menu entry's text.
const MENU_TEXT_SIZE: f32 = 14.0;

/// Tallest a "Move to folder" list grows before it scrolls.
const MOVE_MENU_HEIGHT: f32 = 360.0;

/// Size of a folder's name.
const FOLDER_SIZE: f32 = 13.0;

/// What an open folder shows before its name, and a closed one.
const OPEN_MARKER: &str = "\u{25BE}";
const CLOSED_MARKER: &str = "\u{25B8}";

/// Width of a menu.
const MENU_WIDTH: f32 = 270.0;

/// Space between a menu's card and its entries.
const MENU_PADDING: f32 = 4.0;

/// Size of the protocol label before a profile's name.
const PROTOCOL_SIZE: f32 = 11.0;

/// Width of the accent edge of a selected row.
const SELECTED_EDGE: f32 = 3.0;

/// A menu open in the tree.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TreeMenu {
    /// A profile's menu.
    Profile(ProfileId),
    /// The "Connect as..." entries of a profile.
    ConnectAs(ProfileId),
    /// The menu of the tree's empty area and of the "+" button.
    Add,
    /// The "..." button's menu.
    More,
    /// A tab's menu, drawn as the tree's are.
    Tab(TabId),
    /// A folder's menu, [`NO_FOLDER`] included.
    Folder(String),
    /// Where a folder can move to.
    MoveFolder(String),
    /// Which folder a profile can move to.
    MoveProfile(ProfileId),
    /// The tree's filters, as the C# filter button's menu.
    Filter,
    /// The menu of the profiles selected together.
    Selection,
    /// Which folder the profiles selected together can move to.
    MoveSelection,
    /// The server's folders bookmarked in a Files tab.
    FilesBookmarks(TabId),
    /// The menu of an entry of a Files tab's pane, as the C# Files tab's.
    FilesEntry {
        /// The tab.
        tab: TabId,
        /// The pane.
        side: Side,
        /// The entry.
        index: usize,
    },
}

/// Width of the protocol column: every name starts at the same place, as beside the C#
/// tree's icons.
const PROTOCOL_WIDTH: f32 = 38.0;

/// Widest a gateway badge grows beside a name.
const BADGE_MAX_WIDTH: f32 = 110.0;

/// Size of a session's state dot, as the C# tree's and tabs'.
const DOT_SIZE: f32 = 7.0;

/// A session's state as the C# Heimdall's dot shows it: green connected, amber on its way,
/// red failed, grey when none is open.
#[must_use]
pub fn state_dot<'a>(state: Option<SessionState>) -> Element<'a, Message> {
    container(iced::widget::space())
        .width(DOT_SIZE)
        .height(DOT_SIZE)
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
                    radius: (DOT_SIZE / 2.0).into(),
                    ..iced::Border::default()
                },
                ..container::Style::default()
            }
        })
        .into()
}

/// How far a row moves right for each folder it is in.
const INDENT: f32 = 14.0;

/// `row` moved right for `depth` folders.
#[must_use]
pub fn indented(row: Element<'_, Message>, depth: usize) -> Element<'_, Message> {
    #[allow(
        clippy::cast_precision_loss,
        reason = "a folder depth, far below the 2^24 where f32 loses units"
    )]
    let left = depth as f32 * INDENT;
    container(row)
        .padding(iced::Padding {
            left,
            ..iced::Padding::ZERO
        })
        .into()
}

/// A folder: open or closed at a click, as the C# tree's; "(No Folder)" for [`NO_FOLDER`].
/// The profiles it holds counted at its right, as the C# tree's.
pub fn folder_row<'a>(
    path: String,
    name: String,
    depth: usize,
    open: bool,
    count: usize,
) -> Element<'a, Message> {
    let label = if path == NO_FOLDER {
        fl!("ui-sidebar-group-none")
    } else {
        name
    };
    let marker = if open { OPEN_MARKER } else { CLOSED_MARKER };
    let body = container(
        row![
            text(marker).size(PROTOCOL_SIZE).style(text::secondary),
            text(label).size(FOLDER_SIZE),
            iced::widget::space::horizontal(),
            text(count.to_string())
                .size(PROTOCOL_SIZE)
                .style(text::secondary),
        ]
        .spacing(6.0)
        .align_y(iced::Alignment::Center),
    )
    .width(Length::Fill)
    .padding([2.0, 4.0]);
    indented(
        mouse_area(body)
            .on_press(Message::App(AppMessage::ToggleFolder(path.clone())))
            .on_right_press(Message::OpenTreeMenu(TreeMenu::Folder(path)))
            .interaction(mouse::Interaction::Pointer)
            .into(),
        depth,
    )
}

/// Size of the line under a found profile's name.
const CONTEXT_SIZE: f32 = 11.0;

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

/// One profile: protocol, the state of its sessions and name, as the C# tree's row, with
/// where it is under the name while searching; the host, account and protocol in its
/// tooltip.
pub fn owned_row(
    profile: &ProfileSummary,
    selected: bool,
    state: Option<SessionState>,
    context: Option<String>,
) -> Element<'static, Message> {
    let id = profile.id.clone();
    let mut label = row![
        container(
            text(profile.kind.label())
                .size(PROTOCOL_SIZE)
                .style(text::secondary)
        )
        .width(PROTOCOL_WIDTH),
        state_dot(state),
        column![text(profile.name.clone()).wrapping(text::Wrapping::Glyph)].push(context.map(
            |context| {
                text(context)
                    .size(CONTEXT_SIZE)
                    .style(text::secondary)
                    .wrapping(text::Wrapping::Glyph)
            }
        )),
    ]
    .spacing(6.0)
    .align_y(iced::Alignment::Center);
    if let Some(badge) = &profile.gateway {
        label = label.push(gateway_badge(badge));
    }
    let body = container(label)
        .width(Length::Fill)
        .padding([4.0, 8.0])
        .style(move |theme: &Theme| row_style(theme, selected));
    let area = mouse_area(body)
        .on_press(Message::TreeClick(id.clone()))
        .on_double_click(Message::App(AppMessage::ConnectProfile(id.clone())))
        .on_right_press(Message::OpenTreeMenu(TreeMenu::Profile(id)))
        .interaction(mouse::Interaction::Pointer);
    tooltip(
        area,
        text(row_tooltip(profile)).size(12.0),
        tooltip::Position::Right,
    )
    .style(container::rounded_box)
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
            .size(PROTOCOL_SIZE)
            .wrapping(text::Wrapping::None),
    )
    .max_width(BADGE_MAX_WIDTH)
    .clip(true)
    .padding([0.0, 4.0])
    .style(move |theme: &Theme| {
        let palette = theme.extended_palette();
        let colour = if missing {
            palette.danger.base.color
        } else {
            palette.primary.base.color
        };
        container::Style {
            text_color: Some(colour),
            border: iced::Border {
                color: colour,
                width: 1.0,
                radius: 6.0.into(),
            },
            ..container::Style::default()
        }
    })
    .into()
}

fn row_style(theme: &Theme, selected: bool) -> container::Style {
    let palette = theme.extended_palette();
    if selected {
        container::Style {
            background: Some(palette.background.strong.color.into()),
            border: iced::Border {
                color: palette.primary.strong.color,
                width: SELECTED_EDGE,
                radius: 2.0.into(),
            },
            ..container::Style::default()
        }
    } else {
        container::Style::default()
    }
}

fn row_tooltip(profile: &ProfileSummary) -> String {
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
            radius: 2.0.into(),
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
    button(text(label).size(MENU_TEXT_SIZE))
        .width(Length::Fill)
        .style(menu_style)
        .on_press_maybe(message.map(Message::MenuChoice))
        .into()
}

/// The card every menu is drawn on, so what is under it never shows through.
fn menu_card<'a>(entries: impl Into<Element<'a, Message>>) -> Container<'a, Message> {
    container(entries)
        .padding(MENU_PADDING)
        .style(container::rounded_box)
}

fn separator<'a>() -> Element<'a, Message> {
    rule::horizontal(1).into()
}

/// The entries of `menu`, as the C# Heimdall orders them, limited to what this version does.
pub fn menu_entries<'a>(
    menu: &TreeMenu,
    profile: Option<&ProfileSummary>,
    connect_as: &[ConnectAs],
    editable: bool,
    can_import: bool,
) -> Element<'a, Message> {
    let mut entries = column![].spacing(0.0).width(MENU_WIDTH);
    match (menu, profile) {
        (TreeMenu::Profile(_), Some(profile)) => {
            entries = profile_entries(entries, profile, connect_as, editable);
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
                    can_import.then_some(AppMessage::ImportLegacy),
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
                    fl!("ui-tree-import-known-hosts"),
                    Some(crate::hostkeys_view::app_message(HostKeysMessage::Start)),
                ))
                .push(entry(
                    fl!("ui-tree-export-sessions"),
                    Some(AppMessage::ExportSessions),
                ));
        }
        _ => {}
    }
    menu_card(entries).into()
}

/// The tree's filters, as the C# filter button's menu: the protocols, then connected and
/// through a gateway, then the gateway badge. A box ticked leaves the menu open, as the C#
/// entries stay open, so several can be chosen.
pub fn filter_entries<'a>(filter: &TreeFilter) -> Element<'a, Message> {
    let filter_box = |label: String, on: bool, message: FilterMessage| -> Element<'a, Message> {
        container(
            checkbox(on)
                .label(label)
                .text_size(MENU_TEXT_SIZE)
                .on_toggle(move |_| Message::App(AppMessage::Filter(message))),
        )
        .padding(MENU_PADDING)
        .into()
    };
    let mut entries = column![
        container(text(fl!("ui-tree-filter-protocols")).size(MENU_TEXT_SIZE)).padding(MENU_PADDING)
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
        .push(separator())
        .push(filter_box(
            fl!("ui-tree-filter-gateway-badge"),
            filter.shows_gateway_badge(),
            FilterMessage::GatewayBadge,
        ));
    menu_card(entries).into()
}

/// A profile's menu, in the C# order this version has: Connect, Connect as, Rename, Edit,
/// Duplicate, Move to folder, the copies, Delete.
fn profile_entries<'a>(
    mut entries: Column<'a, Message>,
    profile: &ProfileSummary,
    connect_as: &[ConnectAs],
    editable: bool,
) -> Column<'a, Message> {
    let id = profile.id.clone();
    let copy = |what| {
        Some(AppMessage::CopyProfile {
            id: id.clone(),
            what,
        })
    };
    entries = entries.push(entry(
        fl!("ui-tree-connect"),
        Some(AppMessage::ConnectProfile(id.clone())),
    ));
    if !connect_as.is_empty() {
        entries = entries.push(
            button(text(fl!("ui-tree-connect-as")).size(MENU_TEXT_SIZE))
                .width(Length::Fill)
                .style(menu_style)
                .on_press(Message::OpenTreeMenu(TreeMenu::ConnectAs(id.clone()))),
        );
    }
    entries = entries
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
        .push(separator())
        .push(
            button(text(fl!("ui-tree-move-to-folder")).size(MENU_TEXT_SIZE))
                .width(Length::Fill)
                .style(menu_style)
                .on_press_maybe(
                    editable.then(|| Message::OpenTreeMenu(TreeMenu::MoveProfile(id.clone()))),
                ),
        );
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
    }
    entries = entries.push(separator()).push(
        button(text(fl!("ui-tree-delete")).size(MENU_TEXT_SIZE))
            .width(Length::Fill)
            .style(danger_style)
            .on_press(Message::MenuChoice(AppMessage::RequestDeleteProfile(id))),
    );
    entries
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

/// The menu of an entry of a Files tab's pane, in the C# order, limited to what this
/// version does: open it, send it to the other pane, rename, delete, copy its path; then
/// what applies to the folder shown.
pub fn files_entry_menu<'a>(tab: TabId, side: Side, index: usize) -> Element<'a, Message> {
    let files = |message| Some(AppMessage::Files(message));
    let server = |entry: Element<'a, Message>| (side == Side::Remote).then_some(entry);
    let (send, direction) = match side {
        Side::Remote => (fl!("ui-files-menu-download"), Direction::Download),
        Side::Local => (fl!("ui-files-menu-upload"), Direction::Upload),
    };
    let entries = column![
        entry(
            fl!("ui-files-menu-open"),
            files(FilesMessage::Open { tab, side, index })
        ),
        entry(send, files(FilesMessage::Transfer { tab, direction })),
        separator(),
        entry(
            fl!("ui-files-menu-rename"),
            files(FilesMessage::AskRename { tab, side })
        ),
        entry(
            fl!("ui-files-menu-delete"),
            files(FilesMessage::AskDelete { tab, side })
        ),
        // The server's entries only, as in the C# Files tab.
        server(entry(
            fl!("ui-files-menu-permissions"),
            files(FilesMessage::AskPermissions { tab, side })
        )),
        separator(),
        entry(
            fl!("ui-files-menu-copy-path"),
            files(FilesMessage::CopyPath { tab, side })
        ),
        server(entry(
            fl!("ui-files-menu-properties"),
            files(FilesMessage::ShowProperties { tab, side })
        )),
        separator(),
        entry(
            fl!("ui-files-menu-new-folder"),
            files(FilesMessage::AskNewFolder { tab, side })
        ),
        entry(
            fl!("ui-files-menu-refresh"),
            files(FilesMessage::Refresh { tab, side })
        ),
    ]
    .spacing(0.0)
    .width(MENU_WIDTH);
    menu_card(entries).into()
}

/// The server's folders bookmarked in a Files tab, `bookmarks` as shown, each going
/// there; a line saying there are none, as the C# menu.
pub fn files_bookmarks_menu<'a>(tab: TabId, bookmarks: &[String]) -> Element<'a, Message> {
    let mut entries = column![].spacing(0.0).width(MENU_WIDTH);
    if bookmarks.is_empty() {
        entries = entries.push(entry(fl!("ui-files-bookmarks-empty"), None));
    }
    for (index, path) in bookmarks.iter().enumerate() {
        entries = entries.push(entry(
            path.clone(),
            Some(AppMessage::Files(FilesMessage::OpenBookmark { tab, index })),
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
}

/// The entries of a tab's menu, in the C# Heimdall's order, limited to what this version
/// does: no pin, split, detach or macros.
pub fn tab_menu_entries<'a>(state: &TabMenuState) -> Element<'a, Message> {
    let tab = state.tab;
    let menu = |message| Some(AppMessage::TabMenu(message));
    let mut entries = column![]
        .spacing(0.0)
        .width(MENU_WIDTH)
        .push(entry(
            fl!("ui-tab-menu-disconnect"),
            Some(AppMessage::RequestCloseTab(tab)),
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
    entries = entries
        .push(separator())
        .push(
            button(text(fl!("ui-tab-menu-fullscreen")).size(MENU_TEXT_SIZE))
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
    if let Some(profile) = &state.profile {
        let id = profile.id.clone();
        let has_user = profile
            .username
            .as_deref()
            .is_some_and(|user| !user.is_empty());
        entries = entries
            .push(separator())
            .push(entry(
                fl!("ui-tree-edit"),
                state.editable.then(|| AppMessage::EditProfile(id.clone())),
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
            menu(TabMenuMessage::StopTranscript(tab)),
        )),
    };
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
            .push(
                button(text(fl!("ui-folder-move-to")).size(MENU_TEXT_SIZE))
                    .width(Length::Fill)
                    .style(menu_style)
                    .on_press(Message::OpenTreeMenu(TreeMenu::MoveFolder(path.to_owned()))),
            )
            .push(
                button(text(fl!("ui-folder-delete")).size(MENU_TEXT_SIZE))
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
    menu_card(scrollable(entries).height(Length::Shrink))
        .max_height(MOVE_MENU_HEIGHT)
        .into()
}

/// The menu of `count` profiles selected together, as the C# bulk menu: how many, Connect
/// selected, Duplicate selected, Move to folder, Delete selected.
pub fn selection_menu_entries<'a>(count: usize, connectable: usize) -> Element<'a, Message> {
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
        .push(separator())
        .push(
            button(text(fl!("ui-tree-move-to-folder")).size(MENU_TEXT_SIZE))
                .width(Length::Fill)
                .style(menu_style)
                .on_press(Message::OpenTreeMenu(TreeMenu::MoveSelection)),
        )
        .push(separator())
        .push(
            button(text(fl!("ui-selection-delete", count = count)).size(MENU_TEXT_SIZE))
                .width(Length::Fill)
                .style(danger_style)
                .on_press(Message::MenuChoice(AppMessage::Selection(
                    SelectionMessage::RequestDelete,
                ))),
        );
    menu_card(entries).into()
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
    menu_card(scrollable(entries).height(Length::Shrink))
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
