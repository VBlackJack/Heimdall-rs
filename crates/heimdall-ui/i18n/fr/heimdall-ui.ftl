# Copyright 2026 Julien Bombled
# Licensed under the Apache License, Version 2.0. See LICENSE.

ui-window-title = Heimdall
ui-window-title-tab = { $tab } - Heimdall

ui-sidebar-title = Sessions
ui-sidebar-empty = Aucun profil enregistré.
ui-sidebar-settings-button = Paramètres
ui-desktop-send-keys = Envoyer des touches
ui-desktop-send-keys-tooltip = Envoyer des touches à la session distante
ui-desktop-send-clipboard = Envoyer le presse-papiers
ui-desktop-send-clipboard-tooltip = Envoyer le presse-papiers de cet ordinateur au serveur, sans chiffrement
ui-desktop-fullscreen = Plein écran (F11)
ui-desktop-match-window = Résolution de la fenêtre
ui-desktop-fit-window = Adapter à la fenêtre
ui-desktop-exit-fullscreen = Quitter le plein écran (F11)
ui-desktop-keys-ctrl-alt-del = Ctrl+Alt+Suppr
ui-desktop-keys-windows = Touche Windows
ui-desktop-keys-alt-tab = Alt+Tab
ui-desktop-keys-ctrl-esc = Ctrl+Échap (menu Démarrer)
ui-desktop-keys-escape = Échap
ui-desktop-keys-print-screen = Impr. écran
ui-desktop-keys-win-l = Win+L (verrouiller la session)
ui-desktop-keys-win-d = Win+D (afficher le bureau)
ui-desktop-keys-win-e = Win+E (explorateur de fichiers)
ui-tree-search-placeholder = Rechercher
ui-tree-search-tooltip = Rechercher des sessions (Ctrl+F)
ui-tree-search-clear = Effacer la recherche
ui-tree-search-clear-button = x
ui-tree-search-no-results = Aucune session ne correspond à la recherche.
ui-tree-filter-tooltip = Filtres
ui-tree-filter-protocols = Protocoles
ui-tree-filter-connected = Connectées
ui-tree-filter-gateway = Via passerelle
ui-tree-filter-gateway-badge = Afficher le badge passerelle
ui-tree-filter-no-results = Aucune session ne correspond à la recherche et aux filtres.
ui-tree-filter-reset = Réinitialiser les filtres
ui-sidebar-lock-button = Verrouiller
ui-sidebar-lock-tooltip = Verrouiller l'espace de travail (Ctrl+L)
ui-settings-title = Paramètres
ui-settings-tab-general = Général
ui-settings-tab-terminal = Terminal
ui-settings-tab-ssh = SSH
ui-settings-tab-rdp = RDP
ui-settings-tab-security = Sécurité
ui-settings-security = Sécurité
ui-settings-pin-title = PIN de l'application
ui-settings-pin-enabled = Un PIN est actuellement défini.
ui-settings-pin-disabled = Aucun PIN n'est défini.
ui-settings-pin-configure = Configurer le PIN...
ui-pin-enter-title = Entrer le PIN
ui-pin-setup-title = Configurer le PIN
ui-pin-field-pin = PIN
ui-pin-field-current = PIN actuel
ui-pin-field-new = Nouveau PIN
ui-pin-field-confirm = Confirmer le PIN
ui-pin-unlock-button = Déverrouiller
ui-pin-save-button = Enregistrer
ui-pin-remove-button = Supprimer le PIN
ui-pin-problem-wrong = PIN incorrect. { $remaining ->
    [one] { $remaining } tentative restante.
   *[other] { $remaining } tentatives restantes.
}
ui-pin-problem-locked-out = Trop de tentatives incorrectes. Réessayez dans { $minutes ->
    [one] { $minutes } minute.
   *[other] { $minutes } minutes.
}
ui-pin-problem-wrong-current = Le PIN actuel est incorrect.
ui-pin-problem-too-short = Le PIN doit comporter au moins { $min } chiffres.
ui-pin-problem-too-long = Le PIN doit comporter au plus { $max } chiffres.
ui-pin-problem-not-digits = Le PIN ne doit contenir que des chiffres.
ui-pin-problem-mismatch = Les PIN ne correspondent pas.
ui-pin-problem-system = Le PIN n'a pas pu être enregistré : { $detail }
ui-settings-vault-title = Mot de passe maître
ui-settings-provider-title = Fournisseur d'identifiants externe
ui-profile-field-vault-entry = Nom de l'entrée du coffre
ui-profile-vault-entry-placeholder = Laisser vide pour utiliser le nom d'affichage
ui-profile-vault-entry-help = Nom facultatif de l'entrée de ce serveur dans le gestionnaire de mots de passe externe. Utilisé pour la recherche {"{"}Title{"}"} du fournisseur d'identifiants ; vide, le nom d'affichage est utilisé.
ui-status-provider-no-password = Le fournisseur d'identifiants externe n'a retourné aucun mot de passe pour "{ $name }". Vérifiez la commande dans Paramètres > Sécurité.
ui-status-provider-failed = Échec du fournisseur d'identifiants externe : { $detail }
ui-status-provider-timed-out = Délai du fournisseur d'identifiants externe dépassé.
ui-settings-provider-enabled = Utiliser un fournisseur de credentials externe
ui-settings-provider-disabled-hint = Activez "Utiliser un fournisseur de credentials externe" pour configurer ces options
ui-settings-provider-preset = Préréglage rapide
ui-settings-provider-type = Type de fournisseur
ui-settings-provider-type-command = Commande externe
ui-settings-provider-type-credman = Gestionnaire d'identification Windows
ui-settings-provider-credman-help = Lit les identifiants génériques du Gestionnaire d'identification Windows. L'entrée est recherchée par le nom de l'entrée du coffre du profil, ou son nom d'affichage si non défini.
ui-settings-provider-preset-custom = Personnalisé
ui-settings-provider-command = Commande du fournisseur
ui-settings-provider-command-placeholder = ex. keepassxc-cli show -s {"{"}title{"}"}
ui-settings-provider-placeholders = Variables : {"{"}Host{"}"}  {"{"}Port{"}"}  {"{"}User{"}"}  {"{"}Title{"}"}  {"{"}Database{"}"}  {"{"}KeyFile{"}"}
ui-settings-provider-username = Commande nom d'utilisateur (facultatif)
ui-settings-provider-username-placeholder = ex. keepassxc-cli show -s -a UserName {"{"}title{"}"}
ui-settings-provider-username-help = Commande facultative qui récupère le nom d'utilisateur depuis le coffre. Exécutée uniquement lorsque le profil n'a pas de nom d'utilisateur ; sa sortie remplace le nom d'utilisateur. En cas d'échec, le nom d'utilisateur enregistré est utilisé.
ui-settings-provider-unlock = Secret de déverrouillage
ui-settings-provider-unlock-placeholder = Mot de passe maître de la base ou phrase secrète GPG
ui-settings-provider-unlock-help = Envoyé sur l'entrée standard de la commande, pour les outils qui demandent un déverrouillage (ex. keepassxc-cli ou pass). Conservé avec vos mots de passe enregistrés ; laissez vide si ce n'est pas nécessaire.
ui-settings-provider-unlock-save = Enregistrer
ui-settings-provider-unlock-saved = Un secret de déverrouillage est enregistré.
ui-settings-provider-unlock-forget = Oublier
ui-settings-provider-database = Chemin de la base
ui-settings-provider-key-file = Chemin du fichier de clé
ui-settings-provider-key-file-hint = Requis lorsqu'un modèle KeePassXC avec fichier de clé est sélectionné.
ui-settings-provider-test = Tester
ui-settings-provider-test-running = Test en cours...
ui-settings-provider-test-success = Connexion réussie - mot de passe récupéré.
ui-settings-provider-test-no-result = La commande n'a renvoyé aucune sortie. Vérifiez la commande et le chemin de la base.
ui-settings-provider-test-timeout = La commande a dépassé le délai ({ $seconds } s). Vérifiez que l'outil en ligne de commande est installé et qu'il répond.
ui-settings-provider-test-no-command = Saisissez d'abord un modèle de commande.
ui-settings-provider-test-no-key-file = Sélectionnez d'abord un fichier de clé.
ui-settings-provider-test-unclosed-quote = Un guillemet de la commande n'est pas fermé.
ui-settings-provider-test-error = Échec du test : { $detail }
ui-settings-provider-first-line = Utiliser uniquement la première ligne de sortie
ui-settings-provider-first-line-help = Ne conserver que la première ligne non vide de la sortie de la commande, en ignorant le texte d'état final. Requis pour KeePass2 KPScript (qui ajoute une ligne "OK: ...") et utile pour pass (notes après le mot de passe).
ui-settings-provider-keepass2-hint = KeePass2 : KPScript reçoit le mot de passe maître sur sa ligne de commande (-pw:), ce qui l'expose. Pour les bases .kdbx, keepassxc-cli est plus sûr - il lit le mot de passe de la base depuis l'entrée standard via le champ Secret de déverrouillage ci-dessus.
ui-settings-vault-explanation = Chiffrez vos identifiants stockés sous un mot de passe maître. Il vous sera demandé à chaque démarrage de l'application.
ui-settings-vault-enabled = Activé
ui-settings-vault-disabled = Désactivé
ui-settings-vault-enable = Activer le mot de passe maître
ui-settings-vault-change = Changer
ui-settings-vault-disable = Désactiver
ui-profile-field-password = Mot de passe
ui-profile-password-saved = Mot de passe enregistré
ui-profile-password-clear = Effacer
ui-profile-password-clear-tooltip = Supprimer le mot de passe enregistré pour cette session
ui-profile-password-locked = Déverrouillez le coffre pour enregistrer ou modifier un mot de passe.
ui-profile-password-no-store = Ce système n'a pas de magasin d'identifiants : définissez un mot de passe maître pour enregistrer des mots de passe.
ui-profile-error-username-for-password = Un mot de passe enregistré demande un nom d'utilisateur.
ui-vault-unlock-title = Entrez votre mot de passe maître
ui-vault-unlock-button = Déverrouiller
ui-vault-unlock-busy = Déverrouillage du coffre
ui-vault-locked-title = Espace de travail verrouillé
ui-vault-locked-body = Entrez votre mot de passe maître pour déverrouiller.
ui-vault-enable-title = Définir un mot de passe maître
ui-vault-enable-body = Vos identifiants stockés seront chiffrés sous ce mot de passe. Vous en aurez besoin à chaque démarrage de l'application. Il ne peut être ni récupéré ni réinitialisé : si vous l'oubliez, Heimdall ne s'ouvrira plus et les identifiants stockés sont perdus.
ui-vault-enable-button = Activer
ui-vault-enable-busy = Chiffrement de vos identifiants stockés
ui-vault-change-title = Changer votre mot de passe maître
ui-vault-change-button = Changer
ui-vault-change-busy = Mise à jour du mot de passe maître
ui-vault-disable-title = Désactiver votre mot de passe maître
ui-vault-disable-warning = Vos identifiants enregistrés reviendront au seul magasin d'identifiants du système, et le mot de passe maître ne sera plus demandé au démarrage.
ui-vault-disable-button = Désactiver
ui-vault-disable-busy = Suppression de la protection par mot de passe maître
ui-vault-field-master = Mot de passe maître
ui-vault-field-current = Mot de passe maître actuel
ui-vault-field-new = Nouveau mot de passe maître
ui-vault-field-confirm = Confirmer le mot de passe maître
ui-vault-policy-hint = Utilisez au moins { $min } caractères.
ui-vault-policy-ok = Force du mot de passe suffisante.
ui-vault-policy-too-short = Trop court : utilisez au moins { $min } caractères.
ui-vault-policy-complexity = Utilisez au moins { $classes } types de caractères (minuscule, majuscule, chiffre, symbole), ou au moins { $long } caractères.
ui-vault-problem-unreadable = Mot de passe maître incorrect ou coffre corrompu.
ui-vault-problem-mismatch = Les mots de passe ne correspondent pas.
ui-vault-problem-no-system-store = Ce système n'a pas de magasin d'identifiants pour reprendre les mots de passe : le mot de passe maître reste en place.
ui-vault-problem-exists = Un coffre existe déjà.
ui-vault-problem-system = Le fichier du coffre n'a pas pu être utilisé : { $detail }
ui-vault-save-failed-title = Le mot de passe n'a pas pu être enregistré.
ui-sidebar-group-none = (Sans dossier)

ui-home-welcome = Bienvenue dans Heimdall-rs.
ui-home-subtitle = Ajoutez une session ou importez vos connexions existantes pour commencer.
ui-home-add-button = Ajouter une session
ui-home-import-button = Importer des connexions
ui-home-shortcuts = Ctrl+N pour ajouter une session, Ctrl+K pour une connexion rapide
ui-home-select = Sélectionnez une session ou appuyez sur Ctrl+K pour vous connecter

ui-tab-close-button = ✕
ui-tab-bell-badge = cloche

ui-connect-progress = Connexion à { $target }...
ui-connect-cancel-button = Annuler

ui-hostkey-title = Hôte SSH inconnu
ui-hostkey-body = C'est la première connexion à { $host } sur le port { $port }. Vérifiez que l'empreinte ci-dessous est bien celle du serveur avant de lui faire confiance.
ui-hostkey-fingerprint = Empreinte : { $fingerprint }
ui-hostkey-accept-button = Accepter
ui-hostkey-trust-once-button = Approuver pour cette session
ui-hostkey-reject-button = Rejeter
ui-certificate-title = Certificat de serveur non reconnu
ui-certificate-body = "{ $name }" a répondu sur { $host }:{ $port } avec un certificat que ce profil n'a jamais approuvé.
ui-certificate-caution = Heimdall ne peut pas savoir si c'est la machine attendue. Approuvez-le seulement si vous reconnaissez l'empreinte ci-dessous, ou si vous savez que plusieurs machines répondent à ce nom.
ui-certificate-fingerprint = Empreinte SHA-256 : { $fingerprint }
ui-certificate-trust-button = Approuver ce certificat
ui-certificate-trust-once-button = Juste cette fois
ui-certificate-refuse-button = Ne pas se connecter

ui-prompt-submit-button = Continuer
ui-prompt-cancel-button = Annuler
ui-prompt-username-title = Nom d'utilisateur pour { $target }
ui-prompt-password-title = Mot de passe de { $user } sur { $target }
ui-prompt-password-retry = Le mot de passe a été refusé. Réessayez.
ui-prompt-passphrase-title = Phrase de passe de la clé { $path }
ui-prompt-passphrase-retry = La phrase de passe n'a pas déverrouillé la clé. Réessayez.
ui-prompt-server-password-title = Mot de passe du serveur VNC { $target }
ui-prompt-interactive-title = { $user } sur { $host } : le serveur demande
ui-prompt-server-text = Le serveur indique : { $text }

ui-session-closed = Session terminée.
ui-session-closed-status = Session terminée : le processus s'est terminé avec le code { $status }.
ui-session-cancelled = La connexion a été annulée.
ui-session-failed-title = La connexion a échoué
ui-session-close-button = Fermer l'onglet

ui-error-invalid-host = Le nom d'hôte n'est pas valide.
ui-error-invalid-username = Le nom d'utilisateur n'est pas valide.
ui-error-network = Le serveur est injoignable : { $detail }
ui-error-timeout = Le serveur n'a pas répondu à temps.
ui-error-hostkey-changed = La clé du serveur n'est pas celle enregistrée. Quelqu'un intercepte peut-être la connexion. Enregistrée : { $recorded }. Présentée : { $offered }. Si le changement est attendu, oubliez ce serveur : sa nouvelle clé est alors soumise à votre accord.
ui-error-hostkey-algorithm = Le serveur ne propose plus le type de clé enregistré ({ $recorded }).
ui-error-host-certificate = Le serveur a présenté un certificat d'hôte ; les certificats ne sont pas encore pris en charge.
ui-error-known-hosts = Le fichier des hôtes connus est inutilisable : { $detail }
ui-error-key-unreadable = Le fichier de clé { $path } n'a pas pu être lu.
ui-error-key-unknown-format = Le fichier { $path } n'est pas une clé privée dans un format pris en charge.
ui-error-key-needs-passphrase = La clé { $path } est chiffrée et aucune phrase de passe n'a été donnée.
ui-error-key-wrong-passphrase = La phrase de passe n'a pas déverrouillé la clé { $path }.
ui-error-key-invalid = La clé { $path } n'a pas pu être chargée.
ui-error-auth-failed = L'authentification a échoué. Méthodes essayées : { $methods }.
ui-error-auth-failed-none = L'authentification a échoué : le serveur n'a accepté aucune méthode que Heimdall pouvait proposer.
ui-error-disconnected = Le serveur a fermé la connexion.
ui-error-disconnected-message = Le serveur a fermé la connexion : { $message }
ui-error-connection-lost = Session déconnectée de manière inattendue.
ui-error-cancelled = Annulé.
ui-error-prompt-timeout = Une question est restée trop longtemps sans réponse.
ui-error-pty-refused = Le serveur a refusé d'ouvrir un terminal.
ui-error-shell-refused = Le serveur a refusé de lancer un shell.
ui-error-subsystem-refused = Le serveur a refusé de lancer { $name }.
ui-error-protocol = Erreur du protocole SSH : { $detail }

