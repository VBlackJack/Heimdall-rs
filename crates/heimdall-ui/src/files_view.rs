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

//! The Files tab: this computer on the left, the server on the right, transfers below.
//!
//! A click selects an entry and a second click on a selected folder opens it; the buttons
//! between the panes send the selection, a file or a folder, to the other side.
//!
//! The keyboard acts on the pane with the focus, drawn with a stronger border: arrows, Home
//! and End select, Enter opens or sends, Backspace goes back, Tab or Left and Right change
//! pane, F2 renames, Delete deletes and F5 lists again. Tab and Enter reach the tab through
//! the window, as they act on a dialog first.

use std::collections::BTreeSet;
use std::time::SystemTime;

use heimdall_app::external_edit::EditSession;
use heimdall_app::files::{
    Direction, EntryKind, FileProperties, FilesError, FilesKey, FilesPane, Listed, Side, Sort,
    SortColumn, Special, Transfer, TransferState, local_segments, remote_segments, symbolic_mode,
};
use heimdall_app::local_properties::LocalProperties;
use heimdall_app::{FilesMessage, Message as AppMessage, TabId};
use heimdall_core::utc::UtcTime;
use heimdall_files::Refusal;
use iced::keyboard::{self, Modifiers, key::Named};
use iced::widget::Id;
use iced::widget::{
    Column, Row, button, center, column, container, mouse_area, responsive, row, scrollable, text,
    text_input, tooltip,
};
use iced::{Alignment, Element, Font, Length, Theme};

use crate::i18n::fl;
use crate::icons::{self, Icon, Tint};
use crate::shell::Message;
use crate::styles;
use crate::texts;
use crate::tokens::{BORDER_WIDTH, OPACITY_DISABLED, font_size, spacing};
use crate::tree_row::{Mark, RowChrome};
use crate::tree_view::TreeMenu;

/// Width of the size column until resized, in logical pixels.
const SIZE_WIDTH: f32 = 90.0;

/// Room inside the transport's notice, above and below then beside, as the C# badge's.
const NOTICE_PADDING: [f32; 2] = [1.0, 6.0];

/// Width of the modification time column until resized, in logical pixels.
const MODIFIED_WIDTH: f32 = 130.0;

/// Width of the permissions column until resized, in logical pixels.
const PERMISSIONS_WIDTH: f32 = 90.0;

/// Width of the owner column until resized, in logical pixels.
const OWNER_WIDTH: f32 = 55.0;

/// Narrowest the size column is resized to, in logical pixels.
const SIZE_MIN_WIDTH: f32 = 40.0;

/// Narrowest the modification time column is resized to, in logical pixels.
const MODIFIED_MIN_WIDTH: f32 = 60.0;

/// Narrowest the permissions column is resized to, in logical pixels.
const PERMISSIONS_MIN_WIDTH: f32 = 40.0;

/// Narrowest the owner column is resized to, in logical pixels.
const OWNER_MIN_WIDTH: f32 = 30.0;

/// Room left and right of an entry's cells, inside its row, and of the headers above them.
const ROW_PADDING_X: f32 = 10.0;

/// Room above and below an entry's cells, inside its row.
const ROW_PADDING_Y: f32 = 5.0;

/// How many of a pane's entries a double click on a separator measures, from the first: a
/// folder of tens of thousands is not measured whole for a click.
const FIT_ROWS: usize = 1_000;

/// Width of a Properties dialog's labels, in logical pixels.
const PROPERTY_LABEL_WIDTH: f32 = 100.0;

/// The columns of the server's pane, as the C# Files tab's.
const REMOTE_COLUMNS: &[SortColumn] = &[
    SortColumn::Name,
    SortColumn::Size,
    SortColumn::Modified,
    SortColumn::Permissions,
    SortColumn::Owner,
];

/// The columns of this computer's pane.
const LOCAL_COLUMNS: &[SortColumn] = &[SortColumn::Name, SortColumn::Size, SortColumn::Modified];

/// Tallest the transfer list grows before it scrolls, in logical pixels.
const TRANSFERS_HEIGHT: f32 = 160.0;

/// Width of the border of the pane with the focus, in logical pixels.
const FOCUS_BORDER_WIDTH: f32 = 2.0;

/// Font of a path, as the C# `FontFamilyMonospace`: the terminal's, always embedded.
const PATH_FONT: Font = Font::with_name(crate::terminal_view::FONT_FAMILY);

/// Room around the breadcrumb's folders, above and below then beside, as the C# path box's.
const BREADCRUMB_PADDING: [f32; 2] = [1.0, 4.0];

/// Room around a folder of the breadcrumb, as the C# `SftpIconButtonPadding`.
const SEGMENT_PADDING: [f32; 2] = [2.0, 6.0];

/// Side of the chevron between two folders of the breadcrumb, as the C#'s at
/// `FontSizeSmallCaption`.
const CHEVRON_SIDE: f32 = 8.0;

/// Side of a toolbar's square button, as the C# `SftpToolbarButtonMinHeight`.
const TOOL_SIDE: f32 = 28.0;

/// Width of the filter, as the C# `SftpFilterBoxWidth`.
const FILTER_WIDTH: f32 = 160.0;

/// Width of a toolbar's separator, as the C# `ToolbarVerticalSeparatorStyle`'s.
const SEPARATOR_WIDTH: f32 = 1.5;

/// Height of a toolbar's separator, as the C#'s.
const SEPARATOR_HEIGHT: f32 = 16.0;

/// Side of an entry's icon before its name, as the C# glyph at `FontSizeBodyLarge`.
const ENTRY_ICON_SIDE: f32 = 14.0;

/// Between an entry's icon and its name, as the C# icon's margin.
const ENTRY_ICON_GAP: f32 = 6.0;

/// Room around the pane's footer, above and below then beside, as the C#
/// `SessionHeaderPadding`.
const FOOTER_PADDING: [f32; 2] = [4.0, 8.0];

/// A field of a pane the keyboard can be given to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PaneField {
    /// The path bar, Alt+D and F4.
    Path,
    /// The filter, Ctrl+F.
    Filter,
}

/// Widget identifier of the `field` of `tab`'s pane `side`. Made of the tab: an operation
/// reaches every window, and a Files tab in a window of its own has the same fields as one
/// in the main window.
#[must_use]
pub fn field_id(tab: TabId, side: Side, field: PaneField) -> Id {
    let field = match field {
        PaneField::Path => "path",
        PaneField::Filter => "filter",
    };
    Id::from(format!("files-{}-{}-{field}", tab.value(), side_name(side)))
}

/// A button of a pane that shows a glyph alone, as the C# toolbar's and path bar's.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PaneTool {
    /// Back, Alt+Left.
    Back,
    /// The parent folder, Alt+Up.
    Up,
    /// The home folder.
    Home,
    /// The folder listed again, F5.
    Refresh,
    /// The path typed gone to, Enter.
    Go,
    /// The server's bookmarks.
    Bookmarks,
}

/// Widget identifier of the button `tool` of `tab`'s pane `side`, which shows a glyph
/// alone; made of the tab as its fields are.
#[must_use]
pub fn tool_id(tab: TabId, side: Side, tool: PaneTool) -> Id {
    let tool = match tool {
        PaneTool::Back => "back",
        PaneTool::Up => "up",
        PaneTool::Home => "home",
        PaneTool::Refresh => "refresh",
        PaneTool::Go => "go",
        PaneTool::Bookmarks => "bookmarks",
    };
    Id::from(format!("files-{}-{}-{tool}", tab.value(), side_name(side)))
}

/// Widget identifier of the list of `tab`'s pane `side`, to scroll the selection into view;
/// made of the tab as its fields are.
#[must_use]
pub fn list_id(tab: TabId, side: Side) -> Id {
    Id::from(format!("files-{}-{}", tab.value(), side_name(side)))
}

/// A pane's side in its widgets' identifiers.
fn side_name(side: Side) -> &'static str {
    match side {
        Side::Local => "local",
        Side::Remote => "remote",
    }
}

