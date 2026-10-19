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

//! What the migration of this computer's C# store carries over from its `settings.json`
//! besides the profiles: its preferences, offered for review as a settings file's import
//! shows them and taken only once confirmed, and its trusted RDP and FTPS certificates,
//! trusted here as the C# trusted them, as its trusted SSH servers are.

use heimdall_core::import::csharp_settings::{
    self, PreferenceOutcome, PreferenceRow, TrustedCertificate,
};
use heimdall_core::settings::SettingsImport;
use heimdall_rdp::{CarriedCertificate, CertificateHash, KnownRdpHosts};

use super::{App, Dialog};

/// The preferences of the C# `settings.json`, as the import report says them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportedPreferences {
    /// What taking the preferences carried over changes here, shown before it is taken;
    /// `None` when they change nothing, or once opened.
    pub review: Option<Box<SettingsImport>>,
    /// How many preferences are carried over, to be reviewed.
    pub taken: usize,
    /// Each preference left out, by its C# name, and why: changed here, a secret, a value
    /// not read, or a path of the C# installation.
    pub left_out: Vec<PreferenceRow>,
}

impl ImportedPreferences {
    /// The C# names of the preferences left out for `outcome`.
    pub fn left_out(&self, outcome: PreferenceOutcome) -> impl Iterator<Item = &str> {
        self.left_out
            .iter()
            .filter(move |row| row.outcome == outcome)
            .map(|row| row.key.as_str())
    }
}

/// What carrying the C# trusted certificates over did.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct CarriedCertificates {
    /// RDP certificates now trusted.
    pub rdp: usize,
    /// FTPS certificates now trusted.
    pub ftps: usize,
    /// Certificates left out that read: their server already trusted here, their RDP
    /// profile not imported, a destination typed by hand, or one the user never confirmed.
    pub skipped: usize,
    /// Certificates left out that do not read.
    pub invalid: usize,
}

/// An RDP profile imported, as its certificates are bound: its identifier, host and port.
pub(super) type RdpTarget = (String, String, u16);

/// `certificate`, as the trust store takes it.
fn carried(certificate: &TrustedCertificate) -> CarriedCertificate {
    CarriedCertificate {
        host: certificate.host.clone(),
        port: certificate.port,
        certificate: CertificateHash::from_sha256(certificate.sha256),
        trusted: certificate.trusted,
        subject: certificate.subject.clone(),
        issuer: certificate.issuer.clone(),
    }
}

impl App {
    /// The preferences of the C# `settings.json` `text`, laid over these settings: none
    /// changed here, no secret; or why they could not be read.
    pub(super) fn csharp_preferences(&self, text: &str) -> Result<ImportedPreferences, String> {
        let read = csharp_settings::preferences(text, &self.settings)
            .map_err(|error| error.to_string())?;
        let taken = read.count(PreferenceOutcome::Taken);
        let review = self.settings.csharp_import(&read);
        Ok(ImportedPreferences {
            review: (!review.changes.is_empty()).then(|| Box::new(review)),
            taken,
            left_out: read
                .rows
                .into_iter()
                .filter(|row| row.outcome != PreferenceOutcome::Taken)
                .collect(),
        })
    }

    /// Trusts the RDP and FTPS certificates of the C# `settings.json` `text`, the RDP ones
    /// for the server of the profile of `rdp` that trusted them; or why they could not be.
    pub(super) fn carry_certificates(
        &self,
        text: &str,
        rdp: &[RdpTarget],
    ) -> Result<CarriedCertificates, String> {
        let targets: Vec<csharp_settings::RdpTarget<'_>> = rdp
            .iter()
            .map(|(id, host, port)| (id.as_str(), host.as_str(), *port))
            .collect();
        let read = csharp_settings::trusted_certificates(text, &targets)
            .map_err(|error| error.to_string())?;
        let failed = |error: std::io::Error| error.to_string();
        let to_carry = |list: &[TrustedCertificate]| -> Vec<CarriedCertificate> {
            list.iter().map(carried).collect()
        };
        let rdp = KnownRdpHosts::new(self.known_rdp_hosts())
            .carry_over(&to_carry(&read.rdp))
            .map_err(failed)?;
        let ftps = KnownRdpHosts::new(self.known_ftps_hosts())
            .carry_over(&to_carry(&read.ftps))
            .map_err(failed)?;
        Ok(CarriedCertificates {
            rdp: rdp.carried,
            ftps: ftps.carried,
            skipped: read.skipped + rdp.left + ftps.left,
            invalid: read.invalid,
        })
    }

    /// The import report's "Review": the preferences carried over shown as a settings
    /// file's import shows them, taken once confirmed. Nothing when there is none.
    pub(super) fn review_csharp_preferences(&mut self) {
        let review = match &mut self.dialog {
            Some(Dialog::ImportDone(summary)) => match &mut summary.preferences {
                Some(Ok(preferences)) => preferences.review.take(),
                _ => None,
            },
            _ => None,
        };
        if let Some(review) = review {
            self.dialog = Some(Dialog::ConfirmSettingsImport(review));
        }
    }
}
