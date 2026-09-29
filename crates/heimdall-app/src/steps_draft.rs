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

//! The post-connect steps of an SSH profile's form, as the C# dialog's list: a step is
//! selected, then removed or moved; a new one is added at the end, selected, with the C#
//! defaults.

use heimdall_core::post_connect::{OnFailure, PostConnectStep};

/// A change to the list of steps.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StepEdit {
    /// Add a step at the end, selected.
    Add,
    /// Remove the selected step; the next one, or the last, is selected.
    Remove,
    /// Move the selected step up.
    MoveUp,
    /// Move the selected step down.
    MoveDown,
    /// Select a step.
    Select(usize),
    /// Turn a step on or off.
    Enabled(usize, bool),
    /// The command of a step, as typed; it selects the step.
    Input(usize, String),
    /// The delay of a step, as typed: kept only when it reads as milliseconds, as the C#
    /// number field keeps its value; empty is 0.
    Delay(usize, String),
    /// What a step does when it cannot be typed.
    OnFailure(usize, OnFailure),
}

/// The steps of a form and the one selected.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct StepsDraft {
    /// The steps, in order.
    pub steps: Vec<PostConnectStep>,
    /// The step selected, if any.
    pub selected: Option<usize>,
}

impl StepsDraft {
    /// The steps of a saved profile, none selected.
    #[must_use]
    pub fn of(steps: &[PostConnectStep]) -> Self {
        Self {
            steps: steps.to_vec(),
            selected: None,
        }
    }

    /// Whether the selected step can move up.
    #[must_use]
    pub fn can_move_up(&self) -> bool {
        self.selected.is_some_and(|index| index > 0)
    }

    /// Whether the selected step can move down.
    #[must_use]
    pub fn can_move_down(&self) -> bool {
        self.selected
            .is_some_and(|index| index + 1 < self.steps.len())
    }

    /// Applies `edit`; an edit of a step that is not there changes nothing.
    pub fn apply(&mut self, edit: StepEdit) {
        match edit {
            StepEdit::Add => {
                self.steps.push(PostConnectStep::new(""));
                self.selected = Some(self.steps.len() - 1);
            }
            StepEdit::Remove => {
                if let Some(index) = self.selected.filter(|index| *index < self.steps.len()) {
                    self.steps.remove(index);
                    self.selected =
                        (!self.steps.is_empty()).then(|| index.min(self.steps.len() - 1));
                }
            }
            StepEdit::MoveUp => {
                if let Some(index) = self.selected.filter(|_| self.can_move_up()) {
                    self.steps.swap(index, index - 1);
                    self.selected = Some(index - 1);
                }
            }
            StepEdit::MoveDown => {
                if let Some(index) = self.selected.filter(|_| self.can_move_down()) {
                    self.steps.swap(index, index + 1);
                    self.selected = Some(index + 1);
                }
            }
            StepEdit::Select(index) => {
                if index < self.steps.len() {
                    self.selected = Some(index);
                }
            }
            StepEdit::Enabled(index, on) => self.change(index, |step| step.enabled = on),
            StepEdit::Input(index, input) => self.change(index, |step| step.input = input),
            StepEdit::Delay(index, typed) => {
                let delay = match typed.trim() {
                    "" => Some(0),
                    typed => typed.parse::<u32>().ok(),
                };
                if let Some(delay) = delay {
                    self.change(index, |step| step.delay_ms = delay);
                }
            }
            StepEdit::OnFailure(index, policy) => {
                self.change(index, |step| step.on_failure = policy);
            }
        }
    }

    /// Changes the step `index`, if there, and selects it.
    fn change(&mut self, index: usize, change: impl FnOnce(&mut PostConnectStep)) {
        if let Some(step) = self.steps.get_mut(index) {
            change(step);
            self.selected = Some(index);
        }
    }
}

#[cfg(test)]
mod tests {
    use heimdall_core::post_connect::{DEFAULT_STEP_DELAY_MS, OnFailure, PostConnectStep};

    use super::{StepEdit, StepsDraft};

