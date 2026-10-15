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

//! X.509 certificates as Heimdall issues them (RFC 5280), signed with RSA PKCS #1 v1.5 and
//! SHA-256: a TLS leaf, self-signed or signed by an authority, and that authority.
//!
//! The extensions are written as the C# Heimdall writes them, criticality included: a leaf
//! says it is no CA (not critical), may sign and encipher keys (not critical), serves and
//! authenticates TLS, and carries its alternative names; an authority says it is one and
//! signs certificates and revocation lists, both critical. No subject key identifier is
//! written, as .NET's `CertificateRequest` writes none unless asked.

use std::fmt::{self, Write as _};
use std::net::IpAddr;
use std::time::Duration;

use rcgen::{
    CertificateParams, CustomExtension, DistinguishedName, DnType, DnValue,
    ExtendedKeyUsagePurpose, Issuer, KeyPair, PKCS_RSA_SHA256, SanType, SerialNumber,
};
use zeroize::Zeroize as _;

use crate::Error;

/// OIDs of the extensions written as the C# writes them.
const OID_BASIC_CONSTRAINTS: &[u64] = &[2, 5, 29, 19];
const OID_KEY_USAGE: &[u64] = &[2, 5, 29, 15];

/// `BasicConstraints` of a leaf, `cA` false: an empty sequence.
const LEAF_BASIC_CONSTRAINTS: &[u8] = &[0x30, 0x00];

/// `BasicConstraints` of an authority: `cA` true, no path length.
const CA_BASIC_CONSTRAINTS: &[u8] = &[0x30, 0x03, 0x01, 0x01, 0xff];

/// `KeyUsage` of a leaf: `digitalSignature` and `keyEncipherment`.
const LEAF_KEY_USAGE: &[u8] = &[0x03, 0x02, 0x05, 0xa0];

/// `KeyUsage` of an authority: `keyCertSign` and `cRLSign`.
const CA_KEY_USAGE: &[u8] = &[0x03, 0x02, 0x01, 0x06];

/// The year X.509 times are counted from here, as Unix seconds count.
const EPOCH_YEAR: i32 = 1970;

/// The last second an X.509 time can say, 9999-12-31 23:59:59 UTC (RFC 5280 section
/// 4.1.2.5): no certificate is valid past it.
pub const LAST_X509_SECOND: u64 = 253_402_300_799;

/// The most bytes of a serial number (RFC 5280 section 4.1.2.2).
pub const MAX_SERIAL_LEN: usize = 20;

/// The sign bit of a serial number's first byte: set, the DER integer would be negative.
const SIGN_BIT: u8 = 0x80;

/// A subject's name: written country first, then organisation, then common name, as .NET
/// encodes `CN=..., O=..., C=...`. A country that is printable ASCII is a
/// `PrintableString`, as RFC 5280 asks; anything else UTF-8.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Name {
    /// The common name.
    pub common_name: String,
    /// The organisation, if any.
    pub organization: Option<String>,
    /// The country, if any.
    pub country: Option<String>,
}

/// An alternative name of a leaf.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AltName {
    /// A DNS name.
    Dns(String),
    /// An IP address.
    Ip(IpAddr),
}

/// What a certificate is for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Profile {
    /// A TLS server and client certificate with these alternative names.
    TlsLeaf(Vec<AltName>),
    /// An authority that signs certificates and revocation lists.
    Authority,
}

/// A certificate to issue.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Spec {
    /// Its subject.
    pub subject: Name,
    /// The first second it is valid, Unix time.
    pub not_before: u64,
    /// The last second it is valid, Unix time.
    pub not_after: u64,
    /// Its serial number's bytes, big-endian, written as given: positive is the caller's.
    pub serial: Vec<u8>,
    /// What it is for.
    pub profile: Profile,
}

/// An RSA key that signs certificates, made from its PKCS#8. Dropping it clears the copy
/// of the PKCS#8 rcgen keeps; the key ring parses from it for signing is ring's own and is
/// not wiped by ring.
pub struct SigningKey(KeyPair);

