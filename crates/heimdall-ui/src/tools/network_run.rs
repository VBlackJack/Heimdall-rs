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

//! The run of a network tool, as the C# view models keep a `CancellationTokenSource`: one
//! run at a time per tab, numbered so the messages of a run stopped or replaced are let
//! go, and stopped for real: its task aborted and the work it started off the window told
//! to stop.

use std::convert::Infallible;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use iced::Task;
use iced::futures::SinkExt as _;
use iced::futures::Stream;
use iced::task::Handle;

use crate::shell::Message;

/// Messages a run's stream holds before its sender waits for the window.
const STREAM_BUFFER: usize = 64;

/// A tool's run: which one is current, and how to stop it.
#[derive(Debug, Default)]
pub(super) struct RunSlot {
    /// The current run's number; a message of another is let go.
    generation: u64,
    /// The stop the current run's work watches; `None` when nothing runs.
    stop: Option<Arc<AtomicBool>>,
    /// The current run's task: dropped, it is aborted.
    handle: Option<Handle>,
}

impl RunSlot {
    /// Whether a run is going.
    pub(super) fn running(&self) -> bool {
        self.stop.is_some()
    }

    /// Begins a new run, stopping the one going: its number, and the stop its work watches.
    pub(super) fn begin(&mut self) -> (u64, Arc<AtomicBool>) {
        self.stop();
        self.generation += 1;
        let stop = Arc::new(AtomicBool::new(false));
        self.stop = Some(Arc::clone(&stop));
        (self.generation, stop)
    }

    /// `task`, the current run's, made abortable: [`RunSlot::stop`] aborts it, and so does
    /// dropping the slot with its tab.
    pub(super) fn attach(&mut self, task: Task<Message>) -> Task<Message> {
        let (task, handle) = task.abortable();
        self.handle = Some(handle.abort_on_drop());
        task
    }

    /// Stops the current run, if any: its work told to stop, its task aborted; its messages
    /// still on their way are let go.
    pub(super) fn stop(&mut self) {
        if let Some(stop) = self.stop.take() {
            stop.store(true, Ordering::Relaxed);
        }
        self.handle = None;
    }

    /// Whether a message of run `generation` belongs to the run going.
    pub(super) fn is_current(&self, generation: u64) -> bool {
        self.running() && generation == self.generation
    }

    /// Run `generation` says it is over: `true` when it is the current one, which then no
    /// longer runs.
    pub(super) fn finish(&mut self, generation: u64) -> bool {
        if !self.is_current(generation) {
            return false;
        }
        self.stop = None;
        self.handle = None;
        true
    }
}

/// Sets its stop when dropped: a run's stream dropped, by a stop or with its tab, tells the
/// work it started off the window to stop.
struct StopOnDrop(Arc<AtomicBool>);

impl Drop for StopOnDrop {
    fn drop(&mut self) {
        self.0.store(true, Ordering::Relaxed);
    }
}

/// The messages of blocking `work`, run off the window and watching `stop`: each value it
/// hands over as `item` says, as it comes, then its end as `end` says, `None` when the
/// work failed.
pub(super) fn blocking_run<T, E, M>(
    stop: Arc<AtomicBool>,
    work: impl FnOnce(&AtomicBool, &mut dyn FnMut(T)) -> E + Send + 'static,
    item: impl Fn(T) -> M + Send + 'static,
    end: impl FnOnce(Option<E>) -> M + Send + 'static,
) -> impl Stream<Item = M>
where
    T: Send + 'static,
    E: Send + 'static,
    M: Send + 'static,
{
    iced::stream::channel(STREAM_BUFFER, async move |mut out| {
        let _stop_on_drop = StopOnDrop(Arc::clone(&stop));
        let (sender, mut received) = tokio::sync::mpsc::unbounded_channel();
        let worker = tokio::task::spawn_blocking(move || {
            let mut send = |value: T| {
                let _ = sender.send(value);
            };
            work(&stop, &mut send)
        });
        while let Some(value) = received.recv().await {
            if out.send(item(value)).await.is_err() {
                return;
            }
        }
        let ended = worker.await.ok();
        let _ = out.send(end(ended)).await;
    })
}

/// The message of blocking `work` that hands nothing over as it goes, run off the window
/// and watching `stop`: its end as `end` says, `None` when the work failed.
pub(super) fn blocking_once<E, M>(
    stop: Arc<AtomicBool>,
    work: impl FnOnce(&AtomicBool) -> E + Send + 'static,
    end: impl FnOnce(Option<E>) -> M + Send + 'static,
) -> impl Stream<Item = M>
where
    E: Send + 'static,
    M: Send + 'static,
{
    blocking_run(
        stop,
        move |stop, _: &mut dyn FnMut(Infallible)| work(stop),
        |never: Infallible| match never {},
        end,
    )
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::Ordering;

    use super::RunSlot;

    #[test]
    fn a_new_run_stops_the_one_going_and_its_messages_are_let_go() {
        let mut slot = RunSlot::default();
        assert!(!slot.running());
        let (first, first_stop) = slot.begin();
        assert!(slot.running() && slot.is_current(first));
        let (second, second_stop) = slot.begin();
        assert!(
            first_stop.load(Ordering::Relaxed),
            "the first one told to stop"
        );
        assert!(!second_stop.load(Ordering::Relaxed));
        assert!(!slot.is_current(first), "the first one's messages let go");
        assert!(!slot.finish(first), "nor its end");
        assert!(slot.is_current(second));
        assert!(slot.finish(second));
        assert!(!slot.running());
        assert!(!slot.is_current(second), "over");
    }

    #[test]
    fn a_stop_tells_the_work_and_lets_the_runs_messages_go() {
        let mut slot = RunSlot::default();
        let (run, stop) = slot.begin();
        slot.stop();
        assert!(stop.load(Ordering::Relaxed));
        assert!(!slot.running());
        assert!(!slot.is_current(run));
        assert!(!slot.finish(run));
        slot.stop();
    }
}
