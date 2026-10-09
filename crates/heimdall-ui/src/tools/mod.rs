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

//! The built-in tools as the window shows them: each tool's name, description and icon, as
//! the C# `ToolRegistry` and its locale keys give them; the state of each tool's tab, as the
//! C# tool views and their view models keep it; and how a tool's tab is drawn.
//!
//! A tool's tab is the core's ([`heimdall_app::tools`]): opened, shown and closed as any tab.
//! What is typed in it is the window's, as the integrated editor's text is: kept here by
//! tab, made when the tab opens and dropped when it closes.
//!
//! Adding a tool: its arms in [`label`], [`description`] and [`icon`], its pane module, and
//! its arms in [`Pane`], [`ToolPane::new`], [`ToolPanes::update`] and [`view`].

mod base64_tool;
pub mod catalog;
mod url_tool;
mod uuid_tool;

use std::collections::HashMap;
use std::time::Duration;

use heimdall_app::tools::{ToolCategory, ToolGroup, ToolId};
use heimdall_app::{App, TabId};
use iced::advanced::text::highlighter::PlainText;
use iced::widget::text_editor::{self, Action, Content};
use iced::widget::{Column, button, column, container, row, scrollable, text, tooltip};
use iced::{Element, Length, Task, window};

pub use base64_tool::Base64Message;
pub use url_tool::UrlMessage;
pub use uuid_tool::UuidMessage;

use crate::i18n::fl;
use crate::icons::Icon;
use crate::shell::Message;
use crate::styles;
use crate::tokens::{font_size, spacing};

/// How long a copy button shows its check mark, as the C# `CopyFeedbackHelper`'s
/// `FeedbackDuration`.
const COPY_FEEDBACK: Duration = Duration::from_secs(1);

/// What a copy button shows meanwhile, as the C# check mark.
const CHECK_MARK: &str = "\u{2713}";

/// What the help button shows, as the C# one: a sign, the same in every language.
const HELP_GLYPH: &str = "?";

/// Padding of a tool's header, as the C# `ToolHeaderPadding` (12 across, 8 down).
const HEADER_PADDING: [f32; 2] = [8.0, 12.0];

/// Side of the help button, as the C# `ToolHelpButtonStyle`.
const HELP_BUTTON_SIDE: f32 = 32.0;

/// Height of the help text before it scrolls, as the C# help panel's `ScrollViewer`.
const HELP_MAX_HEIGHT: f32 = 200.0;

/// Padding of the help panel, as the C# `PaddingSectionCard`.
const HELP_PADDING: f32 = 12.0;

/// Widest a tool's content is drawn, as the C# `ToolContentMaxWidth`.
const CONTENT_MAX_WIDTH: f32 = 700.0;

/// Padding inside a text box, as the C# `PaddingInput` (8 across, 6 down).
const INPUT_PADDING: [f32; 2] = [6.0, 8.0];

/// Padding of a tool's main buttons, as the C# `PaddingButtonPrimary` (16 across, 6 down).
const PRIMARY_PADDING: [f32; 2] = [6.0, 16.0];

/// Padding of a tool's copy buttons, as the C# `PaddingButtonCopy` (10 across, 4 down).
const COPY_PADDING: [f32; 2] = [4.0, 10.0];

/// The text of a tool's boxes: monospaced, as the C# Consolas.
const BOX_FONT: iced::Font = iced::Font::MONOSPACE;

/// A line break written into a tool's output, as .NET's `Environment.NewLine`.
#[cfg(windows)]
const NEW_LINE: &str = "\r\n";
#[cfg(not(windows))]
const NEW_LINE: &str = "\n";

/// The name of `tool`, as the C# `PaletteTool*` keys name it: its tab's title, its card's
/// and its row's in the sidebar.
#[must_use]
pub fn label(tool: ToolId) -> String {
    match tool {
        ToolId::Base64 => fl!("ui-tool-base64-name"),
        ToolId::UrlEncoder => fl!("ui-tool-urlenc-name"),
        ToolId::Uuid => fl!("ui-tool-uuid-name"),
    }
}

/// What `tool` does, as the C# `ToolDesc*` keys say it: under its name on its card.
#[must_use]
pub fn description(tool: ToolId) -> String {
    match tool {
        ToolId::Base64 => fl!("ui-tool-base64-description"),
        ToolId::UrlEncoder => fl!("ui-tool-urlenc-description"),
        ToolId::Uuid => fl!("ui-tool-uuid-description"),
    }
}

