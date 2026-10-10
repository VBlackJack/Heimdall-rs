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

//! The crypto dependency guard run over this workspace: no crate but sealvault names a
//! cryptographic crate, and every manifest is read.

use std::fs;
use std::path::{Path, PathBuf};

use xtask::crypto_deps::{
    ALLOWED, CRYPTO_CRATES, Kind, SEALVAULT, check_manifest, manifests, scan,
};

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("xtask sits inside the workspace")
        .to_owned()
}

#[test]
fn no_crate_but_sealvault_depends_directly_on_a_crypto_crate() {
    let result = scan(&workspace_root()).expect("the manifests are readable");
    let report: Vec<String> = result
        .findings
        .iter()
        .map(|f| {
            format!(
                "{} ({}): {} as a {:?} dependency; ask sealvault instead",
                f.crate_name,
                f.manifest.display(),
                f.dependency,
                f.kind
            )
        })
        .collect();
    assert!(
        report.is_empty(),
        "direct crypto dependencies:\n{}",
        report.join("\n")
    );
}

#[test]
fn the_scan_reads_every_crate_manifest() {
    // A scan that silently read nothing would pass the test above. Every directory under
    // crates/ with a manifest is read, sealvault's and the xtask's among them.
    let root = workspace_root();
    let result = scan(&root).expect("the manifests are readable");
    let mut expected: Vec<PathBuf> = fs::read_dir(root.join("crates"))
        .expect("crates/")
        .map(|entry| entry.expect("entry").path().join("Cargo.toml"))
        .filter(|manifest| manifest.is_file())
        .collect();
    expected.push(root.join("xtask").join("Cargo.toml"));
    expected.sort();
    assert_eq!(result.scanned, expected);
    assert_eq!(manifests(&root).expect("listed"), expected);
    for name in [SEALVAULT, "heimdall-app", "heimdall-core"] {
        let manifest = root.join("crates").join(name).join("Cargo.toml");
        assert!(result.scanned.contains(&manifest), "{name} not read");
    }
}

#[test]
fn sealvault_itself_declares_crypto_crates_and_is_exempt() {
    // The positive control of the exemption: sealvault does declare them.
    let manifest = workspace_root()
        .join("crates")
        .join(SEALVAULT)
        .join("Cargo.toml");
    let text = fs::read_to_string(&manifest).expect("sealvault's manifest");
    assert!(text.contains("aes-gcm") && text.contains("sha2"));
    let (name, findings) = check_manifest(&manifest, &text).expect("TOML");
    assert_eq!(name, SEALVAULT);
    assert!(findings.is_empty());
}

#[test]
fn every_kind_of_declaration_is_caught() {
    let manifest = Path::new("probe/Cargo.toml");
    let text = r#"
[package]
name = "probe"

[dependencies]
ring.workspace = true
digest = { package = "sha2", version = "0.11" }
serde = "1"

[dev-dependencies]
hmac = "0.13"

[build-dependencies]
getrandom = "0.4"

[target.'cfg(windows)'.dependencies]
subtle = "2"
"#;
    let (name, findings) = check_manifest(manifest, text).expect("TOML");
    assert_eq!(name, "probe");
    let mut caught: Vec<(String, Kind)> = findings
        .into_iter()
        .map(|finding| (finding.dependency, finding.kind))
        .collect();
    caught.sort_by(|a, b| a.0.cmp(&b.0));
    assert_eq!(
        caught,
        [
            ("getrandom".to_owned(), Kind::Build),
            ("hmac".to_owned(), Kind::Dev),
            ("ring".to_owned(), Kind::Normal),
            ("sha2".to_owned(), Kind::Normal),
            ("subtle".to_owned(), Kind::Normal),
        ]
    );
}

#[test]
fn every_allowance_is_still_used_and_says_why() {
    // An allowance nothing needs any more is a hole left open: it must go.
    let root = workspace_root();
    for allowed in ALLOWED {
        assert!(
            CRYPTO_CRATES.contains(&allowed.dependency),
            "{} is not a crypto crate",
            allowed.dependency
        );
        assert!(!allowed.reason.trim().is_empty());
        let manifest = root
            .join("crates")
            .join(allowed.crate_name)
            .join("Cargo.toml");
        let text = fs::read_to_string(&manifest).expect("an allowed crate's manifest");
        let document: toml::Table = text.parse().expect("TOML");
        let table = match allowed.kind {
            Kind::Normal => "dependencies",
            Kind::Dev => "dev-dependencies",
            Kind::Build => "build-dependencies",
        };
        let declared = document
            .get(table)
            .and_then(toml::Value::as_table)
            .is_some_and(|dependencies| dependencies.contains_key(allowed.dependency));
        assert!(
            declared,
            "{} no longer declares {} in [{table}]",
            allowed.crate_name, allowed.dependency
        );
    }
}
