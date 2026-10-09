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

//! The Hash, HMAC, TOTP and JWT engines against the published vectors (FIPS 180 and 202,
//! RFC 2202, RFC 4231, RFC 4648, RFC 6238, RFC 7519) and the C# tests' own:
//! `HashComputerTests.cs`, `HashVerifierTests.cs`, `HmacComputerTests.cs`,
//! `HmacVerifierTests.cs`, `TotpGeneratorTests.cs`, `Base32CodecTests.cs`,
//! `JwtParserTests.cs`, `JwtClaimsEvaluatorTests.cs` and `JwtHmacVerifierTests.cs`.

use data_encoding::BASE64URL_NOPAD;
use heimdall_core::tools::hash_computer::{
    self, HashAlgorithm, HashFileError, HashMatch, MAX_FILE_BYTES, STREAM_CHUNK_BYTES,
};
use heimdall_core::tools::hmac_computer::{self, HMAC_ALGORITHMS, HmacOutputFormat};
use heimdall_core::tools::jwt_parser::{
    self, Expiration, HmacVerification, JwtAlgorithm, JwtDecodeError,
};
use heimdall_core::tools::totp_generator::{
    self, DEFAULT_ALGORITHM, DEFAULT_DIGITS, DEFAULT_TIME_STEP_SECONDS, InvalidBase32, TotpError,
};

const HEX_ABC: [(HashAlgorithm, &str); 6] = [
    (HashAlgorithm::Md5, "900150983cd24fb0d6963f7d28e17f72"),
    (
        HashAlgorithm::Sha1,
        "a9993e364706816aba3e25717850c26c9cd0d89d",
    ),
    (
        HashAlgorithm::Sha256,
        "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad",
    ),
    (
        HashAlgorithm::Sha384,
        "cb00753f45a35e8bb5a03d699ac65007272c32ab0eded1631a8b605a43ff5bed8086072ba1e7cc2358baeca134c825a7",
    ),
    (
        HashAlgorithm::Sha512,
        "ddaf35a193617abacc417349ae20413112e6fa4e89a97ea20a9eeee64b55d39a2192992a274fc1a836ba3c23a3feebbd454d4423643ce80e2a9ac94fa54ca49f",
    ),
    (
        HashAlgorithm::Sha3_256,
        "3a985da74fe225b2045c172d6bd390bd855f086e3e9d525b46bfe24511431532",
    ),
];

#[test]
fn every_digest_of_abc_is_the_published_one_in_lower_case_hex() {
    for (kind, expected) in HEX_ABC {
        let digest = hash_computer::compute(kind, b"abc");
        assert_eq!(digest, expected, "{kind:?}");
        assert_eq!(digest.len(), kind.hex_length());
    }
    assert_eq!(
        hash_computer::compute(HashAlgorithm::Md5, &[]),
        "d41d8cd98f00b204e9800998ecf8427e"
    );
    let all = hash_computer::compute_all(b"abc");
    assert_eq!(
        all.iter().map(|(kind, _)| *kind).collect::<Vec<_>>(),
        HashAlgorithm::ALL,
        "the C# order"
    );
    assert_eq!(
        HashAlgorithm::ALL.map(HashAlgorithm::display_name),
        ["MD5", "SHA1", "SHA256", "SHA384", "SHA512", "SHA3-256"]
    );
}

#[test]
fn a_stream_is_read_once_by_chunks_for_every_digest() {
    let data: Vec<u8> = (0..STREAM_CHUNK_BYTES * 2 + 17)
        .map(|index| index.to_le_bytes()[0])
        .collect();
    let mut reports = Vec::new();
    let digests =
        hash_computer::compute_stream(&data[..], |read| reports.push(read)).expect("read");
    assert_eq!(digests, hash_computer::compute_all(&data));
    assert_eq!(reports.last().copied(), Some(data.len() as u64));
    assert!(reports.len() >= 3, "{reports:?}");
    let mut empty = Vec::new();
    let _ = hash_computer::compute_stream(&[][..], |read| empty.push(read)).expect("read");
    assert_eq!(empty, [0], "an empty stream reports none read once");
}

