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

//! The Certificate Generator's engine, as the C# `Heimdall.Core.Certificates`:
//! `CertificateOptions` and its validation (`CertificateOptions.cs:3-34`), `SanParser`
//! (`SanParser.cs:3-16`), `DistinguishedNameBuilder` (`DistinguishedNameBuilder.cs:9-26`),
//! `CertificateFingerprint` (`CertificateFingerprint.cs:6-15`) and `CertificateGenerator`
//! (`CertificateGenerator.cs:23-188`): a self-signed leaf, or a CA and a leaf it signs, each
//! with its own RSA key, signed with SHA-256 and PKCS #1 v1.5; their PEM, their PFX.
//!
//! The extensions are the C#'s, with the C#'s criticality: a leaf says it is no CA, may sign
//! and encipher keys, serves and authenticates TLS, and carries its alternative names; a CA
//! says it is one and signs certificates and revocation lists, both critical.
//!
//! Private keys are secrets: held in memory wiped when dropped, never written out by `Debug`.

use std::fmt;
use std::net::IpAddr;
use std::time::Duration;

use rcgen::{
    CertificateParams, CustomExtension, DistinguishedName, DnType, DnValue,
    ExtendedKeyUsagePurpose, Issuer, KeyPair, PKCS_RSA_SHA256, SanType, SerialNumber,
};
use rsa::pkcs8::EncodePrivateKey as _;
use sha2::{Digest as _, Sha256};
use zeroize::Zeroizing;

use super::pkcs8_pem::{self, PemError};
use super::pkcs12::{self, PfxError};

/// Bits of an RSA key of the first size, as the C# `Rsa2048KeySize`.
pub const RSA_2048_BITS: usize = 2048;

/// Bits of an RSA key of the second size, as the C# `Rsa4096KeySize`.
pub const RSA_4096_BITS: usize = 4096;

/// Days a CA is valid, as the C# `CaValidityDays`.
pub const CA_VALIDITY_DAYS: u32 = 3650;

/// Days a certificate is valid unless told otherwise, as the C# `DefaultValidityDays`.
pub const DEFAULT_VALIDITY_DAYS: u32 = 365;

/// Bytes of a leaf's serial number, as the C# `SerialNumberLength`.
pub const SERIAL_NUMBER_LENGTH: usize = 16;

/// Bytes of a self-signed certificate's serial number, as .NET's `CreateSelfSigned` draws.
const SELF_SIGNED_SERIAL_LENGTH: usize = 8;

/// The mask keeping a serial number positive, as the C# `PositiveMsbMask`.
const POSITIVE_MSB_MASK: u8 = 0x7f;

/// What a serial number starts with when its first byte was drawn zero, so that its DER
/// integer keeps the length drawn.
const NONZERO_FIRST_BYTE: u8 = 0x01;

/// Seconds in a day.
const SECONDS_PER_DAY: u64 = 86_400;

/// The last second an X.509 time can say, 9999-12-31 23:59:59 UTC.
const LAST_X509_SECOND: u64 = 253_402_300_799;

/// What separates the alternative names typed, as the C# `SanParser`.
const SAN_SEPARATOR: char = ',';

/// What the subject's CA is named after its leaf, as the C#'s `"{Cn} CA"`.
const CA_NAME_SUFFIX: &str = " CA";

/// What a fingerprint starts with, as the C# `CertificateFingerprint`.
const FINGERPRINT_PREFIX: &str = "SHA256:";

/// What separates a fingerprint's bytes, as the C#'s dashes made colons.
const FINGERPRINT_SEPARATOR: &str = ":";

/// What separates the parts of a distinguished name said, as the C# `string.Join(", ")`.
const NAME_SEPARATOR: &str = ", ";

/// OIDs of the extensions written as the C# writes them.
const OID_BASIC_CONSTRAINTS: &[u64] = &[2, 5, 29, 19];
const OID_KEY_USAGE: &[u64] = &[2, 5, 29, 15];

/// `BasicConstraints` of a leaf, `cA` false: an empty sequence.
const LEAF_BASIC_CONSTRAINTS: &[u8] = &[0x30, 0x00];

/// `BasicConstraints` of a CA: `cA` true, no path length.
const CA_BASIC_CONSTRAINTS: &[u8] = &[0x30, 0x03, 0x01, 0x01, 0xff];

