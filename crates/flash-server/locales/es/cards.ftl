### The open core's words in Spanish, both variants: cards.

## La lista de tarjetas de un mazo

cards-none = No hay tarjetas.
cards-none-filtered = Ninguna tarjeta coincide.
cards-chip-archived = Archivadas
cards-own = Solo este mazo
cards-chip-remove = Quitar { $name } del filtro
cards-match-label = Coincidencia de etiquetas
cards-match-any = Alguna
cards-match-all = Todas
cards-chip-clear = Borrar
cards-select-page = Seleccionar todas las tarjetas de esta página
cards-select-card = Seleccionar esta tarjeta
cards-from-title = En { $name }. Mostrar solo sus tarjetas.
cards-col-front = Anverso
cards-col-back = Reverso
cards-col-tags = Etiquetas
cards-pages = Páginas de tarjetas
cards-previous = Anterior
cards-next = Siguiente
cards-page-of =
    { $count ->
        [one] Página { $page } de { $pages } · { $count } tarjeta
        [many] Página { $page } de { $pages } · { $count } de tarjetas
       *[other] Página { $page } de { $pages } · { $count } tarjetas
    }
cards-edit-label = Editar tarjeta
cards-edit = Editar
cards-unarchive-label = Desarchivar tarjeta
cards-unarchive = Desarchivar
cards-archive-label = Archivar tarjeta
cards-archive = Archivar
cards-delete-label = Eliminar tarjeta
cards-delete = Eliminar
cards-bulk-archived =
    { $count ->
        [one] { $count } tarjeta archivada
        [many] { $count } de tarjetas archivadas
       *[other] { $count } tarjetas archivadas
    }
cards-bulk-unarchived =
    { $count ->
        [one] { $count } tarjeta vuelve al estudio
        [many] { $count } de tarjetas vuelven al estudio
       *[other] { $count } tarjetas vuelven al estudio
    }
cards-bulk-deleted =
    { $count ->
        [one] { $count } tarjeta eliminada
        [many] { $count } de tarjetas eliminadas
       *[other] { $count } tarjetas eliminadas
    }
cards-bulk-moved =
    { $count ->
        [one] { $count } tarjeta movida a { $deck }
        [many] { $count } de tarjetas movidas a { $deck }
       *[other] { $count } tarjetas movidas a { $deck }
    }
cards-bulk-tagged =
    { $count ->
        [one] Etiquetas agregadas a { $count } tarjeta
        [many] Etiquetas agregadas a { $count } de tarjetas
       *[other] Etiquetas agregadas a { $count } tarjetas
    }
cards-bulk-untagged =
    { $count ->
        [one] Etiquetas quitadas de { $count } tarjeta
        [many] Etiquetas quitadas de { $count } de tarjetas
       *[other] Etiquetas quitadas de { $count } tarjetas
    }
cards-bulk-undone = Cambio deshecho
cards-bulk-undo = Deshacer
cards-error-bulk-changed = La lista cambió mientras elegías. Vuelve a seleccionar las tarjetas.
cards-error-bulk-many = Se pueden cambiar como máximo { $max } tarjetas a la vez.
cards-error-move-archived = Ese mazo está archivado. Desarchívalo primero.
cards-error-tags-none = Escribe al menos una etiqueta.
cards-kind-cloze = respuesta anidada { $n }
cards-kind-reversed = invertida
cards-kind-typed = escrita

## La página de una tarjeta y el diálogo de edición

editor-page-title = Editar tarjeta - Flash
editor-crumbs = Mazo
editor-heading = Editar tarjeta
editor-tags = Etiquetas
editor-tags-hint = Separadas por comas; se aplican a todas las tarjetas de esta nota.
editor-history =
    { $count ->
        [one] Historial ({ $count } versión anterior)
        [many] Historial ({ $count } de versiones anteriores)
       *[other] Historial ({ $count } versiones anteriores)
    }
