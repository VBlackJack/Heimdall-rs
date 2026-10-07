# Copyright 2026 Julien Bombled
# Licensed under the Apache License, Version 2.0. See LICENSE.

ui-window-title = Heimdall
ui-window-title-tab = { $tab } - Heimdall
ui-window-title-detached = { $tab } - Separada

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

ui-session-closed = Sesión finalizada.
ui-session-closed-status = Sesión finalizada: el proceso terminó con el código { $status }.
ui-session-cancelled = La conexión se ha cancelado.
ui-session-failed-title = La conexión ha fallado
ui-session-close-button = Cerrar la pestaña

ui-error-invalid-host = El nombre de host no es válido.
ui-error-invalid-username = El nombre de usuario no es válido.
ui-error-network = No se pudo contactar con el servidor: { $detail }
ui-error-timeout = El servidor no respondió a tiempo.
ui-error-hostkey-changed = La clave del servidor no es la registrada. Alguien podría estar interceptando la conexión. Registrada: { $recorded }. Presentada: { $offered }. Si el cambio es esperado, olvide este servidor: su nueva clave se le preguntará entonces.
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
ui-session-vnc-quality = Calidad
ui-session-vnc-quality-best = Mejor calidad
ui-session-vnc-quality-balanced = Equilibrado
ui-session-vnc-quality-performance = Rendimiento
ui-session-vnc-quality-low-bandwidth = Bajo ancho de banda
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
ui-dialog-run-script-title = ¿Ejecutar este script?
ui-dialog-run-script-body = { $name } se ejecuta en una nueva pestaña con el comando siguiente, con tus permisos. Heimdall vuelve a preguntar cada vez que se ejecuta.
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
ui-tree-import-citrix = Importar aplicaciones de Citrix
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
ui-profile-protocol-citrix-name = Citrix
ui-profile-protocol-citrix-desc = Aplicación publicada de Citrix Workspace
ui-profile-citrix-title = Citrix Workspace
ui-profile-citrix-desc = Configura la URL de StoreFront y el nombre de la aplicación, o proporciona un archivo ICA directo.
ui-profile-field-storefront-url = URL de StoreFront
ui-profile-field-app-name = Nombre de la aplicación
ui-profile-citrix-advanced-title = Opciones avanzadas de Citrix
ui-profile-citrix-advanced-desc = Ajustes de archivo ICA, modo transparente e inicio de sesión único.
ui-profile-field-ica-file = Ruta del archivo ICA
ui-profile-citrix-hint = Proporciona una URL de StoreFront + nombre de aplicación, o una ruta de archivo ICA directa.
ui-profile-toggle-seamless = Modo transparente
ui-profile-toggle-sso = Usar SSO (Kerberos)
ui-profile-toggle-sso-hint = Usar inicio de sesión único con la identidad Kerberos de Windows actual. Requiere un cliente unido al dominio.
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
ui-tab-menu-detach = Separar a ventana
ui-detach-reattach = Volver a adjuntar a la ventana principal
ui-tab-menu-close-others = Cerrar las demás
ui-tab-menu-close-right = Cerrar las de la derecha
ui-split-merge-with = Fusionar con...
ui-split-horizontal = Horizontal
ui-split-vertical = Vertical
ui-split-unsplit = Quitar división
ui-split-swap-panes = Intercambiar paneles
ui-split-toggle-orientation = Alternar orientación de la división
ui-split-close-secondary = Cerrar panel secundario
ui-split-detach-secondary = Separar panel secundario
ui-split-max-panes-reached = Se alcanzó el número máximo de paneles ({ $max }).
ui-status-detach-split-refused = Una pestaña dividida no se puede mover a su propia ventana. Deshaga primero la división.
ui-split-menu = Dividir...
ui-split-palette-hint = Buscar servidor con el que dividir...
ui-split-drop-to-split = Soltar para dividir
ui-tab-drag-detach-hint = Soltar para separar a una ventana
ui-split-open-in-split = Abrir en división
ui-split-open-in-split-disabled = Abre primero una sesión: una división reparte la sesión activa.
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
ui-session-closed-reason = Motivo: { $reason }

