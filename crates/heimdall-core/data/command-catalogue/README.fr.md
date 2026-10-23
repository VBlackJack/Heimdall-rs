# Catalogue de commandes livré

[English](README.md)

`actions/*.json` est le catalogue de la bibliothèque de commandes, copié **tel quel** depuis le
Heimdall C# (`data/seed/actions/*.json`, 527 fichiers). Chaque fichier est une action au format
JSON `Action` du C# ; le texte anglais est le pivot et `translations.fr` porte le français.
L'espagnol affiche l'anglais, comme le `ActionLocalizer` du C#. Les catégories sont des clés
stables (`active-directory`), leurs libellés sont écrits par la fenêtre dans sa langue.

Les fichiers sont intégrés à la compilation (`build.rs` les liste, `src/tools/command_library.rs`
les lit). Rien n'est téléchargé à l'exécution, et rien dans un fichier n'est jamais exécuté :
une commande n'est construite que comme du texte, à partir des valeurs saisies, puis copiée ou
envoyée après la question posée à l'utilisateur.

## Origine de cette copie

| Quoi | Valeur |
|---|---|
| Dépôt | Heimdall C# (`G:\_Projects\Heimdall`, `VBlackJack/Heimdall`) |
| Commit | `6a400691b8cfe8588e0dd59a2146c8a6d1132c22` (`release: v2026.102201`) |
| Dossier | `data/seed/actions` |
| Fichiers | 527 |
| Pas encore copié | `data/packs/windows-workstation-debloat.json` (un changement ultérieur) |

## Synchroniser avec le catalogue C#

Une synchronisation est une copie de fichiers, sans liste Rust à modifier :

1. Exporter le dossier au commit C# voulu : `git archive <commit> data/seed/actions`.
2. Remplacer `actions/` par cet export (git stocke ici les fichiers avec des fins de ligne LF).
3. Mettre à jour le commit dans le tableau ci-dessus.
4. Lancer `cargo test -p heimdall-core command_library`. Le test de schéma vérifie chaque
   fichier (format, identités uniques, catégories connues, types de paramètres pris en charge)
   et que chaque action construit sa commande depuis ses valeurs par défaut avec le générateur.
   Les modèles que le générateur Rust refuse volontairement sont listés dans le test
   (`KNOWN_REFUSALS`) : un nouveau refus, ou un refus listé qui est accepté depuis, fait
   échouer le test.
