### The open core's words in Spanish, both variants: auth.

## Compartidos por las páginas de inicio de sesión

# accept: length: the label over the address field on sign-in forms; Spanish has no one-word "email", and "correo electrónico" is the standard field label (bare "correo" also means postal mail).
auth-email = Correo electrónico
auth-email-placeholder = tu@ejemplo.com

## La página de inicio de sesión

login-title-fold = Continuar - Flash
login-title = Acceder - Flash
login-tagline-fold = Tarjetas de repetición espaciada
login-tagline = Tarjetas que se programan solas.
login-other-ways = Otras formas de continuar
login-passkey = Acceder con llave de acceso
login-or-email-password = o con tu correo y contraseña
login-or-password = o con tu contraseña
login-password-placeholder = Contraseña
login-submit = Acceder
login-forgot = ¿Olvidaste tu contraseña?
login-create-email = Crear cuenta con correo electrónico
login-new-here = ¿Primera vez aquí?
login-create = Crear cuenta
login-wrong = Correo electrónico o contraseña incorrectos.
login-again = Por seguridad, vuelve a acceder para hacer esto.
login-deleted = Se eliminaron tu cuenta y todos sus datos.

## Crear una cuenta desde una invitación o un enlace de registro

enroll-title = Bienvenida - Flash
enroll-heading = Te damos la bienvenida, { $name }
enroll-intro-verified = Correo verificado. Ahora crea una llave de acceso para empezar a estudiar: Face ID, huella digital o el PIN de tu dispositivo. Sin contraseña.
enroll-intro = Crea una llave de acceso para empezar a estudiar: Face ID, huella digital o el PIN de tu dispositivo. Sin contraseña.
enroll-create-passkey = Crear mi llave de acceso
enroll-use-password = Usar una contraseña
enroll-password-placeholder = Contraseña (8 caracteres o más)
enroll-confirm-placeholder = Confirmar contraseña
enroll-create-password = Crear cuenta con contraseña

## Restablecer la contraseña

reset-title = Restablecer contraseña - Flash
reset-sent-heading = Revisa tu correo
reset-sent-body =
    Si esa dirección tiene cuenta, te llegará un vínculo para restablecer la contraseña.
    Funciona una vez y expira en 1 hora.
reset-heading = Restablece tu contraseña
reset-intro = Te enviaremos un vínculo por correo para crear una nueva.
reset-send = Enviar vínculo
reset-back = Volver a la página de acceso
reset-new-title = Nueva contraseña - Flash
reset-new-heading = Crea una contraseña nueva
reset-new-intro = Se cerrará tu sesión en todos los demás dispositivos.
reset-new-password = Contraseña nueva
reset-new-confirm = Confirmar
reset-new-save = Guardar contraseña

## Problemas al iniciar sesión o crear una cuenta (una línea de texto en
## la página, o la página entera para un enlace que ya no sirve)

auth-email-required = Falta el correo electrónico
auth-email-invalid = Eso no parece un correo electrónico
auth-email-taken = Ese correo electrónico ya está en uso
auth-password-short = La contraseña debe tener al menos 8 caracteres
auth-password-long = La contraseña es demasiado larga
auth-passwords-differ = Las contraseñas no coinciden
auth-captcha = Completa la verificación y vuelve a intentarlo.
auth-reset-spent = Este vínculo para restablecer la contraseña no es válido, expiró o ya se usó.
auth-reset-spent-line = Este vínculo para restablecer la contraseña no es válido, expiró o ya se usó
auth-invite-malformed = Invitación no válida
auth-invite-spent = Este vínculo de invitación no es válido, expiró o ya se usó.
auth-invite-spent-line = Este vínculo de invitación no es válido, expiró o ya se usó
auth-invite-expired = La invitación no es válida o expiró
auth-no-passkeys = No hay llaves de acceso registradas
auth-passkey-failed = No se pudo verificar la llave de acceso
auth-passkey-unknown = Llave de acceso desconocida
auth-no-ceremony = No hay ningún inicio de sesión en curso

## Aprobar la conexión de un asistente de IA (OAuth)

consent-title = Autorizar - Flash
consent-heading = Autorizar acceso
consent-wants-html =
    <strong>{ $client }</strong> quiere acceder a tus tarjetas:
    sesiones de estudio, contenido de las tarjetas y la posibilidad de crear y editar tarjetas.
consent-where-html =
    Si lo apruebas, el inicio de sesión se enviará a <strong>{ $host }</strong>.
    El nombre de arriba lo eligió quien registró la app; la dirección es adonde
    va realmente tu acceso.
consent-only-if =
    Aprueba solo si iniciaste esto tú desde una app de confianza (por ejemplo, al
    agregar un conector en Claude, ChatGPT, Grok, Mistral o Gemini) y la
    dirección es la de esa app.
consent-approve = Aprobar
consent-deny = Rechazar
consent-done-title = Conexión lista - Flash
consent-done-heading = La conexión está lista
consent-done-body-html =
    <strong>{ $client }</strong> ya puede acceder a tus tarjetas. Vuelve
    a esa app para continuar.
consent-done-open = Abrir Flash
oauth-error-title = Solicitud de autorización rechazada - Flash
oauth-error-heading = No se puede completar esa solicitud
oauth-error-body-html =
    La app que te trajo aquí hizo una solicitud que Flash no puede aceptar
    (<code>{ $code }</code>). No se autorizó nada y no se te envió a ninguna
    parte.
oauth-error-hint =
    Si estabas agregando un conector, regresa a la app y
    vuelve a intentarlo. Si el problema se repite, el conector de esa app está mal
    configurado y a quien lo creó le servirá conocer el código de arriba.
oauth-error-back = Volver a Flash
