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

//! The Certificate Generator, as the C# `CertificateGeneratorView` and
//! `CertificateGeneratorViewModel` (`CertificateGeneratorViewModel.cs:31-477`): the subject,
//! the key size, the validity and the alternative names; a self-signed leaf or a CA and its
//! leaf, made off the window's thread; the fingerprint, the certificates and their keys,
//! each copied, the keys masked until shown; the certificate saved in PEM, the leaf and its
//! key in a PFX sealed with a password asked for first.
//!
//! The private keys and the PFX password are secrets: held in memory wiped when replaced or
//! when the tab closes, never logged. A PFX holds a private key, so it is written readable by
//! the user alone.

use std::fmt;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use heimdall_app::TabId;
use heimdall_core::tools::certificate_generator::{
    self as engine, CA_VALIDITY_DAYS, CaLeafCertificates, CertificateMode, CertificateOptions,
    DEFAULT_VALIDITY_DAYS, IssuedCertificate, RSA_2048_BITS, RSA_4096_BITS, SelfSignedCertificate,
    ValidationCode,
};
use heimdall_core::tools::private_file;
use iced::widget::text_editor::{Action, Content};
use iced::widget::{column, operation, pick_list, radio, row, text_input};
use iced::{Alignment, Element, Length, Task, window};
use zeroize::Zeroizing;

use super::crypto_parts::{self, Tone};
use super::key_parts;
use super::{CopySlot, ToolMessage, ToolPane};
use crate::i18n::fl;
use crate::shell::Message;
use crate::styles;
use crate::tokens::{font_size, spacing};

/// What a hidden key shows, as the C# `MaskedPlaceholder`.
pub const MASKED_PLACEHOLDER: &str = "********";

/// The file a certificate is offered under, as the C#'s `certificate.pem`.
const PEM_FILE_NAME: &str = "certificate.pem";

/// The file a PFX is offered under, as the C#'s `certificate.pfx`.
const PFX_FILE_NAME: &str = "certificate.pfx";

/// The extensions of the C# PEM and PFX filters.
const PEM_EXTENSIONS: &[&str] = &["pem"];
const PFX_EXTENSIONS: &[&str] = &["pfx"];

/// Height of a certificate's or a key's box, as the C# `Height="140"`.
const PEM_HEIGHT: f32 = 140.0;

/// Height of the fingerprint's box: one line.
const FINGERPRINT_HEIGHT: f32 = 36.0;

/// Width of the key size box, as the C# `Width="160"`.
const KEY_SIZE_WIDTH: f32 = 160.0;

/// Room between the two types, as the C# `Margin="0,0,16,0"`.
const CHOICE_GAP: f32 = 16.0;

/// The field the common name is typed in, focused on an error about it.
fn cn_id() -> iced::widget::Id {
    iced::widget::Id::new("certgen-cn")
}

/// The field the validity is typed in, focused on an error about it.
fn validity_id() -> iced::widget::Id {
    iced::widget::Id::new("certgen-validity")
}

/// A key size in the box, as the C#'s two `ComboBoxItem`s.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KeySizeChoice(pub usize);

impl KeySizeChoice {
    /// The two sizes, in the box's order.
    pub const ALL: [Self; 2] = [Self(RSA_2048_BITS), Self(RSA_4096_BITS)];
}

impl fmt::Display for KeySizeChoice {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&if self.0 == RSA_4096_BITS {
            fl!("ui-tool-certgen-rsa-4096")
        } else {
            fl!("ui-tool-certgen-rsa-2048")
        })
    }
}

/// What was made: the C# `SelfSignedCertificateResult` or `CaLeafCertificateResult`.
#[derive(Debug)]
pub enum Generated {
    /// A self-signed leaf.
    SelfSigned(SelfSignedCertificate),
    /// A CA and its leaf.
    CaLeaf(CaLeafCertificates),
}

impl Generated {
    /// The leaf a PFX holds.
    fn leaf(&self) -> &IssuedCertificate {
        match self {
            Self::SelfSigned(made) => &made.leaf,
            Self::CaLeaf(made) => &made.leaf,
        }
    }

    /// Its fingerprint, the leaf's.
    fn fingerprint(&self) -> &str {
        match self {
            Self::SelfSigned(made) => &made.fingerprint,
            Self::CaLeaf(made) => &made.fingerprint,
        }
    }
}

/// A copy button of the page.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CertCopy {
    /// The fingerprint.
    Fingerprint,
    /// The certificate shown first: the self-signed one, or the CA.
    Cert,
    /// Its key.
    Key,
    /// The leaf signed by the CA.
    LeafCert,
    /// Its key.
    LeafKey,
}