## Why an RDP server refused a logon or ended a session, as the C# Heimdall says it.
ui-rdp-severity-warning = Advertencia:
ui-rdp-severity-error = Error:
ui-rdp-reason-bad-credentials = No se aceptaron las credenciales. Verifica tu usuario, contraseña y dominio (NetBIOS DOMINIO\usuario o UPN usuario@dominio.com), luego vuelve a conectar.
ui-rdp-reason-password-expired = La contraseña ha caducado y debe cambiarse antes de conectar.
ui-rdp-reason-account-locked-out = La cuenta está bloqueada actualmente. Espere a que termine el bloqueo o pida a su administrador que la desbloquee.
ui-rdp-reason-account-disabled = La cuenta está deshabilitada en el equipo remoto. Pide a tu administrador que la habilite, luego vuelve a intentar conectar.
ui-rdp-reason-account-expired = La cuenta ha caducado. Pida a su administrador que la renueve.
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
ui-status-connected-short = Conectado
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
ui-files-local-toggle = Archivos locales
ui-files-local-toggle-tooltip = Mostrar los archivos de este equipo junto a los del servidor
ui-files-sudo-toggle = sudo
ui-files-sudo-tooltip = Explorar como root (sudo)
ui-files-follow-toggle = dir. act.
ui-files-follow-tooltip = Seguir el directorio de la terminal SSH
ui-files-sudo-on = Modo sudo activado: explorando como root
ui-files-sudo-off = Modo sudo desactivado
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
ui-trusted-certificates-subject = Sujeto
ui-trusted-certificates-issuer = Emisor
ui-trusted-certificates-trusted = De confianza desde
ui-trusted-certificates-forget = Olvidar
ui-trusted-certificates-forget-server = Olvidar el servidor
ui-trusted-certificates-empty-title = No hay certificados RDP de confianza
ui-trusted-certificates-empty-body = Aquí se listan los certificados que aceptas al conectar a un escritorio remoto, y pueden revocarse desde aquí.
ui-trusted-ftps-certificates-title = Certificados FTPS de confianza
ui-trusted-ftps-certificates-hint = Certificados aceptados para un servidor FTPS, conservados entre reinicios. Olvidar uno lo quita de la lista de confianza de su servidor.
ui-trusted-ftps-certificates-empty-title = No hay certificados FTPS de confianza
ui-trusted-ftps-certificates-empty-body = Aquí se listan los certificados que aceptas al conectar a un servidor FTPS, y pueden revocarse desde aquí.
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
ui-dialog-forget-server-certificates-title = ¿Olvidar los certificados de este servidor?
ui-dialog-forget-server-certificates-body = { $count ->
    [one] Heimdall olvidará { $count } certificado de confianza para { $server }. La próxima conexión a este servidor volverá a preguntar.
   *[other] Heimdall olvidará los { $count } certificados de confianza para { $server }. La próxima conexión a este servidor volverá a preguntar.
}
ui-status-fingerprint-copied = Huella completa copiada para { $server }.
ui-status-host-key-removed = Clave de host de confianza quitada para { $server }.
ui-status-certificate-forgotten = Certificado olvidado para { $server }.
ui-status-server-certificates-forgotten = Todos los certificados olvidados para { $server }.

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
ui-files-error-changed-since-confirmed = Cambió en el servidor desde que se confirmó la eliminación: se dejó como estaba.
ui-files-error-file-too-large = Los archivos de más de 16 MiB deben descargarse.
ui-files-menu-open-in-terminal = Abrir en terminal
ui-files-menu-open-in-explorer = Abrir en el Explorador
ui-files-menu-run-in-shell = Ejecutar en shell
ui-status-resolution-reconnected = El cambio de resolución requirió reconexión.
ui-resolution-mode-smart-sizing = Ajuste de tamaño inteligente
ui-resolution-tooltip = Cambiar resolución - { $mode }
ui-resolution-tooltip-size = Cambiar resolución - { $mode } ({ $width }x{ $height })
ui-resolution-larger-than-window = Mayor que la ventana; la imagen se escalará.
ui-files-menu-upload-here = Subir aquí...
ui-files-menu-edit-external = Editar con editor externo
ui-status-files-editing = Editando: { $name } - guarda en tu editor para subir automáticamente
ui-status-files-auto-uploaded = Subido automáticamente: { $name }
ui-status-files-auto-upload-refused = La subida automática de { $name } fue rechazada y no se reintentará hasta que vuelvas a guardar: { $reason }
ui-files-error-working-folder-unprotected = El archivo no se abrió: su carpeta de trabajo local no pudo restringirse a tu cuenta, así que su contenido podría haber sido legible por otros usuarios de este equipo.
ui-files-error-editor-failed = No se pudo iniciar el editor externo: { $detail }. Comprueba la ruta del editor en Ajustes.
ui-files-error-editor-runs-files = Iniciar intérpretes de shell u hosts de scripts como editores está bloqueado por motivos de seguridad.
ui-files-error-open-failed = No se pudo abrir en este equipo: { $detail }
ui-files-error-script-character = Este script no se ejecutó: su ruta contiene { $character }, que su intérprete leería como algo más que parte de la ruta. Cambie el nombre del archivo o de su carpeta para ejecutarlo.
ui-files-error-script-not-text = Este script no se ejecutó: su ruta no es un texto válido, que su intérprete no podría recibir tal cual.
ui-settings-external-editor = Editor externo
ui-settings-external-editor-path = Ruta del editor externo
ui-settings-external-editor-hint = Ruta al editor de texto para la edición remota SFTP (déjalo vacío para el predeterminado del sistema)
ui-dialog-close-edits-body = "{ $name }" tiene un archivo abierto en un editor externo. ¿Cerrar el panel de todos modos? El editor permanece abierto, pero nada enviará su próximo guardado al servidor.
ui-files-edits-title = Editados en un editor externo
ui-files-edit-watching = Cada guardado en el editor se envía al servidor.
ui-files-edit-send-anyway = Enviar mi versión
ui-files-edit-open-folder = Abrir carpeta
ui-files-edit-stop = Dejar de editar
ui-files-error-sudo-password-needed = sudo pide una contraseña en este servidor.
ui-files-error-sudo-password-rejected = sudo rechazó la contraseña.
ui-files-error-sudo-needs-terminal = sudo requiere un terminal en este servidor (requiretty): Heimdall no se lo da, así que nada se ejecutó como root. Permite sudo sin terminal para tu cuenta, o edita el archivo en un shell.
ui-files-error-sudo-untrusted = El sudo encontrado en el servidor no es el del sistema (no es set-user-id root): nada se ejecutó como root.
ui-files-error-sudo-tooling = Transferencia privilegiada rechazada: al servidor le falta una herramienta que necesita (GNU coreutils: stat, cp, sync, mv). Consulta el registro para ver el nombre de la herramienta.
ui-files-error-sudo-failed = Falló la autenticación de sudo.
ui-files-error-sudo-protected = No eliminado como root: "/", una carpeta de sistema de primer nivel, una carpeta personal en sí, o una ruta que no es absoluta o que sube con "..".
ui-files-menu-paste-explorer = Pegar desde el Explorador
ui-status-explorer-no-files = No hay archivos copiados en el Explorador.
ui-status-rdp-files-too-many = Archivos no copiados al servidor: una copia admite { $count } archivos y carpetas como máximo.
ui-status-rdp-files-too-large = Archivos no copiados al servidor: una copia admite { $size } como máximo.
ui-files-menu-edit-sudo = Editar con sudo
ui-files-edit-save-sudo = Guardar con sudo
ui-status-files-saved-sudo = Guardado mediante sudo: { $name }
ui-dialog-sudo-title = Contraseña de sudo
ui-dialog-sudo-body = sudo pide tu contraseña en este servidor para "{ $name }". Se guarda solo para esta pestaña, hasta que se cierre o sudo la rechace.
ui-dialog-sudo-delete-title = ¿Eliminar como root?
ui-dialog-sudo-delete-body = Estos elementos se eliminarán como root, una carpeta con todo su contenido. No se puede deshacer.
ui-dialog-sudo-delete-more = y { $count } más
ui-dialog-sudo-delete-confirm = Eliminar como root
ui-desktop-save-files = Guardar archivos copiados...
ui-desktop-save-files-tooltip = Guardar los archivos copiados en el servidor en una carpeta de este equipo
ui-desktop-saving-files = Guardando archivos: { $saved } de { $total }
ui-desktop-save-files-cancel = Detener
ui-status-rdp-files-saved = Archivos del servidor guardados: { $count }.
ui-status-rdp-files-save-failed = Archivos del servidor incompletos: { $saved } de { $total } guardados antes del fallo.
ui-status-rdp-files-save-cancelled = Guardado detenido: { $saved } de { $total } guardados.
ui-status-rdp-files-not-saved-too-many = Archivos del servidor no guardados: una copia admite { $count } archivos y carpetas como máximo.
ui-status-rdp-files-not-saved-too-large = Archivos del servidor no guardados: una copia admite { $size } como máximo.
ui-status-rdp-files-not-saved-unknown-size = Archivos del servidor no guardados: el servidor no indicó su tamaño.
ui-files-menu-edit-integrated = Editar
ui-editor-save = Guardar
ui-editor-close = Cerrar
ui-editor-overwrite = Sobrescribir
ui-editor-opening = Abriendo { $name }...
ui-editor-position = Lín { $line }, Col { $column }
ui-editor-lines = { $count ->
    [one] { $count } línea
   *[other] { $count } líneas
}
ui-editor-plain-text = Texto sin formato
ui-editor-encoding-utf8 = UTF-8
ui-editor-encoding-utf8-bom = UTF-8 con BOM
ui-editor-encoding-utf16le = UTF-16 LE
ui-editor-encoding-utf16be = UTF-16 BE
ui-editor-encoding-utf32le = UTF-32 LE
ui-editor-encoding-utf32be = UTF-32 BE
ui-editor-encoding-latin1 = Latin-1
ui-editor-ending-lf = LF
ui-editor-ending-crlf = CRLF
ui-editor-ending-cr = CR
ui-editor-notice-latin1 = Este archivo no es UTF-8 válido y se abrió como Latin-1. Al guardar se escribe de nuevo en Latin-1.
ui-editor-notice-saved = Guardado.
ui-editor-notice-changed = El archivo cambió en el servidor desde que se abrió: no se guardó. Sobrescríbalo, o cierre sin guardar.
ui-editor-notice-unencodable = No guardado: el carácter de la línea { $line }, columna { $column } no se puede almacenar en Latin-1.
ui-editor-notice-save-running = El guardado sigue en curso.
ui-editor-notice-session-ended = La sesión terminó: sus cambios se conservan. Vuelva a conectar para guardarlos.
ui-editor-notice-failed = Error al guardar: { $reason }
ui-dialog-discard-editor-title = Cambios sin guardar
ui-dialog-discard-editor-body = El archivo tiene cambios sin guardar. ¿Cerrar de todos modos?
ui-dialog-close-editor-body = El editor de "{ $name }" tiene cambios sin guardar. ¿Cerrar y descartarlos?
ui-dialog-unsaved-editors = { $count ->
    [one] { $count } editor tiene cambios sin guardar, que se perderían.
   *[other] { $count } editores tienen cambios sin guardar, que se perderían.
}
ui-files-error-looks-binary = Este archivo parece binario (un archivo comprimido, una imagen o un programa): descárguelo en su lugar.
ui-files-error-not-text = La marca de codificación de este archivo no corresponde a su contenido: no se puede abrir como texto.
ui-files-error-too-large-for-editor = Demasiado grande para el editor integrado (más de { $size }): use editar con el editor externo.
ui-dialog-binary-title = Archivo binario
ui-dialog-binary-body = "{ $name }" parece un archivo binario (un archivo comprimido, una imagen o un programa) y no se puede mostrar como texto. ¿Descargarlo en su lugar?
ui-dialog-binary-confirm = Descargar
ui-dialog-open-link-title = Abrir enlace
ui-dialog-open-link-body = El texto pulsado lleva a esta dirección, que se abrirá en su navegador:
    { $url }
ui-dialog-open-link-confirm = Abrir
ui-dialog-open-runnable-title = Abrir un programa
ui-dialog-open-runnable-body = Este archivo se ejecuta como un programa en este equipo, con sus permisos. Ábralo solo si confía en él:
    { $path }