/// What `key` at `physical` does in a Files tab, as the C# `FileBrowserShortcutPolicy`:
/// the clipboard on Ctrl+X, C and V, Ctrl+A, the path on Ctrl+Shift+C, the transfers on
/// Ctrl+Shift+D and U, Back and Up on Alt+Left and Alt+Up, the path bar on Alt+D and F4, a
/// new folder on F7, whatever the keyboard's layout. Enter and Tab reach the tab through the
/// window, as they act on a dialog first; the logo key is the system's.
#[must_use]
pub fn files_key(
    key: &keyboard::Key,
    physical: keyboard::key::Physical,
    modifiers: Modifiers,
) -> Option<FilesKey> {
    let (ctrl, shift, alt) = (modifiers.control(), modifiers.shift(), modifiers.alt());
    if modifiers.logo() || (ctrl && alt) {
        return None;
    }
    let letter = crate::terminal_view::keys::letter(key, physical);
    if ctrl {
        return match (shift, letter) {
            (false, Some('x')) => Some(FilesKey::Cut),
            (false, Some('c')) => Some(FilesKey::Copy),
            (false, Some('v')) => Some(FilesKey::Paste),
            (false, Some('a')) => Some(FilesKey::SelectAll),
            (false, _) if *key == keyboard::Key::Named(Named::Space) => Some(FilesKey::ToggleMark),
            (true, Some('c')) => Some(FilesKey::CopyPath),
            (true, Some('d')) => Some(FilesKey::Download),
            (true, Some('u')) => Some(FilesKey::Upload),
            _ => None,
        };
    }
    if alt {
        return match key {
            _ if shift => None,
            keyboard::Key::Named(Named::ArrowLeft) => Some(FilesKey::Back),
            keyboard::Key::Named(Named::ArrowUp) => Some(FilesKey::Parent),
            keyboard::Key::Named(Named::ArrowDown) => Some(FilesKey::Lower),
            _ if letter == Some('d') => Some(FilesKey::FocusPath),
            _ => None,
        };
    }
    let keyboard::Key::Named(named) = key else {
        return None;
    };
    if shift {
        return match named {
            Named::ArrowUp => Some(FilesKey::ExtendPrevious),
            Named::ArrowDown => Some(FilesKey::ExtendNext),
            _ => None,
        };
    }
    Some(match named {
        Named::ArrowUp => FilesKey::Previous,
        Named::ArrowDown => FilesKey::Next,
        Named::Home => FilesKey::First,
        Named::End => FilesKey::Last,
        // Back, as the C# Files tab's Backspace; Up stays a button.
        Named::Backspace => FilesKey::Back,
        Named::ArrowLeft => FilesKey::Focus(Side::Local),
        Named::ArrowRight => FilesKey::Focus(Side::Remote),
        Named::F2 => FilesKey::Rename,
        Named::F4 => FilesKey::FocusPath,
        Named::F7 => FilesKey::NewFolder,
        Named::Delete => FilesKey::Delete,
        Named::F5 => FilesKey::Refresh,
        _ => return None,
    })
}

fn files(message: FilesMessage) -> Message {
    Message::App(AppMessage::Files(message))
}

/// Narrowest a name is left before a column gives way, or is resized to, in logical pixels.
const NAME_MIN_WIDTH: f32 = 160.0;

/// The widths of a pane's columns besides the name, which takes the rest: dragged on their
/// header, kept for the session, as the C# list keeps them for the view's life.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ColumnWidths {
    size: f32,
    modified: f32,
    permissions: f32,
    owner: f32,
}

impl Default for ColumnWidths {
    fn default() -> Self {
        Self {
            size: SIZE_WIDTH,
            modified: MODIFIED_WIDTH,
            permissions: PERMISSIONS_WIDTH,
            owner: OWNER_WIDTH,
        }
    }
}

impl ColumnWidths {
    /// The width of `column`; none for the name, which takes the rest.
    fn of(self, column: SortColumn) -> Option<f32> {
        match column {
            SortColumn::Name => None,
            SortColumn::Size => Some(self.size),
            SortColumn::Modified => Some(self.modified),
            SortColumn::Permissions => Some(self.permissions),
            SortColumn::Owner => Some(self.owner),
        }
    }

    /// These widths, those of `columns` laid out at `laid`; the name's is the rest, left out.
    fn resized(mut self, columns: &[SortColumn], laid: &[f32]) -> Self {
        for (column, width) in columns.iter().zip(laid.iter().copied()) {
            match column {
                SortColumn::Name => {}
                SortColumn::Size => self.size = width,
                SortColumn::Modified => self.modified = width,
                SortColumn::Permissions => self.permissions = width,
                SortColumn::Owner => self.owner = width,
            }
        }
        self
    }
}

/// The column widths of a Files tab's two panes.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct TabColumns {
    /// This computer's pane.
    pub local: ColumnWidths,
    /// The server's pane.
    pub remote: ColumnWidths,
}

impl TabColumns {
    /// These widths, `side`'s pane's set to `widths`.
    #[must_use]
    pub fn with(mut self, side: Side, widths: ColumnWidths) -> Self {
        match side {
            Side::Local => self.local = widths,
            Side::Remote => self.remote = widths,
        }
        self
    }
}

/// The narrowest `column` is resized to.
fn column_min_width(column: SortColumn) -> f32 {
    match column {
        SortColumn::Name => NAME_MIN_WIDTH,
        SortColumn::Size => SIZE_MIN_WIDTH,
        SortColumn::Modified => MODIFIED_MIN_WIDTH,
        SortColumn::Permissions => PERMISSIONS_MIN_WIDTH,
        SortColumn::Owner => OWNER_MIN_WIDTH,
    }
}

/// The first of `columns` that fit in `width` at `widths` beside a name at least
/// [`NAME_MIN_WIDTH`] wide: the last ones give way first, the name and the size never.
fn fitting_columns(columns: &[SortColumn], widths: ColumnWidths, width: f32) -> Vec<SortColumn> {
    let mut shown = columns.to_vec();
    let needed = |shown: &[SortColumn]| {
        shown
            .iter()
            .map(|column| {
                widths
                    .of(*column)
                    .map_or(NAME_MIN_WIDTH, |fixed| fixed + spacing::SM)
            })
            .sum::<f32>()
    };
    while shown.len() > 2 && needed(&shown) > width {
        shown.pop();
    }
    shown
}

/// The widths `shown` are laid out at in `room`: each its own, the name the rest.
fn laid_widths(shown: &[SortColumn], widths: ColumnWidths, room: f32) -> Vec<f32> {
    let fixed: f32 = shown.iter().filter_map(|column| widths.of(*column)).sum();
    let gaps: f32 = shown.iter().skip(1).map(|_| spacing::SM).sum();
    let name = (room - fixed - gaps).max(0.0);
    shown
        .iter()
        .map(|column| widths.of(*column).unwrap_or(name))
        .collect()
}

/// The size of `column`'s cells; none for the name's, at the window's size.
fn cell_size(column: SortColumn) -> Option<f32> {
    (column != SortColumn::Name).then_some(font_size::CAPTION)
}

/// What `kind` of entry it is, as the C# Properties dialog names it.
fn kind_name(kind: EntryKind) -> String {
    match kind {
        EntryKind::File => fl!("ui-files-type-file"),
        EntryKind::Directory => fl!("ui-files-type-directory"),
        EntryKind::Link => fl!("ui-files-type-link"),
        EntryKind::Other(Special::Pipe) => fl!("ui-files-type-pipe"),
        EntryKind::Other(Special::Socket) => fl!("ui-files-type-socket"),
        EntryKind::Other(Special::Device) => fl!("ui-files-type-device"),
        EntryKind::Other(Special::Unknown) => fl!("ui-files-type-other"),
    }
}

/// `line` with what `entry` is, as the C# row's tooltip: its name, its type, its size, when
/// it changed to the second, and its permissions.
fn row_tooltip<'a, E: Listed>(
    line: iced::widget::Button<'a, Message>,
    entry: &E,
) -> Element<'a, Message> {
    let mut lines = vec![
        entry.label().to_owned(),
        fl!("ui-files-tooltip-type", kind = kind_name(entry.kind())),
    ];
    if let (EntryKind::File, Some(bytes)) = (entry.kind(), entry.size()) {
        lines.push(fl!("ui-files-tooltip-size", size = texts::size(bytes)));
    }
    if let Some(time) = entry.modified() {
        lines.push(fl!("ui-files-tooltip-modified", at = modified_long(time)));
    }
    if let Some(mode) = entry.permissions() {
        lines.push(fl!(
            "ui-files-tooltip-permissions",
            permissions = symbolic_mode(mode)
        ));
    }
    tooltip(
        line,
        text(lines.join("\n")).size(font_size::CAPTION),
        tooltip::Position::FollowCursor,
    )
    .delay(ROW_TOOLTIP_DELAY)
    .style(container::rounded_box)
    .into()
}

/// How long the pointer rests on a row before its tooltip shows: a list scanned with the
/// pointer stays clear.
const ROW_TOOLTIP_DELAY: std::time::Duration = std::time::Duration::from_millis(600);

/// `time` to the second, in UTC, as the C# tooltip's long date.
fn modified_long(time: SystemTime) -> String {
    let at = UtcTime::of(time);
    fl!(
        "ui-files-tooltip-time",
        day = format!("{:04}-{:02}-{:02}", at.year, at.month, at.day),
        time = format!("{:02}:{:02}:{:02}", at.hour, at.minute, at.second)
    )
}

/// What an entry of the server is, as the C# Properties dialog shows it, with `ok` to
/// close it.
pub fn properties<'a>(
    properties: &FileProperties,
    ok: iced::widget::Button<'a, Message>,
) -> Element<'a, Message> {
    let kind = kind_name(properties.kind);
    let number = |value: Option<u32>| value.map(|n| n.to_string()).unwrap_or_default();
    let lines = [
        (fl!("ui-files-properties-name"), properties.name.clone()),
        (fl!("ui-files-properties-type"), kind),
        (
            fl!("ui-files-properties-size"),
            properties.size.map(texts::size).unwrap_or_default(),
        ),
        (
            fl!("ui-files-properties-modified"),
            properties.modified.map(modified_text).unwrap_or_default(),
        ),
        (
            fl!("ui-files-properties-permissions"),
            properties
                .permissions
                .map(|mode| format!("{} ({mode:o})", symbolic_mode(mode)))
                .unwrap_or_default(),
        ),
        (fl!("ui-files-properties-owner"), number(properties.owner)),
        (fl!("ui-files-properties-group"), number(properties.group)),
        (fl!("ui-files-properties-path"), properties.path.clone()),
    ];
    property_lines(&properties.name, lines, ok)
}

