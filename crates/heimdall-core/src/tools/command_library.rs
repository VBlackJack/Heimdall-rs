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

//! The command snippet library: a fixed catalog of administration, network and security
//! commands, each with the values it asks for put in its text. Every text shown about a
//! snippet (its title, description, category, platform and the label of each value) is
//! named here by an enumeration and written by the window in its language.

/// Target operating system platform for a snippet.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SnippetPlatform {
    /// Linux / Unix / POSIX shell.
    Linux,
    /// Windows PowerShell / CMD.
    Windows,
    /// Cross-platform / agnostic commands.
    CrossPlatform,
}

impl SnippetPlatform {
    /// Every platform, in the order the filter offers them.
    pub const ALL: [Self; 3] = [Self::Linux, Self::Windows, Self::CrossPlatform];
}

/// What a snippet is about.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SnippetCategory {
    /// Sockets, routes, names and reachability.
    Network,
    /// Processes and the system.
    System,
    /// Services and their logs.
    Services,
    /// Certificates and exposure.
    Security,
    /// Path quality and troubleshooting.
    Diagnostics,
}

/// Which snippet: its title and description are the window's to write.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SnippetId {
    /// `ss -tulpen`.
    LinuxListeningPorts,
    /// `fuser -k`.
    LinuxKillPort,
    /// `journalctl -f`.
    LinuxJournalFollow,
    /// `openssl s_client` and `x509 -dates`.
    LinuxCertExpiry,
    /// `Test-NetConnection`.
    WindowsTestConnection,
    /// `Get-NetTCPConnection`.
    WindowsPortOwner,
    /// `Restart-Service`.
    WindowsRestartService,
    /// `Clear-DnsClientCache`.
    WindowsFlushDns,
    /// `mtr`.
    MtrProbe,
    /// `nmap -sV -sC`.
    NmapServiceScan,
}

/// What a value of a snippet is, named by its label.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParamLabel {
    /// A host name or address.
    Host,
    /// A port.
    Port,
    /// A local port.
    LocalPort,
    /// Ports or ranges of ports.
    PortRange,
    /// A service's name.
    Service,
}

/// A parameter variable within a command snippet.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SnippetParam {
    /// Placeholder name (e.g. "host", "port").
    pub key: &'static str,
    /// What the value is.
    pub label: ParamLabel,
    /// Default initial value.
    pub default_value: &'static str,
}

/// A command snippet template with description and parameters.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandSnippet {
    /// Which it is.
    pub id: SnippetId,
    /// What it is about.
    pub category: SnippetCategory,
    /// Platform scope.
    pub platform: SnippetPlatform,
    /// Command template text with `{key}` placeholders.
    pub template: &'static str,
    /// Configurable parameters.
    pub parameters: &'static [SnippetParam],
}

impl CommandSnippet {
    /// The command, each `{key}` replaced by the value `value_of` gives for it, its default
    /// when none or a blank one is given.
    #[must_use]
    pub fn render<'v>(&self, value_of: impl Fn(&'static str) -> Option<&'v str>) -> String {
        let mut result = self.template.to_owned();
        for p in self.parameters {
            let val = value_of(p.key)
                .filter(|s| !s.trim().is_empty())
                .unwrap_or(p.default_value);

            let placeholder = format!("{{{}}}", p.key);
            result = result.replace(&placeholder, val);
        }
        result
    }
}

