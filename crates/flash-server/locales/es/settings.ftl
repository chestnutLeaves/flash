### The open core's words in Spanish, both variants: settings.

settings-title = Configuración - Flash
settings-heading = Configuración
settings-save = Guardar
settings-saved = Guardado

## Nombre

settings-name = Nombre
settings-name-error = el nombre es obligatorio (hasta 80 caracteres)
settings-name-hint = Cómo te llaman Flash y tu IA.

## Las tarjetas de la persona

settings-cards = Tus tarjetas
settings-cards-count =
    { $count ->
        [one] { $count } tarjeta.
        [many] { $count } de tarjetas.
       *[other] { $count } tarjetas.
    }
settings-cards-export = Tus tarjetas siempre son tuyas y puedes exportarlas cuando quieras:
settings-export-apkg = Anki .apkg
settings-export-csv = CSV

## Límites diarios

settings-limits = Límites diarios
settings-limits-hint = Valores predeterminados de la cuenta. Cada mazo puede cambiarlos en su propia página.
settings-limit-new = Tarjetas nuevas / día
settings-limit-reviews = Máx. de repasos / día
# accept: length: the label starts a sentence its choices finish ("Las tarjetas quedan pendientes / durante todo el día"); Spanish needs the article and a verb phrase for "come due", and a bare "Tarjetas pendientes" would read as a count.
settings-due = Las tarjetas quedan pendientes
settings-due-day = Durante todo el día
settings-due-exact = A su hora exacta
settings-due-hint = Durante todo el día: todo lo pendiente de hoy está listo cuando te sientas a estudiar, así que una sola sesión completa el día. A su hora exacta: cada tarjeta espera hasta la hora para la que se programó.

## Calificación

settings-grading = Cómo califica tu asistente
settings-grading-hint = Cómo informa tu asistente de IA las calificaciones cuando estudias con él.
settings-grading-silent = Silenciosa: califica sin decirlo y la conversación sigue su curso
settings-grading-announce = Anunciada: dice la calificación y puedes cambiarla
settings-grading-self = Autocalificación: te pide calificar cada tarjeta

## Contraseña

settings-password = Contraseña
settings-password-error = no funcionó. Verifica la contraseña actual y que las nuevas coincidan (8 caracteres o más)
settings-password-add = Agrega una contraseña para acceder sin llave de acceso.
settings-password-add-external = Agrega una contraseña para acceder sin llave de acceso ni cuenta conectada.
settings-password-new = Contraseña (8 caracteres o más)
settings-password-confirm = Confirmar contraseña
settings-password-set = Crear contraseña
settings-password-current = Contraseña actual
settings-password-new-change = Contraseña nueva (8 caracteres o más)
settings-password-confirm-change = Confirmar contraseña nueva
settings-password-change = Cambiar contraseña
settings-password-change-hint = Al cambiar tu contraseña, se cierra la sesión en tus otros dispositivos.
settings-password-remove = Eliminar contraseña
settings-password-tip =
    Consejo: las llaves de acceso son más rápidas y protegen contra el phishing. Puedes
    agregar una cuando te inviten a registrar un dispositivo.

## Llaves de acceso

settings-passkeys = Llaves de acceso
settings-passkeys-none = Ninguna registrada.
settings-passkey-added = agregada el { $date }

## Dispositivos

settings-devices = Dispositivos
settings-devices-signed-out = Sesión cerrada
settings-devices-hint = Cada navegador y teléfono con la sesión iniciada en esta cuenta. Si cierras la sesión en uno, termina ahí.
settings-device-sign-out = Cerrar sesión
settings-devices-sign-out-others = Cerrar sesión en los demás
settings-device-browser = { $browser } en { $os }
settings-device-browser-unknown = Navegador
settings-device-phone-unknown = Teléfono
settings-device-this = Este navegador
settings-device-used = Último uso: { $ago }
settings-device-signed-in = Accedió { $ago }
settings-device-app-used = App de Flash · último uso: { $ago }
settings-device-app-signed-in = App de Flash · accedió { $ago }
settings-device-via-link = { $detail } · accedió con un vínculo de correo

## Apps conectadas (asistentes de IA y la app del teléfono)

settings-apps = Apps conectadas
settings-apps-disconnected = Desconectada
settings-apps-hint = Todo lo que puede ver y cambiar tus tarjetas en tu nombre. Al desconectarlo, se cierra su sesión en todas partes; puede volver a pedir conectarse.
settings-apps-none = Nada conectado.
settings-app-devices =
    { $count ->
        [one] { $count } dispositivo
        [many] { $count } de dispositivos
       *[other] { $count } dispositivos
    }
settings-app-since = desde el { $date }
settings-app-last-used = último uso: { $ago }
settings-app-disconnect = Desconectar
settings-app-flash = App de Flash
settings-app-unnamed = Conector

## Invitar a alguien (solo admins)

settings-invite = Invitar a alguien (admin)
settings-invite-hint =
    Crea un vínculo de registro de un solo uso, válido por 7 días.
    Quien lo abra elige una llave de acceso o una contraseña, y ya puede entrar.
settings-invite-name = Nombre de la persona
settings-invite-create = Crear invitación
settings-invite-send = Envíale este vínculo (se muestra una sola vez):
settings-invite-copy = Copiar vínculo

## Aspecto

settings-appearance = Aspecto
settings-appearance-hint = El tema oscuro es el predeterminado.
theme-dark = Oscuro
theme-light = Claro
theme-clay = Arcilla
theme-mocha = Moca
theme-taupe = Topo

## Eliminar la cuenta

settings-delete = Eliminar cuenta
settings-delete-word = ELIMINAR
settings-delete-error-confirm = escribe { $word } exactamente para confirmar
settings-delete-error-password = esa contraseña es incorrecta
settings-delete-error-admin = no hay otra cuenta de admin; primero haz admin a otra persona
settings-delete-hint =
    Elimina para siempre tu cuenta y todo lo que contiene: mazos, tarjetas, historial
    de repasos, archivos multimedia, llaves de acceso, sesiones y cualquier IA conectada. No se puede deshacer.
settings-delete-copy = ¿Quieres una copia antes?
settings-delete-type = Escribe { $word } para confirmar
settings-delete-type-tip = Escribe { $word } en mayúsculas
settings-delete-submit = Eliminar mi cuenta

## Errores

invite-name-required = el nombre es obligatorio