/// `KeyUsage` of a leaf: `digitalSignature` and `keyEncipherment`.
const LEAF_KEY_USAGE: &[u8] = &[0x03, 0x02, 0x05, 0xa0];

/// `KeyUsage` of a CA: `keyCertSign` and `cRLSign`.
const CA_KEY_USAGE: &[u8] = &[0x03, 0x02, 0x01, 0x06];

/// The kind of certificate made, as the C# `CertificateMode`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CertificateMode {
    /// One leaf, signed by its own key.
    #[default]
    SelfSigned,
    /// A CA, and a leaf it signs.
    CaLeaf,
}

/// What is wrong with the options, as the C# `CertificateValidationCode`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ValidationCode {
    /// Nothing.
    Ok,
    /// No common name.
    CnRequired,
    /// Fewer than one day.
    InvalidValidity,
}

/// What a certificate is made of, as the C# `CertificateOptions`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CertificateOptions {
    /// The common name.
    pub cn: String,
    /// The organisation, left out when blank.
    pub org: String,
    /// The country, left out when blank.
    pub country: String,
    /// Bits of each RSA key.
    pub key_bits: usize,
    /// Days the certificate is valid.
    pub validity_days: i64,
    /// The alternative names: IP addresses, and DNS names.
    pub sans: Vec<String>,
}

impl CertificateOptions {
    /// What is wrong with them, as the C# `Validate`.
    #[must_use]
    pub fn validate(&self) -> ValidationCode {
        if self.cn.trim().is_empty() {
            ValidationCode::CnRequired
        } else if self.validity_days < 1 {
            ValidationCode::InvalidValidity
        } else {
            ValidationCode::Ok
        }
    }
}

/// The alternative names in `input`, comma separated, each trimmed, the empty ones left
/// out, as the C# `SanParser.Parse`.
#[must_use]
pub fn parse_sans(input: &str) -> Vec<String> {
    input
        .split(SAN_SEPARATOR)
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .map(str::to_owned)
        .collect()
}

/// The subject named as .NET says it, `CN=..., O=..., C=...`, the organisation and the
/// country left out when blank, as the C# `DistinguishedNameBuilder.Build`.
#[must_use]
pub fn distinguished_name_text(cn: &str, org: &str, country: &str) -> String {
    let mut parts = vec![format!("CN={cn}")];
    if !org.trim().is_empty() {
        parts.push(format!("O={org}"));
    }
    if !country.trim().is_empty() {
        parts.push(format!("C={country}"));
    }
    parts.join(NAME_SEPARATOR)
}

/// The subject as the C# encodes it: .NET reads `CN=..., O=..., C=...` most specific first
/// and writes it the other way round, the country first.
fn distinguished_name(cn: &str, org: &str, country: &str) -> DistinguishedName {
    let mut name = DistinguishedName::new();
    if !country.trim().is_empty() {
        let value = rcgen::string::PrintableString::try_from(country.to_owned()).map_or_else(
            |_| DnValue::Utf8String(country.to_owned()),
            DnValue::PrintableString,
        );
        name.push(DnType::CountryName, value);
    }
    if !org.trim().is_empty() {
        name.push(DnType::OrganizationName, org);
    }
    name.push(DnType::CommonName, cn);
    name
}

/// The SHA-256 fingerprint of a certificate's `der`, `SHA256:` then its bytes in upper-case
/// hexadecimal separated by colons, as the C# `CertificateFingerprint.ComputeSha256`.
#[must_use]
pub fn fingerprint_sha256(der: &[u8]) -> String {
    let hash = Sha256::digest(der);
    let bytes: Vec<String> = hash.iter().map(|byte| format!("{byte:02X}")).collect();
    format!("{FINGERPRINT_PREFIX}{}", bytes.join(FINGERPRINT_SEPARATOR))
}

/// Why no certificate was made, or exported.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum CertificateError {
    /// What went wrong, said as the C# says an exception's message.
    #[error("{0}")]
    Failed(String),
}

impl From<PemError> for CertificateError {
    fn from(error: PemError) -> Self {
        Self::Failed(error.to_string())
    }
}

impl From<PfxError> for CertificateError {
    fn from(error: PfxError) -> Self {
        Self::Failed(error.to_string())
    }
}

