### The open core's words in Spanish, both variants: decks.

## Tablas de mazos (Hoy, la página de mazos, los submazos de un mazo)

decks-col-deck = Mazo
# accept: length: a one-word column heading; "pendientes" is the word for due, taken from Anki's Spanish, and Spanish has nothing as short as "due".
decks-col-due = Pendientes
decks-col-new = Nuevas
decks-fold-label = Submazos de { $name }
decks-fold-show = Mostrar submazos
decks-fold-hide = Ocultar submazos
decks-archived-note = (archivado)
decks-unarchive = desarchivar
decks-study = estudiar
decks-col-goal = Objetivo
decks-more-label = Más opciones de { $name }
decks-more-title = Más opciones
decks-more-settings = Configuración del mazo
decks-more-new = Más tarjetas nuevas hoy

## Hoy (la página de inicio)

today-heading = Hoy
today-due = pendientes ahora
today-new = nuevas hoy
today-reviewed = repasadas hoy
today-start = Empezar a estudiar
today-caught-up = Todo al día. No hay nada pendiente por ahora.
today-more-new = Más tarjetas nuevas hoy:
today-more-new-boosted =
    { $count ->
        [one] Más tarjetas nuevas hoy (ya va +{ $count }):
        [many] Más tarjetas nuevas hoy (ya van +{ $count }):
       *[other] Más tarjetas nuevas hoy (ya van +{ $count }):
    }
today-decks = Mazos
today-no-decks-html = Todavía no tienes mazos. <a href="/decks">Crea uno</a> o <a href="/import">importa uno</a>.
today-connect = Conecta tu IA
today-connect-sub = Crea tarjetas y estudia con Claude, ChatGPT, Grok, Mistral o Gemini. Se configura en un minuto.
today-connect-dismiss = Descartar

## La página de mazos

decks-title = Mazos - Flash
decks-heading = Mazos
decks-show-archived = Mostrar archivados
decks-hide-archived = Ocultar archivados
decks-import = Importar
decks-new-name = Nombre del mazo nuevo
decks-create = Crear
decks-none = Todavía no hay mazos.
decks-all-archived = Todos los mazos están archivados.

## La página de un mazo

deck-title = { $name } - Flash
deck-crumbs = Mazos superiores
deck-unarchive = Desarchivar
deck-archived-hint = Este mazo está archivado. Sus tarjetas se conservan, pero no entran en la cola de estudio.
deck-front = Anverso
deck-back = Reverso
deck-tags = Etiquetas
deck-tags-placeholder = examen-2, cardio
deck-tags-hint = Separadas por comas. Opcional.
deck-add = Agregar tarjeta
deck-advanced = Avanzado
deck-search = Buscar en este mazo…
deck-subdecks-heading = Submazos
deck-new-subdeck = Nombre del submazo nuevo
deck-add-subdeck = Agregar submazo
deck-subnew-open = Nuevo submazo
deck-subnew-cancel = Cancelar

## La página de un mazo: su cabecera, sus submazos, la búsqueda y el filtro,
## Agregar y la barra de acciones sobre las tarjetas seleccionadas

# accept: length: the word after the count of due cards; "pendientes" is the word for due, taken from Anki's Spanish, and Spanish has nothing as short as "due".
deck-meta-due =
    { $count ->
        [one] pendiente
        [many] pendientes
       *[other] pendientes
    }
deck-meta-new =
    { $count ->
        [one] nueva
        [many] nuevas
       *[other] nuevas
    }
deck-meta-cards =
    { $count ->
        [one] tarjeta
        [many] de tarjetas
       *[other] tarjetas
    }
deck-study-short = Estudiar
deck-more-label = Más opciones de este mazo
deck-more-title = Más opciones
deck-export-apkg = Exportar como mazo de Anki (.apkg)
deck-export-csv = Exportar como hoja de cálculo (.csv)
deck-archive-deck = Archivar mazo
deck-sub-cards =
    { $count ->
        [one] { $count } tarjeta
        [many] { $count } de tarjetas
       *[other] { $count } tarjetas
    }
deck-sub-study-label = Estudiar { $name }
deck-subs-all =
    { $count ->
        [one] Mostrar todos ({ $count })
        [many] Mostrar todos ({ $count })
       *[other] Mostrar todos ({ $count })
    }
