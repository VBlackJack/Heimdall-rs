# Copyright 2026 Julien Bombled
# Licensed under the Apache License, Version 2.0. See LICENSE.

ui-window-title = Heimdall
ui-window-title-tab = { $tab } - Heimdall

ui-sidebar-title = Sesiones
ui-sidebar-empty = Aún no hay perfiles guardados.
ui-sidebar-settings-button = Ajustes
ui-desktop-send-keys = Enviar teclas
ui-desktop-send-keys-tooltip = Enviar teclas al remoto
ui-desktop-send-clipboard = Enviar portapapeles
ui-desktop-send-clipboard-tooltip = Enviar el portapapeles de este equipo al servidor, sin cifrar
ui-desktop-fullscreen = Pantalla completa (F11)
ui-desktop-match-window = Igualar a la ventana
ui-desktop-fit-window = Ajustar a la ventana
ui-desktop-exit-fullscreen = Salir de pantalla completa (F11)
ui-desktop-keys-ctrl-alt-del = Ctrl+Alt+Supr
ui-desktop-keys-windows = Tecla Windows
ui-desktop-keys-alt-tab = Alt+Tab
ui-desktop-keys-ctrl-esc = Ctrl+Esc (menú Inicio)
ui-desktop-keys-escape = Esc
ui-desktop-keys-print-screen = Impr Pant
ui-desktop-keys-win-l = Win+L (bloquear estación de trabajo)
ui-desktop-keys-win-d = Win+D (mostrar escritorio)
ui-desktop-keys-win-e = Win+E (explorador de archivos)
ui-tree-search-placeholder = Buscar
ui-tree-search-tooltip = Buscar sesiones (Ctrl+F)
ui-tree-search-clear = Borrar búsqueda
ui-tree-search-clear-button = x
ui-tree-search-no-results = Ninguna sesión coincide con la búsqueda.
ui-tree-filter-tooltip = Filtros
ui-tree-filter-protocols = Protocolos
ui-tree-filter-connected = Conectadas
ui-tree-filter-gateway = Vía pasarela
ui-tree-filter-gateway-badge = Mostrar insignia de pasarela
ui-tree-filter-no-results = Ninguna sesión coincide con tu búsqueda y filtros.
ui-tree-filter-reset = Restablecer todos los filtros
ui-sidebar-lock-button = Bloquear
ui-sidebar-lock-tooltip = Bloquear espacio de trabajo (Ctrl+L)
ui-settings-title = Ajustes
ui-settings-tab-general = General
ui-settings-tab-terminal = Terminal
ui-settings-tab-ssh = SSH
ui-settings-tab-rdp = RDP
ui-settings-tab-security = Seguridad
ui-settings-security = Seguridad
ui-settings-pin-title = PIN de la aplicación
ui-settings-pin-enabled = Actualmente hay un PIN establecido.
ui-settings-pin-disabled = No hay ningún PIN establecido.
ui-settings-pin-configure = Configurar PIN...
ui-pin-enter-title = Introduce el PIN
ui-pin-setup-title = Configurar PIN
ui-pin-field-pin = PIN
ui-pin-field-current = PIN actual
ui-pin-field-new = PIN nuevo
ui-pin-field-confirm = Confirmar PIN
ui-pin-unlock-button = Desbloquear
ui-pin-save-button = Guardar
ui-pin-remove-button = Quitar PIN
ui-pin-problem-wrong = PIN incorrecto. { $remaining ->
    [one] Queda { $remaining } intento.
   *[other] Quedan { $remaining } intentos.
}
ui-pin-problem-locked-out = Demasiados intentos incorrectos. Inténtalo de nuevo en { $minutes ->
    [one] { $minutes } minuto.
   *[other] { $minutes } minutos.
}
ui-pin-problem-wrong-current = El PIN actual es incorrecto.
ui-pin-problem-too-short = El PIN debe tener al menos { $min } dígitos.
ui-pin-problem-too-long = El PIN debe tener como máximo { $max } dígitos.
ui-pin-problem-not-digits = El PIN solo puede contener dígitos.
ui-pin-problem-mismatch = Los PIN no coinciden.
ui-pin-problem-system = No se pudo guardar el PIN: { $detail }
ui-settings-vault-title = Contraseña maestra
ui-settings-provider-title = Proveedor de credenciales externo
ui-profile-field-vault-entry = Nombre de la entrada en el almacén
ui-profile-vault-entry-placeholder = Déjalo vacío para usar el nombre para mostrar
ui-profile-vault-entry-help = Nombre opcional de la entrada de este servidor en el gestor de contraseñas externo. Se usa para la búsqueda {"{"}Title{"}"} del proveedor de credenciales; si está vacío, se usa el nombre para mostrar.
ui-status-provider-no-password = El proveedor de credenciales externo no devolvió ninguna contraseña para "{ $name }". Comprueba la configuración del comando en Ajustes > Seguridad.
ui-status-provider-failed = Falló el proveedor de credenciales externo: { $detail }
ui-status-provider-timed-out = Se agotó el tiempo de espera del proveedor de credenciales externo.
ui-settings-provider-enabled = Usar proveedor de credenciales externo
ui-settings-provider-disabled-hint = Activa "Usar proveedor de credenciales externo" para configurar estas opciones
ui-settings-provider-preset = Preajuste de configuración rápida
ui-settings-provider-type = Tipo de proveedor
ui-settings-provider-type-command = Comando externo
ui-settings-provider-type-credman = Administrador de credenciales de Windows
ui-settings-provider-credman-help = Lee credenciales genéricas del Administrador de credenciales de Windows. La entrada se busca por el nombre de entrada del almacén del perfil, o por su nombre para mostrar si no está establecido.
ui-settings-provider-preset-custom = Personalizado
ui-settings-provider-command = Comando del proveedor
ui-settings-provider-command-placeholder = p. ej. keepassxc-cli show -s {"{"}title{"}"}
ui-settings-provider-placeholders = Variables: {"{"}Host{"}"}  {"{"}Port{"}"}  {"{"}User{"}"}  {"{"}Title{"}"}  {"{"}Database{"}"}  {"{"}KeyFile{"}"}
ui-settings-provider-username = Comando de usuario (opcional)
ui-settings-provider-username-placeholder = p. ej. keepassxc-cli show -s -a UserName {"{"}title{"}"}
ui-settings-provider-username-help = Segundo comando opcional que obtiene el usuario del almacén. Se ejecuta solo cuando el perfil no tiene usuario; su salida reemplaza al usuario. Si falla, se recurre al usuario almacenado.
ui-settings-provider-unlock = Secreto de desbloqueo
ui-settings-provider-unlock-placeholder = Contraseña maestra de la base de datos o frase de contraseña GPG
ui-settings-provider-unlock-help = Se envía a la entrada estándar del comando, para las herramientas que piden desbloqueo (p. ej. keepassxc-cli o pass). Se guarda con tus contraseñas guardadas; déjalo vacío si no hace falta.
ui-settings-provider-unlock-save = Guardar
ui-settings-provider-unlock-saved = Hay un secreto de desbloqueo guardado.
ui-settings-provider-unlock-forget = Olvidar
ui-settings-provider-database = Ruta de la base de datos
ui-settings-provider-key-file = Ruta del archivo de clave
ui-settings-provider-key-file-hint = Necesario cuando se selecciona un preajuste de archivo de clave de KeePassXC.
ui-settings-provider-test = Probar
ui-settings-provider-test-running = Probando...
ui-settings-provider-test-success = Conexión correcta: contraseña obtenida.
ui-settings-provider-test-no-result = El comando no devolvió ninguna salida. Comprueba el comando y la ruta de la base de datos.
ui-settings-provider-test-timeout = El comando superó el tiempo de espera ({ $seconds } s). Comprueba que la herramienta de línea de comandos está instalada y responde.
ui-settings-provider-test-no-command = Introduce primero una plantilla de comando.
ui-settings-provider-test-no-key-file = Selecciona primero un archivo de clave.
ui-settings-provider-test-unclosed-quote = Una comilla del comando no está cerrada.
ui-settings-provider-test-error = Falló la prueba: { $detail }
ui-settings-provider-first-line = Usar solo la primera línea de la salida
ui-settings-provider-first-line-help = Toma solo la primera línea no vacía de la salida del comando, ignorando el texto de estado final. Necesario para KeePass2 KPScript (que añade una línea "OK: ...") y útil para pass (notas después de la contraseña).
ui-settings-provider-keepass2-hint = KeePass2: KPScript recibe la contraseña maestra en su línea de comandos (-pw:), lo que la expone. Para bases de datos .kdbx, keepassxc-cli es más seguro: lee la contraseña de la base de datos desde la entrada estándar mediante el campo Secreto de desbloqueo de arriba.
ui-settings-vault-explanation = Cifra tus credenciales almacenadas bajo una contraseña maestra. Se te pedirá cada vez que se inicie la aplicación.
ui-settings-vault-enabled = Activado
ui-settings-vault-disabled = Desactivado
ui-settings-vault-enable = Activar contraseña maestra
ui-settings-vault-change = Cambiar
ui-settings-vault-disable = Desactivar
ui-profile-field-password = Contraseña
ui-profile-password-saved = Contraseña guardada
ui-profile-password-clear = Borrar
ui-profile-password-clear-tooltip = Eliminar la contraseña guardada para esta sesión
ui-profile-password-locked = Desbloquea el almacén para guardar o cambiar una contraseña.
ui-profile-password-no-store = Este sistema no tiene almacén de credenciales: define una contraseña maestra para guardar contraseñas.
ui-profile-error-username-for-password = Una contraseña guardada necesita un nombre de usuario.
ui-vault-unlock-title = Introduce tu contraseña maestra
ui-vault-unlock-button = Desbloquear
ui-vault-unlock-busy = Desbloqueando el almacén
ui-vault-locked-title = Espacio de trabajo bloqueado
ui-vault-locked-body = Introduce tu contraseña maestra para desbloquear.
ui-vault-enable-title = Establece una contraseña maestra
ui-vault-enable-body = Tus credenciales almacenadas se cifrarán bajo esta contraseña. La necesitarás cada vez que se inicie la aplicación. No se puede recuperar ni restablecer: si la olvidas, Heimdall ya no se abrirá y las credenciales almacenadas se perderán.
ui-vault-enable-button = Activar
ui-vault-enable-busy = Cifrando tus credenciales almacenadas
ui-vault-change-title = Cambia tu contraseña maestra
ui-vault-change-button = Cambiar
ui-vault-change-busy = Actualizando la contraseña maestra
ui-vault-disable-title = Desactiva tu contraseña maestra
ui-vault-disable-warning = Tus credenciales guardadas volverán a estar solo en el almacén de credenciales del sistema y ya no se pedirá la contraseña maestra al iniciar.
ui-vault-disable-button = Desactivar
ui-vault-disable-busy = Eliminando la protección por contraseña maestra
ui-vault-field-master = Contraseña maestra
ui-vault-field-current = Contraseña maestra actual
ui-vault-field-new = Nueva contraseña maestra
ui-vault-field-confirm = Confirmar contraseña maestra
ui-vault-policy-hint = Usa al menos { $min } caracteres.
ui-vault-policy-ok = La fortaleza de la contraseña es suficiente.
ui-vault-policy-too-short = Demasiado corta: usa al menos { $min } caracteres.
ui-vault-policy-complexity = Usa al menos { $classes } tipos de caracteres (minúscula, mayúscula, dígito, símbolo), o { $long } caracteres o más.
ui-vault-problem-unreadable = Contraseña maestra incorrecta o almacén dañado.
ui-vault-problem-mismatch = Las contraseñas no coinciden.
ui-vault-problem-no-system-store = Este sistema no tiene almacén de credenciales para recuperar las contraseñas: la contraseña maestra se mantiene.
ui-vault-problem-exists = Ya existe un almacén.
ui-vault-problem-system = No se pudo usar el archivo del almacén: { $detail }
ui-vault-save-failed-title = No se pudo guardar la contraseña.
ui-sidebar-group-none = (Sin carpeta)