impl fmt::Debug for SigningKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("SigningKey(..)")
    }
}

impl SigningKey {
    /// The RSA key of PKCS#8 `der`, to sign with RSA PKCS #1 v1.5 and SHA-256.
    ///
    /// # Errors
    ///
    /// [`Error::Encoding`] when it is not an RSA key in PKCS#8.
    pub fn rsa_sha256(der: &[u8]) -> Result<Self, Error> {
        KeyPair::from_pkcs8_der_and_sign_algo(&der.into(), &PKCS_RSA_SHA256)
            .map(Self)
            .map_err(encoding)
    }
}

impl Drop for SigningKey {
    fn drop(&mut self) {
        self.0.zeroize();
    }
}

/// The DER of `spec`, signed by its own `key`.
///
/// # Errors
///
/// [`Error::Encoding`] when a name, a date or the signature cannot be made.
pub fn self_signed(spec: &Spec, key: &SigningKey) -> Result<Vec<u8>, Error> {
    let certificate = params(spec)?.self_signed(&key.0).map_err(encoding)?;
    Ok(certificate.der().to_vec())
}

/// The DER of `spec` for `key`, signed by `issuer_key`, the key of the authority `issuer`.
///
/// # Errors
///
/// [`Error::Encoding`] when `issuer` is not an [`Profile::Authority`], or a name, a date or
/// the signature cannot be made.
pub fn signed_by(
    spec: &Spec,
    key: &SigningKey,
    issuer: &Spec,
    issuer_key: &SigningKey,
) -> Result<Vec<u8>, Error> {
    if issuer.profile != Profile::Authority {
        return Err(Error::Encoding("the issuer is not an authority".to_owned()));
    }
    let issuer_params = params(issuer)?;
    let issuer = Issuer::from_params(&issuer_params, &issuer_key.0);
    let certificate = params(spec)?.signed_by(&key.0, &issuer).map_err(encoding)?;
    Ok(certificate.der().to_vec())
}

/// What is wrong with `spec`'s serial number and validity, before anything is signed: a
/// serial that is empty, zero, negative or past [`MAX_SERIAL_LEN`] bytes; a validity that
/// ends before it starts or past [`LAST_X509_SECOND`].
fn check(spec: &Spec) -> Result<(), Error> {
    let refused = |why: &str| Err(Error::Encoding(why.to_owned()));
    if spec.serial.is_empty() || spec.serial.len() > MAX_SERIAL_LEN {
        return refused("a serial number of 1 to 20 bytes");
    }
    if spec.serial.iter().all(|byte| *byte == 0) {
        return refused("a serial number that is not zero");
    }
    if spec.serial[0] & SIGN_BIT != 0 {
        return refused("a positive serial number");
    }
    if spec.not_after < spec.not_before {
        return refused("a validity that ends after it starts");
    }
    if spec.not_after > LAST_X509_SECOND {
        return refused("a validity that ends by the year 9999");
    }
    Ok(())
}

/// rcgen's parameters for `spec`, once [`check`]ed.
fn params(spec: &Spec) -> Result<CertificateParams, Error> {
    check(spec)?;
    let epoch = rcgen::date_time_ymd(EPOCH_YEAR, 1, 1);
    let mut params = CertificateParams::default();
    params.not_before = epoch + Duration::from_secs(spec.not_before);
    params.not_after = epoch + Duration::from_secs(spec.not_after);
    params.distinguished_name = distinguished_name(&spec.subject);
    params.key_identifier_method = rcgen::KeyIdMethod::PreSpecified(Vec::new());
    params.serial_number = Some(SerialNumber::from_slice(&spec.serial));
    match &spec.profile {
        Profile::TlsLeaf(names) => {
            params.custom_extensions = vec![
                extension(OID_BASIC_CONSTRAINTS, LEAF_BASIC_CONSTRAINTS, false),
                extension(OID_KEY_USAGE, LEAF_KEY_USAGE, false),
            ];
            params.extended_key_usages = vec![
                ExtendedKeyUsagePurpose::ServerAuth,
                ExtendedKeyUsagePurpose::ClientAuth,
            ];
            params.subject_alt_names = names
                .iter()
                .map(|name| match name {
                    AltName::Ip(address) => Ok(SanType::IpAddress(*address)),
                    AltName::Dns(dns) => {
                        Ok(SanType::DnsName(dns.clone().try_into().map_err(encoding)?))
                    }
                })
                .collect::<Result<_, Error>>()?;
        }
        Profile::Authority => {
            params.custom_extensions = vec![
                extension(OID_BASIC_CONSTRAINTS, CA_BASIC_CONSTRAINTS, true),
                extension(OID_KEY_USAGE, CA_KEY_USAGE, true),
            ];
        }
    }
    Ok(params)
}

