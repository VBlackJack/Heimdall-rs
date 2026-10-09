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

//! An OpenSSH client configuration block, as the C# `SshConfigGeneratorView`
//! (`Views/Tools/SshConfigGeneratorView.xaml.cs:170-215`) writes it: `Host`, then
//! `HostName`, then only the options that are not OpenSSH's defaults.

use super::number_text;

/// SSH's port, which the block leaves out, as the C# `DefaultPorts.Ssh`.
pub const DEFAULT_PORT: i64 = 22;

/// The keep-alive interval a new form offers, as the C# `DefaultAliveIntervalInput`.
pub const DEFAULT_ALIVE_INTERVAL_INPUT: i64 = 60;

/// The keep-alive interval of a box not read, which leaves it out, as the C#
/// `DefaultAliveInterval`.
pub const DEFAULT_ALIVE_INTERVAL: i64 = 0;

/// The indent of an option, as the C# `ConfigIndent`.
const INDENT: &str = "    ";

/// What the form says of a host.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct HostBlock {
    /// The `Host` alias.
    pub alias: String,
    /// The `HostName`.
    pub host_name: String,
    /// The `User`, left out when blank.
    pub user: String,
    /// The `Port`, left out when 22.
    pub port: i64,
    /// The `IdentityFile`, left out when blank.
    pub identity_file: String,
    /// The `ProxyJump`, left out when blank.
    pub proxy_jump: String,
    /// `ForwardAgent yes`, when set.
    pub forward_agent: bool,
    /// The `ServerAliveInterval`, left out unless positive.
    pub alive_interval: i64,
}

/// The block of `host`, as the C# `GenerateConfigBlock`: one option a line, the options
/// indented, no line break after the last.
#[must_use]
pub fn generate(host: &HostBlock, new_line: &str) -> String {
    let mut lines = vec![
        format!("Host {}", host.alias),
        format!("{INDENT}HostName {}", host.host_name),
    ];
    if !host.user.trim().is_empty() {
        lines.push(format!("{INDENT}User {}", host.user));
    }
    if host.port != DEFAULT_PORT {
        lines.push(format!("{INDENT}Port {}", host.port));
    }
    if !host.identity_file.trim().is_empty() {
        lines.push(format!("{INDENT}IdentityFile {}", host.identity_file));
    }
    if !host.proxy_jump.trim().is_empty() {
        lines.push(format!("{INDENT}ProxyJump {}", host.proxy_jump));
    }
    if host.forward_agent {
        lines.push(format!("{INDENT}ForwardAgent yes"));
    }
    if host.alive_interval > 0 {
        lines.push(format!(
            "{INDENT}ServerAliveInterval {}",
            host.alive_interval
        ));
    }
    lines.join(new_line).trim_end().to_owned()
}

/// `text` as the C# `ParseInt` reads it (`SshConfigGeneratorView.xaml.cs:217-220`): .NET's
/// `int.TryParse` of it, `fallback` when it does not read.
#[must_use]
pub fn parse_int(text: &str, fallback: i64) -> i64 {
    number_text::parse_int32(text).map_or(fallback, i64::from)
}