#[test]
fn a_file_is_hashed_up_to_the_csharp_limit_and_its_failures_told_apart() {
    let dir = tempfile::tempdir().expect("dir");
    let small = dir.path().join("abc.txt");
    std::fs::write(&small, b"abc").expect("written");
    let mut shares = Vec::new();
    let file = hash_computer::compute_file(&small, |share| shares.push(share)).expect("hashed");
    assert_eq!(file.size, 3);
    assert_eq!(file.digests, hash_computer::compute_all(b"abc"));
    assert!(
        shares
            .iter()
            .any(|share| (*share - 100.0).abs() < f64::EPSILON)
    );
    let large = dir.path().join("large.bin");
    std::fs::File::create(&large)
        .expect("created")
        .set_len(MAX_FILE_BYTES + 1)
        .expect("sized");
    assert_eq!(
        hash_computer::compute_file(&large, |_| {}),
        Err(HashFileError::TooLarge {
            limit: MAX_FILE_BYTES
        })
    );
    assert_eq!(
        hash_computer::compute_file(&dir.path().join("missing"), |_| {}),
        Err(HashFileError::NotFound)
    );
    assert_eq!(
        hash_computer::compute_file(dir.path(), |_| {}),
        Err(HashFileError::NotFound),
        "a folder is no file, as .NET's FileInfo.Exists"
    );
    let empty = dir.path().join("empty");
    std::fs::write(&empty, b"").expect("written");
    let mut shares = Vec::new();
    let _ = hash_computer::compute_file(&empty, |share| shares.push(share)).expect("hashed");
    assert_eq!(shares, [100.0], "an empty file is all read");
}

#[test]
fn a_hash_pasted_is_found_by_its_length_first_then_by_any_digest() {
    assert_eq!(
        [32, 40, 64, 96, 128].map(hash_computer::detect_by_length),
        [
            Some(HashAlgorithm::Md5),
            Some(HashAlgorithm::Sha1),
            Some(HashAlgorithm::Sha256),
            Some(HashAlgorithm::Sha384),
            Some(HashAlgorithm::Sha512),
        ]
    );
    assert_eq!(hash_computer::detect_by_length(12), None);
    let md5 = vec![(
        HashAlgorithm::Md5,
        "900150983cd24fb0d6963f7d28e17f72".to_owned(),
    )];
    assert_eq!(
        hash_computer::find_match(&md5, "900150983cd24fb0d6963f7d28e17f72"),
        Some(HashMatch {
            kind: HashAlgorithm::Md5,
            by_length: true
        })
    );
    let sha3 = vec![(HashAlgorithm::Sha3_256, HEX_ABC[5].1.to_owned())];
    assert_eq!(
        hash_computer::find_match(&sha3, HEX_ABC[5].1),
        Some(HashMatch {
            kind: HashAlgorithm::Sha3_256,
            by_length: false
        })
    );
    let sha256 = vec![(HashAlgorithm::Sha256, HEX_ABC[2].1.to_owned())];
    assert_eq!(hash_computer::find_match(&sha256, &"a".repeat(64)), None);
    assert_eq!(hash_computer::find_match(&[], ""), None);
    let sha1 = vec![(HashAlgorithm::Sha1, HEX_ABC[1].1.to_owned())];
    assert_eq!(
        hash_computer::find_match(&sha1, "  A9993E364706816ABA3E25717850C26C9CD0D89D  ")
            .map(|found| found.kind),
        Some(HashAlgorithm::Sha1)
    );
}

fn hmac_hex(kind: HashAlgorithm, key: &[u8], data: &[u8]) -> String {
    hmac_computer::format(
        &hmac_computer::compute(kind, key, data).expect("offered"),
        HmacOutputFormat::Hex,
    )
}

