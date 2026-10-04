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

//! A shell tab's server health panel, beside its terminal, as the C# SSH view's: CPU,
//! memory and disk, each a bar and its figures.

use heimdall_app::server_health::{HealthPane, ServerHealth};
use iced::widget::{column, container, progress_bar, text};
use iced::{Element, Length};

use crate::i18n::fl;
use crate::shell::Message;

/// Width of the panel, as the C#'s.
const PANEL_WIDTH: f32 = 180.0;

/// Size of its text.
const SMALL_SIZE: f32 = 12.0;

/// Space between its parts.
const SPACING: f32 = 4.0;

/// The panel, from what the server last said.
pub fn view<'a>(health: &HealthPane) -> Element<'a, Message> {
    let figures = figures(health.last.as_ref());
    let mut panel = column![].spacing(SPACING);
    for (label, (share, figure)) in [
        fl!("ui-health-cpu"),
        fl!("ui-health-memory"),
        fl!("ui-health-disk"),
    ]
    .into_iter()
    .zip(figures)
    {
        panel = panel
            .push(text(label).size(SMALL_SIZE).style(text::secondary))
            .push(progress_bar(0.0..=100.0, share).girth(14.0))
            .push(
                container(text(figure).size(SMALL_SIZE))
                    .align_right(Length::Fill)
                    .padding(iced::Padding::ZERO.bottom(8.0)),
            );
    }
    container(panel)
        .width(PANEL_WIDTH)
        .height(Length::Fill)
        .padding(8.0)
        .style(container::bordered_box)
        .into()
}

/// Each bar's share in percent and its figures: CPU, memory, disk.
fn figures(last: Option<&ServerHealth>) -> [(f32, String); 3] {
    match last {
        None => std::array::from_fn(|_| (0.0, fl!("ui-health-waiting"))),
        Some(health) if !health.supported => {
            std::array::from_fn(|_| (0.0, fl!("ui-health-unsupported")))
        }
        Some(health) => {
            let (total, used) = health.memory_mb;
            let memory_share = if total == 0 { 0.0 } else { share(used, total) };
            let (disk_total, disk_used, disk_percent) = &health.disk;
            [
                (
                    narrow(health.cpu_percent),
                    fl!(
                        "ui-health-cpu-value",
                        percent = format!("{:.1}", health.cpu_percent)
                    ),
                ),
                (
                    memory_share,
                    fl!(
                        "ui-health-memory-value",
                        used = used.to_string(),
                        total = total.to_string()
                    ),
                ),
                (
                    f32::from(*disk_percent),
                    fl!(
                        "ui-health-disk-value",
                        used = disk_used.as_str(),
                        total = disk_total.as_str()
                    ),
                ),
            ]
        }
    }
}

#[expect(
    clippy::cast_possible_truncation,
    reason = "a share in percent, 0 to 100, drawn as a bar"
)]
fn narrow(percent: f64) -> f32 {
    percent.clamp(0.0, 100.0) as f32
}

#[expect(
    clippy::cast_precision_loss,
    reason = "a share in percent, drawn as a bar: three digits are plenty"
)]
fn share(used: u64, total: u64) -> f32 {
    narrow(used as f64 * 100.0 / total as f64)
}
