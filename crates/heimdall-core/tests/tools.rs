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

//! The tools' engines against the C# tests' vectors: `Base64CodecTests.cs`,
//! `UrlCodecTests.cs`, `UuidGeneratorTests.cs` and the tool services' tests.

use std::collections::HashSet;
use std::time::{SystemTime, UNIX_EPOCH};

use heimdall_core::settings::{Settings, ToolsSettings};
use heimdall_core::tools::base64_codec::{self, InvalidBase64, LINE_BREAK, LINE_LENGTH};
use heimdall_core::tools::url_codec;
use heimdall_core::tools::uuid_generator::{self, Uuid, UuidFormat, UuidVersion};

/// How far a version 7 timestamp may be from the clock read around it, as the C# test.
const CLOCK_SLACK_MS: u64 = 2000;

fn now_millis() -> u64 {
    u64::try_from(
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("after 1970")
            .as_millis(),
    )
    .expect("fits")
}

#[test]
fn base64_encodes_the_rfc_4648_vectors() {
    for (input, expected) in [
        ("", ""),
        ("f", "Zg=="),
        ("fo", "Zm8="),
        ("foo", "Zm9v"),
        ("foob", "Zm9vYg=="),
        ("fooba", "Zm9vYmE="),
        ("foobar", "Zm9vYmFy"),
    ] {
        assert_eq!(base64_codec::encode(input.as_bytes(), false), expected);
    }
    assert_eq!(base64_codec::encode(b"abc", false), "YWJj");
}

#[test]
fn url_safe_base64_rewrites_its_alphabet_and_trims_the_padding() {
    assert_eq!(base64_codec::encode(&[251, 255, 255], true), "-___");
    assert_eq!(base64_codec::decode("-___", true), Ok(vec![251, 255, 255]));
    assert_eq!(base64_codec::decode("-w", true), Ok(vec![251]));
    assert_eq!(base64_codec::decode("_w", true), Ok(vec![255]));
    assert_eq!(base64_codec::decode("-_8", true), Ok(vec![251, 255]));
}

#[test]
fn base64_breaks_its_lines_as_dotnet_and_reads_them_back() {
    let payload = vec![b'a'; 60];
    let encoded = base64_codec::encode(&payload, false);
    assert!(encoded.contains(LINE_BREAK), "{encoded}");
    let first = encoded.split(LINE_BREAK).next().expect("a line");
    assert_eq!(first.len(), LINE_LENGTH);
    assert!(
        !encoded.ends_with(LINE_BREAK),
        "no break after the last line"
    );
    assert_eq!(base64_codec::decode(&encoded, false), Ok(payload));
}

#[test]
fn base64_decodes_the_foobar_vector_and_refuses_what_is_not_base64() {
    assert_eq!(
        base64_codec::decode("Zm9vYmFy", false),
        Ok(b"foobar".to_vec())
    );
    assert_eq!(base64_codec::decode("YWJj", false), Ok(b"abc".to_vec()));
    assert_eq!(
        base64_codec::decode("not-base64!", false),
        Err(InvalidBase64)
    );
    assert_eq!(base64_codec::decode("Zm9", false), Err(InvalidBase64));
}

#[test]
fn base64_round_trips_every_byte_url_safe_whatever_the_number_of_lines() {
    // 256 bytes make five lines, an even number of breaks; 300 make six, an odd number,
    // which the C# refuses to read back URL-safe.
    for length in [256_usize, 300] {
        let payload: Vec<u8> = (0..length)
            .map(|i| u8::try_from(i % 256).expect("byte"))
            .collect();
        let encoded = base64_codec::encode(&payload, true);
        assert_eq!(
            base64_codec::decode(&encoded, true),
            Ok(payload),
            "{length}"
        );
    }
}

#[test]
fn component_encoding_escapes_every_reserved_character() {
    assert_eq!(url_codec::encode("", true), "");
    assert_eq!(url_codec::encode("a/b?c=d&e", true), "a%2Fb%3Fc%3Dd%26e");
    assert_eq!(url_codec::encode("a/b?c=d", true), "a%2Fb%3Fc%3Dd");
    assert_eq!(url_codec::encode("café", true), "caf%C3%A9");
    assert_eq!(
        url_codec::encode("already%20encoded", true),
        "already%2520encoded"
    );
}

#[test]
fn structure_encoding_keeps_the_structural_characters() {
    assert_eq!(url_codec::encode(":/?#&=@%", false), ":/?#&=@%");
    assert_eq!(
        url_codec::encode("https://example.com/a b?x=1&y=2#frag ment", false),
        "https://example.com/a%20b?x=1&y=2#frag%20ment"
    );
    assert_eq!(
        url_codec::encode("https://example.com/a b?x=1", false),
        "https://example.com/a%20b?x=1"
    );
    assert_eq!(
        url_codec::encode("https://example.com?q=100%25", false),
        "https://example.com?q=100%25"
    );
    assert_eq!(
        url_codec::encode("mailto:a+b@example.com?x=1", false),
        "mailto:a%2Bb@example.com?x=1"
    );
    assert_eq!(url_codec::encode("a b", false), "a%20b");
}

