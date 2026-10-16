[Version française](README.fr.md)

# Heimdall-rs

A rewrite of [Heimdall](https://github.com/VBlackJack/Heimdall), the multi-protocol
remote connection manager, in Rust, for Windows and Linux.

## Status

> **Not usable yet.** This is a rewrite in progress, published so its work can be
> followed. It does not yet do what the C# Heimdall does, its formats may change without
> notice, and it is not released. Use [Heimdall](https://github.com/VBlackJack/Heimdall).

Early development. The first milestone is done: an SSH terminal tab opened from a saved
profile, with profiles imported from the C# Heimdall, host key checks, password, key file
(OpenSSH and PuTTY), agent and keyboard-interactive authentication, and a terminal with
scrollback, selection, copy and paste, and mouse reporting. It is tested automatically on
Linux and Windows and was walked through by hand on both on 2026-09-26. Under way: SFTP.

Known limitations: characters the embedded Source Code Pro font lacks (CJK, Braille) are
drawn with whatever the system provides; no screen reader support, as iced has none yet.

The target is feature parity with the C# Heimdall: SSH, SFTP, FTP/FTPS, RDP, VNC, Telnet,
Citrix, WinRM, local shell, credential vault, command library, diagram editor, updater,
and English, French and Spanish localisation.

The user interface is native, built with [iced](https://iced.rs), with no
embedded web engine.

## Layout

| Crate | Role |
|---|---|
| `heimdall-core` | Server profiles, settings, paths, credential vault, tool engines |
| `heimdall-app` | Application state, tab lifecycle, session manager, tool registry |
| `heimdall-i18n` | Supported languages and the check that keeps them complete |
| `heimdall-net` | TCP connections to a host by name, Happy Eyeballs (RFC 8305) |
| `heimdall-ssh` | SSH sessions, tunnels, jump hosts |
| `heimdall-sftp` | SFTP, FTP and FTPS |
| `heimdall-term` | Terminal emulation and local pseudo-terminals |
| `heimdall-rdp` | RDP sessions |
| `heimdall-twinshell` | Command library |
| `heimdall-remote` | VNC, Telnet, serial |
| `heimdall-tls` | Server certificate check of FTPS and VNC over TLS |
| `heimdall-dragout` | Local files dragged out to Explorer, the only unsafe code (Windows shell calls) |
| `heimdall-ui` | The desktop application |
| `xtask` | Developer tooling |

Only `heimdall-ui` depends on iced. Protocol crates depend on `heimdall-core` and
`heimdall-net` only.

## Built-in Tools

Heimdall embeds 30 sysadmin and security assessment tools. Not all of them are
offline: the network tools, such as the SSL/TLS Auditor, DNS Security Checker, SMB
Enumerator, Default Credentials Scanner and SecNumCloud Auditor, connect to the
hosts you give them; the others run fully offline.

The CVE Lookup uses an embedded, hand-curated database of 68 entries (63 distinct
CVEs) for common infrastructure software, spanning 2015 to 2024. It carries no
CISA KEV (Known Exploited Vulnerabilities) data and is not a live feed.

- **Security & Compliance:** SSH Key Auditor, SSL/TLS Auditor, DNS Security Checker (DNSSEC/DANE), ANSSI SecNumCloud v3.2 Auditor, SMB Enumerator, Default Credentials Scanner, Offline CVE Lookup, Password Auditor, Password Generator, SSH Key Generator, Certificate Generator.
- **Network:** Subnet Calculator, IP Converter, Network Calculator.
- **System:** Chmod Calculator, Date & Time Converter, Crontab Builder, SSH Config Generator.
- **Encoding & Tokens:** Hash Computer, HMAC Generator, JWT Debugger, TOTP Generator, Base64 Encoder/Decoder, URL Encoder/Decoder, JSON Formatter, Regex Tester, Text Diff, Text Case Converter, UUID Generator, ULID Generator.

## Build

On Linux, the default build needs no C development package, only a C compiler
(`gcc` or `clang`, for `ring`): the windowing libraries are opened at run time, not
linked. RDP sound is off by default there; a build that asks for it with
`--features audio` (`cargo build -p heimdall-ui --features audio`) links ALSA and needs
`alsa-lib-devel` (Fedora, RHEL, AlmaLinux) or `libasound2-dev` (Debian, Ubuntu). On
Windows, the build needs the MSVC build tools, and RDP sound is always built in.

Running on Linux needs the libraries the desktop loads at run time: `libxkbcommon`,
`libxkbcommon-x11` and the Wayland or X11 client libraries, and a Vulkan or OpenGL
driver, such as Mesa (without one, the window is drawn in software). Which libraries,
and when, is listed in [Native libraries](docs/native-libraries.md).

```bash
cargo run --package heimdall-ui
```

On Windows, two scripts at the root build and run it for testing by hand, then show the
end of its log (`%LOCALAPPDATA%\Heimdall-rs\data\logs\heimdall.log`) once it closes:

- `run-debug.bat`: debug build, with its console, panics shown there; log level `debug`.
- `run-release.bat`: release build, as a user runs it, without a console; level `info`.

Both take a log level as their argument, `error` to `trace`: `run-debug.bat trace`.

Automated UI tests run a debug build with the `test-data-dir` feature on a scratch folder,
never on the real profiles, vault or saved passwords: see
[Test data folder](docs/test-data-folder.md).

A published release is compiled with `HEIMDALL_RELEASE` set to its release tag, such as
`v2026.100901`: the update check compares it with the latest release on GitHub and offers
a newer one. A build without it, such as `cargo run` or the two scripts, never checks.

```bash
cargo test --workspace
```

## Localisation

Each crate that shows text owns its Fluent files, in
`i18n/<language>/<crate>.ftl`. Keys use hyphens:
`module-component-element-action`. An unknown key fails the build, and a test
fails when a language misses a key the others have.

## License

Apache License 2.0, see [LICENSE](LICENSE).
