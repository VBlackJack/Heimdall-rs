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

//! The Hosts File Editor tool view: the system hosts file read when the tab opens, as the
//! C# `Initialize`, entries enabled, disabled, edited in place, removed or added, and saved
//! without losing what the editor does not show.

use std::collections::BTreeSet;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use heimdall_app::TabId;
use iced::widget::{button, checkbox, column, container, row, scrollable, text, text_input};
use iced::{Alignment, Element, Length, Task, Theme};

use heimdall_core::tools::hosts_editor::{
    self, EntryError, HostEntry, HostsDocument, HostsError, LoadedHosts, SavedHosts,
};

use super::{CopySlot, ToolPane};
use crate::i18n::fl;
use crate::shell::Message;
use crate::styles;
use crate::tokens::{font_size, spacing};

/// Width of the column of the boxes that select a row.
const SELECT_WIDTH: f32 = 30.0;

/// Width of the Active column, as the C# `ColEnabled`.
const ENABLED_WIDTH: f32 = 60.0;

/// Width of the address column.
const IP_WIDTH: f32 = 140.0;

/// Width of the address box of a new entry.
const NEW_IP_WIDTH: f32 = 150.0;

/// Width of the column of a row's buttons: Edit and Delete, or Apply and Cancel.
const ACTIONS_WIDTH: f32 = 180.0;

/// Messages emitted by the Hosts File Editor view.
#[derive(Debug, Clone)]
pub enum HostsMessage {
    /// Reload hosts file from disk.
    Reload,
    /// The load of run `u64` ended; a run older than the latest is ignored.
    Loaded(u64, Result<LoadedHosts, HostsError>),
    /// Toggle whether an entry is active or commented out.
    ToggleEntry(usize),
    /// Delete an entry by index.
    DeleteEntry(usize),
    /// Input text for new entry IP address.
    NewIpChanged(String),
    /// Input text for new entry hostname.
    NewHostChanged(String),
    /// Input text for new entry comment.
    NewCommentChanged(String),
    /// Add the new entry.
    AddEntry,
    /// Save all changes to disk.
    Save,
    /// Save operation completed.
    Saved(Result<SavedHosts, HostsError>),
    /// Copy entire serialized hosts file to clipboard.
    CopyAll,
    /// Select (true) or unselect (false) an entry, as a row of the C# grid's
    /// `SelectionMode="Extended"`.
    SelectEntry(usize, bool),
    /// Select every entry (true), or none (false).
    SelectAll(bool),
    /// Delete every entry selected, as the C# `OnDeleteSelectedClick`.
    DeleteSelected,
    /// Copy the address of the entry selected, as the C# row menu's Copy IP Address.
    CopyIp,
    /// Copy the host name of the entry selected, as the C# row menu's Copy Hostname.
    CopyHostname,
    /// Start editing an entry in place, as the C# grid's cells.
    StartEdit(usize),
    /// Input text for the edited entry's IP address.
    EditIpChanged(String),
    /// Input text for the edited entry's host names.
    EditNamesChanged(String),
    /// Input text for the edited entry's comment.
    EditCommentChanged(String),
    /// Apply the edit to the entry, checked as an entry added.
    ApplyEdit,
    /// Leave the entry as it was.
    CancelEdit,
}

/// The file as last read: the bytes a save compares the disk with, and the lines edited.
#[derive(Debug)]
struct Loaded {
    original: Vec<u8>,
    document: HostsDocument,
}

/// An entry being edited in place: its index and what its boxes hold.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Editing {
    index: usize,
    ip: String,
    names: String,
    comment: String,
}

/// The Hosts File Editor tool pane state.
#[derive(Debug)]
pub struct HostsEditorPane {
    path: PathBuf,
    /// `None` until a load succeeded: nothing can be saved before, so an empty editor never
    /// replaces the file.
    loaded: Option<Loaded>,
    /// A load runs: the editor waits for it, and cannot save.
    loading: bool,
    /// A save runs: nothing else is done meanwhile.
    saving: bool,
    /// The latest load's run: an older one's result is stale.
    run: u64,
    new_ip: String,
    new_host: String,
    new_comment: String,
    /// The entries selected, by index.
    selected: BTreeSet<usize>,
    /// The entry edited in place, if any.
    editing: Option<Editing>,
    /// When the file was last written, as the C# `TxtLastModified`.
    modified: Option<SystemTime>,
    error: Option<String>,
    status: Option<String>,
}

impl Default for HostsEditorPane {
    fn default() -> Self {
        Self::new(hosts_editor::default_path())
    }
}

/// Outcome of updating the Hosts File Editor tool pane.
#[derive(Debug)]
pub enum Outcome {
    None,
    Task(Task<Message>),
    Copy(CopySlot, String),
}

impl HostsEditorPane {
    /// The editor of the hosts file at `path`, nothing read yet.
    #[must_use]
    pub fn new(path: PathBuf) -> Self {
        Self {
            path,
            loaded: None,
            loading: false,
            saving: false,
            run: 0,
            new_ip: String::new(),
            new_host: String::new(),
            new_comment: String::new(),
            selected: BTreeSet::new(),
            editing: None,
            modified: None,
            error: None,
            status: None,
        }
    }

