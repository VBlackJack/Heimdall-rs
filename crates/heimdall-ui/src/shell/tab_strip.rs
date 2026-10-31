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

use std::time::{Duration, Instant};

use heimdall_app::{
    BroadcastMessage, Message as AppMessage, Phase, ProfileSummary, SessionState, Tab, TabId,
    server_text,
};
use iced::advanced::layout::{self, Layout};
use iced::advanced::widget::{Tree, tree};
use iced::advanced::{Clipboard, Shell as WidgetShell, Widget, renderer};
use iced::widget::{button, center, column, container, mouse_area, row, text, tooltip};
use iced::{Color, Element, Event, Length, Rectangle, Size, Theme, mouse, window};

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
use crate::tree_view::{self, TabListItem, TreeMenu};
use heimdall_app::{TabMenuMessage, folder_label};

/// Room around the tab strip, as the C#'s: none below, where the tabs meet the session.
const TAB_STRIP_PADDING: iced::Padding = iced::Padding {
    top: 8.0,
    right: 8.0,
    bottom: 0.0,
    left: 8.0,
};

/// Room around a tab's heading, as the C# tab header's `10,6`.
const TAB_HEADING_PADDING: [f32; 2] = [6.0, 10.0];

/// Side of a tab's close button, as the C#'s 24 by 24 (`MainWindow.xaml:1722`): the tallest
/// thing in a tab's heading, so it sets the tab's height (1 + 8 + 6 + 24 + 6 + 2 + 8 = 55
/// in the C#, the 1 and the 2 being its border and its group bar, which the underline of 3
/// stands for here).
const TAB_CLOSE_SIDE: f32 = 24.0;

/// Side of the pin before a pinned tab's title, as the C#'s glyph.
const TAB_PIN_SIDE: f32 = 10.0;

/// Width of the outline round the tab a dragged tab would take the place of.
const TAB_DROP_EDGE: f32 = 2.0;

/// Widest a tab's title grows before it ends with an ellipsis, as the C# tab title's
/// `MaxWidth`.
const TAB_TITLE_MAX_WIDTH: f32 = 160.0;

/// Side of the mark after the title of a tab that needs a look, as the C#
/// `SessionTabStateMarkSize`.
const TAB_MARK_SIDE: f32 = 8.0;

/// Width of the ring of a dropped tab, as the C# `SessionTabStateRingThickness`.
const TAB_RING_WIDTH: f32 = 1.5;

/// Side of the warning of a failed tab, as the C# `SessionTabStateGlyphFontSize`.
const TAB_WARNING_SIDE: f32 = 11.0;

/// How long the pointer rests on a tab's title before its tooltip shows, as the WPF
/// tooltip's default `InitialShowDelay` the C# tab keeps.
const TAB_TOOLTIP_DELAY: Duration = Duration::from_millis(400);

/// How long the connecting dot takes to fade, then as long to come back, as the C#
/// `TabStatePulse` storyboard's `Duration="0:0:0.6"` with `AutoReverse`.
const PULSE_HALF: Duration = Duration::from_millis(600);

/// The connecting dot's opacity at the faintest of its pulse, as the C# storyboard's `To`.
const PULSE_FAINTEST: f32 = 0.2;

/// How often the pulsing dot is drawn anew: smooth to the eye without drawing every frame.
const PULSE_FRAME: Duration = Duration::from_millis(40);

/// Room the button listing every tab takes at the strip's end: its side and the gap before
/// it, as the C# `SessionTabListButtonWidth` and the 2 it adds (`MainWindow.TabStrip.cs:113`).
const TAB_LIST_RESERVE: f32 = icons::BUTTON_SIDE + TAB_GAP;

/// Room round the chip naming a folder, as the C# group chip's label.
const FOLDER_CHIP_PADDING: [f32; 2] = [2.0, spacing::SM];

/// The room between a folder chip's chevron and its name.
const FOLDER_CHIP_GAP: f32 = spacing::XS;

/// The mark after the title of a tab whose session needs a look, as the C# tab's
/// `TabStateMark`: each state its own shape, so the colour is never the only cue.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum StateMark {
    /// Connecting or waiting to connect again: a pulsing dot.
    Pulse,
    /// Ended, the tab left open: a ring.
    Ring,
    /// Failed: a warning.
    Warning,
}

