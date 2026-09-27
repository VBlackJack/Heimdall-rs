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

//! The status bar along the window's foot, as the C# Heimdall's: on the left the state of
//! the session shown, or what was just done; on the right how many sessions the tree holds.

use heimdall_app::{Notice, SessionStatus};
use iced::widget::{container, row, space, text};
use iced::{Element, Length};

use crate::i18n::fl;
use crate::shell::Message;

/// Size of the bar's text.
const TEXT_SIZE: f32 = 12.0;

/// Room around the bar's text.
const PADDING: [f32; 2] = [2.0, 8.0];

/// What the left of the bar says.
#[must_use]
pub fn status_text(status: &SessionStatus, notice: Option<&Notice>) -> String {
    if let Some(notice) = notice {
        return match notice {
            Notice::Copied(copied) => fl!("ui-status-copied", text = copied.as_str()),
            Notice::FolderCreated(path) => fl!("ui-status-folder-created", path = path.as_str()),
        };
    }
    let (name, state) = match status {
        SessionStatus::Ready => return fl!("ui-status-ready"),
        SessionStatus::Connected(name) => {
            return fl!("ui-status-connected", name = name.as_str());
        }
        SessionStatus::Connecting(name) => (name, fl!("ui-status-connecting")),
        SessionStatus::Reconnecting(name) => (name, fl!("ui-status-reconnecting")),
        SessionStatus::Disconnected(name) => (name, fl!("ui-status-disconnected")),
        SessionStatus::Error(name) => (name, fl!("ui-status-error")),
    };
    fl!("ui-status-state", name = name.as_str(), state = state)
}

/// What the right of the bar says: the sessions, and how many of them the search shows
/// while it filters.
#[must_use]
pub fn count_text(shown: usize, total: usize, filtering: bool) -> String {
    if filtering {
        fl!("ui-status-sessions-filtered", shown = shown, count = total)
    } else {
        fl!("ui-status-sessions", count = total)
    }
}

/// The bar.
pub fn view<'a>(left: String, right: String) -> Element<'a, Message> {
    container(
        row![
            text(left).size(TEXT_SIZE),
            space::horizontal(),
            text(right).size(TEXT_SIZE),
        ]
        .width(Length::Fill),
    )
    .padding(PADDING)
    .width(Length::Fill)
    .style(container::bordered_box)
    .into()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn named(name: &str) -> String {
        name.to_owned()
    }

    #[test]
    fn the_left_says_the_csharp_sentences() {
        assert_eq!(
            status_text(&SessionStatus::Ready, None),
            "Ready. Select a session to get started."
        );
        assert_eq!(
            status_text(&SessionStatus::Connected(named("web")), None),
            "Connected to: web"
        );
        for (status, said) in [
            (
                SessionStatus::Connecting(named("web")),
                "web: Connecting...",
            ),
            (
                SessionStatus::Reconnecting(named("web")),
                "web: Reconnecting...",
            ),
            (
                SessionStatus::Disconnected(named("web")),
                "web: Disconnected",
            ),
            (SessionStatus::Error(named("web")), "web: Error"),
        ] {
            assert_eq!(status_text(&status, None), said);
        }
        let status = SessionStatus::Connected(named("web"));
        assert_eq!(
            status_text(&status, Some(&Notice::Copied(named("web.lab")))),
            "Copied to clipboard: web.lab",
            "a notice first"
        );
        assert_eq!(
            status_text(&status, Some(&Notice::FolderCreated(named("Prod/Archive")))),
            "Folder \"Prod/Archive\" created."
        );
    }

    #[test]
    fn the_right_counts_the_sessions_and_those_a_search_shows() {
        assert_eq!(count_text(1, 1, false), "1 session");
        assert_eq!(
            count_text(0, 3, false),
            "3 sessions",
            "no search: all of them"
        );
        assert_eq!(count_text(1, 3, true), "1 of 3 sessions");
        assert_eq!(count_text(0, 1, true), "0 of 1 session");
    }
}