/// The icon of `tool`, as the C# registry's `Geo.Tool.*` geometry.
#[must_use]
pub const fn icon(tool: ToolId) -> Icon {
    match tool {
        ToolId::Base64 => Icon::ToolBase64,
        ToolId::UrlEncoder => Icon::ToolUrlEncoder,
        ToolId::Uuid => Icon::ToolUuid,
    }
}

/// The name of `category`, as the C# `ToolCategory*` keys.
#[must_use]
pub fn category_label(category: ToolCategory) -> String {
    match category {
        ToolCategory::Network => fl!("ui-tools-category-network"),
        ToolCategory::Security => fl!("ui-tools-category-security"),
        ToolCategory::Encoding => fl!("ui-tools-category-encoding"),
        ToolCategory::System => fl!("ui-tools-category-system"),
        ToolCategory::External => fl!("ui-tools-category-external"),
    }
}

/// The name of a group of the sidebar's tools.
#[must_use]
pub fn group_label(group: ToolGroup) -> String {
    match group {
        ToolGroup::Favorites => fl!("ui-tools-favorites"),
        ToolGroup::Category(category) => category_label(category),
    }
}

/// Whether the sidebar's filter `filter` finds `tool`, as the C# `FilterSidebarTools`: its
/// name or one of its aliases holds the filter, whatever the case; an empty filter finds
/// every tool.
#[must_use]
pub fn sidebar_matches(tool: ToolId, filter: &str) -> bool {
    let filter = filter.trim().to_lowercase();
    if filter.is_empty() {
        return true;
    }
    let searchable = format!("{} {}", label(tool), tool.prefixes().join(" ")).to_lowercase();
    searchable.contains(&filter)
}

/// Whether the Tools page's search `search` finds `tool`, as the C#
/// `RefreshToolsTabSections`: its name, its aliases or its description holds it, whatever
/// the case.
#[must_use]
pub fn page_matches(tool: ToolId, search: &str) -> bool {
    let search = search.trim().to_lowercase();
    if search.is_empty() {
        return true;
    }
    let searchable = format!(
        "{} {} {}",
        label(tool),
        tool.prefixes().join(" "),
        description(tool)
    )
    .to_lowercase();
    searchable.contains(&search)
}

/// A copy button of a tool, by what it copies.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CopySlot {
    /// The Base64 output.
    Base64Output,
    /// The URL encoder's decoded text.
    UrlDecoded,
    /// The URL encoder's encoded text.
    UrlEncoded,
    /// The UUID generated.
    UuidSingle,
    /// The batch of UUIDs.
    UuidBatch,
}

/// What a tool's tab is asked.
#[derive(Debug, Clone)]
pub enum ToolMessage {
    /// Show or hide the tool's help, as the C# "?" button.
    ToggleHelp,
    /// Hide the help, as its close button.
    CloseHelp,
    /// The check mark of copy number `.0` has been shown long enough.
    CopyShown(u64),
    /// The Base64 tool's.
    Base64(Base64Message),
    /// The URL encoder's.
    Url(UrlMessage),
    /// The UUID generator's.
    Uuid(UuidMessage),
}

/// A tool's own state.
#[derive(Debug)]
enum Pane {
    Base64(base64_tool::Base64Pane),
    Url(url_tool::UrlPane),
    Uuid(uuid_tool::UuidPane),
}

/// What a tool's tab holds: the tool's state, its help shown or not, the copy button that
/// shows its check mark.
#[derive(Debug)]
pub struct ToolPane {
    pane: Pane,
    help: bool,
    copied: Option<(CopySlot, u64)>,
    copies: u64,
}

impl ToolPane {
    /// A new tab's state for `tool`, as the C# view's `Initialize`.
    fn new(tool: ToolId) -> Self {
        let pane = match tool {
            ToolId::Base64 => Pane::Base64(base64_tool::Base64Pane::default()),
            ToolId::UrlEncoder => Pane::Url(url_tool::UrlPane::default()),
            ToolId::Uuid => Pane::Uuid(uuid_tool::UuidPane::new()),
        };
        Self {
            pane,
            help: false,
            copied: None,
            copies: 0,
        }
    }

    /// Whether its help is shown.
    #[must_use]
    pub fn help_shown(&self) -> bool {
        self.help
    }

    /// The copy button of `slot` shows its check mark.
    fn copied(&self, slot: CopySlot) -> bool {
        self.copied.is_some_and(|(shown, _)| shown == slot)
    }

