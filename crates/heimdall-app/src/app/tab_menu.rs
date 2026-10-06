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

//! A tab's menu, as the C# Heimdall's: a name of the user's, the session opened again in a
//! second tab, the other tabs or those to the right closed at once. Reconnect, Edit and the
//! copies send the messages the failure card and the tree already send.

use super::reconnect::Reopen;
use super::tree::{ProfileKind, ProfileSummary};
use super::{App, Dialog, Effect, Tab, TabProfile};
use crate::driver::Purpose;
use crate::ids::TabId;
use crate::profile_draft::{DraftProtocol, ProfileDraft};
use crate::text::server_text;

/// Something from a tab's menu.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TabMenuMessage {
    /// Ask for a name for a tab, its present one written in.
    Rename(TabId),
    /// The tab name typed so far.
    NameEdited(String),
    /// Give a tab back the title its session sets.
    ResetTitle(TabId),
    /// Open the session of a tab again, in a new tab.
    Duplicate(TabId),
    /// Start keeping a transcript of a tab's session.
    StartTranscript(TabId),
    /// Stop keeping it.
    StopTranscript(TabId),
    /// Show or hide an SSH shell's server health panel.
    ToggleHealth(TabId),
    /// A choice of an RDP tab's "Resolution" menu.
    Resolution {
        /// The tab.
        tab: TabId,
        /// What was chosen.
        choice: super::ResolutionChoice,
    },
    /// The size typed so far in "Custom resolution".
    ResolutionEdited(String),
    /// Pin a tab, or no longer.
    Pin(TabId),
    /// Resize a VNC tab's remote desktop to the tab, or no longer, as the C# "Remote
    /// resizing".
    VncRemoteResize(TabId),
    /// Open the form of a new profile filled from the session of a tab saved nowhere, as the
    /// C# "Save as profile...".
    SaveAsProfile(TabId),
    /// Select the profile a tab was opened from in the tree, its folders opened, as the C#
    /// "Reveal in tree".
    RevealInTree(TabId),
    /// Close the tabs of a group, asking first when a live session would end.
    Close {
        /// The tab the menu is for.
        tab: TabId,
        /// Which of the others.
        group: TabGroup,
    },
}

/// Tabs closed together from a tab's menu.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TabGroup {
    /// Every tab but this one.
    Others,
    /// The tabs after this one.
    Right,
}

impl App {
    /// Applies a message from a tab's menu.
    pub(super) fn tab_menu(&mut self, message: TabMenuMessage) -> Vec<Effect> {
        match message {
            TabMenuMessage::Rename(tab) => {
                if let Some(found) = self.tab(tab) {
                    let value = found.display_title().to_owned();
                    self.dialog = Some(Dialog::RenameTab { tab, value });
                }
                Vec::new()
            }
            TabMenuMessage::NameEdited(value) => {
                if let Some(Dialog::RenameTab { value: typed, .. }) = self.dialog.as_mut() {
                    *typed = value;
                }
                Vec::new()
            }
            TabMenuMessage::ResetTitle(tab) => {
                if let Some(found) = self.tab_mut(tab) {
                    found.custom_title = None;
                }
                Vec::new()
            }
            TabMenuMessage::Duplicate(tab) => self.duplicate_tab(tab),
            TabMenuMessage::StartTranscript(tab) => {
                self.start_transcript(tab, true);
                Vec::new()
            }
            TabMenuMessage::StopTranscript(tab) => {
                self.end_transcript(tab, true);
                Vec::new()
            }
            TabMenuMessage::ToggleHealth(tab) => self.toggle_health(tab),
            TabMenuMessage::Pin(tab) => {
                self.toggle_pin(tab);
                Vec::new()
            }
            TabMenuMessage::VncRemoteResize(tab) => {
                if let Some(pane) = self.tab_mut(tab).and_then(|tab| tab.desktop.as_mut())
                    && let Some(on) = pane.vnc_remote_resize()
                {
                    pane.set_vnc_remote_resize(!on);
                }
                Vec::new()
            }
            TabMenuMessage::SaveAsProfile(tab) => {
                self.save_tab_as_profile(tab);
                Vec::new()
            }
            TabMenuMessage::RevealInTree(tab) => {
                self.reveal_in_tree(tab);
                Vec::new()
            }
            TabMenuMessage::Close { tab, group } => {
                self.close_group(tab, group);
                Vec::new()
            }
            TabMenuMessage::Resolution { tab, choice } => {
                self.choose_resolution(tab, choice);
                Vec::new()
            }
            TabMenuMessage::ResolutionEdited(value) => {
                if let Some(Dialog::CustomResolution { value: typed, .. }) = self.dialog.as_mut() {
                    *typed = value;
                }
                Vec::new()
            }
        }
    }

