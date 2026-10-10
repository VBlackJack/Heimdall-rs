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
//! the session shown, or what was just done, and the shortcuts' hint; on the right the
//! tunnels' and broadcast input's buttons, then how many sessions and tunnels there are.

use heimdall_app::files::DragOutFailure;
use heimdall_app::windows_hello::HelloRefusal;
use heimdall_app::{Announced, Announcement, Notice, SessionStatus, server_text};
use heimdall_core::settings::BroadcastScope;
use iced::widget::{Button, button, container, row, space, text};
use iced::{Element, Length};

use crate::i18n::fl;
use crate::icons::{self, Icon, Tint};
use crate::shell::Message;
use crate::tokens::{font_size, spacing};

/// Room around the bar's text.
const PADDING: [f32; 2] = [2.0, 8.0];

/// Room inside a button of the bar, as the C#'s `4,2`.
const BUTTON_PADDING: [f32; 2] = [2.0, 4.0];

/// Space between a button's glyph and its words, as the C#'s.
const GLYPH_GAP: f32 = 3.0;

/// The name of broadcast input's `scope`, `targets` the tabs marked.
#[must_use]
pub fn scope_label(scope: BroadcastScope, targets: usize) -> String {
    match scope {
        BroadcastScope::CurrentTab => fl!("ui-broadcast-scope-current"),
        BroadcastScope::AllTabs => fl!("ui-broadcast-scope-all"),
        BroadcastScope::SelectedTabs => fl!("ui-broadcast-scope-selected", count = targets),
    }
}

/// What a delete or a change of permissions of entries came to, as the C# summary says
/// it, the first failure named.
fn batch_outcome(outcome: &heimdall_app::files::BatchOutcome) -> String {
    use heimdall_app::files::{BatchKind, BatchOutcome};
    match outcome {
        BatchOutcome::Failed {
            kind,
            failed,
            total,
            first,
            reason,
        } => {
            let reason = crate::texts::files_error(reason);
            match (kind, *total) {
                (BatchKind::Delete, 1) => fl!(
                    "ui-status-files-delete-failed",
                    name = first.as_str(),
                    reason = reason
                ),
                (BatchKind::Permissions, 1) => fl!(
                    "ui-status-files-permissions-failed",
                    name = first.as_str(),
                    reason = reason
                ),
                (BatchKind::Delete, _) => fl!(
                    "ui-status-files-delete-partial",
                    failed = (*failed),
                    total = (*total),
                    name = first.as_str(),
                    reason = reason
                ),
                (BatchKind::Permissions, _) => fl!(
                    "ui-status-files-permissions-partial",
                    failed = (*failed),
                    total = (*total),
                    name = first.as_str(),
                    reason = reason
                ),
            }
        }
        BatchOutcome::Stopped { kind, done, total } => match kind {
            BatchKind::Delete => fl!(
                "ui-status-files-delete-stopped",
                done = (*done),
                total = (*total)
            ),
            BatchKind::Permissions => fl!(
                "ui-status-files-permissions-stopped",
                done = (*done),
                total = (*total)
            ),
        },
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
        Notice::FilesSudoMode(true) => fl!("ui-files-sudo-on"),
        Notice::FilesSudoMode(false) => fl!("ui-files-sudo-off"),
        Notice::FilesAutoUploadRefused { name, error } => fl!(
            "ui-status-files-auto-upload-refused",
            name = name.as_str(),
            reason = crate::texts::files_error(error)
        ),
        Notice::FilesCopied(count) => fl!("ui-status-files-copied", count = (*count)),
        Notice::DroppedProfiles { count, folder } => match folder {
            Some(folder) => fl!(
                "ui-status-dropped-profiles",
                count = (*count),
                folder = server_text(folder)
            ),
            None => fl!("ui-status-dropped-profiles-none", count = (*count)),
        },
        Notice::DroppedFolder(name) => {
            fl!("ui-status-dropped-folder", name = server_text(name))
        }
        Notice::DropRefused => fl!("ui-status-drop-refused"),
        Notice::Reordered {
            count,
            name,
            folder,
        } => {
            let folder = folder
                .as_deref()
                .map_or_else(|| fl!("ui-sidebar-group-none"), server_text);
            match name {
                Some(name) => fl!(
                    "ui-status-reordered-one",
                    name = server_text(name),
                    folder = folder
                ),
                None => fl!("ui-status-reordered", count = (*count), folder = folder),
            }
        }
        Notice::MoveUndone => fl!("ui-status-move-undone"),
        Notice::ListingCancelled => fl!("ui-status-listing-cancelled"),
        Notice::SessionLimitReached(max) => fl!("ui-status-session-limit", max = (*max)),
        Notice::SftpAutoOpenFailed(error) => fl!(
            "ui-status-sftp-auto-open-failed",
            reason = crate::texts::error(error)
        ),
        Notice::SftpBrowserDisabled => fl!("ui-status-sftp-browser-disabled"),
        Notice::UndoConflict => fl!("ui-status-undo-conflict"),
        Notice::NothingToUndo => fl!("ui-status-nothing-to-undo"),
        Notice::WakeOnLan(Ok(())) => fl!("ui-status-wake-on-lan-sent"),
        Notice::WakeOnLan(Err(reason)) => {
            fl!("ui-status-wake-on-lan-failed", reason = reason.as_str())
        }
        Notice::FilesDuplicated => fl!("ui-status-files-duplicated"),
        Notice::FilesBatch(outcome) => batch_outcome(outcome),
        _ => String::new(),
    }
}

