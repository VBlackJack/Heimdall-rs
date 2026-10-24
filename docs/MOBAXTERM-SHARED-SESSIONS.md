[Version française](fr/MOBAXTERM-SHARED-SESSIONS.md)

# MobaXterm shared session sources

For teams that keep their sessions in a MobaXterm `.mxtsessions` file on a shared folder.
Heimdall-rs reads that file and lists its sessions, read-only, next to your own. This guide
says what is read, what is left out and why, and where Heimdall-rs differs from MobaXterm
and from the C# Heimdall.

## 1. Add a shared source

1. Open **Settings > General** and find the card **Shared session sources**.
2. Press **Add a source**.
3. **Name**: the name the tree shows for this source, for example `Team`.
4. **Path of the file**: the path of the `.mxtsessions` file. The browse button picks it, or
   type a path that starts with an environment variable so it works for everyone, for
   example `%OneDriveCommercial%\Team\sessions.mxtsessions`.
5. Keep **Read this source** ticked and save.

The card says how many sessions the source holds, or why its file could not be read. The
source appears in the session tree under a header of its own, `Team (12)`, with its MobaXterm
folders underneath. A file larger than 16 MB is not read, and a source holds at most 5000
sessions.

## 2. Check the login variable

MobaXterm files often write the login as a variable, such as `[DAS]`. Heimdall-rs replaces
it with the value set in **Settings > General > MobaXterm import variables**. `DAS` defaults
to `%USERNAME%`, your Windows login. If your login on the servers is different, change the
value there; the **Resolves to** column shows what it becomes. A variable with no value is
listed in the import report (section 8) and the login is kept as written.

## 3. What is read-only, and why

Shared sessions belong to the file, which other people maintain. In Heimdall-rs they are
held in memory beside your own sessions: no menu that edits, renames, copies, moves or
deletes a saved session reaches them, and nothing is ever written back to the file. A
**shared** badge on each row names the source.

To change a shared session, change it where the team keeps it: in MobaXterm, or in the file
itself. Heimdall-rs picks up the change on its own (section 7).

Connecting works as for your own sessions, and a shared session still asks about the key or
the certificate of its server exactly as a saved one does: a file cannot trust anything for
the person who reads it.

## 4. Which sessions are taken

Heimdall-rs reads the type of each session, not its icon, so a session with a custom icon
is taken like any other.

| MobaXterm type | Taken as | What is read |
|---|---|---|
| SSH | SSH shell | host, port, user, SSH jump host, login command |
| SFTP | SFTP file session | host, port, user |
| RDP | RDP | host, port, user, SSH jump host, RD Gateway |
| Telnet | Telnet | host, port |
| VNC | VNC | host, port |

Nothing else of a session is read: no key, no password, no option. Every session not in the
table is skipped, and the import report says why (section 8).

### Login command

An SSH session can carry a login command, such as `sudo su - <account>`, that MobaXterm types
after you connect. Heimdall-rs reads it as one post-connect step, and it stays unapproved
until you choose **Run and remember**:

- The first time, Heimdall-rs shows the exact command and asks before typing it. **Run and
  remember** types it and keeps your answer on this computer; **Connect without them** opens
  the shell and types nothing.
- The answer is bound to the session, to the exact command, to the destination (host, port,
  account and bastion) and to the file it came from. If the file changes any of them, you are
  asked again.
- A session that names no account, or whose bastion names none, is asked every time: the
  account you type when connecting is not known to the approval.
- Approvals stay on this computer. They are never exported with the settings, a settings file
  cannot bring any, and removing a source forgets its approvals.
- A command with a control character, or longer than 2048 bytes, is dropped rather than
  offered, and a session holds at most 16 steps.