ui-home-welcome = Bienvenido a Heimdall-rs.
ui-home-subtitle = Añade una sesión o importa tus conexiones existentes para empezar.
ui-home-add-button = Añadir sesión
ui-home-import-button = Importar conexiones
ui-home-shortcuts = Ctrl+N para añadir una sesión, Ctrl+K para conexión rápida
ui-home-select = Selecciona una sesión o pulsa Ctrl+K para conectarte

ui-tab-close-button = ✕
ui-tab-bell-badge = campana

ui-connect-progress = Conectando con { $target }...
ui-connect-cancel-button = Cancelar

ui-hostkey-title = Servidor desconocido
ui-hostkey-body = Es la primera conexión a { $host } en el puerto { $port }. Comprueba que la huella siguiente es la del servidor antes de confiar en él.
ui-hostkey-fingerprint = Huella: { $fingerprint }
ui-hostkey-accept-button = Confiar y conectar
ui-hostkey-trust-once-button = Confiar solo en esta sesión
ui-hostkey-reject-button = No conectar
ui-certificate-title = Certificado de servidor no reconocido
ui-certificate-body = "{ $name }" respondió en { $host }:{ $port } con un certificado que este perfil nunca ha aprobado.
ui-certificate-caution = Heimdall no puede saber si es la máquina esperada. Apruébalo solo si reconoces la huella siguiente, o si sabes que varias máquinas responden a este nombre.
ui-certificate-fingerprint = Huella SHA-256: { $fingerprint }
ui-certificate-trust-button = Confiar en este certificado
ui-certificate-trust-once-button = Solo esta vez
ui-certificate-refuse-button = No conectar

ui-prompt-submit-button = Continuar
ui-prompt-cancel-button = Cancelar
ui-prompt-username-title = Nombre de usuario para { $target }
ui-prompt-password-title = Contraseña de { $user } en { $target }
ui-prompt-password-retry = La contraseña fue rechazada. Inténtalo de nuevo.
ui-prompt-passphrase-title = Frase de contraseña de la clave { $path }
ui-prompt-passphrase-retry = La frase de contraseña no desbloqueó la clave. Inténtalo de nuevo.
ui-prompt-server-password-title = Contraseña del servidor VNC { $target }
ui-prompt-interactive-title = { $user } en { $host }: el servidor pregunta
ui-prompt-server-text = El servidor indica: { $text }

ui-session-closed = La sesión ha terminado.
ui-session-closed-status = La sesión ha terminado (código de salida { $status }).
ui-session-cancelled = La conexión se ha cancelado.
ui-session-failed-title = La conexión ha fallado
ui-session-close-button = Cerrar la pestaña

ui-error-invalid-host = El nombre de host no es válido.
ui-error-invalid-username = El nombre de usuario no es válido.
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
ui-error-connection-lost = La sesión se desconectó de forma inesperada.
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
    [one] { $count } sesión sigue abierta y se desconectará.
   *[other] { $count } sesiones siguen abiertas y se desconectarán.
}
ui-dialog-exit-confirm = Salir
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
    [one] El texto contiene { $count } línea. El shell puede ejecutarla como un comando en cuanto llega.
   *[other] El texto contiene { $count } líneas. El shell puede ejecutar cada una como un comando en cuanto llega.
}
ui-dialog-paste-confirm = Pegar
ui-dialog-import-title = Importación terminada
ui-dialog-import-counts = Añadidos: { $added }. Actualizados: { $updated }. Sin cambios: { $unchanged }.
ui-dialog-import-skipped = Descartados:
ui-dialog-import-skipped-item = { $name }: { $reason }
ui-dialog-import-failed-title = La importación no pudo ejecutarse
ui-import-file-title = Importar sesiones
ui-import-file-filter-all = Todos los compatibles
ui-import-file-filter-json = JSON
ui-import-file-filter-rdp = Archivos RDP
ui-import-file-filter-any = Todos los archivos
ui-import-file-confirm = { $count ->
    [one] ¿Importar { $count } sesión? Las sesiones existentes con el mismo ID se actualizarán.
   *[other] ¿Importar { $count } sesiones? Las sesiones existentes con el mismo ID se actualizarán.
}
ui-import-file-confirm-mobaxterm = { $count ->
    [one] ¿Importar { $count } sesión de MobaXterm? Las contraseñas no se pueden importar y deberán volver a introducirse.
   *[other] ¿Importar { $count } sesiones de MobaXterm? Las contraseñas no se pueden importar y deberán volver a introducirse.
}
ui-import-file-button = Importar
ui-import-file-nothing = No se encontraron sesiones en el archivo seleccionado.
ui-import-file-unreadable = No se pudo leer el archivo: { $detail }
ui-import-file-encrypted = El archivo está cifrado por completo. Descífralo antes en mRemoteNG (Archivo > Guardar como, sin cifrado).
ui-import-file-too-large = El archivo es demasiado grande para importar ({ $size } bytes).
ui-import-mobaxterm-passwords = Las contraseñas de MobaXterm están cifradas con un algoritmo propietario y no se pudieron importar. Vuelve a introducir las credenciales de cada sesión.
ui-import-mobaxterm-passwords-detected = { $count ->
    [one] Se detectó { $count } contraseña almacenada en el archivo de MobaXterm. MobaXterm la cifra con un algoritmo propietario, así que no se importó; vuelve a introducir las credenciales de la sesión afectada.
   *[other] Se detectaron { $count } contraseñas almacenadas en el archivo de MobaXterm. MobaXterm las cifra con un algoritmo propietario, así que no se importaron; vuelve a introducir las credenciales de las sesiones afectadas.
}
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
    [one] { $count } elemento omitido (un enlace, un nombre inutilizable o un fallo)
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
ui-profile-field-name = Nombre para mostrar *
ui-profile-field-group = Grupo
ui-profile-field-host = Dirección del servidor
ui-profile-field-port = Puerto
ui-profile-field-username = Nombre de usuario
ui-profile-field-key = Archivo de clave privada
ui-profile-optional = opcional
ui-profile-host-placeholder = servidor.ejemplo.es
ui-profile-save-button = Guardar
ui-profile-error-name-missing = Dé un nombre al perfil.
ui-profile-error-host-missing = Escribe la dirección del servidor.
ui-profile-error-host-invalid = La dirección del servidor no puede contener espacios.
ui-profile-error-host-has-user = Pon el nombre de usuario en su propio campo, no en la dirección.
ui-profile-error-host-has-port = Pon el puerto en su propio campo, no en la dirección.
ui-profile-error-port-invalid = El puerto es un número de 1 a 65535.
ui-profile-error-username-invalid = El nombre de usuario no puede contener espacios.
ui-profile-error-control = Un campo contiene un carácter de control.
ui-dialog-delete-profile-title = ¿Eliminar el perfil?
ui-dialog-delete-profile-body = El perfil { $name } se eliminará. Las sesiones abiertas siguen abiertas. No se puede deshacer.
ui-dialog-delete-profile-confirm = Eliminar

