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

//! Command library, cheat sheets, and script snippet manager.
//!
//! Provides a curated catalog of sysadmin, network, and operational commands with
//! interactive parameter substitution and custom snippet persistence.

use std::collections::HashMap;

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
    /// Display label for UI tags.
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Linux => "Linux",
            Self::Windows => "Windows",
            Self::CrossPlatform => "Cross-Platform",
        }
    }
}

/// A parameter variable within a command snippet.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SnippetParam {
    /// Placeholder name (e.g. "host", "port").
    pub key: &'static str,
    /// Display prompt label.
    pub label: &'static str,
    /// Default initial value.
    pub default_value: &'static str,
}

/// A command snippet template with description and parameters.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandSnippet {
    /// Unique identifier.
    pub id: &'static str,
    /// Category / tag (e.g. "Network", "Services", "Security").
    pub category: &'static str,
    /// Title name of the command snippet.
    pub title: &'static str,
    /// Detailed description.
    pub description: &'static str,
    /// Platform scope.
    pub platform: SnippetPlatform,
    /// Command template text with `{key}` placeholders.
    pub template: &'static str,
    /// Configurable parameters.
    pub parameters: &'static [SnippetParam],
}

impl CommandSnippet {
    /// Renders the command by replacing `{key}` placeholders with values from `params`.
    #[must_use]
    pub fn render(&self, params: &HashMap<String, String>) -> String {
        let mut result = self.template.to_owned();
        for p in self.parameters {
            let val = params
                .get(p.key)
                .map(String::as_str)
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
        id: "lin-listening-ports",
        category: "Network",
        title: "List Listening Sockets",
        description: "Display all active listening TCP and UDP sockets with processes and PIDs.",
        platform: SnippetPlatform::Linux,
        template: "ss -tulpen",
        parameters: &[],
    },
    CommandSnippet {
        id: "lin-kill-port",
        category: "System",
        title: "Kill Process on Port",
        description: "Forcibly terminates the process listening on the specified TCP port.",
        platform: SnippetPlatform::Linux,
        template: "fuser -k {port}/tcp",
        parameters: &[SnippetParam {
            key: "port",
            label: "Port number",
            default_value: "8080",
        }],
    },
    CommandSnippet {
        id: "lin-journal-follow",
        category: "Services",
        title: "Follow Service Journal",
        description: "Continuously tails systemd journal log entries for a service in real time.",
        platform: SnippetPlatform::Linux,
        template: "journalctl -u {service} -f --no-tail",
        parameters: &[SnippetParam {
            key: "service",
            label: "Systemd service name",
            default_value: "nginx",
        }],
    },
    CommandSnippet {
        id: "lin-cert-expiry",
        category: "Security",
        title: "Inspect Remote TLS Certificate Expiry",
        description: "Connects via TLS SNI and prints validity dates without full handshake.",
        platform: SnippetPlatform::Linux,
        template: "openssl s_client -connect {host}:{port} -servername {host} </dev/null 2>/dev/null | openssl x509 -noout -dates -subject",
        parameters: &[
            SnippetParam {
                key: "host",
                label: "Target Host / FQDN",
                default_value: "example.com",
            },
            SnippetParam {
                key: "port",
                label: "Port",
                default_value: "443",
            },
        ],
    },
    CommandSnippet {
        id: "win-test-net-conn",
        category: "Network",
        title: "Test TCP Port Reachability",
        description: "PowerShell native reachability and TCP port handshake probe.",
        platform: SnippetPlatform::Windows,
        template: "Test-NetConnection -ComputerName {host} -Port {port}",
        parameters: &[
            SnippetParam {
                key: "host",
                label: "Target Host / IP",
                default_value: "1.1.1.1",
            },
            SnippetParam {
                key: "port",
                label: "Port",
                default_value: "443",
            },
        ],
    },
    CommandSnippet {
        id: "win-find-port-pid",
        category: "Network",
        title: "Find Process Owning Port",
        description: "Finds the local socket state and owning PID for a specific port.",
        platform: SnippetPlatform::Windows,
        template: "Get-NetTCPConnection -LocalPort {port} | Select-Object LocalAddress, LocalPort, OwningProcess, State",
        parameters: &[SnippetParam {
            key: "port",
            label: "Local Port",
            default_value: "80",
        }],
    },
    CommandSnippet {
        id: "win-restart-service",
        category: "Services",
        title: "Restart Windows Service",
        description: "Stops and starts a Windows background service immediately.",
        platform: SnippetPlatform::Windows,
        template: "Restart-Service -Name '{service}' -Force",
        parameters: &[SnippetParam {
            key: "service",
            label: "Service Name",
            default_value: "wuauserv",
        }],
    },
    CommandSnippet {
        id: "win-flush-dns",
        category: "Network",
        title: "Flush DNS Resolver Cache",
        description: "Purges client DNS cache entries on Windows.",
        platform: SnippetPlatform::Windows,
        template: "Clear-DnsClientCache",
        parameters: &[],
    },
    CommandSnippet {
        id: "net-mtr-probe",
        category: "Diagnostics",
        title: "MTR Network Path Quality",
        description: "Sends 50 packets per hop to diagnose packet loss and latency jitter.",
        platform: SnippetPlatform::CrossPlatform,
        template: "mtr -rw -c 50 {host}",
        parameters: &[SnippetParam {
            key: "host",
            label: "Remote Host",
            default_value: "8.8.8.8",
        }],
    },
    CommandSnippet {
        id: "sec-nmap-service-scan",
        category: "Security",
        title: "Nmap Service Version Probe",
        description: "Probes open ports to determine service and version info.",
        platform: SnippetPlatform::CrossPlatform,
        template: "nmap -sV -sC -T4 -p {port} {host}",
        parameters: &[
            SnippetParam {
                key: "host",
                label: "Target Host",
                default_value: "192.168.1.1",
            },
            SnippetParam {
                key: "port",
                label: "Port Range",
                default_value: "22,80,443",
            },
        ],
    },
];

/// Filters snippets based on category, platform, and search query.
#[must_use]
pub fn filter_snippets<'a>(
    snippets: &'a [CommandSnippet],
    category_filter: Option<&str>,
    platform_filter: Option<SnippetPlatform>,
    query: &str,
) -> Vec<&'a CommandSnippet> {
    let q = query.trim().to_lowercase();
    snippets
        .iter()
        .filter(|s| {
            if let Some(cat) = category_filter {
                if !s.category.eq_ignore_ascii_case(cat) {
                    return false;
                }
            }
            if let Some(plat) = platform_filter {
                if s.platform != SnippetPlatform::CrossPlatform && s.platform != plat {
                    return false;
                }
            }
            if q.is_empty() {
                true
            } else {
                s.title.to_lowercase().contains(&q)
                    || s.description.to_lowercase().contains(&q)
                    || s.template.to_lowercase().contains(&q)
                    || s.category.to_lowercase().contains(&q)
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_snippet_render() {
        let snippet = &BUILTIN_SNIPPETS[4]; // Test-NetConnection
        let mut params = HashMap::new();
        params.insert("host".to_owned(), "my-server.local".to_owned());
        params.insert("port".to_owned(), "8443".to_owned());

        let rendered = snippet.render(&params);
        assert_eq!(
            rendered,
            "Test-NetConnection -ComputerName my-server.local -Port 8443"
        );
    }

    #[test]
    fn test_snippet_filter() {
        let matched = filter_snippets(BUILTIN_SNIPPETS, Some("Network"), None, "listen");
        assert_eq!(matched.len(), 1);
        assert_eq!(matched[0].id, "lin-listening-ports");
    }
}