    /// Whether Save can be pressed: a load succeeded, and neither a load nor a save runs.
    #[must_use]
    pub fn can_save(&self) -> bool {
        self.loaded.is_some() && !self.loading && !self.saving
    }

    /// Reads the file, as the C# `Initialize` does when the view opens.
    pub fn open(&mut self, tab: TabId) -> Task<Message> {
        self.start_load(tab)
    }

    /// Starts a new load run; the one before, still running, is ignored when it ends.
    fn start_load(&mut self, tab: TabId) -> Task<Message> {
        self.run += 1;
        let run = self.run;
        self.loading = true;
        self.error = None;
        self.status = None;
        let path = self.path.clone();
        Task::perform(
            async move {
                tokio::task::spawn_blocking(move || hosts_editor::load(&path))
                    .await
                    .unwrap_or_else(|error| Err(HostsError::Read(error.to_string())))
            },
            move |result| {
                Message::Tool(
                    tab,
                    super::ToolMessage::Hosts(HostsMessage::Loaded(run, result)),
                )
            },
        )
    }

    /// The document edited, when one was loaded and neither a load nor a save runs.
    fn editable(&mut self) -> Option<&mut HostsDocument> {
        if self.loading || self.saving {
            return None;
        }
        self.loaded.as_mut().map(|loaded| &mut loaded.document)
    }

    /// The number of entries shown.
    fn entry_count(&self) -> usize {
        self.loaded
            .as_ref()
            .map_or(0, |loaded| loaded.document.entries().count())
    }

    /// The one entry selected, when exactly one is.
    fn single_selected(&self) -> Option<&HostEntry> {
        if self.selected.len() != 1 {
            return None;
        }
        let index = *self.selected.first()?;
        self.loaded
            .as_ref()
            .and_then(|loaded| loaded.document.entries().nth(index))
    }

    /// The selection and the edit follow the entries left once those of `removed` are taken
    /// out: an entry removed is neither selected nor edited any more.
    fn follow_removal(&mut self, removed: &BTreeSet<usize>) {
        self.selected = self
            .selected
            .iter()
            .filter_map(|&index| index_after_removal(index, removed))
            .collect();
        self.editing = self.editing.take().and_then(|mut editing| {
            editing.index = index_after_removal(editing.index, removed)?;
            Some(editing)
        });
    }

