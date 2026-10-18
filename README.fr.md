[English version](README.md)

# Heimdall-rs

Réécriture en Rust de [Heimdall](https://github.com/VBlackJack/Heimdall), le
gestionnaire de connexions distantes multi-protocoles, pour Windows et Linux.

## État

> **Pas encore utilisable.** C'est une réécriture en cours, publiée pour que son travail
> puisse être suivi. Elle ne fait pas encore ce que fait Heimdall en C#, ses formats peuvent
> changer sans préavis, et elle n'est pas publiée en version. Utilisez
> [Heimdall](https://github.com/VBlackJack/Heimdall).

Développement en cours. Le premier jalon est atteint : un onglet de terminal SSH ouvert
depuis un profil enregistré, avec les profils importés depuis Heimdall en C#, la
vérification des clés d'hôte, l'authentification par mot de passe, fichier de clé (OpenSSH
et PuTTY), agent et keyboard-interactive, et un terminal avec historique, sélection,
copier-coller et remontée de la souris. Il est testé automatiquement sous Linux et Windows,
et a été parcouru à la main sur les deux le 2026-09-26. En cours : SFTP.

Limites connues : les caractères absents de la police embarquée Source Code Pro (CJK,
Braille) sont dessinés avec ce que fournit le système ; pas de prise en charge des lecteurs
d'écran, iced n'en a pas encore.

La cible est la parité fonctionnelle avec Heimdall en C# : SSH, SFTP, FTP/FTPS, RDP, VNC,
Telnet, Citrix, WinRM, shell local, coffre d'identifiants, bibliothèque de commandes,
éditeur de diagrammes, mise à jour, et localisation en anglais, français et espagnol.

L'interface est native, construite avec [iced](https://iced.rs), sans moteur web
embarqué.

## Organisation

| Crate | Rôle |
|---|---|
| `heimdall-core` | Profils de serveurs, réglages, chemins, coffre d'identifiants, moteurs d'outils |
| `heimdall-app` | État de l'application, cycle de vie des onglets, gestionnaire de sessions, registre d'outils |
| `heimdall-i18n` | Langues prises en charge et contrôle de leur complétude |
| `heimdall-net` | Connexions TCP à un hôte par son nom, Happy Eyeballs (RFC 8305) |
| `heimdall-ssh` | Sessions SSH, tunnels, rebonds |
| `heimdall-sftp` | SFTP, FTP et FTPS |
| `heimdall-term` | Émulation de terminal et pseudo-terminaux locaux |
| `heimdall-rdp` | Sessions RDP |
| `heimdall-twinshell` | Bibliothèque de commandes |
| `heimdall-remote` | VNC, Telnet, série |
| `heimdall-tls` | Vérification du certificat serveur de FTPS et de VNC sur TLS |
| `heimdall-dragout` | Fichiers locaux glissés vers l'Explorateur, seul code unsafe (appels au shell Windows) |
| `heimdall-ui` | L'application de bureau |
| `xtask` | Outillage de développement |

Seul `heimdall-ui` dépend d'iced. Les crates de protocole ne dépendent que de
`heimdall-core` et `heimdall-net`.

## Outils intégrés

Heimdall intègre 30 outils d'administration système et d'audit de sécurité. Tous
ne sont pas hors-ligne : les outils réseau, comme l'Auditeur SSL/TLS, le
Vérificateur de sécurité DNS, l'Énumérateur SMB et l'Auditeur SecNumCloud, se
connectent aux hôtes que vous indiquez ; les autres fonctionnent entièrement
hors-ligne.

La Recherche CVE utilise une base embarquée et constituée à la main de 68 entrées
(63 CVE distinctes) pour des logiciels d'infrastructure courants, couvrant 2015 à
2024. Elle ne contient aucune donnée CISA KEV (Known Exploited Vulnerabilities) et
n'est pas un flux en direct.

Le Scanner d'identifiants par défaut liste les identifiants par défaut connus des
fournisseurs pour un produit ou un service ; contrairement à l'outil C# d'origine,
il ne tente jamais de connexion, pour éviter les verrouillages de compte et les
tentatives de connexion non autorisées.

- **Sécurité & Conformité :** Auditeur de clés SSH, Auditeur SSL/TLS, Vérificateur de sécurité DNS (DNSSEC/DANE), Audit de conformité ANSSI SecNumCloud v3.2, Énumérateur SMB & NetBIOS, Scanner d'identifiants par défaut, Recherche hors-ligne CVE, Auditeur de mots de passe, Générateur de mots de passe, Générateur de clés SSH, Générateur de certificats.
- **Réseau :** Calculateur de sous-réseau, Convertisseur IP, Calculateur réseau.
- **Système :** Calculateur Chmod, Convertisseur date et heure, Générateur Crontab, Générateur de configuration SSH.
- **Encodage & Jetons :** Calculateur de hachage, Générateur HMAC, Débogueur JWT, Générateur TOTP, Encodeur/Décodeur Base64, Encodeur/Décodeur URL, Formateur JSON, Testeur d'expressions régulières, Comparateur de texte (Diff), Convertisseur de casse, Générateur UUID, Générateur ULID.

## Compilation

Sous Linux, la compilation par défaut ne demande aucun paquet de développement C,
seulement un compilateur C (`gcc` ou `clang`, pour `ring`) : les bibliothèques de
fenêtrage sont ouvertes à l'exécution, pas liées. Le son RDP y est désactivé par
défaut ; une compilation qui le demande avec `--features audio`
(`cargo build -p heimdall-ui --features audio`) lie ALSA et demande `alsa-lib-devel`
(Fedora, RHEL, AlmaLinux) ou `libasound2-dev` (Debian, Ubuntu). Sous Windows, la
compilation demande les outils MSVC, et le son RDP est toujours compilé.

L'exécution sous Linux demande les bibliothèques que le bureau charge à l'exécution :
`libxkbcommon`, `libxkbcommon-x11` et les bibliothèques client Wayland ou X11, ainsi
qu'un pilote Vulkan ou OpenGL, par exemple Mesa (sans lui, la fenêtre est dessinée en
logiciel). Leur liste, et les cas où chacune est chargée, sont dans
[Bibliothèques natives](docs/fr/native-libraries.md).

```bash
cargo run --package heimdall-ui
```

Sous Windows, deux scripts à la racine la compilent et la lancent pour les tests à la
main, puis affichent la fin de son journal (`%LOCALAPPDATA%\Heimdall-rs\data\logs\heimdall.log`)
une fois fermée :

- `run-debug.bat` : compilation de débogage, avec sa console, où s'affichent les paniques ;
  niveau de journal `debug`.
- `run-release.bat` : compilation de publication, telle qu'un utilisateur la lance, sans
  console ; niveau `info`.

Les deux prennent un niveau de journal en argument, de `error` à `trace` :
`run-debug.bat trace`.

Les tests d'interface automatisés lancent une version debug compilée avec la fonctionnalité
`test-data-dir` sur un dossier jetable, jamais sur les vrais profils, le coffre ou les mots
de passe enregistrés : voir [Dossier de données de test](docs/fr/test-data-folder.md).

Une release publiée est compilée avec `HEIMDALL_RELEASE` réglé sur son tag, par exemple
`v2026.100901` : la vérification des mises à jour le compare à la dernière release publiée
sur GitHub et propose une version plus récente. Une compilation sans elle, comme
`cargo run` ou les deux scripts, ne vérifie jamais.

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
