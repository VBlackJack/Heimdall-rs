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
//! and End select, Enter opens or sends, Backspace goes up, Tab or Left and Right change
//! pane, F2 renames, Delete deletes and F5 lists again. Tab and Enter reach the tab through
//! the window, as they act on a dialog first.

use std::time::SystemTime;

use heimdall_app::files::{
    Direction, EntryKind, FilesError, FilesKey, FilesPane, Listed, Side, Sort, SortColumn,
    Transfer, TransferState, symbolic_mode,
};
use heimdall_app::{FilesMessage, Message as AppMessage, TabId};
use heimdall_core::utc::UtcTime;
use iced::keyboard::{self, Modifiers, key::Named};
use iced::widget::Id;
use iced::widget::{
    Column, button, column, container, mouse_area, responsive, row, scrollable, text, text_input,
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

/// Widget identifier of a pane's list, to scroll the selection into view.
#[must_use]
pub fn list_id(side: Side) -> Id {
    Id::new(match side {
        Side::Local => "files-local",
        Side::Remote => "files-remote",
    })
}

/// What `key` does in a Files tab. Keys with Ctrl, Alt or the logo key are left to the
/// window; Enter and Tab reach the tab through the window, as they act on a dialog first.
#[must_use]
pub fn files_key(key: &keyboard::Key, modifiers: Modifiers) -> Option<FilesKey> {
    if modifiers.control() || modifiers.alt() || modifiers.logo() {
        return None;
    }
    let keyboard::Key::Named(named) = key else {
        return None;
    };
    Some(match named {
        Named::ArrowUp => FilesKey::Previous,
        Named::ArrowDown => FilesKey::Next,
        Named::Home => FilesKey::First,
        Named::End => FilesKey::Last,
        Named::Backspace => FilesKey::Parent,
        Named::ArrowLeft => FilesKey::Focus(Side::Local),
        Named::ArrowRight => FilesKey::Focus(Side::Remote),
        Named::F2 => FilesKey::Rename,
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

/// `time` as the C# column shows it, `2026-09-27 21:05`, in UTC.
fn modified_text(time: SystemTime) -> String {
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
                EntryKind::File | EntryKind::Other => "",
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
    selected: bool,
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
        .style(if selected {
            button::primary
        } else {
            button::text
        })
        .on_press(on_press);
    // A right click opens its menu, as in the C# Files tab.
    mouse_area(line)
        .on_right_press(Message::OpenTreeMenu(TreeMenu::FilesEntry {
            tab,
            side,
            index,
        }))
        .into()
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
    loading: bool,
    error: Option<&'p FilesError>,
    focused: bool,
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
        loading,
        error,
        focused,
    } = parts;
    let tools = row![
        button(text(fl!("ui-files-new-folder-button")).size(SMALL_SIZE))
            .style(button::secondary)
            .on_press(files(FilesMessage::AskNewFolder { tab, side })),
        button(text(fl!("ui-files-rename-button")).size(SMALL_SIZE))
            .style(button::secondary)
            .on_press_maybe(selected.map(|_| files(FilesMessage::AskRename { tab, side }))),
        button(text(fl!("ui-files-delete-button")).size(SMALL_SIZE))
            .style(button::danger)
            .on_press_maybe(selected.map(|_| files(FilesMessage::AskDelete { tab, side }))),
    ]
    .spacing(SPACING);
    let header = row![
        button(text(fl!("ui-files-up-button")).size(SMALL_SIZE))
            .style(button::secondary)
            .on_press(files(FilesMessage::Up { tab, side })),
        // The folder shown, typed over to go elsewhere, as the C# path bar.
        text_input(&location, typed.unwrap_or(&location))
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
    let failed = error.is_some();
    // The columns that fit beside a name still readable, laid out for the pane's width.
    let listing = responsive(move |size| {
        let shown = fitting_columns(columns, size.width);
        let mut list = Column::new().spacing(2.0);
        if loading {
            list = list.push(text(fl!("ui-files-loading")).size(SMALL_SIZE));
        } else if entries.is_empty() && !failed {
            list = list.push(text(fl!("ui-files-empty")).size(SMALL_SIZE));
        }
        for (index, entry) in entries.iter().enumerate() {
            let place = (tab, side, index);
            list = list.push(entry_row(entry, &shown, selected == Some(index), place));
        }
        column![
            headers(tab, side, &shown, sort),
            scrollable(list).id(list_id(side)).height(Length::Fill)
        ]
        .spacing(SPACING)
        .into()
    });
    let mut content = column![text(title).size(TITLE_SIZE), header, tools].spacing(SPACING);
    if let Some(error) = error {
        content = content.push(text(texts::files_error(error)).size(SMALL_SIZE));
    }
    container(content.push(listing))
        .padding(PADDING)
        .width(Length::FillPortion(1))
        .height(Length::Fill)
        .style(move |theme: &Theme| {
            let mut style = container::bordered_box(theme);
            if focused {
                style.border.color = theme.extended_palette().primary.base.color;
                style.border.width = FOCUS_BORDER_WIDTH;
            }
            style
        })
        .into()
}

fn transfer_row(tab: TabId, transfer: &Transfer) -> Element<'_, Message> {
    let what = match transfer.direction {
        Direction::Download => {
            fl!("ui-files-transfer-download", name = transfer.label.as_str())
        }
        Direction::Upload => fl!("ui-files-transfer-upload", name = transfer.label.as_str()),
    };
    let state = match &transfer.state {
        TransferState::Running => match transfer.total {
            Some(total) => fl!(
                "ui-files-state-running",
                done = texts::size(transfer.bytes),
                total = texts::size(total)
            ),
            None => fl!(
                "ui-files-state-running-unknown",
                done = texts::size(transfer.bytes)
            ),
        },
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
    if transfer.state == TransferState::Running {
        line = line.push(
            button(text(fl!("ui-files-cancel-button")).size(SMALL_SIZE))
                .style(button::secondary)
                .on_press(files(FilesMessage::Cancel {
                    tab,
                    id: transfer.id,
                })),
        );
    }
    line.into()
}

/// The Files tab.
#[must_use]
pub fn view(tab: TabId, files_pane: &FilesPane) -> Element<'_, Message> {
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
        loading: files_pane.local.loading,
        error: files_pane.local.error.as_ref(),
        focused: files_pane.focus == Side::Local,
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
        loading: files_pane.remote.loading,
        error: files_pane.remote.error.as_ref(),
        focused: files_pane.focus == Side::Remote,
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
    if !files_pane.transfers.is_empty() {
        let list = files_pane
            .transfers
            .iter()
            .rev()
            .fold(Column::new().spacing(4.0), |list, transfer| {
                list.push(transfer_row(tab, transfer))
            });
        content = content
            .push(text(fl!("ui-files-transfers-title")).size(TITLE_SIZE))
            .push(container(scrollable(list)).max_height(TRANSFERS_HEIGHT));
    }
    content.into()
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
    fn plain_keys_act_and_modified_ones_are_left_to_the_window() {
        assert_eq!(
            files_key(&named(Named::Delete), Modifiers::empty()),
            Some(FilesKey::Delete)
        );
        for modifiers in [Modifiers::CTRL, Modifiers::ALT, Modifiers::LOGO] {
            assert_eq!(
                files_key(&named(Named::Delete), modifiers),
                None,
                "{modifiers:?}"
            );
        }
        assert_eq!(
            files_key(&named(Named::Enter), Modifiers::empty()),
            None,
            "Enter answers a dialog first"
        );
        assert_eq!(
            files_key(&keyboard::Key::Character("a".into()), Modifiers::empty()),
            None
        );
    }
}