    /// Handles tool messages.
    pub fn update(&mut self, tab: TabId, message: HostsMessage) -> Outcome {
        match message {
            HostsMessage::Reload => {
                if self.saving {
                    return Outcome::None;
                }
                Outcome::Task(self.start_load(tab))
            }
            HostsMessage::Loaded(run, _) if run != self.run => Outcome::None,
            HostsMessage::Loaded(_, Ok(loaded)) => {
                self.loading = false;
                let count = loaded.document.entries().count();
                // Indices of the file read before mean nothing in this one.
                self.selected.clear();
                self.editing = None;
                self.modified = loaded.modified;
                self.loaded = Some(Loaded {
                    original: loaded.original,
                    document: loaded.document,
                });
                self.status = Some(fl!(
                    "ui-tool-hosts-loaded",
                    count = count,
                    path = self.path.display().to_string()
                ));
                Outcome::None
            }
            HostsMessage::Loaded(_, Err(error)) => {
                // What was loaded before stays, and stays the base of a save.
                self.loading = false;
                self.error = Some(hosts_error_text(&error));
                Outcome::None
            }
            HostsMessage::ToggleEntry(idx) => {
                if let Some(document) = self.editable() {
                    document.toggle(idx);
                }
                Outcome::None
            }
            HostsMessage::DeleteEntry(idx) => {
                if let Some(document) = self.editable() {
                    document.remove(idx);
                    self.follow_removal(&BTreeSet::from([idx]));
                }
                Outcome::None
            }
            HostsMessage::DeleteSelected => {
                let removed = self.selected.clone();
                if removed.is_empty() {
                    return Outcome::None;
                }
                if let Some(document) = self.editable() {
                    document.remove_all(&removed);
                    self.follow_removal(&removed);
                }
                Outcome::None
            }
            HostsMessage::SelectEntry(idx, selected) => {
                if selected && idx < self.entry_count() {
                    self.selected.insert(idx);
                } else {
                    self.selected.remove(&idx);
                }
                Outcome::None
            }
            HostsMessage::SelectAll(selected) => {
                self.selected = if selected {
                    (0..self.entry_count()).collect()
                } else {
                    BTreeSet::new()
                };
                Outcome::None
            }
            HostsMessage::CopyIp => match self.single_selected() {
                Some(entry) => Outcome::Copy(CopySlot::HostsIp, entry.ip.clone()),
                None => Outcome::None,
            },
            HostsMessage::CopyHostname => match self.single_selected() {
                Some(entry) => Outcome::Copy(CopySlot::HostsHostname, entry.host.clone()),
                None => Outcome::None,
            },
            HostsMessage::StartEdit(idx) => {
                if self.loading || self.saving {
                    return Outcome::None;
                }
                let entry = self
                    .loaded
                    .as_ref()
                    .and_then(|loaded| loaded.document.entries().nth(idx));
                if let Some(entry) = entry {
                    self.editing = Some(Editing {
                        index: idx,
                        ip: entry.ip.clone(),
                        names: entry.names(),
                        comment: entry.comment.clone(),
                    });
                    self.error = None;
                }
                Outcome::None
            }
            HostsMessage::EditIpChanged(ip) => {
                if let Some(editing) = &mut self.editing {
                    editing.ip = ip;
                }
                Outcome::None
            }
            HostsMessage::EditNamesChanged(names) => {
                if let Some(editing) = &mut self.editing {
                    editing.names = names;
                }
                Outcome::None
            }
            HostsMessage::EditCommentChanged(comment) => {
                if let Some(editing) = &mut self.editing {
                    editing.comment = comment;
                }
                Outcome::None
            }
            HostsMessage::ApplyEdit => {
                let Some(editing) = self.editing.clone() else {
                    return Outcome::None;
                };
                let Some(document) = self.editable() else {
                    return Outcome::None;
                };
                match document.edit(editing.index, &editing.ip, &editing.names, &editing.comment) {
                    Ok(()) => {
                        self.editing = None;
                        self.error = None;
                    }
                    // The boxes keep what was typed, to be corrected.
                    Err(error) => self.error = Some(entry_error_text(error)),
                }
                Outcome::None
            }
            HostsMessage::CancelEdit => {
                self.editing = None;
                self.error = None;
                Outcome::None
            }
            HostsMessage::NewIpChanged(ip) => {
                self.new_ip = ip;
                Outcome::None
            }
            HostsMessage::NewHostChanged(host) => {
                self.new_host = host;
                Outcome::None
            }
            HostsMessage::NewCommentChanged(comment) => {
                self.new_comment = comment;
                Outcome::None
            }
            HostsMessage::AddEntry => {
                let (ip, host, comment) = (
                    self.new_ip.clone(),
                    self.new_host.clone(),
                    self.new_comment.clone(),
                );
                let Some(document) = self.editable() else {
                    return Outcome::None;
                };
                match document.add(&ip, &host, &comment) {
                    Ok(()) => {
                        self.error = None;
                        self.new_ip.clear();
                        self.new_host.clear();
                        self.new_comment.clear();
                    }
                    Err(error) => self.error = Some(entry_error_text(error)),
                }
                Outcome::None
            }
            HostsMessage::Save => {
                if !self.can_save() {
                    return Outcome::None;
                }
                let Some(loaded) = &self.loaded else {
                    return Outcome::None;
                };
                let original = loaded.original.clone();
                let document = loaded.document.clone();
                let path = self.path.clone();
                self.saving = true;
                self.error = None;
                self.status = None;
                let task = Task::perform(
                    async move {
                        tokio::task::spawn_blocking(move || {
                            hosts_editor::save(&path, &original, &document, SystemTime::now())
                        })
                        .await
                        .unwrap_or_else(|error| Err(HostsError::Write(error.to_string())))
                    },
                    move |res| {
                        Message::Tool(tab, super::ToolMessage::Hosts(HostsMessage::Saved(res)))
                    },
                );
                Outcome::Task(task)
            }
            HostsMessage::Saved(Ok(saved)) => {
                self.saving = false;
                // The bytes written hold the same entries in the same order: the selection
                // and an edit open keep their indices.
                self.modified = saved.modified;
                // What is on disk now is what the next save compares with.
                self.loaded = Some(Loaded {
                    document: HostsDocument::parse(&saved.written),
                    original: saved.written,
                });
                self.status = Some(match saved.backup {
                    Some(backup) => fl!(
                        "ui-tool-hosts-save-success",
                        backup = backup.display().to_string()
                    ),
                    None => fl!("ui-tool-hosts-unchanged"),
                });
                Outcome::None
            }
            HostsMessage::Saved(Err(error)) => {
                self.saving = false;
                self.error = Some(hosts_error_text(&error));
                Outcome::None
            }
            HostsMessage::CopyAll => match &self.loaded {
                Some(loaded) => Outcome::Copy(
                    CopySlot::HostsFileText,
                    String::from_utf8_lossy(&loaded.document.to_bytes()).into_owned(),
                ),
                None => Outcome::None,
            },
        }
    }

    /// The entries shown.
    fn entries(&self) -> Vec<&HostEntry> {
        self.loaded
            .as_ref()
            .map(|loaded| loaded.document.entries().collect())
            .unwrap_or_default()
    }

