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
ui-prompt-server-password-title = Password of the VNC server { $target }
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
ui-dialog-exit-title = Quit Heimdall?
ui-dialog-exit-body = { $count ->
    [one] One session is still open and will be disconnected.
   *[other] { $count } sessions are still open and will be disconnected.
}
ui-dialog-exit-confirm = Quit
ui-dialog-overwrite-title = Replace the file?
ui-dialog-overwrite-local-body = { $name } already exists in the local folder. The download replaces it.
ui-dialog-overwrite-remote-body = { $name } already exists on the server. The upload replaces it.
ui-dialog-overwrite-confirm = Replace
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
ui-import-skip-rd-gateway = goes through a Remote Desktop Gateway, not supported yet
ui-import-skip-missing-host = has no host
ui-import-skip-missing-id = has no identifier
ui-import-skip-invalid-port = invalid port { $port }

ui-sidebar-files-button = Files
ui-tab-files-title = { $name } (files)

ui-files-local-title = This computer
ui-files-remote-title = Server
ui-files-up-button = Up
ui-files-refresh-button = Refresh
ui-files-new-folder-button = New folder
ui-files-rename-button = Rename
ui-files-delete-button = Delete
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
    [one] 1 entry left out (a link, an unusable name or a failure)
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

ui-sidebar-new-profile-button = New profile
ui-sidebar-edit-button = Edit
ui-profile-new-title = New profile
ui-profile-edit-title = Edit profile
ui-profile-field-name = Name
ui-profile-field-group = Group
ui-profile-field-host = Server address
ui-profile-field-port = Port
ui-profile-field-username = User name
ui-profile-field-key = Private key file
ui-profile-optional = optional
ui-profile-host-placeholder = server.example.org
ui-profile-save-button = Save
ui-profile-delete-button = Delete this profile
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

ui-sidebar-rdp-target = RDP { $target }
ui-sidebar-telnet-target = Telnet { $target }
ui-sidebar-vnc-target = VNC { $target }
ui-session-forget-server-button = Forget this server
ui-error-security-refused = The server refused the security Heimdall requires (Network Level Authentication): { $detail }
ui-error-rdp-protocol = RDP error: { $detail }
ui-error-vnc-protocol = VNC error: { $detail }
ui-session-vnc-unencrypted = Not encrypted: this desktop and what you type cross the network in clear.
ui-sidebar-local-shell-button = Local shell
ui-local-shell-name = Local shell
ui-local-starting = Starting { $name }...
ui-error-local-shell = The local shell could not be started: { $detail }