#[test]
fn hmacs_match_rfc_2202_and_rfc_4231() {
    // RFC 2202 and RFC 4231, test case 1; the C# tests' vectors.
    let key = [0x0b_u8; 20];
    assert_eq!(
        hmac_hex(HashAlgorithm::Md5, &[0x0b; 16], b"Hi There"),
        "9294727a3638bb1c13f48ef8158bfc9d"
    );
    assert_eq!(
        hmac_hex(HashAlgorithm::Sha1, &key, b"Hi There"),
        "b617318655057264e28bc0b6fb378c8ef146be00"
    );
    assert_eq!(
        hmac_hex(HashAlgorithm::Sha256, &key, b"Hi There"),
        "b0344c61d8db38535ca8afceaf0bf12b881dc200c9833da726e9376c2e32cff7"
    );
    assert_eq!(
        hmac_hex(HashAlgorithm::Sha384, &key, b"Hi There"),
        "afd03944d84895626b0825f4ab46907f15f9dadbe4101ec682aa034c7cebc59cfaea9ea9076ede7f4af152e8b2fa9cb6"
    );
    assert_eq!(
        hmac_hex(HashAlgorithm::Sha512, &key, b"Hi There"),
        "87aa7cdea5ef619d4ff0b4241a1d6cb02379f4e2ce4ec2787ad0b30545e17cdedaa833b7d6b8a702038b274eaea3f4e4be9d914eeb61f1702e696c203a126854"
    );
    // Test case 2: a key shorter than the block.
    let data = b"what do ya want for nothing?";
    assert_eq!(
        hmac_hex(HashAlgorithm::Md5, b"Jefe", data),
        "750c783e6ab0b503eaa86e310a5db738"
    );
    assert_eq!(
        hmac_hex(HashAlgorithm::Sha1, b"Jefe", data),
        "effcdf6ae5eb2fa2d27416d5f184df9c259a7c79"
    );
    assert_eq!(
        hmac_hex(HashAlgorithm::Sha256, b"Jefe", data),
        "5bdcc146bf60754e6a042426089575c75a003f089d2739839dec58b964ec3843"
    );
    assert_eq!(
        hmac_hex(HashAlgorithm::Sha384, b"Jefe", data),
        "af45d2e376484031617f78d2b58a6b1b9c7ef464f5a01b47e42ec3736322445e8e2240ca5e69e2c78b3239ecfab21649"
    );
    assert_eq!(
        hmac_hex(HashAlgorithm::Sha512, b"Jefe", data),
        "164b7a7bfcf819e2e395fbe73b56e0a387bd64222e831fd610270cd7ea2505549758bf75c05a994a6d034f65f8f0e6fdcaeab1a34d4a6b4b636e070a38bce737"
    );
    // RFC 4231 test case 6: a key longer than the block, hashed first.
    let long_key = [0xaa_u8; 131];
    let data = b"Test Using Larger Than Block-Size Key - Hash Key First";
    assert_eq!(
        hmac_hex(HashAlgorithm::Sha256, &long_key, data),
        "60e431591ee0b67f0d8a26aacbf5b77f8e0bc6213728c5140546040f0ee37f54"
    );
    assert_eq!(
        hmac_hex(HashAlgorithm::Sha384, &long_key, data),
        "4ece084485813e9088d2c63a041bc5b44f9ef1012a2b588f3cd11f05033ac4c60c2ef6ab4030fe8296248df163f44952"
    );
    assert_eq!(
        hmac_hex(HashAlgorithm::Sha512, &long_key, data),
        "80b24263c7c1a3ebb71493c1dd7be8b49b46d1f41b4aeec1121b013783f8f3526b56d037e05f2598bd0fd2215d6a1e5295e64f73f63f0aec8b915a985d786598"
    );
}

#[test]
fn hmacs_are_offered_in_the_csharp_order_with_its_names_and_lengths() {
    assert_eq!(
        HMAC_ALGORITHMS.map(|kind| hmac_computer::display_name(kind).expect("named")),
        [
            "HMAC-SHA256",
            "HMAC-SHA384",
            "HMAC-SHA512",
            "HMAC-SHA1",
            "HMAC-MD5"
        ]
    );
    for (kind, length) in [
        (HashAlgorithm::Md5, 16),
        (HashAlgorithm::Sha1, 20),
        (HashAlgorithm::Sha256, 32),
        (HashAlgorithm::Sha384, 48),
        (HashAlgorithm::Sha512, 64),
    ] {
        assert_eq!(
            hmac_computer::compute(kind, b"key", b"message")
                .expect("offered")
                .len(),
            length
        );
    }
    assert!(hmac_computer::compute(HashAlgorithm::Sha3_256, b"key", b"message").is_err());
    assert_eq!(hmac_computer::display_name(HashAlgorithm::Sha3_256), None);
    assert_eq!(
        hmac_computer::format(&[0xAB, 0xCD, 0xEF], HmacOutputFormat::Hex),
        "abcdef"
    );
    assert_eq!(
        hmac_computer::format(&[0x12, 0x34, 0x56, 0x78], HmacOutputFormat::Base64),
        "EjRWeA=="
    );
}