impl From<rcgen::Error> for CertificateError {
    fn from(error: rcgen::Error) -> Self {
        Self::Failed(error.to_string())
    }
}

/// A certificate made and its key.
pub struct IssuedCertificate {
    /// The certificate in PEM.
    pub cert_pem: String,
    /// The certificate's DER.
    pub cert_der: Vec<u8>,
    /// The private key in PKCS#8 PEM.
    pub key_pem: Zeroizing<String>,
    /// The private key's PKCS#8 DER.
    pub key_pkcs8: Zeroizing<Vec<u8>>,
}

impl fmt::Debug for IssuedCertificate {
    /// The private key is never written out.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("IssuedCertificate")
            .field("cert_pem", &self.cert_pem)
            .finish_non_exhaustive()
    }
}

/// A self-signed leaf, as the C# `SelfSignedCertificateResult`.
#[derive(Debug)]
pub struct SelfSignedCertificate {
    /// The leaf.
    pub leaf: IssuedCertificate,
    /// Its fingerprint.
    pub fingerprint: String,
}

/// A CA and the leaf it signed, as the C# `CaLeafCertificateResult`.
#[derive(Debug)]
pub struct CaLeafCertificates {
    /// The CA.
    pub ca: IssuedCertificate,
    /// The leaf.
    pub leaf: IssuedCertificate,
    /// The leaf's fingerprint, as the C# gives the leaf's and not the CA's.
    pub fingerprint: String,
}

/// An RSA key of `bits`, as rcgen signs with it and as its PKCS#8 DER.
fn new_key(bits: usize) -> Result<(KeyPair, Zeroizing<Vec<u8>>), CertificateError> {
    let key = super::rsa_keys::generate(bits).map_err(CertificateError::Failed)?;
    let der = Zeroizing::new(
        key.to_pkcs8_der()
            .map_err(|error| CertificateError::Failed(error.to_string()))?
            .as_bytes()
            .to_vec(),
    );
    let pair = KeyPair::from_pkcs8_der_and_sign_algo(&der.as_slice().into(), &PKCS_RSA_SHA256)?;
    Ok((pair, der))
}

/// A positive serial number of `length` random bytes, as the C# `NewPositiveSerial`.
fn new_serial(length: usize) -> Result<SerialNumber, CertificateError> {
    let mut serial = vec![0_u8; length];
    getrandom::fill(&mut serial).map_err(|error| CertificateError::Failed(error.to_string()))?;
    if let Some(first) = serial.first_mut() {
        *first &= POSITIVE_MSB_MASK;
        if *first == 0 {
            *first = NONZERO_FIRST_BYTE;
        }
    }
    Ok(SerialNumber::from_slice(&serial))
}

/// The parameters of a certificate named `cn`, valid `days` from `now` (Unix seconds).
fn base_params(
    options: &CertificateOptions,
    cn: &str,
    now: u64,
    days: u64,
) -> Result<CertificateParams, CertificateError> {
    let end = days
        .checked_mul(SECONDS_PER_DAY)
        .and_then(|span| now.checked_add(span))
        .filter(|end| *end <= LAST_X509_SECOND)
        .ok_or_else(|| {
            CertificateError::Failed("The validity ends past the year 9999.".to_owned())
        })?;
    let epoch = rcgen::date_time_ymd(1970, 1, 1);
    let mut params = CertificateParams::default();
    params.not_before = epoch + Duration::from_secs(now);
    params.not_after = epoch + Duration::from_secs(end);
    params.distinguished_name = distinguished_name(cn, &options.org, &options.country);
    params.key_identifier_method = rcgen::KeyIdMethod::PreSpecified(Vec::new());
    Ok(params)
}

