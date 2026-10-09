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
//! its arms in [`Pane`], [`ToolPane::new`], [`ToolPanes::update`] (or `update_copying`, for
//! a tool whose update only copies) and [`view`].

mod base64_tool;
pub mod catalog;
mod chmod_tool;
mod crontab_tool;
mod datetime_tool;
mod diff_tool;
mod ip_converter_tool;
mod json_tool;
mod network_calculator_tool;
mod regex_tool;
mod ssh_config_tool;
mod subnet_tool;
mod text_case_tool;
mod ulid_tool;
mod url_tool;
mod uuid_tool;

use std::collections::HashMap;
use std::time::Duration;

use heimdall_app::tools::{ToolCategory, ToolGroup, ToolId};
use heimdall_app::{App, TabId};
use iced::advanced::text::highlighter::PlainText;
use iced::widget::text_editor::{self, Action, Content};
use iced::widget::{Column, button, column, container, row, scrollable, text, tooltip};
use iced::{Color, Element, Length, Task, Theme, window};

pub use base64_tool::Base64Message;
pub use chmod_tool::ChmodMessage;
pub use crontab_tool::{CronOption, CrontabMessage};
pub use datetime_tool::{DateField, DateTimeMessage, ZoneChoice};
pub use diff_tool::{Computed as DiffComputed, DiffMessage, compute as compute_diff};
pub use ip_converter_tool::{IpConverterMessage, IpField};
pub use json_tool::JsonMessage;
pub use network_calculator_tool::{NetCalcField, NetCalcMessage, NetCalcMode};
pub use regex_tool::RegexMessage;
pub use ssh_config_tool::{SshConfigMessage, SshField};
pub use subnet_tool::{SubnetField, SubnetMessage};
pub use text_case_tool::TextCaseMessage;
pub use ulid_tool::UlidMessage;
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

/// Padding of a tool's content, as the C# `ContentAreaMargin`: none above, under the header.
const BODY_PADDING: iced::Padding = iced::Padding {
    top: 0.0,
    right: spacing::LG - spacing::XS,
    bottom: spacing::LG - spacing::XS,
    left: spacing::LG - spacing::XS,
};

/// Room around an empty state, as the C# `ToolEmptyStateStyle`'s padding.
const EMPTY_STATE_PADDING: f32 = 24.0;

/// How strongly the accent marks a match of the regular expression tester, as the C#'s
/// 80 of 255.
const REGEX_MATCH_ALPHA: f32 = 80.0 / 255.0;

/// How strongly the warning colour marks a match with a named group, as the C#'s 100 of
/// 255.
const REGEX_GROUP_ALPHA: f32 = 100.0 / 255.0;

/// How strongly a line removed or added is coloured, as the C# `Diff*LineBrush`'s 48 of 255.
const DIFF_LINE_ALPHA: f32 = 48.0 / 255.0;

/// How strongly a word removed or added is coloured, as the C# `Diff*WordBrush`'s 96 of 255.
const DIFF_WORD_ALPHA: f32 = 96.0 / 255.0;

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
        ToolId::SubnetCalculator => fl!("ui-tool-subnet-name"),
        ToolId::IpConverter => fl!("ui-tool-ipconv-name"),
        ToolId::NetworkCalculator => fl!("ui-tool-netcalc-name"),
        ToolId::Base64 => fl!("ui-tool-base64-name"),
        ToolId::UrlEncoder => fl!("ui-tool-urlenc-name"),
        ToolId::JsonFormatter => fl!("ui-tool-json-name"),
        ToolId::RegexTester => fl!("ui-tool-regex-name"),
        ToolId::TextDiff => fl!("ui-tool-diff-name"),
        ToolId::TextCase => fl!("ui-tool-textcase-name"),
        ToolId::Chmod => fl!("ui-tool-chmod-name"),
        ToolId::DateTime => fl!("ui-tool-datetime-name"),
        ToolId::Uuid => fl!("ui-tool-uuid-name"),
        ToolId::Ulid => fl!("ui-tool-ulid-name"),
        ToolId::Crontab => fl!("ui-tool-crontab-name"),
        ToolId::SshConfig => fl!("ui-tool-sshconfig-name"),
    }
}

