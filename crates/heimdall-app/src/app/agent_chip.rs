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

//! The SSH agent chip of the profile form, as the C# dialog's: which agent answers and how
//! many keys it holds, asked when an SSH form opens and again when the chip is clicked.

use heimdall_ssh::AgentSurvey;

use super::{App, Dialog, Effect, Message};
use crate::profile_draft::DraftProtocol;

/// What the agent chip knows.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum AgentChip {
    /// Not asked since the form opened.
    #[default]
    Unknown,
    /// Being asked.
    Surveying,
    /// The agents that answered, in the order their keys would be offered.
    Found(Vec<AgentSurvey>),
}

impl App {
    /// Applies a message about the profile form, then asks the agents when an SSH form
    /// opened and nothing is known of them yet.
    pub(super) fn profile_form_message(&mut self, message: Message) -> Vec<Effect> {
        // A form opening asks again: an agent may have started, or a key been loaded.
        if matches!(message, Message::NewProfile | Message::EditProfile(_)) {
            self.agent_chip = AgentChip::Unknown;
        }
        self.profile_message(message);
        if self.agent_chip == AgentChip::Unknown && self.ssh_form_open() {
            return self.survey_agents();
        }
        Vec::new()
    }

    /// Applies a message about the agent chip.
    pub(super) fn agent_chip_message(&mut self, message: Message) -> Vec<Effect> {
        match message {
            Message::RefreshAgents if self.agent_chip != AgentChip::Surveying => {
                self.survey_agents()
            }
            Message::AgentsSurveyed(found) => {
                self.agent_chip = AgentChip::Found(found);
                Vec::new()
            }
            _ => Vec::new(),
        }
    }

    /// What the agent chip knows.
    #[must_use]
    pub fn agent_chip(&self) -> &AgentChip {
        &self.agent_chip
    }

    fn survey_agents(&mut self) -> Vec<Effect> {
        self.agent_chip = AgentChip::Surveying;
        vec![Effect::SurveyAgents(self.ssh_options().agent)]
    }

    /// Whether the profile form is open on an SSH or SFTP profile, whose sign-in an agent
    /// can serve.
    fn ssh_form_open(&self) -> bool {
        matches!(
            &self.dialog,
            Some(Dialog::EditProfile { draft, .. })
                if draft.protocol_chosen
                    && matches!(draft.protocol, DraftProtocol::Ssh | DraftProtocol::Sftp)
        )
    }
}