ui-session-forget-server-button = Olvidar este servidor
ui-session-reconnect-button = Reconectar
ui-session-accept-new-key-button = Aceptar nueva clave (destructivo)
ui-error-security-refused = El servidor rechazó la seguridad que exige Heimdall (autenticación a nivel de red): { $detail }
ui-error-rdp-protocol = Error RDP: { $detail }
ui-error-vnc-protocol = Error VNC: { $detail }
ui-session-vnc-unencrypted = Sin cifrar: este escritorio y lo que escribes cruzan la red en claro.
ui-sidebar-local-shell-button = Shell local
ui-local-shell-name = Shell local
ui-local-starting = Iniciando { $name }...
ui-error-local-shell = No se pudo iniciar el shell local: { $detail }
ui-import-skip-elevation = se ejecuta con privilegios elevados, aún no compatible
ui-import-skip-post-connect = ejecuta comandos tras iniciar, aún no compatible
ui-import-skip-unsafe-local = su programa, sus argumentos o su carpeta no se pueden ejecutar tal cual (ruta relativa, comilla, carácter NUL o carpeta en otra máquina)
ui-dialog-local-title = ¿Ejecutar este programa?
ui-dialog-local-body = El perfil { $name } ejecuta el comando siguiente. Heimdall solo lo ejecuta con tu acuerdo, y vuelve a preguntar si cambia.
ui-dialog-local-folder = Se inicia en: { $folder }
ui-dialog-local-rereads = Este programa vuelve a leer su línea de comandos con sus propias reglas: & | ^ < > y % son comandos, no texto.
ui-dialog-local-confirm = Ejecutar
ui-error-remote-forward = La pasarela SSH no quiso escuchar en su puerto { $port } para el reenvío remoto: el reenvío está desactivado en ella, o el puerto ya está en uso allí.
ui-error-proxy-port = El proxy SOCKS no pudo abrir el puerto local { $port }: quizá otro programa lo usa. ({ $detail })
ui-error-jump-refused = La pasarela SSH no quiso conectarse a { $target }: el reenvío está desactivado en ella, o ese host no es accesible desde ella.
ui-import-skip-missing-gateway = pasa por una pasarela SSH que no está en el archivo, o que se descartó
ui-import-skip-gateway-loop = su pasarela SSH se alcanza a través de sí misma, por sus padres
ui-import-skip-missing-username = inicia sesión con una cuenta que no nombra
ui-import-skip-unknown-identity = inicia sesión con un modo de identidad que Heimdall no conoce
ui-error-hostkey-changed-at = La clave de host de { $target } no es la registrada: la conexión podría estar interceptada. Registrada: { $recorded }. Presentada: { $offered }.
ui-error-gateway-missing = La pasarela SSH { $id } por la que pasa este perfil no está en los perfiles.
ui-error-gateway-loop = La pasarela SSH { $id } se alcanza a través de sí misma, por sus padres.
ui-tree-connect = Conectar
ui-tree-connect-as = Conectar como...
ui-tree-edit = Editar
ui-tree-duplicate = Duplicar
ui-tree-duplicate-suffix = {" "}(copia)
ui-tree-copy-hostname = Copiar nombre de host
ui-tree-copy-username = Copiar usuario
ui-tree-copy-address = Copiar dirección
ui-tree-copy-ssh-command = Copiar comando SSH
ui-tree-delete = Eliminar
ui-tree-add-session = Añadir sesión
ui-tree-import-sessions = Importar sesiones
ui-tree-export-sessions = Exportar sesiones
ui-dialog-export-title = Exportar sesiones
ui-dialog-export-done = { $count ->
    [one] { $count } sesión exportada correctamente.
   *[other] { $count } sesiones exportadas correctamente.
}
ui-dialog-export-credentials = Las credenciales no se incluyeron en el archivo de exportación.
ui-dialog-export-failed = Falló la exportación: { $detail }
ui-export-filter-json = Archivos JSON
ui-tree-import-openssh = Importar configuración de OpenSSH...
ui-openssh-title = Importar configuración de OpenSSH
ui-openssh-summary = { $total ->
    [one] { $total } candidato
   *[other] { $total } candidatos
} - { $new ->
    [one] { $new } nuevo
   *[other] { $new } nuevos
}, { $duplicate ->
    [one] { $duplicate } duplicado
   *[other] { $duplicate } duplicados
}
ui-openssh-hint = Las entradas ProxyJump se importan como cadenas de pasarela SSH.
ui-openssh-choose-all = Importar todo
ui-openssh-column-alias = Alias
ui-openssh-column-host = HostName
ui-openssh-column-port = Puerto
ui-openssh-column-user = Usuario
ui-openssh-column-key = IdentityFile
ui-openssh-column-chain = Cadena de pasarelas
ui-openssh-column-status = Estado
ui-openssh-status-new = Nuevo
ui-openssh-status-duplicate = Duplicado
ui-openssh-reusing = reutilizando la pasarela existente "{ $name }"
ui-openssh-diagnostics = Diagnóstico ({ $count })
ui-openssh-diag-line = Línea { $line }: { $said }
ui-openssh-diag-match = Bloque Match no leído
ui-openssh-diag-include = Directiva Include no seguida: { $value }
ui-openssh-diag-wildcard = Alias con comodín ignorado: { $value }
ui-openssh-diag-unknown = Directiva desconocida ignorada: { $value }
ui-openssh-diag-port = Puerto { $value } no válido; se usará 22 en su lugar
ui-openssh-diag-duplicate = Alias duplicado dentro del archivo ignorado: { $value }
ui-openssh-diag-proxycommand = ProxyCommand no es compatible; Heimdall solo admite saltos TCP nativos mediante ProxyJump: { $value }
ui-openssh-diag-mixed = ProxyJump y ProxyCommand combinados; usa solo ProxyJump para la importación en Heimdall: { $value }
ui-openssh-diag-jump-token = ProxyJump con tokens de OpenSSH (%h/%p/%r) no es compatible: { $value }
ui-openssh-diag-cycle = Se detectó un ciclo de ProxyJump en la cadena del host { $value }
ui-openssh-diag-syntax = Sintaxis de ProxyJump no reconocida: { $value }
ui-openssh-diag-tilde = El ~ de IdentityFile se expandió a la carpeta personal: { $value }
ui-openssh-diag-fallback = Falta HostName; se usará el alias en su lugar: { $value }
ui-openssh-diag-host-token = HostName usa un token de OpenSSH que Heimdall no puede expandir (solo se admite %h); host omitido: { $value }
ui-openssh-import-button = Importar
ui-openssh-done = { $imported ->
    [one] { $imported } importado
   *[other] { $imported } importados
}, { $duplicates ->
    [one] { $duplicates } omitido
   *[other] { $duplicates } omitidos
} (duplicados), { $warnings ->
    [one] { $warnings } aviso
   *[other] { $warnings } avisos
}
ui-openssh-done-gateways = { $count ->
    [one] { $count } pasarela SSH creada para las cadenas ProxyJump.
   *[other] { $count } pasarelas SSH creadas para las cadenas ProxyJump.
}
ui-openssh-unreadable = No se puede leer el archivo seleccionado: { $detail }
ui-openssh-empty = El archivo seleccionado no contiene entradas importables.
ui-tree-import-putty = Importar sesiones de PuTTY...
ui-putty-title = Importar sesiones de PuTTY
ui-sessions-summary-invalid = { $total ->
    [one] { $total } candidato
   *[other] { $total } candidatos
} - { $new ->
    [one] { $new } nuevo
   *[other] { $new } nuevos
}, { $duplicate ->
    [one] { $duplicate } duplicado
   *[other] { $duplicate } duplicados
}, { $invalid ->
    [one] { $invalid } no válido
   *[other] { $invalid } no válidos
}
ui-sessions-status-invalid = No válido
ui-sessions-no-host = (sin host)
ui-putty-diag-default = Ajustes predeterminados de PuTTY omitidos: { $session }
ui-putty-diag-not-ssh = Sesión "{ $session }" ignorada porque el protocolo "{ $value }" no es SSH
ui-putty-diag-missing-host = La sesión "{ $session }" no tiene nombre de host y se marcará como no válida
ui-putty-diag-port = La sesión "{ $session }" tiene un puerto no válido "{ $value }"; se usará 22 en su lugar
ui-putty-diag-ppk = La sesión "{ $session }" hace referencia a una clave .ppk conservada sin convertir: { $value }
ui-putty-diag-proxy = La sesión "{ $session }" define ajustes de proxy que se capturaron pero no se asignaron: { $value }
ui-putty-diag-forwards = La sesión "{ $session }" define { $count ->
    [one] { $count } túnel que se capturó pero no se asignó
   *[other] { $count } túneles que se capturaron pero no se asignaron
}
ui-putty-diag-command = La sesión "{ $session }" define un comando de inicio que se capturó pero no se asignó: { $value }
ui-putty-done = { $imported ->
    [one] { $imported } importado
   *[other] { $imported } importados
}, { $duplicates ->
    [one] { $duplicates } omitido
   *[other] { $duplicates } omitidos
} (duplicados), { $invalid ->
    [one] { $invalid } no válido
   *[other] { $invalid } no válidos
}, { $warnings ->
    [one] { $warnings } aviso
   *[other] { $warnings } avisos
}
ui-putty-unreadable = No se pudieron leer las sesiones de PuTTY: { $detail }
ui-putty-empty = No se encontraron sesiones SSH de PuTTY.
ui-tree-import-rdp = Importar archivos RDP...
ui-rdp-title = Importar archivos .rdp
ui-rdp-filter = Archivos de Escritorio remoto
ui-rdp-summary = { $chosen ->
    [one] { $chosen } seleccionado
   *[other] { $chosen } seleccionados
} / { $files ->
    [one] { $files } archivo
   *[other] { $files } archivos
}, { $conflicts ->
    [one] { $conflicts } conflicto
   *[other] { $conflicts } conflictos
}, { $passwords ->
    [one] { $passwords } aviso de contraseña.
   *[other] { $passwords } avisos de contraseña.
}
ui-rdp-unreadable = { $count ->
    [one] No se pudo leer { $count } archivo.
   *[other] No se pudieron leer { $count } archivos.
}
ui-rdp-select-all = Seleccionar todo
ui-rdp-select-none = No seleccionar nada
ui-rdp-apply-all = Aplicar a todos los conflictos:
ui-rdp-column-source = Origen
ui-rdp-column-name = Nombre
ui-rdp-column-host = Host
ui-rdp-column-status = Estado
ui-rdp-column-conflict = Conflicto
ui-rdp-conflict-skip = Omitir
ui-rdp-conflict-replace = Reemplazar
ui-rdp-conflict-rename = Renombrar automáticamente
ui-rdp-status-invalid-address = Falta la dirección de destino RDP o no es válida.
ui-rdp-status-rd-gateway = Pasa por una puerta de enlace de Escritorio remoto, aún no admitido
ui-rdp-status-conflict = Conflicto con { $name }
ui-rdp-status-password = Contraseña no importada
ui-rdp-status-partial = Asignación parcial
ui-rdp-status-unknown = { $count ->
    [one] { $count } clave desconocida
   *[other] { $count } claves desconocidas
}
ui-rdp-import-button = Importar seleccionados
ui-rdp-rename = { $name } (Importado { $n })
ui-rdp-fallback-name = RDP importado
ui-rdp-done = { $imported ->
    [one] { $imported } importado
   *[other] { $imported } importados
}, { $replaced ->
    [one] { $replaced } reemplazado
   *[other] { $replaced } reemplazados
}, { $renamed ->
    [one] { $renamed } renombrado automáticamente
   *[other] { $renamed } renombrados automáticamente
}, { $skipped ->
    [one] { $skipped } omitido
   *[other] { $skipped } omitidos
}, { $passwords ->
    [one] { $passwords } contraseña ignorada.
   *[other] { $passwords } contraseñas ignoradas.
}
ui-rdp-nothing = No se encontraron archivos .rdp válidos para importar.
ui-tree-import-known-hosts = Importar hosts SSH de confianza...
ui-hostkeys-title = Importar hosts SSH de confianza
ui-hostkeys-pick-title = Seleccionar archivo known_hosts
ui-hostkeys-summary = { $total ->
    [one] { $total } entrada
   *[other] { $total } entradas
}: { $new } nuevas, { $existing } ya de confianza, { $conflicts } en conflicto
ui-hostkeys-column-host = Host
ui-hostkeys-column-type = Tipo
ui-hostkeys-column-fingerprint = Huella
ui-hostkeys-column-notes = Notas
ui-hostkeys-status-new = Nuevo
ui-hostkeys-status-existing = Ya es de confianza
ui-hostkeys-status-conflict = Conflicto
ui-hostkeys-note-existing = La misma huella ya es de confianza
ui-hostkeys-note-conflict-store = Conflicto con la huella que ya es de confianza
ui-hostkeys-note-conflict-file = Varias huellas distintas para este host en el archivo de origen
ui-hostkeys-diag-hashed = La entrada de known_hosts con hash no es compatible (línea { $line }).
ui-hostkeys-diag-cert-authority = El marcador @cert-authority no es compatible (línea { $line }).
ui-hostkeys-diag-revoked = El marcador @revoked no es compatible (línea { $line }).
ui-hostkeys-diag-pattern = Patrón de host no compatible en la línea { $line }: { $value }
ui-hostkeys-diag-key-type = Tipo de clave no compatible en la línea { $line }: { $value }
ui-hostkeys-diag-malformed = Línea { $line } mal formada: { $value }
ui-hostkeys-malformed-too-long = línea demasiado larga
ui-hostkeys-malformed-fields = { $count ->
    [one] { $count } campo en lugar de 3
   *[other] { $count } campos en lugar de 3
}
ui-hostkeys-malformed-bad-key = no se puede leer la clave
ui-hostkeys-malformed-marker = marcador desconocido { $marker }
ui-hostkeys-done = { $imported } importados, { $existing } omitidos (ya de confianza), { $conflicts } omitidos (conflicto), { $warnings ->
    [one] { $warnings } aviso
   *[other] { $warnings } avisos
}
ui-hostkeys-empty = No se encontraron entradas utilizables en el archivo known_hosts seleccionado.
ui-hostkeys-unreadable = No se pudo leer el archivo: { $detail }
ui-hostkeys-too-large = El archivo es demasiado grande para importar ({ $size } bytes).
ui-trusted-host-keys-import = Importar known_hosts
ui-tree-add-tooltip = Añadir sesión
ui-tree-more-tooltip = Más acciones
ui-tree-tooltip-host = Host: { $host }
ui-tree-tooltip-user = Usuario: { $user }
ui-tree-tooltip-protocol = Protocolo: { $protocol }
ui-profile-protocol-picker-title = Elige un protocolo
ui-profile-protocol-picker-desc = Selecciona el tipo de conexión que quieres configurar.
ui-profile-protocol-rdp-name = Escritorio remoto
ui-profile-protocol-rdp-desc = Sesión de escritorio remoto de Windows
ui-profile-protocol-ssh-name = SSH
ui-profile-protocol-ssh-desc = Terminal de shell seguro
ui-profile-protocol-winrm-name = WinRM
ui-profile-protocol-winrm-desc = Sesión de administración remota de PowerShell
ui-profile-protocol-sftp-name = SFTP
ui-profile-protocol-sftp-desc = Transferencia de archivos segura por SSH
ui-profile-protocol-ftp-name = FTP
ui-profile-protocol-ftp-desc = Transferencia de archivos clásica
ui-profile-port-ftp = Puerto FTP
ui-profile-credentials-ftp = Autenticación FTP
ui-profile-credentials-ftp-desc = Introduce el usuario y la contraseña FTP. Déjalo en blanco para acceso anónimo.
ui-profile-options-ftp = Opciones FTP
ui-profile-toggle-passive = Modo pasivo (recomendado para redes con cortafuegos)
ui-profile-toggle-ftps = Activar SSL/TLS (FTPS)
ui-profile-protocol-local-name = Shell local
ui-profile-protocol-local-desc = Sesión de terminal local
ui-profile-section-basics-local-desc = Nombra la sesión como la muestra el árbol.
ui-profile-local-title = Shell local
ui-profile-local-desc = Configura el ejecutable del shell y los argumentos de inicio.
ui-profile-local-executable = Ejecutable
ui-profile-local-default-shell = El shell predeterminado
ui-profile-local-presets = Shells habituales
ui-profile-local-arguments = Argumentos
ui-profile-local-advanced-title = Opciones avanzadas del shell
ui-profile-local-advanced-desc = La carpeta en la que se inicia el shell.
ui-profile-local-working-directory = Directorio de trabajo
ui-profile-error-local-arguments = Los argumentos dejan una comilla abierta.
ui-profile-protocol-vnc-name = VNC
ui-profile-protocol-vnc-desc = Compartición de pantalla remota
ui-profile-protocol-telnet-name = Telnet
ui-profile-protocol-telnet-desc = Terminal heredada sin cifrar
ui-profile-protocol-badge = Protocolo
ui-profile-section-basics = Datos básicos de conexión
ui-profile-section-basics-desc = Establece el host de destino y el puerto de servicio que Heimdall debe abrir.
ui-profile-port-rdp = Puerto RDP remoto
ui-profile-port-ssh = Puerto SSH remoto
ui-profile-port-winrm = Puerto WinRM
ui-profile-port-vnc = Puerto VNC
ui-profile-port-telnet = Puerto Telnet
ui-profile-credentials-rdp = Credenciales RDP
ui-profile-credentials-rdp-desc = Credenciales usadas por la sesión de Escritorio remoto una vez completado el enrutamiento.
ui-profile-credentials-ssh = Credenciales SSH
ui-profile-credentials-ssh-desc = Estas credenciales se usan para la propia sesión SSH o SFTP.
ui-profile-credentials-winrm = Credenciales WinRM
ui-profile-credentials-winrm-desc = La administración remota de PowerShell usa la identidad de Windows actual o una credencial almacenada.
ui-profile-credentials-vnc = Autenticación VNC
ui-profile-username-rdp-placeholder = usuario, DOMINIO\usuario, o usuario@dominio
ui-profile-field-domain = Dominio de Windows
ui-profile-domain-placeholder = CORP o corp.ejemplo.com
ui-profile-domain-hint = El nombre NetBIOS (CORP) o el dominio DNS (corp.ejemplo.com). Déjalo vacío si el usuario de arriba ya lleva uno.
ui-profile-winrm-identity = Identidad
ui-profile-winrm-identity-current = Identidad de Windows actual
ui-profile-winrm-identity-stored = Credencial almacenada
ui-profile-options-rdp = Opciones de la sesión RDP
ui-profile-options-vnc = Opciones VNC
ui-profile-options-telnet = Opciones Telnet
ui-profile-toggle-clipboard = Redirigir portapapeles
ui-profile-toggle-drives = Redirigir unidades
ui-profile-toggle-nla = Activar autenticación a nivel de red
ui-profile-rdp-follow-defaults = Usar valores predeterminados globales de RDP
ui-profile-rdp-defaults-banner = Este servidor está usando tus valores predeterminados globales de RDP. Desmarca "Usar valores predeterminados globales de RDP" para establecer opciones por servidor.
ui-profile-rdp-defaults-not-in-effect = Los colores, el sonido, el portapapeles, las unidades, la autenticación a nivel de red y la resolución dinámica vienen de los valores predeterminados globales: los valores que se muestran abajo para ellos son los propios de este servidor, no los que están en vigor.
ui-settings-rdp-defaults = Valores predeterminados de RDP
ui-settings-rdp-defaults-hint = Las opciones de todo servidor RDP que usa los valores predeterminados globales.
ui-profile-toggle-admin = Ejecutar como sesión de administrador (/admin)
ui-profile-audio = Modo de audio
ui-profile-audio-off = Desactivado
ui-profile-audio-local = Reproducción local
ui-profile-audio-on-server = Reproducción remota
ui-profile-color-depth = Profundidad de color
ui-profile-color-16 = 16 bits
ui-profile-color-24 = 24 bits
ui-profile-color-32 = 32 bits
ui-profile-resolution-title = Perfil de resolución
ui-profile-resolution-desc = Elige cómo dimensiona este servidor la sesión de Escritorio remoto incrustada.
ui-profile-resolution-mode = Modo de resolución
ui-profile-resolution-fit-window = Ajustar a la ventana
ui-profile-resolution-fixed = Fija
ui-profile-resolution-smart-sizing = Ajuste de tamaño inteligente
ui-profile-resolution-presets = Resoluciones habituales
ui-profile-resolution-custom = Personalizada...
ui-profile-resolution-preset = { $width }x{ $height }
ui-profile-resolution-width = Ancho
ui-profile-resolution-height = Alto
ui-profile-resolution-scale-fixed = Escalar la resolución fija para ajustarse al panel
ui-profile-resolution-dynamic = Permitir actualizaciones dinámicas de resolución
ui-profile-nla-off-hint = Sin autenticación a nivel de red, no se envía una contraseña guardada: Heimdall la pide.
ui-profile-toggle-use-ssl = Usar SSL
ui-profile-toggle-skip-cert = Omitir la validación de certificados (inseguro)
ui-profile-toggle-view-only = Modo solo visualización (sin entrada de teclado ni ratón)
ui-profile-toggle-no-password = Permitir un servidor que no pide contraseña
ui-profile-telnet-warning = Telnet lo envía todo sin cifrar, contraseñas incluidas.
ui-profile-section-organization = Organización
ui-profile-folder-placeholder = Producción/Bases de datos
ui-profile-browse-button = Examinar...
ui-profile-browse-key-title = Seleccionar clave SSH
ui-profile-browse-key-all = Todos los archivos
ui-profile-browse-key-ppk = Archivos PPK
ui-profile-browse-key-pem = Archivos PEM
ui-profile-folder-hint = Usa / para anidar carpetas: Producción/Bases de datos coloca esta sesión en Bases de datos, dentro de Producción.
ui-profile-error-username-missing = El usuario es obligatorio.
ui-profile-error-domain-invalid = El dominio no puede contener un espacio ni una comilla doble.
ui-profile-error-fixed-width = El ancho fijo RDP debe estar entre { $min } y { $max }.
ui-profile-error-fixed-height = El alto fijo RDP debe estar entre { $min } y { $max }.
ui-profile-error-socks-port = El puerto SOCKS5 debe ser un número de 0 a 65535; 0 desactiva el proxy.
ui-profile-error-remote-bind-port = El puerto remoto debe ser un número de 0 a 65535; 0 desactiva el reenvío.
ui-profile-error-remote-local-port = El puerto local debe ser un número de 0 a 65535; 0 usa el puerto remoto.
ui-profile-gateway-routing = Enrutamiento por pasarela
ui-profile-gateway-routing-desc = Usa una pasarela SSH cuando el servidor de destino solo sea accesible a través de un bastión o host de salto.
ui-profile-direct-connect = Conectar directamente sin pasarela SSH
ui-profile-gateway-direct-hint = Está seleccionada la conexión directa. Desmárcala para enrutar esta sesión a través de una pasarela.
ui-profile-gateway-explain-tunnel = El tráfico se enrutará a través de esta pasarela SSH.
ui-profile-socks-title = Proxy SOCKS5
ui-dialog-post-connect-title = ¿Ejecutar comandos posteriores a la conexión?
ui-dialog-post-connect-body = { $count ->
    [one] "{ $name }" se importó y ejecutará automáticamente { $count } comando en esta sesión. Continúa solo si confías en este perfil. ¿Ejecutarlo y recordar esta elección?
   *[other] "{ $name }" se importó y ejecutará automáticamente { $count } comandos en esta sesión. Continúa solo si confías en este perfil. ¿Ejecutarlos y recordar esta elección?
}
ui-dialog-post-connect-run = Ejecutar y recordar
ui-dialog-post-connect-skip = Conectar sin ellos
ui-profile-toggle-forward-agent = Reenviar agente SSH
ui-profile-toggle-compression = Activar compresión
ui-profile-options-ssh = Opciones SSH
ui-post-connect-title = Secuencia posterior a la conexión
ui-post-connect-hint = Estos pasos se ejecutan una vez lista la sesión SSH incrustada. Los retrasos se aplican antes de cada paso.
ui-post-connect-empty = Aún no hay pasos. Añade un paso para enviar comandos automáticamente en cuanto se conecte esta sesión.
ui-post-connect-command = Comando
ui-post-connect-command-placeholder = Comando a enviar, por ejemplo: sudo -i
ui-post-connect-delay = Retraso (ms)
ui-post-connect-on-failure = En caso de fallo
ui-post-connect-failure-continue = Continuar
ui-post-connect-failure-stop = Detener secuencia
ui-post-connect-order-hint = El orden importa. Los retrasos se aplican antes de cada paso activado.
ui-post-connect-add = Añadir
ui-post-connect-remove = Quitar
ui-post-connect-move-up = Subir
ui-post-connect-move-down = Bajar
ui-post-connect-tooltip = { $progress } - { $status } - { $command }
ui-post-connect-running = En ejecución
ui-post-connect-completed = Completado
ui-post-connect-failed = Fallido
ui-post-connect-skipped = Omitido
ui-post-connect-cancelled = Cancelado
ui-profile-socks-desc = Abre un puerto proxy SOCKS5 local a través de la pasarela. Pon 0 para desactivarlo.
ui-profile-socks-port = Puerto local
ui-profile-socks-off = Desactivado
ui-profile-remote-title = Reenvío de puerto remoto
ui-profile-remote-desc = Abre un puerto en el servidor SSH y reenvía las conexiones a un puerto local. El puerto remoto es obligatorio; deja el puerto local en 0 para usar el mismo valor.
ui-profile-remote-bind-port = Puerto remoto (servidor)
ui-profile-remote-local-port = Puerto local
ui-profile-remote-local-hint = 0 = igual que el puerto remoto
ui-profile-remote-route = servidor:{ $remote } -> local:{ $local }
ui-profile-gateway-explain-direct = Selecciona una pasarela si el servidor solo es accesible a través de un host de salto SSH.
ui-profile-edit-gateway = Editar credenciales de la pasarela...
ui-gateway-list-empty = No hay pasarelas configuradas
ui-gateway-empty-hint = Añade una pasarela SSH para establecer conexiones seguras con túnel a tus sesiones.
ui-gateway-add = Añadir pasarela
ui-gateway-add-title = Añadir pasarela SSH
ui-gateway-edit-title = Editar pasarela SSH
ui-gateway-field-name = Nombre
ui-gateway-field-host = Host
ui-gateway-field-port = Puerto
ui-gateway-field-username = Usuario
ui-gateway-field-key = Ruta de la clave
ui-gateway-field-password = Contraseña
ui-gateway-password-hint = Usado para la autenticación SSH por contraseña. Déjalo en blanco si usas autenticación solo con clave o agente SSH.
ui-gateway-field-parent = Pasarela principal
ui-gateway-parent-none = Ninguna (conexión directa)
ui-gateway-error-loop = Una pasarela no puede alcanzarse a través de sí misma.
ui-tree-gateway-via = vía { $name }
ui-tree-gateway-missing = falta la pasarela

