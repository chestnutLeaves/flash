### The open core's words in Portuguese, both variants: preview.

## A página

preview-title = Pré-visualização - Flash
preview-heading = Pré-visualização
preview-unaffected = Sem afetar a repetição espaçada
preview-end = Encerrar
preview-empty = Este baralho não tem cartões para ver.
preview-go-deck = Voltar para { $name }
preview-missed = Errei
preview-got = Acertei

## O final

preview-unaffected-line = A pré-visualização não afeta a repetição espaçada.
preview-ready =
    { $count ->
        [one] { $count } cartão está pronto para estudar agora.
        [many] { $count } de cartões estão prontos para estudar agora.
       *[other] { $count } cartões estão prontos para estudar agora.
    }
preview-next-later-today =
    { $count ->
        [0] As próximas revisões são ainda hoje.
        [one] A próxima revisão é ainda hoje: { $count } cartão.
        [many] As próximas revisões são ainda hoje: { $count } de cartões.
       *[other] As próximas revisões são ainda hoje: { $count } cartões.
    }
preview-next-tomorrow =
    { $count ->
        [0] As próximas revisões são amanhã.
        [one] A próxima revisão é amanhã: { $count } cartão.
        [many] As próximas revisões são amanhã: { $count } de cartões.
       *[other] As próximas revisões são amanhã: { $count } cartões.
    }
preview-next-weekday =
    { $count ->
        [0] As próximas revisões ficam para { $day }.
        [one] A próxima revisão fica para { $day }: { $count } cartão.
        [many] As próximas revisões ficam para { $day }: { $count } de cartões.
       *[other] As próximas revisões ficam para { $day }: { $count } cartões.
    }
preview-next-in-days =
    { $count ->
        [0] As próximas revisões são daqui a { $days } dias.
        [one] A próxima revisão é daqui a { $days } dias: { $count } cartão.
        [many] As próximas revisões são daqui a { $days } dias: { $count } de cartões.
       *[other] As próximas revisões são daqui a { $days } dias: { $count } cartões.
    }
preview-start-over = Recomeçar

## O menu do baralho

preview-menu =
    { $count ->
        [one] Pré-visualizar o cartão
        [many] Pré-visualizar os { $count } de cartões
       *[other] Pré-visualizar os { $count } cartões
    }
preview-menu-sub = Sem afetar a repetição espaçada

## O que o script da página diz

js-preview-score = Você acertou { $got } de { $total }
js-preview-percent = { $percent }%
js-preview-review-missed =
    { $count ->
        [one] Revisar o que você errou
        [many] Revisar os { $count } que você errou
       *[other] Revisar os { $count } que você errou
    }
