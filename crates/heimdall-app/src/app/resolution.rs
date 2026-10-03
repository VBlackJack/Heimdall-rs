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

//! An RDP tab's "Resolution" menu, as the C# one: the desktop matched to the tab again, a
//! size of its own from the presets or typed, and the size kept as the profile's own.

use heimdall_core::profile::{Resolution, fixed_desktop, parse_resolution};

use super::{App, Dialog, Notice, TabProfile};
use crate::ids::TabId;

/// What the user chose in an RDP tab's "Resolution" menu.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResolutionChoice {
    /// The tab's size, followed from then on.
    MatchWindow,
    /// A size of its own.
    Fixed {
        /// Width, in pixels.
        width: u16,
        /// Height, in pixels.
        height: u16,
    },
    /// A size to type.
    Custom,
    /// What the session has now, kept as the profile's own.
    SaveDefault,
}

/// What the C# "Custom..." starts with.
const CUSTOM_START: &str = "1920x1080";

impl App {
    /// Applies a choice of `tab_id`'s "Resolution" menu.
    pub(super) fn choose_resolution(&mut self, tab_id: TabId, choice: ResolutionChoice) {
        match choice {
            ResolutionChoice::MatchWindow => self.size_desktop(tab_id, None),
            ResolutionChoice::Fixed { width, height } => {
                self.size_desktop(tab_id, Some(fixed_desktop(width, height)));
            }
            ResolutionChoice::Custom => {
                if self.tab(tab_id).is_some_and(|tab| tab.desktop.is_some()) {
                    self.dialog = Some(Dialog::CustomResolution {
                        tab: tab_id,
                        value: CUSTOM_START.to_owned(),
                    });
                }
            }
            ResolutionChoice::SaveDefault => self.save_resolution(tab_id),
        }
    }

    /// "Custom...": the size typed, or, not one, the C# line saying how to type it.
    pub(super) fn confirm_custom_resolution(&mut self, tab_id: TabId, typed: &str) {
        match parse_resolution(typed) {
            Some(size) => self.size_desktop(tab_id, Some(size)),
            None => self.tell(Notice::ResolutionInvalid),
        }
    }

    fn size_desktop(&mut self, tab_id: TabId, size: Option<(u16, u16)>) {
        if let Some(pane) = self
            .tab_mut(tab_id)
            .and_then(|tab| tab.desktop.as_deref_mut())
        {
            pane.choose_size(size);
        }
    }

    /// Keeps the size `tab_id`'s session has now as its saved profile's own, as the C#
    /// "Save as default for this server": a size of its own, or the tab's, followed.
    fn save_resolution(&mut self, tab_id: TabId) {
        let Some(tab) = self.tab(tab_id) else {
            return;
        };
        let fixed = tab.desktop.as_ref().and_then(|pane| pane.fixed_size());
        let saved = match &tab.profile {
            TabProfile::Rdp(profile) => self
                .store
                .rdp_profiles()
                .iter()
                .find(|saved| saved.id == profile.id)
                .cloned(),
            _ => None,
        };
        // A session not opened from a saved profile has nowhere to keep it.
        let Some(mut profile) = saved else {
            self.tell(Notice::ResolutionSaveUnavailable);
            return;
        };
        if let Some((width, height)) = fixed {
            profile.options.resolution = Resolution::Fixed;
            profile.options.fixed_width = width;
            profile.options.fixed_height = height;
        } else {
            profile.options.resolution = Resolution::FitWindow;
            profile.options.dynamic_resolution = true;
        }
        match self.store.apply(|store| store.merge_rdp([profile])) {
            Ok(_) => self.tell(Notice::ResolutionSaved),
            Err(error) => {
                self.dialog = Some(Dialog::StoreError {
                    detail: error.to_string(),
                });
            }
        }
    }
}
