# Vendored crates

Crates copied from crates.io and patched, used through `[patch.crates-io]` in the
workspace `Cargo.toml`. Each entry says what changed, why, and when to remove it.

## ironrdp-connector 0.10.0

Source: `https://static.crates.io/crates/ironrdp-connector/ironrdp-connector-0.10.0.crate`,
MIT or Apache-2.0 (its `LICENSE-MIT` and `LICENSE-APACHE` are kept here). `Cargo.lock` of
the package removed; nothing else added.

Changed, three lines against the published package:

- `Cargo.toml`: `picky` `=7.0.0-rc.25` to `=7.0.0-rc.26`, `sspi` `0.21` to `0.22`.
- `src/credssp.rs`, `write_credssp_request`: `TsRequest::buffer_len` returns a `Result`
  in sspi 0.22; the error is mapped as the next line already maps the encoding error.

Why: sspi 0.21 and picky rc.25 pin `curve25519-dalek =5.0.0-rc.1` and
`ed25519-dalek =3.0.0-rc.1`, while russh 0.63 (the SSH client) needs the final 5.x and
3.x releases. One build cannot hold both. Going back to russh 0.62.2, the last release on
the release candidates, would drop the client-side fixes of GHSA-47hw-gvq5-r2gm,
GHSA-p8qx-h547-fjw9, GHSA-w3jg-pjxf-73p4 and GHSA-g9hv-x236-4qp3. sspi 0.22 and picky
rc.26 use the final releases; ironrdp-connector 0.10.0, the latest published, and the
IronRDP `master` branch of 2026-09-22 still ask for sspi 0.21.

Remove when: a published `ironrdp-connector` depends on sspi 0.22 or later. Then delete
this directory and the `[patch.crates-io]` entry, and run the gates.

Checked: `git diff --no-index` against the published package shows only the three lines
above; builds for `x86_64-unknown-linux-gnu` and `x86_64-pc-windows-gnu`.

## ironrdp-session 0.11.0

Source: `https://static.crates.io/crates/ironrdp-session/ironrdp-session-0.11.0.crate`,
MIT or Apache-2.0 (its `LICENSE-MIT` and `LICENSE-APACHE` are kept here). `Cargo.lock` of
the package removed; nothing else added.

Changed, in `src/fast_path.rs`, `process_bitmap_update` only:

- A new function at the end of the file, `visible_rows`, crops the rows of a decoded bitmap
  to the width of the rectangle it paints.
- The RDP 6.0 (32 bpp) and RLE branches pass their decoded rows through it.
- The uncompressed branch, which already stripped row padding, now also strips the columns
  beyond the rectangle.

Why: a bitmap update is sent wider than the rectangle it paints (its width is padded), and
the decoders produce rows of that width, while the `DecodedImage::apply_*` functions cut
rows at the rectangle's width. Every row then starts a few pixels further than it should,
and the picture shears. Measured on 2026-09-26 against the xrdp login screen: sheared with
the published code, exact with the patch. The live test
`crates/heimdall-rdp/tests/live_xrdp.rs` checks it: on every middle row of the login
window, the first white pixel must sit in the same column (11 different columns with the
published code, 1 with the patch).

Remove when: a published `ironrdp-session` crops the decoded rows to the rectangle (or
decodes to the rectangle's width). Then delete this directory and its `[patch.crates-io]`
entry, and run the live test against the xrdp container.
