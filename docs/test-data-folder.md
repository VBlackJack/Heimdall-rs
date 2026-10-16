[Version française](fr/test-data-folder.md)

# Test data folder

Automated UI tests drive a debug build of `heimdall-rs` on a real desktop. They must never
read or change the profiles, vault, `known_hosts`, settings, logs or saved passwords of the
person whose desktop it is. A build with the `test-data-dir` feature runs on a scratch
folder instead.

## What it is, and what it is not

- A Cargo feature, `test-data-dir`, declared by `heimdall-ui` (and passed down to
  `heimdall-app` and `heimdall-core`). It is **off by default**.
- It **does not compile in a release build**: `heimdall-core` stops the build when the
  feature is on without debug assertions.
- No manifest, workflow or build script may turn it on: the guard in
  `xtask/tests/test_data_dir.rs` refuses a default feature, another feature or a dependency
  declaration that enables it, and any workflow or script that names it or asks for
  `--all-features`. The one exception is the CI job `test-data-dir (debug)`, which lints and
  tests with it and may neither build for release nor upload anything. The guard also
  refuses a release profile, or a profile inheriting it, that turns debug assertions on.
- It fails closed: a build with the feature knows no per-user folder and files credentials
  under `Heimdall-rs-test-unset` until a scratch folder is set, and the application stops
  before reading anything when none is.
- No code but `heimdall_core::paths` asks where a folder is: `xtask/tests/folder_sources.rs`
  refuses `std::env::temp_dir`, `current_exe`, the `directories` crate and the known folders
  anywhere else in the application's code.
- It is not an environment variable: whoever starts the program cannot move its folders
  through the environment, with or without the feature.
- A build without the feature reads no command-line argument at all.

## Build and start

On Windows, from the checkout:

```powershell
cargo build --locked -p heimdall-ui --features test-data-dir
target\debug\heimdall-rs.exe --data-dir C:\Temp\heimdall-ui-test
```

The folder must be a full path; it is made if it is not there. `--data-dir=<folder>` is
accepted too; that form must be text. A test build started without `--data-dir`, with any
other argument, or with a refused folder prints why and exits with code 2: it never starts
on the real folders.

The folder is compared with the real folders as the file system resolves them: links
followed, `..` resolved, Windows' `\\?\` prefix dropped, without regard to case. It may
neither be, hold nor sit in a Heimdall-rs or C# Heimdall folder, and may neither be nor
hold the home, Documents, temporary or application data folder (it may sit inside them, as
`C:\Users\<you>\AppData\Local\Temp\heimdall-ui-test` does). It is checked before it is
made, and again once made.

Every window's title ends with `[test data: <folder>]`, so a test build is never taken for
the real application.

## What goes in the folder

| Folder | What it replaces | What is in it |
|---|---|---|
| `config` | `%APPDATA%\Heimdall-rs\config` | profiles, settings, vault, `known_hosts`, host key pins and details, session snapshot, window state, split layouts, macros, the single-instance lock |
| `data` | `%LOCALAPPDATA%\Heimdall-rs\data` | logs and crash reports, files being edited, the Network Cartography's scan history (`network-scans`) and knowledge base (`network-kb.json`) |
| `legacy` | `%LOCALAPPDATA%\Heimdall` | the C# Heimdall's data, source of the migration |
| `home` | the user's profile folder | `.ssh\known_hosts` (import and export), `.Xauthority`, the Files tab's start, the file dialogs' `.ssh` start, the `~` of an OpenSSH config import, `.putty\sessions` |
| `local-app-data` | `%LOCALAPPDATA%` | the Citrix Workspace cache read by the Citrix import |
| `roaming-app-data` | `%APPDATA%` | nothing of its own: the root of `config` when not under test |
| `documents` | the Documents folder | the PowerShell module path given to a local shell |
| `temp` | the system's temporary folder | Remote Desktop Connection's connection files, and their sweep at start |

Also under a test data folder:

- The PuTTY sessions are read from `home\.putty\sessions`, never from the user's registry.
- The legacy PowerShell Heimdall is not looked for beside the program.
- Windows Hello enrolment is off: no key credential is ever created.

Machine-wide folders (the system folder, the Windows folder, Program Files) are not moved:
they hold Windows' own programs, not the user's data.

Only Heimdall-rs itself is moved. The programs it starts (Remote Desktop Connection, PuTTY,
local shells, the Citrix client, an editor) and what it reads from the environment (the SSH
agent, the clipboard, `XAUTHORITY`) still see the real user's environment and files.

## Saved passwords

Saved passwords without a master password, and the Windows Hello envelope, go to the
system's credential store (Windows Credential Manager, the Secret Service on Linux). Under a
test data folder they are filed under the service `Heimdall-rs-test-<hash>`, where `<hash>`
is 12 hex digits of the SHA-256 of the folder's resolved full path, lower-cased. The real
entries, under `Heimdall-rs`, are never read, replaced or removed.

A profile's credential read from Windows Credential Manager (the "Windows Credential
Manager" provider) is looked up as `Heimdall-rs-test-<hash>/<target>`: a test creates the
credentials it needs under that name, for example
`cmdkey /generic:Heimdall-rs-test-<hash>/corp-admin /user:admin /pass:secret`.

## Cleaning up

Delete the scratch folder. Then remove the test credentials from Windows Credential
Manager; every one of them has `Heimdall-rs-test-` in its target name:

```powershell
cmdkey /list |
    Select-String -Pattern 'Target:\s*(?:LegacyGeneric:target=)?(.*Heimdall-rs-test-.*)$' |
    ForEach-Object { cmdkey /delete:$($_.Matches[0].Groups[1].Value.Trim()) }
```

On Linux, remove the Secret Service items whose service attribute starts with
`Heimdall-rs-test-` (for example with Seahorse, or `secret-tool clear service <name>`).