deck-subs-archived =
    { $count ->
        [one] { $count } submazo archivado
        [many] { $count } de submazos archivados
       *[other] { $count } submazos archivados
    }
deck-search-n =
    { $count ->
        [one] Buscar en { $count } tarjeta…
        [many] Buscar en { $count } de tarjetas…
       *[other] Buscar en { $count } tarjetas…
    }
deck-filter = Filtrar
deck-filter-show = Mostrar
deck-filter-active = Activas
deck-filter-archived = Archivadas
deck-filter-decks = Mazos
deck-filter-tags = Etiquetas
deck-filter-find-tag = Buscar una etiqueta…
deck-add-open = Agregar
deck-add-more = Más formas de agregar tarjetas
deck-add-write = Escribir una tarjeta
deck-add-import = Importar un mazo de Anki
deck-add-close = Cerrar
deck-bulk-label = Tarjetas seleccionadas
deck-bulk-tags = Etiquetas
deck-bulk-tags-label = Agregar o quitar etiquetas
deck-bulk-tag-add = Agregar
deck-bulk-tag-remove = Quitar
deck-bulk-move = Mover
deck-bulk-move-label = Mover al mazo
deck-bulk-archive = Archivar
deck-bulk-unarchive = Desarchivar
deck-bulk-delete = Eliminar
deck-bulk-clear = Quitar selección (Esc)
deck-bulk-delete-warn = ¿Eliminarlas definitivamente? No se puede deshacer.

## La configuración de un mazo

deck-settings-title = Configuración de { $name } - Flash
deck-settings-label = Configuración del mazo
deck-settings-back = Volver al mazo
deck-name-heading = Nombre
deck-name-label = Nombre del mazo
deck-rename = Cambiar nombre
deck-saved = Guardado
deck-parent-heading = Mazo superior
deck-parent-hint =
    { $count ->
        [0] El mazo que contiene a este. Estudiar un mazo incluye sus submazos.
        [one] El mazo que contiene a este. Estudiar un mazo incluye sus submazos. Si mueves este mazo, su submazo se mueve con él.
        [many] El mazo que contiene a este. Estudiar un mazo incluye sus submazos. Si mueves este mazo, sus { $count } de submazos se mueven con él.
       *[other] El mazo que contiene a este. Estudiar un mazo incluye sus submazos. Si mueves este mazo, sus { $count } submazos se mueven con él.
    }
deck-parent-label = Mazo superior
deck-top-level = (nivel superior)
deck-move = Mover
deck-moved = Movido
deck-limits-heading = Límites diarios
deck-limit-new = Tarjetas nuevas / día
deck-limit-reviews = Máx. de repasos / día
deck-limit-default = predeterminado de la cuenta ({ $count })
deck-save-limits = Guardar límites
deck-boosted = +{ $count } hoy
deck-archived-heading = Mazo archivado
deck-archived-explain =
    { $count ->
        [0] Este mazo está fuera de la cola de estudio y de los pendientes. Si lo desarchivas, sus tarjetas vuelven con el historial de repasos que tenían.
        [one] Este mazo está fuera de la cola de estudio y de los pendientes. Si lo desarchivas, sus tarjetas vuelven con el historial de repasos que tenían, y también su submazo.
        [many] Este mazo está fuera de la cola de estudio y de los pendientes. Si lo desarchivas, sus tarjetas vuelven con el historial de repasos que tenían, y también sus { $count } de submazos.
       *[other] Este mazo está fuera de la cola de estudio y de los pendientes. Si lo desarchivas, sus tarjetas vuelven con el historial de repasos que tenían, y también sus { $count } submazos.
    }