/// What an entry of the local file browser is, as the C# local Properties shows it: its
/// name, kind, size, when it was created, modified and last read, its attributes, where a
/// link points and its path; with `ok` to close it.
pub fn local_properties<'a>(
    properties: &LocalProperties,
    ok: iced::widget::Button<'a, Message>,
) -> Element<'a, Message> {
    let time = |time: Option<SystemTime>| time.map(modified_long).unwrap_or_default();
    let mut attributes = Vec::new();
    if properties.read_only {
        attributes.push(fl!("ui-files-attribute-read-only"));
    }
    if properties.hidden == Some(true) {
        attributes.push(fl!("ui-files-attribute-hidden"));
    }
    if attributes.is_empty() {
        attributes.push(fl!("ui-files-attribute-normal"));
    }
    let mut lines = vec![
        (fl!("ui-files-properties-name"), properties.name.clone()),
        (fl!("ui-files-properties-type"), kind_name(properties.kind)),
        (
            fl!("ui-files-properties-size"),
            properties.size.map(texts::size).unwrap_or_default(),
        ),
        (fl!("ui-files-properties-created"), time(properties.created)),
        (
            fl!("ui-files-properties-modified"),
            time(properties.modified),
        ),
        (
            fl!("ui-files-properties-accessed"),
            time(properties.accessed),
        ),
        (fl!("ui-files-properties-attributes"), attributes.join(", ")),
    ];
    if let Some(target) = &properties.link_target {
        lines.push((fl!("ui-files-properties-link-target"), target.clone()));
    }
    lines.push((fl!("ui-files-properties-path"), properties.path.clone()));
    property_lines(&properties.name, lines, ok)
}

/// A Properties dialog: its title naming `name`, then each of `lines`, a label and its
/// value, then `ok`.
fn property_lines<'a>(
    name: &str,
    lines: impl IntoIterator<Item = (String, String)>,
    ok: iced::widget::Button<'a, Message>,
) -> Element<'a, Message> {
    let mut content =
        column![text(fl!("ui-files-properties-title", name = name)).size(font_size::SUBTITLE)]
            .spacing(spacing::SM);
    for (label, value) in lines {
        content = content.push(
            row![
                text(label)
                    .size(font_size::CAPTION)
                    .width(PROPERTY_LABEL_WIDTH),
                text(value)
            ]
            .spacing(spacing::SM),
        );
    }
    content.push(ok).into()
}

/// `time` as the C# column shows it, `2026-09-27 21:05`, in UTC.
pub(crate) fn modified_text(time: SystemTime) -> String {
    let at = UtcTime::of(time);
    format!(
        "{:04}-{:02}-{:02} {:02}:{:02}",
        at.year, at.month, at.day, at.hour, at.minute
    )
}

/// What `entry` shows in `column`.
fn cell_text<E: Listed>(entry: &E, column: SortColumn) -> String {
    match column {
        SortColumn::Name => {
            match entry.kind() {
                // A folder and a link told apart by their icons, as the C#'s.
                EntryKind::Directory | EntryKind::Link | EntryKind::File => {
                    entry.label().to_owned()
                }
                // A pipe, a socket, a device marked, its icon shared.
                special @ EntryKind::Other(_) => fl!(
                    "ui-files-special-mark",
                    name = entry.label(),
                    kind = kind_name(special)
                ),
            }
        }
        SortColumn::Size => match (entry.kind(), entry.size()) {
            (EntryKind::File, Some(bytes)) => texts::size(bytes),
            _ => String::new(),
        },
        SortColumn::Modified => entry.modified().map(modified_text).unwrap_or_default(),
        SortColumn::Permissions => entry.permissions().map(symbolic_mode).unwrap_or_default(),
        SortColumn::Owner => entry.owner().map(|uid| uid.to_string()).unwrap_or_default(),
    }
}

/// The header of `column`, marked when the pane is sorted by it.
fn column_title(column: SortColumn, sort: Sort) -> String {
    let title = match column {
        SortColumn::Name => fl!("ui-files-column-name"),
        SortColumn::Size => fl!("ui-files-column-size"),
        SortColumn::Modified => fl!("ui-files-column-modified"),
        SortColumn::Permissions => fl!("ui-files-column-permissions"),
        SortColumn::Owner => fl!("ui-files-column-owner"),
    };
    match (sort.column == column, sort.descending) {
        (false, _) => title,
        (true, false) => fl!("ui-files-sorted-ascending", column = title),
        (true, true) => fl!("ui-files-sorted-descending", column = title),
    }
}

/// What `column` shows, its header first, each with its size: what a double click on its
/// separator fits it to, from the first [`FIT_ROWS`] entries.
fn column_texts<E: Listed>(
    entries: &[E],
    column: SortColumn,
    sort: Sort,
) -> Vec<(String, Option<f32>)> {
    std::iter::once((column_title(column, sort), Some(font_size::CAPTION)))
        .chain(
            entries
                .iter()
                .take(FIT_ROWS)
                .map(|entry| (cell_text(entry, column), cell_size(column))),
        )
        .collect()
}

/// The headers of the columns `shown`, laid out at `laid` as the entries below: a click
/// sorts by one, a separator between two dragged resizes them, a double click on it fits
/// the column on its left to `entries`.
fn headers<'a, E: Listed>(
    (tab, side): (TabId, Side),
    entries: &'a [E],
    (shown, laid): (&[SortColumn], &[f32]),
    (sort, widths): (Sort, ColumnWidths),
) -> Element<'a, Message> {
    let cells = shown
        .iter()
        .zip(laid.iter().copied())
        .map(|(column, width)| {
            button(
                text(column_title(*column, sort))
                    .size(font_size::CAPTION)
                    .style(text::secondary)
                    .wrapping(text::Wrapping::None),
            )
            .style(styles::subtle)
            .padding(0)
            .width(width)
            .clip(true)
            .on_press(files(FilesMessage::SortBy {
                tab,
                side,
                column: *column,
            }))
            .into()
        })
        .collect();
    let least = shown.iter().copied().map(column_min_width).collect();
    let (fitted, resized) = (shown.to_vec(), shown.to_vec());
    crate::column_header::ColumnHeader::new(cells, laid.to_vec())
        .least(least)
        .inset(ROW_PADDING_X)
        .spacing(spacing::SM)
        .contents(move |index| {
            fitted
                .get(index)
                .map(|column| column_texts(entries, *column, sort))
                .unwrap_or_default()
        })
        .on_resize(move |laid| Message::FileColumns {
            tab,
            side,
            widths: widths.resized(&resized, &laid),
        })
        .into()
}

/// The icon before an entry's name, as the C# list's: a folder or a link in the info
/// colour, a file in the secondary text, a pipe, a socket or a device in the info colour.
fn entry_icon<'a>(kind: EntryKind) -> Element<'a, Message> {
    let (icon, tint) = match kind {
        EntryKind::Directory => (Icon::FolderGlyph, Tint::Info),
        EntryKind::Link => (Icon::Link, Tint::Info),
        EntryKind::File => (Icon::Page, Tint::Secondary),
        EntryKind::Other(_) => (Icon::Info, Tint::Info),
    };
    icons::icon(icon, tint, ENTRY_ICON_SIDE)
}

/// An entry's row: its cells laid out at `laid` as the headers above, each cut at its
/// column's edge, its name after its icon; drawn as the C# `FileBrowserRowStyle`, lit
/// under the pointer, the accent's tint and edge once selected.
fn entry_row<'a, E: Listed>(
    entry: &E,
    (columns, laid): (&[SortColumn], &[f32]),
    (selected, target): (bool, bool),
    (tab, side, index): (TabId, Side, usize),
) -> Element<'a, Message> {
    let on_press = files(FilesMessage::Select { tab, side, index });
    let mut cells = row![].spacing(spacing::SM);
    for (column, width) in columns.iter().zip(laid.iter().copied()) {
        let cell = text(cell_text(entry, *column)).wrapping(text::Wrapping::None);
        let cell: Element<'a, Message> = match cell_size(*column) {
            Some(small) => cell.size(small).into(),
            None => row![entry_icon(entry.kind()), cell]
                .spacing(ENTRY_ICON_GAP)
                .align_y(Alignment::Center)
                .into(),
        };
        cells = cells.push(container(cell).width(width).clip(true));
    }
    let line = button(cells)
        .padding([ROW_PADDING_Y, ROW_PADDING_X])
        .width(Length::Fill)
        .style(if target {
            // A drag over this folder: where the entries would go.
            button::success
        } else {
            styles::bare
        })
        .on_press(on_press);
    let mark = if selected && !target {
        Mark::Selected
    } else {
        Mark::None
    };
    // A right click opens its menu, as in the C# Files tab.
    crate::files_drag::spot(
        mouse_area(RowChrome::new(row_tooltip(line, entry), mark)).on_right_press(
            Message::OpenTreeMenu(TreeMenu::FilesEntry {
                tab,
                side,
                index: Some(index),
            }),
        ),
        crate::files_drag::Spot {
            tab,
            side,
            index: Some(index),
        },
    )
}