#[test]
fn decoding_reads_utf8_back_and_leaves_the_rest_as_typed() {
    assert_eq!(url_codec::decode(""), "");
    assert_eq!(url_codec::decode("caf%C3%A9"), "café");
    assert_eq!(url_codec::decode("caf%c3%a9"), "café");
    assert_eq!(url_codec::decode("a+b"), "a+b");
    assert_eq!(url_codec::decode("a%20b"), "a b");
    // Not UTF-8, or not an escape: kept as typed, as .NET's `UnescapeDataString`.
    assert_eq!(url_codec::decode("%ff%41"), "%ffA");
    assert_eq!(url_codec::decode("100%"), "100%");
    assert_eq!(url_codec::decode("%zz"), "%zz");
    assert_eq!(url_codec::decode("é%C3"), "é%C3");
}

#[test]
fn both_encodings_round_trip() {
    let input = "https://example.com/a b?x=1&y=2#frag ment";
    assert_eq!(url_codec::decode(&url_codec::encode(input, true)), input);
    assert_eq!(url_codec::decode(&url_codec::encode(input, false)), input);
}

#[test]
fn a_version_4_uuid_carries_its_version_and_is_unique() {
    let format = UuidFormat::default();
    let values: HashSet<Uuid> = (0..100)
        .map(|_| uuid_generator::generate(UuidVersion::V4).expect("random"))
        .collect();
    assert_eq!(values.len(), 100);
    for uuid in values {
        let text = uuid_generator::format(uuid, format);
        assert_eq!(text.chars().nth(14), Some('4'), "{text}");
        assert!(
            "89ab".contains(text.chars().nth(19).expect("variant")),
            "{text}"
        );
        assert_ne!(uuid, Uuid([0; 16]));
    }
}

#[test]
fn a_version_7_uuid_carries_the_time_its_version_and_its_variant() {
    let before = now_millis();
    let uuid = uuid_generator::generate(UuidVersion::V7).expect("random");
    let after = now_millis();
    let text = uuid_generator::format(uuid, UuidFormat::default());
    assert_eq!(text.chars().nth(14), Some('7'), "{text}");
    assert!(
        "89ab".contains(text.chars().nth(19).expect("variant")),
        "{text}"
    );
    let stamp = uuid_generator::timestamp_millis(uuid);
    assert!(
        (before - CLOCK_SLACK_MS..=after + CLOCK_SLACK_MS).contains(&stamp),
        "{stamp}"
    );
    std::thread::sleep(std::time::Duration::from_millis(2));
    let later = uuid_generator::generate(UuidVersion::V7).expect("random");
    assert!(uuid_generator::timestamp_millis(later) >= stamp);
    // The random bits vary.
    let tails: HashSet<Vec<u8>> = (0..20)
        .map(|_| {
            let mut bytes = uuid_generator::generate(UuidVersion::V7).expect("random").0;
            bytes[6] &= 0x0F;
            bytes[8] &= 0x3F;
            bytes[6..].to_vec()
        })
        .collect();
    assert!(tails.len() >= 15);
}

#[test]
fn a_uuid_is_written_in_the_four_csharp_formats() {
    let uuid = uuid_generator::parse("A1B2C3D4-E5F6-47A8-9123-ABCDEF123456").expect("parses");
    let written = |uppercase, with_hyphens| {
        uuid_generator::format(
            uuid,
            UuidFormat {
                uppercase,
                with_hyphens,
            },
        )
    };
    assert_eq!(written(false, true), "a1b2c3d4-e5f6-47a8-9123-abcdef123456");
    assert_eq!(written(true, true), "A1B2C3D4-E5F6-47A8-9123-ABCDEF123456");
    assert_eq!(written(false, false), "a1b2c3d4e5f647a89123abcdef123456");
    assert_eq!(written(true, false), "A1B2C3D4E5F647A89123ABCDEF123456");
    assert_eq!(
        uuid_generator::format(Uuid([0; 16]), UuidFormat::default()),
        "00000000-0000-0000-0000-000000000000"
    );
    let random = uuid_generator::generate(UuidVersion::V4).expect("random");
    assert_eq!(
        uuid_generator::parse(&uuid_generator::format(
            random,
            UuidFormat {
                uppercase: true,
                with_hyphens: false
            }
        )),
        Some(random)
    );
}

#[test]
fn the_tools_area_is_kept_across_runs_never_exported_and_kept_by_a_reset() {
    let dir = tempfile::tempdir().expect("dir");
    let path = dir.path().join("settings.toml");
    let tools = ToolsSettings {
        favorites: vec!["UUID".to_owned(), "BASE64".to_owned()],
        show_tools_panel: true,
        collapsed_categories: vec!["encoding".to_owned()],
    };
    let mut settings = Settings {
        tools: tools.clone(),
        ..Settings::default()
    };
    settings.save(&path).expect("saved");
    assert_eq!(Settings::load(&path).expect("read").tools, tools);
    let (exported, _) = settings.export(None, true);
    assert!(!exported.contains("BASE64"), "{exported}");
    assert!(!exported.contains("show_tools_panel"), "{exported}");
    settings.reset_all();
    assert_eq!(settings.tools, tools);
}
