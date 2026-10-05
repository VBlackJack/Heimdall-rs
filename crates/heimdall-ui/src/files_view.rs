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
    SortColumn, Special, Transfer, TransferState, symbolic_mode,
};
use heimdall_app::{FilesMessage, Message as AppMessage, TabId};
use heimdall_core::utc::UtcTime;
use heimdall_files::Refusal;
use iced::keyboard::{self, Modifiers, key::Named};
use iced::widget::Id;
use iced::widget::{
    Column, button, column, container, mouse_area, responsive, row, scrollable, text, text_input,
    tooltip,
};
use iced::{Alignment, Element, Length, Theme};

use crate::i18n::fl;
use crate::shell::Message;
use crate::texts;
use crate::tree_view::TreeMenu;

/// Gap between elements, in logical pixels.
const SPACING: f32 = 8.0;

/// Padding inside panes, in logical pixels.
const PADDING: f32 = 8.0;

/// Size of secondary text, in logical pixels.
const SMALL_SIZE: f32 = 12.0;

/// Size of pane titles, in logical pixels.
const TITLE_SIZE: f32 = 16.0;

/// Width of the size column, in logical pixels.
const SIZE_WIDTH: f32 = 90.0;

/// Width of the modification time column, in logical pixels.
const MODIFIED_WIDTH: f32 = 130.0;

/// Width of the permissions column, in logical pixels.
const PERMISSIONS_WIDTH: f32 = 90.0;

/// Width of the owner column, in logical pixels.
const OWNER_WIDTH: f32 = 55.0;

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

/// Marks a folder after its name: language-neutral, like a path.
const FOLDER_MARK: &str = "/";

/// Marks a link after its name.
const LINK_MARK: &str = " ->";

/// A field of a pane the keyboard can be given to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PaneField {
    /// The path bar, Alt+D and F4.
    Path,
    /// The filter, Ctrl+F.
    Filter,
}

/// Widget identifier of a pane's `field`.
#[must_use]
pub fn field_id(side: Side, field: PaneField) -> Id {
    Id::new(match (side, field) {
        (Side::Local, PaneField::Path) => "files-local-path",
        (Side::Remote, PaneField::Path) => "files-remote-path",
        (Side::Local, PaneField::Filter) => "files-local-filter",
        (Side::Remote, PaneField::Filter) => "files-remote-filter",
    })
}

/// Widget identifier of a pane's list, to scroll the selection into view.
#[must_use]
pub fn list_id(side: Side) -> Id {
    Id::new(match side {
        Side::Local => "files-local",
        Side::Remote => "files-remote",
    })
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
            _ if letter == Some('d') => Some(FilesKey::FocusPath),
            _ => None,
        };
    }
    let keyboard::Key::Named(named) = key else {
        return None;
    };
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

/// Narrowest a name is left before a column gives way, in logical pixels.
const NAME_MIN_WIDTH: f32 = 160.0;

/// The first of `columns` that fit in `width` beside a name at least
/// [`NAME_MIN_WIDTH`] wide: the last ones give way first, the name and the size never.
fn fitting_columns(columns: &[SortColumn], width: f32) -> Vec<SortColumn> {
    let mut shown = columns.to_vec();
    let needed = |shown: &[SortColumn]| {
        shown
            .iter()
            .map(|column| match column_width(*column) {
                Length::Fixed(fixed) => fixed + SPACING,
                _ => NAME_MIN_WIDTH,
            })
            .sum::<f32>()
    };
    while shown.len() > 2 && needed(&shown) > width {
        shown.pop();
    }
    shown
}

/// The width of `column`.
fn column_width(column: SortColumn) -> Length {
    match column {
        SortColumn::Name => Length::Fill,
        SortColumn::Size => Length::Fixed(SIZE_WIDTH),
        SortColumn::Modified => Length::Fixed(MODIFIED_WIDTH),
        SortColumn::Permissions => Length::Fixed(PERMISSIONS_WIDTH),
        SortColumn::Owner => Length::Fixed(OWNER_WIDTH),
    }
}