fn extension(oid: &[u64], content: &[u8], critical: bool) -> CustomExtension {
    let mut extension = CustomExtension::from_oid_content(oid, content.to_vec());
    extension.set_criticality(critical);
    extension
}

/// The subject as .NET encodes it, the country first.
fn distinguished_name(name: &Name) -> DistinguishedName {
    let mut distinguished = DistinguishedName::new();
    if let Some(country) = &name.country {
        let value = rcgen::string::PrintableString::try_from(country.clone()).map_or_else(
            |_| DnValue::Utf8String(country.clone()),
            DnValue::PrintableString,
        );
        distinguished.push(DnType::CountryName, value);
    }
    if let Some(organization) = &name.organization {
        distinguished.push(DnType::OrganizationName, organization.as_str());
    }
    distinguished.push(DnType::CommonName, name.common_name.as_str());
    distinguished
}

/// A library's failure, said.
fn encoding(error: impl fmt::Display) -> Error {
    Error::Encoding(error.to_string())
}

/// Summary of an X.509 certificate's key properties, as the C# certificate inspector reads
/// them from an `X509Certificate2` (`CertInspectorEngine.BuildSummary`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CertificateSummary {
    /// The subject name.
    pub subject: String,
    /// The issuer name.
    pub issuer: String,
    /// The first moment it is valid, as the certificate writes it (RFC 3339, UTC).
    pub not_before: String,
    /// The expiration time formatted as a string (RFC 3339, UTC).
    pub not_after: String,
    /// The first second it is valid, Unix time.
    pub not_before_unix: u64,
    /// The last second it is valid, Unix time.
    pub not_after_unix: u64,
    /// The serial number, upper-case hexadecimal without separators, as .NET's
    /// `SerialNumber` shows it.
    pub serial: String,
    /// The signature algorithm, by the friendly name .NET gives it when it has one, else its
    /// dotted OID.
    pub signature_algorithm: String,
    /// The DNS names then the IP addresses of the subject alternative name extension, in
    /// that order, as the C# `ExtractSans` lists them.
    pub alt_names: Vec<String>,
    /// The public key size in bits, or 0 if unsupported/unknown.
    pub key_size_bits: usize,
}

/// Signature algorithm OIDs and the friendly names .NET's `Oid.FriendlyName` gives them.
const SIGNATURE_ALGORITHMS: &[(&str, &str)] = &[
    ("1.2.840.113549.1.1.4", "md5RSA"),
    ("1.2.840.113549.1.1.5", "sha1RSA"),
    ("1.2.840.113549.1.1.10", "RSASSA-PSS"),
    ("1.2.840.113549.1.1.11", "sha256RSA"),
    ("1.2.840.113549.1.1.12", "sha384RSA"),
    ("1.2.840.113549.1.1.13", "sha512RSA"),
    ("1.2.840.10045.4.1", "sha1ECDSA"),
    ("1.2.840.10045.4.3.2", "sha256ECDSA"),
    ("1.2.840.10045.4.3.3", "sha384ECDSA"),
    ("1.2.840.10045.4.3.4", "sha512ECDSA"),
    ("1.3.101.112", "Ed25519"),
    ("1.3.101.113", "Ed448"),
];

/// Bytes of an IPv4 address in a subject alternative name.
const IPV4_LEN: usize = 4;

