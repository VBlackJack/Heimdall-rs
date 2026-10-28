[English version](SECURITY.md)

# Politique de sécurité

## Signaler une vulnérabilité

Signalez toute vulnérabilité suspectée par le signalement privé de GitHub :
[ouvrir un rapport privé](https://github.com/VBlackJack/Heimdall-rs/security/advisories/new).
Le rapport reste visible de vous et du mainteneur seulement, et le correctif se
discute dans ce même fil. N'ouvrez pas de ticket public pour une vulnérabilité
présumée.

Indiquez dans le rapport :

- la version ou le commit concerné ;
- les étapes de reproduction ;
- l'impact attendu ;
- tout journal pertinent, avec les identifiants et les noms d'hôtes expurgés.

## Où adresser le reste

| Sujet | Canal |
| --- | --- |
| Vulnérabilité présumée | [Signalement privé](https://github.com/VBlackJack/Heimdall-rs/security/advisories/new) |
| Bogue, plantage, régression | [Issues](https://github.com/VBlackJack/Heimdall-rs/issues) |
| Question, idée, retour d'usage | [Discussions](https://github.com/VBlackJack/Heimdall-rs/discussions) |

## Il n'y a pas de canal de messagerie

Ce projet ne publie aucune adresse de contact, et la messagerie n'est pas un
moyen pris en charge pour joindre le mainteneur. Les adresses qui apparaissent
dans les métadonnées de commit sont un artefact de la façon dont Git enregistre
la paternité, pas une invitation : elles ne sont pas relevées, et un message qui
y est envoyé ne reçoit pas de réponse. Utilisez l'un des canaux GitHub ci-dessus.

## Périmètre et mesures

Ce que fait cette version :

- **Secrets au repos** : les mots de passe enregistrés vont dans le coffre, un fichier scellé en AES-256-GCM sous une clé dérivée du mot de passe maître par Argon2id, ou, sans mot de passe maître, dans le magasin d'identifiants du système (Gestionnaire d'identification de Windows, Secret Service sous Linux). Windows Hello peut déverrouiller le coffre ; tout échec revient au mot de passe maître. Les secrets que le Heimdall C# protégeait par DPAPI ne sont jamais importés.
- **Clés d'hôte et certificats** : une clé d'hôte SSH inconnue est montrée et demandée ; une clé changée, une clé d'un autre algorithme ou un certificat d'hôte refuse la connexion. Un certificat FTPS ou VNC (VeNCrypt) auquel le système ne fait pas confiance n'est épinglé qu'une fois accepté, il est vérifié de nouveau à chaque connexion, et la décision est prise avant l'envoi de tout mot de passe ; une nouvelle clé déclenche l'alerte de clé changée.
- **Exécution de processus** : les programmes sont lancés à partir d'un chemin et d'une liste d'arguments, jamais par une ligne de commande qu'un shell lit. Les programmes système que lancent les outils sont désignés par leur chemin complet, jamais cherchés dans le `PATH` ; la synchronisation Git de la bibliothèque de commandes n'autorise que les transports `https` et `ssh`, sans hooks.
- **Données importées** : une commande qui vient d'un import ou d'un fichier de sessions partagées ne s'exécute jamais avant votre confirmation (voir [Différences volontaires](docs/fr/migration-from-csharp.md#différences-volontaires)).
- **Partage de dossier** : en lecture seule, un jeton aléatoire de 256 bits par partage, l'adresse de bouclage par défaut et l'écoute sur toutes les interfaces à la fois refusée. Un lien, une jonction ou tout autre point d'analyse sous le dossier partagé n'est jamais servi ni suivi.
- **Journaux** : les messages, la saisie et le texte du presse-papiers sont journalisés sous des formes expurgées, et la sortie des terminaux n'est jamais journalisée. La copie masquée de la sortie d'un terminal remplace les secrets que ses règles reconnaissent ; c'est une heuristique, à relire avant de partager le texte.
- **Sûreté mémoire** : `unsafe_code = "forbid"` pour tout l'espace de travail (`Cargo.toml`), sauf `heimdall-dragout` (le glisser-déposer du shell Windows) et `heimdall-titlebar` (les couleurs de la barre de titre), qui l'autorisent chacun dans un seul module. Les secrets que fournit la cryptographie sont effacés à leur libération. Les bibliothèques natives chargées sous Linux sont listées dans [docs/fr/native-libraries.md](docs/fr/native-libraries.md).
- **Dépendances** : `cargo-deny` s'exécute en CI avec `deny.toml` : avis de sécurité, licences, et aucune crate d'un registre ou d'un dépôt Git inconnu. Chaque avis ignoré indique sa raison et une date de réexamen.

Limitations connues :

- Pas encore de version signée (#454).
- Le coffre garde les secrets déchiffrés en mémoire tant qu'il est déverrouillé (#536).
- Aucun contrôle de révocation des certificats, ni OCSP ni CRL (#447).
- Le partage de dossier sert 16 connexions à la fois, et des connexions inactives peuvent toutes les occuper (#505).
