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

//! The offline tools' engines against the C# tests' vectors: `UlidGeneratorTests.cs`,
//! `PosixModeTests.cs`, `SymbolicChmodParserTests.cs`, `DateTimeParserTests.cs`,
//! `RelativeTimeComputerTests.cs` and `IpCodecTests.cs`; and the computations of the C#
//! crontab builder, SSH config generator, subnet calculator and network calculator views,
//! which the C# does not test, against what their code computes.

use std::collections::HashSet;
use std::net::{IpAddr, Ipv4Addr};

use heimdall_core::tools::cron_builder::{self, CronProblem, Description, Minute, WeekDay};
use heimdall_core::tools::date_time::{
    self, DetectedFormat, Instant, RelativeUnit, TICKS_PER_SECOND,
};
use heimdall_core::tools::ip_address;
use heimdall_core::tools::ip_codec;
use heimdall_core::tools::network_calculator::{self, NetCalcError};
use heimdall_core::tools::number_text;
use heimdall_core::tools::posix_mode::{PosixMode, PosixPermission, PosixRole};
use heimdall_core::tools::ssh_config::{self, HostBlock};
use heimdall_core::tools::subnet_calculator::{self, HostCount};
use heimdall_core::tools::time_zone_rules::{self, TimeZoneEntry, ZoneRules};
use heimdall_core::tools::ulid_generator::{self, RANDOM_BYTE_COUNT, TEXT_LENGTH};

/// How far a ULID's time may be from the clock read around it, as the C# test.
const CLOCK_SLACK_MS: u64 = 2000;

fn now_millis() -> u64 {
    u64::try_from(Instant::now().unix_ticks() / 10_000).expect("after 1970")
}

/// A wall clock in UTC, for dates written without an offset.
fn utc(_wall: i64) -> i32 {
    0
}

// ULID: `UlidGeneratorTests.cs`.

#[test]
fn a_ulid_encodes_as_the_csharp_vectors() {
    let zero = [0_u8; RANDOM_BYTE_COUNT];
    assert_eq!(
        ulid_generator::encode(0, &zero).expect("in range"),
        "00000000000000000000000000"
    );
    assert_eq!(
        &ulid_generator::encode(32, &zero).expect("in range")[..10],
        "0000000010"
    );
    assert_eq!(
        &ulid_generator::encode(0xFFFF_FFFF_FFFF, &zero).expect("in range")[..10],
        "7ZZZZZZZZZ"
    );
    assert_eq!(
        &ulid_generator::encode(0, &[0xFF; RANDOM_BYTE_COUNT]).expect("in range")[10..],
        "ZZZZZZZZZZZZZZZZ"
    );
    assert!(ulid_generator::encode(0x1_0000_0000_0000, &zero).is_err());
}

#[test]
fn a_ulid_is_26_crockford_characters_unique_and_timed_now() {
    let before = now_millis();
    let values: Vec<String> = (0..100)
        .map(|_| ulid_generator::generate().expect("random"))
        .collect();
    let after = now_millis();
    for value in &values {
        assert_eq!(value.len(), TEXT_LENGTH);
        assert!(
            value
                .bytes()
                .all(|byte| ulid_generator::ALPHABET.contains(&byte)),
            "{value}"
        );
        assert!(!value.contains(['I', 'L', 'O', 'U']));
        let stamp = ulid_generator::timestamp_ms(value).expect("decodes");
        assert!(
            (before - CLOCK_SLACK_MS..=after + CLOCK_SLACK_MS).contains(&stamp),
            "{stamp}"
        );
    }
    let distinct: HashSet<&String> = values.iter().collect();
    assert_eq!(distinct.len(), values.len());
}