/// The mark of a session in `state`, as the C# `SessionTabHeaderStates.From`
/// (`SessionTabHeaderState.cs:52`): none for a connected one, so thirty healthy tabs stay
/// quiet and the one that dropped stands out.
fn state_mark(state: SessionState) -> Option<StateMark> {
    match state {
        SessionState::Connected => None,
        SessionState::Connecting | SessionState::Reconnecting => Some(StateMark::Pulse),
        SessionState::Ended => Some(StateMark::Ring),
        SessionState::Failed => Some(StateMark::Warning),
    }
}

/// The state of a session in words, for its tab's tooltip, `None` for a connected one: a
/// failure says its `reason`, as the C# composer shows the reason written
/// (`SessionTabToolTipComposer.cs:94`), the tooltip being read once, on demand.
fn state_words(state: SessionState, reason: Option<String>) -> Option<String> {
    match state {
        SessionState::Connected => None,
        SessionState::Connecting => Some(fl!("ui-status-connecting")),
        SessionState::Reconnecting => Some(fl!("ui-status-reconnecting")),
        SessionState::Ended => Some(fl!("ui-status-disconnected")),
        SessionState::Failed => Some(reason.unwrap_or_else(|| fl!("ui-status-error"))),
    }
}

/// Where a profile goes, as the C# `ServerItemViewModel.Endpoint`: its host, and its port
/// when it has one.
fn endpoint_text((host, port): &(String, u16)) -> Option<String> {
    let host = host.trim();
    if host.is_empty() {
        None
    } else if *port == 0 {
        Some(host.to_owned())
    } else {
        Some(format!("{host}:{port}"))
    }
}

/// The lines of a tab's tooltip, as the C# `SessionTabToolTipComposer.Compose`: its whole
/// `title`, the strip cutting a long one; then, from the saved `profile` behind it, where
/// it goes and by which protocol, as whom and from which folder; then its `state` when it
/// needs a look. Never a secret: a profile's summary carries no password nor passphrase.
fn tooltip_lines(
    title: String,
    profile: Option<&ProfileSummary>,
    state: Option<String>,
) -> Vec<String> {
    let mut lines = vec![title];
    if let Some(profile) = profile {
        if let Some(endpoint) = profile.endpoint.as_ref().and_then(endpoint_text) {
            lines.push(fl!(
                "ui-tab-tooltip-endpoint",
                endpoint = endpoint,
                protocol = profile.kind.label()
            ));
        }
        if let Some(user) = profile.username.as_deref().map(str::trim)
            && !user.is_empty()
        {
            lines.push(fl!("ui-tab-tooltip-user", user = user));
        }
        if let Some(folder) = profile.group.as_deref().map(str::trim)
            && !folder.is_empty()
        {
            lines.push(fl!("ui-tab-tooltip-folder", folder = folder));
        }
    }
    if let Some(state) = state {
        lines.push(fl!("ui-tab-tooltip-state", state = state));
    }
    lines
}