/// What is saved once its place is picked.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CertSave {
    /// The certificate in PEM.
    Pem,
    /// The PFX built.
    Pfx,
}

/// A box of the page.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CertBox {
    /// The fingerprint's.
    Fingerprint,
    /// The first certificate's.
    Cert,
    /// Its key's.
    Key,
    /// The leaf's.
    LeafCert,
    /// The leaf key's.
    LeafKey,
}

/// What the Certificate Generator is asked.
#[derive(Clone)]
pub enum CertGenMessage {
    /// The common name typed.
    Cn(String),
    /// The organisation typed.
    Org(String),
    /// The country typed.
    Country(String),
    /// The key size chosen.
    KeySize(KeySizeChoice),
    /// The validity typed.
    Validity(String),
    /// The alternative names typed.
    San(String),
    /// The type chosen.
    Mode(CertificateMode),
    /// Make the certificates, as the C# Generate button and Enter in the common name.
    Generate,
    /// Generation `.0` made this, or failed saying why; `None` when it did not finish.
    Generated(u64, Option<Result<Arc<Generated>, String>>),
    /// A copy button pressed.
    Copy(CertCopy),
    /// Show or hide the first key.
    ToggleKey,
    /// Show or hide the leaf's key.
    ToggleLeafKey,
    /// Save the certificate in PEM.
    SavePem,
    /// Save a PFX: its password asked first.
    SavePfx,
    /// The PFX password typed.
    PfxPassword(String),
    /// The PFX password given.
    PfxOk,
    /// The PFX password not given.
    PfxCancel,
    /// Where to save, picked; `None` when cancelled.
    SaveTo(CertSave, Option<PathBuf>),
    /// The file was written, or why not.
    Saved(Result<(), String>),
    /// Something done in a box, which does not change it.
    Box(CertBox, Action),
}

impl fmt::Debug for CertGenMessage {
    /// Keys and passwords are never written out.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Generated(generation, made) => {
                write!(f, "Generated({generation}, {})", made.is_some())
            }
            Self::PfxPassword(_) => f.write_str("PfxPassword(..)"),
            Self::Copy(copy) => write!(f, "Copy({copy:?})"),
            Self::Mode(mode) => write!(f, "Mode({mode:?})"),
            Self::SaveTo(save, _) => write!(f, "SaveTo({save:?}, ..)"),
            Self::Saved(saved) => write!(f, "Saved({})", saved.is_ok()),
            Self::Generate => f.write_str("Generate"),
            _ => f.write_str("CertGenMessage(..)"),
        }
    }
}

/// What an update asks of its tab.
pub enum Outcome {
    /// Nothing more.
    Done,
    /// This copied by its button.
    Copy(CopySlot, String),
    /// The field of this id focused, as the C# `ValidationFocusRequested`.
    Focus(iced::widget::Id),
    /// These certificates made, as generation `.0`.
    Generate(u64, Box<CertificateOptions>, CertificateMode),
    /// The save dialog for this.
    AskSave(CertSave),
    /// These bytes written at this path, readable by the user alone when `private`.
    Write(PathBuf, Zeroizing<Vec<u8>>, bool),
}

impl fmt::Debug for Outcome {
    /// What is copied and the bytes written are never written out.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Done => f.write_str("Done"),
            Self::Copy(slot, _) => write!(f, "Copy({slot:?}, ..)"),
            Self::Focus(_) => f.write_str("Focus(..)"),
            Self::Generate(generation, _, mode) => {
                write!(f, "Generate({generation}, .., {mode:?})")
            }
            Self::AskSave(save) => write!(f, "AskSave({save:?})"),
            Self::Write(path, _, private) => {
                write!(f, "Write({}, .., {private})", path.display())
            }
        }
    }
}

impl Outcome {
    /// What tab `tab` runs for it, its dialogs over the window `main`.
    pub fn task(self, tab: TabId, main: Option<window::Id>) -> Task<Message> {
        let send = move |message| Message::Tool(tab, ToolMessage::CertGen(message));
        match self {
            Self::Done | Self::Copy(..) => Task::none(),
            Self::Focus(id) => operation::focus(id.clone()).chain(operation::select_all(id)),
            Self::Generate(generation, options, mode) => key_parts::off_thread(
                move || generate(&options, mode),
                move |made| send(CertGenMessage::Generated(generation, made)),
            ),
            Self::AskSave(save) => {
                let (file_name, filter, extensions) = match save {
                    CertSave::Pem => (
                        PEM_FILE_NAME,
                        fl!("ui-tool-certgen-pem-filter"),
                        PEM_EXTENSIONS,
                    ),
                    CertSave::Pfx => (
                        PFX_FILE_NAME,
                        fl!("ui-tool-certgen-pfx-filter"),
                        PFX_EXTENSIONS,
                    ),
                };
                key_parts::ask_save(
                    main,
                    fl!("ui-tool-certgen-title"),
                    file_name.to_owned(),
                    filter,
                    extensions,
                    move |path| send(CertGenMessage::SaveTo(save, path)),
                )
            }
            Self::Write(path, bytes, private) => key_parts::off_thread(
                move || {
                    if private {
                        private_file::write_private(&path, &bytes)
                    } else {
                        std::fs::write(&path, bytes.as_slice())
                    }
                    .map_err(|error| error.to_string())
                },
                move |written| {
                    send(CertGenMessage::Saved(
                        written.unwrap_or_else(|| Err(String::new())),
                    ))
                },
            ),
        }
    }
}

