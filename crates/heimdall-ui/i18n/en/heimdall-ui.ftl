# Copyright 2026 Julien Bombled
# Licensed under the Apache License, Version 2.0. See LICENSE.

ui-window-title = Heimdall
ui-window-title-tab = { $tab } - Heimdall

ui-sidebar-title = Profiles
ui-sidebar-empty = No saved profile yet.
ui-sidebar-import-button = Import from Heimdall
ui-sidebar-group-none = Ungrouped

ui-home-welcome = Welcome to Heimdall-rs.
ui-home-hint = Pick a profile on the left to open an SSH session.

ui-tab-close-button = Close
ui-tab-bell-badge = bell

ui-connect-progress = Connecting to { $target }...
ui-connect-cancel-button = Cancel

ui-hostkey-title = Unknown server
ui-hostkey-body = This is the first connection to { $host } on port { $port }. Check that the fingerprint below is the server's before trusting it.
ui-hostkey-fingerprint = Fingerprint: { $fingerprint }
ui-hostkey-accept-button = Trust and connect
ui-hostkey-reject-button = Do not connect

ui-prompt-submit-button = Continue
ui-prompt-cancel-button = Cancel
ui-prompt-username-title = User name for { $target }
ui-prompt-password-title = Password for { $user } on { $target }
ui-prompt-password-retry = The password was refused. Try again.
ui-prompt-passphrase-title = Passphrase of the key { $path }
ui-prompt-passphrase-retry = The passphrase did not unlock the key. Try again.
ui-prompt-interactive-title = { $user } on { $host }: the server asks
ui-prompt-server-text = Server says: { $text }

ui-session-closed = The session ended.
ui-session-closed-status = The session ended (exit status { $status }).
ui-session-cancelled = The connection was cancelled.
ui-session-failed-title = The connection failed
ui-session-close-button = Close the tab

ui-error-invalid-host = The host name is not valid.
ui-error-network = The server could not be reached: { $detail }
ui-error-timeout = The server did not answer in time.
ui-error-hostkey-changed = The server's key is not the one on record. Someone may be intercepting the connection. Recorded: { $recorded }. Presented: { $offered }. If the change is expected, remove the old entry from Heimdall's known hosts file.
ui-error-hostkey-algorithm = The server no longer offers the key type on record ({ $recorded }).
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
ui-error-cancelled = Cancelled.
ui-error-prompt-timeout = A question was left unanswered for too long.
ui-error-pty-refused = The server refused to open a terminal.
ui-error-shell-refused = The server refused to start a shell.
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
ui-dialog-exit-title = Quit Heimdall?
ui-dialog-exit-body = { $count ->
    [one] One session is still open and will be disconnected.
   *[other] { $count } sessions are still open and will be disconnected.
}
ui-dialog-exit-confirm = Quit
ui-dialog-paste-title = Paste several lines?
ui-dialog-paste-body = { $count ->
    [one] The text holds one line. The shell may run it as a command as soon as it arrives.
   *[other] The text holds { $count } lines. The shell may run each one as a command as soon as it arrives.
}
ui-dialog-paste-confirm = Paste
ui-dialog-import-title = Import finished
ui-dialog-import-counts = Added: { $added }. Updated: { $updated }. Unchanged: { $unchanged }.
ui-dialog-import-skipped = Left out:
ui-dialog-import-skipped-item = { $name }: { $reason }
ui-dialog-import-failed-title = The import could not run
ui-dialog-store-title = The profile file could not be used
ui-dialog-store-body = Heimdall started with no profile; changes are saved beside the unreadable file, which is left untouched.
ui-dialog-detail = Detail: { $detail }

ui-import-skip-not-ssh = not an SSH profile ({ $kind })
ui-import-skip-jump-host = goes through an SSH gateway, not supported yet
ui-import-skip-missing-host = has no host
ui-import-skip-missing-id = has no identifier
ui-import-skip-invalid-port = invalid port { $port }
