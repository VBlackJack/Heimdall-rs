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

//! Whether the welcome tour was seen, as the C# `OnboardingCompleted`: read at start to show
//! the tour on a first run, and saved once it is finished or skipped, as the C#
//! `OnboardingFlowViewModel.CompleteAsync` (`OnboardingFlowViewModel.cs:219-277`). The tour
//! itself is the window's.

use super::App;

impl App {
    /// Whether the welcome tour was finished or skipped on this computer.
    #[must_use]
    pub fn onboarding_completed(&self) -> bool {
        self.settings.onboarding_completed
    }

    /// Records the welcome tour as seen, saved at once as the C# `MergeSettingAsync`. A
    /// record that cannot be saved is not made, and `false` says so: the C# tour then stays
    /// open with its message rather than be shown again at the next start unannounced.
    pub fn complete_onboarding(&mut self) -> bool {
        if self.settings.onboarding_completed {
            return true;
        }
        self.settings.onboarding_completed = true;
        if let Err(error) = self.settings.save(&self.settings_file) {
            log::error!("the welcome tour could not be recorded as seen: {error}");
            self.settings.onboarding_completed = false;
            return false;
        }
        true
    }
}
