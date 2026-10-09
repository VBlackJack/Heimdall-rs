# Copyright 2026 Julien Bombled
# Licensed under the Apache License, Version 2.0. See LICENSE.

ui-window-title = Heimdall
ui-window-title-tab = { $tab } - Heimdall
ui-window-title-detached = { $tab } - Détaché

ui-sidebar-title = Sessions
ui-sidebar-tools-tab = Outils
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
ui-settings-search-placeholder = Rechercher dans les paramètres...
ui-settings-search-tooltip = Rechercher dans les paramètres (Ctrl+F)
ui-settings-search-clear = Effacer le filtre de recherche
ui-settings-search-results = Résultats : { $count }
ui-settings-search-no-results = Aucun réglage correspondant
ui-settings-find-modified = Trouver les paramètres modifiés
ui-settings-find-modified-hint = Liste les paramètres qui diffèrent de leur valeur par défaut.
ui-settings-modified-badge = Modifié
ui-settings-modified-from-default = Modifié, valeur par défaut : { $value }
ui-settings-reset-to-default = Rétablir
ui-settings-reset-to-default-tooltip = Rétablir la valeur par défaut : { $value }
ui-settings-tab-general = Général
ui-settings-tab-terminal = Terminal
ui-settings-tab-ssh = SSH
ui-settings-tab-rdp = RDP
ui-settings-tab-security = Sécurité
ui-settings-security = Sécurité
ui-settings-posture-title = Vue d'ensemble de la sécurité
ui-settings-posture-description = Les choix de ce panneau qui touchent à la sécurité, tels qu'ils sont maintenant. Une ligne qui demande attention dit pourquoi et mène au paramètre.
ui-settings-posture-summary-none = Aucun paramètre à risque
ui-settings-posture-summary = { $count ->
    [one] { $count } point demande attention
   *[other] { $count } points demandent attention
}
ui-settings-posture-line = { $label } : { $state }
ui-settings-posture-go-to = Aller au paramètre
ui-settings-posture-state-enabled = Activé
ui-settings-posture-state-disabled = Désactivé
ui-settings-posture-state-after-minutes = { $minutes ->
    [one] Après { $minutes } minute d'inactivité
   *[other] Après { $minutes } minutes d'inactivité
}
ui-settings-posture-state-never = Jamais
ui-settings-posture-state-requires-vault = Demande le mot de passe maître
ui-settings-posture-state-required = Exigé
ui-settings-posture-state-not-required = Non exigé
ui-settings-posture-label-rdp-nla = Authentification au niveau du réseau (NLA) en RDP
ui-settings-posture-label-rdp-strict-server-auth = Authentification stricte du serveur RDP
ui-settings-posture-label-transcripts = Transcription des sessions
ui-settings-posture-label-ps-policy = Politique d'exécution PowerShell
ui-settings-posture-label-vault = Mot de passe maître
ui-settings-posture-label-auto-lock = Verrouillage auto après inactivité
ui-settings-posture-label-disconnect-on-lock = Déconnexion des sessions au verrouillage
ui-settings-posture-label-credential-guard = Credential Guard
ui-settings-posture-label-windows-hello = Windows Hello avant la connexion
ui-settings-posture-label-update-checks = Vérification automatique des mises à jour
ui-settings-posture-label-known-hosts-sync = Import de known_hosts au démarrage
ui-settings-posture-warning-rdp-nla = Sans NLA, vous vous connectez sur un serveur qui n'a pas prouvé son identité.
ui-settings-posture-warning-transcripts = Chaque session de terminal est écrite dans un fichier, y compris les mots de passe ou jetons renvoyés par le terminal.
ui-settings-posture-warning-ps-policy = Cette politique désactive la vérification de signature des scripts dans les sessions PowerShell qu'ouvre Heimdall.
ui-settings-posture-warning-auto-lock = Le mot de passe maître reste déverrouillé tant que Heimdall tourne.
ui-settings-posture-warning-update-checks = Une version corrigeant une faille passe inaperçue tant que vous ne vérifiez pas à la main.
ui-settings-posture-warning-known-hosts-sync = À chaque démarrage, les clés d'hôte de known_hosts, un fichier que d'autres programmes écrivent aussi, sont approuvées sans question.
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
ui-settings-connection-checks = Contrôles à la connexion
ui-settings-require-credential-guard = Exiger Credential Guard
ui-settings-require-credential-guard-hint = Lorsque cette option est activée, les sessions ne s'ouvrent dans le client RDP intégré que si Credential Guard est actif sur cet ordinateur. S'il ne l'est pas, ou si son état ne peut pas être lu, elles sont bloquées. La Connexion Bureau à distance n'est pas concernée.
ui-settings-credential-guard-enabled = Credential Guard : Activé
ui-settings-credential-guard-disabled = Credential Guard : Non disponible
ui-settings-credential-guard-disabled-reason = Credential Guard : Non disponible ({ $reason })
ui-settings-credential-guard-reason-not-windows = ce système n'est pas Windows
ui-settings-credential-guard-reason-no-system-folder = le dossier système de Windows est inconnu
ui-settings-credential-guard-reason-not-started = PowerShell n'a pas pu démarrer : { $detail }
ui-settings-credential-guard-reason-timed-out = aucune réponse en { $seconds } secondes
ui-settings-credential-guard-reason-exited = PowerShell s'est terminé avec le code { $code }
ui-settings-credential-guard-reason-stopped = PowerShell s'est terminé sans code de sortie
ui-settings-credential-guard-reason-no-instance = Device Guard ne donne aucun état
ui-settings-credential-guard-reason-unread = la réponse n'a pas pu être lue : { $detail }
ui-settings-credential-guard-reason-no-value = aucun service de sécurité n'est signalé
ui-settings-credential-guard-reason-invalid-value = les services de sécurité signalés ne peuvent pas être lus
ui-settings-require-windows-hello = Exiger Windows Hello avant la connexion
ui-settings-require-windows-hello-hint = Lorsque cette option est activée, une vérification Windows Hello (biométrie ou code PIN) réussie est requise avant l'utilisation des identifiants enregistrés. Si Windows Hello n'est pas disponible ou non configuré, les connexions sont bloquées.
ui-settings-require-windows-hello-windows-only = Windows Hello n'est disponible que sous Windows : ici, les connexions sont bloquées tant que cette option est activée.
ui-settings-windows-hello-grace = Revérifier après
ui-settings-windows-hello-grace-refused = Le délai de grâce Windows Hello doit être compris entre { $min } et { $max } minutes.
ui-profile-field-vault-entry = Nom de l'entrée du coffre
ui-profile-vault-entry-placeholder = Laisser vide pour utiliser le nom d'affichage
ui-profile-vault-entry-help = Nom facultatif de l'entrée de ce serveur dans le gestionnaire de mots de passe externe. Utilisé pour la recherche {"{"}Title{"}"} du fournisseur d'identifiants ; vide, le nom d'affichage est utilisé.
ui-status-provider-no-password = Le fournisseur d'identifiants externe n'a retourné aucun mot de passe pour "{ $name }". Vérifiez la commande dans Paramètres > Sécurité.
ui-status-provider-failed = Échec du fournisseur d'identifiants externe : { $detail }
ui-status-provider-timed-out = Délai du fournisseur d'identifiants externe dépassé.
ui-status-windows-hello-unavailable = Windows Hello est requis mais indisponible. Configurez Windows Hello ou désactivez l'exigence dans les Paramètres.
ui-status-windows-hello-failed = Échec de la vérification Windows Hello. Connexion annulée.
ui-status-windows-hello-cancelled = Vérification Windows Hello annulée. Connexion annulée.
ui-status-credential-guard-required = Credential Guard est requis mais n'est pas actif sur ce système
ui-status-vault-hello-enrol-again-failed = Le coffre est déverrouillé, mais le déverrouillage Windows Hello n'a pas pu être réactivé.
ui-windows-hello-verify-reason = Vérifiez votre identité pour vous connecter
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
ui-settings-browse-database-filter = Fichiers de base de données
ui-settings-browse-key-file-filter = Fichiers de clé
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
ui-settings-vault-hello-enable = Activer le déverrouillage Windows Hello
ui-settings-vault-hello-disable = Désactiver le déverrouillage Windows Hello
ui-settings-vault-hello-max-days = Redemander le mot de passe maître après (0 = jamais)
ui-settings-vault-hello-max-days-hint = Windows Hello déverrouille le coffre tant que ce nombre de jours ne s'est pas écoulé depuis la dernière saisie du mot de passe maître ; ensuite, le mot de passe maître est demandé une fois.
ui-settings-vault-hello-max-days-refused = Le rappel du mot de passe maître doit être compris entre { $min } et { $max } jours.
ui-settings-vault-hello-status-available = Le déverrouillage Windows Hello est disponible pour ce coffre.
ui-settings-vault-hello-status-enabled = Le déverrouillage Windows Hello est activé.
ui-settings-vault-hello-status-unavailable = Le déverrouillage Windows Hello nécessite un appareil TPM 2.0 et Windows Hello.
ui-settings-vault-hello-status-enrolling = Activation du déverrouillage Windows Hello
ui-settings-vault-hello-status-unlock-required = Déverrouillez le coffre avec votre mot de passe maître avant d'activer le déverrouillage Windows Hello.
ui-settings-auto-lock = Verrouillage auto après inactivité (0 = désactivé)
ui-settings-auto-lock-hint = Verrouille l'espace de travail après ce nombre de minutes d'inactivité globale. 0 désactive le verrouillage auto.
ui-settings-auto-lock-requires-vault = Le verrouillage automatique et la déconnexion au verrouillage demandent le mot de passe maître ci-dessus : activez-le pour les utiliser.
ui-settings-auto-lock-refused = Le seuil de verrouillage automatique par inactivité doit être compris entre { $min } et { $max } minutes.
ui-settings-disconnect-on-lock = Déconnecter les sessions au verrouillage
ui-settings-disconnect-on-lock-hint = Par défaut, les sessions continuent de tourner masquées derrière le verrou. Activez ceci pour les déconnecter au verrouillage.
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
ui-vault-hello-unlock-button = Déverrouiller avec Windows Hello
ui-vault-hello-unlock-busy = Attente de Windows Hello
ui-vault-problem-hello-locked = Windows Hello est temporairement verrouillé. Utilisez votre mot de passe maître.
ui-vault-problem-hello-not-found = Windows Hello n'est plus configuré. Déverrouillez avec votre mot de passe maître.
ui-vault-problem-hello-failed = Impossible de déverrouiller avec Windows Hello. Utilisez votre mot de passe maître.
ui-vault-hello-enrol-again-title = Réactiver le déverrouillage Windows Hello ?
ui-vault-hello-enrol-again-body = Windows Hello a été réinitialisé ou supprimé sur cet appareil. Réactiver le déverrouillage Windows Hello pour ce coffre maintenant ?
ui-vault-hello-enrol-again-button = Réactiver
ui-vault-problem-system = Le fichier du coffre n'a pas pu être utilisé : { $detail }
ui-vault-save-failed-title = Le mot de passe n'a pas pu être enregistré.
ui-sidebar-group-none = (Sans dossier)

ui-home-welcome = Bienvenue dans Heimdall-rs.
ui-home-subtitle = Ajoutez une session ou importez vos connexions existantes pour commencer.
ui-home-add-button = Ajouter une session
ui-home-import-button = Importer des connexions
ui-home-explore-tools-button = Explorer les outils
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
ui-error-pinned-certificate-invalid = Le certificat approuvé pour { $target } n'est plus valide : connexion refusée. { $issue } Valide jusqu'au : { $until }. Clé : { $fingerprint }. Si le serveur a un nouveau certificat, oubliez ce serveur : son certificat est alors soumis à votre accord.
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
ui-dialog-import-gateways = Passerelles SSH : { $created ->
    [one] { $created } créée
   *[other] { $created } créées
}, { $merged ->
    [one] { $merged } fusionnée
   *[other] { $merged } fusionnées
}, { $orphans ->
    [one] { $orphans } référence orpheline
   *[other] { $orphans } références orphelines
}.
ui-dialog-import-gateways-orphans-action = Certaines sessions importées référencent encore des passerelles SSH manquantes. Réexportez depuis un build qui embarque les passerelles, ou recréez/réassignez la passerelle dans les Paramètres avant de vous connecter.
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
ui-dialog-save-failed-title = Erreur de sauvegarde
ui-dialog-save-failed-body = Échec de la sauvegarde : { $detail }
ui-dialog-store-changed-title = Profils non enregistrés
ui-dialog-store-changed-body = Le fichier des profils a été modifié par un autre programme ou un autre Heimdall depuis sa lecture : ce changement n'a donc pas été enregistré et le fichier est resté tel quel. Rouvrez Heimdall pour charger le fichier actuel, puis refaites le changement.
ui-dialog-detail = Détail : { $detail }

ui-import-skip-not-ssh = pas un profil SSH ({ $kind })
ui-import-skip-missing-host = n'a pas d'hôte
ui-import-skip-missing-id = n'a pas d'identifiant
ui-import-skip-invalid-port = port invalide { $port }

ui-tab-files-title = { $name } (fichiers)
ui-tab-rdp-forced-embedded-title = { $name } (forcé intégré)

