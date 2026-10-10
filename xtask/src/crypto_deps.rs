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

//! Crypto dependency guard: no workspace crate but `sealvault` names a cryptographic crate
//! as a direct dependency.
//!
//! Cryptography comes from audited implementations, reached only through `sealvault`, so
//! that what the application may use is listed in one place. A crate that needs a hash, a
//! key or a random number asks `sealvault`; it never adds `sha2` or `ring` to its own
//! manifest. The few exceptions are written below, each with its reason: protocol plumbing
//! that a library takes whole, and test dependencies that read what `sealvault` made back
//! with independent parsers.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

/// The one crate allowed any of [`CRYPTO_CRATES`].
pub const SEALVAULT: &str = "sealvault";

/// Cryptographic crates, by their name on crates.io: every one `sealvault` wraps, and the
/// usual others a crate might reach for instead of it.
pub const CRYPTO_CRATES: [&str; 31] = [
    "aes",
    "aes-gcm",
    "argon2",
    "aws-lc-rs",
    "blake2",
    "blake3",
    "cbc",
    "chacha20poly1305",
    "des",
    "ed25519-dalek",
    "getrandom",
    "hkdf",
    "hmac",
    "md-5",
    "openssl",
    "p256",
    "p384",
    "pbkdf2",
    "pkcs8",
    "rand",
    "rand_core",
    "rcgen",
    "ring",
    "rsa",
    "scrypt",
    "sha1",
    "sha2",
    "sha3",
    "ssh-key",
    "subtle",
    "x509-cert",
];

/// Which table of a manifest a dependency is declared in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// `[dependencies]`, plain or for a target.
    Normal,
    /// `[dev-dependencies]`, plain or for a target.
    Dev,
    /// `[build-dependencies]`, plain or for a target.
    Build,
}

/// A direct dependency allowed outside `sealvault`, with why.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Allowed {
    /// The workspace crate declaring it.
    pub crate_name: &'static str,
    /// The cryptographic crate.
    pub dependency: &'static str,
    /// The table it is declared in.
    pub kind: Kind,
    /// Why it is not behind `sealvault`.
    pub reason: &'static str,
}

/// Every exception, with its reason.
pub const ALLOWED: [Allowed; 11] = [
    Allowed {
        crate_name: "heimdall-rdp",
        dependency: "x509-cert",
        kind: Kind::Normal,
        reason: "parses the server certificate's fields to show them and check its usage; \
                 no cryptographic operation",
    },
    Allowed {
        crate_name: "heimdall-core",
        dependency: "pkcs8",
        kind: Kind::Dev,
        reason: "the tests decrypt and read back the PKCS#8 keys sealvault wrote",
    },
    Allowed {
        crate_name: "heimdall-core",
        dependency: "rsa",
        kind: Kind::Dev,
        reason: "the tests read the RSA keys back and verify a CA's signature on its leaf",
    },
    Allowed {
        crate_name: "heimdall-core",
        dependency: "sha2",
        kind: Kind::Dev,
        reason: "the digest the tests verify the CA's RSA signature with",
    },
    Allowed {
        crate_name: "heimdall-core",
        dependency: "ssh-key",
        kind: Kind::Dev,
        reason: "the tests parse the OpenSSH lines and Ed25519 seeds the generator wrote",
    },
    Allowed {
        crate_name: "heimdall-core",
        dependency: "x509-cert",
        kind: Kind::Dev,
        reason: "the tests parse the certificates the generator made",
    },
    Allowed {
        crate_name: "heimdall-app",
        dependency: "rcgen",
        kind: Kind::Dev,
        reason: "test certificates for the TLS servers the tests run",
    },
    Allowed {
        crate_name: "heimdall-files",
        dependency: "rcgen",
        kind: Kind::Dev,
        reason: "the FTPS test server's certificate",
    },
    Allowed {
        crate_name: "heimdall-tls",
        dependency: "rcgen",
        kind: Kind::Dev,
        reason: "test certificates for the verifier's tests",
    },
    Allowed {
        crate_name: "heimdall-remote",
        dependency: "rcgen",
        kind: Kind::Dev,
        reason: "the VNC TLS test server's certificate",
    },
    Allowed {
        crate_name: "heimdall-ui",
        dependency: "rcgen",
        kind: Kind::Dev,
        reason: "the certificates the FTPS question's tests show",
    },
];