/// The parameters of a leaf, as the C# requests one: not a CA, signature and key
/// encipherment, server and client authentication, its alternative names.
fn leaf_params(
    options: &CertificateOptions,
    now: u64,
    serial_length: usize,
) -> Result<CertificateParams, CertificateError> {
    let days = u64::try_from(options.validity_days).unwrap_or_default();
    let mut params = base_params(options, &options.cn, now, days)?;
    params.serial_number = Some(new_serial(serial_length)?);
    let mut constraints =
        CustomExtension::from_oid_content(OID_BASIC_CONSTRAINTS, LEAF_BASIC_CONSTRAINTS.to_vec());
    constraints.set_criticality(false);
    let mut usage = CustomExtension::from_oid_content(OID_KEY_USAGE, LEAF_KEY_USAGE.to_vec());
    usage.set_criticality(false);
    params.custom_extensions = vec![constraints, usage];
    params.extended_key_usages = vec![
        ExtendedKeyUsagePurpose::ServerAuth,
        ExtendedKeyUsagePurpose::ClientAuth,
    ];
    params.subject_alt_names = subject_alt_names(&options.sans)?;
    Ok(params)
}

/// The alternative names: an IP address where one parses, a DNS name otherwise, as the C#
/// `AddSans`.
fn subject_alt_names(sans: &[String]) -> Result<Vec<SanType>, CertificateError> {
    sans.iter()
        .map(|name| match name.parse::<IpAddr>() {
            Ok(address) => Ok(SanType::IpAddress(address)),
            Err(_) => Ok(SanType::DnsName(name.clone().try_into()?)),
        })
        .collect()
}

/// A certificate's PEM and DER, with its key's.
fn issued(
    certificate: &rcgen::Certificate,
    key_pkcs8: Zeroizing<Vec<u8>>,
) -> Result<IssuedCertificate, CertificateError> {
    let cert_der = certificate.der().to_vec();
    Ok(IssuedCertificate {
        cert_pem: pkcs8_pem::certificate_pem(&cert_der)?,
        key_pem: pkcs8_pem::private_key_pem(&key_pkcs8)?,
        cert_der,
        key_pkcs8,
    })
}

/// A self-signed leaf made from `options` at `now`, Unix seconds, as the C#
/// `GenerateSelfSigned`.
///
/// # Errors
///
/// [`CertificateError`] when the key, a name or a date cannot be made.
pub fn generate_self_signed(
    options: &CertificateOptions,
    now: u64,
) -> Result<SelfSignedCertificate, CertificateError> {
    let (key, der) = new_key(options.key_bits)?;
    let params = leaf_params(options, now, SELF_SIGNED_SERIAL_LENGTH)?;
    let certificate = params.self_signed(&key)?;
    let leaf = issued(&certificate, der)?;
    let fingerprint = fingerprint_sha256(&leaf.cert_der);
    Ok(SelfSignedCertificate { leaf, fingerprint })
}

/// A CA named after `options`' common name, valid `ca_validity_days`, and a leaf of
/// `options` it signs, both from `now`, Unix seconds, as the C# `GenerateCaLeafPair`.
///
/// # Errors
///
/// [`CertificateError`] when a key, a name or a date cannot be made.
pub fn generate_ca_leaf(
    options: &CertificateOptions,
    ca_validity_days: u32,
    now: u64,
) -> Result<CaLeafCertificates, CertificateError> {
    let (ca_key, ca_der) = new_key(options.key_bits)?;
    let ca_cn = format!("{}{CA_NAME_SUFFIX}", options.cn);
    let mut ca_params = base_params(options, &ca_cn, now, u64::from(ca_validity_days))?;
    ca_params.serial_number = Some(new_serial(SELF_SIGNED_SERIAL_LENGTH)?);
    let mut constraints =
        CustomExtension::from_oid_content(OID_BASIC_CONSTRAINTS, CA_BASIC_CONSTRAINTS.to_vec());
    constraints.set_criticality(true);
    let mut usage = CustomExtension::from_oid_content(OID_KEY_USAGE, CA_KEY_USAGE.to_vec());
    usage.set_criticality(true);
    ca_params.custom_extensions = vec![constraints, usage];
    let ca_certificate = ca_params.self_signed(&ca_key)?;
    let issuer = Issuer::new(ca_params, ca_key);
    let (leaf_key, leaf_der) = new_key(options.key_bits)?;
    let leaf_params = leaf_params(options, now, SERIAL_NUMBER_LENGTH)?;
    let leaf_certificate = leaf_params.signed_by(&leaf_key, &issuer)?;
    let leaf = issued(&leaf_certificate, leaf_der)?;
    let fingerprint = fingerprint_sha256(&leaf.cert_der);
    Ok(CaLeafCertificates {
        ca: issued(&ca_certificate, ca_der)?,
        leaf,
        fingerprint,
    })
}

