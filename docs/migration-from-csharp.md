[Version française](fr/migration-from-csharp.md)

# Moving from Heimdall (C#) to Heimdall-rs

Heimdall-rs reads the data of the C# [Heimdall](https://github.com/VBlackJack/Heimdall)
and keeps the C# behaviour wherever it can. This guide lists how to bring your
connections over, what does not cross, and the places where Heimdall-rs differs on purpose.

## Import your connections

1. The C# Heimdall can stay installed and keep its data: Heimdall-rs only reads it.
2. Start Heimdall-rs. On the home page, **Import Connections** is enabled when the C#
   data is found: `servers.json` (and `settings.json` beside it) in
   `%LOCALAPPDATA%\Heimdall`. It is greyed out when that file does not exist.
3. Press it. A dialog reports what was added, updated and left out, with the reason for
   each profile that was skipped.

You can also use **Import Sessions** in the session tree and pick a `servers.json`
(or a file exported by the C# Heimdall). The file import also reads the `settings.json`
beside it.

Importing again is safe: a profile with the same ID is updated, not duplicated.

### What is imported

| Item | Notes |
|---|---|
| Profiles | SSH, SFTP, RDP, VNC, Telnet, FTP, WinRM, Citrix, local shell, and WEB. |
| Folders and colours | Folder paths and the colours of the C# palette. |
| Folder defaults | Copied into each profile at import (see below). |
| SSH gateways | Created or merged; a profile that referenced a missing gateway is reported. |
| Trusted SSH host keys | Keys and fingerprints are carried over, so known servers do not ask again. |
| Scheduled connections | Carried over with their schedule. |
| C# preferences | Shown for review first, and applied only when you confirm. A preference you already changed in Heimdall-rs is kept. |

Folder defaults work differently: the C# resolves them when it connects, from the folder
of the profile, deepest folder first. Heimdall-rs resolves them once, at import, and
writes the result into each profile. Changing a folder default in the C# afterwards does
not change the profiles already imported; import again to take it over.

### What is not imported

- **Passwords, passphrases, the PIN and other secrets.** The C# protects them with
  Windows DPAPI, which only that Windows account can reverse. Enter each password again
  at the first connection and save it in the vault.
- **The PIN hash, the HMAC key, the Git access token** and the credential provider's
  unlock secret, from the preferences.
- Paths that belong to the C# installation, and preference values Heimdall-rs does not
  read. The review dialog names them.
- Some settings the built-in client does not use yet (X11 forwarding, RDP printers, serial
  ports, smart cards, webcam, USB, several monitors). The import dialog lists them per
  profile.

## Differences on purpose

| Area | Heimdall-rs | Why |
|---|---|---|
| DEFAULTCREDS | Lists the known vendor default credentials only. It never tries a login. | No account lockouts, no unauthorised login attempts. |
| Route via (network tools) | The TCP tools connect through an SSH gateway using SSH channels only. Nothing is run on the gateway. Tools that need ICMP, ARP, raw sockets or UDP refuse to run through a gateway. | A gateway can carry TCP, not those protocols, and running commands on it would need rights nobody asked for. |
| Network scanner | Subnets wider than /22 are refused, and you choose which found hosts to add as sessions. | A bounded scan, and nothing is saved that you did not pick. |
| RDP compression | The built-in RDP client compresses by default (up to RDP 6.1 XCRUSH), for profiles with compression enabled, as the C# client. **Compress in the built-in client** in the settings turns it off. | If the server sends data the client cannot read, compression turns itself off for that connection and the client reconnects once without it. |
| RDP extras | An RD Gateway opens in Remote Desktop Connection (`mstsc.exe`). Options only that client honours (smart cards, printers, microphone and the like) are marked "External client (mstsc.exe) only" in the profile. | The built-in client does not implement them yet. `mstsc.exe` exists on Windows only. |
| NOTES | Work notes are Markdown files, edited as plain text; a note can be copied as Markdown or as an HTML page. | No embedded web engine. |
| DIAGRAM | A text editor for the source (Mermaid, Graphviz DOT, PlantUML, ASCII). It does not draw the diagram: render it in an external tool. | No embedded web engine. |
| CERT | Also reads the certificate of a TLS 1.0 to 1.2 server, for display only, with a warning. Heimdall-rs never negotiates those versions for a real connection. | Lets you inspect and audit old servers without weakening any connection. |
| Commands imported or shared | A command that arrives from outside (a C# or MobaXterm import, a shared session file) is never typed until you press **Run and remember** in the confirmation, and each such step is asked about on its own: changing another step, a delay, the order or the enabled flag does not trust it, and saving the profile form does not either. A remembered approval of a shared session is bound to the exact text of the command, the destination (host, port, account, gateway) and the shared file, and is not remembered at all for a session whose account is asked at connection. | The C# approved imported commands when the profile form was saved, and a shared file is edited by other people: a command must never run because something else was saved. |
| Passwords of shared sessions | The password of a shared SSH, SFTP or VNC session can be remembered on this computer: tick **Remember this password on this computer** when it is asked (the box starts cleared). It is kept once the server accepted it, in the system's credential store or in the vault, never in the settings, an export, the offline copy or a log. It is filed under the protocol, host, port and login, not under the session: a shared file that moves a session to another server or account finds no password and you are asked again. A password the server refuses is forgotten, and **Forget remembered passwords** in the shared sources settings forgets a whole source. A bastion on the route and RDP sessions remember nothing. | A shared file is edited by other people: a password must never follow a session to a server it was not typed for. |
| Field in focus | The border of the field you are typing in is 2 px, in the accent colour (yellow in high contrast). The C# draws 1 px. | The keyboard's field is seen at a glance, in every theme. |

## Run a separate profile for testing

Any build takes `--data-dir <full path>`: Heimdall-rs then keeps its configuration, data and
saved passwords under that folder, and runs beside your own instance without ever touching
its profiles or credentials. Use it to try the import on a copy of your data first. See
[`--data-dir` in a normal build](test-data-folder.md#--data-dir-in-a-normal-build).