    /// Renders the Hosts File Editor tool view.
    pub fn view<'a>(
        &'a self,
        tab: TabId,
        state: &ToolPane,
        _theme: &Theme,
    ) -> Element<'a, Message> {
        let send = move |msg| Message::Tool(tab, super::ToolMessage::Hosts(msg));

        let reload_btn = super::action_button(
            fl!("ui-tool-hosts-reload"),
            false,
            (!self.saving).then(|| send(HostsMessage::Reload)),
        );

        let save_btn = super::action_button(
            fl!("ui-tool-hosts-save"),
            true,
            self.can_save().then(|| send(HostsMessage::Save)),
        );

        let copy_btn = super::copy_button(
            fl!("ui-tool-hosts-copy-all"),
            state.copied(CopySlot::HostsFileText),
            send(HostsMessage::CopyAll),
            super::COPY_PADDING,
        );

        let header_row = row![
            text(fl!("ui-tool-hosts-title"))
                .size(font_size::SUBTITLE)
                .width(Length::Fill),
            reload_btn,
            save_btn,
            copy_btn,
        ]
        .spacing(spacing::SM)
        .align_y(Alignment::Center);

        let header_card = container(header_row)
            .padding(spacing::MD)
            .style(styles::card);

        // Add new entry controls
        let ip_input = text_input(fl!("ui-tool-hosts-col-ip").as_str(), &self.new_ip)
            .on_input(move |val| send(HostsMessage::NewIpChanged(val)))
            .padding(super::INPUT_PADDING)
            .size(font_size::BODY)
            .width(Length::Fixed(NEW_IP_WIDTH));

        let host_input = text_input(fl!("ui-tool-hosts-col-hostname").as_str(), &self.new_host)
            .on_input(move |val| send(HostsMessage::NewHostChanged(val)))
            .padding(super::INPUT_PADDING)
            .size(font_size::BODY)
            .width(Length::FillPortion(2));

        let comment_input =
            text_input(fl!("ui-tool-hosts-col-comment").as_str(), &self.new_comment)
                .on_input(move |val| send(HostsMessage::NewCommentChanged(val)))
                .padding(super::INPUT_PADDING)
                .size(font_size::BODY)
                .width(Length::FillPortion(1));

        let add_btn = button(text(fl!("ui-tool-hosts-btn-add")).size(font_size::BODY))
            .padding(super::PRIMARY_PADDING)
            .style(styles::secondary)
            .on_press(send(HostsMessage::AddEntry));

        let add_row = row![ip_input, host_input, comment_input, add_btn]
            .spacing(spacing::SM)
            .align_y(Alignment::Center);

        let add_card = container(add_row).padding(spacing::MD).style(styles::card);

        let mut body = column![header_card, add_card].spacing(spacing::MD);

        if let Some(err) = &self.error {
            body = body.push(
                container(text(err.as_str()).size(font_size::BODY).style(text::danger))
                    .padding(spacing::MD)
                    .style(styles::card),
            );
        }

        if let Some(status) = &self.status {
            body = body.push(
                text(status.as_str())
                    .size(font_size::CAPTION)
                    .style(text::secondary),
            );
        }

        // The file's last write, as the C# status bar's `TxtLastModified`.
        if let Some(time) = self.modified.and_then(unix_seconds) {
            body = body.push(
                text(fl!(
                    "ui-tool-hosts-last-modified",
                    time = heimdall_app::local_time_text(time)
                ))
                .size(font_size::CAPTION)
                .style(text::secondary),
            );
        }

        let entries = self.entries();
        if !entries.is_empty() {
            let all_selected = self.selected.len() == entries.len();
            let select_all = checkbox(all_selected)
                .style(styles::checkbox)
                .label(fl!("ui-tool-hosts-select-all"))
                .text_size(font_size::CAPTION)
                .on_toggle(move |on| send(HostsMessage::SelectAll(on)));

            let table_headers = row![
                iced::widget::space().width(Length::Fixed(SELECT_WIDTH)),
                text(fl!("ui-tool-hosts-col-enabled"))
                    .size(font_size::CAPTION)
                    .style(text::secondary)
                    .width(Length::Fixed(ENABLED_WIDTH)),
                text(fl!("ui-tool-hosts-col-ip"))
                    .size(font_size::CAPTION)
                    .style(text::secondary)
                    .width(Length::Fixed(IP_WIDTH)),
                text(fl!("ui-tool-hosts-col-hostname"))
                    .size(font_size::CAPTION)
                    .style(text::secondary)
                    .width(Length::FillPortion(2)),
                text(fl!("ui-tool-hosts-col-comment"))
                    .size(font_size::CAPTION)
                    .style(text::secondary)
                    .width(Length::FillPortion(1)),
                text(fl!("ui-tool-hosts-col-action"))
                    .size(font_size::CAPTION)
                    .style(text::secondary)
                    .width(Length::Fixed(ACTIONS_WIDTH)),
            ]
            .spacing(spacing::SM);

            let mut entry_rows = column![select_all, table_headers].spacing(spacing::XS);
            for (idx, entry) in entries.into_iter().enumerate() {
                entry_rows = entry_rows.push(self.entry_row(idx, entry, send));
            }

            let mut table = column![].spacing(spacing::SM);
            if let Some(toolbar) = self.selection_toolbar(state, send) {
                table = table.push(toolbar);
            }
            table = table.push(scrollable(entry_rows));

            let card = container(table).padding(spacing::MD).style(styles::card);

            body = body.push(card);
        } else if self.loading {
            body = body.push(super::empty_state(fl!("ui-tool-hosts-loading-hint")));
        } else if self.loaded.is_some() {
            body = body.push(super::empty_state(fl!("ui-tool-hosts-empty-state")));
        }

        super::content_column(body)
    }

    /// The row of entry `idx`: its boxes when it is the one edited, its text otherwise.
    fn entry_row<'a>(
        &'a self,
        idx: usize,
        entry: &'a HostEntry,
        send: impl Fn(HostsMessage) -> Message + Copy + 'a,
    ) -> Element<'a, Message> {
        let select = checkbox(self.selected.contains(&idx))
            .style(styles::checkbox)
            .on_toggle(move |on| send(HostsMessage::SelectEntry(idx, on)));
        let enabled = checkbox(entry.enabled)
            .style(styles::checkbox)
            .on_toggle(move |_| send(HostsMessage::ToggleEntry(idx)));
        let small_button = |label: String, message: HostsMessage| {
            button(text(label).size(font_size::CAPTION))
                .style(styles::subtle)
                .on_press(send(message))
        };

        let editing = self.editing.as_ref().filter(|editing| editing.index == idx);
        let (ip, names, comment, actions): (
            Element<'a, Message>,
            Element<'a, Message>,
            Element<'a, Message>,
            Element<'a, Message>,
        ) = match editing {
            Some(editing) => {
                let field = |placeholder: String, value: &'a str| {
                    text_input(placeholder.as_str(), value)
                        .padding(super::INPUT_PADDING)
                        .size(font_size::BODY)
                        .font(super::BOX_FONT)
                        .on_submit(send(HostsMessage::ApplyEdit))
                };
                (
                    field(fl!("ui-tool-hosts-col-ip"), &editing.ip)
                        .on_input(move |value| send(HostsMessage::EditIpChanged(value)))
                        .width(Length::Fixed(IP_WIDTH))
                        .into(),
                    field(fl!("ui-tool-hosts-col-names"), &editing.names)
                        .on_input(move |value| send(HostsMessage::EditNamesChanged(value)))
                        .width(Length::FillPortion(2))
                        .into(),
                    field(fl!("ui-tool-hosts-col-comment"), &editing.comment)
                        .on_input(move |value| send(HostsMessage::EditCommentChanged(value)))
                        .width(Length::FillPortion(1))
                        .into(),
                    row![
                        button(text(fl!("ui-tool-hosts-btn-apply")).size(font_size::CAPTION))
                            .style(styles::secondary)
                            .on_press_maybe(
                                (!self.saving && !self.loading)
                                    .then(|| send(HostsMessage::ApplyEdit)),
                            ),
                        small_button(fl!("ui-tool-hosts-btn-cancel"), HostsMessage::CancelEdit),
                    ]
                    .spacing(spacing::XS)
                    .width(Length::Fixed(ACTIONS_WIDTH))
                    .into(),
                )
            }
            None => (
                text(&entry.ip)
                    .size(font_size::BODY)
                    .font(super::BOX_FONT)
                    .width(Length::Fixed(IP_WIDTH))
                    .into(),
                text(entry.names())
                    .size(font_size::BODY)
                    .font(super::BOX_FONT)
                    .width(Length::FillPortion(2))
                    .into(),
                text(&entry.comment)
                    .size(font_size::BODY)
                    .font(super::BOX_FONT)
                    .style(text::secondary)
                    .width(Length::FillPortion(1))
                    .into(),
                row![
                    small_button(fl!("ui-tool-hosts-btn-edit"), HostsMessage::StartEdit(idx)),
                    small_button(
                        fl!("ui-tool-hosts-btn-delete"),
                        HostsMessage::DeleteEntry(idx)
                    ),
                ]
                .spacing(spacing::XS)
                .width(Length::Fixed(ACTIONS_WIDTH))
                .into(),
            ),
        };

        row![
            container(select).width(Length::Fixed(SELECT_WIDTH)),
            container(enabled).width(Length::Fixed(ENABLED_WIDTH)),
            ip,
            names,
            comment,
            actions,
        ]
        .spacing(spacing::SM)
        .align_y(Alignment::Center)
        .into()
    }

    /// What can be done with the entries selected, as the C# Delete Selected button and row
    /// menu: shown while one or more are.
    fn selection_toolbar<'a>(
        &'a self,
        state: &ToolPane,
        send: impl Fn(HostsMessage) -> Message + Copy + 'a,
    ) -> Option<Element<'a, Message>> {
        if self.selected.is_empty() {
            return None;
        }
        let mut toolbar = row![
            text(fl!(
                "ui-tool-hosts-selected-count",
                count = self.selected.len()
            ))
            .size(font_size::BODY)
            .width(Length::Fill),
            button(text(fl!("ui-tool-hosts-btn-delete-selected")).size(font_size::BODY))
                .padding(super::COPY_PADDING)
                .style(styles::secondary)
                .on_press_maybe(
                    (!self.saving && !self.loading).then(|| send(HostsMessage::DeleteSelected)),
                ),
        ]
        .spacing(spacing::SM)
        .align_y(Alignment::Center);
        // The C# row menu copies the address and the name of the row it opens on.
        if self.single_selected().is_some() {
            toolbar = toolbar
                .push(super::copy_button(
                    fl!("ui-tool-hosts-copy-ip"),
                    state.copied(CopySlot::HostsIp),
                    send(HostsMessage::CopyIp),
                    super::COPY_PADDING,
                ))
                .push(super::copy_button(
                    fl!("ui-tool-hosts-copy-hostname"),
                    state.copied(CopySlot::HostsHostname),
                    send(HostsMessage::CopyHostname),
                    super::COPY_PADDING,
                ));
        }
        Some(toolbar.into())
    }
}

