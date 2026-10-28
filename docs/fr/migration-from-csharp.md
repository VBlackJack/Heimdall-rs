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
| Mots de passe des sessions partagées | Le mot de passe d'une session partagée SSH, SFTP ou VNC peut être mémorisé sur cet ordinateur : cochez **Mémoriser ce mot de passe sur cet ordinateur** quand il est demandé (la case est décochée au départ). Il est conservé une fois le serveur accepté, dans le magasin d'identifiants du système ou dans le coffre, jamais dans les paramètres, un export, la copie hors ligne ni un journal. Il est classé par protocole, hôte, port et identifiant, pas par session : un fichier partagé qui déplace une session vers un autre serveur ou un autre compte ne trouve aucun mot de passe et il vous est redemandé. Un mot de passe refusé par le serveur est oublié, et **Oublier les mots de passe mémorisés** dans les sources partagées des paramètres oublie toute une source. Un bastion sur la route et les sessions RDP ne mémorisent rien. | Un fichier partagé est modifié par d'autres personnes : un mot de passe ne doit jamais suivre une session vers un serveur pour lequel il n'a pas été saisi. |
| Champ actif | La bordure du champ dans lequel vous saisissez fait 2 px, dans la couleur d'accent (jaune en contraste élevé). Le C# trace 1 px. | Le champ du clavier se repère d'un coup d'oeil, dans tous les thèmes. |
| Partage de dossier (liens) | Un lien symbolique, une jonction ou tout autre point d'analyse Windows (un espace réservé OneDrive, un fichier dédupliqué) sous le dossier partagé n'est jamais servi ni suivi. Le listing l'affiche avec la raison "non servi : lien Windows ou espace réservé cloud", une requête pour lui reçoit une page 403 qui dit la même chose, et la barre d'état compte les refus et montre le dernier chemin refusé jusqu'au lien (jamais ce qui se trouve en dessous, ni le contenu) ; sous un flot de requêtes le compte est un minimum. TFTP n'est pas porté. | Le serveur C# suivait les liens : un lien pouvait servir un fichier extérieur au dossier. Heimdall-rs les refuse tous, visiblement, sans appel natif. Pour partager un tel fichier, copiez-le dans le dossier partagé. |
| Arbre des sessions, noms longs | Quand un nom est plus large que la barre latérale, l'arbre défile horizontalement (une barre fine, visible seulement alors) et le nom reste entier. Le badge de passerelle garde la règle du nom d'abord : il reçoit ce que le nom laisse et disparaît sous 56 px. Le C# a désactivé sa barre horizontale le 2026-10-01 et termine un nom long par une ellipse. | Chaque caractère d'un nom reste accessible. |
| Thème par défaut | Une nouvelle installation, un fichier de paramètres sans thème ou un nom de thème inconnu démarre en Magellan. Le défaut du C# est Drakul. Un thème choisi est conservé, et un thème C# importé (Drakul compris) est conservé tel quel. **Rétablir** sur le thème revient à Magellan. | Décision du propriétaire du 2026-10-25. |
| Titre de la fenêtre | La fenêtre principale s'intitule toujours "Heimdall-rs", quel que soit l'onglet actif ; celle du C# s'intitule "Heimdall - Remote Access Control Center". Le titre propre à un terminal (posé par le shell) ne remplace jamais le nom de la session dans l'onglet, l'en-tête du volet ni la barre d'état, comme dans le C#. | Le nom visible reste "Heimdall-rs", jamais "Heimdall" seul, pour que les deux applications cohabitent ; l'écran d'accueil dit "Heimdall-rs" pour la même raison. |
| Paramètres | Chaque paramètre s'applique dès qu'il change. Il n'y a pas de barre **Enregistrer** / **Annuler les modifications** ; seul **Rétablir les valeurs par défaut** existe. | Volontaire : rien n'attend d'être enregistré, donc rien ne se perd en fermant la fenêtre. |
| Consoles web (profils WEB) | Ouvertes dans le navigateur du système, jamais intégrées à un onglet ; le C# les ouvre dans un onglet WebView2 intégré. Derrière une passerelle SSH, une console est jointe par un tunnel sur le réseau de bouclage de cet ordinateur, chaque console sur une adresse de bouclage qui lui est propre : le navigateur affiche alors cette adresse et le port du tunnel, pas le nom de la console, ce que remarque une console qui vérifie son propre nom, ou un certificat qui la nomme. Le "Ouvrir avec un programme externe" d'un profil C# (son programme et `WebExternalArguments`) n'est pas lu, et la boîte d'import nomme chaque profil dont le programme est laissé de côté. | Pas de moteur web embarqué. Une adresse propre à chaque console sépare les cookies de chacune dans le navigateur. |
| Citrix | Une application ou un bureau publié est lancé dans Citrix Workspace (son `storebrowse.exe` ou `SelfService.exe` sous Windows ; sous Linux, un fichier ICA est ouvert par `xdg-open`), et sa fenêtre reste celle de Citrix : elle n'est jamais intégrée. L'onglet montre le lancement et si le client Citrix (`wfica32.exe`) tourne. Le C# peut intégrer la fenêtre du client dans son onglet. L'intégration est suivie par l'issue #449. | Prendre la fenêtre d'un autre programme dans un onglet demande des appels Win32 que cette version ne fait pas. |
| Recherche de mise à jour | Une version publiée demande à GitHub la dernière version de Heimdall-rs et propose une version plus récente sous forme de lien vers sa page. Rien n'est téléchargé ni installé. Une version de développement ne cherche jamais. Le C# télécharge et installe un installeur vérifié. | L'installation depuis l'application attend des artefacts de version signés (issue #454). Une version de développement ne sait pas distinguer une version plus ancienne d'une plus récente. |
| Élévation du shell local | **Exécuter en tant qu'administrateur** sur un profil de shell local (Windows seulement) lance le shell dans une fenêtre à lui, par l'invite d'élévation de Windows, comme le mode "Fenêtre externe" (Runas) du C# ; l'onglet indique seulement où il tourne. Une invite refusée n'exécute rien : jamais un shell non élevé à sa place. Les modes Auto et gsudo du C#, qui gardent le shell élevé dans l'onglet, sont importés en **Exécuter en tant qu'administrateur**, et la boîte d'import nomme chacun de ces profils ; None reste non élevé. gsudo dans l'onglet est l'issue #448. | Windows ne rend aucun processus à suivre pour un lancement élevé : le shell ne peut pas vivre dans un onglet sans un assistant comme gsudo, qui n'est pas encore porté. |
| Lecteurs d'écran | Pas de prise en charge des lecteurs d'écran : iced, la boîte à outils qui dessine Heimdall-rs, n'expose pas encore d'arbre d'accessibilité, donc aucun contrôle n'a de nom qu'un lecteur d'écran puisse lire. **Ctrl+Maj+A** copie dans le presse-papiers ce que la barre d'état a dit récemment, pour qu'un lecteur d'écran le lise. Le clavier passe d'un contrôle à l'autre, un thème à contraste élevé est pris tant que le contraste élevé de Windows est actif, et la réduction des animations de Windows arrête le clignotement du curseur et la pulsation du point d'onglet. Le C# est WCAG 2.1 AA grâce à UI Automation (noms accessibles, régions dynamiques). | Une limite de la boîte à outils, pas un choix. |
| Paramètres de temporisation | Les délais, attentes et nombres de tentatives que le C# expose en une vingtaine de paramètres (`ExternalToolTimeoutMs`, `HostKeyProbeTimeoutMs`, `TelnetConnectTimeoutMs`, `TunnelRetryDelayMs`, `SftpUploadDebounceMs`, `SharedSourceReloadRetryCount`, `ProcessKillTimeoutMs`, `SleepPreventionIntervalSeconds` et autres) sont des valeurs fixes dans Heimdall-rs. Un import de paramètres C# ne les lit pas : la relecture d'un fichier **Export settings** du C# liste chacun comme sans équivalent ici, et depuis `settings.json` ils sont laissés de côté sans ligne. | Pas encore exposés : l'issue #499 décide lesquels deviennent des paramètres. |

