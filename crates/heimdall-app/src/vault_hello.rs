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

//! Windows Hello unlocking the vault, as the C# `WindowsVaultHelloService` and
//! `VaultHelloProtector`: a copy of the vault's data key, wrapped by a key only a Windows
//! Hello signature gives, kept outside the vault file. The vault file does not change.
//!
//! - Enrolling creates a Windows Hello key credential (`KeyCredentialManager`) named after
//!   the vault id, has it sign a random challenge, and derives from that signature, with
//!   HKDF-SHA256 over a random salt, the key that wraps the data key with AES-256-GCM. What
//!   is needed to do it again, the challenge, the salt, the wrapped key and the hash of the
//!   credential's public key, is the [`Envelope`], kept in the system's credential store
//!   (Windows Credential Manager) under the vault's own entry, where the C# keeps it in its
//!   settings under DPAPI.
//! - Unlocking opens that credential, checks its public key is the one enrolled, has it sign
//!   the same challenge, derives the same key and unwraps the data key, which opens the vault
//!   ([`Vault::open_with_data_key`]). The signature is the same each time because Windows
//!   Hello signs with RSA PKCS#1 v1.5, which is deterministic; were it not, the unwrapping
//!   fails like any other, and the master password is asked, as the C#.
//! - It fails closed: any failure, a cancel or a time limit reached is no unlock, and the
//!   master password stays the way in. Nothing here can lock the user out.
//!
//! The calls to Windows sit behind [`KeyCredentials`], so what they come to is tested
//! without a prompt; each waits on Windows within a time limit, on a worker thread. The data
//! key, the derived key and the signature are zeroed once used; a copy Windows keeps in its
//! own buffers cannot be, without `unsafe`.
//!
//! A C# enrolment cannot be carried over: its envelope is under DPAPI in the C# settings.
//! Windows Hello is enrolled again from this application's settings.

use std::fmt::Write as _;
use std::path::Path;
use std::time::{Duration, SystemTime};

use sealvault::aead::{self, FreshNonce};
use sealvault::secret::SecretKey;
use sealvault::{DATA_KEY_LEN, Vault, hash, kdf, random};
use zeroize::Zeroizing;

use crate::OpenedVault;

/// The algorithm, bound into what the wrapping authenticates, as the C#
/// `VaultHelloProtector.AlgorithmId`.
pub const ALGORITHM_ID: &str = "heimdall.vault-hello.sign-hkdf-aesgcm.v1";

/// Bytes of the random challenge the credential signs, as the C# `ChallengeSizeBytes`.
pub const CHALLENGE_LEN: usize = 32;

/// Bytes of the HKDF salt, as the C# `SaltSizeBytes`.
pub const SALT_LEN: usize = 32;

/// What HKDF expands the signature with, as the C# `HkdfInfo`.
const HKDF_INFO: &[u8] = b"heimdall.vault-hello.kek.v1";

/// The start of a credential's name, as the C# `CredentialNamePrefixBytes`.
const CREDENTIAL_NAME_PREFIX: &str = "Heimdall.VaultHello.";

/// Bytes of a vault id, as the C# `Guid`.
const VAULT_ID_LEN: usize = 16;

/// Bytes of a SHA-256 hash.
const HASH_LEN: usize = hash::SHA256_LEN;

/// Bytes of an AES-GCM nonce.
const NONCE_LEN: usize = aead::NONCE_LEN;

/// Bytes of an AES-GCM tag.
const TAG_LEN: usize = aead::TAG_LEN;

/// Bytes of the wrapped data key and its tag.
const WRAPPED_LEN: usize = DATA_KEY_LEN + TAG_LEN;

/// The first bytes of an envelope.
const ENVELOPE_MAGIC: [u8; 4] = *b"HHVE";

/// The envelope's layout written.
const ENVELOPE_VERSION: u8 = 1;

/// Bytes of an envelope: magic, version, vault id, public key hash, challenge, salt,
/// enrolment time, nonce, wrapped data key.
pub const ENVELOPE_LEN: usize = ENVELOPE_MAGIC.len()
    + 1
    + VAULT_ID_LEN
    + HASH_LEN
    + CHALLENGE_LEN
    + SALT_LEN
    + 8
    + NONCE_LEN
    + WRAPPED_LEN;

/// The start of the name the envelope is kept under in the system's store, beside the saved
/// passwords' own.
const ENTRY_PREFIX: &str = "vault-hello/";

/// Seconds in a day, for the days counted since the master password was typed.
const SECONDS_PER_DAY: u64 = 86_400;