/// What a tab's close button says: the tab it closes, as the C# `A11yCloseSessionTabNamed`
/// (`ConnectionViewModel.cs:230`), since every tab has one.
fn close_label(name: &str) -> String {
    fl!("ui-tab-close-named", name = name)
}

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
                (fl!("ui-tab-route-badge-healthy"), styles::success_text)
            }
            heimdall_app::tunnel::TunnelBadge::Unhealthy => {
                (fl!("ui-tab-route-badge-unhealthy"), styles::warning_text)
            }
        };
        Some(
            styles::tip(
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
    /// and the mark of a state that needs a look, the tooltip over them, then its marks.
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
                styles::tip(
                    icons::icon(Icon::Pin, Tint::Accent, TAB_PIN_SIDE),
                    text(fl!("ui-tab-pinned-badge")).size(font_size::CAPTION),
                    tooltip::Position::Bottom,
                )
                .padding(styles::TOOLTIP_PADDING)
                .style(styles::tooltip),
            );
        }
        // One long server title cannot take the whole strip: it ends with an ellipsis.
        let name = Self::tab_name(tab);
        let mut titled = row![
            crate::ellipsis::text(name.clone())
                .size(font_size::BODY)
                .font(styles::SEMIBOLD)
                .style(text::base)
                .max_width(TAB_TITLE_MAX_WIDTH),
        ]
        .spacing(spacing::XS)
        .align_y(iced::Alignment::Center);
        // A tool's tab is no session: it has no state to show.
        let state = tab.tool().is_none().then(|| SessionState::of(tab));
        if let Some(mark) = state.and_then(state_mark) {
            titled = titled.push(self.state_mark_view(mark));
        }
        // Its whole title, where it goes and its state, as the C# tab's tooltip.
        let profile = self.app.tab_profile(tab);
        let lines = tooltip_lines(name, profile.as_ref(), Self::state_text(tab));
        label = label.push(
            styles::tip(
                titled,
                text(lines.join("\n")).size(font_size::CAPTION),
                tooltip::Position::Bottom,
            )
            .delay(TAB_TOOLTIP_DELAY)
            .padding(styles::TOOLTIP_PADDING)
            .style(styles::tooltip),
        );
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

    /// The state of `tab`'s session in words, `None` for a connected one and for a tool.
    fn state_text(tab: &Tab) -> Option<String> {
        let state = tab.tool().is_none().then(|| SessionState::of(tab))?;
        let reason = match &tab.phase {
            Phase::Failed(error) => Some(super::failure_text(tab, error)),
            _ => None,
        };
        state_words(state, reason)
    }

    /// Every tab of the strip, as the C# `TabListMenuFactory` lists them: in the strip's
    /// order, each with its state when it needs a look, the folder at the right while the
    /// tabs are gathered by folder, the tab shown ticked. `None` with no tab to list.
    pub(super) fn tab_list_menu(&self) -> Option<Element<'_, Message>> {
        let shown = self.shown_tab_id();
        let items: Vec<TabListItem> = self
            .app
            .strip()
            .into_iter()
            .map(|tab| {
                let name = Self::tab_name(tab);
                TabListItem {
                    tab: tab.id,
                    label: match Self::state_text(tab) {
                        Some(state) => fl!("ui-tab-list-entry", name = name, state = state),
                        None => name,
                    },
                    folder: self
                        .app
                        .tab_folder(tab)
                        .map(|folder| folder_label(&folder).to_owned()),
                    active: shown == Some(tab.id),
                }
            })
            .collect();
        (!items.is_empty()).then(|| tree_view::tab_list_entries(items))
    }

    /// The chip heading a folder's tabs while they are gathered by folder, as the C# group
    /// chip carrying the folder's name and a chevron: a click folds the tabs of the folder
    /// into the chip alone, or unfolds them. Its tooltip names the folder by its full path and
    /// the state, because two folders of one name in different branches would read the same.
    fn folder_chip(&self, head: &Tab, folder: &str) -> Element<'static, Message> {
        let shut = self.app.folder_folded(head);
        let words = if shut {
            fl!("ui-tab-folder-chip-folded", folder = folder)
        } else if self.app.can_fold_folder(head.id) {
            fl!("ui-tab-folder-chip-open", folder = folder)
        } else {
            // A click leaves it open: say why, as the C# chip does.
            fl!("ui-tab-folder-chip-lone", folder = folder)
        };
        let chip = container(
            row![
                icons::icon(
                    if shut {
                        Icon::ChevronRight
                    } else {
                        Icon::ChevronDown
                    },
                    Tint::Text,
                    font_size::SMALL_CAPTION,
                ),
                text(folder_label(folder).to_owned())
                    .size(font_size::CAPTION)
                    .font(styles::SEMIBOLD),
            ]
            .spacing(FOLDER_CHIP_GAP)
            .align_y(iced::Alignment::Center),
        )
        .padding(FOLDER_CHIP_PADDING)
        .style(styles::folder_chip);
        styles::tip(
            mouse_area(chip)
                .interaction(mouse::Interaction::Pointer)
                .on_press(Message::App(AppMessage::TabMenu(
                    TabMenuMessage::ToggleFolderFold(head.id),
                ))),
            text(words).size(font_size::CAPTION),
            tooltip::Position::Bottom,
        )
        .padding(styles::TOOLTIP_PADDING)
        .style(styles::tooltip)
        .into()
    }

    /// The mark of a tab that needs a look, as the C# tab's: the dot pulsing unless Windows
    /// asks for reduced motion, in the colours of the tree's state dots.
    fn state_mark_view(&self, mark: StateMark) -> Element<'static, Message> {
        match mark {
            StateMark::Pulse => Element::new(PulseDot {
                side: TAB_MARK_SIDE,
                pulses: !self.reduced_motion,
            }),
            StateMark::Ring => container(iced::widget::space())
                .width(TAB_MARK_SIDE)
                .height(TAB_MARK_SIDE)
                .style(|theme: &Theme| container::Style {
                    border: iced::Border {
                        color: theme.extended_palette().secondary.base.color,
                        width: TAB_RING_WIDTH,
                        radius: (TAB_MARK_SIDE / 2.0).into(),
                    },
                    ..container::Style::default()
                })
                .into(),
            StateMark::Warning => icons::icon(Icon::Warning, Tint::Danger, TAB_WARNING_SIDE),
        }
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
                styles::tip(
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
                styles::tip(
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
        let mut folder: Option<String> = None;
        for tab in self.app.strip() {
            let active = shown == Some(tab.id);
            // A chip before the first tab of each folder, once the tabs are gathered.
            let tab_folder = self.app.tab_folder(tab);
            let key = tab_folder.as_ref().map(|name| name.to_lowercase());
            if key != folder
                && let Some(name) = &tab_folder
            {
                tabs = tabs.push(self.folder_chip(tab, name));
            }
            folder = key;
            // The tabs of a folded folder leave the strip, the chip standing for them.
            if self.app.folder_folded(tab) {
                continue;
            }
            for mark in self.tab_marks(tab) {
                tabs = tabs.push(mark);
            }
            // The close button inside the tab, at its right, as the C# tab's: a circled cross,
            // faded but on the tab selected or pointed at. It takes the press, which selecting
            // the tab then does not see.
            let quiet = !active && self.tab_hover != Some(tab.id);
            // Naming the tab it closes, as the C# button's accessible name, in a tooltip.
            let close = container(
                styles::tip(
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
                    text(close_label(&Self::tab_name(tab))).size(font_size::CAPTION),
                    tooltip::Position::Bottom,
                )
                .padding(styles::TOOLTIP_PADDING)
                .style(styles::tooltip),
            )
            .id(tab_close_id(tab.id));
            let label = self.tab_heading(tab, active).push(close);
            tabs = tabs.push(self.tab_button(tab.id, label, active));
        }
        // Judged against the width the strip has without the list button, so that the room it
        // takes does not undo the decision that showed it, as the C# `UpdateTabStripOverflow`.
        let reserve = if self.tab_overflow {
            TAB_LIST_RESERVE
        } else {
            0.0
        };
        strip = strip.push(
            crate::tab_scroller::tab_scroller(tabs, tab_strip_id(), shown.map(tab_id))
                .on_overflow(reserve, Message::TabOverflow),
        );
        // Every open tab, for the ones scrolled out of the strip.
        if self.tab_overflow {
            strip = strip.push(
                styles::tip(
                    icons::button(Icon::ChevronDown)
                        .style(styles::secondary)
                        .on_press(Message::OpenTreeMenu(TreeMenu::TabList)),
                    text(fl!("ui-tab-list-tooltip")).size(font_size::CAPTION),
                    tooltip::Position::Bottom,
                )
                .padding(styles::TOOLTIP_PADDING)
                .style(styles::tooltip),
            );
        }
        strip.into()
    }
}

