[Version française](fr/windows-debloat-pack.md)

# Windows workstation debloat pack

`data/packs/windows-workstation-debloat.json` holds 72 Command Library actions that strip a
personal Windows workstation (store apps, Edge, OneDrive, telemetry). They are aimed at a
workstation, not a server. It is the C# repository's file, as rewritten in English after the
C# release v2026.102702; its source commit is recorded in `data/packs/README.md`.

- **Where the file is.** Today it lives in the repository, at
  `data/packs/windows-workstation-debloat.json`; download it from there. It will ship in a
  `data/packs` folder next to the application once the releases package that folder; until
  then it is not installed with Heimdall-rs.
- **Never loaded by default.** The file is not part of the library catalogue. Import it
  yourself: open the Command Library, press **Import** and pick the file.
- **Imported as an untrusted file.** Every action goes through the same checks as any import
  (control and invisible characters refused, lengths and counts bounded). A level in the file
  is the file's claim: a destructive command is marked dangerous whatever level it gives.
- **71 of the 72 actions import.** "Configure Windows Defender exclusions" is refused: the
  default value of its path parameter holds parentheses (`Program Files (x86)`), which the
  hardened value generator does not accept. The file is not edited to get around it.
- **Nothing runs on import, and every send is confirmed.** **Send** of any action of the pack
  shows a confirmation with the full command first; a dangerous action adds a red warning to
  it. 13 actions are dangerous (12 marked so by the pack, one more because its command is on
  the destructive list of the paste guard). **Insert** types the command without pressing
  Enter, so nothing runs until you do.
- **English, with French and Spanish carried.** Titles and descriptions of the pack are in
  English, with French and Spanish under each action's `translations`. The import keeps them,
  and the pack follows the library's language like the shipped actions. The commands are
  PowerShell and do not depend on the language.