/// Where entry `index` stands once the entries of `removed` are taken out; `None` when it is
/// one of them.
fn index_after_removal(index: usize, removed: &BTreeSet<usize>) -> Option<usize> {
    (!removed.contains(&index)).then(|| index - removed.range(..index).count())
}

/// `time` in seconds since the Unix epoch, when it is after it.
fn unix_seconds(time: SystemTime) -> Option<i64> {
    time.duration_since(UNIX_EPOCH)
        .ok()
        .and_then(|elapsed| i64::try_from(elapsed.as_secs()).ok())
}

/// What `error`, of a load or a save, says to the user.
fn hosts_error_text(error: &HostsError) -> String {
    match error {
        HostsError::NotFound => fl!("ui-tool-hosts-error-not-found"),
        HostsError::ReadDenied => fl!("ui-tool-hosts-error-read-denied"),
        HostsError::Read(detail) => fl!("ui-tool-hosts-error-read", error = detail.clone()),
        HostsError::AdminRequired => fl!("ui-tool-hosts-error-admin-required"),
        HostsError::ChangedOnDisk => fl!("ui-tool-hosts-error-changed"),
        HostsError::Backup(detail) => fl!("ui-tool-hosts-error-backup", error = detail.clone()),
        HostsError::Write(detail) => fl!("ui-tool-hosts-save-failed", error = detail.clone()),
    }
}

