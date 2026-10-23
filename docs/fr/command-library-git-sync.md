[English version](../command-library-git-sync.md)

# Bibliothèque de commandes : synchronisation Git

La bibliothèque de commandes peut partager vos propres actions et leurs runbooks par un dépôt
Git, pour qu'une équipe garde une seule bibliothèque. Ouvrez la bibliothèque, puis **Synchro Git**.

## Réglages

| Réglage | Sens |
|---|---|
| Dépôt | URL `https://` ou `ssh://`, ou `utilisateur@hôte:chemin`. Rien d'autre n'est accepté (ni `http`, ni `git://`, ni `file://`, ni `ext::`). Une URL https ne peut pas porter de nom d'utilisateur ni de jeton : connectez-vous par votre gestionnaire d'identifiants. |
| Branche | `main` par défaut. |
| Nom et e-mail de l'auteur | Signent les commits. Tant qu'ils portent la valeur d'origine, l'identité Git globale (`git config --global user.name`, `user.email`) est proposée. |

Les réglages sont conservés dans `command-library-sync.json`, à côté de la bibliothèque. Heimdall garde sa propre copie du dépôt dans un dossier qu'il y crée et marque comme sien ; il n'utilise jamais un dossier que vous avez choisi ni un dépôt déjà présent, et il refait la copie si vous changez de dépôt ou de branche.

## Récupérer, Envoyer, Synchroniser

- **Récupérer** lit `command-library.json` dans le dépôt et le fusionne comme un import : le
  fichier n'est pas fiable et chaque action et chaque runbook passent par les mêmes contrôles.
  Rien de ce qui est récupéré n'est jamais exécuté, et une commande destructrice est marquée
  dangereuse quel que soit le niveau indiqué par le fichier.
- **Envoyer** écrit vos actions et vos runbooks dans ce fichier, valide et pousse. L'envoi est
  refusé, avec un message, quand le dépôt a des commits que vous n'avez pas encore récupérés.
- **Synchroniser** enchaîne une récupération, une fusion, puis un envoi.

## Quelle copie l'emporte

Une action dont la copie ici est plus récente que celle du dépôt (`updatedAt`) est conservée
telle quelle. Un runbook est comparé à ce qu'il était quand le dépôt a été d'accord avec lui
pour la dernière fois : l'exécuter ou l'importer ne compte donc jamais comme une modification.
Après une récupération, la bibliothèque dit combien d'actions et de runbooks ont été conservés et
en liste les titres ; ils partent vers le dépôt à l'envoi suivant.

## Sécurité

- Le programme `git` installé est appelé, trouvé dans le `PATH`, avec une liste d'arguments et
  jamais par un interpréteur de commandes. S'il est absent, la bibliothèque le dit.
- Seuls les transports `https` et `ssh` sont autorisés, les hooks sont désactivés, rien de ce
  qui est récupéré n'est extrait (le fichier est lu dans les objets de Git), aucune invite
  n'attend de saisie et chaque appel a une durée limite. Un appel qui dépasse sa durée est
  arrêté ; un processus qu'il a lancé (comme `ssh`) ne l'est pas et se termine de lui-même.
  Une seule synchronisation s'exécute à la fois, d'une fenêtre Heimdall à l'autre. Connectez-vous avant avec
  votre gestionnaire d'identifiants ou votre agent SSH.
- Les identifiants intégrés à une URL sont masqués dans tous les messages.
