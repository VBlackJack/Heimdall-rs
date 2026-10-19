# Copyright 2026 Julien Bombled
# Licensed under the Apache License, Version 2.0. See LICENSE.

ui-window-title = Heimdall
ui-window-title-tab = { $tab } - Heimdall
ui-window-title-detached = { $tab } - Détaché
# A test build's title, its test data folder after it.
ui-window-title-test-data = { $title } [données de test : { $folder }]
# An alternate instance's title, its data folder (--data-dir) after it.
ui-window-title-data = { $title } [données : { $folder }]
# Why a --data-dir folder did not start the application, in a message box and on the console.
ui-data-dir-error-title = Heimdall ne peut pas démarrer
ui-data-dir-error-no-value = { $option } doit être suivi du chemin complet d'un dossier.
ui-data-dir-error-not-text = Le dossier après { $option } n'est pas un texte valide : { $folder }
ui-data-dir-error-unknown = Option inconnue { $given } : le dossier de données se donne avec { $option } <chemin complet>.
ui-data-dir-error-not-absolute = Le dossier de données doit être un chemin complet : { $folder }
ui-data-dir-error-overlaps = Le dossier de données { $folder } recoupe l'un de vos propres dossiers : { $real }. Choisissez un dossier distinct.
ui-data-dir-error-unusable = Le dossier de données ne peut pas être utilisé : { $detail }
ui-data-dir-error-already-set = Un dossier de données est déjà utilisé.
ui-data-dir-error-test-build = Une version de test prend ses dossiers dans son dossier de données de test, pas dans { $option }.

ui-sidebar-title = Sessions
ui-sidebar-tools-tab = Outils
ui-sidebar-empty = Aucun profil enregistré.
ui-desktop-send-keys = Envoyer des touches
ui-desktop-send-keys-tooltip = Envoyer des touches à la session distante
ui-vnc-clipboard = Presse-papiers
ui-vnc-clipboard-tooltip = Synchroniser le presse-papiers
ui-desktop-match-window = Résolution de la fenêtre
ui-desktop-fit-window = Adapter à la fenêtre
ui-desktop-fullscreen-tooltip = Basculer en plein écran (F11)
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
ui-status-scheduled-triggered = Tâche planifiée déclenchée. Connexion à { $name }...
ui-status-scheduled-failed = Erreur de tâche planifiée : { $reason }
ui-status-scheduled-refused = Erreur de tâche planifiée : une planification n'ouvre pas "{ $name }" : { $reason }
ui-status-scheduled-suspended = La tâche planifiée de { $name } a été désactivée après { $count } échecs consécutifs
ui-nav-scheduled = Planifié
ui-scheduled-title = Connexions planifiées
ui-scheduled-add = + Ajouter
ui-scheduled-edit = Modifier
ui-scheduled-delete = Supprimer
ui-scheduled-empty = Aucune tâche planifiée configurée.
ui-scheduled-empty-add = Créer une tâche
ui-scheduled-column-enabled = Act.
ui-scheduled-column-server = Session
ui-scheduled-column-type = Type
ui-scheduled-column-schedule = Planification
ui-scheduled-column-last-run = Dernière exécution
ui-scheduled-column-next-run = Prochaine exécution
ui-scheduled-column-outcome = Dernier résultat
ui-scheduled-schedule-daily = Quotidien { $time }
ui-scheduled-schedule-interval = Toutes les { $minutes } min
ui-scheduled-dialog-add-title = Ajouter une tâche planifiée
ui-scheduled-dialog-edit-title = Modifier la tâche planifiée
ui-scheduled-field-server = Session :
ui-scheduled-field-server-placeholder = Sélectionnez la session à connecter
ui-scheduled-field-kind = Type de planification :
ui-scheduled-kind-daily = Quotidien
ui-scheduled-kind-interval = Intervalle
ui-scheduled-field-time = Heure :
ui-scheduled-field-time-hint = Format 24h, ex. 08:00
ui-scheduled-field-interval = Intervalle (minutes) :
ui-scheduled-field-interval-suffix = min
ui-scheduled-field-enabled = Activée
ui-scheduled-next-run-daily = Prochaine exécution : { $day } à { $time }
ui-scheduled-next-run-today = aujourd'hui
ui-scheduled-next-run-tomorrow = demain
ui-scheduled-next-run-interval = { $minutes ->
    [one] Prochaine exécution : dans { $minutes } minute (toutes les { $minutes } min)
   *[other] Prochaine exécution : dans { $minutes } minutes (toutes les { $minutes } min)
}
ui-scheduled-problem-server = Veuillez sélectionner une session.
ui-scheduled-problem-time = Format d'heure invalide. Utilisez HH:mm (24h).
ui-scheduled-problem-interval = L'intervalle doit être un nombre positif de minutes.
ui-scheduled-save = Enregistrer
ui-scheduled-delete-title = Supprimer la tâche planifiée
ui-scheduled-delete-body = Êtes-vous sûr de vouloir supprimer la tâche planifiée "{ $name }" ?
ui-scheduled-discard-title = Modifications non enregistrées
ui-scheduled-discard-body = Vous avez des modifications non enregistrées. Les abandonner et fermer ?
ui-scheduled-discard-keep = Continuer la modification
ui-scheduled-discard-confirm = Abandonner
ui-scheduled-outcome-ran = Connectée
ui-scheduled-outcome-missed = Manquée : Heimdall était fermé
ui-scheduled-outcome-deferred = En attente du déverrouillage
ui-scheduled-outcome-suspended = Désactivée après { $count } échecs : { $reason }
ui-status-scheduled-added = Tâche planifiée "{ $name }" créée.
ui-status-scheduled-updated = Tâche planifiée "{ $name }" mise à jour.
ui-status-scheduled-deleted = Tâche planifiée "{ $name }" supprimée.
ui-scheduled-refusal-local-program = c'est un shell ou un programme local
ui-scheduled-refusal-external-client = elle s'ouvre dans un client externe (PuTTY ou Connexion Bureau à distance)
ui-scheduled-refusal-rd-gateway = elle passe par une passerelle Bureau à distance, qui s'ouvre dans Connexion Bureau à distance
ui-scheduled-refusal-citrix = c'est une application Citrix, lancée par le client Citrix Workspace
ui-scheduled-refusal-winrm-no-account = elle se connecte avec le compte Windows qui exécute Heimdall, ou ne nomme aucun compte
ui-scheduled-refusal-winrm-password = son mot de passe n'est pas enregistré
ui-scheduled-refusal-x11 = elle transfère X11, ce qui démarrerait le serveur X local
ui-scheduled-refusal-steps = ses étapes post-connexion ne sont pas approuvées
ui-scheduled-refusal-files-browser = le navigateur de fichiers est désactivé
ui-status-scheduled-deferred = La connexion planifiée à { $name } attend le déverrouillage de l'espace de travail
ui-scheduled-failure-not-found = Serveur "{ $name }" introuvable
ui-scheduled-failure-name-ambiguous = plusieurs sessions s'appellent "{ $name }" : modifiez la tâche pour en choisir une
ui-scheduled-failure-credential-unavailable = aucun identifiant enregistré pour "{ $name }"
ui-scheduled-failure-verification-required = "{ $name }" exigerait une demande de Windows Hello ou de Credential Guard
ui-scheduled-failure-confirmation-required = la clé ou le certificat de "{ $name }" n'est pas encore approuvé
ui-scheduled-failure-session-limit = la limite de sessions est atteinte
ui-scheduled-failure-connection-failed = la connexion à "{ $name }" a échoué
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
ui-certificate-key-fingerprint = Clé publique SHA-256 : { $fingerprint }
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
ui-dialog-scan-added-title = Sessions ajoutées
ui-dialog-import-counts = Ajoutés : { $added }. Mis à jour : { $updated }. Inchangés : { $unchanged }.
ui-dialog-import-actions = { $imported ->
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
}.
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
ui-import-mobaxterm-unresolved-variable = Session "{ $session }" : la variable [{ $name }] n'a pas de valeur, son identifiant a donc été conservé tel quel. Définissez-la dans Paramètres > Variables MobaXterm puis importez à nouveau.
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
ui-dialog-config-recovered-title = Configuration restaurée
ui-dialog-config-recovered-body = { $documents } était illisible et a été chargé depuis sa dernière copie valide. Les changements enregistrés après cette copie peuvent manquer.
ui-dialog-config-recovered-separator = {", "}
ui-dialog-detail = Détail : { $detail }

ui-import-skip-not-ssh = pas un profil SSH ({ $kind })
ui-import-skip-missing-host = n'a pas d'hôte
ui-import-skip-missing-id = n'a pas d'identifiant
ui-import-skip-invalid-port = port invalide { $port }
ui-import-skip-invalid-local-port = le port du tunnel local { $port } doit être compris entre 1 et 65535

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
ui-files-upload-files-tooltip = Envoyer des fichiers
ui-files-download-selected-tooltip = Télécharger la sélection (Ctrl+Maj+D)
ui-files-loading = Chargement...
ui-files-listing-in-progress-tooltip = Chargement du dossier en cours : attendez qu'il s'affiche
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
ui-files-error-local-disk-full = Le disque local est plein. Libérez de l'espace puis réessayez.
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
ui-dialog-send-command-title = Envoyer cette commande ?
ui-dialog-send-command-body = La commande ci-dessous est saisie dans { $name } et exécutée avec Entrée, avec les droits de cette session. Heimdall redemande à chaque envoi.
ui-dialog-send-command-dangerous = Cette commande est marquée dangereuse : elle peut détruire des données ou arrêter un service.
ui-dialog-send-command-confirm = Envoyer
ui-dialog-broadcast-command-title = Envoyer cette commande à plusieurs sessions ?
ui-dialog-broadcast-command-body = La commande ci-dessous est saisie dans chaque session listée ({ $count }) et exécutée avec Entrée, avec les droits de chaque session. Heimdall demande à chaque fois.
ui-dialog-broadcast-command-confirm = Envoyer à toutes
ui-error-remote-forward = La passerelle SSH n'a pas voulu écouter sur son port { $port } pour la redirection distante : la redirection y est désactivée, ou le port y est déjà pris.
ui-error-proxy-port = Le proxy SOCKS n'a pas pu ouvrir le port local { $port } : un autre programme l'utilise peut-être. ({ $detail })
ui-error-jump-refused = La passerelle SSH n'a pas voulu se connecter à { $target } : la redirection y est désactivée, ou cet hôte n'est pas joignable depuis elle.
ui-import-skip-missing-username = se connecte avec un compte qu'il ne nomme pas
ui-import-skip-unknown-identity = se connecte avec un mode d'identité que Heimdall ne connaît pas
ui-error-hostkey-changed-at = La clé d'hôte de { $target } n'est pas celle enregistrée : la connexion est peut-être interceptée. Enregistrée : { $recorded }. Présentée : { $offered }.
ui-error-gateway-missing = La passerelle SSH { $id } par laquelle passe ce profil n'est pas dans les profils.
ui-error-gateway-loop = La passerelle SSH { $id } est atteinte par elle-même, via ses parents.
ui-error-gateway-too-deep = Profondeur maximale de la chaîne de passerelles dépassée. Réduisez le nombre de passerelles chaînées.
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
ui-tree-add-tool = Ajouter un outil
ui-tree-tool-open = Ouvrir dans un onglet
ui-tree-tool-remove = Retirer l'outil
ui-tool-picker-title = Ajouter un outil
ui-tool-picker-prompt = Choisissez l'outil à enregistrer dans l'arborescence.
ui-tool-picker-search = Rechercher un outil
ui-tool-picker-host = Hôte
ui-tool-picker-confirm = Ajouter
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
ui-profile-import-title = Importer des profils
ui-profile-import-subtitle = { $count ->
    [one] { $count } profil prêt à l'import.
   *[other] { $count } profils prêts à l'import.
}
ui-profile-import-summary = { $chosen ->
    [one] { $chosen } profil sélectionné sur { $total }
   *[other] { $chosen } profils sélectionnés sur { $total }
}, { $conflicts ->
    [one] { $conflicts } conflit
   *[other] { $conflicts } conflits
}.
ui-profile-import-button = Importer la sélection
ui-profile-import-source = { $file }#{ $position }
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
ui-profile-protocol-web-name = Web
ui-profile-protocol-web-desc = Console web (iLO, iDRAC, switch)
ui-profile-port-web = Port web
ui-profile-web-address-title = Adresse web
ui-profile-web-address-desc = L'adresse est construite à partir de l'hôte et du port ci-dessus, plus ces réglages.
ui-profile-web-use-http = Utiliser HTTP simple (non chiffré)
ui-profile-web-http-warning = HTTP simple transmet tout en clair, mot de passe de la console compris.
ui-profile-field-web-path = Chemin (facultatif)
ui-profile-web-path-placeholder = /login.html
ui-profile-web-address-incomplete = Saisissez un nom d'hôte ou une adresse pour voir l'adresse.
ui-profile-credentials-web = Connexion HTTP
ui-profile-credentials-web-desc = Le compte est gardé avec le profil. Le navigateur demande lui-même son mot de passe : Heimdall ne le met jamais dans l'adresse.
ui-profile-web-opening-title = Ouverture
ui-profile-web-opening-desc = La console s'ouvre dans le navigateur par défaut de cet ordinateur, jamais dans Heimdall.
ui-profile-web-password-hint = Heimdall ne garde aucun mot de passe pour une console web : connectez-vous dans le navigateur.
ui-profile-web-gateway-hint = Par une passerelle SSH, le navigateur ouvre un tunnel de cet ordinateur (une adresse à elle en 127.x.x.x, sur un port choisi par le système) au lieu du nom de la console : attendez-vous à un avertissement sur le nom du certificat, et une console qui redirige vers son propre nom quitte le tunnel. Le tunnel reste ouvert dans la page Tunnels jusqu'à ce que vous l'y fermiez ou quittiez Heimdall.
ui-profile-path-session-web = Console web dans le navigateur
ui-profile-path-client-web-tunnel = Tunnel local sur une adresse à lui en 127.x.x.x, sur un port choisi par le système
ui-status-web-opened = Console web { $name } ouverte dans le navigateur.
ui-status-tool-unknown = L'outil { $tool } n'est pas disponible dans cette version.
ui-status-web-opened-through = Console web { $name } ouverte dans le navigateur par le tunnel de la passerelle SSH sur le port { $port }. Le navigateur nomme le tunnel, pas la console : un avertissement sur le nom du certificat est attendu.
ui-status-web-invalid-address = L'hôte, le port et le chemin de ce profil ne forment pas une adresse web. Modifiez le profil : le champ hôte prend seulement un nom ou une adresse, sans http:// ni chemin.
ui-status-web-not-loopback = Le tunnel n'écoutait pas sur l'adresse de bouclage de cet ordinateur : le navigateur n'a pas été ouvert.
ui-status-web-not-started = Le navigateur n'a pas pu être démarré : { $reason }
ui-import-dropped-web-external-program = son propre programme de navigation, désormais le navigateur par défaut du système
ui-scheduled-refusal-web = c'est une console web, ouverte dans le navigateur du système
ui-scheduled-refusal-tool = c'est un outil, qui s'ouvre dans un onglet : il n'y a aucune connexion à planifier
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
ui-profile-audio-local-unavailable = Lecture locale non disponible dans cette compilation : le client intégré ne joue aucun son.
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
ui-profile-tunnel-port-title = Port local du tunnel
ui-profile-tunnel-port-desc = Heimdall utilise ce port local pour atteindre le service distant via le tunnel SSH.
ui-profile-tunnel-port-auto-toggle = Choisir le port du tunnel automatiquement
ui-profile-tunnel-port-manual = Port local manuel
ui-profile-tunnel-port-auto = Auto ({ $port })
ui-profile-tunnel-port-ssh-hint = Utilisé quand le shell s'ouvre dans PuTTY. Un shell ou des fichiers dans un onglet n'ouvrent aucun port : la connexion reste dans Heimdall.
ui-profile-tunnel-port-rdp-hint = Utilisé quand le bureau s'ouvre dans la Connexion Bureau à distance. Un bureau dans un onglet n'ouvre aucun port : la connexion reste dans Heimdall.
ui-profile-tunnel-port-busy-hint = Si le port est déjà pris au démarrage de la session, le système en choisit un autre.
ui-profile-error-tunnel-port-missing = Entrez un port de tunnel local ou repassez en mode Auto.
ui-profile-error-tunnel-port = Le port du tunnel local doit être compris entre 1 et 65535.
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
ui-gateway-field-host-key-fingerprint = Empreinte de la clé d'hôte
ui-gateway-host-key-fingerprint-hint = Renseigné automatiquement après la première connexion. Utilisé pour vérifier l'identité du serveur et prévenir l'usurpation de clé d'hôte.
ui-tree-gateway-via = via { $name }
ui-tree-gateway-missing = passerelle manquante

ui-tab-menu-disconnect = Déconnecter
ui-tab-menu-rename = Renommer l'onglet
ui-tab-menu-reset-title = Réinitialiser le titre
ui-tab-menu-fullscreen = Plein écran
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
ui-split-palette-hint = Rechercher un serveur ou choisir une session active...
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
ui-rdp-severity-notice = Information :
ui-rdp-reason-socket-connect-failed = Impossible de se connecter à l'ordinateur distant. Il est peut-être éteint, hors réseau, ou le Bureau à distance est désactivé.
ui-rdp-reason-dns-lookup-failed = Impossible de résoudre le nom de l'ordinateur distant. Vérifiez la saisie du nom et vos paramètres DNS ou votre connexion VPN.
ui-rdp-reason-connection-timeout = Le délai de connexion a expiré. Vérifiez que l'hôte distant est joignable et qu'aucun pare-feu ne bloque le port Bureau à distance (3389 par défaut).
ui-rdp-reason-socket-closed = La connexion réseau a été perdue. Vérifiez votre connexion réseau ou VPN et réessayez.
ui-rdp-reason-connect-timeout = Le délai de connexion a expiré pendant le démarrage de la session. L'ordinateur distant n'a pas répondu. Vérifiez l'hôte et le réseau, puis réessayez.
ui-rdp-reason-nla-not-supported = L'ordinateur distant exige l'authentification au niveau du réseau (NLA), que cette connexion n'utilise pas. Activez NLA dans le profil du serveur, ou demandez à votre administrateur si l'ordinateur distant peut accepter des connexions sans NLA.
ui-rdp-reason-gateway-target-unreachable = La passerelle SSH n'a pas pu joindre { $target }. L'hôte cible ou son port RDP est injoignable depuis la passerelle.
ui-failure-details-header = Détails du diagnostic
ui-failure-details-stage = Étape
ui-failure-details-code = Code
ui-failure-details-detail = Détail
ui-failure-details-line = { $label } : { $value }
ui-rdp-stage-tunnel = Tunnel RDP
ui-rdp-stage-connection = Connexion RDP
ui-rdp-stage-negotiation = Négociation de sécurité
ui-rdp-stage-certificate = TLS et certificat du serveur
ui-rdp-stage-authentication = Authentification au niveau du réseau
ui-rdp-stage-disconnect = Déconnexion RDP
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

ui-palette-placeholder = Rechercher un hôte, une IP ou un outil... (Ctrl+K)
ui-palette-ssh-to = [SSH] Connexion vers { $target }
ui-palette-rdp-to = [RDP] Connexion vers { $target }
ui-palette-quick-connect = Connexion rapide
ui-palette-nothing = Aucun résultat. Essayez un hôte, ssh user@host ou tools.
ui-palette-mode-split = Mode division
ui-palette-hints = Entrée = ouvrir · Ctrl+Entrée = scinder · tools = tous les outils · Échap = fermer
ui-palette-servers = Serveurs
ui-palette-active-sessions = Sessions actives
ui-palette-recent-tools = Outils récents
ui-palette-tools = Outils
ui-palette-merge-session = ↔ { $title }
ui-palette-merge-detail = Session active - fusionner sans reconnecter
ui-palette-account = · { $account }
ui-palette-tool-badge = TOOL

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
ui-files-item-count = { $count ->
    [one] { $count } élément
   *[other] { $count } éléments
}
ui-files-item-count-filtered = { $count ->
    [one] { $shown }/{ $count } élément
   *[other] { $shown }/{ $count } éléments
}
ui-files-hidden-unsafe = { $count ->
    [one] { $count } élément masqué : son nom ne peut pas être utilisé ici sans risque
   *[other] { $count } éléments masqués : leurs noms ne peuvent pas être utilisés ici sans risque
}
ui-files-hidden-unsafe-terminal = { $count ->
    [one] { $count } élément masqué : son nom ne peut pas être utilisé ici sans risque ; passez par le terminal
   *[other] { $count } éléments masqués : leurs noms ne peuvent pas être utilisés ici sans risque ; passez par le terminal
}
ui-files-unread-lines = { $count ->
    [one] { $count } ligne de la liste n'a pas pu être lue
   *[other] { $count } lignes de la liste n'ont pas pu être lues
}