/// The count of a tab's running post-connect steps, as the C# tab shows it; its tooltip says
/// which step and what became of it, a click stops the rest.
fn post_connect_badge(tab: TabId, progress: &PostConnectProgress) -> Element<'_, Message> {
    let count = format!("{}/{}", progress.step, progress.total);
    let status = texts::step_status(progress.status);
    styles::tip(
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

/// The pulse of a connecting tab's dot, as the C# `TabStatePulse` storyboard: from opaque to
/// faint in [`PULSE_HALF`], then back, over and over; told the time rather than reading the
/// clock. It holds still, opaque, while it does not pulse.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct Pulse {
    /// When it started pulsing; `None` while it does not.
    since: Option<Instant>,
    /// The time of the frame last drawn.
    frame: Option<Instant>,
}

impl Pulse {
    /// Takes note of a frame drawn at `now`, pulsing or not (`pulses`: Windows does not ask
    /// for reduced motion); when to draw the next one, `None` while it holds still.
    fn observe(&mut self, now: Instant, pulses: bool) -> Option<Instant> {
        if !pulses {
            *self = Self::default();
            return None;
        }
        self.since.get_or_insert(now);
        self.frame = Some(now);
        now.checked_add(PULSE_FRAME)
    }

    /// The dot's opacity in the frame last drawn: opaque while it does not pulse.
    fn opacity(&self) -> f32 {
        let (Some(since), Some(frame)) = (self.since, self.frame) else {
            return 1.0;
        };
        let half = PULSE_HALF.as_secs_f32();
        let into = frame.saturating_duration_since(since).as_secs_f32() % (2.0 * half);
        // How far it has faded: from nothing to all of it, then back.
        let faded = if into <= half {
            into / half
        } else {
            2.0 - into / half
        };
        1.0 - (1.0 - PULSE_FAINTEST) * faded
    }
}

