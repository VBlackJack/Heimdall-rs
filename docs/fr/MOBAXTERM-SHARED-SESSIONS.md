[English version](../MOBAXTERM-SHARED-SESSIONS.md)

# Sources de sessions partagées MobaXterm

Pour les équipes qui gardent leurs sessions dans un fichier MobaXterm `.mxtsessions` sur un
dossier partagé. Heimdall-rs lit ce fichier et affiche ses sessions, en lecture seule, à côté
des vôtres. Ce guide dit ce qui est lu, ce qui est écarté et pourquoi, et les endroits où
Heimdall-rs diffère de MobaXterm et du Heimdall C#.

## 1. Ajouter une source partagée

1. Ouvrez **Paramètres > Général** et cherchez la carte **Sources de sessions partagées**.
2. Cliquez sur **Ajouter une source**.
3. **Nom** : le nom que l'arbre affiche pour cette source, par exemple `Team`.
4. **Chemin du fichier** : le chemin du fichier `.mxtsessions`. Le bouton de parcours le
   choisit, ou saisissez un chemin qui commence par une variable d'environnement pour qu'il
   marche chez tout le monde, par exemple `%OneDriveCommercial%\Team\sessions.mxtsessions`.
5. Laissez **Lire cette source** coché et enregistrez.

La carte dit combien de sessions la source contient, ou pourquoi son fichier n'a pas pu être
lu. La source apparaît dans l'arbre des sessions sous un en-tête à elle, `Team (12)`, avec ses
dossiers MobaXterm dessous. Un fichier de plus de 16 Mo n'est pas lu, et une source contient
5000 sessions au plus.

## 2. Vérifier la variable de login

Les fichiers MobaXterm écrivent souvent le login sous forme de variable, comme `[DAS]`.
Heimdall-rs la remplace par la valeur définie dans **Paramètres > Général > Variables d'import
MobaXterm**. `DAS` vaut par défaut `%USERNAME%`, votre login Windows. Si votre login sur les
serveurs est différent, changez la valeur là ; la colonne **Devient** montre ce qu'elle
devient. Une variable sans valeur figure dans le rapport d'import (section 8) et le login est
gardé tel qu'écrit.

## 3. Ce qui est en lecture seule, et pourquoi

Les sessions partagées appartiennent au fichier, que d'autres personnes maintiennent. Dans
Heimdall-rs, elles sont tenues en mémoire à côté de vos sessions : aucun menu qui modifie,
renomme, copie, déplace ou supprime une session enregistrée ne les atteint, et rien n'est
jamais réécrit dans le fichier. Un badge **partagée** sur chaque ligne nomme la source.

Pour changer une session partagée, changez-la là où l'équipe la garde : dans MobaXterm, ou
dans le fichier lui-même. Heimdall-rs prend le changement tout seul (section 7).

La connexion marche comme pour vos propres sessions, et une session partagée demande toujours
la clé ou le certificat de son serveur exactement comme une session enregistrée : un fichier
ne peut rien approuver à la place de la personne qui le lit.

## 4. Quelles sessions sont prises

Heimdall-rs lit le type de chaque session, pas son icône : une session à icône personnalisée
est donc prise comme les autres.

| Type MobaXterm | Pris comme | Ce qui est lu |
|---|---|---|
| SSH | shell SSH | hôte, port, utilisateur, hôte de rebond SSH, commande de login |
| SFTP | session de fichiers SFTP | hôte, port, utilisateur |
| RDP | RDP | hôte, port, utilisateur, hôte de rebond SSH, RD Gateway |
| Telnet | Telnet | hôte, port |
| VNC | VNC | hôte, port |

Rien d'autre d'une session n'est lu : ni clé, ni mot de passe, ni option. Toute session qui
n'est pas dans le tableau est écartée, et le rapport d'import dit pourquoi (section 8).

### Commande de login

Une session SSH peut porter une commande de login, comme `sudo su - <compte>`, que MobaXterm
tape après la connexion. Heimdall-rs la lit comme une étape post-connexion, et elle reste non
approuvée tant que vous n'avez pas choisi **Exécuter et mémoriser** :

- La première fois, Heimdall-rs montre la commande exacte et demande avant de la taper. **Exécuter
  et mémoriser** la tape et garde votre réponse sur cet ordinateur ; **Se connecter sans elles**
  ouvre le shell sans rien taper.
- La réponse est liée à la session, à la commande exacte, à la destination (hôte, port, compte
  et bastion) et au fichier d'où elle vient. Si le fichier change l'un d'eux, la question
  revient.