/// The certificates of `options` made now, as the C# `CertificateGeneratorService`.
fn generate(options: &CertificateOptions, mode: CertificateMode) -> Result<Arc<Generated>, String> {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |since| since.as_secs());
    match mode {
        CertificateMode::SelfSigned => engine::generate_self_signed(options, now)
            .map(|made| Arc::new(Generated::SelfSigned(made))),
        CertificateMode::CaLeaf => engine::generate_ca_leaf(options, CA_VALIDITY_DAYS, now)
            .map(|made| Arc::new(Generated::CaLeaf(made))),
    }
    .map_err(|error| error.to_string())
}

/// What the line under the Generate button says, as the C# `CertificateMessageKind`.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Said {
    /// A field refused.
    Validation(ValidationCode),
    /// Making.
    Generating,
    /// The generation failed, saying why.
    GenerationError(String),
    /// The PFX failed, saying why.
    ExportError(String),
    /// A file could not be written.
    SaveFailed(String),
}

/// The Certificate Generator's state, as the C# view model's.
pub struct CertGenPane {
    cn: String,
    org: String,
    country: String,
    key_size: KeySizeChoice,
    validity: String,
    san: String,
    mode: CertificateMode,
    generating: bool,
    /// The generation running: the results of another are let go.
    generation: u64,
    result: Option<Arc<Generated>>,
    key_visible: bool,
    leaf_key_visible: bool,
    said: Option<Said>,
    /// The PFX password being typed, while it is asked.
    pfx_password: Option<Zeroizing<String>>,
    /// The PFX built, waiting for its place.
    pending_pfx: Option<Zeroizing<Vec<u8>>>,
    fingerprint_box: Content,
    cert_box: Content,
    key_box: Content,
    leaf_cert_box: Content,
    leaf_key_box: Content,
}

impl fmt::Debug for CertGenPane {
    /// Keys and passwords are never written out.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("CertGenPane")
            .field("mode", &self.mode)
            .field("generating", &self.generating)
            .field("result", &self.result.is_some())
            .finish_non_exhaustive()
    }
}

impl Default for CertGenPane {
    /// A new tab, as the C# view model's defaults: 365 days, RSA 2048, self-signed.
    fn default() -> Self {
        Self {
            cn: String::new(),
            org: String::new(),
            country: String::new(),
            key_size: KeySizeChoice(RSA_2048_BITS),
            validity: DEFAULT_VALIDITY_DAYS.to_string(),
            san: String::new(),
            mode: CertificateMode::SelfSigned,
            generating: false,
            generation: 0,
            result: None,
            key_visible: false,
            leaf_key_visible: false,
            said: None,
            pfx_password: None,
            pending_pfx: None,
            fingerprint_box: Content::new(),
            cert_box: Content::new(),
            key_box: Content::new(),
            leaf_cert_box: Content::new(),
            leaf_key_box: Content::new(),
        }
    }
}

