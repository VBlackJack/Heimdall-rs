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

use heimdall_core::profile::{DesktopSizing, Resolution, fixed_desktop, parse_resolution};

use super::{App, Dialog, Notice, TabProfile};
use crate::ids::TabId;

/// What the user chose in an RDP tab's "Resolution" menu.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResolutionChoice {
    /// The tab's size, followed from then on, the whole of it.
    MatchWindow,
    /// The tab's size followed, fitted to these proportions, as the C# "Match window"
    /// sub-menu.
    MatchAspect(crate::desktop::Aspect),
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
            ResolutionChoice::MatchWindow => {
                self.shape_desktop(tab_id, crate::desktop::Aspect::Stretch);
            }
            ResolutionChoice::MatchAspect(aspect) => self.shape_desktop(tab_id, aspect),
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

    /// `tab_id`'s desktop follows its tab again, fitted to `aspect`, kept for its
    /// reconnections.
    fn shape_desktop(&mut self, tab_id: TabId, aspect: crate::desktop::Aspect) {
        let Some(tab) = self.tab_mut(tab_id) else {
            return;
        };
        tab.desktop_aspect = aspect;
        if let Some(pane) = tab.desktop.as_deref_mut() {
            pane.aspect = aspect;
        }
        self.size_desktop(tab_id, None);
    }

    /// "Custom...": the size typed, or, not one, the C# line saying how to type it.
    pub(super) fn confirm_custom_resolution(&mut self, tab_id: TabId, typed: &str) {
        match parse_resolution(typed) {
            Some(size) => self.size_desktop(tab_id, Some(size)),
            None => self.tell(Notice::ResolutionInvalid),
        }
    }

    /// Sizes `tab_id`'s desktop as chosen, and keeps the choice for the session's
    /// reconnections, as the C# view keeps it while its control connects again.
    fn size_desktop(&mut self, tab_id: TabId, size: Option<(u16, u16)>) {
        let Some(tab) = self.tab_mut(tab_id) else {
            return;
        };
        let Some(pane) = tab.desktop.as_deref_mut() else {
            return;
        };
        pane.choose_size(size);
        // Larger than the tab: shown scaled, and said so, as the C# toast.
        let scaled = matches!(
            (size, pane.tab_size()),
            (Some((width, height)), Some((shown_width, shown_height)))
                if width > shown_width || height > shown_height
        );
        tab.desktop_sizing = Some(match size {
            Some((width, height)) => DesktopSizing::Fixed { width, height },
            None => DesktopSizing::FollowsTab,
        });
        if scaled {
            self.tell(Notice::ResolutionScaled);
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
                self.dialog = Some(Dialog::save_failed(&error));
            }
        }
    }
}
