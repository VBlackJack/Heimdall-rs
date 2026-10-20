#![cfg_attr(doc, doc = include_str!("../README.md"))]
#![doc(html_logo_url = "https://cdnweb.devolutions.net/images/projects/devolutions/logos/devolutions-icon-shadow.svg")]
#![allow(clippy::arithmetic_side_effects)] // FIXME: remove

mod macros;

pub mod fast_path;
pub mod image;
pub mod pointer;
pub mod rfx; // FIXME: maybe this module should not be in this crate
pub mod x224;

mod active_stage;
mod palette;

use core::fmt;

pub use active_stage::{ActiveStage, ActiveStageBuilder, ActiveStageOutput, GracefulDisconnectReason};

pub type SessionResult<T> = Result<T, SessionError>;

/// The source of every error that bulk-compressed data ends the session with: data that does
/// not decompress, compressed data without compression negotiated, a decompressor that
/// cannot be built. A caller tells these failures apart by this type, and may connect again
/// without compression. Heimdall-rs patch, see `vendor/PATCHES.md`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BulkDataRefused;

impl fmt::Display for BulkDataRefused {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("bulk-compressed data refused")
    }
}

impl core::error::Error for BulkDataRefused {}

/// Whether `error` is one that bulk-compressed data ended the session with
/// ([`BulkDataRefused`]). Heimdall-rs patch, see `vendor/PATCHES.md`.
pub fn is_bulk_data_refused(error: &SessionError) -> bool {
    let mut source = core::error::Error::source(error);
    while let Some(cause) = source {
        if cause.is::<BulkDataRefused>() {
            return true;
        }
        source = cause.source();
    }
    false
}

/// The bulk compression a server used on a PDU ([MS-RDPBCGR] 3.1.8, `compressedType` low
/// nibble). Heimdall-rs patch, see `vendor/PATCHES.md`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BulkType {
    /// RDP 4.0, MPPC with an 8 KiB history.
    Rdp4Mppc8K,
    /// RDP 5.0, MPPC with a 64 KiB history.
    Rdp5Mppc64K,
    /// RDP 6.0, NCRUSH.
    Rdp6Ncrush,
    /// RDP 6.1, XCRUSH.
    Rdp61Xcrush,
}

impl BulkType {
    /// Every type, in the order of the wire values.
    pub const ALL: [BulkType; 4] = [
        BulkType::Rdp4Mppc8K,
        BulkType::Rdp5Mppc64K,
        BulkType::Rdp6Ncrush,
        BulkType::Rdp61Xcrush,
    ];

    /// The type a `compressedType` byte (or a fast-path compression type) names, from its
    /// low nibble; `None` for a value no level uses.
    pub fn from_wire(compressed_type: u8) -> Option<Self> {
        match compressed_type & 0x0F {
            0 => Some(Self::Rdp4Mppc8K),
            1 => Some(Self::Rdp5Mppc64K),
            2 => Some(Self::Rdp6Ncrush),
            3 => Some(Self::Rdp61Xcrush),
            _ => None,
        }
    }

    fn index(self) -> usize {
        match self {
            Self::Rdp4Mppc8K => 0,
            Self::Rdp5Mppc64K => 1,
            Self::Rdp6Ncrush => 2,
            Self::Rdp61Xcrush => 3,
        }
    }
}

impl fmt::Display for BulkType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Rdp4Mppc8K => "RDP 4 (MPPC 8K)",
            Self::Rdp5Mppc64K => "RDP 5 (MPPC 64K)",
            Self::Rdp6Ncrush => "RDP 6 (NCRUSH)",
            Self::Rdp61Xcrush => "RDP 6.1 (XCRUSH)",
        })
    }
}

/// How many PDUs of a session arrived compressed, by type, and how many uncompressed. Only
/// counts and types: never a PDU's contents. Heimdall-rs patch, see `vendor/PATCHES.md`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct BulkTally {
    compressed: [u64; 4],
    uncompressed: u64,
    first_seen: Vec<BulkType>,
}

impl BulkTally {
    /// Counts a PDU that arrived compressed with `bulk_type`; the first of each type is queued
    /// for [`BulkTally::take_first_seen`].
    pub fn record_compressed(&mut self, bulk_type: BulkType) {
        let count = &mut self.compressed[bulk_type.index()];
        if *count == 0 {
            self.first_seen.push(bulk_type);
        }
        *count = count.saturating_add(1);
    }