/// What an entry of the server is, as the C# Properties dialog shows it, with `ok` to
/// close it.
pub fn properties<'a>(
    properties: &FileProperties,
    ok: iced::widget::Button<'a, Message>,
) -> Element<'a, Message> {
    let kind = match properties.kind {
        EntryKind::File => fl!("ui-files-type-file"),
        EntryKind::Directory => fl!("ui-files-type-directory"),
        EntryKind::Link => fl!("ui-files-type-link"),
        EntryKind::Other(Special::Pipe) => fl!("ui-files-type-pipe"),
        EntryKind::Other(Special::Socket) => fl!("ui-files-type-socket"),
        EntryKind::Other(Special::Device) => fl!("ui-files-type-device"),
        EntryKind::Other(Special::Unknown) => fl!("ui-files-type-other"),
    };
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
    let mut content = column![
        text(fl!(
            "ui-files-properties-title",
            name = properties.name.as_str()
        ))
        .size(TITLE_SIZE)
    ]
    .spacing(SPACING);
    for (label, value) in lines {
        content = content.push(
            row![
                text(label).size(SMALL_SIZE).width(PROPERTY_LABEL_WIDTH),
                text(value)
            ]
            .spacing(SPACING),
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
            let mark = match entry.kind() {
                EntryKind::Directory => FOLDER_MARK,
                EntryKind::Link => LINK_MARK,
                EntryKind::File | EntryKind::Other(_) => "",
            };
            format!("{}{mark}", entry.label())
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

/// The headers of `columns`, a click sorting by one.
fn headers<'a>(tab: TabId, side: Side, columns: &[SortColumn], sort: Sort) -> Element<'a, Message> {
    let mut line = row![].spacing(SPACING);
    for column in columns {
        line = line.push(
            button(text(column_title(*column, sort)).size(SMALL_SIZE))
                .style(button::text)
                .padding(0)
                .width(column_width(*column))
                .on_press(files(FilesMessage::SortBy {
                    tab,
                    side,
                    column: *column,
                })),
        );
    }
    line.into()
}

fn entry_row<'a, E: Listed>(
    entry: &E,
    columns: &[SortColumn],
    (selected, target): (bool, bool),
    (tab, side, index): (TabId, Side, usize),
) -> Element<'a, Message> {
    let on_press = files(FilesMessage::Select { tab, side, index });
    let mut cells = row![].spacing(SPACING);
    for column in columns {
        let cell = text(cell_text(entry, *column)).width(column_width(*column));
        cells = cells.push(if *column == SortColumn::Name {
            cell
        } else {
            cell.size(SMALL_SIZE)
        });
    }
    let line = button(cells)
        .width(Length::Fill)
        .style(if target {
            // A drag over this folder: where the entries would go.
            button::success
        } else if selected {
            button::primary
        } else {
            button::text
        })
        .on_press(on_press);
    // A right click opens its menu, as in the C# Files tab.
    crate::files_drag::spot(
        mouse_area(line).on_right_press(Message::OpenTreeMenu(TreeMenu::FilesEntry {
            tab,
            side,
            index,
        })),
        crate::files_drag::Spot {
            tab,
            side,
            index: Some(index),
        },
    )
}

