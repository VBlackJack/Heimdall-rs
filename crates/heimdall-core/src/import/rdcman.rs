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

//! Remote Desktop Connection Manager files (`.rdg`), read as the C# `RdcManImporter` reads
//! them: each group a folder, each server a Remote Desktop session, its account, port and
//! gateway taken from the server or else from its group. Passwords are never read. A DTD
//! is refused.
//!
//! `RDCMan` 2.7 and later put a name under `<properties>`, 2.2 directly under the element;
//! both are read, where the C# reads the second only and finds no server in a recent file.

use roxmltree::{Document, Node};
use serde_json::{Map, Value};

use super::foreign::{FileWarning, Parsed, set_text};

/// What a gateway's `enabled` holds when it is off.
const DISABLED: &str = "False";

/// Reads an `RDCMan` file.
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
    // `<file>` itself, or the `<file>` elements under `<RDCMan>`.
    let files: Vec<Node<'_, '_>> = if root.tag_name().name() == "file" {
        vec![root]
    } else {
        root.descendants()
            .filter(|node| node.tag_name().name() == "file")
            .collect()
    };
    for file in files {
        group(file, "", &mut parsed);
    }
    parsed
}

fn child<'a, 'input>(node: Node<'a, 'input>, name: &str) -> Option<Node<'a, 'input>> {
    node.children()
        .find(|child| child.tag_name().name() == name)
}

/// The text of `name` under `node`, or under its `<properties>`.
fn value<'a>(node: Node<'a, '_>, name: &str) -> Option<&'a str> {
    child(node, name)
        .or_else(|| child(node, "properties").and_then(|properties| child(properties, name)))
        .and_then(|found| found.text())
        .map(str::trim)
        .filter(|text| !text.is_empty())
}

fn group(element: Node<'_, '_>, path: &str, parsed: &mut Parsed) {
    let path = match (path.is_empty(), value(element, "name")) {
        (true, name) => name.unwrap_or_default().to_owned(),
        (false, Some(name)) => format!("{path}/{name}"),
        (false, None) => path.to_owned(),
    };
    for node in element.children() {
        match node.tag_name().name() {
            "server" => {
                if let Some(fields) = server(node, &path, element) {
                    parsed.push(fields);
                }
            }
            "group" => group(node, &path, parsed),
            _ => {}
        }
    }
}

/// A server as the C# `ParseServer`: `None` when it has no name.
fn server(node: Node<'_, '_>, path: &str, parent: Node<'_, '_>) -> Option<Map<String, Value>> {
    let name = value(node, "displayName").or_else(|| value(node, "name"))?;
    let host = value(node, "name").unwrap_or(name);
    let mut dto = Map::new();
    set_text(&mut dto, "displayName", name);
    set_text(&mut dto, "remoteServer", host);
    dto.insert("connectionType".to_owned(), Value::String("RDP".to_owned()));
    set_text(&mut dto, "group", path);
    let inherited = |name: &str| child(node, name).or_else(|| child(parent, name));
    if let Some(credentials) = inherited("logonCredentials")
        && let Some(user) = value(credentials, "userName")
    {
        set_text(&mut dto, "rdpUsername", user);
        set_text(
            &mut dto,
            "rdpDomain",
            value(credentials, "domain").unwrap_or_default(),
        );
    }
    if let Some(port) = inherited("connectionSettings")
        .and_then(|settings| value(settings, "port"))
        .and_then(|port| port.parse::<i64>().ok())
        .filter(|port| *port > 0)
    {
        dto.insert("remotePort".to_owned(), Value::from(port));
    }
    // A gateway switched off is not one, unlike the C# which reads the name alone.
    if let Some(gateway) = child(node, "gatewaySettings")
        && value(gateway, "enabled") != Some(DISABLED)
    {
        set_text(
            &mut dto,
            "rdpGateway",
            value(gateway, "hostName").unwrap_or_default(),
        );
    }
    Some(dto)
}