ui-files-local-title = Cet ordinateur
ui-files-remote-title = Serveur
ui-files-ftp-cleartext-badge = Transmis en clair (sans TLS)
ui-files-up-tooltip = Remonter d'un niveau
ui-files-refresh-tooltip = Actualiser le répertoire
ui-files-new-folder-button = Nouveau dossier
ui-files-new-folder-tooltip = Créer un nouveau dossier
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
ui-error-vnc-security-refused = Le serveur VNC ne propose aucune sécurité qu'accepte Heimdall (proposées : { $offered }). Heimdall accepte l'authentification VNC, directe ou dans Tight ou VeNCrypt, le TLS de VeNCrypt avec un certificat de serveur (X509Vnc, X509Plain, X509None), et un serveur qui ne demande pas de mot de passe seulement si le profil l'autorise. Le TLS anonyme (TLSVnc, TLSPlain, TLSNone) n'est pas pris en charge.
ui-error-vnc-tls-required = Un certificat est approuvé pour ce serveur VNC, mais il ne propose plus TLS (proposées : { $offered }). Heimdall ne se rabat pas sur une connexion non chiffrée : quelqu'un peut se trouver entre vous et le serveur. Si le serveur a été reconfiguré, oubliez son certificat dans les Paramètres, onglet SSH, Certificats VNC approuvés.
ui-error-vnc-tls-required-by-profile = Ce profil VNC exige TLS, et le serveur n'en propose pas (proposées : { $offered }). Heimdall ne se replie pas sur une connexion non chiffrée. Si le serveur ne peut pas chiffrer, décochez "Exiger TLS" dans le profil.
ui-session-vnc-unencrypted = Non chiffré : ce bureau et ce que vous tapez traversent le réseau en clair.
ui-session-vnc-encrypted = Chiffré avec { $version }, certificat du serveur vérifié.
ui-session-vnc-quality = Qualité
ui-session-vnc-quality-best = Meilleure qualité
ui-session-vnc-quality-balanced = Équilibré
ui-session-vnc-quality-performance = Performance
ui-session-vnc-quality-low-bandwidth = Faible bande passante
ui-local-starting = Démarrage de { $name }...
ui-local-admin-badge = ADMIN
ui-local-elevated-starting = { $name } : Windows demande les droits d'administrateur.
ui-local-elevated-started = { $name } s'exécute en administrateur dans sa propre fenêtre.
ui-local-elevated-cancelled = L'élévation a été annulée par l'utilisateur.
ui-local-elevated-failed = Échec du démarrage du processus élevé : { $detail }
ui-local-elevated-unsupported = L'exécution en tant qu'administrateur n'existe que sous Windows. Rien n'a été démarré.
ui-local-elevated-program = Programme : { $program }
ui-local-elevated-open-again-button = L'ouvrir à nouveau
ui-error-local-shell = Le shell local n'a pas pu démarrer : { $detail }
ui-import-skip-unsafe-local = son programme, ses arguments ou son dossier ne peuvent pas être lancés tels quels (chemin relatif, guillemet, caractère NUL, ou dossier sur une autre machine)
ui-dialog-local-title = Lancer ce programme ?
ui-dialog-local-body = Le profil { $name } lance la commande ci-dessous. Heimdall ne la lance qu'avec votre accord, et redemande si elle change.
ui-dialog-local-folder = Démarre dans : { $folder }
ui-dialog-local-rereads = Ce programme relit sa ligne de commande avec ses propres règles : & | ^ < > et % y sont des commandes, pas du texte.
ui-dialog-local-elevated = Il s'exécute en administrateur, dans sa propre fenêtre, une fois que Windows a demandé les droits d'administrateur.
ui-dialog-local-confirm = Lancer
ui-dialog-run-script-title = Lancer ce script ?
ui-dialog-run-script-body = { $name } se lance dans un nouvel onglet avec la commande ci-dessous, avec vos droits. Heimdall redemande à chaque lancement.
ui-error-remote-forward = La passerelle SSH n'a pas voulu écouter sur son port { $port } pour la redirection distante : la redirection y est désactivée, ou le port y est déjà pris.
ui-error-proxy-port = Le proxy SOCKS n'a pas pu ouvrir le port local { $port } : un autre programme l'utilise peut-être. ({ $detail })
ui-error-jump-refused = La passerelle SSH n'a pas voulu se connecter à { $target } : la redirection y est désactivée, ou cet hôte n'est pas joignable depuis elle.
ui-import-skip-missing-username = se connecte avec un compte qu'il ne nomme pas
ui-import-skip-unknown-identity = se connecte avec un mode d'identité que Heimdall ne connaît pas
ui-error-hostkey-changed-at = La clé d'hôte de { $target } n'est pas celle enregistrée : la connexion est peut-être interceptée. Enregistrée : { $recorded }. Présentée : { $offered }.
ui-error-gateway-missing = La passerelle SSH { $id } par laquelle passe ce profil n'est pas dans les profils.
ui-error-gateway-loop = La passerelle SSH { $id } est atteinte par elle-même, via ses parents.
ui-tree-connect = Connecter
ui-tree-connect-with = Se connecter en mode...
ui-tree-connect-with-tooltip = Forcer le mode RDP pour cette connexion uniquement, sans modifier le profil.
ui-tree-connect-embedded = Se connecter (intégré)
ui-tree-connect-external-mstsc = Se connecter (mstsc externe)
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
ui-tree-import-citrix = Importer apps Citrix
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
ui-profile-options-ftp-desc = Configurez le comportement de la connexion FTP.
ui-profile-toggle-passive = Mode passif (recommandé pour les réseaux derrière un pare-feu)
ui-profile-toggle-ftps = Activer SSL/TLS (FTPS)
ui-profile-protocol-citrix-name = Citrix
ui-profile-protocol-citrix-desc = Application publiée Citrix Workspace
ui-profile-citrix-title = Citrix Workspace
ui-profile-citrix-desc = Configurez l'URL StoreFront et le nom de l'application, ou fournissez un fichier ICA.
ui-profile-field-storefront-url = URL StoreFront
ui-profile-field-app-name = Nom de l'application
ui-profile-citrix-advanced-title = Options avancées Citrix
ui-profile-citrix-advanced-desc = Fichier ICA, mode transparent et authentification unique.
ui-profile-field-ica-file = Chemin du fichier ICA
ui-profile-citrix-hint = Fournissez une URL StoreFront + nom d'application, ou un chemin direct vers un fichier ICA.
ui-profile-toggle-seamless = Mode transparent
ui-profile-toggle-sso = Utiliser le SSO (Kerberos)
ui-profile-toggle-sso-hint = Utiliser la connexion unique avec l'identité Kerberos Windows actuelle. Nécessite un client joint à un domaine.
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
ui-profile-local-run-as-admin = Exécuter en tant qu'administrateur (ouvre sa propre fenêtre)
ui-profile-local-run-as-admin-hint = Windows demande les droits d'administrateur, puis démarre le shell dans une fenêtre séparée, pas dans un onglet.
ui-profile-local-run-as-admin-windows-only = L'exécution en tant qu'administrateur n'existe que sous Windows : ici, ce profil n'ouvre rien.
ui-profile-error-local-arguments = Les arguments laissent un guillemet ouvert.
ui-profile-protocol-vnc-name = VNC
ui-profile-protocol-vnc-desc = Partage d'écran à distance
ui-profile-protocol-telnet-name = Telnet
ui-profile-protocol-telnet-desc = Terminal non chiffré (legacy)
ui-profile-protocol-badge = Protocole
ui-profile-protocol-change-tooltip = Cliquer pour changer de protocole
ui-profile-tab-general = Général
ui-profile-tab-options = Options
ui-profile-tab-network = Réseau
ui-profile-tab-info = Info
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
ui-profile-credentials-vnc-desc = Configurez le mot de passe VNC pour l'authentification.
ui-profile-username-rdp-placeholder = utilisateur, DOMAINE\utilisateur ou user@domaine
ui-profile-username-vnc-placeholder = facultatif : seulement pour un serveur qui demande un nom d'utilisateur (VeNCrypt Plain, sur TLS)
ui-profile-field-domain = Domaine Windows
ui-profile-domain-placeholder = CORP ou corp.example.com
ui-profile-domain-hint = Le nom NetBIOS (CORP) ou le domaine DNS (corp.example.com). Laissez vide si le nom d'utilisateur ci-dessus en porte déjà un.
ui-profile-winrm-identity = Identité
ui-profile-winrm-identity-current = Identité Windows courante
ui-profile-winrm-identity-stored = Identifiant stocké
ui-profile-winrm-password-hint = Laissez le mot de passe vide pour le saisir dans PowerShell à la connexion. Un mot de passe enregistré que l'hôte refuse n'est plus essayé tant qu'un nouveau n'est pas enregistré.
ui-profile-options-rdp = Options de session RDP
ui-profile-options-rdp-desc = Paramètres d'affichage, de transport et de mode de session pour le client Bureau à distance.
ui-profile-options-vnc = Options VNC
ui-profile-options-vnc-desc = Configurez le comportement de l'affichage VNC.
ui-profile-options-telnet = Options Telnet
ui-profile-options-telnet-desc = Paramètres de connexion Telnet.
ui-profile-toggle-clipboard = Rediriger le presse-papiers
ui-profile-toggle-drives = Rediriger les lecteurs
ui-profile-toggle-nla = Activer l'authentification au niveau du réseau
ui-profile-rdp-follow-defaults = Utiliser les valeurs RDP par défaut globales
ui-profile-rdp-defaults-banner = Ce serveur utilise vos valeurs RDP par défaut. Décochez "Utiliser les valeurs RDP par défaut globales" pour configurer des options propres à ce serveur.
ui-profile-rdp-defaults-not-in-effect = Les options grisées ci-dessous viennent des valeurs par défaut globales : les valeurs affichées pour elles sont celles propres à ce serveur, pas celles appliquées.
ui-profile-rdp-tab-display-audio = Affichage et audio
ui-profile-rdp-display-section = Affichage
ui-profile-rdp-audio-section = Audio
ui-profile-rdp-microphone = Capturer le microphone local
ui-profile-rdp-multi-monitor = Activer le mode multi-écran
ui-profile-rdp-multi-monitor-note = Le multi-écran utilise les affichages locaux sélectionnés. Les changements nécessitent une reconnexion.
ui-profile-rdp-monitors-title = Moniteurs sélectionnés
ui-profile-rdp-monitors-caption = Choisissez les moniteurs utilisés par la session distante. Aucun coché = tous les moniteurs.
ui-profile-rdp-monitors-offline-kept = Les moniteurs enregistrés pour cette session qui ne sont pas connectés actuellement sont conservés.
ui-profile-rdp-monitor = Moniteur { $number } : { $width }x{ $height }
ui-profile-rdp-monitor-primary = { $monitor } (principal)
ui-profile-rdp-monitor-vertical = { $monitor } (vertical)
ui-profile-rdp-tab-devices = Périphériques
ui-profile-rdp-tab-performance = Performance
ui-profile-rdp-connection-section = Connexion
ui-profile-rdp-bitmap-cache = Conserver le cache bitmap sur disque entre les sessions
ui-profile-rdp-compression = Activer la compression RDP
ui-profile-rdp-hardware-acceleration = Utiliser le rendu accéléré par le matériel
ui-profile-rdp-hardware-acceleration-unsupported = Pas encore pris en charge : le client intégré n'a pas ce réglage, et Connexion Bureau à distance (mstsc.exe) n'en lit aucun dans son fichier .rdp.
ui-profile-rdp-disable-udp = Éviter la sonde de transport UDP
ui-profile-rdp-tab-behavior = Comportement
ui-profile-rdp-security-section = Sécurité
ui-profile-rdp-full-screen = Ouvrir en plein écran
ui-profile-rdp-mstsc-only = Client externe (mstsc.exe) uniquement
ui-settings-rdp-defaults = Paramètres RDP
ui-settings-rdp-defaults-hint = Les options de tout serveur RDP qui utilise les valeurs par défaut globales.
ui-settings-rdp-default-mode = Mode RDP par défaut
ui-settings-rdp-default-mode-embedded = Intégré
ui-settings-rdp-default-mode-external = Externe
ui-settings-rdp-default-mode-hint = Intégré : client RDP dans Heimdall. Externe : ouvre mstsc.exe dans une fenêtre séparée.
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
ui-profile-resize-delay = Délai de redimensionnement dynamique (ms)
ui-profile-resize-delay-global = { $ms } (valeur globale par défaut)
ui-profile-resolution-scale-fixed = Mettre la résolution fixe à l'échelle du panneau
ui-profile-resolution-dynamic = Autoriser la résolution dynamique
ui-profile-nla-off-hint = Sans authentification au niveau du réseau, un mot de passe enregistré n'est pas envoyé : Heimdall le demande.
ui-profile-toggle-use-ssl = Utiliser SSL
ui-profile-toggle-skip-cert = Ignorer la validation du certificat (non sûr)
ui-profile-toggle-view-only = Mode lecture seule (pas de clavier ni de souris)
ui-profile-toggle-no-password = Autoriser un serveur qui ne demande pas de mot de passe
ui-profile-toggle-require-tls = Exiger TLS (refuser un serveur qui ne chiffre pas)
ui-profile-telnet-warning = Telnet envoie tout sans chiffrement, mots de passe compris.
ui-profile-section-organization = Organisation
ui-profile-section-organization-desc = Utilisez les métadonnées de regroupement pour organiser la liste des sessions.
ui-profile-section-metadata = Métadonnées
ui-profile-section-metadata-desc = Tags optionnels et métadonnées Wake-on-LAN pour cette entrée de session.
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
ui-profile-error-resize-delay = Laissez le délai de redimensionnement RDP vide pour hériter de la valeur globale, saisissez 0 pour désactiver le verrouillage, ou saisissez une valeur de { $min } à { $max } ms.
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
ui-profile-ssh-mode = Mode SSH
ui-profile-ssh-mode-embedded = Intégré (dans l'application)
ui-profile-ssh-mode-external = Externe (PuTTY)
ui-profile-ssh-mode-external-desc = Ouvre PuTTY dans une fenêtre séparée. PuTTY demande lui-même le mot de passe.
ui-profile-toggle-x11 = Activer le transfert X11
ui-profile-toggle-x11-hint = Transférer l'affichage X11 vers la machine locale pour les applications graphiques (option -X)
ui-profile-x11-warning = Le transfert X11 permet à l'hôte distant de voir l'affichage de cet ordinateur et les touches frappées dans ses fenêtres. Ne l'activez que pour un serveur de confiance.
ui-profile-toggle-compression = Activer la compression
ui-profile-options-ssh = Options SSH
ui-profile-options-ssh-desc = Mode de session et options SSH utilisées par la connexion shell ou transfert de fichiers.
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
ui-tab-menu-detach = Détacher dans une fenêtre
ui-detach-reattach = Réattacher à la fenêtre principale
ui-tab-menu-close-others = Fermer les autres
ui-tab-menu-close-right = Fermer celles à droite
ui-split-merge-with = Fusionner avec...
ui-split-horizontal = Horizontal
ui-split-vertical = Vertical
ui-split-unsplit = Annuler la division
ui-split-swap-panes = Inverser les panneaux
ui-split-toggle-orientation = Changer l'orientation
ui-split-close-secondary = Fermer le panneau secondaire
ui-split-detach-secondary = Détacher le panneau secondaire
ui-split-max-panes-reached = Nombre maximum de panneaux atteint ({ $max }).
ui-status-detach-split-refused = Un onglet divisé ne peut pas être déplacé dans sa propre fenêtre. Annulez d'abord la division.
ui-split-menu = Diviser...
ui-split-session-tooltip = Diviser la vue de session
ui-split-palette-hint = Rechercher un serveur avec lequel diviser...
ui-split-drop-to-split = Déposer pour diviser
ui-tab-drag-detach-hint = Relâcher pour détacher dans une fenêtre
ui-split-open-in-split = Ouvrir dans une division
ui-split-open-in-split-disabled = Ouvrez d'abord une session : une division partage la session active.
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
ui-error-report-tunnel = Tunnel :
ui-error-report-tunnel-route = via { $route }
ui-error-report-tunnel-hops = { $count ->
    [one] par { $count } passerelle SSH
   *[other] par { $count } passerelles SSH
}
ui-error-report-session = Session :
ui-error-report-session-duration = connectée depuis { $duration }
ui-error-report-duration = { $minutes } min { $seconds } s

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
ui-status-connected-short = Connecté
ui-status-state = { $name } : { $state }
ui-status-connecting = Connexion...
ui-status-reconnecting = Reconnexion...
ui-status-disconnected = Déconnecté
ui-status-error = Erreur
ui-status-copied = Copié dans le presse-papiers : { $text }
ui-status-report-now = Maintenant : { $text }
ui-status-report-line = { $time } { $text }
ui-status-folder-created = Dossier "{ $path }" créé.
ui-status-sessions = { $count ->
    [one] { $count } session
   *[other] { $count } sessions
}
ui-status-sessions-filtered = { $shown } sur { $count ->
    [one] { $count } session
   *[other] { $count } sessions
}
ui-status-separator = {" | "}

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
ui-settings-theme = Thème
ui-theme-dracula = Dracula
ui-theme-drakul = Drakul
ui-theme-striga = Striga
ui-theme-cinder = Cinder
ui-theme-bracken = Bracken
ui-theme-tarn = Tarn
ui-theme-mortis = Mortis
ui-theme-slate = Slate
ui-theme-magellan = Magellan
ui-theme-voivode = Voivode
ui-theme-carmilla = Carmilla
ui-theme-whitby = Whitby
ui-theme-vesper = Vesper
ui-theme-parchment = Parchment
ui-theme-folio = Folio
ui-theme-wormwood = Wormwood
ui-theme-sconce = Sconce
ui-theme-high-contrast = Contraste élevé
ui-settings-accent = Accent
ui-accent-default = Défaut
ui-accent-blue = Bleu
ui-accent-cyan = Cyan
ui-accent-green = Vert
ui-accent-orange = Orange
ui-accent-pink = Rose
ui-accent-purple = Violet
ui-accent-red = Rouge
ui-accent-yellow = Jaune
ui-settings-font-size = Taille de police
ui-settings-font-size-unit = px
ui-settings-font-size-refused = La taille de police du terminal doit être entre { $min } et { $max }.
ui-settings-font-family = Police
ui-settings-font-family-missing = { $family } n'est pas installée sur cet ordinateur : le terminal utilise { $fallback }.
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
ui-settings-browse-log-directory-title = Sélectionner le dossier de journalisation
ui-settings-session-log-retention = Supprimer les transcripts de plus de
ui-settings-days-unit = jours
ui-settings-minutes-unit = min
ui-settings-session-log-retention-hint = 0 conserve tous les transcripts. Seuls les transcripts de session sont supprimés ; les journaux d'événements et d'opérations sur fichiers sont conservés.
ui-settings-session-log-retention-refused = La conservation des transcripts doit valoir 0 (tout garder) ou être comprise entre { $min } et { $max } jours.

ui-broadcast-button = DIFFUSION
ui-broadcast-toggle-tooltip = Activer/désactiver la diffusion (envoyer à tous les terminaux), Ctrl+Alt+B
ui-broadcast-on = Mode diffusion ACTIF - { $scope }
ui-broadcast-off = Mode diffusion DÉSACTIVÉ
ui-broadcast-scope-current = Onglet actuel
ui-broadcast-scope-all = Tous les onglets
ui-broadcast-scope-selected = Onglets sélectionnés ({ $count })
ui-broadcast-scope-status = Portée de la diffusion : { $scope }
ui-broadcast-scope-tooltip = Portée de la diffusion (cliquer pour passer de l'onglet actuel à tous les onglets, puis aux onglets marqués)
ui-broadcast-target-on = ◉
ui-broadcast-target-off = ○
ui-broadcast-target-tooltip = Envoyer la diffusion vers cette session (cible de diffusion)
ui-dialog-broadcast-title = Diffuser vers tous les onglets ?
ui-dialog-broadcast-body = La saisie sera envoyée aux terminaux de tous les onglets ouverts, y compris ceux en arrière-plan. Continuer ?
ui-dialog-broadcast-confirm = Diffuser

ui-files-go-tooltip = Naviguer vers le chemin

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
ui-files-properties-created = Créé :
ui-files-properties-accessed = Dernier accès :
ui-files-properties-attributes = Attributs :
ui-files-properties-link-target = Cible du lien :
ui-files-attribute-read-only = Lecture seule
ui-files-attribute-hidden = Caché
ui-files-attribute-normal = Normal
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
ui-files-bookmarks-empty = Aucun signet enregistré
ui-files-bookmark-added = Signet ajouté : { $path }

ui-files-filter-placeholder = Filtrer les fichiers...
ui-files-hidden-toggle = .*
ui-files-hidden-tooltip = Afficher les fichiers cachés
ui-files-local-toggle = Fichiers locaux
ui-files-local-toggle-tooltip = Afficher les fichiers de cet ordinateur à côté de ceux du serveur
ui-files-sudo-toggle = sudo
ui-files-sudo-tooltip = Naviguer en tant que root (sudo)
ui-files-follow-toggle = cwd
ui-files-follow-tooltip = Suivre le répertoire du terminal SSH
ui-files-follow-local-tooltip = Suivre le répertoire du terminal
ui-files-sudo-on = Mode sudo activé - navigation en tant que root
ui-files-sudo-off = Mode sudo désactivé
ui-files-item-count = { $count } éléments
ui-files-item-count-filtered = { $shown }/{ $count } éléments

ui-files-drop-overlay = Déposer les fichiers pour envoyer

ui-trusted-host-keys-title = Clés d'hôtes approuvées
ui-trusted-host-keys-hint = Contrôler les clés d'hôtes SSH que Heimdall approuve pour les prochaines connexions.
ui-settings-sync-known-hosts = Importer le fichier known_hosts d'OpenSSH au démarrage
ui-settings-sync-known-hosts-hint = À chaque démarrage de Heimdall, les clés d'hôte du fichier .ssh/known_hosts de votre utilisateur sont ajoutées à cette liste. Une clé différente d'une clé déjà approuvée ici n'est pas remplacée.
ui-trusted-host-keys-search = Rechercher des hôtes approuvés
ui-trusted-host-keys-host = Hôte:Port
ui-trusted-host-keys-algorithm = Algorithme
ui-trusted-host-keys-fingerprint = Empreinte
ui-trusted-host-keys-source = Source
ui-trusted-host-keys-first-seen = Première vue
ui-trusted-host-keys-last-seen = Dernière vue
ui-trusted-host-keys-source-user = Confirmée par l'utilisateur
ui-trusted-host-keys-source-imported = Importée de known_hosts
ui-trusted-host-keys-source-unknown = Inconnue
ui-trusted-host-keys-date-unknown = Inconnue
ui-trusted-host-keys-details = Détails
ui-trusted-host-keys-copy = Copier l'empreinte
ui-trusted-host-keys-remove = Supprimer
ui-trusted-host-keys-empty-title = Aucune clé d'hôte approuvée
ui-trusted-host-keys-empty-body = Connectez-vous d'abord à un serveur : sa clé est demandée, puis listée ici.
ui-trusted-host-key-details-title = Détails de la clé d'hôte approuvée
ui-trusted-host-key-details-public-key = Blob de clé publique
ui-trusted-host-key-details-public-key-unavailable = (indisponible - reconnectez-vous pour le capturer)
ui-trusted-host-key-details-close = Fermer
ui-trusted-certificates-title = Certificats RDP approuvés
ui-trusted-certificates-hint = Les certificats acceptés pour un bureau à distance, conservés entre les redémarrages. En oublier un le retire de la liste de confiance de son serveur.
ui-trusted-certificates-search = Rechercher par serveur ou empreinte
ui-trusted-certificates-server = Serveur
ui-trusted-certificates-fingerprint = Empreinte
ui-trusted-certificates-thumbprint = Certificat : { $thumbprint }
ui-trusted-certificates-subject = Sujet
ui-trusted-certificates-issuer = Émetteur
ui-trusted-certificates-trusted = Approuvé depuis
ui-trusted-certificates-forget = Oublier
ui-trusted-certificates-forget-server = Oublier le serveur
ui-trusted-certificates-empty-title = Aucun certificat RDP approuvé
ui-trusted-certificates-empty-body = Les certificats que vous acceptez en vous connectant à un bureau distant sont listés ici, et peuvent y être révoqués.
ui-trusted-ftps-certificates-title = Certificats FTPS approuvés
ui-trusted-ftps-certificates-hint = Les certificats acceptés pour un serveur FTPS, conservés entre les redémarrages. En oublier un le retire de la liste de confiance de son serveur.
ui-trusted-ftps-certificates-empty-title = Aucun certificat FTPS approuvé
ui-trusted-ftps-certificates-empty-body = Les certificats que vous acceptez en vous connectant à un serveur FTPS sont listés ici, et peuvent y être révoqués.
ui-trusted-vnc-certificates-title = Certificats VNC approuvés
ui-trusted-vnc-certificates-hint = Les certificats acceptés pour un serveur VNC qui chiffre avec TLS, conservés entre les redémarrages. Tant qu'un certificat est approuvé, son serveur doit continuer à utiliser TLS. En oublier un le retire de la liste de confiance de son serveur.
ui-trusted-vnc-certificates-empty-title = Aucun certificat VNC approuvé
ui-trusted-vnc-certificates-empty-body = Les certificats que vous acceptez en vous connectant à un serveur VNC sur TLS sont listés ici, et peuvent y être révoqués.
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
ui-dialog-forget-server-certificates-title = Oublier les certificats de ce serveur ?
ui-dialog-forget-server-certificates-body = { $count ->
    [one] Heimdall va oublier { $count } certificat approuvé pour { $server }. La prochaine connexion à ce serveur posera de nouveau la question.
   *[other] Heimdall va oublier les { $count } certificats approuvés pour { $server }. La prochaine connexion à ce serveur posera de nouveau la question.
}
ui-status-fingerprint-copied = Empreinte complète copiée pour { $server }.
ui-status-host-key-removed = Clé d'hôte approuvée supprimée pour { $server }.
ui-status-certificate-forgotten = Certificat oublié pour { $server }.
ui-status-server-certificates-forgotten = Tous les certificats oubliés pour { $server }.

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
ui-resolution-skip-stabilization = Ignorer la stabilisation
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
ui-files-error-paste-link = Impossible de coller { $name } : les liens et les jonctions ne sont pas copiés à travers. Collez plutôt ce vers quoi ils pointent.
ui-error-key-not-absolute = Le chemin de la clé SSH doit être absolu : { $path }
ui-files-error-changed-on-server = Le fichier a changé sur le serveur depuis son ouverture : il a été laissé tel quel.
ui-files-error-changed-since-confirmed = L'élément a changé sur le serveur depuis la confirmation de la suppression : il a été laissé tel quel.
ui-files-error-file-too-large = Les fichiers de plus de 16 Mio doivent être téléchargés.
ui-files-menu-open-in-terminal = Ouvrir dans le terminal
ui-files-menu-open-in-explorer = Ouvrir dans l'Explorateur
ui-files-menu-run-in-shell = Exécuter dans le terminal
ui-files-menu-open-with = Ouvrir avec...
ui-files-menu-open-in-editor = Ouvrir dans l'éditeur
ui-status-resolution-reconnected = Le changement de résolution a nécessité une reconnexion.
ui-status-stabilization-skipped = Stabilisation ignorée - la résolution dynamique est désormais active.
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
ui-files-error-open-failed = Impossible de l'ouvrir sur cet ordinateur : { $detail }
ui-files-error-script-character = Ce script n'a pas été lancé : son chemin contient { $character }, que son interpréteur lirait comme autre chose qu'une partie du chemin. Renommez le fichier ou son dossier pour le lancer.
ui-files-error-script-not-text = Ce script n'a pas été lancé : son chemin n'est pas un texte valide, que son interpréteur ne pourrait pas recevoir tel quel.
ui-settings-external-editor = Éditeur externe
ui-settings-external-editor-path = Chemin de l'éditeur externe
ui-settings-external-editor-hint = Chemin vers l'éditeur de texte pour l'édition SFTP distante (laisser vide pour le programme par défaut)
ui-settings-browse-editor-title = Sélectionner l'éditeur externe
ui-settings-putty-path = Chemin PuTTY (pour le mode SSH externe)
ui-settings-putty-path-hint = Nécessaire quand le mode SSH est défini sur Externe. Recherché dans les dossiers du PATH si laissé vide.
ui-settings-browse-putty-title = Sélectionner putty.exe ou puttycac.exe
ui-settings-browse-executables = Exécutables
ui-settings-browse-all-files = Tous les fichiers
ui-settings-ssh-default-mode = Mode SSH par défaut
ui-settings-ssh-default-mode-embedded = Intégré
ui-settings-ssh-default-mode-external = Externe
ui-settings-ssh-default-mode-hint = Intégré : terminal dans Heimdall. Externe : ouvre PuTTY dans une fenêtre séparée.
ui-settings-apply-mode-to-all = Appliquer à toutes les sessions enregistrées
ui-settings-apply-mode-to-all-tooltip = Enregistre ce mode comme mode par défaut et l'écrit dans chaque session enregistrée de ce protocole
ui-settings-x11 = Serveur X11
ui-settings-x11-server-path = Chemin du serveur X11
ui-settings-x11-server-path-hint = Laissez vide pour essayer VcXsrv, Xming et Cygwin/X à leur emplacement d'installation, puis les dossiers du PATH.
ui-settings-browse-x11-title = Sélectionner le serveur X11
ui-settings-x11-auto-start = Démarrer automatiquement le serveur X11 si nécessaire
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
ui-files-error-sudo-protected = Non supprimé en tant que root : "/", un dossier système de premier niveau, un dossier personnel lui-même, ou un chemin qui n'est pas absolu ou qui remonte avec "..".
ui-files-menu-paste-explorer = Coller depuis l'Explorateur
ui-status-explorer-no-files = Aucun fichier n'est copié dans l'Explorateur.
ui-status-rdp-files-too-many = Fichiers non copiés vers le serveur : une copie prend { $count } fichiers et dossiers au plus.
ui-status-rdp-files-too-large = Fichiers non copiés vers le serveur : une copie prend { $size } au plus.
ui-files-menu-edit-sudo = Éditer avec sudo
ui-files-edit-save-sudo = Enregistrer avec sudo
ui-status-files-saved-sudo = Enregistré via sudo : { $name }
ui-dialog-sudo-title = Mot de passe sudo
ui-dialog-sudo-body = sudo demande votre mot de passe sur ce serveur pour "{ $name }". Il est gardé pour cet onglet seulement, jusqu'à sa fermeture ou un refus de sudo.
ui-dialog-sudo-delete-title = Supprimer en tant que root ?
ui-dialog-sudo-delete-body = Ces éléments seront supprimés en tant que root, un dossier avec tout son contenu. Rien ne permet de revenir en arrière.
ui-dialog-sudo-delete-more = et { $count } de plus
ui-dialog-sudo-delete-confirm = Supprimer en tant que root
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
ui-dialog-open-link-title = Ouvrir le lien
ui-dialog-open-link-body = Le texte cliqué mène à cette adresse, qui s'ouvrira dans votre navigateur :
    { $url }
ui-dialog-open-link-confirm = Ouvrir
ui-dialog-open-runnable-title = Ouvrir un programme
ui-dialog-open-runnable-body = Ce fichier s'exécute comme un programme sur cet ordinateur, avec vos droits. Ne l'ouvrez que si vous lui faites confiance :
    { $path }
ui-dialog-open-runnable-confirm = Ouvrir
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
ui-selection-edit-password = Mot de passe... ({ $count })
ui-bulk-password-header = { $count ->
    [one] Modification du mot de passe sur { $count } serveur
   *[other] Modification du mot de passe sur { $count } serveurs
}
ui-bulk-password-label = Nouveau mot de passe :
ui-bulk-password-confirm-label = Confirmer le mot de passe :
ui-bulk-password-control = Le mot de passe ne peut pas contenir de caractères de contrôle.
ui-bulk-password-mismatch = Les mots de passe ne correspondent pas.
ui-bulk-password-skipped-winrm = { $count ->
    [one] { $count } profil WinRM ignoré, car aucun nom d'utilisateur n'est configuré.
   *[other] { $count } profils WinRM ignorés, car aucun nom d'utilisateur n'est configuré.
}
ui-bulk-password-skipped-no-account = { $count ->
    [one] { $count } profil ignoré, car aucun nom d'utilisateur n'est configuré.
   *[other] { $count } profils ignorés, car aucun nom d'utilisateur n'est configuré.
}
ui-bulk-password-skipped-other = { $count ->
    [one] { $count } profil ignoré, car son protocole n'enregistre pas de mot de passe.
   *[other] { $count } profils ignorés, car leur protocole n'enregistre pas de mot de passe.
}
ui-status-bulk-password-updated = { $count ->
    [one] Mot de passe modifié sur { $count } serveur.
   *[other] Mot de passe modifié sur { $count } serveurs.
}
ui-status-bulk-password-updated-with-skipped = { $updated } { $skipped }
ui-status-bulk-password-partial = Mot de passe enregistré sur { $count } des { $total } serveurs.
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
ui-settings-prevent-sleep = Empêcher la mise en veille pendant les sessions actives
ui-settings-prevent-sleep-hint = Empêche Windows de se mettre en veille tant qu'une session est connectée ; l'écran peut toujours s'éteindre ou se verrouiller.
ui-settings-max-sessions = Sessions intégrées max
ui-settings-max-sessions-none = Aucune limite
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
ui-certificate-owner-tab = Cette question vient de l'onglet "{ $tab }".
ui-settings-rdp-resolution-presets = Préréglages de résolution
ui-settings-rdp-resolution-presets-hint = Un préréglage par ligne, format LARGEURxHAUTEUR (ex. 1920x1080). Laissez la zone vide pour utiliser la liste intégrée.
ui-settings-rdp-resolution-presets-reset = Réinitialiser aux valeurs par défaut
ui-settings-rdp-resolution-presets-invalid = Préréglages de résolution : ces lignes ne sont pas au format LARGEURxHAUTEUR avec une largeur de { $min } à { $width } et une hauteur de { $min } à { $height } pixels : { $lines }
ui-settings-rdp-reset-defaults = Réinitialiser les valeurs RDP
ui-settings-rdp-reset-defaults-tooltip = Restaure uniquement les valeurs RDP par défaut. Les autres réglages ne sont pas modifiés.
ui-dialog-reset-rdp-title = Réinitialiser les valeurs RDP ?
ui-dialog-reset-rdp-body = Restaurer toutes les valeurs RDP par défaut ? Les serveurs existants ne sont pas affectés.
ui-settings-reset-all = Valeurs par défaut
ui-dialog-reset-all-title = Réinitialiser tous les réglages ?
ui-dialog-reset-all-body = Restaurer tous les onglets des Paramètres aux valeurs d'usine ? Le texte saisi et pas encore appliqué est perdu. Dans l'onglet Sécurité, cela désactive le fournisseur d'identifiants externe, l'exigence Credential Guard et l'exigence Windows Hello à la connexion, et réinitialise le délai de grâce Windows Hello, le délai de verrouillage automatique et la déconnexion au verrouillage. Votre langue, votre thème, vos sessions, passerelles SSH, mot de passe maître, code PIN et inscription Windows Hello sont conservés. Le changement est enregistré aussitôt.
ui-dialog-apply-ssh-mode-title = Appliquer à toutes les sessions SSH enregistrées ?
ui-dialog-apply-ssh-mode-body = { $changes } des { $total } sessions SSH enregistrées passeront en mode { $mode }, et { $mode } devient le mode par défaut des nouvelles sessions. Action irréversible.
ui-dialog-apply-rdp-mode-title = Appliquer à toutes les sessions RDP enregistrées ?
ui-dialog-apply-rdp-mode-body = { $changes } des { $total } sessions RDP enregistrées passeront en mode { $mode }, et { $mode } devient le mode par défaut des nouvelles sessions. Action irréversible.
ui-settings-tab-about = À propos
ui-about-version = Version { $version }
ui-about-tagline = Gestionnaire de connexions sécurisé RDP/SSH/SFTP
ui-about-section-system = Système
ui-about-platform = Plateforme
ui-about-iced = iced
ui-about-russh = russh
ui-about-ironrdp = IronRDP
ui-about-build-date = Date de build
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
ui-shortcuts-toggle-split = Changer l'orientation de la division
ui-shortcuts-next-pane = Panneau suivant de l'onglet divisé
ui-shortcuts-previous-pane = Panneau précédent de l'onglet divisé
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
ui-shortcuts-copy-status = Copier l'état et ses derniers messages, pour un lecteur d'écran
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
ui-shortcuts-toggle-tools-panel = Afficher/masquer le panneau d'outils
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
ui-settings-updates = Mises à jour
ui-settings-updates-enabled = Vérifier les mises à jour automatiquement
ui-settings-updates-interval = Intervalle de vérification
ui-settings-hours-unit = h
ui-settings-updates-interval-refused = L'intervalle de vérification des mises à jour doit être compris entre { $min } et { $max } heures.
ui-settings-updates-current-version = Version actuelle
ui-settings-updates-development-build = { $version } (version de développement)
ui-settings-updates-check-now = Vérifier maintenant
ui-settings-updates-skipped-version = Version ignorée : { $version }
ui-settings-updates-clear-skipped = La proposer à nouveau
ui-settings-updates-checking = Recherche de mises à jour...
ui-settings-updates-up-to-date = Vous utilisez la dernière version.
ui-settings-updates-available = Mise à jour disponible : { $version }
ui-settings-updates-unknown-version = Version actuelle inconnue ; impossible de vérifier les mises à jour.
ui-settings-updates-failed-network = Impossible de joindre le serveur de mises à jour. Vérifiez votre connexion internet et réessayez.
ui-settings-updates-failed-secure-channel = Impossible d'établir une connexion sécurisée vers le serveur de mises à jour. Un proxy, un certificat manquant ou une horloge système fausse peuvent en être la cause.
ui-settings-updates-failed-proxy = Un proxy a refusé la connexion au serveur de mises à jour. Vérifiez sa configuration, ou s'il exige des identifiants.
ui-settings-updates-failed-rate-limited = Le serveur de mises à jour refuse temporairement les requêtes venant de ce réseau. Réessayez plus tard.
ui-settings-updates-failed-rate-limited-minute = Le serveur de mises à jour refuse temporairement les requêtes venant de ce réseau. Réessayez dans environ une minute.
ui-settings-updates-failed-rate-limited-minutes = Le serveur de mises à jour refuse temporairement les requêtes venant de ce réseau. Réessayez dans environ { $minutes } minutes.
ui-settings-updates-failed-access-denied = Le serveur de mises à jour a refusé la requête. Voir le journal pour le détail.
ui-settings-updates-failed-unavailable = Le serveur de mises à jour est indisponible. Réessayez plus tard.
ui-settings-updates-failed-malformed = Le serveur de mises à jour a répondu quelque chose que Heimdall n'a pas su lire. Voir le journal pour le détail.
ui-settings-updates-failed-timed-out = Le serveur de mises à jour n'a pas répondu à temps. Réessayez plus tard.
ui-update-banner-text = Une nouvelle version est disponible : { $version }
ui-update-banner-view-release = Voir la release
ui-update-banner-later = Plus tard
ui-update-banner-skip = Ignorer cette version
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
ui-status-listing-cancelled = Chargement annulé.
ui-status-session-limit = Le maximum de { $max } sessions distantes intégrées est déjà ouvert. Fermez une session avant d'en ouvrir une autre.
ui-status-sftp-auto-open-failed = Ouverture automatique SFTP échouée : { $reason }
ui-status-sftp-browser-disabled = Le navigateur SFTP intégré est désactivé dans les paramètres.
ui-status-nothing-to-undo = Rien à annuler.
ui-shortcuts-undo-move = Annuler le dernier déplacement fait en glissant dans l'arborescence
ui-tab-menu-vnc-remote-resize = Redimensionner le bureau distant à l'onglet
ui-detail-host-port = { $host } : { $port }
ui-detail-folder = Dossier :
ui-detail-environment = Environnement :
ui-detail-username = Utilisateur :
ui-detail-gateway = Passerelle :
ui-detail-credentials = Identifiants enregistrés :
ui-detail-tags = Tags :
ui-detail-favorite = Favori :
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
ui-nav-tools = Outils
ui-nav-settings = Paramètres
ui-nav-about = À propos
ui-tools-filter-placeholder = Filtrer les outils...
ui-tools-no-results = Aucun outil correspondant
ui-tools-no-results-hint = Essayez un autre terme ou un alias comme ping, dns, json ou password.
ui-tools-context-with = Les outils réseau utiliseront la cible sélectionnée : { $host }
ui-tools-context-none = Les outils réseau s'ouvriront sans cible héritée.
ui-tools-favorites = Favoris
ui-tools-recent = Utilisés récemment
ui-tools-all = Tous les outils
ui-tools-empty-favorites = Épinglez vos outils favoris pour un accès rapide
ui-tools-pin-tooltip = Épingler aux favoris
ui-tools-unpin-tooltip = Retirer des favoris
ui-tools-page-title = Outils
ui-tools-search-placeholder = Rechercher un outil...
ui-tools-count =
    { $count ->
        [one] { $count } outil
       *[other] { $count } outils
    }
ui-tools-category-network = Réseau
ui-tools-category-security = Sécurité
ui-tools-category-encoding = Encodage et format
ui-tools-category-system = Système
ui-tools-category-external = Externe
ui-tool-help-tooltip = Afficher l'aide
ui-tool-help-close = Fermer
ui-tool-copy-tooltip = Copier dans le presse-papiers
ui-tool-base64-name = Encodeur / Décodeur Base64
ui-tool-base64-description = Encodeur et décodeur Base64 pour texte et fichiers
ui-tool-base64-title = Encodeur / Décodeur Base64
ui-tool-base64-input = Entrée
ui-tool-base64-output = Sortie
ui-tool-base64-encode = Encoder →
ui-tool-base64-decode = ← Décoder
ui-tool-base64-copy = Copier la sortie
ui-tool-base64-browse = Parcourir...
ui-tool-base64-file-mode = Mode fichier
ui-tool-base64-url-safe = URL-safe (RFC 4648)
ui-tool-base64-placeholder = texte à encoder/décoder
ui-tool-base64-empty = Saisissez du texte et appuyez sur Encoder ou Décoder.
ui-tool-base64-status-encoded =
    { $count ->
        [one] { $count } octet encodé
       *[other] { $count } octets encodés
    }
ui-tool-base64-status-decoded =
    { $count ->
        [one] { $count } octet décodé
       *[other] { $count } octets décodés
    }
ui-tool-base64-status-saved = Enregistré dans { $path }
ui-tool-base64-status-error = Erreur : { $error }
ui-tool-base64-status-invalid = Entrée Base64 invalide
ui-tool-base64-save-title = Enregistrer le fichier décodé
ui-tool-base64-open-title = Sélectionner un fichier à encoder
ui-tool-base64-file-loaded =
    { $count ->
        [one] Fichier chargé : { $name } ({ $count } octet)
       *[other] Fichier chargé : { $name } ({ $count } octets)
    }
ui-tool-base64-too-large = Le fichier dépasse la limite de 5 Mo.
ui-tool-base64-help =
    Encodeur/Décodeur Base64

    Encode du texte en Base64 ou décode du Base64 en texte.

    Utilisation :
    Collez du texte et cliquez sur Encoder ou Décoder. Supporte la variante Base64 URL-safe.
ui-tool-urlenc-name = Encodeur / Décodeur URL
ui-tool-urlenc-description = Encodeur et décodeur d'URL pour chaînes de requête et chemins
ui-tool-urlenc-title = Encodeur / Décodeur URL
ui-tool-urlenc-decoded = Décodé
ui-tool-urlenc-encoded = Encodé
ui-tool-urlenc-copy = Copier
ui-tool-urlenc-component = Encodage strict des composants (encoder tous les caractères réservés)
ui-tool-urlenc-decoded-placeholder = URL ou texte à encoder/décoder
ui-tool-urlenc-encoded-placeholder = URL encodée
ui-tool-urlenc-help =
    Encodeur/Décodeur URL

    Encode ou décode les chaînes URL-encodées (percent-encoding).

    Utilisation :
    Collez une URL ou du texte et cliquez sur Encoder ou Décoder.
ui-tool-uuid-name = Générateur UUID
ui-tool-uuid-description = Générateur UUID/GUID avec options de formats multiples
ui-tool-uuid-title = Générateur UUID
ui-tool-uuid-result-v4 = UUID généré (v4)
ui-tool-uuid-result-v7 = UUID généré (v7)
ui-tool-uuid-v4 = v4 (Aléatoire)
ui-tool-uuid-v7 = v7 (Horodatage)
ui-tool-uuid-generate = Générer
ui-tool-uuid-copy = Copier
ui-tool-uuid-uppercase = Majuscules
ui-tool-uuid-hyphens = Avec tirets
ui-tool-uuid-batch = Génération par lot
ui-tool-uuid-count = Nombre
ui-tool-uuid-generate-batch = Générer le lot
ui-tool-uuid-copy-batch = Tout copier
ui-tool-uuid-help =
    Générateur UUID

    Génère des identifiants universellement uniques.

    Utilisation :
    Cliquez sur Générer pour créer des UUID. Supporte v4 (aléatoire) avec plusieurs options de format (standard, majuscules, sans tirets, URN).
ui-tool-hash-name = Générateur de hachage
ui-tool-hash-description = Générateur de hachage pour fichiers et texte (MD5, SHA-1, SHA-256, SHA-512)
ui-tool-hash-title = Générateur de hachage
ui-tool-hash-input = Texte en entrée
ui-tool-hash-placeholder = texte à hacher
ui-tool-hash-drop-zone = Déposez un fichier ici ou cliquez Parcourir pour hacher un fichier
ui-tool-hash-browse = Parcourir
ui-tool-hash-clear-file = Effacer le fichier
ui-tool-hash-hashing = Hachage du fichier...
ui-tool-hash-empty = Saisissez du texte ou parcourez un fichier pour calculer les empreintes.
ui-tool-hash-results = Résultats de hachage
ui-tool-hash-copy = Copier
ui-tool-hash-save = Enregistrer
ui-tool-hash-verify = Hash attendu (vérification)
ui-tool-hash-verify-placeholder = coller le hash à vérifier
ui-tool-hash-byte-length = { $count } octets
ui-tool-hash-file-status = { $name } - { $size }
ui-tool-hash-too-large = Le fichier dépasse la taille maximale ({ $size }).
ui-tool-hash-not-found = Fichier introuvable.
ui-tool-hash-access-denied = Accès refusé au fichier.
ui-tool-hash-error = Impossible de calculer le hash du fichier.
ui-tool-hash-match = ✓ Correspondance ({ $algorithm })
ui-tool-hash-no-match = ✗ Aucune correspondance
ui-tool-hash-all-files = Tous les fichiers (*.*)
ui-tool-hash-help =
    Générateur de Hash

    Calcule les empreintes cryptographiques pour du texte ou des fichiers.

    Utilisation :
    1. Tapez ou collez du texte, OU
    2. Glissez-déposez un fichier (ou cliquez Parcourir)
    3. Toutes les valeurs de hash sont calculées simultanément

    Algorithmes supportés :
    - MD5 (128 bits)
    - SHA-1 (160 bits)
    - SHA-256 (256 bits)
    - SHA-384 (384 bits)
    - SHA-512 (512 bits)
    - SHA3-256 (256 bits, si supporté)

    Mode vérification :
    Collez un hash connu dans le champ Vérifier pour comparer. L'algorithme est auto-détecté par la longueur du hash.

    Exemples :
    - Tapez "hello" - SHA-256 : 2cf24dba...
    - Déposez un fichier - Vérifiez avec un checksum connu
ui-tool-hmac-name = Générateur HMAC
ui-tool-hmac-description = Générateur de code d'authentification de message HMAC
ui-tool-hmac-title = Générateur HMAC
ui-tool-hmac-algorithm = Algorithme
ui-tool-hmac-key = Clé secrète
ui-tool-hmac-key-placeholder = clé secrète
ui-tool-hmac-toggle-key = Afficher/masquer la clé
ui-tool-hmac-input = Texte du message
ui-tool-hmac-input-placeholder = message
ui-tool-hmac-format = Format de sortie
ui-tool-hmac-format-hex = Hex
ui-tool-hmac-format-base64 = Base64
ui-tool-hmac-empty = Saisissez une clé et un message pour calculer un HMAC.
ui-tool-hmac-output = Résultat HMAC
ui-tool-hmac-copy = Copier
ui-tool-hmac-byte-length = { $bytes } octets ({ $bits } bits)
ui-tool-hmac-verify = HMAC attendu (vérification)
ui-tool-hmac-verify-placeholder = coller le HMAC à vérifier
ui-tool-hmac-match = Correspondance
ui-tool-hmac-no-match = Aucune correspondance
ui-tool-hmac-help =
    Générateur HMAC

    Calcule les codes d'authentification de message par hachage.

    Utilisation :
    Entrez un message et une clé secrète, puis sélectionnez un algorithme (SHA-256, SHA-512, etc.) pour calculer le HMAC.
ui-tool-jwt-name = Analyseur JWT
ui-tool-jwt-description = Décodeur et validateur de jetons Web JSON
ui-tool-jwt-title = Analyseur JWT
ui-tool-jwt-input = Collez le jeton JWT
ui-tool-jwt-placeholder = coller le jeton JWT (header.payload.signature)
ui-tool-jwt-empty = Collez un jeton JWT pour décoder son en-tête, son contenu et sa signature.
ui-tool-jwt-error-format = Format JWT invalide. Un JWT doit contenir exactement 3 parties séparées par des points.
ui-tool-jwt-error-decode = Échec du décodage JWT. Vérifiez que le jeton est en Base64Url valide.
ui-tool-jwt-expired = Expiré : { $date }
ui-tool-jwt-valid = Valide jusqu'au : { $date }
ui-tool-jwt-no-expiry = Aucune revendication d'expiration (exp) trouvée
ui-tool-jwt-header = En-tête
ui-tool-jwt-payload = Charge utile
ui-tool-jwt-signature = Signature
ui-tool-jwt-copy = Copier
ui-tool-jwt-verify-title = Vérification de la signature
ui-tool-jwt-unsupported = La vérification RSA/ECDSA nécessite une clé publique (non pris en charge)
ui-tool-jwt-secret = Secret HMAC
ui-tool-jwt-verify = Vérifier
ui-tool-jwt-signature-valid = La signature est valide
ui-tool-jwt-signature-invalid = La signature est invalide
ui-tool-jwt-help =
    Analyseur JWT

    Décode et inspecte les JSON Web Tokens.

    Utilisation :
    Collez un JWT pour voir son en-tête, son contenu et sa signature. L'expiration et les claims sont affichés dans un format lisible.
ui-tool-totp-name = Générateur TOTP
ui-tool-totp-description = Générateur de mots de passe à usage unique basés sur le temps (2FA/MFA)
ui-tool-totp-title = Générateur TOTP
ui-tool-totp-secret = Clé secrète (Base32)
ui-tool-totp-secret-placeholder = clé secrète Base32
ui-tool-totp-start = Démarrer
ui-tool-totp-code = Code actuel
ui-tool-totp-copy = Copier
ui-tool-totp-remaining = { $seconds }s restantes
ui-tool-totp-info = Entrez une clé secrète encodée en Base32 (comme fournie par Google Authenticator, Authy, etc.) pour générer des mots de passe à usage unique basés sur le temps (TOTP). Les codes se renouvellent toutes les 30 secondes.
ui-tool-totp-error-required = Veuillez entrer une clé secrète.
ui-tool-totp-error-base32 = Encodage Base32 invalide. Utilisez uniquement les caractères A-Z et 2-7.
ui-tool-totp-help =
    Générateur TOTP

    Génère des mots de passe à usage unique basés sur le temps (RFC 6238).

    Utilisation :
    Entrez une clé secrète en Base32 pour générer des codes TOTP qui se renouvellent toutes les 30 secondes.
ui-tool-copy-value = Copier
ui-tool-number-group-separator = {" "}
ui-tool-number-decimal-separator = {","}
ui-tool-subnet-name = Calculateur de sous-réseau
ui-tool-subnet-description = Calculateur de sous-réseau avec notation CIDR et décomposition des adresses
ui-tool-subnet-title = Calculateur de sous-réseau
ui-tool-subnet-input = Adresse IP / notation CIDR (ex. 192.168.1.0/24)
ui-tool-subnet-placeholder = 192.168.1.0/24
ui-tool-subnet-network = Adresse réseau
ui-tool-subnet-broadcast = Adresse de diffusion
ui-tool-subnet-mask = Masque de sous-réseau
ui-tool-subnet-first-host = Premier hôte
ui-tool-subnet-last-host = Dernier hôte
ui-tool-subnet-total-hosts = Nombre d'hôtes
ui-tool-subnet-cidr = Notation CIDR
ui-tool-subnet-wildcard = Masque inverse
ui-tool-subnet-too-many = Trop nombreux pour lister
ui-tool-subnet-error-invalid = Adresse IP ou notation CIDR invalide. Format attendu : 192.168.1.0/24
ui-tool-subnet-empty = Saisissez une notation CIDR pour calculer les détails du sous-réseau
ui-tool-subnet-help =
    Calculateur de Sous-réseaux

    Calcule les informations réseau IPv4 et IPv6 à partir de la notation CIDR.

    Utilisation :
    1. Entrez une adresse IP avec préfixe CIDR (ex : 192.168.1.0/24)
    2. Les résultats se mettent à jour automatiquement

    Informations affichées :
    - Adresse réseau
    - Adresse de diffusion
    - Masque de sous-réseau et masque inverse
    - Premier et dernier hôte utilisable
    - Nombre total d'hôtes utilisables
    - Notation CIDR

    Exemples :
    - 192.168.1.0/24 - 254 hôtes (Classe C)
    - 10.0.0.0/8 - 16 777 214 hôtes (Classe A)
    - 172.16.0.0/16 - 65 534 hôtes (Classe B)
    - 192.168.1.64/26 - 62 hôtes (sous-réseau)
    - 2001:db8::/32 - Préfixe IPv6
ui-tool-ipconv-name = Convertisseur d'adresses IP
ui-tool-ipconv-description = Convertisseur de format d'adresse IP (décimal, binaire, hexadécimal)
ui-tool-ipconv-title = Convertisseur d'adresses IP
ui-tool-ipconv-input = Entrez une adresse IPv4 (décimale pointée, entier, hex ou binaire)
ui-tool-ipconv-placeholder = adresse IP ou entier
ui-tool-ipconv-dotted = Décimal pointé
ui-tool-ipconv-integer = Entier
ui-tool-ipconv-hex = Hexadécimal
ui-tool-ipconv-binary = Binaire
ui-tool-ipconv-mapped = IPv6 mappée IPv4
ui-tool-ipconv-error-invalid = Entrée invalide. Entrez une adresse IPv4 valide, un entier, un hex (0x...) ou un binaire pointé.
ui-tool-ipconv-empty = Saisissez une adresse IP, un entier, un hex ou un binaire pointé.
ui-tool-ipconv-help =
    Convertisseur IP

    Convertit les adresses IP entre formats décimal, hexadécimal, binaire et entier.

    Utilisation :
    Entrez une adresse IP dans n'importe quel format et toutes les conversions s'affichent instantanément.
ui-tool-netcalc-name = Calculateur réseau
ui-tool-netcalc-description = Calculateur réseau avancé avec support VLAN et supernet
ui-tool-netcalc-title = Calculateur réseau
ui-tool-netcalc-mode = Mode
ui-tool-netcalc-mode-supernet = Calculateur de super-réseau
ui-tool-netcalc-mode-range = Plage IP vers CIDR
ui-tool-netcalc-mode-vlan = Planificateur VLAN
ui-tool-netcalc-supernet-input = Plages CIDR (une par ligne)
ui-tool-netcalc-compute = Calculer
ui-tool-netcalc-start-ip = IP de début
ui-tool-netcalc-end-ip = IP de fin
ui-tool-netcalc-start-placeholder = ex. 192.168.1.0
ui-tool-netcalc-end-placeholder = ex. 192.168.1.255
ui-tool-netcalc-hosts-needed = Hôtes nécessaires
ui-tool-netcalc-hosts-placeholder = ex. 50
ui-tool-netcalc-base-network = Réseau de base
ui-tool-netcalc-base-placeholder = ex. 10.0.0.0
ui-tool-netcalc-error-no-cidrs = Entrez au moins une plage CIDR.
ui-tool-netcalc-error-invalid-cidr = Notation CIDR invalide : { $line }
ui-tool-netcalc-error-invalid-range = Entrez des adresses IPv4 valides pour le début et la fin.
ui-tool-netcalc-error-start-after-end = L'IP de début doit être inférieure ou égale à l'IP de fin.
ui-tool-netcalc-error-host-count = Entrez un nombre positif d'hôtes.
ui-tool-netcalc-error-base-network = Entrez une adresse de réseau de base IPv4 valide.
ui-tool-netcalc-supernet-result = Super-réseau : { $network }/{ $prefix }
ui-tool-netcalc-supernet-range = Plage : { $first } - { $last }
ui-tool-netcalc-supernet-hosts = Hôtes utilisables : { $hosts }
ui-tool-netcalc-range-result = Blocs CIDR couvrant la plage :
ui-tool-netcalc-vlan-network = Réseau : { $network }/{ $prefix }
ui-tool-netcalc-vlan-mask = Masque de sous-réseau : { $mask }
ui-tool-netcalc-vlan-broadcast = Diffusion : { $broadcast }
ui-tool-netcalc-vlan-usable-range = Plage utilisable : { $first } - { $last }
ui-tool-netcalc-vlan-usable-hosts = Hôtes utilisables : { $hosts }
ui-tool-netcalc-vlan-requested = Demandés : { $hosts }
ui-tool-netcalc-vlan-utilization = Utilisation : { $percent } %
ui-tool-netcalc-empty = Saisissez des sous-réseaux ou des plages IP à calculer.
ui-tool-netcalc-help =
    Calculateur Réseau

    Calculs réseau avancés incluant VLAN, supernetting et découpage de sous-réseaux.

    Utilisation :
    Entrez les paramètres réseau pour effectuer des calculs sur plusieurs sous-réseaux.
ui-tool-chmod-name = Calculateur Chmod
ui-tool-chmod-description = Calculateur interactif de permissions chmod pour les modes de fichiers Unix
ui-tool-chmod-title = Calculateur Chmod
ui-tool-chmod-read = Lecture
ui-tool-chmod-write = Écriture
ui-tool-chmod-execute = Exécution
ui-tool-chmod-owner = Propriétaire
ui-tool-chmod-group = Groupe
ui-tool-chmod-others = Autres
ui-tool-chmod-octal = Octal :
ui-tool-chmod-symbolic = Symbolique :
ui-tool-chmod-copy-octal = Copier l'octal
ui-tool-chmod-copy-symbolic = Copier le symbolique
ui-tool-chmod-presets = Préréglages courants
ui-tool-chmod-symbolic-input = Notation symbolique (ex. u+x,g-w,o=r) :
ui-tool-chmod-symbolic-placeholder = u+x,g-w,o=r
ui-tool-chmod-error-symbolic = Notation symbolique invalide
ui-tool-chmod-command-preview = Aperçu de la commande :
ui-tool-chmod-copy-command = Copier la commande
ui-tool-chmod-command = chmod { $mode } filename
ui-tool-chmod-help =
    Calculateur Chmod

    Calculateur interactif de permissions de fichiers Unix.

    Utilisation :
    Basculez les permissions lecture/écriture/exécution pour propriétaire, groupe et autres. Les valeurs chmod numériques et symboliques se mettent à jour en temps réel.
ui-tool-datetime-name = Convertisseur Date/Heure
ui-tool-datetime-description = Convertisseur date/heure et epoch Unix avec support des fuseaux horaires
ui-tool-datetime-title = Convertisseur Date/Heure
ui-tool-datetime-input = Horodatage Unix (secondes) ou date/heure ISO 8601
ui-tool-datetime-placeholder = timestamp, ISO 8601 ou chaîne de date
ui-tool-datetime-now = Maintenant
ui-tool-datetime-copy = Copier
ui-tool-datetime-unix = Horodatage Unix (secondes)
ui-tool-datetime-iso-utc = ISO 8601 (UTC)
ui-tool-datetime-iso-local = ISO 8601 (Local)
ui-tool-datetime-local-time = Heure locale
ui-tool-datetime-timezone = Fuseau horaire
ui-tool-datetime-relative = Temps relatif
ui-tool-datetime-detected-unix = Détecté : horodatage Unix
ui-tool-datetime-detected-ms = Détecté : horodatage Unix (millisecondes)
ui-tool-datetime-detected-iso = Détecté : date/heure ISO 8601
ui-tool-datetime-error-invalid = Entrée invalide. Saisissez un horodatage Unix ou une date/heure ISO 8601.
ui-tool-datetime-empty = Saisissez un horodatage Unix ou une date ISO 8601 à convertir.
ui-tool-datetime-relative-seconds = { $count } secondes
ui-tool-datetime-relative-minutes = { $count } minutes
ui-tool-datetime-relative-hours = { $count } heures
ui-tool-datetime-relative-days = { $count } jours
ui-tool-datetime-relative-months = { $count } mois
ui-tool-datetime-relative-years = { $count } ans
ui-tool-datetime-relative-ago = il y a { $duration }
ui-tool-datetime-relative-in = dans { $duration }
ui-tool-datetime-long = { $weekday } { $d } { $month } { $year } { $time }
ui-tool-datetime-weekday-0 = dimanche
ui-tool-datetime-weekday-1 = lundi
ui-tool-datetime-weekday-2 = mardi
ui-tool-datetime-weekday-3 = mercredi
ui-tool-datetime-weekday-4 = jeudi
ui-tool-datetime-weekday-5 = vendredi
ui-tool-datetime-weekday-6 = samedi
ui-tool-datetime-month-1 = janvier
ui-tool-datetime-month-2 = février
ui-tool-datetime-month-3 = mars
ui-tool-datetime-month-4 = avril
ui-tool-datetime-month-5 = mai
ui-tool-datetime-month-6 = juin
ui-tool-datetime-month-7 = juillet
ui-tool-datetime-month-8 = août
ui-tool-datetime-month-9 = septembre
ui-tool-datetime-month-10 = octobre
ui-tool-datetime-month-11 = novembre
ui-tool-datetime-month-12 = décembre
ui-tool-datetime-help =
    Convertisseur Date/Heure

    Convertit entre dates lisibles et horodatages Unix.

    Utilisation :
    Entrez une date ou une valeur epoch Unix. Supporte plusieurs formats de date et la conversion de fuseaux horaires.
ui-tool-ulid-name = Générateur d'ULID
ui-tool-ulid-description = Générateur d'ULID - identifiant 128 bits triable lexicographiquement (base32 Crockford)
ui-tool-ulid-title = Générateur d'ULID
ui-tool-ulid-result = ULID généré
ui-tool-ulid-generate = Générer
ui-tool-ulid-copy = Copier
ui-tool-ulid-batch = Génération en lot
ui-tool-ulid-count = Nombre
ui-tool-ulid-generate-batch = Générer le lot
ui-tool-ulid-copy-batch = Tout copier
ui-tool-ulid-help =
    Générateur d'ULID

    Génère des identifiants 128 bits triables lexicographiquement.

    Format : 26 caractères en base32 Crockford (sans I, L, O, U).

    Structure :
    - 10 premiers caractères : timestamp Unix en millisecondes (48 bits)
    - 16 derniers caractères : données aléatoires cryptographiques (80 bits)

    Les ULIDs générés dans l'ordre se trient dans l'ordre. Utile comme clé primaire dans les systèmes distribués.
ui-tool-crontab-name = Constructeur Crontab
ui-tool-crontab-description = Constructeur d'expressions crontab avec aperçu lisible
ui-tool-crontab-title = Constructeur Crontab
ui-tool-crontab-presets = Préréglages rapides
ui-tool-crontab-preset-every-minute = Chaque minute
ui-tool-crontab-preset-every-hour = Chaque heure
ui-tool-crontab-preset-daily-midnight = Quotidien minuit
ui-tool-crontab-preset-weekdays-9am = Jours ouvrables 9h
ui-tool-crontab-preset-weekly-sunday = Hebdo dimanche
ui-tool-crontab-preset-monthly-1st = Mensuel 1er
ui-tool-crontab-minute = Minute
ui-tool-crontab-hour = Heure
ui-tool-crontab-day-of-month = Jour du mois
ui-tool-crontab-month = Mois
ui-tool-crontab-day-of-week = Jour de la semaine
ui-tool-crontab-every-minute = Chaque minute (*)
ui-tool-crontab-every-5-min = Toutes les 5 minutes (*/5)
ui-tool-crontab-every-15-min = Toutes les 15 minutes (*/15)
ui-tool-crontab-every-30-min = Toutes les 30 minutes (*/30)
ui-tool-crontab-every-hour = Chaque heure (*)
ui-tool-crontab-every-day = Chaque jour (*)
ui-tool-crontab-every-month = Chaque mois (*)
ui-tool-crontab-every-day-of-week = Chaque jour (*)
ui-tool-crontab-month-1 = Jan
ui-tool-crontab-month-2 = Fév
ui-tool-crontab-month-3 = Mar
ui-tool-crontab-month-4 = Avr
ui-tool-crontab-month-5 = Mai
ui-tool-crontab-month-6 = Juin
ui-tool-crontab-month-7 = Juil
ui-tool-crontab-month-8 = Août
ui-tool-crontab-month-9 = Sep
ui-tool-crontab-month-10 = Oct
ui-tool-crontab-month-11 = Nov
ui-tool-crontab-month-12 = Déc
ui-tool-crontab-day-abbr-0 = Dim
ui-tool-crontab-day-abbr-1 = Lun
ui-tool-crontab-day-abbr-2 = Mar
ui-tool-crontab-day-abbr-3 = Mer
ui-tool-crontab-day-abbr-4 = Jeu
ui-tool-crontab-day-abbr-5 = Ven
ui-tool-crontab-day-abbr-6 = Sam
ui-tool-crontab-day-0 = Dimanche
ui-tool-crontab-day-1 = Lundi
ui-tool-crontab-day-2 = Mardi
ui-tool-crontab-day-3 = Mercredi
ui-tool-crontab-day-4 = Jeudi
ui-tool-crontab-day-5 = Vendredi
ui-tool-crontab-day-6 = Samedi
ui-tool-crontab-expression = Expression Cron
ui-tool-crontab-copy = Copier
ui-tool-crontab-manual-edit = Édition manuelle (expression cron à 5 champs)
ui-tool-crontab-placeholder = { "* * * * *" }
ui-tool-crontab-next-runs = 5 prochaines exécutions
ui-tool-crontab-desc-every-minute = S'exécute chaque minute
ui-tool-crontab-desc-every-hour = S'exécute au début de chaque heure
ui-tool-crontab-desc-every-day = S'exécute tous les jours à minuit
ui-tool-crontab-desc-every-n-min = S'exécute toutes les { $interval } minutes
ui-tool-crontab-desc-daily-at = S'exécute tous les jours à { $time }
ui-tool-crontab-desc-weekly-at = S'exécute chaque { $day } à { $time }
ui-tool-crontab-desc-monthly-at = S'exécute le { $day } de chaque mois à { $time }
ui-tool-crontab-desc-custom = Planification personnalisée : { $expression }
ui-tool-crontab-error-field-count = Une expression cron doit comporter exactement 5 champs séparés par des espaces
ui-tool-crontab-error-invalid-field = Caractères invalides dans le champ "{ $field }" : { $value }
ui-tool-crontab-error-out-of-range = Les valeurs du champ "{ $field }" doivent être entre { $min } et { $max }
ui-tool-crontab-help =
    Constructeur Crontab

    Construit des expressions cron avec un aperçu lisible.

    Utilisation :
    Configurez les champs minute, heure, jour, mois et jour de la semaine. Les prochaines exécutions planifiées sont affichées.
ui-tool-sshconfig-name = Générateur de config SSH
ui-tool-sshconfig-description = Générateur de fichier de configuration SSH client
ui-tool-sshconfig-title = Générateur de config SSH
ui-tool-sshconfig-host-alias = Alias Host
ui-tool-sshconfig-host-name = HostName
ui-tool-sshconfig-user = User
ui-tool-sshconfig-port = Port
ui-tool-sshconfig-identity-file = IdentityFile
ui-tool-sshconfig-proxy-jump = ProxyJump
ui-tool-sshconfig-forward-agent = ForwardAgent
ui-tool-sshconfig-alive-interval = ServerAliveInterval
ui-tool-sshconfig-generate = Générer
ui-tool-sshconfig-generate-all = Générer tout depuis Heimdall
ui-tool-sshconfig-copy = Copier
ui-tool-sshconfig-error-host-required = Le HostName est requis.
ui-tool-sshconfig-generate-all-hint =
    Ouvrez cet outil depuis le contexte d'une session pour préremplir les champs.
    Utilisez le formulaire ci-dessus pour générer manuellement des blocs de configuration pour chaque hôte.
ui-tool-sshconfig-empty = Configurez les options ci-dessus pour générer un bloc de config SSH
ui-tool-sshconfig-help =
    Générateur de Config SSH

    Génère des fichiers de configuration client SSH.

    Utilisation :
    Configurez les entrées hôte avec nom, utilisateur, port, clé et proxy. Exportez au format ~/.ssh/config.
ui-tool-json-name = Formateur JSON
ui-tool-json-description = Formateur, validateur et minifieur JSON
ui-tool-json-title = Formateur JSON
ui-tool-json-input = JSON en entrée
ui-tool-json-output = Sortie
ui-tool-json-prettify = Embellir
ui-tool-json-minify = Minifier
ui-tool-json-copy = Copier la sortie
ui-tool-json-placeholder = coller le JSON ici
ui-tool-json-empty = Collez du JSON et appuyez sur Embellir ou Minifier.
ui-tool-json-processing = Traitement en cours...
ui-tool-json-status-prettified =
    { $count ->
        [one] Embelli ({ $count } caractère)
       *[other] Embelli ({ $count } caractères)
    }
ui-tool-json-status-minified =
    { $count ->
        [one] Minifié ({ $count } caractère)
       *[other] Minifié ({ $count } caractères)
    }
ui-tool-json-status-error = JSON invalide : { $error }
ui-tool-json-status-error-at = Erreur à la ligne { $line }, position { $column } : { $error }
ui-tool-json-too-large = L'entrée dépasse la limite de 5 Mo.
ui-tool-json-help =
    Formateur JSON

    Formate (embellit) ou minifie les données JSON.

    Utilisation :
    1. Collez ou tapez du JSON dans le champ de saisie
    2. Cliquez sur Embellir pour formater avec indentation
    3. Cliquez sur Minifier pour compacter en une seule ligne
    4. Copiez le résultat dans le presse-papiers

    Fonctionnalités :
    - Validation syntaxique avec messages d'erreur
    - Traite les gros documents JSON (jusqu'à 5 Mo)
    - Préserve les caractères Unicode

    Clavier :
    - Ctrl+Entrée : Embellir
    - Ctrl+Maj+Entrée : Minifier

    Exemples :
    - { "{" }"nom":"valeur"{ "}" } → Formaté avec indentation 2 espaces
    - Collez des réponses API pour un formatage rapide
ui-tool-regex-name = Testeur Regex
ui-tool-regex-description = Testeur d'expressions régulières avec mise en évidence des correspondances
ui-tool-regex-title = Testeur Regex
ui-tool-regex-pattern = Expression
ui-tool-regex-pattern-placeholder = expression régulière
ui-tool-regex-ignore-case = Ignorer la casse
ui-tool-regex-multiline = Multiligne
ui-tool-regex-singleline = Monoligne
ui-tool-regex-test-text = Texte de test
ui-tool-regex-test-placeholder = chaîne de test
ui-tool-regex-matches = Correspondances
ui-tool-regex-copy = Copier les correspondances
ui-tool-regex-count =
    { $count ->
        [one] { $count } correspondance
       *[other] { $count } correspondances
    }
ui-tool-regex-match-entry = { "[" }{ $number }] Index { $index } : "{ $value }"
ui-tool-regex-group-entry = { "  " }Groupe { $number } : "{ $value }"
ui-tool-regex-status-valid = Expression valide
ui-tool-regex-status-invalid = Expression invalide : { $error }
ui-tool-regex-status-timeout = Expiration de l'évaluation regex (protection ReDoS)
ui-tool-regex-unsupported-variable-lookbehind = Expression invalide : une assertion arrière de longueur variable n'est pas prise en charge par ce moteur
ui-tool-regex-unsupported-balancing-group = Expression invalide : les groupes d'équilibrage (?<ouvre-ferme>...) ne sont pas pris en charge par ce moteur
ui-tool-regex-truncated = Affichage des { $shown } premiers résultats sur { $total }
ui-tool-regex-empty = Saisissez une expression régulière et une chaîne de test ci-dessus
ui-tool-regex-help =
    Testeur d'Expressions Régulières

    Teste les expressions régulières sur du texte avec correspondance en temps réel.

    Utilisation :
    1. Entrez un pattern regex
    2. Entrez le texte de test
    3. Les correspondances sont surlignées et listées automatiquement

    Options :
    - Ignorer la casse : correspondance insensible à la casse
    - Multiligne : ^ et $ correspondent aux limites de ligne
    - Monoligne : . correspond aux retours à la ligne

    Fonctionnalités :
    - Surlignage en temps réel des correspondances
    - Liste numérotée avec détails des groupes de capture
    - Compteur de correspondances
    - Copier toutes les correspondances

    Exemples :
    - \b\w+@\w+\.\w+\b - Adresses email
    - \d{ "{" }1,3{ "}" }\.\d{ "{" }1,3{ "}" }\.\d{ "{" }1,3{ "}" }\.\d{ "{" }1,3{ "}" } - Adresses IPv4
    - ^#.*$ (Multiligne) - Lignes de commentaire

    Moteur :
    - Les assertions avant et arrière, les références arrière, les groupes atomiques et les conditionnelles sont pris en charge.
    - Non pris en charge : assertion arrière de longueur variable, groupes d'équilibrage.
    - Un test qui dure plus d'une seconde s'arrête (protection ReDoS).
ui-tool-diff-name = Comparaison de texte
ui-tool-diff-description = Comparateur de texte côte à côte avec visualisation des différences
ui-tool-diff-title = Comparaison de texte
ui-tool-diff-original = Original
ui-tool-diff-modified = Modifié
ui-tool-diff-original-placeholder = texte original
ui-tool-diff-modified-placeholder = texte modifié
ui-tool-diff-output = Résultat du diff
ui-tool-diff-compare = Comparer
ui-tool-diff-swap = Échanger
ui-tool-diff-clear = Effacer
ui-tool-diff-copy = Copier le diff
ui-tool-diff-ignore-whitespace = Ignorer les espaces
ui-tool-diff-ignore-case = Ignorer la casse
ui-tool-diff-auto-compare = Comparaison auto
ui-tool-diff-stats = +{ $added } ajouts, -{ $removed } suppressions, { $unchanged } inchangées
ui-tool-diff-status-done =
    { $count ->
        [one] Diff terminé : { $count } ligne
       *[other] Diff terminé : { $count } lignes
    }
ui-tool-diff-status-too-large = L'entrée dépasse { $max } lignes. Veuillez réduire la taille du texte.
ui-tool-diff-comparing = Comparaison...
ui-tool-diff-original-header = --- original
ui-tool-diff-modified-header = +++ modifié
ui-tool-diff-empty = Saisissez un texte original et modifié, puis comparez.
ui-tool-diff-help =
    Comparaison de Texte

    Compare deux textes côte à côte et surligne les différences.

    Utilisation :
    Collez du texte dans les deux panneaux. Les ajouts, suppressions et modifications sont codés par couleur.
ui-tool-textcase-name = Convertisseur de casse
ui-tool-textcase-description = Convertisseur de casse (majuscules, minuscules, camelCase, snake_case, kebab-case)
ui-tool-textcase-title = Convertisseur de casse
ui-tool-textcase-input = Texte d'entrée
ui-tool-textcase-placeholder = texte à convertir
ui-tool-textcase-conversions = Conversions
ui-tool-textcase-output = Sortie
ui-tool-textcase-copy = Copier
ui-tool-textcase-camel = camelCase
ui-tool-textcase-pascal = PascalCase
ui-tool-textcase-snake = snake_case
ui-tool-textcase-kebab = kebab-case
ui-tool-textcase-upper = MAJUSCULES
ui-tool-textcase-lower = minuscules
ui-tool-textcase-title-case = Casse De Titre
ui-tool-textcase-constant = CONSTANTE_CASE
ui-tool-textcase-empty = Saisissez du texte et choisissez une conversion.
ui-tool-textcase-help =
    Convertisseur de Casse

    Convertit le texte entre plusieurs formats de casse.

    Formats supportés : MAJUSCULES, minuscules, Casse Titre, camelCase, PascalCase, snake_case, kebab-case, CONSTANT_CASE, et plus.
ui-tunnels-page-title = Tunnels actifs
ui-import-dropped-rd-gateway = par une passerelle Bureau à distance
ui-error-rd-gateway = Ce serveur se joint par la passerelle Bureau à distance { $gateway }, que le client intégré ne sait pas encore traverser.
ui-error-credential-guard-required = Credential Guard est requis mais n'est pas actif sur ce système
ui-profile-resolution-auto = Auto (recommandé)
ui-profile-resolution-auto-desc = Heimdall choisit le meilleur mode selon le moniteur hôte et le mode plein écran ou fenêtré de la session.
ui-profile-resolution-multi-monitor = Multi-écran
ui-resolution-mode-auto = Auto
ui-resolution-mode-multi-monitor = Multi-écran
ui-profile-aspect-ratio = Rapport d'aspect
ui-profile-aspect-stretch = Étirer (remplir)
ui-profile-aspect-ratio-choice = { $wide }:{ $high }
ui-profile-rdp-session-mode = Mode de session
ui-profile-rdp-mode-embedded = Intégré (dans l'application)
ui-profile-rdp-mode-external = Externe (mstsc.exe)
ui-profile-rdp-mode-external-desc = Lancer le RDP dans une fenêtre mstsc.exe séparée. Connexion Bureau à distance demande elle-même le mot de passe.
ui-profile-field-rd-gateway = Serveur Passerelle RD
ui-profile-rd-gateway-placeholder = passerelle-rd.exemple.com (vide = connexion directe)
ui-profile-rd-gateway-hint = Passerelle Bureau à distance Microsoft utilisée pour joindre cet hôte via HTTPS. À ne pas confondre avec le rebond SSH configuré plus haut.
ui-profile-rd-gateway-mstsc = Avec une passerelle, ce serveur s'ouvre dans Connexion Bureau à distance (mstsc.exe) : le client intégré ne sait pas encore la traverser.
ui-profile-error-rd-gateway = La passerelle RD doit être un nom d'hôte ou une adresse IP valide.
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
ui-import-dropped-rdp-resize-delay = un délai de redimensionnement de { $ms } ms, hors plage (la valeur globale s'applique)
ui-import-dropped-citrix-cache-launch = lancement depuis le cache Citrix Workspace, non importé
ui-import-dropped-local-post-connect = { $count ->
    [one] séquence post-connexion de { $count } étape, qu'un shell local n'exécute jamais
   *[other] séquence post-connexion de { $count } étapes, qu'un shell local n'exécute jamais
}
ui-import-dropped-command-library-links = { $count ->
    [one] { $count } étape post-connexion liée à la bibliothèque de commandes, importée sans son lien
   *[other] { $count } étapes post-connexion liées à la bibliothèque de commandes, importées sans leur lien
}
ui-import-dropped-elevation = élévation "{ $mode }", désormais exécutée en administrateur dans sa propre fenêtre
ui-import-elevation-auto = Auto (gsudo, puis fenêtre externe en fallback)
ui-import-elevation-gsudo = gsudo uniquement (terminal intégré)
ui-import-elevation-runas = Fenêtre externe (compatible AdminByRequest)
ui-import-elevation-unknown = inconnue
ui-citrix-import-title = Applications Citrix
ui-citrix-import-none = Aucune application Citrix trouvée dans le cache local. Ouvrez Citrix Workspace et connectez-vous à un magasin.
ui-citrix-import-confirm = { $count ->
    [one] Importer { $count } application Citrix depuis le cache Workspace local ?
   *[other] Importer { $count } applications Citrix depuis le cache Workspace local ?
}
ui-citrix-import-done = { $count ->
    [one] { $count } application Citrix importée avec succès.
   *[other] { $count } applications Citrix importées avec succès.
}
ui-citrix-import-refreshed = { $count ->
    [one] { $count } application déjà enregistrée : sa ligne de lancement a été mise à jour.
   *[other] { $count } applications déjà enregistrées : leurs lignes de lancement ont été mises à jour.
}
ui-citrix-import-no-launch-lines = Le coffre est verrouillé ou indisponible : les lignes de lancement du cache Workspace n'ont pas été enregistrées. Ces applications se lancent par leur StoreFront.
ui-citrix-cache-folder-missing = Dossier du cache Citrix SelfService introuvable.
ui-citrix-cache-no-files = Aucun fichier de cache Citrix trouvé. Ouvrez Citrix Workspace et connectez-vous à un magasin.
ui-citrix-cache-unreadable = { $file } : { $detail }
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
ui-hostkey-algorithm = Algorithme : { $algorithm }
ui-tunnels-column-started = Démarré
ui-tunnels-manage-gateways = Gérer les passerelles dans les Paramètres...
ui-settings-powershell-policy = Politique d'exécution PowerShell
ui-settings-powershell-policy-hint = Appliqué au lancement des sessions PowerShell/pwsh locales
ui-settings-powershell-policy-default = Par défaut
ui-settings-ctrl-v = Ctrl+V dans un terminal
ui-settings-ctrl-v-always = Colle
ui-settings-ctrl-v-outside = Colle, sauf dans les programmes plein écran (vim, less...)
ui-settings-ctrl-v-never = Est envoyé à la session (Ctrl+Maj+V colle)
ui-settings-ctrl-k = Ctrl+K dans un terminal
ui-settings-ctrl-k-quick-connect = Ouvre la connexion rapide
ui-settings-ctrl-k-send = Est envoyé à la session (Ctrl+Maj+K ouvre la connexion rapide)
ui-connect-via = via { $route }

## Translations of text written in English first.
ui-desktop-anti-idle = Anti-inactivité
ui-desktop-anti-idle-tooltip = L'anti-inactivité garde cette session ouverte. Cliquez pour le désactiver pour cette session.
ui-desktop-stabilizing = Stabilisation de la session... { $seconds }s
ui-desktop-stabilizing-tooltip = Le redimensionnement automatique est suspendu le temps que la session se stabilise. Utilisez le menu Résolution, Ignorer la stabilisation, pour reprendre maintenant.
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
ui-dialog-paste-truncated = L'aperçu est tronqué. Le contenu complet du presse-papiers sera collé si vous continuez.
ui-dialog-paste-dangerous-title = Coller une commande dangereuse ?
ui-dialog-session-logging-body = Chaque session de terminal sera écrite dans un fichier : ce que vous tapez comme ce qui s'affiche, y compris les mots de passe ou jetons renvoyés par le terminal. L'activer ?
ui-dialog-session-logging-confirm = Activer
ui-dialog-session-logging-title = Enregistrer les transcriptions des sessions ?
ui-error-winrm-https-gateway = WinRM par une passerelle SSH ne prend pas en charge HTTPS. Utilisez HTTP, ou connectez-vous directement.
ui-error-winrm-tls-failed = La connexion TLS WinRM vers '{ $host }' sur le port { $port } a échoué (certificat non approuvé ou erreur de négociation).
ui-error-winrm-unreachable = L'hôte WinRM '{ $host }' est injoignable sur le port { $port } (connexion refusée ou délai dépassé).
ui-error-winrm-unresolved = Impossible de résoudre l'hôte WinRM '{ $host }'.
ui-files-back-tooltip = Retour
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
ui-files-home-tooltip = Aller au répertoire personnel
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
ui-settings-rdp-multi-monitor = Multi-écran
ui-settings-rdp-audio-capture = Capture audio (microphone)
ui-settings-rdp-redirect-printers = Rediriger les imprimantes
ui-settings-rdp-redirect-com-ports = Rediriger les ports COM
ui-settings-rdp-redirect-smart-cards = Rediriger les cartes à puce
ui-settings-rdp-redirect-webcam = Rediriger la webcam
ui-settings-rdp-redirect-usb = Rediriger les périphériques USB
ui-settings-rdp-bitmap-cache = Conserver le cache bitmap sur disque
ui-settings-rdp-compression = Compression
ui-settings-rdp-hardware-acceleration = Rendu accéléré par le matériel
ui-settings-rdp-strict-server-auth = Authentification serveur stricte
ui-settings-session-logging-record = Enregistrer les transcriptions des sessions (ce que chaque terminal affiche, saisie comprise)
ui-settings-session-logging-warning = Les transcriptions gardent ce que vous tapez comme ce qui s'affiche, y compris les mots de passe ou jetons renvoyés par le terminal. Gardez le dossier des journaux privé.
ui-settings-ssh-keep-alive-hint = Fréquence à laquelle Heimdall envoie les maintiens de connexion SSH sur les sessions, SFTP, tunnels et passerelles, pour qu'une connexion inactive ne soit pas coupée par un pare-feu ou le serveur. S'applique aux connexions ouvertes après le changement.
ui-settings-ssh-keep-alive-interval = Intervalle de maintien SSH
ui-settings-ssh-keep-alive-refused = L'intervalle de maintien SSH doit être compris entre { $min } et { $max } secondes.
ui-settings-ssh-session = Session
ui-settings-sftp = Navigateur SFTP
ui-settings-sftp-browser-enabled = Activer le navigateur SFTP intégré
ui-settings-sftp-auto-open = Ouvrir le panneau SFTP automatiquement lors d'une connexion SSH
ui-settings-sftp-follow = Le SFTP suit le répertoire courant SSH
ui-settings-dock-local-browser = Ancrer un explorateur de fichiers à côté des shells locaux
ui-settings-local-follow = L'explorateur local suit le répertoire courant du shell
ui-settings-ssh-tmout-reset-interval = Intervalle de réinitialisation TMOUT (0 = désactivé)
ui-settings-ssh-tmout-reset-refused = L'intervalle de réinitialisation TMOUT SSH doit être compris entre { $min } et { $max } secondes.
ui-status-link-not-a-folder = { $name } ne pointe pas vers un dossier.
ui-status-winrm-certificate-skipped = La validation du certificat TLS WinRM a été ignorée pour cette session.
ui-status-ftp-cleartext = La session FTP vers { $host }:{ $port } utilise un canal en clair. Les identifiants et le contenu des fichiers sont transmis sans chiffrement. Préférez SFTP ou FTPS lorsque c'est possible.
ui-status-citrix-launching = Lancement de la session Citrix...
ui-status-citrix-launched = Session Citrix lancée : { $name }
ui-status-citrix-workspace-not-found = Citrix Workspace introuvable. Installez Citrix Workspace App.
ui-status-citrix-invalid-storefront = URL Citrix StoreFront invalide. Utilisez une URL HTTP ou HTTPS absolue.
ui-status-citrix-storefront-credentials = L'URL Citrix StoreFront ne peut pas contenir d'identifiants.
ui-status-citrix-invalid-ica-file = Le fichier ICA doit être un fichier .ica de cet ordinateur, pas d'un partage réseau.
ui-status-citrix-not-configured = Aucune URL StoreFront Citrix ou fichier ICA configuré.
ui-status-citrix-launch-failed = Échec du lancement de la session Citrix.
ui-status-citrix-not-started = Échec du lancement de la session Citrix : { $reason }
ui-status-citrix-command-rejected = La commande de lancement Citrix contient des caractères interdits (|, &, ;, `, $, retours à la ligne).
ui-status-citrix-vault-locked = Déverrouillez le coffre avant de lancer cette session Citrix.
ui-citrix-tab-storefront = StoreFront : { $url }
ui-citrix-tab-application = Application : { $name }
ui-citrix-tab-mode-cache = Mode : cache SelfService
ui-citrix-tab-mode-ica-file = Mode : fichier ICA
ui-citrix-tab-mode-storefront = Mode : StoreFront
ui-citrix-tab-launched-at = Lancement : { $time }
ui-citrix-tab-launcher-exit = Code de sortie du lanceur : { $code }
ui-citrix-tab-launching = Lancement de la session Citrix...
ui-citrix-tab-running = Connecté. PID du client Citrix : { $pid }
ui-citrix-tab-not-found = Client Citrix pas encore trouvé. La session est peut-être encore en cours de démarrage.
ui-citrix-tab-ended = Déconnecté. Le client Citrix (PID : { $pid }) s'est terminé.
ui-citrix-tab-shared = La session utilise un client Citrix déjà en cours d'exécution : impossible de la distinguer des autres sessions.
ui-citrix-tab-launcher-failed = Le lanceur Citrix a échoué avec le code de sortie { $code }.
ui-citrix-tab-windows-only = Le suivi du client Citrix n'est disponible que sous Windows.
ui-citrix-tab-unavailable = Impossible de lister les clients Citrix en cours d'exécution : le client de cette session n'est pas suivi.
ui-citrix-tab-timed-out = Les clients Citrix en cours d'exécution n'ont pas été listés à temps, plusieurs fois de suite : le client de cette session n'est plus suivi.
ui-citrix-tab-terminate = Terminer
ui-citrix-tab-force-terminate = Forcer la fin
ui-citrix-terminate-title = Terminer la session Citrix ?
ui-citrix-terminate-body = Le travail non enregistré dans l'application distante sera perdu. Confirmer la fin de session ?
ui-citrix-force-terminate-title = Forcer la fin de la session Citrix ?
ui-citrix-force-terminate-body = Le client Citrix ne s'est pas fermé à la demande. Le forcer l'arrête aussitôt, avec toutes les sessions qu'il exécute : le travail non enregistré dans l'application distante sera perdu. Forcer la fin de session ?
ui-citrix-tab-terminating = Demande de fermeture au client Citrix...
ui-citrix-tab-force-terminating = Fermeture forcée du client Citrix...
ui-citrix-tab-terminate-asked = Le client Citrix a été invité à se fermer.
ui-citrix-tab-terminate-forced = Le client Citrix a été forcé à se fermer.
ui-citrix-tab-terminate-still-running = Le client Citrix est toujours en cours d'exécution après la demande de fermeture.
ui-citrix-tab-terminate-refused = La demande de fin du client Citrix a échoué : taskkill s'est terminé avec le code de sortie { $code }.
ui-citrix-tab-terminate-timed-out = La demande de fin du client Citrix a échoué : taskkill n'a pas répondu à temps.
ui-citrix-tab-terminate-not-run = La demande de fin du client Citrix a échoué : taskkill n'a pas pu être démarré.
ui-status-rdp-external-launched = Client externe lancé : { $name } s'est ouvert dans Connexion Bureau à distance.
ui-status-rdp-external-launched-gateway = Client externe lancé : { $name } passe par la passerelle Bureau à distance { $gateway }, que le client intégré ne sait pas encore traverser : il s'est ouvert dans Connexion Bureau à distance.
ui-status-rdp-external-not-windows = Ce profil s'ouvre dans Connexion Bureau à distance (mstsc.exe), que seul Windows possède : il ne peut pas être ouvert sur ce système.
ui-status-rdp-external-ssh-gateway = Connexion Bureau à distance (mstsc.exe) ne sait pas passer par la passerelle SSH de ce profil : il n'a pas été ouvert.
ui-status-rdp-external-not-found = mstsc.exe est introuvable sur cet ordinateur.
ui-status-rdp-external-not-written = Échec de l'écriture du fichier de connexion Bureau à distance : { $reason }
ui-status-rdp-external-not-started = mstsc.exe n'a pas démarré : { $reason }
ui-status-putty-launched = Client externe lancé : { $name } s'est ouvert dans PuTTY.
ui-status-putty-launched-through = Client externe lancé : { $name } s'est ouvert dans PuTTY par la passerelle SSH { $gateway }.
ui-status-putty-invalid-host = Hôte cible invalide (rejeté par la validation des entrées).
ui-status-putty-invalid-username = Nom d'utilisateur SSH invalide (rejeté par la validation des entrées).
ui-status-putty-key-file = Le fichier de clé { $path } est introuvable ou n'est pas désigné par un chemin absolu : PuTTY n'a pas été ouvert.
ui-status-putty-host-key = PuTTY n'a pas été ouvert : { $reason }
ui-status-putty-gateway = PuTTY n'a pas été ouvert, sa passerelle SSH n'a pas été atteinte : { $reason }
ui-status-putty-forward = PuTTY n'a pas été ouvert, aucun port local n'a pu être ouvert pour sa passerelle SSH : { $reason }
ui-status-putty-not-found = PuTTY introuvable. Configurez le chemin dans les paramètres ou installez PuTTY dans un dossier du PATH.
ui-status-putty-not-started = PuTTY n'a pas démarré : { $reason }
ui-status-x11-server-not-found = Aucun serveur X11 trouvé. Installez VcXsrv ou Xming pour le transfert X11.
ui-status-winrm-gateway-ntlm = WinRM par une passerelle : Kerberos n'est pas disponible, l'authentification passe par NTLM.
ui-winrm-diagnostic-ntlm-loopback = L'authentification WinRM a échoué pour l'identité Windows actuelle. Utilisez un compte enregistré pour localhost ou les hôtes hors domaine.
ui-winrm-diagnostic-wsman-invalid = WinRM a reçu une réponse WSMan invalide. Si cette session passe par une passerelle, vérifiez que WinRM utilise HTTP dans le tunnel.
ui-tunnels-session-routes = Sessions via une passerelle ({ $count })
ui-tunnels-session-route-local = -
ui-tunnels-session-route-local-tooltip = Acheminé dans Heimdall : aucun port local
ui-tunnels-close-all-tooltip = Ferme les tunnels ouverts à la main ; les sessions via une passerelle restent ouvertes
ui-tab-route-badge = via
ui-origin-rdp-file = Importé depuis un fichier RDP
ui-origin-openssh = Importé depuis la configuration OpenSSH
ui-origin-putty = Importé depuis le registre PuTTY
ui-origin-mremoteng = Importé depuis mRemoteNG
ui-origin-mobaxterm = Importé depuis MobaXterm
ui-origin-rdcman = Importé depuis RDCMan
ui-desktop-shortcuts = Raccourcis clavier...
ui-desktop-shares-clipboard = Presse-papiers
ui-desktop-shares-clipboard-tooltip = Redirection du presse-papiers
ui-desktop-shares-drives = Disques
ui-desktop-shares-drives-tooltip = Redirection des disques
ui-desktop-shares-audio = Son
ui-desktop-shares-audio-tooltip = Redirection audio
ui-settings-rdp-connect-timeout = Délai du watchdog de connexion RDP (0 = désactivé)
ui-settings-rdp-resize-delay = Délai de stabilisation de la résolution après connexion (0 = désactivé)
ui-settings-rdp-resize-delay-refused = Le délai de redimensionnement RDP doit être nul ou compris entre { $min } et { $max } ms.
ui-settings-rdp-connect-timeout-off = Désactivé
ui-settings-rdp-connect-timeout-seconds = { $seconds } s
ui-shortcuts-release-desktop = Rendre le clavier depuis un bureau distant
ui-profile-toggle-strict-server-auth = Exiger la validation de l'identité serveur
ui-error-rdp-server-not-authenticated = L'identité du serveur n'a pas pu être validée : son certificat n'est pas encore approuvé et les autorités de certification de cet ordinateur ne le garantissent pas. L'authentification stricte du serveur le refuse.
ui-certificate-subject = Sujet : { $subject }
ui-certificate-issuer = Émetteur : { $issuer }
ui-certificate-validity = Valide du / au : { $from } - { $until }{ $period ->
    [expired] {" "}(expiré)
    [future] {" "}(pas encore valide)
   *[current] {""}
}
ui-certificate-validation-issue = Problème de validation : { $issue }
ui-certificate-renewed = Certificat renouvelé : même clé, nouveau certificat. Le serveur présente un autre certificat sur la clé que vous avez approuvée. Un renouvellement est courant, mais qui détient la clé a pu aussi le fabriquer : approuvez-le seulement si vous attendez ce renouvellement.
ui-certificate-renewed-previous = Certificat enregistré valide du / au : { $from } - { $until }
ui-certificate-issue-self-signed = Le certificat est auto-signé : aucune autorité de certification ne le garantit.
ui-certificate-issue-unknown-issuer = Il a été émis par une autorité de certification que cet ordinateur n'approuve pas.
ui-certificate-issue-expired = Le certificat a expiré.
ui-certificate-issue-not-yet-valid = Le certificat n'est pas encore valide.
ui-certificate-issue-name-mismatch = Le certificat a été émis pour un autre nom que celui de ce serveur.
ui-certificate-issue-wrong-purpose = Le certificat n'est pas destiné à un serveur : son usage de clé en nomme d'autres.
ui-certificate-issue-revoked = Son émetteur a révoqué le certificat.
ui-certificate-issue-no-system-store = Cet ordinateur n'a aucune autorité de certification pour le vérifier.
ui-certificate-issue-other = Le certificat n'a pas passé la validation de cet ordinateur.
ui-session-copy-anonymous-button = Copier le rapport anonymisé
ui-error-report-anonymous-header = Rapport de diagnostic { $protocol } (anonymisé)
ui-error-report-kind = Échec :
ui-error-report-anonymous-hint = Contient la date, le nombre de passerelles, la durée de connexion, la version et le type d'échec. Exclut les adresses, les noms de passerelles, les comptes et le texte des erreurs.
ui-tree-changed-move = Sessions déplacées.
ui-tree-changed-reorder = Sessions réordonnées.
ui-tree-changed-rename = Session renommée.
ui-tree-changed-folder-move = Dossier déplacé.
ui-tree-changed-folder-rename = Dossier renommé.
ui-tree-undo = Annuler
ui-status-reordered-one = { $name } déplacé dans { $folder }
ui-status-reordered = { $count } sessions déplacées dans { $folder }
ui-status-undo-conflict = Annulation impossible : les sessions ou dossiers concernés ont changé depuis cette action.
ui-tree-no-folder-zone = Déposer ici pour le sortir de son dossier
ui-tree-no-folder-zone-tooltip = Déposez une session ou un dossier ici pour le sortir de son dossier.
ui-tree-filter-chip-remove = ✕
ui-tree-filter-chip-tooltip = Retirer le filtre : { $filter }
ui-tree-filter-result-count = { $shown } / { $total ->
    [one] { $total } session
   *[other] { $total } sessions
}
ui-tree-selection-count = { $count } sessions sélectionnées
ui-tree-selection-move = Déplacer
ui-tree-selection-more = Autres actions
