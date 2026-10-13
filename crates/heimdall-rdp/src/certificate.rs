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

//! What is read from a server's TLS certificate: its key, the key `CredSSP` binds to, a
//! subject and an issuer fit to show, when it holds, and the hash of the whole certificate,
//! which pins an RDP, FTPS or VNC server's certificate as the C# pins it.

use std::fmt;
use std::str::FromStr;
use std::time::SystemTime;

use data_encoding::BASE64_NOPAD;
use sealvault::hash::{SHA256_LEN as SHA256_OUTPUT_LEN, sha256};
use x509_cert::Certificate;
use x509_cert::der::asn1::ObjectIdentifier;
use x509_cert::der::{Decode as _, Encode as _};
use x509_cert::ext::pkix::ExtendedKeyUsage;

/// Prefix of a fingerprint's text form, as OpenSSH writes key fingerprints.
const FINGERPRINT_PREFIX: &str = "SHA256:";

/// What separates the bytes of a thumbprint, as the C# writes one.
const THUMBPRINT_SEPARATOR: &str = ":";

/// Longest subject or issuer shown, in characters: the server chooses them.
const MAX_SUBJECT_CHARS: usize = 200;

/// `id-kp-serverAuth` (RFC 5280, 4.2.1.12): the purpose of a TLS server's certificate.
const SERVER_AUTHENTICATION: ObjectIdentifier = ObjectIdentifier::new_unwrap("1.3.6.1.5.5.7.3.1");

/// `anyExtendedKeyUsage` (RFC 5280, 4.2.1.12): any purpose.
const ANY_PURPOSE: ObjectIdentifier = ObjectIdentifier::new_unwrap("2.5.29.37.0");

/// SHA-256 of a certificate's `SubjectPublicKeyInfo`: what a server was pinned by before
/// its whole certificate was, and still tells a renewal from another key. A renewed
/// certificate on the same key keeps it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Fingerprint([u8; SHA256_OUTPUT_LEN]);

impl fmt::Display for Fingerprint {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{FINGERPRINT_PREFIX}{}", BASE64_NOPAD.encode(&self.0))
    }
}

/// A fingerprint that does not parse.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("not a SHA256 fingerprint")]
pub struct FingerprintParseError;

/// The SHA-256 a fingerprint's text form holds.
fn parse_sha256(text: &str) -> Result<[u8; SHA256_OUTPUT_LEN], FingerprintParseError> {
    let encoded = text
        .strip_prefix(FINGERPRINT_PREFIX)
        .ok_or(FingerprintParseError)?;
    let bytes = BASE64_NOPAD
        .decode(encoded.as_bytes())
        .map_err(|_| FingerprintParseError)?;
    bytes.try_into().map_err(|_| FingerprintParseError)
}

impl FromStr for Fingerprint {
    type Err = FingerprintParseError;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        parse_sha256(text).map(Self)
    }
}

/// SHA-256 of a whole certificate (DER), as the C# pins an RDP or FTPS certificate by its
/// thumbprint: another certificate on the same key, a renewed one or one minted again, is
/// another certificate.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct CertificateHash([u8; SHA256_OUTPUT_LEN]);

impl CertificateHash {
    /// The hash of the DER certificate `der`.
    #[must_use]
    pub fn of(der: &[u8]) -> Self {
        Self(sha256(der))
    }

    /// As the C# shows a thumbprint (`CertificateFingerprint.ComputeSha256`): `SHA256:`,
    /// then each byte in upper-case hexadecimal, separated by colons.
    #[must_use]
    pub fn thumbprint(&self) -> String {
        let bytes: Vec<String> = self.0.iter().map(|byte| format!("{byte:02X}")).collect();
        format!("{FINGERPRINT_PREFIX}{}", bytes.join(THUMBPRINT_SEPARATOR))
    }
}

/// As a file records it, as a key's fingerprint: `SHA256:` then base64 without padding.
impl fmt::Display for CertificateHash {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{FINGERPRINT_PREFIX}{}", BASE64_NOPAD.encode(&self.0))
    }
}

impl FromStr for CertificateHash {
    type Err = FingerprintParseError;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        parse_sha256(text).map(Self)
    }
}

/// A certificate that cannot be read.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("the server certificate cannot be read")]
pub struct CertificateError;

/// What the connection needs from the server's certificate, all read from the same one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServerCertificate {
    /// Its key.
    pub fingerprint: Fingerprint,
    /// The hash of the whole certificate: the pin.
    pub certificate: CertificateHash,
    /// When it holds.
    pub validity: Validity,
    /// The `subjectPublicKey` bits, which `CredSSP` binds its exchange to.
    pub public_key: Vec<u8>,
    /// The subject, made safe to show and bounded.
    pub subject: String,
    /// The issuer, made safe to show and bounded.
    pub issuer: String,
}

impl ServerCertificate {
    /// Reads a DER certificate.
    ///
    /// # Errors
    ///
    /// [`CertificateError`] when the DER does not parse, or the key bits are not whole bytes.
    pub fn from_der(der: &[u8]) -> Result<Self, CertificateError> {
        let certificate = Certificate::from_der(der).map_err(|_| CertificateError)?;
        let info = certificate.tbs_certificate().subject_public_key_info();
        let spki = info.to_der().map_err(|_| CertificateError)?;
        let fingerprint = Fingerprint(sha256(&spki));
        let public_key = info
            .subject_public_key
            .as_bytes()
            .ok_or(CertificateError)?
            .to_vec();
        let validity = certificate.tbs_certificate().validity();
        Ok(Self {
            fingerprint,
            certificate: CertificateHash::of(der),
            validity: Validity {
                not_before: validity.not_before.to_system_time(),
                not_after: validity.not_after.to_system_time(),
            },
            public_key,
            subject: shown(&certificate.tbs_certificate().subject().to_string()),
            issuer: shown(&certificate.tbs_certificate().issuer().to_string()),
        })
    }
}

