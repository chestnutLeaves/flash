### The open core's words in Portuguese, both variants: decks.

## Tabelas de baralhos (Hoje, a página de baralhos, os sub-baralhos de um baralho)

decks-col-deck = Baralho
decks-col-due = A revisar
decks-col-new = Novos
decks-fold-label = Sub-baralhos de { $name }
decks-fold-show = Mostrar sub-baralhos
decks-fold-hide = Ocultar sub-baralhos
decks-archived-note = (arquivado)
decks-unarchive = desarquivar
decks-study = estudar
decks-col-goal = Meta
decks-more-label = Mais opções para { $name }
decks-more-title = Mais opções
decks-more-settings = Configurações do baralho
decks-more-new = Mais cartões novos hoje

## Hoje (a página inicial)

today-heading = Hoje
today-due = a revisar agora
today-new = novos hoje
today-reviewed = revisados hoje
today-start = Começar a estudar
today-caught-up = Tudo em dia. Nada a revisar agora.
today-more-new = Mais cartões novos hoje:
today-more-new-boosted = Mais cartões novos hoje (já +{ $count }):
today-decks = Baralhos
today-no-decks-html = Nenhum baralho ainda. <a href="/decks">Crie um</a> ou <a href="/import">importe</a>.
today-connect = Conecte sua IA
today-connect-sub = Crie cartões e estude com o Claude, ChatGPT, Grok, Mistral ou Gemini. Configuração em um minuto.
today-connect-dismiss = Dispensar

## A página de baralhos

decks-title = Baralhos - Flash
decks-heading = Baralhos
decks-show-archived = Mostrar arquivados
decks-hide-archived = Ocultar arquivados
decks-import = Importar
decks-new-name = Nome do novo baralho
decks-create = Criar
decks-none = Nenhum baralho ainda.
decks-all-archived = Todos os baralhos estão arquivados.

## A página de um baralho

deck-title = { $name } - Flash
deck-crumbs = Baralhos superiores
deck-unarchive = Desarquivar
deck-archived-hint = Este baralho está arquivado. Os cartões continuam aqui, mas ficam fora da fila de estudo.
deck-front = Frente
deck-back = Verso
deck-tags = Etiquetas
deck-tags-placeholder = prova-2, cardiologia
deck-tags-hint = Separadas por vírgula, opcionais.
deck-add = Adicionar cartão
deck-advanced = Avançado
deck-search = Pesquisar neste baralho…
deck-subdecks-heading = Sub-baralhos
deck-new-subdeck = Nome do novo sub-baralho
deck-add-subdeck = Adicionar sub-baralho
deck-subnew-open = Novo sub-baralho
deck-subnew-cancel = Cancelar

## A página de um baralho: o cabeçalho, os sub-baralhos, a pesquisa e o
## filtro, Adicionar e a barra de ações nos cartões selecionados

deck-meta-due =
    { $count ->
        [one] a revisar
        [many] a revisar
       *[other] a revisar
    }
deck-meta-new =
    { $count ->
        [one] novo
        [many] novos
       *[other] novos
    }
deck-meta-cards =
    { $count ->
        [one] cartão
        [many] de cartões
       *[other] cartões
    }
deck-study-short = Estudar
deck-more-label = Mais opções para este baralho
deck-more-title = Mais opções
deck-export-apkg = Exportar como baralho do Anki (.apkg)
deck-export-csv = Exportar como planilha (.csv)
deck-archive-deck = Arquivar baralho
deck-sub-cards =
    { $count ->
        [one] { $count } cartão
        [many] { $count } de cartões
       *[other] { $count } cartões
    }
deck-sub-study-label = Estudar { $name }
deck-subs-all =
    { $count ->
        [one] Mostrar { $count }
        [many] Mostrar todos os { $count }
       *[other] Mostrar todos os { $count }
    }
deck-subs-archived =
    { $count ->
        [one] { $count } sub-baralho arquivado
        [many] { $count } de sub-baralhos arquivados
       *[other] { $count } sub-baralhos arquivados
    }
