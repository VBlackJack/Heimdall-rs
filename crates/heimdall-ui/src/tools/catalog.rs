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

//! Where the tools are found, as the C# `ToolsTabPopulationService` builds them: the
//! sidebar's Tools tab (`MainWindow.xaml:1121-1236`), its filter, the tools pinned first and
//! each category folded or not; and the Tools page (`MainWindow.xaml:2229-2281`), its search,
//! its sections of tools pinned, used lately and all, and its cards with their pin.

use heimdall_app::tools::{ToolCategory, ToolGroup, ToolId};
use heimdall_app::{App, Message as AppMessage, ToolsMessage};
use iced::widget::{
    Column, Row, button, column, container, mouse_area, row, stack, text, text_input, tooltip,
};
use iced::{Color, Element, Length, Theme};

use crate::i18n::fl;
use crate::icons::{self, Icon, Tint};
use crate::shell::Message;
use crate::styles;
use crate::tokens::{font_size, radius, spacing};
use crate::tree_view::TreeMenu;

/// Width of a card, as the C# `CreateToolsTabCard`.
pub const CARD_WIDTH: f32 = 280.0;

/// Room between cards, as the C# card's margin.
const CARD_GAP: f32 = 8.0;

/// Padding inside a card, as the C# card's presenter (10 across, 8 down).
const CARD_PADDING: [f32; 2] = [8.0, 10.0];

/// Room kept at a card's right for its pin, as the C# content's margin.
const CARD_PIN_ROOM: f32 = 24.0;

/// Side of a card's icon square, as the C#'s.
const CARD_ICON_BOX: f32 = 32.0;

/// Side of a card's icon, as the C#'s.
const CARD_ICON_SIDE: f32 = 20.0;

/// Radius of a card's icon square, as the C#'s.
const CARD_ICON_RADIUS: f32 = 6.0;

/// How strongly the category's colour fills a card's icon square, as the C#'s opacity.
const CARD_ICON_ALPHA: f32 = 0.12;

/// Room between a card's icon and its text, as the C#'s.
const CARD_ICON_GAP: f32 = 10.0;

/// Side of a card's pin, as the C# glyph's size.
const PIN_SIDE: f32 = 12.0;

/// Room around a card's pin, as the C# button's margin and padding.
const PIN_INSET: f32 = 6.0;
const PIN_PADDING: f32 = 2.0;

/// Opacity of a pin not set, as the C#'s.
const PIN_UNSET_OPACITY: f32 = 0.4;

/// Widest the Tools page's search is, as the C#'s `MaxWidth`.
const SEARCH_MAX_WIDTH: f32 = 400.0;

/// Padding of the Tools page's header, as the C# (16 across, 12 above, 8 below).
const PAGE_HEADER_PADDING: iced::Padding = iced::Padding {
    top: 12.0,
    right: 16.0,
    bottom: 8.0,
    left: 16.0,
};

/// Padding of the Tools page's sections, as the C# (16 across, 8 above, 16 below).
const PAGE_CONTENT_PADDING: iced::Padding = iced::Padding {
    top: 8.0,
    right: 16.0,
    bottom: 16.0,
    left: 16.0,
};

/// Room under a section, as the C# panels' bottom margin.
const SECTION_GAP: f32 = 16.0;

/// Size of a category's bar on the Tools page, as the C# accent bar.
const CATEGORY_BAR_WIDTH: f32 = 3.0;
const CATEGORY_BAR_HEIGHT: f32 = 16.0;

/// Side of a group's dot in the sidebar, as the C# ellipse.
const GROUP_DOT_SIDE: f32 = 8.0;

/// Side of a tool's icon in the sidebar, as the C# path.
const LEAF_ICON_SIDE: f32 = 14.0;

/// Side of a group's expander arrow in the sidebar, as the tree's.
const EXPANDER_SIDE: f32 = 12.0;

/// How far a tool's row steps right of its group's, as the tree's indent.
const LEAF_INDENT: f32 = 16.0;

/// Padding of a group's count badge, as the C# (6 across, 1 down).
const BADGE_PADDING: [f32; 2] = [1.0, 6.0];

/// Room between a mark and its name, as the C# margins of 6.
const NAME_GAP: f32 = 6.0;

/// Padding of a row of the sidebar's tools, as the tree's rows.
const ROW_PADDING: [f32; 2] = [2.0, 4.0];