/// A glyph alone on a quiet button, as the C# toolbar's: named on hover, found by `id`,
/// faded while it has nothing to do.
fn glyph_button<'a>(
    icon: Icon,
    tip: String,
    id: Id,
    on_press: Option<Message>,
) -> Element<'a, Message> {
    let opacity = if on_press.is_some() {
        1.0
    } else {
        OPACITY_DISABLED
    };
    tooltip(
        container(
            button(center(icons::faded(
                icon,
                Tint::Text,
                icons::GLYPH_SIDE,
                opacity,
            )))
            .width(TOOL_SIDE)
            .height(TOOL_SIDE)
            .padding(0)
            .style(styles::subtle)
            .on_press_maybe(on_press),
        )
        .id(id),
        text(tip).size(font_size::CAPTION),
        tooltip::Position::Bottom,
    )
    .style(container::rounded_box)
    .into()
}

/// A labelled button of a toolbar, its glyph before its words, as the C#'s.
fn glyph_label<'a>(icon: Icon, label: String) -> Row<'a, Message> {
    row![
        icons::icon(icon, Tint::Text, icons::GLYPH_SIDE),
        text(label).size(font_size::CAPTION).font(styles::SEMIBOLD),
    ]
    .spacing(spacing::XS)
    .align_y(Alignment::Center)
}

/// The line between two groups of a toolbar, as the C# `ToolbarVerticalSeparatorStyle`.
fn tool_separator<'a>() -> Element<'a, Message> {
    container(iced::widget::space())
        .width(SEPARATOR_WIDTH)
        .height(SEPARATOR_HEIGHT)
        .style(styles::divider)
        .into()
}

/// A line across a pane between its toolbar, its path bar, its list and its footer, as
/// the C# borders between them.
fn pane_rule<'a>() -> Element<'a, Message> {
    container(iced::widget::space())
        .width(Length::Fill)
        .height(BORDER_WIDTH)
        .style(styles::divider)
        .into()
}

/// A pane's toolbar, as the C# file browser's: Back, Up, Home and Refresh as glyphs, then
/// New folder; on the server's, its toggles, "sudo" over SSH and "cwd" over SFTP, and its
/// bookmarks behind a star; on this computer's in the local file browser, its own "cwd".
/// Renaming and deleting are in the entries' menu, on F2 and Delete, as the C#'s. The
/// filter and the hidden-files toggle on the right. The tools wrap: a narrow pane keeps
/// every one whole.
fn pane_toolbar<'a>(
    (tab, side): (TabId, Side),
    moves: Moves,
    toggles: PaneToggles,
    (filter, show_hidden): (&str, bool),
) -> Element<'a, Message> {
    let tool =
        |icon, tip, which, message| glyph_button(icon, tip, tool_id(tab, side, which), message);
    let mut tools = row![
        tool(
            Icon::Back,
            fl!("ui-files-back-tooltip"),
            PaneTool::Back,
            moves.back.then(|| files(FilesMessage::Back { tab, side })),
        ),
        tool(
            Icon::Up,
            fl!("ui-files-up-tooltip"),
            PaneTool::Up,
            Some(files(FilesMessage::Up { tab, side })),
        ),
        tool(
            Icon::Home,
            fl!("ui-files-home-tooltip"),
            PaneTool::Home,
            moves.home.then(|| files(FilesMessage::Home { tab, side })),
        ),
        tool(
            Icon::Refresh,
            fl!("ui-files-refresh-tooltip"),
            PaneTool::Refresh,
            Some(files(FilesMessage::Refresh { tab, side })),
        ),
        tool_separator(),
        tooltip(
            button(glyph_label(
                Icon::NewFolder,
                fl!("ui-files-new-folder-button")
            ))
            .style(styles::secondary)
            .on_press(files(FilesMessage::AskNewFolder { tab, side })),
            text(fl!("ui-files-new-folder-tooltip")).size(font_size::CAPTION),
            tooltip::Position::Bottom,
        )
        .style(container::rounded_box),
    ]
    .spacing(spacing::XS)
    .align_y(Alignment::Center);
    if toggles.sudo.is_some() || toggles.follow.is_some() {
        tools = tools.push(tool_separator());
    }
    if let Some(on) = toggles.sudo {
        tools = tools.push(sudo_toggle(tab, on));
    }
    if let Some(on) = toggles.follow {
        tools = tools.push(follow_toggle(tab, side, on));
    }
    // The server's folders only, as in the C# tab.
    if side == Side::Remote {
        tools = tools.push(tool_separator()).push(tool(
            Icon::FavoriteStar,
            fl!("ui-files-bookmark-button"),
            PaneTool::Bookmarks,
            Some(Message::OpenTreeMenu(TreeMenu::FilesBookmarks(tab))),
        ));
    }
    row![
        container(tools.wrap().vertical_spacing(spacing::XS)).width(Length::Fill),
        pane_narrowing(tab, side, filter, show_hidden),
    ]
    .spacing(spacing::SM)
    .align_y(Alignment::Center)
    .into()
}

/// The C# "cwd" toggle of an SFTP server pane, lit while the pane follows the working
/// folder of the SSH shell beside it (`on`). Only drawn while the pane is connected, as the
/// C# toolbar only lets it be pressed then. On this computer's `side`, the local file
/// browser's: following the local shell beside it, said so.
fn follow_toggle<'a>(tab: TabId, side: Side, on: bool) -> Element<'a, Message> {
    let tip = match side {
        Side::Remote => fl!("ui-files-follow-tooltip"),
        Side::Local => fl!("ui-files-follow-local-tooltip"),
    };
    tooltip(
        button(glyph_label(
            Icon::FolderGlyph,
            fl!("ui-files-follow-toggle"),
        ))
        .style(styles::toggle(on))
        .on_press(files(FilesMessage::ToggleFollow { tab })),
        text(tip).size(font_size::CAPTION),
        tooltip::Position::Bottom,
    )
    .style(container::rounded_box)
    .into()
}

/// The C# "sudo" toggle of the server pane, outlined in the warning colour while its
/// folders are listed as root.
fn sudo_toggle<'a>(tab: TabId, on: bool) -> Element<'a, Message> {
    tooltip(
        button(glyph_label(Icon::Admin, fl!("ui-files-sudo-toggle")))
            .style(styles::toggle(on))
            .on_press(files(FilesMessage::ToggleSudo { tab })),
        text(fl!("ui-files-sudo-tooltip")).size(font_size::CAPTION),
        tooltip::Position::Bottom,
    )
    .style(container::rounded_box)
    .into()
}

/// The question asked before deleting the server's entries as root, the sudo mode being on:
/// in the danger colour, naming them, the first ones then how many more.
pub fn sudo_delete_question<'a>(names: &[String], more: usize) -> Element<'a, Message> {
    let mut listed = Column::new().spacing(2.0);
    for name in names {
        listed = listed.push(text(name.clone()).size(font_size::CAPTION));
    }
    if more > 0 {
        listed = listed
            .push(text(fl!("ui-dialog-sudo-delete-more", count = more)).size(font_size::CAPTION));
    }
    crate::dialog_parts::choice(
        crate::dialog_parts::Severity::Danger,
        fl!("ui-dialog-sudo-delete-title"),
        column![
            crate::dialog_parts::body(fl!("ui-dialog-sudo-delete-body")),
            styles::scroll(listed).height(Length::Shrink),
        ]
        .spacing(spacing::SM),
        fl!("ui-dialog-cancel-button"),
        fl!("ui-dialog-sudo-delete-confirm"),
    )
}

/// The C# filter and hidden-files toggle of a pane, lit while hidden names show.
fn pane_narrowing<'a>(
    tab: TabId,
    side: Side,
    filter: &str,
    show_hidden: bool,
) -> iced::widget::Row<'a, Message> {
    // Escape empties it first, as the C# one; empty, the field gives the keyboard back.
    let emptied = (!filter.is_empty()).then(|| {
        files(FilesMessage::Filter {
            tab,
            side,
            text: String::new(),
        })
    });
    row![
        crate::search_keys::SearchKeys::escape_only(
            text_input(&fl!("ui-files-filter-placeholder"), filter)
                .style(styles::text_input)
                .id(field_id(tab, side, PaneField::Filter))
                .size(font_size::CAPTION)
                .on_input(move |text| files(FilesMessage::Filter { tab, side, text }))
                .width(FILTER_WIDTH),
            emptied,
        ),
        tooltip(
            button(text(fl!("ui-files-hidden-toggle")).size(font_size::CAPTION))
                .style(if show_hidden {
                    styles::primary
                } else {
                    styles::secondary
                })
                .on_press(files(FilesMessage::ToggleHidden { tab, side })),
            text(fl!("ui-files-hidden-tooltip")).size(font_size::CAPTION),
            tooltip::Position::Bottom,
        )
        .style(container::rounded_box),
    ]
    .spacing(spacing::SM)
    .align_y(Alignment::Center)
}