ui-dialog-open-runnable-confirm = Abrir
ui-tab-menu-show-health = Mostrar el estado del servidor
ui-tab-menu-hide-health = Ocultar el estado del servidor
ui-health-cpu = CPU
ui-health-memory = Memoria
ui-health-disk = Disco
ui-health-waiting = Leyendo...
ui-health-unsupported = No compatible
ui-health-cpu-value = { $percent } %
ui-health-memory-value = { $used } / { $total } MB
ui-health-disk-value = { $used } / { $total }
ui-find-count = { $index } / { $total }
ui-winrm-diagnostic-logon-failed = El host remoto rechazó las credenciales. Compruebe el nombre de usuario y la contraseña (use la forma usuario@dominio para una cuenta de dominio) y que la cuenta no esté bloqueada.
ui-winrm-diagnostic-access-denied = Acceso denegado por el host remoto. La cuenta necesita el derecho de usar WinRM: pida a su administrador que la añada al grupo Usuarios de administración remota del host.
ui-winrm-diagnostic-trusted-hosts = WinRM rechazó la conexión porque el host no es de confianza para esta autenticación. Use el nombre DNS del host en lugar de su dirección IP, use HTTPS, o pida a su administrador que añada el host a la lista TrustedHosts de este equipo.
ui-winrm-diagnostic-kerberos-principal = La autenticación Kerberos falló para este host. Conéctese con el nombre DNS completo del host en lugar de su dirección IP o un alias corto, y compruebe que el host pertenece a su dominio.
ui-winrm-diagnostic-session-not-entered = No se pudo abrir la sesión WinRM remota. Lea el mensaje de PowerShell de arriba para conocer la causa, y compruebe el nombre de host, el puerto y la identidad de este perfil.
ui-profile-winrm-identity-hint = Kerberos o NTLM se negocia automáticamente. Kerberos necesita el nombre DNS del host, no su dirección IP. Por HTTP el contenido sigue cifrado por Kerberos o NTLM, pero NTLM no verifica la identidad del servidor.
ui-profile-winrm-trusted-hosts-hint = Fuera de un dominio, NTLM por HTTP requiere que el host esté en la lista TrustedHosts de este equipo, o usar HTTPS. Heimdall nunca modifica TrustedHosts.
ui-profile-winrm-https-off-by-gateway = HTTPS se desactivó porque hay una pasarela SSH seleccionada: WinRM por pasarela usa HTTP dentro del túnel. Quite la pasarela para restablecer HTTPS.
ui-error-winrm-tls-no-verify = La conexión TLS de WinRM a '{ $host }' en el puerto { $port } falló aunque la validación del certificado está omitida: el puerto probablemente no responde en TLS. Compruebe que el puerto { $port } es el agente de escucha WinRM HTTPS (normalmente 5986).
ui-files-state-rate = { $progress } - { $rate }/s, quedan { $left }
ui-files-eta-seconds = { $seconds } s
ui-files-eta-minutes = { $minutes } min { $seconds } s
ui-files-eta-hours = { $hours } h { $minutes } min
ui-files-conflict-replace-if-newer = Reemplazar si es más reciente
ui-files-conflict-incoming = Nuevo: { $size }, modificado el { $modified }
ui-files-conflict-existing = Existente: { $size }, modificado el { $modified }
ui-files-conflict-newer = El archivo nuevo es más reciente.
ui-files-conflict-older = El archivo nuevo es más antiguo.
ui-files-conflict-same-time = Misma fecha de modificación.
ui-files-conflict-unknown = desconocido
ui-settings-ssh-agent-preference = Preferencia de agente SSH
ui-settings-ssh-agent-openssh-first = Automático: OpenSSH de Windows primero
ui-settings-ssh-agent-pageant-first = Automático: Pageant primero
ui-settings-ssh-agent-openssh-only = Solo OpenSSH de Windows
ui-settings-ssh-agent-pageant-only = Solo Pageant
ui-settings-ssh-agent-preference-hint = Controla qué claves de agente SSH cargadas intenta primero Heimdall. Los cambios se aplican a la siguiente conexión.
ui-status-screenshot-copied = Captura de pantalla copiada al portapapeles
ui-status-screenshot-failed = No se pudo capturar la pantalla
ui-files-state-queued = En cola
ui-files-state-preparing = Preparando la transferencia...
ui-files-retry-button = Reintentar
ui-files-clear-finished-button = Borrar finalizadas
ui-files-error-interrupted = La transferencia se detuvo de forma inesperada.
ui-rdp-session-closed = La sesión de Escritorio remoto ha terminado.
ui-trusted-host-keys-export = Exportar known_hosts
ui-status-known-hosts-exported = { $count ->
    [one] Se exportó { $count } clave a { $path }.
   *[other] Se exportaron { $count } claves a { $path }.
}
ui-status-known-hosts-export-skipped = { $count ->
    [one] { $count } entrada omitida (sin clave pública capturada; vuelve a conectar para habilitar la exportación).
   *[other] { $count } entradas omitidas (sin clave pública capturada; vuelve a conectar para habilitar la exportación).
}
ui-status-known-hosts-export-failed = Falló la exportación de known_hosts: { $detail }
ui-status-known-hosts-export-no-home = Falló la exportación de known_hosts: no se conoce la carpeta personal.
ui-tree-favorite-add = Añadir a favoritos
ui-tree-favorite-remove = Quitar de favoritos
ui-tree-favorite = Favorito
ui-tree-filter-favorites = Favoritos
ui-profile-toggle-favorite = Marcar como favorito
ui-status-favorite-save-failed = No se pudo guardar el favorito.
ui-selection-edit = Editar
ui-selection-edit-port = Puerto...
ui-selection-edit-username = Usuario... ({ $count })
ui-bulk-port-header = { $count ->
    [one] Editando puerto en { $count } elemento
   *[other] Editando puerto en { $count } elementos
}
ui-bulk-port-label = Puerto
ui-bulk-port-mixed = Valores mixtos
ui-bulk-port-invalid = El puerto debe estar entre 1 y 65535.
ui-bulk-username-header = { $count ->
    [one] Editando el usuario en { $count } servidor
   *[other] Editando el usuario en { $count } servidores
}
ui-bulk-username-label = Usuario:
ui-bulk-username-mixed = valores mixtos
ui-bulk-username-invalid = El usuario no puede estar vacío ni contener caracteres de control (incluidos saltos de línea y tabulaciones).
ui-status-bulk-port-updated = { $count ->
    [one] Se actualizó el puerto en { $count } elemento.
   *[other] Se actualizó el puerto en { $count } elementos.
}
ui-status-bulk-port-unchanged = No se aplicó ningún cambio de puerto.
ui-status-bulk-username-updated = { $count ->
    [one] Usuario actualizado en { $count } servidor.
   *[other] Usuario actualizado en { $count } servidores.
}
ui-status-bulk-username-unchanged = No se aplicó ningún cambio: todos los servidores seleccionados ya usan este usuario.
ui-desktop-disconnect = Desconectar
ui-desktop-disconnect-tooltip = Desconectar sesión
ui-desktop-disconnect-title = ¿Desconectar Escritorio remoto?
ui-desktop-disconnect-body = Estás a punto de desconectarte de { $name }. ¿Continuar?
ui-settings-rdp-auto-reconnect-attempts = Intentos máximos de reconexión automática
ui-files-type-pipe = Tubería con nombre (FIFO)
ui-files-type-socket = Socket
ui-files-type-device = Dispositivo
ui-files-bookmark-removed = Marcador eliminado: { $path }
ui-files-bookmark-remove-menu = Eliminar un marcador
ui-files-empty-no-match = Ninguna entrada coincide con "{ $filter }".
ui-files-empty-clear-filter = Borrar filtro
ui-files-empty-hidden-only = Esta carpeta solo contiene entradas ocultas.
ui-files-empty-show-hidden = Mostrar archivos ocultos
ui-tunnels-status-active = Activo
ui-tunnels-status-interrupted = Interrumpido
ui-tunnels-menu-reopen = Reabrir
ui-settings-behavior = Comportamiento
ui-settings-collapse-tunnels-panel = Contraer el panel de túneles de forma predeterminada
ui-settings-collapse-tunnels-panel-hint = Cómo empieza el panel de túneles. Abrirlo o cerrarlo después lo deja así hasta que se cierra la aplicación.
ui-settings-prevent-sleep = Evitar la suspensión del sistema durante sesiones activas
ui-settings-prevent-sleep-hint = Evita que Windows se suspenda mientras haya una sesión conectada; la pantalla puede seguir apagándose o bloqueándose.
ui-settings-max-sessions = Máximo de sesiones incrustadas
ui-settings-max-sessions-none = Sin límite
ui-profile-session-logging = Registro de sesión
ui-profile-session-logging-inherit = Heredar
ui-profile-session-logging-on = Activado
ui-profile-session-logging-off = Desactivado
ui-profile-session-logging-hint = Heredar sigue el ajuste global de registro de sesión.
ui-hostkey-copy-fingerprint-button = Copiar
ui-certificate-already-trusted = { $count ->
    [one] Este perfil ya confía en { $count } certificado más para este nombre, lo que normalmente significa que más de una máquina responde a él.
   *[other] Este perfil ya confía en otros { $count } certificados para este nombre, lo que normalmente significa que varias máquinas responden a él.
}
ui-certificate-route = Alcanzado a través de: { $route }
ui-certificate-owner-tab = Esta pregunta pertenece a la pestaña "{ $tab }".
ui-settings-rdp-resolution-presets = Resoluciones predefinidas
ui-settings-rdp-resolution-presets-hint = Una resolución predefinida por línea, formato ANCHOxALTO (por ejemplo, 1920x1080). Deja el cuadro vacío para usar la lista integrada.
ui-settings-rdp-resolution-presets-reset = Restablecer valores predeterminados
ui-settings-rdp-resolution-presets-invalid = Resoluciones predefinidas: estas líneas no tienen el formato ANCHOxALTO con un ancho de { $min } a { $width } y un alto de { $min } a { $height } píxeles: { $lines }
ui-settings-rdp-reset-defaults = Restablecer valores predeterminados de RDP
ui-settings-rdp-reset-defaults-tooltip = Revierte solo los valores predeterminados de RDP a sus valores de fábrica. No se tocan otros ajustes.
ui-dialog-reset-rdp-title = ¿Restablecer valores predeterminados de RDP?
ui-dialog-reset-rdp-body = ¿Restaurar todos los valores predeterminados relacionados con RDP a sus valores de fábrica? Los servidores existentes no se ven afectados.
ui-settings-tab-about = Acerca de
ui-about-version = Versión { $version }
ui-about-tagline = Administrador seguro de conexiones RDP/SSH/SFTP
ui-about-section-system = Sistema
ui-about-platform = Plataforma
ui-about-author = Autor
ui-about-license = Licencia
ui-about-section-data = Datos
ui-about-sessions = Sesiones
ui-about-gateways = Pasarelas
ui-about-config-path = Configuración
ui-about-log-path = Registros
ui-about-section-links = Acceso rápido
ui-about-open-config = Abrir carpeta de configuración
ui-about-open-logs = Abrir carpeta de registros
ui-about-repository = GitHub
ui-about-section-diagnostics = Diagnóstico
ui-about-diagnostics-log = Escribir el registro de diagnóstico de la aplicación (eventos y errores de Heimdall)
ui-about-diagnostics-log-hint = Se aplica de inmediato. Un informe de fallo se escribe en todo caso: es el único rastro de un fallo.
ui-settings-provider-timeout = Tiempo de espera del comando
ui-settings-provider-timeout-seconds = { $seconds } s
ui-settings-provider-timeout-hint = Cuánto tiempo puede ejecutarse el comando de contraseña antes de que Heimdall lo abandone. Auméntalo para un almacén que pide confirmación.
ui-folder-color = Color
ui-folder-color-none = Sin color
ui-folder-color-blue = Azul
ui-folder-color-green = Verde
ui-folder-color-red = Rojo
ui-folder-color-amber = Ámbar
ui-folder-color-purple = Morado
ui-folder-color-pink = Rosa
ui-folder-color-cyan = Cian
ui-folder-color-orange = Naranja
ui-shortcuts-title = Atajos de teclado
ui-shortcuts-hint = F1 para atajos
ui-shortcuts-session-keys = En un terminal o un escritorio remoto, F1 y las teclas que usa la sesión van al servidor: abre esta lista desde la barra de estado.
ui-shortcuts-group-sessions = Sesiones
ui-shortcuts-group-tabs = Pestañas
ui-shortcuts-group-terminal = Terminal
ui-shortcuts-group-files = Archivos
ui-shortcuts-group-window = Ventana
ui-shortcuts-new-session = Añadir una sesión
ui-shortcuts-edit-session = Editar la sesión seleccionada
ui-shortcuts-quick-connect = Conexión rápida
ui-shortcuts-search = Buscar sesiones, o filtrar la lista de archivos
ui-shortcuts-next-tab = Pestaña siguiente
ui-shortcuts-previous-tab = Pestaña anterior
ui-shortcuts-close-tab = Cerrar la pestaña actual
ui-shortcuts-toggle-split = Alternar la orientación de la división
ui-shortcuts-next-pane = Panel siguiente de la pestaña dividida
ui-shortcuts-previous-pane = Panel anterior de la pestaña dividida
ui-shortcuts-find = Buscar en el terminal
ui-shortcuts-text-size = Texto más grande, más pequeño, tamaño original
ui-shortcuts-broadcast = Activar o desactivar la difusión de entrada
ui-shortcuts-terminal-copy = Copiar la selección
ui-shortcuts-terminal-paste = Pegar
ui-shortcuts-scroll-history = Desplazar el historial una página
ui-shortcuts-files-copy-cut = Copiar, cortar las entradas seleccionadas
ui-shortcuts-files-paste = Pegar entradas, o archivos copiados en el Explorador
ui-shortcuts-files-select-all = Seleccionar todo
ui-shortcuts-files-copy-path = Copiar la ruta completa
ui-shortcuts-files-download-upload = Descargar, subir la selección
ui-shortcuts-files-rename = Cambiar nombre
ui-shortcuts-files-new-folder = Nueva carpeta
ui-shortcuts-files-delete = Eliminar
ui-shortcuts-files-refresh = Actualizar
ui-shortcuts-files-back = Atrás, carpeta superior
ui-shortcuts-files-path = Escribir una ruta
ui-shortcuts-files-switch-pane = Otro panel
ui-shortcuts-full-screen = Alternar pantalla completa
ui-shortcuts-settings = Abrir la configuración
ui-shortcuts-screenshot = Capturar la pantalla
ui-shortcuts-lock = Bloquear
ui-shortcuts-help = Mostrar esta ayuda
ui-shortcuts-close = Cerrar un diálogo o un menú; salir de pantalla completa fuera de un terminal o de un escritorio remoto
ui-resolution-match-aspect = Igualar a la ventana, { $wide }:{ $high }
ui-tab-menu-pin = Fijar pestaña
ui-tab-menu-unpin = Dejar de fijar pestaña
ui-tab-menu-save-as-profile = Guardar como perfil...
ui-tab-menu-reveal-in-tree = Mostrar en el árbol
ui-tab-pinned-badge = fijada
ui-selection-set-gateway = Establecer pasarela... ({ $count })
ui-selection-gateway-direct = Conexión directa (sin pasarela)
ui-status-bulk-gateway-updated = { $count ->
    [one] Ruta de conexión actualizada en { $count } servidor.
   *[other] Ruta de conexión actualizada en { $count } servidores.
}
ui-status-bulk-gateway-unchanged = No se aplicó ningún cambio de ruta.
ui-shortcuts-select-all-sessions = Seleccionar todas las sesiones mostradas
ui-shortcuts-session-menu = Menú de la sesión seleccionada
ui-shortcuts-find-by-name = Ir a la sesión cuyo nombre empieza por lo escrito
ui-shortcuts-toggle-sidebar = Mostrar u ocultar la barra lateral
ui-restore-title = Restaurar sesiones anteriores
ui-restore-message = Heimdall encontró una instantánea de sesión guardada de la ejecución anterior. Selecciona las sesiones a restaurar.
ui-restore-saved-at = Guardado a las { $time }
ui-restore-select-all = Seleccionar todo
ui-restore-missing = Servidor no encontrado ({ $id })
ui-restore-files = Archivos { $protocol }
ui-restore-dont = No restaurar
ui-restore-selected = Restaurar seleccionadas
ui-profile-field-environment = Entorno
ui-profile-environment-none = (Ninguno)
ui-profile-environment-production = Producción
ui-profile-environment-staging = Pruebas
ui-profile-environment-lab = Laboratorio
ui-profile-environment-personal = Personal
ui-profile-field-tags = Etiquetas
ui-profile-tags-placeholder = Palabras con las que la búsqueda encuentra esta sesión
ui-profile-field-mac-address = Dirección MAC
ui-profile-mac-address-placeholder = AA:BB:CC:DD:EE:FF, para Wake on LAN
ui-profile-error-mac-address = La dirección MAC debe tener doce dígitos hexadecimales, como AA:BB:CC:DD:EE:FF.
ui-tree-wake-on-lan = Activar por LAN
ui-status-wake-on-lan-sent = Paquete mágico de Wake-on-LAN enviado.
ui-status-wake-on-lan-failed = No se pudo enviar el paquete de Wake-on-LAN: { $reason }
ui-tree-tooltip-environment = Entorno: { $environment }
ui-tree-tooltip-tags = Etiquetas: { $tags }
ui-files-batch-deleting = Eliminando { $name } ({ $index }/{ $total })...
ui-files-batch-permissions = Cambiando los permisos de { $name } ({ $index }/{ $total })...
ui-files-batch-stop = Cancelar
ui-files-batch-stopping = Deteniendo después de este...
ui-status-files-delete-failed = No se pudo eliminar "{ $name }": { $reason }
ui-status-files-permissions-failed = No se pudieron cambiar los permisos de "{ $name }": { $reason }
ui-status-files-delete-partial = { $failed ->
    [one] No se pudo eliminar { $failed } elemento de { $total }. "{ $name }": { $reason }
   *[other] No se pudieron eliminar { $failed } elementos de { $total }. El primero, "{ $name }": { $reason }
}
ui-status-files-permissions-partial = { $failed ->
    [one] No se pudieron cambiar los permisos de { $failed } elemento de { $total }. "{ $name }": { $reason }
   *[other] No se pudieron cambiar los permisos de { $failed } elementos de { $total }. El primero, "{ $name }": { $reason }
}
ui-status-files-delete-stopped = Eliminación cancelada: { $done } de { $total } elementos eliminados.
ui-status-files-permissions-stopped = Cambio de permisos cancelado: { $done } de { $total } elementos cambiados.
ui-macros-menu = Macros
ui-macros-record = Grabar macro
ui-macros-stop-recording = { $count ->
    [one] Detener la grabación ({ $count } entrada)
   *[other] Detener la grabación ({ $count } entradas)
}
ui-macros-stop = Detener "{ $name }"
ui-macros-play = Reproducir "{ $name }"
ui-macros-none = Aún no hay macros grabadas
ui-macros-empty = Aún no hay macros. Graba una desde el menú de una pestaña de terminal: Macros, Grabar macro.
ui-macros-inputs = { $count ->
    [one] { $count } entrada
   *[other] { $count } entradas
}
ui-macros-delete = Eliminar
ui-tab-recording-badge = REC
ui-tab-macro-badge = macro
ui-dialog-save-macro-title = Guardar macro
ui-dialog-save-macro-prompt = { $count ->
    [one] { $count } entrada grabada. Nombra la macro:
   *[other] { $count } entradas grabadas. Nombra la macro:
}
ui-dialog-save-macro-confirm = Guardar
ui-status-macro-nothing = No se escribió nada: no se grabó ninguna macro.
ui-status-macro-saved = Macro "{ $name }" guardada.
ui-status-macro-deleted = Macro "{ $name }" eliminada.
ui-status-macro-completed = Macro "{ $name }" reproducida.
ui-status-macro-stopped = Macro "{ $name }" detenida.
ui-status-macro-timed-out = Macro "{ $name }" detenida: lo que espera la entrada { $entry } no llegó a tiempo.
ui-status-macro-closed = Macro "{ $name }" detenida: la sesión terminó.
ui-macros-edit = Editar
ui-dialog-save-macro-warning = Se grabó todo lo escrito, contraseñas incluidas: la macro lo guarda tal cual.
ui-dialog-delete-macro-body = ¿Eliminar la macro "{ $name }"? Esto no se puede deshacer.
ui-macro-editor-title = Editar macro
ui-macro-editor-name = Nombre
ui-macro-editor-input-hint = Los caracteres de control se escriben \r (Intro), \n, \t, \xNN, y una barra invertida \\.
ui-macro-editor-input = Entrada
ui-macro-editor-delay = Retraso (ms)
ui-macro-editor-move-up = Subir
ui-macro-editor-move-down = Bajar
ui-macro-editor-delete-entry = Eliminar entrada
ui-macro-editor-expects = Esperar un texto antes
ui-macro-editor-pattern = Patrón esperado
ui-macro-editor-regex = Regex
ui-macro-editor-timeout = Tiempo de espera (ms)
ui-macro-editor-timeout-abort = Cancelar
ui-macro-editor-timeout-continue = Continuar
ui-macro-editor-add-expect = Añadir paso de espera
ui-macro-editor-add-send = Añadir paso de envío
ui-macro-editor-delete-macro = Eliminar macro
ui-macro-editor-name-required = El nombre de la macro es obligatorio.
ui-macro-editor-entry-invalid = Entrada { $entry }: { $reason }
ui-macro-editor-input-trailing = la entrada termina con una barra invertida sola.
ui-macro-editor-input-hex = \x necesita dos dígitos hexadecimales.
ui-macro-editor-input-escape = \{ $escape } no es un escape que el editor conozca.
ui-macro-editor-delay-invalid = el retraso no es un número de milisegundos.
ui-macro-editor-timeout-range = Rango: { $min }-{ $max } ms
ui-macro-editor-regex-invalid = Expresión regular no válida: { $reason }
ui-about-section-settings-file = Archivo de ajustes
ui-about-export-settings = Exportar ajustes...
ui-about-import-settings = Importar ajustes...
ui-about-settings-file-hint = Lleva tus preferencias a otro equipo. El archivo no contiene ningún secreto: ni contraseña maestra, ni PIN, ni contraseña guardada.
ui-settings-file-filter = Ajustes de Heimdall
ui-dialog-settings-export-title = Exportar ajustes
ui-dialog-settings-export-paths = { $count ->
    [one] { $count } ajuste indica una carpeta de tu perfil de usuario en este equipo (rutas de herramientas, carpeta de registros, archivos). ¿Incluirlo en el archivo?
   *[other] { $count } ajustes indican carpetas de tu perfil de usuario en este equipo (rutas de herramientas, carpeta de registros, archivos). ¿Incluirlos en el archivo?
}
ui-dialog-settings-export-without = Dejar fuera
ui-dialog-settings-export-with = Incluir
ui-dialog-settings-import-title = Importar ajustes
ui-dialog-settings-import-body = { $count ->
    [one] { $count } ajuste va a cambiar:
   *[other] { $count } ajustes van a cambiar:
}
ui-dialog-settings-import-line = { $key }: { $before } -> { $after }
ui-dialog-settings-import-confirm = Importar
ui-settings-value-on = Activado
ui-settings-value-off = Desactivado
ui-settings-value-empty = (vacío)
ui-settings-value-items = { $count ->
    [one] { $count } elemento
   *[other] { $count } elementos
}
ui-status-settings-exported = Ajustes exportados. El archivo no contiene ningún secreto: ni contraseña maestra, ni PIN, ni contraseña guardada.
ui-status-settings-export-failed = No se pudieron exportar los ajustes: { $reason }
ui-status-settings-imported = { $count ->
    [one] { $count } ajuste importado.
   *[other] { $count } ajustes importados.
}
ui-status-settings-import-nothing = El archivo contiene los ajustes que ya tienes. No hay nada que cambiar.
ui-status-settings-import-invalid = Este archivo no es un archivo de ajustes de Heimdall, o procede de una versión que esta no sabe leer. No se ha cambiado nada.
ui-status-settings-import-failed = No se pudo leer el archivo de ajustes: { $reason }
ui-settings-tab-gateways = Pasarelas
ui-gateways-title = Pasarelas SSH
ui-gateways-description = Revisa qué sesiones usan cada pasarela SSH y encuentra referencias de pasarela sin resolver.
ui-gateways-summary = { $gateways ->
    [one] { $gateways } pasarela
   *[other] { $gateways } pasarelas
}, { $routed ->
    [one] { $routed } sesión enrutada
   *[other] { $routed } sesiones enrutadas
}, { $unresolved ->
    [one] { $unresolved } referencia sin resolver
   *[other] { $unresolved } referencias sin resolver
}
ui-gateways-configured = Pasarelas configuradas
ui-gateways-empty = No hay pasarelas SSH configuradas.
ui-gateways-parent = Principal: { $name }
ui-gateways-sessions = { $count ->
    [one] { $count } sesión
   *[other] { $count } sesiones
}
ui-gateways-no-sessions = Ninguna sesión usa esta pasarela.
ui-gateways-edit = Editar
ui-gateways-delete = Eliminar
ui-gateways-unresolved = Referencias sin resolver
ui-gateways-missing-description = Estas sesiones o pasarelas hijas hacen referencia a un id de pasarela que no está configurado.
ui-gateways-missing-header = Falta el id de pasarela: { $id }
ui-gateways-child = Pasarela hija: { $name }
ui-gateways-reassign-to = Pasarela
ui-gateways-reassign = Reasignar
ui-gateways-clear = Borrar
ui-dialog-delete-gateway-title = Eliminar pasarela
ui-dialog-delete-gateway-body = ¿Eliminar la pasarela "{ $name }"?

    Referencias a borrar:
    - Servidores: { $servers }
    - Pasarelas hijas: { $gateways }
