[English version](README.md)

# Heimdall-rs

Réécriture en Rust de [Heimdall](https://github.com/VBlackJack/Heimdall), le
gestionnaire de connexions distantes multi-protocoles, pour Windows et Linux.

## État

> **Pas encore utilisable.** C'est une réécriture en cours, publiée pour que son travail
> puisse être suivi. Elle ne fait pas encore ce que fait Heimdall en C#, ses formats peuvent
> changer sans préavis, et elle n'est pas publiée en version. Utilisez
> [Heimdall](https://github.com/VBlackJack/Heimdall).

État au 2026-10-28. Ce qui suit est en place et testé automatiquement sous Linux et
Windows ; rien de cela n'est encore dans une version officielle.

- **Protocoles :** SSH (terminal, tunnels, rebonds, agents dont Pageant et l'agent OpenSSH de
  Windows, redirection X11) ; volets de fichiers SFTP et FTP/FTPS avec transferts, édition et
  mode sudo ; RDP (un client intégré avec presse-papiers, lecteurs et son, et un repli sur
  `mstsc.exe` sous Windows pour les options qui lui manquent) ; VNC ; Telnet ; WinRM
  (PowerShell à distance dans un ConPTY local) ; shell local ; Citrix (lancé dans Citrix
  Workspace, pas intégré) ; consoles web (ouvertes dans le navigateur du système, par le
  tunnel de la passerelle quand il y en a une).
- **Identifiants :** le coffre d'identifiants (`sealvault`), le magasin d'identifiants du
  système (Gestionnaire d'identification Windows, Secret Service), Windows Hello, les
  commandes de fournisseurs d'identifiants externes (préréglages KeePassXC, Bitwarden,
  1Password et pass), code PIN et verrouillage, ACL de dossiers.
- **Outils intégrés :** 57 ; Heimdall en C# en a 58, son Privilege Launcher n'est pas porté.
- **Bibliothèque de commandes :** les 527 actions livrées, les runbooks, l'historique, la
  capture depuis un terminal, l'import et l'export, la synchronisation Git.
- **Imports :** les données de Heimdall en C#, MobaXterm, mRemoteNG, RDCMan, les sessions
  PuTTY, la configuration OpenSSH, les fichiers `.rdp`, `known_hosts` ; les sources de
  sessions partagées MobaXterm, avec copie hors-ligne et rapport d'import.
- **Sessions :** une arborescence avec dossiers, couleurs, glisser-déposer, recherche et
  filtres, favoris et joignabilité ; volets partagés, fenêtres flottantes, onglets déplaçables ;
  macros avec étapes d'attente (expect) ; connexions planifiées ; journaux et transcriptions
  de session ; copie masquée ; capture d'écran ; connexion rapide (Ctrl+K) ; un partage de
  fichiers HTTP.
- **Apparence et langues :** 17 thèmes avec couleurs d'accent, jeux de couleurs du terminal et
  choix de la police, contraste élevé, animations réduites ; anglais, français et espagnol.
- **Instance séparée :** `--data-dir` en lance une à côté de celle de l'utilisateur.

Pas encore en place :

- Des artefacts de version signés et une première version officielle (#454, #514).
- L'intégration de la fenêtre Citrix (#449).
- Dans le client RDP intégré : cartes à puce, passerelle RD Gateway, microphone, imprimantes
  (#443 à #446).
- TFTP dans le partage de fichiers (#494).
- L'élévation dans l'onglet avec gsudo (#448).
- Les contrôles de révocation OCSP et CRL (#447).
- La détection de Sysinternals, NirSoft et NanaRun (#450).
- Les notes WYSIWYG et un canevas de diagrammes (#451, #452), par choix : il leur faudrait un
  moteur web embarqué.
- Les projets (#498).
- Les réglages de délais de Heimdall en C# (#499).
- La prise en charge des lecteurs d'écran : iced n'a pas encore d'intégration AccessKit.

Limite connue : les caractères absents de la police embarquée Source Code Pro (CJK, Braille)
sont dessinés avec ce que fournit le système.

Le nom visible de l'application est Heimdall-rs, jamais Heimdall seul, pour qu'elle tourne à
côté de Heimdall en C#.

L'interface est native, construite avec [iced](https://iced.rs), sans moteur web
embarqué.

Vous venez du Heimdall C# ? Voir [Passer de Heimdall (C#) à Heimdall-rs](docs/fr/migration-from-csharp.md).
Une équipe sur MobaXterm ? Voir [Sources de sessions partagées MobaXterm](docs/fr/MOBAXTERM-SHARED-SESSIONS.md).
Partager la bibliothèque de commandes par Git ? Voir [Bibliothèque de commandes : synchronisation Git](docs/fr/command-library-git-sync.md).
Alléger un poste Windows personnel ? Voir [Pack de nettoyage d'un poste Windows](docs/fr/windows-debloat-pack.md).

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
| `heimdall-twinshell` | Réservé, vide ; la bibliothèque de commandes est dans `heimdall-core` |
| `heimdall-remote` | VNC, Telnet, série |
| `heimdall-tls` | Vérification du certificat serveur de FTPS et de VNC sur TLS |
| `heimdall-dragout` | Fichiers locaux glissés vers l'Explorateur ; code unsafe, exception approuvée (appels au shell Windows) |
| `heimdall-titlebar` | Barres de titre aux couleurs du thème ; code unsafe, exception approuvée (un seul appel à `DwmSetWindowAttribute`, comme le `WindowThemeHelper` C#) |
| `heimdall-ui` | L'application de bureau |
| `xtask` | Outillage de développement |

Seul `heimdall-ui` dépend d'iced. Les crates de protocole ne dépendent que de
`heimdall-core` et `heimdall-net`.

## Outils intégrés

Heimdall-rs intègre 57 outils d'administration système et d'audit de sécurité, dans
les cinq catégories de Heimdall en C# (qui en a 58 : son Privilege Launcher n'est pas
porté). Tous ne sont pas hors-ligne : les outils réseau, comme l'Auditeur TLS, le
Vérificateur de sécurité DNS, l'Énumérateur SMB et l'Audit SecNumCloud, se connectent
aux hôtes que vous indiquez ; les autres fonctionnent entièrement hors-ligne. Les
outils réseau peuvent passer par une passerelle SSH.

La Recherche CVE utilise une base embarquée et constituée à la main de 68 entrées
(63 CVE distinctes) pour des logiciels d'infrastructure courants, couvrant 2015 à
2024. Elle ne contient aucune donnée CISA KEV (Known Exploited Vulnerabilities) et
n'est pas un flux en direct.

Le Scanner d'identifiants par défaut liste les identifiants par défaut connus des
fournisseurs pour un produit ou un service ; contrairement à l'outil C# d'origine,
il ne tente jamais de connexion, pour éviter les verrouillages de compte et les
tentatives de connexion non autorisées.

- **Réseau (17) :** Moniteur Ping, Recherche DNS, Inspecteur de certificat, Scanneur de ports, Calculateur de sous-réseau, Convertisseur d'adresses IP, Codes de statut HTTP, Recherche WHOIS, Analyseur d'en-têtes HTTP, Capture de bannières, Traceroute, SNMP Walker, Moniteur ARP, Testeur de pare-feu, Cartographie réseau, Calculateur réseau, TCP Ping.
- **Sécurité (15) :** Générateur de hachage, Générateur HMAC, Générateur de mots de passe, Générateur de clé SSH, Générateur de certificats, Analyseur JWT, Générateur TOTP, Auditeur de mot de passe, Auditeur de clé SSH, Auditeur TLS, Vérificateur de sécurité DNS (DNSSEC/DANE), Énumérateur SMB, Scanner d'identifiants par défaut, Recherche CVE, Audit SecNumCloud (ANSSI v3.2).
- **Encodage (6) :** Encodeur / Décodeur Base64, Encodeur / Décodeur URL, Formateur JSON, Testeur Regex, Comparaison de texte, Convertisseur de casse.
- **Système (13) :** Calculateur Chmod, Convertisseur Date/Heure, Générateur UUID, Générateur d'ULID, Constructeur Crontab, Générateur de config SSH, Éditeur du fichier hosts, Tableau de bord des services, Visionneuse de logs, Gestionnaire de tâches cron, Notes, Éditeur de diagrammes, Bibliothèque de commandes.
- **Externe (6) :** Wake-on-LAN, Résolution DNS en lot, Ports ouverts, Interfaces réseau, Table de routage, Réseaux WiFi.

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

Toute version, release comprise, lance une instance alternative (un test à la main, un profil
de démonstration) à côté de celle de l'utilisateur avec `--data-dir <chemin complet>`, comme
le Heimdall C# : sa propre configuration, ses données et ses mots de passe enregistrés sous ce
dossier. Voir
[`--data-dir` dans une version normale](docs/fr/test-data-folder.md#--data-dir-dans-une-version-normale).

Une release publiée est compilée avec `HEIMDALL_RELEASE` réglé sur son tag, par exemple
`v2026.100901` : la vérification des mises à jour le compare à la dernière release publiée
sur GitHub et propose une version plus récente. Une compilation sans elle, comme
`cargo run` ou les deux scripts, ne vérifie jamais.

```bash
cargo test --workspace
```

`cargo xtask catalogue-diff <chemin d'un dépôt Heimdall C#>` mesure l'écart entre le
catalogue de commandes livré, le pack de nettoyage et les actions d'origine du C# : il compare
chaque fichier en JSON, affiche ceux qui diffèrent ou n'existent que d'un côté avec leur
nombre, et se termine en erreur s'il y en a.

## Localisation

Chaque crate qui affiche du texte possède ses fichiers Fluent, dans
`i18n/<langue>/<crate>.ftl`. Les clés utilisent des tirets :
`module-composant-element-action`. Une clé inconnue fait échouer la
compilation, et un test échoue quand une langue n'a pas une clé que les autres
ont.

## Licence

Apache License 2.0, voir [LICENSE](LICENSE).
