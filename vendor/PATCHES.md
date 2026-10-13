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

Also changed, for an RDP profile's session options:

- `src/lib.rs`, `Config`: two fields, `remote_console_audio` and `console_session`.
- `src/connection.rs`, the Client Info PDU: `remote_console_audio` sets
  `INFO_REMOTECONSOLEAUDIO`, the server then plays its sound itself (the C# "Remote
  playback"), as FreeRDP's play-on-server audio mode sets `RemoteConsoleAudio`.
  `NO_AUDIO_PLAYBACK` is left as `enable_audio_playback` decides: off here, as no sound is
  played on this side.
- `src/connection.rs`, the GCC blocks: `console_session` sends the Client Cluster Data the
  crate left as a `TODO(#139)`: redirection supported, version 5, session 0 named as valid,
  as FreeRDP's `gcc_write_client_cluster_data` writes it for `/admin` without
  multitransport (the C# "Run as administrator session").

Why: the published `Config` cannot ask for either, and both are read by the server before
any channel opens, so they cannot be added from outside.

Also changed, for smart-card logon left out of the build:

- `Cargo.toml`: `sspi` no longer asks for its `scard` feature; the crate gains its own
  `scard` feature, off by default, that turns it on.
- `src/credssp.rs`: the `Credentials::SmartCard` arm, its imports and the two certificate
  helpers it uses are built only with `scard`; without it, a smart-card logon fails with
  "smart card logon is not built in". Heimdall-rs never asks for one.

Why: `scard` pulled thirteen crates into the build for a logon Heimdall-rs does not offer:
`winscard`, `cryptoki` and `cryptoki-sys` (a PKCS#11 module loader), `libz-sys` (C zlib, which
the build compiled), and `heapless` 0.7 through `iso7816`, whose `atomic-polyfill` is
unmaintained (RUSTSEC-2023-0089, ignored in `deny.toml` until then).

Also changed, for Kerberos logon:

- `src/credssp.rs`, `CredsspSequence::init`: the `NegotiateConfig` names the client by
  `KerberosConfig::hostname`, where the package named it by the server it connects to. sspi
  gives that name to NTLM as the workstation and to Kerberos as the client computer, so a
  server logged the logon as coming from itself.

Why: Negotiate is used only with a `KerberosConfig`, which already carries the client's
name; the package passed the wrong one of the two names it holds.

Also changed, for no C cryptographic library in the build:

- `Cargo.toml`: `sspi` is taken with `default-features = false`. Its only default feature,
  `aws-lc-rs`, asks for `rustls?/aws-lc-rs` and `__install-crypto-provider`; nothing else
  is turned off.

Why: the connector builds no part of sspi that uses rustls (that needs sspi's `tsssp` or
`network_client`, both off), so `aws-lc-rs` was never compiled, but Cargo still resolves a
weak dependency feature when it writes `Cargo.lock`: the lock carried `aws-lc-rs`,
`aws-lc-sys` (the AWS-LC C library), `cmake`, `dunce` and `fs_extra`, and `cargo fetch`
downloaded them. Without the default feature they leave the lock; the compiled code is the
same. `deny.toml` bans both crates, so that a later bump that would build them fails
(cargo-deny reads the built graph: it never saw these lock-only entries). TLS stays on
rustls with ring, chosen explicitly where each configuration is built.

Remove when: a published `ironrdp-connector` depends on sspi 0.22 or later, can ask for the
administrative session and for sound kept on the server, leaves `scard` to the user (or
Heimdall-rs offers smart-card logon), takes sspi without its `aws-lc-rs` default, and
names the client in `NegotiateConfig` by `KerberosConfig::hostname`. Then delete this directory and the
`[patch.crates-io]` entry, map the two options to its fields, and run the gates,
`crates/heimdall-rdp/tests/session_options.rs` included: it reads both off the wire.

Checked: `git diff --no-index` against the published package shows only the lines above;
builds for `x86_64-unknown-linux-gnu` and `x86_64-pc-windows-gnu`.

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

## ironrdp-rdpdr 0.7.0

Source: `https://static.crates.io/crates/ironrdp-rdpdr/ironrdp-rdpdr-0.7.0.crate`,
MIT or Apache-2.0 (its `LICENSE-MIT` and `LICENSE-APACHE` are kept here). `Cargo.lock` of
the package removed; nothing else added.

Changed, in `src/pdu/efs.rs` only:

- `MajorFunction` gains `FlushBuffers`, `Shutdown`, `QuerySecurity`, `SetSecurity` and
  `Unknown`; any value it did not know now reads as `Unknown` instead of an error.
- `ServerDriveIoRequest` gains `Unsupported(DeviceIoRequest)`.
- `ServerDriveIoRequest::decode` tries the original decoding, renamed `decode_known`, and
  returns `Unsupported` with the request's header when it fails: an unhandled major
  function, an information class the crate does not decode.
- `DeviceAnnounceHeader::new_drive` puts the drive's name, its ASCII characters, in
  `PreferredDosName`, where the crate wrote `ignored`.

Why: the crate decodes a drive request inside the channel's `process`, and a decode error
there ends the RDP session. A Windows server sends requests the crate does not decode
(Explorer asks for a file's security descriptor when it shows its properties, an
application flushes a file), so sharing a drive would close the session at the first one.
With the patch the drive backend (`crates/heimdall-rdp/src/drives`) answers them
"not supported" and the session goes on. The test
`drives::tests::a_request_not_understood_is_answered_not_fatal` checks it.

The name: xrdp names a shared drive by its `PreferredDosName` alone, and showed every one
as `ignored` (its chansrv log, 2026-09-27); Windows reads the full name from `DeviceData`,
which the crate already fills. The test `drives::tests::a_drive_is_announced_by_its_name`
checks the encoded bytes.

Remove when: a published `ironrdp-rdpdr` answers the requests it cannot decode rather
than failing, and names a drive in `PreferredDosName`. Then delete this directory and its `[patch.crates-io]` entry, map its
answer in `DriveBackend::answer`, and run the gates.

Checked: `git diff --no-index` against the published package shows only `src/pdu/efs.rs`
(39 lines added, 2 removed); builds for `x86_64-unknown-linux-gnu` and
`x86_64-pc-windows-gnu`.
