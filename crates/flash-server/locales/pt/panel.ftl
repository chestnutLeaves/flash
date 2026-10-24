### The open core's words in Portuguese, both variants: panel.

## O cabeçalho

panel-more = Mais
panel-more-label = Mais opções deste cartão
panel-edit-card = Editar cartão
panel-archive-card = Arquivar cartão
panel-expand = Expandir
panel-expand-title = Mostrar este cartão em tela cheia
panel-expand-label = Expandir: mostrar este cartão em tela cheia
panel-tags = Etiquetas:

## Estudar

panel-loading = Carregando seus cartões…
panel-show-all = Mostrar tudo
panel-show-answer = Mostrar resposta
panel-key-space = Espaço
panel-again = De novo
panel-hard = Difícil
panel-good = Bom
panel-easy = Fácil
panel-key = Tecla { $key }
panel-to-go =
    { $count ->
        [one] Falta { $count }
        [many] Faltam { $count }
       *[other] Faltam { $count }
    }
panel-archived = Cartão arquivado
panel-undo = Desfazer

## Editar um cartão no painel

panel-front = Frente
panel-back = Verso
panel-advanced-edit = Edição avançada
panel-save = Salvar
panel-cancel = Cancelar
panel-save-failed = Não foi possível salvar o cartão. Tente novamente.

## Uma sessão concluída

panel-held-title = Acabaram os cartões novos de hoje
panel-held-text =
    { $count ->
        [one] Mais { $count } cartão ficou retido pelo limite de hoje.
        [many] Mais { $count } de cartões ficaram retidos pelo limite de hoje.
       *[other] Mais { $count } cartões ficaram retidos pelo limite de hoje.
    }
panel-held-text-summary = { $held } { $summary }.
panel-study-more = Estudar mais { $count }
panel-empty-title = Ainda não há cartões aqui
panel-empty-text = Peça cartões a partir das suas anotações, de um documento ou de um tema, e estude-os aqui.
panel-make = Criar cartões
panel-done-title = Sessão concluída
panel-caught-up = Tudo em dia
panel-nothing-due = Nada a revisar agora.
panel-cards-reviewed =
    { $count ->
        [one] { $count } cartão revisado
        [many] { $count } de cartões revisados
       *[other] { $count } cartões revisados
    }
panel-reviewed-in =
    { $count ->
        [one] { $reviewed } em { $count } minuto
        [many] { $reviewed } em { $count } de minutos
       *[other] { $reviewed } em { $count } minutos
    }
panel-reviewed-quick = { $reviewed } em menos de um minuto
panel-score-again = { $count } de novo
panel-score-hard = { $count } difícil
panel-score-good = { $count } bom
panel-score-easy = { $count } fácil
panel-next-today =
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
panel-next-tomorrow =
    { $count ->
        [0] Cartões a revisar amanhã
        [one] { $count } cartão a revisar amanhã
        [many] { $count } de cartões a revisar amanhã
       *[other] { $count } cartões a revisar amanhã
    }
# accept: length: "due" is the two-word "a revisar" in Anki's Portuguese, and a weekday takes no article here so the line reads right for every day.
panel-next-weekday =
    { $count ->
        [0] Cartões a revisar: { $day }
        [one] { $count } cartão a revisar: { $day }
        [many] { $count } de cartões a revisar: { $day }
       *[other] { $count } cartões a revisar: { $day }
    }
panel-next-days =
    { $count ->
        [0] Cartões a revisar em { $days } dias
        [one] { $count } cartão a revisar em { $days } dias
        [many] { $count } de cartões a revisar em { $days } dias
       *[other] { $count } cartões a revisar em { $days } dias
    }
panel-nothing-scheduled = Nada agendado ainda
panel-remind = Ativar lembretes
panel-remind-title = Enviar um e-mail quando houver cartões a revisar
panel-reminded = Lembrete ativado
panel-remind-fallback = Você pode ativar os lembretes nas configurações do Flash.
panel-progress = Ver seu progresso
panel-ask-deck = Me ajude a criar cartões para { $deck }.
panel-ask-tags = Me ajude a criar cartões com as etiquetas { $tags }.
panel-ask-first = Me ajude a criar meus primeiros cartões.

## Os cartões que acabaram de ser criados ou editados

panel-new-cards =
    { $count ->
        [one] { $count } cartão novo
        [many] { $count } de cartões novos
       *[other] { $count } cartões novos
    }
panel-edited =
    { $count ->
        [one] { $count } editado
        [many] { $count } editados
       *[other] { $count } editados
    }
panel-before = Antes
panel-previous = Cartão anterior
panel-next = Próximo cartão
panel-position = { $at } de { $count }
panel-study-now = Estudar agora
panel-see-all = Ver todos

## Problemas

panel-no-answer = O Flash não respondeu.
panel-no-answer-late = O Flash não respondeu a tempo.
panel-try-again = Tentar novamente
panel-needs-host = Este painel precisa de um cliente compatível com MCP Apps.