#[test]
fn an_hmac_pasted_matches_its_hex_or_its_base64_whatever_the_case() {
    let bytes = b"abc";
    for blank in ["", "   "] {
        assert_eq!(hmac_computer::verify(bytes, blank), None);
    }
    let hex = hmac_computer::format(bytes, HmacOutputFormat::Hex);
    assert_eq!(
        hmac_computer::verify(bytes, &hex),
        Some(HmacOutputFormat::Hex)
    );
    assert_eq!(
        hmac_computer::verify(bytes, &hex.to_uppercase()),
        Some(HmacOutputFormat::Hex)
    );
    assert_eq!(
        hmac_computer::verify(bytes, &format!("  {hex}  ")),
        Some(HmacOutputFormat::Hex)
    );
    let code = hmac_computer::compute(HashAlgorithm::Sha256, b"key", b"message").expect("mac");
    let base64 = hmac_computer::format(&code, HmacOutputFormat::Base64);
    assert_eq!(
        hmac_computer::verify(&code, &base64),
        Some(HmacOutputFormat::Base64)
    );
    // As the C#: one letter's case flipped still matches.
    let flipped: String = {
        let mut done = false;
        base64
            .chars()
            .map(|c| {
                if !done && c.is_ascii_alphabetic() {
                    done = true;
                    if c.is_ascii_uppercase() {
                        c.to_ascii_lowercase()
                    } else {
                        c.to_ascii_uppercase()
                    }
                } else {
                    c
                }
            })
            .collect()
    };
    assert_eq!(
        hmac_computer::verify(&code, &flipped),
        Some(HmacOutputFormat::Base64)
    );
    assert_eq!(hmac_computer::verify(bytes, "not-a-match"), None);
    assert_eq!(hmac_computer::verify(&[], "abc"), None);
}

/// The RFC 6238 seeds, by digest.
const SHA1_SEED: &[u8] = b"12345678901234567890";
const SHA256_SEED: &[u8] = b"12345678901234567890123456789012";
const SHA512_SEED: &[u8] = b"1234567890123456789012345678901234567890123456789012345678901234";

#[test]
fn totp_codes_match_rfc_6238_appendix_b() {
    // Time, then the eight-digit codes by SHA-1, SHA-256 and SHA-512.
    let table: [(i64, [&str; 3]); 6] = [
        (59, ["94287082", "46119246", "90693936"]),
        (1_111_111_109, ["07081804", "68084774", "25091201"]),
        (1_111_111_111, ["14050471", "67062674", "99943326"]),
        (1_234_567_890, ["89005924", "91819424", "93441116"]),
        (2_000_000_000, ["69279037", "90698825", "38618901"]),
        (20_000_000_000, ["65353130", "77737706", "47863826"]),
    ];
    for (time, codes) in table {
        for ((kind, seed), expected) in [
            (HashAlgorithm::Sha1, SHA1_SEED),
            (HashAlgorithm::Sha256, SHA256_SEED),
            (HashAlgorithm::Sha512, SHA512_SEED),
        ]
        .into_iter()
        .zip(codes)
        {
            assert_eq!(
                totp_generator::generate(seed, time, kind, 8, DEFAULT_TIME_STEP_SECONDS),
                Ok(expected.to_owned()),
                "{kind:?} at {time}"
            );
        }
    }
}

