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

//! Digests as the C# `HashComputer`, `HashAlgorithmCatalog` and `HashVerifier`
//! (`Heimdall.Core/Hashing`) compute and compare them: MD5, SHA-1, SHA-256, SHA-384, SHA-512
//! and SHA3-256, written in lower-case hexadecimal; a file read once for all of them, at most
//! 50 MB, as the C# `HashGeneratorService`; a hash pasted to check found by its length first.
//!
//! The C# offers SHA3-256 only where Windows has it (`SHA3_256.IsSupported`); it is computed
//! here on every system.

use std::fs::File;
use std::io::{self, Read};
use std::path::Path;

use data_encoding::HEXLOWER;
use sealvault::hash;

/// The largest file hashed, as the C# `HashGeneratorService.MaxFileSizeBytes`: 50 MB.
pub const MAX_FILE_BYTES: u64 = 50 * 1024 * 1024;

/// Bytes read at a time from a file, as the C# `HashComputer.StreamChunkSize`.
pub const STREAM_CHUNK_BYTES: usize = 81_920;

/// A digest the Hash Generator computes, as the C# `HashAlgorithmKind`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum HashAlgorithm {
    /// MD5.
    Md5,
    /// SHA-1.
    Sha1,
    /// SHA-256.
    Sha256,
    /// SHA-384.
    Sha384,
    /// SHA-512.
    Sha512,
    /// SHA3-256.
    Sha3_256,
}

impl HashAlgorithm {
    /// Every one, in the C# `HashAlgorithmCatalog.AllKinds` order (`HashAlgorithmCatalog.cs:23-31`).
    pub const ALL: [Self; 6] = [
        Self::Md5,
        Self::Sha1,
        Self::Sha256,
        Self::Sha384,
        Self::Sha512,
        Self::Sha3_256,
    ];

    /// Its name, as the C# `HashAlgorithmCatalog.DisplayName` (`HashAlgorithmCatalog.cs:51-60`).
    #[must_use]
    pub const fn display_name(self) -> &'static str {
        match self {
            Self::Md5 => "MD5",
            Self::Sha1 => "SHA1",
            Self::Sha256 => "SHA256",
            Self::Sha384 => "SHA384",
            Self::Sha512 => "SHA512",
            Self::Sha3_256 => "SHA3-256",
        }
    }

    /// The same digest in sealvault, which computes it.
    #[must_use]
    pub const fn sealed(self) -> hash::Algorithm {
        match self {
            Self::Md5 => hash::Algorithm::Md5,
            Self::Sha1 => hash::Algorithm::Sha1,
            Self::Sha256 => hash::Algorithm::Sha256,
            Self::Sha384 => hash::Algorithm::Sha384,
            Self::Sha512 => hash::Algorithm::Sha512,
            Self::Sha3_256 => hash::Algorithm::Sha3_256,
        }
    }

    /// Characters of its hexadecimal digest, as the C# `HashAlgorithmCatalog.HexLength`.
    #[must_use]
    pub const fn hex_length(self) -> usize {
        match self {
            Self::Md5 => 32,
            Self::Sha1 => 40,
            Self::Sha256 | Self::Sha3_256 => 64,
            Self::Sha384 => 96,
            Self::Sha512 => 128,
        }
    }
}

/// A digest being computed, fed piece by piece.
#[derive(Clone)]
struct Hasher(hash::Hasher);

impl Hasher {
    fn new(kind: HashAlgorithm) -> Self {
        Self(hash::Hasher::new(kind.sealed()))
    }

    fn update(&mut self, data: &[u8]) {
        self.0.update(data);
    }

    /// The digest in lower-case hexadecimal, as .NET's `Convert.ToHexStringLower`.
    fn finish(self) -> String {
        HEXLOWER.encode(self.0.finalize().as_bytes())
    }
}

/// The digests of one input, by algorithm, in the order they were asked.
pub type Digests = Vec<(HashAlgorithm, String)>;

/// The digest of `data` by `kind`, in lower-case hexadecimal, as the C#
/// `HashComputer.Compute` (`HashComputer.cs:24-44`).
#[must_use]
pub fn compute(kind: HashAlgorithm, data: &[u8]) -> String {
    let mut hasher = Hasher::new(kind);
    hasher.update(data);
    hasher.finish()
}

/// Every digest of `data`, in the [`HashAlgorithm::ALL`] order, as the C#
/// `HashGeneratorService.ComputeTextHashesAsync` (`HashGeneratorService.cs:27-49`).
#[must_use]
pub fn compute_all(data: &[u8]) -> Digests {
    HashAlgorithm::ALL
        .into_iter()
        .map(|kind| (kind, compute(kind, data)))
        .collect()
}