deck-search-n =
    { $count ->
        [one] Pesquisar em { $count } cartão…
        [many] Pesquisar em { $count } de cartões…
       *[other] Pesquisar em { $count } cartões…
    }
deck-filter = Filtrar
deck-filter-show = Mostrar
deck-filter-active = Ativos
deck-filter-archived = Arquivados
deck-filter-decks = Baralhos
deck-filter-tags = Etiquetas
deck-filter-find-tag = Pesquisar etiquetas…
deck-add-open = Adicionar
deck-add-more = Mais formas de adicionar cartões
deck-add-write = Escrever um cartão
deck-add-import = Importar um baralho do Anki
deck-add-close = Fechar
deck-bulk-label = Cartões selecionados
deck-bulk-tags = Etiquetas
deck-bulk-tags-label = Adicionar ou remover etiquetas
deck-bulk-tag-add = Adicionar
deck-bulk-tag-remove = Remover
deck-bulk-move = Mover
deck-bulk-move-label = Mover para o baralho
deck-bulk-archive = Arquivar
deck-bulk-unarchive = Desarquivar
deck-bulk-delete = Excluir
deck-bulk-clear = Limpar seleção (Esc)
deck-bulk-delete-warn = Excluir estes cartões definitivamente? Esta ação é irreversível.

## As configurações de um baralho

deck-settings-title = Configurações de { $name } - Flash
deck-settings-label = Configurações do baralho
deck-settings-back = Voltar ao baralho
deck-name-heading = Nome
deck-name-label = Nome do baralho
deck-rename = Renomear
deck-saved = Salvo
deck-parent-heading = Baralho superior
deck-parent-hint =
    { $count ->
        [0] O baralho que contém este. Estudar um baralho inclui os sub-baralhos dele.
        [one] O baralho que contém este. Estudar um baralho inclui os sub-baralhos dele, e o sub-baralho deste vai com ele.
        [many] O baralho que contém este. Estudar um baralho inclui os sub-baralhos dele, e os { $count } de sub-baralhos deste vão com ele.
       *[other] O baralho que contém este. Estudar um baralho inclui os sub-baralhos dele, e os { $count } sub-baralhos deste vão com ele.
    }
deck-parent-label = Baralho superior
deck-top-level = (nível principal)
deck-move = Mover
deck-moved = Movido
deck-limits-heading = Limites diários
deck-limit-new = Cartões novos / dia
deck-limit-reviews = Revisões máximas / dia
deck-limit-default = padrão da conta ({ $count })
deck-save-limits = Salvar limites
deck-boosted = +{ $count } hoje
deck-archived-heading = Baralho arquivado
deck-archived-explain =
    { $count ->
        [0] Este baralho está fora da fila de estudo e das contagens de cartões a revisar. Desarquive-o e os cartões voltam com o histórico de revisões que tinham.
        [one] Este baralho está fora da fila de estudo e das contagens de cartões a revisar. Desarquive-o e os cartões voltam com o histórico de revisões que tinham, assim como o sub-baralho dele.
        [many] Este baralho está fora da fila de estudo e das contagens de cartões a revisar. Desarquive-o e os cartões voltam com o histórico de revisões que tinham, assim como os { $count } de sub-baralhos dele.
       *[other] Este baralho está fora da fila de estudo e das contagens de cartões a revisar. Desarquive-o e os cartões voltam com o histórico de revisões que tinham, assim como os { $count } sub-baralhos dele.
    }
deck-archived-parents = Os baralhos dentro dos quais ele fica também são desarquivados.
deck-archived-toast = Arquivado
deck-archive-heading = Arquivar baralho
deck-archive-explain =
    { $count ->
        [0] Mantém todos os cartões, a mídia e o histórico de revisões, e tira o baralho do estudo e das contagens de cartões a revisar. Você pode desarquivá-lo quando quiser.
        [one] Mantém todos os cartões, a mídia e o histórico de revisões, e tira do estudo e das contagens de cartões a revisar o baralho e o sub-baralho dele. Você pode desarquivá-lo quando quiser.
        [many] Mantém todos os cartões, a mídia e o histórico de revisões, e tira do estudo e das contagens de cartões a revisar o baralho e os { $count } de sub-baralhos dele. Você pode desarquivá-lo quando quiser.
       *[other] Mantém todos os cartões, a mídia e o histórico de revisões, e tira do estudo e das contagens de cartões a revisar o baralho e os { $count } sub-baralhos dele. Você pode desarquivá-lo quando quiser.
    }
