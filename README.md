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
| `heimdall-core` | Server profiles, settings, paths, credential vault |
| `heimdall-i18n` | Supported languages and the check that keeps them complete |
| `heimdall-ssh` | SSH sessions, tunnels, jump hosts |
| `heimdall-sftp` | SFTP, FTP and FTPS |
| `heimdall-term` | Terminal emulation and local pseudo-terminals |
| `heimdall-rdp` | RDP sessions |
| `heimdall-twinshell` | Command library |
| `heimdall-remote` | VNC, Telnet, serial |
| `heimdall-ui` | The desktop application |
| `xtask` | Developer tooling |

Only `heimdall-ui` depends on iced. Protocol crates depend on `heimdall-core` only.

## Build

On Linux, the build needs the development packages of xkbcommon, Wayland, X11
and fontconfig. Running it also needs `libxkbcommon-x11` and a Vulkan or OpenGL
driver, such as Mesa. On Windows, the build needs the MSVC build tools.

```bash
cargo run --package heimdall-ui
```

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
