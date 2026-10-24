[Version française](fr/windows-debloat-pack.md)

# Windows workstation debloat pack

`data/packs/windows-workstation-debloat.json` holds 72 Command Library actions that strip a
personal Windows workstation (store apps, Edge, OneDrive, telemetry). They are aimed at a
workstation, not a server. It is the same file as the C# release v2026.102101 ships; its
source commit is recorded in `data/packs/README.md`.

- **Never loaded by default.** The file is not part of the library catalogue. Import it
  yourself: open the Command Library, press **Import** and pick the file from the `data/packs`
  folder next to the application.
- **Imported as an untrusted file.** Every action goes through the same checks as any import
  (control and invisible characters refused, lengths and counts bounded). A level in the file
  is the file's claim: a destructive command is marked dangerous whatever level it gives.
- **71 of the 72 actions import.** "Configurer exclusions Windows Defender" is refused: the
  default value of its path parameter holds parentheses (`Program Files (x86)`), which the
  hardened value generator does not accept. The file is not edited to get around it.
- **Nothing runs on import.** 13 actions are dangerous (12 marked so by the pack, one more
  because its command is on the destructive list of the paste guard). Before such an action is
  run it asks for a confirmation, as the paste guard does for any text reaching a shell; the
  others run as any library action does.
- **French only for now.** Titles and descriptions of the pack are in French; the commands
  are PowerShell and do not depend on the language.
