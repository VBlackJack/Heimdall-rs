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

//! The SMB protocol enumerator tool, mirroring the C# `SmbEnumeratorView` and `SmbEnumeratorViewModel`:
//! extracts host identity (NetBIOS/DNS names, domain, forest, OS build, MAC) and SMB protocol metadata
//! (dialect revision, signing enforcement, server GUID, uptime) without authentication.

#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::doc_markdown,
    clippy::fn_params_excessive_bools,
    clippy::module_name_repetitions,
    clippy::redundant_closure_for_method_calls,
    clippy::too_many_lines
)]

use std::time::{Duration, Instant};

use heimdall_app::TabId;
use heimdall_core::tools::smb::{
    NtlmInfo, SmbEnumerationResult, SmbFindingSeverity, SmbNegotiateInfo, build_netbios_header,
    build_netbios_nbstat_query, build_ntlmssp_type1, build_report, build_result,
    build_smb2_negotiate, build_smb2_session_setup, find_ntlmssp_message, parse_negotiate_response,
    parse_netbios_response, parse_ntlm_type2, read_netbios_length, wrap_in_spnego,
};
use iced::widget::{column, container, progress_bar, row, text, text_input};
use iced::{Alignment, Element, Length, Task, Theme};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpStream, UdpSocket};
use tokio::time::timeout;

use super::{CopySlot, ToolMessage, ToolPane, key_parts};
use crate::i18n::fl;
use crate::shell::Message;
use crate::styles;
use crate::tokens::{font_size, spacing};

/// Probe timeouts.
const SMB_TIMEOUT: Duration = Duration::from_millis(3000);
const NETBIOS_TIMEOUT: Duration = Duration::from_millis(1500);

/// Messages dispatched by the SMB enumerator view.
#[derive(Debug, Clone)]
pub enum SmbMessage {
    /// Target host changed.
    HostChanged(String),
    /// Start enumeration.
    Enumerate,
    /// Cancel ongoing enumeration.
    Cancel,
    /// Enumeration completed: (id, result, elapsed_ms).
    Finished(u64, Result<SmbEnumerationResult, String>, u128),
    /// Copy formatted report to clipboard.
    CopyReport,
}

/// Action outcome from updating the tool pane.
pub enum Outcome {
    /// No action.
    None,
    /// Asynchronous task to execute.
    Task(Task<Message>),
    /// Copy text into a copy slot.
    Copy(CopySlot, String),
}

/// SMB Enumerator view state.
#[derive(Debug, Default)]
pub struct SmbPane {
    host: String,
    is_enumerating: bool,
    status_message: Option<String>,
    error: Option<String>,
    result: Option<SmbEnumerationResult>,
    report_text: String,
    enum_id: u64,
}

impl SmbPane {
    /// Updates the view state in response to a message.
    pub fn update(&mut self, message: SmbMessage, tab: TabId) -> Outcome {
        match message {
            SmbMessage::HostChanged(val) => {
                self.host = val;
                self.error = None;
                Outcome::None
            }
            SmbMessage::Enumerate => {
                let trimmed = self.host.trim();
                if trimmed.is_empty() {
                    self.error = Some(fl!("ui-tool-smb-error-host-required"));
                    return Outcome::None;
                }

                self.is_enumerating = true;
                self.error = None;
                self.status_message = None;
                self.result = None;
                self.report_text.clear();
                self.enum_id = self.enum_id.wrapping_add(1);
                let current_id = self.enum_id;

                let target = trimmed.to_owned();
                let task = Task::perform(
                    async move {
                        let start = Instant::now();
                        let outcome = execute_enumeration(&target).await;
                        let elapsed = start.elapsed().as_millis();
                        (current_id, outcome, elapsed)
                    },
                    move |(id, outcome, elapsed)| {
                        Message::Tool(
                            tab,
                            ToolMessage::Smb(SmbMessage::Finished(id, outcome, elapsed)),
                        )
                    },
                );
                Outcome::Task(task)
            }
            SmbMessage::Cancel => {
                self.is_enumerating = false;
                self.enum_id = self.enum_id.wrapping_add(1);
                Outcome::None
            }
            SmbMessage::Finished(id, outcome, elapsed) => {
                if id != self.enum_id || !self.is_enumerating {
                    return Outcome::None;
                }
                self.is_enumerating = false;
                self.status_message = Some(format!("{elapsed} ms"));

                match outcome {
                    Ok(res) => {
                        self.report_text = build_report(&res, localize_key);
                        self.result = Some(res);
                        self.error = None;
                    }
                    Err(err) => {
                        self.error = Some(err);
                        self.result = None;
                    }
                }
                Outcome::None
            }
            SmbMessage::CopyReport => {
                if !self.report_text.is_empty() {
                    Outcome::Copy(CopySlot::SmbReport, self.report_text.clone())
                } else {
                    Outcome::None
                }
            }
        }
    }