/// A pane's footer, as the C# status bar under the list: how many entries it lists and
/// shows as the C# tab counts them, and how many are selected past one, in the secondary
/// text.
fn pane_footer<'a>(
    shown: usize,
    total: usize,
    (chosen, size): (usize, u64),
) -> Element<'a, Message> {
    let count = if shown == total {
        fl!("ui-files-item-count", count = total)
    } else {
        fl!("ui-files-item-count-filtered", shown = shown, count = total)
    };
    let mut said = row![text(count).size(font_size::CAPTION).style(text::secondary)]
        .spacing(spacing::SM)
        .align_y(Alignment::Center);
    if chosen > 1 {
        let selection = fl!("ui-files-selected-count", count = chosen);
        // The files' size beside it, as the C# selection line; folders are not counted.
        let selection = if size > 0 {
            fl!(
                "ui-files-selected-with-size",
                selection = selection,
                size = texts::size(size)
            )
        } else {
            selection
        };
        said = said.push(
            text(selection)
                .size(font_size::CAPTION)
                .style(text::secondary),
        );
    }
    container(said).padding(FOOTER_PADDING).into()
}

/// What a pane is drawn from, shared by both sides.
struct PaneParts<'p, E> {
    tab: TabId,
    side: Side,
    /// Its name beside the other pane; none for the local file browser's, whose pane header
    /// names it.
    title: Option<String>,
    /// What its transport discloses, beside its name: the server's pane over FTP without
    /// TLS, as the C# badge says it.
    notice: Option<String>,
    location: String,
    /// The folders of the one shown, the root first, as the breadcrumb shows them; `None`
    /// while the path bar is typed in.
    breadcrumb: Option<Vec<String>>,
    typed: Option<&'p str>,
    entries: &'p [E],
    columns: &'p [SortColumn],
    /// The widths of the columns besides the name.
    widths: ColumnWidths,
    sort: Sort,
    selected: Option<usize>,
    marked: &'p BTreeSet<usize>,
    filter: &'p str,
    show_hidden: bool,
    /// The listing's entries, then those left once hidden names are left out, unless they
    /// show.
    counts: (usize, usize),
    loading: bool,
    error: Option<&'p FilesError>,
    focused: bool,
    /// Where the pane can go besides up.
    moves: Moves,
    /// The delete or the change of permissions running in this pane.
    batch: Option<&'p heimdall_app::files::Batch>,
    /// Where a drag would drop in this pane.
    drop: Option<DropHere>,
    /// The pane's own toggles: the server's, and the local file browser's "cwd".
    toggles: PaneToggles,
}

/// The toggles of a pane: the server's, as the C# SFTP toolbar's, and the "cwd" toggle of
/// the local file browser.
#[derive(Debug, Clone, Copy, Default)]
struct PaneToggles {
    /// The "sudo" toggle, on or off; none where it is not offered: this computer's pane,
    /// and the server's without SSH.
    sudo: Option<bool>,
    /// The "cwd" toggle, on or off; none where it is not offered: the server's pane over
    /// FTP, and this computer's beside a server's.
    follow: Option<bool>,
}

impl PaneToggles {
    /// The toggles of `files_pane`'s local pane: the "cwd" one in the local file browser
    /// alone, following the local shell beside it.
    fn local(files_pane: &FilesPane) -> Self {
        Self {
            sudo: None,
            follow: files_pane
                .follow
                .as_ref()
                .filter(|_| files_pane.local_only)
                .map(|follow| follow.on),
        }
    }

    /// The toggles of `files_pane`'s server pane.
    fn server(files_pane: &FilesPane) -> Self {
        Self {
            // Over SSH only: an FTP tab has no shell to run sudo on.
            sudo: files_pane.shell.as_ref().map(|_| files_pane.sudo_mode),
            // Over SFTP only, as the C#: an FTP tab never follows a shell, and the local
            // file browser's toggle is on its own pane.
            follow: files_pane
                .follow
                .as_ref()
                .filter(|_| !files_pane.local_only)
                .map(|follow| follow.on),
        }
    }
}

/// Where a pane can go besides up, as the C# Files tab's Back and Home.
#[derive(Debug, Clone, Copy)]
struct Moves {
    /// Back has a folder to go to.
    back: bool,
    /// Home is known: a folder was shown.
    home: bool,
}

/// The folder shown as its folders, each a button going there between chevrons, in a box
/// drawn as the path field, as the C# breadcrumb; a click beside them gives the path back
/// to be typed in. The deepest stays in sight.
fn breadcrumb_bar<'a>(tab: TabId, side: Side, segments: Vec<String>) -> Element<'a, Message> {
    let last = segments.len().saturating_sub(1);
    let mut trail = Row::new().spacing(spacing::XS).align_y(Alignment::Center);
    for (index, name) in segments.into_iter().enumerate() {
        if index > 0 {
            trail = trail.push(icons::icon(
                Icon::ChevronRight,
                Tint::Secondary,
                CHEVRON_SIDE,
            ));
        }
        trail = trail.push(
            button(text(name).font(PATH_FONT))
                .style(styles::subtle)
                .padding(SEGMENT_PADDING)
                .on_press(files(FilesMessage::Ascend {
                    tab,
                    side,
                    levels: last - index,
                })),
        );
    }
    let trail = scrollable(trail)
        .direction(scrollable::Direction::Horizontal(
            scrollable::Scrollbar::hidden(),
        ))
        .anchor_right()
        .width(Length::Fill);
    mouse_area(
        container(trail)
            .padding(BREADCRUMB_PADDING)
            .width(Length::Fill)
            .style(styles::field_box),
    )
    .on_press(Message::EditPath { tab, side })
    .into()
}

/// What a pane listing nothing says, as the C# empty states: the folder is empty; or
/// nothing matches the filter, with a way to clear it; or only hidden entries are there,
/// with a way to show them.
fn empty_state<'a>(
    tab: TabId,
    side: Side,
    filter: &str,
    (total, unhidden): (usize, usize),
) -> Element<'a, Message> {
    let said = |words: String| text(words).size(font_size::CAPTION);
    let way = |label: String, message: FilesMessage| {
        button(text(label).size(font_size::CAPTION))
            .style(styles::secondary)
            .on_press(files(message))
    };
    if total == 0 {
        return said(fl!("ui-files-empty")).into();
    }
    // As the C# `UpdateEmptyState`: only hidden entries first, then no match.
    if unhidden == 0 {
        return column![
            said(fl!("ui-files-empty-hidden-only")),
            way(
                fl!("ui-files-empty-show-hidden"),
                FilesMessage::ToggleHidden { tab, side }
            ),
        ]
        .spacing(spacing::SM)
        .into();
    }
    if !filter.trim().is_empty() {
        return column![
            said(fl!("ui-files-empty-no-match", filter = filter)),
            way(
                fl!("ui-files-empty-clear-filter"),
                FilesMessage::Filter {
                    tab,
                    side,
                    text: String::new(),
                }
            ),
        ]
        .spacing(spacing::SM)
        .into();
    }
    said(fl!("ui-files-empty")).into()
}

/// Where a delete or a change of permissions of several entries is, as the C# status says
/// it, and Cancel, which stops it after the entry being worked on.
fn batch_row<'a>(tab: TabId, batch: &heimdall_app::files::Batch) -> Element<'a, Message> {
    let index = (batch.done + 1).min(batch.total);
    let name = batch.current.as_str();
    let at = match batch.kind {
        heimdall_app::files::BatchKind::Delete => fl!(
            "ui-files-batch-deleting",
            name = name,
            index = index,
            total = batch.total
        ),
        heimdall_app::files::BatchKind::Permissions => fl!(
            "ui-files-batch-permissions",
            name = name,
            index = index,
            total = batch.total
        ),
    };
    let stop: Element<'a, Message> = if batch.stopping {
        text(fl!("ui-files-batch-stopping"))
            .size(font_size::CAPTION)
            .style(text::secondary)
            .into()
    } else {
        button(text(fl!("ui-files-batch-stop")).size(font_size::CAPTION))
            .style(styles::secondary)
            .on_press(files(FilesMessage::StopBatch { tab }))
            .into()
    };
    row![text(at).size(font_size::CAPTION).width(Length::Fill), stop]
        .spacing(spacing::SM)
        .align_y(Alignment::Center)
        .into()
}

