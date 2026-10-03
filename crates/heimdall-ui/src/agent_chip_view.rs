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

//! The SSH agent chip under the profile form's key, as the C# one: no agent, an agent with
//! no key, or the agent holding keys and how many there are; a click asks again.

use heimdall_app::{AgentChip, Message as AppMessage, server_text};
use heimdall_ssh::AgentSurvey;
use iced::Element;
use iced::widget::{button, container, text, tooltip};

use crate::i18n::fl;
use crate::shell::Message;

/// Size of the chip's text.
const SMALL_SIZE: f32 = 12.0;

/// How the chip reads, as the C# chip's states.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentTone {
    /// No agent answered.
    Off,
    /// Agents answered, none holding a key.
    Warn,
    /// An agent holds keys.
    Ok,
}

/// What the chip says of the agents `found`, as the C# says it: the first agent holding keys
/// and how many keys all of them hold together, or the first agent when none holds any.
#[must_use]
pub fn said(found: &[AgentSurvey]) -> (String, AgentTone) {
    let Some(first) = found.first() else {
        return (fl!("ui-agent-chip-off"), AgentTone::Off);
    };
    let total: usize = found.iter().map(|agent| agent.keys).sum();
    if total == 0 {
        return (
            fl!("ui-agent-chip-warn", agent = server_text(&first.name)),
            AgentTone::Warn,
        );
    }
    let holding = found.iter().find(|agent| agent.keys > 0).unwrap_or(first);
    (
        fl!(
            "ui-agent-chip-ok",
            agent = server_text(&holding.name),
            count = total
        ),
        AgentTone::Ok,
    )
}

/// The chip, once the agents answered; `None` before.
#[must_use]
pub fn view(chip: &AgentChip) -> Option<Element<'_, Message>> {
    let AgentChip::Found(found) = chip else {
        return None;
    };
    let (text_said, tone) = said(found);
    let label = text(text_said).size(SMALL_SIZE);
    let label = match tone {
        AgentTone::Off => label.style(text::secondary),
        AgentTone::Warn => label.style(text::warning),
        AgentTone::Ok => label.style(text::success),
    };
    Some(
        tooltip(
            button(label)
                .style(button::text)
                .padding(0)
                .on_press(Message::App(AppMessage::RefreshAgents)),
            text(fl!("ui-agent-chip-tooltip")).size(SMALL_SIZE),
            tooltip::Position::Top,
        )
        .style(container::rounded_box)
        .into(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn agent(name: &str, keys: usize) -> AgentSurvey {
        AgentSurvey {
            name: name.to_owned(),
            keys,
        }
    }

    #[test]
    fn the_chip_says_the_csharp_three_states() {
        assert_eq!(
            said(&[]),
            ("No SSH agent detected".to_owned(), AgentTone::Off)
        );
        assert_eq!(
            said(&[agent("Windows OpenSSH Agent", 0), agent("Pageant", 0)]),
            (
                "SSH agent: Windows OpenSSH Agent (no keys loaded)".to_owned(),
                AgentTone::Warn
            )
        );
        assert_eq!(
            said(&[
                agent("Windows OpenSSH Agent", 0),
                agent("Pageant", 2),
                agent("x", 1)
            ]),
            ("SSH agent: Pageant (3 keys)".to_owned(), AgentTone::Ok),
            "the first agent holding keys, with the keys of all"
        );
    }
}
