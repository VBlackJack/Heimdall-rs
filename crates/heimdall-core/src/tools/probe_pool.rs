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

//! Probes run a bounded number at a time, as the C# `SemaphoreSlim` around its probes, a
//! stop holding while a probe waits. Hosts are checked by [`super::net_host::checked_host`]
//! before they reach a probe; the system programs a probe starts are run by
//! [`super::system_program`].

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, PoisonError, mpsc};

/// Runs `work` on every one of `jobs`, `at_once` at most at the same time, handing each
/// result to `on_result` on the calling thread as it comes, as the C# runs its probes
/// behind a `SemaphoreSlim`. Once `stop` is set no job starts; those running finish.
pub fn for_each_bounded<J, R>(
    jobs: Vec<J>,
    at_once: usize,
    stop: &AtomicBool,
    work: impl Fn(J) -> R + Sync,
    mut on_result: impl FnMut(R),
) where
    J: Send,
    R: Send,
{
    let workers = at_once.clamp(1, jobs.len().max(1));
    let queue = Mutex::new(jobs.into_iter());
    let (results, received) = mpsc::channel();
    std::thread::scope(|scope| {
        for _ in 0..workers {
            let results = results.clone();
            let queue = &queue;
            let work = &work;
            scope.spawn(move || {
                loop {
                    if stop.load(Ordering::Relaxed) {
                        break;
                    }
                    let next = queue.lock().unwrap_or_else(PoisonError::into_inner).next();
                    let Some(job) = next else { break };
                    if results.send(work(job)).is_err() {
                        break;
                    }
                }
            });
        }
        drop(results);
        for result in received {
            on_result(result);
        }
    });
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
    use std::time::Duration;

    use super::for_each_bounded;

    #[test]
    fn bounded_jobs_never_run_more_than_asked_and_all_results_come_back() {
        let running = AtomicUsize::new(0);
        let most = AtomicUsize::new(0);
        let stop = AtomicBool::new(false);
        let mut seen = Vec::new();
        for_each_bounded(
            (0..40).collect(),
            5,
            &stop,
            |job: u32| {
                let now = running.fetch_add(1, Ordering::SeqCst) + 1;
                most.fetch_max(now, Ordering::SeqCst);
                std::thread::sleep(Duration::from_millis(5));
                running.fetch_sub(1, Ordering::SeqCst);
                job * 2
            },
            |result| seen.push(result),
        );
        seen.sort_unstable();
        assert_eq!(seen, (0..40).map(|job| job * 2).collect::<Vec<_>>());
        assert!(most.load(Ordering::SeqCst) <= 5, "five at once at most");
        assert!(most.load(Ordering::SeqCst) > 1, "more than one at once");
    }

    #[test]
    fn once_stopped_no_bounded_job_starts() {
        let stop = AtomicBool::new(false);
        let started = AtomicUsize::new(0);
        let mut results = 0;
        for_each_bounded(
            (0..1000).collect::<Vec<u32>>(),
            2,
            &stop,
            |_| {
                // The third job asks the stop: the other worker may be in its own job.
                if started.fetch_add(1, Ordering::SeqCst) + 1 == 3 {
                    stop.store(true, Ordering::SeqCst);
                }
                std::thread::sleep(Duration::from_millis(2));
            },
            |()| results += 1,
        );
        let started = started.load(Ordering::SeqCst);
        assert!(
            started <= 4,
            "the running ones finish, no more start: {started}"
        );
        assert_eq!(results, started, "every job started is handed back");
    }
}
