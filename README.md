[Version française](README.fr.md)

# Heimdall-rs

A rewrite of [Heimdall](https://github.com/VBlackJack/Heimdall), the multi-protocol
remote connection manager, in Rust, for Windows and Linux.

## Status

> **Not usable yet.** This is a rewrite in progress, published so its work can be
> followed. It does not yet do what the C# Heimdall does, its formats may change without
> notice, and it is not released. Use [Heimdall](https://github.com/VBlackJack/Heimdall).

State on 2026-10-28. What follows is in place and tested automatically on Linux and
Windows; none of it is in an official release yet.

- **Protocols:** SSH (terminal, tunnels, jump hosts, agents including Pageant and the
  Windows OpenSSH agent, X11 forwarding); SFTP and FTP/FTPS file panes with transfers, edits
  and a sudo mode; RDP (a built-in client with clipboard, drives and audio, and an
  `mstsc.exe` fallback on Windows for the options it lacks); VNC; Telnet; WinRM (PowerShell
  remoting in a local ConPTY); local shell; Citrix (launched in Citrix Workspace, not
  embedded); web consoles (opened in the system's browser, through the gateway's tunnel when
  one is set).
- **Credentials:** the credential vault (`sealvault`), the system credential store (Windows
  Credential Manager, Secret Service), Windows Hello, external credential provider commands
  (KeePassXC, Bitwarden, 1Password and pass presets), PIN and lockout, folder ACLs.
- **Built-in tools:** 57; the C# Heimdall has 58, its Privilege Launcher is not ported.
- **Command Library:** the 527 shipped actions, runbooks, history, capture from a terminal,
  import and export, Git synchronization.
- **Imports:** the C# Heimdall's data, MobaXterm, mRemoteNG, RDCMan, PuTTY sessions, OpenSSH
  config, `.rdp` files, `known_hosts`; shared MobaXterm session sources with an offline copy
  and an import report.
- **Sessions:** a tree with folders, colours, drag and drop, search and filters, favourites
  and reachability; split panes, floating windows, tab drag; macros with expect steps;
  scheduled connections; session logs and transcripts; masked copy; screenshot; quick connect
  (Ctrl+K); an HTTP file share.
- **Appearance and languages:** 17 themes with accent colours, terminal colour schemes and a
  font choice, high contrast, reduced motion; English, French and Spanish.
- **Separate instance:** `--data-dir` runs one beside the user's own.

Not in place yet:

- Signed release artifacts and a first official release (#454, #514).
- Citrix window embedding (#449).
- In the built-in RDP client: smart cards, RD Gateway, microphone, printers (#443 to #446).
- TFTP in the file share (#494).
- In-tab elevation with gsudo (#448).
- OCSP and CRL revocation checks (#447).
- Detection of Sysinternals, NirSoft and NanaRun (#450).
- WYSIWYG notes and a diagram canvas (#451, #452), by design: they would need an embedded
  web engine.
- Projects (#498).
- The C# Heimdall's timing settings (#499).
- Screen reader support: iced has no AccessKit integration yet.

Known limitation: characters the embedded Source Code Pro font lacks (CJK, Braille) are
drawn with whatever the system provides.

The application's visible name is Heimdall-rs, never Heimdall alone, so it runs side by side
with the C# Heimdall.

The user interface is native, built with [iced](https://iced.rs), with no
embedded web engine.

Coming from the C# Heimdall? See [Moving from Heimdall (C#) to Heimdall-rs](docs/migration-from-csharp.md).
Team on MobaXterm? See [MobaXterm shared session sources](docs/MOBAXTERM-SHARED-SESSIONS.md).
Sharing the Command Library through Git? See [Command Library: Git synchronization](docs/command-library-git-sync.md).
Stripping a personal Windows workstation? See [Windows workstation debloat pack](docs/windows-debloat-pack.md).

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
| `heimdall-twinshell` | Placeholder, empty; the command library lives in `heimdall-core` |
| `heimdall-remote` | VNC, Telnet, serial |
| `heimdall-tls` | Server certificate check of FTPS and VNC over TLS |
| `heimdall-dragout` | Local files dragged out to Explorer; unsafe code, an approved exception (Windows shell calls) |
| `heimdall-titlebar` | Title bars in the theme's colours; unsafe code, an approved exception (one `DwmSetWindowAttribute` call, as the C# `WindowThemeHelper`) |
| `heimdall-ui` | The desktop application |
| `xtask` | Developer tooling |

Only `heimdall-ui` depends on iced. Protocol crates depend on `heimdall-core` and
`heimdall-net` only.

## Built-in Tools

Heimdall-rs embeds 57 sysadmin and security assessment tools, in the five categories
of the C# Heimdall (which has 58: its Privilege Launcher is not ported). Not all of them
are offline: the network tools, such as the TLS Auditor, DNS Security Checker, SMB
Enumerator and SecNumCloud Audit, connect to the hosts you give them; the others run
fully offline. The network tools can route through an SSH gateway.

The CVE Lookup uses an embedded, hand-curated database of 68 entries (63 distinct
CVEs) for common infrastructure software, spanning 2015 to 2024. It carries no
CISA KEV (Known Exploited Vulnerabilities) data and is not a live feed.

The Default Credential Scanner lists the known vendor default credentials for a
product or service; unlike the upstream C# tool, it never attempts a login, to avoid
account lockouts and unauthorized login attempts.

- **Network (17):** Ping Monitor, DNS Lookup, Certificate Inspector, Port Scanner, Subnet Calculator, IP Address Converter, HTTP Status Codes, WHOIS Lookup, HTTP Header Analyzer, Banner Grabber, Traceroute, SNMP Walker, ARP Monitor, Firewall Tester, Network Cartography, Network Calculator, TCP Ping.
- **Security (15):** Hash Generator, HMAC Generator, Password Generator, SSH Key Generator, Certificate Generator, JWT Parser, TOTP Generator, Password Auditor, SSH Key Auditor, TLS Auditor, DNS Security Checker (DNSSEC/DANE), SMB Enumerator, Default Credential Scanner, CVE Lookup, SecNumCloud Audit (ANSSI v3.2).
- **Encoding (6):** Base64 Encoder / Decoder, URL Encoder / Decoder, JSON Formatter, Regex Tester, Text Diff, Text Case Converter.
- **System (13):** Chmod Calculator, DateTime Converter, UUID Generator, ULID Generator, Crontab Builder, SSH Config Generator, Hosts File Editor, Service Status Dashboard, Log Viewer, Cron Job Manager, Notes, Diagram Editor, Command Library.
- **External (6):** Wake-on-LAN, DNS Batch Resolver, Open Ports, Network Interfaces, Route Table, WiFi Networks.

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

Any build, release included, runs an alternate instance (a hand test, a demo profile) beside
the user's own with `--data-dir <full path>`, as the C# Heimdall does: its own configuration,
data and saved passwords under that folder. See
[`--data-dir` in a normal build](docs/test-data-folder.md#--data-dir-in-a-normal-build).

A published release is compiled with `HEIMDALL_RELEASE` set to its release tag, such as
`v2026.100901`: the update check compares it with the latest release on GitHub and offers
a newer one. A build without it, such as `cargo run` or the two scripts, never checks.

```bash
cargo test --workspace
```

`cargo xtask catalogue-diff <path to a C# Heimdall checkout>` measures how far the shipped
command catalogue and the debloat pack have drifted from the C# seed: it compares each file
as JSON, prints those that differ or exist on one side only with a count, and exits with an
error when there is any.

## Localisation

Each crate that shows text owns its Fluent files, in
`i18n/<language>/<crate>.ftl`. Keys use hyphens:
`module-component-element-action`. An unknown key fails the build, and a test
fails when a language misses a key the others have.

## License

Apache License 2.0, see [LICENSE](LICENSE).