/// A pane's path bar, as the C#'s: the folder shown in a rounded box, as its folders to
/// click or typed over to go elsewhere, then Go as a chevron, there while a path is typed.
fn path_bar<'a>(
    (tab, side): (TabId, Side),
    location: &str,
    breadcrumb: Option<Vec<String>>,
    typed: Option<&str>,
) -> Element<'a, Message> {
    let field = match breadcrumb.filter(|_| typed.is_none()) {
        Some(segments) => breadcrumb_bar(tab, side, segments),
        None => text_input(location, typed.unwrap_or(location))
            .style(styles::text_input)
            .id(field_id(tab, side, PaneField::Path))
            .font(PATH_FONT)
            .on_input(move |text| files(FilesMessage::PathEdited { tab, side, text }))
            .on_submit(files(FilesMessage::GoTo { tab, side }))
            .width(Length::Fill)
            .into(),
    };
    row![
        field,
        glyph_button(
            Icon::ChevronRight,
            fl!("ui-files-go-tooltip"),
            tool_id(tab, side, PaneTool::Go),
            typed.map(|_| files(FilesMessage::GoTo { tab, side })),
        ),
    ]
    .spacing(spacing::XS)
    .align_y(Alignment::Center)
    .into()
}

/// A pane, as the C# file browser: its title beside a server's, its toolbar, its path bar,
/// what it says of itself, its list under the columns' headers, and its footer counting the
/// entries.
fn pane<E: Listed>(parts: PaneParts<'_, E>) -> Element<'_, Message> {
    let PaneParts {
        tab,
        side,
        title,
        notice,
        location,
        breadcrumb,
        typed,
        entries,
        columns,
        widths,
        sort,
        selected,
        marked,
        filter,
        show_hidden,
        counts: (total, unhidden),
        loading,
        error,
        focused,
        moves,
        batch,
        drop,
        toggles,
    } = parts;
    let chosen = marked.len() + usize::from(selected.is_some());
    let failed = error.is_some();
    // The columns that fit beside a name still readable, laid out for the pane's width.
    let listing = responsive(move |size| {
        let shown = fitting_columns(columns, widths, size.width);
        let laid = laid_widths(&shown, widths, size.width - 2.0 * ROW_PADDING_X);
        let mut list = Column::new();
        if loading {
            list = list.push(text(fl!("ui-files-loading")).size(font_size::CAPTION));
        } else if entries.is_empty() && !failed {
            list = list.push(empty_state(tab, side, filter, (total, unhidden)));
        }
        for (index, entry) in entries.iter().enumerate() {
            let place = (tab, side, index);
            let picked = selected == Some(index) || marked.contains(&index);
            let target = drop == Some(DropHere::Entry(index));
            list = list.push(entry_row(entry, (&shown, &laid), (picked, target), place));
        }
        // A right click beside the entries: the menu of the folder shown, as the C# list's.
        let list = mouse_area(
            styles::scroll(list)
                .id(list_id(tab, side))
                .height(Length::Fill),
        )
        .on_right_press(Message::OpenTreeMenu(TreeMenu::FilesEntry {
            tab,
            side,
            index: None,
        }));
        let header = headers((tab, side), entries, (&shown, &laid), (sort, widths));
        column![header, list].spacing(spacing::XS).into()
    });
    let mut content = Column::new().spacing(spacing::SM);
    if let Some(title) = title {
        let mut heading = row![text(title).size(font_size::SUBTITLE).font(styles::SEMIBOLD)]
            .spacing(spacing::SM)
            .align_y(Alignment::Center);
        if let Some(notice) = notice {
            heading = heading.push(security_notice(notice));
        }
        content = content.push(heading);
    }
    content = content
        .push(pane_toolbar(
            (tab, side),
            moves,
            toggles,
            (filter, show_hidden),
        ))
        .push(pane_rule())
        .push(path_bar((tab, side), &location, breadcrumb, typed))
        .push(pane_rule());
    let footer = pane_footer(
        entries.len(),
        total,
        (chosen, chosen_size(entries, selected, marked)),
    );
    pane_frame(
        pane_notes(content, tab, (toggles.sudo, error, batch))
            .push(listing)
            .push(pane_rule())
            .push(footer),
        (tab, side),
        focused,
        drop == Some(DropHere::Pane),
    )
}

/// What a pane says above its entries: listed as root, as the C# status says it while the
/// sudo mode is on; why the last listing failed; where its delete or change of permissions
/// is.
fn pane_notes<'a>(
    mut content: Column<'a, Message>,
    tab: TabId,
    (sudo, error, batch): (
        Option<bool>,
        Option<&FilesError>,
        Option<&heimdall_app::files::Batch>,
    ),
) -> Column<'a, Message> {
    if sudo == Some(true) {
        content = content.push(
            text(fl!("ui-files-sudo-on"))
                .size(font_size::CAPTION)
                .style(text::warning),
        );
    }
    if let Some(error) = error {
        content = content.push(text(texts::files_error(error)).size(font_size::CAPTION));
    }
    if let Some(batch) = batch {
        content = content.push(batch_row(tab, batch));
    }
    content
}

/// The size of the files among the entries chosen, as the C# selection line adds them;
/// folders are not counted.
fn chosen_size<E: Listed>(entries: &[E], selected: Option<usize>, marked: &BTreeSet<usize>) -> u64 {
    entries
        .iter()
        .enumerate()
        .filter(|(index, entry)| {
            (selected == Some(*index) || marked.contains(index)) && entry.kind() == EntryKind::File
        })
        .filter_map(|(_, entry)| entry.size())
        .sum()
}

/// Where a drag would drop in a pane.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DropHere {
    /// The folder the pane shows.
    Pane,
    /// Its folder entry at this place.
    Entry(usize),
}

/// A pane's frame: outlined when it has the keyboard, or when a drag over it would drop in
/// the folder it shows; the pointer over it said.
fn pane_frame(
    content: Column<'_, Message>,
    (tab, side): (TabId, Side),
    focused: bool,
    whole_target: bool,
) -> Element<'_, Message> {
    let pane = container(content)
        .padding(spacing::SM)
        .width(Length::FillPortion(1))
        .height(Length::Fill)
        .style(move |theme: &Theme| {
            // The window's background, as the C# list's: a row is lit over it.
            let mut style = container::bordered_box(theme);
            style.background = Some(theme.palette().background.into());
            if whole_target {
                // A drag over it: where its entries would go.
                style.border.color = theme.extended_palette().success.base.color;
                style.border.width = FOCUS_BORDER_WIDTH;
            } else if focused {
                style.border.color = theme.extended_palette().primary.base.color;
                style.border.width = FOCUS_BORDER_WIDTH;
            }
            style
        });
    crate::files_drag::spot(
        pane,
        crate::files_drag::Spot {
            tab,
            side,
            index: None,
        },
    )
}

/// A file edited with the external editor: its state, its folder, a refused save sent
/// anyway, and stopping; sent with sudo only over the tab's SSH connection, `over_ssh`.
fn edit_row(tab: TabId, edit: &EditSession, over_ssh: bool) -> Element<'_, Message> {
    let local = || edit.local.clone();
    let state = match &edit.refused {
        Some(error) => text(texts::files_error(error))
            .size(font_size::CAPTION)
            .style(text::danger),
        None => text(fl!("ui-files-edit-watching")).size(font_size::CAPTION),
    };
    let mut line = row![
        text(edit.name.as_str()).width(Length::FillPortion(2)),
        state.width(Length::FillPortion(3)),
    ]
    .spacing(spacing::SM)
    .align_y(Alignment::Center);
    // Refused by the server's permissions, or by sudo for its password: sudo is offered,
    // where there is an SSH connection to run it: never over FTP.
    let sudo_helps = over_ssh
        && matches!(
            edit.refused,
            Some(
                FilesError::Server {
                    refusal: Refusal::PermissionDenied,
                    ..
                } | FilesError::SudoPasswordNeeded
                    | FilesError::SudoPasswordRejected
            )
        );
    if sudo_helps {
        line = line.push(
            button(text(fl!("ui-files-edit-save-sudo")).size(font_size::CAPTION))
                .style(styles::primary)
                .on_press(files(FilesMessage::EditSaveWithSudo {
                    tab,
                    local: local(),
                })),
        );
    }
    if edit.refused.is_some() && !edit.privileged {
        line = line.push(
            button(text(fl!("ui-files-edit-send-anyway")).size(font_size::CAPTION))
                .style(styles::primary)
                .on_press(files(FilesMessage::EditSendAnyway {
                    tab,
                    local: local(),
                })),
        );
    }
    line.push(
        button(text(fl!("ui-files-edit-open-folder")).size(font_size::CAPTION))
            .style(styles::secondary)
            .on_press(files(FilesMessage::EditOpenFolder {
                tab,
                local: local(),
            })),
    )
    .push(
        button(text(fl!("ui-files-edit-stop")).size(font_size::CAPTION))
            .style(styles::secondary)
            .on_press(files(FilesMessage::EditStop {
                tab,
                local: local(),
            })),
    )
    .into()
}