impl CertGenPane {
    /// Applies `message`; what is then to be done.
    pub fn update(&mut self, message: CertGenMessage) -> Outcome {
        match message {
            CertGenMessage::Cn(typed) => self.cn = typed,
            CertGenMessage::Org(typed) => self.org = typed,
            CertGenMessage::Country(typed) => self.country = typed,
            CertGenMessage::KeySize(choice) => self.key_size = choice,
            CertGenMessage::Validity(typed) => self.validity = typed,
            CertGenMessage::San(typed) => self.san = typed,
            CertGenMessage::Mode(mode) => {
                self.mode = mode;
                self.refresh_boxes();
            }
            CertGenMessage::Generate => return self.generate(),
            CertGenMessage::Generated(generation, made) => self.generated(generation, made),
            CertGenMessage::Copy(copy) => return self.copy(copy),
            CertGenMessage::ToggleKey => {
                self.key_visible = !self.key_visible;
                self.refresh_boxes();
            }
            CertGenMessage::ToggleLeafKey => {
                self.leaf_key_visible = !self.leaf_key_visible;
                self.refresh_boxes();
            }
            CertGenMessage::SavePem => {
                if !self.pem_to_save().is_empty() {
                    return Outcome::AskSave(CertSave::Pem);
                }
            }
            CertGenMessage::SavePfx => {
                if self.result.is_some() {
                    self.pfx_password = Some(Zeroizing::default());
                }
            }
            CertGenMessage::PfxPassword(typed) => {
                if self.pfx_password.is_some() {
                    self.pfx_password = Some(Zeroizing::new(typed));
                }
            }
            CertGenMessage::PfxCancel => self.pfx_password = None,
            CertGenMessage::PfxOk => return self.build_pfx(),
            CertGenMessage::SaveTo(save, path) => return self.save_to(save, path),
            CertGenMessage::Saved(Ok(())) => {}
            CertGenMessage::Saved(Err(error)) => self.said = Some(Said::SaveFailed(error)),
            CertGenMessage::Box(slot, action) => {
                let content = match slot {
                    CertBox::Fingerprint => &mut self.fingerprint_box,
                    CertBox::Cert => &mut self.cert_box,
                    CertBox::Key => &mut self.key_box,
                    CertBox::LeafCert => &mut self.leaf_cert_box,
                    CertBox::LeafKey => &mut self.leaf_key_box,
                };
                super::read_only(content, action);
            }
        }
        Outcome::Done
    }

    /// The options read and checked, then made, as the C# `GenerateAsync`
    /// (`CertificateGeneratorViewModel.cs:160-244`).
    fn generate(&mut self) -> Outcome {
        if self.generating {
            return Outcome::Done;
        }
        self.said = None;
        let cn = self.cn.trim().to_owned();
        if cn.is_empty() {
            self.said = Some(Said::Validation(ValidationCode::CnRequired));
            return Outcome::Focus(cn_id());
        }
        let Some(days) = self
            .validity
            .trim()
            .parse::<i64>()
            .ok()
            .filter(|days| *days >= 1)
        else {
            self.said = Some(Said::Validation(ValidationCode::InvalidValidity));
            return Outcome::Focus(validity_id());
        };
        let options = CertificateOptions {
            cn,
            org: self.org.trim().to_owned(),
            country: self.country.trim().to_owned(),
            key_bits: self.key_size.0,
            validity_days: days,
            sans: engine::parse_sans(&self.san),
        };
        match options.validate() {
            ValidationCode::Ok => {}
            code => {
                self.said = Some(Said::Validation(code));
                return Outcome::Focus(if code == ValidationCode::CnRequired {
                    cn_id()
                } else {
                    validity_id()
                });
            }
        }
        self.generating = true;
        self.generation += 1;
        self.said = Some(Said::Generating);
        Outcome::Generate(self.generation, Box::new(options), self.mode)
    }

    /// Generation `generation` ended with `made`, as the C# `GenerateAsync`'s end.
    fn generated(&mut self, generation: u64, made: Option<Result<Arc<Generated>, String>>) {
        if generation != self.generation {
            return;
        }
        self.generating = false;
        match made {
            Some(Ok(made)) => {
                self.result = Some(made);
                self.key_visible = false;
                self.leaf_key_visible = false;
                self.said = None;
                self.pending_pfx = None;
            }
            Some(Err(error)) => self.said = Some(Said::GenerationError(error)),
            None => self.said = Some(Said::GenerationError(String::new())),
        }
        self.refresh_boxes();
    }

    /// The certificate and key shown first, as the C# `CurrentCertPem` and `CurrentKeyPem`:
    /// the CA's in CA mode, the self-signed one's otherwise; empty when what was made is of
    /// the other kind.
    fn current(&self) -> Option<&IssuedCertificate> {
        match (self.result.as_deref(), self.mode) {
            (Some(Generated::SelfSigned(made)), CertificateMode::SelfSigned) => Some(&made.leaf),
            (Some(Generated::CaLeaf(made)), CertificateMode::CaLeaf) => Some(&made.ca),
            _ => None,
        }
    }

    /// The CA's leaf, as the C# `LeafCertPem` and `LeafKeyPem`.
    fn leaf(&self) -> Option<&IssuedCertificate> {
        match self.result.as_deref() {
            Some(Generated::CaLeaf(made)) => Some(&made.leaf),
            _ => None,
        }
    }

    /// What Save .pem writes, as the C# `SavePem`: the leaf in CA mode.
    fn pem_to_save(&self) -> String {
        if self.mode == CertificateMode::CaLeaf {
            self.leaf().map(|leaf| leaf.cert_pem.clone())
        } else {
            self.current().map(|current| current.cert_pem.clone())
        }
        .unwrap_or_default()
    }