/// Curated built-in library of administrator and operational snippets.
pub const BUILTIN_SNIPPETS: &[CommandSnippet] = &[
    CommandSnippet {
        id: SnippetId::LinuxListeningPorts,
        category: SnippetCategory::Network,
        platform: SnippetPlatform::Linux,
        template: "ss -tulpen",
        parameters: &[],
    },
    CommandSnippet {
        id: SnippetId::LinuxKillPort,
        category: SnippetCategory::System,
        platform: SnippetPlatform::Linux,
        template: "fuser -k {port}/tcp",
        parameters: &[SnippetParam {
            key: "port",
            label: ParamLabel::Port,
            default_value: "8080",
        }],
    },
    CommandSnippet {
        id: SnippetId::LinuxJournalFollow,
        category: SnippetCategory::Services,
        platform: SnippetPlatform::Linux,
        template: "journalctl -u {service} -f --no-tail",
        parameters: &[SnippetParam {
            key: "service",
            label: ParamLabel::Service,
            default_value: "nginx",
        }],
    },
    CommandSnippet {
        id: SnippetId::LinuxCertExpiry,
        category: SnippetCategory::Security,
        platform: SnippetPlatform::Linux,
        template: "openssl s_client -connect {host}:{port} -servername {host} </dev/null 2>/dev/null | openssl x509 -noout -dates -subject",
        parameters: &[
            SnippetParam {
                key: "host",
                label: ParamLabel::Host,
                default_value: "example.com",
            },
            SnippetParam {
                key: "port",
                label: ParamLabel::Port,
                default_value: "443",
            },
        ],
    },
    CommandSnippet {
        id: SnippetId::WindowsTestConnection,
        category: SnippetCategory::Network,
        platform: SnippetPlatform::Windows,
        template: "Test-NetConnection -ComputerName {host} -Port {port}",
        parameters: &[
            SnippetParam {
                key: "host",
                label: ParamLabel::Host,
                default_value: "1.1.1.1",
            },
            SnippetParam {
                key: "port",
                label: ParamLabel::Port,
                default_value: "443",
            },
        ],
    },
    CommandSnippet {
        id: SnippetId::WindowsPortOwner,
        category: SnippetCategory::Network,
        platform: SnippetPlatform::Windows,
        template: "Get-NetTCPConnection -LocalPort {port} | Select-Object LocalAddress, LocalPort, OwningProcess, State",
        parameters: &[SnippetParam {
            key: "port",
            label: ParamLabel::LocalPort,
            default_value: "80",
        }],
    },
    CommandSnippet {
        id: SnippetId::WindowsRestartService,
        category: SnippetCategory::Services,
        platform: SnippetPlatform::Windows,
        template: "Restart-Service -Name '{service}' -Force",
        parameters: &[SnippetParam {
            key: "service",
            label: ParamLabel::Service,
            default_value: "wuauserv",
        }],
    },
    CommandSnippet {
        id: SnippetId::WindowsFlushDns,
        category: SnippetCategory::Network,
        platform: SnippetPlatform::Windows,
        template: "Clear-DnsClientCache",
        parameters: &[],
    },
    CommandSnippet {
        id: SnippetId::MtrProbe,
        category: SnippetCategory::Diagnostics,
        platform: SnippetPlatform::CrossPlatform,
        template: "mtr -rw -c 50 {host}",
        parameters: &[SnippetParam {
            key: "host",
            label: ParamLabel::Host,
            default_value: "8.8.8.8",
        }],
    },
    CommandSnippet {
        id: SnippetId::NmapServiceScan,
        category: SnippetCategory::Security,
        platform: SnippetPlatform::CrossPlatform,
        template: "nmap -sV -sC -T4 -p {port} {host}",
        parameters: &[
            SnippetParam {
                key: "host",
                label: ParamLabel::Host,
                default_value: "192.168.1.1",
            },
            SnippetParam {
                key: "port",
                label: ParamLabel::PortRange,
                default_value: "22,80,443",
            },
        ],
    },
];

/// The snippet of `id`.
#[must_use]
pub fn snippet(id: SnippetId) -> Option<&'static CommandSnippet> {
    BUILTIN_SNIPPETS.iter().find(|snippet| snippet.id == id)
}

/// Whether `snippet` is shown under the platform chosen, `None` being every platform: a
/// cross-platform snippet is shown under each.
#[must_use]
pub fn on_platform(snippet: &CommandSnippet, platform: Option<SnippetPlatform>) -> bool {
    platform.is_none_or(|platform| {
        snippet.platform == platform || snippet.platform == SnippetPlatform::CrossPlatform
    })
}

/// Whether `query` is in the snippet's command or in one of `texts`, its title, description
/// and category as the window writes them, ignoring case; an empty query matches all.
#[must_use]
pub fn matches_query(snippet: &CommandSnippet, texts: &[&str], query: &str) -> bool {
    let query = query.trim().to_lowercase();
    query.is_empty()
        || snippet.template.to_lowercase().contains(&query)
        || texts
            .iter()
            .any(|text| text.to_lowercase().contains(&query))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_snippet_takes_the_values_given_else_its_defaults() {
        let snippet = snippet(SnippetId::WindowsTestConnection).expect("in the catalog");
        let rendered = snippet.render(|key| match key {
            "host" => Some("my-server.local"),
            "port" => Some("  "),
            _ => None,
        });
        assert_eq!(
            rendered,
            "Test-NetConnection -ComputerName my-server.local -Port 443"
        );
    }

    #[test]
    fn every_snippet_is_in_the_catalog_once_with_its_placeholders() {
        for snippet in BUILTIN_SNIPPETS {
            assert_eq!(
                BUILTIN_SNIPPETS
                    .iter()
                    .filter(|other| other.id == snippet.id)
                    .count(),
                1
            );
            for parameter in snippet.parameters {
                assert!(
                    snippet.template.contains(&format!("{{{}}}", parameter.key)),
                    "{:?} {}",
                    snippet.id,
                    parameter.key
                );
            }
        }
    }

    #[test]
    fn platform_and_query_filter_the_catalog() {
        let listening = snippet(SnippetId::LinuxListeningPorts).expect("in the catalog");
        let mtr = snippet(SnippetId::MtrProbe).expect("in the catalog");
        assert!(on_platform(listening, None));
        assert!(on_platform(listening, Some(SnippetPlatform::Linux)));
        assert!(!on_platform(listening, Some(SnippetPlatform::Windows)));
        assert!(on_platform(mtr, Some(SnippetPlatform::Windows)));
        assert!(matches_query(
            listening,
            &["List listening sockets"],
            "LISTENING"
        ));
        assert!(matches_query(listening, &[], "tulpen"));
        assert!(!matches_query(
            listening,
            &["List listening sockets"],
            "nmap"
        ));
        assert!(matches_query(listening, &[], " "));
    }
}
