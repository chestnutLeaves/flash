### The open core's words in Spanish, both variants: study.

## La página

study-title = Estudiar - Flash
study-heading = Estudiar

## Estudiar por etiquetas (un cuadro que limita la sesión a algunas etiquetas)

study-tags = Etiquetas
study-tags-some = Etiquetas: { $tags }
study-tags-more = Etiquetas: { $first }, { $second } +{ $more }
study-tags-heading = Estudiar por etiquetas
study-tags-none-deck = Este mazo todavía no tiene etiquetas. Las etiquetas se asignan al crear o editar una tarjeta.
study-tags-none = Todavía no hay etiquetas. Las etiquetas se asignan al crear o editar una tarjeta.
study-tags-filter = Filtrar etiquetas
study-tags-no-match = Ninguna etiqueta coincide.
study-tags-every = Una tarjeta entra en la sesión solo si tiene todas las etiquetas elegidas.
study-tags-clear = Limpiar
study-tags-apply = Aplicar

## La tarjeta

study-more-label = Más opciones para esta tarjeta
study-more-title = Más
study-edit-card = Editar tarjeta
study-archive-card = Archivar tarjeta
study-type-answer = Escribe la respuesta…
study-show-answer = Mostrar respuesta
study-key-enter = enter
study-key-space = espacio
study-left =
    { $count ->
        [one] Queda { $count }
        [many] Quedan { $count }
       *[other] Quedan { $count }
    }

## Las cuatro calificaciones, de olvidarla a saberla de memoria. Las palabras
## de Anki.

study-grade-again = Otra vez
study-grade-hard = Difícil
study-grade-good = Bien
study-grade-easy = Fácil

## Cuándo vuelve una tarjeta tras cada calificación, bajo los botones: muy
## corto.

interval-minutes = { $count } m
interval-hours = { $count } h
interval-days = { $count } d
interval-months = { $count } me
interval-years = { $count } añ

## Una sesión terminada

study-done-limit-heading = Ya viste las tarjetas nuevas de hoy
study-done-heading = Sesión terminada
study-done-caught-up = Todo al día
study-done-held-html =
    { $count ->
        [one] El límite de hoy deja <strong>{ $count }</strong> tarjeta más en espera.
        [many] El límite de hoy deja <strong>{ $count }</strong> de tarjetas más en espera.
       *[other] El límite de hoy deja <strong>{ $count }</strong> tarjetas más en espera.
    }
study-done-held-reviewed =
    { $count ->
        [one] { $count } repasada.
        [many] { $count } repasadas.
       *[other] { $count } repasadas.
    }
study-done-held-reviewed-took =
    { $count ->
        [one] { $count } repasada { $took }.
        [many] { $count } repasadas { $took }.
       *[other] { $count } repasadas { $took }.
    }
study-done-reviewed =
    { $count ->
        [one] { $count } tarjeta repasada
        [many] { $count } de tarjetas repasadas
       *[other] { $count } tarjetas repasadas
    }
study-done-reviewed-took =
    { $count ->
        [one] { $count } tarjeta repasada { $took }
        [many] { $count } de tarjetas repasadas { $took }
       *[other] { $count } tarjetas repasadas { $took }
    }
study-took-under-minute = en menos de un minuto
study-took-minutes =
    { $count ->
        [one] en { $count } minuto
        [many] en { $count } de minutos
       *[other] en { $count } minutos
    }
study-done-nothing-due = No hay nada pendiente por ahora.
study-pill-good = { $count } bien
study-pill-easy = { $count } fácil
study-pill-hard = { $count } difícil
study-pill-again = { $count } otra vez
study-more-new = Más tarjetas nuevas hoy:
study-week = Tarjetas pendientes en la próxima semana
study-nothing-scheduled = Todavía no hay nada programado
study-remind = Recibir correos cuando haya tarjetas pendientes
study-remind-elsewhere = Puedes activar los recordatorios en Configuración.
study-back-today = Volver a Hoy
study-goal-on-track = Vas bien para el { $date }
study-goal-behind =
    { $count ->
        [one] { $count } tarjeta de atraso para el { $date }
        [many] { $count } de tarjetas de atraso para el { $date }
       *[other] { $count } tarjetas de atraso para el { $date }
    }
study-goal-learned =
    { $total ->
        [one] { $learned } de { $total } tarjeta aprendida en { $deck }.
        [many] { $learned } de { $total } de tarjetas aprendidas en { $deck }.
       *[other] { $learned } de { $total } tarjetas aprendidas en { $deck }.
    }
study-goal-days-left =
    { $count ->
        [one] Queda { $count } día.
        [many] Quedan { $count } de días.
       *[other] Quedan { $count } días.
    }
study-goal-catch-up =
    { $count ->
        [one] Con { $count } más hoy te pones al día.
        [many] Con { $count } más hoy te pones al día.
       *[other] Con { $count } más hoy te pones al día.
    }
study-goal-study-more = Estudiar { $count } más
study-goal-percent = { $count }%
study-empty = Todavía no hay tarjetas aquí.
study-go-decks = Ir a tus mazos
study-archived = Tarjeta archivada. Sigue en su mazo, pero fuera del estudio.
study-undo = Deshacer

## Cuándo llegan las próximas tarjetas, al terminar una sesión

study-next-later-today =
    { $count ->
        [0] Más tarjetas hoy, hacia { $hour ->
            [1] la
           *[other] las
        } { $time }
        [one] { $count } tarjeta más hoy, hacia { $hour ->
            [1] la
           *[other] las
        } { $time }
        [many] { $count } de tarjetas más hoy, hacia { $hour ->
            [1] la
           *[other] las
        } { $time }
       *[other] { $count } tarjetas más hoy, hacia { $hour ->
            [1] la
           *[other] las
        } { $time }
    }
study-next-tomorrow =
    { $count ->
        [0] Tarjetas pendientes mañana
        [one] { $count } tarjeta pendiente mañana
        [many] { $count } de tarjetas pendientes mañana
       *[other] { $count } tarjetas pendientes mañana
    }
# accept: length: English's text is only "cards due" (11 letters); "pendientes" is the word for due, a weekday takes the article ("el jueves"), and the plural for large numbers adds "de".
study-next-weekday =
    { $count ->
        [0] Tarjetas pendientes el { $day }
        [one] { $count } tarjeta pendiente el { $day }
        [many] { $count } de tarjetas pendientes el { $day }
       *[other] { $count } tarjetas pendientes el { $day }
    }
study-next-in-days =
    { $count ->
        [0] Tarjetas pendientes en { $days } días
        [one] { $count } tarjeta pendiente en { $days } días
        [many] { $count } de tarjetas pendientes en { $days } días
       *[other] { $count } tarjetas pendientes en { $days } días
    }

## Rechazos mientras estudias

study-edit-removes-card = Este cambio eliminaría la tarjeta que estás estudiando. Conserva su respuesta anidada o usa “Edición avanzada”.