- Une session qui ne nomme aucun compte, ou dont le bastion n'en nomme aucun, est interrogée à
  chaque fois : le compte saisi à la connexion n'est pas connu de l'approbation.
- Les approbations restent sur cet ordinateur. Elles ne sont jamais exportées avec les
  réglages, un fichier de réglages ne peut en apporter aucune, et retirer une source oublie ses
  approbations.
- Une commande qui contient un caractère de contrôle, ou de plus de 2048 octets, est écartée
  au lieu d'être proposée, et une session porte 16 étapes au plus.

C'est une différence voulue avec le Heimdall C#, qui approuve les étapes d'un profil importé à
l'enregistrement de son formulaire. Ici, une étape venue de l'extérieur n'est jamais tapée avant
que vous choisissiez **Exécuter et mémoriser** pour elle, quoi que dise le fichier, et enregistrer ou modifier le
profil ne l'approuve pas. Voir [Passer de Heimdall (C#)](migration-from-csharp.md).

## 5. Passerelles et RD Gateway

- **Hôte de rebond SSH.** Une session SSH ou RDP qui nomme un hôte de rebond dans MobaXterm
  passe par lui : le bastion est défini par le fichier partagé, pour les seules sessions
  partagées, et n'apparaît jamais dans vos propres passerelles. Deux sessions par le même hôte,
  port et utilisateur partagent un seul bastion.
- **RD Gateway.** Une session RDP réglée sur " toujours utiliser une RD Gateway ", dont la
  connexion à la passerelle se fait par mot de passe (comme le serveur final, invite, ou un
  login), garde sa passerelle. Le client RDP intégré ne passe pas par une RD Gateway : la
  session s'ouvre donc dans Connexion Bureau à distance avec la passerelle réglée, et vous vous
  connectez à la passerelle avec un mot de passe. Le login de la passerelle n'est pas repris.
  Une session réglée sur " Never " ou " Detect " ne stocke aucun serveur de passerelle et se
  connecte directement, comme dans MobaXterm.
- **Jamais en contournant une route.** Une session dont Heimdall-rs ne sait pas suivre la route
  est écartée, jamais ouverte en connexion directe, car une connexion directe atteindrait le
  serveur en contournant le bastion, le proxy ou la passerelle que le fichier demandait
  (section 8).

## 6. Mots de passe, clés et fichier partagé

Un fichier partagé ne fournit jamais de mot de passe ni de clé : Heimdall-rs ne les lit pas,
même quand MobaXterm en a stocké, et l'en-tête de la source dans l'arbre dit combien il en a
ignoré. Les clés viennent de votre agent SSH (Pageant, l'agent OpenSSH de Windows, ou un autre
agent).

Quand aucune clé de l'agent n'est acceptée, Heimdall-rs demande le mot de passe du bastion,
puis celui du serveur, à la connexion. Heimdall-rs ne retient pas encore le mot de passe d'une
session partagée sur cet ordinateur : il est demandé à chaque connexion et n'est jamais écrit
dans le fichier partagé, dans la copie hors ligne, ni dans les réglages.

- Une session SFTP partagée demande son mot de passe comme une session SSH.
- Une session VNC demande son mot de passe quand le serveur en veut un. La connexion reste
  ouverte pendant la saisie, pour que la tentative ne compte pas comme une connexion échouée
  sur les serveurs qui bloquent votre adresse après quelques échecs.
- Une session RDP demande son mot de passe à la connexion et Heimdall-rs ne le retient pas.

## 7. Rechargement automatique et copie hors ligne

Heimdall-rs regarde chaque fichier source toutes les quelques secondes. Quand il change et s'est
stabilisé, cette source seule est relue, l'arbre suit, et vos dossiers dépliés et votre
sélection restent. Les sessions ouvertes ne sont jamais fermées par un rechargement.

Si le nouveau fichier ne peut pas être lu, ou ne contient aucune session, les sessions
précédentes sont gardées et la source dit pourquoi. Un fichier qui se lit mais dont toutes les
sessions sont écartées (par exemple parce qu'un hôte de rebond ou un proxy a été ajouté à
chacune) n'est pas ce cas : il ne charge aucune session, et ni la lecture précédente ni la copie
hors ligne ne ramène une session écartée en connexion directe.