/// The dot of a connecting tab, in the warning colour of the tree's state dots, pulsing as
/// the C# tab's unless `pulses` is off. Drawn anew on its own, without the strip being
/// built again.
struct PulseDot {
    side: f32,
    pulses: bool,
}

impl<M> Widget<M, Theme, iced::Renderer> for PulseDot {
    fn size(&self) -> Size<Length> {
        Size::new(Length::Fixed(self.side), Length::Fixed(self.side))
    }

    fn tag(&self) -> tree::Tag {
        tree::Tag::of::<Pulse>()
    }

    fn state(&self) -> tree::State {
        tree::State::new(Pulse::default())
    }

    fn layout(
        &mut self,
        _tree: &mut Tree,
        _renderer: &iced::Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        layout::atomic(limits, self.side, self.side)
    }

    fn update(
        &mut self,
        tree: &mut Tree,
        event: &Event,
        _layout: Layout<'_>,
        _cursor: mouse::Cursor,
        _renderer: &iced::Renderer,
        _clipboard: &mut dyn Clipboard,
        shell: &mut WidgetShell<'_, M>,
        _viewport: &Rectangle,
    ) {
        if let Event::Window(window::Event::RedrawRequested(now)) = event
            && let Some(next) = tree
                .state
                .downcast_mut::<Pulse>()
                .observe(*now, self.pulses)
        {
            shell.request_redraw_at(next);
        }
    }