/// What carrying the settings did, as the C# messages say it.
fn settings_notice(notice: &Notice) -> String {
    match notice {
        Notice::SettingsExported => fl!("ui-status-settings-exported"),
        Notice::SettingsExportFailed(reason) => {
            fl!("ui-status-settings-export-failed", reason = reason.as_str())
        }
        Notice::SettingsImported(count) => fl!("ui-status-settings-imported", count = (*count)),
        Notice::SettingsImportNothing => fl!("ui-status-settings-import-nothing"),
        Notice::SettingsImportInvalid | Notice::SettingsImportNewer => {
            fl!("ui-status-settings-import-invalid")
        }
        Notice::SettingsImportFailed(reason) => {
            fl!("ui-status-settings-import-failed", reason = reason.as_str())
        }
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

/// What the bar says of a Citrix application's launch.
fn citrix_notice(notice: &Notice) -> String {
    match notice {
        Notice::CitrixLaunching => fl!("ui-status-citrix-launching"),
        Notice::CitrixLaunched(name) => {
            fl!("ui-status-citrix-launched", name = server_text(name))
        }
        Notice::CitrixRefused(refusal) => crate::texts::citrix_refusal(refusal),
        _ => String::new(),
    }
}

/// What the bar says of an RDP profile opened in Remote Desktop Connection, or not.
fn rdp_external_notice(notice: &Notice) -> String {
    use heimdall_app::rdp_external::ExternalRefusal;
    match notice {
        Notice::RdpExternalLaunched {
            name,
            gateway: None,
        } => fl!("ui-status-rdp-external-launched", name = server_text(name)),
        Notice::RdpExternalLaunched {
            name,
            gateway: Some(gateway),
        } => fl!(
            "ui-status-rdp-external-launched-gateway",
            name = server_text(name),
            gateway = server_text(gateway)
        ),
        Notice::RdpExternalLaunchedThrough { name, gateway } => fl!(
            "ui-status-rdp-external-launched-through",
            name = server_text(name),
            gateway = server_text(gateway)
        ),
        Notice::RdpExternalRefused(refusal) => match refusal {
            ExternalRefusal::NotWindows => fl!("ui-status-rdp-external-not-windows"),
            ExternalRefusal::Gateway(error) => fl!(
                "ui-status-rdp-external-gateway",
                reason = crate::texts::error(error)
            ),
            ExternalRefusal::Forward(reason) => {
                fl!(
                    "ui-status-rdp-external-forward",
                    reason = server_text(reason)
                )
            }
            ExternalRefusal::NotFound => fl!("ui-status-rdp-external-not-found"),
            ExternalRefusal::NotWritten(reason) => fl!(
                "ui-status-rdp-external-not-written",
                reason = server_text(reason)
            ),
            ExternalRefusal::NotStarted(reason) => fl!(
                "ui-status-rdp-external-not-started",
                reason = server_text(reason)
            ),
        },
        _ => String::new(),
    }
}

/// What the bar says of an SSH profile opened in `PuTTY`, or not, and of its X server.
fn putty_notice(notice: &Notice) -> String {
    use heimdall_app::putty::PuttyRefusal;
    match notice {
        Notice::PuttyLaunched(name) => {
            fl!("ui-status-putty-launched", name = server_text(name))
        }
        Notice::PuttyLaunchedThrough { name, gateway } => fl!(
            "ui-status-putty-launched-through",
            name = server_text(name),
            gateway = server_text(gateway)
        ),
        Notice::X11ServerNotFound => fl!("ui-status-x11-server-not-found"),
        Notice::PuttyRefused(refusal) => match refusal {
            PuttyRefusal::Gateway(error) => fl!(
                "ui-status-putty-gateway",
                reason = crate::texts::error(error)
            ),
            PuttyRefusal::Forward(reason) => {
                fl!("ui-status-putty-forward", reason = server_text(reason))
            }
            PuttyRefusal::InvalidHost => fl!("ui-status-putty-invalid-host"),
            PuttyRefusal::InvalidUsername => fl!("ui-status-putty-invalid-username"),
            PuttyRefusal::KeyFile(path) => fl!(
                "ui-status-putty-key-file",
                path = server_text(&path.display().to_string())
            ),
            PuttyRefusal::HostKey(error) => fl!(
                "ui-status-putty-host-key",
                reason = crate::texts::error(error)
            ),
            PuttyRefusal::NotFound => fl!("ui-status-putty-not-found"),
            PuttyRefusal::NotStarted(reason) => {
                fl!("ui-status-putty-not-started", reason = server_text(reason))
            }
        },
        _ => String::new(),
    }
}

/// What the bar says of a desktop: its size, or the files copied not offered to it.
fn desktop_notice(notice: &Notice) -> String {
    match notice {
        Notice::ResolutionReconnected => fl!("ui-status-resolution-reconnected"),
        Notice::ResolutionScaled => fl!("ui-resolution-larger-than-window"),
        Notice::StabilizationSkipped => fl!("ui-status-stabilization-skipped"),
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
            | Notice::FilesSudoMode(_)
            | Notice::FilesAutoUploadRefused { .. }
            | Notice::FilesCopied(_)
            | Notice::DroppedProfiles { .. }
            | Notice::DroppedFolder(_)
            | Notice::DropRefused
            | Notice::Reordered { .. }
            | Notice::MoveUndone
            | Notice::ListingCancelled
            | Notice::SessionLimitReached(_)
            | Notice::SftpAutoOpenFailed(_)
            | Notice::SftpBrowserDisabled
            | Notice::UndoConflict
            | Notice::NothingToUndo
            | Notice::WakeOnLan(_)
            | Notice::FilesBatch(_)
            | Notice::FilesDuplicated) => files_notice(notice),
            notice @ (Notice::MacroNothingRecorded
            | Notice::MacroSaved(_)
            | Notice::MacroDeleted(_)
            | Notice::MacroEnded { .. }) => crate::macros_view::notice(notice),
            Notice::NoteOpened(name) => fl!("ui-status-note-opened", name = server_text(name)),
            Notice::NoteFailed(reason) => {
                fl!("ui-status-note-failed", reason = server_text(reason))
            }
            notice @ (Notice::SettingsExported
            | Notice::SettingsExportFailed(_)
            | Notice::SettingsImported(_)
            | Notice::SettingsImportNothing
            | Notice::SettingsImportInvalid
            | Notice::SettingsImportNewer
            | Notice::SettingsImportFailed(_)) => settings_notice(notice),
            Notice::GatewayDeleted(name) => {
                fl!("ui-status-gateway-deleted", name = server_text(name))
            }
            Notice::GatewaysReassigned(count) => {
                fl!("ui-status-gateways-reassigned", count = (*count))
            }
            Notice::GatewaysCleared(count) => fl!("ui-status-gateways-cleared", count = (*count)),
            Notice::GatewaysUnchanged => fl!("ui-status-gateways-unchanged"),
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
            Notice::BookmarkRemoved(path) => {
                fl!("ui-files-bookmark-removed", path = path.as_str())
            }
            Notice::FavoriteSaveFailed => fl!("ui-status-favorite-save-failed"),
            Notice::BulkPortUpdated(count) => fl!("ui-status-bulk-port-updated", count = (*count)),
            Notice::BulkPortUnchanged => fl!("ui-status-bulk-port-unchanged"),
            Notice::BulkGatewayUpdated(0) => fl!("ui-status-bulk-gateway-unchanged"),
            Notice::BulkGatewayUpdated(count) => {
                fl!("ui-status-bulk-gateway-updated", count = (*count))
            }
            Notice::BulkUsernameUpdated(count) => {
                fl!("ui-status-bulk-username-updated", count = (*count))
            }
            Notice::BulkUsernameUnchanged => fl!("ui-status-bulk-username-unchanged"),
            Notice::BulkPasswordUpdated {
                count,
                winrm_skipped,
            } => {
                let updated = fl!("ui-status-bulk-password-updated", count = (*count));
                if *winrm_skipped == 0 {
                    updated
                } else {
                    fl!(
                        "ui-status-bulk-password-updated-with-skipped",
                        updated = updated,
                        skipped = fl!("ui-bulk-password-skipped-winrm", count = (*winrm_skipped))
                    )
                }
            }
            Notice::BulkPasswordPartial { count, total } => fl!(
                "ui-status-bulk-password-partial",
                count = (*count),
                total = (*total)
            ),
            Notice::BulkPasswordWinRmSkipped(count) => {
                fl!("ui-bulk-password-skipped-winrm", count = (*count))
            }
            Notice::KnownHostsExported {
                count,
                path,
                skipped,
            } => {
                let exported = fl!(
                    "ui-status-known-hosts-exported",
                    count = (*count),
                    path = path.as_str()
                );
                if *skipped == 0 {
                    exported
                } else {
                    let left_out = fl!("ui-status-known-hosts-export-skipped", count = (*skipped));
                    format!("{exported} {left_out}")
                }
            }
            Notice::KnownHostsExportFailed(detail) if detail.is_empty() => {
                fl!("ui-status-known-hosts-export-no-home")
            }
            Notice::KnownHostsExportFailed(detail) => {
                fl!(
                    "ui-status-known-hosts-export-failed",
                    detail = detail.as_str()
                )
            }
            Notice::SplitMaxPanesReached(max) => fl!("ui-split-max-panes-reached", max = (*max)),
            Notice::DetachSplitRefused => fl!("ui-status-detach-split-refused"),
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
            Notice::ServerCertificatesForgotten(server) => {
                fl!(
                    "ui-status-server-certificates-forgotten",
                    server = server.as_str()
                )
            }
            Notice::ProviderNoPassword(name) => {
                fl!("ui-status-provider-no-password", name = name.as_str())
            }
            Notice::ProviderFailed(detail) => {
                fl!("ui-status-provider-failed", detail = detail.as_str())
            }
            Notice::ProviderTimedOut => fl!("ui-status-provider-timed-out"),
            Notice::WindowsHelloRefused(refusal) => match refusal {
                HelloRefusal::Unavailable => fl!("ui-status-windows-hello-unavailable"),
                HelloRefusal::NotVerified => fl!("ui-status-windows-hello-failed"),
                HelloRefusal::Cancelled => fl!("ui-status-windows-hello-cancelled"),
            },
            Notice::CredentialGuardRequired => fl!("ui-status-credential-guard-required"),
            Notice::VaultHelloEnrolAgainFailed => fl!("ui-status-vault-hello-enrol-again-failed"),
            Notice::LinkNotAFolder(name) => {
                fl!("ui-status-link-not-a-folder", name = name.as_str())
            }
            Notice::WinRmGatewayNtlm => fl!("ui-status-winrm-gateway-ntlm"),
            Notice::ExplorerHoldsNoFiles => fl!("ui-status-explorer-no-files"),
            Notice::DragOutFailed(failure) => match failure {
                DragOutFailure::SeveralFolders => fl!("ui-status-drag-out-several-folders"),
                DragOutFailure::Unavailable => fl!("ui-status-drag-out-unavailable"),
                DragOutFailure::Refused(reason) => {
                    fl!("ui-status-drag-out-failed", reason = reason.as_str())
                }
            },
            notice @ (Notice::ResolutionReconnected
            | Notice::ResolutionScaled
            | Notice::StabilizationSkipped
            | Notice::RdpFilesTooMany
            | Notice::RdpFilesTooLarge
            | Notice::RdpFilesSaveEnded(_)) => desktop_notice(notice),
            Notice::WinRmCertificateSkipped => fl!("ui-status-winrm-certificate-skipped"),
            Notice::FtpCleartext { host, port } => fl!(
                "ui-status-ftp-cleartext",
                host = host.as_str(),
                port = port.to_string()
            ),
            notice @ (Notice::CitrixLaunching
            | Notice::CitrixLaunched(_)
            | Notice::CitrixRefused(_)) => citrix_notice(notice),
            notice @ (Notice::RdpExternalLaunched { .. }
            | Notice::RdpExternalLaunchedThrough { .. }
            | Notice::RdpExternalRefused(_)) => rdp_external_notice(notice),
            notice @ (Notice::PuttyLaunched(_)
            | Notice::PuttyLaunchedThrough { .. }
            | Notice::PuttyRefused(_)
            | Notice::X11ServerNotFound) => putty_notice(notice),
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

/// What the bar said with `what`: a state of the session shown, or a notice.
#[must_use]
pub fn announcement_text(what: &Announcement, targets: usize) -> String {
    match what {
        Announcement::Status(status) => status_text(status, None, targets),
        Announcement::Notice(notice) => status_text(&SessionStatus::Ready, Some(notice), targets),
    }
}

/// What Ctrl+Shift+A copies for a screen reader, one plain line each: what the bar says
/// `now`, then each of the `announced`, newest first, with its time. What the C# live
/// region announced, read back.
#[must_use]
pub fn status_report<'a>(
    now: &str,
    announced: impl IntoIterator<Item = &'a Announced>,
    targets: usize,
) -> String {
    let lines = announced.into_iter().filter_map(|each| {
        let said = announcement_text(&each.what, targets);
        (!said.is_empty()).then(|| fl!("ui-status-report-line", time = each.clock(), text = said))
    });
    std::iter::once(fl!("ui-status-report-now", text = now))
        .chain(lines)
        .collect::<Vec<_>>()
        .join("\n")
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

/// A button of the bar's right, as the C#'s: `icon` in `tint` before `label`, its style
/// and action its caller's.
#[must_use]
pub fn glyph_button(icon: Icon, tint: Tint, label: Element<'_, Message>) -> Button<'_, Message> {
    button(
        row![icons::icon(icon, tint, font_size::CAPTION), label]
            .spacing(GLYPH_GAP)
            .align_y(iced::Alignment::Center),
    )
    .padding(BUTTON_PADDING)
}

/// The bar: `left` and the shortcuts' hint, then at the right `controls`, the tunnels' and
/// broadcast input's, and `sessions` and `tunnels` counted, as the C# runs say them.
#[must_use]
pub fn view(
    left: String,
    controls: Element<'_, Message>,
    sessions: String,
    tunnels: String,
) -> Element<'_, Message> {
    let counts = row![
        text(sessions)
            .size(font_size::CAPTION)
            .style(text::secondary),
        text(fl!("ui-status-separator"))
            .size(font_size::CAPTION)
            .style(text::secondary),
        text(tunnels)
            .size(font_size::CAPTION)
            .style(text::secondary),
    ];
    container(
        row![
            text(left).size(font_size::CAPTION).style(text::secondary),
            crate::shortcuts_view::hint(font_size::SMALL_CAPTION),
            space::horizontal(),
            controls,
            counts,
        ]
        .spacing(spacing::SM)
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
    fn a_run_over_several_entries_is_summed_up_as_the_csharp_says_it() {
        use heimdall_app::files::{BatchKind, BatchOutcome, FilesError};

        let said = |outcome: BatchOutcome| {
            status_text(&SessionStatus::Ready, Some(&Notice::FilesBatch(outcome)), 0)
        };
        let failed = |kind, failed, total| BatchOutcome::Failed {
            kind,
            failed,
            total,
            first: "logs".to_owned(),
            reason: FilesError::Exists,
        };
        let one = said(failed(BatchKind::Delete, 1, 1));
        assert!(one.starts_with("Could not delete \"logs\": "), "{one}");
        let some = said(failed(BatchKind::Delete, 2, 5));
        assert!(
            some.starts_with("2 items out of 5 could not be deleted. First, \"logs\": "),
            "{some}"
        );
        let single = said(failed(BatchKind::Permissions, 1, 3));
        assert!(
            single.starts_with("Permissions could not be changed on 1 item out of 3."),
            "{single}"
        );
        assert_eq!(
            said(BatchOutcome::Stopped {
                kind: BatchKind::Delete,
                done: 1,
                total: 4
            }),
            "Deletion cancelled: 1 of 4 items deleted."
        );
    }

    #[test]
    fn an_rdp_profile_opened_in_remote_desktop_connection_is_said_with_its_reason() {
        use heimdall_app::rdp_external::ExternalRefusal;

        let said = |notice: Notice| status_text(&SessionStatus::Ready, Some(&notice), 0);
        assert_eq!(
            said(Notice::RdpExternalLaunched {
                name: named("dc"),
                gateway: None,
            }),
            "External client launched: dc opened in Remote Desktop Connection."
        );
        let gateway = said(Notice::RdpExternalLaunched {
            name: named("dc"),
            gateway: Some(named("rdg.lab")),
        });
        assert!(
            gateway.contains("rdg.lab")
                && gateway.ends_with("opened in Remote Desktop Connection."),
            "{gateway}"
        );
        assert_eq!(
            said(Notice::RdpExternalLaunchedThrough {
                name: named("dc"),
                gateway: named("bastion"),
            }),
            "External client launched: dc opened in Remote Desktop Connection through the SSH gateway bastion."
        );
        assert_eq!(
            said(Notice::RdpExternalRefused(ExternalRefusal::Gateway(
                heimdall_app::UiError::Cancelled
            )))
            .split_once(':')
            .map(|(head, _)| head),
            Some("Remote Desktop Connection was not opened, its SSH gateway not reached")
        );
        assert!(
            said(Notice::RdpExternalRefused(ExternalRefusal::Forward(named(
                "address in use"
            ))))
            .ends_with("address in use")
        );
        assert_eq!(
            said(Notice::RdpExternalRefused(ExternalRefusal::NotStarted(
                named("denied")
            ))),
            "mstsc.exe did not start: denied"
        );
        for refusal in [
            ExternalRefusal::NotWindows,
            ExternalRefusal::NotFound,
            ExternalRefusal::NotWritten(named("full")),
        ] {
            assert!(!said(Notice::RdpExternalRefused(refusal)).is_empty());
        }
    }

    #[test]
    fn an_ssh_profile_opened_in_putty_is_said_with_its_reason() {
        use heimdall_app::putty::PuttyRefusal;

        let said = |notice: Notice| status_text(&SessionStatus::Ready, Some(&notice), 0);
        assert_eq!(
            said(Notice::PuttyLaunched(named("web"))),
            "External client launched: web opened in PuTTY."
        );
        assert_eq!(
            said(Notice::PuttyLaunchedThrough {
                name: named("web"),
                gateway: named("bastion"),
            }),
            "External client launched: web opened in PuTTY through the SSH gateway bastion."
        );
        assert_eq!(
            said(Notice::PuttyRefused(PuttyRefusal::Gateway(
                heimdall_app::UiError::Cancelled
            )))
            .split_once(':')
            .map(|(head, _)| head),
            Some("PuTTY was not opened, its SSH gateway not reached")
        );
        assert!(
            said(Notice::PuttyRefused(PuttyRefusal::Forward(named(
                "address in use"
            ))))
            .ends_with("address in use")
        );
        assert_eq!(
            said(Notice::X11ServerNotFound),
            "No X11 server found. Install VcXsrv or Xming for X11 forwarding support."
        );
        assert_eq!(
            said(Notice::PuttyRefused(PuttyRefusal::InvalidUsername)),
            "Invalid SSH username (rejected by input validation)."
        );
        assert!(
            said(Notice::PuttyRefused(PuttyRefusal::NotStarted(named(
                "denied"
            ))))
            .ends_with("denied")
        );
        for refusal in [
            PuttyRefusal::InvalidHost,
            PuttyRefusal::KeyFile(std::path::PathBuf::from("keys/id")),
            PuttyRefusal::HostKey(heimdall_app::UiError::Cancelled),
            PuttyRefusal::NotFound,
        ] {
            assert!(!said(Notice::PuttyRefused(refusal)).is_empty());
        }
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
                Notice::BroadcastOn(BroadcastScope::CurrentTab),
                "Broadcast mode ON - Current tab",
            ),
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
    fn the_status_report_says_now_then_each_announcement_newest_first_with_its_time() {
        use std::time::{Duration, UNIX_EPOCH};

        let announced = |what, seconds| Announced {
            what,
            at: UNIX_EPOCH + Duration::from_secs(seconds),
        };
        let newer = announced(Announcement::Notice(Notice::BroadcastOff), 90);
        let older = announced(
            Announcement::Status(SessionStatus::Connected(named("web"))),
            30,
        );
        let report = status_report("Broadcast mode OFF", [&newer, &older], 0);
        assert_eq!(
            report,
            format!(
                "Now: Broadcast mode OFF
{} Broadcast mode OFF
{} Connected to: web",
                newer.clock(),
                older.clock()
            )
        );
        assert_eq!(status_report("Ready", [], 0), "Now: Ready", "nothing yet");
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