/// Padding inside the filter box, as the tree's search.
const FILTER_PADDING: [f32; 2] = [6.0, 10.0];

/// The tools of `category`, by name as the C# sorts them, case aside.
fn tools_of(category: ToolCategory) -> Vec<ToolId> {
    let mut tools: Vec<ToolId> = ToolId::ALL
        .into_iter()
        .filter(|tool| tool.category() == category)
        .collect();
    sort_by_name(&mut tools);
    tools
}

/// `tools` by name, case aside.
fn sort_by_name(tools: &mut [ToolId]) {
    tools.sort_by_cached_key(|tool| super::label(*tool).to_lowercase());
}

/// The categories that hold a tool of this version, in order.
fn categories() -> Vec<ToolCategory> {
    ToolCategory::ALL
        .into_iter()
        .filter(|category| ToolId::ALL.iter().any(|tool| tool.category() == *category))
        .collect()
}

/// What the sidebar's network context line says, as the C# `ToolContextProvider`: the host
/// a network tool would inherit, or that none would be.
fn context_label(app: &App) -> String {
    match app.tool_target_host() {
        Some(host) => fl!("ui-tools-context-with", host = host.as_str()),
        None => fl!("ui-tools-context-none"),
    }
}

/// The groups of the sidebar's Tools tab as `filter` leaves them: the tools pinned first, then
/// each category; with a filter, each group with what it finds, a group finding nothing left
/// out, as the C# `FilterSidebarTools`.
#[must_use]
pub fn sidebar_groups(app: &App, filter: &str) -> Vec<(ToolGroup, Vec<ToolId>)> {
    let mut favorites = app.favorite_tools();
    sort_by_name(&mut favorites);
    let filtering = !filter.trim().is_empty();
    std::iter::once((ToolGroup::Favorites, favorites))
        .chain(
            categories()
                .into_iter()
                .map(|category| (ToolGroup::Category(category), tools_of(category))),
        )
        .map(|(group, tools)| {
            let found: Vec<ToolId> = tools
                .into_iter()
                .filter(|tool| super::sidebar_matches(*tool, filter))
                .collect();
            (group, found)
        })
        .filter(|(_, found)| !filtering || !found.is_empty())
        .collect()
}

/// The sidebar's Tools tab, as the C# `SidebarToolsContent`: its filter, the network context,
/// "no matching tool" when the filter finds none, then the groups, each folded or not.
pub fn sidebar_panel<'a>(app: &App, filter: &str) -> Element<'a, Message> {
    let filtering = !filter.trim().is_empty();
    let groups = sidebar_groups(app, filter);
    let mut list = Column::new().spacing(crate::tree_view::ROW_GAP);
    if groups.is_empty() {
        list = list.push(
            container(
                text(fl!("ui-tools-no-results"))
                    .size(font_size::CAPTION)
                    .style(text::secondary),
            )
            .center_x(Length::Fill)
            .padding(spacing::MD),
        );
    }
    for (group, tools) in groups {
        // While filtering, a group is open when it finds a tool, whatever was chosen, as the
        // C# expands it without saving that.
        let open = filtering || app.tool_group_open(group);
        list = list.push(group_row(group, tools.len(), open, filtering));
        if open {
            for tool in tools {
                list = list.push(leaf_row(tool));
            }
        }
    }
    column![
        text_input(&fl!("ui-tools-filter-placeholder"), filter)
            .id(filter_id())
            .style(styles::text_input)
            .size(font_size::BODY)
            .padding(FILTER_PADDING)
            .on_input(Message::ToolsFilter),
        text(context_label(app))
            .size(font_size::SMALL_CAPTION)
            .style(text::secondary),
        styles::scroll(list).height(Length::Fill),
    ]
    .spacing(spacing::SM)
    .height(Length::Fill)
    .into()
}

/// The identifier of the sidebar's tool filter.
#[must_use]
pub fn filter_id() -> iced::widget::Id {
    iced::widget::Id::new("sidebar-tools-filter")
}