ui-status-gateway-deleted = Pasarela "{ $name }" eliminada.
ui-status-gateways-reassigned = { $count ->
    [one] Se reasignó { $count } sesión.
   *[other] Se reasignaron { $count } sesiones.
}
ui-status-gateways-cleared = { $count ->
    [one] Se borró la referencia de pasarela en { $count } sesión.
   *[other] Se borró la referencia de pasarela en { $count } sesiones.
}
ui-status-gateways-unchanged = Ninguna sesión necesitaba cambios.
ui-settings-reachability = Supervisor de estado de sesión
ui-settings-reachability-enabled = Activar sondeos de accesibilidad en segundo plano
ui-settings-reachability-hint = Cada servidor se llama desde este equipo, unos pocos a la vez, y su punto en la lista indica si respondió. Los servidores detrás de una pasarela no se llaman. No se envía nada salvo la conexión.
ui-settings-reachability-interval = Intervalo de comprobación
ui-settings-reachability-timeout = Tiempo de espera del sondeo
ui-settings-reachability-probes = Máximo de sondeos simultáneos
ui-settings-milliseconds-unit = ms
ui-settings-reachability-interval-refused = El intervalo de comprobación debe estar entre { $min } y { $max } segundos.
ui-settings-reachability-timeout-refused = El tiempo de espera del sondeo debe estar entre { $min } y { $max } ms.
ui-settings-reachability-probes-refused = El máximo de sondeos simultáneos debe estar entre { $min } y { $max }.
ui-tree-reachability-checking = Comprobando...
ui-tree-reachability-up = Accesible ({ $millis } ms)
ui-tree-reachability-down = No accesible: { $reason }
ui-tree-reachability-unchecked = Desconocido: { $reason }
ui-reachability-reason-timeout = Se agotó el tiempo de espera de la conexión
ui-reachability-reason-refused = Conexión rechazada
ui-reachability-reason-unreachable = Host no accesible
ui-reachability-reason-dns = Falló la resolución DNS
ui-reachability-reason-behind-gateway = Detrás de una pasarela SSH: no se sondeó
ui-reachability-reason-no-port = No hay puerto de sondeo para este protocolo
ui-reachability-reason-no-host = No hay ningún host configurado
ui-status-dropped-profiles = { $count ->
    [one] { $count } sesión movida a { $folder }. Ctrl+Z lo deshace.
   *[other] { $count } sesiones movidas a { $folder }. Ctrl+Z lo deshace.
}
ui-status-dropped-profiles-none = { $count ->
    [one] { $count } sesión sacada de su carpeta. Ctrl+Z lo deshace.
   *[other] { $count } sesiones sacadas de sus carpetas. Ctrl+Z lo deshace.
}
ui-status-dropped-folder = Carpeta { $name } movida. Ctrl+Z lo deshace.
ui-status-drop-refused = Ya hay una carpeta con ese nombre: no se movió nada.
ui-status-move-undone = Movimiento deshecho.
ui-status-listing-cancelled = Carga cancelada.
ui-status-session-limit = Ya está abierto el máximo de { $max } sesiones remotas incrustadas. Cierra una sesión antes de abrir otra.
ui-status-sftp-auto-open-failed = Falló la apertura automática de SFTP: { $reason }
ui-status-sftp-browser-disabled = El explorador SFTP integrado está desactivado en los ajustes.
ui-status-nothing-to-undo = Nada que deshacer.
ui-shortcuts-undo-move = Deshacer el último movimiento hecho arrastrando en el árbol
ui-tab-menu-vnc-remote-resize = Ajustar el escritorio remoto a la pestaña
ui-detail-folder = Carpeta:
ui-detail-environment = Entorno:
ui-detail-username = Usuario:
ui-detail-gateway = Pasarela:
ui-detail-credentials = Credenciales guardadas:
ui-detail-tags = Etiquetas:
ui-detail-favorite = Favorito:
ui-detail-yes = Sí
ui-detail-connect = Conectar
ui-detail-saved-password = contraseña
ui-detail-saved-key = archivo de clave { $name }
ui-detail-saved-passphrase = frase de contraseña de la clave
ui-detail-hints = Intro o doble clic conecta, Ctrl+E edita, Supr elimina, F1 muestra todos los atajos.
ui-detail-edit = Editar
ui-tree-notes = Notas
ui-notes-new = Nueva
ui-notes-daily = Diaria
ui-notes-incident = Incidente
ui-notes-procedure = Procedimiento
ui-notes-tpl-working-note = Nota de trabajo
ui-notes-tpl-notes = Notas
ui-notes-tpl-commands = Comandos
ui-notes-tpl-next = Siguiente
ui-notes-tpl-daily-note = Nota diaria
ui-notes-tpl-focus = Enfoque
ui-notes-tpl-journal = Diario
ui-notes-tpl-follow-up = Seguimiento
ui-notes-tpl-incident = Incidente
ui-notes-tpl-incident-report = Informe de incidente
ui-notes-tpl-summary = Resumen
ui-notes-tpl-impact = Impacto
ui-notes-tpl-timeline = Cronología
ui-notes-tpl-incident-started = Incidente iniciado
ui-notes-tpl-investigation = Investigación
ui-notes-tpl-actions = Acciones
ui-notes-tpl-resolution = Resolución
ui-notes-tpl-procedure = Procedimiento
ui-notes-tpl-purpose = Propósito
ui-notes-tpl-scope = Alcance
ui-notes-tpl-preconditions = Condiciones previas
ui-notes-tpl-steps = Pasos
ui-notes-tpl-validation = Validación
ui-notes-tpl-rollback = Reversión
ui-notes-tpl-references = Referencias
ui-about-open-notes = Abrir la carpeta de notas
ui-status-note-opened = Nota { $name } abierta en el editor.
ui-status-note-failed = No se pudo abrir la nota: { $reason }
ui-nav-sessions = Sesiones
ui-nav-tunnels = Túneles
ui-nav-settings = Ajustes
ui-nav-about = Acerca de
ui-tunnels-page-title = Túneles activos
ui-import-dropped-rd-gateway = a través de una puerta de enlace de Escritorio remoto
ui-error-rd-gateway = Este servidor se alcanza a través de la puerta de enlace de Escritorio remoto { $gateway }, que el cliente integrado todavía no sabe atravesar.
ui-profile-resolution-auto = Automático (recomendado)
ui-profile-resolution-auto-desc = Heimdall elige el mejor modo según el monitor del equipo y si la sesión está en pantalla completa o en ventana.
ui-profile-resolution-multi-monitor = Varios monitores
ui-resolution-mode-auto = Automático
ui-resolution-mode-multi-monitor = Varios monitores
ui-profile-aspect-ratio = Relación de aspecto
ui-profile-aspect-stretch = Ajustar (rellenar)
ui-profile-aspect-ratio-choice = { $wide }:{ $high }
ui-profile-rdp-session-mode = Modo de sesión
ui-profile-rdp-mode-embedded = Incrustado (integrado)
ui-profile-rdp-mode-external = Externo (mstsc.exe)
ui-profile-rdp-mode-external-desc = Iniciar RDP en una ventana separada de mstsc.exe. Conexión a Escritorio remoto pide la contraseña por sí misma.
ui-profile-rdp-extras = Conservado del perfil importado, todavía no usado por el cliente integrado: { $extras }
ui-rdp-extra-full-screen = abierto en pantalla completa
ui-dialog-import-dropped = Importado con ajustes que el cliente integrado todavía no usa:
ui-dialog-import-dropped-item = { $name }: { $settings }
ui-dialog-import-dropped-separator = {", "}
ui-import-dropped-external-client = abierto en un programa externo
ui-import-dropped-x11 = reenvío X11
ui-import-dropped-rdp-printers = impresoras
ui-import-dropped-rdp-com-ports = puertos serie
ui-import-dropped-rdp-smart-cards = tarjetas inteligentes
ui-import-dropped-rdp-webcam = cámara web
ui-import-dropped-rdp-usb = dispositivos USB
ui-import-dropped-rdp-microphone = micrófono
ui-import-dropped-rdp-multi-monitor = varios monitores
ui-import-dropped-citrix-cache-launch = inicio desde la caché de Citrix Workspace, no importado
ui-citrix-import-title = Aplicaciones de Citrix
ui-citrix-import-none = No se encontraron aplicaciones de Citrix en la caché local. Abre Citrix Workspace y conecta primero a una tienda.
ui-citrix-import-confirm = { $count ->
    [one] ¿Importar { $count } aplicación de Citrix desde la caché local de Workspace?
   *[other] ¿Importar { $count } aplicaciones de Citrix desde la caché local de Workspace?
}
ui-citrix-import-done = { $count ->
    [one] Se importó { $count } aplicación de Citrix correctamente.
   *[other] Se importaron { $count } aplicaciones de Citrix correctamente.
}
ui-citrix-import-refreshed = { $count ->
    [one] { $count } aplicación ya guardada: se actualizó su línea de inicio.
   *[other] { $count } aplicaciones ya guardadas: se actualizaron sus líneas de inicio.
}
ui-citrix-import-no-launch-lines = El almacén está bloqueado o no disponible: no se guardaron las líneas de inicio de la caché de Workspace. Estas aplicaciones se inician a través de su StoreFront.
ui-citrix-cache-folder-missing = No se encontró la carpeta de caché de Citrix SelfService.
ui-citrix-cache-no-files = No se encontraron archivos de caché de Citrix. Abre Citrix Workspace y conecta primero a una tienda.
ui-citrix-cache-unreadable = { $file }: { $detail }
ui-fullscreen-exit = Salir de pantalla completa
ui-fullscreen-exit-tooltip = F11, o Escape fuera de un terminal o de un escritorio remoto
ui-files-selected-with-size = { $selection } ({ $size })
ui-files-special-mark = { $name } ({ $kind })
ui-files-tooltip-type = Tipo: { $kind }
ui-files-tooltip-size = Tamaño: { $size }
ui-files-tooltip-modified = Modificado: { $at }
ui-files-tooltip-permissions = Permisos: { $permissions }
ui-files-tooltip-time = { $day } { $time } UTC
ui-tree-expand-all = Expandir todo
ui-tree-collapse-all = Contraer todo
ui-sidebar-hide-tooltip = Ocultar barra lateral (Ctrl+B)
ui-sidebar-show-tooltip = Mostrar barra lateral (Ctrl+B)
ui-nav-quick-connect = Conexión rápida
ui-nav-quick-connect-tooltip = Conexión rápida (Ctrl+K)
ui-hostkey-algorithm = Algoritmo: { $algorithm }
ui-tunnels-column-started = Iniciado
ui-tunnels-manage-gateways = Gestionar pasarelas en Ajustes...
ui-settings-powershell-policy = Política de ejecución de PowerShell
ui-settings-powershell-policy-hint = Se aplica al iniciar sesiones locales de PowerShell/pwsh
ui-settings-powershell-policy-default = Predeterminada
ui-settings-ctrl-v = Ctrl+V en un terminal
ui-settings-ctrl-v-always = Pega
ui-settings-ctrl-v-outside = Pega, salvo en programas a pantalla completa (vim, less...)
ui-settings-ctrl-v-never = Se envía a la sesión (Ctrl+Mayús+V pega)
ui-connect-via = vía { $route }

