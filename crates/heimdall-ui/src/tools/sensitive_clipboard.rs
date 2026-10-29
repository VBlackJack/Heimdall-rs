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

//! A secret on the clipboard kept out of the Windows clipboard history, the cloud clipboard
//! and the clipboard monitors, as the C# `SensitiveClipboardData` and `SetSensitiveText`
//! (`ClipboardService.cs`): the text, and three registered formats Windows reads before it
//! records, syncs or hands the content on. iced's clipboard writes text only, so on Windows
//! the write goes through `clipboard-win`; elsewhere it stays iced's.

use iced::Task;

use crate::shell::Message;

/// The registered format that excludes the content from monitor processing.
#[cfg(any(windows, test))]
const EXCLUDE_FROM_MONITOR_FORMAT: &str = "ExcludeClipboardContentFromMonitorProcessing";

/// The registered format whose DWORD says whether the history may keep the content.
#[cfg(any(windows, test))]
const CAN_INCLUDE_IN_HISTORY_FORMAT: &str = "CanIncludeInClipboardHistory";

/// The registered format whose DWORD says whether the cloud clipboard may take the content.
#[cfg(any(windows, test))]
const CAN_UPLOAD_TO_CLOUD_FORMAT: &str = "CanUploadToCloudClipboard";

/// The exclusion marker's value, as the C# `ExcludeMarkerValue`: only its presence matters.
#[cfg(any(windows, test))]
const EXCLUDE_MARKER_VALUE: u32 = 1;

/// The DWORD that switches the history and the cloud upload off, as the C# `FlagDisabled`.
#[cfg(any(windows, test))]
const FLAG_DISABLED: u32 = 0;

/// How many times the clipboard is asked for, another program holding it.
#[cfg(windows)]
const CLIPBOARD_ATTEMPTS: usize = 10;

/// How long before the clipboard is asked for again.
#[cfg(windows)]
const CLIPBOARD_RETRY: std::time::Duration = std::time::Duration::from_millis(20);

/// The registered formats a secret carries beside its text, each with its DWORD, little
/// endian as Windows stores it.
#[cfg(any(windows, test))]
fn marker_formats() -> [(&'static str, [u8; 4]); 3] {
    [
        (
            EXCLUDE_FROM_MONITOR_FORMAT,
            EXCLUDE_MARKER_VALUE.to_le_bytes(),
        ),
        (CAN_INCLUDE_IN_HISTORY_FORMAT, FLAG_DISABLED.to_le_bytes()),
        (CAN_UPLOAD_TO_CLOUD_FORMAT, FLAG_DISABLED.to_le_bytes()),
    ]
}

/// `text` written to the clipboard as a secret. Where the clipboard cannot carry the markers
/// (not Windows, or the write failed) it is written as plain text, as before.
pub fn write(text: String) -> Task<Message> {
    #[cfg(windows)]
    {
        let secret = zeroize::Zeroizing::new(text);
        let fallback = secret.clone();
        Task::perform(
            async move {
                tokio::task::spawn_blocking(move || write_marked(&secret))
                    .await
                    .unwrap_or_else(|error| Err(error.to_string()))
            },
            |written| written,
        )
        .then(move |written| match written {
            Ok(()) => Task::none(),
            Err(error) => {
                log::warn!("the secret did not reach the clipboard with its markers: {error}");
                iced::clipboard::write(fallback.to_string())
            }
        })
    }
    #[cfg(not(windows))]
    {
        iced::clipboard::write(text)
    }
}

/// `text` and its markers put on the clipboard in place of what it held. The markers are
/// registered before anything is written, and the clipboard is emptied again when one cannot
/// be set, so the text never stays without them.
#[cfg(windows)]
fn write_marked(text: &str) -> Result<(), String> {
    use clipboard_win::{Clipboard, raw};
    let mut formats = Vec::new();
    for (name, value) in marker_formats() {
        let id =
            raw::register_format(name).ok_or_else(|| format!("format {name} not registered"))?;
        formats.push((id.get(), value));
    }
    // Another program may hold the clipboard a moment: asked again a little later.
    let mut open = None;
    for _ in 0..CLIPBOARD_ATTEMPTS {
        if let Ok(held) = Clipboard::new() {
            open = Some(held);
            break;
        }
        std::thread::sleep(CLIPBOARD_RETRY);
    }
    let _open = open.ok_or_else(|| "the clipboard stayed held".to_owned())?;
    raw::set_string(text).map_err(|error| error.to_string())?;
    for (id, value) in formats {
        if let Err(error) = raw::set_without_clear(id, &value) {
            let _ = raw::empty();
            return Err(error.to_string());
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_secret_carries_the_three_formats_windows_reads_before_it_records_the_content() {
        let names: Vec<&str> = marker_formats().iter().map(|(name, _)| *name).collect();
        assert_eq!(
            names,
            [
                "ExcludeClipboardContentFromMonitorProcessing",
                "CanIncludeInClipboardHistory",
                "CanUploadToCloudClipboard",
            ]
        );
    }

    #[test]
    fn the_exclusion_marker_is_one_and_the_history_and_cloud_flags_are_zero_as_dwords() {
        let [exclude, history, cloud] = marker_formats();
        assert_eq!(exclude.1, [1, 0, 0, 0]);
        assert_eq!(history.1, [0, 0, 0, 0]);
        assert_eq!(cloud.1, [0, 0, 0, 0]);
    }
}