Après chaque lecture réussie, Heimdall-rs garde une copie des sessions dans son dossier de
données, dans le dossier `SharedSessionCache` à côté du fichier de réglages. Si le fichier est
injoignable au démarrage (par exemple tant que le dossier partagé n'est pas synchronisé), les
sessions viennent de cette copie et la source indique que ces sessions viennent de la copie
gardée à une date. La copie ne contient aucune commande de login : une session ouverte depuis
elle ne tape rien après la connexion. Retirer la source supprime sa copie.

## 8. Lire le rapport d'import

Chaque lecture vérifie aussi le fichier à la recherche d'erreurs courantes. Dans **Paramètres >
Général > Sources de sessions partagées**, cliquez sur **Rapport d'import** sur une source. Le rapport
liste, par dossier et par session :

- **Sessions ignorées** : les sessions non chargées, et pourquoi (ci-dessous).
- **Variables non résolues** : un `[NOM]` sans valeur (section 2).
- **Nom et hôte différents** : une session dont le nom n'est ni son hôte ni le premier
  élément de son hôte.
- **Passerelles inhabituelles** : dans un dossier d'au moins 3 sessions où une route couvre au moins 60
  pour cent d'entre elles, une session qui prend une autre route. Seuls comptent les types qui
  peuvent nommer un hôte de rebond dans MobaXterm, c'est-à-dire SSH et RDP : une session Telnet,
  VNC ou SFTP est directe quoi que fasse son dossier.
- **Hôtes en double** : deux sessions d'un même dossier qui atteignent le même hôte avec le même
  protocole. Une session SSH et une session RDP ou SFTP vers un même hôte sont deux façons de
  l'utiliser, pas des doublons ; deux sessions SSH vers lui en sont.

**Copier** met le rapport dans le presse-papiers et **Exporter en CSV...** l'écrit dans un fichier, à
envoyer à la personne qui maintient le fichier. Seules les sessions écartées manquent à l'arbre ;
tout autre constat est un conseil, et sa session reste chargée.

### Pourquoi une session est écartée

| Session | Raison donnée |
|---|---|
| FTP | Pas encore prise : FTP envoie son mot de passe en clair. |
| WSL | Un système Linux de l'ordinateur qui exécute MobaXterm, rien à quoi se connecter. |
| Tout autre type (Serial, Mosh, RSH, Xdmcp, Browser, etc.) | Le type de session N n'est pas pris en charge ; le rapport donne le numéro du type. |
| Telnet ou VNC par un hôte de rebond SSH | Heimdall-rs n'achemine pas ces protocoles par un hôte de rebond, et une connexion directe le contournerait. |
| SSH, Telnet, VNC ou SFTP par un proxy | Heimdall-rs n'utilise pas de proxy pour ces protocoles, et une connexion directe le contournerait. |
| RDP par une RD Gateway avec une carte à puce, ou un réglage de passerelle qu'il ne reconnaît pas | Heimdall-rs se connecte à une passerelle avec un mot de passe, et une connexion directe contournerait la passerelle. |
| Un hôte qui n'est ni un nom d'hôte ni une adresse, ou une passerelle SSH ou RD dont l'hôte n'en est pas un | La session n'est pas prise, car elle atteindrait son hôte en contournant la passerelle. |
| Une seconde session de même dossier et de même nom | Seule la première est gardée. |
| Une source qui contient déjà 5000 sessions | Le reste est écarté. |

L'import ponctuel d'un fichier MobaXterm (**Importer des sessions** dans l'arbre des sessions) écarte
les sessions des lignes de type et de route ci-dessus pour les mêmes raisons, FTP compris, et le
dit dans son résumé. Les autres lignes ne concernent que les sources partagées.

## 9. Limites connues

- Les mots de passe RDP partagés ne sont pas retenus (section 6).
- Les outils réseau (ping, scan de ports et les autres outils qui peuvent passer par une
  passerelle) n'empruntent que vos propres passerelles SSH des réglages, pas les bastions
  qu'une source partagée définit. Pour atteindre depuis un outil des hôtes derrière un bastion
  partagé, définissez ce bastion comme l'une de vos passerelles.
- Le dossier de départ distant d'une session SFTP n'est pas repris : la session s'ouvre dans le
  dossier par défaut.
- Une session Telnet demande son login dans le terminal, donc aucun utilisateur n'est importé
  pour elle.
- Un serveur VNC qui demande un nom d'utilisateur reste hors de portée : Heimdall-rs n'a pas de
  nom d'utilisateur VNC pour une session partagée.
- MobaXterm n'a pas de réglage de proxy pour une session RDP : son autre route est une RD
  Gateway (section 5).
- Les sessions partagées sont en lecture seule dans Heimdall-rs (section 3).