## Translations of text written in English first.
ui-desktop-anti-idle = Anti-inactividad
ui-desktop-anti-idle-tooltip = La anti-inactividad mantiene viva esta sesión. Haga clic para desactivarla en esta sesión.
ui-desktop-keys-f11 = F11
ui-dialog-close-transfers-body = Hay una transferencia de archivos en curso en "{ $name }". Cerrar ahora la cancela. ¿Cerrar de todos modos?
ui-dialog-close-transfers-title = Transferencia en curso
ui-dialog-import-host-keys = Servidores SSH de confianza transferidos: { $keys ->
    [one] { $keys } clave
   *[other] { $keys } claves
} y { $pins ->
    [one] { $pins } huella
   *[other] { $pins } huellas
}.
ui-dialog-import-host-keys-failed = No se pudieron transferir los servidores SSH de confianza: { $detail }
ui-dialog-paste-dangerous-body = El texto contiene { $command }, un comando que puede destruir datos o detener la máquina. Revíselo antes de que llegue al shell.
ui-dialog-paste-dangerous-confirm = Pegar de todos modos
ui-dialog-paste-dangerous-title = ¿Pegar un comando peligroso?
ui-dialog-session-logging-body = Cada sesión de terminal se escribirá en un archivo: lo que escribe y lo que se muestra, incluidas las contraseñas o tokens que el terminal repita. ¿Activarlo?
ui-dialog-session-logging-confirm = Activar
ui-dialog-session-logging-title = ¿Grabar las transcripciones de las sesiones?
ui-error-winrm-https-gateway = WinRM a través de una puerta de enlace SSH no admite HTTPS. Use HTTP, o conéctese directamente.
ui-error-winrm-tls-failed = La conexión TLS de WinRM a '{ $host }' en el puerto { $port } falló (certificado no confiable o error de negociación).
ui-error-winrm-unreachable = El host WinRM '{ $host }' no es accesible en el puerto { $port } (conexión rechazada o tiempo agotado).
ui-error-winrm-unresolved = No se puede resolver el host WinRM '{ $host }'.
ui-files-back-button = Atrás
ui-files-conflict-action = Acción
ui-files-conflict-apply = Aplicar
ui-files-conflict-apply-all = Aplicar a todos:
ui-files-conflict-destination = Destino
ui-files-conflict-folder-skip = Esta carpeta y todo su contenido previsto se omitirán.
ui-files-conflict-hint = Elija qué debe hacer Heimdall antes de que empiece la transferencia.
ui-files-conflict-rename = Renombrar automáticamente
ui-files-conflict-replace = Reemplazar
ui-files-conflict-skip = Omitir
ui-files-conflict-summary = { $count ->
    [one] { $count } destino en conflicto
   *[other] { $count } destinos en conflicto
}
ui-files-conflict-title = Conflictos de archivos
ui-files-error-destination-not-a-file = Envío rechazado: el destino ya existe y no es un archivo normal.
ui-files-error-is-link = No se pueden cambiar los permisos de un enlace simbólico: el servidor cambiaría los de su destino.
ui-files-error-replace-not-safe = Envío rechazado: el destino ya existe y el servidor no puede reemplazarlo de forma segura, así que se ha dejado como estaba.
ui-files-home-button = Inicio
ui-profile-experience = Experiencia visual
ui-profile-experience-composition = Activar la composición de escritorio
ui-profile-experience-font-smoothing = Activar el suavizado de fuentes (ClearType)
ui-profile-experience-no-animations = Desactivar las animaciones de menús
ui-profile-experience-no-cursor-shadow = Desactivar la sombra del cursor
ui-profile-experience-no-drag = Desactivar el arrastre de ventana completa
ui-profile-experience-no-themes = Desactivar los temas
ui-profile-experience-no-wallpaper = Desactivar el fondo de escritorio
ui-profile-field-passphrase = Frase de contraseña de la clave
ui-profile-legacy-algorithms-hint = Los intercambios de claves SHA-1, los cifrados CBC, HMAC-SHA1 y las claves de host RSA SHA-1 se ofrecen después de los actuales. Actívelo solo para un dispositivo que no conozca nada más reciente.
ui-profile-passphrase-clear-tooltip = Quitar la frase de contraseña guardada para esta sesión
ui-profile-passphrase-hint = Solo sirve para descifrar la clave SSH elegida. Déjelo vacío si la clave no tiene frase de contraseña o la desbloquea un agente SSH.
ui-profile-passphrase-saved = Frase de contraseña guardada
ui-profile-skip-cert-hint = Desactiva la verificación del certificado TLS. Úselo solo para hosts internos de confianza con certificados autofirmados.
ui-profile-toggle-anti-idle = Activar el mantenimiento anti-inactividad
ui-profile-toggle-auto-reconnect = Reconectar automáticamente
ui-profile-toggle-legacy-algorithms = Permitir algoritmos antiguos para dispositivos viejos
ui-profile-toggle-several-servers = Varios servidores responden en esta dirección: preguntar por cada certificado nuevo
ui-profile-use-ssl-hint = Usa WinRM sobre HTTPS, normalmente el puerto 5986. HTTP usa normalmente el puerto 5985.
ui-profile-winrm-gateway-http = El SSL de WinRM se desactiva cuando se elige una puerta de enlace SSH. WinRM a través de una puerta de enlace usa HTTP dentro del túnel SSH local.
ui-profile-winrm-tls-on-http-port = TLS está activado pero el puerto es el predeterminado sin cifrar, { $http }; WinRM sobre TLS escucha en { $https }.
ui-settings-anti-idle-interval = Intervalo anti-inactividad (0 = desactivado)
ui-settings-anti-idle-refused = El intervalo anti-inactividad debe ser 0, o estar entre { $min } y { $max } segundos.
ui-settings-anti-idle-unit = s
ui-settings-rdp-auto-reconnect = Reconexión automática
ui-settings-session-logging-record = Grabar las transcripciones de las sesiones (lo que muestra cada terminal, incluida la entrada)
ui-settings-session-logging-warning = Las transcripciones guardan lo que escribe y lo que se muestra, incluidas las contraseñas o tokens que el terminal repita. Mantenga privada la carpeta de registros.
ui-settings-ssh-keep-alive-hint = Con qué frecuencia Heimdall envía mantenimientos de conexión SSH en sesiones, SFTP, túneles y puertas de enlace, para que un cortafuegos o el servidor no corte una conexión inactiva. Se aplica a las conexiones abiertas después del cambio.
ui-settings-ssh-keep-alive-interval = Intervalo de mantenimiento SSH
ui-settings-ssh-keep-alive-refused = El intervalo de mantenimiento SSH debe estar entre { $min } y { $max } segundos.
ui-settings-ssh-session = Sesión
ui-settings-sftp = Navegador SFTP
ui-settings-sftp-browser-enabled = Activar el explorador SFTP integrado
ui-settings-sftp-auto-open = Abrir automáticamente el panel SFTP al conectar por SSH
ui-settings-sftp-follow = SFTP sigue el directorio de trabajo de SSH
ui-settings-dock-local-browser = Acoplar un explorador de archivos junto a los shells locales
ui-settings-ssh-tmout-reset-interval = Intervalo de reinicio de TMOUT (0 = desactivado)
ui-settings-ssh-tmout-reset-refused = El intervalo de reinicio de TMOUT de SSH debe estar entre { $min } y { $max } segundos.
ui-status-link-not-a-folder = { $name } no apunta a una carpeta.
ui-status-winrm-certificate-skipped = Se omitió la validación del certificado TLS de WinRM para esta sesión.
ui-status-citrix-launching = Iniciando sesión de Citrix...
ui-status-citrix-launched = Sesión de Citrix iniciada: { $name }
ui-status-citrix-workspace-not-found = No se encontró Citrix Workspace. Instala Citrix Workspace App.
ui-status-citrix-invalid-storefront = URL de StoreFront de Citrix no válida. Usa una URL absoluta HTTP o HTTPS.
ui-status-citrix-storefront-credentials = La URL de StoreFront de Citrix no puede contener credenciales.
ui-status-citrix-invalid-ica-file = El archivo ICA debe ser un archivo .ica de este equipo, no de un recurso compartido de red.
ui-status-citrix-not-configured = No hay ninguna URL de StoreFront ni archivo ICA de Citrix configurado.
ui-status-citrix-launch-failed = No se pudo iniciar la sesión de Citrix.
ui-status-citrix-not-started = No se pudo iniciar la sesión de Citrix: { $reason }
ui-status-citrix-command-rejected = El comando de inicio de Citrix contiene caracteres prohibidos (|, &, ;, `, $, saltos de línea).
ui-status-citrix-vault-locked = Desbloquea el almacén antes de iniciar esta sesión de Citrix.
ui-status-rdp-external-launched = Cliente externo iniciado: { $name } se abrió en Conexión a Escritorio remoto.
ui-status-rdp-external-launched-gateway = Cliente externo iniciado: { $name } pasa por la puerta de enlace de Escritorio remoto { $gateway }, que el cliente integrado todavía no sabe atravesar, así que se abrió en Conexión a Escritorio remoto.
ui-status-rdp-external-not-windows = Este perfil se abre en Conexión a Escritorio remoto (mstsc.exe), que solo tiene Windows: no se puede abrir en este sistema.
ui-status-rdp-external-ssh-gateway = Conexión a Escritorio remoto (mstsc.exe) no puede atravesar la puerta de enlace SSH de este perfil: no se abrió.
ui-status-rdp-external-not-found = No se encontró mstsc.exe en este equipo.
ui-status-rdp-external-not-written = No se pudo escribir el archivo de conexión de Escritorio remoto: { $reason }
ui-status-rdp-external-not-started = mstsc.exe no se inició: { $reason }
ui-status-winrm-gateway-ntlm = WinRM a través de una puerta de enlace: Kerberos no está disponible, la autenticación pasa a NTLM.
ui-winrm-diagnostic-ntlm-loopback = La autenticación WinRM falló para la identidad de Windows actual. Use una cuenta guardada para localhost o para hosts fuera del dominio.
ui-winrm-diagnostic-wsman-invalid = WinRM recibió una respuesta WSMan no válida. Si esta sesión pasa por una puerta de enlace, compruebe que WinRM usa HTTP dentro del túnel.
ui-tunnels-session-routes = Sesiones a través de una pasarela ({ $count })
ui-tunnels-session-route-local = -
ui-tunnels-session-route-local-tooltip = Transportado dentro de Heimdall: sin puerto local
ui-tunnels-close-all-tooltip = Cierra los túneles abiertos a mano; las sesiones a través de una pasarela siguen abiertas
ui-tab-route-badge = vía
ui-origin-rdp-file = Importado desde archivo RDP
ui-origin-openssh = Importado desde configuración de OpenSSH
ui-origin-putty = Importado desde el registro de PuTTY
ui-origin-mremoteng = Importado desde mRemoteNG
ui-origin-mobaxterm = Importado desde MobaXterm
ui-origin-rdcman = Importado desde RDCMan
ui-desktop-shortcuts = Atajos de teclado...
ui-desktop-shares-clipboard = Portapapeles
ui-desktop-shares-clipboard-tooltip = Redirección del portapapeles
ui-desktop-shares-drives = Unidades
ui-desktop-shares-drives-tooltip = Redirección de unidades
ui-desktop-shares-audio = Sonido
ui-desktop-shares-audio-tooltip = Redirección de audio
ui-settings-rdp-connect-timeout = Tiempo de espera del vigilante de conexión RDP (0 = desactivado)
ui-settings-rdp-connect-timeout-off = Desactivado
ui-settings-rdp-connect-timeout-seconds = { $seconds } s
ui-shortcuts-release-desktop = Devolver el teclado desde un escritorio remoto
ui-profile-toggle-strict-server-auth = Requerir validación de identidad del servidor
ui-error-rdp-server-not-authenticated = No se pudo validar la identidad del servidor: su certificado aún no es de confianza y las autoridades de certificación de este equipo no lo avalan. La autenticación estricta del servidor lo rechaza.
ui-certificate-subject = Sujeto: { $subject }
ui-session-copy-anonymous-button = Copiar informe anonimizado
ui-error-report-anonymous-header = Informe de diagnóstico { $protocol } (anonimizado)
ui-error-report-kind = Fallo:
ui-error-report-anonymous-hint = Incluye la hora, la versión de la aplicación y el tipo de fallo. Excluye direcciones de servidor, nombres de cuenta y texto de mensajes de error.
ui-tree-changed-move = Sesiones movidas.
ui-tree-changed-reorder = Sesiones reordenadas.
ui-tree-changed-rename = Sesión renombrada.
ui-tree-changed-folder-move = Carpeta movida.
ui-tree-changed-folder-rename = Carpeta renombrada.
ui-tree-undo = Deshacer
ui-status-reordered-one = Se movió { $name } dentro de { $folder }
ui-status-reordered = Se movieron { $count } sesiones dentro de { $folder }
ui-status-undo-conflict = No se puede deshacer: las sesiones o carpetas afectadas han cambiado desde esta acción.
ui-tree-no-folder-zone = Suelta aquí para sacarlo de su carpeta
ui-tree-no-folder-zone-tooltip = Suelta una sesión o una carpeta aquí para sacarla de su carpeta.
ui-tree-filter-chip-remove = ✕
ui-tree-filter-chip-tooltip = Quitar filtro: { $filter }
ui-tree-filter-result-count = { $shown } / { $total ->
    [one] { $total } sesión
   *[other] { $total } sesiones
}
ui-tree-selection-count = { $count } sesiones seleccionadas
ui-tree-selection-move = Mover
ui-tree-selection-more = Más acciones