/// What `tool` does, as the C# `ToolDesc*` keys say it: under its name on its card.
#[must_use]
pub fn description(tool: ToolId) -> String {
    match tool {
        ToolId::SubnetCalculator => fl!("ui-tool-subnet-description"),
        ToolId::IpConverter => fl!("ui-tool-ipconv-description"),
        ToolId::NetworkCalculator => fl!("ui-tool-netcalc-description"),
        ToolId::Base64 => fl!("ui-tool-base64-description"),
        ToolId::UrlEncoder => fl!("ui-tool-urlenc-description"),
        ToolId::JsonFormatter => fl!("ui-tool-json-description"),
        ToolId::RegexTester => fl!("ui-tool-regex-description"),
        ToolId::TextDiff => fl!("ui-tool-diff-description"),
        ToolId::TextCase => fl!("ui-tool-textcase-description"),
        ToolId::Chmod => fl!("ui-tool-chmod-description"),
        ToolId::DateTime => fl!("ui-tool-datetime-description"),
        ToolId::Uuid => fl!("ui-tool-uuid-description"),
        ToolId::Ulid => fl!("ui-tool-ulid-description"),
        ToolId::Crontab => fl!("ui-tool-crontab-description"),
        ToolId::SshConfig => fl!("ui-tool-sshconfig-description"),
    }
}

