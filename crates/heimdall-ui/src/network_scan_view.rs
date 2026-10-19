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

//! The network scanner of Ctrl+Shift+N, as the C# `NetworkScannerService` shows it: the
//! subnet asked for, a progress line while it is swept, the hosts found with their open
//! ports, then the ones chosen added to the sessions. The sweep itself runs off the window,
//! in [`run`], and tells the dialog as it goes.

use std::net::Ipv4Addr;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use heimdall_app::{
    Message as AppMessage, NetworkScan, NetworkScanMessage, NetworkScanRow, server_text,
};
use heimdall_core::tools::network_cartography::SystemProber;
use heimdall_core::tools::network_scanner::{self, PORT_PROBE_TIMEOUT_MS, RemoteKind, TargetError};
use iced::futures::SinkExt as _;
use iced::widget::{Column, button, checkbox, column, container, row, text, text_input};
use iced::{Alignment, Element, Length, Task};

use crate::dialog_parts;
use crate::ellipsis;
use crate::i18n::fl;
use crate::shell::Message;
use crate::styles;
use crate::tokens::{font_size, spacing};

/// Tallest the list of hosts grows before it scrolls.
const ROWS_HEIGHT: f32 = 300.0;

/// Widths of the columns: tick, address, name, open ports, session, status.
const COLUMNS: [f32; 6] = [44.0, 130.0, 210.0, 150.0, 170.0, 150.0];

/// Room around a cell's text, as the import previews' cells.
const CELL_PADDING: iced::Padding = iced::Padding {
    top: 0.0,
    right: 8.0,
    bottom: 0.0,
    left: 4.0,
};

/// Messages buffered between the sweep and the window.
const EVENT_BUFFER: usize = 64;

fn app(message: NetworkScanMessage) -> Message {
    Message::App(AppMessage::NetworkScan(message))
}

/// Sweep `scan` of `ips`, off the window, its progress and hosts told as they come and its
/// end last. Stopped when `stop` is set, or when the stream is dropped with the dialog.
pub fn run(scan: u64, ips: Vec<Ipv4Addr>, stop: Arc<AtomicBool>) -> Task<Message> {
    let events = iced::stream::channel(EVENT_BUFFER, async move |mut out| {
        let (sender, mut received) = tokio::sync::mpsc::unbounded_channel();
        let worker_stop = Arc::clone(&stop);
        let worker = tokio::task::spawn_blocking(move || {
            let prober = SystemProber::new(PORT_PROBE_TIMEOUT_MS);
            network_scanner::scan(&ips, &prober, &worker_stop, |event| {
                let _ = sender.send(event);
            });
        });
        while let Some(event) = received.recv().await {
            if out
                .send(NetworkScanMessage::Event { scan, event })
                .await
                .is_err()
            {
                stop.store(true, Ordering::Relaxed);
                return;
            }
        }
        let _ = worker.await;
        let _ = out.send(NetworkScanMessage::Ended(scan)).await;
    });
    Task::run(events, app)
}

/// A cell of `width`: its text on one line, ended with an ellipsis where the column ends.
fn cell<'a>(value: impl Into<String>, width: f32) -> Element<'a, Message> {
    container(
        ellipsis::text(value)
            .width(Length::Fill)
            .size(font_size::BODY),
    )
    .width(width)
    .padding(CELL_PADDING)
    .into()
}

/// Why the target was refused, in the user's words.
fn error_text(error: TargetError) -> String {
    match error {
        TargetError::Empty => fl!("ui-network-scan-error-empty"),
        TargetError::Invalid => fl!("ui-network-scan-error-invalid"),
        TargetError::PrefixOutOfRange => fl!(
            "ui-network-scan-error-range",
            prefix = network_scanner::MIN_PREFIX.to_string()
        ),
    }
}

/// The name of a kind of session.
fn kind_text(kind: RemoteKind) -> String {
    match kind {
        RemoteKind::Rdp => fl!("ui-network-scan-kind-rdp"),
        RemoteKind::Ssh => fl!("ui-network-scan-kind-ssh"),
        RemoteKind::Vnc => fl!("ui-network-scan-kind-vnc"),
        RemoteKind::WinRm => fl!("ui-network-scan-kind-winrm"),
        RemoteKind::Telnet => fl!("ui-network-scan-kind-telnet"),
        RemoteKind::Web => fl!("ui-network-scan-kind-web"),
    }
}

