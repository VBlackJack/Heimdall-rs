[English version](../test-data-folder.md)

# Dossier de données de test

Les tests d'interface automatisés pilotent une version debug de `heimdall-rs` sur un vrai
bureau. Ils ne doivent jamais lire ni modifier les profils, le coffre, le `known_hosts`, les
réglages, les journaux ou les mots de passe enregistrés de la personne dont c'est le bureau.
Une version compilée avec la fonctionnalité `test-data-dir` travaille dans un dossier jetable
à la place.

## Ce que c'est, et ce que ce n'est pas

- Une fonctionnalité Cargo, `test-data-dir`, déclarée par `heimdall-ui` (et transmise à
  `heimdall-app` et `heimdall-core`). Elle est **désactivée par défaut**.
- Elle **ne compile pas dans une version release** : `heimdall-core` arrête la compilation
  quand la fonctionnalité est active sans les assertions de debug.
- Aucun manifeste, workflow ou script de build ne peut l'activer : la garde de
  `xtask/tests/test_data_dir.rs` refuse une fonctionnalité par défaut, une autre
  fonctionnalité ou une déclaration de dépendance qui l'active, et tout workflow ou script qui
  la nomme ou demande `--all-features`. La seule exception est le job de CI
  `test-data-dir (debug)`, qui l'analyse et la teste, sans jamais compiler en release ni rien
  téléverser. La garde refuse aussi un profil release, ou un profil qui en hérite, qui active
  les assertions de debug.
- Elle échoue en position fermée : une version avec la fonctionnalité ne connaît aucun
  dossier de l'utilisateur et range les identifiants sous `Heimdall-rs-test-unset` tant
  qu'aucun dossier jetable n'est fixé, et l'application s'arrête avant toute lecture quand
  il n'y en a pas.
- Seul `heimdall_core::paths` demande où se trouve un dossier : `xtask/tests/folder_sources.rs`
  refuse `std::env::temp_dir`, `current_exe`, la crate `directories` et les dossiers connus
  partout ailleurs dans le code de l'application.
- Ce n'est pas une variable d'environnement : celui qui lance le programme ne peut pas
  déplacer ses dossiers par l'environnement, avec ou sans la fonctionnalité.
