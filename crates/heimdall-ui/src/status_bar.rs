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

use heimdall_app::{Notice, SessionStatus, server_text};
use heimdall_core::settings::BroadcastScope;
use iced::widget::{container, row, space, text};
use iced::{Element, Length};

use crate::i18n::fl;
use crate::shell::Message;

/// Size of the bar's text.
const TEXT_SIZE: f32 = 12.0;

/// Room around the bar's text.
const PADDING: [f32; 2] = [2.0, 8.0];

/// The name of broadcast input's `scope`, `targets` the tabs marked.
#[must_use]
pub fn scope_label(scope: BroadcastScope, targets: usize) -> String {
    match scope {
        BroadcastScope::AllTabs => fl!("ui-broadcast-scope-all"),
        BroadcastScope::SelectedTabs => fl!("ui-broadcast-scope-selected", count = targets),
    }
}

/// What a Files tab's notice says.
fn files_notice(notice: &Notice) -> String {
    match notice {
        Notice::PathCopied(path) => fl!("ui-status-path-copied", path = path.as_str()),
        Notice::FilesCut(count) => fl!("ui-status-files-cut", count = (*count)),
        Notice::FilesPasted => fl!("ui-status-files-pasted"),
        Notice::FilesEditing(name) => fl!("ui-status-files-editing", name = name.as_str()),
        Notice::FilesAutoUploaded(name) => {
            fl!("ui-status-files-auto-uploaded", name = name.as_str())
        }
        Notice::FilesSavedWithSudo(name) => {
            fl!("ui-status-files-saved-sudo", name = name.as_str())
        }
        Notice::FilesAutoUploadRefused { name, error } => fl!(
            "ui-status-files-auto-upload-refused",
            name = name.as_str(),
            reason = crate::texts::files_error(error)
        ),
        Notice::FilesCopied(count) => fl!("ui-status-files-copied", count = (*count)),
        Notice::FilesDuplicated => fl!("ui-status-files-duplicated"),
        _ => String::new(),
    }
}

/// How saving the server's files ended, in the bar's words.
fn save_ended(end: heimdall_rdp::SaveEnd) -> String {
    match end {
        heimdall_rdp::SaveEnd::Saved(count) => fl!("ui-status-rdp-files-saved", count = count),
        heimdall_rdp::SaveEnd::Failed { saved, total } => fl!(
            "ui-status-rdp-files-save-failed",
            saved = saved,
            total = total
        ),
        heimdall_rdp::SaveEnd::Cancelled { saved, total } => fl!(
            "ui-status-rdp-files-save-cancelled",
            saved = saved,
            total = total
        ),
        heimdall_rdp::SaveEnd::Refused(heimdall_rdp::SaveRefusal::TooManyEntries) => fl!(
            "ui-status-rdp-files-not-saved-too-many",
            count = heimdall_rdp::MAX_COPY_ENTRIES
        ),
        heimdall_rdp::SaveEnd::Refused(heimdall_rdp::SaveRefusal::TooLarge) => fl!(
            "ui-status-rdp-files-not-saved-too-large",
            size = crate::texts::size(heimdall_rdp::MAX_COPY_BYTES)
        ),
        heimdall_rdp::SaveEnd::Refused(heimdall_rdp::SaveRefusal::UnknownSize) => {
            fl!("ui-status-rdp-files-not-saved-unknown-size")
        }
    }
}

/// What the bar says of a desktop: its size, or the files copied not offered to it.
fn desktop_notice(notice: &Notice) -> String {
    match notice {
        Notice::ResolutionReconnected => fl!("ui-status-resolution-reconnected"),
        Notice::ResolutionScaled => fl!("ui-resolution-larger-than-window"),
        Notice::RdpFilesTooMany => fl!(
            "ui-status-rdp-files-too-many",
            count = heimdall_rdp::MAX_COPY_ENTRIES
        ),
        Notice::RdpFilesTooLarge => fl!(
            "ui-status-rdp-files-too-large",
            size = crate::texts::size(heimdall_rdp::MAX_COPY_BYTES)
        ),
        Notice::RdpFilesSaveEnded(end) => save_ended(*end),
        _ => String::new(),
    }
}

