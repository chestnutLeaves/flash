### Portuguese as Portugal words it, where it differs from the shared base (pt): cards.

## A lista de cartões de um baralho

cards-next = Seguinte
cards-delete-label = Eliminar cartão
cards-delete = Eliminar
cards-bulk-deleted =
    { $count ->
        [one] { $count } cartão eliminado
        [many] { $count } de cartões eliminados
       *[other] { $count } cartões eliminados
    }
cards-bulk-undone = Anulado
cards-bulk-undo = Anular

## A página de um cartão e a caixa de edição

editor-by-app = uma edição na app
editor-by-sync = uma edição feita noutro lugar
editor-by-restore = um restauro
editor-saved = Guardado.
editor-save = Guardar

## O editor em si

editor-siblings = { $count } cartões partilham esta nota
editor-siblings-hint = Guardar gera novamente todos os cartões desta nota
editor-strike = Rasurado
editor-highlight = Realce
editor-highlight-hue = Realce { $hue }
editor-no-highlight = Sem realce
editor-attach = Anexar multimédia
editor-typed-hint = Ao estudar, vai escrever esta resposta e ver as diferenças.
editor-drop-hint = Largue ou cole imagens e áudio diretamente num campo.
editor-cloze-hint-html = Selecione uma resposta e prima <kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>C</kbd> para a ocultar.
editor-save-hint-html = <kbd>Ctrl</kbd>+<kbd>Enter</kbd> guarda.

## Por que um cartão ou uma nota foi recusado (o editor, o app, um assistente)

card-error-media = O ficheiro multimédia { $id } não está entre os seus envios

## Uma imagem ou um som recusado no envio

media-error-empty = { $filename }: ficheiro vazio
media-error-too-large = { $filename }: passa do limite de { $mb } MB por ficheiro
media-error-count = Limite de ficheiros multimédia atingido. Remova alguns ficheiros primeiro
media-error-storage = Limite de armazenamento multimédia atingido. Contacte o suporte