- Une version sans la fonctionnalité lit `--data-dir` aussi, comme autre chose : la racine
  de données d'une instance alternative, décrite plus bas dans
  [`--data-dir` dans une version normale](#--data-dir-dans-une-version-normale). Les deux ne
  coexistent jamais dans une même version.

## Compiler et lancer

Sous Windows, depuis le dépôt :

```powershell
cargo build --locked -p heimdall-ui --features test-data-dir
target\debug\heimdall-rs.exe --data-dir C:\Temp\heimdall-ui-test
```

Le dossier doit être un chemin complet ; il est créé s'il n'existe pas, avec son dossier
`home` : un vrai utilisateur en a toujours un, et le volet local de l'onglet Fichiers s'y
ouvre. `--data-dir=<dossier>` est aussi accepté ; cette forme doit être du texte. Une version de test lancée sans
`--data-dir`, avec un autre argument, ou avec un dossier refusé affiche la raison et sort
avec le code 2 : elle ne démarre jamais sur les vrais dossiers.

Le dossier est comparé aux vrais dossiers tels que le système de fichiers les résout : liens
suivis, `..` résolus, préfixe Windows `\\?\` retiré, sans tenir compte de la casse. Il ne
peut ni être, ni contenir, ni se trouver dans un dossier de Heimdall-rs ou du Heimdall C#, et
ne peut ni être ni contenir le dossier personnel, Documents, le dossier temporaire ou un
dossier de données d'application (il peut s'y trouver, comme
`C:\Users\<vous>\AppData\Local\Temp\heimdall-ui-test`). Il est vérifié avant d'être créé,
puis de nouveau une fois créé.

Le titre de chaque fenêtre se termine par `[données de test : <dossier>]`, pour qu'une
version de test ne soit jamais prise pour la vraie application.

## Ce qui va dans le dossier

| Dossier | Ce qu'il remplace | Ce qu'il contient |
|---|---|---|
| `config` | `%APPDATA%\Heimdall-rs\config` | profils, réglages, coffre, `known_hosts`, épinglages et détails des clés d'hôte, instantané de session, état de la fenêtre, dispositions partagées, macros, le verrou d'instance unique |
| `data` | `%LOCALAPPDATA%\Heimdall-rs\data` | journaux et rapports de plantage, fichiers en cours d'édition, historique des analyses (`network-scans`) et base de connaissances (`network-kb.json`) de la Cartographie réseau, les notes (`config\notes` : celles de l'outil Notes et celles créées depuis l'arborescence des sessions) |
| `legacy` | `%LOCALAPPDATA%\Heimdall` | les données du Heimdall C#, source de la migration |
| `home` | le dossier de profil de l'utilisateur | `.ssh\known_hosts` (import et export), `.Xauthority`, le départ de l'onglet Fichiers, le départ `.ssh` des boîtes de dialogue, le `~` d'un import de configuration OpenSSH, `.putty\sessions` |
| `local-app-data` | `%LOCALAPPDATA%` | le cache de Citrix Workspace lu par l'import Citrix |
| `roaming-app-data` | `%APPDATA%` | rien en propre : la racine de `config` hors test |
| `documents` | le dossier Documents | le chemin des modules PowerShell donné à un shell local |
| `temp` | le dossier temporaire du système | les fichiers de connexion de Connexion Bureau à distance, et leur nettoyage au démarrage |

Également sous un dossier de données de test :

- Les sessions PuTTY sont lues dans `home\.putty\sessions`, jamais dans le registre de
  l'utilisateur.
- L'ancien Heimdall PowerShell n'est pas cherché à côté du programme.
- L'inscription Windows Hello est désactivée : aucune clé n'est jamais créée.

Les dossiers communs à la machine (le dossier système, le dossier Windows, Program Files) ne
sont pas déplacés : ils contiennent les programmes de Windows, pas les données de
l'utilisateur.

Seul Heimdall-rs est déplacé. Les programmes qu'il lance (Connexion Bureau à distance,
PuTTY, les shells locaux, le client Citrix, un éditeur) et ce qu'il lit dans l'environnement
(l'agent SSH, le presse-papiers, `XAUTHORITY`) voient toujours l'environnement et les fichiers
du vrai utilisateur.

## Mots de passe enregistrés

Les mots de passe enregistrés sans mot de passe maître, et l'enveloppe Windows Hello, vont
dans le magasin d'identifiants du système (Gestionnaire d'identification Windows, Secret
Service sous Linux). Sous un dossier de données de test, ils sont rangés sous le service
`Heimdall-rs-test-<hash>`, où `<hash>` est formé de 12 chiffres hexadécimaux du SHA-256 du
chemin complet résolu du dossier, en minuscules. Les vraies entrées, sous
`Heimdall-rs`, ne sont jamais lues, remplacées ni supprimées.

L'identifiant d'un profil lu dans le Gestionnaire d'identification Windows (le fournisseur
"Gestionnaire d'identification Windows") est cherché sous
`Heimdall-rs-test-<hash>/<cible>` : un test crée les identifiants dont il a besoin sous ce
nom, par exemple
`cmdkey /generic:Heimdall-rs-test-<hash>/corp-admin /user:admin /pass:secret`.

## Nettoyage

Supprimer le dossier jetable. Retirer ensuite les identifiants de test du Gestionnaire
d'identification Windows ; chacun porte `Heimdall-rs-test-` dans son nom de cible :

```powershell
cmdkey /list |
    Select-String -Pattern 'Target:\s*(?:LegacyGeneric:target=)?(.*Heimdall-rs-test-.*)$' |
    ForEach-Object { cmdkey /delete:$($_.Matches[0].Groups[1].Value.Trim()) }
```

Sous Linux, supprimer les éléments du Secret Service dont l'attribut de service commence par
`Heimdall-rs-test-` (par exemple avec Seahorse, ou `secret-tool clear service <nom>`).

## `--data-dir` dans une version normale

Toute version, release comprise, accepte `--data-dir <chemin complet>` (ou
`--data-dir=<chemin complet>`) pour lancer une instance alternative, un test à la main ou un
profil de démonstration, à côté de celle de l'utilisateur, comme le fait `--data-dir` du
Heimdall C#. Ce n'est pas le bac à sable décrit plus haut :

- Elle déplace les dossiers propres à l'application : `config` (profils, réglages, coffre,
  `known_hosts`, état des fenêtres, verrou d'instance unique) et `data` (journaux, fichiers en
  cours d'édition, historique des analyses) sous le dossier donné. L'instance ne partage
  jamais le profil de l'utilisateur et tourne à côté de lui.
- Les mots de passe enregistrés sans mot de passe maître vont dans le magasin d'identifiants
  du système sous le service `Heimdall-rs-data-<hash>`, `<hash>` étant 12 chiffres
  hexadécimaux du SHA-256 du chemin complet résolu du dossier, en minuscules : l'instance ne
  lit, ne remplace ni ne supprime jamais les entrées de l'utilisateur sous `Heimdall-rs`. (Le
  C# les garde dans `servers.json` sous sa racine de données.)
- L'enrôlement Windows Hello est désactivé : l'identifiant de clé appartient à l'utilisateur,
  hors de tout dossier.
- Les dossiers personnel, Documents et temporaire restent ceux de l'utilisateur, comme dans le
  C# : l'instance lit le vrai `~/.ssh/known_hosts` pour un import, les sessions PuTTY et les
  données du Heimdall C# pour une migration. Une entrée du Gestionnaire d'identifiants Windows
  nommée par un profil est seulement lue, comme dans le C#.
- Le dossier est vérifié comme le dossier de données de test : un chemin complet, créé s'il
  n'existe pas, qui n'est ni ne contient ni ne se trouve dans un dossier de Heimdall-rs ou du
  Heimdall C#, et qui n'est ni ne contient le dossier personnel, Documents, temporaire ou de
  données d'application.
- Le nom de l'option est lu sans tenir compte de la casse, et le dossier débarrassé des
  espaces et des guillemets, comme le C# les lit. Là où le C# se rabat sur les dossiers de
  l'utilisateur, cette version refuse de démarrer, avec une boîte de message et le code de
  sortie 2 : un dossier absent ou refusé, un chemin relatif, ou une option dont le nom
  commence seulement comme celle-ci (`--data-directory`).
- Le titre de chaque fenêtre se termine par `[données : <dossier>]`.