/// The icon of `tool`, as the C# registry's `Geo.Tool.*` geometry.
#[must_use]
pub const fn icon(tool: ToolId) -> Icon {
    match tool {
        ToolId::SubnetCalculator => Icon::ToolSubnet,
        ToolId::IpConverter => Icon::ToolIpConverter,
        ToolId::NetworkCalculator => Icon::ToolNetworkCalculator,
        ToolId::Base64 => Icon::ToolBase64,
        ToolId::UrlEncoder => Icon::ToolUrlEncoder,
        ToolId::JsonFormatter => Icon::ToolJson,
        ToolId::RegexTester => Icon::ToolRegex,
        ToolId::TextDiff => Icon::ToolDiff,
        ToolId::TextCase => Icon::ToolTextCase,
        ToolId::Chmod => Icon::ToolChmod,
        ToolId::DateTime => Icon::ToolDateTime,
        ToolId::Uuid => Icon::ToolUuid,
        ToolId::Ulid => Icon::ToolUlid,
        ToolId::Crontab => Icon::ToolCrontab,
        ToolId::SshConfig => Icon::ToolSshConfig,
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
    /// The JSON formatter's output.
    JsonOutput,
    /// The regular expression tester's matches.
    RegexMatches,
    /// The text comparison's unified diff.
    DiffUnified,
    /// The text case converter's output.
    TextCaseOutput,
    /// A value of the subnet calculator.
    Subnet(SubnetField),
    /// A form of the IP converter.
    IpConverter(IpField),
    /// The network calculator's result.
    NetCalcResult,
    /// The chmod command.
    ChmodCommand,
    /// The octal mode.
    ChmodOctal,
    /// The `rwx` form.
    ChmodSymbolic,
    /// A form of the date and time converter.
    DateTime(DateField),
    /// The ULID generated.
    UlidSingle,
    /// The batch of ULIDs.
    UlidBatch,
    /// The cron expression.
    CrontabExpression,
    /// The SSH config block.
    SshConfigOutput,
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
    /// The JSON formatter's.
    Json(JsonMessage),
    /// The regular expression tester's.
    Regex(RegexMessage),
    /// The text comparison's.
    Diff(DiffMessage),
    /// The text case converter's.
    TextCase(TextCaseMessage),
    /// The subnet calculator's.
    Subnet(SubnetMessage),
    /// The IP converter's.
    IpConverter(IpConverterMessage),
    /// The network calculator's.
    NetCalc(NetCalcMessage),
    /// The chmod calculator's.
    Chmod(ChmodMessage),
    /// The date and time converter's.
    DateTime(DateTimeMessage),
    /// The ULID generator's.
    Ulid(UlidMessage),
    /// The crontab builder's.
    Crontab(CrontabMessage),
    /// The SSH config generator's.
    SshConfig(SshConfigMessage),
}

/// A tool's own state.
#[derive(Debug)]
enum Pane {
    Base64(base64_tool::Base64Pane),
    Url(url_tool::UrlPane),
    Uuid(uuid_tool::UuidPane),
    Json(json_tool::JsonPane),
    Regex(regex_tool::RegexPane),
    Diff(diff_tool::DiffPane),
    TextCase(text_case_tool::TextCasePane),
    Subnet(subnet_tool::SubnetPane),
    IpConverter(ip_converter_tool::IpConverterPane),
    NetCalc(network_calculator_tool::NetCalcPane),
    Chmod(chmod_tool::ChmodPane),
    DateTime(Box<datetime_tool::DateTimePane>),
    Ulid(ulid_tool::UlidPane),
    Crontab(crontab_tool::CrontabPane),
    SshConfig(ssh_config_tool::SshConfigPane),
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
            ToolId::JsonFormatter => Pane::Json(json_tool::JsonPane::default()),
            ToolId::RegexTester => Pane::Regex(regex_tool::RegexPane::default()),
            ToolId::TextDiff => Pane::Diff(diff_tool::DiffPane::default()),
            ToolId::TextCase => Pane::TextCase(text_case_tool::TextCasePane::default()),
            ToolId::SubnetCalculator => Pane::Subnet(subnet_tool::SubnetPane::default()),
            ToolId::IpConverter => Pane::IpConverter(ip_converter_tool::IpConverterPane::default()),
            ToolId::NetworkCalculator => {
                Pane::NetCalc(network_calculator_tool::NetCalcPane::default())
            }
            ToolId::Chmod => Pane::Chmod(chmod_tool::ChmodPane::default()),
            ToolId::DateTime => Pane::DateTime(Box::new(datetime_tool::DateTimePane::new())),
            ToolId::Ulid => Pane::Ulid(ulid_tool::UlidPane::new()),
            ToolId::Crontab => Pane::Crontab(crontab_tool::CrontabPane::new()),
            ToolId::SshConfig => Pane::SshConfig(ssh_config_tool::SshConfigPane::default()),
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
            Task::perform(wait(COPY_FEEDBACK), move |()| {
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
            (ToolMessage::Json(message), Pane::Json(pane)) => {
                return match pane.update(message) {
                    json_tool::Outcome::Copy(content) => {
                        state.copy(tab, CopySlot::JsonOutput, content)
                    }
                    outcome => outcome.task(tab),
                };
            }
            (ToolMessage::Regex(message), Pane::Regex(pane)) => {
                return match pane.update(message) {
                    regex_tool::Outcome::Copy(content) => {
                        state.copy(tab, CopySlot::RegexMatches, content)
                    }
                    outcome => outcome.task(tab),
                };
            }
            (ToolMessage::Diff(message), Pane::Diff(pane)) => {
                return match pane.update(message) {
                    diff_tool::Outcome::Copy(content) => {
                        state.copy(tab, CopySlot::DiffUnified, content)
                    }
                    outcome => outcome.task(tab),
                };
            }
            (ToolMessage::TextCase(message), Pane::TextCase(pane)) => {
                if let Some((slot, content)) = pane.update(message) {
                    return state.copy(tab, slot, content);
                }
            }
            (ToolMessage::DateTime(message), Pane::DateTime(pane)) => {
                return match pane.update(message) {
                    datetime_tool::Outcome::Copy(slot, content) => state.copy(tab, slot, content),
                    outcome => outcome.task(tab),
                };
            }
            (message, pane) => {
                if let Some((slot, content)) = update_copying(message, pane) {
                    return state.copy(tab, slot, content);
                }
            }
        }
        Task::none()
    }
}

/// Applies `message` to `pane`, a tool whose update only ever copies; what a copy button
/// copies, when one is pressed. A message of another tool's, its tab closed and another in
/// its place, does nothing.
fn update_copying(message: ToolMessage, pane: &mut Pane) -> Option<(CopySlot, String)> {
    match (message, pane) {
        (ToolMessage::Subnet(message), Pane::Subnet(pane)) => pane.update(message),
        (ToolMessage::IpConverter(message), Pane::IpConverter(pane)) => pane.update(message),
        (ToolMessage::NetCalc(message), Pane::NetCalc(pane)) => pane.update(message),
        (ToolMessage::Chmod(message), Pane::Chmod(pane)) => pane.update(message),
        (ToolMessage::Ulid(message), Pane::Ulid(pane)) => pane.update(message),
        (ToolMessage::Crontab(message), Pane::Crontab(pane)) => pane.update(message),
        (ToolMessage::SshConfig(message), Pane::SshConfig(pane)) => pane.update(message),
        _ => None,
    }
}

/// The colours a tool marks its text with, read from the theme: the regular expression
/// tester's matches, as the C# `RenderHighlight`, and the text comparison's lines and words,
/// as the C# `BuildDerivedBrushes`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Marks {
    /// The text, as the C# `TextPrimaryBrush`.
    pub text: Color,
    /// The secondary text, as the C# `TextSecondaryBrush`.
    pub secondary: Color,
    /// Behind a match: the accent, faint.
    pub regex_match: Color,
    /// Behind a match with a named group: the warning colour, faint.
    pub regex_group: Color,
    /// A removed line's prefix: the error colour.
    pub removed: Color,
    /// An added line's prefix: the success colour.
    pub added: Color,
    /// Behind a removed line.
    pub removed_line: Color,
    /// Behind an added line.
    pub added_line: Color,
    /// Behind a removed word.
    pub removed_word: Color,
    /// Behind an added word.
    pub added_word: Color,
}