deck-archive = Arquivar
deck-restored = Restaurado
deck-delete-heading = Excluir baralho
deck-delete-explain =
    { $count ->
        [0] Remove este baralho e todos os cartões dele. O histórico de revisões é mantido. Esta ação é irreversível.
        [one] Remove este baralho, o sub-baralho dele e todos os cartões deles. O histórico de revisões é mantido. Esta ação é irreversível.
        [many] Remove este baralho, os { $count } de sub-baralhos dele e todos os cartões deles. O histórico de revisões é mantido. Esta ação é irreversível.
       *[other] Remove este baralho, os { $count } sub-baralhos dele e todos os cartões deles. O histórico de revisões é mantido. Esta ação é irreversível.
    }
deck-delete-confirm =
    { $subdecks ->
        [0] { $cards ->
            [one] Excluir este baralho e o cartão dele? Esta ação é irreversível.
            [many] Excluir este baralho e todos os { $cards } de cartões? Esta ação é irreversível.
           *[other] Excluir este baralho e todos os { $cards } cartões? Esta ação é irreversível.
        }
        [one] { $cards ->
            [one] Excluir este baralho, o sub-baralho dele e o cartão? Esta ação é irreversível.
            [many] Excluir este baralho, o sub-baralho dele e todos os { $cards } de cartões? Esta ação é irreversível.
           *[other] Excluir este baralho, o sub-baralho dele e todos os { $cards } cartões? Esta ação é irreversível.
        }
        [many] { $cards ->
            [one] Excluir este baralho, os { $subdecks } de sub-baralhos dele e o cartão? Esta ação é irreversível.
            [many] Excluir este baralho, os { $subdecks } de sub-baralhos dele e todos os { $cards } de cartões? Esta ação é irreversível.
           *[other] Excluir este baralho, os { $subdecks } de sub-baralhos dele e todos os { $cards } cartões? Esta ação é irreversível.
        }
       *[other] { $cards ->
            [one] Excluir este baralho, os { $subdecks } sub-baralhos dele e o cartão? Esta ação é irreversível.
            [many] Excluir este baralho, os { $subdecks } sub-baralhos dele e todos os { $cards } de cartões? Esta ação é irreversível.
           *[other] Excluir este baralho, os { $subdecks } sub-baralhos dele e todos os { $cards } cartões? Esta ação é irreversível.
        }
    }
deck-delete = Excluir baralho

## Por que um cartão não foi adicionado (um aviso sobre o formulário)

card-not-added-cloze = O cartão não foi adicionado: uma nota de omissão de palavras precisa de pelo menos uma omissão {"{{"}c1::…{"}}"}. Selecione a resposta e pressione Ctrl+Shift+C.
card-not-added-empty = O cartão não foi adicionado: os dois lados precisam de conteúdo.
card-not-added-long = O cartão não foi adicionado: um dos lados é muito longo.
card-not-added-media = O cartão não foi adicionado: ele usa mídia que não é sua.
card-not-added-invalid = O cartão não foi adicionado: não foi possível salvar o conteúdo.
card-not-added-limit = O cartão não foi adicionado: esta conta atingiu o limite de cartões.

## Por que não foi possível renomear ou mover um baralho (um selo ao lado do título)

deck-error-nest = O nome não pode conter “::”; use Baralho superior para colocar este baralho dentro de outro
deck-error-empty = O nome não pode ficar em branco
deck-error-taken = Você já tem um baralho com esse nome
deck-error-self = Um baralho não pode ficar dentro de si mesmo
deck-error-long = Isso deixaria o nome completo do baralho muito longo
deck-error-archived = Esse baralho está arquivado; desarquive-o primeiro
deck-error-cap = Esta conta já tem o máximo de baralhos permitido
deck-error-invalid = Não funcionou