ui-tab-menu-disconnect = Desconectar
ui-tab-menu-rename = Renombrar pestaña
ui-tab-menu-reset-title = Restablecer título
ui-tab-menu-fullscreen = Pantalla completa (F11)
ui-tab-menu-reconnect = Reconectar sesión
ui-tab-menu-duplicate = Duplicar sesión
ui-tab-menu-close-others = Cerrar las demás
ui-tab-menu-close-right = Cerrar las de la derecha
ui-dialog-rename-tab-title = Renombrar pestaña
ui-dialog-rename-tab-prompt = Introduce el nuevo nombre de la pestaña:
ui-dialog-close-tabs-title = Cerrar sesiones
ui-dialog-close-tabs-body = Sesiones a cerrar: { $count }. Aún conectadas: { $live }. ¿Continuar?

ui-session-copy-error-button = Copiar error
ui-session-edit-profile-button = Editar perfil
ui-error-report-header = Informe de error { $protocol } de Heimdall
ui-error-report-time = Hora:
ui-error-report-server = Servidor:
ui-error-report-app = Aplicación:

ui-error-network-refused = Conexión rechazada.
ui-error-network-reset = Conexión reiniciada.
ui-error-network-timed-out = Se agotó el tiempo de conexión. Comprueba que el host es accesible.
ui-error-network-unreachable = El host o la red no son accesibles. Comprueba el DNS y el enrutamiento.
ui-session-closed-reason = El servidor indicó: { $reason }

