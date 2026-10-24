### The open core's words in Spanish: refusals the phone app shows.

## Cualquier solicitud

api-unauthorized = Accede para continuar
api-invalid-credentials = Esos datos no coinciden
api-forbidden = No tienes permiso
# accept: length: English names the missing thing through $what, which counts as nothing; Spanish writes the noun in each variant ("Sesión de estudio no encontrada", "Llave de acceso no encontrada").
api-not-found =
    { $what ->
        [deck] Mazo no encontrado
        [card] Tarjeta no encontrada
        [note] Nota no encontrada
        [media] Archivo no encontrado
        [session] Sesión no encontrada
        [study_session] Sesión de estudio no encontrada
        [passkey] Llave de acceso no encontrada
        [device] Dispositivo no encontrado
       *[other] No encontrado
    }
# accept: length: English names the feature through $what, which counts as nothing; Spanish writes it in each variant ("las llaves de acceso", "restablecer la contraseña").
api-not-available =
    { $what ->
        [passkeys] Las llaves de acceso no están disponibles en este servidor
        [reset] En este servidor no se puede restablecer la contraseña
       *[other] Esto no está disponible en este servidor
    }
api-too-large = Ese archivo es demasiado grande
# accept: length: "slow down" has no equally short calm Spanish ("más despacio" reads as a scolding), so the refusal says why (too many requests) and what to do (wait).
api-rate-limited = Demasiadas solicitudes seguidas; espera un momento

## Inicio de sesión y configuración de la cuenta

api-passkey-expired = Se agotó el tiempo para usar la llave de acceso; vuelve a empezar
api-last-login-method = Es tu única forma de acceder; primero agrega otra
api-current-password = Escribe tu contraseña actual

## Mazos y archivos

api-deck-name-long = El nombre del mazo es demasiado largo
api-file-name-long = El nombre del archivo es demasiado largo
store-error-tags-many = Una tarjeta puede tener como máximo { $max } etiquetas
store-error-tag-long = Una etiqueta puede tener como máximo { $max } bytes
card-limit-reached = Límite de tarjetas alcanzado: { $current } de { $cap }
# accept: length: "importación" and "exportación" run 11 letters against English's 6, and Spanish needs the article on each item of the list.
heavy-busy-yours = Todavía tienes una importación, una exportación o la página de estadísticas en curso; vuelve a intentarlo en un momento
# accept: length: "importaciones y exportaciones" is 29 letters against English's "imports and exports" (19), and no shorter Spanish names both.
heavy-busy-everyone = El servidor está ocupado con otras importaciones y exportaciones; vuelve a intentarlo en un momento