    /// `content` copied by the button of `slot`: written to the clipboard, the button's
    /// check mark shown for [`COPY_FEEDBACK`], as the C# copy buttons; nothing to copy,
    /// nothing happens.
    fn copy(&mut self, tab: TabId, slot: CopySlot, content: String) -> Task<Message> {
        if content.is_empty() {
            return Task::none();
        }
        self.copies += 1;
        let copy = self.copies;
        self.copied = Some((slot, copy));
        Task::batch([
            iced::clipboard::write(content),
            Task::perform(tokio::time::sleep(COPY_FEEDBACK), move |()| {
                Message::Tool(tab, ToolMessage::CopyShown(copy))
            }),
        ])
    }
}

/// The tool tabs' states, by tab.
#[derive(Debug, Default)]
pub struct ToolPanes {
    panes: HashMap<TabId, ToolPane>,
}

impl ToolPanes {
    /// Follows the core's tabs: a state made for each tool tab just opened, as the C# view
    /// is made with its tab; the states of tabs closed let go.
    pub fn sync(&mut self, app: &App) {
        self.panes
            .retain(|id, _| app.tab(*id).and_then(heimdall_app::Tab::tool).is_some());
        for tab in &app.tabs {
            if let Some(tool) = tab.tool() {
                self.panes
                    .entry(tab.id)
                    .or_insert_with(|| ToolPane::new(tool));
            }
        }
    }

    /// The state of tool tab `tab`.
    #[must_use]
    pub fn get(&self, tab: TabId) -> Option<&ToolPane> {
        self.panes.get(&tab)
    }

    /// Applies `message` to tool tab `tab`; the window `main` holds the file dialogs.
    pub fn update(
        &mut self,
        tab: TabId,
        message: ToolMessage,
        main: Option<window::Id>,
    ) -> Task<Message> {
        let Some(state) = self.panes.get_mut(&tab) else {
            return Task::none();
        };
        match (message, &mut state.pane) {
            (ToolMessage::ToggleHelp, _) => state.help = !state.help,
            (ToolMessage::CloseHelp, _) => state.help = false,
            (ToolMessage::CopyShown(copy), _) => {
                if state.copied.is_some_and(|(_, shown)| shown == copy) {
                    state.copied = None;
                }
            }
            (ToolMessage::Base64(message), Pane::Base64(pane)) => {
                return match pane.update(message) {
                    base64_tool::Outcome::Copy(content) => {
                        state.copy(tab, CopySlot::Base64Output, content)
                    }
                    outcome => outcome.task(tab, main),
                };
            }
            (ToolMessage::Url(message), Pane::Url(pane)) => {
                if let Some((slot, content)) = pane.update(message) {
                    return state.copy(tab, slot, content);
                }
            }
            (ToolMessage::Uuid(message), Pane::Uuid(pane)) => {
                if let Some((slot, content)) = pane.update(message) {
                    return state.copy(tab, slot, content);
                }
            }
            // A message of another tool's: its tab was closed, another took its place.
            _ => {}
        }
        Task::none()
    }
}

/// What tool tab `tab`, showing `tool`, draws.
#[must_use]
pub fn view(tab: TabId, tool: ToolId, state: Option<&ToolPane>) -> Element<'_, Message> {
    let Some(state) = state else {
        return column![].into();
    };
    let (title, help) = match tool {
        ToolId::Base64 => (fl!("ui-tool-base64-title"), fl!("ui-tool-base64-help")),
        ToolId::UrlEncoder => (fl!("ui-tool-urlenc-title"), fl!("ui-tool-urlenc-help")),
        ToolId::Uuid => (fl!("ui-tool-uuid-title"), fl!("ui-tool-uuid-help")),
    };
    let body = match &state.pane {
        Pane::Base64(pane) => pane.view(tab, state),
        Pane::Url(pane) => pane.view(tab, state),
        Pane::Uuid(pane) => pane.view(tab, state),
    };
    column![
        header(tab, title),
        state.help.then(|| help_panel(tab, help))
    ]
    .push(body)
    .width(Length::Fill)
    .height(Length::Fill)
    .into()
}