/// A pane's buttons: new folder, rename (one entry), delete; on the server's, its
/// bookmarks, as in the C# tab. The row wraps: a narrow pane keeps every button whole.
fn pane_tools<'a>(
    tab: TabId,
    side: Side,
    selected: Option<usize>,
    chosen: usize,
) -> Element<'a, Message> {
    let mut tools = row![
        button(text(fl!("ui-files-new-folder-button")).size(SMALL_SIZE))
            .style(button::secondary)
            .on_press(files(FilesMessage::AskNewFolder { tab, side })),
        button(text(fl!("ui-files-rename-button")).size(SMALL_SIZE))
            .style(button::secondary)
            // One entry at a time, as in the C# tab.
            .on_press_maybe((chosen == 1).then(|| files(FilesMessage::AskRename { tab, side })),),
        button(text(fl!("ui-files-delete-button")).size(SMALL_SIZE))
            .style(button::danger)
            .on_press_maybe(selected.map(|_| files(FilesMessage::AskDelete { tab, side }))),
    ]
    .spacing(SPACING);
    // The server's folders only, as in the C# tab.
    if side == Side::Remote {
        tools = tools
            .push(
                button(text(fl!("ui-files-bookmark-button")).size(SMALL_SIZE))
                    .style(button::secondary)
                    .on_press(files(FilesMessage::Bookmark { tab })),
            )
            .push(
                button(text(fl!("ui-files-bookmarks-button")).size(SMALL_SIZE))
                    .style(button::secondary)
                    .on_press(Message::OpenTreeMenu(TreeMenu::FilesBookmarks(tab))),
            );
    }
    tools.wrap().vertical_spacing(SPACING).into()
}

/// The C# filter and hidden-files toggle of a pane, lit while hidden names show.
fn pane_narrowing<'a>(
    tab: TabId,
    side: Side,
    filter: &str,
    show_hidden: bool,
) -> iced::widget::Row<'a, Message> {
    row![
        text_input(&fl!("ui-files-filter-placeholder"), filter)
            .id(field_id(side, PaneField::Filter))
            .size(SMALL_SIZE)
            .on_input(move |text| files(FilesMessage::Filter { tab, side, text }))
            .width(Length::Fill),
        tooltip(
            button(text(fl!("ui-files-hidden-toggle")).size(SMALL_SIZE))
                .style(if show_hidden {
                    button::primary
                } else {
                    button::secondary
                })
                .on_press(files(FilesMessage::ToggleHidden { tab, side })),
            text(fl!("ui-files-hidden-tooltip")).size(SMALL_SIZE),
            tooltip::Position::Bottom,
        )
        .style(container::rounded_box),
    ]
    .spacing(SPACING)
    .align_y(Alignment::Center)
}

/// A pane's title, how many entries it lists and shows as the C# tab counts them, and how
/// many are selected past one.
fn pane_heading<'a>(
    title: String,
    shown: usize,
    total: usize,
    chosen: usize,
) -> iced::widget::Row<'a, Message> {
    let count = if shown == total {
        fl!("ui-files-item-count", count = total)
    } else {
        fl!("ui-files-item-count-filtered", shown = shown, count = total)
    };
    let mut heading = row![text(title).size(TITLE_SIZE), text(count).size(SMALL_SIZE)]
        .spacing(SPACING)
        .align_y(Alignment::Center);
    if chosen > 1 {
        heading =
            heading.push(text(fl!("ui-files-selected-count", count = chosen)).size(SMALL_SIZE));
    }
    heading
}

/// What a pane is drawn from, shared by both sides.
struct PaneParts<'p, E> {
    tab: TabId,
    side: Side,
    title: String,
    location: String,
    typed: Option<&'p str>,
    entries: &'p [E],
    columns: &'p [SortColumn],
    sort: Sort,
    selected: Option<usize>,
    marked: &'p BTreeSet<usize>,
    filter: &'p str,
    show_hidden: bool,
    total: usize,
    loading: bool,
    error: Option<&'p FilesError>,
    focused: bool,
    /// Where the pane can go besides up.
    moves: Moves,
    /// The delete or the change of permissions running in this pane.
    batch: Option<&'p heimdall_app::files::Batch>,
    /// Where a drag would drop in this pane.
    drop: Option<DropHere>,
}

/// Where a pane can go besides up, as the C# Files tab's Back and Home.
#[derive(Debug, Clone, Copy)]
struct Moves {
    /// Back has a folder to go to.
    back: bool,
    /// Home is known: a folder was shown.
    home: bool,
}

