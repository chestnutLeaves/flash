### Spanish as Spain words it, where it differs from the shared base (es),
### which holds the Latin American wording: añadir, Ajustes, ordenador,
### vídeo, comprobar, pulsar, and the keys as Spain's keyboards label
### them (Intro, Mayús).

## Inicio de sesión, editor y estadísticas

login-title = Iniciar sesión - Flash
login-passkey = Iniciar sesión con llave de acceso
login-submit = Iniciar sesión
login-again = Por seguridad, vuelve a iniciar sesión para hacer esto.
reset-sent-body =
    Si esa dirección tiene una cuenta, te llegará un enlace para restablecer la contraseña.
    Funciona una vez y caduca en 1 hora.
reset-intro = Te enviaremos un enlace por correo para crear una nueva.
reset-send = Enviar enlace
reset-back = Volver al inicio de sesión
auth-captcha = Completa la verificación e inténtalo de nuevo.
auth-reset-spent = Este enlace para restablecer la contraseña no es válido, ha caducado o ya se ha usado.
auth-reset-spent-line = Este enlace para restablecer la contraseña no es válido, ha caducado o ya se ha usado
auth-invite-spent = Este enlace de invitación no es válido, ha caducado o ya se ha usado.
auth-invite-spent-line = Este enlace de invitación no es válido, ha caducado o ya se ha usado
auth-passkey-failed = No se ha podido verificar la llave de acceso
consent-where-html =
    Si lo apruebas, el inicio de sesión se enviará a <strong>{ $host }</strong>.
    El nombre de arriba lo eligió quien registró la aplicación; la dirección es adonde
    va realmente tu acceso.
consent-only-if =
    Aprueba solo si iniciaste esto tú desde una aplicación de confianza (por ejemplo, al
    añadir un conector en Claude, ChatGPT, Grok, Mistral o Gemini) y la
    dirección es la de esa aplicación.
consent-deny = Denegar
consent-done-body-html =
    <strong>{ $client }</strong> ya puede acceder a tus tarjetas. Vuelve
    a esa aplicación para continuar.
oauth-error-body-html =
    La aplicación que te ha traído aquí ha hecho una solicitud que Flash no puede aceptar
    (<code>{ $code }</code>). No se ha autorizado nada y no se te ha enviado a ninguna
    parte.
oauth-error-hint =
    Si estabas añadiendo un conector, vuelve a la aplicación e
    inténtalo de nuevo. Si el problema se repite, el conector de esa aplicación está mal
    configurado y a quien lo creó le servirá conocer el código de arriba.
email-paste-link = O pega este enlace en tu navegador:
email-reset-preheader = Crea una contraseña nueva. El enlace funciona una sola vez y expira en 1 hora.
email-reset-expiry = El enlace funciona una sola vez y expira en 1 hora.
email-reset-text-end = El enlace funciona una sola vez y expira en 1 hora. Si no fuiste tú, ignora este correo. No cambiará nada.
editor-by-app = una edición en la aplicación
editor-attach-tip = Adjuntar imagen, audio o vídeo
editor-cloze-tip = Respuesta anidada (Ctrl+Mayús+C; Ctrl+Mayús+Alt+C repite el último número)
editor-cloze-hint-html = Selecciona una respuesta y pulsa <kbd>Ctrl</kbd>+<kbd>Mayús</kbd>+<kbd>C</kbd> para ocultarla.
editor-save-hint-html = <kbd>Ctrl</kbd>+<kbd>Intro</kbd> guarda.
card-error-no-cloze = añade al menos una respuesta anidada {"{{"}c1::…{"}}"}
shell-settings = Ajustes
stats-legend-avg-html =
    { $count ->
        [one] Media diaria: <span>{ $count } repaso</span>
        [many] Media diaria: <span>{ $count } de repasos</span>
       *[other] Media diaria: <span>{ $count } repasos</span>
    }

## Mazos, estudio y ajustes

today-connect-dismiss = Cerrar
deck-add = Añadir tarjeta
deck-add-subdeck = Añadir submazo
deck-add-open = Añadir
deck-add-more = Más formas de añadir tarjetas
deck-bulk-tags-label = Añadir o quitar etiquetas
deck-bulk-tag-add = Añadir
cards-bulk-tagged =
    { $count ->
        [one] Etiquetas añadidas a { $count } tarjeta
        [many] Etiquetas añadidas a { $count } de tarjetas
       *[other] Etiquetas añadidas a { $count } tarjetas
    }
cards-error-bulk-changed = La lista ha cambiado mientras elegías. Vuelve a seleccionar las tarjetas.
deck-settings-title = Ajustes de { $name } - Flash
deck-settings-label = Ajustes del mazo
decks-more-settings = Ajustes del mazo
goal-passed = Tu fecha objetivo ya ha pasado.
deck-error-invalid = no ha funcionado
card-not-added-cloze = No se ha añadido la tarjeta: una nota de respuesta anidada necesita al menos una respuesta {"{{"}c1::…{"}}"}. Selecciona la respuesta y pulsa Ctrl+Mayús+C.
card-not-added-empty = No se ha añadido la tarjeta: los dos lados necesitan contenido.
card-not-added-long = No se ha añadido la tarjeta: uno de los lados es demasiado largo.
card-not-added-media = No se ha añadido la tarjeta: usa archivos multimedia que no son tuyos.
card-not-added-invalid = No se ha añadido la tarjeta: no se ha podido guardar el contenido.
card-not-added-limit = No se ha añadido la tarjeta: esta cuenta ha alcanzado su límite de tarjetas.
study-key-enter = intro
settings-title = Ajustes - Flash
settings-heading = Ajustes
settings-password-error = no ha funcionado. Comprueba la contraseña actual y que las nuevas coincidan (8 caracteres o más)
settings-password-add = Añade una contraseña para iniciar sesión sin llave de acceso.
settings-password-add-external = Añade una contraseña para iniciar sesión sin llave de acceso ni cuenta conectada.
settings-password-tip =
    Consejo: las llaves de acceso son más rápidas y protegen contra el phishing. Puedes
    añadir una cuando te inviten a registrar un dispositivo.