/// The PFX of `leaf` and its key, sealed with `password`, as the C# `BuildPfx`: the leaf
/// alone, a CA's certificate never in it.
///
/// # Errors
///
/// [`CertificateError`] when the key cannot be encrypted.
pub fn build_pfx(leaf: &IssuedCertificate, password: &str) -> Result<Vec<u8>, CertificateError> {
    Ok(pkcs12::build(&leaf.cert_der, &leaf.key_pkcs8, password)?)
}

#[cfg(test)]
mod tests {
    use rsa::pkcs8::DecodePrivateKey as _;
    use rsa::traits::PublicKeyParts as _;
    use x509_cert::Certificate;
    use x509_cert::der::{Decode as _, DecodePem as _, Encode as _};
    use x509_cert::ext::pkix::name::GeneralName;
    use x509_cert::ext::pkix::{BasicConstraints, ExtendedKeyUsage, KeyUsage, SubjectAltName};

    use super::*;

    /// The C# tests' moment, 2026-01-02 03:04:05 UTC.
    const FIXED_NOW: u64 = 1_767_323_045;

    fn options(sans: &[&str]) -> CertificateOptions {
        CertificateOptions {
            cn: "server.local".to_owned(),
            org: "Heimdall".to_owned(),
            country: "FR".to_owned(),
            key_bits: RSA_2048_BITS,
            validity_days: 365,
            sans: sans.iter().map(|name| (*name).to_owned()).collect(),
        }
    }

    fn parsed(pem: &str) -> Certificate {
        Certificate::from_pem(pem.as_bytes()).expect("a certificate")
    }