#[test]
fn totp_codes_with_the_csharp_defaults_match_its_tests() {
    for (time, expected) in [
        (59, "287082"),
        (1_111_111_109, "081804"),
        (1_111_111_111, "050471"),
        (1_234_567_890, "005924"),
        (2_000_000_000, "279037"),
        (20_000_000_000, "353130"),
    ] {
        assert_eq!(
            totp_generator::generate(
                SHA1_SEED,
                time,
                DEFAULT_ALGORITHM,
                DEFAULT_DIGITS,
                DEFAULT_TIME_STEP_SECONDS
            ),
            Ok(expected.to_owned())
        );
    }
    let six = |kind, seed| totp_generator::generate(seed, 59, kind, 6, 30);
    assert_eq!(
        six(HashAlgorithm::Sha256, SHA256_SEED),
        Ok("119246".to_owned())
    );
    assert_eq!(
        six(HashAlgorithm::Sha512, SHA512_SEED),
        Ok("693936".to_owned())
    );
    assert_eq!(
        six(HashAlgorithm::Md5, SHA1_SEED),
        Err(TotpError::Algorithm)
    );
    assert_eq!(
        six(HashAlgorithm::Sha384, SHA1_SEED),
        Err(TotpError::Algorithm)
    );
    let with =
        |digits, step| totp_generator::generate(SHA1_SEED, 59, DEFAULT_ALGORITHM, digits, step);
    assert_eq!(with(0, 30), Err(TotpError::Digits));
    assert_eq!(with(10, 30), Err(TotpError::Digits));
    assert_eq!(with(6, 0), Err(TotpError::TimeStep));
    assert_eq!(with(6, -30), Err(TotpError::TimeStep));
    assert_eq!(with(1, 30).map(|code| code.len()), Ok(1));
    assert_eq!(with(9, 30).map(|code| code.len()), Ok(9));
}

#[test]
fn the_step_counts_seconds_gone_and_left() {
    assert_eq!(totp_generator::elapsed_in_step(60, 30), Ok(0));
    assert_eq!(totp_generator::elapsed_in_step(75, 30), Ok(15));
    assert_eq!(totp_generator::remaining_in_step(60, 30), Ok(30));
    assert_eq!(totp_generator::remaining_in_step(75, 30), Ok(15));
    assert_eq!(
        totp_generator::elapsed_in_step(60, 0),
        Err(TotpError::TimeStep)
    );
    assert_eq!(
        totp_generator::remaining_in_step(60, 0),
        Err(TotpError::TimeStep)
    );
}

#[test]
fn base32_is_read_as_the_csharp_reads_it() {
    assert_eq!(totp_generator::decode_base32(""), Ok(Vec::new()));
    assert_eq!(
        totp_generator::decode_base32("JBSWY3DPEB3W64TMMQ======"),
        Ok(b"Hello world".to_vec())
    );
    assert_eq!(
        totp_generator::decode_base32("jbswy3dpeb3w64tmmq======"),
        Ok(b"Hello world".to_vec())
    );
    // RFC 4648, section 10.
    for (encoded, expected) in [
        ("MY======", "f"),
        ("MZXQ====", "fo"),
        ("MZXW6===", "foo"),
        ("MZXW6YQ=", "foob"),
        ("MZXW6YTB", "fooba"),
        ("MZXW6YTBOI======", "foobar"),
    ] {
        assert_eq!(
            totp_generator::decode_base32(encoded),
            Ok(expected.as_bytes().to_vec()),
            "{encoded}"
        );
    }
    assert_eq!(
        totp_generator::decode_base32("ABC1"),
        Err(InvalidBase32('1'))
    );
    assert_eq!(
        InvalidBase32('1').to_string(),
        "Invalid Base32 character: 1"
    );
    assert_eq!(
        totp_generator::decode_base32("AAAA===="),
        totp_generator::decode_base32("AAAA")
    );
    assert_eq!(
        totp_generator::decode_base32("A A"),
        Err(InvalidBase32(' '))
    );
}

fn segment(json: &str) -> String {
    BASE64URL_NOPAD.encode(json.as_bytes())
}

fn token(header: &str, payload: &str, signature: &[u8]) -> String {
    format!(
        "{}.{}.{}",
        segment(header),
        segment(payload),
        BASE64URL_NOPAD.encode(signature)
    )
}

/// The token of RFC 7519, section 3.1, signed by the key of RFC 7515, appendix A.1.
const RFC_7519_TOKEN: &str = "eyJ0eXAiOiJKV1QiLA0KICJhbGciOiJIUzI1NiJ9.\
    eyJpc3MiOiJqb2UiLA0KICJleHAiOjEzMDA4MTkzODAsDQogImh0dHA6Ly9leGFtcGxlLmNvbS9pc19yb290Ijp0cnVlfQ.\
    dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk";