    /// Renders the SMB Enumerator tool view.
    pub fn view<'a>(
        &'a self,
        tab: TabId,
        state: &'a ToolPane,
        theme: &Theme,
    ) -> Element<'a, Message> {
        let send = move |msg| Message::Tool(tab, ToolMessage::Smb(msg));

        // 1. Host input & Action button
        let host_input = text_input(&fl!("ui-tool-smb-host-placeholder"), &self.host)
            .on_input(move |val| send(SmbMessage::HostChanged(val)))
            .on_submit(send(SmbMessage::Enumerate))
            .font(super::BOX_FONT)
            .size(font_size::BODY_LARGE)
            .padding(super::INPUT_PADDING);

        let action_btn = if self.is_enumerating {
            super::action_button(
                fl!("ui-tool-smb-btn-stop"),
                false,
                Some(send(SmbMessage::Cancel)),
            )
        } else {
            super::action_button(
                fl!("ui-tool-smb-btn-enum"),
                true,
                Some(send(SmbMessage::Enumerate)),
            )
        };

        let input_row = row![
            column![
                super::field_label(fl!("ui-tool-smb-host-label")),
                host_input,
            ]
            .spacing(spacing::XS)
            .width(Length::Fill),
            container(action_btn).align_y(Alignment::End),
        ]
        .spacing(spacing::SM)
        .align_y(Alignment::End);

        let input_card = key_parts::card(column![input_row]);

        let mut body_col = column![input_card].spacing(spacing::MD);

        // 2. Loading indicator
        if self.is_enumerating {
            body_col = body_col.push(progress_bar(0.0..=1.0, 0.5).length(Length::Fill).girth(4.0));
        }

        // 3. Error line
        if let Some(err) = &self.error {
            body_col = body_col.push(super::error_line(err.clone()));
        }

        // 4. Empty state or Results
        if self.result.is_none() && !self.is_enumerating && self.error.is_none() {
            body_col = body_col.push(super::empty_state(fl!("ui-tool-smb-empty-state")));
        } else if let Some(res) = &self.result {
            // Identity Card
            body_col = body_col.push(self.render_identity_card(res));

            // Protocol Card (if SMB negotiate info was present)
            if res.dialect_raw.is_some() {
                body_col = body_col.push(self.render_protocol_card(res));
            }

            // Findings Card (if any findings)
            if !res.findings.is_empty() {
                body_col = body_col.push(self.render_findings_card(res, theme));
            }

            // Footer / Copy Card
            let copy_btn = super::copy_button(
                fl!("ui-tool-smb-btn-copy"),
                state.copied(CopySlot::SmbReport),
                send(SmbMessage::CopyReport),
                super::COPY_PADDING,
            );

            let status_text = self.status_message.as_deref().unwrap_or("");
            let footer_row = row![
                text(status_text)
                    .font(super::BOX_FONT)
                    .size(font_size::BODY)
                    .style(text::secondary)
                    .width(Length::Fill),
                copy_btn,
            ]
            .spacing(spacing::SM)
            .align_y(Alignment::Center);

            body_col = body_col.push(key_parts::card(column![footer_row]));
        }

        super::content_column(body_col)
    }

    fn render_identity_card<'a>(&'a self, res: &'a SmbEnumerationResult) -> Element<'a, Message> {
        let mut card_col = column![key_parts::section_title(fl!("ui-tool-smb-card-identity"))]
            .spacing(spacing::SM);

        card_col = card_col.push(key_parts::labeled(
            fl!("ui-tool-smb-computer-name"),
            text(field_str(&res.computer_name))
                .font(super::BOX_FONT)
                .size(font_size::BODY),
        ));
        card_col = card_col.push(key_parts::labeled(
            fl!("ui-tool-smb-domain"),
            text(field_str(&res.domain))
                .font(super::BOX_FONT)
                .size(font_size::BODY),
        ));
        card_col = card_col.push(key_parts::labeled(
            fl!("ui-tool-smb-dns-name"),
            text(field_str(&res.dns_name))
                .font(super::BOX_FONT)
                .size(font_size::BODY),
        ));
        card_col = card_col.push(key_parts::labeled(
            fl!("ui-tool-smb-dns-domain"),
            text(field_str(&res.dns_domain))
                .font(super::BOX_FONT)
                .size(font_size::BODY),
        ));
        card_col = card_col.push(key_parts::labeled(
            fl!("ui-tool-smb-forest"),
            text(field_str(&res.forest))
                .font(super::BOX_FONT)
                .size(font_size::BODY),
        ));
        card_col = card_col.push(key_parts::labeled(
            fl!("ui-tool-smb-os-build"),
            text(field_str(&res.os_build))
                .font(super::BOX_FONT)
                .size(font_size::BODY),
        ));
        card_col = card_col.push(key_parts::labeled(
            fl!("ui-tool-smb-mac"),
            text(field_str(&res.mac_address))
                .font(super::BOX_FONT)
                .size(font_size::BODY),
        ));

        key_parts::card(card_col)
    }

    fn render_protocol_card<'a>(&'a self, res: &'a SmbEnumerationResult) -> Element<'a, Message> {
        let mut card_col = column![key_parts::section_title(fl!("ui-tool-smb-card-protocol"))]
            .spacing(spacing::SM);

        card_col = card_col.push(key_parts::labeled(
            fl!("ui-tool-smb-dialect"),
            text(field_str(&res.dialect))
                .font(super::BOX_FONT)
                .size(font_size::BODY),
        ));

        let signing_str = match res.signing_required {
            Some(true) => fl!("ui-tool-smb-yes"),
            Some(false) => fl!("ui-tool-smb-no"),
            None => fl!("ui-tool-smb-not-available"),
        };
        card_col = card_col.push(key_parts::labeled(
            fl!("ui-tool-smb-signing"),
            text(signing_str)
                .font(super::BOX_FONT)
                .size(font_size::BODY),
        ));
        card_col = card_col.push(key_parts::labeled(
            fl!("ui-tool-smb-server-guid"),
            text(field_str(&res.server_guid))
                .font(super::BOX_FONT)
                .size(font_size::BODY),
        ));
        card_col = card_col.push(key_parts::labeled(
            fl!("ui-tool-smb-system-time"),
            text(field_str(&res.system_time))
                .font(super::BOX_FONT)
                .size(font_size::BODY),
        ));
        card_col = card_col.push(key_parts::labeled(
            fl!("ui-tool-smb-boot-time"),
            text(field_str(&res.boot_time))
                .font(super::BOX_FONT)
                .size(font_size::BODY),
        ));

        key_parts::card(card_col)
    }

    fn render_findings_card<'a>(
        &'a self,
        res: &'a SmbEnumerationResult,
        theme: &Theme,
    ) -> Element<'a, Message> {
        let palette = theme.extended_palette();

        let mut card_col = column![key_parts::section_title(fl!("ui-tool-smb-card-findings"))]
            .spacing(spacing::SM);

        for finding in &res.findings {
            let (icon, color) = match finding.severity {
                SmbFindingSeverity::Success => ("[OK]", palette.success.base.color),
                SmbFindingSeverity::Warning => ("[WARN]", palette.warning.base.color),
                SmbFindingSeverity::Critical => ("[CRIT]", palette.danger.base.color),
                SmbFindingSeverity::Info => ("[INFO]", palette.secondary.base.color),
            };

            let localized = localize_finding(finding);

            let row = row![
                text(icon)
                    .font(super::BOX_FONT)
                    .size(font_size::CAPTION)
                    .font(styles::SEMIBOLD)
                    .color(color)
                    .width(Length::Fixed(50.0)),
                text(localized)
                    .size(font_size::BODY)
                    .color(color)
                    .width(Length::Fill),
            ]
            .spacing(spacing::SM)
            .align_y(Alignment::Center);

            card_col = card_col.push(row);
        }

        key_parts::card(card_col)
    }
}

