[English version](../windows-debloat-pack.md)

# Pack de nettoyage d'un poste Windows

`data/packs/windows-workstation-debloat.json` contient 72 actions de la bibliothèque de
commandes qui allègent un poste Windows personnel (applications du Store, Edge, OneDrive,
télémétrie). Elles visent un poste de travail, pas un serveur. C'est le même fichier que
celui de la version C# v2026.102101 ; le commit d'origine est noté dans
`data/packs/README.md`.

- **Jamais chargé par défaut.** Le fichier ne fait pas partie du catalogue de la
  bibliothèque. Importez-le vous-même : ouvrez la bibliothèque de commandes, appuyez sur
  **Importer** et choisissez le fichier dans le dossier `data/packs` à côté de l'application.
- **Importé comme un fichier non fiable.** Chaque action passe les mêmes contrôles que
  n'importe quel import (caractères de contrôle et invisibles refusés, longueurs et nombres
  bornés). Un niveau écrit dans le fichier est une déclaration du fichier : une commande
  destructive est marquée dangereuse quel que soit le niveau indiqué.
- **71 actions sur 72 sont importées.** "Configurer exclusions Windows Defender" est refusée :
  la valeur par défaut de son paramètre de chemin contient des parenthèses
  (`Program Files (x86)`), que le générateur de valeurs renforcé n'accepte pas. Le fichier
  n'est pas modifié pour contourner cela.
- **Rien ne s'exécute à l'import.** 13 actions sont dangereuses (12 le sont dans le pack, une
  de plus parce que sa commande figure dans la liste des commandes destructrices de la garde
  de collage). Avant d'exécuter l'une d'elles, une confirmation est demandée, comme la garde
  de collage pour tout texte qui atteint un shell ; les autres s'exécutent comme toute action
  de la bibliothèque.
- **En français seulement pour l'instant.** Les titres et descriptions du pack sont en
  français ; les commandes sont du PowerShell et ne dépendent pas de la langue.