    /// Names `tab_id` as typed, made safe as a server's title is; a name left empty gives the
    /// tab its own title back, as in the C# Heimdall.
    pub(super) fn rename_tab(&mut self, tab_id: TabId, typed: &str) {
        if let Some(tab) = self.tab_mut(tab_id) {
            let name = server_text(typed.trim());
            tab.custom_title = (!name.is_empty()).then_some(name);
        }
    }

    /// The tabs `group` of `tab_id` holds, in the strip's order: a pane docked in a split is
    /// none, and holds none.
    #[must_use]
    pub fn tab_group(&self, tab_id: TabId, group: TabGroup) -> Vec<TabId> {
        let strip = self.strip();
        let Some(index) = strip.iter().position(|tab| tab.id == tab_id) else {
            return Vec::new();
        };
        // From the tab itself for the right, which the filter drops as it drops it from the
        // others.
        let from = match group {
            TabGroup::Others => 0,
            TabGroup::Right => index,
        };
        // A pinned tab is left, as the C# leaves it.
        strip[from..]
            .iter()
            .filter(|tab| tab.id != tab_id && !tab.pinned)
            .map(|tab| tab.id)
            .collect()
    }

    /// Tab `tab_id` put where `onto` is, as the C# `MoveSession`: the place asked for, then
    /// the pinned tabs first again, so that a drag never mixes the two groups.
    pub(super) fn move_tab(&mut self, tab_id: TabId, onto: TabId) {
        // A pane docked in a split has no place on the strip.
        if self.is_docked(tab_id) || self.is_docked(onto) {
            return;
        }
        let at = |id: TabId| self.tabs.iter().position(|tab| tab.id == id);
        let (Some(from), Some(to)) = (at(tab_id), at(onto)) else {
            return;
        };
        if from == to {
            return;
        }
        let moved = self.tabs.remove(from);
        self.tabs.insert(to, moved);
        // A stable sort: each group keeps the order the move gave it.
        self.tabs.sort_by_key(|tab| !tab.pinned);
    }

    /// Pins `tab_id`, or no longer: the pinned tabs come first, each group in its order, as
    /// the C# `SetPinned` keeps them; the tab shown stays shown.
    fn toggle_pin(&mut self, tab_id: TabId) {
        // Off the strip, a docked pane is never pinned.
        if self.is_docked(tab_id) {
            return;
        }
        let Some(tab) = self.tab_mut(tab_id) else {
            return;
        };
        tab.pinned = !tab.pinned;
        // A stable sort: each group keeps its order.
        self.tabs.sort_by_key(|tab| !tab.pinned);
    }

    /// The form of a new profile, filled from `tab_id`'s session when it is saved nowhere: a
    /// session typed in Quick Connect, or opened with "Connect as".
    fn save_tab_as_profile(&mut self, tab_id: TabId) {
        let Some(tab) = self.tab(tab_id) else {
            return;
        };
        let Reopen::Transient(profile, purpose) = &tab.reopen else {
            return;
        };
        let mut draft = match profile.as_ref() {
            TabProfile::Ssh(profile) => {
                let mut draft = ProfileDraft::from_profile(profile);
                if *purpose == Purpose::Files {
                    draft.protocol = DraftProtocol::Sftp;
                }
                draft
            }
            TabProfile::Rdp(profile) => ProfileDraft::from_rdp(profile),
            TabProfile::Telnet(profile) => ProfileDraft::from_telnet(profile),
            TabProfile::Vnc(profile) => ProfileDraft::from_vnc(profile),
            TabProfile::Ftp(profile) => ProfileDraft::from_ftp(profile),
            TabProfile::WinRm(profile) => ProfileDraft::from_winrm(profile),
            TabProfile::Local(_) => return,
        };
        // A new profile: saved under an identifier of its own.
        draft.editing = None;
        self.dialog = Some(Dialog::EditProfile {
            draft: Box::new(draft),
            error: None,
        });
    }