// ── Networking Engine ───────────────────────────────────────────────────────

async fn execute_enumeration(host: &str) -> Result<SmbEnumerationResult, String> {
    let (netbios_res, smb_res) = tokio::join!(
        probe_netbios(host, NETBIOS_TIMEOUT),
        probe_smb_and_ntlm(host, SMB_TIMEOUT),
    );

    let (nb_name, nb_domain, nb_mac, nb_failed) = netbios_res;
    let (ntlm, smb) = smb_res?;

    if ntlm.is_none()
        && smb.is_none()
        && nb_name.is_none()
        && nb_domain.is_none()
        && nb_mac.is_none()
    {
        return Err(fl!(
            "ui-tool-smb-error-connection",
            error = "port 445 closed or filtered"
        ));
    }

    Ok(build_result(
        ntlm, smb, nb_name, nb_domain, nb_mac, nb_failed,
    ))
}

async fn probe_netbios(
    host: &str,
    dur: Duration,
) -> (Option<String>, Option<String>, Option<String>, bool) {
    let query_task = async {
        let socket = UdpSocket::bind("0.0.0.0:0").await.ok()?;
        let target_addr = format!("{host}:137");
        socket.connect(&target_addr).await.ok()?;

        let query = build_netbios_nbstat_query(0x1337);
        socket.send(&query).await.ok()?;

        let mut buf = vec![0u8; 1024];
        let n = socket.recv(&mut buf).await.ok()?;
        buf.truncate(n);
        Some(parse_netbios_response(&buf))
    };

    match timeout(dur, query_task).await {
        Ok(Some((name, domain, mac))) => {
            let failed = name.is_none() && domain.is_none() && mac.is_none();
            (name, domain, mac, failed)
        }
        _ => (None, None, None, true),
    }
}