    /// The boxes written from what was made, the keys masked unless shown.
    fn refresh_boxes(&mut self) {
        let shown = |issued: Option<&IssuedCertificate>, visible: bool| {
            if visible {
                issued
                    .map(|issued| issued.key_pem.to_string())
                    .unwrap_or_default()
            } else {
                MASKED_PLACEHOLDER.to_owned()
            }
        };
        let fingerprint = self
            .result
            .as_deref()
            .map(|made| made.fingerprint().to_owned())
            .unwrap_or_default();
        let cert = self
            .current()
            .map(|issued| issued.cert_pem.clone())
            .unwrap_or_default();
        let key = shown(self.current(), self.key_visible);
        let leaf_cert = self
            .leaf()
            .map(|issued| issued.cert_pem.clone())
            .unwrap_or_default();
        let leaf_key = shown(self.leaf(), self.leaf_key_visible);
        self.fingerprint_box = Content::with_text(&fingerprint);
        self.cert_box = Content::with_text(&cert);
        self.key_box = Content::with_text(&Zeroizing::new(key));
        self.leaf_cert_box = Content::with_text(&leaf_cert);
        self.leaf_key_box = Content::with_text(&Zeroizing::new(leaf_key));
    }

    /// What a copy button copies, as the C# `Copy*` commands: nothing when empty.
    fn copy(&self, copy: CertCopy) -> Outcome {
        let (slot, content) = match copy {
            CertCopy::Fingerprint => (
                CopySlot::CertFingerprint,
                self.result
                    .as_deref()
                    .map(|made| made.fingerprint().to_owned()),
            ),
            CertCopy::Cert => (
                CopySlot::CertCert,
                self.current().map(|c| c.cert_pem.clone()),
            ),
            CertCopy::Key => (
                CopySlot::CertKey,
                self.current().map(|c| c.key_pem.to_string()),
            ),
            CertCopy::LeafCert => (
                CopySlot::CertLeafCert,
                self.leaf().map(|c| c.cert_pem.clone()),
            ),
            CertCopy::LeafKey => (
                CopySlot::CertLeafKey,
                self.leaf().map(|c| c.key_pem.to_string()),
            ),
        };
        match content {
            Some(content) if !content.is_empty() => Outcome::Copy(slot, content),
            _ => Outcome::Done,
        }
    }

    /// The PFX built with the password typed, then its place asked, as the C# `SavePfx`.
    fn build_pfx(&mut self) -> Outcome {
        let Some(password) = self.pfx_password.take() else {
            return Outcome::Done;
        };
        let Some(made) = self.result.clone() else {
            return Outcome::Done;
        };
        match engine::build_pfx(made.leaf(), &password) {
            Ok(bytes) => {
                self.pending_pfx = Some(Zeroizing::new(bytes));
                Outcome::AskSave(CertSave::Pfx)
            }
            Err(error) => {
                self.said = Some(Said::ExportError(error.to_string()));
                Outcome::Done
            }
        }
    }

    /// `save` written at `path`, when one was picked.
    fn save_to(&mut self, save: CertSave, path: Option<PathBuf>) -> Outcome {
        let Some(path) = path else {
            if save == CertSave::Pfx {
                self.pending_pfx = None;
            }
            return Outcome::Done;
        };
        match save {
            CertSave::Pem => {
                let pem = self.pem_to_save();
                if pem.is_empty() {
                    return Outcome::Done;
                }
                Outcome::Write(path, Zeroizing::new(pem.into_bytes()), false)
            }
            CertSave::Pfx => self
                .pending_pfx
                .take()
                .map_or(Outcome::Done, |bytes| Outcome::Write(path, bytes, true)),
        }
    }

    /// What the line under the Generate button says, and whether it is an error.
    fn said_text(&self) -> Option<(String, Tone)> {
        Some(match self.said.as_ref()? {
            Said::Validation(ValidationCode::CnRequired) => {
                (fl!("ui-tool-certgen-error-cn-required"), Tone::Error)
            }
            Said::Validation(_) => (fl!("ui-tool-certgen-error-invalid-validity"), Tone::Error),
            Said::Generating => (fl!("ui-tool-certgen-generating"), Tone::Quiet),
            Said::GenerationError(error) => (
                fl!("ui-tool-certgen-error-generation", error = error.as_str()),
                Tone::Error,
            ),
            Said::ExportError(error) => (
                fl!("ui-tool-certgen-error-export", error = error.as_str()),
                Tone::Error,
            ),
            Said::SaveFailed(error) => (
                fl!("ui-tool-certgen-save-failed", error = error.as_str()),
                Tone::Error,
            ),
        })
    }