impl Marks {
    /// The marks of `theme`.
    #[must_use]
    pub fn of(theme: &Theme) -> Self {
        let palette = theme.extended_palette();
        let removed = palette.danger.base.color;
        let added = palette.success.base.color;
        Self {
            text: palette.background.base.text,
            secondary: palette.secondary.base.color,
            regex_match: palette.primary.base.color.scale_alpha(REGEX_MATCH_ALPHA),
            regex_group: palette.warning.base.color.scale_alpha(REGEX_GROUP_ALPHA),
            removed,
            added,
            removed_line: removed.scale_alpha(DIFF_LINE_ALPHA),
            added_line: added.scale_alpha(DIFF_LINE_ALPHA),
            removed_word: removed.scale_alpha(DIFF_WORD_ALPHA),
            added_word: added.scale_alpha(DIFF_WORD_ALPHA),
        }
    }
}

/// What tool tab `tab`, showing `tool`, draws in `theme`.
#[must_use]
pub fn view<'a>(
    tab: TabId,
    tool: ToolId,
    state: Option<&'a ToolPane>,
    theme: &Theme,
) -> Element<'a, Message> {
    let Some(state) = state else {
        return column![].into();
    };
    let marks = Marks::of(theme);
    let (title, help) = match tool {
        ToolId::Base64 => (fl!("ui-tool-base64-title"), fl!("ui-tool-base64-help")),
        ToolId::UrlEncoder => (fl!("ui-tool-urlenc-title"), fl!("ui-tool-urlenc-help")),
        ToolId::JsonFormatter => (fl!("ui-tool-json-title"), fl!("ui-tool-json-help")),
        ToolId::RegexTester => (fl!("ui-tool-regex-title"), fl!("ui-tool-regex-help")),
        ToolId::TextDiff => (fl!("ui-tool-diff-title"), fl!("ui-tool-diff-help")),
        ToolId::TextCase => (fl!("ui-tool-textcase-title"), fl!("ui-tool-textcase-help")),
        ToolId::Uuid => (fl!("ui-tool-uuid-title"), fl!("ui-tool-uuid-help")),
        ToolId::SubnetCalculator => (fl!("ui-tool-subnet-title"), fl!("ui-tool-subnet-help")),
        ToolId::IpConverter => (fl!("ui-tool-ipconv-title"), fl!("ui-tool-ipconv-help")),
        ToolId::NetworkCalculator => (fl!("ui-tool-netcalc-title"), fl!("ui-tool-netcalc-help")),
        ToolId::Chmod => (fl!("ui-tool-chmod-title"), fl!("ui-tool-chmod-help")),
        ToolId::DateTime => (fl!("ui-tool-datetime-title"), fl!("ui-tool-datetime-help")),
        ToolId::Ulid => (fl!("ui-tool-ulid-title"), fl!("ui-tool-ulid-help")),
        ToolId::Crontab => (fl!("ui-tool-crontab-title"), fl!("ui-tool-crontab-help")),
        ToolId::SshConfig => (
            fl!("ui-tool-sshconfig-title"),
            fl!("ui-tool-sshconfig-help"),
        ),
    };
    let (body, actions) = match &state.pane {
        Pane::Base64(pane) => (pane.view(tab, state), None),
        Pane::Url(pane) => (pane.view(tab, state), None),
        Pane::Uuid(pane) => (pane.view(tab, state), None),
        Pane::Json(pane) => (pane.view(tab, state), None),
        Pane::Regex(pane) => (pane.view(tab, state, marks), None),
        Pane::Diff(pane) => (pane.view(tab, state, marks), Some(pane.header_actions(tab))),
        Pane::TextCase(pane) => (pane.view(tab, state), None),
        Pane::Subnet(pane) => (pane.view(tab, state), None),
        Pane::IpConverter(pane) => (pane.view(tab, state), None),
        Pane::NetCalc(pane) => (pane.view(tab, state), None),
        Pane::Chmod(pane) => (pane.view(tab, state), None),
        Pane::DateTime(pane) => (pane.view(tab, state), None),
        Pane::Ulid(pane) => (pane.view(tab, state), None),
        Pane::Crontab(pane) => (pane.view(tab, state), None),
        Pane::SshConfig(pane) => (pane.view(tab, state), None),
    };
    column![
        header(tab, title, actions),
        state.help.then(|| help_panel(tab, help))
    ]
    .push(body)
    .width(Length::Fill)
    .height(Length::Fill)
    .into()
}