ui-auth-method-agent = agent SSH
ui-auth-method-key-file = fichier de clé
ui-auth-method-keyboard-interactive = clavier interactif
ui-auth-method-password = mot de passe

ui-dialog-ok-button = OK
ui-dialog-cancel-button = Annuler
ui-dialog-close-tab-title = Fermer cette session ?
ui-dialog-close-tab-body = La session est encore ouverte. Fermer l'onglet la déconnecte.
ui-dialog-close-tab-confirm = Fermer
ui-dialog-exit-title = Quitter Heimdall ?
ui-dialog-exit-body = { $count ->
    [one] { $count } session est encore ouverte et sera déconnectée.
   *[other] { $count } sessions sont encore ouvertes et seront déconnectées.
}
ui-dialog-exit-confirm = Quitter
ui-dialog-new-folder-title = Nouveau dossier
ui-dialog-new-folder-confirm = Créer
ui-dialog-rename-title = Renommer
ui-dialog-rename-confirm = Renommer
ui-dialog-name-placeholder = Nom
ui-dialog-delete-title = Supprimer ?
ui-dialog-delete-file-body = { $name } sera supprimé. C'est définitif.
ui-dialog-delete-folder-body = Le dossier { $name } et tout son contenu seront supprimés. Les liens qu'il contient sont retirés, jamais ce vers quoi ils pointent. C'est définitif.
ui-dialog-delete-confirm = Supprimer
ui-dialog-paste-title = Coller plusieurs lignes ?
ui-dialog-paste-body = { $count ->
    [one] Le texte contient { $count } ligne. Le shell peut l'exécuter comme une commande dès son arrivée.
   *[other] Le texte contient { $count } lignes. Le shell peut exécuter chacune comme une commande dès son arrivée.
}
ui-dialog-paste-confirm = Coller
ui-dialog-import-title = Import terminé
ui-dialog-import-counts = Ajoutés : { $added }. Mis à jour : { $updated }. Inchangés : { $unchanged }.
ui-dialog-import-skipped = Écartés :
ui-dialog-import-skipped-item = { $name } : { $reason }
ui-dialog-import-failed-title = L'import n'a pas pu s'exécuter
ui-import-file-title = Importer des sessions
ui-import-file-filter-all = Tous les formats
ui-import-file-filter-json = JSON
ui-import-file-filter-rdp = Fichiers RDP
ui-import-file-filter-any = Tous les fichiers
ui-import-file-confirm = { $count ->
    [one] Importer { $count } session ? Les sessions existantes avec le même ID seront mises à jour.
   *[other] Importer { $count } sessions ? Les sessions existantes avec le même ID seront mises à jour.
}
ui-import-file-confirm-mobaxterm = { $count ->
    [one] Importer { $count } session depuis MobaXterm ? Les mots de passe ne peuvent pas être importés et devront être ressaisis.
   *[other] Importer { $count } sessions depuis MobaXterm ? Les mots de passe ne peuvent pas être importés et devront être ressaisis.
}
ui-import-file-button = Importer
ui-import-file-nothing = Aucune session trouvée dans le fichier sélectionné.
ui-import-file-unreadable = Le fichier n'a pas pu être lu : { $detail }
ui-import-file-encrypted = Le fichier est entièrement chiffré. Déchiffrez-le d'abord dans mRemoteNG (Fichier > Enregistrer sous, sans chiffrement).
ui-import-file-too-large = Le fichier est trop volumineux pour être importé ({ $size } octets).
ui-import-mobaxterm-passwords = Les mots de passe MobaXterm sont chiffrés avec un algorithme propriétaire et n'ont pas pu être importés. Veuillez ressaisir les identifiants pour chaque session.
ui-import-mobaxterm-passwords-detected = { $count ->
    [one] { $count } mot de passe stocké détecté dans le fichier MobaXterm. MobaXterm le chiffre avec un algorithme propriétaire : il n'a pas été importé - veuillez ressaisir les identifiants de la session concernée.
   *[other] { $count } mots de passe stockés détectés dans le fichier MobaXterm. MobaXterm les chiffre avec un algorithme propriétaire : ils n'ont pas été importés - veuillez ressaisir les identifiants des sessions concernées.
}
ui-dialog-store-title = Le fichier des profils est inutilisable
ui-dialog-store-body = Heimdall a démarré sans profil ; les changements sont enregistrés à côté du fichier illisible, qui reste intact.
ui-dialog-detail = Détail : { $detail }

ui-import-skip-not-ssh = pas un profil SSH ({ $kind })
ui-import-skip-missing-host = n'a pas d'hôte
ui-import-skip-missing-id = n'a pas d'identifiant
ui-import-skip-invalid-port = port invalide { $port }

ui-tab-files-title = { $name } (fichiers)

ui-files-local-title = Cet ordinateur
ui-files-remote-title = Serveur
ui-files-up-button = Remonter
ui-files-refresh-button = Actualiser
ui-files-new-folder-button = Nouveau dossier
ui-files-rename-button = Renommer
ui-files-delete-button = Supprimer
ui-files-download-button = Télécharger
ui-files-upload-button = Envoyer
ui-files-loading = Chargement...
ui-files-empty = Dossier vide
ui-files-cancel-button = Annuler
ui-files-transfers-title = Transferts
ui-files-transfer-download = Téléchargement de { $name }
ui-files-transfer-upload = Envoi de { $name }
ui-files-state-running = { $done } sur { $total }
ui-files-state-running-unknown = { $done }
ui-files-state-done = Terminé
ui-files-state-incomplete = Terminé, { $count ->
    [one] { $count } élément laissé de côté (un lien, un nom inutilisable ou un échec)
   *[other] { $count } éléments laissés de côté (liens, noms inutilisables ou échecs)
}
ui-files-state-cancelled = Annulé ; relancez-le pour reprendre
ui-files-state-failed = Échec : { $reason }
ui-files-size-bytes = { $value } o
ui-files-size-kib = { $value } Kio
ui-files-size-mib = { $value } Mio
ui-files-size-gib = { $value } Gio

ui-files-error-server = Le serveur a refusé : { $message }
ui-files-error-no-such-file = le fichier n'existe pas
ui-files-error-permission-denied = permission refusée
ui-files-error-unsupported = le serveur ne prend pas en charge cette opération
ui-files-error-failure = l'opération a échoué
ui-files-error-session = La session SFTP est terminée.
ui-files-error-local = Cet ordinateur a refusé : { $detail }
ui-files-error-unsafe-name = Le nom du serveur "{ $name }" n'est pas utilisable ici : { $reason }.
ui-files-error-not-a-file = Seuls les fichiers et les dossiers peuvent être transférés, pas les liens ni les fichiers spéciaux.
ui-files-error-too-large = Le dossier contient trop d'éléments pour être parcouru.
ui-files-error-invalid-name = Ce nom ne peut pas être utilisé.
ui-files-error-exists = Un élément de ce nom existe déjà.
ui-files-name-not-a-name = ce n'est pas un nom de fichier
ui-files-name-separator = il contient un séparateur de dossiers
ui-files-name-control = il contient un caractère de contrôle
ui-files-name-forbidden = il contient "{ $character }"
ui-files-name-reserved = c'est un nom réservé par Windows
ui-files-name-trailing = il se termine par un point ou une espace
ui-files-name-too-long = il est trop long

ui-profile-new-title = Ajouter une session
ui-profile-edit-title = Modifier la session
ui-profile-field-name = Nom d'affichage *
ui-profile-field-group = Dossier
ui-profile-field-host = Serveur *
ui-profile-field-port = Port
ui-profile-field-username = Nom d'utilisateur
ui-profile-field-key = Clé SSH
ui-profile-optional = facultatif
ui-profile-host-placeholder = serveur.exemple.fr
ui-profile-save-button = Enregistrer
ui-profile-error-name-missing = Donnez un nom au profil.
ui-profile-error-host-missing = Saisissez l'adresse du serveur.
ui-profile-error-host-invalid = L'adresse du serveur ne peut pas contenir d'espace.
ui-profile-error-host-has-user = Mettez le nom d'utilisateur dans son propre champ, pas dans l'adresse.
ui-profile-error-host-has-port = Mettez le port dans son propre champ, pas dans l'adresse.
ui-profile-error-port-invalid = Le port est un nombre de 1 à 65535.
ui-profile-error-username-invalid = Le nom d'utilisateur ne peut pas contenir d'espace.
ui-profile-error-control = Un champ contient un caractère de contrôle.
ui-dialog-delete-profile-title = Supprimer le profil ?
ui-dialog-delete-profile-body = Le profil { $name } sera supprimé. Les sessions ouvertes restent ouvertes. C'est définitif.
ui-dialog-delete-profile-confirm = Supprimer