    /// The tool's page, as the C# `CertificateGeneratorView.xaml`.
    pub fn view<'a>(&'a self, tab: TabId, state: &ToolPane) -> Element<'a, Message> {
        let send = move |message| Message::Tool(tab, ToolMessage::CertGen(message));
        let field = |placeholder: &str, value: &'a str| {
            text_input(placeholder, value)
                .size(font_size::BODY_LARGE)
                .padding(super::INPUT_PADDING)
                .style(styles::text_input)
        };
        let subject = key_parts::card(key_parts::stack([
            key_parts::section_title(fl!("ui-tool-certgen-subject")),
            key_parts::labeled(
                fl!("ui-tool-certgen-cn"),
                field("", &self.cn)
                    .id(cn_id())
                    .on_input(move |typed| send(CertGenMessage::Cn(typed)))
                    .on_submit(send(CertGenMessage::Generate)),
            ),
            key_parts::labeled(
                fl!("ui-tool-certgen-org"),
                field("", &self.org).on_input(move |typed| send(CertGenMessage::Org(typed))),
            ),
            key_parts::labeled(
                fl!("ui-tool-certgen-country"),
                field("", &self.country)
                    .on_input(move |typed| send(CertGenMessage::Country(typed))),
            ),
        ]));
        let options = key_parts::card(key_parts::stack([
            key_parts::section_title(fl!("ui-tool-certgen-options")),
            key_parts::labeled(
                fl!("ui-tool-certgen-key-size"),
                pick_list(KeySizeChoice::ALL, Some(self.key_size), move |choice| {
                    send(CertGenMessage::KeySize(choice))
                })
                .width(KEY_SIZE_WIDTH)
                .style(styles::pick_list)
                .menu_style(styles::menu)
                .text_size(font_size::BODY),
            ),
            key_parts::labeled(
                fl!("ui-tool-certgen-validity"),
                field("", &self.validity)
                    .id(validity_id())
                    .on_input(move |typed| send(CertGenMessage::Validity(typed))),
            ),
            key_parts::labeled(
                fl!("ui-tool-certgen-san"),
                field("", &self.san).on_input(move |typed| send(CertGenMessage::San(typed))),
            ),
            key_parts::hint(fl!("ui-tool-certgen-san-hint")),
        ]));
        let kind = key_parts::card(key_parts::stack([
            key_parts::section_title(fl!("ui-tool-certgen-type")),
            row![
                radio(
                    fl!("ui-tool-certgen-type-self-signed"),
                    CertificateMode::SelfSigned,
                    Some(self.mode),
                    move |mode| send(CertGenMessage::Mode(mode)),
                )
                .text_size(font_size::BODY),
                radio(
                    fl!("ui-tool-certgen-type-ca-leaf"),
                    CertificateMode::CaLeaf,
                    Some(self.mode),
                    move |mode| send(CertGenMessage::Mode(mode)),
                )
                .text_size(font_size::BODY),
            ]
            .spacing(CHOICE_GAP)
            .into(),
        ]));
        let generate = super::action_button(
            fl!("ui-tool-certgen-generate"),
            true,
            (!self.generating).then_some(send(CertGenMessage::Generate)),
        );
        let said = self
            .said_text()
            .map(|(said, tone)| crypto_parts::said(said, tone, font_size::BODY, false));
        let mut page = column![subject, options, kind, generate]
            .push(said)
            .push(self.pfx_prompt(send))
            .spacing(spacing::MD);
        if self.result.is_some() {
            page = page.push(self.results(send, state));
        }
        super::content_column(page)
    }

    /// The PFX password's prompt, while it is asked, as the C# `PromptPfxPassword` window.
    fn pfx_prompt<'a>(
        &'a self,
        send: impl Fn(CertGenMessage) -> Message + Copy + 'a,
    ) -> Option<Element<'a, Message>> {
        let password = self.pfx_password.as_ref()?;
        Some(key_parts::card(key_parts::stack([
            key_parts::section_title(fl!("ui-tool-certgen-pfx-password-title")),
            key_parts::hint(fl!("ui-tool-certgen-pfx-password-prompt")),
            text_input("", password)
                .secure(true)
                .size(font_size::BODY_LARGE)
                .padding(super::INPUT_PADDING)
                .style(styles::text_input)
                .on_input(move |typed| send(CertGenMessage::PfxPassword(typed)))
                .on_submit(send(CertGenMessage::PfxOk))
                .into(),
            row![
                super::action_button(
                    fl!("ui-tool-certgen-ok"),
                    true,
                    Some(send(CertGenMessage::PfxOk))
                ),
                super::action_button(
                    fl!("ui-tool-certgen-cancel"),
                    false,
                    Some(send(CertGenMessage::PfxCancel))
                ),
            ]
            .spacing(spacing::SM)
            .into(),
        ])))
    }

    /// The fingerprint, the certificates, the keys and the export buttons, as the C#'s
    /// panels shown once something is made.
    #[expect(
        clippy::too_many_lines,
        reason = "one panel per certificate and key, as the C#"
    )]
    fn results<'a>(
        &'a self,
        send: impl Fn(CertGenMessage) -> Message + Copy + 'a,
        state: &ToolPane,
    ) -> Element<'a, Message> {
        let copy = |slot: CopySlot, copy: CertCopy| {
            super::copy_button(
                fl!("ui-tool-certgen-copy"),
                state.copied(slot),
                send(CertGenMessage::Copy(copy)),
                super::COPY_PADDING,
            )
        };
        let toggle = |visible: bool, message: CertGenMessage| {
            super::action_button(
                if visible {
                    fl!("ui-tool-certgen-hide")
                } else {
                    fl!("ui-tool-certgen-show")
                },
                false,
                Some(send(message)),
            )
        };
        let pem_box = |content: &'a Content, slot: CertBox, height: f32| {
            super::text_box(content, None)
                .height(height)
                .on_action(move |action| send(CertGenMessage::Box(slot, action)))
        };
        let (cert_label, key_label) = if self.mode == CertificateMode::CaLeaf {
            (
                fl!("ui-tool-certgen-ca-cert-pem"),
                fl!("ui-tool-certgen-ca-key-pem"),
            )
        } else {
            (
                fl!("ui-tool-certgen-cert-pem"),
                fl!("ui-tool-certgen-key-pem"),
            )
        };
        let heading = |label: String, buttons: Vec<Element<'a, Message>>| {
            row![super::field_label(label)]
                .extend(buttons)
                .spacing(spacing::SM)
                .align_y(Alignment::Center)
        };
        let mut panels = column![
            key_parts::card(key_parts::stack([
                heading(
                    fl!("ui-tool-certgen-fingerprint"),
                    vec![copy(CopySlot::CertFingerprint, CertCopy::Fingerprint)]
                )
                .into(),
                pem_box(
                    &self.fingerprint_box,
                    CertBox::Fingerprint,
                    FINGERPRINT_HEIGHT
                )
                .into(),
            ])),
            key_parts::card(key_parts::stack([
                heading(cert_label, vec![copy(CopySlot::CertCert, CertCopy::Cert)]).into(),
                pem_box(&self.cert_box, CertBox::Cert, PEM_HEIGHT).into(),
            ])),
            key_parts::card(key_parts::stack([
                heading(
                    key_label,
                    vec![
                        copy(CopySlot::CertKey, CertCopy::Key),
                        toggle(self.key_visible, CertGenMessage::ToggleKey),
                    ]
                )
                .into(),
                pem_box(&self.key_box, CertBox::Key, PEM_HEIGHT).into(),
            ])),
        ]
        .spacing(spacing::MD);
        if self.mode == CertificateMode::CaLeaf {
            panels = panels.push(key_parts::card(key_parts::stack([
                heading(
                    fl!("ui-tool-certgen-leaf-cert-pem"),
                    vec![copy(CopySlot::CertLeafCert, CertCopy::LeafCert)],
                )
                .into(),
                pem_box(&self.leaf_cert_box, CertBox::LeafCert, PEM_HEIGHT).into(),
                heading(
                    fl!("ui-tool-certgen-leaf-key-pem"),
                    vec![
                        copy(CopySlot::CertLeafKey, CertCopy::LeafKey),
                        toggle(self.leaf_key_visible, CertGenMessage::ToggleLeafKey),
                    ],
                )
                .into(),
                pem_box(&self.leaf_key_box, CertBox::LeafKey, PEM_HEIGHT).into(),
            ])));
        }
        panels
            .push(
                row![
                    super::action_button(
                        fl!("ui-tool-certgen-save-pem"),
                        false,
                        Some(send(CertGenMessage::SavePem))
                    ),
                    super::action_button(
                        fl!("ui-tool-certgen-save-pfx"),
                        false,
                        Some(send(CertGenMessage::SavePfx))
                    ),
                ]
                .spacing(spacing::SM),
            )
            .width(Length::Fill)
            .into()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_outcome_never_writes_out_its_secrets() {
        let write = Outcome::Write(
            PathBuf::from("c.pfx"),
            Zeroizing::new(b"PRIVATE".to_vec()),
            true,
        );
        let copy = Outcome::Copy(CopySlot::CertKey, "PRIVATE".to_owned());
        let shown = format!("{write:?} {copy:?}");
        assert!(
            !shown.contains("PRIVATE") && !shown.contains("80, 82"),
            "{shown}"
        );
    }

    fn made(pane: &mut CertGenPane) {
        let Outcome::Generate(generation, options, mode) = pane.update(CertGenMessage::Generate)
        else {
            panic!("asked to generate");
        };
        let result = generate(&options, mode);
        pane.update(CertGenMessage::Generated(generation, Some(result)));
    }

    #[test]
    fn the_common_name_and_the_validity_are_checked_as_the_csharp() {
        let mut pane = CertGenPane::default();
        assert_eq!(pane.validity, "365");
        assert!(matches!(
            pane.update(CertGenMessage::Generate),
            Outcome::Focus(_)
        ));
        assert_eq!(
            pane.said,
            Some(Said::Validation(ValidationCode::CnRequired))
        );
        pane.update(CertGenMessage::Cn("server.local".to_owned()));
        pane.update(CertGenMessage::Validity("0".to_owned()));
        assert!(matches!(
            pane.update(CertGenMessage::Generate),
            Outcome::Focus(_)
        ));
        assert_eq!(
            pane.said,
            Some(Said::Validation(ValidationCode::InvalidValidity))
        );
        pane.update(CertGenMessage::Validity("30".to_owned()));
        assert!(matches!(
            pane.update(CertGenMessage::Generate),
            Outcome::Generate(1, _, CertificateMode::SelfSigned)
        ));
        assert_eq!(pane.said, Some(Said::Generating));
        assert!(
            matches!(pane.update(CertGenMessage::Generate), Outcome::Done),
            "one at a time"
        );
    }

    #[test]
    fn a_result_shows_its_certificate_its_key_masked_and_copies_both() {
        let mut pane = CertGenPane::default();
        pane.update(CertGenMessage::Cn("server.local".to_owned()));
        made(&mut pane);
        assert!(pane.said.is_none() && !pane.generating);
        assert!(
            pane.cert_box
                .text()
                .starts_with("-----BEGIN CERTIFICATE-----")
        );
        assert_eq!(pane.key_box.text().trim_end(), MASKED_PLACEHOLDER);
        assert!(pane.fingerprint_box.text().starts_with("SHA256:"));
        let Outcome::Copy(CopySlot::CertKey, key) =
            pane.update(CertGenMessage::Copy(CertCopy::Key))
        else {
            panic!("the key copied");
        };
        assert!(
            key.starts_with("-----BEGIN PRIVATE KEY-----"),
            "the key, not its mask"
        );
        pane.update(CertGenMessage::ToggleKey);
        assert!(
            pane.key_box
                .text()
                .starts_with("-----BEGIN PRIVATE KEY-----")
        );
        assert!(matches!(
            pane.update(CertGenMessage::SavePem),
            Outcome::AskSave(CertSave::Pem)
        ));
        let shown = format!("{pane:?}");
        assert!(!shown.contains("PRIVATE"), "{shown}");
    }

    #[test]
    fn a_ca_shows_its_leaf_and_a_pfx_asks_its_password_then_its_place() {
        let mut pane = CertGenPane::default();
        pane.update(CertGenMessage::Cn("server.local".to_owned()));
        pane.update(CertGenMessage::Mode(CertificateMode::CaLeaf));
        made(&mut pane);
        assert!(
            pane.leaf_cert_box
                .text()
                .starts_with("-----BEGIN CERTIFICATE-----")
        );
        assert!(matches!(
            pane.update(CertGenMessage::SavePfx),
            Outcome::Done
        ));
        assert!(pane.pfx_password.is_some());
        pane.update(CertGenMessage::PfxPassword("secret".to_owned()));
        assert!(matches!(
            pane.update(CertGenMessage::PfxOk),
            Outcome::AskSave(CertSave::Pfx)
        ));
        assert!(pane.pfx_password.is_none() && pane.pending_pfx.is_some());
        let path = PathBuf::from("certificate.pfx");
        assert!(matches!(
            pane.update(CertGenMessage::SaveTo(CertSave::Pfx, Some(path))),
            Outcome::Write(_, _, true)
        ));
        assert!(
            pane.pending_pfx.is_none(),
            "handed to the write, kept nowhere"
        );
        // The self-signed view of a CA result is empty, as the C#'s.
        pane.update(CertGenMessage::Mode(CertificateMode::SelfSigned));
        assert!(pane.cert_box.text().trim().is_empty());
    }

    #[test]
    fn a_result_of_a_generation_let_go_is_ignored() {
        let mut pane = CertGenPane::default();
        pane.update(CertGenMessage::Generated(7, Some(Err("late".to_owned()))));
        assert!(pane.said.is_none());
    }
}
