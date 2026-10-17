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

//! The tabs' strip over the sessions, as the C# one: each tab's icon, pin, title and
//! marks, its close button, and the row that scrolls sideways once the tabs pass its
//! width.

use heimdall_app::{
    BroadcastMessage, Message as AppMessage, SessionState, Tab, TabId, server_text,
};
use iced::widget::{button, center, column, container, mouse_area, row, text, tooltip};
use iced::{Color, Element, Length, Theme};

use super::{
    BroadcastScope, Message, PostConnectProgress, Shell, TAB_GAP, TAB_PADDING, TAB_UNDERLINE,
    TreeShortcut, shows_terminal, sidebar_toggle_id, tab_badges, tab_close_id, tab_id,
    tab_strip_id, transcript_mark,
};
use crate::i18n::fl;
use crate::icons::{self, Icon, Tint};
use crate::styles;
use crate::texts;
use crate::tokens::{font_size, radius, spacing};
use crate::tree_view::{self, TreeMenu};

/// Room around the tab strip, as the C#'s: none below, where the tabs meet the session.
const TAB_STRIP_PADDING: iced::Padding = iced::Padding {
    top: 8.0,
    right: 8.0,
    bottom: 0.0,
    left: 8.0,
};

/// Room around a tab's heading, as the C# tab header's `10,6`.
const TAB_HEADING_PADDING: [f32; 2] = [6.0, 10.0];

/// Side of a tab's close button, as the C#'s 20 by 20.
const TAB_CLOSE_SIDE: f32 = 20.0;

/// Side of the pin before a pinned tab's title, as the C#'s glyph.
const TAB_PIN_SIDE: f32 = 10.0;

/// Width of the outline round the tab a dragged tab would take the place of.
const TAB_DROP_EDGE: f32 = 2.0;

/// Widest a tab's title grows before it ends with an ellipsis, as the C# tab title's
/// `MaxWidth`.
const TAB_TITLE_MAX_WIDTH: f32 = 160.0;