/// The key of RFC 7515, appendix A.1, as its JWK writes it.
const RFC_7515_KEY: &str =
    "AyM1SysPpbyDfgZld3umj1qzKObwVMkoqQ-EstJQLr_T-1qS0gZH75aKtMN3Yj0iPS4hcgUuTwjAzZr1Z9CAow";

#[test]
fn the_rfc_7519_example_is_read_shown_and_its_signature_is_the_hmac_of_its_segments() {
    let decoded = jwt_parser::decode(RFC_7519_TOKEN).expect("decoded");
    assert_eq!(
        decoded.header_json,
        "{\"typ\":\"JWT\",\r\n \"alg\":\"HS256\"}"
    );
    assert_eq!(
        decoded.pretty_header("\n"),
        "{\n  \"typ\": \"JWT\",\n  \"alg\": \"HS256\"\n}"
    );
    assert_eq!(
        decoded.pretty_payload("\n"),
        "{\n  \"iss\": \"joe\",\n  \"exp\": 1300819380,\n  \"http://example.com/is_root\": true\n}"
    );
    assert_eq!(
        jwt_parser::extract_algorithm(&decoded.header_json).as_deref(),
        Some("HS256")
    );
    assert_eq!(
        jwt_parser::evaluate_expiration(&decoded.payload_json, 1_300_819_381),
        Expiration::Expired(1_300_819_380)
    );
    let key = BASE64URL_NOPAD
        .decode(RFC_7515_KEY.as_bytes())
        .expect("the key");
    let input = format!("{}.{}", decoded.header_raw, decoded.payload_raw);
    let code = hmac_computer::compute(HashAlgorithm::Sha256, &key, input.as_bytes()).expect("mac");
    assert_eq!(code, decoded.signature);
}

#[test]
fn a_token_signed_by_a_typed_secret_is_valid_by_it_alone() {
    // The token jwt.io shows, signed by its default secret.
    let token = "eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.\
        eyJzdWIiOiIxMjM0NTY3ODkwIiwibmFtZSI6IkpvaG4gRG9lIiwiaWF0IjoxNTE2MjM5MDIyfQ.\
        SflKxwRJSMeKKF2QT4fwpMeJf36POk6yJV_adQssw5c";
    let decoded = jwt_parser::decode(token).expect("decoded");
    let alg = jwt_parser::classify_algorithm(
        jwt_parser::extract_algorithm(&decoded.header_json).as_deref(),
    );
    assert_eq!(alg, JwtAlgorithm::Hmac256);
    assert_eq!(
        jwt_parser::verify_hmac(&decoded, alg, "your-256-bit-secret"),
        HmacVerification::Valid
    );
    assert_eq!(
        jwt_parser::verify_hmac(&decoded, alg, "other"),
        HmacVerification::Invalid
    );
    assert_eq!(
        jwt_parser::evaluate_expiration(&decoded.payload_json, 0),
        Expiration::NoExpiry
    );
}

/// A token signed by `secret` with `alg`, as the C# tests' `CreateSignedDecoded`.
fn signed(alg: &str, secret: &str) -> jwt_parser::JwtDecoded {
    let header = segment(&format!("{{\"alg\":\"{alg}\",\"typ\":\"JWT\"}}"));
    let payload = segment("{\"sub\":\"john\"}");
    let kind = match alg {
        "HS384" => HashAlgorithm::Sha384,
        "HS512" => HashAlgorithm::Sha512,
        _ => HashAlgorithm::Sha256,
    };
    let signature = hmac_computer::compute(
        kind,
        secret.as_bytes(),
        format!("{header}.{payload}").as_bytes(),
    )
    .expect("mac");
    jwt_parser::decode(&format!(
        "{header}.{payload}.{}",
        BASE64URL_NOPAD.encode(&signature)
    ))
    .expect("decoded")
}

