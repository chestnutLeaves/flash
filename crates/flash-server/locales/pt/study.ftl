### The open core's words in Portuguese, both variants: study.

## A página

study-title = Estudar - Flash
study-heading = Estudar

## Estudar por etiquetas (uma caixa que limita a sessão a algumas etiquetas)

study-tags = Etiquetas
study-tags-some = Etiquetas: { $tags }
study-tags-more = Etiquetas: { $first }, { $second } +{ $more }
study-tags-heading = Estudar por etiquetas
study-tags-none-deck = Ainda não há etiquetas neste baralho. As etiquetas são definidas ao adicionar ou editar um cartão.
study-tags-none = Ainda não há etiquetas. As etiquetas são definidas ao adicionar ou editar um cartão.
study-tags-filter = Filtrar etiquetas
study-tags-no-match = Nenhuma etiqueta corresponde.
study-tags-every = Um cartão só entra na sessão se tiver todas as etiquetas escolhidas.
study-tags-clear = Limpar
study-tags-apply = Aplicar

## O cartão

study-more-label = Mais opções deste cartão
study-more-title = Mais
study-edit-card = Editar cartão
study-archive-card = Arquivar cartão
study-type-answer = Digite a resposta…
study-show-answer = Mostrar resposta
study-key-enter = enter
study-key-space = espaço
study-left =
    { $count ->
        [one] Falta { $count }
        [many] Faltam { $count }
       *[other] Faltam { $count }
    }

## As quatro avaliações, de esquecer a saber de cor. As palavras do próprio
## Anki; como o app de cartões da pessoa as diz.

study-grade-again = De novo
study-grade-hard = Difícil
study-grade-good = Bom
study-grade-easy = Fácil

## Quando um cartão volta depois de cada avaliação, sob os botões: bem curto,
## como cabe num botão.

# accept: length: the unit is spelled as Anki's Portuguese writes it ("min"), with a space, where English has a one-letter unit.
interval-minutes = { $count } min
interval-hours = { $count } h
# accept: length: Portuguese has no one-letter unit for a day; it is spelled as Anki does ("dia", "dias").
interval-days =
    { $count ->
        [one] { $count } dia
        [many] { $count } dias
       *[other] { $count } dias
    }
interval-months =
    { $count ->
        [one] { $count } mês
        [many] { $count } meses
       *[other] { $count } meses
    }
# accept: length: Portuguese has no one-letter unit for a year; it is spelled as Anki does ("ano", "anos").
interval-years =
    { $count ->
        [one] { $count } ano
        [many] { $count } anos
       *[other] { $count } anos
    }

## Uma sessão concluída

study-done-limit-heading = Acabaram os cartões novos de hoje
study-done-heading = Sessão concluída
study-done-caught-up = Tudo em dia
study-done-held-html =
    { $count ->
        [one] Mais <strong>{ $count }</strong> cartão ficou retido pelo limite de hoje.
        [many] Mais <strong>{ $count }</strong> de cartões ficaram retidos pelo limite de hoje.
       *[other] Mais <strong>{ $count }</strong> cartões ficaram retidos pelo limite de hoje.
    }
study-done-held-reviewed =
    { $count ->
        [one] { $count } revisado.
        [many] { $count } revisados.
       *[other] { $count } revisados.
    }
study-done-held-reviewed-took =
    { $count ->
        [one] { $count } revisado { $took }.
        [many] { $count } revisados { $took }.
       *[other] { $count } revisados { $took }.
    }
study-done-reviewed =
    { $count ->
        [one] { $count } cartão revisado
        [many] { $count } de cartões revisados
       *[other] { $count } cartões revisados
    }
study-done-reviewed-took =
    { $count ->
        [one] { $count } cartão revisado { $took }
        [many] { $count } de cartões revisados { $took }
       *[other] { $count } cartões revisados { $took }
    }
study-took-under-minute = em menos de um minuto
study-took-minutes =
    { $count ->
        [one] em { $count } minuto
        [many] em { $count } de minutos
       *[other] em { $count } minutos
    }
study-done-nothing-due = Nada a revisar agora.
study-pill-good = { $count } bom
study-pill-easy = { $count } fácil
study-pill-hard = { $count } difícil
study-pill-again = { $count } de novo
study-more-new = Mais cartões novos hoje:
study-week = Cartões a revisar na próxima semana
study-nothing-scheduled = Nada agendado ainda
study-remind = Enviar um e-mail quando houver cartões a revisar
study-remind-elsewhere = Você pode ativar os lembretes nas Configurações.
study-back-today = Voltar para Hoje
study-goal-on-track = Em dia para { $date }
study-goal-behind =
    { $count ->
        [one] { $count } cartão de atraso para { $date }
        [many] { $count } de cartões de atraso para { $date }
       *[other] { $count } cartões de atraso para { $date }
    }
study-goal-learned =
    { $total ->
        [one] { $learned } de { $total } cartão aprendido em { $deck }.
        [many] { $learned } de { $total } de cartões aprendidos em { $deck }.
       *[other] { $learned } de { $total } cartões aprendidos em { $deck }.
    }
study-goal-days-left =
    { $count ->
        [one] Falta { $count } dia.
        [many] Faltam { $count } de dias.
       *[other] Faltam { $count } dias.
    }
study-goal-catch-up =
    { $count ->
        [one] Mais { $count } hoje e você volta a ficar em dia.
        [many] Mais { $count } hoje e você volta a ficar em dia.
       *[other] Mais { $count } hoje e você volta a ficar em dia.
    }
study-goal-study-more = Estudar mais { $count }
study-goal-percent = { $count }%
study-empty = Ainda não há cartões aqui.
study-go-decks = Ir para seus baralhos
study-archived = Cartão arquivado. Ele continua no baralho, fora do estudo.
study-undo = Desfazer

## Quando vêm os próximos cartões, depois que a sessão acabou. $count é
## quantos cartões; 0 quer dizer um número desconhecido ("Cartões").

study-next-later-today =
    { $count ->
        [0] Mais cartões ainda hoje, por volta { $hour ->
            [0] da
            [1] da
           *[other] das
        } { $time }
        [one] Mais { $count } cartão ainda hoje, por volta { $hour ->
            [0] da
            [1] da
           *[other] das
        } { $time }
        [many] Mais { $count } de cartões ainda hoje, por volta { $hour ->
            [0] da
            [1] da
           *[other] das
        } { $time }
       *[other] Mais { $count } cartões ainda hoje, por volta { $hour ->
            [0] da
            [1] da
           *[other] das
        } { $time }
    }
study-next-tomorrow =
    { $count ->
        [0] Cartões a revisar amanhã
        [one] { $count } cartão a revisar amanhã
        [many] { $count } de cartões a revisar amanhã
       *[other] { $count } cartões a revisar amanhã
    }
# accept: length: "due" is the two-word "a revisar" in Anki's Portuguese, and a weekday takes no article here so the line reads right for every day.
study-next-weekday =
    { $count ->
        [0] Cartões a revisar: { $day }
        [one] { $count } cartão a revisar: { $day }
        [many] { $count } de cartões a revisar: { $day }
       *[other] { $count } cartões a revisar: { $day }
    }
study-next-in-days =
    { $count ->
        [0] Cartões a revisar em { $days } dias
        [one] { $count } cartão a revisar em { $days } dias
        [many] { $count } de cartões a revisar em { $days } dias
       *[other] { $count } cartões a revisar em { $days } dias
    }

## Recusas durante o estudo

study-edit-removes-card = Esta edição removeria o cartão que você está estudando. Mantenha a omissão dele ou use Edição avançada.