/// Longest wait for Windows to say whether a credential can be enrolled, or to open one.
pub const AVAILABILITY_TIME_LIMIT: Duration = crate::windows_hello::AVAILABILITY_TIME_LIMIT;

/// Longest wait for the user at a Windows Hello prompt: the request is then cancelled.
pub const PROMPT_TIME_LIMIT: Duration = crate::windows_hello::VERIFICATION_TIME_LIMIT;

/// Why Windows Hello did not enrol or unlock, as the C# `VaultHelloFailureReason`: coarse,
/// and nothing secret.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HelloFailure {
    /// The credential is gone, or is no longer the one enrolled: Windows Hello was reset or
    /// removed on this computer.
    NotFound,
    /// The user dismissed the prompt.
    UserCanceled,
    /// The user chose the password instead at the prompt.
    UserPrefersPassword,
    /// The security device is locked for a while.
    SecurityDeviceLocked,
    /// Windows Hello, its key credentials or a TPM 2.0 are not there, or the system's store
    /// could not keep the envelope.
    Unavailable,
    /// The envelope or the vault did not open with what the signature gave: tampered with,
    /// of another vault, or a signature that changed.
    CryptoFailure,
    /// No answer within the time limit: the request was cancelled.
    TimedOut,
}

/// What `KeyCredentialStatus` says of a key credential request, as the C#
/// `VaultHelloStatusMapper` reads it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CredentialStatus {
    /// It succeeded.
    Success,
    /// No credential of that name.
    NotFound,
    /// The user dismissed the prompt.
    UserCanceled,
    /// The user chose the password instead.
    UserPrefersPassword,
    /// The security device is locked.
    SecurityDeviceLocked,
    /// Any other answer: an unknown error, a credential already there.
    Other,
}

/// What a status comes to: `None` for a success, else the failure, as the C#
/// `MapKeyCredentialStatus`; anything not named is [`HelloFailure::Unavailable`].
#[must_use]
pub fn status_failure(status: CredentialStatus) -> Option<HelloFailure> {
    match status {
        CredentialStatus::Success => None,
        CredentialStatus::NotFound => Some(HelloFailure::NotFound),
        CredentialStatus::UserCanceled => Some(HelloFailure::UserCanceled),
        CredentialStatus::UserPrefersPassword => Some(HelloFailure::UserPrefersPassword),
        CredentialStatus::SecurityDeviceLocked => Some(HelloFailure::SecurityDeviceLocked),
        CredentialStatus::Other => Some(HelloFailure::Unavailable),
    }
}

/// The Windows Hello key credential calls, put behind a trait as the C# `IVaultHelloService`
/// is behind its interface.
pub trait KeyCredentials {
    /// Whether a credential can be enrolled here: key credentials supported and a TPM 2.0
    /// present, as the C# `IsEnrollmentAvailableAsync`.
    fn enrolment_available(&self) -> bool;

    /// Creates the credential `name`, replacing one of that name, which prompts; its public
    /// key.
    ///
    /// # Errors
    ///
    /// Why it was not created.
    fn create(&self, name: &str) -> Result<Vec<u8>, HelloFailure>;

    /// Opens the credential `name`, without a prompt; its public key.
    ///
    /// # Errors
    ///
    /// [`HelloFailure::NotFound`] when there is none, or why it did not open.
    fn open(&self, name: &str) -> Result<Vec<u8>, HelloFailure>;

    /// Has the credential `name` sign `challenge`, which prompts; the signature.
    ///
    /// # Errors
    ///
    /// Why it did not sign.
    fn sign(&self, name: &str, challenge: &[u8]) -> Result<Zeroizing<Vec<u8>>, HelloFailure>;

    /// Deletes the credential `name`; one already gone is not a failure.
    fn delete(&self, name: &str);
}

/// What enrolling left to unlock with: kept outside the vault file, nothing secret in it
/// without the Windows Hello credential's signature.
///
/// Its bytes, fixed at [`ENVELOPE_LEN`], integers big-endian as the C# writes them:
///
/// | Bytes | Field |
/// |---|---|
/// | 4 | magic `HHVE` |
/// | 1 | layout version, 1 |
/// | 16 | vault id |
/// | 32 | SHA-256 of the credential's public key |
/// | 32 | challenge |
/// | 32 | HKDF salt |
/// | 8 | enrolment time, seconds since 1970 UTC |
/// | 12 | AES-GCM nonce |
/// | 48 | the wrapped data key and its tag |
#[derive(Clone, PartialEq, Eq)]
pub struct Envelope {
    vault_id: [u8; VAULT_ID_LEN],
    public_key_hash: [u8; HASH_LEN],
    challenge: [u8; CHALLENGE_LEN],
    salt: [u8; SALT_LEN],
    enrolled_at: u64,
    nonce: [u8; NONCE_LEN],
    wrapped: [u8; WRAPPED_LEN],
}