/// Every digest of what `reader` gives, read once by [`STREAM_CHUNK_BYTES`], as the C#
/// `HashComputer.ComputeStreamMultiAsync` (`HashComputer.cs:46-113`): `progress` is told the
/// bytes read so far after each piece, and once with none for an empty stream.
///
/// # Errors
///
/// What reading `reader` failed with.
pub fn compute_stream(mut reader: impl Read, mut progress: impl FnMut(u64)) -> io::Result<Digests> {
    let mut hashers: Vec<(HashAlgorithm, Hasher)> = HashAlgorithm::ALL
        .into_iter()
        .map(|kind| (kind, Hasher::new(kind)))
        .collect();
    let mut buffer = vec![0_u8; STREAM_CHUNK_BYTES];
    let mut total: u64 = 0;
    loop {
        let read = match reader.read(&mut buffer) {
            Ok(0) => break,
            Ok(read) => read,
            Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
            Err(error) => return Err(error),
        };
        for (_, hasher) in &mut hashers {
            hasher.update(&buffer[..read]);
        }
        total += read as u64;
        progress(total);
    }
    if total == 0 {
        progress(0);
    }
    Ok(hashers
        .into_iter()
        .map(|(kind, hasher)| (kind, hasher.finish()))
        .collect())
}

/// Why a file was not hashed, as the C# view model tells the exceptions of
/// `HashGeneratorService.ComputeFileHashesAsync` apart (`HashGeneratorViewModel.cs:157-213`).
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum HashFileError {
    /// There is no such file.
    #[error("file not found")]
    NotFound,
    /// It is larger than [`MAX_FILE_BYTES`].
    #[error("file larger than {limit} bytes")]
    TooLarge {
        /// The limit it passes.
        limit: u64,
    },
    /// Reading it is not allowed.
    #[error("access denied")]
    AccessDenied,
    /// Anything else, said as the system says it.
    #[error("{0}")]
    Failed(String),
}

impl From<io::Error> for HashFileError {
    fn from(error: io::Error) -> Self {
        match error.kind() {
            io::ErrorKind::NotFound => Self::NotFound,
            io::ErrorKind::PermissionDenied => Self::AccessDenied,
            _ => Self::Failed(error.to_string()),
        }
    }
}

/// A file's digests, and its size.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileDigests {
    /// Every digest, in the [`HashAlgorithm::ALL`] order.
    pub digests: Digests,
    /// Its size, in bytes.
    pub size: u64,
}

/// Every digest of the file at `path`, as the C# `HashGeneratorService.ComputeFileHashesAsync`
/// (`HashGeneratorService.cs:51-95`): refused when missing or past [`MAX_FILE_BYTES`];
/// `progress` is told the share read, from 0 to 100, after each piece.
///
/// # Errors
///
/// [`HashFileError`] when the file is missing, too large, not readable, or fails midway.
pub fn compute_file(
    path: &Path,
    mut progress: impl FnMut(f64),
) -> Result<FileDigests, HashFileError> {
    let metadata = std::fs::metadata(path)?;
    if !metadata.is_file() {
        return Err(HashFileError::NotFound);
    }
    let size = metadata.len();
    if size > MAX_FILE_BYTES {
        return Err(HashFileError::TooLarge {
            limit: MAX_FILE_BYTES,
        });
    }
    let file = File::open(path)?;
    let digests = compute_stream(file, |read| progress(share(read, size)))?;
    Ok(FileDigests { digests, size })
}

/// The share of `size` that `read` is, in percent, as the C# progress: all of an empty file.
#[expect(
    clippy::cast_precision_loss,
    reason = "a share shown on a bar needs no more than a float's precision"
)]
fn share(read: u64, size: u64) -> f64 {
    const ALL: f64 = 100.0;
    if size == 0 {
        ALL
    } else {
        (read as f64 / size as f64 * ALL).min(ALL)
    }
}

/// The algorithm whose hexadecimal digest is `length` characters long, as the C#
/// `HashVerifier.DetectByLength` (`HashVerifier.cs:23-31`): SHA3-256, as long as SHA-256, is
/// never the one guessed.
#[must_use]
pub const fn detect_by_length(length: usize) -> Option<HashAlgorithm> {
    match length {
        32 => Some(HashAlgorithm::Md5),
        40 => Some(HashAlgorithm::Sha1),
        64 => Some(HashAlgorithm::Sha256),
        96 => Some(HashAlgorithm::Sha384),
        128 => Some(HashAlgorithm::Sha512),
        _ => None,
    }
}

/// Which digest a hash pasted to check is, as the C# `VerifyResult`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HashMatch {
    /// The algorithm whose digest it is.
    pub kind: HashAlgorithm,
    /// Whether its length named it, rather than a look through every digest.
    pub by_length: bool,
}

/// The digest of `computed` that `candidate` is, as the C# `HashVerifier.FindMatch`
/// (`HashVerifier.cs:33-62`): trimmed and compared whatever its case, the algorithm its
/// length names tried first, then every digest in order; `None` for a blank candidate or no
/// match.
#[must_use]
pub fn find_match(computed: &[(HashAlgorithm, String)], candidate: &str) -> Option<HashMatch> {
    let candidate = candidate.trim().to_lowercase();
    if candidate.is_empty() {
        return None;
    }
    if let Some(kind) = detect_by_length(candidate.chars().count())
        && computed
            .iter()
            .any(|(each, digest)| *each == kind && digest.eq_ignore_ascii_case(&candidate))
    {
        return Some(HashMatch {
            kind,
            by_length: true,
        });
    }
    computed
        .iter()
        .find(|(_, digest)| digest.eq_ignore_ascii_case(&candidate))
        .map(|(kind, _)| HashMatch {
            kind: *kind,
            by_length: false,
        })
}