/// How far a running transfer is, and, once measured, how fast it goes and how long is
/// left, as the C# says it: "42 MB of 100 MB - 5.2 MB/s, 11 s left".
fn running_text(transfer: &Transfer) -> String {
    let progress = match transfer.total {
        Some(total) => fl!(
            "ui-files-state-running",
            done = texts::size(transfer.bytes),
            total = texts::size(total)
        ),
        None => fl!(
            "ui-files-state-running-unknown",
            done = texts::size(transfer.bytes)
        ),
    };
    let (Some(speed), Some(left)) = (
        transfer.rate.bytes_per_second(),
        transfer.rate.remaining(transfer.bytes, transfer.total),
    ) else {
        return progress;
    };
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "a speed in whole bytes a second, never negative"
    )]
    let speed = speed as u64;
    fl!(
        "ui-files-state-rate",
        progress = progress,
        rate = texts::size(speed),
        left = eta_text(left)
    )
}

/// A time left as the C# writes it: seconds, minutes and seconds, or hours and minutes.
fn eta_text(left: std::time::Duration) -> String {
    let seconds = left.as_secs();
    match seconds {
        0..60 => fl!("ui-files-eta-seconds", seconds = seconds),
        60..3_600 => fl!(
            "ui-files-eta-minutes",
            minutes = (seconds / 60),
            seconds = (seconds % 60)
        ),
        _ => fl!(
            "ui-files-eta-hours",
            hours = (seconds / 3_600),
            minutes = (seconds % 3_600 / 60)
        ),
    }
}

/// How much of a running transfer of a known size is done, from 0 to 1.
#[expect(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    reason = "a share drawn as a bar"
)]
fn done_share(transfer: &Transfer) -> Option<f32> {
    let total = transfer.total.filter(|total| *total > 0)?;
    Some((transfer.bytes.min(total) as f64 / total as f64) as f32)
}

/// A transfer's line: what, how far, and what can be done with it, Retry while the
/// session lives.
fn transfer_row(tab: TabId, transfer: &Transfer, session_live: bool) -> Element<'_, Message> {
    let what = match transfer.direction {
        Direction::Download => {
            fl!("ui-files-transfer-download", name = transfer.label.as_str())
        }
        Direction::Upload => fl!("ui-files-transfer-upload", name = transfer.label.as_str()),
    };
    let state = match &transfer.state {
        TransferState::Queued => fl!("ui-files-state-queued"),
        TransferState::Preparing => fl!("ui-files-state-preparing"),
        TransferState::Running => running_text(transfer),
        TransferState::Done => fl!("ui-files-state-done"),
        TransferState::Incomplete { skipped } => {
            fl!("ui-files-state-incomplete", count = (*skipped))
        }
        TransferState::Cancelled => fl!("ui-files-state-cancelled"),
        TransferState::Failed(error) => {
            fl!("ui-files-state-failed", reason = texts::files_error(error))
        }
    };
    let mut line = row![
        text(what).width(Length::FillPortion(2)),
        text(state)
            .size(font_size::CAPTION)
            .width(Length::FillPortion(3)),
    ]
    .spacing(spacing::SM)
    .align_y(Alignment::Center);
    let small = |label: String, message: FilesMessage| {
        button(text(label).size(font_size::CAPTION))
            .style(styles::secondary)
            .on_press(files(message))
    };
    if matches!(
        transfer.state,
        TransferState::Queued | TransferState::Preparing
    ) {
        line = line.push(small(
            fl!("ui-files-cancel-button"),
            FilesMessage::Cancel {
                tab,
                id: transfer.id,
            },
        ));
    }
    if session_live && transfer.state.retryable() && transfer.picked.is_some() {
        line = line.push(small(
            fl!("ui-files-retry-button"),
            FilesMessage::Retry {
                tab,
                id: transfer.id,
            },
        ));
    }
    if transfer.state == TransferState::Running {
        if let Some(share) = done_share(transfer) {
            line = line.push(
                iced::widget::progress_bar(0.0..=1.0, share)
                    .length(Length::FillPortion(1))
                    .girth(8.0),
            );
        }
        line = line.push(small(
            fl!("ui-files-cancel-button"),
            FilesMessage::Cancel {
                tab,
                id: transfer.id,
            },
        ));
    }
    line.into()
}

/// The persistent notice of a transport's limitation, beside the server pane's name, as the
/// C# `SecurityNoticeBadge` (`EmbeddedSftpView.xaml:161-186`): outlined and written in the
/// warning colour, its text again in its tooltip.
fn security_notice<'a>(notice: String) -> Element<'a, Message> {
    tooltip(
        container(
            text(notice.clone())
                .size(font_size::SMALL_CAPTION)
                .style(text::warning),
        )
        .padding(NOTICE_PADDING)
        .style(styles::warning_badge),
        text(notice).size(font_size::CAPTION),
        tooltip::Position::Bottom,
    )
    .style(container::rounded_box)
    .into()
}

/// The Files tab, its panes' columns at `columns`, `notice` beside the server pane's name.
#[must_use]
pub fn view(
    tab: TabId,
    files_pane: &FilesPane,
    live: bool,
    editing: Option<Side>,
    drop: Option<crate::files_drag::Spot>,
    columns: TabColumns,
    notice: Option<String>,
) -> Element<'_, Message> {
    let drop_in = |side: Side| {
        drop.filter(|spot| spot.tab == tab && spot.side == side)
            .map(|spot| spot.index.map_or(DropHere::Pane, DropHere::Entry))
    };
    let local_location = files_pane.local.path.display().to_string();
    let remote_location = files_pane.remote.path.display();
    let local = pane(PaneParts {
        tab,
        side: Side::Local,
        title: (!files_pane.local_only).then(|| fl!("ui-files-local-title")),
        notice: None,
        location: local_location,
        breadcrumb: (editing != Some(Side::Local)).then(|| local_segments(&files_pane.local.path)),
        typed: files_pane.local.typed.as_deref(),
        entries: &files_pane.local.entries,
        columns: LOCAL_COLUMNS,
        widths: columns.local,
        sort: files_pane.local.sort,
        selected: files_pane.local.selected,
        marked: &files_pane.local.marked,
        filter: &files_pane.local.filter,
        show_hidden: files_pane.local.show_hidden,
        counts: files_pane.local.counts(),
        loading: files_pane.local.loading,
        error: files_pane.local.error.as_ref(),
        focused: files_pane.focus == Side::Local,
        moves: Moves {
            back: files_pane.local.can_go_back(),
            home: files_pane.local.home.is_some(),
        },
        batch: files_pane
            .batch
            .as_ref()
            .filter(|batch| batch.side == Side::Local),
        drop: drop_in(Side::Local),
        toggles: PaneToggles::local(files_pane),
    });
    let remote = pane(PaneParts {
        tab,
        side: Side::Remote,
        title: Some(fl!("ui-files-remote-title")),
        notice,
        location: remote_location,
        breadcrumb: (editing != Some(Side::Remote))
            .then(|| remote_segments(&files_pane.remote.path)),
        typed: files_pane.remote.typed.as_deref(),
        entries: &files_pane.remote.entries,
        columns: REMOTE_COLUMNS,
        widths: columns.remote,
        sort: files_pane.remote.sort,
        selected: files_pane.remote.selected,
        marked: &files_pane.remote.marked,
        filter: &files_pane.remote.filter,
        show_hidden: files_pane.remote.show_hidden,
        counts: files_pane.remote.counts(),
        loading: files_pane.remote.loading,
        error: files_pane.remote.error.as_ref(),
        focused: files_pane.focus == Side::Remote,
        moves: Moves {
            back: files_pane.remote.can_go_back(),
            home: files_pane.remote.home.is_some(),
        },
        batch: files_pane
            .batch
            .as_ref()
            .filter(|batch| batch.side == Side::Remote),
        drop: drop_in(Side::Remote),
        toggles: PaneToggles::server(files_pane),
    });
    let mut content = column![panes(tab, files_pane, local, remote)]
        .spacing(spacing::SM)
        .padding(spacing::SM);
    if !files_pane.edits.is_empty() {
        let list = files_pane
            .edits
            .iter()
            .fold(Column::new().spacing(spacing::XS), |list, edit| {
                list.push(edit_row(tab, edit, files_pane.shell.is_some()))
            });
        content = content
            .push(text(fl!("ui-files-edits-title")).size(font_size::SUBTITLE))
            .push(list);
    }
    if !files_pane.transfers.is_empty() {
        content = content.push(transfers(tab, files_pane, live));
    }
    content.into()
}

/// The two panes, Upload and Download between them with the toggle hiding this computer's;
/// once hidden, the server's alone under that toggle, as the C# SFTP pane docked beside a
/// shell shows it. The local file browser docked beside a local shell has no server: this
/// computer's pane alone, as the C# browser.
fn panes<'a>(
    tab: TabId,
    files_pane: &'a FilesPane,
    local: Element<'a, Message>,
    remote: Element<'a, Message>,
) -> Element<'a, Message> {
    if files_pane.local_only {
        return local;
    }
    if files_pane.local_hidden {
        return column![row![local_toggle(tab, false)], remote]
            .spacing(spacing::SM)
            .height(Length::Fill)
            .into();
    }
    let actions = send_buttons(tab, files_pane).push(local_toggle(tab, true));
    row![local, container(actions).center_y(Length::Fill), remote]
        .spacing(spacing::SM)
        .height(Length::Fill)
        .into()
}