/// What `error`, of an entry typed in, says to the user.
fn entry_error_text(error: EntryError) -> String {
    match error {
        EntryError::InvalidIp => fl!("ui-tool-hosts-invalid-ip"),
        EntryError::MissingHost => fl!("ui-tool-hosts-missing-host"),
        EntryError::InvalidHost => fl!("ui-tool-hosts-invalid-host"),
        EntryError::InvalidComment => fl!("ui-tool-hosts-invalid-comment"),
    }
}

#[cfg(test)]
mod tests {
    use heimdall_app::tools::ToolId;
    use heimdall_app::{App, AppConfig, Message as AppMessage, ToolsMessage};
    use heimdall_ssh::AgentSource;
    use heimdall_term::GridSize;

    use super::*;

    /// A tool tab's identifier, from an application of its own in `dir`.
    fn tab(dir: &std::path::Path) -> TabId {
        let mut app = App::new(AppConfig {
            profiles_file: dir.join("profiles.toml"),
            known_hosts: dir.join("known_hosts"),
            legacy_dir: None,
            agent: AgentSource::Disabled,
            initial_grid: GridSize { cols: 80, rows: 24 },
            files_start: dir.to_owned(),
            system_credentials: heimdall_app::SystemCredentials::memory(),
        });
        let _ = app.update(AppMessage::Tools(ToolsMessage::Open {
            tool: ToolId::Hosts,
            title: String::new(),
            context: heimdall_app::tools::ToolContext::default(),
        }));
        app.tabs[0].id
    }

    fn loaded(bytes: &[u8]) -> LoadedHosts {
        LoadedHosts {
            original: bytes.to_vec(),
            document: HostsDocument::parse(bytes),
            modified: None,
        }
    }

    /// A pane in `dir` that has loaded `bytes`.
    fn pane_with(dir: &std::path::Path, tab: TabId, bytes: &[u8]) -> HostsEditorPane {
        let mut pane = HostsEditorPane::new(dir.join("hosts"));
        let _ = pane.open(tab);
        let _ = pane.update(tab, HostsMessage::Loaded(pane.run, Ok(loaded(bytes))));
        pane
    }

    /// The bytes the pane would save.
    fn bytes(pane: &HostsEditorPane) -> Vec<u8> {
        pane.loaded
            .as_ref()
            .map(|loaded| loaded.document.to_bytes())
            .unwrap_or_default()
    }

