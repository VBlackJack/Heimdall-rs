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

//! Web addresses handed to the system's browser, as the C# `ExternalUrlPolicy` decides
//! them: an absolute `http` or `https` address only. What a server prints never reaches
//! the system as anything else, a `file:` or `javascript:` address, a program or a switch.

use std::io;

/// The schemes handed to the browser, as the C# policy keeps them.
const SCHEMES: [&str; 2] = ["http://", "https://"];

/// `url` as it is handed to the browser, when it may be: an `http` or `https` address
/// naming a host, holding no space, quote or control character; its scheme lowercased.
#[must_use]
pub fn launchable_url(url: &str) -> Option<String> {
    let scheme = SCHEMES.iter().find(|scheme| {
        url.get(..scheme.len())
            .is_some_and(|start| start.eq_ignore_ascii_case(scheme))
    })?;
    let rest = &url[scheme.len()..];
    let host = rest.split(['/', '?', '#']).next().unwrap_or_default();
    let refused = |ch: char| ch.is_whitespace() || ch.is_control() || matches!(ch, '"' | '\'');
    (!host.is_empty() && !host.starts_with(':') && !url.chars().any(refused))
        .then(|| format!("{scheme}{rest}"))
}

/// Opens `url`, already [launchable](launchable_url), in the system's browser.
///
/// # Errors
///
/// The browser could not be started.
pub fn open_url(url: &str) -> io::Result<()> {
    let mut command = if cfg!(windows) {
        // The protocol handler, given the address alone: no shell reads it.
        let mut command = std::process::Command::new("rundll32.exe");
        command.arg("url.dll,FileProtocolHandler");
        command
    } else if cfg!(target_os = "macos") {
        std::process::Command::new("/usr/bin/open")
    } else {
        std::process::Command::new("xdg-open")
    };
    command
        .arg(url)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .map(drop)
}

#[cfg(test)]
mod tests {
    use super::launchable_url;

    #[test]
    fn only_a_web_address_naming_a_host_is_handed_to_the_browser() {
        assert_eq!(
            launchable_url("https://git.lab/team/repo?tab=1#top").as_deref(),
            Some("https://git.lab/team/repo?tab=1#top")
        );
        assert_eq!(
            launchable_url("HTTP://Wiki.lab").as_deref(),
            Some("http://Wiki.lab"),
            "the scheme lowercased"
        );
        for refused in [
            "file:///C:/Windows/System32/calc.exe",
            "javascript:alert(1)",
            "ms-settings:privacy",
            "https://",
            "https:///path",
            "https://:8080",
            "https://host/a b",
            "https://host/\"quoted\"",
            "https://host/\u{7}",
            "/c calc",
            "",
        ] {
            assert_eq!(launchable_url(refused), None, "{refused}");
        }
    }
}
