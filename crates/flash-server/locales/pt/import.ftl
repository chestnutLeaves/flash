### The open core's words in Portuguese, both variants: import.

## A página de importar

import-title = Importar - Flash
import-heading = Importar
import-intro-html =
    CSV com cabeçalho <code>front,back,tags,deck</code> ou uma exportação <code>.apkg</code> do Anki.
    Os cartões entram como novos, a menos que você traga junto seu progresso de estudo do Anki.
import-file = Arquivo
import-choose = Escolher arquivo
import-no-file-chosen = Nenhum arquivo escolhido, ou solte um aqui
import-into = No baralho
import-deck-placeholder = Por exemplo: Farmacologia
import-reading = Enviando e lendo o baralho…
import-preview-button = Pré-visualizar
import-preview-heading = Pré-visualização
import-ready-html =
    { $count ->
        [one] <strong>{ $count } cartão</strong> pronto para importar de <span class="mono">{ $name }</span> <span class="muted">({ $size })</span>.
        [many] <strong>{ $count } de cartões</strong> prontos para importar de <span class="mono">{ $name }</span> <span class="muted">({ $size })</span>.
       *[other] <strong>{ $count } cartões</strong> prontos para importar de <span class="mono">{ $name }</span> <span class="muted">({ $size })</span>.
    }
import-skipped =
    { $count ->
        [one] { $count } linha ignorada
        [many] { $count } de linhas ignoradas
       *[other] { $count } linhas ignoradas
    }
import-decks-in-file = Baralhos neste arquivo
import-deck-cards =
    { $count ->
        [one] { $count } cartão
        [many] { $count } de cartões
       *[other] { $count } cartões
    }
import-more = +{ $count } outros
import-keep-hint-html = Cada baralho é mantido, e <code>Parent::Child</code> passa a ser um sub-baralho. Se você já tiver um baralho com esse nome, ele recebe os cartões.
import-put-under = Colocar dentro de (opcional)
import-top-level = Nível principal
import-put-under-hint = Indique um baralho que vai conter todos como sub-baralhos, ou deixe em branco.
import-name-deck = Dê um nome a este baralho
import-advanced = Avançado
import-keep-colors =
    { $count ->
        [one] Manter as cores do baralho, ajustadas à paleta do Flash ({ $count } cartão colorido).
        [many] Manter as cores do baralho, ajustadas à paleta do Flash ({ $count } de cartões coloridos).
       *[other] Manter as cores do baralho, ajustadas à paleta do Flash ({ $count } cartões coloridos).
    }
import-clean-look = Visual limpo, importar sem cores.
import-bring-progress =
    { $count ->
        [one]
            Trazer meu progresso de estudo ({ $count } revisão encontrada):
            os cartões mantêm o agendamento em vez de começar como novos, e cada revisão
            entra nas suas estatísticas, incluindo as sequências.
        [many]
            Trazer meu progresso de estudo ({ $count } de revisões encontradas):
            os cartões mantêm o agendamento em vez de começar como novos, e cada revisão
            entra nas suas estatísticas, incluindo as sequências.
       *[other]
            Trazer meu progresso de estudo ({ $count } revisões encontradas):
            os cartões mantêm o agendamento em vez de começar como novos, e cada revisão
            entra nas suas estatísticas, incluindo as sequências.
    }
import-adopt-settings = Adotar as configurações FSRS do Anki deste baralho ({ $settings }).
import-adopt-hint = Substitui suas configurações de agendamento atuais.
import-importing = Importando cartões e mídia…
import-submit =
    { $count ->
        [one] Importar { $count } cartão
        [many] Importar { $count } de cartões
       *[other] Importar { $count } cartões
    }
import-imported = importado
import-added =
    { $count ->
        [one] { $count } cartão adicionado.
        [many] { $count } de cartões adicionados.
       *[other] { $count } cartões adicionados.
    }

## O tamanho de um arquivo: $n é o número, já escrito como o leitor escreve

import-size-mb = { $n } MB
import-size-kb = { $n } KB
import-size-b = { $n } B

## As configurações do Anki do pacote, em resumo (separadas por vírgula)

import-settings-retention = { $pct }% de retenção
import-settings-params = parâmetros FSRS ajustados ({ $count } valores)
import-settings-new =
    { $count ->
        [one] { $count } cartão novo/dia
        [many] { $count } de cartões novos/dia
       *[other] { $count } cartões novos/dia
    }

## O que a pré-visualização observa sobre o arquivo

import-merge = O arquivo tem { $count } baralhos ({ $decks }); todos os cartões deles vão para o baralho indicado acima.
import-media = este baralho usa { $what } ({ $mb } MB)
import-media-images =
    { $count ->
        [one] { $count } imagem
        [many] { $count } de imagens
       *[other] { $count } imagens
    }
import-media-audio =
    { $count ->
        [one] { $count } áudio
        [many] { $count } de áudios
       *[other] { $count } áudios
    }
import-media-video =
    { $count ->
        [one] { $count } vídeo
        [many] { $count } de vídeos
       *[other] { $count } vídeos
    }