settings-passkey-added = añadida el { $date }
settings-device-signed-in = Inició sesión { $ago }
settings-device-app-used = Aplicación de Flash · último uso: { $ago }
settings-device-app-signed-in = Aplicación de Flash · inició sesión { $ago }
settings-device-via-link = { $detail } · inició sesión con un enlace de correo
settings-apps = Aplicaciones conectadas
settings-app-flash = Aplicación de Flash
settings-invite-hint =
    Crea un enlace de registro de un solo uso, válido por 7 días.
    Quien lo abra elige una llave de acceso o una contraseña, y ya puede entrar.
settings-invite-send = Envíale este enlace (se muestra una sola vez):
settings-invite-copy = Copiar enlace

## Importar, conectar y los scripts de las páginas

import-media-video =
    { $count ->
        [one] { $count } vídeo
        [many] { $count } de vídeos
       *[other] { $count } vídeos
    }
import-csv-header-unreadable = no se ha podido leer la fila de encabezado del CSV
import-media-list-damaged = la lista de multimedia del paquete está dañada y no se ha podido leer
import-compressed-damaged = el contenido comprimido está dañado y no se ha podido descomprimir
import-media-unreadable = no se ha podido leer { $file } del paquete
import-collection-damaged = la colección dentro del paquete está dañada y no se ha podido leer
import-staging-failed = No se ha podido preparar el paquete para leerlo; inténtalo de nuevo
import-busy-uploads = demasiadas subidas a la vez; inténtalo de nuevo en un momento
import-busy = hay otra importación en curso; inténtalo de nuevo en un momento
connect-log-in = Iniciar sesión
connect-sign-in = Inicia sesión en Flash cuando te lo pida.
connect-grok-3 = Pega la dirección e inicia sesión en Flash cuando te lo pida.
# accept: length: Spain's web says "aplicación" (Android's word in Spain) where English says "app", which puts this full step a character over.
connect-gemini-3-html = Inicia sesión en Flash cuando te lo pida y luego selecciona <strong>{ ui-gemini-connect }</strong> para guardar la aplicación.
connect-claude-code-body-html = Pega esto en tu terminal y luego ejecuta <span class="cn-mono">/mcp</span> para iniciar sesión.
connect-other-body = Flash es un servidor MCP estándar, así que la misma dirección funciona en cualquier lugar compatible con MCP: otros asistentes, editores, frameworks de agentes o lo que construyas tú. Pégala donde esa aplicación pida un servidor MCP personalizado e inicia sesión en Flash cuando te mande aquí.
connect-sub = Copia tu dirección de Flash, añádela en el asistente que usas e inicia sesión en Flash una vez. Es cuestión de un minuto.
connect-step-add = 2. Añade Flash en los ajustes de tu IA
ui-claude-add-custom = Añadir conector personalizado
ui-claude-add = Añadir
ui-chatgpt-settings = Ajustes
connect-gemini-1-html = En un ordenador, ve a <strong>gemini.google.com/apps</strong>.
connect-claude-code = Añade Flash a Claude Code
connect-codex = Añade Flash a Codex
js-ui-copied = Copiado
js-ui-copy-failed = No se ha podido copiar
js-passkey-failed = No se ha podido completar. Inténtalo de nuevo.
js-passkey-link-incomplete = Este enlace de registro está incompleto.
js-editor-upload-failed = No se ha podido subir.
js-passkey-cancelled = Cancelado o tiempo agotado.
js-passkey-not-used = No se ha usado ninguna llave de acceso. Inicia sesión de otra forma y vuelve a añadirla en Ajustes.
js-deck-add = Añadir
js-deck-failed = No se ha podido completar. Inténtalo de nuevo.

## El panel de estudio

# accept: length: Spain's errors take the perfect tense ("No se ha podido"), as Apple and Android write them in Spain, which puts this a character over.
panel-save-failed = No se ha podido guardar la tarjeta. Inténtalo de nuevo.
panel-no-answer = Flash no ha respondido.
panel-no-answer-late = Flash no ha respondido a tiempo.
panel-remind-fallback = Puedes activar los recordatorios en los ajustes de Flash.
study-remind-elsewhere = Puedes activar los recordatorios en Ajustes.
panel-needs-host = Este panel necesita una aplicación compatible con MCP Apps.

## Lo que la app rechaza

api-unauthorized = Inicia sesión para continuar
api-last-login-method = Es tu única forma de iniciar sesión; primero añade otra
# accept: length: "importación" and "exportación" run 11 letters against English's 6, and Spanish needs the article on each item of the list.
heavy-busy-yours = Todavía tienes una importación, una exportación o la página de estadísticas en curso; inténtalo de nuevo en un momento
# accept: length: "importaciones y exportaciones" is 29 letters against English's "imports and exports" (19), and no shorter Spanish names both.
heavy-busy-everyone = El servidor está ocupado con otras importaciones y exportaciones; inténtalo de nuevo en un momento

## Un archivo multimedia rechazado al subirlo

media-error-count = Has alcanzado el límite de archivos multimedia. Elimina algunos primero.
media-error-storage = Has alcanzado el límite de espacio multimedia. Contacta con soporte.
gone-title = Este enlace ya no funciona - Flash
gone-heading = Este enlace ya no funciona
gone-link-body = Ha caducado, ya se ha usado o se ha desactivado.
