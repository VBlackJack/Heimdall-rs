[Version française](README.fr.md)

# Heimdall-rs

A rewrite of [Heimdall](https://github.com/VBlackJack/Heimdall), the multi-protocol
remote connection manager, in Rust, for Windows and Linux.

## Status

Early development, working towards a first milestone: an SSH terminal tab opened from a
saved profile. Done: profiles and their import from the C# Heimdall, the SSH client
(host keys, agent, key files, keyboard-interactive, password), terminal emulation with
keyboard, mouse and paste encoding, and the desktop window around them (profile list,
tabs, host key and credential questions, confirmations). The window is tested headless
and starts on Linux; it has not yet been walked through by hand against a live server.

Known limitations of the milestone: characters the embedded Source Code Pro font lacks
(CJK, Braille) are drawn with whatever the system provides; no screen reader support, as
iced has none yet.

The target is feature parity with the C# Heimdall: SSH, SFTP, FTP/FTPS,
RDP, VNC, Telnet, serial, local shell, credential vault, command library,
diagram editor, updater, and English, French and Spanish localisation.

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