async fn probe_smb_and_ntlm(
    host: &str,
    dur: Duration,
) -> Result<(Option<NtlmInfo>, Option<SmbNegotiateInfo>), String> {
    let probe_task = async {
        let addr = format!("{host}:445");
        let mut stream = TcpStream::connect(&addr).await.map_err(|e| e.to_string())?;

        // 1. SMB2 Negotiate
        let negotiate_body = build_smb2_negotiate();
        let nb_hdr = build_netbios_header(negotiate_body.len());
        stream.write_all(&nb_hdr).await.map_err(|e| e.to_string())?;
        stream
            .write_all(&negotiate_body)
            .await
            .map_err(|e| e.to_string())?;
        stream.flush().await.map_err(|e| e.to_string())?;

        let neg_resp = read_netbios_message(&mut stream)
            .await
            .map_err(|e| e.to_string())?;
        if neg_resp.len() < 68 || neg_resp[0] != 0xFE || neg_resp[1] != b'S' {
            return Err("Invalid SMB2 negotiate response".to_string());
        }

        let smb_info = parse_negotiate_response(&neg_resp);

        // 2. SMB2 Session Setup with NTLMSSP Type 1
        let type1 = build_ntlmssp_type1();
        let spnego = wrap_in_spnego(&type1);
        let setup_body = build_smb2_session_setup(&spnego);
        let setup_hdr = build_netbios_header(setup_body.len());
        stream
            .write_all(&setup_hdr)
            .await
            .map_err(|e| e.to_string())?;
        stream
            .write_all(&setup_body)
            .await
            .map_err(|e| e.to_string())?;
        stream.flush().await.map_err(|e| e.to_string())?;

        let setup_resp = read_netbios_message(&mut stream)
            .await
            .map_err(|e| e.to_string())?;
        if setup_resp.len() < 73 {
            return Ok((None, smb_info));
        }

        let sec_buf_off = u16::from_le_bytes([setup_resp[68], setup_resp[69]]) as usize;
        let sec_buf_len = u16::from_le_bytes([setup_resp[70], setup_resp[71]]) as usize;
        if sec_buf_off + sec_buf_len > setup_resp.len() {
            return Ok((None, smb_info));
        }

        let sec_buf = &setup_resp[sec_buf_off..sec_buf_off + sec_buf_len];
        let ntlm_type2 = find_ntlmssp_message(sec_buf);
        let ntlm_info = ntlm_type2.and_then(parse_ntlm_type2);

        Ok((ntlm_info, smb_info))
    };

    match timeout(dur, probe_task).await {
        Ok(res) => res.map_err(|e| fl!("ui-tool-smb-error-ntlm", error = e)),
        Err(_) => Err(fl!("ui-tool-smb-error-connection", error = "timed out")),
    }
}