#[test]
fn hmac_signatures_are_checked_by_their_algorithm_alone() {
    for (alg, kind) in [
        ("HS256", JwtAlgorithm::Hmac256),
        ("HS384", JwtAlgorithm::Hmac384),
        ("HS512", JwtAlgorithm::Hmac512),
    ] {
        assert_eq!(jwt_parser::classify_algorithm(Some(alg)), kind);
        let decoded = signed(alg, "secret");
        assert_eq!(
            jwt_parser::verify_hmac(&decoded, kind, "secret"),
            HmacVerification::Valid
        );
        assert_eq!(
            jwt_parser::verify_hmac(&decoded, kind, "other"),
            HmacVerification::Invalid
        );
    }
    let decoded = signed("HS256", "secret");
    for kind in [
        JwtAlgorithm::Rsa,
        JwtAlgorithm::Ecdsa,
        JwtAlgorithm::RsaPss,
        JwtAlgorithm::Unknown,
        JwtAlgorithm::None,
    ] {
        assert_eq!(
            jwt_parser::verify_hmac(&decoded, kind, "secret"),
            HmacVerification::AlgorithmNotHmac
        );
    }
    assert_eq!(
        jwt_parser::verify_hmac(&decoded, JwtAlgorithm::Hmac256, ""),
        HmacVerification::MalformedInput
    );
    for (alg, kind) in [
        (Some("RS256"), JwtAlgorithm::Rsa),
        (Some("ES256"), JwtAlgorithm::Ecdsa),
        (Some("PS256"), JwtAlgorithm::RsaPss),
        (Some("none"), JwtAlgorithm::None),
        (Some("weird"), JwtAlgorithm::Unknown),
        (Some("hs256"), JwtAlgorithm::Unknown),
        (None, JwtAlgorithm::Unknown),
    ] {
        assert_eq!(jwt_parser::classify_algorithm(alg), kind, "{alg:?}");
    }
}

#[test]
fn a_token_not_in_three_decodable_segments_is_refused_as_the_csharp_says() {
    for input in ["", " ", "abc", "a.b", "a.b.c.d"] {
        assert_eq!(
            jwt_parser::decode(input),
            Err(JwtDecodeError::InvalidFormat),
            "{input:?}"
        );
    }
    for (header, payload, signature) in [
        ("%%%%", "eyJzdWIiOiJqb2huIn0", ""),
        ("eyJhbGciOiJIUzI1NiJ9", "%%%%", ""),
        ("eyJhbGciOiJIUzI1NiJ9", "eyJzdWIiOiJqb2huIn0", "%%%%"),
        ("bm90LWpzb24", "eyJzdWIiOiJqb2huIn0", ""),
        ("eyJhbGciOiJIUzI1NiJ9", "bm90LWpzb24", ""),
    ] {
        assert_eq!(
            jwt_parser::decode(&format!("{header}.{payload}.{signature}")),
            Err(JwtDecodeError::DecodeFailed)
        );
    }
}

#[test]
fn a_token_keeps_its_segments_and_may_have_no_signature() {
    let jwt = token(
        "{\"alg\":\"HS256\",\"typ\":\"JWT\"}",
        "{\"sub\":\"john\",\"role\":\"admin\"}",
        &[0xAB, 0xCD],
    );
    let decoded = jwt_parser::decode(&format!("  {jwt}  ")).expect("decoded");
    assert_eq!(decoded.header_json, "{\"alg\":\"HS256\",\"typ\":\"JWT\"}");
    assert_eq!(
        decoded.payload_json,
        "{\"sub\":\"john\",\"role\":\"admin\"}"
    );
    assert_eq!(decoded.signature_hex(), "abcd");
    assert!(decoded.pretty_header("\n").contains("  \"alg\""));
    let header = segment("{\"alg\":\"none\"}");
    let payload = segment("{\"sub\":\"john\"}");
    let unsigned = jwt_parser::decode(&format!("{header}.{payload}.")).expect("decoded");
    assert_eq!(unsigned.header_raw, header);
    assert_eq!(unsigned.payload_raw, payload);
    assert!(unsigned.signature_raw.is_empty() && unsigned.signature.is_empty());
    let arrays = jwt_parser::decode(&token("[\"alg\",\"HS256\"]", "[\"a\",\"b\"]", &[]))
        .expect("arrays are JSON");
    assert_eq!(jwt_parser::extract_algorithm(&arrays.header_json), None);
    let utf8 = jwt_parser::decode(&token("{\"alg\":\"HS256\"}", "{\"name\":\"éclair\"}", &[]))
        .expect("decoded");
    assert!(utf8.payload_json.contains("éclair"));
}