## Recusas ao nomear ou criar um baralho (uma frase, mostrada também pelo app e pelos assistentes)

deck-error-nest-sentence = O nome não pode conter “::”. Use Baralho superior para colocar este baralho dentro de outro.
deck-error-name-new = Dê um nome ao novo baralho.
deck-error-archived-sentence = Seu baralho “{ $name }” está arquivado. Desarquive-o ou escolha outro nome.

## A data da meta de um baralho: o dia em que a pessoa quer saber o
## baralho (uma prova, uma viagem, um prazo). O Flash distribui os cartões
## novos para terminar antes dela e traz de volta, nos últimos dias, os
## cartões que precisam.

goal-error-past = Escolha uma data depois de hoje.
goal-error-far = Escolha uma data nos próximos cinco anos.

goal-set = Definir data da meta…
goal-change = Alterar data da meta…
goal-remove = Remover data da meta

goal-left-days = { $count } d
goal-left-today = Hoje
goal-left-done = Concluída
goal-left-month-year = { $month } { $year }

goal-tip-on-track = Em dia para { $date }
goal-tip-behind = Em atraso para { $date }
goal-tip-learned =
    { $total ->
        [one] { $learned } de { $total } cartão aprendido.
        [many] { $learned } de { $total } de cartões aprendidos.
       *[other] { $learned } de { $total } cartões aprendidos.
    }
goal-tip-pace =
    { $count ->
        [one] { $count } cartão novo por dia até { $date }, depois uma última revisão.
        [many] { $count } de cartões novos por dia até { $date }, depois uma última revisão.
       *[other] { $count } cartões novos por dia até { $date }, depois uma última revisão.
    }
goal-tip-final = As últimas revisões antes de { $date }.
goal-tip-catch-up =
    { $count ->
        [one] Mais { $count } cartão novo hoje e você volta a ficar em dia.
        [many] Mais { $count } de cartões novos hoje e você volta a ficar em dia.
       *[other] Mais { $count } cartões novos hoje e você volta a ficar em dia.
    }
goal-tip-today = Hoje é o dia da sua meta. Boa sorte!
goal-tip-passed = A data da sua meta foi { $date }.
goal-passed = A data da sua meta já passou.
goal-archive = Arquivar baralho
goal-keep = Continuar estudando

goal-heading-set = Definir data da meta
goal-heading-change = Alterar data da meta
goal-card-heading = Data da meta
goal-date-label = Até quando você precisa dominar este baralho?
goal-picks = Opções rápidas
goal-pick-weeks =
    { $count ->
        [one] { $count } semana
        [many] { $count } de semanas
       *[other] { $count } semanas
    }
goal-pick-months =
    { $count ->
        [one] { $count } mês
        [many] { $count } de meses
       *[other] { $count } meses
    }
goal-pick-year = 1 ano
goal-plan-pace-html =
    { $count ->
        [one] <b>{ $count } cartão novo por dia</b> até { $date } (ainda não vistos: { $unseen })
        [many] <b>{ $count } de cartões novos por dia</b> até { $date } (ainda não vistos: { $unseen })
       *[other] <b>{ $count } cartões novos por dia</b> até { $date } (ainda não vistos: { $unseen })
    }
goal-plan-seen-html = <b>Todos os cartões já foram vistos.</b> As revisões mantêm tudo na memória até lá.
goal-plan-final-html = <b>De { $from } a { $to }:</b> uma última revisão dos cartões que precisam dela
goal-plan-final-close-html = <b>Até { $to }:</b> uma última revisão de todos os cartões que precisam dela
goal-plan-after-html = <b>Depois de { $date },</b> as revisões voltam ao normal. Você poderá arquivar o baralho a partir daí.
goal-plan-pick = Escolha uma data para ver o plano.
goal-tight = São cerca de { $count } cartões novos por dia. Uma data mais distante deixa os dias mais leves.
goal-save = Salvar meta
goal-cancel = Cancelar
