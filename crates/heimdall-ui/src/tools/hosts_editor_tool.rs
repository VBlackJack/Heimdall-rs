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
//! C# `Initialize`, entries enabled, disabled, removed or added, and saved without losing what
//! the editor does not show.

use std::path::PathBuf;
use std::time::SystemTime;

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
}

/// The file as last read: the bytes a save compares the disk with, and the lines edited.
#[derive(Debug)]
struct Loaded {
    original: Vec<u8>,
    document: HostsDocument,
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
                }
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
            .width(Length::Fixed(150.0));

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

        let entries = self.entries();
        if !entries.is_empty() {
            let table_headers = row![
                text(fl!("ui-tool-hosts-col-enabled"))
                    .size(font_size::CAPTION)
                    .style(text::secondary)
                    .width(Length::Fixed(60.0)),
                text(fl!("ui-tool-hosts-col-ip"))
                    .size(font_size::CAPTION)
                    .style(text::secondary)
                    .width(Length::Fixed(140.0)),
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
                    .width(Length::Fixed(70.0)),
            ]
            .spacing(spacing::SM);

            let mut entry_rows = column![table_headers].spacing(spacing::XS);

            for (idx, e) in entries.into_iter().enumerate() {
                let chk =
                    checkbox(e.enabled).on_toggle(move |_| send(HostsMessage::ToggleEntry(idx)));

                let del_btn =
                    button(text(fl!("ui-tool-hosts-btn-delete")).size(font_size::CAPTION))
                        .style(styles::subtle)
                        .on_press(send(HostsMessage::DeleteEntry(idx)));

                let r = row![
                    container(chk).width(Length::Fixed(60.0)),
                    text(&e.ip)
                        .size(font_size::BODY)
                        .font(super::BOX_FONT)
                        .width(Length::Fixed(140.0)),
                    text(e.names())
                        .size(font_size::BODY)
                        .font(super::BOX_FONT)
                        .width(Length::FillPortion(2)),
                    text(&e.comment)
                        .size(font_size::BODY)
                        .font(super::BOX_FONT)
                        .style(text::secondary)
                        .width(Length::FillPortion(1)),
                    del_btn,
                ]
                .spacing(spacing::SM)
                .align_y(Alignment::Center);

                entry_rows = entry_rows.push(r);
            }

            let card = container(scrollable(entry_rows))
                .padding(spacing::MD)
                .style(styles::card);

            body = body.push(card);
        } else if self.loading {
            body = body.push(super::empty_state(fl!("ui-tool-hosts-loading-hint")));
        } else if self.loaded.is_some() {
            body = body.push(super::empty_state(fl!("ui-tool-hosts-empty-state")));
        }

        super::content_column(body)
    }
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
        }));
        app.tabs[0].id
    }

    fn loaded(bytes: &[u8]) -> LoadedHosts {
        LoadedHosts {
            original: bytes.to_vec(),
            document: HostsDocument::parse(bytes),
        }
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