/// A tool's header, as the C# tool views': its title, what the tool puts there, and the "?"
/// button at its right.
fn header(
    tab: TabId,
    title: String,
    actions: Option<Element<'_, Message>>,
) -> Element<'_, Message> {
    container(
        row![
            text(title)
                .size(font_size::SUBTITLE)
                .font(styles::SEMIBOLD)
                .width(Length::Fill),
        ]
        .push(actions)
        .push(
            tooltip(
                button(
                    container(
                        text(HELP_GLYPH)
                            .size(font_size::BODY)
                            .font(styles::SEMIBOLD),
                    )
                    .center(Length::Fill),
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
        )
        .spacing(spacing::SM)
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

/// A label over a tool's box, as the C# labels in the secondary text.
fn field_label<'a>(label: String) -> Element<'a, Message> {
    text(label)
        .size(font_size::BODY)
        .style(text::secondary)
        .into()
}

/// What a tool shows before it has a result, as the C# `ToolEmptyStateStyle`: its hint,
/// centred in the room left.
fn empty_state<'a>(hint: String) -> Element<'a, Message> {
    container(
        text(hint)
            .size(font_size::BODY_LARGE)
            .style(text::secondary),
    )
    .padding(EMPTY_STATE_PADDING)
    .center(Length::Fill)
    .into()
}

/// A tool's status line: what it says, in the error colour when it is an error, as the C#
/// `StatusForegroundBrushKey`; nothing when it says nothing.
fn status_line<'a>(said: Option<(String, bool)>) -> Element<'a, Message> {
    match said {
        Some((said, error)) => text(said)
            .size(font_size::CAPTION)
            .style(if error { text::danger } else { text::secondary })
            .into(),
        None => column![].into(),
    }
}

/// A tool's content that fills its tab, as the C# tool grids under the header.
fn tool_body(content: Column<'_, Message>) -> Element<'_, Message> {
    container(content)
        .padding(BODY_PADDING)
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
}

/// A wait of `duration`, its timer made when it is first awaited: in the runtime that runs
/// it, not in the update that asks for it.
async fn wait(duration: Duration) {
    tokio::time::sleep(duration).await;
}

/// Width of a result's label, as the C# results grids' label column.
const RESULT_LABEL_WIDTH: f32 = 160.0;

/// Room between a result's label and its value, as the C# grids' column of 16.
const RESULT_LABEL_GAP: f32 = 16.0;

/// What a tool that scrolls shows before it has a result, as the C# `ToolEmptyStateStyle`:
/// its hint, centred across; a page that scrolls has no height to fill.
fn hint<'a>(hint: String) -> Element<'a, Message> {
    container(
        text(hint)
            .size(font_size::BODY_LARGE)
            .style(text::secondary),
    )
    .padding(EMPTY_STATE_PADDING)
    .center_x(Length::Fill)
    .into()
}

/// A tool's error under its input, in the error colour, as the C# `TxtError` lines.
fn error_line<'a>(said: String) -> Element<'a, Message> {
    status_line(Some((said, true)))
}

/// A value a tool shows, as the C# read-only boxes: monospaced, in the accent, framed.
fn value_box<'a>(value: String) -> Element<'a, Message> {
    container(
        text(value)
            .font(BOX_FONT)
            .size(font_size::BODY_LARGE)
            .style(|theme: &iced::Theme| text::Style {
                color: Some(theme.palette().primary),
            }),
    )
    .padding(INPUT_PADDING)
    .width(Length::Fill)
    .style(styles::field_box)
    .into()
}

/// A row of a tool's results, as the C# results grids: its label, its value monospaced,
/// and its copy button.
fn result_row(label: String, value: String, copy: Element<'_, Message>) -> Element<'_, Message> {
    row![
        text(label)
            .size(font_size::BODY)
            .style(text::secondary)
            .width(RESULT_LABEL_WIDTH),
        iced::widget::space().width(RESULT_LABEL_GAP),
        text(value)
            .font(BOX_FONT)
            .size(font_size::BODY)
            .width(Length::Fill),
        copy,
    ]
    .align_y(iced::Alignment::Center)
    .into()
}

/// A tool's results, one a row, in a card, as the C# results panels.
fn results_card<'a>(rows: impl IntoIterator<Item = Element<'a, Message>>) -> Element<'a, Message> {
    container(Column::with_children(rows).spacing(spacing::SM))
        .padding(spacing::MD)
        .width(Length::Fill)
        .style(styles::card)
        .into()
}