impl std::fmt::Debug for Envelope {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Nothing of the wrapping: its name says enough to tell two apart.
        f.debug_struct("Envelope")
            .field("credential", &self.credential_name())
            .finish_non_exhaustive()
    }
}

impl Envelope {
    /// The envelope written in `bytes`; `None` unless every field is there, with this
    /// layout's magic and version, and nothing after.
    #[must_use]
    pub fn parse(bytes: &[u8]) -> Option<Self> {
        if bytes.len() != ENVELOPE_LEN {
            return None;
        }
        let mut reader = Reader(bytes);
        if reader.take::<4>()? != ENVELOPE_MAGIC || reader.take::<1>()? != [ENVELOPE_VERSION] {
            return None;
        }
        Some(Self {
            vault_id: reader.take()?,
            public_key_hash: reader.take()?,
            challenge: reader.take()?,
            salt: reader.take()?,
            enrolled_at: u64::from_be_bytes(reader.take()?),
            nonce: reader.take()?,
            wrapped: reader.take()?,
        })
    }

    /// The envelope as written.
    #[must_use]
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(ENVELOPE_LEN);
        bytes.extend_from_slice(&ENVELOPE_MAGIC);
        bytes.push(ENVELOPE_VERSION);
        bytes.extend_from_slice(&self.vault_id);
        bytes.extend_from_slice(&self.public_key_hash);
        bytes.extend_from_slice(&self.challenge);
        bytes.extend_from_slice(&self.salt);
        bytes.extend_from_slice(&self.enrolled_at.to_be_bytes());
        bytes.extend_from_slice(&self.nonce);
        bytes.extend_from_slice(&self.wrapped);
        debug_assert_eq!(bytes.len(), ENVELOPE_LEN);
        bytes
    }

    /// When it was enrolled; `None` past what this system's clock can say, read from the
    /// system's store as it is.
    #[must_use]
    pub fn enrolled_at(&self) -> Option<SystemTime> {
        SystemTime::UNIX_EPOCH.checked_add(Duration::from_secs(self.enrolled_at))
    }

    /// The name of its Windows Hello credential, as the C# `CreateCredentialName`: the
    /// prefix, then the SHA-256 of the vault id in upper-case hex.
    #[must_use]
    pub fn credential_name(&self) -> String {
        credential_name(&self.vault_id)
    }

    /// What the wrapping authenticates, as the C# `BuildAad`: each field after its length on
    /// four bytes, the algorithm, the vault id, the public key's hash in upper-case hex, the
    /// challenge and the salt; then the enrolment time, which the C# does not keep.
    fn authenticated(&self) -> Vec<u8> {
        let mut bytes = Vec::new();
        for field in [
            ALGORITHM_ID.as_bytes(),
            vault_id_text(&self.vault_id).as_bytes(),
            hex_upper(&self.public_key_hash).as_bytes(),
            &self.challenge,
            &self.salt,
            &self.enrolled_at.to_be_bytes(),
        ] {
            let length = u32::try_from(field.len()).unwrap_or(u32::MAX);
            bytes.extend_from_slice(&length.to_be_bytes());
            bytes.extend_from_slice(field);
        }
        bytes
    }
}

/// Reads fixed-size fields in order.
struct Reader<'a>(&'a [u8]);

impl Reader<'_> {
    fn take<const N: usize>(&mut self) -> Option<[u8; N]> {
        let (field, rest) = self.0.split_first_chunk::<N>()?;
        self.0 = rest;
        Some(*field)
    }
}

/// The name the envelope of the vault at `vault_path` is kept under in the system's store:
/// each vault file its own, so a vault moved elsewhere is enrolled again rather than given
/// another one's envelope.
#[must_use]
pub fn entry_name(vault_path: &Path) -> String {
    let path = vault_path.to_string_lossy();
    format!("{ENTRY_PREFIX}{}", hex_lower(&sha256(path.as_bytes())))
}

