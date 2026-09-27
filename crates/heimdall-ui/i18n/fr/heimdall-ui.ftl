# Copyright 2026 Julien Bombled
# Licensed under the Apache License, Version 2.0. See LICENSE.

ui-window-title = Heimdall
ui-window-title-tab = { $tab } - Heimdall

ui-sidebar-title = Profils
ui-sidebar-empty = Aucun profil enregistré.
ui-sidebar-group-none = (Sans dossier)

ui-home-welcome = Bienvenue dans Heimdall-rs.
ui-home-hint = Choisissez un profil à gauche pour ouvrir une session SSH.

ui-tab-close-button = Fermer
ui-tab-bell-badge = cloche

ui-connect-progress = Connexion à { $target }...
ui-connect-cancel-button = Annuler

ui-hostkey-title = Serveur inconnu
ui-hostkey-body = C'est la première connexion à { $host } sur le port { $port }. Vérifiez que l'empreinte ci-dessous est bien celle du serveur avant de lui faire confiance.
ui-hostkey-fingerprint = Empreinte : { $fingerprint }
ui-hostkey-accept-button = Faire confiance et se connecter
ui-hostkey-reject-button = Ne pas se connecter

ui-prompt-submit-button = Continuer
ui-prompt-cancel-button = Annuler
ui-prompt-username-title = Nom d'utilisateur pour { $target }
ui-prompt-password-title = Mot de passe de { $user } sur { $target }
ui-prompt-password-retry = Le mot de passe a été refusé. Réessayez.
ui-prompt-passphrase-title = Phrase de passe de la clé { $path }
ui-prompt-passphrase-retry = La phrase de passe n'a pas déverrouillé la clé. Réessayez.
ui-prompt-interactive-title = { $user } sur { $host } : le serveur demande
ui-prompt-server-text = Le serveur indique : { $text }

ui-session-closed = La session est terminée.
ui-session-closed-status = La session est terminée (code de sortie { $status }).
ui-session-cancelled = La connexion a été annulée.
ui-session-failed-title = La connexion a échoué
ui-session-close-button = Fermer l'onglet

ui-error-invalid-host = Le nom d'hôte n'est pas valide.
ui-error-network = Le serveur est injoignable : { $detail }
ui-error-timeout = Le serveur n'a pas répondu à temps.
ui-error-hostkey-changed = La clé du serveur n'est pas celle enregistrée. Quelqu'un intercepte peut-être la connexion. Enregistrée : { $recorded }. Présentée : { $offered }. Si le changement est attendu, retirez l'ancienne entrée du fichier des hôtes connus de Heimdall.
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
    [one] Une session est encore ouverte et sera déconnectée.
   *[other] { $count } sessions sont encore ouvertes et seront déconnectées.
}
ui-dialog-exit-confirm = Quitter
ui-dialog-overwrite-title = Remplacer le fichier ?
ui-dialog-overwrite-local-body = { $name } existe déjà dans le dossier local. Le téléchargement le remplace.
ui-dialog-overwrite-remote-body = { $name } existe déjà sur le serveur. L'envoi le remplace.
ui-dialog-overwrite-confirm = Remplacer
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
    [one] Le texte contient une ligne. Le shell peut l'exécuter comme une commande dès son arrivée.
   *[other] Le texte contient { $count } lignes. Le shell peut exécuter chacune comme une commande dès son arrivée.
}
ui-dialog-paste-confirm = Coller
ui-dialog-import-title = Import terminé
ui-dialog-import-counts = Ajoutés : { $added }. Mis à jour : { $updated }. Inchangés : { $unchanged }.
ui-dialog-import-skipped = Écartés :
ui-dialog-import-skipped-item = { $name } : { $reason }
ui-dialog-import-failed-title = L'import n'a pas pu s'exécuter
ui-dialog-store-title = Le fichier des profils est inutilisable
ui-dialog-store-body = Heimdall a démarré sans profil ; les changements sont enregistrés à côté du fichier illisible, qui reste intact.
ui-dialog-detail = Détail : { $detail }

ui-import-skip-not-ssh = pas un profil SSH ({ $kind })
ui-import-skip-rd-gateway = passe par une passerelle Bureau à distance, pas encore pris en charge
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
    [one] 1 élément laissé de côté (un lien, un nom inutilisable ou un échec)
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

ui-profile-new-title = Nouveau profil
ui-profile-edit-title = Modifier le profil
ui-profile-field-name = Nom
ui-profile-field-group = Groupe
ui-profile-field-host = Adresse du serveur
ui-profile-field-port = Port
ui-profile-field-username = Nom d'utilisateur
ui-profile-field-key = Fichier de clé privée
ui-profile-optional = facultatif
ui-profile-host-placeholder = serveur.exemple.fr
ui-profile-save-button = Enregistrer
ui-profile-delete-button = Supprimer ce profil
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