ui-session-forget-server-button = Oublier ce serveur
ui-session-reconnect-button = Reconnecter
ui-session-accept-new-key-button = Accepter la nouvelle empreinte (destructif)
ui-error-security-refused = Le serveur a refusé la sécurité qu'exige Heimdall (authentification au niveau du réseau) : { $detail }
ui-error-rdp-protocol = Erreur RDP : { $detail }
ui-error-vnc-protocol = Erreur VNC : { $detail }
ui-session-vnc-unencrypted = Non chiffré : ce bureau et ce que vous tapez traversent le réseau en clair.
ui-sidebar-local-shell-button = Shell local
ui-local-shell-name = Shell local
ui-local-starting = Démarrage de { $name }...
ui-error-local-shell = Le shell local n'a pas pu démarrer : { $detail }
ui-import-skip-elevation = s'exécute en administrateur, pas encore pris en charge
ui-import-skip-post-connect = exécute des commandes après le démarrage, pas encore pris en charge
ui-import-skip-unsafe-local = son programme, ses arguments ou son dossier ne peuvent pas être lancés tels quels (chemin relatif, guillemet, caractère NUL, ou dossier sur une autre machine)
ui-dialog-local-title = Lancer ce programme ?
ui-dialog-local-body = Le profil { $name } lance la commande ci-dessous. Heimdall ne la lance qu'avec votre accord, et redemande si elle change.
ui-dialog-local-folder = Démarre dans : { $folder }
ui-dialog-local-rereads = Ce programme relit sa ligne de commande avec ses propres règles : & | ^ < > et % y sont des commandes, pas du texte.
ui-dialog-local-confirm = Lancer
ui-error-remote-forward = La passerelle SSH n'a pas voulu écouter sur son port { $port } pour la redirection distante : la redirection y est désactivée, ou le port y est déjà pris.
ui-error-proxy-port = Le proxy SOCKS n'a pas pu ouvrir le port local { $port } : un autre programme l'utilise peut-être. ({ $detail })
ui-error-jump-refused = La passerelle SSH n'a pas voulu se connecter à { $target } : la redirection y est désactivée, ou cet hôte n'est pas joignable depuis elle.
ui-import-skip-missing-gateway = passe par une passerelle SSH absente du fichier, ou écartée
ui-import-skip-gateway-loop = sa passerelle SSH est atteinte par elle-même, via ses parents
ui-import-skip-missing-username = se connecte avec un compte qu'il ne nomme pas
ui-import-skip-unknown-identity = se connecte avec un mode d'identité que Heimdall ne connaît pas
ui-error-hostkey-changed-at = La clé d'hôte de { $target } n'est pas celle enregistrée : la connexion est peut-être interceptée. Enregistrée : { $recorded }. Présentée : { $offered }.
ui-error-gateway-missing = La passerelle SSH { $id } par laquelle passe ce profil n'est pas dans les profils.
ui-error-gateway-loop = La passerelle SSH { $id } est atteinte par elle-même, via ses parents.
ui-tree-connect = Connecter
ui-tree-connect-as = Se connecter avec un autre protocole...
ui-tree-edit = Modifier
ui-tree-duplicate = Dupliquer
ui-tree-duplicate-suffix = {" "}(copie)
ui-tree-copy-hostname = Copier le nom d'hôte
ui-tree-copy-username = Copier l'identifiant
ui-tree-copy-address = Copier l'adresse
ui-tree-copy-ssh-command = Copier la commande SSH
ui-tree-delete = Supprimer
ui-tree-add-session = Ajouter une session
ui-tree-import-sessions = Importer des sessions
ui-tree-export-sessions = Exporter les sessions
ui-dialog-export-title = Exporter les sessions
ui-dialog-export-done = { $count ->
    [one] { $count } session exportée avec succès.
   *[other] { $count } sessions exportées avec succès.
}
ui-dialog-export-credentials = Les identifiants n'ont pas été inclus dans le fichier exporté.
ui-dialog-export-failed = Échec de l'export : { $detail }
ui-export-filter-json = Fichiers JSON
ui-tree-import-openssh = Importer une config OpenSSH...
ui-openssh-title = Importer une config OpenSSH
ui-openssh-summary = { $total ->
    [one] { $total } candidat
   *[other] { $total } candidats
} - { $new ->
    [one] { $new } nouveau
   *[other] { $new } nouveaux
}, { $duplicate ->
    [one] { $duplicate } doublon
   *[other] { $duplicate } doublons
}
ui-openssh-hint = Les entrées ProxyJump sont importées comme chaînes de passerelles SSH.
ui-openssh-choose-all = Tout importer
ui-openssh-column-alias = Alias
ui-openssh-column-host = HostName
ui-openssh-column-port = Port
ui-openssh-column-user = Utilisateur
ui-openssh-column-key = IdentityFile
ui-openssh-column-chain = Chaîne de passerelles
ui-openssh-column-status = Statut
ui-openssh-status-new = Nouveau
ui-openssh-status-duplicate = Doublon
ui-openssh-reusing = réutilise la passerelle existante "{ $name }"
ui-openssh-diagnostics = Diagnostics ({ $count })
ui-openssh-diag-line = Ligne { $line } : { $said }
ui-openssh-diag-match = Bloc Match non lu
ui-openssh-diag-include = Directive Include non suivie : { $value }
ui-openssh-diag-wildcard = Alias avec wildcard ignoré : { $value }
ui-openssh-diag-unknown = Directive inconnue ignorée : { $value }
ui-openssh-diag-port = Port invalide { $value } ; repli sur 22
ui-openssh-diag-duplicate = Alias dupliqué dans le fichier ignoré : { $value }
ui-openssh-diag-proxycommand = ProxyCommand n'est pas pris en charge ; Heimdall prend uniquement en charge les sauts TCP natifs via ProxyJump : { $value }
ui-openssh-diag-mixed = ProxyJump et ProxyCommand sont combinés ; utilisez uniquement ProxyJump pour l'import Heimdall : { $value }
ui-openssh-diag-jump-token = ProxyJump avec substitutions OpenSSH (%h/%p/%r) n'est pas pris en charge : { $value }
ui-openssh-diag-cycle = Cycle ProxyJump détecté dans la chaîne pour Host { $value }
ui-openssh-diag-syntax = Syntaxe ProxyJump non reconnue : { $value }
ui-openssh-diag-tilde = IdentityFile ~ remplacé par le dossier personnel : { $value }
ui-openssh-diag-fallback = HostName absent ; repli sur l'alias : { $value }
ui-openssh-diag-host-token = HostName utilise une substitution OpenSSH que Heimdall ne peut pas développer (seul %h est pris en charge) ; hôte ignoré : { $value }
ui-openssh-import-button = Importer
ui-openssh-done = { $imported ->
    [one] { $imported } importé
   *[other] { $imported } importés
}, { $duplicates ->
    [one] { $duplicates } ignoré
   *[other] { $duplicates } ignorés
} (doublons), { $warnings ->
    [one] { $warnings } avertissement
   *[other] { $warnings } avertissements
}
ui-openssh-done-gateways = { $count ->
    [one] { $count } passerelle SSH créée pour les chaînes ProxyJump.
   *[other] { $count } passerelles SSH créées pour les chaînes ProxyJump.
}
ui-openssh-unreadable = Impossible de lire le fichier sélectionné : { $detail }
ui-openssh-empty = Le fichier sélectionné ne contient aucune entrée importable.
ui-tree-import-putty = Importer des sessions PuTTY...
ui-putty-title = Importer des sessions PuTTY
ui-sessions-summary-invalid = { $total ->
    [one] { $total } candidat
   *[other] { $total } candidats
} - { $new ->
    [one] { $new } nouveau
   *[other] { $new } nouveaux
}, { $duplicate ->
    [one] { $duplicate } doublon
   *[other] { $duplicate } doublons
}, { $invalid ->
    [one] { $invalid } invalide
   *[other] { $invalid } invalides
}
ui-sessions-status-invalid = Invalide
ui-sessions-no-host = (aucun hôte)
ui-putty-diag-default = Paramètres par défaut PuTTY ignorés : { $session }
ui-putty-diag-not-ssh = Session "{ $session }" ignorée car le protocole "{ $value }" n'est pas SSH
ui-putty-diag-missing-host = La session "{ $session }" n'a pas de nom d'hôte et sera marquée invalide
ui-putty-diag-port = La session "{ $session }" a un port invalide "{ $value }" ; repli sur 22
ui-putty-diag-ppk = La session "{ $session }" référence une clé .ppk conservée sans conversion : { $value }
ui-putty-diag-proxy = La session "{ $session }" définit un proxy capturé mais non mappé : { $value }
ui-putty-diag-forwards = La session "{ $session }" définit { $count ->
    [one] { $count } tunnel capturé mais non mappé
   *[other] { $count } tunnels capturés mais non mappés
}
ui-putty-diag-command = La session "{ $session }" définit une commande de démarrage capturée mais non mappée : { $value }
ui-putty-done = { $imported ->
    [one] { $imported } importé
   *[other] { $imported } importés
}, { $duplicates ->
    [one] { $duplicates } ignoré
   *[other] { $duplicates } ignorés
} (doublons), { $invalid ->
    [one] { $invalid } invalide
   *[other] { $invalid } invalides
}, { $warnings ->
    [one] { $warnings } avertissement
   *[other] { $warnings } avertissements
}
ui-putty-unreadable = Les sessions PuTTY n'ont pas pu être lues : { $detail }
ui-putty-empty = Aucune session SSH PuTTY trouvée.
ui-tree-import-rdp = Importer des fichiers RDP...
ui-rdp-title = Importer des fichiers .rdp
ui-rdp-filter = Fichiers Bureau à distance
ui-rdp-summary = { $chosen ->
    [one] { $chosen } sélectionné
   *[other] { $chosen } sélectionnés
} / { $files ->
    [one] { $files } fichier
   *[other] { $files } fichiers
}, { $conflicts ->
    [one] { $conflicts } conflit
   *[other] { $conflicts } conflits
}, { $passwords ->
    [one] { $passwords } avertissement mot de passe.
   *[other] { $passwords } avertissements mot de passe.
}
ui-rdp-unreadable = { $count ->
    [one] { $count } fichier n'a pas pu être lu.
   *[other] { $count } fichiers n'ont pas pu être lus.
}
ui-rdp-select-all = Tout sélectionner
ui-rdp-select-none = Tout désélectionner
ui-rdp-apply-all = Appliquer à tous les conflits :
ui-rdp-column-source = Source
ui-rdp-column-name = Nom
ui-rdp-column-host = Hôte
ui-rdp-column-status = Statut
ui-rdp-column-conflict = Conflit
ui-rdp-conflict-skip = Ignorer
ui-rdp-conflict-replace = Remplacer
ui-rdp-conflict-rename = Renommer auto
ui-rdp-status-invalid-address = Adresse cible RDP manquante ou invalide.
ui-rdp-status-rd-gateway = Passe par une passerelle Bureau à distance, pas encore pris en charge
ui-rdp-status-conflict = Conflit avec { $name }
ui-rdp-status-password = Mot de passe non importé
ui-rdp-status-partial = Mapping partiel
ui-rdp-status-unknown = { $count ->
    [one] { $count } clé inconnue
   *[other] { $count } clés inconnues
}
ui-rdp-import-button = Importer la sélection
ui-rdp-rename = { $name } (Importé { $n })
ui-rdp-fallback-name = RDP importé
ui-rdp-done = { $imported ->
    [one] { $imported } importé
   *[other] { $imported } importés
}, { $replaced ->
    [one] { $replaced } remplacé
   *[other] { $replaced } remplacés
}, { $renamed ->
    [one] { $renamed } renommé auto
   *[other] { $renamed } renommés auto
}, { $skipped ->
    [one] { $skipped } ignoré
   *[other] { $skipped } ignorés
}, { $passwords ->
    [one] { $passwords } mot de passe ignoré.
   *[other] { $passwords } mots de passe ignorés.
}
ui-rdp-nothing = Aucun fichier .rdp valide à importer.
ui-tree-import-known-hosts = Importer des hôtes SSH de confiance...
ui-hostkeys-title = Importer les hôtes SSH de confiance
ui-hostkeys-pick-title = Sélectionner le fichier known_hosts
ui-hostkeys-summary = { $total ->
    [one] { $total } entrée
   *[other] { $total } entrées
} : { $new } nouvelles, { $existing } déjà approuvées, { $conflicts } en conflit
ui-hostkeys-column-host = Hôte
ui-hostkeys-column-type = Type
ui-hostkeys-column-fingerprint = Empreinte
ui-hostkeys-column-notes = Notes
ui-hostkeys-status-new = Nouvelle
ui-hostkeys-status-existing = Déjà approuvée
ui-hostkeys-status-conflict = Conflit
ui-hostkeys-note-existing = Empreinte déjà approuvée à l'identique
ui-hostkeys-note-conflict-store = Conflit avec l'empreinte déjà approuvée
ui-hostkeys-note-conflict-file = Plusieurs empreintes différentes pour cet hôte dans le fichier source
ui-hostkeys-diag-hashed = Les entrées known_hosts hachées ne sont pas prises en charge (ligne { $line }).
ui-hostkeys-diag-cert-authority = Le marqueur @cert-authority n'est pas pris en charge (ligne { $line }).
ui-hostkeys-diag-revoked = Le marqueur @revoked n'est pas pris en charge (ligne { $line }).
ui-hostkeys-diag-pattern = Motif d'hôte non pris en charge ligne { $line } : { $value }
ui-hostkeys-diag-key-type = Type de clé non pris en charge ligne { $line } : { $value }
ui-hostkeys-diag-malformed = Ligne malformée { $line } : { $value }
ui-hostkeys-malformed-too-long = ligne trop longue
ui-hostkeys-malformed-fields = { $count ->
    [one] { $count } champ au lieu de 3
   *[other] { $count } champs au lieu de 3
}
ui-hostkeys-malformed-bad-key = la clé est illisible
ui-hostkeys-malformed-marker = marqueur inconnu { $marker }
ui-hostkeys-done = { $imported } importées, { $existing } ignorées (déjà approuvées), { $conflicts } ignorées (conflit), { $warnings ->
    [one] { $warnings } avertissement
   *[other] { $warnings } avertissements
}
ui-hostkeys-empty = Aucune entrée exploitable trouvée dans le fichier known_hosts sélectionné.
ui-hostkeys-unreadable = Le fichier n'a pas pu être lu : { $detail }
ui-hostkeys-too-large = Le fichier est trop volumineux pour être importé ({ $size } octets).
ui-trusted-host-keys-import = Importer known_hosts
ui-tree-add-tooltip = Ajouter une session
ui-tree-more-tooltip = Autres actions
ui-tree-tooltip-host = Hôte : { $host }
ui-tree-tooltip-user = Utilisateur : { $user }
ui-tree-tooltip-protocol = Protocole : { $protocol }
ui-profile-protocol-picker-title = Choisir un protocole
ui-profile-protocol-picker-desc = Sélectionnez le type de connexion à configurer.
ui-profile-protocol-rdp-name = Bureau à distance
ui-profile-protocol-rdp-desc = Session de bureau à distance Windows
ui-profile-protocol-ssh-name = SSH
ui-profile-protocol-ssh-desc = Terminal sécurisé
ui-profile-protocol-winrm-name = WinRM
ui-profile-protocol-winrm-desc = Session PowerShell Remoting
ui-profile-protocol-sftp-name = SFTP
ui-profile-protocol-sftp-desc = Transfert de fichiers sécurisé via SSH
ui-profile-protocol-ftp-name = FTP
ui-profile-protocol-ftp-desc = Transfert de fichiers classique
ui-profile-port-ftp = Port FTP
ui-profile-credentials-ftp = Authentification FTP
ui-profile-credentials-ftp-desc = Saisissez le nom d'utilisateur et le mot de passe FTP. Laissez vide pour un accès anonyme.
ui-profile-options-ftp = Options FTP
ui-profile-toggle-passive = Mode passif (recommandé pour les réseaux derrière un pare-feu)
ui-profile-toggle-ftps = Activer SSL/TLS (FTPS)
ui-profile-protocol-local-name = Shell local
ui-profile-protocol-local-desc = Session de terminal local
ui-profile-section-basics-local-desc = Nommez la session telle que l'arborescence l'affiche.
ui-profile-local-title = Shell local
ui-profile-local-desc = Configurez l'exécutable du shell et les arguments de démarrage.
ui-profile-local-executable = Exécutable
ui-profile-local-default-shell = Le shell par défaut
ui-profile-local-presets = Shells courants
ui-profile-local-arguments = Arguments
ui-profile-local-advanced-title = Options avancées du shell
ui-profile-local-advanced-desc = Le dossier dans lequel le shell démarre.
ui-profile-local-working-directory = Répertoire de travail
ui-profile-error-local-arguments = Les arguments laissent un guillemet ouvert.
ui-profile-protocol-vnc-name = VNC
ui-profile-protocol-vnc-desc = Partage d'écran à distance
ui-profile-protocol-telnet-name = Telnet
ui-profile-protocol-telnet-desc = Terminal non chiffré (legacy)
ui-profile-protocol-badge = Protocole
ui-profile-section-basics = Paramètres de connexion
ui-profile-section-basics-desc = Indiquez l'hôte de destination et le port de service que Heimdall doit ouvrir.
ui-profile-port-rdp = Port RDP distant
ui-profile-port-ssh = Port SSH distant
ui-profile-port-winrm = Port WinRM
ui-profile-port-vnc = Port VNC
ui-profile-port-telnet = Port Telnet
ui-profile-credentials-rdp = Identifiants RDP
ui-profile-credentials-rdp-desc = Identifiants utilisés par la session Bureau à distance une fois le routage terminé.
ui-profile-credentials-ssh = Identifiants SSH
ui-profile-credentials-ssh-desc = Ces identifiants sont utilisés pour la session SSH ou SFTP elle-même.
ui-profile-credentials-winrm = Identifiants WinRM
ui-profile-credentials-winrm-desc = PowerShell Remoting utilise l'identité Windows courante ou un identifiant stocké.
ui-profile-credentials-vnc = Authentification VNC
ui-profile-username-rdp-placeholder = utilisateur, DOMAINE\utilisateur ou user@domaine
ui-profile-field-domain = Domaine Windows
ui-profile-domain-placeholder = CORP ou corp.example.com
ui-profile-domain-hint = Le nom NetBIOS (CORP) ou le domaine DNS (corp.example.com). Laissez vide si le nom d'utilisateur ci-dessus en porte déjà un.
ui-profile-winrm-identity = Identité
ui-profile-winrm-identity-current = Identité Windows courante
ui-profile-winrm-identity-stored = Identifiant stocké
ui-profile-options-rdp = Options de session RDP
ui-profile-options-vnc = Options VNC
ui-profile-options-telnet = Options Telnet
ui-profile-toggle-clipboard = Rediriger le presse-papiers
ui-profile-toggle-drives = Rediriger les lecteurs
ui-profile-toggle-nla = Activer l'authentification au niveau du réseau
ui-profile-rdp-follow-defaults = Utiliser les valeurs RDP par défaut globales
ui-profile-rdp-defaults-banner = Ce serveur utilise vos valeurs RDP par défaut. Décochez "Utiliser les valeurs RDP par défaut globales" pour configurer des options propres à ce serveur.
ui-profile-rdp-defaults-not-in-effect = Les couleurs, le son, le presse-papiers, les lecteurs, l'authentification au niveau du réseau et la résolution dynamique viennent des valeurs par défaut globales : les valeurs affichées ci-dessous pour eux sont celles propres à ce serveur, pas celles appliquées.
ui-settings-rdp-defaults = Paramètres RDP
ui-settings-rdp-defaults-hint = Les options de tout serveur RDP qui utilise les valeurs par défaut globales.
ui-profile-toggle-admin = Session administrateur (/admin)
ui-profile-audio = Mode audio
ui-profile-audio-off = Désactivé
ui-profile-audio-local = Lecture locale
ui-profile-audio-on-server = Lecture distante
ui-profile-color-depth = Profondeur de couleur
ui-profile-color-16 = 16 bits
ui-profile-color-24 = 24 bits
ui-profile-color-32 = 32 bits
ui-profile-resolution-title = Profil de résolution
ui-profile-resolution-desc = Choisissez comment ce serveur dimensionne la session Bureau à distance embarquée.
ui-profile-resolution-mode = Mode de résolution
ui-profile-resolution-fit-window = Adapter à la fenêtre
ui-profile-resolution-fixed = Fixe
ui-profile-resolution-smart-sizing = Mise à l'échelle intelligente
ui-profile-resolution-presets = Résolutions courantes
ui-profile-resolution-custom = Personnalisé...
ui-profile-resolution-preset = { $width }x{ $height }
ui-profile-resolution-width = Largeur
ui-profile-resolution-height = Hauteur
ui-profile-resolution-scale-fixed = Mettre la résolution fixe à l'échelle du panneau
ui-profile-resolution-dynamic = Autoriser la résolution dynamique
ui-profile-nla-off-hint = Sans authentification au niveau du réseau, un mot de passe enregistré n'est pas envoyé : Heimdall le demande.
ui-profile-toggle-use-ssl = Utiliser SSL
ui-profile-toggle-skip-cert = Ignorer la validation du certificat (non sûr)
ui-profile-toggle-view-only = Mode lecture seule (pas de clavier ni de souris)
ui-profile-toggle-no-password = Autoriser un serveur qui ne demande pas de mot de passe
ui-profile-telnet-warning = Telnet envoie tout sans chiffrement, mots de passe compris.
ui-profile-section-organization = Organisation
ui-profile-folder-placeholder = Production/Bases de données
ui-profile-browse-button = Parcourir...
ui-profile-browse-key-title = Sélectionner la clé SSH
ui-profile-browse-key-all = Tous les fichiers
ui-profile-browse-key-ppk = Fichiers PPK
ui-profile-browse-key-pem = Fichiers PEM
ui-profile-folder-hint = Utilisez / pour imbriquer les dossiers : Production/Bases de données place cette session dans Bases de données, dans Production.
ui-profile-error-username-missing = Le nom d'utilisateur est requis.
ui-profile-error-domain-invalid = Le domaine ne peut contenir ni espace ni guillemet double.
ui-profile-error-fixed-width = La largeur fixe RDP doit être comprise entre { $min } et { $max }.
ui-profile-error-fixed-height = La hauteur fixe RDP doit être comprise entre { $min } et { $max }.
ui-profile-error-socks-port = Le port SOCKS5 doit être un nombre de 0 à 65535 ; 0 désactive le proxy.
ui-profile-error-remote-bind-port = Le port distant doit être un nombre de 0 à 65535 ; 0 désactive la redirection.
ui-profile-error-remote-local-port = Le port local doit être un nombre de 0 à 65535 ; 0 reprend le port distant.
ui-profile-gateway-routing = Routage par passerelle
ui-profile-gateway-routing-desc = Utilisez une passerelle SSH lorsque le serveur cible n'est accessible que via un bastion ou un serveur de rebond.
ui-profile-direct-connect = Connexion directe sans passerelle SSH
ui-profile-gateway-direct-hint = La connexion directe est sélectionnée. Décochez-la pour faire passer cette session par une passerelle.
ui-profile-gateway-explain-tunnel = Le trafic sera acheminé via cette passerelle SSH.
ui-profile-socks-title = Proxy SOCKS5
ui-dialog-post-connect-title = Exécuter les commandes post-connexion ?
ui-dialog-post-connect-body = { $count ->
    [one] "{ $name }" a été importé et va exécuter automatiquement { $count } commande dans cette session. Ne continuez que si vous faites confiance à ce profil. L'exécuter et mémoriser ce choix ?
   *[other] "{ $name }" a été importé et va exécuter automatiquement { $count } commandes dans cette session. Ne continuez que si vous faites confiance à ce profil. Les exécuter et mémoriser ce choix ?
}
ui-dialog-post-connect-run = Exécuter et mémoriser
ui-dialog-post-connect-skip = Se connecter sans elles
ui-profile-toggle-forward-agent = Transférer l'agent SSH
ui-profile-toggle-compression = Activer la compression
ui-profile-options-ssh = Options SSH
ui-post-connect-title = Séquence post-connexion
ui-post-connect-hint = Ces étapes s'exécutent après que la session SSH embarquée est prête. Les délais s'appliquent avant chaque étape.
ui-post-connect-empty = Aucune étape pour l'instant. Ajoutez une étape pour envoyer des commandes automatiquement une fois la session connectée.
ui-post-connect-command = Commande
ui-post-connect-command-placeholder = Commande à envoyer, par exemple : sudo -i
ui-post-connect-delay = Délai (ms)
ui-post-connect-on-failure = En cas d'échec
ui-post-connect-failure-continue = Continuer
ui-post-connect-failure-stop = Arrêter la séquence
ui-post-connect-order-hint = L'ordre compte. Les délais s'appliquent avant chaque étape activée.
ui-post-connect-add = Ajouter
ui-post-connect-remove = Supprimer
ui-post-connect-move-up = Monter
ui-post-connect-move-down = Descendre
ui-post-connect-tooltip = { $progress } - { $status } - { $command }
ui-post-connect-running = En cours
ui-post-connect-completed = Terminé
ui-post-connect-failed = Échec
ui-post-connect-skipped = Ignoré
ui-post-connect-cancelled = Annulé
ui-profile-socks-desc = Ouvre un port proxy SOCKS5 local via la passerelle. Mettre à 0 pour désactiver.
ui-profile-socks-port = Port local
ui-profile-socks-off = Désactivé
ui-profile-remote-title = Redirection de port inverse
ui-profile-remote-desc = Ouvre un port sur le serveur SSH et redirige les connexions vers un port local. Le port distant est obligatoire ; laissez le port local à 0 pour utiliser la même valeur.
ui-profile-remote-bind-port = Port distant (serveur)
ui-profile-remote-local-port = Port local
ui-profile-remote-local-hint = 0 = identique au port distant
ui-profile-remote-route = serveur:{ $remote } -> local:{ $local }
ui-profile-gateway-explain-direct = Sélectionnez une passerelle si le serveur n'est accessible que via un hôte SSH intermédiaire.
ui-profile-edit-gateway = Modifier les identifiants de la passerelle...
ui-gateway-list-empty = Aucune passerelle configurée
ui-gateway-empty-hint = Ajoutez une passerelle SSH pour établir des connexions sécurisées vers vos sessions.
ui-gateway-add = Ajouter une passerelle
ui-gateway-add-title = Ajouter une passerelle SSH
ui-gateway-edit-title = Modifier la passerelle SSH
ui-gateway-field-name = Nom
ui-gateway-field-host = Hôte
ui-gateway-field-port = Port
ui-gateway-field-username = Nom d'utilisateur
ui-gateway-field-key = Chemin de la clé
ui-gateway-field-password = Mot de passe
ui-gateway-password-hint = Utilisé pour l'authentification par mot de passe SSH. Laissez vide pour une clé seule ou un agent SSH.
ui-gateway-field-parent = Passerelle parente
ui-gateway-parent-none = Aucune (connexion directe)
ui-gateway-error-loop = Une passerelle ne peut pas être atteinte par elle-même.
ui-tree-gateway-via = via { $name }
ui-tree-gateway-missing = passerelle manquante