## Files dragged from Explorer over a Files tab, as the C# `DescribeExternalDrop`.
ui-files-drop-items = { $count ->
    [one] { $count } élément
   *[other] { $count } éléments
}
ui-files-drop-overlay-target = Envoyer { $items } vers { $folder }
ui-files-drop-overlay-queued = Ajouter { $items } à la file d'attente vers { $folder }

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
ui-tunnels-close-selected = Fermer la sélection
ui-tunnels-close-selected-tooltip = Fermer le tunnel sélectionné
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
ui-route-test-auth = Échec d'authentification. Vérifiez le compte, la clé, la phrase de passe et l'agent SSH.
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
ui-resolution-mode-fit-window = Suivre la fenêtre
ui-resolution-mode-fixed = Fixe
ui-resolution-match-window = Suivre la fenêtre
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
ui-agent-chip-ok = { $count ->
    [one] Agent SSH : { $agent } ({ $count } clé)
   *[other] Agent SSH : { $agent } ({ $count } clés)
}
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
ui-status-rdp-compression-off = Le serveur a envoyé des données compressées que ce client n'a pas pu lire : la compression est désactivée pour cette connexion.
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
ui-status-drag-out-several-folders = Non glissés hors de la fenêtre : les fichiers ne sont pas tous dans le même dossier.
ui-status-drag-out-unavailable = Les fichiers ne peuvent pas être glissés hors de cette fenêtre sur ce système.
ui-status-drag-out-failed = Non glissés hors de la fenêtre : { $reason }
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
ui-dialog-close-tool-body = "{ $name }" a des modifications non enregistrées. Les enregistrer avant de fermer l'onglet ?
ui-dialog-close-tool-discard = Abandonner
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
ui-status-keys-sent = { $keys } envoyé à la session distante
ui-status-keys-not-sent = Impossible d'envoyer les touches à la session distante.
ui-files-state-queued = En attente
ui-files-state-preparing = Préparation du transfert...
ui-files-state-scanning = { $count ->
    [one] Analyse des dossiers : { $count } entrée trouvée...
   *[other] Analyse des dossiers : { $count } entrées trouvées...
}
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
ui-settings-session-tree = Arbre des sessions
ui-settings-show-properties-panel = Afficher le panneau des propriétés
ui-settings-show-properties-panel-hint = Ancre sous l'arbre un panneau avec les détails de la session sélectionnée. Aussi dans le menu de filtre de l'arbre et sur Ctrl+Maj+B.
ui-settings-prevent-sleep = Empêcher la mise en veille pendant les sessions actives
ui-settings-prevent-sleep-hint = Empêche Windows de se mettre en veille tant qu'une session est connectée ; l'écran peut toujours s'éteindre ou se verrouiller.
ui-settings-max-sessions = Avertir au-delà de ce nombre de sessions ouvertes
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
ui-settings-rdp-reset-defaults-tooltip = Restaure uniquement les valeurs RDP par défaut. Les autres paramètres ne sont pas modifiés.
ui-dialog-reset-rdp-title = Réinitialiser les valeurs RDP ?
ui-dialog-reset-rdp-body = Restaurer toutes les valeurs RDP par défaut ? Les serveurs existants ne sont pas affectés.
ui-settings-reset-all = Valeurs par défaut
ui-dialog-reset-all-title = Réinitialiser tous les paramètres ?
ui-dialog-reset-all-body = Restaurer tous les onglets des Paramètres aux valeurs d'usine ? Le texte saisi et pas encore appliqué est perdu. Dans l'onglet Sécurité, cela désactive le fournisseur d'identifiants externe, l'exigence Credential Guard et l'exigence Windows Hello à la connexion, et réinitialise le délai de grâce Windows Hello, le délai de verrouillage automatique et la déconnexion au verrouillage. Votre langue, votre thème, vos sessions, passerelles SSH, mot de passe maître, code PIN et inscription Windows Hello sont conservés. Le changement est enregistré aussitôt.
ui-dialog-apply-ssh-mode-title = Appliquer à toutes les sessions SSH enregistrées ?
ui-dialog-apply-ssh-mode-body = { $changes } des { $total } sessions SSH enregistrées passeront en mode { $mode }, et { $mode } devient le mode par défaut des nouvelles sessions. Action irréversible.
ui-dialog-apply-rdp-mode-title = Appliquer à toutes les sessions RDP enregistrées ?
ui-dialog-apply-rdp-mode-body = { $changes } des { $total } sessions RDP enregistrées passeront en mode { $mode }, et { $mode } devient le mode par défaut des nouvelles sessions. Action irréversible.
ui-about-version = Version { $version }
ui-about-tagline = Gestionnaire de connexions sécurisé RDP/SSH/SFTP
ui-about-section-system = Système
ui-about-platform = Plateforme
ui-about-iced = iced
ui-about-russh = russh
ui-about-ironrdp = IronRDP
ui-about-build-date = Date de compilation
ui-about-author = Auteur
ui-about-license = Licence
ui-about-section-data = Données
ui-about-sessions = Sessions
ui-about-gateways = Passerelles
ui-about-config-path = Config
ui-about-log-path = Journaux
ui-about-section-links = Accès rapide
ui-about-open-config = Ouvrir le dossier de configuration
ui-about-open-logs = Ouvrir le dossier des journaux
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
ui-shortcuts-full-screen-over-sessions = Basculer le plein écran sur les sessions intégrées
ui-shortcuts-settings = Ouvrir les paramètres
ui-shortcuts-screenshot = Capturer l'écran
ui-shortcuts-copy-status = Copier l'état et ses derniers messages, pour un lecteur d'écran
ui-shortcuts-lock = Verrouiller
ui-shortcuts-help = Afficher cette aide
ui-shortcuts-close = Fermer une boîte de dialogue ou un menu ; quitter le plein écran hors d'un terminal ou d'un bureau distant
ui-resolution-match-aspect = Suivre la fenêtre, { $wide }:{ $high }
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
ui-profile-environment-staging = Préproduction
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
ui-status-files-delete-stopped = { $total ->
    [one] Suppression annulée : { $done } élément sur { $total } supprimé.
   *[other] Suppression annulée : { $done } éléments sur { $total } supprimés.
}
ui-status-files-permissions-stopped = { $total ->
    [one] Modification des permissions annulée : { $done } élément sur { $total } modifié.
   *[other] Modification des permissions annulée : { $done } éléments sur { $total } modifiés.
}
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
ui-macro-editor-timeout = Délai (ms)
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
ui-dialog-settings-import-sensitive = Vérifiez d'abord ceux-ci : ils décident de ce que Heimdall exécute sur cet ordinateur.
ui-dialog-settings-import-weakens = Ceux-ci affaiblissent un contrôle de sécurité :
ui-dialog-settings-import-more = { $count ->
    [one] et { $count } autre
   *[other] et { $count } autres
}
ui-dialog-settings-import-ignored = { $count ->
    [one] Le fichier contient aussi { $count } paramètre que cette version n'importe pas. Il est laissé de côté :
   *[other] Le fichier contient aussi { $count } paramètres que cette version n'importe pas. Ils sont laissés de côté :
}
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
ui-settings-reachability-hint = Chaque serveur est appelé depuis cet ordinateur, quelques-uns à la fois, et sa pastille dans la liste montre s'il a répondu. Les serveurs derrière une passerelle ne sont pas appelés. Rien n'est envoyé hormis la connexion.
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
ui-session-load-notice-text = { $open ->
    [one] { $open } session intégrée est ouverte
   *[other] { $open } sessions intégrées sont ouvertes
}, au-delà du seuil de { $threshold } fixé dans les Paramètres. Chacune consomme de la mémoire, surtout en RDP : fermez celles dont vous n'avez plus besoin si Heimdall ralentit.
ui-session-load-notice-hide = Masquer
ui-tree-reachability-checking = Vérification...
ui-tree-reachability-up = Joignable ({ $millis } ms)
ui-tree-reachability-down = Injoignable : { $reason }
ui-tree-reachability-unchecked = Inconnu : { $reason }
ui-reachability-reason-timeout = Délai de connexion dépassé
ui-reachability-reason-refused = Connexion refusée
ui-reachability-reason-unreachable = Hôte injoignable
ui-reachability-reason-dns = Résolution DNS échouée
ui-reachability-reason-behind-gateway = Derrière une passerelle SSH - non sondé
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
ui-status-listing-in-progress = Chargement du dossier en cours : attendez qu'il s'affiche, puis réessayez.
ui-status-sftp-auto-open-failed = Ouverture automatique SFTP échouée : { $reason }
ui-status-sftp-browser-disabled = Le navigateur SFTP intégré est désactivé dans les paramètres.
ui-status-nothing-to-undo = Rien à annuler.
ui-shortcuts-undo-move = Annuler le dernier déplacement fait en glissant dans l'arborescence
ui-vnc-scaling-tooltip = Mode de mise à l'échelle
ui-vnc-scaling-remote-resize = Redimensionnement distant
ui-vnc-scaling-scale = Mise à l'échelle
ui-vnc-scaling-none = Sans mise à l'échelle
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
ui-detail-saved-passphrase = phrase de passe de la clé
ui-detail-hints = Entrée ou double-clic connecte, Ctrl+E modifie, Suppr supprime, F1 liste tous les raccourcis.
ui-detail-edit = Modifier
## The properties panel under the tree, as the C# SessionPropertiesView docked there.
ui-properties-title = Propriétés
ui-properties-hide-tooltip = Masquer le panneau de propriétés (Ctrl+Shift+B le réaffiche)
ui-properties-empty = Sélectionnez une session pour voir ses propriétés.
ui-properties-auth = Authentification :
ui-properties-ssh-key = Clé SSH :
ui-properties-origin = Origine :
ui-properties-latency = Latence :
ui-properties-latency-value = { $millis } ms
ui-properties-auth-username = Nom d'utilisateur
ui-properties-auth-ssh-key = Clé SSH
ui-properties-auth-password = Mot de passe
ui-properties-auth-ssh-agent = Agent SSH
ui-properties-auth-current-user = Utilisateur actuel
ui-properties-auth-provider = Fournisseur d'identifiants
ui-properties-auth-none-saved = Aucun identifiant enregistré
ui-properties-copy-hint = Cliquez pour copier
ui-properties-summary-sessions = Sessions :
ui-properties-summary-connected = Connectées :
ui-tree-filter-properties-panel = Afficher le panneau de propriétés
ui-menu-keys-properties-panel = Ctrl+Shift+B
ui-shortcuts-properties-panel = Afficher ou masquer le panneau de propriétés
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
ui-file-dialog-no-session-bus = Aucun dialogue de fichier : le bus de session D-Bus est injoignable ({ $reason }). Heimdall affiche ses dialogues de fichier par le portail du bureau, xdg-desktop-portal.
ui-file-dialog-no-portal = Aucun dialogue de fichier : le portail du bureau ne répond pas ({ $reason }). Installez xdg-desktop-portal et le portail de votre bureau (GNOME, KDE ou GTK).
ui-file-dialog-failed = Le dialogue de fichier n'a pas pu s'ouvrir : { $reason }
ui-file-dialog-closed = Le dialogue de fichier s'est fermé : le bus de session a disparu avant sa réponse.
ui-file-dialog-ended = Le dialogue de fichier s'est terminé sans réponse (code { $code }).
ui-file-dialog-no-location = Le dialogue de fichier a répondu sans fichier.
ui-file-dialog-not-local = L'emplacement choisi n'est pas un fichier de cet ordinateur : { $uri }
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
# A tool tab opened on a host or an argument, as the C# PaletteTool*With keys.
ui-tools-tab-with = { $tool } : { $target }
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
ui-tool-export-tooltip = Enregistrer dans un fichier
ui-tool-export-filter-csv = Fichiers CSV
ui-tool-export-filter-html = Fichiers HTML
ui-tool-export-failed = Échec de l'export : { $detail }
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
ui-tool-chmod-command = chmod { $mode } fichier
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
ui-tool-datetime-iso-local = ISO 8601 (heure locale)
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
ui-tool-crontab-error-malformed-field = Le champ "{ $field }" n'est ni un nombre, ni une plage, ni un pas : { $value }
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
ui-tool-password-name = Générateur de mots de passe
ui-tool-password-description = Générateur de mots de passe aléatoires sécurisés avec règles personnalisables
ui-tool-password-title = Générateur de mot de passe
ui-tool-password-generate = Générer
ui-tool-password-generate-tooltip = Générer un nouveau mot de passe
ui-tool-password-copy = Copier
ui-tool-password-length = Longueur
ui-tool-password-total-length = Longueur totale : { $count } caractères
ui-tool-password-uppercase = Majuscules (A-Z)
ui-tool-password-lowercase = Minuscules (a-z)
ui-tool-password-digits = Chiffres (0-9)
ui-tool-password-symbols = Spéciaux (!@#$...)
ui-tool-password-mode = Mode :
ui-tool-password-mode-random = Aléatoire
ui-tool-password-mode-syllable = Syllabique
ui-tool-password-mode-passphrase = Phrase de passe
ui-tool-password-mode-leet = Leet
ui-tool-password-mode-random-desc = Caractères aléatoires parmi les jeux sélectionnés
ui-tool-password-mode-syllable-desc = Paires consonne-voyelle prononçables (facile à retenir)
ui-tool-password-mode-passphrase-desc = Mots du dictionnaire séparés par un délimiteur
ui-tool-password-mode-leet-desc = Un mot réécrit en leet, avec chiffres et caractères spéciaux ajoutés
ui-tool-password-case = Casse
ui-tool-password-case-mixed = Mixte
ui-tool-password-case-lower = Minuscules
ui-tool-password-case-upper = Majuscules
ui-tool-password-case-title = Première lettre
ui-tool-password-case-alternating = Alterné
ui-tool-password-case-word = Casse mot
ui-tool-password-case-inverse = Inverse
ui-tool-password-case-blocks = Motif
ui-tool-password-word-count = Nombre de mots
ui-tool-password-separator = Séparateur
ui-tool-password-language = Langue
ui-tool-password-word-list-size = { $words } mots, { $bits } bits chacun
ui-tool-password-lang-english = Anglais
ui-tool-password-lang-french = Français
ui-tool-password-lang-spanish = Espagnol
ui-tool-password-lang-latin = Latin
ui-tool-password-strength-critical = Critique
ui-tool-password-strength-weak = Faible
ui-tool-password-strength-fair = Moyen
ui-tool-password-strength-good = Bon
ui-tool-password-strength-strong = Fort
ui-tool-password-bits = { $bits } bits
ui-tool-password-issue-too-short = Trop court (< 8 caractères)
ui-tool-password-issue-no-upper = Pas de majuscules
ui-tool-password-issue-no-lower = Pas de minuscules
ui-tool-password-issue-no-digit = Pas de chiffres
ui-tool-password-issue-no-special = Pas de caractères spéciaux
ui-tool-password-issue-chosen-word = Un mot que vous avez choisi vous-même ne compte pour rien dans la force affichée
ui-tool-password-issue-floor-unreachable = Ces réglages ne peuvent pas garantir { $floor } bits. Leur maximum est { $ceiling }, donc rien n'a été changé
ui-tool-password-classes-not-promised = Ce mot de passe est un tirage ordinaire : la longueur ne peut pas porter un caractère de chaque sorte cochée.
ui-tool-password-counts-cut = Cette longueur laisse la place à { $digits } chiffres et { $specials } spéciaux.
ui-tool-password-floor-raised = Ce mot de passe a utilisé { $used } là où vous avez réglé { $chosen }, pour atteindre le minimum. Votre réglage est inchangé.
ui-tool-password-floor-set-length = Longueur portée à { $length } pour garantir { $floor } bits.
ui-tool-password-floor-set-syllable = Réglé sur { $length } caractères, { $digits } chiffres et { $specials } caractères spéciaux, pour garantir { $floor } bits.
ui-tool-password-floor-set-passphrase = Réglé sur { $words } mots, { $digits } chiffres et { $specials } caractères spéciaux, pour garantir { $floor } bits.
ui-tool-password-floor-set-leet = Réglé sur { $digits } chiffres et { $specials } caractères spéciaux, pour garantir { $floor } bits.
ui-tool-password-exclude-ambiguous = Exclure les ambiguës (0O, 1lI)
ui-tool-password-cli-safe = CLI-safe (exclure les caractères shell)
ui-tool-password-layout-safe = Layout-safe (compatible AZERTY/QWERTY)
ui-tool-password-custom-specials = Spéciaux personnalisés :
ui-tool-password-specials-usable = Parmi ceux-ci, le générateur utilisera : { $specials }
ui-tool-password-specials-none-usable = Aucun de ceux-ci n'est utilisable : un caractère spécial doit être une ponctuation ASCII, tapable sur tout clavier
ui-tool-password-clipboard-auto-clear = Effacement automatique du presse-papiers
ui-tool-password-clipboard-clear-hint = Le presse-papiers s'efface dans { $seconds } s
ui-tool-password-clipboard-cleared = Presse-papiers effacé
ui-tool-password-seconds = s
ui-tool-password-remember-settings = Retenir mes réglages
ui-tool-password-advanced = Avancé
ui-tool-password-advanced-tooltip = Ouvrir les options avancées
ui-tool-password-preset-label = Préréglage :
ui-tool-password-preset-tooltip = Choisissez un préréglage, ou Enregistrer pour conserver les réglages actuels
ui-tool-password-preset-custom = Personnalisé
ui-tool-password-presets-saved = Enregistré : { $name }
ui-tool-password-preset-pin4 = PIN 4
ui-tool-password-preset-pin6 = PIN 6
ui-tool-password-preset-wifi = WiFi
ui-tool-password-preset-api-key = Clé API
ui-tool-password-preset-mysql = MySQL
ui-tool-password-preset-ssh = SSH
ui-tool-password-preset-syl-easy = Facile 18
ui-tool-password-preset-syl-balanced = Équilibré 24
ui-tool-password-preset-syl-strong = Fort 30
ui-tool-password-preset-passphrase4 = 4 mots
ui-tool-password-preset-passphrase6 = 6 mots
ui-tool-password-save-preset = Sauvegarder le préréglage
ui-tool-password-save-preset-tooltip = Enregistrer un préréglage personnalisé
ui-tool-password-save-preset-prompt = Entrez un nom pour ce préréglage :
ui-tool-password-delete-preset = Supprimer
ui-tool-password-delete-preset-tooltip = Supprimer le préréglage enregistré sélectionné
ui-tool-password-delete-preset-title = Supprimer le préréglage
ui-tool-password-delete-preset-confirm = Supprimer le préréglage "{ $name }" ?
ui-tool-password-entropy-floor = Force minimale :
ui-tool-password-entropy-floor-off = Aucun minimum
ui-tool-password-phonetic = Phonétique :
ui-tool-password-copy-phonetic = Copier la phonétique
ui-tool-password-keyboard-hint = Entrée : regénérer - Échap : effacer
ui-tool-password-quick-length = Rapide :
ui-tool-password-history = Mots de passe récents
ui-tool-password-clear-history = Effacer
ui-tool-password-clear-history-tooltip = Effacer l'historique
ui-tool-password-history-empty = Les mots de passe générés apparaîtront ici
ui-tool-password-syl-step-note = Incréments de 2 (paires consonne-voyelle)
ui-tool-password-syl-step-note-cvc = Mélange de syllabes de 2 et 3 caractères
ui-tool-password-syl-cvc = Syllabes fermées (CVC)
ui-tool-password-syl-cvc-hint = bat, fen au lieu de ba, fe - plus naturel
ui-tool-password-syl-structure = Structure :
ui-tool-password-leet-word = Mot de base
ui-tool-password-leet-random-word = Tirer le mot au hasard
ui-tool-password-leet-full-substitution = Réécrire toutes les lettres de la table
ui-tool-password-leet-word-source = Mot tiré :
ui-tool-password-blocks = Motif de casse :
ui-tool-password-blocks-hint = Cliquez un bloc pour le faire tourner : U majuscules, l minuscules, T capitale initiale
ui-tool-password-blocks-random = Mélanger
ui-tool-password-blocks-auto-sync = Un bloc par syllabe
ui-tool-password-blocks-auto-sync-words = Un bloc par mot
ui-tool-password-blocks-add = Ajouter un bloc
ui-tool-password-blocks-remove = Retirer un bloc
ui-tool-password-blocks-all-upper = Tous les blocs en majuscules
ui-tool-password-blocks-all-lower = Tous les blocs en minuscules
ui-tool-password-blocks-all-title = Tous les blocs en capitale initiale
ui-tool-password-placement = Placement
ui-tool-password-placement-random = Aléatoire
ui-tool-password-placement-start = Début
ui-tool-password-placement-end = Fin
ui-tool-password-placement-middle = Milieu
ui-tool-password-placement-positions = Par position
ui-tool-password-placement-bar = Barre de placement :
ui-tool-password-placement-bar-hint = Faites glisser un curseur, ou donnez-lui le focus et utilisez les flèches
ui-tool-password-placement-distribute = Répartir
ui-tool-password-batch-count = Combien :
ui-tool-password-batch = Lot généré :
ui-tool-password-batch-mask = Masquer
ui-tool-password-batch-copy-all = Tout copier
ui-tool-password-batch-export = Exporter
ui-tool-password-batch-export-failed = Échec de l'export : { $error }
ui-tool-password-text-filter = Fichiers texte
ui-tool-password-crack-time = Temps de crack : { $time }
ui-tool-password-crack-assumption = cas moyen, à { $rate } essais par seconde contre une empreinte volée
ui-tool-password-crack-instant = instantané
ui-tool-password-crack-forever = des milliards d'années
ui-tool-password-help =
    Générateur de Mots de Passe

    Génère des mots de passe sécurisés selon quatre modes.

    Modes :
    1. Aléatoire : caractères aléatoires d'un jeu configurable
    2. Syllabique : mots de passe prononçables par paires consonne-voyelle
    3. Phrase de passe : mots de passe basés sur un dictionnaire
    4. Leet : un mot réécrit en leet, avec chiffres et caractères spéciaux ajoutés

    Fonctionnalités :
    - Indicateur de force en temps réel avec calcul d'entropie
    - Estimation du temps de cassage
    - Lecture phonétique alphabet OTAN
    - Mode layout-safe (évite les différences QWERTY/AZERTY)
    - Mode CLI-safe (évite les métacaractères shell)
    - Préréglages rapides (PIN, Wi-Fi, clé API, SSH, etc.)
    - Préréglages personnalisés : sauvegardez vos propres configurations
    - Historique des mots de passe (10 derniers)

    Clavier :
    - Entrée : regénérer le mot de passe
    - Échap : effacer la sortie

ui-tool-sshkey-name = Générateur de clé SSH
ui-tool-sshkey-description = Générateur de paires de clés SSH (RSA, Ed25519, ECDSA)
ui-tool-sshkey-title = Générateur de clé SSH
ui-tool-sshkey-algorithm = Algorithme
ui-tool-sshkey-rsa-2048 = RSA (2048)
ui-tool-sshkey-rsa-4096 = RSA (4096)
ui-tool-sshkey-ed25519 = Ed25519
ui-tool-sshkey-comment = Commentaire
ui-tool-sshkey-comment-placeholder = utilisateur@hôte
ui-tool-sshkey-passphrase = Phrase de passe
ui-tool-sshkey-passphrase-hint = Facultatif. Chiffre la clé privée avec AES-256-CBC.
ui-tool-sshkey-generate = Générer
ui-tool-sshkey-generating = Génération...
ui-tool-sshkey-copy = Copier
ui-tool-sshkey-save = Enregistrer
ui-tool-sshkey-show = Afficher
ui-tool-sshkey-hide = Masquer
ui-tool-sshkey-fingerprint = Empreinte
ui-tool-sshkey-public-key = Clé publique
ui-tool-sshkey-private-key = Clé privée
ui-tool-sshkey-ed25519-notice = Les clés Ed25519 offrent une sécurité robuste avec des clés compactes de 256 bits.
ui-tool-sshkey-public-filter = Clé publique SSH
ui-tool-sshkey-private-filter = Clé privée PEM
ui-tool-sshkey-help =
    Générateur de Clés SSH

    Génère des paires de clés SSH pour l'authentification sécurisée.

    Utilisation :
    1. Sélectionnez l'algorithme : RSA 2048, RSA 4096 ou Ed25519
    2. Entrez un commentaire optionnel (défaut : utilisateur@machine)
    3. Entrez une phrase de passe optionnelle pour le chiffrement
    4. Cliquez sur Générer

    Sortie :
    - Clé publique au format OpenSSH (ssh-rsa / ssh-ed25519)
    - Clé privée au format PKCS#8 PEM
    - Empreinte SHA-256

    Note :
    - RSA 4096 est recommandé pour la compatibilité maximale
    - Les clés avec phrase de passe sont chiffrées en AES-256-CBC

    Astuce : copiez la clé publique et ajoutez-la à ~/.ssh/authorized_keys sur le serveur distant.
ui-tool-certgen-name = Générateur de certificats
ui-tool-certgen-description = Générateur de certificats auto-signés pour le développement et les tests
ui-tool-certgen-title = Générateur de certificats
ui-tool-certgen-subject = Sujet
ui-tool-certgen-cn = Nom commun (CN)
ui-tool-certgen-org = Organisation (O)
ui-tool-certgen-country = Pays (C)
ui-tool-certgen-options = Options
ui-tool-certgen-key-size = Taille de clé
ui-tool-certgen-rsa-2048 = RSA 2048
ui-tool-certgen-rsa-4096 = RSA 4096
ui-tool-certgen-validity = Validité (jours)
ui-tool-certgen-san = Noms alternatifs
ui-tool-certgen-san-hint = Noms d'hôtes ou IPs séparés par des virgules (ex. server1.local, 10.0.0.1)
ui-tool-certgen-type = Type de certificat
ui-tool-certgen-type-self-signed = Certificat auto-signé
ui-tool-certgen-type-ca-leaf = Paire CA + Leaf
ui-tool-certgen-generate = Générer
ui-tool-certgen-generating = Génération...
ui-tool-certgen-fingerprint = Empreinte
ui-tool-certgen-copy = Copier
ui-tool-certgen-show = Afficher
ui-tool-certgen-hide = Masquer
ui-tool-certgen-cert-pem = Certificat (PEM)
ui-tool-certgen-key-pem = Clé privée (PEM)
ui-tool-certgen-ca-cert-pem = Certificat CA (PEM)
ui-tool-certgen-ca-key-pem = Clé privée CA (PEM)
ui-tool-certgen-leaf-cert-pem = Certificat Leaf (PEM)
ui-tool-certgen-leaf-key-pem = Clé privée Leaf (PEM)
ui-tool-certgen-save-pem = Enregistrer .pem
ui-tool-certgen-save-pfx = Enregistrer .pfx
ui-tool-certgen-ok = OK
ui-tool-certgen-cancel = Annuler
ui-tool-certgen-pfx-password-title = Mot de passe PFX
ui-tool-certgen-pfx-password-prompt = Entrez un mot de passe pour le fichier PFX (peut être vide) :
ui-tool-certgen-error-cn-required = Veuillez entrer un nom commun (CN).
ui-tool-certgen-error-invalid-validity = Veuillez entrer un nombre de jours valide (1 ou plus).
ui-tool-certgen-error-generation = La génération du certificat a échoué : { $error }
ui-tool-certgen-error-export = L'export PFX a échoué : { $error }
ui-tool-certgen-pem-filter = Certificat PEM
ui-tool-certgen-pfx-filter = PFX/PKCS#12
ui-tool-certgen-help =
    Générateur de Certificats

    Génère des certificats X.509 auto-signés pour le développement et les tests.

    Utilisation :
    Configurez le sujet, la validité, la taille de clé et les SANs, puis générez et exportez le certificat.
ui-tool-pwdaudit-name = Auditeur de mot de passe
ui-tool-pwdaudit-description = Analyseur de robustesse des mots de passe avec vérification de politique
ui-tool-pwdaudit-title = Vérificateur de politique de mot de passe
ui-tool-pwdaudit-input = Mot de passe
ui-tool-pwdaudit-placeholder = Saisissez un mot de passe
ui-tool-pwdaudit-show = Afficher le mot de passe
ui-tool-pwdaudit-hide = Masquer le mot de passe
ui-tool-pwdaudit-policy = Politique
ui-tool-pwdaudit-policy-nist = NIST 800-63B
ui-tool-pwdaudit-policy-anssi = ANSSI
ui-tool-pwdaudit-policy-custom = Personnalisé
ui-tool-pwdaudit-strength = Robustesse
ui-tool-pwdaudit-length = Longueur
ui-tool-pwdaudit-uppercase = Majuscules
ui-tool-pwdaudit-lowercase = Minuscules
ui-tool-pwdaudit-digits = Chiffres
ui-tool-pwdaudit-symbols = Symboles
ui-tool-pwdaudit-entropy = Entropie
ui-tool-pwdaudit-common = Mot de passe courant
ui-tool-pwdaudit-patterns = Motifs détectés
ui-tool-pwdaudit-pass = OK
ui-tool-pwdaudit-fail = Échec
ui-tool-pwdaudit-warn = Avertissement
ui-tool-pwdaudit-score-weak = Faible
ui-tool-pwdaudit-score-fair = Moyen
ui-tool-pwdaudit-score-good = Bon
ui-tool-pwdaudit-score-strong = Fort
ui-tool-pwdaudit-empty = Saisissez un mot de passe pour analyser sa robustesse
ui-tool-pwdaudit-entropy-bits = { $bits } bits
ui-tool-pwdaudit-length-detail = { $length } caractères (minimum : { $minimum })
ui-tool-pwdaudit-pattern-repeat = Caractères répétés
ui-tool-pwdaudit-pattern-sequence = Caractères séquentiels
ui-tool-pwdaudit-pattern-keyboard = Motif clavier
ui-tool-pwdaudit-not-in-common-list = Absent de la liste des mots de passe courants
ui-tool-pwdaudit-in-common-list = Présent dans la liste des mots de passe courants
ui-tool-pwdaudit-help =
    Vérificateur de politique de mot de passe

    Analyse la robustesse d'un mot de passe selon les normes industrielles.

    Politiques :
    - NIST 800-63B : Privilégie la longueur plutôt que la complexité. Minimum 8 caractères, vérifie les mots de passe courants.
    - ANSSI : Norme française exigeant 12+ caractères avec majuscules, minuscules, chiffres et symboles.
    - Personnalisé : Configurez vos propres règles.

    L'analyse comprend :
    - Couverture des classes de caractères
    - Calcul de l'entropie de Shannon
    - Vérification du dictionnaire de mots de passe courants (top 100)
    - Détection de motifs (séquences clavier, suites, répétitions)
    - Score de robustesse global (0-100)
ui-tool-password-saved-elsewhere =
    { $count ->
        [one] { $count } autre enregistré dans un autre mode.
       *[other] { $count } autres enregistrés dans d'autres modes.
    }
ui-tool-password-crack-seconds =
    { $count ->
        [one] { $count } seconde
       *[other] { $count } secondes
    }
ui-tool-password-crack-minutes =
    { $count ->
        [one] { $count } minute
       *[other] { $count } minutes
    }
ui-tool-password-crack-hours =
    { $count ->
        [one] { $count } heure
       *[other] { $count } heures
    }
ui-tool-password-crack-days =
    { $count ->
        [one] { $count } jour
       *[other] { $count } jours
    }
ui-tool-password-crack-years =
    { $count ->
        [one] { $count } an
       *[other] { $count } ans
    }
ui-tool-password-crack-centuries =
    { $count ->
        [one] { $count } siècle
       *[other] { $count } siècles
    }
ui-tool-password-remember-settings-note = Rouvre l'outil là où vous l'avez laissé. Conservé sur cet ordinateur, dans le dossier de réglages de Heimdall, lisible par votre seul compte.
ui-tool-sshkey-error = La génération de la clé a échoué : { $error }
ui-tool-sshkey-save-failed = L'enregistrement a échoué : { $error }
ui-tool-certgen-save-failed = L'enregistrement a échoué : { $error }
ui-tunnels-page-title = Tunnels actifs
ui-import-dropped-rd-gateway = par une passerelle Bureau à distance
ui-error-rd-gateway = Ce serveur se joint par la passerelle Bureau à distance { $gateway }, que le client intégré ne sait pas encore traverser.
ui-error-credential-guard-required = Credential Guard est requis mais n'est pas actif sur ce système
ui-profile-resolution-auto = Auto (recommandé)
ui-profile-resolution-auto-desc = Heimdall choisit le meilleur mode selon le moniteur hôte et le mode plein écran ou fenêtré de la session.
ui-profile-resolution-multi-monitor = Multi-écran
ui-profile-resolution-multi-monitor-unavailable = Multi-écran (nécessite plusieurs moniteurs)
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
ui-import-dropped-scheduled-time = heure de connexion planifiée illisible, désormais { $time }
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
ui-fullscreen-exit-tooltip = F11, Ctrl+Maj+F11, ou Échap hors d'un terminal ou d'un bureau distant
ui-files-selected-with-size = { $selection } ({ $size })
ui-files-special-mark = { $name } ({ $kind })
ui-files-tooltip-type = Type : { $kind }
ui-files-tooltip-size = Taille : { $size }
ui-files-tooltip-modified = Modifié : { $at }
ui-files-tooltip-permissions = Permissions : { $permissions }
ui-files-tooltip-owner = Propriétaire : { $owner }
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
ui-dialog-import-scheduled = Connexions planifiées reprises : { $count }
ui-dialog-import-scheduled-failed = Les connexions planifiées n'ont pas pu être reprises : { $detail }
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
ui-profile-field-passphrase = Phrase de passe de la clé
ui-profile-legacy-algorithms-hint = Les échanges de clés SHA-1, les chiffrements CBC, HMAC-SHA1 et les clés d'hôte RSA SHA-1 sont proposés après les actuels. À activer uniquement pour un appareil qui ne connaît rien de plus récent.
ui-profile-passphrase-clear-tooltip = Supprimer la phrase de passe enregistrée pour cette session
ui-profile-passphrase-hint = Sert uniquement à déchiffrer la clé SSH choisie. Laissez vide si la clé n'a pas de phrase de passe ou si un agent SSH la déverrouille.
ui-profile-passphrase-saved = Phrase de passe enregistrée
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
ui-status-rdp-external-launched-through = Client externe lancé : { $name } s'est ouvert dans Connexion Bureau à distance par la passerelle SSH { $gateway }.
ui-status-rdp-external-gateway = Connexion Bureau à distance n'a pas été ouverte, sa passerelle SSH n'a pas été atteinte : { $reason }
ui-status-rdp-external-forward = Connexion Bureau à distance n'a pas été ouverte, aucun port local n'a pu être ouvert pour sa passerelle SSH : { $reason }
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
ui-tab-route-badge-healthy = Tunnels actifs
ui-tab-route-badge-unhealthy = Tunnels actifs - au moins un est en erreur
ui-tab-tooltip-endpoint = { $endpoint } ({ $protocol })
ui-tab-tooltip-user = Utilisateur : { $user }
ui-tab-tooltip-folder = Dossier : { $folder }
ui-tab-tooltip-state = État : { $state }
ui-tab-close-named = Fermer { $name }
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
ui-desktop-shares-printers = Imprimantes
ui-desktop-shares-printers-tooltip = Redirection des imprimantes
ui-desktop-shares-com-ports = Ports COM
ui-desktop-shares-com-ports-tooltip = Redirection des ports COM
ui-desktop-shares-smart-cards = Cartes à puce
ui-desktop-shares-smart-cards-tooltip = Redirection des cartes à puce
ui-desktop-shares-usb = USB
ui-desktop-shares-usb-tooltip = Redirection USB
ui-desktop-shares-multi-monitor = Multi-écran
ui-desktop-shares-multi-monitor-tooltip = Multi-écran
ui-rdp-redirection-on = { $name } : activée
ui-rdp-redirection-off = { $name } : désactivée
ui-rdp-redirection-unsupported = { $name } : pas encore prise en charge par le client intégré
ui-rdp-redirections-more = +{ $count }
ui-rdp-redirections-more-tooltip = Afficher les redirections désactivées
ui-rdp-health-healthy = Connectée
ui-rdp-health-transitional = En transition
ui-rdp-health-faulted = En erreur
ui-rdp-health-idle = Inactive
ui-rdp-health-tooltip = Santé de la session : { $state }
ui-rdp-phase-preparing = Préparation de la session Bureau à distance.
ui-rdp-phase-connecting = Connexion...
ui-rdp-phase-loading = Chargement de la session distante...
ui-rdp-phase-connected = Connecté
ui-rdp-phase-progress = { $phase } ({ $step } sur { $steps })
ui-rdp-reconnect-elapsed = { $seconds }s écoulées
ui-rdp-letterbox-hint = Fixe { $width }x{ $height } - redimensionnez la fenêtre ou changez la résolution pour remplir.
ui-settings-rdp-connect-timeout = Délai du watchdog de connexion RDP (0 = désactivé)
ui-settings-rdp-resize-delay = Délai de stabilisation de la résolution après connexion (0 = désactivé)
ui-settings-rdp-resize-delay-refused = Le délai de redimensionnement RDP doit être nul ou compris entre { $min } et { $max } ms.
ui-settings-rdp-keep-alive-interval = Intervalle de maintien de session
ui-settings-rdp-keep-alive-interval-refused = L'intervalle de maintien de connexion RDP doit être compris entre { $min } et { $max } ms.
ui-settings-rdp-builtin-compression = Compresser dans le client intégré (expérimental)
ui-settings-rdp-builtin-compression-hint = S'applique uniquement au client RDP intégré, pour les profils dont la compression RDP est activée. La Connexion Bureau à distance suit le profil.
ui-settings-rdp-connect-timeout-off = Désactivé
ui-settings-rdp-connect-timeout-seconds = { $seconds } s
ui-shortcuts-release-desktop = Rendre le clavier depuis un bureau distant
ui-profile-toggle-strict-server-auth = Exiger la validation de l'identité serveur
ui-error-rdp-server-not-authenticated = L'identité du serveur n'a pas pu être validée : les autorités de certification de cet ordinateur ne garantissent pas son certificat. L'authentification stricte du serveur le refuse, même s'il a été approuvé auparavant.
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

## La migration depuis l'ancien Heimdall PowerShell, comme le C# la formule.
ui-legacy-migration-title = Migration
ui-legacy-migration-offer = Une installation Heimdall existante a été détectée dans { $path }, avec { $profiles ->
    [one] { $profiles } serveur
   *[other] { $profiles } serveurs
} et { $gateways ->
    [one] { $gateways } passerelle SSH
   *[other] { $gateways } passerelles SSH
}. Souhaitez-vous importer vos serveurs et paramètres ?
ui-legacy-migration-import = Importer
ui-legacy-migration-decline = Ne pas importer
ui-legacy-migration-success = { $count ->
    [one] { $count } serveur importé avec succès.
   *[other] { $count } serveurs importés avec succès.
}
ui-legacy-migration-partial = Migration terminée avec des profils ignorés : { $examined ->
    [one] { $examined } examiné
   *[other] { $examined } examinés
}, { $imported ->
    [one] { $imported } importé
   *[other] { $imported } importés
}, { $skipped ->
    [one] { $skipped } ignoré
   *[other] { $skipped } ignorés
}.
ui-legacy-migration-partial-item = # { $index } - { $name } : { $reason }
ui-legacy-migration-unnamed = Profil sans nom
ui-legacy-migration-invalid-field = valeur de champ hérité invalide
ui-legacy-migration-omitted = { $count ->
    [one] { $count } autre profil ignoré non affiché.
   *[other] { $count } autres profils ignorés non affichés.
}
ui-legacy-migration-projects = { $count ->
    [one] { $count } projet n'a pas été importé : Heimdall n'a pas de projets.
   *[other] { $count } projets n'ont pas été importés : Heimdall n'a pas de projets.
}
ui-legacy-migration-secrets = Les mots de passe, les phrases secrètes, le PIN et les chemins de programmes de l'ancienne version ne sont pas importés : Heimdall redemande chaque mot de passe à la prochaine connexion.
ui-legacy-migration-key-paths = { $count ->
    [one] { $count } chemin de clé a été écarté : ce n'était pas un fichier sur un lecteur local (chemin réseau, de périphérique ou relatif). Choisissez de nouveau la clé dans le profil ou la passerelle.
   *[other] { $count } chemins de clé ont été écartés : ce n'étaient pas des fichiers sur un lecteur local (chemins réseau, de périphérique ou relatifs). Choisissez de nouveau les clés dans les profils ou les passerelles.
}
ui-legacy-migration-settings-not-saved = Les paramètres de l'ancienne version n'ont pas pu être enregistrés. Consultez le journal pour plus de détails.
ui-settings-legacy-migration = Migration de l'ancienne version
ui-settings-legacy-migration-description = Autorise la proposition de migration de l'ancienne version au prochain démarrage si la source est toujours disponible et si l'inventaire actuel des serveurs est vide.
ui-settings-legacy-migration-reoffer = Reproposer la migration au prochain démarrage
ui-settings-legacy-migration-unavailable = Disponible uniquement après avoir refusé une proposition de migration au démarrage.
ui-status-legacy-migration-reoffered = La proposition de migration pourra réapparaître au prochain démarrage. Aucune migration n'a été lancée maintenant.
ui-status-legacy-migration-reoffer-failed = La préférence de migration de l'ancienne version n'a pas pu être enregistrée. Le comportement actuel au démarrage n'a pas été modifié.
ui-onboarding-sessions-title = Connectez-vous à des sessions
ui-onboarding-sessions-body = Gérez RDP, SSH, SFTP, VNC et plus depuis une interface unique. Ajoutez votre première session ou importez depuis MobaXterm, mRemoteNG ou RDCMan.
ui-onboarding-add-title = Ajoutez votre première session
ui-onboarding-add-body = Le bouton + en haut de la liste des sessions crée une session, un outil, un dossier ou une passerelle SSH. Vous pouvez aussi importer depuis MobaXterm, mRemoteNG ou RDCMan.
ui-onboarding-search-title = Retrouvez tout rapidement
ui-onboarding-search-body = Filtre les sessions et les outils au fil de la frappe. Cherche dans les noms, les hôtes et les étiquettes.
ui-onboarding-tools-title = Outils intégrés
ui-onboarding-tools-body = { $count } outils réseau, sécurité, encodage et système à portée de main. Accédez-y depuis l'onglet Outils ou appuyez sur Ctrl+K.
ui-onboarding-quick-connect-title = Connexion rapide
ui-onboarding-quick-connect-body = Appuyez sur Ctrl+K pour ouvrir la Palette de commandes. Tapez un nom d'hôte, une adresse IP ou un nom d'outil pour vous connecter ou le lancer instantanément.
ui-onboarding-settings-title = Les paramètres, et cette visite
ui-onboarding-settings-body = Les valeurs par défaut des protocoles, l'apparence et la sécurité sont dans l'onglet Paramètres. Le bouton Revoir la visite aussi, sous Visite guidée, quand vous voulez.
ui-onboarding-skip = Passer
ui-onboarding-next = Suivant
ui-onboarding-get-started = Commencer
ui-onboarding-save-failed = Heimdall n'a pas pu enregistrer la fin de la configuration. Vérifiez l'accès au dossier de configuration, puis réessayez.
ui-settings-onboarding = Visite guidée
ui-settings-onboarding-replay = Revoir la visite
ui-settings-onboarding-hint = Rejoue la courte introduction affichée au premier lancement. Vous pouvez la quitter à tout moment et revenir ici.
ui-profile-rdp-monitors-refresh = Actualiser
ui-profile-resolution-switch-auto = Revenir à Auto
ui-macros-edit-named = Modifier "{ $name }"
# The C# Network tab's connection path diagram.
ui-profile-path-title = Chemin de connexion
ui-profile-path-headline-tunnel = Heimdall créera un tunnel SSH avant d'ouvrir la session.
ui-profile-path-headline-direct = Heimdall se connectera directement à l'hôte de destination.
ui-profile-path-your-pc = Votre PC
ui-profile-path-gateway = Passerelle
ui-profile-path-destination = Destination
ui-profile-path-client-tunnel = Tunnel local sur localhost:{ $port }
ui-profile-path-client-direct = Connexion sortante directe
ui-profile-path-destination-server = Serveur de destination
ui-profile-path-ssh-tunnel = Tunnel SSH
ui-profile-path-no-gateway = Aucune passerelle sélectionnée
ui-profile-path-no-ssh-gateway = Aucune passerelle SSH sélectionnée
ui-profile-path-arrow = →
ui-profile-path-session-rdp = Session RDP
ui-profile-path-session-ftp = Session FTP
ui-profile-path-session-sftp = Session SFTP
ui-profile-path-session-ssh = Session SSH
ui-profile-path-session-winrm = Session WinRM
ui-profile-path-session-vnc = Session VNC
ui-profile-path-session-telnet = Session Telnet
ui-profile-path-session-citrix = Session Citrix
ui-profile-path-session-local = Terminal local

ui-tool-sshaudit-name = Auditeur de clé SSH
ui-tool-sshaudit-description = Analyse la sécurité des clés SSH et fournit des recommandations
ui-tool-sshaudit-title = Auditeur de clé SSH
ui-tool-sshaudit-input = Collez une clé SSH (publique ou privée, format OpenSSH ou PEM)
ui-tool-sshaudit-browse = Parcourir...
ui-tool-sshaudit-details = Détails de la clé
ui-tool-sshaudit-findings = Résultats de l'audit de sécurité
ui-tool-sshaudit-algorithm = Algorithme
ui-tool-sshaudit-key-size = Taille de clé
ui-tool-sshaudit-fingerprint = Empreinte
ui-tool-sshaudit-format = Format
ui-tool-sshaudit-type = Type
ui-tool-sshaudit-public = Clé publique
ui-tool-sshaudit-private = Clé privée
ui-tool-sshaudit-encrypted = Chiffrée
ui-tool-sshaudit-yes = Oui
ui-tool-sshaudit-no = Non
ui-tool-sshaudit-strong = Forte
ui-tool-sshaudit-acceptable = Acceptable
ui-tool-sshaudit-weak = Faible
ui-tool-sshaudit-deprecated = Obsolète
ui-tool-sshaudit-bits = { $bits } bits
ui-tool-sshaudit-copy-fingerprint = Copier l'empreinte
ui-tool-sshaudit-empty-state = Collez une clé SSH ou parcourez pour auditer sa sécurité
ui-tool-sshaudit-parse-error = Impossible d'analyser la clé fournie. Assurez-vous qu'il s'agit d'une clé SSH valide.
ui-tool-sshaudit-file-too-large = Le fichier dépasse { $limit } octets et ne peut pas être audité.
ui-tool-sshaudit-file-read-error = Impossible de lire le fichier de clé : { $error }
ui-tool-sshaudit-finding-ed25519 = Ed25519 est l'algorithme recommandé - rapide, sécurisé, compact
ui-tool-sshaudit-finding-rsa-strong = La taille de la clé RSA est forte ({ $bits } bits)
ui-tool-sshaudit-finding-rsa-ok = RSA 2048 bits satisfait le minimum requis mais 4096 est recommandé
ui-tool-sshaudit-finding-rsa-weak = La clé RSA est trop courte ({ $bits } bits) - minimum 2048 requis
ui-tool-sshaudit-finding-dsa = DSA est obsolète et ne devrait plus être utilisé - migrez vers Ed25519
ui-tool-sshaudit-finding-ecdsa-strong = ECDSA avec courbe { $bits } bits est forte
ui-tool-sshaudit-finding-ecdsa-ok = ECDSA 256 bits est acceptable - considérez Ed25519 pour de meilleures performances
ui-tool-sshaudit-finding-unencrypted = La clé privée n'est pas chiffrée - ajoutez une phrase de passe
ui-tool-sshaudit-finding-encrypted = La clé privée est chiffrée avec une phrase de passe
ui-tool-sshaudit-finding-old-format = L'ancien format PEM utilise un chiffrement MD5 faible - convertissez au format OpenSSH
ui-tool-sshaudit-help =
    Auditeur de clé SSH

    Analyse la sécurité des clés SSH et fournit des recommandations.

    Utilisation :
    1. Collez une clé SSH (publique ou privée) dans la zone de texte, OU
    2. Cliquez sur Parcourir pour charger un fichier de clé

    Formats pris en charge :
    - Clés publiques OpenSSH (ssh-rsa, ssh-ed25519, ecdsa-sha2-*)
    - Clés privées OpenSSH
    - Format PEM (PKCS#1, PKCS#8)

    Contrôles de sécurité :
    - Force de l'algorithme (Ed25519 > ECDSA > RSA > DSA)
    - Taille adéquate de la clé (RSA : min 2048, recommandé 4096)
    - Statut de chiffrement (clés privées non chiffrées signalées)
    - Sécurité du format (chiffrement PEM ancien vs OpenSSH moderne)

ui-tool-tlsaudit-name = Auditeur TLS
ui-tool-tlsaudit-description = Audite les protocoles TLS, les suites de chiffrement et les certificats
ui-tool-tlsaudit-title = Auditeur SSL/TLS
ui-tool-tlsaudit-host = Hôte
ui-tool-tlsaudit-port = Port
ui-tool-tlsaudit-audit = Auditer
ui-tool-tlsaudit-cancel = Annuler
ui-tool-tlsaudit-grade = Note
ui-tool-tlsaudit-protocols = Protocoles supportés
ui-tool-tlsaudit-ciphers = Suites de chiffrement (TLS 1.2)
ui-tool-tlsaudit-supported = Supporté
ui-tool-tlsaudit-not-supported = Non supporté
ui-tool-tlsaudit-strong = Fort
ui-tool-tlsaudit-acceptable = Acceptable
ui-tool-tlsaudit-weak = Faible
ui-tool-tlsaudit-findings = Constatations
ui-tool-tlsaudit-finding-ssl3 = SSL 3.0 est supporté - vulnérable à l'attaque POODLE
ui-tool-tlsaudit-finding-tls10 = TLS 1.0 est supporté - obsolète depuis 2020
ui-tool-tlsaudit-finding-tls11 = TLS 1.1 est supporté - obsolète depuis 2020
ui-tool-tlsaudit-finding-no-tls13 = TLS 1.3 n'est pas supporté - recommandé pour une sécurité optimale
ui-tool-tlsaudit-finding-weak-cipher = Suite de chiffrement faible détectée : { $suite }
ui-tool-tlsaudit-finding-no-pfs = Suites sans PFS acceptées (pas d'échange ECDHE/DHE)
ui-tool-tlsaudit-finding-all-good = Configuration TLS robuste - uniquement des protocoles et chiffrements modernes
ui-tool-tlsaudit-cert-section = Certificat
ui-tool-tlsaudit-cert-subject = Sujet
ui-tool-tlsaudit-cert-issuer = Émetteur
ui-tool-tlsaudit-cert-expires = Expiration
ui-tool-tlsaudit-cert-key-size = Taille de clé
ui-tool-tlsaudit-bits = { $bits } bits
ui-tool-tlsaudit-progress = Test de { $step }...
ui-tool-tlsaudit-progress-ciphers = Énumération des suites de chiffrement...
ui-tool-tlsaudit-copy-report = Copier le rapport
ui-tool-tlsaudit-empty-state = Saisissez un hôte et un port pour auditer sa configuration TLS
ui-tool-tlsaudit-error-host-required = L'hôte est requis
ui-tool-tlsaudit-error-invalid-port = Le port doit être compris entre 1 et 65535
ui-tool-tlsaudit-error-connection = Échec de connexion : { $error }
ui-tool-tlsaudit-report-title = Rapport d'audit SSL/TLS : { $host }:{ $port }
ui-tool-tlsaudit-report-grade = Note : { $grade }
ui-tool-tlsaudit-report-protocols = === Protocoles supportés ===
ui-tool-tlsaudit-report-ciphers = === Suites de chiffrement (TLS 1.2) ===
ui-tool-tlsaudit-report-cert = === Certificat ===
ui-tool-tlsaudit-report-findings = === Résultats ===
ui-tool-tlsaudit-help =
    Auditeur SSL/TLS

    Audite la configuration TLS d'un serveur, ses suites de chiffrement et son certificat.

    Utilisation :
    1. Saisissez un nom d'hôte ou une adresse IP cible
    2. Saisissez le port cible (par défaut : 443)
    3. Cliquez sur Auditer pour lancer le test

    Vérifications effectuées :
    - Support des protocoles (SSL 3.0, TLS 1.0, 1.1, 1.2, 1.3)
    - Énumération des suites TLS 1.2 (PFS, échange de clés, robustesse du chiffrement)
    - Inspection du certificat serveur (sujet, émetteur, validité, taille de clé)
    - Calcul de la note de sécurité (de A+ à F) et recommandations

ui-tool-dnssec-name = Vérificateur de sécurité DNS
ui-tool-dnssec-description = Vérifie les enregistrements SPF, DKIM, DMARC, CAA, DNSSEC et MX
ui-tool-dnssec-title = Vérificateur de sécurité DNS
ui-tool-dnssec-domain = Domaine
ui-tool-dnssec-domain-watermark = exemple.fr
ui-tool-dnssec-check = Vérifier
ui-tool-dnssec-cancel = Annuler
ui-tool-dnssec-copy-report = Copier le rapport
ui-tool-dnssec-empty-state = Saisissez un domaine pour analyser ses enregistrements de sécurité DNS
ui-tool-dnssec-summary-all-pass = Toutes les vérifications ont réussi ({ $passed }/{ $total })
ui-tool-dnssec-summary-good = Bonne posture de sécurité ({ $passed }/{ $total })
ui-tool-dnssec-summary-partial = Couverture de sécurité partielle ({ $passed }/{ $total })
ui-tool-dnssec-summary-bad = Faible posture de sécurité ({ $passed }/{ $total })
ui-tool-dnssec-pass = Conforme
ui-tool-dnssec-warn = Avertissement
ui-tool-dnssec-fail = Non conforme
ui-tool-dnssec-no-record = Aucun enregistrement trouvé
ui-tool-dnssec-progress = Interrogation de { $record } pour { $domain }...
ui-tool-dnssec-error-domain-required = Le domaine est requis
ui-tool-dnssec-error-invalid-domain = Format de nom de domaine non valide
ui-tool-dnssec-finding-missing = Enregistrement manquant ou non configuré
ui-tool-dnssec-finding-spf-permissive = La politique SPF utilise +all (permissif - tout serveur peut envoyer des e-mails)
ui-tool-dnssec-finding-spf-neutral = La politique SPF utilise ?all (neutre - aucune application)
ui-tool-dnssec-finding-spf-no-all = L'enregistrement SPF ne comporte aucun mécanisme all
ui-tool-dnssec-finding-spf-good = L'enregistrement SPF restreint les expéditeurs non autorisés
ui-tool-dnssec-finding-dmarc-none = La politique DMARC est none (surveillance uniquement, pas de rejet)
ui-tool-dnssec-finding-dmarc-enforced = La politique DMARC applique { $policy }
ui-tool-dnssec-finding-dmarc-pct-zero = Politique DMARC définie sur { $policy } mais pct=0 (non appliquée)
ui-tool-dnssec-finding-dkim-found = Clé publique DKIM configurée pour le sélecteur { $selector }
ui-tool-dnssec-finding-dkim-revoked = La clé DKIM pour le sélecteur { $selector } a une clé publique vide (révoquée)
ui-tool-dnssec-finding-dkim-none = Aucun enregistrement DKIM trouvé pour les sélecteurs courants
ui-tool-dnssec-finding-caa-missing = Enregistrement CAA absent (toute autorité de certification peut émettre des certificats)
ui-tool-dnssec-finding-caa-present = CAA autorise : { $issuers }
ui-tool-dnssec-finding-dnssec-present = DNSSEC activé avec signatures cryptographiques
ui-tool-dnssec-finding-dnssec-missing = DNSSEC n'est pas activé
ui-tool-dnssec-finding-mx-servers = Serveurs de messagerie : { $servers }
ui-tool-dnssec-finding-no-mx = Aucun enregistrement MX trouvé
ui-tool-dnssec-finding-error = Erreur : { $error }
ui-tool-dnssec-report-title = === Rapport de sécurité DNS pour { $domain } ===
ui-tool-dnssec-report-summary = Résumé : { $passed } / { $total } vérifications réussies
ui-tool-dnssec-help =
    Vérificateur de sécurité DNS

    Audite la posture de sécurité d'un domaine à travers six contrôles DNS clés.

    Utilisation :
    1. Saisissez un nom de domaine (ex. exemple.fr)
    2. Cliquez sur Vérifier pour interroger les enregistrements DNS

    Contrôles évalués :
    - SPF : Valide les serveurs autorisés à envoyer des e-mails
    - DKIM : Vérifie les signatures cryptographiques pour les e-mails sortants
    - DMARC : Vérifie l'alignement et les politiques de rejet (none, quarantine, reject)
    - CAA : Vérifie les autorités de certification autorisées pour l'émission TLS
    - DNSSEC : Vérifie l'authenticité cryptographique des réponses DNS
    - MX : Identifie les serveurs de réception du courrier

## L'outil de recherche de CVE hors ligne, identique à CveLookupView en C#.
ui-tool-cve-name = Recherche CVE
ui-tool-cve-description = Interroge une base de données hors ligne de vulnérabilités connues pour les logiciels d'infrastructure courants
ui-tool-cve-title = Recherche CVE
ui-tool-cve-input-placeholder = Logiciel et version (ou coller une bannière)
ui-tool-cve-btn-search = Rechercher
ui-tool-cve-summary = { $count ->
    [one] { $count } CVE trouvée pour { $query }
   *[other] { $count } CVE trouvées pour { $query }
}
ui-tool-cve-no-results = Aucune CVE connue pour cette version du logiciel.
ui-tool-cve-empty-state = Saisissez un nom de logiciel et une version pour rechercher les vulnérabilités connues
ui-tool-cve-db-info = Base hors ligne : { $cves } CVE couvrant { $products } produits
ui-tool-cve-col-affected = Versions affectées
ui-tool-cve-severity-critical = Critique
ui-tool-cve-severity-high = Élevé
ui-tool-cve-severity-medium = Moyen
ui-tool-cve-severity-low = Faible
ui-tool-cve-btn-copy = Copier le rapport
ui-tool-cve-help =
    Recherche CVE

    Interroge une base de données hors ligne de vulnérabilités connues pour les logiciels d'infrastructure courants.

    Utilisation :
    1. Saisissez un nom de logiciel et une version (ex. : "OpenSSH 8.9")
    2. Ou collez une bannière de service (ex. : "SSH-2.0-OpenSSH_8.9p1")
    3. Cliquez sur Rechercher

    L'outil détecte automatiquement les noms de logiciels et versions à partir des formats de bannières courants (en-têtes HTTP Server, bannières SSH, salutations FTP, etc.).

    Score CVSS :
    - Critique (9.0-10.0) : Correction immédiate requise
    - Élevé (7.0-8.9) : Corriger dès que possible
    - Moyen (4.0-6.9) : Planifier la correction
    - Faible (0.1-3.9) : Surveillance uniquement

    Base de données :
    Base hors ligne couvrant OpenSSH, Apache, nginx, MySQL, PostgreSQL, Redis, ProFTPD et plus. Aucune connexion Internet requise.



## L'outil Identifiants par défaut, pour le catalogue de DefaultCredentialView en C#,
## jamais pour les tentatives de connexion qu'il effectue : liste seulement, décision
## du propriétaire (2026-10-17, point 10).
ui-tool-defaultcreds-name = Identifiants par défaut
ui-tool-defaultcreds-description = Liste les identifiants par défaut connus des fournisseurs pour un produit ou un service
ui-tool-defaultcreds-title = Identifiants par défaut
ui-tool-defaultcreds-note = Cet outil liste les identifiants par défaut connus des fournisseurs. Il ne tente jamais de s'y connecter, pour éviter les verrouillages de compte et les tentatives de connexion non autorisées.
ui-tool-defaultcreds-filter-label = Service
ui-tool-defaultcreds-filter-placeholder = Filtrer par nom de service (par exemple SSH, HTTP, MySQL)
ui-tool-defaultcreds-col-service = Service
ui-tool-defaultcreds-col-port = Port
ui-tool-defaultcreds-col-user = Identifiant
ui-tool-defaultcreds-col-pass = Mot de passe
ui-tool-defaultcreds-btn-copy = Copier le rapport
ui-tool-defaultcreds-btn-export = Exporter CSV
ui-tool-defaultcreds-empty-state = Aucun service du catalogue ne correspond à ce filtre
ui-tool-defaultcreds-empty = (vide)
ui-tool-defaultcreds-count = { $count ->
    [one] { $count } identifiant par défaut listé
   *[other] { $count } identifiants par défaut listés
}
ui-tool-defaultcreds-help =
    Identifiants par défaut

    Liste les identifiants par défaut connus des fournisseurs pour un produit ou un
    service, à partir d'un catalogue intégré (SSH, Telnet, FTP, HTTP, SNMP, MySQL,
    PostgreSQL, Redis, MSSQL, VNC).

    Utilisation :
    1. Filtrez éventuellement par nom de service
    2. Copiez le rapport, ou exportez-le en CSV

    Cet outil ne tente jamais de connexion : contrairement à l'outil C# d'origine, il
    se limite à lister le catalogue, pour éviter les verrouillages de compte et les
    tentatives de connexion non autorisées.
ui-tool-secnumcloud-name = Audit SecNumCloud
ui-tool-secnumcloud-description = Audit de conformité SecNumCloud v3.2 sur le réseau, la crypto, les accès et l'exploitation
ui-tool-secnumcloud-title = Audit de conformité SecNumCloud v3.2
ui-tool-secnumcloud-scope-label = Périmètre d'audit
ui-tool-secnumcloud-scope-placeholder = Un hôte ou une adresse IP par ligne
ui-tool-secnumcloud-depth-label = Profondeur
ui-tool-secnumcloud-depth-quick = Rapide
ui-tool-secnumcloud-depth-standard = Standard
ui-tool-secnumcloud-depth-deep = Approfondi
ui-tool-secnumcloud-chk-network = Réseau (Ch.9)
ui-tool-secnumcloud-chk-crypto = Cryptographie (Ch.10)
ui-tool-secnumcloud-chk-access = Contrôle d'accès (Ch.11)
ui-tool-secnumcloud-chk-operations = Exploitation (Ch.12)
ui-tool-secnumcloud-btn-start = Lancer l'audit
ui-tool-secnumcloud-btn-stop = Arrêter
ui-tool-secnumcloud-compliance = Conformité
ui-tool-secnumcloud-status-pass = Conforme
ui-tool-secnumcloud-status-warn = Avertissement
ui-tool-secnumcloud-status-fail = Échec
ui-tool-secnumcloud-status-error = Erreur
ui-tool-secnumcloud-status-skipped = Ignoré
ui-tool-secnumcloud-ch-network = Sécurité réseau
ui-tool-secnumcloud-ch-crypto = Cryptographie
ui-tool-secnumcloud-ch-access = Contrôle d'accès
ui-tool-secnumcloud-ch-operations = Sécurité opérationnelle
ui-tool-secnumcloud-net01-name = Inventaire des ports ouverts
ui-tool-secnumcloud-net02-name = Services inutiles
ui-tool-secnumcloud-net03-name = En-têtes de sécurité HTTP
ui-tool-secnumcloud-net04-name = Cloisonnement réseau
ui-tool-secnumcloud-cry01-name = Versions des protocoles TLS
ui-tool-secnumcloud-cry02-name = Suites de chiffrement faibles
ui-tool-secnumcloud-cry03-name = Validité des certificats
ui-tool-secnumcloud-cry04-name = Robustesse des clés SSH
ui-tool-secnumcloud-acc01-name = Identifiants par défaut
ui-tool-secnumcloud-acc02-name = Signature SMB
ui-tool-secnumcloud-acc03-name = Chaînes de communauté SNMP
ui-tool-secnumcloud-ops01-name = Enregistrements SPF/DKIM/DMARC
ui-tool-secnumcloud-ops02-name = Enregistrements CAA
ui-tool-secnumcloud-ops03-name = Détection des CVE connues
ui-tool-secnumcloud-ops04-name = Inventaire des services
ui-tool-secnumcloud-btn-export-csv = Exporter en CSV
ui-tool-secnumcloud-btn-export-html = Exporter en HTML
ui-tool-secnumcloud-empty-state = Saisissez un sous-réseau ou une liste d'hôtes pour lancer un audit de conformité SecNumCloud
ui-tool-secnumcloud-error-scope = Saisissez au moins un hôte cible.
ui-tool-secnumcloud-phase-discovery = Phase 1/5 : Découverte réseau...
ui-tool-secnumcloud-phase-network = Phase 2/5 : Contrôles de sécurité réseau...
ui-tool-secnumcloud-phase-crypto = Phase 3/5 : Contrôles de cryptographie...
ui-tool-secnumcloud-phase-access = Phase 4/5 : Contrôles de gestion des accès...
ui-tool-secnumcloud-phase-operations = Phase 5/5 : Contrôles de sécurité opérationnelle...
ui-tool-secnumcloud-phase-complete = Audit terminé.
ui-tool-secnumcloud-summary = { $compliance } % de conformité ({ $pass } conforme, { $warn } avertissement, { $fail } échec)
ui-tool-secnumcloud-help =
    Audit de conformité SecNumCloud v3.2

    Audit de sécurité automatisé selon les exigences ANSSI SecNumCloud v3.2.

    Périmètre :
    Saisissez les hôtes à auditer, un nom d'hôte ou une adresse IP par ligne. Les plages CIDR ne sont pas développées et sont refusées.

    Chapitres audités :
    - Sécurité réseau (Ch.9) : Inventaire des ports, en-têtes HTTP, cloisonnement
    - Cryptographie (Ch.10) : Versions TLS, suites de chiffrement, certificats, clés SSH
    - Contrôle d'accès (Ch.11) : Identifiants par défaut, signature SMB, communautés SNMP
    - Exploitation (Ch.12) : SPF/DKIM/DMARC, CAA, recherche de CVE, inventaire des services

    Profondeur :
    - Rapide : Contrôles essentiels uniquement (~2 min)
    - Standard : Audit complet (~5 min)
    - Approfondi : Énumération étendue (~15 min)

    Exports :
    - HTML : Rapport autonome pour les auditeurs
    - CSV : Données de preuves pour outils GRC

ui-tool-smb-name = Énumérateur SMB
ui-tool-smb-description = Énumérateur de protocole SMB avec détection de signature et de version
ui-tool-smb-title = Énumérateur SMB
ui-tool-smb-host-label = Hôte
ui-tool-smb-host-placeholder = 192.168.1.100 ou nom d'hôte
ui-tool-smb-btn-enum = Énumérer
ui-tool-smb-btn-stop = Arrêter
ui-tool-smb-btn-copy = Copier le rapport
ui-tool-smb-empty-state = Saisissez un nom d'hôte ou une IP pour énumérer les informations SMB
ui-tool-smb-card-identity = Identité de l'hôte
ui-tool-smb-card-protocol = Protocole SMB
ui-tool-smb-card-findings = Constats de sécurité
ui-tool-smb-computer-name = Nom de l'ordinateur
ui-tool-smb-domain = Domaine
ui-tool-smb-dns-name = Nom DNS
ui-tool-smb-dns-domain = Domaine DNS
ui-tool-smb-forest = Forêt
ui-tool-smb-os-build = Build OS
ui-tool-smb-mac = Adresse MAC
ui-tool-smb-dialect = Dialecte SMB
ui-tool-smb-signing = Signature requise
ui-tool-smb-server-guid = GUID serveur
ui-tool-smb-system-time = Heure système
ui-tool-smb-boot-time = Heure de démarrage
ui-tool-smb-yes = Oui
ui-tool-smb-no = Non
ui-tool-smb-not-available = N/A
ui-tool-smb-signing-disabled = La signature SMB n'est pas requise - vulnérable aux attaques par relais
ui-tool-smb-signing-enabled = La signature SMB est requise
ui-tool-smb-v1-detected = Dialecte SMBv1 détecté - obsolète et vulnérable (EternalBlue)
ui-tool-smb-modern-dialect = Utilisation d'un dialecte SMB moderne : { $dialect }
ui-tool-smb-netbios-failed = Requête NetBIOS expirée (le port UDP 137 est peut-être bloqué)
ui-tool-smb-error-connection = Connexion au port 445 impossible : { $error }
ui-tool-smb-error-ntlm = Échec de la sonde NTLM : { $error }
ui-tool-smb-error-host-required = Entrez un hôte à énumérer.
ui-tool-smb-report-identity = === Identité de l'hôte ===
ui-tool-smb-report-protocol = === Protocole SMB ===
ui-tool-smb-report-findings = === Résultats de sécurité ===
ui-tool-smb-help =
    Énumérateur SMB

    Extrait l'identité de l'hôte et les informations du protocole SMB sans identifiants.

    Utilisation :
    1. Entrez un nom d'hôte ou une adresse IP
    2. Cliquez sur Énumérer

    Informations extraites :
    - Nom de l'ordinateur, domaine, noms DNS (via négociation NTLM)
    - Numéro de build de l'OS
    - Adresse MAC (via NetBIOS)
    - Version du dialecte SMB, exigences de signature
    - GUID serveur, heure système, heure de démarrage

    Vérifications de sécurité :
    - Signature SMB désactivée (risque d'attaque par relais)
    - Dialecte SMB2/SMB3 négocié

# --- Review follow-ups (#402): security tools ---
ui-tool-dnssec-report-record = Enregistrement
ui-tool-dnssec-report-detail = Détail
ui-tool-dnssec-error-nslookup-missing = nslookup est introuvable. Installez-le (bind-utils / dnsutils sous Linux) ou assurez-vous qu'il est dans le PATH.
ui-tool-secnumcloud-error-cidr = Les plages CIDR ne sont pas développées. Saisissez des hôtes individuels, un par ligne.
ui-tool-secnumcloud-clause = Clause { $clause }
ui-tool-secnumcloud-unknown = inconnu
ui-tool-secnumcloud-net01-summary = { $open } ports ouverts recensés sur { $hosts } hôtes.
ui-tool-secnumcloud-net01-evidence = { $count } ports ouverts : { $ports }
ui-tool-secnumcloud-net02-pass = Aucun service superflu ou non standard détecté.
ui-tool-secnumcloud-net02-warn = { $count } ports non standard trouvés - à examiner selon la nécessité opérationnelle.
ui-tool-secnumcloud-net02-evidence = Ports non standard : { $ports }
ui-tool-secnumcloud-net03-skipped = Aucun service HTTP/HTTPS à vérifier.
ui-tool-secnumcloud-net03-pass = Les { $count } services web possèdent les en-têtes de sécurité requis.
ui-tool-secnumcloud-net03-fail = { $missing } services web sur { $checked } n'ont pas les en-têtes de sécurité requis (HSTS, CSP, XFO, XCTO).
ui-tool-secnumcloud-net03-evidence = Port { $port } : en-têtes manquants : { $headers }
ui-tool-secnumcloud-net04-summary = { $count } sous-réseaux détectés. Vérifiez la segmentation réseau selon les recommandations de l'ANSSI.
ui-tool-secnumcloud-cry01-skipped = Aucun service TLS à vérifier.
ui-tool-secnumcloud-cry01-pass = { $count } points d'accès TLS vérifiés pour des protocoles acceptables.
ui-tool-secnumcloud-cry02-pass = Aucune suite de chiffrement faible négociée sur les points d'accès audités.
ui-tool-secnumcloud-cry03-pass = Tous les certificats capturés sont dans leur période de validité.
ui-tool-secnumcloud-cry04-skipped = Aucun service SSH à vérifier.
ui-tool-secnumcloud-cry04-fail = { $weak } serveurs SSH sur { $checked } proposent des algorithmes de clé faibles.
ui-tool-secnumcloud-cry04-pass = Les { $checked } serveurs SSH utilisent des algorithmes de clé acceptables.
ui-tool-secnumcloud-cry04-evidence = Port 22 : algorithme de clé d'hôte SSH faible (DSA) détecté
ui-tool-secnumcloud-acc01-pass = Aucun identifiant par défaut accepté sur les services audités.
ui-tool-secnumcloud-acc02-skipped = Aucun service SMB à vérifier.
ui-tool-secnumcloud-acc02-pass = { $count } services SMB évalués pour la signature des messages.
ui-tool-secnumcloud-acc03-skipped = Aucun service SNMP à vérifier.
ui-tool-secnumcloud-acc03-pass = Aucune chaîne de communauté SNMP par défaut acceptée.
ui-tool-secnumcloud-ops01-skipped = Aucun domaine public configuré dans la portée pour les vérifications DNS.
ui-tool-secnumcloud-ops02-skipped = Aucun domaine public configuré dans la portée pour les vérifications CAA.
ui-tool-secnumcloud-ops03-pass = Aucune CVE connue de gravité élevée identifiée dans les bannières de service analysées.
ui-tool-secnumcloud-ops03-fail = { $count } CVE connues identifiées sur les services analysés.
ui-tool-secnumcloud-ops03-evidence = Port { $port } : { $software } { $version } affecté par { $cve } ({ $severity })
ui-tool-secnumcloud-ops04-summary = { $count } services recensés sur { $hosts } hôtes.
ui-tool-secnumcloud-ops04-evidence = Port { $port } : { $service }
ui-tool-secnumcloud-html-doc-title = Rapport d'audit SecNumCloud
ui-tool-secnumcloud-html-heading = SecNumCloud v3.2 - Rapport d'audit de conformité
ui-tool-secnumcloud-html-clause = Clause
ui-tool-secnumcloud-html-generated-by = Généré par Heimdall
    - SMBv1 détecté (vulnérabilité EternalBlue)

ui-tool-http-name = Codes de statut HTTP
ui-tool-http-description = Guide de référence des codes de statut HTTP avec filtrage par catégorie
ui-tool-http-filter-placeholder = Rechercher par code ou description...
ui-tool-http-copy-hint = Cliquer pour copier
ui-tool-http-no-results = Aucun code correspondant
ui-tool-http-cat-1xx = 1xx Information
ui-tool-http-cat-2xx = 2xx Succès
ui-tool-http-cat-3xx = 3xx Redirection
ui-tool-http-cat-4xx = 4xx Erreur client
ui-tool-http-cat-5xx = 5xx Erreur serveur
ui-tool-http-help =
    Codes de statut HTTP

    Guide de référence des codes de statut HTTP standard.

    Catégories :
    - 1xx : Information (requête reçue, traitement en cours)
    - 2xx : Succès (action reçue, comprise et acceptée)
    - 3xx : Redirection (une action supplémentaire est nécessaire)
    - 4xx : Erreur client (syntaxe incorrecte ou requête impossible à satisfaire)
    - 5xx : Erreur serveur (le serveur n'a pas pu traiter une requête apparemment valide)

    Utilisation :
    - Saisissez un terme dans la zone de recherche pour filtrer par code ou par nom.
    - Cliquez sur une entrée pour copier son code et son intitulé dans le presse-papiers.

ui-tool-net-error-host-required = Veuillez entrer un nom d'hôte ou une adresse IP.
ui-tool-net-error-invalid-host = Nom d'hôte ou adresse IP invalide. Seuls les lettres, chiffres, points et tirets sont autorisés.
ui-tool-net-error-port = Le port doit être un nombre compris entre 1 et 65535.
ui-tool-net-error-port-range = Veuillez entrer un port ou une plage de ports.
ui-tool-net-reason-other = Échec de la connexion : { $detail }
ui-tool-net-no-value = -
ui-tool-net-millis = { $ms } ms

ui-tool-tcpping-name = TCP Ping
ui-tool-tcpping-description = Mesure de la latence de connexion TCP vers un hôte et un port
ui-tool-tcpping-host-label = Hôte
ui-tool-tcpping-host-placeholder = example.com ou 192.168.1.1
ui-tool-tcpping-port-label = Port
ui-tool-tcpping-count-label = Nombre
ui-tool-tcpping-btn-start = Démarrer
ui-tool-tcpping-btn-stop = Arrêter
ui-tool-tcpping-btn-copy = Copier les résultats
ui-tool-tcpping-summary = Min : { $min } ms, Moy : { $avg } ms, Max : { $max } ms, Perdus : { $lost }/{ $total } ({ $pct } %)
ui-tool-tcpping-summary-all-lost = Min : -, Moy : -, Max : -, Perdus : { $lost }/{ $total } ({ $pct } %)
ui-tool-tcpping-probe-ok = [{ $seq }/{ $total }] { $target } - { $ms } ms
ui-tool-tcpping-probe-failed = [{ $seq }/{ $total }] { $target } - ÉCHEC : { $reason }
ui-tool-tcpping-empty-state = Entrez un hôte et un port, puis cliquez sur Démarrer pour mesurer la latence de connexion TCP
ui-tool-tcpping-error-count = Le nombre doit être compris entre 1 et 10000.
ui-tool-tcpping-help =
    Outil TCP Ping

    Mesure la latence de la poignée de main TCP vers un hôte et un port cibles.
    Contrairement au ping ICMP, le ping TCP traverse les pare-feu qui bloquent les requêtes echo ICMP.

    Utilisation :
    1. Saisissez le nom d'hôte ou l'adresse IP cible.
    2. Saisissez le numéro de port (par défaut : 443).
    3. Indiquez le nombre de sondes à envoyer (par défaut : 10, au plus 10000).
    4. Cliquez sur Démarrer pour lancer la mesure ; Arrêter y met fin aussitôt.

    Le nom d'hôte est résolu une seule fois, avant la première sonde : les temps affichés
    sont ceux de la seule poignée de main. Les sondes sont espacées d'une seconde, et
    chacune dispose de cinq secondes pour obtenir une réponse.

ui-tool-wol-name = Wake-on-LAN
ui-tool-wol-description = Envoi de paquets magiques pour allumer des machines sur le réseau
ui-tool-wol-mac-label = Adresse MAC
ui-tool-wol-mac-placeholder = AA:BB:CC:DD:EE:FF
ui-tool-wol-broadcast-label = Adresse de diffusion
ui-tool-wol-port-label = Port UDP
ui-tool-wol-btn-send = Envoyer le Wake-on-LAN
ui-tool-wol-status-sending = Envoi du paquet magique...
ui-tool-wol-status-sent = Paquet magique envoyé à { $mac } via { $dest }
ui-tool-wol-status-error = Échec de l'envoi du paquet : { $error }
ui-tool-wol-error-invalid-mac = Adresse MAC invalide. Format : AA:BB:CC:DD:EE:FF ou AABBCCDDEEFF.
ui-tool-wol-error-invalid-broadcast = Adresse de diffusion invalide. Saisissez une adresse IPv4 ou IPv6.
ui-tool-wol-help =
    Wake-on-LAN

    Envoie des paquets magiques UDP en diffusion pour réveiller des machines du réseau.

    Prérequis :
    - Le Wake-on-LAN doit être activé dans le BIOS/UEFI de la machine cible.
    - La carte réseau doit prendre en charge le WOL et rester alimentée.
    - Le commutateur ou le routeur doit laisser passer les paquets UDP de diffusion.

    Utilisation :
    1. Saisissez l'adresse MAC de la machine cible.
    2. Ajustez si nécessaire l'adresse de diffusion, une adresse IP (par défaut : 255.255.255.255).
    3. Ajustez si nécessaire le port UDP, de 1 à 65535 (par défaut : 9).
    4. Cliquez sur Envoyer le Wake-on-LAN.

ui-tool-whois-name = Recherche WHOIS
ui-tool-whois-description = Interrogation des serveurs WHOIS pour les domaines et les adresses IP
ui-tool-whois-domain-placeholder = example.com ou 1.1.1.1
ui-tool-whois-btn-lookup = Rechercher
ui-tool-whois-btn-copy = Copier le rapport
ui-tool-whois-empty-state = Saisissez un domaine ou une adresse IP pour interroger les données d'enregistrement WHOIS
ui-tool-whois-status-querying = Interrogation du serveur WHOIS...
ui-tool-whois-status-complete = Requête terminée en { $elapsed } ms
ui-tool-whois-result-header = Résultats WHOIS pour { $domain }
ui-tool-whois-truncated = La réponse dépassait { $kib } Kio : seul son début est affiché.
ui-tool-whois-error-required = Veuillez entrer un nom de domaine ou une adresse IP.
ui-tool-whois-error-invalid = Nom de domaine ou adresse IP non valide.
ui-tool-whois-error-timeout = La requête WHOIS a expiré. Le serveur WHOIS est peut-être inaccessible ou limite les requêtes.
ui-tool-whois-error-failed = La requête WHOIS a échoué : { $reason }. Réessayez ou utilisez un autre domaine/IP.
ui-tool-whois-help =
    Recherche WHOIS

    Interroge directement un serveur WHOIS sur le port TCP 43 pour obtenir les données
    d'enregistrement d'un domaine : registrar, dates, serveurs de noms.

    Fonctionnement :
    - Le serveur est choisi selon le TLD du domaine dans une liste intégrée (com, net, org,
      fr, io, dev et d'autres) ; whois.iana.org est interrogé pour tout autre TLD et pour
      une adresse IP.
    - La réponse est affichée telle que le serveur l'envoie : un renvoi vers un autre
      serveur n'est pas suivi.
    - La recherche entière dispose de 10 secondes, et 64 Kio de la réponse au plus sont conservés.

ui-tool-banner-name = Capture de bannières
ui-tool-banner-description = Sondage des ports réseau pour capturer les bannières et versions des services
ui-tool-banner-host-label = Hôte cible
ui-tool-banner-host-placeholder = example.com ou 192.168.1.1
ui-tool-banner-ports-label = Ports
ui-tool-banner-ports-placeholder = 21,22,25,80,443,3306,8080
ui-tool-banner-btn-grab = Capturer
ui-tool-banner-btn-stop = Arrêter
ui-tool-banner-btn-copy = Copier les résultats
ui-tool-banner-btn-export = Exporter en CSV
ui-tool-banner-col-port = Port
ui-tool-banner-col-service = Service
ui-tool-banner-col-time = Temps
ui-tool-banner-col-banner = Bannière
ui-tool-banner-preset-common = Usuels
ui-tool-banner-preset-web = Web
ui-tool-banner-preset-mail = Messagerie
ui-tool-banner-preset-database = Bases de données
ui-tool-banner-empty-state = Saisissez un hôte et des ports pour capturer les bannières des services
ui-tool-banner-progress = { $completed } / { $total } ports sondés
ui-tool-banner-help =
    Capture de bannières

    Sonde les ports réseau ouverts et lit leur bannière d'accueil pour identifier les
    logiciels en service, les démons et leurs numéros de version.

    Fonctionnalités :
    - Listes et plages de ports personnalisables (ex. : 80,443,8000-8005)
    - Préréglages pour les ports usuels, Web, de messagerie et de bases de données
    - 20 ports sondés à la fois, chaque résultat affiché dès qu'il arrive ; Arrêter met fin au sondage aussitôt
    - Identification automatique des services (OpenSSH, Apache, nginx, MySQL, Redis, etc.)
    - Export des résultats en CSV ou en texte mis en forme
    Fonctionnalites :
    - Liste et plages de ports personnalisables (ex : 80,443,8000-8005)
    - Profils predefinis pour ports Usuels, Web, Mail et Bases de donnees
    - Empreinte automatique des services (OpenSSH, Apache, nginx, MySQL, Redis, etc.)
    - Export des resultats en CSV ou texte formate

ui-tool-ping-name = Moniteur Ping
ui-tool-ping-description = Moniteur de ping ICMP continu avec statistiques de latence en direct
ui-tool-ping-host = Hôte cible
ui-tool-ping-host-watermark = example.com ou 192.168.1.1
ui-tool-ping-timeout = Délai d'attente (ms)
ui-tool-ping-timeout-watermark = 2000
ui-tool-ping-count = Nombre
ui-tool-ping-count-watermark = 4
ui-tool-ping-start = Démarrer le ping
ui-tool-ping-stop = Arrêter
ui-tool-ping-min = Min
ui-tool-ping-avg = Moy
ui-tool-ping-max = Max
ui-tool-ping-jitter = Gigue
ui-tool-ping-loss = Perte de paquets
ui-tool-ping-sent = Envoyés/Reçus
ui-tool-ping-results-title = Historique des sondes
ui-tool-ping-copy-results = Copier les résultats
ui-tool-ping-export-csv = Exporter CSV
ui-tool-ping-status-timeout = Délai dépassé
ui-tool-ping-error-host-required = Veuillez saisir un hôte ou une adresse IP cible.
ui-tool-ping-error-timeout-invalid = Le délai d'attente doit être compris entre 100 ms et 30000 ms.
ui-tool-ping-error-count-invalid = Le nombre doit être compris entre 0 et 100000.
ui-tool-ping-help =
    Moniteur Ping

    Envoie des paquets ICMP Echo Request à un hôte cible pour tester la latence aller-retour,
    la perte de paquets et la stabilité du réseau.

    Fonctionnalités :
    - Délai d'attente et nombre de paquets configurables (0 pour surveillance continue)
    - Statistiques en temps réel : minimum, moyenne, maximum, gigue et pourcentage de perte
    - Tableau d'historique avec numéro de séquence, horodatages, TTL et badges d'état
    - Export des résultats vers texte formaté ou feuille de calcul CSV

ui-tool-dns-name = Recherche DNS
ui-tool-dns-description = Interroger les types d'enregistrements DNS sur des serveurs standards ou personnalisés
ui-tool-dns-host = Domaine ou nom d'hôte
ui-tool-dns-host-watermark = example.com
ui-tool-dns-type = Type d'enregistrement
ui-tool-dns-server = Serveur DNS
ui-tool-dns-lookup = Interroger DNS
ui-tool-dns-searching = Requête en cours...
ui-tool-dns-copy = Copier la sortie
ui-tool-dns-results-title = Résultats DNS pour { $host } ({ $record_type }, { $time } ms)
ui-tool-dns-no-records = Aucun enregistrement retourné.
ui-tool-dns-error-host-required = Veuillez saisir un nom de domaine.
ui-tool-dns-help =
    Recherche DNS

    Interroge les serveurs DNS (Domain Name System) pour examiner les enregistrements d'un domaine ou d'une IP.

    Fonctionnalités :
    - Types d'enregistrements standards : A, AAAA, MX, CNAME, TXT, NS, PTR, SOA, SRV, ANY
    - Préréglages de serveurs publics (Cloudflare, Google, Quad9, OpenDNS)
    - Mesure du temps de réponse et suivi de la chaîne d'alias
    - Sortie formatée et claire avec copie en un clic dans le presse-papiers

ui-tool-dnsbatch-name = Résolution DNS en masse
ui-tool-dnsbatch-description = Résoudre plusieurs noms d'hôtes simultanément en adresses IPv4 et IPv6
ui-tool-dnsbatch-hostnames-label = Noms d'hôtes à résoudre (un par ligne ou séparés par des virgules)
ui-tool-dnsbatch-resolve = Résoudre le lot
ui-tool-dnsbatch-copy-csv = Exporter CSV
ui-tool-dnsbatch-total = Total
ui-tool-dnsbatch-resolved = Résolu
ui-tool-dnsbatch-failed = Échoué
ui-tool-dnsbatch-results-title = Résultats du lot
ui-tool-dnsbatch-status-resolved = Résolu
ui-tool-dnsbatch-status-failed = Échoué
ui-tool-dnsbatch-status-pending = En attente
ui-tool-dnsbatch-error-required = Veuillez saisir au moins un nom d'hôte à résoudre.
ui-tool-dnsbatch-help =
    Résolution DNS en masse

    Résout de grands lots de noms d'hôtes et de domaines en parallèle vers leurs adresses IPv4 et IPv6.

    Fonctionnalités :
    - Saisie flexible multiligne ou séparée par des virgules
    - Résolution asynchrone concurrente
    - Récapitulatif ventilé des requêtes résolues et échouées
    - Export en feuille de calcul CSV avec statut et durée de réponse

ui-tool-portscan-name = Scanner de ports
ui-tool-portscan-description = Scanner les ports TCP cibles pour détecter les services ouverts et le filtrage de pare-feu
ui-tool-portscan-host = Hôte cible
ui-tool-portscan-host-watermark = example.com ou 192.168.1.1
ui-tool-portscan-ports = Plage de ports
ui-tool-portscan-ports-watermark = 20-80,443
ui-tool-portscan-timeout = Délai d'attente (ms)
ui-tool-portscan-timeout-watermark = 2000
ui-tool-portscan-start = Démarrer le scan
ui-tool-portscan-stop = Arrêter
ui-tool-portscan-export-csv = Exporter CSV
ui-tool-portscan-total = Total
ui-tool-portscan-open = Ouvert
ui-tool-portscan-closed = Fermé
ui-tool-portscan-filtered = Filtré
ui-tool-portscan-results-title = Résultats du scan de ports
ui-tool-portscan-status-open = Ouvert
ui-tool-portscan-status-closed = Fermé
ui-tool-portscan-status-filtered = Filtré
ui-tool-portscan-error-host-required = Veuillez saisir un hôte cible.
ui-tool-portscan-error-ports-invalid = Veuillez saisir un numéro ou une plage de ports valide (ex : 80,443,8000-8080).
ui-tool-portscan-help =
    Scanner de ports

    Sonde les ports TCP sur les hôtes cibles pour cartographier les services réseau ouverts, les règles de pare-feu,
    et les ports filtrés.

    Fonctionnalités :
    - Préréglages pour les services courants (SSH, HTTP, HTTPS, RDP, MySQL, etc.)
    - Prise en charge des numéros, listes et plages de ports personnalisés (ex : 1-1024)
    - Classification en temps réel en Ouvert, Fermé et Filtré
    - Statistiques de synthèse et export CSV

ui-tool-headers-name = Analyseur d'en-têtes HTTP
ui-tool-headers-description = Inspecter les en-têtes HTTP, détecter les fuites d'informations et noter la sécurité
ui-tool-headers-url = URL cible
ui-tool-headers-url-watermark = https://example.com
ui-tool-headers-analyze = Analyser les en-têtes
ui-tool-headers-analyzing = Analyse en cours...
ui-tool-headers-copy-report = Copier le rapport complet
ui-tool-headers-sec-headers-title = Évaluation des en-têtes de sécurité
ui-tool-headers-disclosure-title = Divulgation d'informations
ui-tool-headers-raw-title = En-têtes bruts de réponse
ui-tool-headers-status-pass = CONFORME
ui-tool-headers-status-warn = ATTENTION
ui-tool-headers-status-fail = NON CONFORME
ui-tool-headers-status-clean = PROPRE
ui-tool-headers-status-exposed = EXPOSÉ
ui-tool-headers-missing = (manquant)
ui-tool-headers-not-exposed = (non exposé)
ui-tool-headers-grade-aplus = Remarquable : tous les en-têtes de sécurité clés appliqués selon les meilleures pratiques.
ui-tool-headers-grade-a = Excellent : en-têtes de sécurité modernes et robustes en place.
ui-tool-headers-grade-b = Bon : omissions mineures dans les en-têtes de défense navigateur modernes.
ui-tool-headers-grade-c = Moyen : plusieurs en-têtes de sécurité fondamentaux sont manquants.
ui-tool-headers-grade-d = Médiocre : la plupart des protections standard sont manquantes.
ui-tool-headers-grade-f = Critique : aucun en-tête de sécurité moderne détecté.
ui-tool-headers-rec-hsts = Recommandation : activer Strict-Transport-Security avec max-age=31536000 et includeSubDomains.
ui-tool-headers-rec-csp = Recommandation : définir une Content-Security-Policy restrictive pour prévenir XSS et injection de code.
ui-tool-headers-rec-xfo = Recommandation : définir X-Frame-Options à DENY ou SAMEORIGIN pour empêcher le clickjacking.
ui-tool-headers-rec-xcto = Recommandation : définir X-Content-Type-Options à nosniff pour éviter les attaques de reniflage MIME.
ui-tool-headers-rec-referrer = Recommandation : configurer Referrer-Policy à strict-origin-when-cross-origin.
ui-tool-headers-rec-permissions = Recommandation : configurer Permissions-Policy pour restreindre les API sensibles du navigateur.
ui-tool-headers-rec-xxss = Recommandation : définir X-XSS-Protection à 0 ou s'appuyer sur Content-Security-Policy.
ui-tool-headers-rec-cookie = Recommandation : définir Secure, HttpOnly et SameSite=Lax/Strict sur les cookies.
ui-tool-headers-rec-disclosure = Recommandation : désactiver ou masquer l'en-tête serveur pour cacher les versions logicielles internes.
ui-tool-headers-reason-hsts-http = Les navigateurs ignorent cet en-tête en HTTP simple : analysez l'adresse HTTPS.
ui-tool-headers-reason-hsts-max-age-invalid = max-age est absent, répété ou n'est pas un nombre : les navigateurs ignorent cet en-tête.
ui-tool-headers-reason-hsts-max-age-zero = max-age=0 demande aux navigateurs d'oublier la politique HTTPS uniquement.
ui-tool-headers-reason-hsts-max-age-short = max-age est inférieur à { $seconds } secondes (six mois).
ui-tool-headers-reason-csp-empty = La politique est vide et ne restreint rien.
ui-tool-headers-reason-csp-no-script = Ni script-src ni default-src : les scripts ne sont pas restreints.
ui-tool-headers-reason-csp-unsafe = Les scripts autorisent 'unsafe-inline' sans nonce ni empreinte, ou 'unsafe-eval'.
ui-tool-headers-reason-xfo-allow-from = Les navigateurs actuels ignorent ALLOW-FROM : utilisez la directive frame-ancestors de la CSP.
ui-tool-headers-reason-xfo-invalid = La valeur n'est ni DENY ni SAMEORIGIN : les navigateurs ignorent cet en-tête.
ui-tool-headers-reason-xcto-invalid = La valeur n'est pas nosniff : les navigateurs ignorent cet en-tête.
ui-tool-headers-reason-referrer-invalid = Aucune politique connue : les navigateurs ignorent cet en-tête.
ui-tool-headers-reason-referrer-unsafe = Cette politique envoie l'URL complète aux autres sites.
ui-tool-headers-reason-permissions-empty = La politique est vide et ne restreint rien.
ui-tool-headers-error-url-required = Veuillez saisir une URL cible.
ui-tool-headers-error-invalid-url = Veuillez saisir une URL HTTP ou HTTPS valide.
ui-tool-headers-help =
    Analyseur d'en-têtes HTTP

    Récupère les en-têtes de réponse HTTP et évalue la posture de sécurité, les politiques de défense
    du navigateur et la divulgation d'informations du serveur.

    En-têtes évalués :
    - Strict-Transport-Security (HSTS)
    - Content-Security-Policy (CSP)
    - X-Frame-Options (Défense contre le clickjacking)
    - X-Content-Type-Options (nosniff)
    - Referrer-Policy
    - Permissions-Policy
    - Drapeaux de sécurité des cookies (Secure, HttpOnly, SameSite)
    - Divulgation d'informations serveur (Server, X-Powered-By, X-AspNet-Version)

## Inspecteur de certificat SSL/TLS
ui-tool-cert-name = Inspecteur de certificat SSL/TLS
ui-tool-cert-description = Inspecter les chaînes de certificats SSL/TLS, les SAN, la validité et les propriétés cryptographiques
ui-tool-cert-host = Hôte
ui-tool-cert-host-watermark = example.com ou IP
ui-tool-cert-ports = Ports
ui-tool-cert-ports-watermark = 443, 8443
ui-tool-cert-profiles = Profil d'analyse
ui-tool-cert-profile-quick = Rapide ({ $count } ports)
ui-tool-cert-profile-extended = Étendu ({ $count } ports)
ui-tool-cert-inspect = Inspecter
ui-tool-cert-copy-details = Copier les détails
ui-tool-cert-export-csv = Exporter en CSV
ui-tool-cert-subject = Sujet
ui-tool-cert-issuer = Émetteur
ui-tool-cert-valid-from = Valide depuis
ui-tool-cert-valid-to = Valide jusqu'au
ui-tool-cert-days-remaining = Jours restants
ui-tool-cert-tls-version = Version TLS
ui-tool-cert-key-size = Taille de clé
ui-tool-cert-hostname-match = Correspondance du nom d'hôte
ui-tool-cert-sans = SAN
ui-tool-cert-match-yes = Conforme
ui-tool-cert-match-no = Non conforme
ui-tool-cert-status-valid = Valide
ui-tool-cert-status-warning = Expire bientôt
ui-tool-cert-status-expired = Expiré
ui-tool-cert-error-host-required = Veuillez saisir le nom de l'hôte cible.

## Traceroute TCP
ui-tool-tcptrace-name = Traceroute TCP
ui-tool-tcptrace-description = Découvrir les sauts réseau et leur latence sur le chemin vers une destination
ui-tool-tcptrace-host = Hôte cible
ui-tool-tcptrace-host-watermark = example.com ou IP
ui-tool-tcptrace-max-hops = Sauts maximum
ui-tool-tcptrace-start = Démarrer le traçage
ui-tool-tcptrace-stop = Arrêter
ui-tool-tcptrace-export-csv = Exporter en CSV
ui-tool-tcptrace-col-hop = Saut
ui-tool-tcptrace-col-address = Adresse
ui-tool-tcptrace-col-hostname = Nom d'hôte
ui-tool-tcptrace-col-latency = Latence
ui-tool-tcptrace-col-status = Statut
ui-tool-tcptrace-status-reply = Réponse
ui-tool-tcptrace-status-timeout = Délai dépassé
ui-tool-tcptrace-status-destination = Destination
ui-tool-tcptrace-error-host-required = Veuillez saisir un hôte cible.

## Explorateur MIB SNMP
ui-tool-snmpwalk-name = Explorateur MIB SNMP
ui-tool-snmpwalk-description = Interroger et parcourir les arbres MIB SNMPv2c pour les métriques système et réseau
ui-tool-snmpwalk-host = Hôte cible
ui-tool-snmpwalk-host-watermark = 192.168.1.1
ui-tool-snmpwalk-community = Communauté
ui-tool-snmpwalk-oid = OID racine
ui-tool-snmpwalk-start = Démarrer le parcours
ui-tool-snmpwalk-stop = Arrêter
ui-tool-snmpwalk-export-csv = Exporter en CSV
ui-tool-snmpwalk-col-oid = OID
ui-tool-snmpwalk-col-name = Nom
ui-tool-snmpwalk-col-type = Type
ui-tool-snmpwalk-col-value = Valeur
ui-tool-snmpwalk-error-host-required = Veuillez saisir l'hôte SNMP cible.

## Moniteur de table ARP
ui-tool-arpmon-name = Moniteur de table ARP
ui-tool-arpmon-description = Inspecter le cache ARP local, identifier les fabricants et détecter les anomalies réseau
ui-tool-arpmon-scan = Analyser maintenant
ui-tool-arpmon-scanning = Analyse en cours...
ui-tool-arpmon-copy-csv = Exporter en CSV
ui-tool-arpmon-col-ip = Adresse IP
ui-tool-arpmon-col-mac = Adresse MAC
ui-tool-arpmon-col-vendor = Fabricant
ui-tool-arpmon-col-status = Statut
ui-tool-arpmon-col-first-seen = Première détection
ui-tool-arpmon-col-last-seen = Dernière détection
ui-tool-arpmon-status-new = Nouveau
ui-tool-arpmon-status-stable = Stable
ui-tool-arpmon-status-changed = Modifié
ui-tool-arpmon-status-conflict = Conflit

## Testeur de sortie pare-feu
ui-tool-fwtest-name = Testeur de sortie pare-feu
ui-tool-fwtest-description = Tester la connectivité sortante à travers les ports du pare-feu, avec mesure de latence
ui-tool-fwtest-hosts = Hôtes cibles
ui-tool-fwtest-hosts-watermark = Un hôte par ligne, ou séparés par des virgules (ex. : 1.1.1.1, 8.8.8.8)
ui-tool-fwtest-ports = Ports à tester
ui-tool-fwtest-ports-watermark = 80,443,8080
ui-tool-fwtest-start = Tester la sortie
ui-tool-fwtest-stop = Arrêter
ui-tool-fwtest-export-csv = Exporter la matrice en CSV
ui-tool-fwtest-total = Sondes au total
ui-tool-fwtest-open = Ouvert
ui-tool-fwtest-closed = Fermé
ui-tool-fwtest-timeout = Filtré
ui-tool-fwtest-error-hosts-required = Veuillez saisir au moins un hôte cible.

## Cartographie réseau
ui-tool-netmap-name = Cartographie réseau
ui-tool-netmap-description = Découvrir les hôtes actifs d'un sous-réseau et leurs services ouverts, et classer leurs rôles
ui-tool-netmap-subnet = Sous-réseau (CIDR)
ui-tool-netmap-subnet-watermark = 192.168.1.0/24
ui-tool-netmap-discover = Découvrir le réseau
ui-tool-netmap-export-csv = Exporter en CSV
ui-tool-netmap-hosts-scanned = Hôtes analysés
ui-tool-netmap-hosts-up = Hôtes actifs
ui-tool-netmap-ports-open = Ports ouverts
ui-tool-netmap-col-ip = Adresse IP
ui-tool-netmap-col-hostname = Nom d'hôte
ui-tool-netmap-col-mac = MAC
ui-tool-netmap-col-vendor = Fabricant
ui-tool-netmap-col-role = Rôle
ui-tool-netmap-col-ports = Ports ouverts
ui-tool-netmap-col-latency = Latence
ui-tool-netmap-error-cidr-required = Veuillez saisir un sous-réseau valide en notation CIDR (ex. : 192.168.1.0/24).

## Ports ouverts
ui-tool-openports-name = Ports ouverts
ui-tool-openports-description = Inspecter les ports TCP et UDP actifs et les sockets avec identification des processus
ui-tool-openports-title = Ports ouverts
ui-tool-openports-help =
    Liste les ports TCP et UDP ouverts de cet ordinateur avec le processus qui détient chacun.

    Sous Windows, les tables viennent de Windows PowerShell (Get-NetTCPConnection, Get-NetUDPEndpoint, Get-Process) ; sous Linux, de ss, qui ne nomme les processus des autres utilisateurs qu'à root.

    Utilisez le filtre pour rechercher par processus, adresse, port ou état.
ui-tool-openports-refresh = Actualiser
ui-tool-openports-scanning = Actualisation...
ui-tool-openports-copy-csv = Exporter CSV
ui-tool-openports-filter-placeholder = Filtrer par processus, adresse, port, état...
ui-tool-openports-status = { $count ->
    [one] { $count } connexion à { $time }
   *[other] { $count } connexions à { $time }
}
ui-tool-openports-empty-state = Cliquez sur Actualiser pour lister les ports ouverts et les connexions.
ui-tool-openports-col-proto = Protocole
ui-tool-openports-col-local-addr = Adresse locale
ui-tool-openports-col-local-port = Port local
ui-tool-openports-col-remote-addr = Adresse distante
ui-tool-openports-col-remote-port = Port distant
ui-tool-openports-col-state = État
ui-tool-openports-col-pid = PID
ui-tool-openports-col-process = Processus

## Interfaces réseau
ui-tool-netif-name = Interfaces réseau
ui-tool-netif-description = Lister les cartes réseau avec vitesse, adresse MAC, IPv4, passerelle et statut DHCP
ui-tool-netif-title = Interfaces réseau
ui-tool-netif-help =
    Liste les interfaces réseau de cet ordinateur telles que le système les décrit : type, état opérationnel, vitesse de liaison, adresse MAC, adresse IPv4, masque, passerelle et DHCP.

    Une vitesse que le système ne connaît pas est affichée comme inconnue, jamais devinée. Sous Linux, les informations viennent de ip et de /sys/class/net.
ui-tool-netif-refresh = Actualiser
ui-tool-netif-scanning = Actualisation...
ui-tool-netif-copy-csv = Exporter CSV
ui-tool-netif-status = { $count ->
    [one] { $count } interface à { $time }
   *[other] { $count } interfaces à { $time }
}
ui-tool-netif-empty-state = Cliquez sur Actualiser pour inspecter les interfaces réseau.
ui-tool-netif-col-name = Nom
ui-tool-netif-col-type = Type
ui-tool-netif-col-status = Statut
ui-tool-netif-col-speed = Vitesse
ui-tool-netif-col-mac = MAC
ui-tool-netif-col-ipv4 = IPv4
ui-tool-netif-col-subnet = Masque
ui-tool-netif-col-gateway = Passerelle
ui-tool-netif-col-dhcp = DHCP

## Table de routage
ui-tool-routes-name = Table de routage
ui-tool-routes-description = Afficher la table de routage IPv4 active avec destinations, masques, passerelles et métriques
ui-tool-routes-title = Table de routage
ui-tool-routes-help =
    Affiche la table de routage IPv4 active : destination, masque, passerelle, adresse de l'interface et métrique de chaque route.

    Sous Windows, la table vient de Get-NetRoute, la métrique incluant celle de l'interface comme route print l'affiche ; sous Linux, de ip route.
ui-tool-routes-refresh = Actualiser
ui-tool-routes-scanning = Actualisation...
ui-tool-routes-copy-csv = Exporter CSV
ui-tool-routes-status = { $count ->
    [one] { $count } route à { $time }
   *[other] { $count } routes à { $time }
}
ui-tool-routes-empty-state = Cliquez sur Actualiser pour inspecter les routes IPv4.
ui-tool-routes-col-dest = Destination
ui-tool-routes-col-mask = Masque
ui-tool-routes-col-gateway = Passerelle
ui-tool-routes-col-interface = Interface
ui-tool-routes-col-metric = Métrique

## Réseaux Wi-Fi
ui-tool-wifi-name = Réseaux Wi-Fi
ui-tool-wifi-description = Scanner les réseaux sans fil proches avec BSSID, puissance du signal, canal et chiffrement
ui-tool-wifi-title = Réseaux Wi-Fi
ui-tool-wifi-help =
    Scanne les réseaux Wi-Fi à proximité via netsh wlan show networks mode=bssid, ou nmcli sous Linux.

    Affiche SSID, BSSID (MAC), puissance du signal, canal, authentification, chiffrement et type de radio. Windows peut exiger l'autorisation de localisation pour lister les points d'accès.
ui-tool-wifi-scan = Scanner
ui-tool-wifi-scanning = Scan en cours...
ui-tool-wifi-copy-csv = Exporter CSV
ui-tool-wifi-status = { $count ->
    [one] { $count } réseau à { $time }
   *[other] { $count } réseaux à { $time }
}
ui-tool-wifi-empty-state = Cliquez sur Scanner pour détecter les réseaux Wi-Fi à portée.
ui-tool-wifi-col-ssid = SSID
ui-tool-wifi-col-bssid = BSSID
ui-tool-wifi-col-signal = Signal
ui-tool-wifi-col-channel = Canal
ui-tool-wifi-col-auth = Authentification
ui-tool-wifi-col-encryption = Chiffrement
ui-tool-wifi-col-radio = Type de radio

## Éditeur du fichier hosts
ui-tool-hosts-name = Éditeur du fichier hosts
ui-tool-hosts-description = Visualiser, basculer et modifier les associations de domaines du fichier hosts
ui-tool-hosts-title = Éditeur du fichier hosts
ui-tool-hosts-help =
    Édite le fichier hosts du système avec validation syntaxique.

    Le fichier est lu à l'ouverture de l'outil. Les entrées peuvent être activées, désactivées, supprimées ou ajoutées ; les commentaires, les alias et les fins de ligne sont conservés tels quels. L'enregistrement nécessite les privilèges administrateur, garde une copie de sauvegarde datée à côté du fichier, et est refusé si un autre programme a modifié le fichier depuis sa lecture.
ui-tool-hosts-reload = Recharger
ui-tool-hosts-save = Enregistrer
ui-tool-hosts-copy-all = Copier hosts
ui-tool-hosts-btn-add = Ajouter
ui-tool-hosts-btn-delete = Supprimer
ui-tool-hosts-invalid-ip = Format d'adresse IP invalide.
ui-tool-hosts-missing-host = Le nom d'hôte ne peut pas être vide.
ui-tool-hosts-invalid-host = Nom d'hôte invalide : utilisez des lettres, des chiffres, des points, des tirets ou des soulignés, sans espace ni '#'.
ui-tool-hosts-invalid-comment = Un commentaire ne peut pas contenir de saut de ligne ni de caractère de contrôle.
ui-tool-hosts-loaded = { $count } entrées chargées depuis { $path }
ui-tool-hosts-save-success = Fichier hosts enregistré. Copie de sauvegarde de la version précédente : { $backup }
ui-tool-hosts-unchanged = Aucune modification à enregistrer : le fichier hosts est resté tel quel.
ui-tool-hosts-save-failed = Échec de l'enregistrement du fichier hosts : { $error }
ui-tool-hosts-error-not-found = Fichier hosts introuvable.
ui-tool-hosts-error-read-denied = Accès refusé (lecture). Exécutez en tant qu'administrateur.
ui-tool-hosts-error-read = Le fichier hosts n'a pas pu être lu : { $error }
ui-tool-hosts-error-admin-required = Impossible d'écrire le fichier hosts. Lancez Heimdall en tant qu'administrateur pour enregistrer les modifications.
ui-tool-hosts-error-changed = Un autre programme a modifié le fichier hosts depuis sa lecture. Rien n'a été écrit : rechargez-le, puis refaites vos modifications.
ui-tool-hosts-error-backup = La copie de sauvegarde n'a pas pu être écrite, le fichier hosts n'a donc pas été modifié : { $error }
ui-tool-hosts-empty-state = Aucune entrée trouvée dans le fichier hosts.
ui-tool-hosts-loading-hint = Lecture du fichier hosts...
ui-tool-hosts-col-enabled = Actif
ui-tool-hosts-col-ip = Adresse IP
ui-tool-hosts-col-hostname = Nom d'hôte
ui-tool-hosts-col-comment = Commentaire
ui-tool-hosts-col-action = Action
ui-tool-hosts-col-names = Noms d'hôte (séparés par des espaces)
ui-tool-hosts-btn-edit = Modifier
ui-tool-hosts-btn-apply = Appliquer
ui-tool-hosts-btn-cancel = Annuler
ui-tool-hosts-btn-delete-selected = Supprimer la sélection
ui-tool-hosts-select-all = Sélectionner toutes les entrées
ui-tool-hosts-selected-count = { $count ->
    [one] { $count } entrée sélectionnée
   *[other] { $count } entrées sélectionnées
}
ui-tool-hosts-copy-ip = Copier l'adresse IP
ui-tool-hosts-copy-hostname = Copier le nom d'hôte
ui-tool-hosts-last-modified = Dernière modification : { $time }

## Tableau de bord des services
ui-tool-services-name = Tableau de bord des services
ui-tool-services-description = Surveiller les services d'arrière-plan Windows et contrôler leur état
ui-tool-services-title = Tableau de bord des services
ui-tool-services-help =
    Surveille le statut des services Windows avec commandes de démarrage et d'arrêt.

    Visualisez les services actifs, leur statut et leur type de démarrage. Démarrez, arrêtez ou redémarrez un service : Windows demande d'abord les droits administrateur, et l'action ne porte que sur un service de la liste qui vient d'être lue.
ui-tool-services-refresh = Actualiser
ui-tool-services-scanning = Actualisation...
ui-tool-services-copy-csv = Exporter CSV
ui-tool-services-filter-placeholder = Filtrer par nom de service ou nom d'affichage...
ui-tool-services-running-only = En cours uniquement
ui-tool-services-total = Total :
ui-tool-services-running-label = En cours :
ui-tool-services-stopped-label = Arrêtés :
ui-tool-services-col-name = Nom du service
ui-tool-services-col-display-name = Nom d'affichage
ui-tool-services-col-status = Statut
ui-tool-services-col-start-type = Type de démarrage
ui-tool-services-col-actions = Actions
ui-tool-services-act-start = Démarrer
ui-tool-services-act-stop = Arrêter
ui-tool-services-act-restart = Redémarrer
ui-tool-services-empty-state = Cliquez sur Actualiser pour inspecter les services Windows.
ui-tool-services-action-success = Action sur le service terminée avec succès.
ui-tool-services-error-invalid-name = Ce nom de service contient des caractères qui ne peuvent pas être transmis sans risque au système : aucune action n'a été lancée.
ui-tool-services-error-unknown-name = Ce service ne figure pas dans la liste qui vient d'être lue : actualisez la liste, puis réessayez.
ui-tool-services-error-no-powershell = Windows PowerShell est introuvable dans le dossier système.
ui-tool-services-error-no-systemctl = systemctl est introuvable dans /usr/bin et /bin.
ui-tool-services-error-not-started = La commande n'a pas pu être lancée : { $error }
ui-tool-services-error-timeout = La commande ne s'est pas terminée à temps et a été arrêtée. Une action peut encore aboutir : actualisez la liste pour le vérifier.
ui-tool-services-error-not-elevated = L'action n'a pas été lancée : la demande de droits administrateur a été refusée, ou Windows n'a pas pu la démarrer.
ui-tool-services-error-action-failed = L'action sur le service a échoué (code de sortie { $code }).
ui-tool-services-error-action-failed-detail = L'action sur le service a échoué : { $error }
ui-tool-services-error-list-failed = Échec du chargement des services : { $error }
ui-tool-services-exit-code-none = aucun

## Outils réseau, troisième lot : suites de la revue
ui-tool-cert-stop = Arrêter
ui-tool-cert-col-port = Port
ui-tool-cert-sig-alg = Algorithme de signature
ui-tool-cert-serial = Numéro de série
ui-tool-cert-sha256 = Empreinte SHA-256
ui-tool-cert-chain = Chaîne
ui-tool-cert-chain-element = { $subject } (expire le { $expiry })
ui-tool-cert-port-title = Port { $port } : { $name }
ui-tool-cert-port-header = === Port { $port } ===
ui-tool-cert-detail-host = Hôte
ui-tool-cert-key-bits = { $bits } bits
ui-tool-cert-days-left =
    { $count ->
        [one] { $count } jour restant
       *[other] { $count } jours restants
    }
ui-tool-cert-days-expired =
    { $count ->
        [one] expiré depuis { $count } jour
       *[other] expiré depuis { $count } jours
    }
ui-tool-cert-progress = Ports sondés : { $done } sur { $total }
ui-tool-cert-scan-found =
    { $certificates ->
        [one] { $certificates } certificat trouvé
       *[other] { $certificates } certificats trouvés
    } sur { $ports ->
        [one] { $ports } port sondé.
       *[other] { $ports } ports sondés.
    }
ui-tool-cert-stopped = Inspection arrêtée : { $found }
ui-tool-cert-error-invalid-host = Saisissez une adresse IP ou un nom d'hôte.
ui-tool-cert-error-invalid-port = Saisissez des ports de 1 à 65535, séparés par des virgules, ou des plages comme 8000-8010.
ui-tool-cert-error-too-many-ports = { $count } ports saisis : { $max } au plus sont inspectés à la fois.
ui-tool-cert-error-connection = La connexion à { $host }:{ $port } a échoué : { $detail }
ui-tool-cert-error-handshake = La négociation TLS avec { $host }:{ $port } a échoué : { $detail }
ui-tool-cert-error-legacy = { $host }:{ $port } ne propose qu'une version obsolète de TLS, et son certificat n'a pas pu être lu dans sa réponse : { $detail }
ui-tool-cert-error-legacy-version = { $host }:{ $port } ne propose que { $version }, antérieur à TLS 1.0 : son certificat n'est pas lu.
ui-tool-cert-legacy-alert = le serveur a refusé la connexion (alerte TLS { $code }).
ui-tool-cert-legacy-not-tls = la réponse n'est pas du TLS.
ui-tool-cert-legacy-too-large = la réponse dépasse la taille de toute chaîne de certificats.
ui-tool-cert-legacy-malformed = la réponse est mal formée.
ui-tool-cert-legacy-no-certificate = le serveur n'a présenté aucun certificat.
ui-tool-cert-legacy-closed = le serveur a fermé la connexion avant de présenter son certificat.
ui-tool-cert-legacy-title = TLS obsolète : { $version }, { $cipher }
ui-tool-cert-legacy-warning = Ce serveur a répondu avec une version de TLS ou une suite de chiffrement obsolète, que Heimdall ne négocie pas. Son certificat a été lu en clair dans une négociation jamais achevée : rien ne prouve que le serveur détient la clé privée correspondante. Il est affiché à titre d'information et n'est jamais considéré comme fiable.
ui-tool-cert-legacy-audit = Auditer avec l'Auditeur TLS
ui-tool-cert-cipher-suite = Suite de chiffrement
ui-tool-cert-col-read = Lecture
ui-tool-cert-read-verified = Négociation TLS vérifiée
ui-tool-cert-read-unverified = En clair dans une réponse TLS obsolète, non vérifié ({ $cipher })
ui-tool-cert-error-no-certificate = Le serveur n'a présenté aucun certificat.
ui-tool-cert-error-unreadable = Le certificat présenté est illisible.
ui-tool-cert-error-timeout = Le serveur n'a pas répondu à temps.
ui-tool-tcptrace-error-invalid-host = Saisissez une adresse IP ou un nom d'hôte.
ui-tool-tcptrace-error-not-found = Le programme traceroute du système est introuvable.
ui-tool-tcptrace-error-failed = Le traçage n'a pas pu s'exécuter.
ui-tool-tcptrace-stopped = Traçage arrêté.
ui-tool-tcptrace-timed-out = Le traçage a trop duré et a été interrompu.
ui-tool-tcptrace-no-hops = Aucun saut n'a répondu.
ui-tool-tcptrace-hop-count =
    { $count ->
        [one] { $count } saut
       *[other] { $count } sauts
    }
ui-tool-snmpwalk-community-watermark = Vide : { $community }
ui-tool-snmpwalk-stopped = Parcours arrêté.
ui-tool-snmpwalk-agent-silent = L'agent ne répond plus : les entrées lues sont conservées.
ui-tool-snmpwalk-entry-count =
    { $count ->
        [one] { $count } entrée
       *[other] { $count } entrées
    }
ui-tool-snmpwalk-error-invalid-host = Saisissez une adresse IP ou un nom d'hôte.
ui-tool-snmpwalk-error-invalid-oid = Saisissez un OID numérique comme 1.3.6.1.2.1.1.
ui-tool-snmpwalk-error-community-too-long = La communauté est limitée à { $max } octets.
ui-tool-snmpwalk-error-unresolved = Le nom d'hôte n'a pas pu être résolu.
ui-tool-snmpwalk-error-socket = La requête SNMP n'a pas pu être envoyée.
ui-tool-snmpwalk-error-no-response = L'agent n'a pas répondu : vérifiez l'hôte, la communauté et que SNMP v2c est activé.
ui-tool-snmpwalk-error-failed = Le parcours n'a pas pu s'exécuter.
ui-tool-arpmon-error-read = La table ARP n'a pas pu être lue.
ui-tool-arpmon-vendor-unknown = Inconnu
ui-tool-fwtest-stopped = Test arrêté.
ui-tool-fwtest-error-invalid-host = Aucun des hôtes saisis n'est une adresse IP ou un nom d'hôte.
ui-tool-fwtest-error-no-ports = Saisissez au moins un port de 1 à 65535.
ui-tool-fwtest-error-failed = Le test n'a pas pu s'exécuter.
ui-tool-fwtest-limited = Les hôtes invalides sont écartés, et un test sonde au plus { $max_hosts } hôtes et { $max_ports } ports.
ui-tool-fwtest-progress = Sondes effectuées : { $done } sur { $total }
ui-tool-fwtest-unresolved = Non résolu
ui-tool-fwtest-unresolved-count =
    { $count ->
        [one] { $count } sonde n'a pas pu résoudre son nom d'hôte.
       *[other] { $count } sondes n'ont pas pu résoudre leur nom d'hôte.
    }
ui-tool-fwtest-col-host = Hôte
ui-tool-fwtest-col-port = Port
ui-tool-fwtest-col-status = Statut
ui-tool-fwtest-col-latency = Latence
ui-tool-fwtest-latency-ms = { $ms } ms
ui-tool-fwtest-cell-open-ms = { $label } ({ $ms } ms)
ui-tool-fwtest-not-tested = Non testé
ui-tool-netmap-stop = Arrêter
ui-tool-netmap-stopped = Découverte arrêtée.
ui-tool-netmap-error-failed = La découverte n'a pas pu s'exécuter.
ui-tool-netmap-error-prefix = Saisissez un sous-réseau de /{ $min } à /{ $max }.
ui-tool-netmap-warning-large = { $count } hôtes à sonder : cela peut prendre longtemps.
ui-tool-netmap-scan-anyway = Analyser quand même
ui-tool-netmap-progress = Hôtes sondés : { $done } sur { $total }
ui-tool-netmap-latency-ms = { $ms } ms
ui-tool-netmap-role-directory = Contrôleur de domaine / annuaire
ui-tool-netmap-role-database = Serveur de bases de données
ui-tool-netmap-role-mail = Serveur de messagerie
ui-tool-netmap-role-file = Serveur de fichiers / stockage
ui-tool-netmap-role-web = Serveur web
ui-tool-netmap-role-remote = Hôte d'administration à distance
ui-tool-netmap-role-appliance = Passerelle / équipement réseau
ui-tool-netmap-role-active = Noeud actif
ui-tool-netmap-role-no-ports = Hôte (aucun port ouvert)
ui-tool-mac-vendor-randomized = Privée (MAC aléatoire)
ui-tool-arpmon-status-gone = Disparu
ui-tool-arpmon-start = Démarrer
ui-tool-arpmon-stop = Arrêter
ui-tool-arpmon-interval = Rafraîchissement
ui-tool-arpmon-interval-secs = { $secs } s
ui-tool-arpmon-dismiss-alert = Fermer
ui-tool-arpmon-alert-title = Anomalie ARP détectée
ui-tool-arpmon-changes-title = Changements de la table ARP
ui-tool-arpmon-alert-mac-changed = L'adresse MAC de { $ip } est passée de { $previous } à { $current } (possible usurpation ARP)
ui-tool-arpmon-alert-conflict = { $ip } est revendiquée par plusieurs adresses MAC à la fois : { $macs } (possible usurpation ARP)
ui-tool-arpmon-alert-new =
    { $count ->
        [one] { $count } nouvel hôte : { $list }
       *[other] { $count } nouveaux hôtes : { $list }
    }
ui-tool-arpmon-alert-gone =
    { $count ->
        [one] { $count } hôte disparu : { $list }
       *[other] { $count } hôtes disparus : { $list }
    }
ui-tool-arpmon-alert-more = { $list } et { $more } autres
ui-tool-arpmon-mac-was = auparavant { $mac }
ui-tool-arpmon-mac-also = aussi { $macs }
ui-tool-arpmon-total =
    { $count ->
        [one] { $count } entrée
       *[other] { $count } entrées
    }
ui-tool-arpmon-last-refresh = Dernier rafraîchissement : { $time }
ui-tool-arpmon-empty = Cliquez sur Démarrer pour surveiller la table ARP locale, ou sur Analyser maintenant pour la lire une fois.
ui-tool-arpmon-empty-running = Surveillance active : en attente d'entrées dans la table ARP locale.
ui-tool-netmap-reverse-dns = DNS inverse
ui-tool-netmap-use-kb = Utiliser la base de connaissances
ui-tool-netmap-kb-empty = Base de connaissances : vide
ui-tool-netmap-kb-stats =
    { $count ->
        [one] Base de connaissances : { $count } hôte, mise à jour { $updated }
       *[other] Base de connaissances : { $count } hôtes, mise à jour { $updated }
    }
ui-tool-netmap-kb-just-now = à l'instant
ui-tool-netmap-kb-minutes-ago = il y a { $count } min
ui-tool-netmap-kb-hours-ago = il y a { $count } h
ui-tool-netmap-kb-days-ago =
    { $count ->
        [one] il y a { $count } jour
       *[other] il y a { $count } jours
    }
ui-tool-netmap-kb-clear = Effacer la base de connaissances
ui-tool-netmap-kb-keep = Conserver
ui-tool-netmap-kb-clear-confirm = Effacer toutes les données d'hôtes accumulées dans la base de connaissances ?
ui-tool-netmap-kb-cleared = Base de connaissances effacée.
ui-tool-netmap-error-kb-clear = La base de connaissances n'a pas pu être effacée : { $reason }
ui-tool-netmap-saved = Analyse enregistrée dans l'historique.
ui-tool-netmap-history-entry = { $time } - { $subnet }
ui-tool-netmap-compare-with = Comparer avec
ui-tool-netmap-compare = Comparer
ui-tool-netmap-diff-none = Aucun changement depuis cette analyse.
ui-tool-netmap-diff-new =
    { $count ->
        [one] { $count } nouvel hôte
       *[other] { $count } nouveaux hôtes
    }
ui-tool-netmap-diff-removed =
    { $count ->
        [one] { $count } hôte disparu
       *[other] { $count } hôtes disparus
    }
ui-tool-netmap-diff-changed =
    { $count ->
        [one] { $count } hôte modifié
       *[other] { $count } hôtes modifiés
    }
ui-tool-netmap-error-history-read = Cette analyse de l'historique n'a pas pu être lue.
ui-tool-netmap-naming =
    { $count ->
        [one] Lecture de la table ARP et recherche du nom de { $count } hôte...
       *[other] Lecture de la table ARP et recherche des noms de { $count } hôtes...
    }
ui-tool-snmpwalk-test-communities = Tester les communautés courantes
ui-tool-snmpwalk-testing = Test en cours...
ui-tool-snmpwalk-test-stopped = Test des communautés arrêté.
ui-tool-snmpwalk-accepted-count =
    { $count ->
        [one] { $count } communauté acceptée
       *[other] { $count } communautés acceptées
    }
ui-tool-snmpwalk-col-status = Statut
ui-tool-snmpwalk-col-sys-name = sysName
ui-tool-snmpwalk-community-accepted = Acceptée
ui-tool-snmpwalk-community-rejected = Pas de réponse
## Visionneuse de logs
ui-tool-logview-name = Visionneuse de logs
ui-tool-logview-description = Afficher un fichier journal, filtrer ses lignes et suivre ce qui s'y ajoute
ui-tool-logview-title = Visionneuse de logs / Tail
ui-tool-logview-help =
    Visionneuse de logs

    Affiche les 10 000 dernières lignes d'un fichier journal, lues dans l'encodage choisi.

    Utilisation :
    Parcourez pour choisir un fichier ou saisissez son chemin puis appuyez sur Entrée. Filtrer garde les lignes qui contiennent le texte saisi, ou qui correspondent à l'expression régulière quand Expression régulière est coché. Tail lit ce qui s'ajoute toutes les demi-secondes ; un fichier qui raccourcit est relu depuis son début. Seule la fin d'un très gros fichier est lue.
ui-tool-logview-path-placeholder = Chemin du fichier journal
ui-tool-logview-browse = Parcourir
ui-tool-logview-open = Ouvrir
ui-tool-logview-tail = Tail
ui-tool-logview-filter-placeholder = Texte de filtre
ui-tool-logview-filter = Filtrer
ui-tool-logview-clear = Effacer
ui-tool-logview-case-sensitive = Sensible à la casse
ui-tool-logview-regex = Expression régulière
ui-tool-logview-encoding = Encodage :
ui-tool-logview-encoding-utf8 = UTF-8
ui-tool-logview-encoding-ascii = ASCII
ui-tool-logview-encoding-utf16 = UTF-16
ui-tool-logview-encoding-windows1252 = Windows-1252
ui-tool-logview-auto-scroll = Suivre la dernière ligne
ui-tool-logview-tail-active = Tail actif
ui-tool-logview-copy = Copier
ui-tool-logview-file-filter = Fichiers journaux
ui-tool-logview-empty-state = Parcourez ou saisissez le chemin d'un fichier journal pour l'afficher
ui-tool-logview-no-matches = Aucune ligne de log ne correspond au filtre actuel
ui-tool-logview-stats =
    { $total ->
        [one] { $shown } / { $total } ligne | { $size } Ko | Dernière mise à jour : { $time }
       *[other] { $shown } / { $total } lignes | { $size } Ko | Dernière mise à jour : { $time }
    }
ui-tool-logview-error-not-found = Fichier introuvable.
ui-tool-logview-error-open = Impossible d'ouvrir le fichier : { $error }
ui-tool-logview-error-regex = Expression régulière invalide.

## Gestionnaire de tâches cron
ui-tool-cronjob-name = Gestionnaire de tâches cron
ui-tool-cronjob-description = Analyser une crontab et lister les tâches planifiées de Windows
ui-tool-cronjob-title = Gestionnaire de tâches cron
ui-tool-cronjob-help =
    Gestionnaire de tâches cron

    Lit une crontab et les tâches planifiées de cet ordinateur.

    Utilisation :
    Collez la sortie de crontab -l puis appuyez sur Analyser : chaque planification est listée avec sa commande, sa prochaine exécution et ce qu'elle fait ; sélectionnez-en une pour voir ses cinq prochaines exécutions. Tâches Windows liste les tâches planifiées de cet ordinateur avec leur statut et leur dernier résultat. Copier place la liste affichée dans le presse-papiers, une ligne par entrée.
ui-tool-cronjob-tab-paste = Coller crontab
ui-tool-cronjob-tab-tasks = Tâches Windows
ui-tool-cronjob-paste-instructions = Collez la sortie brute de crontab (depuis crontab -l via un terminal SSH) pour analyser les entrées.
ui-tool-cronjob-paste-placeholder = Contenu de la crontab
ui-tool-cronjob-parse = Analyser
ui-tool-cronjob-clear = Effacer
ui-tool-cronjob-refresh = Actualiser
ui-tool-cronjob-copy = Copier
ui-tool-cronjob-col-schedule = Planification
ui-tool-cronjob-col-command = Commande
ui-tool-cronjob-col-next-run = Prochaine exécution
ui-tool-cronjob-col-description = Description
ui-tool-cronjob-col-task-name = Nom de la tâche
ui-tool-cronjob-col-status = Statut
ui-tool-cronjob-col-last-run = Dernière exécution
ui-tool-cronjob-col-last-result = Dernier résultat
ui-tool-cronjob-detail-header = Détails de l'entrée
ui-tool-cronjob-detail-schedule = Planification : { $schedule }
ui-tool-cronjob-detail-command = Commande : { $command }
ui-tool-cronjob-detail-next-runs = Prochaines exécutions :
ui-tool-cronjob-empty-state = Collez une crontab pour lister ses planifications
ui-tool-cronjob-tasks-empty = Actualisez pour lister les tâches planifiées de Windows
ui-tool-cronjob-loading = Chargement des tâches planifiées...
ui-tool-cronjob-error-empty = Veuillez coller le contenu de crontab à analyser.
ui-tool-cronjob-error-no-entry = Aucune entrée crontab valide trouvée.
ui-tool-cronjob-invalid-schedule = Planification invalide : { $reason }
ui-tool-cronjob-error-timeout = Le chargement des tâches a expiré.
ui-tool-cronjob-error-failed = Échec du chargement des tâches : { $error }
ui-tool-cronjob-error-unsupported = Les tâches planifiées ne peuvent être listées que sous Windows.
ui-tool-cronjob-error-no-system-folder = Windows n'a pas donné son dossier système : les tâches planifiées ne peuvent pas être listées.
ui-tool-cronjob-status-parsed =
    { $count ->
        [one] { $count } entrée analysée.
       *[other] { $count } entrées analysées.
    }
ui-tool-cronjob-status-tasks =
    { $count ->
        [one] { $count } tâche planifiée trouvée.
       *[other] { $count } tâches planifiées trouvées.
    }

## Notes
ui-tool-notes-name = Notes
ui-tool-notes-description = Notes de travail en Markdown gardées en fichiers, avec étiquettes et export HTML
ui-tool-notes-title = Notes de travail
ui-tool-notes-help =
    Notes

    Des notes de travail en Markdown, chacune dans son fichier du dossier des notes des données de Heimdall.

    Utilisation :
    Nouveau crée une note ; la première ligne qui commence par "# " est son titre, et une ligne comme "> tags: réseau, prod" donne ses étiquettes. Une note est enregistrée peu après chaque modification, quand une autre est sélectionnée et quand l'onglet se ferme. La recherche trouve une note par son texte, son titre ou son nom de fichier. La note affichée se copie en Markdown ou en page HTML.
ui-tool-notes-new-title = Note de travail
ui-tool-notes-search-placeholder = Rechercher dans les notes
ui-tool-notes-new = Nouveau
ui-tool-notes-count =
    { $count ->
        [one] { $count } note
       *[other] { $count } notes
    }
ui-tool-notes-delete = Supprimer
ui-tool-notes-delete-confirm = Supprimer la note "{ $title }" ?
ui-tool-notes-cancel = Annuler
ui-tool-notes-copy-markdown = Copier en Markdown
ui-tool-notes-copy-html = Copier en HTML
ui-tool-notes-content-placeholder = Écrivez la note en Markdown
ui-tool-notes-empty-title = Aucune note sélectionnée
ui-tool-notes-empty-body = Créez une note ou sélectionnez-en une dans la liste.
ui-tool-notes-error-no-folder = Ce système ne donne à Heimdall aucun dossier de données où garder les notes.
ui-tool-notes-error-list = Impossible de lire les notes : { $error }
ui-tool-notes-error-save = Impossible d'enregistrer la note : { $error }
ui-tool-notes-unsaved-blocked = Cette note n'a pas pu être enregistrée. Réessayez, ou abandonnez ses modifications pour continuer.
ui-tool-notes-retry = Réessayer
ui-tool-notes-discard = Abandonner les modifications

## Éditeur de diagrammes
ui-tool-diagram-name = Éditeur de diagrammes
ui-tool-diagram-description = Éditer le texte source d'un diagramme (Mermaid, Graphviz, PlantUML ou ASCII) à partir de modèles
ui-tool-diagram-title = Éditeur de diagrammes
ui-tool-diagram-help =
    Éditeur de diagrammes

    Un éditeur de texte pour la source d'un diagramme, en Mermaid, Graphviz DOT, PlantUML ou art ASCII. Il ne dessine pas le diagramme : collez la source dans un outil ou une page qui affiche son format.

    Utilisation :
    Partez d'un modèle ou ouvrez un fichier texte. Enregistrer écrit dans le fichier courant ; Enregistrer sous en demande un nouveau. Un modèle ou un fichier ne remplace jamais des modifications non enregistrées sans le demander d'abord.
ui-tool-diagram-templates = Modèles
ui-tool-diagram-template-perimeter = Périmètre et DMZ
ui-tool-diagram-template-cloud = VPC cloud
ui-tool-diagram-template-ascii = Topologie réseau en ASCII
ui-tool-diagram-template-ad = Active Directory
ui-tool-diagram-format-mermaid = Mermaid
ui-tool-diagram-format-dot = Graphviz DOT
ui-tool-diagram-format-plantuml = PlantUML
ui-tool-diagram-format-ascii = Art ASCII
ui-tool-diagram-open = Ouvrir
ui-tool-diagram-save = Enregistrer
ui-tool-diagram-save-as = Enregistrer sous
ui-tool-diagram-copy = Copier
ui-tool-diagram-untitled = Diagramme sans titre
ui-tool-diagram-modified = { $name } (modifié)
ui-tool-diagram-confirm-replace = Ce diagramme contient des modifications non enregistrées. Le remplacer quand même ?
ui-tool-diagram-replace = Remplacer
ui-tool-diagram-keep = Garder mes modifications
ui-tool-diagram-placeholder = Source du diagramme
ui-tool-diagram-open-title = Ouvrir un diagramme
ui-tool-diagram-save-title = Enregistrer le diagramme
ui-tool-diagram-default-file-name = diagramme
ui-tool-diagram-saved = Enregistré dans { $path }
ui-tool-diagram-error-open = Impossible d'ouvrir le diagramme : { $error }
ui-tool-diagram-error-save = Impossible d'enregistrer le diagramme : { $error }
ui-tool-diagram-error-too-large = le fichier est trop volumineux pour être ouvert

## Bibliothèque de commandes
ui-tool-cmdlib-name = Bibliothèque de commandes
ui-tool-cmdlib-description = Commandes d'administration, de réseau et de sécurité avec leurs valeurs renseignées
ui-tool-cmdlib-title = Bibliothèque de commandes
ui-tool-cmdlib-help =
    Bibliothèque de commandes

    Des commandes courantes d'administration, de réseau et de sécurité, prêtes à copier.

    Utilisation :
    Choisissez une plateforme ou saisissez un mot pour trouver une commande. Saisissez ses valeurs : la commande les montre telle qu'elle sera copiée. Une valeur laissée vide prend l'exemple affiché.
ui-tool-cmdlib-search-placeholder = Rechercher des commandes
ui-tool-cmdlib-platform-all = Toutes les plateformes
ui-tool-cmdlib-platform-linux = Linux
ui-tool-cmdlib-platform-windows = Windows
ui-tool-cmdlib-platform-cross = Multiplateforme
ui-tool-cmdlib-category-network = Réseau
ui-tool-cmdlib-category-system = Système
ui-tool-cmdlib-category-services = Services
ui-tool-cmdlib-category-security = Sécurité
ui-tool-cmdlib-category-diagnostics = Diagnostic
ui-tool-cmdlib-param-host = Hôte
ui-tool-cmdlib-param-port = Port
ui-tool-cmdlib-param-local-port = Port local
ui-tool-cmdlib-param-port-range = Ports
ui-tool-cmdlib-param-service = Service
ui-tool-cmdlib-copy = Copier
ui-tool-cmdlib-empty = Aucune commande ne correspond aux filtres.
ui-tool-cmdlib-count =
    { $count ->
        [one] { $count } commande
       *[other] { $count } commandes
    }
ui-tool-cmdlib-snippet-listening-ports = Lister les sockets à l'écoute
ui-tool-cmdlib-snippet-listening-ports-description = Affiche les sockets TCP et UDP à l'écoute, avec leur processus.
ui-tool-cmdlib-snippet-kill-port = Arrêter le processus d'un port
ui-tool-cmdlib-snippet-kill-port-description = Termine le processus à l'écoute sur un port TCP.
ui-tool-cmdlib-snippet-journal-follow = Suivre le journal d'un service
ui-tool-cmdlib-snippet-journal-follow-description = Affiche le journal systemd d'un service au fur et à mesure.
ui-tool-cmdlib-snippet-cert-expiry = Dates du certificat TLS distant
ui-tool-cmdlib-snippet-cert-expiry-description = Se connecte en TLS et affiche le sujet et les dates de validité du certificat du serveur.
ui-tool-cmdlib-snippet-test-connection = Tester un port TCP
ui-tool-cmdlib-snippet-test-connection-description = Vérifie avec PowerShell qu'un hôte répond sur un port TCP.
ui-tool-cmdlib-snippet-port-owner = Trouver le processus d'un port
ui-tool-cmdlib-snippet-port-owner-description = Affiche l'état et le processus propriétaire d'un port TCP local.
ui-tool-cmdlib-snippet-restart-service = Redémarrer un service Windows
ui-tool-cmdlib-snippet-restart-service-description = Arrête puis démarre un service Windows.
ui-tool-cmdlib-snippet-flush-dns = Vider le cache DNS
ui-tool-cmdlib-snippet-flush-dns-description = Vide le cache du client DNS de Windows.
ui-tool-cmdlib-snippet-mtr = Qualité du chemin réseau
ui-tool-cmdlib-snippet-mtr-description = Envoie 50 sondes à chaque saut pour mesurer les pertes et la latence.
ui-tool-cmdlib-snippet-nmap = Versions des services avec Nmap
ui-tool-cmdlib-snippet-nmap-description = Interroge les ports ouverts pour connaître le service et sa version.
ui-tool-cmdlib-platform-both = Les deux
ui-tool-cmdlib-template-windows = PowerShell
ui-tool-cmdlib-template-linux = Shell
ui-tool-cmdlib-section-mine = Mes commandes
ui-tool-cmdlib-section-catalog = Catalogue
ui-tool-cmdlib-new = Nouvelle commande
ui-tool-cmdlib-edit = Modifier
ui-tool-cmdlib-delete = Supprimer
ui-tool-cmdlib-delete-keep = Garder
ui-tool-cmdlib-delete-question = Supprimer { $title } ?
ui-tool-cmdlib-send = Envoyer
ui-tool-cmdlib-send-to = Envoyer vers :
ui-tool-cmdlib-send-none = Ouvrez un onglet de terminal connecté pour lui envoyer une commande. Rien n'est envoyé sans votre accord.
ui-tool-cmdlib-library-unreadable = Le fichier de vos commandes n'a pas pu être lu. Il est laissé tel quel et ne peut pas être modifié ici.
ui-tool-cmdlib-library-unsaved = Vos commandes n'ont pas pu être enregistrées. Le changement n'a pas été fait.
ui-tool-cmdlib-editor-new = Nouvelle commande
ui-tool-cmdlib-editor-edit = Modifier la commande
ui-tool-cmdlib-field-title = Titre
ui-tool-cmdlib-field-description = Description
ui-tool-cmdlib-field-category = Catégorie
ui-tool-cmdlib-field-tags = Étiquettes (séparées par des virgules)
ui-tool-cmdlib-field-platform = Plateformes
ui-tool-cmdlib-field-level = Niveau
ui-tool-cmdlib-field-windows-pattern = Commande PowerShell
ui-tool-cmdlib-field-linux-pattern = Commande shell
ui-tool-cmdlib-field-pattern-hint = Écrivez chaque valeur à demander entre accolades, par exemple ping -c 3 {"{"}host{"}"}. Une seule ligne.
ui-tool-cmdlib-save = Enregistrer
ui-tool-cmdlib-cancel = Annuler
ui-tool-cmdlib-level-info = Info
ui-tool-cmdlib-level-run = Exécution
ui-tool-cmdlib-level-dangerous = Dangereuse
ui-tool-cmdlib-error-title-missing = Donnez un titre à la commande.
ui-tool-cmdlib-error-title-long = Le titre est trop long.
ui-tool-cmdlib-error-category-missing = Donnez une catégorie à la commande.
ui-tool-cmdlib-error-category-long = La catégorie est trop longue.
ui-tool-cmdlib-error-description-long = La description est trop longue.
ui-tool-cmdlib-error-pattern-missing = Écrivez la commande de chaque plateforme choisie.
ui-tool-cmdlib-error-pattern-long = Une commande est trop longue.
ui-tool-cmdlib-error-pattern-multiline = Une commande doit tenir sur une seule ligne.
ui-tool-cmdlib-error-too-many-parameters = La commande demande trop de valeurs.
ui-tool-cmdlib-error-too-many-tags = Il y a trop d'étiquettes.
ui-tool-cmdlib-error-gone = Cette commande n'existe plus.
ui-tool-cmdlib-error-empty-pattern = Cette commande n'a rien à exécuter.
ui-tool-cmdlib-error-command-long = La commande avec ses valeurs est trop longue.
ui-tool-cmdlib-value-required = { $label } est obligatoire.
ui-tool-cmdlib-value-not-offered = { $label } ne fait pas partie des valeurs proposées.
ui-tool-cmdlib-value-not-integer = { $label } doit être un nombre entier.
ui-tool-cmdlib-value-not-number = { $label } doit être un nombre.
ui-tool-cmdlib-value-hostname = { $label } doit être un nom d'hôte.
ui-tool-cmdlib-value-ip = { $label } doit être une adresse IP.
ui-tool-cmdlib-value-drive = { $label } doit être une seule lettre de lecteur.
ui-tool-cmdlib-value-path = { $label } doit être un chemin.
ui-tool-cmdlib-value-long = { $label } dépasse { $max } caractères.
ui-tool-cmdlib-value-dangerous = { $label } contient un caractère que le shell lirait comme de la syntaxe.
ui-tool-cmdlib-value-control = { $label } contient un saut de ligne ou un autre caractère de contrôle.
ui-tool-cmdlib-import = Importer
ui-tool-cmdlib-export = Exporter
ui-tool-cmdlib-import-title = Importer des commandes
ui-tool-cmdlib-export-title = Exporter les commandes
ui-tool-cmdlib-import-done = Importées : { $imported }, mises à jour : { $updated }, ignorées : { $skipped }.
ui-tool-cmdlib-export-done = Commandes exportées.
ui-tool-cmdlib-transfer-too-large = Le fichier est trop volumineux.
ui-tool-cmdlib-transfer-format = Le fichier n'est pas un export de la bibliothèque de commandes.
ui-tool-cmdlib-transfer-failed = Le fichier n'a pas pu être utilisé : { $detail }
ui-tool-cmdlib-favorite = Favori
ui-tool-cmdlib-unfavorite = Retirer des favoris
ui-tool-cmdlib-favorites-only = Favoris seulement
ui-tool-cmdlib-duplicate = Dupliquer
ui-tool-cmdlib-duplicate-title = Copie de { $title }
ui-tool-cmdlib-history = Historique
ui-tool-cmdlib-history-note = Les valeurs saisies ne sont jamais conservées : un rejeu demande la commande telle qu'elle est écrite maintenant, et la question qui suit la montre en entier.
ui-tool-cmdlib-history-empty = Rien n'a encore été envoyé.
ui-tool-cmdlib-history-sent = Envoyée à { $target } le { $when }
ui-tool-cmdlib-history-replay = Rejouer
ui-tool-cmdlib-history-clear = Effacer l'historique
ui-tool-cmdlib-history-clear-question = Effacer tout l'historique ? Les favoris restent.
ui-tool-cmdlib-history-clear-confirm = Effacer
ui-tool-cmdlib-replay-gone = La commande de cette entrée n'existe plus.
ui-tool-cmdlib-replay-blocked = Cette entrée ne peut pas être envoyée maintenant : ouvrez un onglet de terminal connecté et renseignez les valeurs demandées.
ui-tool-cmdlib-activity-unsaved = Les favoris et l'historique n'ont pas pu être enregistrés.
ui-tool-cmdlib-broadcast-to = Diffuser vers :
ui-tool-cmdlib-broadcast = Diffuser
ui-tool-cmdlib-broadcast-all = Tout sélectionner
ui-tool-cmdlib-broadcast-none = Tout désélectionner
ui-tool-cmdlib-broadcast-sent =
    { $count ->
        [one] Envoyée à { $count } terminal.
       *[other] Envoyée à { $count } terminaux.
    }
ui-tool-cmdlib-broadcast-partial = Envoyée à { $delivered } terminaux sur { $total }. Non atteints : { $names }
## Network tools batch 2: review follow-ups
ui-tool-ping-error-host-invalid = Nom d'hôte ou adresse IP invalide. Seuls les lettres, chiffres, points et tirets sont autorisés.
ui-tool-ping-latency = { $ms } ms
ui-tool-ping-detail-unreachable = Hôte de destination injoignable
ui-tool-ping-detail-unresolved = Hôte introuvable
ui-tool-ping-detail-no-program = Le programme ping du système est introuvable.
ui-tool-ping-detail-failed = Échec du ping : { $reason }
ui-tool-ping-copy-header = --- Statistiques ping pour { $host } ---
ui-tool-ping-copy-counts = Envoyés = { $sent }, Reçus = { $received }, Perdus = { $lost } ({ $loss } % de perte)
ui-tool-ping-copy-times = Min = { $min } ms, Max = { $max } ms, Moy = { $avg } ms, Gigue = { $jitter } ms
ui-tool-ping-copy-line = [{ $time }] { $host } : { $status }, TTL={ $ttl }
ui-tool-ping-loss-value = { $loss } %
ui-tool-ping-shown-last = Affichage des { $shown } dernières sondes sur { $total }. Les exports les contiennent toutes.
ui-tool-ping-seq = #{ $seq }
ui-tool-ping-ttl = TTL : { $ttl }
ui-tool-portscan-error-host-invalid = Nom d'hôte ou adresse IP invalide. Seuls les lettres, chiffres, points et tirets sont autorisés.
ui-tool-portscan-error-timeout-invalid = Le délai d'attente doit être compris entre 100 ms et 10000 ms.
ui-tool-portscan-error-unresolved = Le nom d'hôte n'a pas pu être résolu.
ui-tool-portscan-error-unresolved-reason = Le nom d'hôte n'a pas pu être résolu : { $reason }
ui-tool-portscan-large-warning = Vous êtes sur le point d'analyser { $count } ports. Cela peut prendre beaucoup de temps. Continuer ?
ui-tool-portscan-confirm = Analyser quand même
ui-tool-portscan-progress = { $done } ports analysés sur { $total }
ui-tool-portscan-shown-open = Affichage des { $shown } ports ouverts sur { $total } analysés. L'export CSV contient tous les ports.
ui-tool-portscan-latency = { $ms } ms
ui-tool-portscan-no-latency = -
ui-tool-portscan-port = Port { $port }
ui-tool-dns-server-preset = { $name } ({ $address })
ui-tool-dns-server-system = Résolveur par défaut du système
ui-tool-dns-error-timeout = La requête a expiré après 5 secondes. Vérifiez que le serveur DNS est joignable.
ui-tool-dns-error-no-program = Le programme nslookup du système est introuvable.
ui-tool-dns-error-failed = Échec de la recherche : { $reason }. Vérifiez le nom de domaine et essayez un autre type d'enregistrement.
ui-tool-dns-error-host-invalid = Nom d'hôte ou adresse IP invalide. Seuls les lettres, chiffres, points et tirets sont autorisés.
ui-tool-dns-stop = Arrêter
ui-tool-dnsbatch-no-address = Aucune adresse trouvée
ui-tool-dnsbatch-timed-out = Délai dépassé
ui-tool-dnsbatch-error-too-many = { $count } noms d'hôtes saisis : { $max } au plus peuvent être résolus à la fois.
ui-tool-dnsbatch-stop = Arrêter
ui-tool-dnsbatch-progress = { $done } noms d'hôtes résolus sur { $total }
ui-tool-dnsbatch-time = { $ms } ms
ui-tool-headers-report-title = === Rapport de sécurité des en-têtes HTTP : { $target } ===
ui-tool-headers-report-grade = Note : { $grade }
ui-tool-headers-cookie-missing = Attributs manquants : { $flags }
ui-tool-headers-certificate-trusted = Certificat TLS : approuvé par cet ordinateur.
ui-tool-headers-certificate-untrusted = Certificat TLS non approuvé : { $reason } Les en-têtes ci-dessous ont tout de même été lus.
ui-tool-headers-error-timeout = La requête a expiré après 15 secondes.
ui-tool-headers-error-unresolved = Le nom d'hôte { $host } n'a pas pu être résolu.
ui-tool-headers-error-connection = Échec de la connexion : { $reason }
ui-tool-headers-error-tls = Échec de la négociation TLS : { $reason }
ui-tool-headers-error-certificate = Le serveur a présenté un autre certificat pendant l'analyse : { $reason }
ui-tool-headers-error-empty = Le serveur a fermé la connexion sans répondre.
ui-tool-headers-error-oversized = Les en-têtes de la réponse dépassent { $kib } Kio : ils n'ont pas été analysés.
ui-tool-headers-error-incomplete = Le serveur a fermé la connexion avant la fin des en-têtes de la réponse : ils n'ont pas été analysés.
ui-tool-headers-http-status = Statut HTTP : { $code }
ui-tool-headers-stop = Arrêter
ui-tool-systable-error-not-available = Cette information ne peut pas être lue sur ce système.
ui-tool-systable-error-not-started = La commande n'a pas pu être lancée : { $error }
ui-tool-systable-error-timeout = La commande ne s'est pas terminée à temps et a été arrêtée.
ui-tool-systable-error-failed = La commande a échoué (code de sortie { $code }).
ui-tool-systable-error-failed-detail = La commande a échoué : { $error }
ui-tool-systable-error-unreadable = La sortie de la commande n'a pas pu être lue : { $error }
ui-tool-systable-exit-code-none = aucun
ui-tool-openports-process-idle = Processus inactif du système
ui-tool-openports-process-pid = [{ $pid }]
ui-tool-netif-kind-ethernet = Ethernet
ui-tool-netif-kind-wireless = Wi-Fi
ui-tool-netif-kind-loopback = Bouclage
ui-tool-netif-kind-tunnel = Tunnel
ui-tool-netif-kind-ppp = PPP
ui-tool-netif-kind-mobile = Haut débit mobile
ui-tool-netif-kind-unknown = Inconnu
ui-tool-netif-kind-other = Autre ({ $code })
ui-tool-netif-status-up = Active
ui-tool-netif-status-down = Inactive
ui-tool-netif-status-testing = En test
ui-tool-netif-status-unknown = Inconnu
ui-tool-netif-status-dormant = En veille
ui-tool-netif-status-not-present = Absente
ui-tool-netif-status-lower-layer-down = Couche inférieure inactive
ui-tool-netif-speed-kbps = { $value } kbit/s
ui-tool-netif-speed-mbps = { $value } Mbit/s
ui-tool-netif-speed-gbps = { $value } Gbit/s
ui-tool-netif-speed-unknown = Inconnue
ui-tool-netif-dhcp-yes = DHCP
ui-tool-netif-dhcp-no = Statique
ui-tool-netif-dhcp-unknown = Inconnu
ui-tool-routes-on-link = Sur la liaison
## A tab's menu: its last entries, as the C# SessionCloseTab and SessionCloseAllTabs.
ui-tab-menu-close-tab = Fermer l'onglet
ui-tab-menu-close-all = Fermer tous les onglets
## Why "Copy username" is greyed, said on hover as the C# TreeCtxCopyUsernameDisabledReason.
ui-tree-copy-username-disabled = Cette session n'a pas d'identifiant enregistré.
## A folder's count while searching or filtering: those found, then how many it holds.
ui-tree-folder-count-filtered = { $shown }/{ $total }
## The keys said at the right of a menu's entries, as the C# InputGestureText.
ui-menu-keys-rename = F2
ui-menu-keys-edit = Ctrl+E
ui-menu-keys-delete = Suppr
ui-menu-keys-new-session = Ctrl+N
ui-menu-keys-fullscreen = F11
ui-menu-keys-close-tab = Ctrl+F4
ui-menu-keys-toggle-orientation = Ctrl+Shift+O
## The tab keys of the keyboard shortcuts list.
ui-shortcuts-go-to-tab = Aller à l'onglet 1 à 8
ui-shortcuts-last-tab = Aller au dernier onglet
ui-shortcuts-move-tab = Déplacer l'onglet d'une place à gauche ou à droite
ui-shortcuts-tab-menu = Menu de l'onglet affiché
## A tab's menu: the tabs of the last close opened again, as the C# SessionReopenClosedTab
## and SessionReopenClosedTabs; the second when that close took several.
ui-tab-menu-reopen-closed = Rouvrir l'onglet fermé
ui-tab-menu-reopen-closed-many = Rouvrir les { $count } onglets fermés
ui-menu-keys-reopen-closed = Ctrl+Shift+R
## The keyboard shortcuts list: Ctrl+Shift+R.
ui-shortcuts-reopen-closed-tab = Rouvrir les onglets de la dernière fermeture
## The external tools: the Settings card, the session and tab menus, Quick Connect and the
## status bar, as the C# "External Tools".
ui-external-tools-title = Outils externes
ui-external-tools-description = Programmes lancés pour une session depuis son menu, le menu de son onglet et la connexion rapide. Les variables ci-dessous sont remplacées par les valeurs de la session, chacune dans son propre argument.
ui-external-tools-empty = Aucun outil externe pour l'instant.
ui-external-tools-row = { $name } - { $program }
ui-external-tools-add = Ajouter
ui-external-tools-move-up = Monter
ui-external-tools-move-down = Descendre
ui-external-tools-remove = Supprimer
ui-external-tools-name = Nom
ui-external-tools-program = Exécutable
ui-external-tools-arguments = Arguments
ui-external-tools-working-directory = Répertoire de travail
ui-external-tools-run-as-administrator = Exécuter en tant qu'administrateur
ui-external-tools-run-hidden = Exécuter en arrière-plan
ui-external-tools-var-host = Adresse de l'hôte distant
ui-external-tools-var-port = Numéro de port distant
ui-external-tools-var-user = Nom d'utilisateur
ui-external-tools-var-server-name = Nom d'affichage de la session
ui-external-tools-var-protocol = Protocole de connexion (RDP, SSH, etc.)
ui-external-tools-var-key-file = Chemin du fichier clé SSH
ui-external-tools-var-project = Nom du projet (toujours vide dans cette version)
ui-external-tools-var-gateway = Nom de la passerelle SSH
ui-external-tools-test = Tester
ui-external-tools-new-name = Nouvel outil
ui-external-tools-incomplete = Chaque outil externe doit avoir un nom et un chemin d'exécutable.
ui-external-tools-duplicate = Nom d'outil externe en doublon : "{ $name }". Chaque outil doit avoir un nom unique.
ui-external-tools-not-found = Outil externe "{ $name }" : exécutable introuvable à "{ $program }". Vérifiez le chemin ou installez le programme.
ui-external-tools-shell-blocked = Le lancement d'interpréteurs de commandes ou d'hôtes de scripts comme outils externes est bloqué pour des raisons de sécurité.
ui-external-tools-program-missing = L'exécutable "{ $program }" est introuvable.
ui-external-tools-program-invalid = L'exécutable "{ $program }" doit être un chemin complet ou un simple nom de programme.
ui-external-tools-option-like = La valeur de { $placeholder } serait lue comme une option du programme : outil non lancé.
ui-external-tools-folder-missing = Le répertoire de travail "{ $folder }" n'existe pas.
ui-external-tools-elevation-unsupported = L'exécution en tant qu'administrateur n'est disponible que sous Windows.
ui-external-tools-menu = Outils externes
ui-palette-external-tools = Outils externes
ui-status-external-tool-started = Outil externe lancé : { $name }
ui-status-external-tool-failed = Échec du lancement de l'outil externe "{ $name }" : { $reason }
ui-external-tools-arguments-invalid = Les arguments ou le répertoire de travail contiennent un caractère NUL.
ui-settings-value-tool = { $name } ({ $program })
ui-settings-value-tool-admin = { $name } ({ $program }, en administrateur)
## La reprise des paramètres de Heimdall C# : ses préférences.
ui-dialog-import-settings-unreadable = Les paramètres de Heimdall C# n'ont pas pu être lus : les profils ont été importés sans les valeurs par défaut de leurs dossiers, et ses serveurs SSH de confiance et connexions planifiées n'ont pas été repris. { $detail }
ui-dialog-import-preferences = { $count ->
    [one] { $count } préférence de Heimdall C# peut être reprise : vérifiez-la avant qu'elle soit appliquée.
   *[other] { $count } préférences de Heimdall C# peuvent être reprises : vérifiez-les avant qu'elles soient appliquées.
}
ui-dialog-import-preferences-kept = Conservées car modifiées ici : { $names }
ui-dialog-import-preferences-secret = Secrets, jamais repris : { $names }
ui-dialog-import-preferences-invalid = Valeurs non lues : { $names }
ui-dialog-import-preferences-relative = Chemins de l'installation C#, non repris : { $names }
ui-dialog-import-preferences-failed = Les préférences de Heimdall C# n'ont pas pu être lues : { $detail }
ui-dialog-import-preferences-review = Vérifier les préférences

## L'analyseur réseau, Ctrl+Maj+N.
ui-shortcuts-network-scan = Analyser le réseau pour ajouter des hôtes
ui-network-scan-title = Analyseur réseau
ui-network-scan-prompt = Sous-réseau à analyser (CIDR, par exemple 192.168.1.0/24) ou une adresse IPv4. Les hôtes qui répondent au ping sont sondés sur les ports SSH, Telnet, HTTP, HTTPS, RDP, VNC et WinRM.
ui-network-scan-start = Analyser
ui-network-scan-stop = Arrêter
ui-network-scan-error-empty = Saisissez un sous-réseau tel que 192.168.1.0/24.
ui-network-scan-error-invalid = Ce n'est pas une adresse ou un sous-réseau IPv4. Utilisez une forme telle que 192.168.1.0/24.
ui-network-scan-error-range = Le sous-réseau est trop large : utilisez un préfixe de /{ $prefix } ou plus long.
ui-network-scan-progress = Analyse de { $cidr } : { $done } adresses sur { $total }.
ui-network-scan-none = Aucun hôte n'a répondu dans { $cidr }.
ui-network-scan-complete = { $count ->
    [one] { $count } hôte trouvé.
   *[other] { $count } hôtes trouvés.
}
ui-network-scan-column-address = Adresse
ui-network-scan-column-name = Nom
ui-network-scan-column-ports = Ports ouverts
ui-network-scan-column-session = Session
ui-network-scan-column-status = État
ui-network-scan-status-saved = Déjà enregistré
ui-network-scan-status-no-port = Aucun port d'accès distant
ui-network-scan-add = { $count ->
    [one] Ajouter { $count } session
   *[other] Ajouter { $count } sessions
}
ui-network-scan-kind-rdp = RDP
ui-network-scan-kind-ssh = SSH
ui-network-scan-kind-vnc = VNC
ui-network-scan-kind-winrm = WinRM
ui-network-scan-kind-telnet = Telnet
ui-network-scan-kind-web = Web
# Tab strip: the list of every open tab and the arrangement by folder, as the C# one.
ui-tab-menu-group-by-folder = Grouper les onglets par dossier
ui-tab-list-tooltip = Tous les onglets ouverts
ui-tab-list-entry = { $name } - { $state }

ui-settings-ssh-auto-reconnect-first-delay = Délai avant la première tentative
ui-settings-ssh-auto-reconnect-second-delay = Délai avant la deuxième tentative
ui-settings-ssh-auto-reconnect-subsequent-delay = Délai avant chaque tentative suivante
ui-settings-ssh-auto-reconnect-delay-refused = Le délai doit être compris entre { $min } et { $max } secondes.
ui-settings-rdp-confirm-disconnect = Confirmer avant de déconnecter une session RDP
ui-settings-rdp-confirm-disconnect-hint = Demande une confirmation quand on appuie sur le bouton Déconnecter d'une session RDP.
ui-settings-rdp-redirection-indicators = Toujours afficher tous les indicateurs de redirection
ui-settings-rdp-redirection-indicators-hint = Affiche aussi les redirections non partagées sur la barre d'une session RDP, au lieu de les ranger derrière le badge "+N".
# "Route via" des outils réseau : leurs connexions TCP faites depuis une passerelle SSH.
ui-tool-route-label = Passer par
ui-tool-route-direct = Direct (depuis cet ordinateur)
ui-tool-route-hint = Les connexions sont faites depuis la passerelle, qui résout le nom d'hôte. Rien n'est exécuté sur la passerelle.
ui-tool-route-refused = Cet outil a besoin d'ICMP, d'ARP, de sockets bruts ou d'UDP, qu'une passerelle SSH ne peut pas transporter : il ne s'exécute donc pas à travers une passerelle. Choisissez Direct pour l'exécuter depuis cet ordinateur.
ui-tool-route-unavailable = La passerelle choisie n'est plus disponible. Choisissez-la à nouveau, ou choisissez Direct.
ui-tool-route-error-untrusted = La clé d'hôte de la passerelle { $host }:{ $port } n'est pas encore approuvée. Connectez-vous-y une fois depuis une session pour approuver sa clé, puis réessayez.
ui-tool-route-error-gateway = Passerelle : { $detail }
# Cartographie réseau : ce que les sondes en lecture seule apprennent d'un hôte (colonnes de l'export CSV, puis la ligne affichée sous l'hôte).
ui-tool-netmap-col-os = Système d'exploitation
ui-tool-netmap-col-os-confidence = Confiance du système
ui-tool-netmap-col-netbios-name = Nom NetBIOS
ui-tool-netmap-col-netbios-domain = Domaine NetBIOS
ui-tool-netmap-col-snmp-descr = Description SNMP
ui-tool-netmap-col-snmp-name = Nom SNMP
ui-tool-netmap-col-snmp-location = Emplacement SNMP
ui-tool-netmap-col-ntlm-dns-name = Nom DNS Windows
ui-tool-netmap-col-os-build = Build Windows
ui-tool-netmap-col-smb-dialect = Dialecte SMB
ui-tool-netmap-col-smb-signing = Signature SMB requise
ui-tool-netmap-col-ssh-banner = Bannière SSH
ui-tool-netmap-col-ssh-hassh = HASSH SSH
ui-tool-netmap-col-tls-subject = Sujet du certificat TLS
ui-tool-netmap-fp-os = Système : { $os } ({ $confidence }%)
ui-tool-netmap-fp-netbios = NetBIOS : { $name }
ui-tool-netmap-fp-netbios-domain = NetBIOS : { $domain }\{ $name }
ui-tool-netmap-fp-workgroup = Domaine NetBIOS : { $domain }
ui-tool-netmap-fp-snmp = SNMP : { $descr }
ui-tool-netmap-fp-snmp-name = Nom SNMP : { $name }
ui-tool-netmap-fp-windows = Nom Windows : { $name }
ui-tool-netmap-fp-build = Build Windows : { $build }
ui-tool-netmap-fp-smb =
    SMB { $dialect }, { $required ->
        [yes] signature requise
       *[no] signature non requise
    }
ui-tool-netmap-fp-ssh = SSH : { $banner }
ui-tool-netmap-fp-hassh = HASSH : { $hash }
ui-tool-netmap-fp-tls = TLS : { $subject }
ui-settings-rdp-default-width = Largeur de la résolution par défaut
ui-settings-rdp-default-height = Hauteur de la résolution par défaut
ui-settings-rdp-default-resolution-refused = La résolution par défaut doit être comprise entre { $min } et { $max } pixels.
ui-settings-pixels-unit = px
ui-tool-route-udp-skipped = La requête de nom NetBIOS utilise UDP, qu'une passerelle SSH ne peut pas transporter : elle n'a pas été envoyée. Choisissez Direct pour l'envoyer depuis cet ordinateur.

ui-shared-title = Sessions partagées
ui-shared-source-header = { $name } ({ $count })
ui-shared-badge = partagée
ui-shared-row-tooltip = Lecture seule : cette session vient du fichier partagé de { $source }. { $address }
ui-shared-disabled = Désactivée.
ui-shared-problem-invalid-id = L'identifiant de la source n'est pas valide.
ui-shared-problem-duplicate-id = Une autre source a le même identifiant.
ui-shared-problem-no-path = La source n'a pas de chemin de fichier.
ui-shared-problem-missing = Le fichier { $path } est introuvable.
ui-shared-problem-too-large = Le fichier { $path } dépasse { $limit } Mo et n'a pas été lu.
ui-shared-problem-unreadable = Le fichier { $path } n'a pas pu être lu. { $reason }
ui-shared-problem-unparsable = Le fichier { $path } n'a pas pu être compris. { $reason }
ui-shared-problem-empty = Le fichier { $path } ne contient aucune session SSH ou RDP utilisable.
ui-shared-kept-previous = La dernière modification n'a pas pu être prise en compte, les sessions précédentes sont conservées. { $reason }
ui-shared-refused =
    { $count ->
        [one] { $count } session du fichier a été écartée : autre protocole ou hôte invalide.
       *[other] { $count } sessions du fichier ont été écartées : autre protocole ou hôte invalide.
    }
ui-shared-passwords =
    { $count ->
        [one] { $count } mot de passe enregistré dans le fichier n'est jamais lu.
       *[other] { $count } mots de passe enregistrés dans le fichier ne sont jamais lus.
    }
ui-shared-skipped =
    { $count ->
        [one] { $count } entrée du fichier n'a pas pu être lue et a été ignorée.
       *[other] { $count } entrées du fichier n'ont pas pu être lues et ont été ignorées.
    }
ui-shared-truncated = Le fichier contient plus qu'une source partagée ne peut en prendre, la suite n'a pas été lue.
ui-mobaxterm-variables-title = Variables d'import MobaXterm
ui-mobaxterm-variables-description = À l'import d'un fichier de sessions MobaXterm, [NOM] dans un champ utilisateur est remplacé par la valeur de NOM : [DAS] devient la valeur de DAS.
ui-mobaxterm-variables-name = Nom
ui-mobaxterm-variables-value = Valeur
ui-mobaxterm-variables-resolved = Devient
ui-mobaxterm-variables-unresolved = Non résolue
ui-mobaxterm-variables-add = Ajouter
ui-mobaxterm-variables-remove = Supprimer
ui-mobaxterm-variables-name-required = Saisissez un nom.
ui-mobaxterm-variables-name-invalid = Utilisez des lettres, des chiffres et des tirets bas, en commençant par une lettre ou un tiret bas.
ui-mobaxterm-variables-name-duplicate = Une autre variable porte ce nom.
ui-mobaxterm-variables-value-required = Saisissez une valeur.
ui-shared-sources-title = Sources de sessions partagées
ui-shared-sources-description = Fichiers de sessions MobaXterm tenus par une équipe, lus et jamais modifiés. Leurs sessions SSH et RDP apparaissent dans l'arborescence, en lecture seule. Le chemin est absolu ou commence par une variable comme %USERPROFILE%.
ui-shared-sources-name = Nom
ui-shared-sources-path = Chemin du fichier
ui-shared-sources-enabled = Lire cette source
ui-shared-sources-report = Rapport d'import
ui-shared-sources-remove = Supprimer
ui-shared-sources-add = Ajouter une source
ui-shared-sources-browse-title = Sélectionner un fichier de sessions partagées
ui-shared-sources-browse-filter = Fichiers de sessions MobaXterm
ui-shared-sources-status =
    { $count ->
        [one] { $count } session chargée.
       *[other] { $count } sessions chargées.
    }
ui-shared-sources-name-required = Saisissez un nom.
ui-shared-sources-name-duplicate = Une autre source porte ce nom.
ui-shared-sources-path-required = Saisissez le chemin du fichier.
ui-shared-sources-path-invalid = Le chemin doit être absolu, ou commencer par une variable comme %USERPROFILE%.
ui-shared-sources-id-invalid = L'identifiant de la source n'est pas valide.
ui-shared-sources-id-duplicate = Une autre source a le même identifiant.
ui-shared-offline = Le fichier est inaccessible : ces sessions viennent de la copie conservée le { $date }. { $reason }
ui-shared-report-title = Rapport d'import : { $name }
ui-shared-report-taken =
    { $count ->
        [one] { $count } session prise dans le fichier.
       *[other] { $count } sessions prises dans le fichier.
    }
ui-shared-report-empty = Rien à signaler : toutes les sessions du fichier ont été prises telles quelles.
ui-shared-report-close = Fermer
ui-shared-report-skipped =
    { $count ->
        [one] { $count } session ignorée
       *[other] { $count } sessions ignorées
    }
ui-shared-report-unresolved =
    { $count ->
        [one] { $count } identifiant avec une variable non définie
       *[other] { $count } identifiants avec une variable non définie
    }
ui-shared-report-mismatch =
    { $count ->
        [one] { $count } nom qui n'est pas celui de son hôte
       *[other] { $count } noms qui ne sont pas ceux de leur hôte
    }
ui-shared-report-duplicate-host =
    { $count ->
        [one] { $count } session sur un hôte classé deux fois
       *[other] { $count } sessions sur un hôte classé deux fois
    }
ui-shared-report-reason-other-protocol = Le protocole { $protocol } n'est pas pris en charge, seuls SSH et RDP sont pris.
ui-shared-report-reason-invalid-host = L'hôte { $host } n'est ni un nom d'hôte ni une adresse.
ui-shared-report-reason-duplicate-session = Une autre session du fichier a le même dossier et le même nom.
ui-shared-report-reason-over-limit = La source contient déjà autant de sessions qu'elle peut en contenir.
ui-shared-report-detail-unresolved = L'identifiant utilise la variable { $name }, qui n'a pas de valeur. Définissez-la dans les variables d'import MobaXterm.
ui-shared-report-detail-mismatch = Le nom n'est ni l'hôte { $host } ni son premier libellé.
ui-shared-report-detail-duplicate-host = L'hôte { $host } est aussi utilisé par { $others }.
