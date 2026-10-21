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

//! Commands an SSH session types by itself once its shell is ready, as the C# Heimdall's
//! post-connect sequence: each step in order, after its delay, followed by a new line.
//!
//! A profile's steps run only once the user approved them: steps written in the session
//! editor are, steps that arrived by an import are not until the user says so. The approval
//! names what it approved, so steps changed since run only once approved again.

use serde::{Deserialize, Serialize};

/// Delay before a step, in milliseconds, as a new C# step has.
pub const DEFAULT_STEP_DELAY_MS: u32 = 150;

/// What the sequence does when a step cannot be sent.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OnFailure {
    /// The next steps still run.
    #[default]
    Continue,
    /// The sequence stops.
    Stop,
}

impl OnFailure {
    #[expect(
        clippy::trivially_copy_pass_by_ref,
        reason = "serde's skip_serializing_if passes a reference"
    )]
    fn is_default(&self) -> bool {
        *self == Self::default()
    }
}

/// One command of the sequence.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PostConnectStep {
    /// What is typed, without the new line sent after it.
    pub input: String,
    /// Wait before it is typed, in milliseconds.
    #[serde(default = "default_delay", skip_serializing_if = "is_default_delay")]
    pub delay_ms: u32,
    /// Whether it runs; a step turned off is kept.
    #[serde(default = "enabled", skip_serializing_if = "is_enabled")]
    pub enabled: bool,
    /// What happens when it cannot be sent.
    #[serde(default, skip_serializing_if = "OnFailure::is_default")]
    pub on_failure: OnFailure,
    /// The text arrived from outside (an import, a shared file) and nobody has said yes to it.
    /// Set on arrival whatever the file says, so a file can never mark its own step trusted.
    /// Only the question's Run, or the user writing or changing this step's text, clears it;
    /// a change to another step, a delay, the order or whether it runs does not. A runnable
    /// step with the mark is never typed (see [`PostConnect::needs_approval`]).
    #[serde(default, skip_serializing_if = "is_false")]
    pub untrusted: bool,
}

impl PostConnectStep {
    /// A step typing `input`, on, with the default delay.
    #[must_use]
    pub fn new(input: impl Into<String>) -> Self {
        Self {
            input: input.into(),
            delay_ms: DEFAULT_STEP_DELAY_MS,
            enabled: true,
            on_failure: OnFailure::Continue,
            untrusted: false,
        }
    }

    /// The same step, marked as arrived from outside.
    #[must_use]
    pub fn from_outside(mut self) -> Self {
        self.untrusted = true;
        self
    }

    /// The step as approved: what it does, without the mark.
    fn trusted(&self) -> Self {
        Self {
            untrusted: false,
            ..self.clone()
        }
    }

    /// Whether it is typed: on, with something to type.
    #[must_use]
    pub fn runs(&self) -> bool {
        self.enabled && !self.input.trim().is_empty()
    }
}

/// A profile's sequence and what the user approved of it.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PostConnect {
    /// The steps, in order.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub steps: Vec<PostConnectStep>,
    /// The steps as the user approved them; `None` until approved.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub approved: Option<Vec<PostConnectStep>>,
}

impl PostConnect {
    /// Steps approved as they are now: the question's Run, which trusts every step shown.
    #[must_use]
    pub fn approved_as(steps: Vec<PostConnectStep>) -> Self {
        let steps: Vec<PostConnectStep> = steps
            .into_iter()
            .map(|step| PostConnectStep {
                untrusted: false,
                ..step
            })
            .collect();
        Self {
            approved: Some(steps.clone()),
            steps,
        }
    }

    /// How many steps are typed.
    #[must_use]
    pub fn runnable(&self) -> usize {
        self.steps.iter().filter(|step| step.runs()).count()
    }

    /// Whether the steps as they are now were approved (what each does, marks aside).
    #[must_use]
    pub fn is_approved(&self) -> bool {
        self.approved.as_ref().is_some_and(|approved| {
            let now: Vec<PostConnectStep> =
                self.steps.iter().map(PostConnectStep::trusted).collect();
            *approved == now
        })
    }

    /// Whether a step that would be typed arrived from outside and was not trusted.
    #[must_use]
    pub fn has_untrusted_step(&self) -> bool {
        self.steps.iter().any(|step| step.runs() && step.untrusted)
    }

    /// Whether steps would be typed but were not approved as they are, or one of them still
    /// waits for its Run.
    #[must_use]
    pub fn needs_approval(&self) -> bool {
        self.runnable() > 0 && (!self.is_approved() || self.has_untrusted_step())
    }

    /// The steps to type in a session: none unless approved, and none while a step that
    /// arrived from outside is untrusted.
    #[must_use]
    pub fn to_run(&self) -> Vec<PostConnectStep> {
        if self.is_approved() && !self.has_untrusted_step() {
            self.steps.clone()
        } else {
            Vec::new()
        }
    }

    /// Whether nothing is kept.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.steps.is_empty() && self.approved.is_none()
    }
}

#[expect(
    clippy::trivially_copy_pass_by_ref,
    reason = "serde's skip_serializing_if passes a reference"
)]
fn is_false(value: &bool) -> bool {
    !*value
}

fn default_delay() -> u32 {
    DEFAULT_STEP_DELAY_MS
}

#[expect(
    clippy::trivially_copy_pass_by_ref,
    reason = "serde's skip_serializing_if passes a reference"
)]
fn is_default_delay(delay: &u32) -> bool {
    *delay == DEFAULT_STEP_DELAY_MS
}

fn enabled() -> bool {
    true
}

#[expect(
    clippy::trivially_copy_pass_by_ref,
    reason = "serde's skip_serializing_if passes a reference"
)]
fn is_enabled(value: &bool) -> bool {
    *value
}

#[cfg(test)]
mod tests {
    use super::{OnFailure, PostConnect, PostConnectStep};

    fn steps() -> Vec<PostConnectStep> {
        vec![
            PostConnectStep::new("sudo -i"),
            PostConnectStep {
                untrusted: false,
                enabled: false,
                ..PostConnectStep::new("off")
            },
            PostConnectStep::new("   "),
        ]
    }

    #[test]
    fn only_steps_on_with_something_to_type_run() {
        let sequence = PostConnect {
            steps: steps(),
            approved: None,
        };
        assert_eq!(sequence.runnable(), 1);
    }

    #[test]
    fn steps_run_only_as_they_were_approved() {
        let mut sequence = PostConnect {
            steps: steps(),
            approved: None,
        };
        assert!(sequence.needs_approval());
        assert!(sequence.to_run().is_empty());

        sequence = PostConnect::approved_as(steps());
        assert!(!sequence.needs_approval());
        assert_eq!(sequence.to_run(), steps());

        // Changed since: approved no longer.
        sequence.steps[0].on_failure = OnFailure::Stop;
        assert!(sequence.needs_approval());
        assert!(sequence.to_run().is_empty());
    }

    #[test]
    fn nothing_to_type_needs_no_approval() {
        let sequence = PostConnect {
            steps: vec![PostConnectStep::new("")],
            approved: None,
        };
        assert!(!sequence.needs_approval());
    }
}
