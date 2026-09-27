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

//! The body before sealing: the number of entries, then each name and value, every length a
//! little-endian `u32` before its bytes. Written and read here, into buffers wiped on drop,
//! rather than through a serialisation library that would leave copies behind.

use std::collections::BTreeMap;

use zeroize::Zeroizing;

/// `entries` laid out, in a buffer wiped on drop.
pub fn encode(entries: &BTreeMap<String, Zeroizing<Vec<u8>>>) -> Zeroizing<Vec<u8>> {
    let size = 4 + entries
        .iter()
        .map(|(name, value)| 8 + name.len() + value.len())
        .sum::<usize>();
    let mut out = Zeroizing::new(Vec::with_capacity(size));
    push_len(&mut out, entries.len());
    for (name, value) in entries {
        push_len(&mut out, name.len());
        out.extend_from_slice(name.as_bytes());
        push_len(&mut out, value.len());
        out.extend_from_slice(value);
    }
    out
}

/// The entries of `plain`; `None` unless it is exactly what [`encode`] writes: no bytes
/// left over, names in UTF-8, none twice.
pub fn decode(plain: &[u8]) -> Option<BTreeMap<String, Zeroizing<Vec<u8>>>> {
    let mut rest = plain;
    let count = take_len(&mut rest)?;
    let mut entries = BTreeMap::new();
    for _ in 0..count {
        let name_len = take_len(&mut rest)?;
        let name = String::from_utf8(take(&mut rest, name_len)?.to_vec()).ok()?;
        let value_len = take_len(&mut rest)?;
        let value = Zeroizing::new(take(&mut rest, value_len)?.to_vec());
        if entries.insert(name, value).is_some() {
            return None;
        }
    }
    rest.is_empty().then_some(entries)
}

fn push_len(out: &mut Vec<u8>, len: usize) {
    // A body never holds 4 GiB: a vault that did could not be sealed in one piece anyway.
    let len = u32::try_from(len).expect("under 4 GiB");
    out.extend_from_slice(&len.to_le_bytes());
}

fn take_len(rest: &mut &[u8]) -> Option<usize> {
    let bytes = take(rest, 4)?;
    usize::try_from(u32::from_le_bytes(bytes.try_into().ok()?)).ok()
}

fn take<'a>(rest: &mut &'a [u8], len: usize) -> Option<&'a [u8]> {
    if rest.len() < len {
        return None;
    }
    let (field, remaining) = rest.split_at(len);
    *rest = remaining;
    Some(field)
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use zeroize::Zeroizing;

    use super::{decode, encode};

    fn entries() -> BTreeMap<String, Zeroizing<Vec<u8>>> {
        BTreeMap::from([
            ("a".to_owned(), Zeroizing::new(b"one".to_vec())),
            ("é".to_owned(), Zeroizing::new(Vec::new())),
        ])
    }

    #[test]
    fn entries_read_back_as_written() {
        assert_eq!(decode(&encode(&entries())), Some(entries()));
        assert_eq!(decode(&encode(&BTreeMap::new())), Some(BTreeMap::new()));
    }

    #[test]
    fn anything_else_is_refused() {
        let good = encode(&entries());
        assert!(decode(&good[..good.len() - 1]).is_none(), "cut short");
        assert!(
            decode(&[good.as_slice(), &[0]].concat()).is_none(),
            "bytes left over"
        );
        // Two entries both named `a`.
        let twice = [
            &2_u32.to_le_bytes()[..],
            &1_u32.to_le_bytes(),
            b"a",
            &0_u32.to_le_bytes(),
            &1_u32.to_le_bytes(),
            b"a",
            &0_u32.to_le_bytes(),
        ]
        .concat();
        assert!(decode(&twice).is_none(), "a name twice");
        let bad_utf8 = [
            &1_u32.to_le_bytes()[..],
            &1_u32.to_le_bytes(),
            &[0xff],
            &0_u32.to_le_bytes(),
        ]
        .concat();
        assert!(decode(&bad_utf8).is_none(), "a name not in UTF-8");
    }
}