/// What a pane listing nothing says, as the C# empty states: the folder is empty; or
/// nothing matches the filter, with a way to clear it; or only hidden entries are there,
/// with a way to show them.
fn empty_state<'a>(
    tab: TabId,
    side: Side,
    filter: &str,
    show_hidden: bool,
    total: usize,
) -> Element<'a, Message> {
    let said = |words: String| text(words).size(SMALL_SIZE);
    let way = |label: String, message: FilesMessage| {
        button(text(label).size(SMALL_SIZE))
            .style(button::secondary)
            .on_press(files(message))
    };
    if total == 0 {
        return said(fl!("ui-files-empty")).into();
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
        .spacing(SPACING)
        .into();
    }
    if show_hidden {
        return said(fl!("ui-files-empty")).into();
    }
    column![
        said(fl!("ui-files-empty-hidden-only")),
        way(
            fl!("ui-files-empty-show-hidden"),
            FilesMessage::ToggleHidden { tab, side }
        ),
    ]
    .spacing(SPACING)
    .into()
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
            .size(SMALL_SIZE)
            .style(text::secondary)
            .into()
    } else {
        button(text(fl!("ui-files-batch-stop")).size(SMALL_SIZE))
            .style(button::secondary)
            .on_press(files(FilesMessage::StopBatch { tab }))
            .into()
    };
    row![text(at).size(SMALL_SIZE).width(Length::Fill), stop]
        .spacing(SPACING)
        .align_y(Alignment::Center)
        .into()
}