deck-archived-parents = Los mazos que lo contienen también se desarchivan.
deck-archived-toast = Archivado
deck-archive-heading = Archivar mazo
deck-archive-explain =
    { $count ->
        [0] Conserva cada tarjeta, sus archivos multimedia y su historial de repasos, y saca el mazo del estudio y de los pendientes. Puedes desarchivarlo cuando quieras.
        [one] Conserva cada tarjeta, sus archivos multimedia y su historial de repasos, y saca del estudio y de los pendientes el mazo y su submazo. Puedes desarchivarlo cuando quieras.
        [many] Conserva cada tarjeta, sus archivos multimedia y su historial de repasos, y saca del estudio y de los pendientes el mazo y sus { $count } de submazos. Puedes desarchivarlo cuando quieras.
       *[other] Conserva cada tarjeta, sus archivos multimedia y su historial de repasos, y saca del estudio y de los pendientes el mazo y sus { $count } submazos. Puedes desarchivarlo cuando quieras.
    }
deck-archive = Archivar
deck-restored = Restaurado
deck-delete-heading = Eliminar mazo
deck-delete-explain =
    { $count ->
        [0] Elimina este mazo y todas sus tarjetas. El historial de repasos se conserva. No se puede deshacer.
        [one] Elimina este mazo, su submazo y todas sus tarjetas. El historial de repasos se conserva. No se puede deshacer.
        [many] Elimina este mazo, sus { $count } de submazos y todas sus tarjetas. El historial de repasos se conserva. No se puede deshacer.
       *[other] Elimina este mazo, sus { $count } submazos y todas sus tarjetas. El historial de repasos se conserva. No se puede deshacer.
    }
deck-delete-confirm =
    { $subdecks ->
        [0] { $cards ->
            [one] ¿Eliminar este mazo y su única tarjeta? No se puede deshacer.
            [many] ¿Eliminar este mazo y sus { $cards } de tarjetas? No se puede deshacer.
           *[other] ¿Eliminar este mazo y sus { $cards } tarjetas? No se puede deshacer.
        }
        [one] { $cards ->
            [one] ¿Eliminar este mazo, su submazo y su única tarjeta? No se puede deshacer.
            [many] ¿Eliminar este mazo, su submazo y los { $cards } de tarjetas? No se puede deshacer.
           *[other] ¿Eliminar este mazo, su submazo y las { $cards } tarjetas? No se puede deshacer.
        }
        [many] { $cards ->
            [one] ¿Eliminar este mazo, sus { $subdecks } de submazos y su única tarjeta? No se puede deshacer.
            [many] ¿Eliminar este mazo, sus { $subdecks } de submazos y los { $cards } de tarjetas? No se puede deshacer.
           *[other] ¿Eliminar este mazo, sus { $subdecks } de submazos y las { $cards } tarjetas? No se puede deshacer.
        }
       *[other] { $cards ->
            [one] ¿Eliminar este mazo, sus { $subdecks } submazos y su única tarjeta? No se puede deshacer.
            [many] ¿Eliminar este mazo, sus { $subdecks } submazos y los { $cards } de tarjetas? No se puede deshacer.
           *[other] ¿Eliminar este mazo, sus { $subdecks } submazos y las { $cards } tarjetas? No se puede deshacer.
        }
    }
deck-delete = Eliminar mazo

## Por qué no se agregó una tarjeta (un aviso sobre el formulario)

card-not-added-cloze = No se agregó la tarjeta: una nota de respuesta anidada necesita al menos una respuesta {"{{"}c1::…{"}}"}. Selecciona la respuesta y presiona Ctrl+Shift+C.
card-not-added-empty = No se agregó la tarjeta: los dos lados necesitan contenido.
card-not-added-long = No se agregó la tarjeta: uno de los lados es demasiado largo.
card-not-added-media = No se agregó la tarjeta: usa archivos multimedia que no son tuyos.
card-not-added-invalid = No se agregó la tarjeta: no se pudo guardar el contenido.
card-not-added-limit = No se agregó la tarjeta: esta cuenta llegó a su límite de tarjetas.

## Por qué no se pudo cambiar el nombre de un mazo o moverlo (una etiqueta junto al título)

deck-error-nest = un nombre no puede contener “::”; usa Mazo superior para anidar este mazo
deck-error-empty = el nombre no puede estar vacío
deck-error-taken = ya tienes un mazo con ese nombre
deck-error-self = un mazo no puede moverse dentro de sí mismo
deck-error-long = el nombre completo del mazo quedaría demasiado largo
deck-error-archived = ese mazo está archivado; desarchívalo primero
deck-error-cap = esta cuenta ya tiene todos los mazos que admite
deck-error-invalid = no funcionó

