### Portuguese as Portugal words it, where it differs from the shared base (pt): study.

## A página

## O cartão

study-type-answer = Escreva a resposta…

## As quatro avaliações, de esquecer a saber de cor. As palavras do próprio
## Anki; como o app de cartões da pessoa as diz.

study-grade-again = Errei

## Uma sessão concluída

study-done-held-reviewed =
    { $count ->
        [one] { $count } revisto.
        [many] { $count } revistos.
       *[other] { $count } revistos.
    }
study-done-held-reviewed-took =
    { $count ->
        [one] { $count } revisto { $took }.
        [many] { $count } revistos { $took }.
       *[other] { $count } revistos { $took }.
    }
study-done-reviewed =
    { $count ->
        [one] { $count } cartão revisto
        [many] { $count } de cartões revistos
       *[other] { $count } cartões revistos
    }
study-done-reviewed-took =
    { $count ->
        [one] { $count } cartão revisto { $took }
        [many] { $count } de cartões revistos { $took }
       *[other] { $count } cartões revistos { $took }
    }
study-done-nothing-due = Nada a rever agora.
study-pill-again = { $count } errei
study-week = Cartões a rever na próxima semana
study-remind = Enviar um email quando houver cartões a rever
study-remind-elsewhere = Pode ativar os lembretes nas Definições.
study-goal-catch-up =
    { $count ->
        [one] Mais { $count } hoje e volta a ficar em dia.
        [many] Mais { $count } hoje e volta a ficar em dia.
       *[other] Mais { $count } hoje e volta a ficar em dia.
    }
study-go-decks = Ir para os seus baralhos
study-archived = Cartão arquivado. Continua no baralho, fora do estudo.
study-undo = Anular

## Quando vêm os próximos cartões, depois que a sessão acabou. $count é
## quantos cartões; 0 quer dizer um número desconhecido ("Cartões").

study-next-tomorrow =
    { $count ->
        [0] Cartões a rever amanhã
        [one] { $count } cartão a rever amanhã
        [many] { $count } de cartões a rever amanhã
       *[other] { $count } cartões a rever amanhã
    }
study-next-weekday =
    { $count ->
        [0] Cartões a rever: { $day }
        [one] { $count } cartão a rever: { $day }
        [many] { $count } de cartões a rever: { $day }
       *[other] { $count } cartões a rever: { $day }
    }
study-next-in-days =
    { $count ->
        [0] Cartões a rever daqui a { $days } dias
        [one] { $count } cartão a rever daqui a { $days } dias
        [many] { $count } de cartões a rever daqui a { $days } dias
       *[other] { $count } cartões a rever daqui a { $days } dias
    }

## Recusas durante o estudo

study-edit-removes-card = Esta edição removeria o cartão que está a estudar. Mantenha a respetiva omissão ou use Edição avançada.