    fn draw(
        &self,
        tree: &Tree,
        renderer: &mut iced::Renderer,
        theme: &Theme,
        _style: &renderer::Style,
        layout: Layout<'_>,
        _cursor: mouse::Cursor,
        _viewport: &Rectangle,
    ) {
        let opacity = tree.state.downcast_ref::<Pulse>().opacity();
        <iced::Renderer as renderer::Renderer>::fill_quad(
            renderer,
            renderer::Quad {
                bounds: layout.bounds(),
                border: iced::Border {
                    radius: (self.side / 2.0).into(),
                    ..iced::Border::default()
                },
                ..renderer::Quad::default()
            },
            theme
                .extended_palette()
                .warning
                .base
                .color
                .scale_alpha(opacity),
        );
    }
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, Instant};

    use heimdall_app::{ProfileKind, ProfileSummary, SessionState};
    use heimdall_core::profile::ProfileId;

    use super::{
        PULSE_FAINTEST, PULSE_FRAME, PULSE_HALF, Pulse, StateMark, close_label, state_mark,
        state_words, tooltip_lines,
    };

    fn summary(
        kind: ProfileKind,
        endpoint: Option<(&str, u16)>,
        user: Option<&str>,
        folder: Option<&str>,
    ) -> ProfileSummary {
        ProfileSummary {
            tool: None,
            id: ProfileId::new("web"),
            name: "web-prod-01".to_owned(),
            group: folder.map(str::to_owned),
            kind,
            endpoint: endpoint.map(|(host, port)| (host.to_owned(), port)),
            username: user.map(str::to_owned),
            gateway: None,
            favorite: false,
            metadata: heimdall_core::metadata::ProfileMetadata::default(),
        }
    }

    fn opaque(pulse: &Pulse) -> bool {
        (pulse.opacity() - 1.0).abs() < 1e-3
    }

    #[test]
    fn each_state_that_needs_a_look_has_its_own_mark_and_a_connected_one_none() {
        assert_eq!(
            state_mark(SessionState::Connected),
            None,
            "healthy tabs stay quiet"
        );
        assert_eq!(state_mark(SessionState::Connecting), Some(StateMark::Pulse));
        assert_eq!(
            state_mark(SessionState::Reconnecting),
            Some(StateMark::Pulse)
        );
        assert_eq!(state_mark(SessionState::Ended), Some(StateMark::Ring));
        assert_eq!(state_mark(SessionState::Failed), Some(StateMark::Warning));
    }

    #[test]
    fn the_tooltip_says_where_an_ssh_tab_goes_as_whom_and_from_which_folder() {
        let profile = summary(
            ProfileKind::Ssh,
            Some(("web01.lab", 22)),
            Some("admin"),
            Some("Prod/Web"),
        );
        assert_eq!(
            tooltip_lines("web-prod-01".to_owned(), Some(&profile), None),
            [
                "web-prod-01",
                "web01.lab:22 (SSH)",
                "User: admin",
                "Folder: Prod/Web",
            ]
        );
    }

    #[test]
    fn the_tooltip_leaves_out_what_a_profile_does_not_have() {
        // Telnet takes no account; a profile filed nowhere has no folder.
        let telnet = summary(ProfileKind::Telnet, Some(("switch.lab", 23)), None, None);
        assert_eq!(
            tooltip_lines("switch".to_owned(), Some(&telnet), None),
            ["switch", "switch.lab:23 (Telnet)"]
        );
        let rdp = summary(ProfileKind::Rdp, Some(("dc01", 3389)), Some("  "), Some(""));
        assert_eq!(
            tooltip_lines("dc01".to_owned(), Some(&rdp), None),
            ["dc01", "dc01:3389 (RDP)"]
        );
        // A local program reaches no server: no endpoint, so no protocol either, as the C#
        // composer.
        let local = summary(ProfileKind::Local, None, None, Some("Tools"));
        assert_eq!(
            tooltip_lines("cmd".to_owned(), Some(&local), None),
            ["cmd", "Folder: Tools"]
        );
        // A tab with no saved profile behind it: its title, and its state.
        assert_eq!(
            tooltip_lines(
                "shell".to_owned(),
                None,
                state_words(SessionState::Connecting, None)
            ),
            ["shell", "State: Connecting..."]
        );
    }

    #[test]
    fn the_tooltip_says_the_state_of_a_tab_that_needs_a_look_and_why_it_failed() {
        let profile = summary(ProfileKind::Vnc, Some(("vnc.lab", 5900)), None, None);
        let said = |state, reason: Option<&str>| {
            tooltip_lines(
                "vnc".to_owned(),
                Some(&profile),
                state_words(state, reason.map(str::to_owned)),
            )
            .last()
            .cloned()
        };
        assert_eq!(
            said(SessionState::Connected, None).as_deref(),
            Some("vnc.lab:5900 (VNC)"),
            "a connected tab says no state"
        );
        assert_eq!(
            said(SessionState::Connecting, None).as_deref(),
            Some("State: Connecting...")
        );
        assert_eq!(
            said(SessionState::Reconnecting, None).as_deref(),
            Some("State: Reconnecting...")
        );
        assert_eq!(
            said(SessionState::Ended, None).as_deref(),
            Some("State: Disconnected")
        );
        assert_eq!(
            said(SessionState::Failed, Some("Connection refused")).as_deref(),
            Some("State: Connection refused"),
            "the reason as the pane's card says it"
        );
        assert_eq!(
            said(SessionState::Failed, None).as_deref(),
            Some("State: Error")
        );
    }

    #[test]
    fn the_close_button_names_the_tab_it_closes() {
        assert_eq!(close_label("web-prod-01"), "Close web-prod-01");
    }

    #[test]
    fn the_dot_fades_and_comes_back_as_the_csharp_storyboard() {
        let start = Instant::now();
        let mut pulse = Pulse::default();
        assert!(opaque(&pulse), "opaque before its first frame");
        assert_eq!(
            pulse.observe(start, true),
            Some(start + PULSE_FRAME),
            "the next frame asked for"
        );
        assert!(opaque(&pulse));
        pulse.observe(start + PULSE_HALF / 2, true);
        assert!((pulse.opacity() - f32::midpoint(1.0, PULSE_FAINTEST)).abs() < 1e-3);
        pulse.observe(start + PULSE_HALF, true);
        assert!(
            (pulse.opacity() - PULSE_FAINTEST).abs() < 1e-3,
            "faintest after half a pulse"
        );
        pulse.observe(start + PULSE_HALF * 2, true);
        assert!(opaque(&pulse), "opaque again after a whole one");
    }

    #[test]
    fn reduced_motion_holds_the_dot_still_and_opaque_asking_no_frame() {
        let start = Instant::now();
        let mut pulse = Pulse::default();
        pulse.observe(start, true);
        pulse.observe(start + PULSE_HALF, true);
        assert!(!opaque(&pulse), "pulsing");
        let still = start + PULSE_HALF + Duration::from_millis(1);
        assert_eq!(pulse.observe(still, false), None, "no frame asked for");
        assert!(opaque(&pulse));
        assert_eq!(pulse.observe(still + PULSE_HALF, false), None);
        assert!(opaque(&pulse), "still opaque half a pulse later");
    }
}