This is a deliberate difference from the C# Heimdall, which approves the steps of an imported
profile when its form is saved. Here, a step that comes from outside is never typed before you
choose **Run and remember** for it, whatever the file says, and saving or editing the profile does not
approve it. See [Moving from Heimdall (C#)](migration-from-csharp.md).

## 5. Gateways and RD Gateway

- **SSH jump host.** An SSH or RDP session that names a jump host in MobaXterm goes through it:
  the bastion is defined by the shared file, only for the shared sessions, and never appears in
  your own gateways. Two sessions through the same host, port and user share one bastion.
- **RD Gateway.** An RDP session set to always use an RD Gateway, and whose gateway sign-in uses
  a password (same as the end server, prompt, or a login), keeps its gateway. The built-in RDP
  client does not go through an RD Gateway, so the session opens in Remote Desktop Connection
  with the gateway set, and you sign in to the gateway with a password. The gateway login is not
  carried over. A session set to "Never" or "Detect" stores no gateway server and connects
  directly, as it does in MobaXterm.
- **Never around a route.** A session whose route Heimdall-rs cannot follow is skipped, never
  opened as a direct connection, because a direct connection would reach the server around the
  bastion, proxy or gateway the file asked for (section 8).

## 6. Passwords, keys and the shared file

A shared file never provides a password or a key: Heimdall-rs does not read them from it, even
when MobaXterm stored some, and the source header in the tree says how many it ignored. Keys come from
your SSH agent (Pageant, the Windows OpenSSH agent, or another agent).

When no agent key is accepted, Heimdall-rs asks for the password of the bastion, then of the
server, when you connect. For an SSH, SFTP or VNC session you can keep the password of the
server on this computer:

- **Opt in.** The prompt has a box, **Remember this password on this computer**. It starts
  cleared every time, and nothing is remembered unless you tick it.
- **Kept once accepted.** The password is stored after the server accepted it (the shell is
  open, the files are listed, the desktop is shown), never before. A password the server
  refuses is not kept, and a remembered one that is refused is forgotten.
- **Filed by what it opens.** It is stored per source under the protocol, the host, the port
  and the login, not under the session. A shared file that moves a session to another host,
  port or account finds no password, and you are asked again: a password never follows a
  session to a server it was not typed for.
- **First try only.** A remembered password answers the first try of a connection. If the
  server asks again, the stored one is forgotten and you type it.
- **Where it lives.** In the system's credential store, or in the vault once a master
  password is set. It is never written to the shared file, the offline copy, the settings, an
  export or the log, and the log does not say that a password is remembered.
- **Forget.** **Forget remembered passwords** in the shared sources settings forgets the
  passwords of one source, and the settings show how many are remembered. Taking a source out,
  or resetting the settings to their defaults, forgets them too.
- **Not covered.** The password of a bastion on the route is not remembered, and neither is
  any RDP password.

- A shared SFTP session asks for its password like an SSH session.
- A VNC session asks for its password when the server wants one. The connection stays open while
  you type, so the attempt does not count as a failed sign-in on servers that block your address
  after a few.
- An RDP session asks for its password at connection and Heimdall-rs does not remember it.

## 7. Automatic reload and offline copy

Heimdall-rs looks at each source file every few seconds. When it changes and has settled, that
source alone is read again, the tree follows, and your expanded folders and selection stay. Open
sessions are never closed by a reload.

If the new file cannot be read, or holds no session at all, the previous sessions are kept and
the source says why. A file that reads but whose sessions are all skipped (for example because a
jump host or a proxy was added to each) is not that case: it loads no session, and neither the
previous read nor the offline copy brings back a skipped session as a direct connection.

After each successful read, Heimdall-rs keeps a copy of the sessions in its data folder, in the
`SharedSessionCache` folder beside the settings file. If the file cannot be reached at startup
(for example while the shared folder is not synchronized yet), the sessions come from that copy
and the source says `these sessions come from the copy kept on` a date. The copy holds no login
command: a session opened from it types nothing after login. Removing the source deletes its
copy.

## 8. Read the import report

Each read also checks the file for common mistakes. In **Settings > General > Shared session
sources**, press **Import report** on a source. The report lists, by folder and session:

- **Skipped sessions**: sessions not loaded, and why (below).
- **Unresolved variables**: a `[NAME]` with no value (section 2).
- **Name and host mismatch**: a session named neither as its host nor as the first label of its
  host.
- **Gateway outliers**: in a folder of at least 3 sessions where one route covers at least 60
  percent of them, a session that takes another route. Only types that can name a jump host in
  MobaXterm count, which means SSH and RDP: a Telnet, VNC or SFTP session is direct whatever its
  folder does.
- **Duplicate hosts**: two sessions of one folder reaching the same host with the same protocol.
  An SSH session and an RDP or SFTP session to one host are two ways to use it, not duplicates;
  two SSH sessions to it are.

**Copy** puts the report on the clipboard and **Export CSV...** writes it to a file, to send to
whoever maintains the file. Only the skipped sessions are left out of the tree; every other
finding is an advice, and its session is still loaded.

### Why a session is skipped

| Session | Reason given |
|---|---|
| FTP | Not taken yet: FTP sends its password in clear. |
| WSL | A Linux system on the computer that runs MobaXterm, nothing to connect to. |
| Any other type (Serial, Mosh, RSH, Xdmcp, Browser and so on) | Session type N is not supported; the report gives the type number. |
| Telnet or VNC through an SSH jump host | Heimdall-rs does not route these protocols through a jump host, and a direct connection would go around it. |
| SSH, Telnet, VNC or SFTP through a proxy | Heimdall-rs does not use a proxy for these protocols, and a direct connection would go around it. |
| RDP through an RD Gateway with a smart card, or a gateway setting it does not recognise | Heimdall-rs signs in to a gateway with a password, and a direct connection would go around the gateway. |
| A host that is not a host name or an address, or an SSH or RD Gateway whose host is not one | The session is not taken, as it would reach its host around the gateway. |
| A second session with the same folder and name | Only the first is kept. |
| A source already holding 5000 sessions | The rest is left out. |

The one-time import of a MobaXterm file (**Import Sessions** in the session tree) skips the
sessions of the type and route rows above for the same reasons, FTP included, and says so in
its summary. The other rows concern shared sources only.

## 9. Known limits

- Shared RDP passwords and the passwords of a bastion on the route are not remembered (section 6).
- Network tools (ping, port scan and the other tools that can go through a gateway) route only
  through your own SSH gateways from the settings, not through the bastions a shared source
  defines. To reach hosts behind a shared bastion from a tool, define that bastion as one of
  your own gateways.
- The remote start folder of an SFTP session is not carried over: the session opens in the
  default folder.
- A Telnet session asks for its login in the terminal, so no user is imported for it.
- A VNC server that asks for a username stays out of reach: Heimdall-rs has no VNC username for
  a shared session.
- MobaXterm has no proxy setting for an RDP session: its other route is an RD Gateway
  (section 5).
- Shared sessions are read-only in Heimdall-rs (section 3).
