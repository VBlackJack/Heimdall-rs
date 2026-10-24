[English version](../windows-debloat-pack.md)

# Pack de nettoyage d'un poste Windows

`data/packs/windows-workstation-debloat.json` contient 72 actions de la bibliothèque de
commandes qui allègent un poste Windows personnel (applications du Store, Edge, OneDrive,
télémétrie). Elles visent un poste de travail, pas un serveur. C'est le même fichier que
celui de la version C# v2026.102101 ; le commit d'origine est noté dans
`data/packs/README.md`.

- **Où est le fichier.** Aujourd'hui il se trouve dans le dépôt, à
  `data/packs/windows-workstation-debloat.json` ; téléchargez-le de là. Il sera livré dans un
  dossier `data/packs` à côté de l'application quand les versions publiées embarqueront ce
  dossier ; d'ici là il n'est pas installé avec Heimdall-rs.
- **Jamais chargé par défaut.** Le fichier ne fait pas partie du catalogue de la
  bibliothèque. Importez-le vous-même : ouvrez la bibliothèque de commandes, appuyez sur
  **Importer** et choisissez le fichier.
- **Importé comme un fichier non fiable.** Chaque action passe les mêmes contrôles que
  n'importe quel import (caractères de contrôle et invisibles refusés, longueurs et nombres
  bornés). Un niveau écrit dans le fichier est une déclaration du fichier : une commande
  destructive est marquée dangereuse quel que soit le niveau indiqué.
- **71 actions sur 72 sont importées.** "Configurer exclusions Windows Defender" est refusée :
  la valeur par défaut de son paramètre de chemin contient des parenthèses
  (`Program Files (x86)`), que le générateur de valeurs renforcé n'accepte pas. Le fichier
  n'est pas modifié pour contourner cela.
- **Rien ne s'exécute à l'import, et chaque envoi est confirmé.** **Envoyer** une action du
  pack affiche d'abord une confirmation avec la commande complète ; une action dangereuse y
  ajoute un avertissement rouge. 13 actions sont dangereuses (12 le sont dans le pack, une de
  plus parce que sa commande figure dans la liste des commandes destructrices de la garde de
  collage). **Insérer** tape la commande sans appuyer sur Entrée : rien ne s'exécute avant que
  vous le fassiez.
- **En français seulement pour l'instant.** Les titres et descriptions du pack sont en
  français ; les commandes sont du PowerShell et ne dépendent pas de la langue.
