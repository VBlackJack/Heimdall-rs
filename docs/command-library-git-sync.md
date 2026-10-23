[Version française](fr/command-library-git-sync.md)

# Command Library: Git synchronization

The Command Library can share your own actions and their runbooks through a Git repository, so
a team keeps one library. Open the library, then **Git sync**.

## Settings

| Setting | Meaning |
|---|---|
| Repository | `https://`, `ssh://` or `file://` URL, or `user@host:path`. Nothing else is accepted (no `http`, no `git://`, no `ext::`). |
| Local folder | An absolute path, empty or already a clone of this repository. It is only a mirror: your library is never kept inside it. |
| Branch | Default `main`. |
| Author name, e-mail | Sign the commits. While they hold the shipped placeholder, the global Git identity (`git config --global user.name`, `user.email`) is offered. |

The settings are kept in `command-library-sync.json` beside the library.

## Pull, Push, Sync

- **Pull** reads `command-library.json` from the repository and merges it like an import: the
  file is untrusted and every action and runbook goes through the same checks. Nothing that is
  pulled is ever run, and a destructive command is marked dangerous whatever level the file
  gives it.
- **Push** writes your actions and runbooks to that file, commits and pushes. It is refused,
  with a message, when the repository has commits you have not pulled yet.
- **Sync** is a pull, a merge, then a push.

## Which copy wins

An action whose copy here is newer than the repository's (`updatedAt`) is kept as it is. A
runbook is compared with what it was when the repository last agreed with it, so running or
importing a runbook never counts as an edit. After a pull, the library says how many actions
and runbooks were kept and lists their titles; they travel to the repository with the next push.

## Security

- The installed `git` program is called, found on the `PATH`, with an argument list and never
  through a shell. If it is missing, the library says so.
- Only the `https`, `ssh` and `file` transports are allowed, hooks are disabled, no prompt
  waits for input and every call has a time limit. Sign in with your credential manager or your
  SSH agent beforehand.
- Credentials embedded in a URL are masked in every message.
