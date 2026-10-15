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

//! Wi-Fi wireless network inspection and scanning engine.
//!
//! Scans visible Wi-Fi access points and parses SSID, BSSID, signal strength,
//! channel, authentication, encryption, and radio type.

use std::process::Command;

/// A single visible wireless network access point entry.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct WifiEntry {
    /// Network SSID name.
    pub ssid: String,
    /// Hardware BSSID MAC address.
    pub bssid: String,
    /// Human-readable signal percentage (e.g. "85%").
    pub signal: String,
    /// Numerical signal level (0..=100).
    pub signal_value: i32,
    /// Radio channel.
    pub channel: String,
    /// Authentication standard (e.g. "WPA2-Personal", "Open").
    pub auth: String,
    /// Encryption cipher (e.g. "CCMP", "None").
    pub encryption: String,
    /// Radio standard (e.g. "802.11ax", "802.11ac").
    pub radio_type: String,
}

impl WifiEntry {
    /// Formats the wireless network entry as a CSV line.
    #[must_use]
    pub fn to_csv_line(&self) -> String {
        format!(
            "{},{},{},{},{},{},{},{}",
            self.ssid,
            self.bssid,
            self.signal,
            self.signal_value,
            self.channel,
            self.auth,
            self.encryption,
            self.radio_type
        )
    }
}

/// Parser for `netsh wlan show networks mode=bssid` output.
pub struct NetshWifiParser;

impl NetshWifiParser {
    /// Parses `netsh wlan show networks mode=bssid` text into a list of [`WifiEntry`].
    #[must_use]
    pub fn parse(output: &str) -> Vec<WifiEntry> {
        let mut results = Vec::new();
        let mut current_ssid = String::new();
        let mut current_auth = String::new();
        let mut current_encryption = String::new();
        let mut current_bssid = String::new();
        let mut current_signal = String::new();
        let mut current_signal_val = 0;
        let mut current_channel = String::new();
        let mut current_radio = String::new();
        let mut in_bssid = false;

        for raw_line in output.lines() {
            let line = raw_line.trim();
            if line.is_empty() {
                continue;
            }

            let upper = line.to_uppercase();
            if upper.starts_with("SSID") && !upper.starts_with("BSSID") {
                Self::flush(
                    &mut results,
                    &current_ssid,
                    &current_bssid,
                    &current_signal,
                    current_signal_val,
                    &current_channel,
                    &current_auth,
                    &current_encryption,
                    &current_radio,
                    in_bssid,
                );

                current_ssid = Self::extract_value(line);
                current_auth.clear();
                current_encryption.clear();
                current_bssid.clear();
                current_signal.clear();
                current_signal_val = 0;
                current_channel.clear();
                current_radio.clear();
                in_bssid = false;
                continue;
            }

            if line.starts_with("Authentication") || line.starts_with("Authentification") {
                current_auth = Self::extract_value(line);
                continue;
            }

            if line.starts_with("Encryption") || line.starts_with("Chiffrement") {
                current_encryption = Self::extract_value(line);
                continue;
            }

            if upper.starts_with("BSSID") {
                Self::flush(
                    &mut results,
                    &current_ssid,
                    &current_bssid,
                    &current_signal,
                    current_signal_val,
                    &current_channel,
                    &current_auth,
                    &current_encryption,
                    &current_radio,
                    in_bssid,
                );

                current_bssid = Self::extract_value(line);
                current_signal.clear();
                current_signal_val = 0;
                current_channel.clear();
                current_radio.clear();
                in_bssid = true;
                continue;
            }

            if !in_bssid {
                continue;
            }

            if line.starts_with("Signal") {
                let sig = Self::extract_value(line);
                current_signal = sig.clone();
                let num_str = sig.trim_end_matches('%').trim();
                current_signal_val = num_str.parse::<i32>().unwrap_or(0);
                continue;
            }

            if line.starts_with("Radio type") || line.starts_with("Type de radio") {
                current_radio = Self::extract_value(line);
                continue;
            }

            if line.starts_with("Channel") || line.starts_with("Canal") {
                current_channel = Self::extract_value(line);
            }
        }

        Self::flush(
            &mut results,
            &current_ssid,
            &current_bssid,
            &current_signal,
            current_signal_val,
            &current_channel,
            &current_auth,
            &current_encryption,
            &current_radio,
            in_bssid,
        );

        results.sort_by(|a, b| b.signal_value.cmp(&a.signal_value));
        results
    }