#[test]
fn ulids_made_in_turn_sort_in_turn() {
    let mut values = Vec::new();
    for _ in 0..5 {
        values.push(ulid_generator::generate().expect("random"));
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    let mut sorted = values.clone();
    sorted.sort();
    assert_eq!(values, sorted);
}

// Chmod: `PosixModeTests.cs` and `SymbolicChmodParserTests.cs`.

#[test]
fn an_octal_mode_reads_and_writes_as_the_csharp_vectors() {
    for (octal, digits) in [
        ("755", [7, 5, 5]),
        ("644", [6, 4, 4]),
        ("000", [0, 0, 0]),
        ("777", [7, 7, 7]),
    ] {
        let mode = PosixMode::parse_octal(octal).expect(octal);
        let read: Vec<u8> = PosixRole::ALL
            .iter()
            .map(|role| mode.digit(*role))
            .collect();
        assert_eq!(read, digits);
        assert_eq!(mode.to_octal(), octal);
    }
    for invalid in ["", "75", "7555", "78a", "abc", "999", " 755"] {
        assert_eq!(PosixMode::parse_octal(invalid), None, "{invalid:?}");
    }
    for (octal, symbolic) in [
        ("755", "rwxr-xr-x"),
        ("644", "rw-r--r--"),
        ("600", "rw-------"),
        ("700", "rwx------"),
    ] {
        assert_eq!(
            PosixMode::parse_octal(octal).expect(octal).to_symbolic(),
            symbolic
        );
    }
    let presets: Vec<String> = PosixMode::PRESETS.iter().map(|m| m.to_octal()).collect();
    assert_eq!(presets, ["644", "755", "600", "700", "777"]);
    assert_eq!(PosixMode::EMPTY.to_octal(), "000");
}

#[test]
fn a_bit_is_given_and_taken_and_read() {
    let mode = PosixMode::EMPTY
        .with_bit(PosixRole::Owner, PosixPermission::Read, true)
        .with_bit(PosixRole::Group, PosixPermission::Write, true)
        .with_bit(PosixRole::Others, PosixPermission::Execute, true)
        .with_bit(PosixRole::Group, PosixPermission::Write, false);
    assert_eq!(mode.to_octal(), "401");
    let mode = PosixMode::parse_octal("751").expect("mode");
    let bits: Vec<bool> = PosixRole::ALL
        .iter()
        .flat_map(|role| {
            PosixPermission::ALL
                .iter()
                .map(move |permission| mode.has(*role, *permission))
        })
        .collect();
    assert_eq!(
        bits,
        [true, true, true, true, false, true, false, false, true]
    );
}

#[test]
fn a_symbolic_notation_reads_as_the_csharp_vectors() {
    for (input, octal) in [
        ("u+x", "100"),
        ("g+w", "020"),
        ("o+r", "004"),
        ("a+r", "444"),
        ("u=rw", "600"),
        ("g=rx", "050"),
        ("o=", "000"),
        ("u+rwx,g+rx,o+r", "754"),
        ("u=rw,g=r,o=", "640"),
        ("u+x,,g+w", "120"),
        ("a+rwx,g=", "707"),
        (" u+x , g+w ", "120"),
    ] {
        assert_eq!(
            PosixMode::parse_symbolic(input).map(PosixMode::to_octal),
            Some(octal.to_owned()),
            "{input}"
        );
    }
    for invalid in [
        "",
        " ",
        "z+r",
        "u?x",
        "u+r,garbage",
        ",",
        "u+s",
        "+r",
        "u+-x",
    ] {
        assert_eq!(PosixMode::parse_symbolic(invalid), None, "{invalid:?}");
    }
}

// Date and time: `DateTimeParserTests.cs` and `RelativeTimeComputerTests.cs`.

#[test]
fn a_unix_time_reads_in_seconds_or_milliseconds_as_the_csharp_vectors() {
    for invalid in ["", "   "] {
        assert_eq!(date_time::parse(invalid, &utc), None);
    }
    for (input, seconds) in [
        ("0", 0),
        (" 1712345678 ", 1_712_345_678),
        ("-62135596800", -62_135_596_800),
        ("32503680000", 32_503_680_000),
    ] {
        let parsed = date_time::parse(input, &utc).expect(input);
        assert_eq!(parsed.detected, DetectedFormat::UnixSeconds, "{input}");
        assert_eq!(parsed.instant.unix_seconds(), seconds);
    }
    for (input, millis) in [
        ("1712345678000", 1_712_345_678_000_i64),
        ("-62135596800000", -62_135_596_800_000),
        ("32503680000000", 32_503_680_000_000),
    ] {
        let parsed = date_time::parse(input, &utc).expect(input);
        assert_eq!(parsed.detected, DetectedFormat::UnixMilliseconds, "{input}");
        assert_eq!(parsed.instant, Instant::from_unix_milliseconds(millis));
    }
    assert_eq!(
        date_time::parse("32503680001", &utc).map(|p| p.detected),
        Some(DetectedFormat::UnixMilliseconds)
    );
    for invalid in ["32503680000001", "-62135596800001", "999999999999999999"] {
        assert_eq!(date_time::parse(invalid, &utc), None, "{invalid}");
    }
}

#[test]
fn a_written_date_reads_as_the_csharp_vectors() {
    for (input, seconds) in [
        ("2024-12-25T10:30:45Z", 1_735_122_645),
        ("2024-12-25T10:30:45+02:00", 1_735_115_445),
    ] {
        let parsed = date_time::parse(input, &utc).expect(input);
        assert_eq!(parsed.detected, DetectedFormat::Iso8601);
        assert_eq!(parsed.instant.unix_seconds(), seconds, "{input}");
    }
    for invalid in [
        "not-a-date",
        "2024-99-99T00:00:00Z",
        "2024-13-40T99:99:99Z",
        "2023-02-29",
    ] {
        assert_eq!(date_time::parse(invalid, &utc), None, "{invalid}");
    }
}

#[test]
fn the_common_written_forms_read_and_a_date_without_offset_is_local() {
    let expected = 1_735_122_645;
    for input in [
        "2024-12-25 10:30:45Z",
        "2024-12-25T10:30:45.0000000Z",
        "2024/12/25 10:30:45 GMT",
        "12/25/2024 10:30:45 AM +00:00",
        "Wed, 25 Dec 2024 10:30:45 GMT",
        "December 25, 2024 10:30:45 UTC",
        "25 Dec 2024 12:30:45 +0200",
    ] {
        let parsed = date_time::parse(input, &utc).expect(input);
        assert_eq!(parsed.instant.unix_seconds(), expected, "{input}");
    }
    let parsed = date_time::parse("2024-12-25T10:30:45.1234567Z", &utc).expect("fraction");
    assert_eq!(
        parsed.instant.unix_ticks(),
        expected * TICKS_PER_SECOND + 1_234_567
    );
    // Without an offset, the computer's wall clock: here two hours east.
    let plus_two = |_wall: i64| 7_200;
    let parsed = date_time::parse("2024-12-25 12:30:45", &plus_two).expect("local");
    assert_eq!(parsed.instant.unix_seconds(), expected);
    assert_eq!(parsed.offset_seconds, 7_200);
    let parsed = date_time::parse("2024-12-25 12:30 PM", &utc).expect("twelve-hour");
    assert_eq!(parsed.instant.wall_clock(0).hour, 12);
    assert_eq!(date_time::parse("2024-12-25 13:30 PM", &utc), None);
}

#[test]
fn a_moment_is_written_in_the_round_trip_form() {
    let instant = Instant::from_unix_seconds(1_712_345_678);
    assert_eq!(
        date_time::round_trip_utc(instant),
        "2024-04-05T19:34:38.0000000Z"
    );
    assert_eq!(
        date_time::round_trip_with_offset(instant, 7_200),
        "2024-04-05T21:34:38.0000000+02:00"
    );
    assert_eq!(
        date_time::round_trip_with_offset(instant, -(3 * 3_600 + 1_800)),
        "2024-04-05T16:04:38.0000000-03:30"
    );
    assert_eq!(
        date_time::round_trip_utc(Instant::from_unix_milliseconds(-1)),
        "1969-12-31T23:59:59.9990000Z"
    );
    let first = date_time::parse("-62135596800", &utc).expect("year 1");
    assert_eq!(
        date_time::round_trip_utc(first.instant),
        "0001-01-01T00:00:00.0000000Z"
    );
    let clock = Instant::from_unix_seconds(1_735_122_645).wall_clock(7_200);
    assert_eq!((clock.year, clock.month, clock.day), (2024, 12, 25));
    assert_eq!(clock.time_text(), "12:30:45");
    assert_eq!(clock.weekday, 3, "a Wednesday");
}

#[test]
fn the_relative_time_takes_the_csharp_buckets() {
    // 2026-04-19T12:00:00Z, the C# `ReferenceNow`.
    let now = Instant::from_unix_seconds(1_776_600_000);
    let shifted = |seconds: i64| Instant::from_unix_seconds(now.unix_seconds() + seconds);
    for (offset, unit, value, past) in [
        (-30, RelativeUnit::Seconds, 30, true),
        (30, RelativeUnit::Seconds, 30, false),
        (-120, RelativeUnit::Minutes, 2, true),
        (120, RelativeUnit::Minutes, 2, false),
        (-10_800, RelativeUnit::Hours, 3, true),
        (10_800, RelativeUnit::Hours, 3, false),
        (-432_000, RelativeUnit::Days, 5, true),
        (432_000, RelativeUnit::Days, 5, false),
        (-90 * 86_400, RelativeUnit::Months, 3, true),
        (730 * 86_400, RelativeUnit::Years, 2, false),
        (0, RelativeUnit::Seconds, 0, true),
    ] {
        let relative = date_time::relative(shifted(offset), now);
        assert_eq!(
            (relative.unit, relative.value, relative.is_past),
            (unit, value, past),
            "{offset}"
        );
    }
}

// Time zones: the rules the converter's list converts with.

#[test]
fn a_posix_rule_reads_and_its_daylight_time_applies() {
    let paris = ZoneRules::from_posix("CET-1CEST,M3.5.0,M10.5.0/3").expect("Paris");
    assert_eq!(paris.standard_seconds, 3_600);
    let at = |text: &str| date_time::parse(text, &utc).expect(text).instant;
    assert_eq!(paris.offset_at(at("2024-01-15T12:00:00Z")), 3_600);
    assert_eq!(paris.offset_at(at("2024-07-15T12:00:00Z")), 7_200);
    // The last Sunday of March 2024 is the 31st, at 02:00 standard, 01:00 UTC.
    assert_eq!(paris.offset_at(at("2024-03-31T00:59:59Z")), 3_600);
    assert_eq!(paris.offset_at(at("2024-03-31T01:00:00Z")), 7_200);
    // The last Sunday of October 2024 is the 27th, at 03:00 daylight, 01:00 UTC.
    assert_eq!(paris.offset_at(at("2024-10-27T00:59:59Z")), 7_200);
    assert_eq!(paris.offset_at(at("2024-10-27T01:00:00Z")), 3_600);
    let sydney = ZoneRules::from_posix("AEST-10AEDT,M10.1.0,M4.1.0/3").expect("Sydney");
    assert_eq!(sydney.offset_at(at("2024-01-15T00:00:00Z")), 11 * 3_600);
    assert_eq!(sydney.offset_at(at("2024-07-15T00:00:00Z")), 10 * 3_600);
    let new_york = ZoneRules::from_posix("EST5EDT,M3.2.0,M11.1.0").expect("New York");
    assert_eq!(new_york.offset_at(at("2024-07-04T12:00:00Z")), -4 * 3_600);
    assert_eq!(
        ZoneRules::from_posix("<+0530>-5:30"),
        Some(ZoneRules::fixed(5 * 3_600 + 1_800))
    );
    assert_eq!(ZoneRules::from_posix("UTC0"), Some(ZoneRules::fixed(0)));
    assert_eq!(ZoneRules::from_posix("CET-1CEST,J60,J300"), None);
    let mut tzif = b"TZif2".to_vec();
    tzif.extend_from_slice(&[0; 40]);
    tzif.extend_from_slice(b"\nCET-1CEST,M3.5.0,M10.5.0/3\n");
    assert_eq!(ZoneRules::from_tzif(&tzif), Some(paris));
    assert_eq!(ZoneRules::from_tzif(b"TZif\0 old"), None);
}

#[test]
fn a_windows_tzi_reads_as_the_registry_holds_it() {
    // Romance Standard Time: bias -60, daylight bias -60, back on the last Sunday of
    // October at 03:00, forward on the last Sunday of March at 02:00.
    let mut tzi = Vec::new();
    for bias in [-60_i32, 0, -60] {
        tzi.extend_from_slice(&bias.to_le_bytes());
    }
    for fields in [[0_u16, 10, 0, 5, 3, 0, 0, 0], [0, 3, 0, 5, 2, 0, 0, 0]] {
        for field in fields {
            tzi.extend_from_slice(&field.to_le_bytes());
        }
    }
    let rules = ZoneRules::from_windows_tzi(&tzi).expect("TZI");
    assert_eq!(
        Some(rules),
        ZoneRules::from_posix("CET-1CEST,M3.5.0/2,M10.5.0/3")
    );
    let mut fixed = tzi.clone();
    fixed[14] = 0;
    fixed[30] = 0;
    assert_eq!(
        ZoneRules::from_windows_tzi(&fixed),
        Some(ZoneRules::fixed(3_600))
    );
    assert_eq!(ZoneRules::from_windows_tzi(&tzi[..10]), None);
}

#[test]
fn zones_are_named_and_sorted_as_dotnet_lists_them() {
    assert_eq!(
        time_zone_rules::display_name("Europe/Paris", 3_600),
        "(UTC+01:00) Europe/Paris"
    );
    assert_eq!(
        time_zone_rules::display_name("America/St_Johns", -(3 * 3_600 + 1_800)),
        "(UTC-03:30) America/St_Johns"
    );
    assert_eq!(time_zone_rules::display_name("UTC", 0), "(UTC) UTC");
    let entry = |id: &str, seconds: i32| TimeZoneEntry {
        id: id.to_owned(),
        display_name: time_zone_rules::display_name(id, seconds),
        rules: ZoneRules::fixed(seconds),
    };
    let mut zones = vec![
        entry("Europe/Paris", 3_600),
        entry("UTC", 0),
        entry("America/New_York", -18_000),
        entry("Africa/Lagos", 3_600),
    ];
    time_zone_rules::sort(&mut zones);
    let ids: Vec<&str> = zones.iter().map(|zone| zone.id.as_str()).collect();
    assert_eq!(
        ids,
        ["America/New_York", "UTC", "Africa/Lagos", "Europe/Paris"]
    );
}

// IP addresses: `IpCodecTests.cs` and .NET's `IPAddress.TryParse`.

#[test]
fn an_address_reads_as_dotnet_reads_it() {
    let v4 = |text: &str| match ip_address::parse(text) {
        Some(IpAddr::V4(address)) => Some(address),
        _ => None,
    };
    assert_eq!(v4("192.168.1.1"), Some(Ipv4Addr::new(192, 168, 1, 1)));
    assert_eq!(v4("3232235777"), Some(Ipv4Addr::new(192, 168, 1, 1)));
    assert_eq!(v4("0xC0A80101"), Some(Ipv4Addr::new(192, 168, 1, 1)));
    assert_eq!(v4("010.0.0.1"), Some(Ipv4Addr::new(8, 0, 0, 1)));
    assert_eq!(v4("192.168.257"), Some(Ipv4Addr::new(192, 168, 1, 1)));
    assert_eq!(v4("10.1"), Some(Ipv4Addr::new(10, 0, 0, 1)));
    assert_eq!(v4("0"), Some(Ipv4Addr::UNSPECIFIED));
    for invalid in [
        "999.999.999.999",
        "1.2.3.4.5",
        "1..2.3",
        "1.2.3.",
        "08.1.1.1",
        "0x",
        "4294967296",
        "-1",
        "1.2.3.4 ",
        "",
    ] {
        assert_eq!(ip_address::parse(invalid), None, "{invalid:?}");
    }
    assert!(matches!(
        ip_address::parse("2001:db8::1"),
        Some(IpAddr::V6(_))
    ));
    assert!(matches!(ip_address::parse("[::1]"), Some(IpAddr::V6(_))));
    assert!(matches!(
        ip_address::parse("fe80::1%12"),
        Some(IpAddr::V6(_))
    ));
}

#[test]
fn an_ip_converts_to_every_form_as_the_csharp_vectors() {
    for invalid in ["", "   "] {
        assert_eq!(ip_codec::convert(invalid), None);
    }
    for valid in [
        "192.168.1.1",
        " 192.168.1.1 ",
        "3232235777",
        "0xC0A80101",
        "0Xc0a80101",
        "11000000.10101000.00000001.00000001",
    ] {
        let result = ip_codec::convert(valid).expect(valid);
        assert_eq!(result.dotted, "192.168.1.1", "{valid}");
        assert_eq!(result.decimal, "3232235777");
        assert_eq!(result.hex, "0xC0A80101");
        assert_eq!(result.binary, "11000000.10101000.00000001.00000001");
        assert_eq!(result.mapped_ipv6, "::ffff:c0a8:0101");
    }
    let quirk = ip_codec::convert("1.1.1.1").expect("binary branch");
    assert_eq!(
        (quirk.decimal.as_str(), quirk.hex.as_str()),
        ("16843009", "0x01010101")
    );
    assert_eq!(quirk.mapped_ipv6, "::ffff:0101:0101");
    let quirk = ip_codec::convert("10.0.0.1").expect("binary branch");
    assert_eq!(quirk.dotted, "2.0.0.1");
    assert_eq!(quirk.decimal, "33554433");
    assert_eq!(quirk.binary, "00000010.00000000.00000000.00000001");
    for (input, decimal, hex, binary, mapped) in [
        (
            "0.0.0.0",
            "0",
            "0x00000000",
            "00000000.00000000.00000000.00000000",
            "::ffff:0000:0000",
        ),
        (
            "255.255.255.255",
            "4294967295",
            "0xFFFFFFFF",
            "11111111.11111111.11111111.11111111",
            "::ffff:ffff:ffff",
        ),
        (
            "127.0.0.1",
            "2130706433",
            "0x7F000001",
            "01111111.00000000.00000000.00000001",
            "::ffff:7f00:0001",
        ),
    ] {
        let result = ip_codec::convert(input).expect(input);
        assert_eq!(result.dotted, input);
        assert_eq!(
            (
                result.decimal.as_str(),
                result.hex.as_str(),
                result.binary.as_str(),
                result.mapped_ipv6.as_str()
            ),
            (decimal, hex, binary, mapped)
        );
    }
    for invalid in [
        "2001:db8::1",
        "-1",
        "4294967296",
        "0x100000000",
        "0xZZZZZZZZ",
        "11000000.10101000.00000001",
        "11000000.10101000.00000001.000000012",
        "999.999.999.999",
        "abc",
    ] {
        assert_eq!(ip_codec::convert(invalid), None, "{invalid}");
    }
    let first = ip_codec::convert("192.168.1.1").expect("dotted");
    for form in [&first.decimal, &first.hex, &first.binary] {
        assert_eq!(ip_codec::convert(form).as_ref(), Some(&first));
    }
}

// Subnet calculator: `SubnetCalculatorView.xaml.cs`.

#[test]
fn an_ipv4_network_is_broken_down() {
    let subnet = subnet_calculator::calculate("192.168.1.77/24").expect("network");
    assert_eq!(subnet.network, "192.168.1.0");
    assert_eq!(subnet.broadcast.as_deref(), Some("192.168.1.255"));
    assert_eq!(subnet.mask.as_deref(), Some("255.255.255.0"));
    assert_eq!(subnet.first_host, "192.168.1.1");
    assert_eq!(subnet.last_host, "192.168.1.254");
    assert_eq!(subnet.total_hosts, HostCount::Count(254));
    assert_eq!(subnet.cidr, "192.168.1.0/24");
    assert_eq!(subnet.wildcard.as_deref(), Some("0.0.0.255"));
    let single = subnet_calculator::calculate("10.0.0.5").expect("host");
    assert_eq!(single.cidr, "10.0.0.5/32");
    assert_eq!(single.first_host, "10.0.0.5");
    assert_eq!(single.total_hosts, HostCount::Count(1));
    let pair = subnet_calculator::calculate("10.0.0.5/31").expect("pair");
    assert_eq!(
        (pair.first_host.as_str(), pair.last_host.as_str()),
        ("10.0.0.4", "10.0.0.5")
    );
    assert_eq!(pair.total_hosts, HostCount::Count(2));
    let all = subnet_calculator::calculate("10.0.0.0/8").expect("class A");
    assert_eq!(all.total_hosts, HostCount::Count(16_777_214));
    let everything = subnet_calculator::calculate("1.2.3.4/0").expect("all");
    assert_eq!(everything.total_hosts, HostCount::Count(4_294_967_294));
    assert_eq!(everything.mask.as_deref(), Some("0.0.0.0"));
    for invalid in [
        "",
        "10.0.0.0/33",
        "10.0.0.0/-1",
        "10.0.0.0/x",
        "a/24",
        "1/2/3",
    ] {
        assert_eq!(subnet_calculator::calculate(invalid), None, "{invalid:?}");
    }
}

#[test]
fn an_ipv6_network_is_broken_down() {
    let subnet = subnet_calculator::calculate("2001:db8::1/64").expect("network");
    assert_eq!(subnet.network, "2001:db8::");
    assert_eq!(subnet.first_host, "2001:db8::1");
    assert_eq!(subnet.last_host, "2001:db8::ffff:ffff:ffff:fffe");
    assert_eq!(
        subnet.total_hosts,
        HostCount::Count(18_446_744_073_709_551_614)
    );
    assert_eq!(subnet.cidr, "2001:db8::/64");
    assert_eq!(subnet.broadcast, None);
    let wide = subnet_calculator::calculate("2001:db8::/32").expect("prefix");
    assert_eq!(wide.total_hosts, HostCount::TooMany);
    let host = subnet_calculator::calculate("::1").expect("host");
    assert_eq!(host.cidr, "::1/128");
    assert_eq!(host.total_hosts, HostCount::Count(1));
}

#[test]
fn numbers_are_grouped_and_rounded_as_dotnet_writes_them() {
    assert_eq!(number_text::group_digits(0, ","), "0");
    assert_eq!(number_text::group_digits(254, ","), "254");
    assert_eq!(number_text::group_digits(16_777_214, ","), "16,777,214");
    assert_eq!(number_text::group_digits(1_000, " "), "1 000");
    assert_eq!(number_text::one_decimal(78.125, ","), "78,1");
    assert_eq!(number_text::one_decimal(100.0, "."), "100.0");
    assert_eq!(number_text::parse_int32(" +42 "), Some(42));
    assert_eq!(number_text::parse_int32("-7"), Some(-7));
    assert_eq!(number_text::parse_int32("2147483648"), None);
    assert_eq!(number_text::parse_int32("4 2"), None);
}

// Network calculator: `NetworkCalculatorView.xaml.cs`.

#[test]
fn the_supernet_covers_every_network() {
    let supernet =
        network_calculator::supernet("192.168.0.0/24\r\n192.168.1.0/24\n\n  192.168.3.0/24  ")
            .expect("supernet");
    assert_eq!(
        network_calculator::address_text(supernet.network),
        "192.168.0.0"
    );
    assert_eq!(supernet.prefix, 22);
    assert_eq!(
        network_calculator::address_text(supernet.broadcast),
        "192.168.3.255"
    );
    assert_eq!(supernet.hosts, 1_022);
    let single = network_calculator::supernet("10.0.0.1/32").expect("one");
    assert_eq!((single.prefix, single.hosts), (32, 0));
    assert_eq!(
        network_calculator::supernet(" \n "),
        Err(NetCalcError::NoCidrs)
    );
    assert_eq!(
        network_calculator::supernet("10.0.0.0/8\nnonsense"),
        Err(NetCalcError::InvalidCidr("nonsense".to_owned()))
    );
    assert_eq!(
        network_calculator::supernet("10.0.0.0"),
        Err(NetCalcError::InvalidCidr("10.0.0.0".to_owned()))
    );
}

#[test]
fn a_range_is_covered_by_the_fewest_aligned_blocks() {
    assert_eq!(
        network_calculator::range_to_cidrs("192.168.1.0", "192.168.1.255"),
        Ok(vec!["192.168.1.0/24".to_owned()])
    );
    assert_eq!(
        network_calculator::range_to_cidrs("10.0.0.1", "10.0.0.6"),
        Ok(vec![
            "10.0.0.1/32".to_owned(),
            "10.0.0.2/31".to_owned(),
            "10.0.0.4/31".to_owned(),
            "10.0.0.6/32".to_owned(),
        ])
    );
    assert_eq!(
        network_calculator::range_to_cidrs("0.0.0.0", "255.255.255.255"),
        Ok(vec!["0.0.0.0/0".to_owned()])
    );
    assert_eq!(
        network_calculator::range_to_cidrs("255.255.255.255", "255.255.255.255"),
        Ok(vec!["255.255.255.255/32".to_owned()])
    );
    assert_eq!(
        network_calculator::range_to_cidrs("10.0.0.9", "10.0.0.1"),
        Err(NetCalcError::StartAfterEnd)
    );
    assert_eq!(
        network_calculator::range_to_cidrs("10.0.0.1", "nope"),
        Err(NetCalcError::InvalidIpRange)
    );
}

#[test]
fn a_vlan_gets_the_smallest_network_for_its_hosts() {
    let plan = network_calculator::vlan("50", "10.0.0.77").expect("plan");
    assert_eq!(network_calculator::address_text(plan.network), "10.0.0.64");
    assert_eq!(plan.prefix, 26);
    assert_eq!(
        network_calculator::address_text(plan.mask),
        "255.255.255.192"
    );
    assert_eq!(
        network_calculator::address_text(plan.broadcast),
        "10.0.0.127"
    );
    assert_eq!(
        network_calculator::address_text(plan.first_usable),
        "10.0.0.65"
    );
    assert_eq!(
        network_calculator::address_text(plan.last_usable),
        "10.0.0.126"
    );
    assert_eq!((plan.usable_hosts, plan.requested), (62, 50));
    assert_eq!(number_text::one_decimal(plan.utilization, "."), "80.6");
    let tight = network_calculator::vlan("2", "10.0.0.0").expect("plan");
    assert_eq!((tight.prefix, tight.usable_hosts), (30, 2));
    // The C#'s `int` wraps past its largest value: two hosts short of it need no bits.
    let wrapped = network_calculator::vlan("2147483647", "10.0.0.0").expect("plan");
    assert_eq!((wrapped.prefix, wrapped.usable_hosts), (32, 0));
    let huge = network_calculator::vlan("2147483645", "10.0.0.0").expect("plan");
    assert_eq!(huge.prefix, 0);
    for hosts in ["0", "-3", "x", ""] {
        assert_eq!(
            network_calculator::vlan(hosts, "10.0.0.0"),
            Err(NetCalcError::InvalidHostCount),
            "{hosts}"
        );
    }
    assert_eq!(
        network_calculator::vlan("5", "ten"),
        Err(NetCalcError::InvalidBaseNetwork)
    );
}

// Crontab builder: `CrontabBuilderView.xaml.cs`.

fn fields(text: &str) -> [String; cron_builder::FIELD_COUNT] {
    cron_builder::parse(text).expect(text)
}

#[test]
fn a_cron_expression_is_checked_as_the_csharp_checks_it() {
    assert_eq!(cron_builder::validate("* * * * *"), None);
    assert_eq!(cron_builder::validate("*/15  0-6 1,15 * 1-5"), None);
    assert_eq!(
        cron_builder::validate("* * * *"),
        Some(CronProblem::FieldCount)
    );
    assert_eq!(
        cron_builder::validate("* * a * *"),
        Some(CronProblem::InvalidField {
            field: 2,
            text: "a".to_owned()
        })
    );
    assert_eq!(
        cron_builder::validate("60 * * * *"),
        Some(CronProblem::OutOfRange { field: 0 })
    );
    assert_eq!(
        cron_builder::validate("* * * * 7"),
        Some(CronProblem::OutOfRange { field: 4 })
    );
    assert_eq!(
        cron_builder::validate("*/0 * * * *"),
        Some(CronProblem::OutOfRange { field: 0 })
    );
    // As the C#: a step's start that is not a number is not checked.
    assert_eq!(cron_builder::validate("1-5/2 * * * *"), None);
    assert_eq!(cron_builder::parse("* * * *"), None);
    assert_eq!(cron_builder::parse("* * * * x"), None);
}

#[test]
fn a_cron_expression_is_described_as_the_csharp_describes_it() {
    for (text, description) in [
        ("* * * * *", Description::EveryMinute),
        ("0 * * * *", Description::EveryHour),
        ("0 0 * * *", Description::EveryDay),
        ("*/5 * * * *", Description::EveryNMinutes("5".to_owned())),
        ("30 9 * * *", Description::DailyAt("09:30".to_owned())),
        (
            "0 0 * * 0",
            Description::WeeklyAt(WeekDay::Day(0), "00:00".to_owned()),
        ),
        (
            "0 9 * * 1-5",
            Description::WeeklyAt(WeekDay::Field("1-5".to_owned()), "09:00".to_owned()),
        ),
        ("0 0 1 * *", Description::MonthlyAt(1, "00:00".to_owned())),
        ("0 0 1 6 *", Description::Custom("0 0 1 6 *".to_owned())),
    ] {
        assert_eq!(cron_builder::describe(&fields(text)), description, "{text}");
    }
}

#[test]
fn the_next_runs_follow_the_minute_after_now() {
    // Friday 2026-10-09 14:05 on the wall clock.
    let now = Minute::of(&Instant::from_unix_seconds(1_791_554_700).wall_clock(0));
    assert_eq!(
        cron_builder::next_runs(&fields("* * * * *"), now, 2),
        ["2026-10-09 14:06 (Fri)", "2026-10-09 14:07 (Fri)"]
    );
    assert_eq!(
        cron_builder::next_runs(&fields("0 9 * * 1-5"), now, 3),
        [
            "2026-10-12 09:00 (Mon)",
            "2026-10-13 09:00 (Tue)",
            "2026-10-14 09:00 (Wed)"
        ]
    );
    assert_eq!(
        cron_builder::next_runs(&fields("0 0 1 * *"), now, 1),
        ["2026-11-01 00:00 (Sun)"]
    );
    // A date that never comes within a year lists nothing.
    assert!(cron_builder::next_runs(&fields("0 0 31 2 *"), now, 5).is_empty());
}

// SSH config generator: `SshConfigGeneratorView.xaml.cs`.

#[test]
fn an_ssh_config_block_holds_only_what_is_not_a_default() {
    let mut host = HostBlock {
        alias: "web".to_owned(),
        host_name: "web.example.com".to_owned(),
        port: ssh_config::DEFAULT_PORT,
        ..HostBlock::default()
    };
    assert_eq!(
        ssh_config::generate(&host, "\n"),
        "Host web\n    HostName web.example.com"
    );
    host.user = "admin".to_owned();
    host.port = 2222;
    host.identity_file = "~/.ssh/id_ed25519".to_owned();
    host.proxy_jump = "bastion".to_owned();
    host.forward_agent = true;
    host.alive_interval = 60;
    assert_eq!(
        ssh_config::generate(&host, "\r\n"),
        "Host web\r\n    HostName web.example.com\r\n    User admin\r\n    Port 2222\r\n    \
         IdentityFile ~/.ssh/id_ed25519\r\n    ProxyJump bastion\r\n    ForwardAgent yes\r\n    \
         ServerAliveInterval 60"
    );
    assert_eq!(ssh_config::parse_int(" 2222 ", 22), 2222);
    assert_eq!(ssh_config::parse_int("abc", 22), 22);
    assert_eq!(ssh_config::parse_int("", 0), 0);
}