fn pane<E: Listed>(parts: PaneParts<'_, E>) -> Element<'_, Message> {
    let PaneParts {
        tab,
        side,
        title,
        location,
        typed,
        entries,
        columns,
        sort,
        selected,
        marked,
        filter,
        show_hidden,
        total,
        loading,
        error,
        focused,
        moves,
        batch,
        drop,
    } = parts;
    let chosen = marked.len() + usize::from(selected.is_some());
    let tools = pane_tools(tab, side, selected, chosen);
    let header = row![
        // As the C# Files tab: Back, Up, Home.
        button(text(fl!("ui-files-back-button")).size(SMALL_SIZE))
            .style(button::secondary)
            .on_press_maybe(moves.back.then(|| files(FilesMessage::Back { tab, side }))),
        button(text(fl!("ui-files-up-button")).size(SMALL_SIZE))
            .style(button::secondary)
            .on_press(files(FilesMessage::Up { tab, side })),
        button(text(fl!("ui-files-home-button")).size(SMALL_SIZE))
            .style(button::secondary)
            .on_press_maybe(moves.home.then(|| files(FilesMessage::Home { tab, side }))),
        // The folder shown, typed over to go elsewhere, as the C# path bar.
        text_input(&location, typed.unwrap_or(&location))
            .id(field_id(side, PaneField::Path))
            .size(SMALL_SIZE)
            .on_input(move |text| files(FilesMessage::PathEdited { tab, side, text }))
            .on_submit(files(FilesMessage::GoTo { tab, side }))
            .width(Length::Fill),
        button(text(fl!("ui-files-go-button")).size(SMALL_SIZE))
            .style(button::secondary)
            .on_press_maybe(typed.map(|_| files(FilesMessage::GoTo { tab, side }))),
        button(text(fl!("ui-files-refresh-button")).size(SMALL_SIZE))
            .style(button::secondary)
            .on_press(files(FilesMessage::Refresh { tab, side })),
    ]
    .spacing(SPACING)
    .align_y(Alignment::Center);
    let narrowing = pane_narrowing(tab, side, filter, show_hidden);
    let failed = error.is_some();
    // The columns that fit beside a name still readable, laid out for the pane's width.
    let listing = responsive(move |size| {
        let shown = fitting_columns(columns, size.width);
        let mut list = Column::new().spacing(2.0);
        if loading {
            list = list.push(text(fl!("ui-files-loading")).size(SMALL_SIZE));
        } else if entries.is_empty() && !failed {
            list = list.push(empty_state(tab, side, filter, show_hidden, total));
        }
        for (index, entry) in entries.iter().enumerate() {
            let place = (tab, side, index);
            let picked = selected == Some(index) || marked.contains(&index);
            let target = drop == Some(DropHere::Entry(index));
            list = list.push(entry_row(entry, &shown, (picked, target), place));
        }
        column![
            headers(tab, side, &shown, sort),
            scrollable(list).id(list_id(side)).height(Length::Fill)
        ]
        .spacing(SPACING)
        .into()
    });
    let heading = pane_heading(title, entries.len(), total, chosen);
    let mut content = column![heading, header, tools, narrowing].spacing(SPACING);
    if let Some(error) = error {
        content = content.push(text(texts::files_error(error)).size(SMALL_SIZE));
    }
    if let Some(batch) = batch {
        content = content.push(batch_row(tab, batch));
    }
    pane_frame(
        content.push(listing),
        (tab, side),
        focused,
        drop == Some(DropHere::Pane),
    )
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
        .padding(PADDING)
        .width(Length::FillPortion(1))
        .height(Length::Fill)
        .style(move |theme: &Theme| {
            let mut style = container::bordered_box(theme);
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
/// anyway, and stopping.
fn edit_row(tab: TabId, edit: &EditSession) -> Element<'_, Message> {
    let local = || edit.local.clone();
    let state = match &edit.refused {
        Some(error) => text(texts::files_error(error))
            .size(SMALL_SIZE)
            .style(text::danger),
        None => text(fl!("ui-files-edit-watching")).size(SMALL_SIZE),
    };
    let mut line = row![
        text(edit.name.as_str()).width(Length::FillPortion(2)),
        state.width(Length::FillPortion(3)),
    ]
    .spacing(SPACING)
    .align_y(Alignment::Center);
    // Refused by the server's permissions, or by sudo for its password: sudo is offered.
    let sudo_helps = matches!(
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
            button(text(fl!("ui-files-edit-save-sudo")).size(SMALL_SIZE)).on_press(files(
                FilesMessage::EditSaveWithSudo {
                    tab,
                    local: local(),
                },
            )),
        );
    }
    if edit.refused.is_some() && !edit.privileged {
        line = line.push(
            button(text(fl!("ui-files-edit-send-anyway")).size(SMALL_SIZE)).on_press(files(
                FilesMessage::EditSendAnyway {
                    tab,
                    local: local(),
                },
            )),
        );
    }
    line.push(
        button(text(fl!("ui-files-edit-open-folder")).size(SMALL_SIZE))
            .style(button::secondary)
            .on_press(files(FilesMessage::EditOpenFolder {
                tab,
                local: local(),
            })),
    )
    .push(
        button(text(fl!("ui-files-edit-stop")).size(SMALL_SIZE))
            .style(button::secondary)
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
        text(state).size(SMALL_SIZE).width(Length::FillPortion(3)),
    ]
    .spacing(SPACING)
    .align_y(Alignment::Center);
    let small = |label: String, message: FilesMessage| {
        button(text(label).size(SMALL_SIZE))
            .style(button::secondary)
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

/// The Files tab.
#[must_use]
pub fn view(
    tab: TabId,
    files_pane: &FilesPane,
    live: bool,
    drop: Option<crate::files_drag::Spot>,
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
        title: fl!("ui-files-local-title"),
        location: local_location,
        typed: files_pane.local.typed.as_deref(),
        entries: &files_pane.local.entries,
        columns: LOCAL_COLUMNS,
        sort: files_pane.local.sort,
        selected: files_pane.local.selected,
        marked: &files_pane.local.marked,
        filter: &files_pane.local.filter,
        show_hidden: files_pane.local.show_hidden,
        total: files_pane.local.listing.len(),
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
    });
    let remote = pane(PaneParts {
        tab,
        side: Side::Remote,
        title: fl!("ui-files-remote-title"),
        location: remote_location,
        typed: files_pane.remote.typed.as_deref(),
        entries: &files_pane.remote.entries,
        columns: REMOTE_COLUMNS,
        sort: files_pane.remote.sort,
        selected: files_pane.remote.selected,
        marked: &files_pane.remote.marked,
        filter: &files_pane.remote.filter,
        show_hidden: files_pane.remote.show_hidden,
        total: files_pane.remote.listing.len(),
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
    });
    let can_upload = files_pane.local.selected.is_some();
    let can_download = files_pane.remote.selected.is_some();
    let actions = column![
        button(text(fl!("ui-files-upload-button"))).on_press_maybe(can_upload.then(|| files(
            FilesMessage::Transfer {
                tab,
                direction: Direction::Upload,
            }
        ))),
        button(text(fl!("ui-files-download-button"))).on_press_maybe(can_download.then(|| files(
            FilesMessage::Transfer {
                tab,
                direction: Direction::Download,
            }
        ))),
    ]
    .spacing(SPACING)
    .align_x(Alignment::Center);
    let panes = row![local, container(actions).center_y(Length::Fill), remote]
        .spacing(SPACING)
        .height(Length::Fill);
    let mut content = column![panes].spacing(SPACING).padding(PADDING);
    if !files_pane.edits.is_empty() {
        let list = files_pane
            .edits
            .iter()
            .fold(Column::new().spacing(4.0), |list, edit| {
                list.push(edit_row(tab, edit))
            });
        content = content
            .push(text(fl!("ui-files-edits-title")).size(TITLE_SIZE))
            .push(list);
    }
    if !files_pane.transfers.is_empty() {
        content = content.push(transfers(tab, files_pane, live));
    }
    content.into()
}

/// The transfers, newest first, under their title and "Clear finished" once one has ended.
fn transfers(tab: TabId, files_pane: &FilesPane, live: bool) -> Column<'_, Message> {
    let list = files_pane
        .transfers
        .iter()
        .rev()
        .fold(Column::new().spacing(4.0), |list, transfer| {
            list.push(transfer_row(tab, transfer, live))
        });
    let mut title = row![text(fl!("ui-files-transfers-title")).size(TITLE_SIZE)]
        .spacing(SPACING)
        .align_y(Alignment::Center);
    if files_pane
        .transfers
        .iter()
        .any(|transfer| transfer.state.ended())
    {
        title = title.push(
            button(text(fl!("ui-files-clear-finished-button")).size(SMALL_SIZE))
                .style(button::secondary)
                .on_press(files(FilesMessage::ClearFinished { tab })),
        );
    }
    column![
        title,
        container(scrollable(list)).max_height(TRANSFERS_HEIGHT)
    ]
    .spacing(SPACING)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn named(key: Named) -> keyboard::Key {
        keyboard::Key::Named(key)
    }

    #[test]
    fn the_last_columns_give_way_to_a_readable_name_but_never_the_size() {
        use SortColumn::{Modified, Name, Permissions, Size};
        assert_eq!(fitting_columns(REMOTE_COLUMNS, 2000.0), REMOTE_COLUMNS);
        // Name 160 + size 98 + modified 138 + permissions 98 + owner 63 = 557.
        assert_eq!(fitting_columns(REMOTE_COLUMNS, 557.0), REMOTE_COLUMNS);
        assert_eq!(
            fitting_columns(REMOTE_COLUMNS, 556.0),
            [Name, Size, Modified, Permissions]
        );
        assert_eq!(fitting_columns(REMOTE_COLUMNS, 300.0), [Name, Size]);
        assert_eq!(
            fitting_columns(REMOTE_COLUMNS, 10.0),
            [Name, Size],
            "never fewer"
        );
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
        assert_eq!(key(&letter("d"), Modifiers::ALT), Some(FilesKey::FocusPath));
        assert_eq!(
            key(&named(Named::Delete), Modifiers::CTRL | Modifiers::ALT),
            None,
            "AltGr is Ctrl+Alt: it types"
        );
        assert_eq!(key(&named(Named::Delete), Modifiers::LOGO), None);
    }
}