/// Bytes of an IPv6 address in a subject alternative name.
const IPV6_LEN: usize = 16;


/// OID content octets (no tag or length) of the public key algorithms whose key size is
/// decided by the algorithm, not by the length of the public key bytes.
const OID_RSA_ENCRYPTION: &[u8] = &[0x2A, 0x86, 0x48, 0x86, 0xF7, 0x0D, 0x01, 0x01, 0x01];
const OID_RSASSA_PSS: &[u8] = &[0x2A, 0x86, 0x48, 0x86, 0xF7, 0x0D, 0x01, 0x01, 0x0A];
const OID_EC_PUBLIC_KEY: &[u8] = &[0x2A, 0x86, 0x48, 0xCE, 0x3D, 0x02, 0x01];
const OID_ED25519: &[u8] = &[0x2B, 0x65, 0x70];
const OID_ED448: &[u8] = &[0x2B, 0x65, 0x71];
const OID_DSA: &[u8] = &[0x2A, 0x86, 0x48, 0xCE, 0x38, 0x04, 0x01];

/// EC named-curve OID content octets and the field size they denote.
const OID_P256: &[u8] = &[0x2A, 0x86, 0x48, 0xCE, 0x3D, 0x03, 0x01, 0x07];
const OID_P384: &[u8] = &[0x2B, 0x81, 0x04, 0x00, 0x22];
const OID_P521: &[u8] = &[0x2B, 0x81, 0x04, 0x00, 0x23];

/// Key size in bits of Ed25519 and Ed448, which carry no size parameter.
const ED25519_BITS: usize = 256;
const ED448_BITS: usize = 456;

/// Inspects a DER-encoded X.509 certificate and extracts a summary of its key properties;
/// `None` when it is not a certificate.
#[must_use]
pub fn inspect(der: &[u8]) -> Option<CertificateSummary> {
    use rsa::pkcs1::DecodeRsaPublicKey as _;
    use rsa::traits::PublicKeyParts as _;
    use x509_cert::Certificate;
    use x509_cert::der::{Decode as _, Encode as _};

    let cert = Certificate::from_der(der).ok()?;
    let tbs = cert.tbs_certificate();
    let subject = tbs.subject().to_string();
    let issuer = tbs.issuer().to_string();
    let validity = tbs.validity();
    let serial = tbs
        .serial_number()
        .as_bytes()
        .iter()
        .fold(String::new(), |mut hex, byte| {
            let _ = write!(hex, "{byte:02X}");
            hex
        });
    let signature_oid = cert.signature_algorithm().oid.to_string();
    let signature_algorithm = SIGNATURE_ALGORITHMS
        .iter()
        .find(|(oid, _)| *oid == signature_oid)
        .map_or_else(|| signature_oid.clone(), |(_, name)| (*name).to_owned());
    let spki = tbs.subject_public_key_info();

    // The key size is decided by the algorithm OID (and, for EC and DSA, the algorithm
    // parameters), never by the raw byte length of the public key: a point encoding's
    // byte count does not reliably map to a curve, and an RSA modulus is read exactly.
    let oid = spki.algorithm.oid.as_bytes();
    let params_der = spki
        .algorithm
        .parameters
        .as_ref()
        .and_then(|p| p.to_der().ok());

    let key_size_bits = if oid == OID_RSA_ENCRYPTION || oid == OID_RSASSA_PSS {
        spki.subject_public_key
            .as_bytes()
            .and_then(|bytes| rsa::RsaPublicKey::from_pkcs1_der(bytes).ok())
            .map_or(0, |k| k.size() * 8)
    } else if oid == OID_ED25519 {
        ED25519_BITS
    } else if oid == OID_ED448 {
        ED448_BITS
    } else if oid == OID_EC_PUBLIC_KEY {
        ec_curve_bits(params_der.as_deref())
    } else if oid == OID_DSA {
        dsa_prime_bits(params_der.as_deref())
    } else {
        0
    };

    Some(CertificateSummary {
        subject,
        issuer,
        not_before: validity.not_before.to_string(),
        not_after: validity.not_after.to_string(),
        not_before_unix: validity.not_before.to_unix_duration().as_secs(),
        not_after_unix: validity.not_after.to_unix_duration().as_secs(),
        serial,
        signature_algorithm,
        alt_names: alt_names(tbs),
        key_size_bits,
    })
}