ui-tab-menu-disconnect = Déconnecter
ui-tab-menu-rename = Renommer l'onglet
ui-tab-menu-reset-title = Réinitialiser le titre
ui-tab-menu-fullscreen = Plein écran (F11)
ui-tab-menu-reconnect = Reconnecter la session
ui-tab-menu-duplicate = Dupliquer la session
ui-tab-menu-close-others = Fermer les autres
ui-tab-menu-close-right = Fermer celles à droite
ui-dialog-rename-tab-title = Renommer l'onglet
ui-dialog-rename-tab-prompt = Entrez le nouveau nom :
ui-dialog-close-tabs-title = Fermer les sessions
ui-dialog-close-tabs-body = Sessions à fermer : { $count }. Encore connectées : { $live }. Continuer ?

ui-session-copy-error-button = Copier l'erreur
ui-session-edit-profile-button = Modifier le profil
ui-error-report-header = Rapport d'erreur { $protocol } de Heimdall
ui-error-report-time = Heure :
ui-error-report-server = Serveur :
ui-error-report-app = Application :

ui-error-network-refused = Connexion refusée.
ui-error-network-reset = Connexion réinitialisée.
ui-error-network-timed-out = Délai de connexion dépassé. Vérifiez que l'hôte est joignable.
ui-error-network-unreachable = Hôte ou réseau inaccessible. Vérifiez le DNS et le routage.
ui-session-closed-reason = Raison : { $reason }

## Why an RDP server refused a logon or ended a session, as the C# Heimdall says it.
ui-rdp-severity-warning = Attention :
ui-rdp-severity-error = Erreur :
ui-rdp-reason-bad-credentials = Les identifiants n'ont pas été acceptés. Vérifiez votre nom d'utilisateur, mot de passe et domaine (NetBIOS DOMAIN\utilisateur ou UPN utilisateur@domaine.com), puis réessayez.
ui-rdp-reason-password-expired = Le mot de passe a expiré et doit être changé avant la connexion.
ui-rdp-reason-account-locked-out = Le compte est actuellement verrouillé. Attendez la fin du verrouillage, ou demandez à votre administrateur de le déverrouiller.
ui-rdp-reason-account-disabled = Le compte est désactivé sur l'ordinateur distant. Demandez à votre administrateur de l'activer, puis réessayez de vous connecter.
ui-rdp-reason-account-expired = Le compte a expiré. Demandez à votre administrateur de le renouveler.
ui-rdp-reason-time-of-day = Le compte n'est pas autorisé à ouvrir une session à cette heure. Une restriction sur les plages horaires du compte a mis fin à la session.
ui-rdp-reason-no-authority = Aucune autorité d'authentification n'a pu être jointe pour valider le compte. L'ordinateur distant a peut-être perdu le contact avec son contrôleur de domaine.
ui-rdp-reason-clock-skew = Les horloges de cet ordinateur et de l'ordinateur distant sont trop éloignées pour que l'authentification aboutisse. Corrigez l'heure système de l'un ou l'autre, puis réessayez de vous connecter.
ui-rdp-reason-security-error = Une erreur de sécurité a empêché la connexion. L'ordinateur distant a signalé que les données de sécurité échangées pendant la connexion n'étaient pas valides.
ui-rdp-reason-admin-disconnect = L'ordinateur distant a mis fin à la session. Un administrateur a pu y mettre fin, la connexion a pu échouer pendant son établissement, ou un problème réseau a pu l'interrompre.
ui-rdp-reason-license = Une erreur de licence Bureau à distance a bloqué la session. Contactez votre administrateur; le serveur de licences est peut-être inaccessible ou n'a plus de CAL disponible.
ui-rdp-reason-with-severity = { $severity } { $reason }
ui-rdp-certificate-refused = Connexion annulée : vous n'avez pas approuvé le certificat présenté par ce serveur.

ui-session-reconnecting = Reconnexion (tentative { $attempt }/{ $max })...
ui-session-reconnecting-in = dans { $seconds }s
ui-session-reconnecting-cancel = Annuler

ui-folder-connect-all = Tout connecter ({ $count })
ui-folder-new = Nouveau dossier
ui-folder-rename = Renommer
ui-folder-move-to = Déplacer vers
ui-folder-move-top = Premier niveau
ui-folder-delete = Supprimer le dossier
ui-folder-new-title = Nouveau dossier
ui-folder-rename-title = Renommer le dossier
ui-folder-name-field = Nom du dossier :
ui-folder-error-collision = Un dossier portant ce nom existe déjà au même niveau.
ui-folder-error-invalid = Un nom de dossier ne peut pas être vide ni contenir "/".
ui-folder-delete-body = Supprimer le dossier "{ $name }" ? Entrées concernées dans ce dossier et ses sous-dossiers, y compris celles masquées par le filtre actuel : { $count }. Toutes seront déplacées vers "(Sans dossier)".
ui-folder-connect-all-title = Tout connecter
ui-folder-connect-all-body = Se connecter aux { $count } sessions de ce dossier ?
ui-folder-connect-all-confirm = Connecter

ui-tree-rename = Renommer
ui-tree-rename-title = Renommer la session
ui-tree-move-to-folder = Déplacer vers le dossier

ui-selection-count = { $count ->
    [one] { $count } élément sélectionné
   *[other] { $count } éléments sélectionnés
}
ui-selection-connect = Connecter la sélection ({ $count })
ui-selection-duplicate = Dupliquer la sélection
ui-selection-delete = Supprimer la sélection ({ $count })
ui-dialog-delete-selection-title = Supprimer les éléments sélectionnés
ui-dialog-delete-selection-body = Êtes-vous sûr de vouloir supprimer { $count ->
    [one] { $count } élément sélectionné
   *[other] { $count } éléments sélectionnés
} ?

ui-palette-placeholder = Rechercher un hôte ou une IP... (Ctrl+K)
ui-palette-ssh-to = [SSH] Connexion vers { $target }
ui-palette-rdp-to = [RDP] Connexion vers { $target }
ui-palette-quick-connect = Connexion rapide
ui-palette-nothing = Aucune session ne correspond, et ce n'est pas un hôte auquel se connecter.

ui-status-ready = Prêt. Sélectionnez une session pour commencer.
ui-status-connected = Connecté à : { $name }
ui-status-state = { $name } : { $state }
ui-status-connecting = Connexion...
ui-status-reconnecting = Reconnexion...
ui-status-disconnected = Déconnecté
ui-status-error = Erreur
ui-status-copied = Copié dans le presse-papiers : { $text }
ui-status-folder-created = Dossier "{ $path }" créé.
ui-status-sessions = { $count ->
    [one] { $count } session
   *[other] { $count } sessions
}
ui-status-sessions-filtered = { $shown } sur { $count ->
    [one] { $count } session
   *[other] { $count } sessions
}

ui-find-placeholder = Rechercher...
ui-find-previous = ▲
ui-find-next = ▼
ui-find-close = ✕
ui-find-nothing = Aucune correspondance

ui-settings-terminal = Apparence du terminal
ui-settings-color-scheme = Palette de couleurs
ui-settings-appearance = Apparence
ui-vault-problem-locked-out = Trop de tentatives incorrectes. Réessayez dans { $minutes ->
    [one] { $minutes } minute.
   *[other] { $minutes } minutes.
}
ui-settings-language = Langue
ui-settings-language-en = Anglais
ui-settings-language-fr = Français
ui-settings-language-es = Español
ui-settings-font-size = Taille de police
ui-settings-font-size-unit = px
ui-settings-font-size-refused = La taille de police du terminal doit être entre { $min } et { $max }.
ui-scheme-default = Par défaut
ui-scheme-dracula = Dracula
ui-scheme-solarized-dark = Solarized Dark
ui-scheme-monokai = Monokai
ui-scheme-nord = Nord

ui-tab-menu-start-transcript = Démarrer la transcription
ui-tab-menu-stop-transcript = Arrêter la transcription
ui-tab-recording = REC
ui-tab-recording-tooltip = La sortie de la session est enregistrée
ui-status-transcript-started = Transcription démarrée : { $path }
ui-status-transcript-stopped = Transcription arrêtée
ui-status-transcript-failed = La transcription n'a pas pu être écrite et s'est arrêtée : { $reason }
ui-transcript-header = ===== Session démarrée { $started } | { $protocol } | hôte { $host } | { $title } =====
ui-transcript-footer = ===== Session terminée { $ended } | durée { $duration } =====
ui-settings-session-logging = Journalisation des sessions
ui-settings-ssh-auto-reconnect = Reconnexion auto SSH
ui-settings-ssh-auto-reconnect-description = Tente automatiquement de rétablir une session SSH déconnectée. Désactivé par défaut.
ui-settings-ssh-auto-reconnect-enable = Activer la reconnexion automatique bornée
ui-settings-ssh-auto-reconnect-attempts = Nombre maximum de tentatives avant le retour à la reconnexion manuelle
ui-settings-session-log-directory = Répertoire des journaux de session :
ui-settings-session-log-directory-hint = Dossier des journaux de session, relatif au dossier des paramètres sauf s'il est absolu. Appuyez sur Entrée pour appliquer.

ui-broadcast-button = DIFFUSION
ui-broadcast-toggle-tooltip = Activer/désactiver la diffusion (envoyer à tous les terminaux), Ctrl+Alt+B
ui-broadcast-on = Mode diffusion ACTIF - { $scope }
ui-broadcast-off = Mode diffusion DÉSACTIVÉ
ui-broadcast-scope-all = Tous les onglets
ui-broadcast-scope-selected = Onglets sélectionnés ({ $count })
ui-broadcast-scope-status = Portée de la diffusion : { $scope }
ui-broadcast-scope-tooltip = Portée de la diffusion (cliquer pour basculer entre tous les onglets et les onglets marqués)
ui-broadcast-target-on = ◉
ui-broadcast-target-off = ○
ui-broadcast-target-tooltip = Envoyer la diffusion vers cette session (cible de diffusion)
ui-dialog-broadcast-title = Diffuser vers tous les onglets ?
ui-dialog-broadcast-body = La saisie sera envoyée aux terminaux de tous les onglets ouverts, y compris ceux en arrière-plan. Continuer ?
ui-dialog-broadcast-confirm = Diffuser

ui-files-go-button = Aller

ui-files-column-name = Nom
ui-files-column-size = Taille
ui-files-column-modified = Modifié
ui-files-column-permissions = Permissions
ui-files-column-owner = Propriétaire
ui-files-sorted-ascending = { $column } ▲
ui-files-sorted-descending = { $column } ▼

ui-files-menu-open = Ouvrir
ui-files-menu-download = Télécharger
ui-files-menu-upload = Envoyer
ui-files-menu-rename = Renommer
ui-files-menu-delete = Supprimer
ui-files-menu-copy-path = Copier le chemin
ui-files-menu-new-folder = Nouveau dossier
ui-files-menu-refresh = Actualiser

ui-files-menu-permissions = Modifier les permissions...
ui-files-menu-properties = Propriétés
ui-dialog-permissions-title = Modifier les permissions
ui-dialog-permissions-label = Permissions (octal, ex : 755) :
ui-dialog-permissions-placeholder = 755
ui-dialog-permissions-confirm = Appliquer
ui-files-error-invalid-permissions = Les permissions sont de un à quatre chiffres octaux, comme 755 ou 4755.
ui-files-properties-title = Propriétés - { $name }
ui-files-properties-name = Nom :
ui-files-properties-type = Type :
ui-files-properties-size = Taille :
ui-files-properties-modified = Modifié :
ui-files-properties-permissions = Permissions :
ui-files-properties-owner = Propriétaire :
ui-files-properties-group = Groupe :
ui-files-properties-path = Chemin :
ui-files-type-file = Fichier
ui-files-type-directory = Dossier
ui-files-type-link = Lien symbolique
ui-files-type-other = Type inconnu

