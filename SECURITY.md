[Version française](SECURITY.fr.md)

# Security Policy

## Reporting a vulnerability

Report a suspected vulnerability through GitHub private vulnerability
reporting: [open a private report](https://github.com/VBlackJack/Heimdall-rs/security/advisories/new).
The report stays visible to you and the maintainer only, and the fix is
discussed in that same thread. Do not open a public issue for a suspected
vulnerability.

Include in the report:

- affected version or commit;
- reproduction steps;
- expected impact;
- any relevant logs with credentials and hostnames redacted.

## Where to raise everything else

| Subject | Channel |
| --- | --- |
| Suspected vulnerability | [Private vulnerability reporting](https://github.com/VBlackJack/Heimdall-rs/security/advisories/new) |
| Bug, crash, regression | [Issues](https://github.com/VBlackJack/Heimdall-rs/issues) |
| Question, idea, feedback | [Discussions](https://github.com/VBlackJack/Heimdall-rs/discussions) |

## There is no mail channel

This project publishes no contact address, and mail is not a supported way to
reach the maintainer. Addresses that appear in commit metadata are an artifact
of how Git records authorship, not an invitation: they are not monitored, and
mail sent to them receives no reply. Use one of the GitHub channels above.

## Scope and controls

What this build does:

- **Secrets at rest**: saved passwords go to the vault, a file sealed with AES-256-GCM under a key derived from the master password by Argon2id, or, when no master password is set, to the system's credential store (Windows Credential Manager, the Secret Service on Linux). Windows Hello can unlock the vault; any failure falls back to the master password. Secrets the C# Heimdall protected with DPAPI are never imported.
- **Host keys and certificates**: an unknown SSH host key is shown and asked about; a changed key, a key of another algorithm or a host certificate refuses the connection. An FTPS or VNC (VeNCrypt) certificate the system does not trust is pinned only once you accept it, checked again at every connection, and decided before any password is sent; a new key raises the key-changed alarm.
- **Process execution**: programs are started from a path and a list of arguments, never through a command line a shell reads. The system programs the tools run are named by their full path, never looked up on the `PATH`; the Git sync of the command library allows only the `https` and `ssh` transports, without hooks.
- **Imported data**: a command that comes from an import or a shared session file never runs before you confirm it (see [Differences on purpose](docs/migration-from-csharp.md#differences-on-purpose)).
- **File share**: read-only, a random 256-bit token per share, the loopback address by default and every interface at once refused. A link, junction or other reparse point under the shared folder is never served nor followed.
- **Logs**: messages, typed input and clipboard text are logged in redacted forms, and terminal output is never logged. The masked copy of a terminal's output replaces the secrets its rules recognise; it is a heuristic, to be reviewed before the text is shared.
- **Memory safety**: `unsafe_code = "forbid"` for the whole workspace (`Cargo.toml`), except `heimdall-dragout` (the Windows shell's drag and drop) and `heimdall-titlebar` (the title bar colours), each allowing it in one module. Secrets the cryptography hands out are wiped when dropped. The native libraries loaded on Linux are listed in [docs/native-libraries.md](docs/native-libraries.md).
- **Dependencies**: `cargo-deny` runs in CI against `deny.toml`: security advisories, licences, and no crate from an unknown registry or Git repository. Each advisory ignored states its reason and a review date.

Known limitations:

- No signed release yet (#454).
- The vault holds the decrypted secrets in memory while it is unlocked (#536).
- No certificate revocation check, neither OCSP nor CRL (#447).
- The share folder serves 16 connections at once, and idle connections can hold all of them (#505).