import-media-unsupported =
    { $count ->
        [one] { $count } arquivo não suportado
        [many] { $count } de arquivos não suportados
       *[other] { $count } arquivos não suportados
    }
import-history-capped = o histórico de revisões fica limitado às { $max } revisões mais recentes; as mais antigas continuam no Anki
import-history-capped-phone = no celular, o histórico de revisões fica limitado às { $max } revisões mais recentes; as mais antigas continuam no Anki
import-row-invalid-utf8 = linha { $row }: UTF-8 inválido no campo { $field }
import-notes-skipped =
    { $count ->
        [one] { $count } nota ignorada (vazia ou com tipo não suportado)
        [many] { $count } de notas ignoradas (vazias ou com tipo não suportado)
       *[other] { $count } notas ignoradas (vazias ou com tipo não suportado)
    }
# accept: length: "cloze note" is Anki's note type, whose shipped Portuguese name "Omissão de palavras" the reader knows from Anki.
import-cloze-expanded =
    { $notes ->
        [one] { $notes } nota de omissão de palavras gerou
        [many] { $notes } de notas de omissão de palavras geraram
       *[other] { $notes } notas de omissão de palavras geraram
    } { $cards ->
        [one] { $cards } cartão
        [many] { $cards } de cartões
       *[other] { $cards } cartões
    }
import-extra-layouts =
    { $count ->
        [one] { $count } tipo de cartão adicional incluído (invertido etc.)
        [many] { $count } de tipos de cartão adicionais incluídos (invertido etc.)
       *[other] { $count } tipos de cartão adicionais incluídos (invertido etc.)
    }
import-suspended =
    { $count ->
        [one] { $count } cartão chega suspenso, como no Anki
        [many] { $count } de cartões chegam suspensos, como no Anki
       *[other] { $count } cartões chegam suspensos, como no Anki
    }
import-layouts-skipped =
    { $count ->
        [one] { $count } cartão extra ignorado (tipo de nota ilegível)
        [many] { $count } de cartões extras ignorados (tipo de nota ilegível)
       *[other] { $count } cartões extras ignorados (tipo de nota ilegível)
    }
import-row-unreadable = linha { $row }: { $error }
import-row-empty = linha { $row }: frente ou verso vazio
import-about-file = { $file }: { $what }

## Por que não foi possível importar um arquivo

import-too-many-cards = Esse arquivo tem mais de { $max } cartões; cada importação aceita até { $max } por arquivo. Divida-o no Anki primeiro
import-file-too-many-cards = Esse arquivo tem { $count } cartões; cada importação aceita até { $max } por arquivo, por isso divida-o no Anki primeiro
import-nothing =
    { $count ->
        [one] Nenhum cartão importável encontrado ({ $count } linha ignorada)
        [many] Nenhum cartão importável encontrado ({ $count } de linhas ignoradas)
       *[other] Nenhum cartão importável encontrado ({ $count } linhas ignoradas)
    }
import-csv-header-unreadable = Não foi possível ler a linha de cabeçalho do CSV
import-csv-columns = O CSV precisa das colunas de cabeçalho 'front' e 'back'
import-not-a-zip = Esse arquivo não é um pacote do Anki (não é um arquivo zip)
import-media-list-damaged = A lista de mídia do pacote está danificada e não pôde ser lida
import-compressed-damaged = O conteúdo compactado está danificado e não pôde ser descompactado
import-too-many-files = Esse pacote tem mais de { $max } arquivos, o máximo por importação
import-media-list-large = A lista de mídia do pacote é muito grande
import-media-missing = { $file } não está no pacote
import-media-unreadable = Não foi possível ler { $file } do pacote
import-media-too-large = maior que o limite de { $max } bytes
import-media-hash = o conteúdo não corresponde ao hash declarado
import-decompress-limit = passa do limite de { $max } bytes ao descompactar
import-collection-damaged = A coleção dentro do pacote está danificada e não pôde ser lida
import-collection-layout = A coleção dentro do pacote tem uma estrutura inesperada
import-staging-failed = Não foi possível preparar o pacote para leitura; tente novamente
import-collection-large = A coleção dentro do arquivo passa de { $mb } MB
import-no-collection = Nenhuma coleção do Anki encontrada no arquivo
import-expired = A pré-visualização expirou; envie de novo
import-not-yours = A pré-visualização pertence a outro usuário
import-needs-deck = Alguns cartões não indicam um baralho: indique um baralho para colocar a importação dentro dele
import-deck-required = Indique o nome do baralho
import-cut-off = Esta importação foi interrompida no meio, e alguns cartões já estão nos seus baralhos; confira-os antes de importar o arquivo de novo
import-busy-uploads = Muitos envios ao mesmo tempo; tente novamente daqui a pouco
import-busy = Outra importação está em andamento; tente novamente daqui a pouco
import-deck-too-long = O nome do baralho é muito longo
import-no-file = Nenhum arquivo
import-name-too-long = O nome do arquivo é muito longo
