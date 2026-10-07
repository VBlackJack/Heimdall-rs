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

//! One Heimdall per configuration folder, as the C# `App` starts: a launch finding the
//! folder owned asks the owner to come forward and ends before reading any file, and the
//! owner brings its main window forward when asked.

use std::path::{Path, PathBuf};
use std::time::Duration;

use heimdall_core::instance::{self, InstanceGuard, Ownership};
use iced::Subscription;
use iced::futures::{Stream, stream};

use crate::shell::Message;

/// How often the owning instance looks for a later launch's request to come forward: soon
/// enough to answer a double click, one file system call each time.
const REQUEST_LOOK: Duration = Duration::from_millis(500);

/// How this process goes on, the configuration folder claimed.
pub(crate) enum Start {
    /// Another instance owns the folder and was asked to come forward: this one ends, as the
    /// C# second launch shuts down.
    HandedOver,
    /// This process runs: owning the folder while the guard is kept, or unguarded when
    /// ownership could not be told.
    Run(Option<InstanceGuard>),
}

/// Claims the configuration folder `dir` for this process, as the C# `TryAcquire`: never
/// refused for anything but another live instance.
pub(crate) fn claim(dir: Option<&Path>) -> Start {
    let Some(dir) = dir else {
        log::warn!("single instance: no configuration folder, started unguarded");
        return Start::Run(None);
    };
    if instance::disabled_by_environment() {
        log::warn!(
            "single instance: turned off by {}, instances may overwrite each other's profiles",
            instance::DISABLE_VARIABLE
        );
        return Start::Run(None);
    }
    match instance::acquire(dir) {
        Ownership::Owner(guard) => {
            log::info!(
                "single instance: owning {} (pid {})",
                dir.display(),
                std::process::id()
            );
            Start::Run(Some(guard))
        }
        Ownership::AlreadyRunning => {
            // Not asked is no reason to run beside it: the overwrite is what is avoided.
            match instance::request_activation(dir) {
                Ok(()) => log::info!(
                    "single instance: {} is owned by another instance, handing over to it (pid {})",
                    dir.display(),
                    std::process::id()
                ),
                Err(error) => log::warn!(
                    "single instance: {} is owned by another instance, not asked to come forward: {error}",
                    dir.display()
                ),
            }
            Start::HandedOver
        }
        Ownership::Unavailable(error) => {
            log::warn!(
                "single instance: guard unavailable on {}, started unguarded: {error}",
                dir.display()
            );
            Start::Run(None)
        }
    }
}

/// The later launches' requests to come forward, for the instance owning `dir`.
pub(crate) fn requests(dir: PathBuf) -> Subscription<Message> {
    Subscription::run_with(dir, |dir: &PathBuf| asked(dir.clone()))
}

/// [`Message::BringForward`] each time a later launch asks the instance owning `dir`.
fn asked(dir: PathBuf) -> impl Stream<Item = Message> {
    stream::unfold(dir, |dir| async move {
        loop {
            tokio::time::sleep(REQUEST_LOOK).await;
            if instance::take_activation_request(&dir) {
                return Some((Message::BringForward, dir));
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use std::pin::pin;
    use std::time::Duration;

    use iced::futures::StreamExt as _;

    use super::{Start, asked, claim};
    use crate::shell::Message;

    /// How long a request is waited for before the test fails: many looks.
    const ANSWER_WAIT: Duration = Duration::from_secs(10);

    /// How long nothing more is to come once the request is answered: two looks.
    const SILENCE_WAIT: Duration = Duration::from_secs(1);

    #[tokio::test]
    async fn a_later_launch_brings_the_owner_forward_and_ends() {
        let dir = tempfile::tempdir().expect("dir");
        let Start::Run(Some(guard)) = claim(Some(dir.path())) else {
            panic!("the first launch owns the folder");
        };
        let mut asked = pin!(asked(guard.dir().to_owned()));

        assert!(matches!(claim(Some(dir.path())), Start::HandedOver));

        let answered = tokio::time::timeout(ANSWER_WAIT, asked.next())
            .await
            .expect("answered");
        assert!(matches!(answered, Some(Message::BringForward)));
        // Answered once: the request is taken.
        assert!(
            tokio::time::timeout(SILENCE_WAIT, asked.next())
                .await
                .is_err()
        );
    }

    #[test]
    fn a_folder_freed_is_owned_by_the_next_launch() {
        let dir = tempfile::tempdir().expect("dir");
        let first = claim(Some(dir.path()));
        assert!(matches!(first, Start::Run(Some(_))));
        drop(first);
        assert!(matches!(claim(Some(dir.path())), Start::Run(Some(_))));
    }

    #[test]
    fn an_unusable_folder_still_starts_the_application() {
        let dir = tempfile::tempdir().expect("dir");
        let file = dir.path().join("plain");
        std::fs::write(&file, "x").expect("written");
        assert!(matches!(
            claim(Some(&file.join("config"))),
            Start::Run(None)
        ));
        assert!(matches!(claim(None), Start::Run(None)));
    }
}
