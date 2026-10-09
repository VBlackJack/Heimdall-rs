# Copyright 2026 Julien Bombled
# Licensed under the Apache License, Version 2.0. See LICENSE.

ui-window-title = Heimdall
ui-window-title-tab = { $tab } - Heimdall
ui-window-title-detached = { $tab } - Detached

ui-sidebar-title = Sessions
ui-sidebar-tools-tab = Tools
ui-sidebar-empty = No saved profile yet.
ui-sidebar-settings-button = Settings
ui-desktop-send-keys = Send keys
ui-desktop-send-keys-tooltip = Send keys to remote
ui-desktop-anti-idle = Anti-idle
ui-desktop-anti-idle-tooltip = Anti-idle is keeping this session alive. Click to disable for the current session.
ui-desktop-stabilizing = Stabilizing session... { $seconds }s
ui-desktop-stabilizing-tooltip = Automatic resizing is paused while the session settles. Use the Resolution menu, Skip stabilization, to resume now.
ui-desktop-send-clipboard = Send clipboard
ui-desktop-send-clipboard-tooltip = Send this computer's clipboard to the server, unencrypted
ui-desktop-fullscreen = Fullscreen (F11)
ui-desktop-match-window = Match window
ui-desktop-fit-window = Fit to Window
ui-desktop-exit-fullscreen = Exit fullscreen (F11)
ui-desktop-keys-ctrl-alt-del = Ctrl+Alt+Del
ui-desktop-keys-windows = Windows key
ui-desktop-keys-alt-tab = Alt+Tab
ui-desktop-keys-ctrl-esc = Ctrl+Esc (Start menu)
ui-desktop-keys-escape = Escape
ui-desktop-keys-print-screen = Print Screen
ui-desktop-keys-f11 = F11
ui-desktop-keys-win-l = Win+L (lock workstation)
ui-desktop-keys-win-d = Win+D (show desktop)
ui-desktop-keys-win-e = Win+E (file explorer)
ui-tree-search-placeholder = Search
ui-tree-search-tooltip = Search sessions (Ctrl+F)
ui-tree-search-clear = Clear search
ui-tree-search-clear-button = x
ui-tree-search-no-results = No sessions match your search.
ui-tree-filter-tooltip = Filters
ui-tree-filter-protocols = Protocols
ui-tree-filter-connected = Connected
ui-tree-filter-gateway = Via gateway
ui-tree-filter-gateway-badge = Show gateway badge
ui-tree-filter-no-results = No sessions match your search and filters.
ui-tree-filter-reset = Reset all filters
ui-sidebar-lock-button = Lock
ui-sidebar-lock-tooltip = Lock workspace (Ctrl+L)
ui-settings-search-placeholder = Search settings...
ui-settings-search-tooltip = Search settings (Ctrl+F)
ui-settings-search-clear = Clear search filter
ui-settings-search-results = Results: { $count }
ui-settings-search-no-results = No matching settings
ui-settings-find-modified = Find modified settings
ui-settings-find-modified-hint = Lists the settings that differ from their default value.
ui-settings-modified-badge = Modified
ui-settings-modified-from-default = Modified from default: { $value }
ui-settings-reset-to-default = Reset
ui-settings-reset-to-default-tooltip = Reset to the default value: { $value }
ui-settings-tab-general = General
ui-settings-tab-terminal = Terminal
ui-settings-tab-ssh = SSH
ui-settings-tab-rdp = RDP
ui-settings-tab-security = Security
ui-settings-security = Security
ui-settings-posture-title = Security overview
ui-settings-posture-description = The choices on this panel that affect security, as they stand now. A line that needs attention says why and leads to the setting.
ui-settings-posture-summary-none = No risky setting
ui-settings-posture-summary = { $count ->
    [one] { $count } item needs attention
   *[other] { $count } items need attention
}
ui-settings-posture-line = { $label }: { $state }
ui-settings-posture-go-to = Go to setting
ui-settings-posture-state-enabled = Enabled
ui-settings-posture-state-disabled = Disabled
ui-settings-posture-state-after-minutes = { $minutes ->
    [one] After { $minutes } minute of inactivity
   *[other] After { $minutes } minutes of inactivity
}
ui-settings-posture-state-never = Never
ui-settings-posture-state-requires-vault = Needs the master password
ui-settings-posture-state-required = Required
ui-settings-posture-state-not-required = Not required
ui-settings-posture-label-rdp-nla = RDP Network Level Authentication (NLA)
ui-settings-posture-label-rdp-strict-server-auth = RDP strict server authentication
ui-settings-posture-label-transcripts = Session transcripts
ui-settings-posture-label-ps-policy = PowerShell execution policy
ui-settings-posture-label-vault = Master password
ui-settings-posture-label-auto-lock = Auto-lock when idle
ui-settings-posture-label-disconnect-on-lock = Disconnect sessions when locking
ui-settings-posture-label-credential-guard = Credential Guard
ui-settings-posture-label-windows-hello = Windows Hello before connecting
ui-settings-posture-label-update-checks = Automatic update checks
ui-settings-posture-warning-rdp-nla = Without NLA, you sign in on a server that has not proved its identity.
ui-settings-posture-warning-transcripts = Every terminal session is written to a file, including passwords or tokens echoed to the terminal.
ui-settings-posture-warning-ps-policy = This policy turns off the script signing check in the PowerShell sessions Heimdall opens.
ui-settings-posture-warning-auto-lock = The master password stays unlocked for as long as Heimdall runs.
ui-settings-posture-warning-update-checks = A security release goes unnoticed until you check by hand.
ui-settings-pin-title = Application PIN
ui-settings-pin-enabled = A PIN is currently set.
ui-settings-pin-disabled = No PIN is set.
ui-settings-pin-configure = Configure PIN...
ui-pin-enter-title = Enter PIN
ui-pin-setup-title = Configure PIN
ui-pin-field-pin = PIN
ui-pin-field-current = Current PIN
ui-pin-field-new = New PIN
ui-pin-field-confirm = Confirm PIN
ui-pin-unlock-button = Unlock
ui-pin-save-button = Save
ui-pin-remove-button = Remove PIN
ui-pin-problem-wrong = Incorrect PIN. { $remaining ->
    [one] { $remaining } attempt remaining.
   *[other] { $remaining } attempts remaining.
}
ui-pin-problem-locked-out = Too many incorrect attempts. Try again in { $minutes ->
    [one] { $minutes } minute.
   *[other] { $minutes } minutes.
}
ui-pin-problem-wrong-current = Current PIN is incorrect.
ui-pin-problem-too-short = PIN must be at least { $min } digits.
ui-pin-problem-too-long = PIN must be at most { $max } digits.
ui-pin-problem-not-digits = PIN must contain digits only.
ui-pin-problem-mismatch = The PINs do not match.
ui-pin-problem-system = The PIN could not be saved: { $detail }
ui-settings-vault-title = Master password
ui-settings-provider-title = External Credential Provider
ui-settings-connection-checks = Connection checks
ui-settings-require-credential-guard = Require Credential Guard
ui-settings-require-credential-guard-hint = When enabled, sessions open in the built-in RDP client only while Credential Guard is running on this computer. If it is not running, or its state cannot be read, they are blocked. Remote Desktop Connection is not affected.
ui-settings-credential-guard-enabled = Credential Guard: Enabled
ui-settings-credential-guard-disabled = Credential Guard: Not available
ui-settings-credential-guard-disabled-reason = Credential Guard: Not available ({ $reason })
ui-settings-credential-guard-reason-not-windows = this system is not Windows
ui-settings-credential-guard-reason-no-system-folder = the Windows system folder is unknown
ui-settings-credential-guard-reason-not-started = PowerShell could not start: { $detail }
ui-settings-credential-guard-reason-timed-out = no answer within { $seconds } seconds
ui-settings-credential-guard-reason-exited = PowerShell ended with code { $code }
ui-settings-credential-guard-reason-stopped = PowerShell ended without an exit code
ui-settings-credential-guard-reason-no-instance = Device Guard reports no state
ui-settings-credential-guard-reason-unread = the answer could not be read: { $detail }
ui-settings-credential-guard-reason-no-value = no security service is reported
ui-settings-credential-guard-reason-invalid-value = the security services reported cannot be read
ui-settings-require-windows-hello = Require Windows Hello before connecting
ui-settings-require-windows-hello-hint = When enabled, a successful Windows Hello (biometric or PIN) verification is required before stored credentials are used. If Windows Hello is not available or not enrolled, connections are blocked.
ui-settings-require-windows-hello-windows-only = Windows Hello is available on Windows only: here connections are blocked while this is on.
ui-settings-windows-hello-grace = Re-verify after
ui-settings-windows-hello-grace-refused = Windows Hello grace period must be between { $min } and { $max } minutes.
ui-profile-field-vault-entry = Vault entry name
ui-profile-vault-entry-placeholder = Leave empty to use the display name
ui-profile-vault-entry-help = Optional name of this server's entry in the external password manager. Used for the credential provider's {"{"}Title{"}"} lookup; when empty, the display name is used.
ui-status-provider-no-password = The external credential provider returned no password for "{ $name }". Check the command configuration in Settings > Security.
ui-status-provider-failed = External credential provider failed: { $detail }
ui-status-provider-timed-out = External credential provider timed out.
ui-status-windows-hello-unavailable = Windows Hello is required but unavailable. Enroll Windows Hello or disable the requirement in Settings.
ui-status-windows-hello-failed = Windows Hello verification failed. Connection cancelled.
ui-status-windows-hello-cancelled = Windows Hello verification was cancelled. Connection cancelled.
ui-status-credential-guard-required = Credential Guard is required but not enabled on this system
ui-status-vault-hello-enrol-again-failed = The vault was unlocked, but Windows Hello unlock could not be re-enabled.
ui-windows-hello-verify-reason = Verify your identity to connect
ui-status-link-not-a-folder = { $name } does not point at a directory.
ui-status-winrm-gateway-ntlm = WinRM over a gateway: Kerberos is unavailable, authentication falls back to NTLM.
ui-status-winrm-certificate-skipped = WinRM TLS certificate validation was skipped for this session.
ui-status-ftp-cleartext = FTP session to { $host }:{ $port } is using a cleartext channel. Credentials and file contents are transmitted unencrypted. Prefer SFTP or FTPS when available.
ui-status-citrix-launching = Launching Citrix session...
ui-status-citrix-launched = Citrix session launched: { $name }
ui-status-citrix-workspace-not-found = Citrix Workspace not found. Install Citrix Workspace App.
ui-status-citrix-invalid-storefront = Invalid Citrix StoreFront URL. Use an absolute HTTP or HTTPS URL.
ui-status-citrix-storefront-credentials = Citrix StoreFront URL cannot contain credentials.
ui-status-citrix-invalid-ica-file = The ICA file must be an .ica file on this computer, not on a network share.
ui-status-citrix-not-configured = No Citrix StoreFront URL or ICA file configured.
ui-status-citrix-launch-failed = Failed to launch the Citrix session.
ui-status-citrix-not-started = Failed to launch Citrix session: { $reason }
ui-status-citrix-command-rejected = The Citrix launch command contains forbidden characters (|, &, ;, `, $, newlines).
ui-status-citrix-vault-locked = Unlock the vault before launching this Citrix session.
ui-citrix-tab-storefront = StoreFront: { $url }
ui-citrix-tab-application = Application: { $name }
ui-citrix-tab-mode-cache = Mode: SelfService cache
ui-citrix-tab-mode-ica-file = Mode: ICA file
ui-citrix-tab-mode-storefront = Mode: StoreFront
ui-citrix-tab-launched-at = Launched: { $time }
ui-citrix-tab-launcher-exit = Launcher exit code: { $code }
ui-citrix-tab-launching = Launching Citrix session...
ui-citrix-tab-running = Connected. Citrix client PID: { $pid }
ui-citrix-tab-not-found = Citrix client not found yet. The session may still be starting.
ui-citrix-tab-ended = Disconnected. The Citrix client (PID: { $pid }) ended.
ui-citrix-tab-shared = The session uses a Citrix client that was already running: it cannot be told apart from the other sessions.
ui-citrix-tab-launcher-failed = The Citrix launcher failed with exit code { $code }.
ui-citrix-tab-windows-only = Citrix client tracking is available on Windows only.
ui-citrix-tab-unavailable = The running Citrix clients could not be listed: this session's client is not tracked.
ui-citrix-tab-timed-out = The running Citrix clients were not listed in time, several times in a row: this session's client is not tracked any more.
ui-citrix-tab-terminate = Terminate
ui-citrix-tab-force-terminate = Force terminate
ui-citrix-terminate-title = Terminate the Citrix session?
ui-citrix-terminate-body = Unsaved work in the remote application will be lost. Are you sure you want to terminate?
ui-citrix-force-terminate-title = Force terminate the Citrix session?
ui-citrix-force-terminate-body = The Citrix client did not close when asked. Forcing it ends it at once, with every session it runs: unsaved work in the remote application will be lost. Are you sure you want to force it?
ui-citrix-tab-terminating = Asking the Citrix client to close...
ui-citrix-tab-force-terminating = Forcing the Citrix client to close...
ui-citrix-tab-terminate-asked = The Citrix client was asked to close.
ui-citrix-tab-terminate-forced = The Citrix client was forced to close.
ui-citrix-tab-terminate-still-running = The Citrix client is still running after it was asked to close.
ui-citrix-tab-terminate-refused = The request to end the Citrix client failed: taskkill ended with exit code { $code }.
ui-citrix-tab-terminate-timed-out = The request to end the Citrix client failed: taskkill did not answer in time.
ui-citrix-tab-terminate-not-run = The request to end the Citrix client failed: taskkill could not be started.
ui-status-rdp-external-launched = External client launched: { $name } opened in Remote Desktop Connection.
ui-status-rdp-external-launched-gateway = External client launched: { $name } goes through the Remote Desktop Gateway { $gateway }, which the built-in client does not go through yet, so it opened in Remote Desktop Connection.
ui-status-rdp-external-not-windows = This profile opens in Remote Desktop Connection (mstsc.exe), which only Windows has: it cannot be opened on this system.
ui-status-rdp-external-ssh-gateway = Remote Desktop Connection (mstsc.exe) cannot go through the SSH gateway of this profile: it was not opened.
ui-status-rdp-external-not-found = mstsc.exe was not found on this computer.
ui-status-rdp-external-not-written = Failed to write the Remote Desktop connection file: { $reason }
ui-status-rdp-external-not-started = mstsc.exe did not start: { $reason }
ui-status-putty-launched = External client launched: { $name } opened in PuTTY.
ui-status-putty-launched-through = External client launched: { $name } opened in PuTTY through the SSH gateway { $gateway }.
ui-status-putty-invalid-host = Invalid target host (rejected by input validation).
ui-status-putty-invalid-username = Invalid SSH username (rejected by input validation).
ui-status-putty-key-file = The key file { $path } is missing or not named by an absolute path: PuTTY was not opened.
ui-status-putty-host-key = PuTTY was not opened: { $reason }
ui-status-putty-gateway = PuTTY was not opened, its SSH gateway not reached: { $reason }
ui-status-putty-forward = PuTTY was not opened, no local port could be opened for its SSH gateway: { $reason }
ui-status-putty-not-found = PuTTY not found. Set the path in Settings or install PuTTY in a folder of PATH.
ui-status-putty-not-started = PuTTY did not start: { $reason }
ui-status-x11-server-not-found = No X11 server found. Install VcXsrv or Xming for X11 forwarding support.
ui-settings-provider-enabled = Use external credential provider
ui-settings-provider-disabled-hint = Enable 'Use external credential provider' to configure these options
ui-settings-provider-preset = Quick setup preset
ui-settings-provider-type = Provider type
ui-settings-provider-type-command = External command
ui-settings-provider-type-credman = Windows Credential Manager
ui-settings-provider-credman-help = Reads generic credentials from Windows Credential Manager. The entry is looked up by the profile's vault entry name, or its display name when not set.
ui-settings-provider-preset-custom = Custom
ui-settings-provider-command = Provider command
ui-settings-provider-command-placeholder = e.g. keepassxc-cli show -s {"{"}title{"}"}
ui-settings-provider-placeholders = Placeholders: {"{"}Host{"}"}  {"{"}Port{"}"}  {"{"}User{"}"}  {"{"}Title{"}"}  {"{"}Database{"}"}  {"{"}KeyFile{"}"}
ui-settings-provider-username = Username command (optional)
ui-settings-provider-username-placeholder = e.g. keepassxc-cli show -s -a UserName {"{"}title{"}"}
ui-settings-provider-username-help = Optional second command that retrieves the username from the vault. Run only when the profile has no username; its output replaces the username. A failure falls back to the stored username.
ui-settings-provider-unlock = Unlock secret
ui-settings-provider-unlock-placeholder = Database master password or GPG passphrase
ui-settings-provider-unlock-help = Sent to the command's standard input for tools that prompt to unlock (e.g. keepassxc-cli or pass). Kept with your saved passwords; leave empty if not required.
ui-settings-provider-unlock-save = Save
ui-settings-provider-unlock-saved = An unlock secret is saved.
ui-settings-provider-unlock-forget = Forget
ui-settings-provider-database = Database path
ui-settings-provider-key-file = Key file path
ui-settings-provider-key-file-hint = Required when a KeePassXC key-file preset is selected.
ui-settings-browse-database-filter = Database files
ui-settings-browse-key-file-filter = Key files
ui-settings-provider-test = Test
ui-settings-provider-test-running = Testing...
ui-settings-provider-test-success = Connection successful - password retrieved.
ui-settings-provider-test-no-result = Command returned no output. Check the command and database path.
ui-settings-provider-test-timeout = Command timed out ({ $seconds }s). Check that the CLI tool is installed and responsive.
ui-settings-provider-test-no-command = Enter a command template first.
ui-settings-provider-test-no-key-file = Select a key file first.
ui-settings-provider-test-unclosed-quote = A quote in the command is not closed.
ui-settings-provider-test-error = Test failed: { $detail }
ui-settings-provider-first-line = Use only the first line of output
ui-settings-provider-first-line-help = Take only the first non-empty line of the command output, ignoring trailing status text. Required for KeePass2 KPScript (which appends an "OK: ..." line) and useful for pass (notes after the password).
ui-settings-provider-keepass2-hint = KeePass2: KPScript takes the master password on its command line (-pw:), which exposes it. For .kdbx databases, keepassxc-cli is more secure - it reads the database password from standard input via the Unlock secret field above.
ui-settings-vault-explanation = Encrypt your stored credentials under a master password. You will be asked for it each time the app starts.
ui-settings-vault-enabled = Enabled
ui-settings-vault-disabled = Disabled
ui-settings-vault-enable = Enable master password
ui-settings-vault-change = Change
ui-settings-vault-disable = Disable
ui-settings-vault-hello-enable = Enable Windows Hello unlock
ui-settings-vault-hello-disable = Disable Windows Hello unlock
ui-settings-vault-hello-max-days = Ask for the master password again after (0 = never)
ui-settings-vault-hello-max-days-hint = Windows Hello unlocks the vault until this many days have passed since the master password was last typed; then the master password is asked once.
ui-settings-vault-hello-max-days-refused = The master password reminder must be between { $min } and { $max } days.
ui-settings-vault-hello-status-available = Windows Hello unlock is available for this vault.
ui-settings-vault-hello-status-enabled = Windows Hello unlock is enabled.
ui-settings-vault-hello-status-unavailable = Windows Hello unlock requires a TPM 2.0 device and Windows Hello.
ui-settings-vault-hello-status-enrolling = Enabling Windows Hello unlock
ui-settings-vault-hello-status-unlock-required = Unlock the vault with your master password before enabling Windows Hello unlock.
ui-settings-auto-lock = Auto-lock after idle (0 = off)
ui-settings-auto-lock-hint = Lock the workspace after this many minutes of system-wide inactivity. 0 disables auto-lock.
ui-settings-auto-lock-requires-vault = Auto-lock and disconnect on lock need the master password above: turn it on to use them.
ui-settings-auto-lock-refused = Idle auto-lock threshold must be between { $min } and { $max } minutes.
ui-settings-disconnect-on-lock = Disconnect sessions when locking
ui-settings-disconnect-on-lock-hint = By default, sessions keep running hidden behind the lock. Enable this to disconnect them when the workspace locks.
ui-profile-field-password = Password
ui-profile-password-saved = Password saved
ui-profile-password-clear = Clear
ui-profile-password-clear-tooltip = Remove the password saved for this session
ui-profile-password-locked = Unlock the vault to save or change a password.
ui-profile-password-no-store = This system has no credential store: set a master password to save passwords.
ui-profile-field-passphrase = Key passphrase
ui-profile-passphrase-saved = Passphrase saved
ui-profile-passphrase-clear-tooltip = Remove the passphrase saved for this session
ui-profile-passphrase-hint = Used only to decrypt the selected SSH key. Leave blank if the key has no passphrase or is unlocked by an SSH agent.
ui-profile-error-username-for-password = A saved password needs a user name.
ui-vault-unlock-title = Enter your master password
ui-vault-unlock-button = Unlock
ui-vault-unlock-busy = Unlocking the vault
ui-vault-locked-title = Workspace locked
ui-vault-locked-body = Enter your master password to unlock.
ui-vault-enable-title = Set a master password
ui-vault-enable-body = Your stored credentials will be encrypted under this password. You will need it every time the app starts. It cannot be recovered or reset: if you forget it, Heimdall will no longer open and the stored credentials are lost.
ui-vault-enable-button = Enable
ui-vault-enable-busy = Encrypting your stored credentials
ui-vault-change-title = Change your master password
ui-vault-change-button = Change
ui-vault-change-busy = Updating the master password
ui-vault-disable-title = Disable your master password
ui-vault-disable-warning = Your stored credentials will revert to the system's credential store only and will no longer require a master password at startup.
ui-vault-disable-button = Disable
ui-vault-disable-busy = Removing master-password protection
ui-vault-field-master = Master password
ui-vault-field-current = Current master password
ui-vault-field-new = New master password
ui-vault-field-confirm = Confirm master password
ui-vault-policy-hint = Use at least { $min } characters.
ui-vault-policy-ok = Password strength is sufficient.
ui-vault-policy-too-short = Too short: use at least { $min } characters.
ui-vault-policy-complexity = Use at least { $classes } character types (lower, upper, digit, symbol), or { $long } characters or more.
ui-vault-problem-unreadable = Master password incorrect or vault corrupted.
ui-vault-problem-mismatch = Passwords do not match.
ui-vault-problem-no-system-store = This system has no credential store to take the passwords back: the master password stays.
ui-vault-problem-exists = A vault already exists.
ui-vault-hello-unlock-button = Unlock with Windows Hello
ui-vault-hello-unlock-busy = Waiting for Windows Hello
ui-vault-problem-hello-locked = Windows Hello is temporarily locked. Use your master password.
ui-vault-problem-hello-not-found = Windows Hello is no longer set up. Unlock with your master password.
ui-vault-problem-hello-failed = Could not unlock with Windows Hello. Use your master password.
ui-vault-hello-enrol-again-title = Re-enable Windows Hello unlock?
ui-vault-hello-enrol-again-body = Windows Hello was reset or removed on this device. Re-enable Windows Hello unlock for this vault now?
ui-vault-hello-enrol-again-button = Re-enable
ui-vault-problem-system = The vault file could not be used: { $detail }
ui-vault-save-failed-title = The password could not be saved.
ui-sidebar-group-none = (No Folder)

ui-home-welcome = Welcome to Heimdall-rs
ui-home-subtitle = Add a session or import your existing connections to get started.
ui-home-add-button = Add Session
ui-home-import-button = Import Connections
ui-home-explore-tools-button = Explore Tools
ui-home-shortcuts = Ctrl+N to add a session, Ctrl+K to quick connect
ui-home-select = Select a session or press Ctrl+K to connect

ui-tab-close-button = ✕
ui-tab-bell-badge = bell

ui-connect-progress = Connecting to { $target }...
ui-connect-cancel-button = Cancel

ui-hostkey-title = Unknown SSH host
ui-hostkey-body = This is the first time connecting to { $host }:{ $port }. Verify the fingerprint below matches what the server administrator provided before accepting.
ui-hostkey-fingerprint = Fingerprint: { $fingerprint }
ui-hostkey-accept-button = Accept
ui-hostkey-trust-once-button = Trust this session
ui-hostkey-reject-button = Reject
ui-certificate-title = Unrecognised Server Certificate
ui-certificate-body = "{ $name }" answered at { $host }:{ $port }, presenting a certificate this profile has never approved.
ui-certificate-caution = Heimdall cannot tell whether this is the machine you expect. Approve it only if you recognise the fingerprint below, or if you know that several machines answer to this name.
ui-certificate-fingerprint = SHA-256 fingerprint: { $fingerprint }
ui-certificate-trust-button = Trust this certificate
ui-certificate-trust-once-button = Just this once
ui-certificate-refuse-button = Do not connect

ui-prompt-submit-button = Continue
ui-prompt-cancel-button = Cancel
ui-prompt-username-title = User name for { $target }
ui-prompt-password-title = Password for { $user } on { $target }
ui-prompt-password-retry = The password was refused. Try again.
ui-prompt-passphrase-title = Passphrase of the key { $path }
ui-prompt-passphrase-retry = The passphrase did not unlock the key. Try again.
ui-prompt-server-password-title = Password of the VNC server { $target }
ui-prompt-interactive-title = { $user } on { $host }: the server asks
ui-prompt-server-text = Server says: { $text }

ui-session-closed = Session ended.
ui-session-closed-status = Session ended: process exited with code { $status }.
ui-session-cancelled = The connection was cancelled.
ui-session-failed-title = The connection failed
ui-session-close-button = Close the tab

ui-error-invalid-host = The host name is not valid.
ui-error-invalid-username = The user name is not valid.
ui-winrm-diagnostic-ntlm-loopback = WinRM authentication failed for the current Windows identity. Use a stored account for localhost or off-domain hosts.
ui-winrm-diagnostic-wsman-invalid = WinRM received an invalid WSMan response. If this session uses a gateway, verify that WinRM uses HTTP through the tunnel.
ui-error-winrm-https-gateway = WinRM over an SSH gateway does not support HTTPS. Use HTTP, or connect directly.
ui-error-winrm-unresolved = Cannot resolve WinRM host '{ $host }'.
ui-error-winrm-unreachable = WinRM host '{ $host }' is unreachable on port { $port } (connection refused or timed out).
ui-error-winrm-tls-failed = The WinRM TLS connection to '{ $host }' on port { $port } failed (untrusted certificate or handshake error).
ui-profile-winrm-gateway-http = WinRM SSL is disabled when an SSH gateway is selected. Gateway-routed WinRM uses HTTP over the local SSH tunnel.
ui-error-network = The server could not be reached: { $detail }
ui-error-timeout = The server did not answer in time.
ui-error-hostkey-changed = The server's key is not the one on record. Someone may be intercepting the connection. Recorded: { $recorded }. Presented: { $offered }. If the change is expected, forget this server: its new key is then asked about.
ui-error-hostkey-algorithm = The server no longer offers the key type on record ({ $recorded }).
ui-error-pinned-certificate-invalid = The certificate trusted for { $target } is no longer valid: connection refused. { $issue } Valid until: { $until }. Key: { $fingerprint }. If the server has a new certificate, forget this server: its certificate is then asked about.
ui-error-host-certificate = The server presented a host certificate; certificates are not supported yet.
ui-error-known-hosts = The known hosts file could not be used: { $detail }
ui-error-key-unreadable = The key file { $path } could not be read.
ui-error-key-unknown-format = The file { $path } is not a private key in a supported format.
ui-error-key-needs-passphrase = The key { $path } is encrypted and no passphrase was given.
ui-error-key-wrong-passphrase = The passphrase did not unlock the key { $path }.
ui-error-key-invalid = The key { $path } could not be loaded.
ui-error-auth-failed = Authentication failed. Methods tried: { $methods }.
ui-error-auth-failed-none = Authentication failed: the server accepted no method Heimdall could offer.
ui-error-disconnected = The server closed the connection.
ui-error-disconnected-message = The server closed the connection: { $message }
ui-error-connection-lost = Session disconnected unexpectedly.
ui-error-cancelled = Cancelled.
ui-error-prompt-timeout = A question was left unanswered for too long.
ui-error-pty-refused = The server refused to open a terminal.
ui-error-shell-refused = The server refused to start a shell.
ui-error-subsystem-refused = The server refused to start { $name }.
ui-error-protocol = SSH protocol error: { $detail }

ui-auth-method-agent = SSH agent
ui-auth-method-key-file = key file
ui-auth-method-keyboard-interactive = keyboard-interactive
ui-auth-method-password = password

ui-dialog-ok-button = OK
ui-dialog-cancel-button = Cancel
ui-dialog-close-tab-title = Close this session?
ui-dialog-close-tab-body = The session is still open. Closing the tab disconnects it.
ui-dialog-close-tab-confirm = Close
ui-dialog-close-transfers-title = Transfer In Progress
ui-dialog-close-transfers-body = A file transfer is running on "{ $name }". Closing now cancels it. Close anyway?
ui-dialog-exit-title = Quit Heimdall?
ui-dialog-exit-body = { $count ->
    [one] { $count } session is still open and will be disconnected.
   *[other] { $count } sessions are still open and will be disconnected.
}
ui-dialog-exit-confirm = Quit
ui-dialog-new-folder-title = New folder
ui-dialog-new-folder-confirm = Create
ui-dialog-rename-title = Rename
ui-dialog-rename-confirm = Rename
ui-dialog-name-placeholder = Name
ui-dialog-delete-title = Delete?
ui-dialog-delete-file-body = { $name } will be deleted. This cannot be undone.
ui-dialog-delete-folder-body = The folder { $name } and everything in it will be deleted. Links inside are removed, never what they point to. This cannot be undone.
ui-dialog-delete-confirm = Delete
ui-dialog-paste-title = Paste several lines?
ui-dialog-paste-body = { $count ->
    [one] The text holds { $count } line. The shell may run it as a command as soon as it arrives.
   *[other] The text holds { $count } lines. The shell may run each one as a command as soon as it arrives.
}
ui-dialog-paste-confirm = Paste
ui-dialog-paste-dangerous-title = Paste a dangerous command?
ui-dialog-paste-dangerous-body = The text holds { $command }, a command that can destroy data or stop the machine. Check it before it reaches the shell.
ui-dialog-paste-dangerous-confirm = Paste anyway
ui-dialog-paste-truncated = Preview is truncated. The full clipboard content will be pasted if you continue.
ui-dialog-import-title = Import finished
ui-dialog-import-counts = Added: { $added }. Updated: { $updated }. Unchanged: { $unchanged }.
ui-dialog-import-gateways = SSH gateways: { $created } created, { $merged } merged, { $orphans ->
    [one] { $orphans } orphan reference
   *[other] { $orphans } orphan references
}.
ui-dialog-import-gateways-orphans-action = Some imported sessions still reference missing SSH gateways. Re-export from a build that includes gateways, or recreate/reassign the gateway in Settings before connecting.
ui-dialog-import-skipped = Left out:
ui-dialog-import-host-keys = Trusted SSH servers carried over: { $keys ->
    [one] { $keys } key
   *[other] { $keys } keys
} and { $pins ->
    [one] { $pins } fingerprint
   *[other] { $pins } fingerprints
}.
ui-dialog-import-host-keys-failed = The trusted SSH servers could not be carried over: { $detail }
ui-dialog-import-skipped-item = { $name }: { $reason }
ui-dialog-import-dropped = Imported with settings the built-in client does not use yet:
ui-dialog-import-dropped-item = { $name }: { $settings }
ui-dialog-import-dropped-separator = {", "}
ui-import-dropped-external-client = opened in an external program
ui-import-dropped-x11 = X11 forwarding
ui-import-dropped-rdp-printers = printers
ui-import-dropped-rdp-com-ports = serial ports
ui-import-dropped-rdp-smart-cards = smart cards
ui-import-dropped-rdp-webcam = webcam
ui-import-dropped-rdp-usb = USB devices
ui-import-dropped-rdp-microphone = microphone
ui-import-dropped-rdp-multi-monitor = several monitors
ui-import-dropped-rdp-resize-delay = a resize delay of { $ms } ms, out of range (the global setting applies)
ui-import-dropped-citrix-cache-launch = launch from the Citrix Workspace cache, not imported
ui-import-dropped-local-post-connect = { $count ->
    [one] post-connect sequence of { $count } step, which a local shell never runs
   *[other] post-connect sequence of { $count } steps, which a local shell never runs
}
ui-import-dropped-command-library-links = { $count ->
    [one] { $count } post-connect step linked to the Command Library, imported without its link
   *[other] { $count } post-connect steps linked to the Command Library, imported without their links
}
ui-import-dropped-elevation = elevation "{ $mode }", now run as administrator in its own window
ui-import-elevation-auto = Auto (gsudo, then external window fallback)
ui-import-elevation-gsudo = gsudo only (embedded terminal)
ui-import-elevation-runas = External window (AdminByRequest compatible)
ui-import-elevation-unknown = unknown
ui-citrix-import-title = Citrix Applications
ui-citrix-import-none = No Citrix applications found in the local cache. Open Citrix Workspace and connect to a store first.
ui-citrix-import-confirm = { $count ->
    [one] Import { $count } Citrix application from local Workspace cache?
   *[other] Import { $count } Citrix applications from local Workspace cache?
}
ui-citrix-import-done = { $count ->
    [one] { $count } Citrix application imported successfully.
   *[other] { $count } Citrix applications imported successfully.
}
ui-citrix-import-refreshed = { $count ->
    [one] { $count } application already saved: its launch line was refreshed.
   *[other] { $count } applications already saved: their launch lines were refreshed.
}
ui-citrix-import-no-launch-lines = The vault is locked or unavailable: the launch lines from the Workspace cache were not saved. These applications launch through their StoreFront.
ui-citrix-cache-folder-missing = Citrix SelfService cache directory not found.
ui-citrix-cache-no-files = No Citrix cache files found. Open Citrix Workspace and connect to a store first.
ui-citrix-cache-unreadable = { $file }: { $detail }
ui-dialog-import-failed-title = The import could not run
ui-import-file-title = Import Sessions
ui-import-file-filter-all = All supported
ui-import-file-filter-json = JSON
ui-import-file-filter-rdp = RDP files
ui-import-file-filter-any = All files
ui-import-file-confirm = { $count ->
    [one] Import { $count } session? Existing sessions with the same ID will be updated.
   *[other] Import { $count } sessions? Existing sessions with the same ID will be updated.
}
ui-import-file-confirm-mobaxterm = { $count ->
    [one] Import { $count } session from MobaXterm? Passwords cannot be imported and must be re-entered.
   *[other] Import { $count } sessions from MobaXterm? Passwords cannot be imported and must be re-entered.
}
ui-import-file-button = Import
ui-import-file-nothing = No sessions found in the selected file.
ui-import-file-unreadable = The file could not be read: { $detail }
ui-import-file-encrypted = The file is fully encrypted. Decrypt it in mRemoteNG first (File > Save As with no encryption).
ui-import-file-too-large = The file is too large to import ({ $size } bytes).
ui-import-mobaxterm-passwords = MobaXterm passwords are encrypted with a proprietary algorithm and could not be imported. Please re-enter credentials for each session.
ui-import-mobaxterm-passwords-detected = { $count ->
    [one] Detected { $count } stored password in the MobaXterm file. MobaXterm encrypts it with a proprietary algorithm, so it was not imported - please re-enter credentials for the affected session.
   *[other] Detected { $count } stored passwords in the MobaXterm file. MobaXterm encrypts them with a proprietary algorithm, so they were not imported - please re-enter credentials for the affected sessions.
}
ui-dialog-store-title = The profile file could not be used
ui-dialog-store-body = Heimdall started with no profile; changes are saved beside the unreadable file, which is left untouched.
ui-dialog-save-failed-title = Save Error
ui-dialog-save-failed-body = Failed to save: { $detail }
ui-dialog-store-changed-title = Profiles not saved
ui-dialog-store-changed-body = The profile file was changed by another program or another Heimdall since it was read, so this change was not saved and the file was left as it is. Reopen Heimdall to load the current file, then make the change again.
ui-dialog-detail = Detail: { $detail }

ui-import-skip-not-ssh = not an SSH profile ({ $kind })
ui-import-skip-missing-host = has no host
ui-import-skip-missing-id = has no identifier
ui-import-skip-invalid-port = invalid port { $port }

ui-tab-files-title = { $name } (files)
ui-tab-rdp-forced-embedded-title = { $name } (forced embedded)

ui-files-local-title = This computer
ui-files-remote-title = Server
ui-files-ftp-cleartext-badge = Sent in clear text (no TLS)
ui-files-up-tooltip = Navigate up one level
ui-files-back-tooltip = Navigate back
ui-files-home-tooltip = Go to home directory
ui-files-refresh-tooltip = Refresh directory
ui-files-new-folder-button = New folder
ui-files-new-folder-tooltip = Create new folder
ui-files-download-button = Download
ui-files-upload-button = Upload
ui-files-loading = Loading...
ui-files-empty = Empty folder
ui-files-cancel-button = Cancel
ui-files-transfers-title = Transfers
ui-files-transfer-download = Download of { $name }
ui-files-transfer-upload = Upload of { $name }
ui-files-state-running = { $done } of { $total }
ui-files-state-running-unknown = { $done }
ui-files-state-done = Done
ui-files-state-incomplete = Done, { $count ->
    [one] { $count } entry left out (a link, an unusable name or a failure)
   *[other] { $count } entries left out (links, unusable names or failures)
}
ui-files-state-cancelled = Cancelled; start it again to resume
ui-files-state-failed = Failed: { $reason }
ui-files-size-bytes = { $value } B
ui-files-size-kib = { $value } KiB
ui-files-size-mib = { $value } MiB
ui-files-size-gib = { $value } GiB

ui-files-error-server = The server refused: { $message }
ui-files-error-no-such-file = the file does not exist
ui-files-error-permission-denied = permission denied
ui-files-error-unsupported = the server does not support this operation
ui-files-error-failure = the operation failed
ui-files-error-session = The SFTP session ended.
ui-files-error-local = This computer refused: { $detail }
ui-files-error-unsafe-name = The server's name "{ $name }" cannot be used here: { $reason }.
ui-files-error-not-a-file = Only files and folders can be transferred, not links or special files.
ui-files-error-is-link = The permissions of a symbolic link cannot be changed: the server would change those of what it points to.
ui-files-conflict-title = File conflicts
ui-files-conflict-hint = Choose what Heimdall should do before the transfer starts.
ui-files-conflict-summary = { $count ->
    [one] { $count } conflicting destination
   *[other] { $count } conflicting destinations
}
ui-files-conflict-apply-all = Apply to all:
ui-files-conflict-skip = Skip
ui-files-conflict-replace = Replace
ui-files-conflict-rename = Auto-rename
ui-files-conflict-destination = Destination
ui-files-conflict-action = Action
ui-files-conflict-apply = Apply
ui-files-conflict-folder-skip = This folder and all of its planned contents will be skipped.
ui-files-error-destination-not-a-file = Upload refused: the destination already exists and is not a regular file.
ui-files-error-replace-not-safe = Upload refused: the destination already exists and the server cannot replace it safely, so it was left as it is.
ui-files-error-too-large = The folder holds too many entries to walk.
ui-files-error-invalid-name = That name cannot be used.
ui-files-error-exists = An entry of that name exists already.
ui-files-name-not-a-name = it is not a file name
ui-files-name-separator = it contains a folder separator
ui-files-name-control = it contains a control character
ui-files-name-forbidden = it contains "{ $character }"
ui-files-name-reserved = it is a name Windows reserves
ui-files-name-trailing = it ends with a dot or a space
ui-files-name-too-long = it is too long

ui-profile-new-title = Add Session
ui-profile-edit-title = Edit Session
ui-profile-field-name = Display name *
ui-profile-field-group = Folder
ui-profile-field-host = Server *
ui-profile-field-port = Port
ui-profile-field-username = Username
ui-profile-field-key = SSH key
ui-profile-optional = optional
ui-profile-host-placeholder = server.example.org
ui-profile-save-button = Save
ui-profile-error-name-missing = Give the profile a name.
ui-profile-error-host-missing = Enter the server address.
ui-profile-error-host-invalid = The server address cannot hold a space.
ui-profile-error-host-has-user = Put the user name in its own field, not in the address.
ui-profile-error-host-has-port = Put the port in its own field, not in the address.
ui-profile-error-port-invalid = The port is a number from 1 to 65535.
ui-profile-error-username-invalid = The user name cannot hold a space.
ui-profile-error-control = A field holds a control character.
ui-dialog-delete-profile-title = Delete the profile?
ui-dialog-delete-profile-body = The profile { $name } will be deleted. Open sessions stay open. This cannot be undone.
ui-dialog-delete-profile-confirm = Delete

ui-session-forget-server-button = Forget this server
ui-session-reconnect-button = Reconnect
ui-session-accept-new-key-button = Accept new key (destructive)
ui-error-security-refused = The server refused the security Heimdall requires (Network Level Authentication): { $detail }
ui-error-rdp-protocol = RDP error: { $detail }
ui-error-vnc-protocol = VNC error: { $detail }
ui-error-vnc-security-refused = The VNC server offers no security Heimdall accepts (offered: { $offered }). Heimdall accepts VNC Authentication, directly or inside Tight or VeNCrypt, VeNCrypt's TLS with a server certificate (X509Vnc, X509Plain, X509None), and a server that asks no password only when the profile allows it. Anonymous TLS (TLSVnc, TLSPlain, TLSNone) is not supported.
ui-error-vnc-tls-required = A certificate is trusted for this VNC server, but it no longer offers TLS (offered: { $offered }). Heimdall does not fall back to an unencrypted connection: someone may stand between you and the server. If the server was reconfigured, forget its certificate in Settings, SSH tab, Trusted VNC certificates.
ui-error-vnc-tls-required-by-profile = This VNC profile requires TLS, and the server offers none (offered: { $offered }). Heimdall does not fall back to an unencrypted connection. If the server cannot encrypt, turn off "Require TLS" in the profile.
ui-session-vnc-unencrypted = Not encrypted: this desktop and what you type cross the network in clear.
ui-session-vnc-encrypted = Encrypted with { $version }, server certificate verified.
ui-session-vnc-quality = Quality
ui-session-vnc-quality-best = Best Quality
ui-session-vnc-quality-balanced = Balanced
ui-session-vnc-quality-performance = Performance
ui-session-vnc-quality-low-bandwidth = Low Bandwidth
ui-local-starting = Starting { $name }...
ui-local-admin-badge = ADMIN
ui-local-elevated-starting = { $name }: Windows asks for administrator rights.
ui-local-elevated-started = { $name } runs as administrator in its own window.
ui-local-elevated-cancelled = Elevation was cancelled by the user.
ui-local-elevated-failed = Failed to start the elevated process: { $detail }
ui-local-elevated-unsupported = Run as administrator is available on Windows only. Nothing was started.
ui-local-elevated-program = Program: { $program }
ui-local-elevated-open-again-button = Open it again
ui-error-local-shell = The local shell could not be started: { $detail }
ui-import-skip-unsafe-local = its program, arguments or folder cannot be run as written (relative path, quote, NUL character, or a folder on another machine)
ui-dialog-local-title = Run this program?
ui-dialog-local-body = The profile { $name } runs the command below. Heimdall runs it only once you agree, and asks again if it changes.
ui-dialog-local-folder = Starts in: { $folder }
ui-dialog-local-rereads = This program reads its command line again with its own rules: & | ^ < > and % in it are commands, not text.
ui-dialog-local-elevated = It runs as administrator, in its own window, once Windows asks for administrator rights.
ui-dialog-local-confirm = Run
ui-dialog-run-script-title = Run this script?
ui-dialog-run-script-body = { $name } runs in a new tab with the command below, with your rights. Heimdall asks each time it runs.
ui-error-remote-forward = The SSH gateway would not listen on its port { $port } for the remote forward: forwarding is off on it, or the port is taken there.
ui-error-proxy-port = The SOCKS proxy could not open local port { $port }: another program may be using it. ({ $detail })
ui-error-jump-refused = The SSH gateway would not connect onward to { $target }: forwarding is off on it, or that host cannot be reached from it.
ui-import-skip-missing-username = logs in with an account it does not name
ui-import-skip-unknown-identity = logs in with an identity mode Heimdall does not know
ui-error-hostkey-changed-at = The host key of { $target } is not the one recorded: the connection may be intercepted. Recorded: { $recorded }. Presented: { $offered }.
ui-error-gateway-missing = The SSH gateway { $id } this profile goes through is not in the profiles.
ui-error-gateway-loop = The SSH gateway { $id } is reached through itself, by way of its parents.
ui-tree-connect = Connect
ui-tree-connect-with = Connect with...
ui-tree-connect-with-tooltip = Override the profile's RDP mode for this connection only.
ui-tree-connect-embedded = Connect (embedded)
ui-tree-connect-external-mstsc = Connect (external mstsc)
ui-tree-connect-as = Connect as...
ui-tree-edit = Edit
ui-tree-duplicate = Duplicate
ui-tree-duplicate-suffix = {" "}(copy)
ui-tree-copy-hostname = Copy hostname
ui-tree-copy-username = Copy username
ui-tree-copy-address = Copy address
ui-tree-copy-ssh-command = Copy SSH command
ui-tree-delete = Delete
ui-tree-add-session = Add Session
ui-tree-import-sessions = Import Sessions
ui-tree-export-sessions = Export Sessions
ui-dialog-export-title = Export Sessions
ui-dialog-export-done = { $count ->
    [one] { $count } session exported successfully.
   *[other] { $count } sessions exported successfully.
}
ui-dialog-export-credentials = Credentials were not included in the export file.
ui-dialog-export-failed = Export failed: { $detail }
ui-export-filter-json = JSON Files
ui-tree-import-openssh = Import OpenSSH config...
ui-openssh-title = Import OpenSSH config
ui-openssh-summary = { $total ->
    [one] { $total } candidate
   *[other] { $total } candidates
} - { $new } new, { $duplicate ->
    [one] { $duplicate } duplicate
   *[other] { $duplicate } duplicates
}
ui-openssh-hint = ProxyJump entries are imported as SSH gateway chains.
ui-openssh-choose-all = Import all
ui-openssh-column-alias = Alias
ui-openssh-column-host = HostName
ui-openssh-column-port = Port
ui-openssh-column-user = User
ui-openssh-column-key = IdentityFile
ui-openssh-column-chain = Gateway chain
ui-openssh-column-status = Status
ui-openssh-status-new = New
ui-openssh-status-duplicate = Duplicate
ui-openssh-reusing = reusing existing gateway "{ $name }"
ui-openssh-diagnostics = Diagnostics ({ $count })
ui-openssh-diag-line = Line { $line }: { $said }
ui-openssh-diag-match = Match block not read
ui-openssh-diag-include = Include directive not followed: { $value }
ui-openssh-diag-wildcard = Wildcard alias ignored: { $value }
ui-openssh-diag-unknown = Unknown directive ignored: { $value }
ui-openssh-diag-port = Invalid port { $value }; falling back to 22
ui-openssh-diag-duplicate = Duplicate alias within file ignored: { $value }
ui-openssh-diag-proxycommand = ProxyCommand is not supported; Heimdall only supports native TCP jumps via ProxyJump: { $value }
ui-openssh-diag-mixed = ProxyJump and ProxyCommand combined; use only ProxyJump for Heimdall import: { $value }
ui-openssh-diag-jump-token = ProxyJump with OpenSSH tokens (%h/%p/%r) is not supported: { $value }
ui-openssh-diag-cycle = ProxyJump cycle detected in chain for Host { $value }
ui-openssh-diag-syntax = Unrecognised ProxyJump syntax: { $value }
ui-openssh-diag-tilde = IdentityFile ~ expanded to the home folder: { $value }
ui-openssh-diag-fallback = HostName missing; falling back to alias: { $value }
ui-openssh-diag-host-token = HostName uses an OpenSSH token Heimdall cannot expand (only %h is supported); host skipped: { $value }
ui-openssh-import-button = Import
ui-openssh-done = { $imported } imported, { $duplicates } skipped (duplicates), { $warnings ->
    [one] { $warnings } warning
   *[other] { $warnings } warnings
}
ui-openssh-done-gateways = { $count ->
    [one] { $count } SSH gateway created for the ProxyJump chains.
   *[other] { $count } SSH gateways created for the ProxyJump chains.
}
ui-openssh-unreadable = Unable to read the selected file: { $detail }
ui-openssh-empty = The selected file contains no importable entries.
ui-tree-import-putty = Import PuTTY sessions...
ui-tree-import-citrix = Import Citrix Apps
ui-putty-title = Import PuTTY sessions
ui-sessions-summary-invalid = { $total ->
    [one] { $total } candidate
   *[other] { $total } candidates
} - { $new } new, { $duplicate ->
    [one] { $duplicate } duplicate
   *[other] { $duplicate } duplicates
}, { $invalid } invalid
ui-sessions-status-invalid = Invalid
ui-sessions-no-host = (no host)
ui-putty-diag-default = PuTTY default settings skipped: { $session }
ui-putty-diag-not-ssh = Session "{ $session }" ignored because protocol "{ $value }" is not SSH
ui-putty-diag-missing-host = Session "{ $session }" has no host name and will be marked invalid
ui-putty-diag-port = Session "{ $session }" has invalid port "{ $value }"; falling back to 22
ui-putty-diag-ppk = Session "{ $session }" references a .ppk key preserved without conversion: { $value }
ui-putty-diag-proxy = Session "{ $session }" defines proxy settings that were captured but not mapped: { $value }
ui-putty-diag-forwards = Session "{ $session }" defines { $count ->
    [one] { $count } tunnel
   *[other] { $count } tunnels
} captured but not mapped
ui-putty-diag-command = Session "{ $session }" defines a startup command that was captured but not mapped: { $value }
ui-putty-done = { $imported } imported, { $duplicates } skipped (duplicates), { $invalid } invalid, { $warnings ->
    [one] { $warnings } warning
   *[other] { $warnings } warnings
}
ui-putty-unreadable = PuTTY's sessions could not be read: { $detail }
ui-putty-empty = No PuTTY SSH sessions were found.
ui-tree-import-rdp = Import RDP files...
ui-rdp-title = Import .rdp files
ui-rdp-filter = Remote Desktop files
ui-rdp-summary = { $chosen } selected / { $files ->
    [one] { $files } file
   *[other] { $files } files
}, { $conflicts ->
    [one] { $conflicts } conflict
   *[other] { $conflicts } conflicts
}, { $passwords ->
    [one] { $passwords } password warning.
   *[other] { $passwords } password warnings.
}
ui-rdp-unreadable = { $count ->
    [one] { $count } file could not be read.
   *[other] { $count } files could not be read.
}
ui-rdp-select-all = Select all
ui-rdp-select-none = Select none
ui-rdp-apply-all = Apply to all conflicts:
ui-rdp-column-source = Source
ui-rdp-column-name = Name
ui-rdp-column-host = Host
ui-rdp-column-status = Status
ui-rdp-column-conflict = Conflict
ui-rdp-conflict-skip = Skip
ui-rdp-conflict-replace = Replace
ui-rdp-conflict-rename = Auto-rename
ui-rdp-status-invalid-address = Missing or invalid RDP target address.
ui-rdp-status-rd-gateway = Goes through a Remote Desktop Gateway, not supported yet
ui-rdp-status-conflict = Conflict with { $name }
ui-rdp-status-password = Password not imported
ui-rdp-status-partial = Partial mapping
ui-rdp-status-unknown = { $count ->
    [one] { $count } unknown key
   *[other] { $count } unknown keys
}
ui-rdp-import-button = Import selected
ui-rdp-rename = { $name } (Imported { $n })
ui-rdp-fallback-name = Imported RDP
ui-rdp-done = { $imported } imported, { $replaced } replaced, { $renamed } auto-renamed, { $skipped } skipped, { $passwords ->
    [one] { $passwords } password ignored.
   *[other] { $passwords } passwords ignored.
}
ui-rdp-nothing = No valid .rdp files were found to import.
ui-tree-import-known-hosts = Import trusted SSH hosts...
ui-hostkeys-title = Import trusted SSH hosts
ui-hostkeys-pick-title = Select known_hosts file
ui-hostkeys-summary = { $total ->
    [one] { $total } entry
   *[other] { $total } entries
}: { $new } new, { $existing } already trusted, { $conflicts } conflict
ui-hostkeys-column-host = Host
ui-hostkeys-column-type = Type
ui-hostkeys-column-fingerprint = Fingerprint
ui-hostkeys-column-notes = Notes
ui-hostkeys-status-new = New
ui-hostkeys-status-existing = Already trusted
ui-hostkeys-status-conflict = Conflict
ui-hostkeys-note-existing = Same fingerprint already trusted
ui-hostkeys-note-conflict-store = Conflict with the fingerprint already trusted
ui-hostkeys-note-conflict-file = Multiple different fingerprints for this host in the source file
ui-hostkeys-diag-hashed = Hashed known_hosts entry is not supported (line { $line }).
ui-hostkeys-diag-cert-authority = @cert-authority marker is not supported (line { $line }).
ui-hostkeys-diag-revoked = @revoked marker is not supported (line { $line }).
ui-hostkeys-diag-pattern = Unsupported host pattern on line { $line }: { $value }
ui-hostkeys-diag-key-type = Unsupported key type on line { $line }: { $value }
ui-hostkeys-diag-malformed = Malformed line { $line }: { $value }
ui-hostkeys-malformed-too-long = line too long
ui-hostkeys-malformed-fields = { $count ->
    [one] { $count } field instead of 3
   *[other] { $count } fields instead of 3
}
ui-hostkeys-malformed-bad-key = the key cannot be read
ui-hostkeys-malformed-marker = unknown marker { $marker }
ui-hostkeys-done = { $imported } imported, { $existing } skipped (already trusted), { $conflicts } skipped (conflict), { $warnings ->
    [one] { $warnings } warning
   *[other] { $warnings } warnings
}
ui-hostkeys-empty = No usable entries were found in the selected known_hosts file.
ui-hostkeys-unreadable = The file could not be read: { $detail }
ui-hostkeys-too-large = The file is too large to import ({ $size } bytes).
ui-trusted-host-keys-import = Import known_hosts
ui-tree-add-tooltip = Add session
ui-tree-more-tooltip = More actions
ui-tree-tooltip-host = Host: { $host }
ui-tree-tooltip-user = User: { $user }
ui-tree-tooltip-protocol = Protocol: { $protocol }
ui-profile-protocol-picker-title = Choose a protocol
ui-profile-protocol-picker-desc = Select the connection type you want to configure.
ui-profile-protocol-rdp-name = Remote Desktop
ui-profile-protocol-rdp-desc = Windows remote desktop session
ui-profile-protocol-ssh-name = SSH
ui-profile-protocol-ssh-desc = Secure shell terminal
ui-profile-protocol-winrm-name = WinRM
ui-profile-protocol-winrm-desc = PowerShell remoting session
ui-profile-protocol-sftp-name = SFTP
ui-profile-protocol-sftp-desc = Secure file transfer over SSH
ui-profile-protocol-ftp-name = FTP
ui-profile-protocol-ftp-desc = Classic file transfer
ui-profile-port-ftp = FTP port
ui-profile-credentials-ftp = FTP Authentication
ui-profile-credentials-ftp-desc = Enter the FTP username and password. Leave blank for anonymous access.
ui-profile-options-ftp = FTP Options
ui-profile-options-ftp-desc = Configure FTP connection behavior.
ui-profile-toggle-passive = Passive mode (recommended for firewalled networks)
ui-profile-toggle-ftps = Enable SSL/TLS (FTPS)
ui-profile-protocol-citrix-name = Citrix
ui-profile-protocol-citrix-desc = Citrix Workspace published app
ui-profile-citrix-title = Citrix Workspace
ui-profile-citrix-desc = Configure the StoreFront URL and application name, or provide a direct ICA file.
ui-profile-field-storefront-url = StoreFront URL
ui-profile-field-app-name = Application name
ui-profile-citrix-advanced-title = Advanced Citrix options
ui-profile-citrix-advanced-desc = ICA file, seamless mode, and single sign-on settings.
ui-profile-field-ica-file = ICA file path
ui-profile-citrix-hint = Provide either a StoreFront URL + application name, or a direct ICA file path.
ui-profile-toggle-seamless = Seamless mode
ui-profile-toggle-sso = Use SSO (Kerberos)
ui-profile-toggle-sso-hint = Use Single Sign-On with the current Windows Kerberos identity. Requires a domain-joined client.
ui-profile-protocol-local-name = Local Shell
ui-profile-protocol-local-desc = Local terminal session
ui-profile-section-basics-local-desc = Name the session as the tree lists it.
ui-profile-local-title = Local shell
ui-profile-local-desc = Configure the shell executable and startup arguments.
ui-profile-local-executable = Executable
ui-profile-local-default-shell = The default shell
ui-profile-local-presets = Common shells
ui-profile-local-arguments = Arguments
ui-profile-local-advanced-title = Advanced shell options
ui-profile-local-advanced-desc = The folder the shell starts in.
ui-profile-local-working-directory = Working directory
ui-profile-local-run-as-admin = Run as administrator (opens its own window)
ui-profile-local-run-as-admin-hint = Windows asks for administrator rights, then starts the shell in a separate window, not in a tab.
ui-profile-local-run-as-admin-windows-only = Run as administrator is available on Windows only: here this profile opens nothing.
ui-profile-error-local-arguments = The arguments leave a quote open.
ui-profile-protocol-vnc-name = VNC
ui-profile-protocol-vnc-desc = Remote screen sharing
ui-profile-protocol-telnet-name = Telnet
ui-profile-protocol-telnet-desc = Legacy unencrypted terminal
ui-profile-protocol-badge = Protocol
ui-profile-protocol-change-tooltip = Click to change the protocol
ui-profile-tab-general = General
ui-profile-tab-options = Options
ui-profile-tab-network = Network
ui-profile-tab-info = Info
ui-profile-section-basics = Connection basics
ui-profile-section-basics-desc = Set the destination host and the service port Heimdall should open.
ui-profile-port-rdp = Remote RDP port
ui-profile-port-ssh = Remote SSH port
ui-profile-port-winrm = WinRM port
ui-profile-port-vnc = VNC port
ui-profile-port-telnet = Telnet port
ui-profile-credentials-rdp = RDP credentials
ui-profile-credentials-rdp-desc = Credentials used by the Remote Desktop session after routing is complete.
ui-profile-credentials-ssh = SSH credentials
ui-profile-credentials-ssh-desc = These credentials are used for the SSH or SFTP session itself.
ui-profile-credentials-winrm = WinRM credentials
ui-profile-credentials-winrm-desc = PowerShell remoting uses the current Windows identity or a stored credential.
ui-profile-credentials-vnc = VNC Authentication
ui-profile-credentials-vnc-desc = Configure the VNC password for authentication.
ui-profile-username-rdp-placeholder = username, DOMAIN\username, or user@domain
ui-profile-username-vnc-placeholder = optional: only for a server asking for a user name (VeNCrypt Plain, over TLS)
ui-profile-field-domain = Windows domain
ui-profile-domain-placeholder = CORP or corp.example.com
ui-profile-domain-hint = The NetBIOS name (CORP) or the DNS domain (corp.example.com). Leave empty if the username above already carries one.
ui-profile-winrm-identity = Identity
ui-profile-winrm-identity-current = Current Windows identity
ui-profile-winrm-identity-stored = Stored credential
ui-profile-winrm-password-hint = Leave the password empty to type it in PowerShell when connecting. A stored password the host refuses is not tried again until a new one is saved.
ui-profile-options-rdp = RDP session options
ui-profile-options-rdp-desc = Display, transport, and session mode settings for the Remote Desktop client.
ui-profile-options-vnc = VNC Options
ui-profile-options-vnc-desc = Configure VNC display behavior.
ui-profile-options-telnet = Telnet Options
ui-profile-options-telnet-desc = Telnet connection settings.
ui-profile-toggle-clipboard = Redirect clipboard
ui-profile-toggle-drives = Redirect drives
ui-profile-toggle-nla = Enable Network Level Authentication
ui-profile-toggle-several-servers = Several servers answer at this address: ask about each new certificate
ui-profile-toggle-anti-idle = Enable anti-idle keepalive
ui-profile-toggle-auto-reconnect = Automatically reconnect
ui-profile-experience = Visual experience
ui-profile-experience-no-wallpaper = Disable wallpaper
ui-profile-experience-no-themes = Disable themes
ui-profile-experience-no-animations = Disable menu animations
ui-profile-experience-no-drag = Disable full-window drag
ui-profile-experience-no-cursor-shadow = Disable cursor shadow
ui-profile-experience-font-smoothing = Enable font smoothing (ClearType)
ui-profile-experience-composition = Enable desktop composition
ui-profile-rdp-follow-defaults = Use global RDP defaults
ui-profile-rdp-defaults-banner = This server is using your global RDP defaults. Uncheck "Use global RDP defaults" to set per-server options.
ui-profile-rdp-defaults-not-in-effect = The greyed options below come from the global defaults: the values shown for them are this server's own, not the ones in effect.
ui-profile-rdp-tab-display-audio = Display & Audio
ui-profile-rdp-display-section = Display
ui-profile-rdp-audio-section = Audio
ui-profile-rdp-microphone = Capture local microphone
ui-profile-rdp-multi-monitor = Enable multi-monitor mode
ui-profile-rdp-multi-monitor-note = Multi-monitor uses the selected local displays. Changes require reconnection.
ui-profile-rdp-monitors-title = Selected monitors
ui-profile-rdp-monitors-caption = Choose which monitors the remote session uses. Leaving none checked = all monitors.
ui-profile-rdp-monitors-offline-kept = Monitors saved for this session that are not connected right now are kept.
ui-profile-rdp-monitor = Monitor { $number }: { $width }x{ $height }
ui-profile-rdp-monitor-primary = { $monitor } (primary)
ui-profile-rdp-monitor-vertical = { $monitor } (vertical)
ui-profile-rdp-tab-devices = Devices
ui-profile-rdp-tab-performance = Performance
ui-profile-rdp-connection-section = Connection
ui-profile-rdp-bitmap-cache = Keep bitmap cache on disk between sessions
ui-profile-rdp-compression = Enable RDP compression
ui-profile-rdp-hardware-acceleration = Use hardware-accelerated rendering
ui-profile-rdp-hardware-acceleration-unsupported = Not supported yet: the built-in client has no such switch, and Remote Desktop Connection (mstsc.exe) reads no setting for it from its .rdp file.
ui-profile-rdp-disable-udp = Avoid UDP transport probing
ui-profile-rdp-tab-behavior = Behavior
ui-profile-rdp-security-section = Security
ui-profile-rdp-full-screen = Open in fullscreen
ui-profile-rdp-mstsc-only = External client (mstsc.exe) only
ui-settings-rdp-defaults = RDP Defaults
ui-settings-rdp-defaults-hint = The options of every RDP server that uses the global defaults.
ui-settings-rdp-default-mode = Default RDP mode
ui-settings-rdp-default-mode-embedded = Embedded
ui-settings-rdp-default-mode-external = External
ui-settings-rdp-default-mode-hint = Embedded: RDP client runs inside Heimdall. External: opens mstsc.exe in a separate window.
ui-settings-rdp-auto-reconnect = Auto-reconnect
ui-settings-rdp-multi-monitor = Multi-monitor
ui-settings-rdp-audio-capture = Audio capture (microphone)
ui-settings-rdp-redirect-printers = Redirect printers
ui-settings-rdp-redirect-com-ports = Redirect COM ports
ui-settings-rdp-redirect-smart-cards = Redirect smart cards
ui-settings-rdp-redirect-webcam = Redirect webcam
ui-settings-rdp-redirect-usb = Redirect USB devices
ui-settings-rdp-bitmap-cache = Keep bitmap cache on disk
ui-settings-rdp-compression = Compression
ui-settings-rdp-hardware-acceleration = Hardware-accelerated rendering
ui-settings-rdp-strict-server-auth = Strict server authentication
ui-profile-toggle-admin = Run as administrator session (/admin)
# An RDP profile's sound and colours, as the C# Display & Audio card.
ui-profile-audio = Audio mode
ui-profile-audio-off = Disabled
ui-profile-audio-local = Local playback
ui-profile-audio-on-server = Remote playback
ui-profile-color-depth = Color depth
ui-profile-color-16 = 16-bit
ui-profile-color-24 = 24-bit
ui-profile-color-32 = 32-bit
# How an RDP profile's desktop is sized, as the C# Resolution profile card.
ui-profile-resolution-title = Resolution profile
ui-profile-resolution-desc = Choose how this server sizes the embedded Remote Desktop session.
ui-profile-resolution-mode = Resolution mode
ui-profile-resolution-fit-window = Fit window
ui-profile-resolution-fixed = Fixed
ui-profile-resolution-smart-sizing = Smart sizing
ui-profile-resolution-presets = Common resolutions
ui-profile-resolution-custom = Custom...
ui-profile-resolution-preset = { $width }x{ $height }
ui-profile-resolution-width = Width
ui-profile-resolution-height = Height
ui-profile-resize-delay = Dynamic resize delay (ms)
ui-profile-resize-delay-global = { $ms } (global default)
ui-profile-resolution-scale-fixed = Scale fixed resolution to fit the pane
ui-profile-resolution-dynamic = Allow dynamic resolution updates
ui-profile-nla-off-hint = Without Network Level Authentication, a saved password is not sent: Heimdall asks for it.
ui-profile-toggle-use-ssl = Use SSL
ui-profile-toggle-skip-cert = Skip certificate validation (insecure)
ui-profile-use-ssl-hint = Uses WinRM over HTTPS, normally port 5986. HTTP normally uses port 5985.
ui-profile-skip-cert-hint = Disables TLS certificate verification. Use only for trusted internal hosts with self-signed certificates.
ui-profile-winrm-tls-on-http-port = TLS is enabled but the port is the plaintext default { $http }; WinRM over TLS listens on { $https }.
ui-profile-toggle-view-only = View-only mode (no keyboard or mouse input)
ui-profile-toggle-no-password = Allow a server that asks no password
ui-profile-toggle-require-tls = Require TLS (refuse a server that does not encrypt)
ui-profile-telnet-warning = Telnet sends everything unencrypted, passwords included.
ui-profile-section-organization = Organization
ui-profile-section-organization-desc = Use grouping metadata to keep the session list organized.
ui-profile-section-metadata = Metadata
ui-profile-section-metadata-desc = Optional tags and Wake-on-LAN metadata for this session entry.
ui-profile-folder-placeholder = Production/Databases
ui-profile-browse-button = Browse...
ui-profile-browse-key-title = Select SSH Key
ui-profile-browse-key-all = All files
ui-profile-browse-key-ppk = PPK files
ui-profile-browse-key-pem = PEM files
ui-profile-folder-hint = Use / to nest folders: Production/Databases puts this session in Databases, inside Production.
ui-profile-error-username-missing = Username is required.
ui-profile-error-domain-invalid = The domain cannot hold a space or a double quote.
ui-profile-error-fixed-width = RDP fixed width must be between { $min } and { $max }.
ui-profile-error-fixed-height = RDP fixed height must be between { $min } and { $max }.
ui-profile-error-resize-delay = Leave the RDP resize delay empty to inherit the global default, enter 0 to disable the lockout, or enter a value from { $min } to { $max } ms.
ui-profile-error-socks-port = The SOCKS5 port must be a number from 0 to 65535; 0 disables the proxy.
ui-profile-error-remote-bind-port = The remote port must be a number from 0 to 65535; 0 disables the forward.
ui-profile-error-remote-local-port = The local port must be a number from 0 to 65535; 0 uses the remote port.
ui-profile-gateway-routing = Gateway routing
ui-profile-gateway-routing-desc = Use an SSH gateway when the target server is only reachable through a bastion or jump host.
ui-profile-direct-connect = Connect directly without an SSH gateway
ui-profile-gateway-direct-hint = Direct connection is selected. Clear it to route this session through a gateway.
ui-profile-gateway-explain-tunnel = Traffic will be routed through this SSH gateway.
ui-profile-socks-title = SOCKS5 Proxy
ui-dialog-post-connect-title = Run post-connect commands?
ui-dialog-post-connect-body = { $count ->
    [one] "{ $name }" was imported and will automatically run { $count } command in this session. Only continue if you trust this profile. Run it and remember this choice?
   *[other] "{ $name }" was imported and will automatically run { $count } commands in this session. Only continue if you trust this profile. Run them and remember this choice?
}
ui-dialog-post-connect-run = Run and remember
ui-dialog-post-connect-skip = Connect without them
ui-profile-toggle-forward-agent = Forward SSH agent
ui-profile-ssh-mode = SSH mode
ui-profile-ssh-mode-embedded = Embedded (integrated)
ui-profile-ssh-mode-external = External (PuTTY)
ui-profile-ssh-mode-external-desc = Opens PuTTY in a separate window. PuTTY asks for the password itself.
ui-profile-toggle-x11 = Enable X11 forwarding
ui-profile-toggle-x11-hint = Forward X11 display to local machine for graphical applications (-X flag)
ui-profile-x11-warning = X11 forwarding lets the remote host see this computer's display and the keys typed in its windows. Turn it on only for a server you trust.
ui-profile-toggle-compression = Enable compression
ui-profile-toggle-legacy-algorithms = Allow legacy algorithms for older devices
ui-profile-legacy-algorithms-hint = SHA-1 key exchanges, CBC ciphers, HMAC-SHA1 and SHA-1 RSA host keys are offered after the current ones. Turn this on only for a device that speaks nothing newer.
ui-profile-options-ssh = SSH options
ui-profile-options-ssh-desc = Session mode and SSH feature flags used by the shell or file transfer connection.
ui-post-connect-title = Post-connect sequence
ui-post-connect-hint = These steps run after the embedded SSH session is ready. Delays apply before each step.
ui-post-connect-empty = No steps yet. Add a step to send commands automatically once this session is connected.
ui-post-connect-command = Command
ui-post-connect-command-placeholder = Command to send, for example: sudo -i
ui-post-connect-delay = Delay (ms)
ui-post-connect-on-failure = On failure
ui-post-connect-failure-continue = Continue
ui-post-connect-failure-stop = Stop sequence
ui-post-connect-order-hint = Order matters. Delays are applied before each enabled step.
ui-post-connect-add = Add
ui-post-connect-remove = Remove
ui-post-connect-move-up = Move up
ui-post-connect-move-down = Move down
ui-post-connect-tooltip = { $progress } - { $status } - { $command }
ui-post-connect-running = Running
ui-post-connect-completed = Completed
ui-post-connect-failed = Failed
ui-post-connect-skipped = Skipped
ui-post-connect-cancelled = Cancelled
ui-profile-socks-desc = Opens a local SOCKS5 proxy port through the gateway. Set to 0 to disable.
ui-profile-socks-port = Local port
ui-profile-socks-off = Disabled
ui-profile-remote-title = Remote Port Forwarding
ui-profile-remote-desc = Opens a port on the SSH server and forwards connections to a local port. Remote port is required; leave local port at 0 to use the same value.
ui-profile-remote-bind-port = Remote port (server)
ui-profile-remote-local-port = Local port
ui-profile-remote-local-hint = 0 = same as remote port
ui-profile-remote-route = server:{ $remote } -> local:{ $local }
ui-profile-gateway-explain-direct = Select a gateway if the server is only reachable through an SSH jump host.
ui-profile-edit-gateway = Edit gateway credentials...
ui-gateway-list-empty = No gateways configured
ui-gateway-empty-hint = Add an SSH gateway to establish secure tunneled connections to your sessions.
ui-gateway-add = Add Gateway
ui-gateway-add-title = Add SSH Gateway
ui-gateway-edit-title = Edit SSH Gateway
ui-gateway-field-name = Name
ui-gateway-field-host = Host
ui-gateway-field-port = Port
ui-gateway-field-username = Username
ui-gateway-field-key = Key Path
ui-gateway-field-password = Password
ui-gateway-password-hint = Used for SSH password authentication. Leave blank if using key-only or SSH agent auth.
ui-gateway-field-parent = Parent Gateway
ui-gateway-parent-none = None (direct connection)
ui-gateway-error-loop = A gateway cannot be reached through itself.
ui-tree-gateway-via = via { $name }
ui-tree-gateway-missing = gateway missing

## A tab's menu, as the C# Heimdall's.
ui-tab-menu-disconnect = Disconnect
ui-tab-menu-rename = Rename tab
ui-tab-menu-reset-title = Reset title
ui-tab-menu-fullscreen = Fullscreen (F11)
ui-tab-menu-reconnect = Reconnect Session
ui-tab-menu-duplicate = Duplicate Session
ui-tab-menu-detach = Detach to Window
ui-detach-reattach = Reattach to Main Window
ui-tab-menu-close-others = Close others
ui-tab-menu-close-right = Close to the right
ui-split-merge-with = Merge with...
ui-split-horizontal = Horizontal
ui-split-vertical = Vertical
ui-split-unsplit = Unsplit
ui-split-swap-panes = Swap Panes
ui-split-toggle-orientation = Toggle Split Orientation
ui-split-close-secondary = Close Secondary Pane
ui-split-detach-secondary = Detach Secondary Pane
ui-split-max-panes-reached = Maximum number of panes reached ({ $max }).
ui-status-detach-split-refused = A split tab cannot be moved to its own window. Unsplit it first.
ui-split-menu = Split...
ui-split-session-tooltip = Split session view
ui-split-palette-hint = Search server to split with...
ui-split-drop-to-split = Drop to split
ui-tab-drag-detach-hint = Release to detach to a window
ui-split-open-in-split = Open in split
ui-split-open-in-split-disabled = Open a session first: a split divides the active session.
ui-dialog-rename-tab-title = Rename Tab
ui-dialog-rename-tab-prompt = Enter new tab name:
ui-dialog-close-tabs-title = Close Sessions
ui-dialog-close-tabs-body = Sessions to close: { $count }. Still connected: { $live }. Continue?

## The failure card's other ways out, and the report "Copy error" copies.
ui-session-copy-error-button = Copy error
ui-session-edit-profile-button = Edit profile
ui-error-report-header = Heimdall { $protocol } error report
ui-error-report-time = Time:
ui-error-report-server = Server:
ui-error-report-app = App:
ui-error-report-tunnel = Tunnel:
ui-error-report-tunnel-route = via { $route }
ui-error-report-tunnel-hops = { $count ->
    [one] through { $count } SSH gateway
   *[other] through { $count } SSH gateways
}
ui-error-report-session = Session:
ui-error-report-session-duration = connected for { $duration }
ui-error-report-duration = { $minutes }m { $seconds }s

## How a connection failed, as the C# Heimdall tells them apart, and why a server ended one.
ui-error-network-refused = Connection refused.
ui-error-network-reset = Connection reset.
ui-error-network-timed-out = Connection timed out. Check that the host is reachable.
ui-error-network-unreachable = Host or network is unreachable. Check DNS and routing.
ui-session-closed-reason = Reason: { $reason }

## Why an RDP server refused a logon or ended a session, as the C# Heimdall says it.
ui-rdp-severity-warning = Warning:
ui-rdp-severity-error = Error:
ui-rdp-reason-bad-credentials = The credentials were not accepted. Verify your username, password, and domain (NetBIOS DOMAIN\user or UPN user@domain.com), then try reconnecting.
ui-rdp-reason-password-expired = The password has expired and must be changed before connecting.
ui-rdp-reason-account-locked-out = The account is currently locked out. Wait for the lockout to expire, or ask your administrator to unlock it.
ui-rdp-reason-account-disabled = The account is disabled on the remote computer. Ask your administrator to enable it, then try connecting again.
ui-rdp-reason-account-expired = The account has expired. Ask your administrator to renew it.
ui-rdp-reason-time-of-day = The account is not allowed to sign in at this time. A logon-hours restriction on the account ended the session.
ui-rdp-reason-no-authority = No authentication authority could be reached to validate the account. The remote computer may have lost contact with its domain controller.
ui-rdp-reason-clock-skew = The clocks of this computer and the remote computer are too far apart for authentication to succeed. Correct the system time on either side, then try reconnecting.
ui-rdp-reason-security-error = A security error prevented the connection. The remote computer reported that the security data exchanged during the connection was not valid.
ui-rdp-reason-admin-disconnect = The remote computer ended the session. An administrator may have ended it, the connection may have failed while it was being established, or a network problem may have interrupted it.
ui-rdp-reason-license = A Remote Desktop licensing error blocked the session. Contact your administrator; the license server may be unreachable or out of CALs.
ui-rdp-reason-with-severity = { $severity } { $reason }
ui-rdp-certificate-refused = Connection cancelled: you did not approve the certificate this server presented.

## A dropped desktop opening again by itself, as the C# countdown.
ui-session-reconnecting = Reconnecting (attempt { $attempt }/{ $max })...
ui-session-reconnecting-in = in { $seconds }s
ui-session-reconnecting-cancel = Cancel

## A folder's menu and dialogs, as the C# Heimdall's.
ui-folder-connect-all = Connect all ({ $count })
ui-folder-new = New folder
ui-folder-rename = Rename
ui-folder-move-to = Move to
ui-folder-move-top = Top level
ui-folder-delete = Delete folder
ui-folder-new-title = New Folder
ui-folder-rename-title = Rename Folder
ui-folder-name-field = Folder name:
ui-folder-error-collision = A folder with this name already exists at the same level.
ui-folder-error-invalid = A folder name cannot be empty or contain "/".
ui-folder-delete-body = Delete folder "{ $name }"? Affected entries in this folder and its subfolders, including entries hidden by the current filter: { $count }. All will be moved to "(No Folder)".
ui-folder-connect-all-title = Connect All
ui-folder-connect-all-body = Connect to all { $count } sessions in this folder?
ui-folder-connect-all-confirm = Connect

## A profile's Rename and "Move to folder", as the C# tree menu's.
ui-tree-rename = Rename
ui-tree-rename-title = Rename Session
ui-tree-move-to-folder = Move to folder

## Several profiles selected together, as the C# bulk menu.
ui-selection-count = { $count ->
    [one] { $count } item selected
   *[other] { $count } items selected
}
ui-selection-connect = Connect selected ({ $count })
ui-selection-duplicate = Duplicate selected
ui-selection-delete = Delete selected ({ $count })
ui-dialog-delete-selection-title = Delete Selected Items
ui-dialog-delete-selection-body = Are you sure you want to delete { $count ->
    [one] { $count } selected item
   *[other] { $count } selected items
}?

## Quick Connect, as the C# Ctrl+K palette without its tools.
ui-palette-placeholder = Search host or IP... (Ctrl+K)
ui-palette-ssh-to = [SSH] Connect to { $target }
ui-palette-rdp-to = [RDP] Connect to { $target }
ui-palette-quick-connect = Quick Connect
ui-palette-nothing = No session matches, and this is no host to connect to.

## The status bar, as the C# one.
ui-status-ready = Ready. Select a session to get started.
ui-status-connected = Connected to: { $name }
ui-status-connected-short = Connected
ui-status-state = { $name }: { $state }
ui-status-connecting = Connecting...
ui-status-reconnecting = Reconnecting...
ui-status-disconnected = Disconnected
ui-status-error = Error
ui-status-copied = Copied to clipboard: { $text }
ui-status-report-now = Now: { $text }
ui-status-report-line = { $time } { $text }
ui-status-folder-created = Folder "{ $path }" created.
ui-status-sessions = { $count ->
    [one] { $count } session
   *[other] { $count } sessions
}
ui-status-sessions-filtered = { $shown } of { $count ->
    [one] { $count } session
   *[other] { $count } sessions
}
ui-status-separator = {" | "}

## The terminal's search bar, as the C# one.
ui-find-placeholder = Search...
ui-find-previous = ▲
ui-find-next = ▼
ui-find-close = ✕
ui-find-nothing = No match

## The terminal's appearance in the Settings page, as the C# one.
ui-settings-terminal = Terminal Appearance
ui-settings-color-scheme = Color scheme
ui-settings-appearance = Appearance
ui-vault-problem-locked-out = Too many incorrect attempts. Try again in { $minutes ->
    [one] { $minutes } minute.
   *[other] { $minutes } minutes.
}
ui-settings-language = Language
# Each language in its own name, whatever the language shown, as the C# list.
ui-settings-language-en = English
ui-settings-language-fr = Français
ui-settings-language-es = Español
# The window's theme and accent, as the C# lists them: the theme names are not translated.
ui-settings-theme = Theme
ui-theme-dracula = Dracula
ui-theme-drakul = Drakul
ui-theme-striga = Striga
ui-theme-cinder = Cinder
ui-theme-bracken = Bracken
ui-theme-tarn = Tarn
ui-theme-mortis = Mortis
ui-theme-slate = Slate
ui-theme-magellan = Magellan
ui-theme-voivode = Voivode
ui-theme-carmilla = Carmilla
ui-theme-whitby = Whitby
ui-theme-vesper = Vesper
ui-theme-parchment = Parchment
ui-theme-folio = Folio
ui-theme-wormwood = Wormwood
ui-theme-sconce = Sconce
ui-theme-high-contrast = High contrast
ui-settings-accent = Accent
ui-accent-default = Default
ui-accent-blue = Blue
ui-accent-cyan = Cyan
ui-accent-green = Green
ui-accent-orange = Orange
ui-accent-pink = Pink
ui-accent-purple = Purple
ui-accent-red = Red
ui-accent-yellow = Yellow
ui-settings-font-size = Font size
ui-settings-font-size-unit = px
ui-settings-font-size-refused = Terminal font size must be between { $min } and { $max }.
ui-settings-font-family = Font family
ui-settings-font-family-missing = { $family } is not installed on this computer: the terminal uses { $fallback }.
ui-scheme-default = Default
ui-scheme-dracula = Dracula
ui-scheme-solarized-dark = Solarized Dark
ui-scheme-monokai = Monokai
ui-scheme-nord = Nord

## Session transcripts, as the C# session log.
ui-tab-menu-start-transcript = Start Transcript
ui-tab-menu-stop-transcript = Stop Transcript
ui-tab-recording = REC
ui-tab-recording-tooltip = Session output is being recorded
ui-status-transcript-started = Transcript started: { $path }
ui-status-transcript-stopped = Transcript stopped
ui-status-transcript-failed = The transcript could not be written and stopped: { $reason }
ui-transcript-header = ===== Session started { $started } | { $protocol } | host { $host } | { $title } =====
ui-transcript-footer = ===== Session ended { $ended } | duration { $duration } =====
ui-settings-session-logging = Session Logging
ui-settings-session-logging-record = Record session transcripts (what each terminal shows, typed input included)
ui-settings-session-logging-warning = Transcripts keep what you type as well as what is shown, including passwords or tokens echoed to the terminal. Keep the log folder private.
ui-settings-ssh-auto-reconnect = SSH auto-reconnect
ui-settings-ssh-auto-reconnect-description = Automatically retry an SSH session that disconnects unexpectedly. Disabled by default.
ui-settings-ssh-auto-reconnect-enable = Enable bounded auto-reconnect
ui-settings-ssh-auto-reconnect-attempts = Max attempts before falling back to manual reconnect
ui-settings-ssh-session = Session
ui-settings-sftp = SFTP browser
ui-settings-sftp-browser-enabled = Enable integrated SFTP browser
ui-settings-sftp-auto-open = Auto-open SFTP panel on SSH connection
ui-settings-sftp-follow = SFTP follows SSH working directory
ui-settings-dock-local-browser = Dock a file browser beside local shells
ui-settings-local-follow = Local file browser follows the shell's working directory
ui-settings-ssh-keep-alive-interval = SSH keep-alive interval
ui-settings-ssh-keep-alive-hint = How often Heimdall sends SSH protocol keep-alives on sessions, SFTP, tunnels and gateways, so an idle connection is not dropped by a firewall or the server. Applies to connections opened after the change.
ui-settings-ssh-keep-alive-refused = SSH keep-alive interval must be between { $min } and { $max } seconds.
ui-settings-ssh-tmout-reset-interval = TMOUT reset interval (0 = off)
ui-settings-ssh-tmout-reset-refused = SSH TMOUT reset interval must be between { $min } and { $max } seconds.
ui-settings-anti-idle-interval = Anti-idle interval (0 = off)
ui-settings-anti-idle-unit = s
ui-settings-anti-idle-refused = Anti-idle interval must be 0, or between { $min } and { $max } seconds.
ui-settings-session-log-directory = Session log directory:
ui-settings-session-log-directory-hint = Directory for session log files, relative to the settings folder unless absolute. Press Enter to apply.
ui-settings-browse-log-directory-title = Select Session Log Directory
ui-settings-session-log-retention = Delete transcripts older than
ui-settings-days-unit = days
ui-settings-minutes-unit = min
ui-settings-session-log-retention-hint = 0 keeps every transcript. Only session transcripts are deleted; the event and file-operation logs are kept.
ui-settings-session-log-retention-refused = Transcript retention must be 0 (keep all) or between { $min } and { $max } days.

## Broadcast input, as the C# one.
ui-broadcast-button = BROADCAST
ui-broadcast-toggle-tooltip = Toggle Broadcast Mode (send to all terminals), Ctrl+Alt+B
ui-broadcast-on = Broadcast mode ON - { $scope }
ui-broadcast-off = Broadcast mode OFF
ui-broadcast-scope-current = Current tab
ui-broadcast-scope-all = All tabs
ui-broadcast-scope-selected = Selected tabs ({ $count })
ui-broadcast-scope-status = Broadcast scope: { $scope }
ui-broadcast-scope-tooltip = Broadcast scope (click to switch between the current tab, all tabs and the tabs marked)
ui-broadcast-target-on = ◉
ui-broadcast-target-off = ○
ui-broadcast-target-tooltip = Send broadcast input to this session (broadcast target)
ui-dialog-broadcast-title = Broadcast to all tabs?
ui-dialog-broadcast-body = Input you type will be sent to terminal panes in every open tab, including tabs running in the background. Continue?
ui-dialog-broadcast-confirm = Broadcast
ui-dialog-session-logging-title = Record session transcripts?
ui-dialog-session-logging-body = Every terminal session will be written to a file: what you type as well as what is shown, including passwords or tokens echoed to the terminal. Turn it on?
ui-dialog-session-logging-confirm = Turn on

## The Files tab's path bar, as the C# one.
ui-files-go-tooltip = Navigate to path

## The Files tab's columns, as the C# ones.
ui-files-column-name = Name
ui-files-column-size = Size
ui-files-column-modified = Modified
ui-files-column-permissions = Permissions
ui-files-column-owner = Owner
ui-files-sorted-ascending = { $column } ▲
ui-files-sorted-descending = { $column } ▼

## The menu of an entry of the Files tab, as the C# one.
ui-files-menu-open = Open
ui-files-menu-download = Download
ui-files-menu-upload = Upload
ui-files-menu-rename = Rename
ui-files-menu-delete = Delete
ui-files-menu-copy-path = Copy path
ui-files-menu-new-folder = New Folder
ui-files-menu-refresh = Refresh

## Permissions and properties of a server's entry, as the C# Files tab.
ui-files-menu-permissions = Change permissions...
ui-files-menu-properties = Properties
ui-dialog-permissions-title = Change Permissions
ui-dialog-permissions-label = Permissions (octal, e.g. 755):
ui-dialog-permissions-placeholder = 755
ui-dialog-permissions-confirm = Apply
ui-files-error-invalid-permissions = Permissions are one to four octal digits, such as 755 or 4755.
ui-files-properties-title = Properties - { $name }
ui-files-properties-name = Name:
ui-files-properties-type = Type:
ui-files-properties-size = Size:
ui-files-properties-modified = Modified:
ui-files-properties-permissions = Permissions:
ui-files-properties-owner = Owner:
ui-files-properties-group = Group:
ui-files-properties-path = Path:
ui-files-properties-created = Created:
ui-files-properties-accessed = Accessed:
ui-files-properties-attributes = Attributes:
ui-files-properties-link-target = Link target:
ui-files-attribute-read-only = Read-only
ui-files-attribute-hidden = Hidden
ui-files-attribute-normal = Normal
ui-files-type-file = File
ui-files-type-directory = Directory
ui-files-type-link = Symbolic link
ui-files-type-other = Unknown type

## Several entries of the Files tab selected together, as the C# tab.
ui-files-selected-count = { $count } selected
ui-dialog-delete-many-body = Delete { $count } items? Folders are deleted with everything they contain. This cannot be undone.

## Bookmarks of the server's folders, as the C# Files tab.
ui-files-bookmark-button = Bookmark this path
ui-files-bookmarks-empty = No bookmarks saved
ui-files-bookmark-added = Bookmark added: { $path }

## Narrowing a Files pane down, as the C# tab.
ui-files-filter-placeholder = Filter files...
ui-files-hidden-toggle = .*
ui-files-hidden-tooltip = Show hidden files
ui-files-local-toggle = Local files
ui-files-local-toggle-tooltip = Show this computer's files beside the server's
ui-files-sudo-toggle = sudo
ui-files-sudo-tooltip = Browse as root (sudo)
ui-files-follow-toggle = cwd
ui-files-follow-tooltip = Follow the SSH terminal directory
ui-files-follow-local-tooltip = Follow the terminal directory
ui-files-sudo-on = Sudo mode enabled - browsing as root
ui-files-sudo-off = Sudo mode disabled
ui-files-item-count = { $count } items
ui-files-item-count-filtered = { $shown }/{ $count } items

## Files dropped from Explorer on a Files tab, as the C# one.
ui-files-drop-overlay = Drop files to upload

## The keys trusted for servers, on the Settings page, as the C# Host keys and Certificates pages.
ui-trusted-host-keys-title = Trusted host keys
ui-trusted-host-keys-hint = Review the SSH host keys Heimdall trusts for future connections.
ui-trusted-host-keys-search = Search trusted hosts
ui-trusted-host-keys-host = Host:Port
ui-trusted-host-keys-algorithm = Algorithm
ui-trusted-host-keys-fingerprint = Fingerprint
ui-trusted-host-keys-source = Source
ui-trusted-host-keys-first-seen = First seen
ui-trusted-host-keys-last-seen = Last seen
ui-trusted-host-keys-source-user = User confirmed
ui-trusted-host-keys-source-imported = Imported known_hosts
ui-trusted-host-keys-source-unknown = Unknown
ui-trusted-host-keys-date-unknown = Unknown
ui-trusted-host-keys-details = Details
ui-trusted-host-keys-copy = Copy fingerprint
ui-trusted-host-keys-remove = Remove
ui-trusted-host-keys-empty-title = No trusted host keys
ui-trusted-host-keys-empty-body = Connect to a server first: its key is asked about, then listed here.
ui-trusted-host-key-details-title = Trusted host key details
ui-trusted-host-key-details-public-key = Public key blob
ui-trusted-host-key-details-public-key-unavailable = (not available - reconnect to capture)
ui-trusted-host-key-details-close = Close
ui-trusted-certificates-title = Trusted RDP certificates
ui-trusted-certificates-hint = Certificates you accepted for a remote desktop, kept across restarts. Forgetting one removes that certificate from the trust list of its server.
ui-trusted-certificates-search = Search by server or fingerprint
ui-trusted-certificates-server = Server
ui-trusted-certificates-fingerprint = Fingerprint
ui-trusted-certificates-thumbprint = Certificate: { $thumbprint }
ui-trusted-certificates-subject = Subject
ui-trusted-certificates-issuer = Issuer
ui-trusted-certificates-trusted = Trusted since
ui-trusted-certificates-forget = Forget
ui-trusted-certificates-forget-server = Forget server
ui-trusted-certificates-empty-title = No trusted RDP certificates
ui-trusted-certificates-empty-body = Certificates you accept when connecting to a remote desktop are listed here, and can be revoked from here.
ui-trusted-ftps-certificates-title = Trusted FTPS certificates
ui-trusted-ftps-certificates-hint = Certificates you accepted for an FTPS server, kept across restarts. Forgetting one removes that certificate from the trust list of its server.
ui-trusted-ftps-certificates-empty-title = No trusted FTPS certificates
ui-trusted-ftps-certificates-empty-body = Certificates you accept when connecting to an FTPS server are listed here, and can be revoked from here.
ui-trusted-vnc-certificates-title = Trusted VNC certificates
ui-trusted-vnc-certificates-hint = Certificates you accepted for a VNC server encrypting with TLS, kept across restarts. While one is trusted, its server must keep using TLS. Forgetting one removes that certificate from the trust list of its server.
ui-trusted-vnc-certificates-empty-title = No trusted VNC certificates
ui-trusted-vnc-certificates-empty-body = Certificates you accept when connecting to a VNC server over TLS are listed here, and can be revoked from here.
ui-trusted-keys-unreadable = The trusted keys could not all be read: { $detail }
ui-dialog-forget-host-key-title = Remove trusted host key
ui-dialog-forget-host-key-body = Remove the trusted host key for { $server }?
ui-dialog-forget-host-key-fingerprint = Fingerprint: { $fingerprint }
ui-dialog-forget-host-key-consequence = Removing this trusted host key will require re-verification on the next connection to { $server }.
ui-dialog-forget-host-key-confirm = Remove
ui-dialog-forget-certificate-title = Forget this certificate?
ui-dialog-forget-certificate-body = Heimdall will forget the certificate { $fingerprint } for { $server }. Only that certificate is affected; any other certificate trusted for the same server stays trusted.
ui-dialog-forget-certificate-keep = Keep
ui-dialog-forget-certificate-confirm = Forget
ui-dialog-forget-server-certificates-title = Forget this server's certificates?
ui-dialog-forget-server-certificates-body = { $count ->
    [one] Heimdall will forget the { $count } certificate trusted for { $server }. The next connection to it asks again.
   *[other] Heimdall will forget the { $count } certificates trusted for { $server }. The next connection to it asks again.
}
ui-status-fingerprint-copied = Copied full fingerprint for { $server }.
ui-status-host-key-removed = Removed trusted host key for { $server }.
ui-status-certificate-forgotten = Certificate forgotten for { $server }.
ui-status-server-certificates-forgotten = Every certificate forgotten for { $server }.

## Tunnels opened by hand, as the C# "New tunnel" dialog and tunnels panel say them.
ui-tunnel-new-title = New tunnel
ui-tunnel-new-description = Create a session-scoped local port forward through one of your configured SSH gateways.
ui-tunnel-gateway-label = Gateway
ui-tunnel-remote-host-label = Remote host
ui-tunnel-remote-port-label = Remote port
ui-tunnel-local-port-label = Local port
ui-tunnel-label-label = Label (optional)
ui-tunnel-open-button = Open tunnel
ui-tunnel-no-gateways = No SSH gateway configured. Add one in Settings before creating a tunnel.
ui-tunnel-problem-gateway = Gateway is required.
ui-tunnel-problem-remote-host = Remote host is required.
ui-tunnel-problem-remote-port = Remote port must be between { $min } and { $max }.
ui-tunnel-problem-local-port = Local port must be between { $min } and { $max }.
ui-tunnel-problem-local-port-in-use = Local port { $port } is already in use by an active tunnel.
ui-tunnel-opened = Tunnel opened on local port { $port } → { $host }:{ $remote }.
ui-tunnel-failed = Tunnel creation failed: { $reason }
ui-tunnel-closed = Tunnel on port { $port } closed.
ui-tunnels-all-closed = All tunnels closed.
ui-tunnel-port-copied = Port { $port } copied to clipboard.
ui-tunnel-closed-reason = { $closed } ({ $reason })
ui-error-local-port-unavailable = Local port { $port } is already in use, or reserved by the system.

## The tunnels panel and its status-bar button, as the C# ones.
ui-tunnels-header = Tunnels ({ $count })
ui-tunnels-close-all = Close All
ui-tunnels-new = + New
ui-tunnels-collapse-tooltip = Collapse tunnel panel
ui-tunnels-toggle-tooltip = Toggle tunnel panel
ui-tunnels-column-gateway = Gateway
ui-tunnels-column-label = Label
ui-tunnels-column-local = Local
ui-tunnels-column-remote = Remote
ui-tunnels-column-port = Port
ui-tunnels-close-tooltip = Close tunnel
ui-tunnels-menu-close = Close Tunnel
ui-tunnels-menu-copy-port = Copy Local Port
ui-tunnels-menu-close-all = Close All Tunnels
ui-tunnels-empty = No active tunnels
ui-tunnels-count =
    { $count ->
        [one] { $count } tunnel
       *[other] { $count } tunnels
    }
ui-tunnels-collapse-button = ▼

## An SSH key file not there, and what the agents offered a gateway that refused, as the C# says them.
ui-error-key-not-found = SSH key file not found: { $path }
ui-error-auth-agent-none = No key was loaded in an SSH agent when Heimdall dialled this gateway, so no agent key was offered. If this gateway signs in with an agent key, load it in Pageant or in the Windows OpenSSH Agent and connect again; otherwise check the sign-in details saved for this gateway.
ui-error-auth-agent-one = One key was loaded in an SSH agent and offered to this gateway; it was not accepted. If this gateway expects a different key, load that one and connect again.
ui-error-auth-agent-many = { $count } keys were loaded in an SSH agent and offered to this gateway; none of them was accepted. If this gateway expects a different key, load that one and connect again.
ui-error-auth-with-agent = { $refused } { $agent }

## "Test address" in the profile form, as the C# dialog says it.
ui-address-test-button = Test address
ui-address-test-hint = Checks that the address and port answer. Does not check your username or password.
ui-address-test-running = Testing the address...
ui-address-test-success = Address answers: { $address } ({ $millis } ms). Credentials were not checked.
ui-address-test-success-ssh = Address answers and an SSH server replied: { $banner }. Credentials were not checked.
ui-address-test-failure = The address did not answer: { $reason }
ui-address-test-direct-scope = Tested directly from this computer, not through { $gateway }.
ui-address-test-cancel = Cancel
ui-address-test-dns-timeout = DNS lookup timed out.
ui-address-test-dns-failed = DNS lookup failed: { $reason }
ui-address-test-dns-no-results = DNS lookup returned no addresses.
ui-address-test-tcp-timeout = TCP connect timed out (host: { $address }). The host may be off, unreachable, or the port may be blocked.
ui-address-test-tcp-failed = Could not connect to { $address }: { $reason }
ui-address-test-cancelled = Test cancelled.
ui-address-test-scoped = { $verdict } { $scope }

## The gateway dialog's "Test route", as the C# card.
ui-route-test-title = Test and understand this route
ui-route-test-workstation = This workstation
ui-route-test-target-host = Optional destination host (leave empty to test gateways only)
ui-route-test-target-port = Destination TCP port
ui-route-test-test = Test route
ui-route-test-stop = Stop test
ui-route-test-copy = Copy diagnostic report
ui-route-test-running = Testing the route. Results appear after each step.
ui-route-test-hop-step = Gateway { $number }: SSH connection and authentication
ui-route-test-target-step = Destination TCP access
ui-route-test-passed = Passed
ui-route-test-trust-required = Not tested: no trusted host key. Verify and register it in SSH trusted keys first.
ui-route-test-trust-changed = Host key differs. Verify the server identity before updating SSH trusted keys.
ui-route-test-network = Connection unavailable. Check the host, port, VPN and firewall.
ui-route-test-forwarding = Destination access was not confirmed. Check the address, port and TCP forwarding permissions on the gateway.
ui-route-test-cancelled = Cancelled.
ui-route-test-interactive = Interactive authentication is required. Use an interactive SSH connection to investigate the required method.
ui-route-test-auth = Authentication failed. Check the account, key, passphrase and SSH agent.
ui-route-test-unavailable = Diagnostic unavailable or unclassified failure. Check configuration and authentication prerequisites.
ui-route-test-invalid-route = Invalid route: check missing parents, cycles and the maximum chain depth.
ui-route-test-invalid-target = Enter a valid destination hostname or IP address and a TCP port from 1 to 65535.
ui-route-test-report-header = Heimdall SSH route diagnostic (anonymized: no hostnames, accounts, key paths or raw errors)
ui-route-test-tcp-only = The destination step checks TCP access only. It does not validate the application protocol or a login on the destination.
ui-route-test-no-target = No destination specified. Only gateway SSH connections were tested.
ui-route-test-hop = { $name } ({ $host }:{ $port })
ui-route-test-step-line = { $step }: { $outcome } ({ $millis } ms)
ui-route-test-hint = Tests the current form without saving it. Uses trusted host keys only. Existing sessions stay open.
ui-route-test-timeout = Timed out. Check VPN, routing and firewall.
ui-route-test-separator = {" "}→{" "}

## "Test reachability" in a profile's menu, as the C# tree says it in the status bar.
ui-tree-test-reachability = Test reachability
ui-status-reachability-testing = Testing { $host }:{ $port } ...
ui-status-reachability-success = { $host }:{ $port } reachable in { $millis } ms
ui-status-reachability-failed = { $host }:{ $port } unreachable: { $reason }

## An RDP tab's "Resolution" menu, as the C# one.
ui-resolution-menu = Resolution
ui-resolution-active-mode = Active mode
ui-resolution-header = { $label }: { $mode }
ui-resolution-header-size = { $label }: { $mode } ({ $width }x{ $height })
ui-resolution-mode-fit-window = Fit window
ui-resolution-mode-fixed = Fixed
ui-resolution-match-window = Match window
ui-resolution-custom = Custom...
ui-resolution-skip-stabilization = Skip stabilization
ui-resolution-custom-title = Custom resolution
ui-resolution-custom-prompt = Enter resolution as WIDTHxHEIGHT.
ui-resolution-custom-invalid = Invalid resolution. Use WIDTHxHEIGHT.
ui-resolution-save-default = Save as default for this server
ui-resolution-save-default-done = RDP resolution default saved for this server.
ui-resolution-save-default-unavailable = Cannot save a default resolution for this session.

## The SSH agent chip of the profile form, as the C# one.
ui-agent-chip-off = No SSH agent detected
ui-agent-chip-warn = SSH agent: { $agent } (no keys loaded)
ui-agent-chip-ok = SSH agent: { $agent } ({ $count } keys)
ui-agent-chip-tooltip = Click to re-scan SSH agents
ui-files-menu-cut = Cut
ui-files-menu-paste = Paste
ui-status-files-cut = { $count ->
    [one] { $count } item cut
   *[other] { $count } items cut
}
ui-status-files-pasted = Paste complete
ui-status-path-copied = Path copied: { $path }
ui-files-menu-copy = Copy
ui-files-menu-duplicate = Duplicate
ui-status-files-copied = { $count ->
    [one] { $count } item copied
   *[other] { $count } items copied
}
ui-status-files-duplicated = Duplicate complete
ui-files-error-copy-refused = Copy refused: this server did not perform the server-side copy, and Heimdall will not fall back to a transfer that could overwrite an existing destination. Copy the file locally, or check that the server allows running cp, ln and mkdir.
ui-files-error-paste-into-itself = Cannot paste { $name } into itself or its own subfolder.
ui-files-error-paste-link = Cannot paste { $name }: links and junctions are not copied through. Paste what they point at instead.
ui-error-key-not-absolute = SSH key path must be absolute: { $path }
ui-files-error-changed-on-server = The file changed on the server since it was opened: it was left as it is.
ui-files-error-changed-since-confirmed = It changed on the server since the deletion was confirmed: it was left as it is.
ui-files-error-file-too-large = Files larger than 16 MiB must be downloaded instead.
ui-files-menu-open-in-terminal = Open in terminal
ui-files-menu-open-in-explorer = Open in Explorer
ui-files-menu-run-in-shell = Run in Shell
ui-files-menu-open-with = Open With...
ui-files-menu-open-in-editor = Open in Editor
ui-status-resolution-reconnected = Resolution change required reconnect.
ui-status-stabilization-skipped = Stabilization skipped - dynamic resolution is now active.
ui-resolution-mode-smart-sizing = Smart sizing
ui-resolution-tooltip = Change resolution - { $mode }
ui-resolution-tooltip-size = Change resolution - { $mode } ({ $width }x{ $height })
ui-resolution-larger-than-window = Larger than window - image will be scaled.
ui-files-menu-upload-here = Upload here...
ui-files-menu-edit-external = Edit with external editor
ui-status-files-editing = Editing: { $name } - save in your editor to auto-upload
ui-status-files-auto-uploaded = Auto-uploaded: { $name }
ui-status-files-auto-upload-refused = Auto-upload of { $name } was refused and will not be retried until you save again: { $reason }
ui-files-error-working-folder-unprotected = The file was not opened: its local working folder could not be restricted to your account, so its content could have been readable by other users of this computer.
ui-files-error-editor-failed = The external editor could not be started: { $detail }. Check the editor path in Settings.
ui-files-error-editor-runs-files = Launching shell interpreters or script hosts as editors is blocked for security reasons.
ui-files-error-open-failed = Could not open it on this computer: { $detail }
ui-files-error-script-character = This script was not run: its path holds { $character }, which its interpreter would read as more than part of the path. Rename the file or its folder to run it.
ui-files-error-script-not-text = This script was not run: its path is not valid text, which its interpreter could not be handed as it is.
ui-settings-external-editor = External editor
ui-settings-external-editor-path = External editor path
ui-settings-external-editor-hint = Path to text editor for SFTP remote editing (leave empty for system default)
ui-settings-browse-editor-title = Select External Editor
ui-settings-putty-path = PuTTY path (for External SSH mode)
ui-settings-putty-path-hint = Required when SSH mode is set to External. Looked for in the folders of PATH if left blank.
ui-settings-browse-putty-title = Select putty.exe or puttycac.exe
ui-settings-browse-executables = Executables
ui-settings-browse-all-files = All files
ui-settings-ssh-default-mode = Default SSH mode
ui-settings-ssh-default-mode-embedded = Embedded
ui-settings-ssh-default-mode-external = External
ui-settings-ssh-default-mode-hint = Embedded: terminal runs inside Heimdall. External: opens PuTTY in a separate window.
ui-settings-apply-mode-to-all = Apply to all saved sessions
ui-settings-apply-mode-to-all-tooltip = Saves this mode as the default and rewrites it into every saved session of this protocol
ui-settings-x11 = X11 server
ui-settings-x11-server-path = X11 server path
ui-settings-x11-server-path-hint = Leave blank to try VcXsrv, Xming and Cygwin/X where they install, then the folders of PATH.
ui-settings-browse-x11-title = Select X11 Server
ui-settings-x11-auto-start = Auto-start X11 server when needed
ui-dialog-close-edits-body = "{ $name }" has a file open in an external editor. Close the pane anyway? The editor stays open, but nothing will send its next save to the server.
ui-files-edits-title = Edited in an external editor
ui-files-edit-watching = Each save in the editor is sent to the server.
ui-files-edit-send-anyway = Send my version
ui-files-edit-open-folder = Open folder
ui-files-edit-stop = Stop editing
ui-files-error-sudo-password-needed = sudo asks for a password on this server.
ui-files-error-sudo-password-rejected = sudo rejected the password.
ui-files-error-sudo-needs-terminal = sudo on this server requires a terminal (requiretty): Heimdall does not give it one, so nothing ran as root. Allow sudo without a terminal for your account, or edit the file in a shell.
ui-files-error-sudo-untrusted = The sudo found on the server is not the system's (not set-user-id root): nothing ran as root.
ui-files-error-sudo-tooling = Privileged transfer refused: the server is missing a tool it needs (GNU coreutils: stat, cp, sync, mv). See the log for the tool named.
ui-files-error-sudo-failed = Sudo authentication failed.
ui-files-error-sudo-protected = Not deleted as root: "/", a top-level system folder, a home folder itself, or a path that is not absolute or goes up with "..".
ui-files-menu-paste-explorer = Paste from Explorer
ui-status-explorer-no-files = No files are copied in Explorer.
ui-status-rdp-files-too-many = Files not copied to the server: one copy takes { $count } files and folders at most.
ui-status-rdp-files-too-large = Files not copied to the server: one copy takes { $size } at most.
ui-files-menu-edit-sudo = Edit with sudo
ui-files-edit-save-sudo = Save with sudo
ui-status-files-saved-sudo = Saved via sudo: { $name }
ui-dialog-sudo-title = sudo password
ui-dialog-sudo-body = sudo on this server asks for your password for "{ $name }". It is kept for this tab only, until the tab closes or sudo refuses it.
ui-dialog-sudo-delete-title = Delete as root?
ui-dialog-sudo-delete-body = These items will be deleted as root, a folder with all of its contents. Nothing undoes it.
ui-dialog-sudo-delete-more = and { $count } more
ui-dialog-sudo-delete-confirm = Delete as root
ui-desktop-save-files = Save copied files...
ui-desktop-save-files-tooltip = Save the files copied on the server into a folder of this computer
ui-desktop-saving-files = Saving files: { $saved } of { $total }
ui-desktop-save-files-cancel = Stop
ui-status-rdp-files-saved = Server's files saved: { $count }.
ui-status-rdp-files-save-failed = Server's files not all saved: { $saved } of { $total } saved before it failed.
ui-status-rdp-files-save-cancelled = Saving stopped: { $saved } of { $total } saved.
ui-status-rdp-files-not-saved-too-many = Server's files not saved: one copy takes { $count } files and folders at most.
ui-status-rdp-files-not-saved-too-large = Server's files not saved: one copy takes { $size } at most.
ui-status-rdp-files-not-saved-unknown-size = Server's files not saved: the server did not say their size.
ui-files-menu-edit-integrated = Edit
ui-editor-save = Save
ui-editor-close = Close
ui-editor-overwrite = Overwrite
ui-editor-opening = Opening { $name }...
ui-editor-position = Ln { $line }, Col { $column }
ui-editor-lines = { $count ->
    [one] { $count } line
   *[other] { $count } lines
}
ui-editor-plain-text = Plain text
ui-editor-encoding-utf8 = UTF-8
ui-editor-encoding-utf8-bom = UTF-8 with BOM
ui-editor-encoding-utf16le = UTF-16 LE
ui-editor-encoding-utf16be = UTF-16 BE
ui-editor-encoding-utf32le = UTF-32 LE
ui-editor-encoding-utf32be = UTF-32 BE
ui-editor-encoding-latin1 = Latin-1
ui-editor-ending-lf = LF
ui-editor-ending-crlf = CRLF
ui-editor-ending-cr = CR
ui-editor-notice-latin1 = This file is not valid UTF-8 and was opened as Latin-1. Saving writes it back as Latin-1.
ui-editor-notice-saved = Saved.
ui-editor-notice-changed = The file changed on the server since it was opened: not saved. Overwrite it, or close without saving.
ui-editor-notice-unencodable = Not saved: the character at line { $line }, column { $column } cannot be stored as Latin-1.
ui-editor-notice-save-running = The save is still running.
ui-editor-notice-session-ended = The session ended: your changes are kept. Reconnect to save them.
ui-editor-notice-failed = Failed to save: { $reason }
ui-dialog-discard-editor-title = Unsaved Changes
ui-dialog-discard-editor-body = File has unsaved changes. Close anyway?
ui-dialog-close-editor-body = The editor on "{ $name }" has unsaved changes. Close and discard them?
ui-dialog-unsaved-editors = { $count ->
    [one] { $count } editor holds unsaved changes, which would be lost.
   *[other] { $count } editors hold unsaved changes, which would be lost.
}
ui-files-error-looks-binary = This file looks like a binary file (an archive, an image or a program): download it instead.
ui-files-error-not-text = This file's encoding mark does not match its content: it cannot be opened as text.
ui-files-error-too-large-for-editor = Too large for the integrated editor (over { $size }): use Edit with external editor.
ui-dialog-binary-title = Binary file
ui-dialog-binary-body = "{ $name }" looks like a binary file (an archive, an image or a program) and cannot be shown as text. Download it instead?
ui-dialog-binary-confirm = Download
ui-dialog-open-link-title = Open link
ui-dialog-open-link-body = The text clicked leads to this address, which will open in your browser:
    { $url }
ui-dialog-open-link-confirm = Open
ui-dialog-open-runnable-title = Open a program
ui-dialog-open-runnable-body = This file runs as a program on this computer, with your rights. Open it only if you trust it:
    { $path }
ui-dialog-open-runnable-confirm = Open
ui-tab-menu-show-health = Show server health
ui-tab-menu-hide-health = Hide server health
ui-health-cpu = CPU
ui-health-memory = Memory
ui-health-disk = Disk
ui-health-waiting = Reading...
ui-health-unsupported = Unsupported
ui-health-cpu-value = { $percent }%
ui-health-memory-value = { $used } / { $total } MB
ui-health-disk-value = { $used } / { $total }
ui-find-count = { $index } / { $total }
ui-winrm-diagnostic-logon-failed = The remote host rejected the credentials. Check the user name and password (use the user@domain form for a domain account) and that the account is not locked.
ui-winrm-diagnostic-access-denied = Access denied by the remote host. The account needs the right to use WinRM: ask your administrator to add it to the Remote Management Users group on the host.
ui-winrm-diagnostic-trusted-hosts = WinRM refused the connection because the host is not trusted for this authentication. Use the host DNS name instead of its IP address, use HTTPS, or ask your administrator to add the host to the TrustedHosts list of this computer.
ui-winrm-diagnostic-kerberos-principal = Kerberos authentication failed for this host. Connect with the fully qualified DNS name of the host instead of its IP address or a short alias, and check that the host belongs to your domain.
ui-winrm-diagnostic-session-not-entered = The remote WinRM session could not be opened. Read the PowerShell message above for the cause, then check the host name, the port and the identity of this profile.
ui-profile-winrm-identity-hint = Kerberos or NTLM is negotiated automatically. Kerberos needs the host DNS name, not its IP address. Over HTTP the content is still encrypted by Kerberos or NTLM, but NTLM does not verify the server identity.
ui-profile-winrm-trusted-hosts-hint = Outside a domain, NTLM over HTTP requires the host to be in the TrustedHosts list of this computer, or use HTTPS. Heimdall never edits TrustedHosts.
ui-profile-winrm-https-off-by-gateway = HTTPS was turned off because an SSH gateway is selected: gateway-routed WinRM uses HTTP through the tunnel. Remove the gateway to restore HTTPS.
ui-error-winrm-tls-no-verify = The WinRM TLS connection to '{ $host }' on port { $port } failed even though certificate validation is skipped: the port probably does not answer in TLS. Check that port { $port } is the HTTPS WinRM listener (usually 5986).
ui-files-state-rate = { $progress } - { $rate }/s, { $left } left
ui-files-eta-seconds = { $seconds } s
ui-files-eta-minutes = { $minutes } min { $seconds } s
ui-files-eta-hours = { $hours } h { $minutes } min
ui-files-conflict-replace-if-newer = Replace if newer
ui-files-conflict-incoming = New: { $size }, modified { $modified }
ui-files-conflict-existing = Existing: { $size }, modified { $modified }
ui-files-conflict-newer = The new file is newer.
ui-files-conflict-older = The new file is older.
ui-files-conflict-same-time = Same modification time.
ui-files-conflict-unknown = unknown
ui-settings-ssh-agent-preference = SSH agent preference
ui-settings-ssh-agent-openssh-first = Auto: Windows OpenSSH first
ui-settings-ssh-agent-pageant-first = Auto: Pageant first
ui-settings-ssh-agent-openssh-only = Windows OpenSSH only
ui-settings-ssh-agent-pageant-only = Pageant only
ui-settings-ssh-agent-preference-hint = Controls which loaded SSH agent keys Heimdall tries first. Changes apply to the next connection.
ui-status-screenshot-copied = Screenshot copied to clipboard
ui-status-screenshot-failed = Failed to capture screenshot
ui-files-state-queued = Queued
ui-files-state-preparing = Preparing transfer...
ui-files-retry-button = Retry
ui-files-clear-finished-button = Clear finished
ui-files-error-interrupted = The transfer stopped unexpectedly.
ui-rdp-session-closed = The Remote Desktop session has ended.
ui-trusted-host-keys-export = Export known_hosts
ui-status-known-hosts-exported = { $count ->
    [one] Exported { $count } key to { $path }.
   *[other] Exported { $count } keys to { $path }.
}
ui-status-known-hosts-export-skipped = { $count ->
    [one] { $count } entry skipped (no public key captured - reconnect to enable export).
   *[other] { $count } entries skipped (no public key captured - reconnect to enable export).
}
ui-status-known-hosts-export-failed = known_hosts export failed: { $detail }
ui-status-known-hosts-export-no-home = known_hosts export failed: the home folder is not known.
ui-tree-favorite-add = Add to favorites
ui-tree-favorite-remove = Remove from favorites
ui-tree-favorite = Favorite
ui-tree-filter-favorites = Favorites
ui-profile-toggle-favorite = Mark as favorite
ui-status-favorite-save-failed = Could not save the favorite.
ui-selection-edit = Edit
ui-selection-edit-port = Port...
ui-selection-edit-username = Username... ({ $count })
ui-bulk-port-header = { $count ->
    [one] Editing port on { $count } item
   *[other] Editing port on { $count } items
}
ui-bulk-port-label = Port
ui-bulk-port-mixed = Mixed values
ui-bulk-port-invalid = Port must be between 1 and 65535.
ui-bulk-username-header = { $count ->
    [one] Editing username on { $count } server
   *[other] Editing username on { $count } servers
}
ui-bulk-username-label = Username:
ui-bulk-username-mixed = mixed values
ui-bulk-username-invalid = Username cannot be empty and cannot contain control characters (including line breaks and tabs).
ui-status-bulk-port-updated = { $count ->
    [one] Updated port on { $count } item.
   *[other] Updated port on { $count } items.
}
ui-status-bulk-port-unchanged = No port changes were applied.
ui-status-bulk-username-updated = { $count ->
    [one] Username updated on { $count } server.
   *[other] Username updated on { $count } servers.
}
ui-status-bulk-username-unchanged = No changes applied - every selected server already uses this username.
ui-selection-edit-password = Password... ({ $count })
ui-bulk-password-header = { $count ->
    [one] Setting password on { $count } server
   *[other] Setting password on { $count } servers
}
ui-bulk-password-label = New password:
ui-bulk-password-confirm-label = Confirm password:
ui-bulk-password-control = Password cannot contain control characters.
ui-bulk-password-mismatch = Passwords do not match.
ui-bulk-password-skipped-winrm = { $count ->
    [one] Skipped { $count } WinRM profile because no username is configured.
   *[other] Skipped { $count } WinRM profiles because no username is configured.
}
ui-bulk-password-skipped-no-account = { $count ->
    [one] Skipped { $count } profile because no username is configured.
   *[other] Skipped { $count } profiles because no username is configured.
}
ui-bulk-password-skipped-other = { $count ->
    [one] Skipped { $count } profile whose protocol has no saved password.
   *[other] Skipped { $count } profiles whose protocol has no saved password.
}
ui-status-bulk-password-updated = { $count ->
    [one] Password updated on { $count } server.
   *[other] Password updated on { $count } servers.
}
ui-status-bulk-password-updated-with-skipped = { $updated } { $skipped }
ui-status-bulk-password-partial = Password saved on { $count } of { $total } servers.
ui-desktop-disconnect = Disconnect
ui-desktop-disconnect-tooltip = Disconnect session
ui-desktop-disconnect-title = Disconnect Remote Desktop?
ui-desktop-disconnect-body = You are about to disconnect from { $name }. Continue?
ui-settings-rdp-auto-reconnect-attempts = Auto-reconnect maximum attempts
ui-files-type-pipe = Named pipe (FIFO)
ui-files-type-socket = Socket
ui-files-type-device = Device
ui-files-bookmark-removed = Bookmark removed: { $path }
ui-files-bookmark-remove-menu = Remove a bookmark
ui-files-empty-no-match = No entries match "{ $filter }".
ui-files-empty-clear-filter = Clear filter
ui-files-empty-hidden-only = This folder only contains hidden entries.
ui-files-empty-show-hidden = Show hidden files
ui-tunnels-status-active = Active
ui-tunnels-status-interrupted = Interrupted
ui-tunnels-menu-reopen = Reopen
ui-settings-behavior = Behavior
ui-settings-collapse-tunnels-panel = Collapse Tunnels panel by default
ui-settings-collapse-tunnels-panel-hint = How the Tunnels panel starts. Opening or closing it later keeps it that way until the application closes.
ui-settings-prevent-sleep = Prevent system sleep during active sessions
ui-settings-prevent-sleep-hint = Keeps Windows from sleeping while a session is connected; the screen may still turn off or lock.
ui-settings-max-sessions = Max embedded sessions
ui-settings-max-sessions-none = No limit
ui-profile-session-logging = Session logging
ui-profile-session-logging-inherit = Inherit
ui-profile-session-logging-on = On
ui-profile-session-logging-off = Off
ui-profile-session-logging-hint = Inherit follows the global session logging setting.
ui-hostkey-copy-fingerprint-button = Copy
ui-certificate-already-trusted = { $count ->
    [one] This profile already trusts { $count } other certificate for this name, which usually means more than one machine answers to it.
   *[other] This profile already trusts { $count } other certificates for this name, which usually means several machines answer to it.
}
ui-certificate-route = Reached through: { $route }
ui-certificate-owner-tab = This question belongs to the tab "{ $tab }".
ui-settings-rdp-resolution-presets = Resolution presets
ui-settings-rdp-resolution-presets-hint = One preset per line, format WIDTHxHEIGHT (e.g. 1920x1080). Leave the box empty to use the built-in list.
ui-settings-rdp-resolution-presets-reset = Reset to defaults
ui-settings-rdp-resolution-presets-invalid = Resolution presets: these lines are not WIDTHxHEIGHT with a width from { $min } to { $width } and a height from { $min } to { $height } pixels: { $lines }
ui-settings-rdp-reset-defaults = Reset RDP defaults
ui-settings-rdp-reset-defaults-tooltip = Reverts only the RDP defaults to their factory values. Other settings are untouched.
ui-dialog-reset-rdp-title = Reset RDP defaults?
ui-dialog-reset-rdp-body = Restore all RDP-related defaults to their factory values? Existing servers are not affected.
ui-settings-reset-all = Reset defaults
ui-dialog-reset-all-title = Reset all settings?
ui-dialog-reset-all-body = Restore every Settings tab to factory defaults? Text typed and not applied yet is discarded. On the Security tab this turns off the external credential provider, the Credential Guard requirement and the Windows Hello requirement on connect, and resets the Windows Hello grace period, the auto-lock delay and disconnect on lock. Your language, theme, sessions, SSH gateways, master password, PIN and Windows Hello enrolment are kept. The change is saved at once.
ui-dialog-apply-ssh-mode-title = Apply to all saved SSH sessions?
ui-dialog-apply-ssh-mode-body = { $changes } of { $total } saved SSH sessions will switch to the { $mode } mode, and { $mode } becomes the default for new sessions. This cannot be undone.
ui-dialog-apply-rdp-mode-title = Apply to all saved RDP sessions?
ui-dialog-apply-rdp-mode-body = { $changes } of { $total } saved RDP sessions will switch to the { $mode } mode, and { $mode } becomes the default for new sessions. This cannot be undone.
ui-settings-tab-about = About
ui-about-version = Version { $version }
ui-about-tagline = Secure RDP/SSH/SFTP connection manager
ui-about-section-system = System
ui-about-platform = Platform
ui-about-iced = iced
ui-about-russh = russh
ui-about-ironrdp = IronRDP
ui-about-build-date = Build date
ui-about-author = Author
ui-about-license = License
ui-about-section-data = Data
ui-about-sessions = Sessions
ui-about-gateways = Gateways
ui-about-config-path = Config
ui-about-log-path = Logs
ui-about-section-links = Quick access
ui-about-open-config = Open config folder
ui-about-open-logs = Open logs folder
ui-about-repository = GitHub
ui-about-section-diagnostics = Diagnostics
ui-about-diagnostics-log = Write the application diagnostics log (Heimdall's own events and errors)
ui-about-diagnostics-log-hint = Applied at once. A crash report is written whatever this says: it is the only trace of a crash.
ui-settings-provider-timeout = Command timeout
ui-settings-provider-timeout-seconds = { $seconds } s
ui-settings-provider-timeout-hint = How long the password command may run before Heimdall gives up on it. Raise it for a vault that asks for confirmation.
ui-folder-color = Colour
ui-folder-color-none = No colour
ui-folder-color-blue = Blue
ui-folder-color-green = Green
ui-folder-color-red = Red
ui-folder-color-amber = Amber
ui-folder-color-purple = Purple
ui-folder-color-pink = Pink
ui-folder-color-cyan = Cyan
ui-folder-color-orange = Orange
ui-shortcuts-title = Keyboard Shortcuts
ui-shortcuts-hint = F1 for shortcuts
ui-shortcuts-session-keys = In a terminal or a remote desktop, F1 and the keys the session uses go to the server: open this list from the status bar.
ui-shortcuts-group-sessions = Sessions
ui-shortcuts-group-tabs = Tabs
ui-shortcuts-group-terminal = Terminal
ui-shortcuts-group-files = Files
ui-shortcuts-group-window = Window
ui-shortcuts-new-session = Add new session
ui-shortcuts-edit-session = Edit selected session
ui-shortcuts-quick-connect = Quick Connect
ui-shortcuts-search = Search sessions, or filter the files list
ui-shortcuts-next-tab = Next tab
ui-shortcuts-previous-tab = Previous tab
ui-shortcuts-close-tab = Close current tab
ui-shortcuts-toggle-split = Toggle split orientation
ui-shortcuts-next-pane = Next pane of the split tab
ui-shortcuts-previous-pane = Previous pane of the split tab
ui-shortcuts-find = Find in the terminal
ui-shortcuts-text-size = Larger, smaller, original text size
ui-shortcuts-broadcast = Broadcast input on or off
ui-shortcuts-terminal-copy = Copy the selection
ui-shortcuts-terminal-paste = Paste
ui-shortcuts-scroll-history = Scroll the history a page
ui-shortcuts-files-copy-cut = Copy, cut the selected entries
ui-shortcuts-files-paste = Paste entries, or files copied in Explorer
ui-shortcuts-files-select-all = Select all
ui-shortcuts-files-copy-path = Copy the full path
ui-shortcuts-files-download-upload = Download, upload the selection
ui-shortcuts-files-rename = Rename
ui-shortcuts-files-new-folder = New folder
ui-shortcuts-files-delete = Delete
ui-shortcuts-files-refresh = Refresh
ui-shortcuts-files-back = Back, parent folder
ui-shortcuts-files-path = Type a path
ui-shortcuts-files-switch-pane = Other pane
ui-shortcuts-full-screen = Toggle fullscreen
ui-shortcuts-settings = Open settings
ui-shortcuts-screenshot = Capture screenshot
ui-shortcuts-copy-status = Copy the status and its recent messages, for a screen reader
ui-shortcuts-lock = Lock
ui-shortcuts-help = Show this help
ui-shortcuts-close = Close a dialog or a menu; exit fullscreen outside a terminal or a remote desktop
ui-resolution-match-aspect = Match window, { $wide }:{ $high }
ui-tab-menu-pin = Pin tab
ui-tab-menu-unpin = Unpin tab
ui-tab-menu-save-as-profile = Save as profile...
ui-tab-menu-reveal-in-tree = Reveal in tree
ui-tab-pinned-badge = pinned
ui-selection-set-gateway = Set gateway... ({ $count })
ui-selection-gateway-direct = Direct connection (no gateway)
ui-status-bulk-gateway-updated = { $count ->
    [one] Connection route updated on { $count } server.
   *[other] Connection route updated on { $count } servers.
}
ui-status-bulk-gateway-unchanged = No route changes were applied.
ui-shortcuts-select-all-sessions = Select every session shown
ui-shortcuts-session-menu = The selected session's menu
ui-shortcuts-find-by-name = Go to the session whose name starts with what is typed
ui-shortcuts-toggle-sidebar = Show or hide the sidebar
ui-shortcuts-toggle-tools-panel = Toggle tools panel
ui-restore-title = Restore previous sessions
ui-restore-message = Heimdall found a saved session snapshot from the previous run. Select the sessions to restore.
ui-restore-saved-at = Saved at { $time }
ui-restore-select-all = Select all
ui-restore-missing = Missing server ({ $id })
ui-restore-files = { $protocol } files
ui-restore-dont = Don't restore
ui-restore-selected = Restore selected
ui-profile-field-environment = Environment
ui-profile-environment-none = (None)
ui-profile-environment-production = Production
ui-profile-environment-staging = Staging
ui-profile-environment-lab = Lab
ui-profile-environment-personal = Personal
ui-profile-field-tags = Tags
ui-profile-tags-placeholder = Words the search finds this session by
ui-profile-field-mac-address = MAC address
ui-profile-mac-address-placeholder = AA:BB:CC:DD:EE:FF, for Wake on LAN
ui-profile-error-mac-address = The MAC address must be twelve hexadecimal digits, as AA:BB:CC:DD:EE:FF.
ui-tree-wake-on-lan = Wake on LAN
ui-status-wake-on-lan-sent = Wake-on-LAN magic packet sent.
ui-status-wake-on-lan-failed = Failed to send Wake-on-LAN packet: { $reason }
ui-tree-tooltip-environment = Environment: { $environment }
ui-tree-tooltip-tags = Tags: { $tags }
ui-files-batch-deleting = Deleting { $name } ({ $index }/{ $total })...
ui-files-batch-permissions = Changing the permissions of { $name } ({ $index }/{ $total })...
ui-files-batch-stop = Cancel
ui-files-batch-stopping = Stopping after this one...
ui-status-files-delete-failed = Could not delete "{ $name }": { $reason }
ui-status-files-permissions-failed = Could not change the permissions of "{ $name }": { $reason }
ui-status-files-delete-partial = { $failed ->
    [one] { $failed } item out of { $total } could not be deleted. "{ $name }": { $reason }
   *[other] { $failed } items out of { $total } could not be deleted. First, "{ $name }": { $reason }
}
ui-status-files-permissions-partial = { $failed ->
    [one] Permissions could not be changed on { $failed } item out of { $total }. "{ $name }": { $reason }
   *[other] Permissions could not be changed on { $failed } items out of { $total }. First, "{ $name }": { $reason }
}
ui-status-files-delete-stopped = Deletion cancelled: { $done } of { $total } items deleted.
ui-status-files-permissions-stopped = Permission change cancelled: { $done } of { $total } items changed.
ui-macros-menu = Macros
ui-macros-record = Record macro
ui-macros-stop-recording = { $count ->
    [one] Stop recording ({ $count } input)
   *[other] Stop recording ({ $count } inputs)
}
ui-macros-stop = Stop "{ $name }"
ui-macros-play = Play "{ $name }"
ui-macros-none = No macros recorded yet
ui-macros-empty = No macros yet. Record one from a terminal tab's menu: Macros, Record macro.
ui-macros-inputs = { $count ->
    [one] { $count } input
   *[other] { $count } inputs
}
ui-macros-delete = Delete
ui-tab-recording-badge = REC
ui-tab-macro-badge = macro
ui-dialog-save-macro-title = Save macro
ui-dialog-save-macro-prompt = { $count ->
    [one] { $count } input recorded. Name the macro:
   *[other] { $count } inputs recorded. Name the macro:
}
ui-dialog-save-macro-confirm = Save
ui-status-macro-nothing = Nothing was typed: no macro was recorded.
ui-status-macro-saved = Macro "{ $name }" saved.
ui-status-macro-deleted = Macro "{ $name }" deleted.
ui-status-macro-completed = Macro "{ $name }" played.
ui-status-macro-stopped = Macro "{ $name }" stopped.
ui-status-macro-timed-out = Macro "{ $name }" stopped: what input { $entry } waits for did not come in time.
ui-status-macro-closed = Macro "{ $name }" stopped: the session ended.
ui-macros-edit = Edit
ui-dialog-save-macro-warning = Everything typed was recorded, passwords included: the macro keeps it as typed.
ui-dialog-delete-macro-body = Delete the macro "{ $name }"? This cannot be undone.
ui-macro-editor-title = Edit macro
ui-macro-editor-name = Name
ui-macro-editor-input-hint = Control characters are written \r (Enter), \n, \t, \xNN, and a backslash \\.
ui-macro-editor-input = Input
ui-macro-editor-delay = Delay (ms)
ui-macro-editor-move-up = Move up
ui-macro-editor-move-down = Move down
ui-macro-editor-delete-entry = Delete entry
ui-macro-editor-expects = Wait for text first
ui-macro-editor-pattern = Expect pattern
ui-macro-editor-regex = Regex
ui-macro-editor-timeout = Timeout (ms)
ui-macro-editor-timeout-abort = Abort
ui-macro-editor-timeout-continue = Continue
ui-macro-editor-add-expect = Add expect step
ui-macro-editor-add-send = Add send step
ui-macro-editor-delete-macro = Delete macro
ui-macro-editor-name-required = Macro name is required.
ui-macro-editor-entry-invalid = Entry { $entry }: { $reason }
ui-macro-editor-input-trailing = the input ends with a lone backslash.
ui-macro-editor-input-hex = \x needs two hexadecimal digits.
ui-macro-editor-input-escape = \{ $escape } is not an escape the editor knows.
ui-macro-editor-delay-invalid = the delay is not a number of milliseconds.
ui-macro-editor-timeout-range = Range: { $min }-{ $max } ms
ui-macro-editor-regex-invalid = Invalid regex: { $reason }
ui-about-section-settings-file = Settings file
ui-about-export-settings = Export settings...
ui-about-import-settings = Import settings...
ui-about-settings-file-hint = Carries your preferences to another computer. The file holds no secret: no master password, PIN or saved password.
ui-settings-file-filter = Heimdall settings
ui-dialog-settings-export-title = Export settings
ui-dialog-settings-export-paths = { $count ->
    [one] { $count } setting names a folder in your user profile on this computer (tool paths, log folder, files). Include it in the file?
   *[other] { $count } settings name folders in your user profile on this computer (tool paths, log folder, files). Include them in the file?
}
ui-dialog-settings-export-without = Leave out
ui-dialog-settings-export-with = Include
ui-dialog-settings-import-title = Import settings
ui-dialog-settings-import-body = { $count ->
    [one] { $count } setting will change:
   *[other] { $count } settings will change:
}
ui-dialog-settings-import-line = { $key }: { $before } -> { $after }
ui-dialog-settings-import-confirm = Import
ui-settings-value-on = On
ui-settings-value-off = Off
ui-settings-value-empty = (empty)
ui-settings-value-items = { $count ->
    [one] { $count } item
   *[other] { $count } items
}
ui-status-settings-exported = Settings exported. The file holds no secret: no master password, PIN or saved password.
ui-status-settings-export-failed = The settings could not be exported: { $reason }
ui-status-settings-imported = { $count ->
    [one] { $count } setting imported.
   *[other] { $count } settings imported.
}
ui-status-settings-import-nothing = The file holds the settings you already have. Nothing to change.
ui-status-settings-import-invalid = This file is not a Heimdall settings file, or it comes from a version this one cannot read. Nothing was changed.
ui-status-settings-import-failed = The settings file could not be read: { $reason }
ui-settings-tab-gateways = Gateways
ui-gateways-title = SSH Gateways
ui-gateways-description = Review which sessions use each SSH gateway and find unresolved gateway references.
ui-gateways-summary = { $gateways ->
    [one] { $gateways } gateway
   *[other] { $gateways } gateways
}, { $routed ->
    [one] { $routed } routed session
   *[other] { $routed } routed sessions
}, { $unresolved ->
    [one] { $unresolved } unresolved reference
   *[other] { $unresolved } unresolved references
}
ui-gateways-configured = Configured gateways
ui-gateways-empty = No SSH gateways are configured.
ui-gateways-parent = Parent: { $name }
ui-gateways-sessions = { $count ->
    [one] { $count } session
   *[other] { $count } sessions
}
ui-gateways-no-sessions = No sessions use this gateway.
ui-gateways-edit = Edit
ui-gateways-delete = Delete
ui-gateways-unresolved = Unresolved references
ui-gateways-missing-description = These sessions or child gateways reference a gateway id that is not configured.
ui-gateways-missing-header = Missing gateway id: { $id }
ui-gateways-child = Child gateway: { $name }
ui-gateways-reassign-to = Gateway
ui-gateways-reassign = Reassign
ui-gateways-clear = Clear
ui-dialog-delete-gateway-title = Delete gateway
ui-dialog-delete-gateway-body = Delete gateway "{ $name }"?

    References to clear:
    - Servers: { $servers }
    - Child gateways: { $gateways }
ui-status-gateway-deleted = Gateway "{ $name }" deleted.
ui-status-gateways-reassigned = { $count ->
    [one] Reassigned { $count } session.
   *[other] Reassigned { $count } sessions.
}
ui-status-gateways-cleared = { $count ->
    [one] Cleared gateway reference on { $count } session.
   *[other] Cleared gateway reference on { $count } sessions.
}
ui-status-gateways-unchanged = No sessions needed changes.
ui-settings-reachability = Session Health Monitor
ui-settings-reachability-enabled = Enable background reachability probes
ui-settings-reachability-hint = Every server is dialled from this computer, a few at a time, and its dot in the list shows whether it answered. Servers behind a gateway are not dialled. Nothing is sent but the connection.
ui-settings-reachability-interval = Check interval
ui-settings-reachability-timeout = Probe timeout
ui-settings-reachability-probes = Max concurrent probes
ui-settings-milliseconds-unit = ms
ui-settings-reachability-interval-refused = Check interval must be between { $min } and { $max } seconds.
ui-settings-reachability-timeout-refused = Probe timeout must be between { $min } and { $max } ms.
ui-settings-reachability-probes-refused = Max concurrent probes must be between { $min } and { $max }.
ui-settings-updates = Updates
ui-settings-updates-enabled = Check for updates automatically
ui-settings-updates-interval = Check interval
ui-settings-hours-unit = h
ui-settings-updates-interval-refused = Update check interval must be between { $min } and { $max } hours.
ui-settings-updates-current-version = Current version
ui-settings-updates-development-build = { $version } (development build)
ui-settings-updates-check-now = Check now
ui-settings-updates-skipped-version = Skipped version: { $version }
ui-settings-updates-clear-skipped = Offer it again
ui-settings-updates-checking = Checking for updates...
ui-settings-updates-up-to-date = You are running the latest version.
ui-settings-updates-available = Update available: { $version }
ui-settings-updates-unknown-version = Current version is unknown; cannot check for updates.
ui-settings-updates-failed-network = Could not reach the update server. Check your internet connection and try again.
ui-settings-updates-failed-secure-channel = Could not establish a secure connection to the update server. A proxy, a missing certificate or an incorrect system clock can cause this.
ui-settings-updates-failed-proxy = A proxy refused the connection to the update server. Check your proxy settings, or whether it needs credentials.
ui-settings-updates-failed-rate-limited = The update server is temporarily refusing requests from this network. Try again later.
ui-settings-updates-failed-rate-limited-minute = The update server is temporarily refusing requests from this network. Try again in about a minute.
ui-settings-updates-failed-rate-limited-minutes = The update server is temporarily refusing requests from this network. Try again in about { $minutes } minutes.
ui-settings-updates-failed-access-denied = The update server refused the request. See the log for details.
ui-settings-updates-failed-unavailable = The update server is unavailable. Try again later.
ui-settings-updates-failed-malformed = The update server answered with something Heimdall could not read. See the log for details.
ui-settings-updates-failed-timed-out = The update server did not answer in time. Try again later.
ui-update-banner-text = A new version is available: { $version }
ui-update-banner-view-release = View release
ui-update-banner-later = Later
ui-update-banner-skip = Skip this version
ui-tree-reachability-checking = Checking...
ui-tree-reachability-up = Reachable ({ $millis } ms)
ui-tree-reachability-down = Unreachable: { $reason }
ui-tree-reachability-unchecked = Unknown: { $reason }
ui-reachability-reason-timeout = Connection timed out
ui-reachability-reason-refused = Connection refused
ui-reachability-reason-unreachable = Host unreachable
ui-reachability-reason-dns = DNS resolution failed
ui-reachability-reason-behind-gateway = Behind SSH gateway - not probed
ui-reachability-reason-no-port = No probe port for this protocol
ui-reachability-reason-no-host = No host configured
ui-status-dropped-profiles = { $count ->
    [one] Moved { $count } session to { $folder }. Ctrl+Z undoes it.
   *[other] Moved { $count } sessions to { $folder }. Ctrl+Z undoes it.
}
ui-status-dropped-profiles-none = { $count ->
    [one] Moved { $count } session out of its folder. Ctrl+Z undoes it.
   *[other] Moved { $count } sessions out of their folders. Ctrl+Z undoes it.
}
ui-status-dropped-folder = Moved the folder { $name }. Ctrl+Z undoes it.
ui-status-drop-refused = A folder of that name is already there: nothing was moved.
ui-status-move-undone = Move undone.
ui-status-listing-cancelled = Loading cancelled.
ui-status-session-limit = The maximum of { $max } remote embedded sessions is already open. Close a session before opening another.
ui-status-sftp-auto-open-failed = SFTP auto-open failed: { $reason }
ui-status-sftp-browser-disabled = The integrated SFTP browser is disabled in settings.
ui-status-nothing-to-undo = Nothing to undo.
ui-shortcuts-undo-move = Undo the last move made by dragging in the tree
ui-tab-menu-vnc-remote-resize = Resize the remote desktop to the tab
ui-detail-host-port = { $host } : { $port }
ui-detail-folder = Folder:
ui-detail-environment = Environment:
ui-detail-username = Username:
ui-detail-gateway = Gateway:
ui-detail-credentials = Saved credentials:
ui-detail-tags = Tags:
ui-detail-favorite = Favorite:
ui-detail-connect = Connect
ui-detail-saved-password = password
ui-detail-saved-key = key file { $name }
ui-detail-saved-passphrase = key passphrase
ui-detail-hints = Enter or double-click connects, Ctrl+E edits, Delete deletes, F1 lists every shortcut.
ui-detail-edit = Edit
ui-tree-notes = Notes
ui-notes-new = New
ui-notes-daily = Daily
ui-notes-incident = Incident
ui-notes-procedure = Procedure
ui-notes-tpl-working-note = Working Note
ui-notes-tpl-notes = Notes
ui-notes-tpl-commands = Commands
ui-notes-tpl-next = Next
ui-notes-tpl-daily-note = Daily Note
ui-notes-tpl-focus = Focus
ui-notes-tpl-journal = Journal
ui-notes-tpl-follow-up = Follow-up
ui-notes-tpl-incident = Incident
ui-notes-tpl-incident-report = Incident Report
ui-notes-tpl-summary = Summary
ui-notes-tpl-impact = Impact
ui-notes-tpl-timeline = Timeline
ui-notes-tpl-incident-started = Incident started
ui-notes-tpl-investigation = Investigation
ui-notes-tpl-actions = Actions
ui-notes-tpl-resolution = Resolution
ui-notes-tpl-procedure = Procedure
ui-notes-tpl-purpose = Purpose
ui-notes-tpl-scope = Scope
ui-notes-tpl-preconditions = Preconditions
ui-notes-tpl-steps = Steps
ui-notes-tpl-validation = Validation
ui-notes-tpl-rollback = Rollback
ui-notes-tpl-references = References
ui-about-open-notes = Open notes folder
ui-status-note-opened = Note { $name } opened in the editor.
ui-status-note-failed = The note could not be opened: { $reason }
ui-nav-sessions = Sessions
ui-nav-tunnels = Tunnels
ui-nav-tools = Tools
ui-nav-settings = Settings
ui-nav-about = About
ui-tools-filter-placeholder = Filter tools...
ui-tools-no-results = No matching tool
ui-tools-no-results-hint = Try another term or an alias such as ping, dns, json, or password.
ui-tools-context-with = Network tools will use the selected target: { $host }
ui-tools-context-none = Network tools will open without an inherited target.
ui-tools-favorites = Favorites
ui-tools-recent = Recently Used
ui-tools-all = All Tools
ui-tools-empty-favorites = Pin your favorite tools for quick access
ui-tools-pin-tooltip = Pin to favorites
ui-tools-unpin-tooltip = Unpin from favorites
ui-tools-page-title = Tools
ui-tools-search-placeholder = Search tools...
ui-tools-count =
    { $count ->
        [one] { $count } tool
       *[other] { $count } tools
    }
ui-tools-category-network = Network
ui-tools-category-security = Security
ui-tools-category-encoding = Encoding & Format
ui-tools-category-system = System
ui-tools-category-external = External
ui-tool-help-tooltip = Show help
ui-tool-help-close = Close
ui-tool-copy-tooltip = Copy to clipboard
ui-tool-base64-name = Base64 Encoder / Decoder
ui-tool-base64-description = Base64 encoder and decoder for text and files
ui-tool-base64-title = Base64 Encoder / Decoder
ui-tool-base64-input = Input
ui-tool-base64-output = Output
ui-tool-base64-encode = Encode →
ui-tool-base64-decode = ← Decode
ui-tool-base64-copy = Copy output
ui-tool-base64-browse = Browse file...
ui-tool-base64-file-mode = File mode
ui-tool-base64-url-safe = URL-safe (RFC 4648)
ui-tool-base64-placeholder = text to encode/decode
ui-tool-base64-empty = Enter text and press Encode or Decode.
ui-tool-base64-status-encoded =
    { $count ->
        [one] Encoded { $count } byte
       *[other] Encoded { $count } bytes
    }
ui-tool-base64-status-decoded =
    { $count ->
        [one] Decoded { $count } byte
       *[other] Decoded { $count } bytes
    }
ui-tool-base64-status-saved = Saved to { $path }
ui-tool-base64-status-error = Error: { $error }
ui-tool-base64-status-invalid = Invalid Base64 input
ui-tool-base64-save-title = Save decoded file
ui-tool-base64-open-title = Select file to encode
ui-tool-base64-file-loaded =
    { $count ->
        [one] File loaded: { $name } ({ $count } byte)
       *[other] File loaded: { $name } ({ $count } bytes)
    }
ui-tool-base64-too-large = File exceeds the 5 MB size limit.
ui-tool-base64-help =
    Base64 Encoder/Decoder

    Encodes text to Base64 or decodes Base64 back to text.

    Usage:
    Paste text and click Encode or Decode. Supports URL-safe Base64 variant.
ui-tool-urlenc-name = URL Encoder / Decoder
ui-tool-urlenc-description = URL encoder and decoder for query strings and paths
ui-tool-urlenc-title = URL Encoder / Decoder
ui-tool-urlenc-decoded = Decoded
ui-tool-urlenc-encoded = Encoded
ui-tool-urlenc-copy = Copy
ui-tool-urlenc-component = Strict component encoding (encode all reserved characters)
ui-tool-urlenc-decoded-placeholder = URL or text to encode/decode
ui-tool-urlenc-encoded-placeholder = encoded URL
ui-tool-urlenc-help =
    URL Encoder/Decoder

    Encodes or decodes URL-encoded (percent-encoded) strings.

    Usage:
    Paste a URL or text and click Encode or Decode.
ui-tool-uuid-name = UUID Generator
ui-tool-uuid-description = UUID/GUID generator with multiple format options
ui-tool-uuid-title = UUID Generator
ui-tool-uuid-result-v4 = Generated UUID (v4)
ui-tool-uuid-result-v7 = Generated UUID (v7)
ui-tool-uuid-v4 = v4 (Random)
ui-tool-uuid-v7 = v7 (Timestamp)
ui-tool-uuid-generate = Generate
ui-tool-uuid-copy = Copy
ui-tool-uuid-uppercase = Uppercase
ui-tool-uuid-hyphens = With hyphens
ui-tool-uuid-batch = Batch Generation
ui-tool-uuid-count = Count
ui-tool-uuid-generate-batch = Generate Batch
ui-tool-uuid-copy-batch = Copy all
ui-tool-uuid-help =
    UUID Generator

    Generates universally unique identifiers.

    Usage:
    Click Generate to create UUIDs. Supports v4 (random) with multiple format options (standard, uppercase, no dashes, URN).
ui-tool-hash-name = Hash Generator
ui-tool-hash-description = File and text hash generator (MD5, SHA-1, SHA-256, SHA-512)
ui-tool-hash-title = Hash Generator
ui-tool-hash-input = Input text
ui-tool-hash-placeholder = text to hash
ui-tool-hash-drop-zone = Drop a file here or click Browse to hash a file
ui-tool-hash-browse = Browse File
ui-tool-hash-clear-file = Clear file
ui-tool-hash-hashing = Hashing file...
ui-tool-hash-empty = Enter text or browse a file to compute hashes.
ui-tool-hash-results = Hash results
ui-tool-hash-copy = Copy
ui-tool-hash-save = Save
ui-tool-hash-verify = Expected hash (verify)
ui-tool-hash-verify-placeholder = paste hash to verify
ui-tool-hash-byte-length = { $count } bytes
ui-tool-hash-file-status = { $name } - { $size }
ui-tool-hash-too-large = File exceeds maximum size ({ $size }).
ui-tool-hash-not-found = File not found.
ui-tool-hash-access-denied = Access denied to file.
ui-tool-hash-error = Could not hash the file.
ui-tool-hash-match = ✓ Match ({ $algorithm })
ui-tool-hash-no-match = ✗ No match
ui-tool-hash-all-files = All files (*.*)
ui-tool-hash-help =
    Hash Generator

    Computes cryptographic hashes for text input or files.

    Usage:
    1. Type or paste text in the input field, OR
    2. Drag and drop a file (or click Browse)
    3. All hash values are computed simultaneously

    Supported algorithms:
    - MD5 (128-bit)
    - SHA-1 (160-bit)
    - SHA-256 (256-bit)
    - SHA-384 (384-bit)
    - SHA-512 (512-bit)
    - SHA3-256 (256-bit, if supported)

    Verify mode:
    Paste a known hash in the Verify field to check if it matches. The matching algorithm is auto-detected by hash length.

    Examples:
    - Type "hello" - SHA-256: 2cf24dba...
    - Drop a file - Verify against known checksum
ui-tool-hmac-name = HMAC Generator
ui-tool-hmac-description = HMAC message authentication code generator
ui-tool-hmac-title = HMAC Generator
ui-tool-hmac-algorithm = Algorithm
ui-tool-hmac-key = Secret key
ui-tool-hmac-key-placeholder = secret key
ui-tool-hmac-toggle-key = Show/hide key
ui-tool-hmac-input = Message text
ui-tool-hmac-input-placeholder = message
ui-tool-hmac-format = Output format
ui-tool-hmac-format-hex = Hex
ui-tool-hmac-format-base64 = Base64
ui-tool-hmac-empty = Enter a key and a message to compute HMAC.
ui-tool-hmac-output = HMAC output
ui-tool-hmac-copy = Copy
ui-tool-hmac-byte-length = { $bytes } bytes ({ $bits } bits)
ui-tool-hmac-verify = Expected HMAC (verify)
ui-tool-hmac-verify-placeholder = paste HMAC to verify
ui-tool-hmac-match = Match
ui-tool-hmac-no-match = No match
ui-tool-hmac-help =
    HMAC Generator

    Computes keyed-hash message authentication codes.

    Usage:
    Enter a message and secret key, then select an algorithm (SHA-256, SHA-512, etc.) to compute the HMAC.
ui-tool-jwt-name = JWT Parser
ui-tool-jwt-description = JSON Web Token decoder and validator
ui-tool-jwt-title = JWT Parser
ui-tool-jwt-input = Paste JWT token
ui-tool-jwt-placeholder = paste JWT token (header.payload.signature)
ui-tool-jwt-empty = Paste a JWT token to decode its header, payload, and signature.
ui-tool-jwt-error-format = Invalid JWT format. A JWT must contain exactly 3 parts separated by dots.
ui-tool-jwt-error-decode = Failed to decode JWT. Check that the token is valid Base64Url.
ui-tool-jwt-expired = Expired: { $date }
ui-tool-jwt-valid = Valid until: { $date }
ui-tool-jwt-no-expiry = No expiration claim (exp) found
ui-tool-jwt-header = Header
ui-tool-jwt-payload = Payload
ui-tool-jwt-signature = Signature
ui-tool-jwt-copy = Copy
ui-tool-jwt-verify-title = Signature Verification
ui-tool-jwt-unsupported = RSA/ECDSA verification requires a public key (not supported)
ui-tool-jwt-secret = HMAC Secret
ui-tool-jwt-verify = Verify
ui-tool-jwt-signature-valid = Signature is valid
ui-tool-jwt-signature-invalid = Signature is invalid
ui-tool-jwt-help =
    JWT Parser

    Decodes and inspects JSON Web Tokens.

    Usage:
    Paste a JWT to view its header, payload, and signature. Expiry and claims are displayed in a readable format.
ui-tool-totp-name = TOTP Generator
ui-tool-totp-description = Time-based one-time password generator (2FA/MFA)
ui-tool-totp-title = TOTP Generator
ui-tool-totp-secret = Secret Key (Base32)
ui-tool-totp-secret-placeholder = Base32 secret key
ui-tool-totp-start = Start
ui-tool-totp-code = Current Code
ui-tool-totp-copy = Copy
ui-tool-totp-remaining = { $seconds }s remaining
ui-tool-totp-info = Enter a Base32-encoded secret key (as provided by Google Authenticator, Authy, etc.) to generate time-based one-time passwords (TOTP). Codes refresh every 30 seconds.
ui-tool-totp-error-required = Please enter a secret key.
ui-tool-totp-error-base32 = Invalid Base32 encoding. Use characters A-Z and 2-7 only.
ui-tool-totp-help =
    TOTP Generator

    Generates time-based one-time passwords (RFC 6238).

    Usage:
    Enter a Base32 secret key to generate TOTP codes that refresh every 30 seconds.
ui-tool-copy-value = Copy
ui-tool-number-group-separator = {","}
ui-tool-number-decimal-separator = {"."}
ui-tool-subnet-name = Subnet Calculator
ui-tool-subnet-description = Subnet calculator with CIDR notation and address range breakdown
ui-tool-subnet-title = Subnet Calculator
ui-tool-subnet-input = IP address / CIDR notation (e.g. 192.168.1.0/24)
ui-tool-subnet-placeholder = 192.168.1.0/24
ui-tool-subnet-network = Network Address
ui-tool-subnet-broadcast = Broadcast Address
ui-tool-subnet-mask = Subnet Mask
ui-tool-subnet-first-host = First Host
ui-tool-subnet-last-host = Last Host
ui-tool-subnet-total-hosts = Total Hosts
ui-tool-subnet-cidr = CIDR Notation
ui-tool-subnet-wildcard = Wildcard Mask
ui-tool-subnet-too-many = Too many to list
ui-tool-subnet-error-invalid = Invalid IP address or CIDR notation. Use format: 192.168.1.0/24
ui-tool-subnet-empty = Enter a CIDR notation to calculate subnet details
ui-tool-subnet-help =
    Subnet Calculator

    Calculates IPv4 and IPv6 network information from CIDR notation.

    Usage:
    1. Enter an IP address with CIDR prefix (e.g. 192.168.1.0/24)
    2. Results update automatically

    Displayed information:
    - Network address
    - Broadcast address
    - Subnet mask and wildcard mask
    - First and last usable host
    - Total number of usable hosts
    - CIDR notation

    Examples:
    - 192.168.1.0/24 - 254 hosts (Class C)
    - 10.0.0.0/8 - 16,777,214 hosts (Class A)
    - 172.16.0.0/16 - 65,534 hosts (Class B)
    - 192.168.1.64/26 - 62 hosts (subnet)
    - 2001:db8::/32 - IPv6 prefix
ui-tool-ipconv-name = IP Address Converter
ui-tool-ipconv-description = IP address format converter (decimal, binary, hexadecimal)
ui-tool-ipconv-title = IP Address Converter
ui-tool-ipconv-input = Enter IPv4 address (dotted, integer, hex, or binary)
ui-tool-ipconv-placeholder = IP address or integer
ui-tool-ipconv-dotted = Dotted Decimal
ui-tool-ipconv-integer = Integer
ui-tool-ipconv-hex = Hexadecimal
ui-tool-ipconv-binary = Binary
ui-tool-ipconv-mapped = IPv4-Mapped IPv6
ui-tool-ipconv-error-invalid = Invalid input. Enter a valid IPv4 address, integer, hex (0x...), or dotted binary.
ui-tool-ipconv-empty = Enter an IP address, integer, hex, or dotted binary to convert.
ui-tool-ipconv-help =
    IP Converter

    Converts IP addresses between decimal, hexadecimal, binary, and integer formats.

    Usage:
    Enter an IP address in any supported format and all conversions are displayed instantly.
ui-tool-netcalc-name = Network Calculator
ui-tool-netcalc-description = Advanced network calculator with VLAN and supernet support
ui-tool-netcalc-title = Network Calculator
ui-tool-netcalc-mode = Mode
ui-tool-netcalc-mode-supernet = Supernet Calculator
ui-tool-netcalc-mode-range = IP Range to CIDR
ui-tool-netcalc-mode-vlan = VLAN Planner
ui-tool-netcalc-supernet-input = CIDR ranges (one per line)
ui-tool-netcalc-compute = Compute
ui-tool-netcalc-start-ip = Start IP
ui-tool-netcalc-end-ip = End IP
ui-tool-netcalc-start-placeholder = e.g. 192.168.1.0
ui-tool-netcalc-end-placeholder = e.g. 192.168.1.255
ui-tool-netcalc-hosts-needed = Hosts needed
ui-tool-netcalc-hosts-placeholder = e.g. 50
ui-tool-netcalc-base-network = Base network
ui-tool-netcalc-base-placeholder = e.g. 10.0.0.0
ui-tool-netcalc-error-no-cidrs = Enter at least one CIDR range.
ui-tool-netcalc-error-invalid-cidr = Invalid CIDR notation: { $line }
ui-tool-netcalc-error-invalid-range = Enter valid IPv4 addresses for start and end.
ui-tool-netcalc-error-start-after-end = Start IP must be less than or equal to End IP.
ui-tool-netcalc-error-host-count = Enter a positive number of hosts.
ui-tool-netcalc-error-base-network = Enter a valid IPv4 base network address.
ui-tool-netcalc-supernet-result = Supernet: { $network }/{ $prefix }
ui-tool-netcalc-supernet-range = Range: { $first } - { $last }
ui-tool-netcalc-supernet-hosts = Total usable hosts: { $hosts }
ui-tool-netcalc-range-result = CIDR blocks covering the range:
ui-tool-netcalc-vlan-network = Network: { $network }/{ $prefix }
ui-tool-netcalc-vlan-mask = Subnet mask: { $mask }
ui-tool-netcalc-vlan-broadcast = Broadcast: { $broadcast }
ui-tool-netcalc-vlan-usable-range = Usable range: { $first } - { $last }
ui-tool-netcalc-vlan-usable-hosts = Usable hosts: { $hosts }
ui-tool-netcalc-vlan-requested = Requested: { $hosts }
ui-tool-netcalc-vlan-utilization = Utilization: { $percent }%
ui-tool-netcalc-empty = Enter subnets or IP ranges to calculate.
ui-tool-netcalc-help =
    Network Calculator

    Advanced network calculations including VLAN, supernetting, and subnet splitting.

    Usage:
    Enter network parameters to perform calculations across multiple subnets.
ui-tool-chmod-name = Chmod Calculator
ui-tool-chmod-description = Interactive chmod permission calculator for Unix file modes
ui-tool-chmod-title = Chmod Calculator
ui-tool-chmod-read = Read
ui-tool-chmod-write = Write
ui-tool-chmod-execute = Execute
ui-tool-chmod-owner = Owner
ui-tool-chmod-group = Group
ui-tool-chmod-others = Others
ui-tool-chmod-octal = Octal:
ui-tool-chmod-symbolic = Symbolic:
ui-tool-chmod-copy-octal = Copy octal
ui-tool-chmod-copy-symbolic = Copy symbolic
ui-tool-chmod-presets = Common presets
ui-tool-chmod-symbolic-input = Symbolic notation (e.g. u+x,g-w,o=r):
ui-tool-chmod-symbolic-placeholder = u+x,g-w,o=r
ui-tool-chmod-error-symbolic = Invalid symbolic notation
ui-tool-chmod-command-preview = Command preview:
ui-tool-chmod-copy-command = Copy command
ui-tool-chmod-command = chmod { $mode } filename
ui-tool-chmod-help =
    Chmod Calculator

    Interactive Unix file permission calculator.

    Usage:
    Toggle read/write/execute permissions for owner, group, and others. The numeric and symbolic chmod values update in real time.
ui-tool-datetime-name = DateTime Converter
ui-tool-datetime-description = Date/time and Unix epoch converter with timezone support
ui-tool-datetime-title = DateTime Converter
ui-tool-datetime-input = Unix timestamp (seconds) or ISO 8601 datetime
ui-tool-datetime-placeholder = timestamp, ISO 8601, or date string
ui-tool-datetime-now = Now
ui-tool-datetime-copy = Copy
ui-tool-datetime-unix = Unix Timestamp (seconds)
ui-tool-datetime-iso-utc = ISO 8601 (UTC)
ui-tool-datetime-iso-local = ISO 8601 (Local)
ui-tool-datetime-local-time = Local Time
ui-tool-datetime-timezone = Timezone
ui-tool-datetime-relative = Relative time
ui-tool-datetime-detected-unix = Detected: Unix timestamp
ui-tool-datetime-detected-ms = Detected: Unix timestamp (milliseconds)
ui-tool-datetime-detected-iso = Detected: ISO 8601 datetime
ui-tool-datetime-error-invalid = Invalid input. Enter a Unix timestamp or ISO 8601 datetime.
ui-tool-datetime-empty = Enter a Unix timestamp or ISO 8601 date to convert.
ui-tool-datetime-relative-seconds = { $count } seconds
ui-tool-datetime-relative-minutes = { $count } minutes
ui-tool-datetime-relative-hours = { $count } hours
ui-tool-datetime-relative-days = { $count } days
ui-tool-datetime-relative-months = { $count } months
ui-tool-datetime-relative-years = { $count } years
ui-tool-datetime-relative-ago = { $duration } ago
ui-tool-datetime-relative-in = in { $duration }
ui-tool-datetime-long = { $weekday }, { $d } { $month } { $year } { $time }
ui-tool-datetime-weekday-0 = Sunday
ui-tool-datetime-weekday-1 = Monday
ui-tool-datetime-weekday-2 = Tuesday
ui-tool-datetime-weekday-3 = Wednesday
ui-tool-datetime-weekday-4 = Thursday
ui-tool-datetime-weekday-5 = Friday
ui-tool-datetime-weekday-6 = Saturday
ui-tool-datetime-month-1 = January
ui-tool-datetime-month-2 = February
ui-tool-datetime-month-3 = March
ui-tool-datetime-month-4 = April
ui-tool-datetime-month-5 = May
ui-tool-datetime-month-6 = June
ui-tool-datetime-month-7 = July
ui-tool-datetime-month-8 = August
ui-tool-datetime-month-9 = September
ui-tool-datetime-month-10 = October
ui-tool-datetime-month-11 = November
ui-tool-datetime-month-12 = December
ui-tool-datetime-help =
    Date/Time Converter

    Converts between human-readable dates and Unix timestamps.

    Usage:
    Enter a date or Unix epoch value. Supports multiple date formats and timezone conversion.
ui-tool-ulid-name = ULID Generator
ui-tool-ulid-description = ULID generator - 128-bit lexicographically sortable identifier (Crockford base32)
ui-tool-ulid-title = ULID Generator
ui-tool-ulid-result = Generated ULID
ui-tool-ulid-generate = Generate
ui-tool-ulid-copy = Copy
ui-tool-ulid-batch = Batch Generation
ui-tool-ulid-count = Count
ui-tool-ulid-generate-batch = Generate Batch
ui-tool-ulid-copy-batch = Copy all
ui-tool-ulid-help =
    ULID Generator

    Generates 128-bit lexicographically sortable identifiers.

    Format: 26 characters in Crockford base32 (no I, L, O, U).

    Structure:
    - First 10 chars: 48-bit Unix timestamp in milliseconds
    - Last 16 chars: 80 bits of cryptographically random data

    ULIDs generated in order sort in order. Useful as primary keys in distributed systems.
ui-tool-crontab-name = Crontab Builder
ui-tool-crontab-description = Crontab expression builder with human-readable preview
ui-tool-crontab-title = Crontab Builder
ui-tool-crontab-presets = Quick presets
ui-tool-crontab-preset-every-minute = Every minute
ui-tool-crontab-preset-every-hour = Every hour
ui-tool-crontab-preset-daily-midnight = Daily midnight
ui-tool-crontab-preset-weekdays-9am = Weekdays 9am
ui-tool-crontab-preset-weekly-sunday = Weekly Sunday
ui-tool-crontab-preset-monthly-1st = Monthly 1st
ui-tool-crontab-minute = Minute
ui-tool-crontab-hour = Hour
ui-tool-crontab-day-of-month = Day of Month
ui-tool-crontab-month = Month
ui-tool-crontab-day-of-week = Day of Week
ui-tool-crontab-every-minute = Every minute (*)
ui-tool-crontab-every-5-min = Every 5 minutes (*/5)
ui-tool-crontab-every-15-min = Every 15 minutes (*/15)
ui-tool-crontab-every-30-min = Every 30 minutes (*/30)
ui-tool-crontab-every-hour = Every hour (*)
ui-tool-crontab-every-day = Every day (*)
ui-tool-crontab-every-month = Every month (*)
ui-tool-crontab-every-day-of-week = Every day (*)
ui-tool-crontab-month-1 = Jan
ui-tool-crontab-month-2 = Feb
ui-tool-crontab-month-3 = Mar
ui-tool-crontab-month-4 = Apr
ui-tool-crontab-month-5 = May
ui-tool-crontab-month-6 = Jun
ui-tool-crontab-month-7 = Jul
ui-tool-crontab-month-8 = Aug
ui-tool-crontab-month-9 = Sep
ui-tool-crontab-month-10 = Oct
ui-tool-crontab-month-11 = Nov
ui-tool-crontab-month-12 = Dec
ui-tool-crontab-day-abbr-0 = Sun
ui-tool-crontab-day-abbr-1 = Mon
ui-tool-crontab-day-abbr-2 = Tue
ui-tool-crontab-day-abbr-3 = Wed
ui-tool-crontab-day-abbr-4 = Thu
ui-tool-crontab-day-abbr-5 = Fri
ui-tool-crontab-day-abbr-6 = Sat
ui-tool-crontab-day-0 = Sunday
ui-tool-crontab-day-1 = Monday
ui-tool-crontab-day-2 = Tuesday
ui-tool-crontab-day-3 = Wednesday
ui-tool-crontab-day-4 = Thursday
ui-tool-crontab-day-5 = Friday
ui-tool-crontab-day-6 = Saturday
ui-tool-crontab-expression = Cron Expression
ui-tool-crontab-copy = Copy
ui-tool-crontab-manual-edit = Manual edit (5-field cron expression)
ui-tool-crontab-placeholder = { "* * * * *" }
ui-tool-crontab-next-runs = Next 5 executions
ui-tool-crontab-desc-every-minute = Runs every minute
ui-tool-crontab-desc-every-hour = Runs at the start of every hour
ui-tool-crontab-desc-every-day = Runs daily at midnight
ui-tool-crontab-desc-every-n-min = Runs every { $interval } minutes
ui-tool-crontab-desc-daily-at = Runs daily at { $time }
ui-tool-crontab-desc-weekly-at = Runs every { $day } at { $time }
ui-tool-crontab-desc-monthly-at = Runs on the { $day }th of every month at { $time }
ui-tool-crontab-desc-custom = Custom schedule: { $expression }
ui-tool-crontab-error-field-count = A cron expression must have exactly 5 fields separated by spaces
ui-tool-crontab-error-invalid-field = Invalid characters in field "{ $field }": { $value }
ui-tool-crontab-error-out-of-range = Field "{ $field }" values must be between { $min } and { $max }
ui-tool-crontab-help =
    Crontab Builder

    Builds cron expressions with a human-readable preview.

    Usage:
    Configure minute, hour, day, month, and weekday fields. The next scheduled execution times are displayed.
ui-tool-sshconfig-name = SSH Config Generator
ui-tool-sshconfig-description = SSH client configuration file generator
ui-tool-sshconfig-title = SSH Config Generator
ui-tool-sshconfig-host-alias = Host alias
ui-tool-sshconfig-host-name = HostName
ui-tool-sshconfig-user = User
ui-tool-sshconfig-port = Port
ui-tool-sshconfig-identity-file = IdentityFile
ui-tool-sshconfig-proxy-jump = ProxyJump
ui-tool-sshconfig-forward-agent = ForwardAgent
ui-tool-sshconfig-alive-interval = ServerAliveInterval
ui-tool-sshconfig-generate = Generate
ui-tool-sshconfig-generate-all = Generate All from Heimdall
ui-tool-sshconfig-copy = Copy
ui-tool-sshconfig-error-host-required = HostName is required.
ui-tool-sshconfig-generate-all-hint =
    Open this tool from a session context to pre-fill fields.
    Use the form above to generate config blocks manually for each host.
ui-tool-sshconfig-empty = Configure options above to generate an SSH config block
ui-tool-sshconfig-help =
    SSH Config Generator

    Generates SSH client configuration files.

    Usage:
    Configure host entries with hostname, user, port, key file, and proxy settings. Export to ~/.ssh/config format.
ui-tool-json-name = JSON Formatter
ui-tool-json-description = JSON formatter, validator, and minifier
ui-tool-json-title = JSON Formatter
ui-tool-json-input = Input JSON
ui-tool-json-output = Output
ui-tool-json-prettify = Prettify
ui-tool-json-minify = Minify
ui-tool-json-copy = Copy output
ui-tool-json-placeholder = paste JSON here
ui-tool-json-empty = Paste JSON and press Prettify or Minify.
ui-tool-json-processing = Processing...
ui-tool-json-status-prettified =
    { $count ->
        [one] Prettified ({ $count } character)
       *[other] Prettified ({ $count } characters)
    }
ui-tool-json-status-minified =
    { $count ->
        [one] Minified ({ $count } character)
       *[other] Minified ({ $count } characters)
    }
ui-tool-json-status-error = Invalid JSON: { $error }
ui-tool-json-status-error-at = Error at line { $line }, position { $column }: { $error }
ui-tool-json-too-large = Input exceeds the 5 MB size limit.
ui-tool-json-help =
    JSON Formatter

    Formats (prettifies) or minifies JSON data.

    Usage:
    1. Paste or type JSON in the input field
    2. Click Prettify to format with indentation
    3. Click Minify to compact into a single line
    4. Copy the result to clipboard

    Features:
    - Syntax validation with error messages
    - Handles large JSON documents (up to 5 MB)
    - Preserves Unicode characters

    Keyboard:
    - Ctrl+Enter: Prettify
    - Ctrl+Shift+Enter: Minify

    Examples:
    - { "{" }"name":"value"{ "}" } → Prettified with 2-space indentation
    - Paste API responses for quick formatting
ui-tool-regex-name = Regex Tester
ui-tool-regex-description = Regular expression tester with match highlighting
ui-tool-regex-title = Regex Tester
ui-tool-regex-pattern = Pattern
ui-tool-regex-pattern-placeholder = regex pattern
ui-tool-regex-ignore-case = Ignore case
ui-tool-regex-multiline = Multiline
ui-tool-regex-singleline = Singleline
ui-tool-regex-test-text = Test text
ui-tool-regex-test-placeholder = test string
ui-tool-regex-matches = Matches
ui-tool-regex-copy = Copy matches
ui-tool-regex-count =
    { $count ->
        [one] { $count } match
       *[other] { $count } matches
    }
ui-tool-regex-match-entry = { "[" }{ $number }] Index { $index }: "{ $value }"
ui-tool-regex-group-entry = { "  " }Group { $number }: "{ $value }"
ui-tool-regex-status-valid = Valid regex
ui-tool-regex-status-invalid = Invalid regex: { $error }
ui-tool-regex-status-timeout = Regex evaluation timed out (ReDoS protection)
ui-tool-regex-unsupported-variable-lookbehind = Invalid regex: a look-behind whose length varies is not supported by this engine
ui-tool-regex-unsupported-balancing-group = Invalid regex: balancing groups (?<open-close>...) are not supported by this engine
ui-tool-regex-truncated = Showing first { $shown } of { $total } matches
ui-tool-regex-empty = Enter a regex pattern and test string above
ui-tool-regex-help =
    Regex Tester

    Tests regular expressions against sample text with real-time matching.

    Usage:
    1. Enter a regex pattern
    2. Enter test text
    3. Matches are highlighted and listed automatically

    Options:
    - Ignore Case: case-insensitive matching
    - Multiline: ^ and $ match line boundaries
    - Singleline: . matches newline characters

    Features:
    - Real-time highlighting of matches in the test text
    - Numbered match list with capture group details
    - Match count display
    - Copy all matches to clipboard

    Examples:
    - \b\w+@\w+\.\w+\b - Match email addresses
    - \d{ "{" }1,3{ "}" }\.\d{ "{" }1,3{ "}" }\.\d{ "{" }1,3{ "}" }\.\d{ "{" }1,3{ "}" } - Match IPv4 addresses
    - ^#.*$ (Multiline) - Match comment lines

    Engine:
    - Look-ahead, look-behind, backreferences, atomic groups and conditionals are supported.
    - Not supported: look-behind of varying length, balancing groups.
    - A test that takes longer than one second stops (ReDoS protection).
ui-tool-diff-name = Text Diff
ui-tool-diff-description = Side-by-side text comparison and difference viewer
ui-tool-diff-title = Text Diff
ui-tool-diff-original = Original
ui-tool-diff-modified = Modified
ui-tool-diff-original-placeholder = original text
ui-tool-diff-modified-placeholder = modified text
ui-tool-diff-output = Diff output
ui-tool-diff-compare = Compare
ui-tool-diff-swap = Swap
ui-tool-diff-clear = Clear
ui-tool-diff-copy = Copy diff
ui-tool-diff-ignore-whitespace = Ignore whitespace
ui-tool-diff-ignore-case = Ignore case
ui-tool-diff-auto-compare = Auto-compare
ui-tool-diff-stats = +{ $added } additions, -{ $removed } deletions, { $unchanged } unchanged
ui-tool-diff-status-done =
    { $count ->
        [one] Diff complete: { $count } line
       *[other] Diff complete: { $count } lines
    }
ui-tool-diff-status-too-large = Input exceeds { $max } lines. Please reduce the text size.
ui-tool-diff-comparing = Comparing...
ui-tool-diff-original-header = --- original
ui-tool-diff-modified-header = +++ modified
ui-tool-diff-empty = Enter original and modified text, then press Compare.
ui-tool-diff-help =
    Text Diff

    Compares two texts side by side and highlights differences.

    Usage:
    Paste text in both panels. Additions, deletions, and changes are color-coded.
ui-tool-textcase-name = Text Case Converter
ui-tool-textcase-description = Text case converter (upper, lower, camel, snake, kebab)
ui-tool-textcase-title = Text Case Converter
ui-tool-textcase-input = Input Text
ui-tool-textcase-placeholder = text to convert
ui-tool-textcase-conversions = Conversions
ui-tool-textcase-output = Output
ui-tool-textcase-copy = Copy
ui-tool-textcase-camel = camelCase
ui-tool-textcase-pascal = PascalCase
ui-tool-textcase-snake = snake_case
ui-tool-textcase-kebab = kebab-case
ui-tool-textcase-upper = UPPER CASE
ui-tool-textcase-lower = lower case
ui-tool-textcase-title-case = Title Case
ui-tool-textcase-constant = CONSTANT_CASE
ui-tool-textcase-empty = Enter text and select a case conversion.
ui-tool-textcase-help =
    Text Case Converter

    Converts text between multiple case formats.

    Supported formats: UPPER, lower, Title Case, camelCase, PascalCase, snake_case, kebab-case, CONSTANT_CASE, and more.
ui-tunnels-page-title = Active Tunnels
ui-import-dropped-rd-gateway = through a Remote Desktop Gateway
ui-error-rd-gateway = This server is reached through the Remote Desktop Gateway { $gateway }, which the built-in client does not go through yet.
ui-error-credential-guard-required = Credential Guard is required but not enabled on this system
ui-profile-resolution-auto = Auto (recommended)
ui-profile-resolution-auto-desc = Heimdall picks the best mode based on the host monitor and whether the session is fullscreen or windowed.
ui-profile-resolution-multi-monitor = Multi-monitor
ui-resolution-mode-auto = Auto
ui-resolution-mode-multi-monitor = Multi-monitor
ui-profile-aspect-ratio = Aspect ratio
ui-profile-aspect-stretch = Stretch (fill)
ui-profile-aspect-ratio-choice = { $wide }:{ $high }
ui-profile-rdp-session-mode = Session mode
ui-profile-rdp-mode-embedded = Embedded (integrated)
ui-profile-rdp-mode-external = External (mstsc.exe)
ui-profile-rdp-mode-external-desc = Launch RDP in a separate mstsc.exe window. Remote Desktop Connection asks for the password itself.
ui-profile-field-rd-gateway = RD Gateway server
ui-profile-rd-gateway-placeholder = rdgateway.example.com (empty = direct connection)
ui-profile-rd-gateway-hint = Microsoft Remote Desktop Gateway used to reach this host over HTTPS. Not the same as the SSH jump host configured above.
ui-profile-rd-gateway-mstsc = With a gateway, this server opens in Remote Desktop Connection (mstsc.exe): the built-in client does not go through one yet.
ui-profile-error-rd-gateway = The RD Gateway must be a valid host name or IP address.
ui-fullscreen-exit = Exit fullscreen
ui-fullscreen-exit-tooltip = F11, or Escape outside a terminal or a remote desktop
ui-files-selected-with-size = { $selection } ({ $size })
ui-files-special-mark = { $name } ({ $kind })
ui-files-tooltip-type = Type: { $kind }
ui-files-tooltip-size = Size: { $size }
ui-files-tooltip-modified = Modified: { $at }
ui-files-tooltip-permissions = Permissions: { $permissions }
ui-files-tooltip-time = { $day } { $time } UTC
ui-tree-expand-all = Expand all
ui-tree-collapse-all = Collapse all
ui-sidebar-hide-tooltip = Hide sidebar (Ctrl+B)
ui-sidebar-show-tooltip = Show sidebar (Ctrl+B)
ui-nav-quick-connect = Quick connect
ui-nav-quick-connect-tooltip = Quick connect (Ctrl+K)
ui-profile-toggle-strict-server-auth = Require server identity validation
ui-error-rdp-server-not-authenticated = The server's identity could not be validated: its certificate is not trusted yet and this computer's certificate authorities do not vouch for it. Strict server authentication refuses it.
ui-certificate-subject = Subject: { $subject }
ui-certificate-issuer = Issuer: { $issuer }
ui-certificate-validity = Valid from / until: { $from } - { $until }{ $period ->
    [expired] {" "}(expired)
    [future] {" "}(not yet valid)
   *[current] {""}
}
ui-certificate-validation-issue = Validation issue: { $issue }
ui-certificate-renewed = Renewed certificate: same key, new certificate. The server presents another certificate on the key you trusted. A renewal is routine, but whoever holds the key could also have made it: approve it only if you expect this renewal.
ui-certificate-renewed-previous = Certificate on record valid from / until: { $from } - { $until }
ui-certificate-issue-self-signed = The certificate is self-signed: no certificate authority vouches for it.
ui-certificate-issue-unknown-issuer = It was issued by a certificate authority this computer does not trust.
ui-certificate-issue-expired = The certificate has expired.
ui-certificate-issue-not-yet-valid = The certificate is not valid yet.
ui-certificate-issue-name-mismatch = The certificate was issued for another name than this server's.
ui-certificate-issue-wrong-purpose = The certificate is not meant for a server: its key usage names other purposes.
ui-certificate-issue-revoked = Its issuer has revoked the certificate.
ui-certificate-issue-no-system-store = This computer has no certificate authority to check it against.
ui-certificate-issue-other = The certificate did not pass this computer's validation.
ui-session-copy-anonymous-button = Copy anonymized report
ui-error-report-anonymous-header = { $protocol } diagnostic report (anonymized)
ui-error-report-kind = Failure:
ui-error-report-anonymous-hint = Includes the time, the number of gateways, how long the session was connected, the application version and the kind of failure. Excludes server addresses, gateway names, account names and error message text.
ui-hostkey-algorithm = Algorithm: { $algorithm }
ui-tunnels-column-started = Started
ui-tunnels-manage-gateways = Manage gateways in Settings...
ui-settings-powershell-policy = PowerShell Execution Policy
ui-settings-powershell-policy-hint = Applied when launching local PowerShell/pwsh sessions
ui-settings-powershell-policy-default = Default
ui-settings-ctrl-v = Ctrl+V in a terminal
ui-settings-ctrl-v-always = Pastes
ui-settings-ctrl-v-outside = Pastes, except in full-screen programs (vim, less...)
ui-settings-ctrl-v-never = Is sent to the session (Ctrl+Shift+V pastes)
ui-settings-ctrl-k = Ctrl+K in a terminal
ui-settings-ctrl-k-quick-connect = Opens Quick Connect
ui-settings-ctrl-k-send = Is sent to the session (Ctrl+Shift+K opens Quick Connect)
ui-connect-via = via { $route }
ui-tree-changed-move = Sessions moved.
ui-tree-changed-reorder = Sessions reordered.
ui-tree-changed-rename = Session renamed.
ui-tree-changed-folder-move = Folder moved.
ui-tree-changed-folder-rename = Folder renamed.
ui-tree-undo = Undo
ui-status-reordered-one = Moved { $name } within { $folder }
ui-status-reordered = Moved { $count } sessions within { $folder }
ui-status-undo-conflict = Cannot undo: the affected sessions or folders have changed since this action.
ui-tree-no-folder-zone = Drop here to take it out of its folder
ui-tree-no-folder-zone-tooltip = Drop a session or a folder here to take it out of its folder.
ui-tree-filter-chip-remove = ✕
ui-tree-filter-chip-tooltip = Remove filter: { $filter }
ui-tree-filter-result-count = { $shown } / { $total ->
    [one] { $total } session
   *[other] { $total } sessions
}
ui-tree-selection-count = { $count } sessions selected
ui-tree-selection-move = Move
ui-tree-selection-more = More actions
ui-tunnels-session-routes = Sessions through a gateway ({ $count })
ui-tunnels-session-route-local = -
ui-tunnels-session-route-local-tooltip = Carried inside Heimdall: no local port
ui-tunnels-close-all-tooltip = Closes the tunnels opened by hand; sessions through a gateway stay open
ui-tab-route-badge = via
ui-origin-rdp-file = Imported from RDP file
ui-origin-openssh = Imported from OpenSSH config
ui-origin-putty = Imported from PuTTY registry
ui-origin-mremoteng = Imported from mRemoteNG
ui-origin-mobaxterm = Imported from MobaXterm
ui-origin-rdcman = Imported from RDCMan
ui-desktop-shortcuts = Keyboard shortcuts...
ui-desktop-shares-clipboard = Clipboard
ui-desktop-shares-clipboard-tooltip = Clipboard redirection
ui-desktop-shares-drives = Drives
ui-desktop-shares-drives-tooltip = Drive redirection
ui-desktop-shares-audio = Sound
ui-desktop-shares-audio-tooltip = Audio redirection
ui-settings-rdp-connect-timeout = RDP connection watchdog timeout (0 = off)
ui-settings-rdp-resize-delay = Resolution stabilization delay after connect (0 = off)
ui-settings-rdp-resize-delay-refused = RDP resize delay must be zero or between { $min } and { $max } ms.
ui-settings-rdp-connect-timeout-off = Off
ui-settings-rdp-connect-timeout-seconds = { $seconds } s
ui-shortcuts-release-desktop = Give the keyboard back from a remote desktop