## Why an RDP server refused a logon or ended a session, as the C# Heimdall says it.
ui-rdp-severity-warning = Advertencia:
ui-rdp-severity-error = Error:
ui-rdp-reason-bad-credentials = No se aceptaron las credenciales. Verifica tu usuario, contraseña y dominio (NetBIOS DOMINIO\usuario o UPN usuario@dominio.com), luego vuelve a conectar.
ui-rdp-reason-password-expired = La contraseña ha caducado y debe cambiarse antes de conectar.
ui-rdp-reason-account-locked-out = La cuenta está bloqueada actualmente.
ui-rdp-reason-account-disabled = La cuenta está deshabilitada en el equipo remoto. Pide a tu administrador que la habilite, luego vuelve a intentar conectar.
ui-rdp-reason-account-expired = La cuenta ha caducado.
ui-rdp-reason-time-of-day = No se permite iniciar sesión con esta cuenta en este momento. Una restricción de horario de inicio de sesión en la cuenta terminó la sesión.
ui-rdp-reason-no-authority = No se pudo contactar con ninguna autoridad de autenticación para validar la cuenta. El equipo remoto podría haber perdido el contacto con su controlador de dominio.
ui-rdp-reason-clock-skew = Los relojes de este equipo y del equipo remoto están demasiado desfasados para que la autenticación tenga éxito. Corrige la hora del sistema en uno de los dos, luego vuelve a conectar.
ui-rdp-reason-security-error = Un error de seguridad impidió la conexión. El equipo remoto informó de que los datos de seguridad intercambiados durante la conexión no eran válidos.
ui-rdp-reason-admin-disconnect = El equipo remoto terminó la sesión. Un administrador podría haberla finalizado, la conexión pudo fallar mientras se establecía, o un problema de red pudo haberla interrumpido.
ui-rdp-reason-license = Un error de licencias de Escritorio remoto bloqueó la sesión. Contacta con tu administrador; el servidor de licencias podría ser inaccesible o quedarse sin CAL.
ui-rdp-reason-with-severity = { $severity } { $reason }
ui-rdp-certificate-refused = Conexión cancelada: no aprobaste el certificado que presentó este servidor.