/// One direct dependency on a cryptographic crate, not allowed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    /// The manifest declaring it.
    pub manifest: PathBuf,
    /// The workspace crate.
    pub crate_name: String,
    /// The cryptographic crate.
    pub dependency: String,
    /// The table it is declared in.
    pub kind: Kind,
}

/// Result of a scan: what was found, and which manifests were read.
#[derive(Debug, Default)]
pub struct Scan {
    /// Dependencies refused.
    pub findings: Vec<Finding>,
    /// Manifests read.
    pub scanned: Vec<PathBuf>,
}

/// The direct dependencies on cryptographic crates in the manifest `text` of crate
/// `crate_name`, read from `manifest`, minus [`ALLOWED`]; nothing for `sealvault`.
///
/// # Errors
///
/// A manifest that is not TOML.
pub fn check_manifest(manifest: &Path, text: &str) -> io::Result<(String, Vec<Finding>)> {
    let document: toml::Table = text.parse().map_err(io::Error::other)?;
    let crate_name = document
        .get("package")
        .and_then(|package| package.get("name"))
        .and_then(toml::Value::as_str)
        .unwrap_or_default()
        .to_owned();
    if crate_name == SEALVAULT {
        return Ok((crate_name, Vec::new()));
    }
    let mut findings = Vec::new();
    let mut tables: Vec<(Kind, &toml::Table)> = Vec::new();
    collect_tables(&document, &mut tables);
    if let Some(targets) = document.get("target").and_then(toml::Value::as_table) {
        for target in targets.values().filter_map(toml::Value::as_table) {
            collect_tables(target, &mut tables);
        }
    }
    for (kind, table) in tables {
        for (key, value) in table {
            // A dependency renamed in its manifest names its crate in `package`.
            let dependency = value
                .get("package")
                .and_then(toml::Value::as_str)
                .unwrap_or(key);
            let allowed = ALLOWED.iter().any(|allowed| {
                allowed.crate_name == crate_name
                    && allowed.dependency == dependency
                    && allowed.kind == kind
            });
            if CRYPTO_CRATES.contains(&dependency) && !allowed {
                findings.push(Finding {
                    manifest: manifest.to_owned(),
                    crate_name: crate_name.clone(),
                    dependency: dependency.to_owned(),
                    kind,
                });
            }
        }
    }
    Ok((crate_name, findings))
}

/// The dependency tables of `table`, by kind.
fn collect_tables<'a>(table: &'a toml::Table, into: &mut Vec<(Kind, &'a toml::Table)>) {
    for (name, kind) in [
        ("dependencies", Kind::Normal),
        ("dev-dependencies", Kind::Dev),
        ("build-dependencies", Kind::Build),
    ] {
        if let Some(found) = table.get(name).and_then(toml::Value::as_table) {
            into.push((kind, found));
        }
    }
}

/// The manifests of the workspace at `root`: one per directory under `crates`, and the
/// `xtask` one. The root manifest declares versions, not dependencies of a crate.
///
/// # Errors
///
/// A directory or manifest that cannot be read.
pub fn manifests(root: &Path) -> io::Result<Vec<PathBuf>> {
    let mut found = Vec::new();
    for entry in fs::read_dir(root.join("crates"))? {
        let manifest = entry?.path().join("Cargo.toml");
        if manifest.is_file() {
            found.push(manifest);
        }
    }
    found.push(root.join("xtask").join("Cargo.toml"));
    found.sort();
    Ok(found)
}

/// Every manifest of the workspace at `root` checked.
///
/// # Errors
///
/// A manifest that cannot be read or is not TOML.
pub fn scan(root: &Path) -> io::Result<Scan> {
    let mut result = Scan::default();
    for manifest in manifests(root)? {
        let text = fs::read_to_string(&manifest)?;
        let (_, findings) = check_manifest(&manifest, &text)?;
        result.findings.extend(findings);
        result.scanned.push(manifest);
    }
    Ok(result)
}