ui-files-selected-count = { $count ->
    [one] { $count } sélectionné
   *[other] { $count } sélectionnés
}
ui-dialog-delete-many-body = Supprimer { $count } éléments ? Les dossiers sont supprimés avec tout ce qu'ils contiennent. Cette action est irréversible.

ui-files-bookmark-button = Ajouter aux signets
ui-files-bookmarks-button = Signets
ui-files-bookmarks-empty = Aucun signet enregistré
ui-files-bookmark-added = Signet ajouté : { $path }

ui-files-filter-placeholder = Filtrer les fichiers...
ui-files-hidden-toggle = .*
ui-files-hidden-tooltip = Afficher les fichiers cachés
ui-files-item-count = { $count } éléments
ui-files-item-count-filtered = { $shown }/{ $count } éléments

ui-files-drop-overlay = Déposer les fichiers pour envoyer

ui-trusted-host-keys-title = Clés d'hôtes approuvées
ui-trusted-host-keys-hint = Contrôler les clés d'hôtes SSH que Heimdall approuve pour les prochaines connexions.
ui-trusted-host-keys-search = Rechercher des hôtes approuvés
ui-trusted-host-keys-host = Hôte:Port
ui-trusted-host-keys-algorithm = Algorithme
ui-trusted-host-keys-fingerprint = Empreinte
ui-trusted-host-keys-copy = Copier l'empreinte
ui-trusted-host-keys-remove = Supprimer
ui-trusted-host-keys-empty-title = Aucune clé d'hôte approuvée
ui-trusted-host-keys-empty-body = Connectez-vous d'abord à un serveur : sa clé est demandée, puis listée ici.
ui-trusted-certificates-title = Certificats RDP approuvés
ui-trusted-certificates-hint = Les certificats acceptés pour un bureau à distance, conservés entre les redémarrages. En oublier un le retire de la liste de confiance de son serveur.
ui-trusted-certificates-search = Rechercher par serveur ou empreinte
ui-trusted-certificates-server = Serveur
ui-trusted-certificates-fingerprint = Empreinte
ui-trusted-certificates-forget = Oublier
ui-trusted-certificates-empty-title = Aucun certificat RDP approuvé
ui-trusted-certificates-empty-body = Les certificats que vous acceptez en vous connectant à un bureau distant sont listés ici, et peuvent y être révoqués.
ui-trusted-keys-unreadable = Les clés de confiance n'ont pas toutes pu être lues : { $detail }
ui-dialog-forget-host-key-title = Supprimer la clé d'hôte approuvée
ui-dialog-forget-host-key-body = Supprimer la clé d'hôte de confiance de { $server } ?
ui-dialog-forget-host-key-fingerprint = Empreinte : { $fingerprint }
ui-dialog-forget-host-key-consequence = Supprimer cette clé d'hôte de confiance imposera une nouvelle vérification à la prochaine connexion à { $server }.
ui-dialog-forget-host-key-confirm = Supprimer
ui-dialog-forget-certificate-title = Oublier ce certificat ?
ui-dialog-forget-certificate-body = Heimdall va oublier le certificat { $fingerprint } de { $server }. Seul ce certificat est concerné ; tout autre certificat approuvé pour ce serveur le reste.
ui-dialog-forget-certificate-keep = Conserver
ui-dialog-forget-certificate-confirm = Oublier
ui-status-fingerprint-copied = Empreinte complète copiée pour { $server }.
ui-status-host-key-removed = Clé d'hôte approuvée supprimée pour { $server }.
ui-status-certificate-forgotten = Certificat oublié pour { $server }.

## Tunnels opened by hand, as the C# "New tunnel" dialog and tunnels panel say them.
ui-tunnel-new-title = Nouveau tunnel
ui-tunnel-new-description = Créez une redirection de port local liée à la session via l'une de vos passerelles SSH configurées.
ui-tunnel-gateway-label = Passerelle
ui-tunnel-remote-host-label = Hôte distant
ui-tunnel-remote-port-label = Port distant
ui-tunnel-local-port-label = Port local
ui-tunnel-label-label = Étiquette (optionnel)
ui-tunnel-open-button = Ouvrir le tunnel
ui-tunnel-no-gateways = Aucune passerelle SSH configurée. Ajoutez-en une dans les Paramètres avant de créer un tunnel.
ui-tunnel-problem-gateway = La passerelle est requise.
ui-tunnel-problem-remote-host = L'hôte distant est requis.
ui-tunnel-problem-remote-port = Le port distant doit être compris entre { $min } et { $max }.
ui-tunnel-problem-local-port = Le port local doit être compris entre { $min } et { $max }.
ui-tunnel-problem-local-port-in-use = Le port local { $port } est déjà utilisé par un tunnel actif.
ui-tunnel-opened = Tunnel ouvert sur le port local { $port } → { $host }:{ $remote }.
ui-tunnel-failed = Échec de la création du tunnel : { $reason }
ui-tunnel-closed = Tunnel sur le port { $port } fermé.
ui-tunnels-all-closed = Tous les tunnels fermés.
ui-tunnel-port-copied = Port { $port } copié dans le presse-papiers.
ui-tunnel-closed-reason = { $closed } ({ $reason })
ui-error-local-port-unavailable = Le port local { $port } est déjà utilisé, ou réservé par le système.

## The tunnels panel and its status-bar button, as the C# ones.
ui-tunnels-header = Tunnels ({ $count })
ui-tunnels-close-all = Tout fermer
ui-tunnels-new = + Nouveau
ui-tunnels-collapse-tooltip = Réduire le panneau des tunnels
ui-tunnels-toggle-tooltip = Afficher/masquer le panneau des tunnels
ui-tunnels-column-gateway = Passerelle
ui-tunnels-column-label = Étiquette
ui-tunnels-column-local = Local
ui-tunnels-column-remote = Distant
ui-tunnels-column-port = Port
ui-tunnels-close-tooltip = Fermer le tunnel
ui-tunnels-menu-close = Fermer le tunnel
ui-tunnels-menu-copy-port = Copier le port local
ui-tunnels-menu-close-all = Fermer tous les tunnels
ui-tunnels-empty = Aucun tunnel actif
ui-tunnels-count =
    { $count ->
        [one] { $count } tunnel
       *[other] { $count } tunnels
    }
ui-tunnels-collapse-button = ▼

## An SSH key file not there, and what the agents offered a gateway that refused, as the C# says them.
ui-error-key-not-found = Fichier de clé SSH introuvable : { $path }
ui-error-auth-agent-none = Aucune clé n'était chargée dans un agent SSH au moment de l'appel de cette passerelle, aucune clé d'agent n'a donc été présentée. Si cette passerelle se connecte avec une clé d'agent, chargez-la dans Pageant ou dans l'agent OpenSSH de Windows puis reconnectez-vous ; sinon, vérifiez les identifiants enregistrés pour cette passerelle.
ui-error-auth-agent-one = Une clé était chargée dans un agent SSH et a été présentée à cette passerelle ; elle n'a pas été acceptée. Si cette passerelle attend une autre clé, chargez celle-là puis reconnectez-vous.
ui-error-auth-agent-many = { $count } clés étaient chargées dans un agent SSH et ont été présentées à cette passerelle ; aucune n'a été acceptée. Si cette passerelle attend une autre clé, chargez celle-là puis reconnectez-vous.
ui-error-auth-with-agent = { $refused } { $agent }

## "Test address" in the profile form, as the C# dialog says it.
ui-address-test-button = Tester l'adresse
ui-address-test-hint = Vérifie que l'adresse et le port répondent. Ne vérifie ni votre identifiant ni votre mot de passe.
ui-address-test-running = Test de l'adresse en cours...
ui-address-test-success = L'adresse répond : { $address } ({ $millis } ms). Les identifiants n'ont pas été vérifiés.
ui-address-test-success-ssh = L'adresse répond et un serveur SSH a répondu : { $banner }. Les identifiants n'ont pas été vérifiés.
ui-address-test-failure = L'adresse n'a pas répondu : { $reason }
ui-address-test-direct-scope = Test effectué directement depuis cet ordinateur, pas via { $gateway }.
ui-address-test-cancel = Annuler
ui-address-test-dns-timeout = La résolution DNS a expiré.
ui-address-test-dns-failed = La résolution DNS a échoué : { $reason }
ui-address-test-dns-no-results = La résolution DNS n'a retourné aucune adresse.
ui-address-test-tcp-timeout = Le connect TCP a expiré (hôte : { $address }). L'hôte est peut-être éteint, inaccessible, ou le port est bloqué.
ui-address-test-tcp-failed = Impossible de se connecter à { $address } : { $reason }
ui-address-test-cancelled = Test annulé.
ui-address-test-scoped = { $verdict } { $scope }