    #[allow(clippy::too_many_arguments)]
    fn flush(
        results: &mut Vec<WifiEntry>,
        ssid: &str,
        bssid: &str,
        signal: &str,
        signal_val: i32,
        channel: &str,
        auth: &str,
        encryption: &str,
        radio: &str,
        in_bssid: bool,
    ) {
        if in_bssid && !bssid.is_empty() {
            results.push(WifiEntry {
                ssid: ssid.to_owned(),
                bssid: bssid.to_owned(),
                signal: signal.to_owned(),
                signal_value: signal_val,
                channel: channel.to_owned(),
                auth: auth.to_owned(),
                encryption: encryption.to_owned(),
                radio_type: radio.to_owned(),
            });
        }
    }

    fn extract_value(line: &str) -> String {
        if let Some(colon) = line.find(':') {
            line[colon + 1..].trim().to_owned()
        } else {
            String::new()
        }
    }
}

/// Executes the system wireless scanning command (`netsh` or `nmcli`).
#[must_use]
pub fn run_wifi_command() -> Vec<WifiEntry> {
    #[cfg(windows)]
    let output = Command::new("netsh")
        .args(["wlan", "show", "networks", "mode=bssid"])
        .output();

    #[cfg(not(windows))]
    let output = Command::new("nmcli")
        .args(["-f", "SSID,BSSID,SIGNAL,CHAN,SECURITY", "dev", "wifi"])
        .output();

    match output {
        Ok(out) => {
            let stdout = String::from_utf8_lossy(&out.stdout);
            NetshWifiParser::parse(&stdout)
        }
        Err(_) => Vec::new(),
    }
}

/// Builds a CSV string from a slice of [`WifiEntry`].
#[must_use]
pub fn build_wifi_csv(entries: &[WifiEntry]) -> String {
    let mut csv = String::from("SSID,BSSID,Signal,SignalValue,Channel,Auth,Encryption,RadioType\n");
    for w in entries {
        csv.push_str(&w.to_csv_line());
        csv.push('\n');
    }
    csv
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_wifi_parser_sample() {
        let sample = "\
Interface name : Wi-Fi 
There are 2 networks currently visible. 

SSID 1 : HomeNetwork
    Network type            : Infrastructure
    Authentication          : WPA2-Personal
    Encryption              : CCMP 
    BSSID 1                 : 10:20:30:40:50:60
         Signal             : 82% 
         Radio type         : 802.11ax
         Channel            : 36 

SSID 2 : GuestWiFi
    Network type            : Infrastructure
    Authentication          : Open
    Encryption              : None 
    BSSID 1                 : aa:bb:cc:dd:ee:ff
         Signal             : 45% 
         Radio type         : 802.11ac
         Channel            : 6 
";
        let list = NetshWifiParser::parse(sample);
        assert_eq!(list.len(), 2);

        // Highest signal first
        assert_eq!(list[0].ssid, "HomeNetwork");
        assert_eq!(list[0].bssid, "10:20:30:40:50:60");
        assert_eq!(list[0].signal, "82%");
        assert_eq!(list[0].signal_value, 82);
        assert_eq!(list[0].channel, "36");
        assert_eq!(list[0].auth, "WPA2-Personal");
        assert_eq!(list[0].radio_type, "802.11ax");

        assert_eq!(list[1].ssid, "GuestWiFi");
        assert_eq!(list[1].signal_value, 45);
    }
}