ui-session-reconnecting = Reconectando (intento { $attempt }/{ $max })...
ui-session-reconnecting-in = en { $seconds }s
ui-session-reconnecting-cancel = Cancelar

ui-folder-connect-all = Conectar todo ({ $count })
ui-folder-new = Nueva carpeta
ui-folder-rename = Renombrar
ui-folder-move-to = Mover a
ui-folder-move-top = Nivel superior
ui-folder-delete = Eliminar carpeta
ui-folder-new-title = Nueva carpeta
ui-folder-rename-title = Renombrar carpeta
ui-folder-name-field = Nombre de la carpeta:
ui-folder-error-collision = Ya existe una carpeta con este nombre en el mismo nivel.
ui-folder-error-invalid = Un nombre de carpeta no puede estar vacío ni contener "/".
ui-folder-delete-body = ¿Eliminar la carpeta "{ $name }"? Entradas afectadas en esta carpeta y sus subcarpetas, incluidas las ocultas por el filtro actual: { $count }. Todas se moverán a "(Sin carpeta)".
ui-folder-connect-all-title = Conectar todo
ui-folder-connect-all-body = ¿Conectar a las { $count } sesiones de esta carpeta?
ui-folder-connect-all-confirm = Conectar

ui-tree-rename = Renombrar
ui-tree-rename-title = Cambiar nombre de la sesión
ui-tree-move-to-folder = Mover a carpeta

ui-selection-count = { $count } elementos seleccionados
ui-selection-connect = Conectar seleccionados ({ $count })
ui-selection-duplicate = Duplicar seleccionados
ui-selection-delete = Eliminar seleccionados ({ $count })
ui-dialog-delete-selection-title = Eliminar elementos seleccionados
ui-dialog-delete-selection-body = ¿Seguro que quieres eliminar { $count ->
    [one] { $count } elemento seleccionado
   *[other] { $count } elementos seleccionados
}?

ui-palette-placeholder = Buscar host o IP... (Ctrl+K)
ui-palette-ssh-to = [SSH] Conectar a { $target }
ui-palette-rdp-to = [RDP] Conectar a { $target }
ui-palette-quick-connect = Conexión rápida
ui-palette-nothing = Ninguna sesión coincide, y no es un host al que conectarse.

ui-status-ready = Listo. Selecciona una sesión para empezar.
ui-status-connected = Conectado a: { $name }
ui-status-state = { $name }: { $state }
ui-status-connecting = Conectando...
ui-status-reconnecting = Reconectando...
ui-status-disconnected = Desconectado
ui-status-error = Error
ui-status-copied = Copiado al portapapeles: { $text }
ui-status-folder-created = Carpeta "{ $path }" creada.
ui-status-sessions = { $count ->
    [one] { $count } sesión
   *[other] { $count } sesiones
}
ui-status-sessions-filtered = { $shown } de { $count ->
    [one] { $count } sesión
   *[other] { $count } sesiones
}

ui-find-placeholder = Buscar...
ui-find-previous = ▲
ui-find-next = ▼
ui-find-close = ✕
ui-find-nothing = No coincide

ui-settings-terminal = Apariencia de la terminal
ui-settings-color-scheme = Esquema de color
ui-settings-appearance = Apariencia
ui-vault-problem-locked-out = Demasiados intentos incorrectos. Inténtalo de nuevo en { $minutes ->
    [one] { $minutes } minuto.
   *[other] { $minutes } minutos.
}
ui-settings-language = Idioma
ui-settings-language-en = Inglés
ui-settings-language-fr = Français
ui-settings-language-es = Español
ui-settings-font-size = Tamaño de fuente
ui-settings-font-size-unit = px
ui-settings-font-size-refused = El tamaño de fuente de la terminal debe estar entre { $min } y { $max }.
ui-scheme-default = Predeterminado
ui-scheme-dracula = Dracula
ui-scheme-solarized-dark = Solarized oscuro
ui-scheme-monokai = Monokai
ui-scheme-nord = Nord

ui-tab-menu-start-transcript = Iniciar transcripción
ui-tab-menu-stop-transcript = Detener transcripción
ui-tab-recording = REC
ui-tab-recording-tooltip = Se está grabando la salida de la sesión
ui-status-transcript-started = Transcripción iniciada: { $path }
ui-status-transcript-stopped = Transcripción detenida
ui-status-transcript-failed = No se pudo escribir la transcripción y se detuvo: { $reason }
ui-transcript-header = ===== Sesión iniciada { $started } | { $protocol } | host { $host } | { $title } =====
ui-transcript-footer = ===== Sesión finalizada { $ended } | duración { $duration } =====
ui-settings-session-logging = Registro de sesión
ui-settings-ssh-auto-reconnect = Reconexión automática SSH
ui-settings-ssh-auto-reconnect-description = Reintentar automáticamente una sesión SSH que se desconecta de forma inesperada. Desactivado de forma predeterminada.
ui-settings-ssh-auto-reconnect-enable = Activar reconexión automática limitada
ui-settings-ssh-auto-reconnect-attempts = Intentos máximos antes de pasar a reconexión manual
ui-settings-session-log-directory = Directorio de registro de sesiones:
ui-settings-session-log-directory-hint = Carpeta de los registros de sesión, relativa a la carpeta de configuración salvo si es absoluta. Pulsa Intro para aplicar.