/// A group's header in the sidebar, as the C# category template: its arrow, its dot, its
/// name and how many tools it shows; a press folds or unfolds it, but while filtering.
fn group_row<'a>(
    group: ToolGroup,
    count: usize,
    open: bool,
    filtering: bool,
) -> Element<'a, Message> {
    let (arrow, arrow_tint) = if open {
        (Icon::ChevronDown, Tint::Text)
    } else {
        (Icon::ChevronRight, Tint::Secondary)
    };
    let dot = container(
        iced::widget::space()
            .width(GROUP_DOT_SIDE)
            .height(GROUP_DOT_SIDE),
    )
    .style(move |theme: &Theme| styles::filled(group_color(theme, group), GROUP_DOT_SIDE / 2.0));
    let badge = container(
        text(count.to_string())
            .size(font_size::CAPTION)
            .style(text::secondary),
    )
    .padding(BADGE_PADDING)
    .style(|theme: &Theme| styles::filled(styles::card_color(theme), radius::MD));
    button(
        row![
            icons::icon(arrow, arrow_tint, EXPANDER_SIDE),
            dot,
            text(super::group_label(group))
                .size(font_size::BODY)
                .font(styles::SEMIBOLD)
                .width(Length::Fill),
            badge,
        ]
        .spacing(NAME_GAP)
        .align_y(iced::Alignment::Center),
    )
    .width(Length::Fill)
    .padding(ROW_PADDING)
    .style(styles::subtle)
    .on_press_maybe((!filtering).then_some(Message::App(AppMessage::Tools(
        ToolsMessage::ToggleGroup(group),
    ))))
    .into()
}

/// The colour of `group`: the tools pinned in the warning colour, as the C# Favorites
/// category, a category in its own.
fn group_color(theme: &Theme, group: ToolGroup) -> Color {
    match group {
        ToolGroup::Favorites => theme.extended_palette().warning.base.color,
        ToolGroup::Category(category) => icons::tool_color(theme, category),
    }
}

/// A tool's row in the sidebar, as the C# leaf template: its icon in its category's colour
/// and its name; a click opens it, a right click offers to pin it.
fn leaf_row<'a>(tool: ToolId) -> Element<'a, Message> {
    let row = button(
        row![
            icons::icon(
                super::icon(tool),
                Tint::Tool(tool.category()),
                LEAF_ICON_SIDE
            ),
            text(super::label(tool)).size(font_size::BODY),
        ]
        .spacing(spacing::SM)
        .align_y(iced::Alignment::Center),
    )
    .width(Length::Fill)
    .padding(ROW_PADDING)
    .style(styles::subtle)
    .on_press(Message::OpenTool(tool));
    container(mouse_area(row).on_right_press(Message::OpenTreeMenu(TreeMenu::Tool(tool))))
        .padding(iced::Padding {
            left: LEAF_INDENT,
            ..iced::Padding::ZERO
        })
        .into()
}

/// What the Tools page lists for `search`, as the C# `RefreshToolsTabSections`: with no
/// search, the tools pinned in their order, those used lately, then every tool by category;
/// with one, the tools it finds by category alone.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PageSections {
    /// The tools pinned; `None` while searching.
    pub favorites: Option<Vec<ToolId>>,
    /// The tools used lately, the newest first; `None` while searching or when none was.
    pub recent: Option<Vec<ToolId>>,
    /// Every tool found, by category.
    pub categories: Vec<(ToolCategory, Vec<ToolId>)>,
    /// How many tools the count says: those found, or every tool.
    pub count: usize,
}

/// The Tools page's sections for `search`.
#[must_use]
pub fn page_sections(app: &App, search: &str) -> PageSections {
    let searching = !search.trim().is_empty();
    let categories: Vec<(ToolCategory, Vec<ToolId>)> = categories()
        .into_iter()
        .map(|category| {
            let found: Vec<ToolId> = tools_of(category)
                .into_iter()
                .filter(|tool| super::page_matches(*tool, search))
                .collect();
            (category, found)
        })
        .filter(|(_, found)| !found.is_empty())
        .collect();
    let found = categories.iter().map(|(_, tools)| tools.len()).sum();
    PageSections {
        favorites: (!searching).then(|| app.favorite_tools()),
        recent: (!searching && !app.recent_tools().is_empty()).then(|| app.recent_tools().to_vec()),
        categories,
        count: if searching { found } else { ToolId::ALL.len() },
    }
}