/// A tool's header, as the C# tool views': its title, and the "?" button at its right.
fn header<'a>(tab: TabId, title: String) -> Element<'a, Message> {
    container(
        row![
            text(title)
                .size(font_size::SUBTITLE)
                .font(styles::SEMIBOLD)
                .width(Length::Fill),
            tooltip(
                button(
                    container(
                        text(HELP_GLYPH)
                            .size(font_size::BODY)
                            .font(styles::SEMIBOLD)
                    )
                    .center(Length::Fill)
                )
                .width(HELP_BUTTON_SIDE)
                .height(HELP_BUTTON_SIDE)
                .padding(0.0)
                .style(styles::secondary)
                .on_press(Message::Tool(tab, ToolMessage::ToggleHelp)),
                text(fl!("ui-tool-help-tooltip")).size(font_size::CAPTION),
                tooltip::Position::Bottom,
            )
            .style(container::rounded_box),
        ]
        .align_y(iced::Alignment::Center),
    )
    .padding(HEADER_PADDING)
    .width(Length::Fill)
    .style(styles::strip)
    .into()
}

/// The help under the header, as the C# inline help panel: the text, and a close button.
fn help_panel<'a>(tab: TabId, help: String) -> Element<'a, Message> {
    container(
        row![
            scrollable(text(help).size(font_size::BODY).style(text::secondary))
                .height(Length::Shrink)
                .width(Length::Fill),
            tooltip(
                crate::icons::button(Icon::Close)
                    .style(styles::subtle)
                    .on_press(Message::Tool(tab, ToolMessage::CloseHelp)),
                text(fl!("ui-tool-help-close")).size(font_size::CAPTION),
                tooltip::Position::Left,
            )
            .style(container::rounded_box),
        ]
        .spacing(spacing::SM),
    )
    .max_height(HELP_MAX_HEIGHT)
    .padding(HELP_PADDING)
    .width(Length::Fill)
    .style(styles::strip)
    .into()
}

/// A copy button of a tool: `label`, or the check mark while its copy is shown, disabled
/// then, as the C# `CopyFeedbackHelper`; its tooltip says it copies to the clipboard.
fn copy_button<'a>(
    label: String,
    copied: bool,
    on_press: Message,
    padding: [f32; 2],
) -> Element<'a, Message> {
    let shown = if copied { CHECK_MARK.to_owned() } else { label };
    tooltip(
        button(text(shown).size(font_size::BODY))
            .padding(padding)
            .style(styles::secondary)
            .on_press_maybe((!copied).then_some(on_press)),
        text(fl!("ui-tool-copy-tooltip")).size(font_size::CAPTION),
        tooltip::Position::Bottom,
    )
    .style(container::rounded_box)
    .into()
}

/// A tool's main button: `label`, `primary` filled with the accent as the C#
/// `PrimaryButtonStyle`, else secondary.
fn action_button<'a>(
    label: String,
    primary: bool,
    on_press: Option<Message>,
) -> Element<'a, Message> {
    button(text(label).size(font_size::BODY))
        .padding(PRIMARY_PADDING)
        .style(if primary {
            styles::primary
        } else {
            styles::secondary
        })
        .on_press_maybe(on_press)
        .into()
}

/// `content` with `action` applied unless it is an edit: a box that shows a result, which
/// the user selects and scrolls but does not change, as the C# `IsReadOnly` boxes.
fn read_only(content: &mut Content, action: Action) {
    if !action.is_edit() {
        content.perform(action);
    }
}

/// The text of a tool's box, without the line break iced's box may end it with, which the
/// C# box does not have.
fn box_text(content: &Content) -> String {
    let text = content.text();
    text.strip_suffix('\n')
        .map_or_else(|| text.clone(), str::to_owned)
}

/// A box of a tool's text, `content`, in the tool's font, as the C# `TextBox`: its caller
/// says what an action in it does.
fn text_box(
    content: &Content,
    placeholder: Option<String>,
) -> text_editor::TextEditor<'_, PlainText, Message> {
    let editor = iced::widget::text_editor(content)
        .font(BOX_FONT)
        .size(font_size::BODY_LARGE)
        .padding(INPUT_PADDING)
        .style(styles::text_box);
    match placeholder {
        Some(placeholder) => editor.placeholder(placeholder),
        None => editor,
    }
}

/// A tool's content, as the C# tool views': at most [`CONTENT_MAX_WIDTH`] wide, scrolled.
fn content_column(content: Column<'_, Message>) -> Element<'_, Message> {
    styles::scroll(
        container(content.max_width(CONTENT_MAX_WIDTH))
            .padding(spacing::LG)
            .width(Length::Fill),
    )
    .height(Length::Fill)
    .into()
}
