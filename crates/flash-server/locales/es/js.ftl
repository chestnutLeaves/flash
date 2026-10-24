### The open core's words in Spanish, both variants: js.

## Comportamientos compartidos (ui.js)

js-ui-play-audio = Reproducir audio
js-ui-pause-audio = Pausar audio
js-ui-copied = Se copió
js-ui-copy-failed = No se pudo copiar
js-ui-show-subdecks = Mostrar submazos
js-ui-hide-subdecks = Ocultar submazos
js-ui-show-archived = Mostrar archivados
js-ui-hide-archived = Ocultar archivados

## Iniciar sesión con una llave de acceso (webauthn.js)

js-passkey-cancelled = Se canceló o se agotó el tiempo.
js-passkey-failed = No se pudo completar. Vuelve a intentarlo.
js-passkey-bad-list = La lista de credenciales del servidor no es una lista.
js-passkey-bad-entry = La lista de credenciales del servidor contiene algo inesperado.
js-passkey-not-passkey = El servidor indicó una credencial que no es una llave de acceso.
js-passkey-none-made = El navegador no creó una llave de acceso.
js-passkey-none-used = El navegador no usó una llave de acceso.
js-passkey-not-used = No se usó ninguna llave de acceso. Accede de otra forma y vuelve a agregarla en Configuración.
js-passkey-link-incomplete = Este vínculo de registro está incompleto.

## La página de un mazo (deck.js)

js-deck-selected =
    { $count ->
        [one] { $count } seleccionada
        [many] { $count } seleccionadas
       *[other] { $count } seleccionadas
    }
js-deck-select-all =
    { $count ->
        [one] Seleccionar todas ({ $count })
        [many] Seleccionar todas ({ $count })
       *[other] Seleccionar todas ({ $count })
    }
js-deck-delete-n =
    { $count ->
        [one] Eliminar { $count } tarjeta
        [many] Eliminar { $count } de tarjetas
       *[other] Eliminar { $count } tarjetas
    }
js-deck-row-confirm = Haz clic otra vez para eliminar
js-deck-add = Agregar
js-deck-close = Cerrar
js-deck-failed = No se pudo completar. Vuelve a intentarlo.

## El editor de tarjetas (editor.js)

js-editor-front = Anverso
js-editor-back = Reverso
js-editor-question = Pregunta
js-editor-answer = Respuesta
js-editor-answer-to-type = Respuesta para escribir
js-editor-text = Texto
# accept: length: the label of a cloze note's Extra field; Spanish "extra" is an adjective that needs a noun as a label (alone it reads as a bonus), and "Más información" would read as a "learn more" link.
js-editor-extra = Información extra
js-editor-cloze-hint = Frase con {"{{"}c1::palabras ocultas{"}}"}
js-editor-extra-hint = Se muestra con la respuesta (opcional)
js-editor-uploading = Subiendo { $name }…
js-editor-upload-failed = No se pudo subir el archivo.
js-editor-select-first = Primero selecciona un texto.
js-editor-wait-upload = Espera a que termine la subida.
js-editor-simple = Sencillo
js-editor-advanced = Avanzado