    #[test]
    fn an_entry_is_edited_in_place_checked_and_a_cancel_leaves_it() {
        let dir = tempfile::tempdir().expect("dir");
        let tab = tab(dir.path());
        let sample = b"127.0.0.1 a\n# keep\n10.0.0.1 b # old\n";
        let mut pane = pane_with(dir.path(), tab, sample);

        let _ = pane.update(tab, HostsMessage::StartEdit(1));
        let editing = pane.editing.clone().expect("editing");
        assert_eq!(
            (
                editing.ip.as_str(),
                editing.names.as_str(),
                editing.comment.as_str()
            ),
            ("10.0.0.1", "b", "old")
        );

        // A bad address is said, the boxes keep what was typed and the file is unchanged.
        let _ = pane.update(tab, HostsMessage::EditIpChanged("10.0.0".to_owned()));
        let _ = pane.update(tab, HostsMessage::ApplyEdit);
        assert!(pane.error.is_some());
        assert_eq!(pane.editing.as_ref().map(|e| e.ip.as_str()), Some("10.0.0"));
        assert_eq!(bytes(&pane), sample);

        // A bad name too.
        let _ = pane.update(tab, HostsMessage::EditIpChanged("10.0.0.2".to_owned()));
        let _ = pane.update(tab, HostsMessage::EditNamesChanged("c #d".to_owned()));
        let _ = pane.update(tab, HostsMessage::ApplyEdit);
        assert!(pane.error.is_some());
        assert_eq!(bytes(&pane), sample);

        let _ = pane.update(tab, HostsMessage::EditNamesChanged("c d".to_owned()));
        let _ = pane.update(tab, HostsMessage::EditCommentChanged(String::new()));
        let _ = pane.update(tab, HostsMessage::ApplyEdit);
        assert!(pane.editing.is_none());
        assert!(pane.error.is_none());
        assert_eq!(bytes(&pane), b"127.0.0.1 a\n# keep\n10.0.0.2\tc d\n");

        // Cancelled, an edit changes nothing.
        let _ = pane.update(tab, HostsMessage::StartEdit(0));
        let _ = pane.update(tab, HostsMessage::EditIpChanged("10.9.9.9".to_owned()));
        let _ = pane.update(tab, HostsMessage::CancelEdit);
        let _ = pane.update(tab, HostsMessage::ApplyEdit);
        assert_eq!(bytes(&pane), b"127.0.0.1 a\n# keep\n10.0.0.2\tc d\n");
    }

    #[test]
    fn no_edit_while_a_save_runs_and_a_reload_drops_the_edit() {
        let dir = tempfile::tempdir().expect("dir");
        let tab = tab(dir.path());
        let mut pane = pane_with(dir.path(), tab, b"127.0.0.1 a\n");
        let _ = pane.update(tab, HostsMessage::StartEdit(0));
        let _ = pane.update(tab, HostsMessage::EditNamesChanged("b".to_owned()));
        pane.saving = true;
        let _ = pane.update(tab, HostsMessage::ApplyEdit);
        assert_eq!(bytes(&pane), b"127.0.0.1 a\n");
        pane.saving = false;
        let _ = pane.update(tab, HostsMessage::Reload);
        let _ = pane.update(
            tab,
            HostsMessage::Loaded(pane.run, Ok(loaded(b"127.0.0.1 a\n"))),
        );
        assert!(pane.editing.is_none());
    }

    #[test]
    fn the_entries_selected_are_deleted_at_once_and_the_edit_follows() {
        let dir = tempfile::tempdir().expect("dir");
        let tab = tab(dir.path());
        let mut pane = pane_with(
            dir.path(),
            tab,
            b"# head\n10.0.0.1 a\n10.0.0.2 b\n# keep\n10.0.0.3 c\n10.0.0.4 d\n",
        );
        let _ = pane.update(tab, HostsMessage::SelectEntry(0, true));
        let _ = pane.update(tab, HostsMessage::SelectEntry(2, true));
        // Past the entries: never selected.
        let _ = pane.update(tab, HostsMessage::SelectEntry(9, true));
        assert_eq!(pane.selected, BTreeSet::from([0, 2]));
        let _ = pane.update(tab, HostsMessage::StartEdit(3));

        let _ = pane.update(tab, HostsMessage::DeleteSelected);
        assert_eq!(bytes(&pane), b"# head\n10.0.0.2 b\n# keep\n10.0.0.4 d\n");
        assert!(pane.selected.is_empty());
        // "10.0.0.4 d" was entry 3, it is entry 1 now.
        assert_eq!(pane.editing.as_ref().map(|e| e.index), Some(1));
        assert_eq!(
            pane.editing.as_ref().map(|e| e.ip.as_str()),
            Some("10.0.0.4")
        );

        // Select all, then none.
        let _ = pane.update(tab, HostsMessage::SelectAll(true));
        assert_eq!(pane.selected, BTreeSet::from([0, 1]));
        let _ = pane.update(tab, HostsMessage::SelectAll(false));
        assert!(pane.selected.is_empty());

        // A single delete shifts the selection and drops the edit of the entry deleted.
        let _ = pane.update(tab, HostsMessage::SelectEntry(1, true));
        let _ = pane.update(tab, HostsMessage::DeleteEntry(1));
        assert!(pane.selected.is_empty());
        assert!(pane.editing.is_none());
        assert_eq!(bytes(&pane), b"# head\n10.0.0.2 b\n# keep\n");
    }

