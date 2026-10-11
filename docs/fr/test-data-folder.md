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
  la nomme ou demande `--all-features`.
- Ce n'est pas une variable d'environnement : celui qui lance le programme ne peut pas
  déplacer ses dossiers par l'environnement, avec ou sans la fonctionnalité.
- Une version sans la fonctionnalité ne lit aucun argument de ligne de commande.

## Compiler et lancer

Sous Windows, depuis le dépôt :

```powershell
cargo build --locked -p heimdall-ui --features test-data-dir
target\debug\heimdall-rs.exe --data-dir C:\Temp\heimdall-ui-test
```

Le dossier doit être un chemin complet ; il est créé s'il n'existe pas. `--data-dir=<dossier>`
est aussi accepté. Une version de test lancée sans `--data-dir`, avec un autre argument, ou
avec un dossier dont l'arborescence rencontrerait les vrais dossiers de Heimdall-rs ou du
Heimdall C#, affiche la raison et sort avec le code 2 : elle ne démarre jamais sur les vrais
dossiers.

Le titre de chaque fenêtre se termine par `[données de test : <dossier>]`, pour qu'une
version de test ne soit jamais prise pour la vraie application.

## Ce qui va dans le dossier

| Dossier | Ce qu'il remplace | Ce qu'il contient |
|---|---|---|
| `config` | `%APPDATA%\Heimdall-rs\config` | profils, réglages, coffre, `known_hosts`, épinglages et détails des clés d'hôte, instantané de session, état de la fenêtre, dispositions partagées, macros, le verrou d'instance unique |
| `data` | `%LOCALAPPDATA%\Heimdall-rs\data` | journaux et rapports de plantage, fichiers en cours d'édition |
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

## Mots de passe enregistrés

Les mots de passe enregistrés sans mot de passe maître, et l'enveloppe Windows Hello, vont
dans le magasin d'identifiants du système (Gestionnaire d'identification Windows, Secret
Service sous Linux). Sous un dossier de données de test, ils sont rangés sous le service
`Heimdall-rs-test-<hash>`, où `<hash>` est formé de 12 chiffres hexadécimaux du SHA-256 du
chemin complet du dossier (en minuscules sous Windows). Les vraies entrées, sous
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
