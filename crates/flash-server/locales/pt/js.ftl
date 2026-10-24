### The open core's words in Portuguese, both variants: js.

## Comportamentos comuns (ui.js)

js-ui-play-audio = Reproduzir áudio
js-ui-pause-audio = Pausar áudio
js-ui-copied = Copiado
js-ui-copy-failed = Não foi possível copiar
js-ui-show-subdecks = Mostrar sub-baralhos
js-ui-hide-subdecks = Ocultar sub-baralhos
js-ui-show-archived = Mostrar arquivados
js-ui-hide-archived = Ocultar arquivados

## Login com chave de acesso (webauthn.js)

js-passkey-cancelled = Cancelado ou tempo esgotado.
js-passkey-failed = Não foi possível concluir. Tente novamente.
js-passkey-bad-list = A lista de credenciais do servidor não é uma lista.
js-passkey-bad-entry = A lista de credenciais do servidor contém outra coisa.
js-passkey-not-passkey = O servidor indicou uma credencial que não é uma chave de acesso.
js-passkey-none-made = O navegador não criou uma chave de acesso.
js-passkey-none-used = O navegador não usou uma chave de acesso.
js-passkey-not-used = Nenhuma chave de acesso foi usada. Faça login de outra forma e adicione-a novamente em Configurações.
js-passkey-link-incomplete = Este link de inscrição está incompleto.

## A página de um baralho (deck.js)

js-deck-selected =
    { $count ->
        [one] { $count } selecionado
        [many] { $count } selecionados
       *[other] { $count } selecionados
    }
js-deck-select-all =
    { $count ->
        [one] Selecionar { $count }
        [many] Selecionar todos os { $count }
       *[other] Selecionar todos os { $count }
    }
js-deck-delete-n =
    { $count ->
        [one] Excluir { $count } cartão
        [many] Excluir { $count } de cartões
       *[other] Excluir { $count } cartões
    }
js-deck-row-confirm = Clique de novo para excluir
js-deck-add = Adicionar
js-deck-close = Fechar
js-deck-failed = Não foi possível concluir. Tente novamente.

## O editor de cartões (editor.js)

js-editor-front = Frente
js-editor-back = Verso
js-editor-question = Pergunta
js-editor-answer = Resposta
js-editor-answer-to-type = Resposta a digitar
js-editor-text = Texto
js-editor-extra = Adicional
js-editor-cloze-hint = Frase com {"{{"}c1::omissões{"}}"}
js-editor-extra-hint = Mostrado com a resposta (opcional)
js-editor-uploading = Enviando { $name }…
js-editor-upload-failed = Não foi possível enviar.
js-editor-select-first = Selecione algum texto primeiro.
js-editor-wait-upload = Espere o envio terminar.
js-editor-simple = Simples
js-editor-advanced = Avançado