/// What the left of the bar says; `targets` the tabs marked for broadcast input.
#[must_use]
#[expect(clippy::too_many_lines, reason = "one arm per notice")]
pub fn status_text(status: &SessionStatus, notice: Option<&Notice>, targets: usize) -> String {
    if let Some(notice) = notice {
        return match notice {
            Notice::Copied(copied) => fl!("ui-status-copied", text = copied.as_str()),
            Notice::FolderCreated(path) => fl!("ui-status-folder-created", path = path.as_str()),
            notice @ (Notice::PathCopied(_)
            | Notice::FilesCut(_)
            | Notice::FilesPasted
            | Notice::FilesEditing(_)
            | Notice::FilesAutoUploaded(_)
            | Notice::FilesSavedWithSudo(_)
            | Notice::FilesAutoUploadRefused { .. }
            | Notice::FilesCopied(_)
            | Notice::FilesDuplicated) => files_notice(notice),
            Notice::TranscriptStarted(path) => {
                fl!("ui-status-transcript-started", path = path.as_str())
            }
            Notice::TranscriptStopped => fl!("ui-status-transcript-stopped"),
            Notice::TranscriptFailed(reason) => {
                fl!("ui-status-transcript-failed", reason = reason.as_str())
            }
            Notice::BroadcastOn(scope) => {
                fl!("ui-broadcast-on", scope = scope_label(*scope, targets))
            }
            Notice::BroadcastOff => fl!("ui-broadcast-off"),
            Notice::ResolutionInvalid => fl!("ui-resolution-custom-invalid"),
            Notice::ResolutionSaved => fl!("ui-resolution-save-default-done"),
            Notice::ResolutionSaveUnavailable => fl!("ui-resolution-save-default-unavailable"),
            Notice::ReachabilityTesting { host, port } => fl!(
                "ui-status-reachability-testing",
                host = server_text(host),
                port = (*port)
            ),
            Notice::Reachable { host, port, millis } => fl!(
                "ui-status-reachability-success",
                host = server_text(host),
                port = (*port),
                millis = (*millis)
            ),
            Notice::Unreachable {
                host,
                port,
                failure,
            } => fl!(
                "ui-status-reachability-failed",
                host = server_text(host),
                port = (*port),
                reason = crate::address_test_view::reason(failure)
            ),
            notice @ (Notice::TunnelOpened { .. }
            | Notice::TunnelFailed(_)
            | Notice::TunnelClosed { .. }
            | Notice::AllTunnelsClosed
            | Notice::PortCopied(_)) => {
                crate::tunnels_view::notice_text(notice).unwrap_or_default()
            }
            Notice::Bookmarked(path) => fl!("ui-files-bookmark-added", path = path.as_str()),
            Notice::ScreenshotCopied => fl!("ui-status-screenshot-copied"),
            Notice::ScreenshotFailed => fl!("ui-status-screenshot-failed"),
            Notice::FingerprintCopied(server) => {
                fl!("ui-status-fingerprint-copied", server = server.as_str())
            }
            Notice::HostKeyRemoved(server) => {
                fl!("ui-status-host-key-removed", server = server.as_str())
            }
            Notice::CertificateForgotten(server) => {
                fl!("ui-status-certificate-forgotten", server = server.as_str())
            }
            Notice::ProviderNoPassword(name) => {
                fl!("ui-status-provider-no-password", name = name.as_str())
            }
            Notice::ProviderFailed(detail) => {
                fl!("ui-status-provider-failed", detail = detail.as_str())
            }
            Notice::ProviderTimedOut => fl!("ui-status-provider-timed-out"),
            Notice::LinkNotAFolder(name) => {
                fl!("ui-status-link-not-a-folder", name = name.as_str())
            }
            Notice::WinRmGatewayNtlm => fl!("ui-status-winrm-gateway-ntlm"),
            Notice::ExplorerHoldsNoFiles => fl!("ui-status-explorer-no-files"),
            notice @ (Notice::ResolutionReconnected
            | Notice::ResolutionScaled
            | Notice::RdpFilesTooMany
            | Notice::RdpFilesTooLarge
            | Notice::RdpFilesSaveEnded(_)) => desktop_notice(notice),
            Notice::WinRmCertificateSkipped => fl!("ui-status-winrm-certificate-skipped"),
            Notice::BroadcastScope(scope) => {
                fl!(
                    "ui-broadcast-scope-status",
                    scope = scope_label(*scope, targets)
                )
            }
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

/// The bar; `controls` beside the count, broadcast input's.
pub fn view(left: String, right: String, controls: Element<'_, Message>) -> Element<'_, Message> {
    container(
        row![
            text(left).size(TEXT_SIZE),
            space::horizontal(),
            controls,
            text(right).size(TEXT_SIZE),
        ]
        .spacing(PADDING[1])
        .align_y(iced::Alignment::Center)
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
    fn a_reachability_test_is_said_as_the_csharp_status_line() {
        let said = |notice: Notice| status_text(&SessionStatus::Ready, Some(&notice), 0);
        assert_eq!(
            said(Notice::ReachabilityTesting {
                host: "web.lab".to_owned(),
                port: 22,
            }),
            "Testing web.lab:22 ..."
        );
        assert_eq!(
            said(Notice::Reachable {
                host: "web.lab".to_owned(),
                port: 22,
                millis: 4,
            }),
            "web.lab:22 reachable in 4 ms"
        );
        assert_eq!(
            said(Notice::Unreachable {
                host: "web.lab".to_owned(),
                port: 22,
                failure: heimdall_app::reachability::Unreached::DnsNoResults,
            }),
            "web.lab:22 unreachable: DNS lookup returned no addresses."
        );
    }

    #[test]
    fn how_saving_the_servers_files_ended_is_said() {
        let said = |end| {
            status_text(
                &SessionStatus::Ready,
                Some(&Notice::RdpFilesSaveEnded(end)),
                0,
            )
        };
        assert_eq!(
            said(heimdall_rdp::SaveEnd::Saved(3)),
            "Server's files saved: 3."
        );
        assert_eq!(
            said(heimdall_rdp::SaveEnd::Failed { saved: 1, total: 4 }),
            "Server's files not all saved: 1 of 4 saved before it failed."
        );
        assert_eq!(
            said(heimdall_rdp::SaveEnd::Refused(
                heimdall_rdp::SaveRefusal::UnknownSize
            )),
            "Server's files not saved: the server did not say their size."
        );
    }

    #[test]
    fn files_not_offered_to_a_desktop_are_said_with_the_limit_they_pass() {
        let said = |notice: Notice| status_text(&SessionStatus::Ready, Some(&notice), 0);
        assert_eq!(
            said(Notice::RdpFilesTooMany),
            "Files not copied to the server: one copy takes 10000 files and folders at most."
        );
        assert_eq!(
            said(Notice::RdpFilesTooLarge),
            "Files not copied to the server: one copy takes 2.0 GiB at most."
        );
    }

    #[test]
    fn the_left_says_the_csharp_sentences() {
        assert_eq!(
            status_text(&SessionStatus::Ready, None, 0),
            "Ready. Select a session to get started."
        );
        assert_eq!(
            status_text(&SessionStatus::Connected(named("web")), None, 0),
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
            assert_eq!(status_text(&status, None, 0), said);
        }
        let status = SessionStatus::Connected(named("web"));
        assert_eq!(
            status_text(&status, Some(&Notice::Copied(named("web.lab"))), 0),
            "Copied to clipboard: web.lab",
            "a notice first"
        );
        assert_eq!(
            status_text(
                &status,
                Some(&Notice::FolderCreated(named("Prod/Archive"))),
                0
            ),
            "Folder \"Prod/Archive\" created."
        );
        for (notice, said) in [
            (
                Notice::BroadcastOn(BroadcastScope::AllTabs),
                "Broadcast mode ON - All tabs",
            ),
            (
                Notice::BroadcastOn(BroadcastScope::SelectedTabs),
                "Broadcast mode ON - Selected tabs (2)",
            ),
            (Notice::BroadcastOff, "Broadcast mode OFF"),
            (
                Notice::BroadcastScope(BroadcastScope::SelectedTabs),
                "Broadcast scope: Selected tabs (2)",
            ),
        ] {
            assert_eq!(status_text(&status, Some(&notice), 2), said);
        }
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
