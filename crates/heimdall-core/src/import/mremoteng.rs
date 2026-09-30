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

//! `mRemoteNG` connections (`confCons.xml`), read as the C# `MRemoteNgImporter` reads them:
//! each `Container` node a folder, each `Connection` node a session. A file encrypted whole
//! is refused; passwords, encrypted per node, are never read. A DTD is refused.
//!
//! Unlike the C#, a protocol Heimdall does not have (HTTP, HTTPS, an external application)
//! is left out and said, rather than imported as a Remote Desktop session that cannot work.

use roxmltree::{Document, Node};
use serde_json::{Map, Value};

use super::foreign::{FileWarning, Parsed, set_text};

/// `mRemoteNG`'s protocol names and the C# `ConnectionType` each maps to; an absent protocol
/// is RDP, as `mRemoteNG`'s default.
const PROTOCOLS: [(&str, &str); 5] = [
    ("RDP", "RDP"),
    ("SSH1", "SSH"),
    ("SSH2", "SSH"),
    ("VNC", "VNC"),
    ("Telnet", "Telnet"),
];

/// `mRemoteNG`'s protocol a node without one uses.
const DEFAULT_PROTOCOL: &str = "RDP";

/// Rlogin, which the C# maps onto Telnet.
const RLOGIN: &str = "Rlogin";

/// `mRemoteNG`'s colour depths and their bits, raised to Heimdall's 16-bit floor as the C#
/// `NormalizeColorDepth`.
const COLORS: [(&str, i64); 5] = [
    ("Colors256", 16),
    ("Colors15Bit", 16),
    ("Colors16Bit", 16),
    ("Colors24Bit", 24),
    ("Colors32Bit", 32),
];

/// An unknown colour depth, as the C#.
const DEFAULT_COLOR_BITS: i64 = 32;

/// The gateway usage that never goes through the gateway named.
const GATEWAY_NEVER: &str = "Never";

/// The resolution that follows the window.
const SMART_SIZE: &str = "SmartSize";

/// Reads an `mRemoteNG` file.
#[must_use]
pub fn parse(text: &str) -> Parsed {
    let mut parsed = Parsed::default();
    let document = match Document::parse(text) {
        Ok(document) => document,
        Err(error) => {
            parsed
                .warnings
                .push(FileWarning::Unreadable(error.to_string()));
            return parsed;
        }
    };
    let root = document.root_element();
    if root
        .attribute("FullFileEncryption")
        .is_some_and(|value| value.eq_ignore_ascii_case("true"))
    {
        parsed.warnings.push(FileWarning::FullyEncrypted);
        return parsed;
    }
    nodes(root, "", &mut parsed);
    parsed
}

fn nodes(parent: Node<'_, '_>, group: &str, parsed: &mut Parsed) {
    for node in parent
        .children()
        .filter(|node| node.tag_name().name() == "Node")
    {
        let kind = node.attribute("Type").unwrap_or("Connection");
        if kind.eq_ignore_ascii_case("Container") {
            let name = node.attribute("Name").unwrap_or_default().trim();
            let path = if group.trim().is_empty() {
                name.to_owned()
            } else {
                format!("{group}/{name}")
            };
            nodes(node, &path, parsed);
        } else if kind.eq_ignore_ascii_case("Connection")
            && let Some(fields) = connection(node, group)
        {
            parsed.push(fields);
        }
    }
}

fn text<'a>(node: Node<'a, '_>, name: &str) -> &'a str {
    node.attribute(name).map(str::trim).unwrap_or_default()
}

/// A connection as the C# `ParseConnection`: `None` when it has neither name nor host.
fn connection(node: Node<'_, '_>, group: &str) -> Option<Map<String, Value>> {
    let (name, host) = (text(node, "Name"), text(node, "Hostname"));
    if name.is_empty() && host.is_empty() {
        return None;
    }
    let protocol = match text(node, "Protocol") {
        "" => DEFAULT_PROTOCOL,
        protocol => protocol,
    };
    let connection_type = if protocol.eq_ignore_ascii_case(RLOGIN) {
        "Telnet".to_owned()
    } else {
        PROTOCOLS
            .iter()
            .find(|(known, _)| known.eq_ignore_ascii_case(protocol))
            // Left out by the conversion, as a protocol Heimdall does not have.
            .map_or_else(|| protocol.to_owned(), |(_, kind)| (*kind).to_owned())
    };
    let mut dto = Map::new();
    set_text(
        &mut dto,
        "displayName",
        if name.is_empty() { host } else { name },
    );
    set_text(
        &mut dto,
        "remoteServer",
        if host.is_empty() { name } else { host },
    );
    set_text(&mut dto, "group", group);
    let port = text(node, "Port")
        .parse::<i64>()
        .ok()
        .filter(|port| *port > 0);
    let (user, domain) = (text(node, "Username"), text(node, "Domain"));
    // As the C#, `user@domain` outside RDP; RDP keeps the domain its own field.
    let full_user = if domain.is_empty() || user.is_empty() {
        user.to_owned()
    } else {
        format!("{user}@{domain}")
    };
    match connection_type.as_str() {
        "RDP" => {
            if let Some(port) = port {
                dto.insert("remotePort".to_owned(), Value::from(port));
            }
            set_text(&mut dto, "rdpUsername", user);
            if !user.is_empty() {
                set_text(&mut dto, "rdpDomain", domain);
            }
            rdp_settings(node, &mut dto);
        }
        "SSH" => {
            set_port(&mut dto, "sshPort", port);
            set_text(&mut dto, "sshUsername", &full_user);
        }
        "VNC" => set_port(&mut dto, "vncPort", port),
        "Telnet" => {
            set_port(&mut dto, "telnetPort", port);
            set_text(&mut dto, "telnetUsername", &full_user);
        }
        _ => {}
    }
    dto.insert("connectionType".to_owned(), Value::String(connection_type));
    Some(dto)
}

fn set_port(dto: &mut Map<String, Value>, key: &str, port: Option<i64>) {
    if let Some(port) = port {
        dto.insert(key.to_owned(), Value::from(port));
    }
}

/// The RDP settings a node carries; any of them makes the profile stop following the
/// application's defaults, as the C#.
fn rdp_settings(node: Node<'_, '_>, dto: &mut Map<String, Value>) {
    let mut own = false;
    if let Some(colors) = node.attribute("Colors") {
        let bits = COLORS
            .iter()
            .find(|(name, _)| *name == colors)
            .map_or(DEFAULT_COLOR_BITS, |(_, bits)| *bits);
        dto.insert("rdpColorDepth".to_owned(), Value::from(bits));
        own = true;
    }
    if node
        .attribute("Resolution")
        .is_some_and(|resolution| resolution.eq_ignore_ascii_case(SMART_SIZE))
    {
        dto.insert("rdpDynamicResolution".to_owned(), Value::Bool(true));
        own = true;
    }
    for (attribute, key) in [
        ("RedirectClipboard", "rdpRedirectClipboard"),
        ("RedirectDiskDrives", "rdpRedirectDrives"),
    ] {
        if let Some(value) = node.attribute(attribute) {
            dto.insert(
                key.to_owned(),
                Value::Bool(value.eq_ignore_ascii_case("true")),
            );
            own = true;
        }
    }
    // A gateway named but never used is not one, unlike the C# which reads the name alone.
    if !text(node, "RDGatewayUsageMethod").eq_ignore_ascii_case(GATEWAY_NEVER) {
        set_text(dto, "rdpGateway", text(node, "RDGatewayHostname"));
    }
    if own {
        dto.insert("rdpUseGlobalDefaults".to_owned(), Value::Bool(false));
    }
}
