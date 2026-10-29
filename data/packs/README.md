[Version française](../../docs/fr/windows-debloat-pack.md)

# Optional action packs

Files here are **never loaded by default**. They are not part of the Command Library
catalogue; you import one yourself with the library's **Import** button.

| File | Actions | Source |
|---|---|---|
| `windows-workstation-debloat.json` | 72 | Heimdall (C#), `data/packs/windows-workstation-debloat.json`, commit `225d7143fbb8e20a9bae0fa47fea13cb9b560205` (2026-10-28, "fix(cmdlib): write the debloat pack in English with fr and es blocks"), later than the C# release v2026.102702 |

Importing it keeps 71 of the 72 actions: one is refused by the import's value rules (see the
debloat pack note). 13 actions are dangerous after the import. Until the releases package this
folder, the file is only in the repository (this folder); once they do, it will ship in a
`data/packs` folder next to the application.

The pack is copied from the C# repository without any change apart from the line ends (LF in the repository; the C# file has CRLF). Its text is English, with French and Spanish under each action's `translations`.
See [the debloat pack note](../../docs/windows-debloat-pack.md).
