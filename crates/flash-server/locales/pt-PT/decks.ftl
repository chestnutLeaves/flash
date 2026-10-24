### Portuguese as Portugal words it, where it differs from the shared base (pt): decks.

## Tabelas de baralhos (Hoje, a página de baralhos, os sub-baralhos de um baralho)

decks-col-due = A rever
decks-col-goal = Objetivo
decks-more-settings = Definições do baralho

## Hoje (a página inicial)

today-due = a rever agora
today-reviewed = revistos hoje
today-caught-up = Tudo em dia. Nada a rever agora.
today-connect = Ligue a sua IA
today-connect-sub = Crie cartões e estude com o Claude, ChatGPT, Grok, Mistral ou Gemini. Configuração num minuto.
today-connect-dismiss = Ignorar

## A página de um baralho

deck-tags-placeholder = teste-2, cardiologia

## A página de um baralho: o cabeçalho, os sub-baralhos, a pesquisa e o
## filtro, Adicionar e a barra de ações nos cartões selecionados

deck-meta-due =
    { $count ->
        [one] a rever
        [many] a rever
       *[other] a rever
    }
deck-export-csv = Exportar como folha de cálculo (.csv)
deck-bulk-delete = Eliminar
deck-bulk-delete-warn = Quer eliminar estes cartões de vez? Esta ação é irreversível.

## As configurações de um baralho

deck-settings-title = Definições de { $name } - Flash
deck-settings-label = Definições do baralho
deck-rename = Mudar o nome
deck-saved = Guardado
deck-save-limits = Guardar limites
deck-archived-explain =
    { $count ->
        [0] Este baralho está fora da fila de estudo e das contagens de cartões a rever. Desarquive-o e os cartões voltam com o histórico de revisões que tinham.
        [one] Este baralho está fora da fila de estudo e das contagens de cartões a rever. Desarquive-o e os cartões voltam com o histórico de revisões que tinham, assim como o respetivo sub-baralho.
        [many] Este baralho está fora da fila de estudo e das contagens de cartões a rever. Desarquive-o e os cartões voltam com o histórico de revisões que tinham, assim como os { $count } de sub-baralhos.
       *[other] Este baralho está fora da fila de estudo e das contagens de cartões a rever. Desarquive-o e os cartões voltam com o histórico de revisões que tinham, assim como os { $count } sub-baralhos.
    }
deck-archive-explain =
    { $count ->
        [0] Mantém todos os cartões, os ficheiros multimédia e o histórico de revisões, e tira o baralho do estudo e das contagens de cartões a rever. Pode desarquivá-lo quando quiser.
        [one] Mantém todos os cartões, os ficheiros multimédia e o histórico de revisões, e tira do estudo e das contagens de cartões a rever o baralho e o respetivo sub-baralho. Pode desarquivá-lo quando quiser.
        [many] Mantém todos os cartões, os ficheiros multimédia e o histórico de revisões, e tira do estudo e das contagens de cartões a rever o baralho e os { $count } de sub-baralhos. Pode desarquivá-lo quando quiser.
       *[other] Mantém todos os cartões, os ficheiros multimédia e o histórico de revisões, e tira do estudo e das contagens de cartões a rever o baralho e os { $count } sub-baralhos. Pode desarquivá-lo quando quiser.
    }
deck-delete-heading = Eliminar baralho
deck-delete-confirm =
    { $subdecks ->
        [0] { $cards ->
            [one] Quer eliminar este baralho e o respetivo cartão? Esta ação é irreversível.
            [many] Quer eliminar este baralho e todos os { $cards } de cartões? Esta ação é irreversível.
           *[other] Quer eliminar este baralho e todos os { $cards } cartões? Esta ação é irreversível.
        }
        [one] { $cards ->
            [one] Quer eliminar este baralho, o respetivo sub-baralho e o cartão? Esta ação é irreversível.
            [many] Quer eliminar este baralho, o respetivo sub-baralho e todos os { $cards } de cartões? Esta ação é irreversível.
           *[other] Quer eliminar este baralho, o respetivo sub-baralho e todos os { $cards } cartões? Esta ação é irreversível.
        }
        [many] { $cards ->
            [one] Quer eliminar este baralho, os { $subdecks } de sub-baralhos e o cartão? Esta ação é irreversível.
            [many] Quer eliminar este baralho, os { $subdecks } de sub-baralhos e todos os { $cards } de cartões? Esta ação é irreversível.
           *[other] Quer eliminar este baralho, os { $subdecks } de sub-baralhos e todos os { $cards } cartões? Esta ação é irreversível.
        }
       *[other] { $cards ->
            [one] Quer eliminar este baralho, os { $subdecks } sub-baralhos e o cartão? Esta ação é irreversível.
            [many] Quer eliminar este baralho, os { $subdecks } sub-baralhos e todos os { $cards } de cartões? Esta ação é irreversível.
           *[other] Quer eliminar este baralho, os { $subdecks } sub-baralhos e todos os { $cards } cartões? Esta ação é irreversível.
        }
    }
deck-delete = Eliminar baralho

## Por que um cartão não foi adicionado (um aviso sobre o formulário)

card-not-added-cloze = O cartão não foi adicionado: uma nota de omissão de palavras precisa de pelo menos uma omissão {"{{"}c1::…{"}}"}. Selecione a resposta e prima Ctrl+Shift+C.
card-not-added-media = O cartão não foi adicionado: usa ficheiros multimédia que não são seus.
card-not-added-invalid = O cartão não foi adicionado: não foi possível guardar o conteúdo.

## Por que não foi possível renomear ou mover um baralho (um selo ao lado do título)

deck-error-taken = Já tem um baralho com esse nome

## Recusas ao nomear ou criar um baralho (uma frase, mostrada também pelo app e pelos assistentes)

deck-error-archived-sentence = O seu baralho “{ $name }” está arquivado. Desarquive-o ou escolha outro nome.

## A data da meta de um baralho: o dia em que a pessoa quer saber o
## baralho (uma prova, uma viagem, um prazo). O Flash distribui os cartões
## novos para terminar antes dela e traz de volta, nos últimos dias, os
## cartões que precisam.

goal-set = Definir data do objetivo…
goal-change = Alterar data do objetivo…
goal-remove = Remover data do objetivo

goal-left-done = Concluído

goal-tip-catch-up =
    { $count ->
        [one] Mais { $count } cartão novo hoje e volta a ficar em dia.
        [many] Mais { $count } de cartões novos hoje e volta a ficar em dia.
       *[other] Mais { $count } cartões novos hoje e volta a ficar em dia.
    }
goal-tip-today = Hoje é o dia do seu objetivo. Boa sorte!
goal-tip-passed = A data do seu objetivo foi { $date }.
goal-passed = A data do seu objetivo já passou.
goal-keep = Continuar a estudar

goal-heading-set = Definir data do objetivo
goal-heading-change = Alterar data do objetivo
goal-card-heading = Data do objetivo
goal-date-label = Até quando precisa de dominar este baralho?
goal-plan-after-html = <b>Depois de { $date },</b> as revisões voltam ao normal. Poderá arquivar o baralho a partir daí.
goal-save = Guardar objetivo