ui-broadcast-button = DIFUSIÓN
ui-broadcast-toggle-tooltip = Activar/desactivar la difusión (enviar a todos los terminales), Ctrl+Alt+B
ui-broadcast-on = Modo difusión ACTIVADO - { $scope }
ui-broadcast-off = Modo difusión DESACTIVADO
ui-broadcast-scope-all = Todas las pestañas
ui-broadcast-scope-selected = Pestañas seleccionadas ({ $count })
ui-broadcast-scope-status = Ámbito de difusión: { $scope }
ui-broadcast-scope-tooltip = Alcance de la difusión (clic para alternar entre todas las pestañas y las pestañas marcadas)
ui-broadcast-target-on = ◉
ui-broadcast-target-off = ○
ui-broadcast-target-tooltip = Enviar la entrada de difusión a esta sesión (destino de difusión)
ui-dialog-broadcast-title = ¿Difundir a todas las pestañas?
ui-dialog-broadcast-body = Lo que escribas se enviará a los paneles de terminal de todas las pestañas abiertas, incluidas las que se ejecutan en segundo plano. ¿Continuar?
ui-dialog-broadcast-confirm = Difundir

ui-files-go-button = Ir

ui-files-column-name = Nombre
ui-files-column-size = Tamaño
ui-files-column-modified = Modificado
ui-files-column-permissions = Permisos
ui-files-column-owner = Propietario
ui-files-sorted-ascending = { $column } ▲
ui-files-sorted-descending = { $column } ▼

ui-files-menu-open = Abrir
ui-files-menu-download = Descargar
ui-files-menu-upload = Enviar
ui-files-menu-rename = Renombrar
ui-files-menu-delete = Eliminar
ui-files-menu-copy-path = Copiar ruta
ui-files-menu-new-folder = Nueva carpeta
ui-files-menu-refresh = Actualizar

ui-files-menu-permissions = Cambiar permisos...
ui-files-menu-properties = Propiedades
ui-dialog-permissions-title = Cambiar permisos
ui-dialog-permissions-label = Permisos (octal, por ejemplo 755):
ui-dialog-permissions-placeholder = 755
ui-dialog-permissions-confirm = Aplicar
ui-files-error-invalid-permissions = Los permisos son de uno a cuatro dígitos octales, como 755 o 4755.
ui-files-properties-title = Propiedades - { $name }
ui-files-properties-name = Nombre:
ui-files-properties-type = Tipo:
ui-files-properties-size = Tamaño:
ui-files-properties-modified = Modificado:
ui-files-properties-permissions = Permisos:
ui-files-properties-owner = Propietario:
ui-files-properties-group = Grupo:
ui-files-properties-path = Ruta:
ui-files-type-file = Archivo
ui-files-type-directory = Directorio
ui-files-type-link = Enlace simbólico
ui-files-type-other = Tipo desconocido

ui-files-selected-count = { $count ->
    [one] { $count } seleccionado
   *[other] { $count } seleccionados
}
ui-dialog-delete-many-body = ¿Eliminar { $count } elementos? Las carpetas se eliminan con todo su contenido. Esto no se puede deshacer.

ui-files-bookmark-button = Marcar esta ruta
ui-files-bookmarks-button = Marcadores
ui-files-bookmarks-empty = No hay marcadores guardados
ui-files-bookmark-added = Marcador añadido: { $path }

ui-files-filter-placeholder = Filtrar archivos...
ui-files-hidden-toggle = .*
ui-files-hidden-tooltip = Mostrar archivos ocultos
ui-files-item-count = { $count } elementos
ui-files-item-count-filtered = { $shown }/{ $count } elementos

ui-files-drop-overlay = Suelta archivos para subirlos

ui-trusted-host-keys-title = Claves de host de confianza
ui-trusted-host-keys-hint = Revisa las claves de host SSH en las que Heimdall confía para futuras conexiones.
ui-trusted-host-keys-search = Buscar hosts de confianza
ui-trusted-host-keys-host = Host:Puerto
ui-trusted-host-keys-algorithm = Algoritmo
ui-trusted-host-keys-fingerprint = Huella
ui-trusted-host-keys-copy = Copiar huella
ui-trusted-host-keys-remove = Quitar
ui-trusted-host-keys-empty-title = No hay claves de host de confianza
ui-trusted-host-keys-empty-body = Conéctate primero a un servidor: se pregunta por su clave y luego aparece aquí.
ui-trusted-certificates-title = Certificados RDP de confianza
ui-trusted-certificates-hint = Certificados aceptados para un escritorio remoto, conservados entre reinicios. Olvidar uno lo quita de la lista de confianza de su servidor.
ui-trusted-certificates-search = Buscar por servidor o huella
ui-trusted-certificates-server = Servidor
ui-trusted-certificates-fingerprint = Huella
ui-trusted-certificates-forget = Olvidar
ui-trusted-certificates-empty-title = No hay certificados RDP de confianza
ui-trusted-certificates-empty-body = Aquí se listan los certificados que aceptas al conectar a un escritorio remoto, y pueden revocarse desde aquí.
ui-trusted-keys-unreadable = No se pudieron leer todas las claves de confianza: { $detail }
ui-dialog-forget-host-key-title = Quitar clave de host de confianza
ui-dialog-forget-host-key-body = ¿Quitar la clave de host de confianza de { $server }?
ui-dialog-forget-host-key-fingerprint = Huella: { $fingerprint }
ui-dialog-forget-host-key-consequence = Quitar esta clave de host de confianza obligará a verificarla de nuevo en la próxima conexión a { $server }.
ui-dialog-forget-host-key-confirm = Quitar
ui-dialog-forget-certificate-title = ¿Olvidar este certificado?
ui-dialog-forget-certificate-body = Heimdall olvidará el certificado { $fingerprint } de { $server }. Solo afecta a ese certificado; cualquier otro certificado de confianza para el mismo servidor lo sigue siendo.
ui-dialog-forget-certificate-keep = Mantener
ui-dialog-forget-certificate-confirm = Olvidar
ui-status-fingerprint-copied = Huella completa copiada para { $server }.
ui-status-host-key-removed = Clave de host de confianza quitada para { $server }.
ui-status-certificate-forgotten = Certificado olvidado para { $server }.

## Tunnels opened by hand, as the C# "New tunnel" dialog and tunnels panel say them.
ui-tunnel-new-title = Nuevo túnel
ui-tunnel-new-description = Crea un reenvío de puerto local limitado a la sesión a través de una de tus pasarelas SSH configuradas.
ui-tunnel-gateway-label = Pasarela
ui-tunnel-remote-host-label = Host remoto
ui-tunnel-remote-port-label = Puerto remoto
ui-tunnel-local-port-label = Puerto local
ui-tunnel-label-label = Etiqueta (opcional)
ui-tunnel-open-button = Abrir túnel
ui-tunnel-no-gateways = No hay ninguna pasarela SSH configurada. Añade una en Ajustes antes de crear un túnel.
ui-tunnel-problem-gateway = La pasarela es obligatoria.
ui-tunnel-problem-remote-host = El host remoto es obligatorio.
ui-tunnel-problem-remote-port = El puerto remoto debe estar entre { $min } y { $max }.
ui-tunnel-problem-local-port = El puerto local debe estar entre { $min } y { $max }.
ui-tunnel-problem-local-port-in-use = El puerto local { $port } ya lo está usando un túnel activo.
ui-tunnel-opened = Túnel abierto en el puerto local { $port } → { $host }:{ $remote }.
ui-tunnel-failed = Falló la creación del túnel: { $reason }
ui-tunnel-closed = Túnel en el puerto { $port } cerrado.
ui-tunnels-all-closed = Todos los túneles cerrados.
ui-tunnel-port-copied = Puerto { $port } copiado al portapapeles.
ui-tunnel-closed-reason = { $closed } ({ $reason })
ui-error-local-port-unavailable = El puerto local { $port } ya está en uso o reservado por el sistema.

## The tunnels panel and its status-bar button, as the C# ones.
ui-tunnels-header = Túneles ({ $count })
ui-tunnels-close-all = Cerrar todo
ui-tunnels-new = + Nuevo
ui-tunnels-collapse-tooltip = Contraer el panel de túneles
ui-tunnels-toggle-tooltip = Mostrar/ocultar el panel de túneles
ui-tunnels-column-gateway = Pasarela
ui-tunnels-column-label = Etiqueta
ui-tunnels-column-local = Local
ui-tunnels-column-remote = Remoto
ui-tunnels-column-port = Puerto
ui-tunnels-close-tooltip = Cerrar túnel
ui-tunnels-menu-close = Cerrar túnel
ui-tunnels-menu-copy-port = Copiar puerto local
ui-tunnels-menu-close-all = Cerrar todos los túneles
ui-tunnels-empty = No hay túneles activos
ui-tunnels-count =
    { $count ->
        [one] { $count } túnel
       *[other] { $count } túneles
    }
ui-tunnels-collapse-button = ▼

## An SSH key file not there, and what the agents offered a gateway that refused, as the C# says them.
ui-error-key-not-found = No se encontró el archivo de clave SSH: { $path }
ui-error-auth-agent-none = No había ninguna clave cargada en un agente SSH cuando Heimdall marcó esta pasarela, así que no se ofreció ninguna clave del agente. Si esta pasarela inicia sesión con una clave de agente, cárgala en Pageant o en el agente OpenSSH de Windows y vuelve a conectar; si no, comprueba los datos de inicio de sesión guardados para esta pasarela.
ui-error-auth-agent-one = Se cargó una clave en un agente SSH y se ofreció a esta pasarela; no fue aceptada. Si esta pasarela espera una clave distinta, cárgala y vuelve a conectar.
ui-error-auth-agent-many = Se cargaron { $count } claves en un agente SSH y se ofrecieron a esta pasarela; ninguna fue aceptada. Si esta pasarela espera una clave distinta, cárgala y vuelve a conectar.
ui-error-auth-with-agent = { $refused } { $agent }

