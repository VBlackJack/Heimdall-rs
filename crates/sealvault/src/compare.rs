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

//! Comparison of secrets in constant time.

use subtle::ConstantTimeEq as _;

/// Whether `a` and `b` hold the same bytes. The time taken depends on their lengths, never
/// on where they differ; lengths that differ answer at once, so only a length that is not
/// itself secret may vary.
#[must_use]
pub fn equal(a: &[u8], b: &[u8]) -> bool {
    bool::from(a.ct_eq(b))
}

#[cfg(test)]
mod tests {
    use super::equal;

    #[test]
    fn equal_bytes_are_equal_and_any_difference_is_not() {
        assert!(equal(b"cookie", b"cookie"));
        assert!(equal(b"", b""));
        assert!(!equal(b"cookie", b"cookiE"));
        assert!(!equal(b"Cookie", b"cookie"));
        assert!(!equal(b"cookie", b"cookies"), "a prefix is not equal");
        assert!(!equal(b"", b"x"));
    }
}
