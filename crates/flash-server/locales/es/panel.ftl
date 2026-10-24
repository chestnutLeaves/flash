### The open core's words in Spanish, both variants: panel.

## El encabezado

panel-more = Más
panel-more-label = Más opciones de esta tarjeta
panel-edit-card = Editar tarjeta
panel-archive-card = Archivar tarjeta
panel-expand = Ampliar
panel-expand-title = Ver esta tarjeta en pantalla completa
panel-expand-label = Ampliar: ver esta tarjeta en pantalla completa
panel-tags = Etiquetas:

## Estudiar

panel-loading = Cargando tus tarjetas...
panel-show-all = Mostrar todo
panel-show-answer = Mostrar respuesta
panel-key-space = Espacio
panel-again = Otra vez
panel-hard = Difícil
panel-good = Bien
panel-easy = Fácil
panel-key = Tecla { $key }
panel-to-go =
    { $count ->
        [one] Queda { $count }
        [many] Quedan { $count }
       *[other] Quedan { $count }
    }
panel-archived = Tarjeta archivada
panel-undo = Deshacer

## Editar una tarjeta en el panel

panel-front = Anverso
panel-back = Reverso
panel-advanced-edit = Edición avanzada
panel-save = Guardar
panel-cancel = Cancelar
panel-save-failed = No se pudo guardar la tarjeta. Vuelve a intentarlo.

## Una sesión terminada

panel-held-title = Ya viste las tarjetas nuevas de hoy
panel-held-text =
    { $count ->
        [one] Queda { $count } tarjeta más en espera por el límite de hoy.
        [many] Quedan { $count } de tarjetas más en espera por el límite de hoy.
       *[other] Quedan { $count } tarjetas más en espera por el límite de hoy.
    }
panel-held-text-summary = { $held } { $summary }.
panel-study-more = Estudiar { $count } más
panel-empty-title = Aún no hay tarjetas aquí
panel-empty-text = Pide tarjetas de tus apuntes, un documento o un tema, y estúdialas aquí.
panel-make = Crear tarjetas
panel-done-title = Sesión terminada
panel-caught-up = Todo al día
panel-nothing-due = No hay nada pendiente por ahora.
panel-cards-reviewed =
    { $count ->
        [one] { $count } tarjeta repasada
        [many] { $count } de tarjetas repasadas
       *[other] { $count } tarjetas repasadas
    }
panel-reviewed-in =
    { $count ->
        [one] { $reviewed } en { $count } minuto
        [many] { $reviewed } en { $count } de minutos
       *[other] { $reviewed } en { $count } minutos
    }
panel-reviewed-quick = { $reviewed } en menos de un minuto
panel-score-again = { $count } otra vez
panel-score-hard = { $count } difícil
panel-score-good = { $count } bien
panel-score-easy = { $count } fácil
panel-next-today =
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
panel-next-tomorrow =
    { $count ->
        [0] Tarjetas pendientes mañana
        [one] { $count } tarjeta pendiente mañana
        [many] { $count } de tarjetas pendientes mañana
       *[other] { $count } tarjetas pendientes mañana
    }
# accept: length: English's text is only "cards due" (11 letters); "pendientes" is the word for due, a weekday takes the article ("el jueves"), and the plural for large numbers adds "de".
panel-next-weekday =
    { $count ->
        [0] Tarjetas pendientes el { $day }
        [one] { $count } tarjeta pendiente el { $day }
        [many] { $count } de tarjetas pendientes el { $day }
       *[other] { $count } tarjetas pendientes el { $day }
    }
panel-next-days =
    { $count ->
        [0] Tarjetas pendientes en { $days } días
        [one] { $count } tarjeta pendiente en { $days } días
        [many] { $count } de tarjetas pendientes en { $days } días
       *[other] { $count } tarjetas pendientes en { $days } días
    }
panel-nothing-scheduled = Aún no hay repasos previstos
panel-remind = Avisarme
panel-remind-title = Recibir correos cuando haya tarjetas pendientes
panel-reminded = Recordatorio activado
panel-remind-fallback = Puedes activar los recordatorios en la configuración de Flash.
panel-progress = Ver tu progreso
panel-ask-deck = Ayúdame a crear tarjetas de estudio para { $deck }.
# accept: length: the person's own message to the assistant, worded like panel-ask-deck: "tarjetas de estudio" tells the assistant these are flashcards where bare "tarjetas" may not, and a tag list takes "con las etiquetas".
panel-ask-tags = Ayúdame a crear tarjetas de estudio con las etiquetas { $tags }.
panel-ask-first = Ayúdame a crear mis primeras tarjetas de estudio.

## Las tarjetas recién creadas o editadas

panel-new-cards =
    { $count ->
        [one] { $count } tarjeta nueva
        [many] { $count } de tarjetas nuevas
       *[other] { $count } tarjetas nuevas
    }
panel-edited =
    { $count ->
        [one] { $count } editada
        [many] { $count } editadas
       *[other] { $count } editadas
    }
panel-before = Antes
panel-previous = Tarjeta anterior
panel-next = Tarjeta siguiente
panel-position = { $at } de { $count }
panel-study-now = Estudiar ahora
panel-see-all = Ver todas

## Problemas

panel-no-answer = Flash no respondió.
panel-no-answer-late = Flash no respondió a tiempo.
panel-try-again = Reintentar
panel-needs-host = Este panel necesita una app compatible con MCP Apps.
