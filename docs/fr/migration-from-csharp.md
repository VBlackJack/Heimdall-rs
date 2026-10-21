[English version](../migration-from-csharp.md)

# Passer de Heimdall (C#) à Heimdall-rs

Heimdall-rs lit les données du [Heimdall](https://github.com/VBlackJack/Heimdall) C# et
garde le comportement du C# partout où c'est possible. Ce guide dit comment reprendre vos
connexions, ce qui ne passe pas, et les endroits où Heimdall-rs diffère volontairement.

## Importer vos connexions

1. Le Heimdall C# peut rester installé et garder ses données : Heimdall-rs ne fait que les
   lire.
2. Lancez Heimdall-rs. Sur la page d'accueil, **Import Connections** est actif quand les
   données du C# sont trouvées : `servers.json` (et `settings.json` à côté) dans
   `%LOCALAPPDATA%\Heimdall`. Il est grisé quand ce fichier n'existe pas.
3. Cliquez dessus. Une boîte de dialogue indique ce qui a été ajouté, mis à jour et laissé
   de côté, avec la raison pour chaque profil ignoré.

Vous pouvez aussi utiliser **Import Sessions** dans l'arbre des sessions et choisir un
`servers.json` (ou un fichier exporté par le Heimdall C#). L'import de fichier lit aussi le
`settings.json` placé à côté.

Importer de nouveau est sans risque : un profil de même identifiant est mis à jour, pas
dupliqué.

### Ce qui est importé

| Élément | Remarques |
|---|---|
| Profils | SSH, SFTP, RDP, VNC, Telnet, FTP, WinRM, Citrix, shell local, et WEB. |
| Dossiers et couleurs | Chemins des dossiers et couleurs de la palette du C#. |
| Valeurs par défaut des dossiers | Copiées dans chaque profil à l'import (voir plus bas). |
| Passerelles SSH | Créées ou fusionnées ; un profil qui référençait une passerelle absente est signalé. |
| Clés d'hôte SSH de confiance | Clés et empreintes sont reprises : les serveurs connus ne redemandent rien. |
| Connexions planifiées | Reprises avec leur planning. |
| Préférences du C# | Présentées d'abord pour relecture, appliquées seulement si vous confirmez. Une préférence que vous avez déjà changée dans Heimdall-rs est conservée. |

Les valeurs par défaut des dossiers fonctionnent autrement : le C# les résout à la
connexion, depuis le dossier du profil, le plus profond d'abord. Heimdall-rs les résout une
seule fois, à l'import, et écrit le résultat dans chaque profil. Modifier ensuite une valeur
par défaut de dossier dans le C# ne change pas les profils déjà importés : importez de
nouveau pour la reprendre.

### Ce qui n'est pas importé

- **Mots de passe, phrases secrètes, code PIN et autres secrets.** Le C# les protège avec
  Windows DPAPI, que seul ce compte Windows peut défaire. Saisissez chaque mot de passe de
  nouveau à la première connexion et enregistrez-le dans le coffre.
- **Le hachage du PIN, la clé HMAC, le jeton d'accès Git** et le secret de déverrouillage du
  fournisseur d'identifiants, parmi les préférences.
- Les chemins qui appartiennent à l'installation du C#, et les valeurs de préférences que
  Heimdall-rs ne lit pas. La boîte de relecture les nomme.
- Certains réglages que le client intégré n'utilise pas encore (redirection X11,
  imprimantes RDP, ports série, cartes à puce, webcam, USB, plusieurs moniteurs). La boîte
  d'import les liste par profil.

## Différences volontaires

| Domaine | Heimdall-rs | Pourquoi |
|---|---|---|
| DEFAULTCREDS | Liste seulement les identifiants constructeur connus. N'essaie jamais de connexion. | Pas de verrouillage de comptes, pas de tentative de connexion non autorisée. |
| Route via (outils réseau) | Les outils TCP se connectent à travers une passerelle SSH, par canaux SSH uniquement. Aucune commande n'est exécutée sur la passerelle. Les outils qui demandent ICMP, ARP, sockets brutes ou UDP refusent de passer par une passerelle. | Une passerelle porte du TCP, pas ces protocoles, et y exécuter des commandes demanderait des droits que personne n'a demandés. |
| Scanner réseau | Les sous-réseaux plus larges que /22 sont refusés, et vous choisissez les hôtes trouvés à ajouter comme sessions. | Un balayage borné, et rien n'est enregistré sans votre choix. |
| Compression RDP | Le client RDP intégré compresse par défaut (jusqu'à RDP 6.1 XCRUSH), pour les profils où la compression est cochée, comme le client C#. **Compresser dans le client intégré**, dans les réglages, la désactive. | Si le serveur envoie des données que le client ne sait pas lire, la compression se coupe d'elle-même pour cette connexion et le client se reconnecte une fois sans elle. |
| Extras RDP | Une RD Gateway s'ouvre dans Connexion Bureau à distance (`mstsc.exe`). Les options que seul ce client honore (cartes à puce, imprimantes, microphone et autres) sont marquées "External client (mstsc.exe) only" dans le profil. | Le client intégré ne les implémente pas encore. `mstsc.exe` n'existe que sous Windows. |
| NOTES | Les notes de travail sont des fichiers Markdown, éditées en texte brut ; une note peut être copiée en Markdown ou en page HTML. | Pas de moteur web embarqué. |
| DIAGRAM | Un éditeur de texte pour la source (Mermaid, Graphviz DOT, PlantUML, ASCII). Il ne dessine pas le diagramme : affichez-le dans un outil externe. | Pas de moteur web embarqué. |
| CERT | Lit aussi le certificat d'un serveur TLS 1.0 à 1.2, pour affichage seulement, avec un avertissement. Heimdall-rs ne négocie jamais ces versions pour une vraie connexion. | Permet d'inspecter et d'auditer d'anciens serveurs sans affaiblir aucune connexion. |
| Commandes importées ou partagées | Une commande qui vient de l'extérieur (import C# ou MobaXterm, fichier de sessions partagées) n'est jamais tapée avant que vous appuyiez sur **Exécuter et mémoriser** dans la confirmation, et chaque étape de ce genre est demandée à part : modifier une autre étape, un délai, l'ordre ou l'activation ne la rend pas fiable, et enregistrer le formulaire du profil non plus. L'approbation mémorisée d'une session partagée est liée au texte exact de la commande, à la destination (hôte, port, compte, passerelle) et au fichier partagé, et n'est pas mémorisée du tout pour une session dont le compte est demandé à la connexion. | Le C# approuvait les commandes importées à l'enregistrement du formulaire du profil, et un fichier partagé est modifié par d'autres personnes : une commande ne doit jamais s'exécuter parce que autre chose a été enregistré. |

## Utiliser un profil séparé pour tester

Toute version accepte `--data-dir <chemin complet>` : Heimdall-rs garde alors sa
configuration, ses données et ses mots de passe enregistrés sous ce dossier, et tourne à côté
de votre propre instance sans jamais toucher à ses profils ni à ses identifiants. Servez-vous-en
pour essayer d'abord l'import sur une copie de vos données. Voir
[`--data-dir` dans une version normale](test-data-folder.md#--data-dir-dans-une-version-normale).
