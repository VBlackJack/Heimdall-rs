# Shipped command catalogue

[Français](README.fr.md)

`actions/*.json` is the command library's catalogue, copied **verbatim** from the C# Heimdall
(`data/seed/actions/*.json`, 527 files). Each file is one action in the C# `Action` JSON
format; the English text is the pivot and `translations.fr` holds the French. Spanish shows the
English text, as the C# `ActionLocalizer` does. Categories are stable keys (`active-directory`),
their labels are written by the window in its language.

The files are embedded at build time (`build.rs` lists them, `src/tools/command_library.rs`
reads them). Nothing is downloaded at run time, and nothing in a file is ever run: a command is
only built as text, from the values the user types, and copied or sent after the user's own
question.

## Source of this copy

| What | Value |
|---|---|
| Repository | C# Heimdall (`G:\_Projects\Heimdall`, `VBlackJack/Heimdall`) |
| Commit | `6a400691b8cfe8588e0dd59a2146c8a6d1132c22` (`release: v2026.102201`) |
| Folder | `data/seed/actions` |
| Files | 527 |
| Not copied yet | `data/packs/windows-workstation-debloat.json` (a later change) |

## Syncing with the C# catalogue

A sync is a copy of the files, with no Rust list to edit:

1. Export the folder at the C# commit wanted: `git archive <commit> data/seed/actions`.
2. Replace `actions/` with it (git stores the files with LF line ends here).
3. Update the commit in the table above.
4. Run `cargo test -p heimdall-core command_library`. The schema test holds every file to the
   format, unique identities, known categories and supported parameter types, and checks that
   each action builds its command from its defaults through the generator. The templates the
   Rust generator refuses on purpose are listed in the test (`KNOWN_REFUSALS`); a new refusal,
   or a listed one that is accepted since, fails the test.