/// The dialog.
#[must_use]
pub fn dialog(scan: &NetworkScan) -> Element<'_, Message> {
    let mut target = text_input(network_scanner::DEFAULT_SUBNET, &scan.target)
        .size(font_size::BODY)
        .style(styles::text_input);
    if !scan.running {
        target = target
            .on_input(|typed| app(NetworkScanMessage::TargetChanged(typed)))
            .on_submit(app(NetworkScanMessage::Start));
    }
    let sweep = if scan.running {
        dialog_parts::action(fl!("ui-network-scan-stop"), styles::secondary)
            .on_press(app(NetworkScanMessage::Stop))
    } else {
        dialog_parts::confirm(
            fl!("ui-network-scan-start"),
            Some(app(NetworkScanMessage::Start)),
        )
    };
    let mut content = column![
        dialog_parts::list_title(fl!("ui-network-scan-title")),
        dialog_parts::note(fl!("ui-network-scan-prompt")),
        row![target, sweep]
            .spacing(spacing::SM)
            .align_y(Alignment::Center),
    ]
    .spacing(spacing::SM);
    if let Some(error) = scan.error {
        content = content.push(dialog_parts::error(error_text(error)));
    }
    if scan.running {
        content = content.push(dialog_parts::note(fl!(
            "ui-network-scan-progress",
            cidr = scan.target.trim().to_owned(),
            done = scan.progress.0.to_string(),
            total = scan.progress.1.to_string()
        )));
    } else if scan.swept {
        content = content.push(dialog_parts::note(if scan.rows.is_empty() {
            fl!("ui-network-scan-none", cidr = scan.target.trim().to_owned())
        } else {
            fl!("ui-network-scan-complete", count = scan.rows.len())
        }));
    }
    if !scan.rows.is_empty() {
        content = content.push(list(scan));
    }
    let add = dialog_parts::confirm(
        fl!("ui-network-scan-add", count = scan.chosen()),
        scan.can_add().then(|| app(NetworkScanMessage::Add)),
    );
    content
        .push(dialog_parts::buttons([dialog_parts::cancel(), add]))
        .width(Length::Shrink)
        .into()
}

/// The hosts found, with the buttons choosing them.
fn list(scan: &NetworkScan) -> Element<'_, Message> {
    let header = row![
        cell("", COLUMNS[0]),
        cell(fl!("ui-network-scan-column-address"), COLUMNS[1]),
        cell(fl!("ui-network-scan-column-name"), COLUMNS[2]),
        cell(fl!("ui-network-scan-column-ports"), COLUMNS[3]),
        cell(fl!("ui-network-scan-column-session"), COLUMNS[4]),
        cell(fl!("ui-network-scan-column-status"), COLUMNS[5]),
    ];
    let rows = Column::with_children(
        scan.rows
            .iter()
            .enumerate()
            .map(|(index, row)| host_row(index, row)),
    )
    .spacing(2.0);
    let choose = row![
        button(text(fl!("ui-rdp-select-all")).size(font_size::BODY))
            .style(styles::secondary)
            .on_press(app(NetworkScanMessage::ChooseAll(true))),
        button(text(fl!("ui-rdp-select-none")).size(font_size::BODY))
            .style(styles::secondary)
            .on_press(app(NetworkScanMessage::ChooseAll(false))),
    ]
    .spacing(spacing::SM);
    column![choose, header, styles::scroll(rows).height(ROWS_HEIGHT)]
        .spacing(spacing::SM)
        .into()
}

fn host_row(index: usize, row: &NetworkScanRow) -> Element<'_, Message> {
    let tick: Element<'_, Message> = if row.choosable() {
        checkbox(row.chosen)
            .style(styles::checkbox)
            .on_toggle(move |_| app(NetworkScanMessage::Choose(index)))
            .into()
    } else {
        checkbox(false).style(styles::checkbox).into()
    };
    let ports = if row.host.open_ports.is_empty() {
        "-".to_owned()
    } else {
        row.host
            .open_ports
            .iter()
            .map(u16::to_string)
            .collect::<Vec<_>>()
            .join(", ")
    };
    let session = row
        .suggestion
        .map(|suggestion| format!("{} :{}", kind_text(suggestion.kind), suggestion.port))
        .unwrap_or_default();
    let status = if row.duplicate {
        fl!("ui-network-scan-status-saved")
    } else if row.suggestion.is_none() {
        fl!("ui-network-scan-status-no-port")
    } else {
        String::new()
    };
    row![
        container(tick).width(COLUMNS[0]),
        cell(row.host.ip.to_string(), COLUMNS[1]),
        cell(server_text(&row.host.display_name()), COLUMNS[2]),
        cell(ports, COLUMNS[3]),
        cell(session, COLUMNS[4]),
        cell(status, COLUMNS[5]),
    ]
    .align_y(Alignment::Center)
    .into()
}
