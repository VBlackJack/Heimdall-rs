[English version](README.md)

# Heimdall-rs

Réécriture en Rust de [Heimdall](https://github.com/VBlackJack/Heimdall), le
gestionnaire de connexions distantes multi-protocoles, pour Windows et Linux.

## État

Développement en cours. Le premier jalon est atteint : un onglet de terminal SSH ouvert
depuis un profil enregistré, avec les profils importés depuis Heimdall en C#, la
vérification des clés d'hôte, l'authentification par mot de passe, fichier de clé (OpenSSH
et PuTTY), agent et keyboard-interactive, et un terminal avec historique, sélection,
copier-coller et remontée de la souris. Il est testé automatiquement sous Linux et Windows,
et a été parcouru à la main sur les deux le 2026-09-26. En cours : SFTP.

Limites connues : les caractères absents de la police embarquée Source Code Pro (CJK,
Braille) sont dessinés avec ce que fournit le système ; pas de prise en charge des lecteurs
d'écran, iced n'en a pas encore ; les profils sont importés ou écrits dans le fichier des
profils, pas encore modifiables dans la fenêtre.

La cible est la parité fonctionnelle avec Heimdall en C# : SSH, SFTP, FTP/FTPS, RDP, VNC,
Telnet, Citrix, WinRM, shell local, coffre d'identifiants, bibliothèque de commandes,
éditeur de diagrammes, mise à jour, et localisation en anglais, français et espagnol.

L'interface est native, construite avec [iced](https://iced.rs), sans moteur web
embarqué.

## Organisation

| Crate | Rôle |
|---|---|
| `heimdall-core` | Profils de serveurs, réglages, chemins, coffre d'identifiants |
| `heimdall-i18n` | Langues prises en charge et contrôle de leur complétude |
| `heimdall-ssh` | Sessions SSH, tunnels, rebonds |
| `heimdall-sftp` | SFTP, FTP et FTPS |
| `heimdall-term` | Émulation de terminal et pseudo-terminaux locaux |
| `heimdall-rdp` | Sessions RDP |
| `heimdall-twinshell` | Bibliothèque de commandes |
| `heimdall-remote` | VNC, Telnet, série |
| `heimdall-ui` | L'application de bureau |
| `xtask` | Outillage de développement |

Seul `heimdall-ui` dépend d'iced. Les crates de protocole ne dépendent que de
`heimdall-core`.

## Compilation

Sous Linux, la compilation demande les paquets de développement de xkbcommon,
Wayland, X11 et fontconfig. L'exécution demande aussi `libxkbcommon-x11` et un
pilote Vulkan ou OpenGL, par exemple Mesa. Sous Windows, la compilation demande
les outils MSVC.

```bash
cargo run --package heimdall-ui
```

```bash
cargo test --workspace
```

## Localisation

Chaque crate qui affiche du texte possède ses fichiers Fluent, dans
`i18n/<langue>/<crate>.ftl`. Les clés utilisent des tirets :
`module-composant-element-action`. Une clé inconnue fait échouer la
compilation, et un test échoue quand une langue n'a pas une clé que les autres
ont.

## Licence

Apache License 2.0, voir [LICENSE](LICENSE).
