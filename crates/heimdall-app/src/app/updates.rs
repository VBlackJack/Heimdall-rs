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

//! The look for a newer release, as the C# update banner: a while after start, then every
//! hour, each time only once the interval chosen has passed since the last answer; "Check
//! now" at any time. A newer release not skipped is offered in a banner: "View release"
//! opens its page, "Later" hides it until a later look finds it again, "Skip this version"
//! hides it for good. Nothing is downloaded.

use std::time::{Duration, SystemTime};

use crate::update_check::{
    self, Failure, Outcome, RELEASE_VARIABLE, ReleaseTag, SECONDS_PER_HOUR, START_DELAY, TICK,
};

use super::{App, Effect};

/// What the banner and the Settings page's "Check now" send.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UpdateMessage {
    /// Time to see whether a look is due.
    Tick,
    /// Look now, whatever the interval, as the Settings page's "Check now".
    CheckNow,
    /// What a look found.
    Checked(Outcome),
    /// The banner's "View release": the release's page in the browser.
    ViewRelease,
    /// The banner's "Later": hidden until a later look.
    Later,
    /// The banner's "Skip this version": hidden for good, for this release.
    Skip,
    /// The Settings page's "Offer it again": the release skipped is forgotten, so the next
    /// look offers it again, as the C# `ClearSkippedVersion` (`SettingsViewModel.cs:253`).
    ClearSkipped,
}

/// What the Settings page says of its last "Check now".
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UpdateStatus {
    /// It is looking.
    Checking,
    /// Nothing newer.
    UpToDate,
    /// This newer release.
    Available(ReleaseTag),
    /// No answer, and why.
    Failed(Failure),
    /// This build does not know its release: a development build never looks.
    NeedsRelease,
}

/// Who asked for the look running.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Look {
    /// The schedule.
    Scheduled,
    /// "Check now".
    Manual,
}

/// What the application knows of the looks.
#[derive(Debug, Default)]
pub(super) struct Updates {
    /// The release this build is; `None` for a development build.
    release: Option<ReleaseTag>,
    /// The look running, if one is.
    running: Option<Look>,
    /// The first tick came: the next come every [`TICK`].
    started: bool,
    /// The newer release found.
    offer: Option<ReleaseTag>,
    /// The banner offers it.
    shown: bool,
    /// What the last "Check now" found.
    status: Option<UpdateStatus>,
}

impl Updates {
    /// The looks of a build of `release`; a development build says once why it never looks.
    pub(super) fn new(release: Option<ReleaseTag>) -> Self {
        if release.is_none() {
            log::info!("update checks need a release build: {RELEASE_VARIABLE} was not set");
        }
        Self {
            release,
            ..Self::default()
        }
    }
}

impl App {
    /// The release this build is; `None` for a development build, which never looks.
    #[must_use]
    pub fn running_release(&self) -> Option<ReleaseTag> {
        self.updates.release
    }

    /// The application as a build of `release` runs it: what the looks compare with.
    #[must_use]
    pub fn with_release(mut self, release: Option<ReleaseTag>) -> Self {
        self.updates.release = release;
        self
    }

    /// How long until the next tick: [`START_DELAY`] before the first, then [`TICK`]; `None`
    /// while the looks are off or this build does not know its release.
    #[must_use]
    pub fn update_tick_interval(&self) -> Option<Duration> {
        if !self.settings.updates.enabled || self.updates.release.is_none() {
            return None;
        }
        Some(if self.updates.started {
            TICK
        } else {
            START_DELAY
        })
    }

    /// The release the banner offers; none in full screen, where the session has the whole
    /// screen, as the C# banner gives way to it.
    #[must_use]
    pub fn update_offer(&self, fullscreen: bool) -> Option<ReleaseTag> {
        self.updates
            .offer
            .filter(|_| self.updates.shown && !fullscreen)
    }

    /// What the last "Check now" found, for the Settings page.
    #[must_use]
    pub fn update_status(&self) -> Option<UpdateStatus> {
        self.updates.status
    }

    /// A message of the looks, the banner or "Check now".
    pub(super) fn update_message(&mut self, message: UpdateMessage) -> Vec<Effect> {
        match message {
            UpdateMessage::Tick => {
                self.updates.started = true;
                self.look(Look::Scheduled, SystemTime::now())
            }
            UpdateMessage::CheckNow => self.look(Look::Manual, SystemTime::now()),
            UpdateMessage::Checked(outcome) => {
                self.update_checked(outcome, SystemTime::now());
                Vec::new()
            }
            UpdateMessage::ViewRelease => self
                .updates
                .offer
                .and_then(update_check::release_page)
                .map(Effect::OpenUrl)
                .into_iter()
                .collect(),
            UpdateMessage::Later => {
                self.updates.shown = false;
                Vec::new()
            }
            UpdateMessage::Skip => {
                if let Some(offer) = self.updates.offer.take() {
                    self.settings.update_check.skipped = Some(offer.tag());
                    self.save_update_check();
                }
                self.updates.shown = false;
                Vec::new()
            }
            UpdateMessage::ClearSkipped => {
                if self.settings.update_check.skipped.take().is_some() {
                    self.save_update_check();
                }
                Vec::new()
            }
        }
    }