/// The toggle showing this computer's pane beside the server's, lit while it shows.
fn local_toggle<'a>(tab: TabId, shown: bool) -> Element<'a, Message> {
    tooltip(
        button(text(fl!("ui-files-local-toggle")).size(font_size::CAPTION))
            .style(if shown {
                styles::primary
            } else {
                styles::secondary
            })
            .on_press(files(FilesMessage::ToggleLocal { tab })),
        text(fl!("ui-files-local-toggle-tooltip")).size(font_size::CAPTION),
        tooltip::Position::Bottom,
    )
    .style(container::rounded_box)
    .into()
}

/// Upload and Download, between the panes: each sends the selection of its side.
fn send_buttons(tab: TabId, files_pane: &FilesPane) -> Column<'_, Message> {
    let can_upload = files_pane.local.selected.is_some();
    let can_download = files_pane.remote.selected.is_some();
    column![
        button(text(fl!("ui-files-upload-button")))
            .style(styles::primary)
            .on_press_maybe(can_upload.then(|| files(FilesMessage::Transfer {
                tab,
                direction: Direction::Upload,
            }))),
        button(text(fl!("ui-files-download-button")))
            .style(styles::primary)
            .on_press_maybe(can_download.then(|| files(FilesMessage::Transfer {
                tab,
                direction: Direction::Download,
            }))),
    ]
    .spacing(spacing::SM)
    .align_x(Alignment::Center)
}

/// The transfers, newest first, under their title and "Clear finished" once one has ended.
fn transfers(tab: TabId, files_pane: &FilesPane, live: bool) -> Column<'_, Message> {
    let list = files_pane
        .transfers
        .iter()
        .rev()
        .fold(Column::new().spacing(spacing::XS), |list, transfer| {
            list.push(transfer_row(tab, transfer, live))
        });
    let mut title = row![text(fl!("ui-files-transfers-title")).size(font_size::SUBTITLE)]
        .spacing(spacing::SM)
        .align_y(Alignment::Center);
    if files_pane
        .transfers
        .iter()
        .any(|transfer| transfer.state.ended())
    {
        title = title.push(
            button(text(fl!("ui-files-clear-finished-button")).size(font_size::CAPTION))
                .style(styles::secondary)
                .on_press(files(FilesMessage::ClearFinished { tab })),
        );
    }
    column![
        title,
        container(styles::scroll(list)).max_height(TRANSFERS_HEIGHT)
    ]
    .spacing(spacing::SM)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn named(key: Named) -> keyboard::Key {
        keyboard::Key::Named(key)
    }

    #[test]
    fn the_last_columns_give_way_to_a_readable_name_but_never_the_size() {
        use SortColumn::{Modified, Name, Owner, Permissions, Size};
        let widths = ColumnWidths::default();
        assert_eq!(
            fitting_columns(REMOTE_COLUMNS, widths, 2000.0),
            REMOTE_COLUMNS
        );
        // Name 160 + size 98 + modified 138 + permissions 98 + owner 63 = 557.
        assert_eq!(
            fitting_columns(REMOTE_COLUMNS, widths, 557.0),
            REMOTE_COLUMNS
        );
        assert_eq!(
            fitting_columns(REMOTE_COLUMNS, widths, 556.0),
            [Name, Size, Modified, Permissions]
        );
        assert_eq!(fitting_columns(REMOTE_COLUMNS, widths, 300.0), [Name, Size]);
        assert_eq!(
            fitting_columns(REMOTE_COLUMNS, widths, 10.0),
            [Name, Size],
            "never fewer"
        );
        let wider = widths.resized(&[Owner], &[155.0]);
        assert_eq!(
            fitting_columns(REMOTE_COLUMNS, wider, 600.0),
            [Name, Size, Modified, Permissions],
            "the owner widened by 100 gives way sooner"
        );
    }

    #[test]
    fn the_name_takes_the_room_the_other_columns_leave() {
        use SortColumn::{Modified, Name, Size};
        let widths = ColumnWidths::default();
        // 500 - size 90 - modified 130 - two gaps of 8.
        assert_eq!(
            laid_widths(&[Name, Size, Modified], widths, 500.0),
            [264.0, 90.0, 130.0]
        );
        assert_eq!(
            laid_widths(&[Name, Size], widths, 50.0),
            [0.0, 90.0],
            "never less than nothing"
        );
    }

    #[test]
    fn resized_widths_keep_every_column_but_the_name() {
        use SortColumn::{Modified, Name, Owner, Permissions, Size};
        let widths =
            ColumnWidths::default().resized(&[Name, Size, Modified], &[400.0, 70.0, 150.0]);
        assert_eq!(widths.of(Name), None, "the rest, never kept");
        assert_eq!(widths.of(Size), Some(70.0));
        assert_eq!(widths.of(Modified), Some(150.0));
        assert_eq!(
            widths.of(Permissions),
            Some(PERMISSIONS_WIDTH),
            "not shown, kept"
        );
        assert_eq!(widths.of(Owner), Some(OWNER_WIDTH));
        let tab = TabColumns::default().with(Side::Remote, widths);
        assert_eq!(tab.remote, widths);
        assert_eq!(tab.local, ColumnWidths::default(), "each pane its own");
    }

    #[test]
    fn a_column_is_fitted_to_its_header_and_its_cells() {
        let entry = |name: &str, kind, size| heimdall_app::files::LocalEntry {
            name: name.into(),
            label: name.to_owned(),
            kind,
            size,
            modified: None,
        };
        let entries = [
            entry("a.txt", EntryKind::File, Some(2048)),
            entry("docs", EntryKind::Directory, None),
        ];
        let sort = Sort::default();
        let names = column_texts(&entries, SortColumn::Name, sort);
        assert_eq!(names.len(), 3, "the header, then each entry");
        assert_eq!(names[0].1, Some(font_size::CAPTION), "the header's size");
        assert_eq!(
            names[2],
            ("docs".to_owned(), None),
            "its icon tells it a folder"
        );
        let sizes = column_texts(&entries, SortColumn::Size, sort);
        assert_eq!(sizes[1].1, Some(font_size::CAPTION));
        assert_eq!(sizes[2].0, "", "a folder shows no size");
    }

    #[test]
    fn the_keys_are_the_csharp_file_browser_ones_whatever_the_layout() {
        use iced::keyboard::key::{Code, NativeCode, Physical};

        let anywhere = Physical::Unidentified(NativeCode::Unidentified);
        let key = |key: &keyboard::Key, modifiers| files_key(key, anywhere, modifiers);
        let letter = |c: &str| keyboard::Key::Character(c.into());
        let ctrl_shift = Modifiers::CTRL | Modifiers::SHIFT;
        assert_eq!(
            key(&named(Named::Delete), Modifiers::empty()),
            Some(FilesKey::Delete)
        );
        assert_eq!(
            key(&named(Named::Enter), Modifiers::empty()),
            None,
            "a dialog's first"
        );
        assert_eq!(key(&letter("a"), Modifiers::empty()), None);
        assert_eq!(
            key(&named(Named::Backspace), Modifiers::empty()),
            Some(FilesKey::Back)
        );
        assert_eq!(
            key(&named(Named::F7), Modifiers::empty()),
            Some(FilesKey::NewFolder)
        );
        assert_eq!(
            key(&named(Named::F4), Modifiers::empty()),
            Some(FilesKey::FocusPath)
        );
        for (typed, wanted) in [
            ("x", FilesKey::Cut),
            ("c", FilesKey::Copy),
            ("v", FilesKey::Paste),
            ("a", FilesKey::SelectAll),
        ] {
            assert_eq!(
                key(&letter(typed), Modifiers::CTRL),
                Some(wanted),
                "Ctrl+{typed}"
            );
        }
        assert_eq!(key(&letter("C"), ctrl_shift), Some(FilesKey::CopyPath));
        assert_eq!(key(&letter("D"), ctrl_shift), Some(FilesKey::Download));
        assert_eq!(key(&letter("U"), ctrl_shift), Some(FilesKey::Upload));
        assert_eq!(
            files_key(
                &letter("\u{0441}"),
                Physical::Code(Code::KeyC),
                Modifiers::CTRL
            ),
            Some(FilesKey::Copy),
            "the C key of a Cyrillic keyboard"
        );
        assert_eq!(
            key(&named(Named::ArrowLeft), Modifiers::ALT),
            Some(FilesKey::Back)
        );
        assert_eq!(
            key(&named(Named::ArrowUp), Modifiers::ALT),
            Some(FilesKey::Parent)
        );
        assert_eq!(
            key(&named(Named::ArrowDown), Modifiers::ALT),
            Some(FilesKey::Lower),
            "the session tree's Alt+Down"
        );
        assert_eq!(key(&letter("d"), Modifiers::ALT), Some(FilesKey::FocusPath));
        assert_eq!(
            key(&named(Named::Delete), Modifiers::CTRL | Modifiers::ALT),
            None,
            "AltGr is Ctrl+Alt: it types"
        );
        assert_eq!(key(&named(Named::Delete), Modifiers::LOGO), None);
    }
}
