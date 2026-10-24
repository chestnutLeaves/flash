### The open core's words in Spanish, both variants: preview.

## La página

preview-title = Vista previa - Flash
preview-heading = Vista previa
preview-unaffected = Sin afectar la repetición espaciada
preview-end = Terminar
preview-empty = Este mazo no tiene tarjetas que mostrar.
preview-go-deck = Volver a { $name }
preview-missed = Fallé
preview-got = Acerté

## El final

preview-unaffected-line = La vista previa no afecta la repetición espaciada.
preview-ready =
    { $count ->
        [one] { $count } tarjeta está lista para estudiar ahora.
        [many] { $count } de tarjetas están listas para estudiar ahora.
       *[other] { $count } tarjetas están listas para estudiar ahora.
    }
preview-next-later-today =
    { $count ->
        [0] Los próximos repasos son más tarde hoy.
        [one] El próximo repaso es más tarde hoy: { $count } tarjeta.
        [many] Los próximos repasos son más tarde hoy: { $count } de tarjetas.
       *[other] Los próximos repasos son más tarde hoy: { $count } tarjetas.
    }
preview-next-tomorrow =
    { $count ->
        [0] Los próximos repasos son mañana.
        [one] El próximo repaso es mañana: { $count } tarjeta.
        [many] Los próximos repasos son mañana: { $count } de tarjetas.
       *[other] Los próximos repasos son mañana: { $count } tarjetas.
    }
preview-next-weekday =
    { $count ->
        [0] Los próximos repasos son el { $day }.
        [one] El próximo repaso es el { $day }: { $count } tarjeta.
        [many] Los próximos repasos son el { $day }: { $count } de tarjetas.
       *[other] Los próximos repasos son el { $day }: { $count } tarjetas.
    }
preview-next-in-days =
    { $count ->
        [0] Los próximos repasos son dentro de { $days } días.
        [one] El próximo repaso es dentro de { $days } días: { $count } tarjeta.
        [many] Los próximos repasos son dentro de { $days } días: { $count } de tarjetas.
       *[other] Los próximos repasos son dentro de { $days } días: { $count } tarjetas.
    }
preview-start-over = Empezar de nuevo

## El menú del mazo

preview-menu =
    { $count ->
        [one] Vista previa de la tarjeta
        [many] Vista previa de las { $count } de tarjetas
       *[other] Vista previa de las { $count } tarjetas
    }
preview-menu-sub = Sin afectar la repetición espaciada

## Lo que dice el script de la página

js-preview-score = Acertaste { $got } de { $total }
js-preview-percent = { $percent }%
js-preview-review-missed =
    { $count ->
        [one] Repasar la que fallaste
        [many] Repasar las { $count } que fallaste
       *[other] Repasar las { $count } que fallaste
    }
