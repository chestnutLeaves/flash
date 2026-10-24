### Portuguese as Portugal words it, where it differs from the shared base (pt): import.

## A página de importar

import-intro-html =
    CSV com cabeçalho <code>front,back,tags,deck</code> ou uma exportação <code>.apkg</code> do Anki.
    Os cartões entram como novos, a menos que traga também o seu progresso de estudo do Anki.
import-file = Ficheiro
import-choose = Escolher ficheiro
import-no-file-chosen = Nenhum ficheiro escolhido, ou largue um aqui
import-reading = A enviar e a ler o baralho…
import-decks-in-file = Baralhos neste ficheiro
import-keep-hint-html = Cada baralho é mantido, e <code>Parent::Child</code> passa a ser um sub-baralho. Se já tiver um baralho com esse nome, é esse que recebe os cartões.
import-bring-progress =
    { $count ->
        [one]
            Trazer o meu progresso de estudo ({ $count } revisão encontrada):
            os cartões mantêm o agendamento em vez de começar como novos, e cada revisão
            entra nas suas estatísticas, incluindo as sequências.
        [many]
            Trazer o meu progresso de estudo ({ $count } de revisões encontradas):
            os cartões mantêm o agendamento em vez de começar como novos, e cada revisão
            entra nas suas estatísticas, incluindo as sequências.
       *[other]
            Trazer o meu progresso de estudo ({ $count } revisões encontradas):
            os cartões mantêm o agendamento em vez de começar como novos, e cada revisão
            entra nas suas estatísticas, incluindo as sequências.
    }
import-adopt-settings = Adotar as definições FSRS do Anki deste baralho ({ $settings }).
import-adopt-hint = Substitui as suas definições de agendamento atuais.
import-importing = A importar cartões e ficheiros multimédia…

## O que a pré-visualização observa sobre o arquivo

import-merge = O ficheiro tem { $count } baralhos ({ $decks }); todos os cartões deles vão para o baralho indicado acima.
import-media-unsupported =
    { $count ->
        [one] { $count } ficheiro não suportado
        [many] { $count } de ficheiros não suportados
       *[other] { $count } ficheiros não suportados
    }
import-history-capped-phone = no telemóvel, o histórico de revisões fica limitado às { $max } revisões mais recentes; as mais antigas continuam no Anki

## Por que não foi possível importar um arquivo

import-too-many-cards = Esse ficheiro tem mais de { $max } cartões; cada importação aceita até { $max } por ficheiro. Divida-o no Anki primeiro
import-file-too-many-cards = Esse ficheiro tem { $count } cartões; cada importação aceita até { $max } por ficheiro, por isso divida-o no Anki primeiro
import-not-a-zip = Esse ficheiro não é um pacote do Anki (não é um ficheiro zip)
import-media-list-damaged = A lista de ficheiros multimédia do pacote está danificada e não pôde ser lida
import-compressed-damaged = O conteúdo comprimido está danificado e não pôde ser descomprimido
import-too-many-files = Esse pacote tem mais de { $max } ficheiros, o máximo por importação
import-media-list-large = A lista de ficheiros multimédia do pacote é muito grande
import-decompress-limit = passa do limite de { $max } bytes ao descomprimir
import-collection-large = A coleção dentro do ficheiro passa de { $mb } MB
import-no-collection = Nenhuma coleção do Anki encontrada no ficheiro
import-not-yours = A pré-visualização pertence a outro utilizador
import-cut-off = Esta importação foi interrompida a meio, e alguns cartões já estão nos seus baralhos; verifique-os antes de importar o ficheiro de novo
import-busy = Outra importação está em curso; tente novamente daqui a pouco
import-no-file = Nenhum ficheiro
import-name-too-long = O nome do ficheiro é muito longo