async fn read_netbios_message(stream: &mut TcpStream) -> Result<Vec<u8>, std::io::Error> {
    let mut header = [0u8; 4];
    stream.read_exact(&mut header).await?;
    let len = read_netbios_length(&header).ok_or_else(|| {
        std::io::Error::new(std::io::ErrorKind::InvalidData, "invalid NetBIOS header")
    })?;

    let mut data = vec![0u8; len];
    stream.read_exact(&mut data).await?;
    Ok(data)
}

fn localize_key(key: &str) -> String {
    match key {
        "ToolSmbReportIdentity" => fl!("ui-tool-smb-report-identity"),
        "ToolSmbComputerName" => fl!("ui-tool-smb-computer-name"),
        "ToolSmbDomain" => fl!("ui-tool-smb-domain"),
        "ToolSmbDnsName" => fl!("ui-tool-smb-dns-name"),
        "ToolSmbDnsDomain" => fl!("ui-tool-smb-dns-domain"),
        "ToolSmbForest" => fl!("ui-tool-smb-forest"),
        "ToolSmbOsBuild" => fl!("ui-tool-smb-os-build"),
        "ToolSmbMac" => fl!("ui-tool-smb-mac"),
        "ToolSmbReportProtocol" => fl!("ui-tool-smb-report-protocol"),
        "ToolSmbDialect" => fl!("ui-tool-smb-dialect"),
        "ToolSmbSigning" => fl!("ui-tool-smb-signing"),
        "ToolSmbServerGuid" => fl!("ui-tool-smb-server-guid"),
        "ToolSmbSystemTime" => fl!("ui-tool-smb-system-time"),
        "ToolSmbBootTime" => fl!("ui-tool-smb-boot-time"),
        "ToolSmbYes" => fl!("ui-tool-smb-yes"),
        "ToolSmbNo" => fl!("ui-tool-smb-no"),
        "ToolSmbNotAvailable" => fl!("ui-tool-smb-not-available"),
        "ToolSmbReportFindings" => fl!("ui-tool-smb-report-findings"),
        "ToolSmbSigningDisabled" => fl!("ui-tool-smb-signing-disabled"),
        "ToolSmbSigningEnabled" => fl!("ui-tool-smb-signing-enabled"),
        "ToolSmbV1Detected" => fl!("ui-tool-smb-v1-detected"),
        "ToolSmbNetBiosFailed" => fl!("ui-tool-smb-netbios-failed"),
        other => other.to_string(),
    }
}

fn localize_finding(finding: &heimdall_core::tools::smb::SmbFinding) -> String {
    match finding.message_key.as_str() {
        "ToolSmbSigningDisabled" => fl!("ui-tool-smb-signing-disabled"),
        "ToolSmbSigningEnabled" => fl!("ui-tool-smb-signing-enabled"),
        "ToolSmbV1Detected" => fl!("ui-tool-smb-v1-detected"),
        "ToolSmbModernDialect" => {
            let dialect = finding.message_args.first().cloned().unwrap_or_default();
            fl!("ui-tool-smb-modern-dialect", dialect = dialect)
        }
        "ToolSmbNetBiosFailed" => fl!("ui-tool-smb-netbios-failed"),
        _ => finding.message_key.clone(),
    }
}

fn field_str(opt: &Option<String>) -> String {
    opt.clone()
        .unwrap_or_else(|| fl!("ui-tool-smb-not-available"))
}
