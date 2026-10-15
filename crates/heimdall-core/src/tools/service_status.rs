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

//! System services enumeration and lifecycle dashboard.
//!
//! Provides inspection of operating system services, status (Running / Stopped),
//! start type, and administrative action execution (Start, Stop, Restart).

use std::process::Command;

/// A single system service entry.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct ServiceEntry {
    /// Internal system service name (e.g. "wuauserv", "sshd").
    pub name: String,
    /// User-friendly display title.
    pub display_name: String,
    /// Current execution state (e.g. "Running", "Stopped").
    pub status: String,
    /// Startup trigger mode (e.g. "Automatic", "Manual", "Disabled").
    pub start_type: String,
}

impl ServiceEntry {
    /// Formats the service entry as a CSV line.
    #[must_use]
    pub fn to_csv_line(&self) -> String {
        format!(
            "\"{}\",\"{}\",\"{}\",\"{}\"",
            self.name, self.display_name, self.status, self.start_type
        )
    }

    /// Checks if the service is currently in a running state.
    #[must_use]
    pub fn is_running(&self) -> bool {
        self.status.eq_ignore_ascii_case("running")
    }
}

/// Service administrative action verb.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ServiceAction {
    /// Start the stopped service.
    Start,
    /// Stop the running service.
    Stop,
    /// Restart the service.
    Restart,
}

/// Parser for PowerShell `ConvertTo-Csv` output.
pub struct PowershellServiceCsvParser;

impl PowershellServiceCsvParser {
    /// Parses CSV text output from PowerShell into a list of [`ServiceEntry`].
    #[must_use]
    pub fn parse(csv: &str) -> Vec<ServiceEntry> {
        let mut entries = Vec::new();

        for raw_line in csv.lines() {
            let line = raw_line.trim();
            if line.is_empty() {
                continue;
            }

            let fields = Self::parse_csv_line(line);
            if fields.len() < 4 {
                continue;
            }

            // Header line check
            if fields[0].eq_ignore_ascii_case("Name")
                && fields[1].eq_ignore_ascii_case("DisplayName")
            {
                continue;
            }

            entries.push(ServiceEntry {
                name: fields[0].clone(),
                display_name: fields[1].clone(),
                status: fields[2].clone(),
                start_type: fields[3].clone(),
            });
        }

        entries
    }

    fn parse_csv_line(line: &str) -> Vec<String> {
        let mut fields = Vec::new();
        let mut current = String::new();
        let mut in_quotes = false;

        let chars: Vec<char> = line.chars().collect();
        let mut i = 0;
        while i < chars.len() {
            let c = chars[i];
            if c == '"' {
                if in_quotes && i + 1 < chars.len() && chars[i + 1] == '"' {
                    current.push('"');
                    i += 1;
                } else {
                    in_quotes = !in_quotes;
                }
            } else if c == ',' && !in_quotes {
                fields.push(current.trim().to_owned());
                current.clear();
            } else {
                current.push(c);
            }
            i += 1;
        }

        fields.push(current.trim().to_owned());
        fields
    }
}

/// Executes the system command to fetch local services.
pub fn run_services_command() -> Result<Vec<ServiceEntry>, String> {
    #[cfg(windows)]
    {
        let script = "Get-Service | Select-Object Name,DisplayName,Status,StartType | ConvertTo-Csv -NoTypeInformation";
        let output = Command::new("powershell")
            .args(["-NoProfile", "-Command", script])
            .output()
            .map_err(|err| format!("Failed to execute PowerShell: {err}"))?;

        if !output.status.success() {
            return Err(String::from_utf8_lossy(&output.stderr).into_owned());
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        Ok(PowershellServiceCsvParser::parse(&stdout))
    }

    #[cfg(not(windows))]
    {
        let output = Command::new("systemctl")
            .args(["list-units", "--type=service", "--all", "--no-pager"])
            .output()
            .map_err(|err| format!("Failed to execute systemctl: {err}"))?;

        let stdout = String::from_utf8_lossy(&output.stdout);
        let mut entries = Vec::new();
        for line in stdout.lines() {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() >= 4 && parts[0].ends_with(".service") {
                entries.push(ServiceEntry {
                    name: parts[0].to_owned(),
                    display_name: parts[0].to_owned(),
                    status: if parts[2] == "active" {
                        "Running".to_owned()
                    } else {
                        "Stopped".to_owned()
                    },
                    start_type: parts[1].to_owned(),
                });
            }
        }
        Ok(entries)
    }
}

/// Executes an administrative service action (Start, Stop, Restart) on `service_name`.
pub fn run_service_action(action: ServiceAction, service_name: &str) -> Result<(), String> {
    let clean_name = service_name.trim();
    if clean_name.is_empty() {
        return Err("Service name cannot be empty".to_owned());
    }

    #[cfg(windows)]
    {
        let verb = match action {
            ServiceAction::Start => "Start-Service",
            ServiceAction::Stop => "Stop-Service",
            ServiceAction::Restart => "Restart-Service",
        };
        let script = format!("{verb} -Name '{clean_name}'");
        let output = Command::new("powershell")
            .args(["-NoProfile", "-Command", &script])
            .output()
            .map_err(|err| format!("Failed to run service action: {err}"))?;

        if !output.status.success() {
            return Err(String::from_utf8_lossy(&output.stderr).into_owned());
        }
        Ok(())
    }

    #[cfg(not(windows))]
    {
        let verb = match action {
            ServiceAction::Start => "start",
            ServiceAction::Stop => "stop",
            ServiceAction::Restart => "restart",
        };
        let output = Command::new("systemctl")
            .args([verb, clean_name])
            .output()
            .map_err(|err| format!("Failed to run systemctl action: {err}"))?;

        if !output.status.success() {
            return Err(String::from_utf8_lossy(&output.stderr).into_owned());
        }
        Ok(())
    }
}

/// Builds a CSV string from a slice of [`ServiceEntry`].
#[must_use]
pub fn build_services_csv(entries: &[ServiceEntry]) -> String {
    let mut csv = String::from("Name,DisplayName,Status,StartType\n");
    for s in entries {
        csv.push_str(&s.to_csv_line());
        csv.push('\n');
    }
    csv
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_powershell_service_csv_parser() {
        let sample = "\
\"Name\",\"DisplayName\",\"Status\",\"StartType\"
\"Appinfo\",\"Application Information\",\"Running\",\"Manual\"
\"AudioSrv\",\"Windows Audio\",\"Running\",\"Automatic\"
\"wuauserv\",\"Windows Update\",\"Stopped\",\"Manual\"
";
        let services = PowershellServiceCsvParser::parse(sample);
        assert_eq!(services.len(), 3);

        assert_eq!(services[0].name, "Appinfo");
        assert_eq!(services[0].display_name, "Application Information");
        assert_eq!(services[0].status, "Running");
        assert_eq!(services[0].start_type, "Manual");
        assert!(services[0].is_running());

        assert_eq!(services[1].name, "AudioSrv");
        assert!(services[1].is_running());

        assert_eq!(services[2].name, "wuauserv");
        assert_eq!(services[2].status, "Stopped");
        assert!(!services[2].is_running());
    }
}
