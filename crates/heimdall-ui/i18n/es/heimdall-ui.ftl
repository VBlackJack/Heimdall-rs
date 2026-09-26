# Copyright 2026 Julien Bombled
# Licensed under the Apache License, Version 2.0. See LICENSE.

ui-window-title = Heimdall
ui-window-title-tab = { $tab } - Heimdall

ui-sidebar-title = Perfiles
ui-sidebar-empty = Aún no hay perfiles guardados.
ui-sidebar-import-button = Importar desde Heimdall
ui-sidebar-group-none = Sin grupo

ui-home-welcome = Bienvenido a Heimdall-rs.
ui-home-hint = Elija un perfil a la izquierda para abrir una sesión SSH.

ui-tab-close-button = Cerrar
ui-tab-bell-badge = campana

ui-connect-progress = Conectando con { $target }...
ui-connect-cancel-button = Cancelar

ui-hostkey-title = Servidor desconocido
ui-hostkey-body = Es la primera conexión a { $host } en el puerto { $port }. Compruebe que la huella siguiente es la del servidor antes de confiar en él.
ui-hostkey-fingerprint = Huella: { $fingerprint }
ui-hostkey-accept-button = Confiar y conectar
ui-hostkey-reject-button = No conectar

ui-prompt-submit-button = Continuar
ui-prompt-cancel-button = Cancelar
ui-prompt-username-title = Nombre de usuario para { $target }
ui-prompt-password-title = Contraseña de { $user } en { $target }
ui-prompt-password-retry = La contraseña fue rechazada. Inténtelo de nuevo.
ui-prompt-passphrase-title = Frase de contraseña de la clave { $path }
ui-prompt-passphrase-retry = La frase de contraseña no desbloqueó la clave. Inténtelo de nuevo.
ui-prompt-interactive-title = { $user } en { $host }: el servidor pregunta
ui-prompt-server-text = El servidor indica: { $text }

ui-session-closed = La sesión ha terminado.
ui-session-closed-status = La sesión ha terminado (código de salida { $status }).
ui-session-failed-title = La conexión ha fallado
ui-session-close-button = Cerrar la pestaña

ui-error-invalid-host = El nombre de host no es válido.
ui-error-network = No se pudo contactar con el servidor: { $detail }
ui-error-timeout = El servidor no respondió a tiempo.
ui-error-hostkey-changed = La clave del servidor no es la registrada. Alguien podría estar interceptando la conexión. Registrada: { $recorded }. Presentada: { $offered }. Si el cambio es esperado, elimine la entrada antigua del archivo de hosts conocidos de Heimdall.
ui-error-hostkey-algorithm = El servidor ya no ofrece el tipo de clave registrado ({ $recorded }).
ui-error-host-certificate = El servidor presentó un certificado de host; los certificados aún no están soportados.
ui-error-known-hosts = No se pudo usar el archivo de hosts conocidos: { $detail }
ui-error-key-unreadable = No se pudo leer el archivo de clave { $path }.
ui-error-key-unknown-format = El archivo { $path } no es una clave privada en un formato soportado.
ui-error-key-needs-passphrase = La clave { $path } está cifrada y no se dio ninguna frase de contraseña.
ui-error-key-wrong-passphrase = La frase de contraseña no desbloqueó la clave { $path }.
ui-error-key-invalid = No se pudo cargar la clave { $path }.
ui-error-auth-failed = La autenticación falló. Métodos probados: { $methods }.
ui-error-auth-failed-none = La autenticación falló: el servidor no aceptó ningún método que Heimdall pudiera ofrecer.
ui-error-disconnected = El servidor cerró la conexión.
ui-error-disconnected-message = El servidor cerró la conexión: { $message }
ui-error-cancelled = Cancelado.
ui-error-prompt-timeout = Una pregunta quedó sin respuesta demasiado tiempo.
ui-error-pty-refused = El servidor se negó a abrir un terminal.
ui-error-shell-refused = El servidor se negó a iniciar un shell.
ui-error-protocol = Error del protocolo SSH: { $detail }

ui-auth-method-agent = agente SSH
ui-auth-method-key-file = archivo de clave
ui-auth-method-keyboard-interactive = teclado interactivo
ui-auth-method-password = contraseña

ui-dialog-ok-button = Aceptar
ui-dialog-cancel-button = Cancelar
ui-dialog-close-tab-title = ¿Cerrar esta sesión?
ui-dialog-close-tab-body = La sesión sigue abierta. Cerrar la pestaña la desconecta.
ui-dialog-close-tab-confirm = Cerrar
ui-dialog-exit-title = ¿Salir de Heimdall?
ui-dialog-exit-body = { $count ->
    [one] Una sesión sigue abierta y se desconectará.
   *[other] { $count } sesiones siguen abiertas y se desconectarán.
}
ui-dialog-exit-confirm = Salir
ui-dialog-paste-title = ¿Pegar varias líneas?
ui-dialog-paste-body = El texto contiene { $count } líneas. El shell puede ejecutar cada una como un comando en cuanto llega.
ui-dialog-paste-confirm = Pegar
ui-dialog-import-title = Importación terminada
ui-dialog-import-counts = Añadidos: { $added }. Actualizados: { $updated }. Sin cambios: { $unchanged }.
ui-dialog-import-skipped = Descartados:
ui-dialog-import-skipped-item = { $name }: { $reason }
ui-dialog-import-failed-title = La importación no pudo ejecutarse
ui-dialog-store-title = No se pudo usar el archivo de perfiles
ui-dialog-store-body = Heimdall arrancó sin perfiles; los cambios se guardan junto al archivo ilegible, que queda intacto.
ui-dialog-detail = Detalle: { $detail }

ui-import-skip-not-ssh = no es un perfil SSH ({ $kind })
ui-import-skip-jump-host = pasa por una pasarela SSH, aún no soportado
ui-import-skip-missing-host = no tiene host
ui-import-skip-missing-id = no tiene identificador
ui-import-skip-invalid-port = puerto no válido { $port }