/// When a certificate holds, as its `notBefore` and `notAfter` say: shown in the FTPS
/// certificate question, as the C# prompt's "Valid from / until", and in the question about
/// a renewed certificate.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Validity {
    /// The first moment it holds.
    pub not_before: SystemTime,
    /// The last moment it holds.
    pub not_after: SystemTime,
}

/// Where a moment falls in a certificate's [`Validity`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ValidityPeriod {
    /// Between its two dates, both included.
    Current,
    /// After its last moment.
    Expired,
    /// Before its first moment.
    NotYetValid,
}

impl Validity {
    /// Reads the validity of a DER certificate.
    ///
    /// # Errors
    ///
    /// [`CertificateError`] when the DER does not parse.
    pub fn from_der(der: &[u8]) -> Result<Self, CertificateError> {
        let certificate = Certificate::from_der(der).map_err(|_| CertificateError)?;
        let validity = certificate.tbs_certificate().validity();
        Ok(Self {
            not_before: validity.not_before.to_system_time(),
            not_after: validity.not_after.to_system_time(),
        })
    }

    /// Where `now` falls in it.
    #[must_use]
    pub fn period(&self, now: SystemTime) -> ValidityPeriod {
        if now < self.not_before {
            ValidityPeriod::NotYetValid
        } else if now > self.not_after {
            ValidityPeriod::Expired
        } else {
            ValidityPeriod::Current
        }
    }
}

/// Whether a DER certificate may serve a TLS server, as its extended key usage says: yes
/// without that extension, else only when it names a server's purpose or any purpose.
///
/// # Errors
///
/// [`CertificateError`] when the DER, or its extended key usage, does not parse, or when the
/// extension is there more than once.
pub fn serves_tls_servers(der: &[u8]) -> Result<bool, CertificateError> {
    let certificate = Certificate::from_der(der).map_err(|_| CertificateError)?;
    let usage = certificate
        .tbs_certificate()
        .get_extension::<ExtendedKeyUsage>()
        .map_err(|_| CertificateError)?;
    Ok(usage.is_none_or(|(_, usage)| {
        usage
            .0
            .iter()
            .any(|purpose| *purpose == SERVER_AUTHENTICATION || *purpose == ANY_PURPOSE)
    }))
}

/// Text chosen by the server, fit to show: no control or direction-changing characters,
/// bounded.
pub(crate) fn shown(text: &str) -> String {
    text.chars()
        .filter(|c| !c.is_control() && !is_bidi_control(*c))
        .take(MAX_SUBJECT_CHARS)
        .collect()
}

/// Characters that reorder the text around them: a subject could hide its real content.
fn is_bidi_control(c: char) -> bool {
    matches!(
        c,
        '\u{061C}' | '\u{200E}' | '\u{200F}' | '\u{202A}'..='\u{202E}' | '\u{2066}'..='\u{2069}'
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_fingerprint_reads_back_from_its_text() {
        let fingerprint = Fingerprint([7; SHA256_OUTPUT_LEN]);
        assert_eq!(fingerprint.to_string().parse(), Ok(fingerprint));
        for bad in [
            "",
            "SHA256:",
            "sha256:BwcH",
            "SHA256:not base64!",
            "SHA256:BwcH",
        ] {
            assert_eq!(
                bad.parse::<Fingerprint>(),
                Err(FingerprintParseError),
                "{bad}"
            );
        }
    }

    #[test]
    fn a_certificate_hash_reads_back_and_shows_as_the_c_sharp_thumbprint() {
        let hash = CertificateHash::of(b"abc");
        assert_eq!(hash.to_string().parse(), Ok(hash));
        // SHA-256 of "abc", from FIPS 180-2.
        assert_eq!(
            hash.thumbprint(),
            "SHA256:BA:78:16:BF:8F:01:CF:EA:41:41:40:DE:5D:AE:22:23:\
             B0:03:61:A3:96:17:7A:9C:B4:10:FF:61:F2:00:15:AD"
        );
        assert_eq!(
            "SHA256:".parse::<CertificateHash>(),
            Err(FingerprintParseError)
        );
    }

    #[test]
    fn a_moment_falls_before_within_or_after_a_validity() {
        use std::time::{Duration, UNIX_EPOCH};

        let at = |seconds| UNIX_EPOCH + Duration::from_secs(seconds);
        let validity = Validity {
            not_before: at(100),
            not_after: at(200),
        };
        assert_eq!(validity.period(at(99)), ValidityPeriod::NotYetValid);
        assert_eq!(validity.period(at(100)), ValidityPeriod::Current);
        assert_eq!(validity.period(at(200)), ValidityPeriod::Current);
        assert_eq!(validity.period(at(201)), ValidityPeriod::Expired);
        assert_eq!(
            Validity::from_der(b"not a certificate"),
            Err(CertificateError)
        );
    }

    #[test]
    fn a_subject_loses_what_could_mislead() {
        assert_eq!(shown("CN=a\u{202E}b\u{0007}c"), "CN=abc");
        assert_eq!(shown(&"x".repeat(500)).chars().count(), MAX_SUBJECT_CHARS);
    }
}