/// The Tools page, as the C# Tools tab: its title, search and count, the network context,
/// then its sections of cards.
pub fn page<'a>(app: &App, search: &str) -> Element<'a, Message> {
    let sections = page_sections(app, search);
    let header = page_header(app, search, sections.count);
    let mut content = Column::new();
    if let Some(favorites) = &sections.favorites {
        content = content.push(section_header(fl!("ui-tools-favorites"), |theme| {
            theme.extended_palette().warning.base.color
        }));
        content = if favorites.is_empty() {
            content.push(
                container(
                    text(fl!("ui-tools-empty-favorites"))
                        .size(font_size::CAPTION)
                        .style(text::secondary),
                )
                .padding(iced::Padding {
                    left: spacing::XS,
                    bottom: SECTION_GAP,
                    ..iced::Padding::ZERO
                }),
            )
        } else {
            content.push(cards(app, favorites, SECTION_GAP))
        };
    }
    if let Some(recent) = &sections.recent {
        content = content
            .push(section_header(fl!("ui-tools-recent"), |theme| {
                theme.palette().primary
            }))
            .push(cards(app, recent, SECTION_GAP));
    }
    if sections.favorites.is_some() {
        content = content.push(section_header(fl!("ui-tools-all"), |theme| {
            theme.palette().text
        }));
    }
    for (category, tools) in &sections.categories {
        content = content
            .push(category_header(*category))
            .push(cards(app, tools, CARD_GAP));
    }
    if sections.categories.is_empty() {
        content = content.push(
            container(
                column![
                    text(fl!("ui-tools-no-results"))
                        .size(font_size::BODY_LARGE)
                        .font(styles::SEMIBOLD)
                        .style(text::secondary),
                    text(fl!("ui-tools-no-results-hint"))
                        .size(font_size::CAPTION)
                        .style(text::secondary),
                ]
                .spacing(NAME_GAP)
                .align_x(iced::Alignment::Center),
            )
            .center_x(Length::Fill)
            .padding(iced::Padding {
                top: spacing::XL,
                ..iced::Padding::ZERO
            }),
        );
    }
    column![
        header,
        styles::scroll(
            container(content)
                .padding(PAGE_CONTENT_PADDING)
                .width(Length::Fill)
        )
        .height(Length::Fill),
    ]
    .width(Length::Fill)
    .height(Length::Fill)
    .into()
}

/// The identifier of the pin of `tool`'s card.
#[must_use]
pub fn pin_id(tool: ToolId) -> iced::widget::Id {
    iced::widget::Id::from(format!("tool-pin-{}", tool.code()))
}

/// The Tools page's header, as the C#: its title, its search and the `count` of tools it
/// lists, then the network context in the accent when a host would be inherited.
fn page_header<'a>(app: &App, search: &str, count: usize) -> Element<'a, Message> {
    let has_target = app.tool_target_host().is_some();
    container(
        column![
            row![
                text(fl!("ui-tools-page-title"))
                    .size(font_size::SUBTITLE)
                    .font(styles::SEMIBOLD),
                container(
                    text_input(&fl!("ui-tools-search-placeholder"), search)
                        .id(search_id())
                        .style(styles::text_input)
                        .size(font_size::BODY)
                        .padding(super::INPUT_PADDING)
                        .on_input(Message::ToolsSearch),
                )
                .max_width(SEARCH_MAX_WIDTH)
                .width(Length::Fill),
                iced::widget::space::horizontal(),
                text(fl!("ui-tools-count", count = count))
                    .size(font_size::CAPTION)
                    .style(text::secondary),
            ]
            .spacing(spacing::MD + spacing::XS)
            .align_y(iced::Alignment::Center),
            text(context_label(app))
                .size(font_size::CAPTION)
                .style(move |theme: &Theme| text::Style {
                    color: Some(if has_target {
                        theme.palette().primary
                    } else {
                        theme.extended_palette().secondary.base.color
                    }),
                }),
        ]
        .spacing(spacing::SM),
    )
    .padding(PAGE_HEADER_PADDING)
    .width(Length::Fill)
    .style(styles::strip)
    .into()
}

/// The identifier of the Tools page's search.
#[must_use]
pub fn search_id() -> iced::widget::Id {
    iced::widget::Id::new("tools-page-search")
}

