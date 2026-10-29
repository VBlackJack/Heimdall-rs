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
- Protocols: SSH tunnels, jump hosts, agents (Pageant and the Windows OpenSSH agent included)
  and X11 forwarding; FTP/FTPS file panes and a sudo mode in the file panes; the built-in RDP
  client with clipboard, drives and audio, `mstsc.exe` kept for the options it lacks; Telnet;
  WinRM (PowerShell remoting in a local ConPTY); local shell; Citrix launched in Citrix
  Workspace, not embedded; web consoles opened in the system's browser, through the gateway's
  tunnel when one is set.
- Credentials: Windows Credential Manager and Secret Service, Windows Hello, external
  credential provider commands (KeePassXC, Bitwarden, 1Password and pass presets), PIN and
  lockout, folder ACLs.
- Tools: the network and security tools ported from the C#; 57 built-in tools in all, the
  C#'s 58th, the Privilege Launcher, not ported.
- Command Library: the 527 shipped actions, runbooks, history, import and export.
- Command Library: the shipped catalogue and the debloat pack resynchronised with the C# seed
  as of its commit `edb753df` (2026-10-28, the v2026.102702 seed plus its later fixes);
  Spanish titles and descriptions for every shipped action; `xtask catalogue-diff` measures
  the drift.
- Imports: the C# Heimdall's data, MobaXterm, mRemoteNG, RDCMan, PuTTY sessions, OpenSSH
  config, `.rdp` files, `known_hosts`; shared MobaXterm session sources with an offline copy
  and an import report.
- Sessions: tree with folders, colours, drag and drop, search and filters, favourites,
  reachability; split panes, floating windows, tab drag; macros with expect steps; scheduled
  connections; session logs and transcripts; screenshot; quick connect (Ctrl+K); HTTP file
  share.
- Appearance: 17 themes with accent colours, terminal colour schemes and font choice, high
  contrast, reduced motion.
- Localisation: English, French and Spanish, every string a Fluent key.
- `--data-dir <full path>` in every build: an alternate instance (a hand test, a demo
  profile) with its own configuration, data and saved passwords, as the C# `--data-dir`. The
  automated UI tests keep their own sandbox, the debug-only `test-data-dir` feature.
- Shared sessions: the password of a shared SSH, SFTP or VNC session can be remembered on this
  computer (opt in, kept once the server accepted it, filed under protocol, host, port and
  login, forgotten when refused, forgettable per source in the settings), as the C#.
- Citrix import (C# v2026.102301): only the applications the user is subscribed to are
  imported (a legacy cache with no subscription state imports as before); a second import
  finds each application by its Citrix identifier and refreshes its launch metadata in place,
  keeping names, folders and notes, and says how many were added, updated or already up to
  date; optional `SelfService.exe` path, cache folder and store file-name prefix in Settings,
  checked and carried by the settings export.
- Default theme: Magellan, an intentional divergence from the C# default (Drakul). It applies
  when no theme is set or the name is unknown; a theme already chosen is kept, and an imported
  C# theme is kept as it is. Reset to defaults on the theme goes back to Magellan.
- No official release is published until the build artifacts are signed (issue #454).
  Unsigned pre-release builds exist for personal testing only.
- Not in place: signed release artifacts and a first official release (#454, #514); Citrix
  window embedding (#449); built-in RDP smart cards, RD Gateway, microphone, printers (#443 to
  #446); TFTP in the file share (#494); gsudo in-tab elevation (#448); OCSP/CRL revocation
  checks (#447); Sysinternals/NirSoft/NanaRun detection (#450); WYSIWYG notes and a diagram
  canvas (#451, #452, by design: no embedded web engine); projects (#498); the C#'s timing
  settings (#499); screen reader support (iced has no AccessKit integration yet).

[Unreleased]: https://github.com/VBlackJack/Heimdall-rs/commits/master