## Pas encore porté

- **Privilege Launcher** (`PRIVLAUNCH`), le 58e outil du C# : cette version a les 57 autres. Une entrée d'outil C# pour lui est importée et conservée, et l'ouvrir indique que l'outil n'est pas dans cette version. Pas encore d'issue.
- **Projets**, regroupement des profils par projet : issue #498.
- **Paramètres de temporisation** (voir le tableau) : issue #499.
- **TFTP** dans le partage de dossier : issue #494.
- **Détection de Sysinternals, NirSoft et NanaRun** parmi les outils externes : issue #450.
- **Client RDP intégré** : cartes à puce (#443), RD Gateway (#444), microphone (#445) et imprimantes (#446). D'ici là, voir "Extras RDP" dans le tableau.
- **Contrôle de révocation des certificats** (OCSP, CRL) pour RDP, FTPS et VeNCrypt : issue #447.

## Utiliser un profil séparé pour tester

Toute version accepte `--data-dir <chemin complet>` : Heimdall-rs garde alors sa
configuration, ses données et ses mots de passe enregistrés sous ce dossier, et tourne à côté
de votre propre instance sans jamais toucher à ses profils ni à ses identifiants. Servez-vous-en
pour essayer d'abord l'import sur une copie de vos données. Voir
[`--data-dir` dans une version normale](test-data-folder.md#--data-dir-dans-une-version-normale).