    #[test]
    fn the_address_and_name_of_the_one_entry_selected_are_copied() {
        let dir = tempfile::tempdir().expect("dir");
        let tab = tab(dir.path());
        let mut pane = pane_with(dir.path(), tab, b"10.0.0.1 a\n10.0.0.2 b alias\n");
        assert!(matches!(
            pane.update(tab, HostsMessage::CopyIp),
            Outcome::None
        ));
        let _ = pane.update(tab, HostsMessage::SelectEntry(1, true));
        match pane.update(tab, HostsMessage::CopyIp) {
            Outcome::Copy(slot, text) => {
                assert_eq!((slot, text.as_str()), (CopySlot::HostsIp, "10.0.0.2"));
            }
            other => panic!("{other:?}"),
        }
        match pane.update(tab, HostsMessage::CopyHostname) {
            Outcome::Copy(slot, text) => {
                assert_eq!((slot, text.as_str()), (CopySlot::HostsHostname, "b"));
            }
            other => panic!("{other:?}"),
        }
        // Two entries selected: nothing to copy.
        let _ = pane.update(tab, HostsMessage::SelectEntry(0, true));
        assert!(matches!(
            pane.update(tab, HostsMessage::CopyHostname),
            Outcome::None
        ));
    }

    #[test]
    fn an_index_follows_the_entries_removed_before_it() {
        let removed = BTreeSet::from([1, 3]);
        assert_eq!(index_after_removal(0, &removed), Some(0));
        assert_eq!(index_after_removal(1, &removed), None);
        assert_eq!(index_after_removal(2, &removed), Some(1));
        assert_eq!(index_after_removal(5, &removed), Some(3));
    }

    #[test]
    fn save_is_refused_until_a_load_succeeded_and_while_one_runs() {
        let dir = tempfile::tempdir().expect("dir");
        let hosts = dir.path().join("hosts");
        std::fs::write(&hosts, b"127.0.0.1 a\n").expect("written");
        let tab = tab(dir.path());
        let mut pane = HostsEditorPane::new(hosts.clone());

        // Nothing loaded: Save does nothing, and the file is untouched.
        assert!(!pane.can_save());
        assert!(matches!(
            pane.update(tab, HostsMessage::Save),
            Outcome::None
        ));
        assert!(!pane.saving);

        // A load that failed leaves nothing to save either.
        let _ = pane.open(tab);
        assert!(pane.loading && !pane.can_save());
        let _ = pane.update(
            tab,
            HostsMessage::Loaded(pane.run, Err(HostsError::ReadDenied)),
        );
        assert!(!pane.can_save());
        assert!(pane.error.is_some());

        // Loaded: Save offered; a reload running takes it away again.
        let _ = pane.update(tab, HostsMessage::Reload);
        let _ = pane.update(
            tab,
            HostsMessage::Loaded(pane.run, Ok(loaded(b"127.0.0.1 a\n"))),
        );
        assert!(pane.can_save());
        let _ = pane.update(tab, HostsMessage::Reload);
        assert!(!pane.can_save());
        assert!(matches!(
            pane.update(tab, HostsMessage::Save),
            Outcome::None
        ));
        assert_eq!(std::fs::read(&hosts).expect("read"), b"127.0.0.1 a\n");
    }

    #[test]
    fn a_stale_load_never_replaces_a_newer_one() {
        let dir = tempfile::tempdir().expect("dir");
        let tab = tab(dir.path());
        let mut pane = HostsEditorPane::new(dir.path().join("hosts"));
        let _ = pane.open(tab);
        let first = pane.run;
        let _ = pane.update(tab, HostsMessage::Reload);
        let second = pane.run;
        let _ = pane.update(
            tab,
            HostsMessage::Loaded(second, Ok(loaded(b"10.0.0.2 newer\n"))),
        );
        let _ = pane.update(
            tab,
            HostsMessage::Loaded(first, Ok(loaded(b"10.0.0.1 older\n"))),
        );
        let names: Vec<String> = pane.entries().iter().map(|entry| entry.names()).collect();
        assert_eq!(names, ["newer"]);
        assert!(pane.can_save());
    }

    #[test]
    fn edits_wait_for_the_load_and_a_bad_entry_is_said() {
        let dir = tempfile::tempdir().expect("dir");
        let tab = tab(dir.path());
        let mut pane = HostsEditorPane::new(dir.path().join("hosts"));
        let _ = pane.update(tab, HostsMessage::NewIpChanged("10.0.0.1".to_owned()));
        let _ = pane.update(tab, HostsMessage::NewHostChanged("a".to_owned()));
        let _ = pane.update(tab, HostsMessage::AddEntry);
        assert!(pane.entries().is_empty(), "nothing loaded, nothing added");

        let _ = pane.open(tab);
        let _ = pane.update(tab, HostsMessage::Loaded(pane.run, Ok(loaded(b""))));
        let _ = pane.update(tab, HostsMessage::NewHostChanged("a b".to_owned()));
        let _ = pane.update(tab, HostsMessage::AddEntry);
        assert!(pane.entries().is_empty());
        assert!(pane.error.is_some());
        let _ = pane.update(tab, HostsMessage::NewHostChanged("a".to_owned()));
        let _ = pane.update(tab, HostsMessage::AddEntry);
        assert_eq!(pane.entries().len(), 1);
        assert!(pane.error.is_none());
    }
}
