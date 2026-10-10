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

use std::fmt;
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
}