/// Whether the master password must be typed before Windows Hello unlocks again, as the C#
/// `VaultHelloReauthPolicy.ShouldRequireMasterPassword`: never with no days set; always
/// when it was never typed on this computer; else once more than `max_days` passed since.
/// Unlike the C#, a time still to come (a clock put back, a file edited) is due as well,
/// rather than fresh for as long as it lies ahead.
#[must_use]
pub fn master_password_due(
    last_master_unlock: Option<SystemTime>,
    max_days: u32,
    now: SystemTime,
) -> bool {
    if max_days == 0 {
        return false;
    }
    let Some(last) = last_master_unlock else {
        return true;
    };
    let limit = Duration::from_secs(u64::from(max_days) * SECONDS_PER_DAY);
    now.duration_since(last).map_or(true, |since| since > limit)
}

/// Enrols Windows Hello for the vault whose data key is `data_key`, as the C#
/// `EnrollHelloAsync`: nothing unless a credential can be enrolled here; then a credential
/// created, replacing the vault's former one, a challenge signed, and the data key wrapped
/// by what the signature gives. The vault id of `previous`, the envelope enrolled before,
/// is kept, as the C# keeps its `VaultId`, so its credential is the one replaced.
///
/// # Errors
///
/// Why it was not enrolled: two prompts may each be dismissed.
pub fn enrol(
    platform: &impl KeyCredentials,
    data_key: &[u8; DATA_KEY_LEN],
    previous: Option<&Envelope>,
    now: SystemTime,
) -> Result<Envelope, HelloFailure> {
    if !platform.enrolment_available() {
        return Err(HelloFailure::Unavailable);
    }
    let vault_id = match previous {
        Some(previous) => previous.vault_id,
        None => random()?,
    };
    let name = credential_name(&vault_id);
    let public_key = platform.create(&name)?;
    let nonce = FreshNonce::random().map_err(|_| no_randomness())?;
    let mut envelope = Envelope {
        vault_id,
        public_key_hash: sha256(&public_key),
        challenge: random()?,
        salt: random()?,
        enrolled_at: now
            .duration_since(SystemTime::UNIX_EPOCH)
            .map_or(0, |since| since.as_secs()),
        nonce: nonce.bytes(),
        wrapped: [0; WRAPPED_LEN],
    };
    let signature = platform.sign(&name, &envelope.challenge)?;
    let wrapping_key = derive(&signature, &envelope.salt)?;
    let wrapped = aead::seal(&wrapping_key, nonce, data_key, &envelope.authenticated())
        .map_err(|_| HelloFailure::CryptoFailure)?;
    envelope.wrapped = wrapped
        .as_slice()
        .try_into()
        .map_err(|_| HelloFailure::CryptoFailure)?;
    log::info!("Windows Hello vault enrollment succeeded.");
    Ok(envelope)
}

/// Opens the vault at `vault_path` with Windows Hello and `envelope`, as the C#
/// `UnlockAsync`: its credential opened, then, unlike the C#, its public key checked against
/// the one enrolled before any prompt, so a credential replaced since says so rather than
/// asking for a signature that cannot fit; the challenge signed, the data key unwrapped,
/// the vault opened with it.
///
/// # Errors
///
/// Why it was not opened; the vault is then left closed.
pub fn unlock(
    platform: &impl KeyCredentials,
    vault_path: &Path,
    envelope: &Envelope,
) -> Result<Vault, HelloFailure> {
    let name = envelope.credential_name();
    let public_key = platform.open(&name)?;
    if sha256(&public_key) != envelope.public_key_hash {
        log::warn!("the Windows Hello credential is not the one enrolled for the vault");
        return Err(HelloFailure::NotFound);
    }
    let signature = platform.sign(&name, &envelope.challenge)?;
    let wrapping_key = derive(&signature, &envelope.salt)?;
    let unwrapped = aead::open(
        &wrapping_key,
        &envelope.nonce,
        &envelope.wrapped,
        &envelope.authenticated(),
    )
    .map_err(|_| HelloFailure::CryptoFailure)?;
    let data_key = SecretKey::<DATA_KEY_LEN>::from_slice(unwrapped.as_bytes())
        .map_err(|_| HelloFailure::CryptoFailure)?;
    let vault = Vault::open_with_data_key(vault_path, data_key.as_bytes()).map_err(|error| {
        log::warn!("the vault did not open with the key Windows Hello unwrapped: {error}");
        HelloFailure::CryptoFailure
    })?;
    log::info!("Windows Hello vault unlock succeeded.");
    Ok(vault)
}

/// Whether a credential can be enrolled here, asked away from the window's thread.
pub async fn check<P: KeyCredentials + Send + 'static>(platform: P) -> bool {
    tokio::task::spawn_blocking(move || platform.enrolment_available())
        .await
        .unwrap_or_else(|error| {
            log::warn!("the Windows Hello availability check stopped: {error}");
            false
        })
}

