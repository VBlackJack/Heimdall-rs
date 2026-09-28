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

//! Too many wrong master passwords in a row: no more tries for a while, as the C#
//! `PinManager` counts them.

use std::time::{Duration, SystemTime};

/// Wrong tries in a row that lock the tries out, as the C# default.
pub const MAX_FAILED_ATTEMPTS: u32 = 5;
/// How long the tries stay locked out, as the C# default.
pub const LOCKOUT_DURATION: Duration = Duration::from_mins(5);
/// Seconds in a minute, for the minutes the lockout still lasts.
const SECONDS_PER_MINUTE: u64 = 60;

/// Wrong tries in a row, and until when no more are taken.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Lockout {
    failures: u32,
    until: Option<SystemTime>,
}

impl Lockout {
    /// The state as it was saved: a lockout over by `now` starts the count again, as the C#
    /// restore does.
    #[must_use]
    pub fn restored(failures: u32, until: Option<SystemTime>, now: SystemTime) -> Self {
        let mut lockout = Self { failures, until };
        lockout.expire(now);
        lockout
    }

    /// Wrong tries in a row so far.
    #[must_use]
    pub fn failures(&self) -> u32 {
        self.failures
    }

    /// Until when no try is taken, as saved: `None` when tries are taken.
    #[must_use]
    pub fn until(&self) -> Option<SystemTime> {
        self.until
    }

    /// Until when no try is taken, at `now`; a lockout over by then is forgotten with its
    /// count.
    pub fn locked_until(&mut self, now: SystemTime) -> Option<SystemTime> {
        self.expire(now);
        self.until
    }

    /// Counts a wrong try at `now`; the last one allowed locks the tries out.
    pub fn register_failure(&mut self, now: SystemTime) {
        self.failures = self.failures.saturating_add(1);
        if self.failures >= MAX_FAILED_ATTEMPTS {
            self.until = Some(now + LOCKOUT_DURATION);
        }
    }

    /// A right try: the count starts again.
    pub fn reset(&mut self) {
        *self = Self::default();
    }

    fn expire(&mut self, now: SystemTime) {
        if self.until.is_some_and(|until| now >= until) {
            self.reset();
        }
    }
}

/// The whole minutes a lockout lasting until `until` still has at `now`, rounded up as the
/// C# message counts them.
#[must_use]
pub fn minutes_left(until: SystemTime, now: SystemTime) -> u64 {
    let seconds = until.duration_since(now).map_or(0, |left| {
        left.as_secs() + u64::from(left.subsec_nanos() > 0)
    });
    seconds.div_ceil(SECONDS_PER_MINUTE)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(seconds: u64) -> SystemTime {
        SystemTime::UNIX_EPOCH + Duration::from_secs(seconds)
    }

    #[test]
    fn the_fifth_wrong_try_locks_out_for_five_minutes_then_the_count_starts_again() {
        let mut lockout = Lockout::default();
        for _ in 1..MAX_FAILED_ATTEMPTS {
            lockout.register_failure(at(100));
            assert_eq!(lockout.locked_until(at(100)), None, "{lockout:?}");
        }
        lockout.register_failure(at(100));
        assert_eq!(lockout.locked_until(at(100)), Some(at(400)));
        assert_eq!(lockout.locked_until(at(399)), Some(at(400)), "still");
        assert_eq!(lockout.locked_until(at(400)), None, "over");
        assert_eq!(lockout.failures(), 0, "counted again from nothing");
    }

    #[test]
    fn a_right_try_starts_the_count_again() {
        let mut lockout = Lockout::default();
        for _ in 1..MAX_FAILED_ATTEMPTS {
            lockout.register_failure(at(0));
        }
        lockout.reset();
        lockout.register_failure(at(0));
        assert_eq!(lockout.locked_until(at(0)), None);
        assert_eq!(lockout.failures(), 1);
    }

    #[test]
    fn a_saved_lockout_holds_until_it_is_over() {
        let held = Lockout::restored(5, Some(at(400)), at(100));
        assert_eq!(held.until(), Some(at(400)));
        let over = Lockout::restored(5, Some(at(400)), at(400));
        assert_eq!((over.failures(), over.until()), (0, None));
        let counting = Lockout::restored(3, None, at(400));
        assert_eq!(counting.failures(), 3);
    }

    #[test]
    fn the_minutes_left_are_rounded_up() {
        assert_eq!(minutes_left(at(400), at(100)), 5);
        assert_eq!(minutes_left(at(400), at(101)), 5);
        assert_eq!(minutes_left(at(400), at(340)), 1);
        assert_eq!(minutes_left(at(400), at(339)), 2);
        assert_eq!(minutes_left(at(400), at(399)), 1);
        assert_eq!(
            minutes_left(at(400), at(399) + Duration::from_millis(500)),
            1
        );
        assert_eq!(minutes_left(at(400), at(400)), 0);
        assert_eq!(minutes_left(at(400), at(500)), 0);
    }
}
