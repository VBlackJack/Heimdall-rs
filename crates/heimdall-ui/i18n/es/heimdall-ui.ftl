# Copyright 2026 Julien Bombled
# Licensed under the Apache License, Version 2.0. See LICENSE.

ui-window-title = Heimdall
ui-window-title-tab = { $tab } - Heimdall

ui-sidebar-title = Perfiles
ui-sidebar-empty = Aún no hay perfiles guardados.
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
ui-session-cancelled = La conexión se ha cancelado.
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
ui-error-subsystem-refused = El servidor se negó a iniciar { $name }.
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
ui-dialog-overwrite-title = ¿Reemplazar el archivo?
ui-dialog-overwrite-local-body = { $name } ya existe en la carpeta local. La descarga lo reemplaza.
ui-dialog-overwrite-remote-body = { $name } ya existe en el servidor. El envío lo reemplaza.
ui-dialog-overwrite-confirm = Reemplazar
ui-dialog-new-folder-title = Nueva carpeta
ui-dialog-new-folder-confirm = Crear
ui-dialog-rename-title = Cambiar nombre
ui-dialog-rename-confirm = Cambiar nombre
ui-dialog-name-placeholder = Nombre
ui-dialog-delete-title = ¿Eliminar?
ui-dialog-delete-file-body = { $name } se eliminará. No se puede deshacer.
ui-dialog-delete-folder-body = La carpeta { $name } y todo su contenido se eliminarán. Los enlaces que contiene se quitan, nunca aquello a lo que apuntan. No se puede deshacer.
ui-dialog-delete-confirm = Eliminar
ui-dialog-paste-title = ¿Pegar varias líneas?
ui-dialog-paste-body = { $count ->
    [one] El texto contiene una línea. El shell puede ejecutarla como un comando en cuanto llega.
   *[other] El texto contiene { $count } líneas. El shell puede ejecutar cada una como un comando en cuanto llega.
}
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
ui-import-skip-rd-gateway = pasa por una puerta de enlace de Escritorio remoto, aún no soportado
ui-import-skip-missing-host = no tiene host
ui-import-skip-missing-id = no tiene identificador
ui-import-skip-invalid-port = puerto no válido { $port }

ui-tab-files-title = { $name } (archivos)

ui-files-local-title = Este equipo
ui-files-remote-title = Servidor
ui-files-up-button = Subir
ui-files-refresh-button = Actualizar
ui-files-new-folder-button = Nueva carpeta
ui-files-rename-button = Cambiar nombre
ui-files-delete-button = Eliminar
ui-files-download-button = Descargar
ui-files-upload-button = Enviar
ui-files-loading = Cargando...
ui-files-empty = Carpeta vacía
ui-files-cancel-button = Cancelar
ui-files-transfers-title = Transferencias
ui-files-transfer-download = Descarga de { $name }
ui-files-transfer-upload = Envío de { $name }
ui-files-state-running = { $done } de { $total }
ui-files-state-running-unknown = { $done }
ui-files-state-done = Terminado
ui-files-state-incomplete = Terminado, { $count ->
    [one] 1 elemento omitido (un enlace, un nombre inutilizable o un fallo)
   *[other] { $count } elementos omitidos (enlaces, nombres inutilizables o fallos)
}
ui-files-state-cancelled = Cancelado; vuelva a iniciarlo para reanudar
ui-files-state-failed = Error: { $reason }
ui-files-size-bytes = { $value } B
ui-files-size-kib = { $value } KiB
ui-files-size-mib = { $value } MiB
ui-files-size-gib = { $value } GiB

ui-files-error-server = El servidor lo rechazó: { $message }
ui-files-error-no-such-file = el archivo no existe
ui-files-error-permission-denied = permiso denegado
ui-files-error-unsupported = el servidor no admite esta operación
ui-files-error-failure = la operación falló
ui-files-error-session = La sesión SFTP terminó.
ui-files-error-local = Este equipo lo rechazó: { $detail }
ui-files-error-unsafe-name = El nombre del servidor "{ $name }" no se puede usar aquí: { $reason }.
ui-files-error-not-a-file = Solo se pueden transferir archivos y carpetas, no enlaces ni archivos especiales.
ui-files-error-too-large = La carpeta contiene demasiados elementos para recorrerla.
ui-files-error-invalid-name = Ese nombre no se puede usar.
ui-files-error-exists = Ya existe un elemento con ese nombre.
ui-files-name-not-a-name = no es un nombre de archivo
ui-files-name-separator = contiene un separador de carpetas
ui-files-name-control = contiene un carácter de control
ui-files-name-forbidden = contiene "{ $character }"
ui-files-name-reserved = es un nombre reservado por Windows
ui-files-name-trailing = termina con un punto o un espacio
ui-files-name-too-long = es demasiado largo

ui-profile-new-title = Nuevo perfil
ui-profile-edit-title = Editar perfil
ui-profile-field-name = Nombre
ui-profile-field-group = Grupo
ui-profile-field-host = Dirección del servidor
ui-profile-field-port = Puerto
ui-profile-field-username = Nombre de usuario
ui-profile-field-key = Archivo de clave privada
ui-profile-optional = opcional
ui-profile-host-placeholder = servidor.ejemplo.es
ui-profile-save-button = Guardar
ui-profile-delete-button = Eliminar este perfil
ui-profile-error-name-missing = Dé un nombre al perfil.
ui-profile-error-host-missing = Escriba la dirección del servidor.
ui-profile-error-host-invalid = La dirección del servidor no puede contener espacios.
ui-profile-error-host-has-user = Ponga el nombre de usuario en su propio campo, no en la dirección.
ui-profile-error-host-has-port = Ponga el puerto en su propio campo, no en la dirección.
ui-profile-error-port-invalid = El puerto es un número de 1 a 65535.
ui-profile-error-username-invalid = El nombre de usuario no puede contener espacios.
ui-profile-error-control = Un campo contiene un carácter de control.
ui-dialog-delete-profile-title = ¿Eliminar el perfil?
ui-dialog-delete-profile-body = El perfil { $name } se eliminará. Las sesiones abiertas siguen abiertas. No se puede deshacer.
ui-dialog-delete-profile-confirm = Eliminar