## Rechazos al nombrar o crear un mazo (una oración, que también muestran la app
## y los asistentes)

deck-error-nest-sentence = Un nombre no puede contener “::”. Usa “Mazo superior” para anidar este mazo.
deck-error-name-new = Ponle nombre al mazo nuevo.
deck-error-archived-sentence = Tu mazo “{ $name }” está archivado. Desarchívalo o elige otro nombre.

## La fecha objetivo de un mazo: el día para el que quieres sabértelo (un
## examen, un viaje, una entrega). Flash reparte sus tarjetas nuevas para
## terminarlas antes y, en los últimos días, repasa las que lo necesitan.

goal-error-past = Elige una fecha posterior a hoy.
goal-error-far = Elige una fecha dentro de los próximos cinco años.

goal-set = Establecer fecha objetivo…
goal-change = Cambiar fecha objetivo…
goal-remove = Quitar fecha objetivo

goal-left-days = { $count } d
goal-left-today = Hoy
goal-left-done = Terminado
goal-left-month-year = { $month } { $year }

goal-tip-on-track = Vas bien para el { $date }
goal-tip-behind = Con atraso para el { $date }
goal-tip-learned =
    { $total ->
        [one] { $learned } de { $total } tarjeta aprendida.
        [many] { $learned } de { $total } de tarjetas aprendidas.
       *[other] { $learned } de { $total } tarjetas aprendidas.
    }
goal-tip-pace =
    { $count ->
        [one] { $count } nueva al día hasta el { $date } y luego un último repaso.
        [many] { $count } nuevas al día hasta el { $date } y luego un último repaso.
       *[other] { $count } nuevas al día hasta el { $date } y luego un último repaso.
    }
goal-tip-final = Los últimos repasos antes del { $date }.
goal-tip-catch-up =
    { $count ->
        [one] Con { $count } tarjeta nueva más hoy te pones al día.
        [many] Con { $count } de tarjetas nuevas más hoy te pones al día.
       *[other] Con { $count } tarjetas nuevas más hoy te pones al día.
    }
goal-tip-today = Tu fecha objetivo es hoy. ¡Mucha suerte!
goal-tip-passed = Tu fecha objetivo fue el { $date }.
goal-passed = Tu fecha objetivo ya pasó.
goal-archive = Archivar mazo
goal-keep = Seguir estudiando

goal-heading-set = Establecer fecha objetivo
goal-heading-change = Cambiar fecha objetivo
goal-card-heading = Fecha objetivo
goal-date-label = ¿Para cuándo necesitas sabértelo?
goal-picks = Opciones rápidas
goal-pick-weeks =
    { $count ->
        [one] { $count } semana
        [many] { $count } de semanas
       *[other] { $count } semanas
    }
goal-pick-months =
    { $count ->
        [one] { $count } mes
        [many] { $count } de meses
       *[other] { $count } meses
    }
goal-pick-year = 1 año
goal-plan-pace-html =
    { $count ->
        [one] <b>{ $count } tarjeta nueva al día</b> hasta el { $date } ({ $unseen } aún sin ver)
        [many] <b>{ $count } de tarjetas nuevas al día</b> hasta el { $date } ({ $unseen } aún sin ver)
       *[other] <b>{ $count } tarjetas nuevas al día</b> hasta el { $date } ({ $unseen } aún sin ver)
    }
goal-plan-seen-html = <b>No quedan tarjetas sin ver.</b> Los repasos las mantienen frescas hasta ese día.
goal-plan-final-html = <b>Del { $from } al { $to }:</b> un último repaso de las tarjetas que lo necesitan
goal-plan-final-close-html = <b>Hasta el { $to }:</b> un último repaso de cada tarjeta que lo necesite
goal-plan-after-html = <b>Después del { $date },</b> los repasos vuelven a la normalidad. Entonces puedes archivar el mazo.
goal-plan-pick = Elige una fecha para ver el plan.
goal-tight = Son unas { $count } tarjetas nuevas al día. Con una fecha posterior, los días serán más ligeros.
goal-save = Guardar objetivo
goal-cancel = Cancelar