/// [`enrol`], away from the window's thread: the prompts hold the call until answered.
///
/// # Errors
///
/// Why it was not enrolled.
pub async fn enrol_away<P: KeyCredentials + Send + 'static>(
    platform: P,
    data_key: Box<Zeroizing<[u8; DATA_KEY_LEN]>>,
    previous: Option<Envelope>,
) -> Result<Envelope, HelloFailure> {
    tokio::task::spawn_blocking(move || {
        enrol(&platform, &data_key, previous.as_ref(), SystemTime::now())
    })
    .await
    .unwrap_or_else(|error| {
        log::warn!("the Windows Hello enrolment stopped: {error}");
        Err(HelloFailure::Unavailable)
    })
}

/// [`unlock`], away from the window's thread, the vault handed over as the master
/// password's is.
///
/// # Errors
///
/// Why it was not opened.
pub async fn unlock_away<P: KeyCredentials + Send + 'static>(
    platform: P,
    vault_path: std::path::PathBuf,
    envelope: Envelope,
) -> Result<OpenedVault, HelloFailure> {
    tokio::task::spawn_blocking(move || unlock(&platform, &vault_path, &envelope))
        .await
        .unwrap_or_else(|error| {
            log::warn!("the Windows Hello unlock stopped: {error}");
            Err(HelloFailure::Unavailable)
        })
        .map(OpenedVault::new)
}

/// Deletes the credential `name`, away from the window's thread.
pub async fn delete_away<P: KeyCredentials + Send + 'static>(platform: P, name: String) {
    if let Err(error) = tokio::task::spawn_blocking(move || platform.delete(&name)).await {
        log::warn!("the Windows Hello credential removal stopped: {error}");
    }
}

/// The credential name of vault id `vault_id`.
fn credential_name(vault_id: &[u8; VAULT_ID_LEN]) -> String {
    let hash = sha256(vault_id_text(vault_id).as_bytes());
    format!("{CREDENTIAL_NAME_PREFIX}{}", hex_upper(&hash))
}

/// A vault id written as the C# `Guid.ToString("N")`: lower-case hex, no dash.
fn vault_id_text(vault_id: &[u8; VAULT_ID_LEN]) -> String {
    hex_lower(vault_id)
}

fn hex_lower(bytes: &[u8]) -> String {
    bytes.iter().fold(String::new(), |mut text, byte| {
        // Writing to a String cannot fail.
        let _ = write!(text, "{byte:02x}");
        text
    })
}

/// Upper-case hex, as the C# `Convert.ToHexString`.
fn hex_upper(bytes: &[u8]) -> String {
    bytes.iter().fold(String::new(), |mut text, byte| {
        // Writing to a String cannot fail.
        let _ = write!(text, "{byte:02X}");
        text
    })
}

fn sha256(bytes: &[u8]) -> [u8; HASH_LEN] {
    hash::sha256(bytes)
}

/// The failure of a random draw, said once in the log.
fn no_randomness() -> HelloFailure {
    log::warn!("no random bytes for Windows Hello");
    HelloFailure::Unavailable
}

fn random<const N: usize>() -> Result<[u8; N], HelloFailure> {
    random::array().map_err(|_| no_randomness())
}

/// The key that wraps the data key, from the signature and the salt, as the C#
/// `DeriveHelloKek`: HKDF-SHA256, the salt as HKDF's, [`HKDF_INFO`], 32 bytes.
fn derive(signature: &[u8], salt: &[u8; SALT_LEN]) -> Result<aead::Key, HelloFailure> {
    if signature.is_empty() {
        return Err(HelloFailure::CryptoFailure);
    }
    kdf::hkdf_sha256(signature, salt, HKDF_INFO).map_err(|_| HelloFailure::CryptoFailure)
}

/// What `tpmtool getdeviceinformation` printed says of a TPM 2.0, as the C#
/// `WindowsTpmPresenceService` reads it: a success, "2.0" somewhere, and "true" (or the
/// French "vrai") somewhere, whatever the case.
#[must_use]
pub fn reports_tpm2(succeeded: bool, output: &str) -> bool {
    let output = output.to_lowercase();
    succeeded && output.contains("2.0") && (output.contains("true") || output.contains("vrai"))
}

/// How often a tool is looked at while it is let finish.
const TOOL_POLL: Duration = Duration::from_millis(50);