    fn extension<'a>(
        certificate: &'a Certificate,
        oid: &str,
    ) -> Option<&'a x509_cert::ext::Extension> {
        certificate
            .tbs_certificate()
            .extensions()
            .and_then(|extensions| {
                extensions
                    .iter()
                    .find(|extension| extension.extn_id.to_string() == oid)
            })
    }

    fn sans_of(certificate: &Certificate) -> Option<Vec<String>> {
        let extension = extension(certificate, "2.5.29.17")?;
        let names = SubjectAltName::from_der(extension.extn_value.as_bytes()).expect("SAN");
        Some(
            names
                .0
                .iter()
                .map(|name| match name {
                    GeneralName::DnsName(dns) => dns.to_string(),
                    GeneralName::IpAddress(octets) => {
                        let bytes: [u8; 4] = octets.as_bytes().try_into().expect("IPv4");
                        std::net::Ipv4Addr::from(bytes).to_string()
                    }
                    other => format!("{other:?}"),
                })
                .collect(),
        )
    }

    #[test]
    fn options_are_validated_as_the_csharp() {
        assert_eq!(options(&[]).validate(), ValidationCode::Ok);
        let blank = CertificateOptions {
            cn: "   ".to_owned(),
            ..options(&[])
        };
        assert_eq!(blank.validate(), ValidationCode::CnRequired);
        for days in [0, -1] {
            let invalid = CertificateOptions {
                validity_days: days,
                ..options(&[])
            };
            assert_eq!(invalid.validate(), ValidationCode::InvalidValidity);
        }
        let bare = CertificateOptions {
            org: String::new(),
            country: String::new(),
            ..options(&[])
        };
        assert_eq!(bare.validate(), ValidationCode::Ok);
    }

    #[test]
    fn alternative_names_are_split_and_trimmed_as_the_csharp() {
        assert!(parse_sans("").is_empty());
        assert!(parse_sans("  ,   ").is_empty());
        assert_eq!(parse_sans("server.local"), ["server.local"]);
        assert_eq!(
            parse_sans("server.local,api.local,10.0.0.1"),
            ["server.local", "api.local", "10.0.0.1"]
        );
        assert_eq!(
            parse_sans(" server.local , api.local "),
            ["server.local", "api.local"]
        );
        assert_eq!(
            parse_sans("server.local, , ,,10.0.0.1"),
            ["server.local", "10.0.0.1"]
        );
    }

    #[test]
    fn the_subject_is_named_as_the_csharp() {
        assert_eq!(
            distinguished_name_text("server.local", "", ""),
            "CN=server.local"
        );
        assert_eq!(
            distinguished_name_text("server.local", "Heimdall", ""),
            "CN=server.local, O=Heimdall"
        );
        assert_eq!(
            distinguished_name_text("server.local", "Heimdall", "FR"),
            "CN=server.local, O=Heimdall, C=FR"
        );
        assert_eq!(
            distinguished_name_text("server.local", "   ", "FR"),
            "CN=server.local, C=FR"
        );
        assert_eq!(
            distinguished_name_text("server.local", "Heimdall", "   "),
            "CN=server.local, O=Heimdall"
        );
    }

    #[test]
    fn a_self_signed_leaf_parses_back_with_its_fields_dates_and_extensions() {
        let made =
            generate_self_signed(&options(&["server.local", "10.0.0.1"]), FIXED_NOW).expect("made");
        assert!(
            made.leaf
                .cert_pem
                .starts_with("-----BEGIN CERTIFICATE-----\n")
        );
        assert!(
            made.leaf
                .key_pem
                .starts_with("-----BEGIN PRIVATE KEY-----\n")
        );
        let certificate = parsed(&made.leaf.cert_pem);
        let tbs = certificate.tbs_certificate();
        assert_eq!(tbs.subject(), tbs.issuer(), "self-signed");
        assert_eq!(tbs.subject().to_string(), "CN=server.local,O=Heimdall,C=FR");
        let validity = tbs.validity();
        assert_eq!(validity.not_before.to_unix_duration().as_secs(), FIXED_NOW);
        assert_eq!(
            validity.not_after.to_unix_duration().as_secs(),
            FIXED_NOW + 365 * 86_400
        );
        assert_eq!(
            sans_of(&certificate).expect("SAN"),
            ["server.local", "10.0.0.1"]
        );
        let constraints = extension(&certificate, "2.5.29.19").expect("basic constraints");
        assert!(!constraints.critical);
        let constraints =
            BasicConstraints::from_der(constraints.extn_value.as_bytes()).expect("BC");
        assert!(!constraints.ca);
        let usage = extension(&certificate, "2.5.29.15").expect("key usage");
        assert!(!usage.critical);
        let usage = KeyUsage::from_der(usage.extn_value.as_bytes()).expect("KU");
        assert!(usage.digital_signature() && usage.key_encipherment());
        assert!(!usage.key_cert_sign());
        let eku = extension(&certificate, "2.5.29.37").expect("EKU");
        let eku = ExtendedKeyUsage::from_der(eku.extn_value.as_bytes()).expect("EKU");
        let purposes: Vec<String> = eku.0.iter().map(ToString::to_string).collect();
        assert_eq!(purposes, ["1.3.6.1.5.5.7.3.1", "1.3.6.1.5.5.7.3.2"]);
        assert_eq!(
            made.fingerprint,
            fingerprint_sha256(&certificate.to_der().expect("der"))
        );
        // The key is the certificate's.
        let key = rsa::RsaPrivateKey::from_pkcs8_der(&made.leaf.key_pkcs8).expect("key");
        assert_eq!(key.size() * 8, 2048);
        let public = tbs.subject_public_key_info().subject_public_key.raw_bytes();
        let ours =
            rsa::pkcs1::EncodeRsaPublicKey::to_pkcs1_der(&key.to_public_key()).expect("public");
        assert_eq!(public, ours.as_bytes());
    }

    #[test]
    fn a_leaf_without_names_has_no_alternative_names_extension() {
        let made = generate_self_signed(&options(&[]), FIXED_NOW).expect("made");
        assert!(sans_of(&parsed(&made.leaf.cert_pem)).is_none());
    }

    #[test]
    fn the_fingerprint_is_upper_hex_separated_by_colons() {
        let fingerprint = fingerprint_sha256(b"abc");
        assert_eq!(
            fingerprint,
            "SHA256:BA:78:16:BF:8F:01:CF:EA:41:41:40:DE:5D:AE:22:23:B0:03:61:A3:96:17:7A:9C:B4:10:FF:61:F2:00:15:AD"
        );
        assert!(!fingerprint.contains('-'));
    }

    #[test]
    fn a_ca_signs_its_leaf_and_says_it_is_one() {
        let made = generate_ca_leaf(&options(&[]), CA_VALIDITY_DAYS, FIXED_NOW).expect("made");
        let ca = parsed(&made.ca.cert_pem);
        let leaf = parsed(&made.leaf.cert_pem);
        assert_eq!(
            ca.tbs_certificate().subject(),
            ca.tbs_certificate().issuer()
        );
        assert!(
            ca.tbs_certificate()
                .subject()
                .to_string()
                .starts_with("CN=server.local CA")
        );
        assert_eq!(
            leaf.tbs_certificate().issuer(),
            ca.tbs_certificate().subject()
        );
        assert_eq!(
            ca.tbs_certificate()
                .validity()
                .not_after
                .to_unix_duration()
                .as_secs(),
            FIXED_NOW + 3650 * 86_400
        );
        let constraints = extension(&ca, "2.5.29.19").expect("basic constraints");
        assert!(constraints.critical);
        assert!(
            BasicConstraints::from_der(constraints.extn_value.as_bytes())
                .expect("BC")
                .ca
        );
        let usage = extension(&ca, "2.5.29.15").expect("key usage");
        assert!(usage.critical);
        let usage = KeyUsage::from_der(usage.extn_value.as_bytes()).expect("KU");
        assert!(usage.key_cert_sign() && usage.crl_sign());
        let leaf_constraints = extension(&leaf, "2.5.29.19").expect("basic constraints");
        assert!(
            !BasicConstraints::from_der(leaf_constraints.extn_value.as_bytes())
                .expect("BC")
                .ca
        );
        let serial = leaf.tbs_certificate().serial_number().as_bytes();
        assert_eq!(serial.len(), SERIAL_NUMBER_LENGTH);
        assert!(serial[0] & 0x80 == 0, "positive");
        assert_eq!(made.fingerprint, fingerprint_sha256(&made.leaf.cert_der));
        assert_ne!(made.fingerprint, fingerprint_sha256(&made.ca.cert_der));
        // The leaf's signature is the CA key's.
        let ca_key = rsa::RsaPrivateKey::from_pkcs8_der(&made.ca.key_pkcs8).expect("key");
        let verifying = rsa::pkcs1v15::VerifyingKey::<Sha256>::new(ca_key.to_public_key());
        let signature =
            rsa::pkcs1v15::Signature::try_from(leaf.signature().raw_bytes()).expect("signature");
        rsa::signature::Verifier::verify(
            &verifying,
            &leaf.tbs_certificate().to_der().expect("tbs"),
            &signature,
        )
        .expect("signed by the CA");
    }

    #[test]
    fn an_rsa_4096_leaf_has_a_4096_bit_key() {
        let made = generate_self_signed(
            &CertificateOptions {
                key_bits: RSA_4096_BITS,
                ..options(&[])
            },
            FIXED_NOW,
        )
        .expect("made");
        let key = rsa::RsaPrivateKey::from_pkcs8_der(&made.leaf.key_pkcs8).expect("key");
        assert_eq!(key.size() * 8, 4096);
    }

    #[test]
    fn a_validity_past_9999_is_refused() {
        let far = CertificateOptions {
            validity_days: 3_000_000,
            ..options(&[])
        };
        assert!(generate_self_signed(&far, FIXED_NOW).is_err());
    }

    #[test]
    fn a_pfx_holds_the_leaf_and_its_key_under_its_password() {
        let made = generate_ca_leaf(&options(&[]), CA_VALIDITY_DAYS, FIXED_NOW).expect("made");
        let pfx = build_pfx(&made.leaf, "secret").expect("pfx");
        let (certificate, key) = super::pkcs12::tests::open(&pfx, "secret").expect("opened");
        assert_eq!(certificate, made.leaf.cert_der);
        assert_eq!(key, made.leaf.key_pkcs8.as_slice());
        assert!(super::pkcs12::tests::open(&pfx, "").is_none());
        let empty = build_pfx(&made.leaf, "").expect("pfx");
        assert!(super::pkcs12::tests::open(&empty, "").is_some());
    }

    #[test]
    fn private_keys_are_never_written_out() {
        let made = generate_self_signed(&options(&[]), FIXED_NOW).expect("made");
        let shown = format!("{made:?}");
        assert!(!shown.contains("PRIVATE KEY"), "{shown}");
    }
}