    /// Selects the profile `tab_id` was opened from in the tree, every folder on the way to
    /// it opened.
    fn reveal_in_tree(&mut self, tab_id: TabId) {
        let Some(Reopen::Profile(id)) = self.tab(tab_id).map(|tab| tab.reopen.clone()) else {
            return;
        };
        let Some(profile) = self.profile_summary(&id) else {
            return;
        };
        if let Some(group) = profile.group.as_deref() {
            let parts = heimdall_core::folder::parts(group);
            for end in 1..=parts.len() {
                self.closed_folders.remove(&parts[..end].join("/"));
            }
        } else {
            self.closed_folders.remove(super::NO_FOLDER);
        }
        self.update(super::Message::SelectProfile(id));
    }

    /// Closes the tabs of `group`, once asked when some are live, as the C# Heimdall asks
    /// once for them all.
    fn close_group(&mut self, tab_id: TabId, group: TabGroup) {
        self.closing_pane = None;
        let tabs = self.tab_group(tab_id, group);
        // Every pane of a split tab closes with it, and counts.
        let panes: Vec<TabId> = tabs.iter().flat_map(|id| self.panes_of(*id)).collect();
        let live = panes
            .iter()
            .filter(|id| self.tab(**id).is_some_and(Tab::is_live))
            .count();
        let unsaved = self.unsaved_tabs(&panes);
        if live > 0 || unsaved > 0 {
            self.dialog = Some(Dialog::ConfirmCloseTabs {
                tabs,
                live,
                unsaved,
            });
            return;
        }
        for tab in tabs {
            self.close_tab(tab);
        }
    }

    /// Whether `tab`'s session is saved nowhere, to be saved as a profile: one typed in
    /// Quick Connect or opened with "Connect as"; a local shell is the sidebar's own.
    #[must_use]
    pub fn can_save_as_profile(&self, tab: &Tab) -> bool {
        matches!(&tab.reopen, Reopen::Transient(profile, _) if !matches!(**profile, TabProfile::Local(_)))
    }

    /// Whether the session of `tab` can open again, in its place or in a new tab: what it
    /// ran, its profile or its shell, is still there.
    #[must_use]
    pub fn can_reopen(&self, tab: &Tab) -> bool {
        match &tab.reopen {
            Reopen::Profile(id) => self.profile_summary(id).is_some(),
            Reopen::Shell(_) | Reopen::Script(_) | Reopen::Transient(..) => true,
            Reopen::LocalBrowser => false,
        }
    }

    /// Opens the session of `tab_id` again in a new tab, the last; the tab stays as it is.
    fn duplicate_tab(&mut self, tab_id: TabId) -> Vec<Effect> {
        // A profile deleted since opens nothing, as Reconnect's.
        let Some(tab) = self.tab(tab_id) else {
            return Vec::new();
        };
        let (reopen, purpose) = (tab.reopen.clone(), tab.purpose);
        self.open_again(reopen, purpose)
    }

    /// The protocol of `tab`: its saved profile's, else that of what it runs.
    #[must_use]
    pub fn tab_kind(&self, tab: &Tab) -> ProfileKind {
        self.tab_profile(tab)
            .map_or_else(|| tab.profile.kind(), |profile| profile.kind)
    }

    /// The saved profile `tab` was opened from, while it is still saved.
    #[must_use]
    pub fn tab_profile(&self, tab: &Tab) -> Option<ProfileSummary> {
        match &tab.reopen {
            Reopen::Profile(id) => self.profile_summary(id),
            Reopen::Shell(_) | Reopen::Script(_) | Reopen::Transient(..) | Reopen::LocalBrowser => {
                None
            }
        }
    }
}