/// Runs `command`, its output read, within `limit` in all: whether it succeeded, and what it
/// printed; `None` when it could not run, or was stopped past the limit, its output closed
/// or not.
#[must_use]
pub fn run_within(command: &mut std::process::Command, limit: Duration) -> Option<(bool, String)> {
    use std::io::Read as _;
    use std::process::{Child, Stdio};

    let deadline = std::time::Instant::now() + limit;
    let stop = |child: &mut Child| {
        child.kill().ok();
        child.wait().ok();
    };
    let mut child = command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .inspect_err(|error| log::warn!("{} did not start: {error}", tool_name(command)))
        .ok()?;
    let Some(mut output) = child.stdout.take() else {
        stop(&mut child);
        return None;
    };
    let (sender, receiver) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let mut text = String::new();
        let read = output.read_to_string(&mut text).map(|_| text);
        // The receiver gone means the wait is over: nothing to tell.
        sender.send(read).ok();
    });
    let left = deadline.saturating_duration_since(std::time::Instant::now());
    let Ok(Ok(text)) = receiver.recv_timeout(left) else {
        log::warn!("{} failed or took too long", tool_name(command));
        stop(&mut child);
        return None;
    };
    // Its output closed, it still has the rest of the time to end.
    loop {
        match child.try_wait() {
            Ok(Some(status)) => return Some((status.success(), text)),
            Ok(None) if std::time::Instant::now() < deadline => std::thread::sleep(TOOL_POLL),
            Ok(None) | Err(_) => {
                log::warn!("{} took too long", tool_name(command));
                stop(&mut child);
                return None;
            }
        }
    }
}

/// The program `command` runs, for the log.
fn tool_name(command: &std::process::Command) -> String {
    command.get_program().to_string_lossy().into_owned()
}

/// This computer's Windows Hello key credentials.
#[derive(Debug, Clone, Copy, Default)]
pub struct SystemKeyCredentials;

#[cfg(windows)]
mod system {
    use std::os::windows::process::CommandExt as _;
    use std::process::Command;
    use std::sync::mpsc;

    use windows::Security::Credentials::{
        KeyCredential, KeyCredentialCreationOption, KeyCredentialManager, KeyCredentialStatus,
    };
    use windows::Security::Cryptography::CryptographicBuffer;
    use windows::Storage::Streams::IBuffer;
    use windows::core::{Array, HRESULT, HSTRING};
    use zeroize::{Zeroize as _, Zeroizing};

    use super::{
        AVAILABILITY_TIME_LIMIT, CredentialStatus, HelloFailure, KeyCredentials, PROMPT_TIME_LIMIT,
        SystemKeyCredentials, reports_tpm2, run_within, status_failure,
    };

    /// The TPM tool, in the system folder, as the C# `TpmToolExecutableName`.
    const TPM_TOOL: &str = "tpmtool.exe";

    /// The one verb the TPM check needs, as the C# `TpmToolDeviceInformationArguments`.
    const TPM_TOOL_ARGUMENT: &str = "getdeviceinformation";

    /// What `DeleteAsync` answers for a credential already gone, as the C# catches it:
    /// `NTE_NO_KEY`.
    const NO_KEY: u32 = 0x8009_000D;

    /// How a call to Windows ended within its time limit.
    enum Waited<T> {
        Answered(T),
        Failed(windows::core::Error),
        TimedOut,
    }

    /// Starts `$operation`, a `WinRT` asynchronous call, and waits for its answer within
    /// `$limit`, the call cancelled past it. A macro: the operation's type lives in a crate
    /// this one does not name.
    macro_rules! within {
        ($operation:expr, $limit:expr) => {{
            let (sender, receiver) = mpsc::channel();
            let started = $operation.and_then(|operation| {
                operation.when(move |answer| {
                    // The receiver gone means the wait is over: nothing to tell.
                    sender.send(answer).ok();
                })?;
                Ok(operation)
            });
            match started {
                Err(error) => Waited::Failed(error),
                Ok(operation) => match receiver.recv_timeout($limit) {
                    Ok(Ok(answer)) => Waited::Answered(answer),
                    Ok(Err(error)) => Waited::Failed(error),
                    Err(_) => {
                        operation.Cancel().ok();
                        Waited::TimedOut
                    }
                },
            }
        }};
    }