/// The DNS names, then the IP addresses, of `tbs`'s subject alternative name extension;
/// none when it has none or it does not decode, as the C# reads only what decodes.
fn alt_names(tbs: &x509_cert::TbsCertificate) -> Vec<String> {
    use x509_cert::ext::pkix::SubjectAltName;
    use x509_cert::ext::pkix::name::GeneralName;

    let Ok(Some((_, SubjectAltName(names)))) = tbs.get_extension::<SubjectAltName>() else {
        return Vec::new();
    };
    let dns = names.iter().filter_map(|name| match name {
        GeneralName::DnsName(dns) => Some(dns.to_string()),
        _ => None,
    });
    let addresses = names.iter().filter_map(|name| match name {
        GeneralName::IpAddress(bytes) => ip_address(bytes.as_bytes()).map(|ip| ip.to_string()),
        _ => None,
    });
    dns.chain(addresses).collect()
}

/// The address of an IP address alternative name's `bytes`: four for IPv4, sixteen for IPv6.
fn ip_address(bytes: &[u8]) -> Option<IpAddr> {
    match bytes.len() {
        IPV4_LEN => <[u8; IPV4_LEN]>::try_from(bytes).ok().map(IpAddr::from),
        IPV6_LEN => <[u8; IPV6_LEN]>::try_from(bytes).ok().map(IpAddr::from),
        _ => None,
    }
}

/// The field size of the EC named curve in `params` (a DER `OBJECT IDENTIFIER`), or 0 when
/// the curve is absent or unrecognised.
fn ec_curve_bits(params: Option<&[u8]>) -> usize {
    match read_der_tlv(params.unwrap_or(&[])) {
        Some((0x06, curve_oid, _)) if curve_oid == OID_P256 => 256,
        Some((0x06, curve_oid, _)) if curve_oid == OID_P384 => 384,
        Some((0x06, curve_oid, _)) if curve_oid == OID_P521 => 521,
        _ => 0,
    }
}

/// The bit length of the DSA prime `p` from `params` (a DER `SEQUENCE { p, q, g }`), which
/// is the DSA key size, or 0 when the parameters cannot be read.
fn dsa_prime_bits(params: Option<&[u8]>) -> usize {
    let Some((0x30, seq, _)) = read_der_tlv(params.unwrap_or(&[])) else {
        return 0;
    };
    match read_der_tlv(seq) {
        Some((0x02, p, _)) => der_integer_bit_length(p),
        _ => 0,
    }
}

/// Reads one DER tag-length-value from `input`, returning `(tag, value, rest)`.
fn read_der_tlv(input: &[u8]) -> Option<(u8, &[u8], &[u8])> {
    if input.len() < 2 {
        return None;
    }
    let tag = input[0];
    let len_byte = input[1];
    let mut idx = 2;
    let length = if len_byte & 0x80 == 0 {
        len_byte as usize
    } else {
        let octets = (len_byte & 0x7F) as usize;
        if octets == 0 || octets > 4 || idx + octets > input.len() {
            return None;
        }
        let mut len = 0usize;
        for &b in &input[idx..idx + octets] {
            len = (len << 8) | (b as usize);
        }
        idx += octets;
        len
    };
    if idx + length > input.len() {
        return None;
    }
    Some((tag, &input[idx..idx + length], &input[idx + length..]))
}

/// The bit length of a DER `INTEGER`'s value octets, leading sign and zero bytes removed.
fn der_integer_bit_length(value: &[u8]) -> usize {
    let mut offset = 0;
    while offset < value.len() && value[offset] == 0 {
        offset += 1;
    }
    if offset >= value.len() {
        return 0;
    }
    let remaining = value.len() - offset;
    let mut top = value[offset];
    let mut top_bits = 0;
    while top > 0 {
        top_bits += 1;
        top >>= 1;
    }
    (remaining - 1) * 8 + top_bits
}