## The gateway dialog's "Test route", as the C# card.
ui-route-test-title = Tester et comprendre ce parcours
ui-route-test-workstation = Ce poste
ui-route-test-target-host = Hôte cible facultatif (vide pour tester seulement les passerelles)
ui-route-test-target-port = Port TCP de la cible
ui-route-test-test = Tester le parcours
ui-route-test-stop = Arrêter le test
ui-route-test-copy = Copier le diagnostic
ui-route-test-running = Test du parcours en cours. Les résultats apparaissent après chaque étape.
ui-route-test-hop-step = Passerelle { $number } : connexion et authentification SSH
ui-route-test-target-step = Accès TCP à la cible
ui-route-test-passed = Réussi
ui-route-test-trust-required = Non testé : aucune clé d'hôte de confiance. Vérifiez-la et enregistrez-la d'abord dans les clés SSH de confiance.
ui-route-test-trust-changed = La clé d'hôte diffère. Vérifiez l'identité du serveur avant de modifier les clés SSH de confiance.
ui-route-test-network = Connexion indisponible. Vérifiez l'hôte, le port, le VPN et le pare-feu.
ui-route-test-forwarding = Accès à la cible non confirmé. Vérifiez l'adresse, le port et les autorisations de transfert TCP sur la passerelle.
ui-route-test-cancelled = Annulé.
ui-route-test-interactive = Une authentification interactive est nécessaire. Utilisez une connexion SSH interactive pour examiner la méthode requise.
ui-route-test-auth = Échec d'authentification. Vérifiez le compte, la clé, la phrase secrète et l'agent SSH.
ui-route-test-unavailable = Diagnostic indisponible ou échec non classé. Vérifiez la configuration et les prérequis d'authentification.
ui-route-test-invalid-route = Parcours invalide : vérifiez les parents manquants, les cycles et la profondeur maximale.
ui-route-test-invalid-target = Saisissez un nom d'hôte ou une adresse IP valide et un port TCP entre 1 et 65535.
ui-route-test-report-header = Diagnostic de parcours SSH Heimdall (anonymisé : sans noms d'hôtes, comptes, chemins de clés ni erreurs brutes)
ui-route-test-tcp-only = L'étape cible vérifie uniquement l'accès TCP. Elle ne valide ni le protocole applicatif ni une authentification sur la cible.
ui-route-test-no-target = Aucune cible indiquée. Seules les connexions SSH aux passerelles ont été testées.
ui-route-test-hop = { $name } ({ $host }:{ $port })
ui-route-test-step-line = { $step } : { $outcome } ({ $millis } ms)
ui-route-test-hint = Teste le formulaire actuel sans l'enregistrer, uniquement avec les clés d'hôtes de confiance. Les sessions existantes restent ouvertes.
ui-route-test-timeout = Délai dépassé. Vérifiez le VPN, le routage et le pare-feu.
ui-route-test-separator = {" "}→{" "}

## "Test reachability" in a profile's menu, as the C# tree says it in the status bar.
ui-tree-test-reachability = Tester l'accessibilité
ui-status-reachability-testing = Test de { $host }:{ $port } ...
ui-status-reachability-success = { $host }:{ $port } accessible en { $millis } ms
ui-status-reachability-failed = { $host }:{ $port } inaccessible : { $reason }

## An RDP tab's "Resolution" menu, as the C# one.
ui-resolution-menu = Résolution
ui-resolution-active-mode = Mode actif
ui-resolution-header = { $label } : { $mode }
ui-resolution-header-size = { $label } : { $mode } ({ $width }x{ $height })
ui-resolution-mode-fit-window = Adapter à la fenêtre
ui-resolution-mode-fixed = Fixe
ui-resolution-match-window = Adapter à la fenêtre
ui-resolution-custom = Personnalisé...
ui-resolution-custom-title = Résolution personnalisée
ui-resolution-custom-prompt = Saisissez la résolution au format LARGEURxHAUTEUR.
ui-resolution-custom-invalid = Résolution invalide. Utilisez LARGEURxHAUTEUR.
ui-resolution-save-default = Enregistrer comme défaut pour ce serveur
ui-resolution-save-default-done = Résolution RDP par défaut enregistrée pour ce serveur.
ui-resolution-save-default-unavailable = Impossible d'enregistrer une résolution par défaut pour cette session.

## The SSH agent chip of the profile form, as the C# one.
ui-agent-chip-off = Aucun agent SSH détecté
ui-agent-chip-warn = Agent SSH : { $agent } (aucune clé chargée)
ui-agent-chip-ok = Agent SSH : { $agent } ({ $count } clés)
ui-agent-chip-tooltip = Cliquer pour rescanner les agents SSH
ui-files-menu-cut = Couper
ui-files-menu-paste = Coller
ui-status-files-cut = { $count ->
    [one] { $count } élément coupé
   *[other] { $count } éléments coupés
}
ui-status-files-pasted = Collage terminé
ui-status-path-copied = Chemin copié : { $path }
ui-files-menu-copy = Copier
ui-files-menu-duplicate = Dupliquer
ui-status-files-copied = { $count ->
    [one] { $count } élément copié
   *[other] { $count } éléments copiés
}
ui-status-files-duplicated = Duplication terminée
ui-files-error-copy-refused = Copie refusée : ce serveur n'a pas effectué la copie côté serveur, et Heimdall ne basculera pas vers un transfert susceptible d'écraser une destination existante. Copiez le fichier localement, ou vérifiez que le serveur autorise l'exécution de cp, ln et mkdir.
ui-files-error-paste-into-itself = Impossible de coller { $name } dans lui-même ou dans son propre sous-dossier.
ui-error-key-not-absolute = Le chemin de la clé SSH doit être absolu : { $path }
ui-files-error-changed-on-server = Le fichier a changé sur le serveur depuis son ouverture : il a été laissé tel quel.
ui-files-error-file-too-large = Les fichiers de plus de 16 Mio doivent être téléchargés.
ui-files-menu-open-in-terminal = Ouvrir dans le terminal
ui-status-resolution-reconnected = Le changement de résolution a nécessité une reconnexion.
ui-resolution-mode-smart-sizing = Mise à l'échelle intelligente
ui-resolution-tooltip = Changer la résolution - { $mode }
ui-resolution-tooltip-size = Changer la résolution - { $mode } ({ $width }x{ $height })
ui-resolution-larger-than-window = Plus grand que la fenêtre - l'image sera mise à l'échelle.
ui-files-menu-upload-here = Envoyer ici...
ui-files-menu-edit-external = Éditer avec un éditeur externe
ui-status-files-editing = Édition : { $name } - enregistrez dans votre éditeur pour envoyer automatiquement
ui-status-files-auto-uploaded = Envoyé automatiquement : { $name }
ui-status-files-auto-upload-refused = L'envoi automatique de { $name } a été refusé et ne sera pas retenté avant votre prochain enregistrement : { $reason }
ui-files-error-working-folder-unprotected = Le fichier n'a pas été ouvert : son dossier de travail local n'a pas pu être réservé à votre compte, son contenu aurait donc pu être lisible par les autres utilisateurs de cet ordinateur.
ui-files-error-editor-failed = L'éditeur externe n'a pas pu être lancé : { $detail }. Vérifiez le chemin de l'éditeur dans les paramètres.
ui-files-error-editor-runs-files = Le lancement d'interpréteurs de commandes ou d'hôtes de scripts comme éditeurs est bloqué pour des raisons de sécurité.
ui-settings-external-editor = Éditeur externe
ui-settings-external-editor-path = Chemin de l'éditeur externe
ui-settings-external-editor-hint = Chemin vers l'éditeur de texte pour l'édition SFTP distante (laisser vide pour le programme par défaut)
ui-dialog-close-edits-body = "{ $name }" a un fichier ouvert dans un éditeur externe. Fermer le panneau quand même ? L'éditeur reste ouvert, mais plus rien n'enverra sa prochaine sauvegarde au serveur.
ui-files-edits-title = Édités dans un éditeur externe
ui-files-edit-watching = Chaque enregistrement dans l'éditeur est envoyé au serveur.
ui-files-edit-send-anyway = Envoyer ma version
ui-files-edit-open-folder = Ouvrir le dossier
ui-files-edit-stop = Arrêter l'édition
ui-files-error-sudo-password-needed = sudo demande un mot de passe sur ce serveur.
ui-files-error-sudo-password-rejected = sudo a refusé le mot de passe.
ui-files-error-sudo-needs-terminal = sudo exige un terminal sur ce serveur (requiretty) : Heimdall ne lui en donne pas, donc rien n'a été exécuté en root. Autorisez sudo sans terminal pour votre compte, ou éditez le fichier dans un shell.
ui-files-error-sudo-untrusted = Le sudo trouvé sur le serveur n'est pas celui du système (pas set-user-id root) : rien n'a été exécuté en root.
ui-files-error-sudo-tooling = Transfert privilégié refusé : il manque sur le serveur un outil nécessaire (GNU coreutils : stat, cp, sync, mv). Le journal nomme l'outil.
ui-files-error-sudo-failed = Échec de l'authentification sudo.
ui-files-menu-paste-explorer = Coller depuis l'Explorateur
ui-status-explorer-no-files = Aucun fichier n'est copié dans l'Explorateur.
ui-status-rdp-files-too-many = Fichiers non copiés vers le serveur : une copie prend { $count } fichiers et dossiers au plus.
ui-status-rdp-files-too-large = Fichiers non copiés vers le serveur : une copie prend { $size } au plus.
ui-files-menu-edit-sudo = Éditer avec sudo
ui-files-edit-save-sudo = Enregistrer avec sudo
ui-status-files-saved-sudo = Enregistré via sudo : { $name }
ui-dialog-sudo-title = Mot de passe sudo
ui-dialog-sudo-body = sudo demande votre mot de passe sur ce serveur pour "{ $name }". Il est gardé pour cet onglet seulement, jusqu'à sa fermeture ou un refus de sudo.
ui-desktop-save-files = Enregistrer les fichiers copiés...
ui-desktop-save-files-tooltip = Enregistrer les fichiers copiés sur le serveur dans un dossier de cet ordinateur
ui-desktop-saving-files = Enregistrement : { $saved } sur { $total }
ui-desktop-save-files-cancel = Arrêter
ui-status-rdp-files-saved = Fichiers du serveur enregistrés : { $count }.
ui-status-rdp-files-save-failed = Fichiers du serveur incomplets : { $saved } sur { $total } enregistrés avant l'échec.
ui-status-rdp-files-save-cancelled = Enregistrement arrêté : { $saved } sur { $total } enregistrés.
ui-status-rdp-files-not-saved-too-many = Fichiers du serveur non enregistrés : une copie prend { $count } fichiers et dossiers au plus.
ui-status-rdp-files-not-saved-too-large = Fichiers du serveur non enregistrés : une copie prend { $size } au plus.
ui-status-rdp-files-not-saved-unknown-size = Fichiers du serveur non enregistrés : le serveur n'a pas donné leur taille.
ui-files-menu-edit-integrated = Éditer
ui-editor-save = Enregistrer
ui-editor-close = Fermer
ui-editor-overwrite = Écraser
ui-editor-opening = Ouverture de { $name }...
ui-editor-position = Ln { $line }, Col { $column }
ui-editor-lines = { $count ->
    [one] { $count } ligne
   *[other] { $count } lignes
}
ui-editor-plain-text = Texte brut
ui-editor-encoding-utf8 = UTF-8
ui-editor-encoding-utf8-bom = UTF-8 avec BOM
ui-editor-encoding-utf16le = UTF-16 LE
ui-editor-encoding-utf16be = UTF-16 BE
ui-editor-encoding-utf32le = UTF-32 LE
ui-editor-encoding-utf32be = UTF-32 BE
ui-editor-encoding-latin1 = Latin-1
ui-editor-ending-lf = LF
ui-editor-ending-crlf = CRLF
ui-editor-ending-cr = CR
ui-editor-notice-latin1 = Ce fichier n'est pas en UTF-8 valide et a été ouvert en Latin-1. L'enregistrement le réécrit en Latin-1.
ui-editor-notice-saved = Enregistré.
ui-editor-notice-changed = Le fichier a changé sur le serveur depuis son ouverture : non enregistré. Écrasez-le, ou fermez sans enregistrer.
ui-editor-notice-unencodable = Non enregistré : le caractère ligne { $line }, colonne { $column } ne peut pas être stocké en Latin-1.
ui-editor-notice-save-running = L'enregistrement est encore en cours.
ui-editor-notice-session-ended = La session est terminée : vos modifications sont conservées. Reconnectez-vous pour les enregistrer.
ui-editor-notice-failed = Échec de l'enregistrement : { $reason }
ui-dialog-discard-editor-title = Modifications non enregistrées
ui-dialog-discard-editor-body = Le fichier a des modifications non enregistrées. Fermer quand même ?
ui-dialog-close-editor-body = L'éditeur sur "{ $name }" a des modifications non enregistrées. Fermer et les abandonner ?
ui-dialog-unsaved-editors = { $count ->
    [one] { $count } éditeur a des modifications non enregistrées, qui seraient perdues.
   *[other] { $count } éditeurs ont des modifications non enregistrées, qui seraient perdues.
}
ui-files-error-looks-binary = Ce fichier semble binaire (une archive, une image ou un programme) : téléchargez-le plutôt.
ui-files-error-not-text = La marque d'encodage de ce fichier ne correspond pas à son contenu : il ne peut pas être ouvert comme texte.
ui-files-error-too-large-for-editor = Trop volumineux pour l'éditeur intégré (plus de { $size }) : utilisez l'édition avec l'éditeur externe.
ui-dialog-binary-title = Fichier binaire
ui-dialog-binary-body = "{ $name }" semble être un fichier binaire (une archive, une image ou un programme) et ne peut pas être affiché comme texte. Le télécharger plutôt ?
ui-dialog-binary-confirm = Télécharger
ui-tab-menu-show-health = Afficher la santé du serveur
ui-tab-menu-hide-health = Masquer la santé du serveur
ui-health-cpu = CPU
ui-health-memory = Mémoire
ui-health-disk = Disque
ui-health-waiting = Lecture...
ui-health-unsupported = Non pris en charge
ui-health-cpu-value = { $percent } %
ui-health-memory-value = { $used } / { $total } Mo
ui-health-disk-value = { $used } / { $total }
ui-find-count = { $index } / { $total }
ui-winrm-diagnostic-logon-failed = L'hôte distant a refusé les identifiants. Vérifiez le nom d'utilisateur et le mot de passe (forme utilisateur@domaine pour un compte de domaine) et que le compte n'est pas verrouillé.
ui-winrm-diagnostic-access-denied = Accès refusé par l'hôte distant. Le compte doit avoir le droit d'utiliser WinRM : demandez à votre administrateur de l'ajouter au groupe Utilisateurs de gestion à distance de l'hôte.
ui-winrm-diagnostic-trusted-hosts = WinRM a refusé la connexion car l'hôte n'est pas approuvé pour cette authentification. Utilisez le nom DNS de l'hôte plutôt que son adresse IP, utilisez HTTPS, ou demandez à votre administrateur d'ajouter l'hôte à la liste TrustedHosts de cet ordinateur.
ui-winrm-diagnostic-kerberos-principal = L'authentification Kerberos a échoué pour cet hôte. Connectez-vous avec le nom DNS complet de l'hôte plutôt que son adresse IP ou un alias court, et vérifiez que l'hôte appartient à votre domaine.
ui-winrm-diagnostic-session-not-entered = La session WinRM distante n'a pas pu être ouverte. Lisez le message PowerShell ci-dessus pour la cause, puis vérifiez le nom d'hôte, le port et l'identité de ce profil.
ui-profile-winrm-identity-hint = Kerberos ou NTLM est négocié automatiquement. Kerberos demande le nom DNS de l'hôte, pas son adresse IP. En HTTP le contenu reste chiffré par Kerberos ou NTLM, mais NTLM ne vérifie pas l'identité du serveur.
ui-profile-winrm-trusted-hosts-hint = Hors domaine, NTLM en HTTP demande que l'hôte figure dans la liste TrustedHosts de cet ordinateur, ou d'utiliser HTTPS. Heimdall ne modifie jamais TrustedHosts.
ui-profile-winrm-https-off-by-gateway = HTTPS a été désactivé car une passerelle SSH est choisie : WinRM par passerelle utilise HTTP dans le tunnel. Retirez la passerelle pour rétablir HTTPS.
ui-error-winrm-tls-no-verify = La connexion TLS WinRM vers '{ $host }' sur le port { $port } a échoué alors que la vérification du certificat est ignorée : le port ne répond probablement pas en TLS. Vérifiez que le port { $port } est l'écouteur WinRM HTTPS (en général 5986).
ui-files-state-rate = { $progress } - { $rate }/s, { $left } restantes
ui-files-eta-seconds = { $seconds } s
ui-files-eta-minutes = { $minutes } min { $seconds } s
ui-files-eta-hours = { $hours } h { $minutes } min
ui-files-conflict-replace-if-newer = Remplacer si plus récent
ui-files-conflict-incoming = Nouveau : { $size }, modifié le { $modified }
ui-files-conflict-existing = Existant : { $size }, modifié le { $modified }
ui-files-conflict-newer = Le nouveau fichier est plus récent.
ui-files-conflict-older = Le nouveau fichier est plus ancien.
ui-files-conflict-same-time = Même date de modification.
ui-files-conflict-unknown = inconnu
ui-settings-ssh-agent-preference = Préférence d'agent SSH
ui-settings-ssh-agent-openssh-first = Auto : Windows OpenSSH d'abord
ui-settings-ssh-agent-pageant-first = Auto : Pageant d'abord
ui-settings-ssh-agent-openssh-only = Windows OpenSSH uniquement
ui-settings-ssh-agent-pageant-only = Pageant uniquement
ui-settings-ssh-agent-preference-hint = Contrôle quelles clés d'agent SSH Heimdall essaie en premier. Les changements s'appliquent à la prochaine connexion.
ui-status-screenshot-copied = Capture d'écran copiée dans le presse-papiers
ui-status-screenshot-failed = Échec de la capture d'écran
ui-files-state-queued = En attente
ui-files-state-preparing = Préparation du transfert...
ui-files-retry-button = Réessayer
ui-files-clear-finished-button = Effacer les terminés
ui-files-error-interrupted = Le transfert s'est interrompu de façon inattendue.
ui-rdp-session-closed = La session Bureau à distance s'est terminée.
ui-trusted-host-keys-export = Exporter known_hosts
ui-status-known-hosts-exported = { $count ->
    [one] { $count } clé exportée vers { $path }.
   *[other] { $count } clés exportées vers { $path }.
}
ui-status-known-hosts-export-skipped = { $count ->
    [one] { $count } entrée ignorée (aucune clé publique capturée - reconnectez-vous pour permettre l'export).
   *[other] { $count } entrées ignorées (aucune clé publique capturée - reconnectez-vous pour permettre l'export).
}
ui-status-known-hosts-export-failed = Échec de l'export known_hosts : { $detail }
ui-status-known-hosts-export-no-home = Échec de l'export known_hosts : le dossier personnel est inconnu.
ui-tree-favorite-add = Ajouter aux favoris
ui-tree-favorite-remove = Retirer des favoris
ui-tree-favorite = Favori
ui-tree-filter-favorites = Favoris
ui-profile-toggle-favorite = Marquer comme favori
ui-status-favorite-save-failed = Impossible d'enregistrer le favori.
ui-selection-edit = Modifier
ui-selection-edit-port = Port...
ui-selection-edit-username = Nom d'utilisateur... ({ $count })
ui-bulk-port-header = { $count ->
    [one] Modification du port sur { $count } élément
   *[other] Modification du port sur { $count } éléments
}
ui-bulk-port-label = Port
ui-bulk-port-mixed = Valeurs mixtes
ui-bulk-port-invalid = Le port doit être compris entre 1 et 65535.
ui-bulk-username-header = { $count ->
    [one] Modification du nom d'utilisateur sur { $count } serveur
   *[other] Modification du nom d'utilisateur sur { $count } serveurs
}
ui-bulk-username-label = Nom d'utilisateur :
ui-bulk-username-mixed = valeurs multiples
ui-bulk-username-invalid = Le nom d'utilisateur ne peut pas être vide ni contenir de caractères de contrôle (y compris les sauts de ligne et les tabulations).
ui-status-bulk-port-updated = { $count ->
    [one] Port mis à jour sur { $count } élément.
   *[other] Port mis à jour sur { $count } éléments.
}
ui-status-bulk-port-unchanged = Aucun changement de port n'a été appliqué.
ui-status-bulk-username-updated = { $count ->
    [one] Nom d'utilisateur modifié sur { $count } serveur.
   *[other] Nom d'utilisateur modifié sur { $count } serveurs.
}
ui-status-bulk-username-unchanged = Aucune modification - tous les serveurs sélectionnés utilisent déjà ce nom d'utilisateur.
ui-desktop-disconnect = Déconnecter
ui-desktop-disconnect-tooltip = Déconnecter la session
ui-desktop-disconnect-title = Déconnecter Bureau à distance ?
ui-desktop-disconnect-body = Vous allez vous déconnecter de { $name }. Continuer ?
ui-settings-rdp-auto-reconnect-attempts = Nombre maximal de tentatives de reconnexion automatique
ui-files-type-pipe = Tube nommé (FIFO)
ui-files-type-socket = Socket
ui-files-type-device = Périphérique
ui-files-bookmark-removed = Signet supprimé : { $path }
ui-files-bookmark-remove-menu = Supprimer un signet
ui-files-empty-no-match = Aucune entrée ne correspond à "{ $filter }".
ui-files-empty-clear-filter = Effacer le filtre
ui-files-empty-hidden-only = Ce dossier ne contient que des entrées masquées.
ui-files-empty-show-hidden = Afficher les fichiers masqués
ui-tunnels-status-active = Actif
ui-tunnels-status-interrupted = Interrompu
ui-tunnels-menu-reopen = Rouvrir
ui-settings-behavior = Comportement
ui-settings-collapse-tunnels-panel = Replier le panneau Tunnels par défaut
ui-settings-collapse-tunnels-panel-hint = État du panneau Tunnels au démarrage. L'ouvrir ou le fermer ensuite le laisse ainsi jusqu'à la fermeture de l'application.
ui-profile-session-logging = Journalisation des sessions
ui-profile-session-logging-inherit = Hériter
ui-profile-session-logging-on = Activé
ui-profile-session-logging-off = Désactivé
ui-profile-session-logging-hint = Hériter suit le paramètre global de journalisation des sessions.
ui-hostkey-copy-fingerprint-button = Copier
ui-certificate-already-trusted = { $count ->
    [one] Ce profil approuve déjà { $count } autre certificat pour ce nom, ce qui signifie le plus souvent que plusieurs machines y répondent.
   *[other] Ce profil approuve déjà { $count } autres certificats pour ce nom, ce qui signifie le plus souvent que plusieurs machines y répondent.
}
ui-certificate-route = Atteint via : { $route }
ui-settings-rdp-resolution-presets = Préréglages de résolution
ui-settings-rdp-resolution-presets-hint = Un préréglage par ligne, format LARGEURxHAUTEUR (ex. 1920x1080). Laissez la zone vide pour utiliser la liste intégrée.
ui-settings-rdp-resolution-presets-reset = Réinitialiser aux valeurs par défaut
ui-settings-rdp-resolution-presets-invalid = Préréglages de résolution : ces lignes ne sont pas au format LARGEURxHAUTEUR avec une largeur de { $min } à { $width } et une hauteur de { $min } à { $height } pixels : { $lines }
ui-settings-rdp-reset-defaults = Réinitialiser les valeurs RDP
ui-settings-rdp-reset-defaults-tooltip = Restaure uniquement les valeurs RDP par défaut. Les autres réglages ne sont pas modifiés.
ui-dialog-reset-rdp-title = Réinitialiser les valeurs RDP ?
ui-dialog-reset-rdp-body = Restaurer toutes les valeurs RDP par défaut ? Les serveurs existants ne sont pas affectés.
ui-settings-tab-about = À propos
ui-about-version = Version { $version }
ui-about-tagline = Gestionnaire de connexions sécurisé RDP/SSH/SFTP
ui-about-section-system = Système
ui-about-platform = Plateforme
ui-about-author = Auteur
ui-about-license = Licence
ui-about-section-data = Données
ui-about-sessions = Sessions
ui-about-gateways = Passerelles
ui-about-config-path = Config
ui-about-log-path = Logs
ui-about-section-links = Accès rapide
ui-about-open-config = Ouvrir le dossier config
ui-about-open-logs = Ouvrir le dossier logs
ui-about-repository = GitHub
ui-about-section-diagnostics = Diagnostic
ui-about-diagnostics-log = Écrire le journal de diagnostic de l'application (événements et erreurs de Heimdall)
ui-about-diagnostics-log-hint = Appliqué immédiatement. Un rapport de plantage est écrit quoi qu'il en soit : c'est la seule trace d'un plantage.
ui-settings-provider-timeout = Délai d'expiration de la commande
ui-settings-provider-timeout-seconds = { $seconds } s
ui-settings-provider-timeout-hint = Durée pendant laquelle la commande de mot de passe peut s'exécuter avant que Heimdall y renonce. Augmentez-la pour un coffre qui demande une confirmation.
ui-folder-color = Couleur
ui-folder-color-none = Aucune couleur
ui-folder-color-blue = Bleu
ui-folder-color-green = Vert
ui-folder-color-red = Rouge
ui-folder-color-amber = Ambre
ui-folder-color-purple = Violet
ui-folder-color-pink = Rose
ui-folder-color-cyan = Cyan
ui-folder-color-orange = Orange
ui-shortcuts-title = Raccourcis clavier
ui-shortcuts-hint = F1 pour les raccourcis
ui-shortcuts-session-keys = Dans un terminal ou un bureau à distance, F1 et les touches que la session utilise vont au serveur : ouvrez cette liste depuis la barre d'état.
ui-shortcuts-group-sessions = Sessions
ui-shortcuts-group-tabs = Onglets
ui-shortcuts-group-terminal = Terminal
ui-shortcuts-group-files = Fichiers
ui-shortcuts-group-window = Fenêtre
ui-shortcuts-new-session = Ajouter une session
ui-shortcuts-edit-session = Modifier la session sélectionnée
ui-shortcuts-quick-connect = Connexion rapide
ui-shortcuts-search = Rechercher des sessions, ou filtrer la liste des fichiers
ui-shortcuts-next-tab = Onglet suivant
ui-shortcuts-previous-tab = Onglet précédent
ui-shortcuts-close-tab = Fermer l'onglet courant
ui-shortcuts-find = Rechercher dans le terminal
ui-shortcuts-text-size = Texte plus grand, plus petit, taille d'origine
ui-shortcuts-broadcast = Activer ou désactiver la diffusion de saisie
ui-shortcuts-terminal-copy = Copier la sélection
ui-shortcuts-terminal-paste = Coller
ui-shortcuts-scroll-history = Faire défiler l'historique d'une page
ui-shortcuts-files-copy-cut = Copier, couper les entrées sélectionnées
ui-shortcuts-files-paste = Coller des entrées, ou des fichiers copiés dans l'Explorateur
ui-shortcuts-files-select-all = Tout sélectionner
ui-shortcuts-files-copy-path = Copier le chemin complet
ui-shortcuts-files-download-upload = Télécharger, envoyer la sélection
ui-shortcuts-files-rename = Renommer
ui-shortcuts-files-new-folder = Nouveau dossier
ui-shortcuts-files-delete = Supprimer
ui-shortcuts-files-refresh = Actualiser
ui-shortcuts-files-back = Retour, dossier parent
ui-shortcuts-files-path = Saisir un chemin
ui-shortcuts-files-switch-pane = Autre volet
ui-shortcuts-full-screen = Basculer en plein écran
ui-shortcuts-settings = Ouvrir les paramètres
ui-shortcuts-screenshot = Capturer l'écran
ui-shortcuts-lock = Verrouiller
ui-shortcuts-help = Afficher cette aide
ui-shortcuts-close = Fermer une boîte de dialogue ou un menu ; quitter le plein écran hors d'un terminal ou d'un bureau distant
ui-resolution-match-aspect = Adapter à la fenêtre, { $wide }:{ $high }
ui-tab-menu-pin = Épingler l'onglet
ui-tab-menu-unpin = Désépingler l'onglet
ui-tab-menu-save-as-profile = Enregistrer comme profil...
ui-tab-menu-reveal-in-tree = Afficher dans l'arborescence
ui-tab-pinned-badge = épinglé
ui-selection-set-gateway = Définir la passerelle... ({ $count })
ui-selection-gateway-direct = Connexion directe (sans passerelle)
ui-status-bulk-gateway-updated = { $count ->
    [one] Route de connexion modifiée sur { $count } serveur.
   *[other] Route de connexion modifiée sur { $count } serveurs.
}
ui-status-bulk-gateway-unchanged = Aucune route n'a été modifiée.
ui-shortcuts-select-all-sessions = Sélectionner toutes les sessions affichées
ui-shortcuts-session-menu = Menu de la session sélectionnée
ui-shortcuts-find-by-name = Aller à la session dont le nom commence par ce qui est tapé
ui-shortcuts-toggle-sidebar = Afficher ou masquer le panneau latéral
ui-restore-title = Restaurer les sessions précédentes
ui-restore-message = Heimdall a trouvé un instantané de session sauvegardé lors de l'exécution précédente. Sélectionnez les sessions à restaurer.
ui-restore-saved-at = Sauvegardé le { $time }
ui-restore-select-all = Tout sélectionner
ui-restore-missing = Serveur introuvable ({ $id })
ui-restore-files = Fichiers { $protocol }
ui-restore-dont = Ne pas restaurer
ui-restore-selected = Restaurer la sélection
ui-profile-field-environment = Environnement
ui-profile-environment-none = (Aucun)
ui-profile-environment-production = Production
ui-profile-environment-staging = Staging
ui-profile-environment-lab = Lab
ui-profile-environment-personal = Personnel
ui-profile-field-tags = Tags
ui-profile-tags-placeholder = Mots par lesquels la recherche trouve cette session
ui-profile-field-mac-address = Adresse MAC
ui-profile-mac-address-placeholder = AA:BB:CC:DD:EE:FF, pour Wake on LAN
ui-profile-error-mac-address = L'adresse MAC doit comporter douze chiffres hexadécimaux, comme AA:BB:CC:DD:EE:FF.
ui-tree-wake-on-lan = Réveil réseau (WOL)
ui-status-wake-on-lan-sent = Paquet magique Wake-on-LAN envoyé.
ui-status-wake-on-lan-failed = Échec de l'envoi du paquet Wake-on-LAN : { $reason }
ui-tree-tooltip-environment = Environnement : { $environment }
ui-tree-tooltip-tags = Tags : { $tags }
ui-files-batch-deleting = Suppression de { $name } ({ $index }/{ $total })...
ui-files-batch-permissions = Modification des permissions de { $name } ({ $index }/{ $total })...
ui-files-batch-stop = Annuler
ui-files-batch-stopping = Arrêt après celui-ci...
ui-status-files-delete-failed = Impossible de supprimer "{ $name }" : { $reason }
ui-status-files-permissions-failed = Impossible de modifier les permissions de "{ $name }" : { $reason }
ui-status-files-delete-partial = { $failed ->
    [one] { $failed } élément sur { $total } n'a pas pu être supprimé. "{ $name }" : { $reason }
   *[other] { $failed } éléments sur { $total } n'ont pas pu être supprimés. Le premier, "{ $name }" : { $reason }
}
ui-status-files-permissions-partial = { $failed ->
    [one] Permissions non modifiées pour { $failed } élément sur { $total }. "{ $name }" : { $reason }
   *[other] Permissions non modifiées pour { $failed } éléments sur { $total }. Le premier, "{ $name }" : { $reason }
}
ui-status-files-delete-stopped = Suppression annulée : { $done } éléments sur { $total } supprimés.
ui-status-files-permissions-stopped = Modification des permissions annulée : { $done } éléments sur { $total } modifiés.
ui-macros-menu = Macros
ui-macros-record = Enregistrer une macro
ui-macros-stop-recording = { $count ->
    [one] Arrêter l'enregistrement ({ $count } saisie)
   *[other] Arrêter l'enregistrement ({ $count } saisies)
}
ui-macros-stop = Arrêter "{ $name }"
ui-macros-play = Lire "{ $name }"
ui-macros-none = Aucune macro enregistrée
ui-macros-empty = Aucune macro pour l'instant. Enregistrez-en une depuis le menu d'un onglet de terminal : Macros, Enregistrer une macro.
ui-macros-inputs = { $count ->
    [one] { $count } saisie
   *[other] { $count } saisies
}
ui-macros-delete = Supprimer
ui-tab-recording-badge = REC
ui-tab-macro-badge = macro
ui-dialog-save-macro-title = Enregistrer la macro
ui-dialog-save-macro-prompt = { $count ->
    [one] { $count } saisie enregistrée. Nommez la macro :
   *[other] { $count } saisies enregistrées. Nommez la macro :
}
ui-dialog-save-macro-confirm = Enregistrer
ui-status-macro-nothing = Rien n'a été saisi : aucune macro enregistrée.
ui-status-macro-saved = Macro "{ $name }" enregistrée.
ui-status-macro-deleted = Macro "{ $name }" supprimée.
ui-status-macro-completed = Macro "{ $name }" lue.
ui-status-macro-stopped = Macro "{ $name }" arrêtée.
ui-status-macro-timed-out = Macro "{ $name }" arrêtée : ce qu'attend la saisie { $entry } n'est pas venu à temps.
ui-status-macro-closed = Macro "{ $name }" arrêtée : la session s'est terminée.
ui-macros-edit = Modifier
ui-dialog-save-macro-warning = Tout ce qui a été saisi est enregistré, mots de passe compris : la macro le garde tel quel.
ui-dialog-delete-macro-body = Supprimer la macro "{ $name }" ? Cette action est irréversible.
ui-macro-editor-title = Modifier la macro
ui-macro-editor-name = Nom
ui-macro-editor-input-hint = Les caractères de contrôle s'écrivent \r (Entrée), \n, \t, \xNN, et une barre oblique inverse \\.
ui-macro-editor-input = Entrée
ui-macro-editor-delay = Délai (ms)
ui-macro-editor-move-up = Monter
ui-macro-editor-move-down = Descendre
ui-macro-editor-delete-entry = Supprimer l'entrée
ui-macro-editor-expects = Attendre un texte d'abord
ui-macro-editor-pattern = Motif attendu
ui-macro-editor-regex = Regex
ui-macro-editor-timeout = Timeout (ms)
ui-macro-editor-timeout-abort = Arrêter
ui-macro-editor-timeout-continue = Continuer
ui-macro-editor-add-expect = Ajouter une étape expect
ui-macro-editor-add-send = Ajouter une étape d'envoi
ui-macro-editor-delete-macro = Supprimer la macro
ui-macro-editor-name-required = Le nom de la macro est requis.
ui-macro-editor-entry-invalid = Entrée { $entry } : { $reason }
ui-macro-editor-input-trailing = la saisie se termine par une barre oblique inverse seule.
ui-macro-editor-input-hex = \x demande deux chiffres hexadécimaux.
ui-macro-editor-input-escape = \{ $escape } n'est pas un échappement connu de l'éditeur.
ui-macro-editor-delay-invalid = le délai n'est pas un nombre de millisecondes.
ui-macro-editor-timeout-range = Plage : { $min }-{ $max } ms
ui-macro-editor-regex-invalid = Regex invalide : { $reason }
ui-about-section-settings-file = Fichier de paramètres
ui-about-export-settings = Exporter les paramètres...
ui-about-import-settings = Importer des paramètres...
ui-about-settings-file-hint = Emporte vos préférences vers un autre ordinateur. Le fichier ne contient aucun secret : ni mot de passe maître, ni PIN, ni mot de passe enregistré.
ui-settings-file-filter = Paramètres Heimdall
ui-dialog-settings-export-title = Exporter les paramètres
ui-dialog-settings-export-paths = { $count ->
    [one] { $count } paramètre désigne un dossier de votre profil utilisateur sur cet ordinateur (chemins d'outils, dossier des journaux, fichiers). L'inclure dans le fichier ?
   *[other] { $count } paramètres désignent des dossiers de votre profil utilisateur sur cet ordinateur (chemins d'outils, dossier des journaux, fichiers). Les inclure dans le fichier ?
}
ui-dialog-settings-export-without = Laisser de côté
ui-dialog-settings-export-with = Inclure
ui-dialog-settings-import-title = Importer des paramètres
ui-dialog-settings-import-body = { $count ->
    [one] { $count } paramètre va changer :
   *[other] { $count } paramètres vont changer :
}
ui-dialog-settings-import-line = { $key } : { $before } -> { $after }
ui-dialog-settings-import-confirm = Importer
ui-settings-value-on = Activé
ui-settings-value-off = Désactivé
ui-settings-value-empty = (vide)
ui-settings-value-items = { $count ->
    [one] { $count } élément
   *[other] { $count } éléments
}
ui-status-settings-exported = Paramètres exportés. Le fichier ne contient aucun secret : ni mot de passe maître, ni PIN, ni mot de passe enregistré.
ui-status-settings-export-failed = Les paramètres n'ont pas pu être exportés : { $reason }
ui-status-settings-imported = { $count ->
    [one] { $count } paramètre importé.
   *[other] { $count } paramètres importés.
}
ui-status-settings-import-nothing = Le fichier contient les paramètres que vous avez déjà. Rien à modifier.
ui-status-settings-import-invalid = Ce fichier n'est pas un fichier de paramètres Heimdall, ou il provient d'une version que celle-ci ne sait pas lire. Rien n'a été modifié.
ui-status-settings-import-failed = Le fichier de paramètres n'a pas pu être lu : { $reason }
ui-settings-tab-gateways = Passerelles
ui-gateways-title = Passerelles SSH
ui-gateways-description = Vérifie quelles sessions utilisent chaque passerelle SSH et repère les références introuvables.
ui-gateways-summary = { $gateways ->
    [one] { $gateways } passerelle
   *[other] { $gateways } passerelles
}, { $routed ->
    [one] { $routed } session routée
   *[other] { $routed } sessions routées
}, { $unresolved ->
    [one] { $unresolved } référence introuvable
   *[other] { $unresolved } références introuvables
}
ui-gateways-configured = Passerelles configurées
ui-gateways-empty = Aucune passerelle SSH n'est configurée.
ui-gateways-parent = Parent : { $name }
ui-gateways-sessions = { $count ->
    [one] { $count } session
   *[other] { $count } sessions
}
ui-gateways-no-sessions = Aucune session n'utilise cette passerelle.
ui-gateways-edit = Modifier
ui-gateways-delete = Supprimer
ui-gateways-unresolved = Références non résolues
ui-gateways-missing-description = Ces sessions ou passerelles enfants référencent un id de passerelle qui n'est pas configuré.
ui-gateways-missing-header = Id de passerelle manquant : { $id }
ui-gateways-child = Passerelle enfant : { $name }
ui-gateways-reassign-to = Passerelle
ui-gateways-reassign = Réassigner
ui-gateways-clear = Vider
ui-dialog-delete-gateway-title = Supprimer la passerelle
ui-dialog-delete-gateway-body = Supprimer la passerelle "{ $name }" ?

    Références à effacer :
    - Serveurs : { $servers }
    - Passerelles enfants : { $gateways }
ui-status-gateway-deleted = Passerelle "{ $name }" supprimée.
ui-status-gateways-reassigned = { $count ->
    [one] { $count } session réassignée.
   *[other] { $count } sessions réassignées.
}
ui-status-gateways-cleared = { $count ->
    [one] Référence de passerelle vidée pour { $count } session.
   *[other] Référence de passerelle vidée pour { $count } sessions.
}
ui-status-gateways-unchanged = Aucune session ne nécessitait de changement.
ui-settings-reachability = Moniteur de santé des sessions
ui-settings-reachability-enabled = Activer les sondes de joignabilité en arrière-plan
ui-settings-reachability-hint = Chaque serveur est appelé depuis cet ordinateur, quelques-uns à la fois, et sa pastille dans la liste montre s'il a répondu. Les serveurs derrière une gateway ne sont pas appelés. Rien n'est envoyé hormis la connexion.
ui-settings-reachability-interval = Intervalle de vérification
ui-settings-reachability-timeout = Délai d'expiration des sondes
ui-settings-reachability-probes = Sondes simultanées max
ui-settings-milliseconds-unit = ms
ui-settings-reachability-interval-refused = L'intervalle de vérification doit être compris entre { $min } et { $max } secondes.
ui-settings-reachability-timeout-refused = Le délai d'expiration des sondes doit être compris entre { $min } et { $max } ms.
ui-settings-reachability-probes-refused = Le nombre de sondes simultanées doit être compris entre { $min } et { $max }.
ui-tree-reachability-checking = Vérification...
ui-tree-reachability-up = Joignable ({ $millis } ms)
ui-tree-reachability-down = Injoignable : { $reason }
ui-tree-reachability-unchecked = Inconnu : { $reason }
ui-reachability-reason-timeout = Délai de connexion dépassé
ui-reachability-reason-refused = Connexion refusée
ui-reachability-reason-unreachable = Hôte injoignable
ui-reachability-reason-dns = Résolution DNS échouée
ui-reachability-reason-behind-gateway = Derrière une gateway SSH - non sondé
ui-reachability-reason-no-port = Pas de port à sonder pour ce protocole
ui-reachability-reason-no-host = Aucun hôte configuré
ui-status-dropped-profiles = { $count ->
    [one] { $count } session déplacée dans { $folder }. Ctrl+Z l'annule.
   *[other] { $count } sessions déplacées dans { $folder }. Ctrl+Z l'annule.
}
ui-status-dropped-profiles-none = { $count ->
    [one] { $count } session sortie de son dossier. Ctrl+Z l'annule.
   *[other] { $count } sessions sorties de leurs dossiers. Ctrl+Z l'annule.
}
ui-status-dropped-folder = Dossier { $name } déplacé. Ctrl+Z l'annule.
ui-status-drop-refused = Un dossier de ce nom s'y trouve déjà : rien n'a été déplacé.
ui-status-move-undone = Déplacement annulé.
ui-status-nothing-to-undo = Rien à annuler.
ui-shortcuts-undo-move = Annuler le dernier déplacement fait en glissant dans l'arborescence
ui-tab-menu-vnc-remote-resize = Redimensionner le bureau distant à l'onglet
ui-detail-folder = Dossier :
ui-detail-environment = Environnement :
ui-detail-username = Utilisateur :
ui-detail-gateway = Passerelle :
ui-detail-credentials = Identifiants enregistrés :
ui-detail-tags = Tags :
ui-detail-favorite = Favori :
ui-detail-yes = Oui
ui-detail-connect = Se connecter
ui-detail-saved-password = mot de passe
ui-detail-saved-key = fichier de clé { $name }
ui-detail-saved-passphrase = phrase secrète de la clé
ui-detail-hints = Entrée ou double-clic connecte, Ctrl+E modifie, Suppr supprime, F1 liste tous les raccourcis.
ui-detail-edit = Modifier
ui-tree-notes = Notes
ui-notes-new = Nouveau
ui-notes-daily = Journal
ui-notes-incident = Incident
ui-notes-procedure = Procédure
ui-notes-tpl-working-note = Note de travail
ui-notes-tpl-notes = Notes
ui-notes-tpl-commands = Commandes
ui-notes-tpl-next = Suite
ui-notes-tpl-daily-note = Journal du jour
ui-notes-tpl-focus = Objectifs
ui-notes-tpl-journal = Journal
ui-notes-tpl-follow-up = Suivi
ui-notes-tpl-incident = Incident
ui-notes-tpl-incident-report = Rapport d'incident
ui-notes-tpl-summary = Résumé
ui-notes-tpl-impact = Impact
ui-notes-tpl-timeline = Chronologie
ui-notes-tpl-incident-started = Début de l'incident
ui-notes-tpl-investigation = Investigation
ui-notes-tpl-actions = Actions
ui-notes-tpl-resolution = Résolution
ui-notes-tpl-procedure = Procédure
ui-notes-tpl-purpose = Objectif
ui-notes-tpl-scope = Périmètre
ui-notes-tpl-preconditions = Préconditions
ui-notes-tpl-steps = Étapes
ui-notes-tpl-validation = Validation
ui-notes-tpl-rollback = Retour arrière
ui-notes-tpl-references = Références
ui-about-open-notes = Ouvrir le dossier des notes
ui-status-note-opened = Note { $name } ouverte dans l'éditeur.
ui-status-note-failed = La note n'a pas pu être ouverte : { $reason }
ui-nav-sessions = Sessions
ui-nav-tunnels = Tunnels
ui-nav-settings = Paramètres
ui-nav-about = À propos
ui-tunnels-page-title = Tunnels actifs
ui-import-dropped-rd-gateway = par une passerelle Bureau à distance
ui-error-rd-gateway = Ce serveur se joint par la passerelle Bureau à distance { $gateway }, que le client intégré ne sait pas encore traverser.
ui-profile-resolution-auto = Auto (recommandé)
ui-profile-resolution-auto-desc = Heimdall choisit le meilleur mode selon le moniteur hôte et le mode plein écran ou fenêtré de la session.
ui-profile-resolution-multi-monitor = Multi-écran
ui-resolution-mode-auto = Auto
ui-resolution-mode-multi-monitor = Multi-écran
ui-profile-aspect-ratio = Rapport d'aspect
ui-profile-aspect-stretch = Étirer (remplir)
ui-profile-aspect-ratio-choice = { $wide }:{ $high }
ui-profile-rdp-extras = Gardé du profil importé, pas encore utilisé par le client intégré : { $extras }
ui-rdp-extra-strict-server-authentication = authentification stricte du serveur
ui-rdp-extra-full-screen = ouvert en plein écran
ui-dialog-import-dropped = Importé avec des réglages que le client intégré n'utilise pas encore :
ui-dialog-import-dropped-item = { $name } : { $settings }
ui-dialog-import-dropped-separator = {", "}
ui-import-dropped-external-client = ouvert dans un programme externe
ui-import-dropped-x11 = transfert X11
ui-import-dropped-rdp-printers = imprimantes
ui-import-dropped-rdp-com-ports = ports série
ui-import-dropped-rdp-smart-cards = cartes à puce
ui-import-dropped-rdp-webcam = webcam
ui-import-dropped-rdp-usb = périphériques USB
ui-import-dropped-rdp-microphone = microphone
ui-import-dropped-rdp-multi-monitor = plusieurs écrans
ui-fullscreen-exit = Quitter le plein écran
ui-fullscreen-exit-tooltip = F11, ou Échap hors d'un terminal ou d'un bureau distant
ui-files-selected-with-size = { $selection } ({ $size })
ui-files-special-mark = { $name } ({ $kind })
ui-files-tooltip-type = Type : { $kind }
ui-files-tooltip-size = Taille : { $size }
ui-files-tooltip-modified = Modifié : { $at }
ui-files-tooltip-permissions = Permissions : { $permissions }
ui-files-tooltip-time = { $day } { $time } UTC
ui-tree-expand-all = Tout développer
ui-tree-collapse-all = Tout réduire
ui-sidebar-hide-tooltip = Masquer le panneau (Ctrl+B)
ui-sidebar-show-tooltip = Afficher le panneau (Ctrl+B)
ui-nav-quick-connect = Connexion rapide
ui-nav-quick-connect-tooltip = Connexion rapide (Ctrl+K)

## Translations of text written in English first.
ui-desktop-anti-idle = Anti-inactivité
ui-desktop-anti-idle-tooltip = L'anti-inactivité garde cette session ouverte. Cliquez pour le désactiver pour cette session.
ui-desktop-keys-f11 = F11
ui-dialog-close-transfers-body = Un transfert de fichiers est en cours sur "{ $name }". Fermer maintenant l'annule. Fermer quand même ?
ui-dialog-close-transfers-title = Transfert en cours
ui-dialog-import-host-keys = Serveurs SSH de confiance repris : { $keys ->
    [one] { $keys } clé
   *[other] { $keys } clés
} et { $pins ->
    [one] { $pins } empreinte
   *[other] { $pins } empreintes
}.
ui-dialog-import-host-keys-failed = Les serveurs SSH de confiance n'ont pas pu être repris : { $detail }
ui-dialog-paste-dangerous-body = Le texte contient { $command }, une commande qui peut détruire des données ou arrêter la machine. Vérifiez-le avant qu'il n'atteigne le shell.
ui-dialog-paste-dangerous-confirm = Coller quand même
ui-dialog-paste-dangerous-title = Coller une commande dangereuse ?
ui-dialog-session-logging-body = Chaque session de terminal sera écrite dans un fichier : ce que vous tapez comme ce qui s'affiche, y compris les mots de passe ou jetons renvoyés par le terminal. L'activer ?
ui-dialog-session-logging-confirm = Activer
ui-dialog-session-logging-title = Enregistrer les transcriptions des sessions ?
ui-error-winrm-https-gateway = WinRM par une passerelle SSH ne prend pas en charge HTTPS. Utilisez HTTP, ou connectez-vous directement.
ui-error-winrm-tls-failed = La connexion TLS WinRM vers '{ $host }' sur le port { $port } a échoué (certificat non approuvé ou erreur de négociation).
ui-error-winrm-unreachable = L'hôte WinRM '{ $host }' est injoignable sur le port { $port } (connexion refusée ou délai dépassé).
ui-error-winrm-unresolved = Impossible de résoudre l'hôte WinRM '{ $host }'.
ui-files-back-button = Précédent
ui-files-conflict-action = Action
ui-files-conflict-apply = Appliquer
ui-files-conflict-apply-all = Appliquer à tous :
ui-files-conflict-destination = Destination
ui-files-conflict-folder-skip = Ce dossier et tout son contenu prévu seront ignorés.
ui-files-conflict-hint = Choisissez ce que Heimdall doit faire avant le début du transfert.
ui-files-conflict-rename = Renommer automatiquement
ui-files-conflict-replace = Remplacer
ui-files-conflict-skip = Ignorer
ui-files-conflict-summary = { $count ->
    [one] { $count } destination en conflit
   *[other] { $count } destinations en conflit
}
ui-files-conflict-title = Conflits de fichiers
ui-files-error-destination-not-a-file = Envoi refusé : la destination existe déjà et n'est pas un fichier ordinaire.
ui-files-error-is-link = Les permissions d'un lien symbolique ne peuvent pas être changées : le serveur changerait celles de sa cible.
ui-files-error-replace-not-safe = Envoi refusé : la destination existe déjà et le serveur ne peut pas la remplacer sans risque, elle a donc été laissée telle quelle.
ui-files-home-button = Accueil
ui-profile-experience = Expérience visuelle
ui-profile-experience-composition = Activer la composition du bureau
ui-profile-experience-font-smoothing = Activer le lissage des polices (ClearType)
ui-profile-experience-no-animations = Désactiver les animations de menu
ui-profile-experience-no-cursor-shadow = Désactiver l'ombre du curseur
ui-profile-experience-no-drag = Désactiver le glisser fenêtre complète
ui-profile-experience-no-themes = Désactiver les thèmes
ui-profile-experience-no-wallpaper = Désactiver le fond d'écran
ui-profile-field-passphrase = Passphrase de clé
ui-profile-legacy-algorithms-hint = Les échanges de clés SHA-1, les chiffrements CBC, HMAC-SHA1 et les clés d'hôte RSA SHA-1 sont proposés après les actuels. À activer uniquement pour un appareil qui ne connaît rien de plus récent.
ui-profile-passphrase-clear-tooltip = Supprimer la passphrase enregistrée pour cette session
ui-profile-passphrase-hint = Sert uniquement à déchiffrer la clé SSH choisie. Laissez vide si la clé n'a pas de passphrase ou si un agent SSH la déverrouille.
ui-profile-passphrase-saved = Passphrase enregistrée
ui-profile-skip-cert-hint = Désactive la vérification du certificat TLS. À utiliser uniquement pour des hôtes internes de confiance aux certificats auto-signés.
ui-profile-toggle-anti-idle = Activer le maintien anti-inactivité
ui-profile-toggle-auto-reconnect = Se reconnecter automatiquement
ui-profile-toggle-legacy-algorithms = Autoriser les anciens algorithmes pour les vieux appareils
ui-profile-toggle-several-servers = Plusieurs serveurs répondent à cette adresse : demander pour chaque nouveau certificat
ui-profile-use-ssl-hint = Utilise WinRM sur HTTPS, normalement le port 5986. HTTP utilise normalement le port 5985.
ui-profile-winrm-gateway-http = Le SSL WinRM est désactivé quand une passerelle SSH est choisie. WinRM par une passerelle utilise HTTP dans le tunnel SSH local.
ui-profile-winrm-tls-on-http-port = TLS est activé mais le port est celui par défaut en clair, { $http } ; WinRM sur TLS écoute sur { $https }.
ui-settings-anti-idle-interval = Intervalle anti-inactivité (0 = désactivé)
ui-settings-anti-idle-refused = L'intervalle anti-inactivité doit être 0, ou compris entre { $min } et { $max } secondes.
ui-settings-anti-idle-unit = s
ui-settings-rdp-auto-reconnect = Reconnexion automatique
ui-settings-session-logging-record = Enregistrer les transcriptions des sessions (ce que chaque terminal affiche, saisie comprise)
ui-settings-session-logging-warning = Les transcriptions gardent ce que vous tapez comme ce qui s'affiche, y compris les mots de passe ou jetons renvoyés par le terminal. Gardez le dossier des journaux privé.
ui-settings-ssh-keep-alive-hint = Fréquence à laquelle Heimdall envoie les maintiens de connexion SSH sur les sessions, SFTP, tunnels et passerelles, pour qu'une connexion inactive ne soit pas coupée par un pare-feu ou le serveur. S'applique aux connexions ouvertes après le changement.
ui-settings-ssh-keep-alive-interval = Intervalle de maintien SSH
ui-settings-ssh-keep-alive-refused = L'intervalle de maintien SSH doit être compris entre { $min } et { $max } secondes.
ui-settings-ssh-session = Session
ui-settings-ssh-tmout-reset-interval = Intervalle de réinitialisation TMOUT (0 = désactivé)
ui-settings-ssh-tmout-reset-refused = L'intervalle de réinitialisation TMOUT SSH doit être compris entre { $min } et { $max } secondes.
ui-status-link-not-a-folder = { $name } ne pointe pas vers un dossier.
ui-status-winrm-certificate-skipped = La validation du certificat TLS WinRM a été ignorée pour cette session.
ui-status-winrm-gateway-ntlm = WinRM par une passerelle : Kerberos n'est pas disponible, l'authentification passe par NTLM.
ui-winrm-diagnostic-ntlm-loopback = L'authentification WinRM a échoué pour l'identité Windows actuelle. Utilisez un compte enregistré pour localhost ou les hôtes hors domaine.
ui-winrm-diagnostic-wsman-invalid = WinRM a reçu une réponse WSMan invalide. Si cette session passe par une passerelle, vérifiez que WinRM utilise HTTP dans le tunnel.
ui-certificate-subject = Sujet : { $subject }
ui-session-copy-anonymous-button = Copier le rapport anonymisé
ui-error-report-anonymous-header = Rapport de diagnostic { $protocol } (anonymisé)
ui-error-report-kind = Échec :
ui-error-report-anonymous-hint = Contient la date, la version et le type d'échec. Exclut les adresses, les comptes et le texte des erreurs.