/// A section's title, as the C# `AddToolsTabSectionHeader`: in the colour `color` gives.
fn section_header<'a>(label: String, color: fn(&Theme) -> Color) -> Element<'a, Message> {
    container(
        text(label)
            .size(font_size::BODY_LARGE)
            .font(styles::SEMIBOLD)
            .style(move |theme: &Theme| text::Style {
                color: Some(color(theme)),
            }),
    )
    .padding([spacing::SM, 0.0])
    .into()
}

/// A category's title, as the C# `AddToolsTabCategoryHeader`: a bar and its name in upper
/// case, in its colour.
fn category_header<'a>(category: ToolCategory) -> Element<'a, Message> {
    container(
        row![
            container(
                iced::widget::space()
                    .width(CATEGORY_BAR_WIDTH)
                    .height(CATEGORY_BAR_HEIGHT)
            )
            .style(move |theme: &Theme| {
                styles::filled(icons::tool_color(theme, category), CATEGORY_BAR_WIDTH / 2.0)
            }),
            text(super::category_label(category).to_uppercase())
                .size(font_size::CAPTION)
                .font(styles::SEMIBOLD)
                .style(move |theme: &Theme| text::Style {
                    color: Some(icons::tool_color(theme, category)),
                }),
        ]
        .spacing(NAME_GAP)
        .align_y(iced::Alignment::Center),
    )
    .padding([spacing::XS, 0.0])
    .into()
}

/// `tools` as cards, wrapped, then `gap` below them.
fn cards<'a>(app: &App, tools: &[ToolId], gap: f32) -> Element<'a, Message> {
    container(
        Row::with_children(
            tools
                .iter()
                .map(|tool| card(*tool, app.is_favorite_tool(*tool))),
        )
        .spacing(CARD_GAP)
        .wrap()
        .vertical_spacing(CARD_GAP),
    )
    .padding(iced::Padding {
        bottom: gap,
        ..iced::Padding::ZERO
    })
    .width(Length::Fill)
    .into()
}

/// A tool's card, as the C# `CreateToolsTabCard`: its icon on its category's colour, its
/// name and description; pressed, the tool opens; its pin at its top right pins or unpins it.
fn card<'a>(tool: ToolId, favorite: bool) -> Element<'a, Message> {
    let category = tool.category();
    let icon = container(icons::icon(
        super::icon(tool),
        Tint::Tool(category),
        CARD_ICON_SIDE,
    ))
    .center(CARD_ICON_BOX)
    .style(move |theme: &Theme| {
        styles::filled(
            icons::tool_color(theme, category).scale_alpha(CARD_ICON_ALPHA),
            CARD_ICON_RADIUS,
        )
    });
    let launch = button(
        row![
            icon,
            column![
                text(super::label(tool))
                    .size(font_size::BODY)
                    .font(styles::SEMIBOLD)
                    .wrapping(text::Wrapping::None),
                text(super::description(tool))
                    .size(font_size::SMALL_CAPTION)
                    .style(text::secondary)
                    .wrapping(text::Wrapping::None),
            ]
            .width(Length::Fill)
            .clip(true),
        ]
        .spacing(CARD_ICON_GAP)
        .padding(iced::Padding {
            right: CARD_PIN_ROOM,
            ..iced::Padding::ZERO
        })
        .align_y(iced::Alignment::Center),
    )
    .width(CARD_WIDTH)
    .padding(CARD_PADDING)
    .style(styles::tool_card)
    .on_press(Message::OpenTool(tool));
    let (glyph, tint, opacity, tip) = if favorite {
        (
            Icon::FavoriteStarFill,
            Tint::Warning,
            1.0,
            fl!("ui-tools-unpin-tooltip"),
        )
    } else {
        (
            Icon::FavoriteStar,
            Tint::Secondary,
            PIN_UNSET_OPACITY,
            fl!("ui-tools-pin-tooltip"),
        )
    };
    let pin = tooltip(
        button(icons::faded(glyph, tint, PIN_SIDE, opacity))
            .padding(PIN_PADDING)
            .style(styles::subtle)
            .on_press(Message::App(AppMessage::Tools(
                ToolsMessage::ToggleFavorite(tool),
            ))),
        text(tip).size(font_size::CAPTION),
        tooltip::Position::Bottom,
    )
    .style(container::rounded_box);
    stack![
        launch,
        container(container(pin).id(pin_id(tool)))
            .align_right(CARD_WIDTH)
            .padding(PIN_INSET),
    ]
    .into()
}