#[cfg(test)]
mod tests {
    use rsa::pkcs1::EncodeRsaPublicKey as _;
    use rsa::pkcs8::DecodePrivateKey as _;
    use x509_cert::Certificate;
    use x509_cert::der::{Decode as _, Encode as _};

    use super::{
        AltName, LAST_X509_SECOND, MAX_SERIAL_LEN, Name, Profile, SigningKey, Spec, self_signed,
        signed_by,
    };
    use crate::Error;
    use crate::keys::KeyPair;

    /// Bits of the test keys.
    const RSA_BITS: usize = 2048;

    /// 2026-01-02 03:04:05 UTC.
    const NOW: u64 = 1_767_323_045;

    /// Seconds in a year of the tests.
    const YEAR: u64 = 365 * 86_400;

    fn spec(cn: &str, profile: Profile) -> Spec {
        Spec {
            subject: Name {
                common_name: cn.to_owned(),
                organization: Some("Heimdall".to_owned()),
                country: Some("FR".to_owned()),
            },
            not_before: NOW,
            not_after: NOW + YEAR,
            serial: vec![0x01, 0x02, 0x03],
            profile,
        }
    }

    fn key() -> (KeyPair, SigningKey) {
        let pair = KeyPair::rsa(RSA_BITS).expect("made");
        let signing = SigningKey::rsa_sha256(pair.pkcs8_der()).expect("RSA");
        (pair, signing)
    }

    #[test]
    fn a_self_signed_leaf_is_der_and_signed_twice_alike() {
        let (_, key) = key();
        let leaf = spec(
            "server.local",
            Profile::TlsLeaf(vec![
                AltName::Dns("server.local".to_owned()),
                AltName::Ip("10.0.0.1".parse().expect("IP")),
            ]),
        );
        let first = self_signed(&leaf, &key).expect("signed");
        // A SEQUENCE, and RSA PKCS #1 v1.5 is deterministic: the same key and spec, the
        // same bytes.
        assert_eq!(first[0], 0x30);
        assert_eq!(self_signed(&leaf, &key).expect("signed"), first);
    }

    #[test]
    fn an_authority_signs_a_leaf() {
        let (_, ca_key) = key();
        let (_, leaf_key) = key();
        let authority = spec("server.local CA", Profile::Authority);
        let leaf = spec("server.local", Profile::TlsLeaf(Vec::new()));
        let der = signed_by(&leaf, &leaf_key, &authority, &ca_key).expect("signed");
        assert_eq!(der[0], 0x30);
    }

    #[test]
    fn a_leaf_carries_its_own_key_and_the_authority_signs_it() {
        let (ca_pair, ca_key) = key();
        let (leaf_pair, leaf_key) = key();
        let authority = spec("server.local CA", Profile::Authority);
        let leaf = spec("server.local", Profile::TlsLeaf(Vec::new()));
        let der = signed_by(&leaf, &leaf_key, &authority, &ca_key).expect("signed");
        let certificate = Certificate::from_der(&der).expect("X.509");
        let tbs = certificate.tbs_certificate();
        // The leaf's public key is the leaf key's, not the authority's.
        let leaf_public = rsa::RsaPrivateKey::from_pkcs8_der(leaf_pair.pkcs8_der())
            .expect("PKCS#8")
            .to_public_key()
            .to_pkcs1_der()
            .expect("PKCS#1");
        assert_eq!(
            tbs.subject_public_key_info().subject_public_key.raw_bytes(),
            leaf_public.as_bytes()
        );
        // Its signature is the authority key's.
        let ca_public = rsa::RsaPrivateKey::from_pkcs8_der(ca_pair.pkcs8_der())
            .expect("PKCS#8")
            .to_public_key();
        let verifying = rsa::pkcs1v15::VerifyingKey::<sha2::Sha256>::new(ca_public);
        let signature = rsa::pkcs1v15::Signature::try_from(certificate.signature().raw_bytes())
            .expect("a signature");
        rsa::signature::Verifier::verify(&verifying, &tbs.to_der().expect("TBS"), &signature)
            .expect("signed by the authority");
        let leaf_verifying = rsa::pkcs1v15::VerifyingKey::<sha2::Sha256>::new(
            rsa::RsaPrivateKey::from_pkcs8_der(leaf_pair.pkcs8_der())
                .expect("PKCS#8")
                .to_public_key(),
        );
        assert!(
            rsa::signature::Verifier::verify(
                &leaf_verifying,
                &tbs.to_der().expect("TBS"),
                &signature
            )
            .is_err(),
            "not by the leaf's own key"
        );
    }