    /// The answer of a call that waited, or why it gave none: a failure is
    /// [`HelloFailure::Unavailable`], logged.
    fn answered<T>(waited: Waited<T>, what: &str) -> Result<T, HelloFailure> {
        match waited {
            Waited::Answered(answer) => Ok(answer),
            Waited::Failed(error) => {
                log::warn!("Windows Hello: {what} failed: {error}");
                Err(HelloFailure::Unavailable)
            }
            Waited::TimedOut => {
                log::warn!("Windows Hello: {what} took too long and was cancelled");
                Err(HelloFailure::TimedOut)
            }
        }
    }

    fn checked(status: &windows::core::Result<KeyCredentialStatus>) -> Result<(), HelloFailure> {
        let status = match status {
            Ok(KeyCredentialStatus::Success) => CredentialStatus::Success,
            Ok(KeyCredentialStatus::NotFound) => CredentialStatus::NotFound,
            Ok(KeyCredentialStatus::UserCanceled) => CredentialStatus::UserCanceled,
            Ok(KeyCredentialStatus::UserPrefersPassword) => CredentialStatus::UserPrefersPassword,
            Ok(KeyCredentialStatus::SecurityDeviceLocked) => CredentialStatus::SecurityDeviceLocked,
            Ok(_) | Err(_) => CredentialStatus::Other,
        };
        status_failure(status).map_or(Ok(()), Err)
    }

    /// The bytes of `buffer`, copied through `CryptographicBuffer`; the copy Windows hands
    /// over is zeroed once read.
    fn bytes(buffer: &IBuffer) -> Result<Zeroizing<Vec<u8>>, HelloFailure> {
        let mut array = Array::<u8>::new();
        CryptographicBuffer::CopyToByteArray(buffer, &mut array).map_err(|error| {
            log::warn!("Windows Hello: a buffer could not be read: {error}");
            HelloFailure::CryptoFailure
        })?;
        let copy = Zeroizing::new(array.to_vec());
        array.zeroize();
        Ok(copy)
    }

    fn public_key(credential: &KeyCredential) -> Result<Vec<u8>, HelloFailure> {
        let buffer = credential
            .RetrievePublicKeyWithDefaultBlobType()
            .map_err(|error| {
                log::warn!("Windows Hello: the public key could not be read: {error}");
                HelloFailure::Unavailable
            })?;
        Ok(bytes(&buffer)?.to_vec())
    }

    fn opened(name: &str) -> Result<KeyCredential, HelloFailure> {
        let result = answered(
            within!(
                KeyCredentialManager::OpenAsync(&HSTRING::from(name)),
                AVAILABILITY_TIME_LIMIT
            ),
            "opening the vault credential",
        )?;
        checked(&result.Status())?;
        result.Credential().map_err(|_| HelloFailure::NotFound)
    }

    /// Whether a TPM 2.0 is present, as the C# `IsTpm2PresentAsync` asks `tpmtool`, which
    /// needs no administrator: run from the system folder Windows names, never one the
    /// environment names, and in it, as the C# `SystemExecutablePath.InSystemDirectory`;
    /// within [`AVAILABILITY_TIME_LIMIT`] in all.
    fn tpm2_present() -> bool {
        let Some(system) = heimdall_core::paths::system_dir() else {
            log::warn!("the TPM presence check failed: no system folder");
            return false;
        };
        let mut command = Command::new(system.join(TPM_TOOL));
        command
            .arg(TPM_TOOL_ARGUMENT)
            .current_dir(&system)
            .creation_flags(crate::citrix::CREATE_NO_WINDOW);
        let present = run_within(&mut command, AVAILABILITY_TIME_LIMIT)
            .is_some_and(|(succeeded, text)| reports_tpm2(succeeded, &text));
        log::info!("TPM 2.0 presence check returned {present}.");
        present
    }

    impl KeyCredentials for SystemKeyCredentials {
        fn enrolment_available(&self) -> bool {
            let supported = answered(
                within!(
                    KeyCredentialManager::IsSupportedAsync(),
                    AVAILABILITY_TIME_LIMIT
                ),
                "the key credential support check",
            )
            .unwrap_or(false);
            let available = supported && tpm2_present();
            log::info!("Windows Hello vault enrollment availability returned {available}.");
            available
        }

        fn create(&self, name: &str) -> Result<Vec<u8>, HelloFailure> {
            let result = answered(
                within!(
                    KeyCredentialManager::RequestCreateAsync(
                        &HSTRING::from(name),
                        KeyCredentialCreationOption::ReplaceExisting,
                    ),
                    PROMPT_TIME_LIMIT
                ),
                "creating the vault credential",
            )?;
            checked(&result.Status())?;
            let credential = result.Credential().map_err(|_| HelloFailure::Unavailable)?;
            public_key(&credential)
        }