    fn inputs(draft: &StepsDraft) -> Vec<&str> {
        draft.steps.iter().map(|step| step.input.as_str()).collect()
    }

    fn three() -> StepsDraft {
        StepsDraft::of(&[
            PostConnectStep::new("a"),
            PostConnectStep::new("b"),
            PostConnectStep::new("c"),
        ])
    }

    #[test]
    fn a_step_is_added_at_the_end_selected_with_the_csharp_defaults() {
        let mut draft = StepsDraft::default();
        draft.apply(StepEdit::Add);
        assert_eq!(draft.steps, [PostConnectStep::new("")]);
        assert_eq!(draft.steps[0].delay_ms, DEFAULT_STEP_DELAY_MS);
        assert_eq!(draft.selected, Some(0));
        draft.apply(StepEdit::Add);
        assert_eq!(draft.selected, Some(1));
    }

    #[test]
    fn the_selected_step_moves_and_stays_selected_within_the_list() {
        let mut draft = three();
        draft.apply(StepEdit::MoveUp);
        assert_eq!(inputs(&draft), ["a", "b", "c"], "nothing selected");
        draft.apply(StepEdit::Select(1));
        draft.apply(StepEdit::MoveUp);
        assert_eq!(inputs(&draft), ["b", "a", "c"]);
        assert_eq!(draft.selected, Some(0));
        assert!(!draft.can_move_up());
        draft.apply(StepEdit::MoveUp);
        assert_eq!(inputs(&draft), ["b", "a", "c"], "the first stays first");
        draft.apply(StepEdit::Select(2));
        assert!(!draft.can_move_down());
        draft.apply(StepEdit::MoveDown);
        assert_eq!(inputs(&draft), ["b", "a", "c"], "the last stays last");
        draft.apply(StepEdit::Select(1));
        draft.apply(StepEdit::MoveDown);
        assert_eq!(inputs(&draft), ["b", "c", "a"]);
        assert_eq!(draft.selected, Some(2));
        draft.apply(StepEdit::Select(9));
        assert_eq!(draft.selected, Some(2), "no such step");
    }

    #[test]
    fn removing_selects_the_next_step_or_the_last() {
        let mut draft = three();
        draft.apply(StepEdit::Remove);
        assert_eq!(draft.steps.len(), 3, "nothing selected, nothing removed");
        draft.apply(StepEdit::Select(1));
        draft.apply(StepEdit::Remove);
        assert_eq!(inputs(&draft), ["a", "c"]);
        assert_eq!(draft.selected, Some(1));
        draft.apply(StepEdit::Remove);
        assert_eq!(inputs(&draft), ["a"]);
        assert_eq!(draft.selected, Some(0));
        draft.apply(StepEdit::Remove);
        assert!(draft.steps.is_empty());
        assert_eq!(draft.selected, None);
    }

    #[test]
    fn a_step_changed_is_selected_and_a_delay_that_does_not_read_is_ignored() {
        let mut draft = three();
        draft.apply(StepEdit::Input(2, "sudo -i".to_owned()));
        assert_eq!(draft.steps[2].input, "sudo -i");
        assert_eq!(draft.selected, Some(2));
        draft.apply(StepEdit::Enabled(0, false));
        assert!(!draft.steps[0].enabled);
        assert_eq!(draft.selected, Some(0));
        draft.apply(StepEdit::OnFailure(1, OnFailure::Stop));
        assert_eq!(draft.steps[1].on_failure, OnFailure::Stop);
        draft.apply(StepEdit::Delay(1, "500".to_owned()));
        assert_eq!(draft.steps[1].delay_ms, 500);
        draft.apply(StepEdit::Delay(1, "5x".to_owned()));
        assert_eq!(draft.steps[1].delay_ms, 500, "kept");
        draft.apply(StepEdit::Delay(1, "-1".to_owned()));
        assert_eq!(draft.steps[1].delay_ms, 500, "kept");
        draft.apply(StepEdit::Delay(1, String::new()));
        assert_eq!(draft.steps[1].delay_ms, 0);
        draft.apply(StepEdit::Input(7, "lost".to_owned()));
        assert_eq!(draft.steps.len(), 3);
    }
}
