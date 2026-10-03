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

//! What is read from a server's TLS certificate: the key it is pinned by, the key `CredSSP`
//! binds to, and a subject fit to show.

use std::fmt;
use std::str::FromStr;

use data_encoding::BASE64_NOPAD;
use ring::digest::{SHA256, SHA256_OUTPUT_LEN, digest};
use x509_cert::Certificate;
use x509_cert::der::{Decode as _, Encode as _};

/// Prefix of a fingerprint's text form, as OpenSSH writes key fingerprints.
const FINGERPRINT_PREFIX: &str = "SHA256:";

/// Longest subject shown, in characters: the server chooses it.
const MAX_SUBJECT_CHARS: usize = 200;

/// SHA-256 of a certificate's `SubjectPublicKeyInfo`: what a server is pinned by. A renewed
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

impl FromStr for Fingerprint {
    type Err = FingerprintParseError;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        let encoded = text
            .strip_prefix(FINGERPRINT_PREFIX)
            .ok_or(FingerprintParseError)?;
        let bytes = BASE64_NOPAD
            .decode(encoded.as_bytes())
            .map_err(|_| FingerprintParseError)?;
        bytes
            .try_into()
            .map(Self)
            .map_err(|_| FingerprintParseError)
    }
}

/// A certificate that cannot be read.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("the server certificate cannot be read")]
pub struct CertificateError;

/// What the connection needs from the server's certificate, all read from the same one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServerCertificate {
    /// The pin.
    pub fingerprint: Fingerprint,
    /// The `subjectPublicKey` bits, which `CredSSP` binds its exchange to.
    pub public_key: Vec<u8>,
    /// The subject, made safe to show and bounded.
    pub subject: String,
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
        let hash = digest(&SHA256, &spki);
        let fingerprint = Fingerprint(hash.as_ref().try_into().map_err(|_| CertificateError)?);
        let public_key = info
            .subject_public_key
            .as_bytes()
            .ok_or(CertificateError)?
            .to_vec();
        Ok(Self {
            fingerprint,
            public_key,
            subject: shown(&certificate.tbs_certificate().subject().to_string()),
        })
    }
}

/// Text chosen by the server, fit to show: no control or direction-changing characters,
/// bounded.
fn shown(text: &str) -> String {
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
    fn a_subject_loses_what_could_mislead() {
        assert_eq!(shown("CN=a\u{202E}b\u{0007}c"), "CN=abc");
        assert_eq!(shown(&"x".repeat(500)).chars().count(), MAX_SUBJECT_CHARS);
    }
}
