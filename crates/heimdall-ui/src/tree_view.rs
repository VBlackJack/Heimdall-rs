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

use heimdall_app::{
    ConnectAs, FolderMessage, GatewayBadge, Message as AppMessage, NO_FOLDER, ProfileCopy,
    ProfileKind, ProfileMenuMessage, ProfileSummary, TabGroup, TabId, TabMenuMessage,
};
use heimdall_core::profile::ProfileId;
use iced::advanced::layout::{self, Layout};
use iced::advanced::widget::{Operation, Tree, Widget};
use iced::advanced::{Clipboard, Shell, mouse, overlay, renderer};
use iced::widget::{
    Column, button, column, container, mouse_area, row, rule, scrollable, text, tooltip,
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
const MENU_WIDTH: f32 = 230.0;

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
pub fn folder_row<'a>(
    path: String,
    name: String,
    depth: usize,
    open: bool,
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

/// One profile: protocol and name; the host, account and protocol in its tooltip.
pub fn owned_row(profile: &ProfileSummary, selected: bool) -> Element<'static, Message> {
    let id = profile.id.clone();
    let mut label = row![
        text(profile.kind.label())
            .size(PROTOCOL_SIZE)
            .style(text::secondary),
        text(profile.name.clone()).wrapping(text::Wrapping::Glyph),
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
        .on_press(Message::App(AppMessage::SelectProfile(id.clone())))
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
    container(text(label).size(PROTOCOL_SIZE))
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
            entries = entries.push(entry(
                fl!("ui-tree-import-sessions"),
                can_import.then_some(AppMessage::ImportLegacy),
            ));
        }
        _ => {}
    }
    container(entries)
        .padding(4.0)
        .style(container::rounded_box)
        .into()
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
}

/// The entries of a tab's menu, in the C# Heimdall's order, limited to what this version
/// does: no pin, split, detach, transcript or macros.
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
    container(entries)
        .padding(4.0)
        .style(container::rounded_box)
        .into()
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
    container(entries)
        .padding(4.0)
        .style(container::rounded_box)
        .into()
}

/// Which folder profile `id` can move to: "(No Folder)" first, then every folder, its own
/// greyed, as the C# "Move to folder".
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
    container(scrollable(entries).height(Length::Shrink))
        .max_height(MOVE_MENU_HEIGHT)
        .padding(4.0)
        .style(container::rounded_box)
        .into()
}

/// Where folder `path` can move: the top level first, then every folder but itself, those
/// it holds and the one it is in, as the C# "Move to".
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
    container(entries)
        .padding(4.0)
        .style(container::rounded_box)
        .into()
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
