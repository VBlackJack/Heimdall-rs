# Changelog

All notable changes to Heimdall-rs are recorded here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/). Versions follow the C# Heimdall's
calendar scheme: a release is tagged `vYYYY.MMDDNN` (year, month, day and the day's build
number), and its package version is `YYYY.MMDDNN.0`, the minor written without a leading zero
as Cargo requires (`2027.10101.0` for `v2027.010101`).

This is an engineering journal, kept in English only. It is not translated: a release note
meant for users is published separately.

## [Unreleased]

Heimdall-rs is a rewrite of the C# Heimdall in Rust, for Windows and Linux, published so its
work can be followed. It is not usable yet and no official release is published.

- Core: server profiles in `profiles.toml`, imported from the C# Heimdall, their secrets in
  the encrypted vault (`sealvault`) or the system's credential store, with the settings,
  folder layout and defaults mirrored from the C#.
- SSH: terminal tabs from a saved profile, host key checks, password, key file (OpenSSH and
  PuTTY), agent and keyboard-interactive authentication, and a terminal with scrollback,
  selection, copy and paste, and mouse reporting.
- SFTP: a Files tab, local and remote panes, transfers and edits.
- Copy output, secrets masked (Ctrl+Shift+M, the tab menu): the session text is masked by
  rule, reviewed in a preview and copied only on Copy. Best effort: the preview has to be read
  before it is shared.
- Capture a command from the terminal (Ctrl+Shift+L, or Shift+right-click then "Save
  selection to Command Library"): the selection of an SSH, Telnet, WinRM or local shell opens
  the editor of a new action, its pattern for the session's platform, risk at Run, the
  protocol as a tag. A multi-line selection is joined with `;` (Windows) or `&&` (Linux) for
  review. Terminal escape sequences and control characters are removed; nothing is sent or run.
- Command Library Git synchronization (Pull, Push, Sync) through the installed `git` program,
  called with an argument list and no shell: `https` and `ssh` URLs only, hooks and
  prompts disabled, time limits, credentials masked in messages. The repository holds the
  library's export (actions and runbooks); a pulled file is untrusted and goes through the
  checks of an import. A copy newer here than the repository's is kept and listed. See
  `docs/command-library-git-sync.md`.
- RDP and VNC: remote desktop sessions, with an `mstsc` fallback on Windows.
- Tools: the network and security tools ported from the C#.
- Localisation: English, French and Spanish, every string a Fluent key.
- `--data-dir <full path>` in every build: an alternate instance (a hand test, a demo
  profile) with its own configuration, data and saved passwords, as the C# `--data-dir`. The
  automated UI tests keep their own sandbox, the debug-only `test-data-dir` feature.
- No official release is published until the build artifacts are signed (issue #454).
  Unsigned pre-release builds exist for personal testing only.

[Unreleased]: https://github.com/VBlackJack/Heimdall-rs/commits/master