#[test]
fn base64url_segments_are_read_without_their_padding() {
    for (segment, expected) in [("QQ", "A"), ("SGVsbG8", "Hello"), ("YWJjZA", "abcd")] {
        assert_eq!(
            jwt_parser::decode_base64url_string(segment).as_deref(),
            Some(expected)
        );
    }
    assert_eq!(jwt_parser::decode_base64url_string("").as_deref(), Some(""));
    assert_eq!(
        jwt_parser::decode_base64url_string("eyJmb28iOiJiYXIifQ").as_deref(),
        Some("{\"foo\":\"bar\"}")
    );
    assert_eq!(jwt_parser::decode_base64url_bytes("%%%%"), None);
}

#[test]
fn json_is_shown_as_dotnet_writes_it_indented() {
    assert_eq!(
        jwt_parser::pretty_print(
            "{\"sub\":\"john\",\"scope\":[\"a\",{\"b\":1.50}],\"e\":{},\"f\":[],\"n\":null}",
            "\r\n"
        ),
        "{\r\n  \"sub\": \"john\",\r\n  \"scope\": [\r\n    \"a\",\r\n    {\r\n      \"b\": 1.50\r\n    }\r\n  ],\r\n  \"e\": {},\r\n  \"f\": [],\r\n  \"n\": null\r\n}"
    );
    // .NET's default encoder: past ASCII, and the HTML-sensitive characters, escaped.
    assert_eq!(
        jwt_parser::pretty_print("{\"x\":\"é<&>'+`\\\"\\\\\\/\\n\\t\\u0001\u{1F600}\"}", "\n"),
        "{\n  \"x\": \"\\u00E9\\u003C\\u0026\\u003E\\u0027\\u002B\\u0060\\u0022\\\\/\\n\\t\\u0001\\uD83D\\uDE00\"\n}"
    );
    assert_eq!(jwt_parser::pretty_print("[]", "\n"), "[]");
    assert_eq!(jwt_parser::pretty_print("42", "\n"), "42");
    // As deep as .NET's JsonDocument reads, and one level past it.
    let nested = |depth: usize| format!("{}{}", "[".repeat(depth), "]".repeat(depth));
    assert!(jwt_parser::is_valid_json(&nested(64)));
    assert!(!jwt_parser::is_valid_json(&nested(65)));
}

#[test]
fn the_expiry_is_read_against_the_clock_as_the_csharp_reads_it() {
    // 2026-04-19T12:00:00Z, the C# tests' moment.
    let now = 1_776_600_000;
    assert_eq!(
        jwt_parser::evaluate_expiration("{\"sub\":\"john\"}", now),
        Expiration::NoExpiry
    );
    assert_eq!(
        jwt_parser::evaluate_expiration("{\"exp\":1776600001}", now),
        Expiration::Valid(1_776_600_001)
    );
    assert_eq!(
        jwt_parser::evaluate_expiration("{\"exp\":1600000000}", now),
        Expiration::Expired(1_600_000_000)
    );
    assert_eq!(
        jwt_parser::evaluate_expiration(&format!("{{\"exp\":{now}}}"), now),
        Expiration::Valid(now),
        "a moment equal to now is not past"
    );
    for invalid in [
        "{\"exp\":\"tomorrow\"}",
        "{\"exp\":null}",
        "{\"exp\":true}",
        "{\"exp\":1.5}",
        "{",
        "{\"exp\":999999999999999999}",
        "[1]",
    ] {
        assert_eq!(
            jwt_parser::evaluate_expiration(invalid, now),
            Expiration::InvalidClaim,
            "{invalid}"
        );
    }
    let now = 1_000_000_000;
    assert_eq!(
        jwt_parser::evaluate_expiration("{\"exp\":1}", now),
        Expiration::Expired(1)
    );
    assert_eq!(
        jwt_parser::evaluate_expiration("{\"exp\":31536000000}", now),
        Expiration::Valid(31_536_000_000)
    );
}