    #[test]
    fn only_an_authority_signs_a_leaf() {
        let (_, key) = key();
        let leaf = spec("server.local", Profile::TlsLeaf(Vec::new()));
        assert!(matches!(
            signed_by(&leaf, &key, &leaf, &key),
            Err(Error::Encoding(_))
        ));
    }

    #[test]
    fn a_serial_or_validity_rfc_5280_refuses_is_refused_before_signing() {
        let (_, key) = key();
        let refused = |change: fn(&mut Spec)| {
            let mut leaf = spec("server.local", Profile::TlsLeaf(Vec::new()));
            change(&mut leaf);
            matches!(self_signed(&leaf, &key), Err(Error::Encoding(_)))
        };
        assert!(refused(|spec| spec.serial.clear()), "empty");
        assert!(refused(|spec| spec.serial = vec![0; 4]), "zero");
        assert!(refused(|spec| spec.serial = vec![0x80, 1]), "negative");
        assert!(
            refused(|spec| spec.serial = vec![1; MAX_SERIAL_LEN + 1]),
            "past 20 bytes"
        );
        assert!(
            refused(|spec| spec.not_after = spec.not_before - 1),
            "ends before it starts"
        );
        assert!(
            refused(|spec| spec.not_after = LAST_X509_SECOND + 1),
            "past 9999"
        );
        assert!(refused(|spec| spec.not_after = u64::MAX), "no overflow");
        assert!(
            !refused(|spec| spec.serial = vec![1; MAX_SERIAL_LEN]),
            "20 bytes"
        );
        assert!(
            !refused(|spec| spec.not_after = LAST_X509_SECOND),
            "the last second"
        );
        assert!(
            !refused(|spec| spec.not_after = spec.not_before),
            "one second"
        );
    }

    #[test]
    fn a_key_that_is_not_rsa_pkcs8_is_refused_and_debug_shows_nothing() {
        assert!(SigningKey::rsa_sha256(b"not a key").is_err());
        let ed25519 = KeyPair::ed25519().expect("made");
        assert!(SigningKey::rsa_sha256(ed25519.pkcs8_der()).is_err());
        let (_, key) = key();
        assert_eq!(format!("{key:?}"), "SigningKey(..)");
    }

    #[test]
    fn a_dns_name_that_is_not_ia5_is_refused() {
        let (_, key) = key();
        let leaf = spec(
            "server.local",
            Profile::TlsLeaf(vec![AltName::Dns("s\u{e9}rveur".to_owned())]),
        );
        assert!(self_signed(&leaf, &key).is_err());
    }

    #[test]
    fn inspect_der_certificate() {
        let (_, key) = key();
        let leaf = spec("server.local", Profile::TlsLeaf(Vec::new()));
        let der = self_signed(&leaf, &key).expect("signed");
        let summary = super::inspect(&der).expect("inspected");
        assert!(summary.subject.contains("server.local"));
        // RSA 2048 is read from the modulus, decided by the rsaEncryption OID.
        assert_eq!(summary.key_size_bits, 2048);
    }

