### The open core's words in Portuguese, both variants: cards.

## A lista de cartões de um baralho

cards-none = Nenhum cartão.
cards-none-filtered = Nenhum cartão encontrado.
cards-chip-archived = Arquivados
cards-own = Só este baralho
cards-chip-remove = Remover { $name } do filtro
cards-match-label = Correspondência de etiquetas
cards-match-any = Qualquer
cards-match-all = Todas
cards-chip-clear = Limpar
cards-select-page = Selecionar todos os cartões desta página
cards-select-card = Selecionar este cartão
cards-from-title = Em { $name }. Mostrar só os cartões dele.
cards-col-front = Frente
cards-col-back = Verso
cards-col-tags = Etiquetas
cards-pages = Páginas de cartões
cards-previous = Anterior
cards-next = Próxima
cards-page-of =
    Página { $page } de { $pages } · { $count ->
        [one] { $count } cartão
        [many] { $count } de cartões
       *[other] { $count } cartões
    }
cards-edit-label = Editar cartão
cards-edit = Editar
cards-unarchive-label = Desarquivar cartão
cards-unarchive = Desarquivar
cards-archive-label = Arquivar cartão
cards-archive = Arquivar
cards-delete-label = Excluir cartão
cards-delete = Excluir
cards-bulk-archived =
    { $count ->
        [one] { $count } cartão arquivado
        [many] { $count } de cartões arquivados
       *[other] { $count } cartões arquivados
    }
cards-bulk-unarchived =
    { $count ->
        [one] { $count } cartão de volta ao estudo
        [many] { $count } de cartões de volta ao estudo
       *[other] { $count } cartões de volta ao estudo
    }
cards-bulk-deleted =
    { $count ->
        [one] { $count } cartão excluído
        [many] { $count } de cartões excluídos
       *[other] { $count } cartões excluídos
    }
cards-bulk-moved =
    { $count ->
        [one] { $count } cartão movido para { $deck }
        [many] { $count } de cartões movidos para { $deck }
       *[other] { $count } cartões movidos para { $deck }
    }
cards-bulk-tagged =
    { $count ->
        [one] Etiquetas adicionadas a { $count } cartão
        [many] Etiquetas adicionadas a { $count } de cartões
       *[other] Etiquetas adicionadas a { $count } cartões
    }
cards-bulk-untagged =
    { $count ->
        [one] Etiquetas removidas de { $count } cartão
        [many] Etiquetas removidas de { $count } de cartões
       *[other] Etiquetas removidas de { $count } cartões
    }
cards-bulk-undone = Desfeito
cards-bulk-undo = Desfazer
cards-error-bulk-changed = A lista mudou durante a seleção. Selecione os cartões novamente.
cards-error-bulk-many = É possível alterar no máximo { $max } cartões de uma vez.
cards-error-move-archived = Esse baralho está arquivado. Desarquive-o primeiro.
cards-error-tags-none = Escreva pelo menos uma etiqueta.
cards-kind-cloze = omissão { $n }
cards-kind-reversed = invertido
cards-kind-typed = digitado

## A página de um cartão e a caixa de edição

editor-page-title = Editar cartão - Flash
editor-crumbs = Baralho
editor-heading = Editar cartão
editor-tags = Etiquetas
editor-tags-hint = Separadas por vírgula, aplicadas a todos os cartões desta nota.
editor-history =
    { $count ->
        [one] Histórico ({ $count } versão anterior)
        [many] Histórico ({ $count } de versões anteriores)
       *[other] Histórico ({ $count } versões anteriores)
    }
editor-replaced = { $ago }, antes de { $by }
editor-by-web = uma edição aqui
editor-by-mcp = uma edição de um assistente
editor-by-app = uma edição no app
editor-by-sync = uma edição feita em outro lugar
editor-by-restore = uma restauração
editor-restore = Restaurar
editor-restored = Versão anterior restaurada.
editor-saved = Salvo.
editor-cancel = Cancelar
editor-save = Salvar

## O editor em si

editor-type = Tipo
note-type-basic = Básico
note-type-reversed = Básico (e cartão invertido)
note-type-typed = Básico (digite a resposta)
# accept: length: this is Anki's own note type name, which Anki's shipped Portuguese (notetypes-cloze-name) writes "Omissão de palavras".
note-type-cloze = Omissão de palavras
editor-siblings = { $count } cartões compartilham esta nota
editor-siblings-hint = Salvar gera de novo todos os cartões desta nota
editor-toolbar = Formatação
editor-bold-tip = Negrito (Ctrl+B)
editor-bold = Negrito
editor-italic-tip = Itálico (Ctrl+I)
editor-italic = Itálico
editor-underline-tip = Sublinhado (Ctrl+U)
editor-underline = Sublinhado
editor-strike = Tachado
editor-superscript = Sobrescrito
editor-subscript = Subscrito
editor-text-color = Cor do texto
editor-text-hue = Texto { $hue }
editor-no-color = Sem cor
editor-highlight = Destaque
editor-highlight-hue = Destaque { $hue }
editor-no-highlight = Sem destaque
editor-bulleted = Lista com marcadores
editor-numbered = Lista numerada
editor-clear = Limpar formatação
editor-attach-tip = Anexar imagem, áudio ou vídeo
editor-attach = Anexar mídia
editor-cloze-tip = Omissão de palavras (Ctrl+Shift+C; Ctrl+Shift+Alt+C reutiliza o último número)
editor-cloze = Omissão de palavras
editor-source-tip = Código HTML
editor-source = Mostrar ou ocultar o código HTML
editor-front = Frente
editor-back = Verso
editor-front-placeholder = Pergunta
editor-back-placeholder = Resposta
editor-front-source = Código HTML da frente
editor-back-source = Código HTML do verso
editor-typed-hint = Ao estudar, você vai digitar esta resposta e ver as diferenças.
editor-drop-hint = Solte ou cole imagens e áudio direto em um campo.
editor-cloze-hint-html = Selecione uma resposta e pressione <kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>C</kbd> para ocultá-la.
editor-reversed-hint = Dois cartões: da frente para o verso e do verso para a frente.
editor-save-hint-html = <kbd>Ctrl</kbd>+<kbd>Enter</kbd> salva.

## Cores, para os nomes das amostras (minúsculas, como dentro de uma frase)

hue-red = vermelho
hue-amber = âmbar
hue-green = verde
hue-teal = turquesa
hue-blue = azul
hue-purple = roxo
hue-pink = rosa

## Por que um cartão ou uma nota foi recusado (o editor, o app, um assistente)

card-error-front-empty = A frente do cartão está vazia
card-error-back-empty = O verso do cartão está vazio
card-error-side-long = Um lado do cartão passa de { $max } bytes
card-error-field-long = Um campo passa de { $max } bytes de HTML
card-error-no-cloze = Adicione pelo menos uma omissão {"{{"}c1::…{"}}"}
card-error-cloze-empty = A omissão c{ $n } está com a resposta vazia
card-error-cloze-card = Omissão c{ $n }: { $error }
card-error-media = A mídia { $id } não está entre os seus envios

## Uma imagem ou um som recusado no envio

media-error-format = { $filename }: formato não suportado
media-error-empty = { $filename }: arquivo vazio
media-error-too-large = { $filename }: passa do limite de { $mb } MB por arquivo
media-error-mismatch = { $filename }: o conteúdo não corresponde à extensão
media-error-count = Limite de arquivos de mídia atingido. Remova alguns arquivos primeiro
media-error-storage = Limite de armazenamento de mídia atingido. Fale com o suporte
