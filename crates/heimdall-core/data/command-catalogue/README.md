# Shipped command catalogue

[Français](README.fr.md)

`actions/*.json` is the command library's catalogue, copied **verbatim** from the C# Heimdall
(`data/seed/actions/*.json`, 527 files). Each file is one action in the C# `Action` JSON
format; the English text is the pivot, `translations.fr` holds the French and `translations.es`
the Spanish. A part a language does not translate (the Spanish holds titles and descriptions
only) shows the English text, as the C# `ActionLocalizer` does. Categories are stable keys (`active-directory`),
their labels are written by the window in its language.

The files are embedded at build time (`build.rs` lists them, `src/tools/command_library.rs`
reads them). Nothing is downloaded at run time, and nothing in a file is ever run: a command is
only built as text, from the values the user types, and copied or sent after the user's own
question.

## Source of this copy

| What | Value |
|---|---|
| Repository | C# Heimdall (`G:\_Projects\Heimdall`, `VBlackJack/Heimdall`) |
| Commit | `edb753df5da6508418d9da5bcdbc71ac87a5f90f` (master on 2026-10-28: the seed of v2026.102702 plus the Spanish texts and the later fixes) |
| Folder | `data/seed/actions` |
| Files | 527 |
| Also copied | `data/packs/windows-workstation-debloat.json`, same commit, to `data/packs` at the root of the workspace (an optional import, not part of this catalogue) |

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
5. Run `cargo xtask catalogue-diff <C# checkout>`: it lists each file that still differs from
   the C# seed, the debloat pack included, and fails when there is one.