editor-replaced = { $ago }, reemplazada por { $by }
editor-by-web = una edición aquí
editor-by-mcp = una edición de un asistente
editor-by-app = una edición en la app
editor-by-sync = una edición desde otro dispositivo
editor-by-restore = una restauración
editor-restore = Restaurar
editor-restored = Versión anterior restaurada.
editor-saved = Guardado.
editor-cancel = Cancelar
editor-save = Guardar

## El editor

editor-type = Tipo
note-type-basic = Básico
note-type-reversed = Básico (y tarjeta invertida)
note-type-typed = Básico (escribir la respuesta)
# accept: length: the note type's name; "respuesta anidada" is Anki's Spanish name for Cloze, and Spanish has no one-word term for it.
note-type-cloze = Respuesta anidada
editor-siblings = { $count } tarjetas comparten esta nota
editor-siblings-hint = Al guardar, se regeneran todas las tarjetas de esta nota
editor-toolbar = Formato
editor-bold-tip = Negrita (Ctrl+B)
editor-bold = Negrita
editor-italic-tip = Cursiva (Ctrl+I)
editor-italic = Cursiva
editor-underline-tip = Subrayado (Ctrl+U)
editor-underline = Subrayado
editor-strike = Tachado
editor-superscript = Superíndice
editor-subscript = Subíndice
editor-text-color = Color del texto
editor-text-hue = Texto { $hue }
editor-no-color = Sin color
editor-highlight = Resaltado
editor-highlight-hue = Resaltado { $hue }
editor-no-highlight = Sin resaltado
editor-bulleted = Lista con viñetas
editor-numbered = Lista numerada
editor-clear = Quitar formato
editor-attach-tip = Adjuntar imagen, audio o video
editor-attach = Adjuntar multimedia
editor-cloze-tip = Respuesta anidada (Ctrl+Shift+C; Ctrl+Shift+Alt+C repite el último número)
editor-cloze = Respuesta anidada
editor-source-tip = Código HTML
editor-source = Mostrar u ocultar el código HTML
editor-front = Anverso
editor-back = Reverso
editor-front-placeholder = Pregunta
editor-back-placeholder = Respuesta
editor-front-source = Código HTML del anverso
editor-back-source = Código HTML del reverso
editor-typed-hint = Al estudiar, escribirás esta respuesta y verás las diferencias.
editor-drop-hint = Arrastra o pega imágenes y audio directamente en un campo.
editor-cloze-hint-html = Selecciona una respuesta y presiona <kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>C</kbd> para ocultarla.
editor-reversed-hint = Dos tarjetas: del anverso al reverso, y del reverso al anverso.
editor-save-hint-html = <kbd>Ctrl</kbd>+<kbd>Enter</kbd> guarda.

## Colores, para los nombres de las muestras (en minúscula, como dentro de una frase)

hue-red = rojo
hue-amber = ámbar
hue-green = verde
# accept: length: Spanish has no one-word name for teal; "verde azulado" is the usual one, and "turquesa" is a lighter, different colour.
hue-teal = verde azulado
hue-blue = azul
hue-purple = morado
hue-pink = rosa

## Por qué se rechazó una tarjeta o una nota (el editor, la app, un asistente)

card-error-front-empty = el anverso de la tarjeta está vacío
card-error-back-empty = el reverso de la tarjeta está vacío
card-error-side-long = un lado de la tarjeta supera los { $max } bytes
card-error-field-long = un campo supera los { $max } bytes de HTML
card-error-no-cloze = agrega al menos una respuesta anidada {"{{"}c1::…{"}}"}
card-error-cloze-empty = la respuesta anidada c{ $n } está vacía
card-error-cloze-card = respuesta anidada c{ $n }: { $error }
card-error-media = el archivo multimedia { $id } no está entre los que subiste

## Un archivo multimedia rechazado al subirlo

media-error-format = { $filename }: formato no compatible
media-error-empty = { $filename }: archivo vacío
media-error-too-large = { $filename }: supera el límite de { $mb } MB por archivo
media-error-mismatch = { $filename }: el contenido no coincide con su extensión
media-error-count = Alcanzaste el límite de archivos multimedia. Elimina algunos primero.
media-error-storage = Alcanzaste el límite de espacio multimedia. Comunícate con soporte.