    /// Counts a PDU that arrived uncompressed.
    pub fn record_uncompressed(&mut self) {
        self.uncompressed = self.uncompressed.saturating_add(1);
    }

    /// The types seen for the first time since the last call.
    pub fn take_first_seen(&mut self) -> Vec<BulkType> {
        core::mem::take(&mut self.first_seen)
    }

    /// The PDUs that arrived compressed with `bulk_type`.
    pub fn compressed(&self, bulk_type: BulkType) -> u64 {
        self.compressed[bulk_type.index()]
    }

    /// The PDUs that arrived uncompressed.
    pub fn uncompressed(&self) -> u64 {
        self.uncompressed
    }
}

#[non_exhaustive]
#[derive(Debug)]
pub enum SessionErrorKind {
    Pdu(ironrdp_pdu::PduError),
    Encode(ironrdp_core::EncodeError),
    Decode(ironrdp_core::DecodeError),
    Reason(String),
    General,
    Custom,
}

impl fmt::Display for SessionErrorKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self {
            SessionErrorKind::Pdu(_) => write!(f, "PDU error"),
            SessionErrorKind::Encode(_) => write!(f, "encode error"),
            SessionErrorKind::Decode(_) => write!(f, "decode error"),
            SessionErrorKind::Reason(description) => write!(f, "reason: {description}"),
            SessionErrorKind::General => write!(f, "general error"),
            SessionErrorKind::Custom => write!(f, "custom error"),
        }
    }
}

impl core::error::Error for SessionErrorKind {
    fn source(&self) -> Option<&(dyn core::error::Error + 'static)> {
        match &self {
            SessionErrorKind::Pdu(e) => Some(e),
            SessionErrorKind::Encode(e) => Some(e),
            SessionErrorKind::Decode(e) => Some(e),
            SessionErrorKind::Reason(_) => None,
            SessionErrorKind::General => None,
            SessionErrorKind::Custom => None,
        }
    }
}

pub type SessionError = ironrdp_error::Error<SessionErrorKind>;

pub trait SessionErrorExt {
    fn pdu(error: ironrdp_pdu::PduError) -> Self;
    fn encode(error: ironrdp_core::EncodeError) -> Self;
    fn decode(error: ironrdp_core::DecodeError) -> Self;
    fn general(context: &'static str) -> Self;
    fn reason(context: &'static str, reason: impl Into<String>) -> Self;
    fn custom<E>(context: &'static str, e: E) -> Self
    where
        E: core::error::Error + Sync + Send + 'static;
}

impl SessionErrorExt for SessionError {
    #[track_caller]
    fn pdu(error: ironrdp_pdu::PduError) -> Self {
        Self::new("payload error", SessionErrorKind::Pdu(error))
    }

    #[track_caller]
    fn encode(error: ironrdp_core::EncodeError) -> Self {
        Self::new("encode error", SessionErrorKind::Encode(error))
    }

    #[track_caller]
    fn decode(error: ironrdp_core::DecodeError) -> Self {
        Self::new("decode error", SessionErrorKind::Decode(error))
    }

    #[track_caller]
    fn general(context: &'static str) -> Self {
        Self::new(context, SessionErrorKind::General)
    }

    #[track_caller]
    fn reason(context: &'static str, reason: impl Into<String>) -> Self {
        Self::new(context, SessionErrorKind::Reason(reason.into()))
    }

    #[track_caller]
    fn custom<E>(context: &'static str, e: E) -> Self
    where
        E: core::error::Error + Sync + Send + 'static,
    {
        Self::new(context, SessionErrorKind::Custom).with_source(e)
    }
}

pub trait SessionResultExt {
    #[must_use]
    fn with_context(self, context: &'static str) -> Self;
    #[must_use]
    fn with_source<E>(self, source: E) -> Self
    where
        E: core::error::Error + Sync + Send + 'static;
}

impl<T> SessionResultExt for SessionResult<T> {
    fn with_context(self, context: &'static str) -> Self {
        self.map_err(|mut e| {
            e.set_context(context);
            e
        })
    }

    fn with_source<E>(self, source: E) -> Self
    where
        E: core::error::Error + Sync + Send + 'static,
    {
        self.map_err(|e| e.with_source(source))
    }
}