    /// A look, when `look` may start one at `now`: the schedule only when it is due, "Check
    /// now" whenever none runs. A development build never looks, and says so to "Check
    /// now".
    fn look(&mut self, look: Look, now: SystemTime) -> Vec<Effect> {
        if self.updates.release.is_none() {
            if look == Look::Manual {
                self.updates.status = Some(UpdateStatus::NeedsRelease);
            }
            return Vec::new();
        }
        if self.updates.running.is_some() || (look == Look::Scheduled && !self.update_due(now)) {
            return Vec::new();
        }
        self.updates.running = Some(look);
        if look == Look::Manual {
            self.updates.status = Some(UpdateStatus::Checking);
        }
        vec![Effect::CheckForUpdate]
    }

    /// Whether the schedule looks at `now`: on, no banner shown, and the interval passed
    /// since the last answer. A last answer said to be later than now is not believed.
    fn update_due(&self, now: SystemTime) -> bool {
        let updates = self.settings.updates;
        if !updates.enabled || self.updates.shown {
            return false;
        }
        let interval = Duration::from_secs(u64::from(updates.interval_hours) * SECONDS_PER_HOUR);
        self.settings
            .update_check
            .last_check
            .is_none_or(|last| match now.duration_since(last) {
                Ok(since) => since >= interval,
                Err(_) => true,
            })
    }

    /// What a look found, at `now`: an answer is stamped, a newer release not skipped is
    /// offered, and "Check now" is told.
    fn update_checked(&mut self, outcome: Outcome, now: SystemTime) {
        let look = self.updates.running.take();
        if outcome.answered() {
            self.settings.update_check.last_check = Some(now);
            self.save_update_check();
        }
        let status = match outcome {
            Outcome::Latest(latest) if self.updates.release.is_some_and(|built| latest > built) => {
                if !self.skips(latest) {
                    self.updates.offer = Some(latest);
                    self.updates.shown = true;
                }
                UpdateStatus::Available(latest)
            }
            Outcome::Latest(_) | Outcome::NoRelease => UpdateStatus::UpToDate,
            Outcome::Failed(failure) => UpdateStatus::Failed(failure),
        };
        if look == Some(Look::Manual) {
            self.updates.status = Some(status);
        }
    }

    /// Whether the user skipped `release`: compared as releases, not as text.
    fn skips(&self, release: ReleaseTag) -> bool {
        self.settings
            .update_check
            .skipped
            .as_deref()
            .and_then(ReleaseTag::parse)
            == Some(release)
    }

    /// The last look and the release skipped, saved; a save that fails is logged, and the
    /// next start looks again.
    fn save_update_check(&self) {
        if let Err(error) = self.settings.save(&self.settings_file) {
            log::warn!("the update check's state was not saved: {error}");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A release build's application, its files in `dir`.
    fn released(dir: &std::path::Path) -> App {
        App::new(crate::AppConfig {
            profiles_file: dir.join("profiles.toml"),
            known_hosts: dir.join("known_hosts"),
            legacy_dir: None,
            agent: heimdall_ssh::AgentSource::Disabled,
            initial_grid: heimdall_term::GridSize { cols: 80, rows: 24 },
            files_start: dir.to_owned(),
            system_credentials: crate::SystemCredentials::memory(),
        })
        .with_release(ReleaseTag::parse("v2026.100901"))
    }

    #[test]
    fn the_schedule_waits_for_the_interval_and_while_the_banner_is_shown() {
        let dir = tempfile::tempdir().expect("dir");
        let mut app = released(dir.path());
        let now = SystemTime::now();
        assert_eq!(app.look(Look::Scheduled, now).len(), 1);
        app.update_checked(
            Outcome::Latest(ReleaseTag::parse("v2026.101001").expect("tag")),
            now,
        );
        let day = Duration::from_hours(24);
        assert!(
            app.look(Look::Scheduled, now + day - Duration::from_secs(1))
                .is_empty()
        );
        assert!(
            app.look(Look::Scheduled, now + day).is_empty(),
            "the banner shown holds it"
        );
        let _ = app.update_message(UpdateMessage::Later);
        assert_eq!(app.look(Look::Scheduled, now + day).len(), 1);
        // A last look said to be later than now is not believed.
        app.update_checked(Outcome::NoRelease, now + day * 2);
        assert_eq!(app.look(Look::Scheduled, now).len(), 1);
    }
}
