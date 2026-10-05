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

//! What a profile says of its server besides how to reach it, as the C# server form's
//! Metadata section: its environment, its tags, and the MAC address Wake-on-LAN wakes it
//! with. The same for every protocol, kept beside the profiles rather than in each one.

use std::fmt;
use std::str::FromStr;

/// The environment a server belongs to, as the C# list offers them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Environment {
    /// Production.
    Production,
    /// Staging.
    Staging,
    /// A lab.
    Lab,
    /// The user's own.
    Personal,
}

impl Environment {
    /// Every environment, in the C# list's order.
    pub const ALL: [Self; 4] = [Self::Production, Self::Staging, Self::Lab, Self::Personal];

    /// The name the C# and the profile file keep it under.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::Production => "Production",
            Self::Staging => "Staging",
            Self::Lab => "Lab",
            Self::Personal => "Personal",
        }
    }

    /// The environment named `name`, whatever its case; `None` for "None", empty, or one
    /// not known.
    #[must_use]
    pub fn named(name: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|environment| environment.name().eq_ignore_ascii_case(name.trim()))
    }
}

/// A network card's address, six bytes, as Wake-on-LAN needs it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct MacAddress(pub [u8; 6]);

/// Why a MAC address typed cannot be read.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InvalidMacAddress;

impl fmt::Display for InvalidMacAddress {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("not a MAC address")
    }
}

impl std::error::Error for InvalidMacAddress {}

/// Separators accepted between the pairs of digits, as the C# accepts them.
const MAC_SEPARATORS: [char; 3] = [':', '-', '.'];

impl FromStr for MacAddress {
    type Err = InvalidMacAddress;

    /// Twelve hexadecimal digits, whatever their case, with `:`, `-` or `.` anywhere
    /// between them or none, as the C# `WakeOnLan` reads them.
    fn from_str(typed: &str) -> Result<Self, Self::Err> {
        let digits: String = typed
            .trim()
            .chars()
            .filter(|c| !MAC_SEPARATORS.contains(c))
            .collect();
        if digits.len() != 12 || !digits.chars().all(|c| c.is_ascii_hexdigit()) {
            return Err(InvalidMacAddress);
        }
        let mut bytes = [0; 6];
        for (index, byte) in bytes.iter_mut().enumerate() {
            *byte = u8::from_str_radix(&digits[index * 2..index * 2 + 2], 16)
                .map_err(|_| InvalidMacAddress)?;
        }
        Ok(Self(bytes))
    }
}

impl fmt::Display for MacAddress {
    /// `AA:BB:CC:DD:EE:FF`, as the C# form shows it.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let pairs: Vec<String> = self.0.iter().map(|byte| format!("{byte:02X}")).collect();
        f.write_str(&pairs.join(":"))
    }
}

impl MacAddress {
    /// The Wake-on-LAN magic packet waking this card: six bytes of `0xFF`, then the address
    /// sixteen times, as the C# sends it.
    #[must_use]
    pub fn magic_packet(self) -> Vec<u8> {
        let mut packet = vec![0xFF; 6];
        for _ in 0..16 {
            packet.extend_from_slice(&self.0);
        }
        packet
    }
}

/// What a profile says of its server besides how to reach it.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ProfileMetadata {
    /// Its environment, when one is chosen.
    pub environment: Option<Environment>,
    /// Its tags, as typed: words the search finds it by.
    pub tags: String,
    /// The MAC address Wake-on-LAN wakes it with.
    pub mac_address: Option<MacAddress>,
}

impl ProfileMetadata {
    /// Whether it says nothing: then nothing is kept.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.environment.is_none() && self.tags.trim().is_empty() && self.mac_address.is_none()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_mac_address_is_read_as_the_csharp_reads_it_and_written_one_way() {
        let expected = MacAddress([0xAA, 0xBB, 0xCC, 0x0D, 0xEE, 0x0F]);
        for typed in [
            "AA:BB:CC:0D:EE:0F",
            "aa-bb-cc-0d-ee-0f",
            "aabb.cc0d.ee0f",
            " AABBCC0DEE0F ",
        ] {
            assert_eq!(typed.parse(), Ok(expected), "{typed:?}");
        }
        for refused in [
            "",
            "AA:BB:CC:DD:EE",
            "AA:BB:CC:DD:EE:FF:00",
            "GG:BB:CC:DD:EE:FF",
        ] {
            assert_eq!(
                refused.parse::<MacAddress>(),
                Err(InvalidMacAddress),
                "{refused:?}"
            );
        }
        assert_eq!(expected.to_string(), "AA:BB:CC:0D:EE:0F");
    }

    #[test]
    fn the_magic_packet_is_six_ff_then_the_address_sixteen_times() {
        let mac = MacAddress([1, 2, 3, 4, 5, 6]);
        let packet = mac.magic_packet();
        assert_eq!(packet.len(), 102);
        assert_eq!(&packet[..6], &[0xFF; 6]);
        assert!(
            packet[6..]
                .chunks(6)
                .all(|chunk| chunk == [1, 2, 3, 4, 5, 6])
        );
    }

    #[test]
    fn an_environment_is_known_by_its_csharp_name() {
        assert_eq!(
            Environment::named("production"),
            Some(Environment::Production)
        );
        assert_eq!(Environment::named(" Lab "), Some(Environment::Lab));
        assert_eq!(Environment::named("None"), None);
        assert_eq!(Environment::named(""), None);
    }
}