## "Test address" in the profile form, as the C# dialog says it.
ui-address-test-button = Probar dirección
ui-address-test-hint = Comprueba que la dirección y el puerto respondan. No comprueba tu usuario ni contraseña.
ui-address-test-running = Probando la dirección...
ui-address-test-success = La dirección responde: { $address } ({ $millis } ms). No se comprobaron las credenciales.
ui-address-test-success-ssh = La dirección responde y un servidor SSH contestó: { $banner }. No se comprobaron las credenciales.
ui-address-test-failure = La dirección no respondió: { $reason }
ui-address-test-direct-scope = Probado directamente desde este equipo, no a través de { $gateway }.
ui-address-test-cancel = Cancelar
ui-address-test-dns-timeout = Se agotó el tiempo de espera de la búsqueda DNS.
ui-address-test-dns-failed = Falló la búsqueda DNS: { $reason }
ui-address-test-dns-no-results = La búsqueda DNS no devolvió ninguna dirección.
ui-address-test-tcp-timeout = Se agotó el tiempo de espera de la conexión TCP (host: { $address }). El host podría estar apagado, ser inaccesible, o el puerto podría estar bloqueado.
ui-address-test-tcp-failed = No se pudo conectar con { $address }: { $reason }
ui-address-test-cancelled = Prueba cancelada.
ui-address-test-scoped = { $verdict } { $scope }

## The gateway dialog's "Test route", as the C# card.
ui-route-test-title = Probar y entender esta ruta
ui-route-test-workstation = Este equipo
ui-route-test-target-host = Host de destino opcional (déjalo vacío para probar solo las pasarelas)
ui-route-test-target-port = Puerto TCP de destino
ui-route-test-test = Probar ruta
ui-route-test-stop = Detener prueba
ui-route-test-copy = Copiar informe de diagnóstico
ui-route-test-running = Probando la ruta. Los resultados aparecen después de cada paso.
ui-route-test-hop-step = Pasarela { $number }: conexión SSH y autenticación
ui-route-test-target-step = Acceso TCP al destino
ui-route-test-passed = Correcto
ui-route-test-trust-required = No probado: no hay clave de host de confianza. Verifícala y regístrala antes en las claves de host SSH de confianza.
ui-route-test-trust-changed = La clave de host es distinta. Verifica la identidad del servidor antes de actualizar las claves de host SSH de confianza.
ui-route-test-network = Conexión no disponible. Comprueba el host, el puerto, la VPN y el cortafuegos.
ui-route-test-forwarding = No se confirmó el acceso al destino. Comprueba la dirección, el puerto y los permisos de reenvío TCP en la pasarela.
ui-route-test-cancelled = Cancelado.
ui-route-test-interactive = Se requiere autenticación interactiva. Usa una conexión SSH interactiva para investigar el método requerido.
ui-route-test-auth = Fallo de autenticación. Comprueba la cuenta, la clave, la contraseña de la clave y el agente SSH.
ui-route-test-unavailable = Diagnóstico no disponible o fallo sin clasificar. Comprueba la configuración y los requisitos de autenticación.
ui-route-test-invalid-route = Ruta no válida: comprueba padres faltantes, ciclos y la profundidad máxima de la cadena.
ui-route-test-invalid-target = Introduce un nombre de host o dirección IP de destino válido y un puerto TCP entre 1 y 65535.
ui-route-test-report-header = Diagnóstico de ruta SSH de Heimdall (anonimizado: sin nombres de host, cuentas, rutas de clave ni errores en bruto)
ui-route-test-tcp-only = El paso de destino solo comprueba el acceso TCP. No valida el protocolo de la aplicación ni un inicio de sesión en el destino.
ui-route-test-no-target = No se especificó ningún destino. Solo se probaron las conexiones SSH de las pasarelas.
ui-route-test-hop = { $name } ({ $host }:{ $port })
ui-route-test-step-line = { $step }: { $outcome } ({ $millis } ms)
ui-route-test-hint = Prueba el formulario actual sin guardarlo. Usa solo claves de host de confianza. Las sesiones existentes permanecen abiertas.
ui-route-test-timeout = Tiempo de espera agotado. Comprueba la VPN, el enrutamiento y el cortafuegos.
ui-route-test-separator = {" "}→{" "}

## "Test reachability" in a profile's menu, as the C# tree says it in the status bar.
ui-tree-test-reachability = Probar accesibilidad
ui-status-reachability-testing = Probando { $host }:{ $port } ...
ui-status-reachability-success = { $host }:{ $port } accesible en { $millis } ms
ui-status-reachability-failed = { $host }:{ $port } no accesible: { $reason }

## An RDP tab's "Resolution" menu, as the C# one.
ui-resolution-menu = Resolución
ui-resolution-active-mode = Modo activo
ui-resolution-header = { $label }: { $mode }
ui-resolution-header-size = { $label }: { $mode } ({ $width }x{ $height })
ui-resolution-mode-fit-window = Ajustar a la ventana
ui-resolution-mode-fixed = Fija
ui-resolution-match-window = Igualar a la ventana
ui-resolution-custom = Personalizada...
ui-resolution-custom-title = Resolución personalizada
ui-resolution-custom-prompt = Introduce la resolución como ANCHOxALTO.
ui-resolution-custom-invalid = Resolución no válida. Usa ANCHOxALTO.
ui-resolution-save-default = Guardar como predeterminada para este servidor
ui-resolution-save-default-done = Resolución RDP predeterminada guardada para este servidor.
ui-resolution-save-default-unavailable = No se puede guardar una resolución predeterminada para esta sesión.

## The SSH agent chip of the profile form, as the C# one.
ui-agent-chip-off = No se detectó ningún agente SSH
ui-agent-chip-warn = Agente SSH: { $agent } (sin claves cargadas)
ui-agent-chip-ok = Agente SSH: { $agent } ({ $count } claves)
ui-agent-chip-tooltip = Haz clic para volver a escanear los agentes SSH
ui-files-menu-cut = Cortar
ui-files-menu-paste = Pegar
ui-status-files-cut = { $count ->
    [one] { $count } elemento cortado
   *[other] { $count } elementos cortados
}
ui-status-files-pasted = Pegado completo
ui-status-path-copied = Ruta copiada: { $path }
ui-files-menu-copy = Copiar
ui-files-menu-duplicate = Duplicar
ui-status-files-copied = { $count ->
    [one] { $count } elemento copiado
   *[other] { $count } elementos copiados
}
ui-status-files-duplicated = Duplicado completo
ui-files-error-copy-refused = Copia rechazada: este servidor no realizó la copia del lado del servidor, y Heimdall no recurrirá a una transferencia que podría sobrescribir un destino existente. Copia el archivo localmente, o comprueba que el servidor permite ejecutar cp, ln y mkdir.
ui-files-error-paste-into-itself = No se puede pegar { $name } dentro de sí mismo o de su propia subcarpeta.
ui-error-key-not-absolute = La ruta de la clave SSH debe ser absoluta: { $path }
ui-files-error-changed-on-server = El archivo cambió en el servidor desde que se abrió: se dejó como estaba.
ui-files-error-file-too-large = Los archivos de más de 16 MiB deben descargarse.
ui-files-menu-open-in-terminal = Abrir en terminal
ui-status-resolution-reconnected = El cambio de resolución requirió reconexión.
ui-resolution-mode-smart-sizing = Ajuste de tamaño inteligente
ui-resolution-tooltip = Cambiar resolución - { $mode }
ui-resolution-tooltip-size = Cambiar resolución - { $mode } ({ $width }x{ $height })
ui-resolution-larger-than-window = Mayor que la ventana; la imagen se escalará.
ui-files-menu-edit-external = Editar con editor externo
ui-status-files-editing = Editando: { $name } - guarda en tu editor para subir automáticamente
ui-status-files-auto-uploaded = Subido automáticamente: { $name }
ui-status-files-auto-upload-refused = La subida automática de { $name } fue rechazada y no se reintentará hasta que vuelvas a guardar: { $reason }
ui-files-error-working-folder-unprotected = El archivo no se abrió: su carpeta de trabajo local no pudo restringirse a tu cuenta, así que su contenido podría haber sido legible por otros usuarios de este equipo.
ui-files-error-editor-failed = No se pudo iniciar el editor externo: { $detail }. Comprueba la ruta del editor en Ajustes.
ui-files-error-editor-runs-files = Iniciar intérpretes de shell u hosts de scripts como editores está bloqueado por motivos de seguridad.
ui-settings-external-editor = Editor externo
ui-settings-external-editor-path = Ruta del editor externo
ui-settings-external-editor-hint = Ruta al editor de texto para la edición remota SFTP (déjalo vacío para el predeterminado del sistema)
ui-dialog-close-edits-body = "{ $name }" tiene un archivo abierto en un editor externo. ¿Cerrar el panel de todos modos? El editor permanece abierto, pero nada enviará su próximo guardado al servidor.
ui-files-edits-title = Editados en un editor externo
ui-files-edit-watching = Cada guardado en el editor se envía al servidor.
ui-files-edit-send-anyway = Enviar mi versión
ui-files-edit-open-folder = Abrir carpeta
ui-files-edit-stop = Dejar de editar