        fn open(&self, name: &str) -> Result<Vec<u8>, HelloFailure> {
            public_key(&opened(name)?)
        }

        fn sign(&self, name: &str, challenge: &[u8]) -> Result<Zeroizing<Vec<u8>>, HelloFailure> {
            let credential = opened(name)?;
            let data = CryptographicBuffer::CreateFromByteArray(challenge)
                .map_err(|_| HelloFailure::CryptoFailure)?;
            let result = answered(
                within!(credential.RequestSignAsync(&data), PROMPT_TIME_LIMIT),
                "signing with the vault credential",
            )?;
            checked(&result.Status())?;
            let signature = result.Result().map_err(|_| HelloFailure::CryptoFailure)?;
            let signature = bytes(&signature)?;
            if signature.is_empty() {
                return Err(HelloFailure::CryptoFailure);
            }
            Ok(signature)
        }

        fn delete(&self, name: &str) {
            match within!(
                KeyCredentialManager::DeleteAsync(&HSTRING::from(name)),
                AVAILABILITY_TIME_LIMIT
            ) {
                Waited::Answered(()) => {
                    log::info!("Windows Hello vault credential removed.");
                }
                Waited::Failed(error) if error.code() == HRESULT(NO_KEY.cast_signed()) => {
                    log::info!("Windows Hello vault credential was already absent.");
                }
                Waited::Failed(error) => {
                    log::warn!("Windows Hello vault credential removal failed: {error}");
                }
                Waited::TimedOut => {
                    log::warn!("Windows Hello vault credential removal took too long");
                }
            }
        }
    }
}

/// Elsewhere there is no Windows Hello: nothing is ever enrolled, nothing unlocks.
#[cfg(not(windows))]
impl KeyCredentials for SystemKeyCredentials {
    fn enrolment_available(&self) -> bool {
        false
    }

    fn create(&self, _name: &str) -> Result<Vec<u8>, HelloFailure> {
        Err(HelloFailure::Unavailable)
    }

    fn open(&self, _name: &str) -> Result<Vec<u8>, HelloFailure> {
        Err(HelloFailure::Unavailable)
    }

    fn sign(&self, _name: &str, _challenge: &[u8]) -> Result<Zeroizing<Vec<u8>>, HelloFailure> {
        Err(HelloFailure::Unavailable)
    }

    fn delete(&self, _name: &str) {}
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_wrapping_key_is_hkdf_sha256_of_the_signature_as_the_csharp_derives_it() {
        // HKDF's two steps by hand, as the C# writes them: extract with the salt, then the
        // one expand block 32 bytes need.
        let signature = [7_u8; 256];
        let salt = [9_u8; SALT_LEN];
        let key = derive(&signature, &salt).expect("derived");
        let prk = sealvault::mac::compute(sealvault::mac::Algorithm::HmacSha256, &salt, &signature);
        let block = sealvault::mac::compute(
            sealvault::mac::Algorithm::HmacSha256,
            prk.as_bytes(),
            &[HKDF_INFO, &[1]].concat(),
        );
        assert_eq!(key.as_bytes().as_slice(), block.as_bytes());
        assert!(
            derive(&[], &salt).is_err(),
            "an empty signature, as the C# refuses it"
        );
    }

    #[test]
    fn what_the_wrapping_authenticates_is_the_csharp_fields_length_prefixed_then_the_date() {
        let envelope = Envelope {
            vault_id: [0xAB; VAULT_ID_LEN],
            public_key_hash: [0x0F; HASH_LEN],
            challenge: [1; CHALLENGE_LEN],
            salt: [2; SALT_LEN],
            enrolled_at: 5,
            nonce: [3; NONCE_LEN],
            wrapped: [4; WRAPPED_LEN],
        };
        let bytes = envelope.authenticated();
        let id = "ab".repeat(VAULT_ID_LEN);
        let hash = "0F".repeat(HASH_LEN);
        let mut expected = Vec::new();
        for field in [
            ALGORITHM_ID.as_bytes(),
            id.as_bytes(),
            hash.as_bytes(),
            &[1; CHALLENGE_LEN],
            &[2; SALT_LEN],
            &5_u64.to_be_bytes(),
        ] {
            expected.extend_from_slice(&u32::try_from(field.len()).expect("short").to_be_bytes());
            expected.extend_from_slice(field);
        }
        assert_eq!(bytes, expected);
        assert_eq!(
            envelope.credential_name(),
            format!("Heimdall.VaultHello.{}", hex_upper(&sha256(id.as_bytes())))
        );
    }
}