impl Shell {
    /// A tab of the bar: a click shows it, a right click opens its menu, a middle click closes
    /// it; pressed and moved, it is dragged, the tab it would take the place of outlined.
    fn tab_button<'a>(
        &self,
        tab: TabId,
        label: iced::widget::Row<'a, Message>,
        active: bool,
    ) -> Element<'a, Message> {
        let dragging = self.tab_drag.filter(|drag| drag.active);
        // As the C# tab: its heading, and under it the accent's line once selected.
        let content = column![
            container(label.align_y(iced::Alignment::Center)).padding(TAB_HEADING_PADDING),
            container(iced::widget::space())
                .width(Length::Fill)
                .height(TAB_UNDERLINE)
                .style(styles::underline(active)),
        ]
        .width(Length::Shrink);
        let mut area = mouse_area(
            button(content)
                .padding(TAB_PADDING)
                .style(styles::tab(active))
                .on_press(Message::App(AppMessage::SelectTab(tab))),
        )
        .on_right_press(Message::OpenTreeMenu(TreeMenu::Tab(tab)))
        // A middle click closes it, as the C# `OnTabMiddleClick`: through the close
        // button's request, so a live session still asks first.
        .on_middle_press(Message::App(AppMessage::RequestCloseTab(tab)))
        .on_enter(Message::TabHover(tab))
        .on_exit(Message::TabHoverLeft(tab));
        if dragging.is_some() {
            area = area.interaction(iced::mouse::Interaction::Grabbing);
        }
        // Where a dragged tab goes, outlined as the C# drop target.
        let target = dragging.and_then(|drag| drag.onto(self.tab_hover)) == Some(tab);
        container(area)
            .id(tab_id(tab))
            .style(move |theme: &Theme| container::Style {
                border: iced::Border {
                    color: if target {
                        theme.extended_palette().primary.strong.color
                    } else {
                        Color::TRANSPARENT
                    },
                    width: TAB_DROP_EDGE,
                    radius: radius::SM.into(),
                },
                ..container::Style::default()
            })
            .into()
    }

    /// The gateways `tab` goes through, as the C# says them: "via" and their names; `None`
    /// when it goes straight.
    pub(super) fn route_text(&self, tab: &Tab) -> Option<String> {
        let route = self.app.tab_route(tab);
        if route.is_empty() {
            return None;
        }
        let names = route
            .iter()
            .map(|name| server_text(name))
            .collect::<Vec<_>>()
            .join(&fl!("ui-route-test-separator"));
        Some(fl!("ui-connect-via", route = names))
    }

    /// "via" when `tab` goes through gateways, naming them when pointed at; nothing when it
    /// goes straight.
    /// Its colour says the route's health, as the C# tab's tunnel badge: the success colour
    /// while the session runs, the warning colour once it failed, ended or waits to open
    /// again, said in its tooltip above the route.
    fn route_badge(&self, tab: &Tab) -> Option<Element<'static, Message>> {
        let route = self.route_text(tab)?;
        let (health, colour): (String, fn(&Theme) -> text::Style) = match self.app.tunnel_badge(tab)
        {
            heimdall_app::tunnel::TunnelBadge::Hidden => return None,
            heimdall_app::tunnel::TunnelBadge::Healthy => {
                (fl!("ui-tab-route-badge-healthy"), text::success)
            }
            heimdall_app::tunnel::TunnelBadge::Unhealthy => {
                (fl!("ui-tab-route-badge-unhealthy"), text::warning)
            }
        };
        Some(
            tooltip(
                text(fl!("ui-tab-route-badge"))
                    .size(font_size::CAPTION)
                    .style(colour),
                column![
                    text(health).size(font_size::CAPTION),
                    text(route).size(font_size::CAPTION),
                ],
                tooltip::Position::Bottom,
            )
            .padding(styles::TOOLTIP_PADDING)
            .style(styles::tooltip)
            .into(),
        )
    }

    /// The title of `tab`, as its tab and its pane's header name it: a Files tab says so,
    /// and a desktop "Connect with" opened embedded this once, as the C# title suffix, which
    /// a name the user gave replaces (`SessionTabViewModel.cs:255-269`). Remote Desktop
    /// Connection opens no tab to name.
    pub(super) fn tab_name(tab: &Tab) -> String {
        let name = if tab.files.is_some() && tab.custom_title.is_none() {
            fl!("ui-tab-files-title", name = tab.display_title())
        } else {
            tab.display_title().to_owned()
        };
        if tab.custom_title.is_none()
            && tab.rdp_mode_override == Some(heimdall_core::profile::RdpMode::Embedded)
        {
            fl!("ui-tab-rdp-forced-embedded-title", name = name)
        } else {
            name
        }
    }

    /// What a tab says of itself, as the C# tab: its protocol's icon, its pin, its title
    /// and a dot while it connects, then its marks.
    fn tab_title(&self, tab: &Tab, active: bool) -> iced::widget::Row<'static, Message> {
        let kind = self.app.tab_kind(tab);
        // A tool's tab bears the tool's icon, in its category's colour.
        let (glyph, tint) = match tab.tool() {
            Some(tool) => (crate::tools::icon(tool), Tint::Tool(tool.category())),
            None => (Icon::of(kind), Tint::Protocol(kind)),
        };
        let mut label = row![icons::icon(glyph, tint, icons::GLYPH_SIDE)]
            .spacing(spacing::XS)
            .align_y(iced::Alignment::Center);
        if tab.pinned {
            label = label.push(
                tooltip(
                    icons::icon(Icon::Pin, Tint::Accent, TAB_PIN_SIDE),
                    text(fl!("ui-tab-pinned-badge")).size(font_size::CAPTION),
                    tooltip::Position::Bottom,
                )
                .padding(styles::TOOLTIP_PADDING)
                .style(styles::tooltip),
            );
        }
        // One long server title cannot take the whole strip: it ends with an ellipsis.
        label = label.push(
            crate::ellipsis::text(Self::tab_name(tab))
                .size(font_size::BODY)
                .font(styles::SEMIBOLD)
                .style(text::base)
                .max_width(TAB_TITLE_MAX_WIDTH),
        );
        // While it connects, as the C# tab's busy dot.
        let state = SessionState::of(tab);
        if matches!(state, SessionState::Connecting | SessionState::Reconnecting) {
            label = label.push(tree_view::state_dot(Some(state)));
        }
        // Through gateways, said on the tab while the tunnels panel that lists it is
        // closed, as the C# tab's tunnel badge.
        if !self.app.tunnels_panel() {
            label = label.push(self.route_badge(tab));
        }
        for badge in tab_badges(tab, active) {
            label = label.push(badge);
        }
        label
    }

    /// What goes before a tab, on the strip or in a pane's header: its broadcast marker
    /// while broadcasting to the tabs chosen, as the C# tab's, and its post-connect steps.
    pub(super) fn tab_marks<'a>(&'a self, tab: &'a Tab) -> Vec<Element<'a, Message>> {
        let mut marks = Vec::new();
        let marked = self.app.broadcasting()
            && self.app.settings().broadcast_scope == BroadcastScope::SelectedTabs
            && shows_terminal(tab);
        if marked {
            // A target of broadcast input, or not, as the C# tab's marker.
            let target = self.app.is_broadcast_target(tab.id);
            marks.push(
                tooltip(
                    button(
                        text(if target {
                            fl!("ui-broadcast-target-on")
                        } else {
                            fl!("ui-broadcast-target-off")
                        })
                        .size(font_size::CAPTION),
                    )
                    .style(styles::subtle)
                    .on_press(Message::App(AppMessage::Broadcast(
                        BroadcastMessage::Target(tab.id),
                    ))),
                    text(fl!("ui-broadcast-target-tooltip")).size(font_size::CAPTION),
                    tooltip::Position::Bottom,
                )
                .padding(styles::TOOLTIP_PADDING)
                .style(styles::tooltip)
                .into(),
            );
        }
        if let Some(progress) = &tab.post_connect {
            marks.push(post_connect_badge(tab.id, progress));
        }
        marks
    }

    /// What a tab says of itself on the strip: its title and marks, the transcript it keeps
    /// among them.
    fn tab_heading(&self, tab: &Tab, active: bool) -> iced::widget::Row<'static, Message> {
        self.tab_title(tab, active).push(transcript_mark(tab))
    }

    /// The tabs' strip, as the C# one: on one row that scrolls sideways once the tabs pass
    /// its width, the tab shown kept in view.
    pub(super) fn tab_bar(&self) -> Element<'_, Message> {
        let mut strip = row![]
            .spacing(TAB_GAP)
            .padding(TAB_STRIP_PADDING)
            .align_y(iced::Alignment::Center);
        let mut tabs = row![].spacing(TAB_GAP).align_y(iced::Alignment::Center);
        // The sidebar hidden, a way to show it again, as the C# button where it was.
        if self.sidebar_hidden {
            strip = strip.push(
                tooltip(
                    container(
                        icons::button(Icon::ChevronRight)
                            .style(styles::secondary)
                            .on_press(Message::TreeShortcut(TreeShortcut::ToggleSidebar)),
                    )
                    .id(sidebar_toggle_id()),
                    text(fl!("ui-sidebar-show-tooltip")).size(font_size::CAPTION),
                    tooltip::Position::Bottom,
                )
                .padding(styles::TOOLTIP_PADDING)
                .style(styles::tooltip),
            );
        }
        let shown = self.shown_tab_id();
        for tab in self.app.strip() {
            let active = shown == Some(tab.id);
            for mark in self.tab_marks(tab) {
                tabs = tabs.push(mark);
            }
            // The close button inside the tab, at its right, as the C# tab's: a circled cross,
            // faded but on the tab selected or pointed at. It takes the press, which selecting
            // the tab then does not see.
            let quiet = !active && self.tab_hover != Some(tab.id);
            // Named as the C# button's accessible name, in a tooltip.
            let close = container(
                tooltip(
                    button(center(icons::faded(
                        Icon::Close,
                        Tint::Text,
                        font_size::SMALL_CAPTION,
                        if quiet {
                            styles::CLOSE_REST_OPACITY
                        } else {
                            1.0
                        },
                    )))
                    .width(TAB_CLOSE_SIDE)
                    .height(TAB_CLOSE_SIDE)
                    .padding(0.0)
                    .style(styles::close_mark(quiet))
                    .on_press(Message::App(AppMessage::RequestCloseTab(tab.id))),
                    text(fl!("ui-session-close-button")).size(font_size::CAPTION),
                    tooltip::Position::Bottom,
                )
                .padding(styles::TOOLTIP_PADDING)
                .style(styles::tooltip),
            )
            .id(tab_close_id(tab.id));
            let label = self.tab_heading(tab, active).push(close);
            tabs = tabs.push(self.tab_button(tab.id, label, active));
        }
        strip
            .push(crate::tab_scroller::tab_scroller(
                tabs,
                tab_strip_id(),
                shown.map(tab_id),
            ))
            .into()
    }
}

/// The count of a tab's running post-connect steps, as the C# tab shows it; its tooltip says
/// which step and what became of it, a click stops the rest.
fn post_connect_badge(tab: TabId, progress: &PostConnectProgress) -> Element<'_, Message> {
    let count = format!("{}/{}", progress.step, progress.total);
    let status = texts::step_status(progress.status);
    tooltip(
        button(text(count.clone()).size(font_size::CAPTION))
            .style(styles::subtle)
            .on_press(Message::App(AppMessage::StopPostConnect(tab))),
        text(fl!(
            "ui-post-connect-tooltip",
            progress = count,
            status = status,
            command = progress.command.as_str()
        ))
        .size(font_size::CAPTION),
        tooltip::Position::Bottom,
    )
    .padding(styles::TOOLTIP_PADDING)
    .style(styles::tooltip)
    .into()
}
