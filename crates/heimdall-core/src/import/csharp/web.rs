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

//! The C# WEB profiles (`ServerProfileDto.cs:466-490`): a web console's host, port, scheme,
//! path, account and gateway. Its password, DPAPI-encrypted, is never read, as no secret is;
//! the program the C# could open it with is left out, said so: the system's browser opens
//! every web console here.

use serde::Deserialize;

use super::{Dropped, LegacyServer, SkipReason, non_empty, routed_gateway};
use crate::profile::{ProfileId, WebProfile};
use crate::web::{DEFAULT_HTTPS_PORT, default_port_for};

/// `connectionType` of a web console, compared without case as the C# catalog does.
pub(super) const WEB_CONNECTION_TYPE: &str = "WEB";

/// The web fields of a C# server, read beside the others, each by its C# key.
#[derive(Debug, Default, Deserialize)]
pub(super) struct LegacyWeb {
    /// Absent is the C# default, `DefaultPorts.HttpsStd`; zero or less is the scheme's.
    #[serde(rename = "webPort")]
    port: Option<i64>,
    /// "Use plain HTTP (unencrypted)"; absent is off.
    #[serde(default, rename = "webUseHttp")]
    use_http: bool,
    #[serde(rename = "webPath")]
    path: Option<String>,
    #[serde(rename = "webUsername")]
    username: Option<String>,
    /// "Open with an external program instead of a tab".
    #[serde(default, rename = "webOpenExternally")]
    open_externally: bool,
    /// The program it opens with; empty is the default browser.
    #[serde(rename = "webExternalProgram")]
    external_program: Option<String>,
}

/// Whether `connection_type` is a web console's.
pub(super) fn is_web(connection_type: &str) -> bool {
    connection_type.eq_ignore_ascii_case(WEB_CONNECTION_TYPE)
}

/// A web console as the C# `WebHandler` opens it: on the address its host, port, scheme and
/// path form, through the SSH gateway [`routed_gateway`] decides. A host that forms no
/// address is kept as written, as the C# keeps it: opening it says why, and the form asks
/// for it to be corrected.
pub(super) fn convert(server: &LegacyServer) -> Result<WebProfile, SkipReason> {
    if server.id.is_empty() {
        return Err(SkipReason::MissingId);
    }
    if server.remote_server.trim().is_empty() {
        return Err(SkipReason::MissingHost);
    }
    let web = &server.web;
    let port = match web.port {
        None => DEFAULT_HTTPS_PORT,
        Some(value) if value <= 0 => default_port_for(web.use_http),
        Some(value) => u16::try_from(value).map_err(|_| SkipReason::InvalidPort(value))?,
    };
    Ok(WebProfile {
        remote_os: super::remote_os_of(server),
        id: ProfileId::new(server.id.clone()),
        name: if server.display_name.trim().is_empty() {
            server.remote_server.clone()
        } else {
            server.display_name.clone()
        },
        group: non_empty(server.group.as_ref()),
        host: server.remote_server.trim().to_owned(),
        port,
        use_http: web.use_http,
        path: trimmed(web.path.as_ref()),
        username: trimmed(web.username.as_ref()),
        gateway: routed_gateway(server),
    })
}

/// What a web console asked for that is left out: the program it was opened with, the
/// system's browser opening it here.
pub(super) fn dropped(server: &LegacyServer) -> Vec<Dropped> {
    let web = &server.web;
    let program = web
        .external_program
        .as_deref()
        .is_some_and(|program| !program.trim().is_empty());
    if web.open_externally && program {
        vec![Dropped::WebExternalProgram]
    } else {
        Vec::new()
    }
}

/// `value` trimmed, blank as none, as the C# dialog saves it.
fn trimmed(value: Option<&String>) -> Option<String> {
    value
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
}
