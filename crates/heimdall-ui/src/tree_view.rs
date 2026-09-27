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

use heimdall_app::{GatewayBadge, Message as AppMessage, ProfileCopy, ProfileKind, ProfileSummary};
use heimdall_core::profile::ProfileId;
use iced::advanced::layout::{self, Layout};
use iced::advanced::widget::{Operation, Tree, Widget};
use iced::advanced::{Clipboard, Shell, mouse, overlay, renderer};
use iced::widget::{button, column, container, mouse_area, row, rule, text, tooltip};
use iced::{Element, Event, Length, Point, Rectangle, Size, Theme, Vector};

use crate::i18n::fl;
use crate::shell::Message;

/// Size of a menu entry's text.
const MENU_TEXT_SIZE: f32 = 14.0;

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
}

/// The protocol's name as the tree shows it.
#[must_use]
pub fn protocol_label(kind: ProfileKind) -> &'static str {
    match kind {
        ProfileKind::Ssh => "SSH",
        ProfileKind::Rdp => "RDP",
        ProfileKind::Telnet => "Telnet",
        ProfileKind::Vnc => "VNC",
        ProfileKind::Local => "Local",
        ProfileKind::WinRm => "WinRM",
    }
}

/// One profile: protocol and name; the host, account and protocol in its tooltip.
pub fn owned_row(profile: &ProfileSummary, selected: bool) -> Element<'static, Message> {
    let id = profile.id.clone();
    let mut label = row![
        text(protocol_label(profile.kind))
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
        protocol = protocol_label(profile.kind)
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
    editable: bool,
    can_import: bool,
) -> Element<'a, Message> {
    let mut entries = column![].spacing(0.0).width(MENU_WIDTH);
    match (menu, profile) {
        (TreeMenu::Profile(_), Some(profile)) => {
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
            if profile.kind == ProfileKind::Ssh {
                entries = entries.push(
                    button(text(fl!("ui-tree-connect-as")).size(MENU_TEXT_SIZE))
                        .width(Length::Fill)
                        .style(menu_style)
                        .on_press(Message::OpenTreeMenu(TreeMenu::ConnectAs(id.clone()))),
                );
            }
            entries = entries
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
                ));
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
        }
        (TreeMenu::ConnectAs(_), Some(profile)) => {
            // SFTP opens the Files view of an SSH profile.
            entries = entries.push(entry(
                "SFTP".to_owned(),
                (profile.kind == ProfileKind::Ssh)
                    .then(|| AppMessage::OpenFiles(profile.id.clone())),
            ));
        }
        (TreeMenu::Add, _) => {
            entries = entries
                .push(entry(
                    fl!("ui-tree-add-session"),
                    Some(AppMessage::NewProfile),
                ))
                .push(separator())
                .push(entry(fl!("ui-gateway-add"), Some(AppMessage::NewGateway)));
        }
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