    #[test]
    fn inspect_reads_the_dates_serial_algorithm_and_alternative_names_written() {
        let (_, key) = key();
        let mut leaf = spec(
            "server.local",
            Profile::TlsLeaf(vec![
                AltName::Ip("10.0.0.1".parse().expect("IP")),
                AltName::Dns("server.local".to_owned()),
                AltName::Ip("2001:db8::1".parse().expect("IP")),
                AltName::Dns("*.server.local".to_owned()),
            ]),
        );
        leaf.serial = vec![0x01, 0xAB, 0x0C];
        let der = self_signed(&leaf, &key).expect("signed");
        let summary = super::inspect(&der).expect("inspected");
        assert_eq!(summary.not_before_unix, NOW);
        assert_eq!(summary.not_after_unix, NOW + YEAR);
        assert_eq!(summary.not_before, "2026-01-02T03:04:05Z");
        assert_eq!(summary.serial, "01AB0C");
        assert_eq!(summary.signature_algorithm, "sha256RSA");
        assert_eq!(
            summary.alt_names,
            ["server.local", "*.server.local", "10.0.0.1", "2001:db8::1"],
            "the DNS names first, then the addresses, as the C#"
        );
    }

    #[test]
    fn inspect_lists_no_alternative_name_when_there_is_none_and_refuses_what_is_no_certificate() {
        let (_, key) = key();
        let der = self_signed(&spec("ca", Profile::Authority), &key).expect("signed");
        assert!(
            super::inspect(&der)
                .expect("inspected")
                .alt_names
                .is_empty()
        );
        assert!(super::inspect(b"not a certificate").is_none());
        assert!(super::inspect(&der[..der.len() / 2]).is_none(), "cut short");
    }

    #[test]
    fn ec_curve_bits_decided_by_named_curve_oid() {
        // OBJECT IDENTIFIER TLVs for the three supported curves.
        let p256 = [0x06, 0x08, 0x2A, 0x86, 0x48, 0xCE, 0x3D, 0x03, 0x01, 0x07];
        let p384 = [0x06, 0x05, 0x2B, 0x81, 0x04, 0x00, 0x22];
        let p521 = [0x06, 0x05, 0x2B, 0x81, 0x04, 0x00, 0x23];
        assert_eq!(super::ec_curve_bits(Some(&p256)), 256);
        assert_eq!(super::ec_curve_bits(Some(&p384)), 384);
        assert_eq!(super::ec_curve_bits(Some(&p521)), 521);
        // An unknown curve or absent parameters yields 0, not a byte-length guess.
        let unknown = [0x06, 0x03, 0x2B, 0x65, 0x70];
        assert_eq!(super::ec_curve_bits(Some(&unknown)), 0);
        assert_eq!(super::ec_curve_bits(None), 0);
    }

    #[test]
    fn der_integer_bit_length_strips_sign_and_zero_bytes() {
        assert_eq!(super::der_integer_bit_length(&[0x00]), 0);
        assert_eq!(super::der_integer_bit_length(&[0x01]), 1);
        assert_eq!(super::der_integer_bit_length(&[0xFF]), 8);
        // A DER positive integer with a leading 0x00 sign byte: 0x80 0x00 is 16 bits.
        assert_eq!(super::der_integer_bit_length(&[0x00, 0x80, 0x00]), 16);
    }

    #[test]
    fn dsa_prime_bits_reads_prime_from_parameters() {
        // SEQUENCE { INTEGER p (256-bit), INTEGER q = 2, INTEGER g = 2 }.
        let mut p_value = vec![0x00, 0x80];
        p_value.extend_from_slice(&[0x00; 31]); // 0x00 sign byte + 32 bytes, top bit set -> 256 bits
        let mut p_int = vec![0x02, u8::try_from(p_value.len()).expect("short length")];
        p_int.extend_from_slice(&p_value);
        let q_int = [0x02, 0x01, 0x02];
        let g_int = [0x02, 0x01, 0x02];

        let mut content = p_int;
        content.extend_from_slice(&q_int);
        content.extend_from_slice(&g_int);
        let mut seq = vec![0x30, u8::try_from(content.len()).expect("short length")];
        seq.extend_from_slice(&content);

        assert_eq!(super::dsa_prime_bits(Some(&seq)), 256);
        assert_eq!(super::dsa_prime_bits(None), 0);
    }
}
