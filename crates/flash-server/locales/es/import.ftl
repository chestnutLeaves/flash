### The open core's words in Spanish, both variants: import.

## La página de importación

import-title = Importar - Flash
import-heading = Importar
import-intro-html =
    CSV con encabezado <code>front,back,tags,deck</code>, o una exportación <code>.apkg</code> de Anki.
    Las tarjetas se importan como nuevas, salvo que traigas con ellas tu progreso de estudio de Anki.
import-file = Archivo
import-choose = Elegir archivo
import-no-file-chosen = Ningún archivo elegido, o suelta uno aquí
import-into = En el mazo
import-deck-placeholder = p. ej., Farmacología
import-reading = Subiendo y leyendo el mazo…
import-preview-button = Vista previa
import-preview-heading = Vista previa
import-ready-html =
    { $count ->
        [one] <strong>{ $count } tarjeta</strong> lista para importar desde <span class="mono">{ $name }</span> <span class="muted">({ $size })</span>.
        [many] <strong>{ $count } de tarjetas</strong> listas para importar desde <span class="mono">{ $name }</span> <span class="muted">({ $size })</span>.
       *[other] <strong>{ $count } tarjetas</strong> listas para importar desde <span class="mono">{ $name }</span> <span class="muted">({ $size })</span>.
    }
import-skipped =
    { $count ->
        [one] { $count } fila omitida
        [many] { $count } de filas omitidas
       *[other] { $count } filas omitidas
    }
import-decks-in-file = Mazos en este archivo
import-deck-cards =
    { $count ->
        [one] { $count } tarjeta
        [many] { $count } de tarjetas
       *[other] { $count } tarjetas
    }
import-more = +{ $count } más
import-keep-hint-html = Cada mazo se conserva, y <code>Parent::Child</code> se convierte en un submazo. Si ya tienes un mazo con ese nombre, las tarjetas van a ese mazo.
import-put-under = Dentro de (opcional)
import-top-level = Nivel superior
import-put-under-hint = Escribe el nombre de un mazo para guardarlos todos como submazos, o déjalo en blanco.
import-name-deck = Ponle nombre a este mazo
import-advanced = Avanzado
import-keep-colors =
    { $count ->
        [one] Conservar los colores de los mazos, adaptados a la paleta de Flash ({ $count } tarjeta con color).
        [many] Conservar los colores de los mazos, adaptados a la paleta de Flash ({ $count } de tarjetas con color).
       *[other] Conservar los colores de los mazos, adaptados a la paleta de Flash ({ $count } tarjetas con color).
    }
import-clean-look = Aspecto limpio: importar sin colores.
import-bring-progress =
    { $count ->
        [one]
            Traer mi progreso de estudio ({ $count } repaso encontrado):
            las tarjetas conservan su calendario de repasos en lugar de empezar como nuevas, y cada repaso
            se suma a tus estadísticas, rachas incluidas.
        [many]
            Traer mi progreso de estudio ({ $count } de repasos encontrados):
            las tarjetas conservan su calendario de repasos en lugar de empezar como nuevas, y cada repaso
            se suma a tus estadísticas, rachas incluidas.
       *[other]
            Traer mi progreso de estudio ({ $count } repasos encontrados):
            las tarjetas conservan su calendario de repasos en lugar de empezar como nuevas, y cada repaso
            se suma a tus estadísticas, rachas incluidas.
    }
import-adopt-settings = Usar la configuración de FSRS de Anki de este mazo ({ $settings }).
import-adopt-hint = Reemplaza tu configuración actual de programación.
import-importing = Importando tarjetas y multimedia…
import-submit =
    { $count ->
        [one] Importar { $count } tarjeta
        [many] Importar { $count } de tarjetas
       *[other] Importar { $count } tarjetas
    }
import-imported = importado
import-added =
    { $count ->
        [one] { $count } tarjeta importada.
        [many] { $count } de tarjetas importadas.
       *[other] { $count } tarjetas importadas.
    }

## El tamaño de un archivo: $n es el número, ya escrito como lo escribe quien lee

import-size-mb = { $n } MB
import-size-kb = { $n } KB
import-size-b = { $n } B

## La configuración de Anki del paquete, en breve (separada por comas)

import-settings-retention = { $pct }% de retención
import-settings-params = parámetros de FSRS personalizados ({ $count } valores)
import-settings-new =
    { $count ->
        [one] { $count } nueva al día
        [many] { $count } nuevas al día
       *[other] { $count } nuevas al día
    }

## Lo que la vista previa señala del archivo

import-merge = El archivo contiene { $count } mazos ({ $decks }); todas sus tarjetas van al mazo indicado arriba.
import-media = este mazo usa { $what } ({ $mb } MB)
import-media-images =
    { $count ->
        [one] { $count } imagen
        [many] { $count } de imágenes
       *[other] { $count } imágenes
    }
import-media-audio =
    { $count ->
        [one] { $count } archivo de audio
        [many] { $count } de archivos de audio
       *[other] { $count } archivos de audio
    }
import-media-video =
    { $count ->
        [one] { $count } video
        [many] { $count } de videos
       *[other] { $count } videos
    }
import-media-unsupported =
    { $count ->
        [one] { $count } archivo no admitido
        [many] { $count } de archivos no admitidos
       *[other] { $count } archivos no admitidos
    }
import-history-capped = el historial de repasos se limita a los { $max } más recientes; los anteriores se quedan en Anki
import-history-capped-phone = en el teléfono, el historial de repasos se limita a los { $max } más recientes; los anteriores se quedan en Anki
import-row-invalid-utf8 = fila { $row }: UTF-8 no válido en el campo { $field }
import-notes-skipped =
    { $count ->
        [one] { $count } nota omitida (vacía o con un formato no admitido)
        [many] { $count } de notas omitidas (vacías o con un formato no admitido)
       *[other] { $count } notas omitidas (vacías o con un formato no admitido)
    }
# accept: length: "cloze note" is Anki's "nota de respuesta anidada" (25 letters against 10), and the plural for large numbers adds "de".
import-cloze-expanded =
    { $notes ->
        [one] { $notes } nota de respuesta anidada se convirtió en
        [many] { $notes } de notas de respuesta anidada se convirtieron en
       *[other] { $notes } notas de respuesta anidada se convirtieron en
    } { $cards ->
        [one] { $cards } tarjeta
        [many] { $cards } de tarjetas
       *[other] { $cards } tarjetas
    }
import-extra-layouts =
    { $count ->
        [one] { $count } formato de tarjeta adicional incluido (invertidas, etc.)
        [many] { $count } de formatos de tarjeta adicionales incluidos (invertidas, etc.)
       *[other] { $count } formatos de tarjeta adicionales incluidos (invertidas, etc.)
    }
import-suspended =
    { $count ->
        [one] { $count } tarjeta llega suspendida, como en Anki
        [many] { $count } de tarjetas llegan suspendidas, como en Anki
       *[other] { $count } tarjetas llegan suspendidas, como en Anki
    }
import-layouts-skipped =
    { $count ->
        [one] { $count } tarjeta adicional omitida (formato de nota ilegible)
        [many] { $count } de tarjetas adicionales omitidas (formato de nota ilegible)
       *[other] { $count } tarjetas adicionales omitidas (formato de nota ilegible)
    }
import-row-unreadable = fila { $row }: { $error }
import-row-empty = fila { $row }: anverso o reverso vacío
import-about-file = { $file }: { $what }

## Por qué no se pudo importar un archivo

import-too-many-cards = ese archivo tiene más de { $max } tarjetas; cada importación admite hasta { $max } por archivo. Divídelo primero en Anki
import-file-too-many-cards = ese archivo tiene { $count } tarjetas; se importan hasta { $max } por archivo, así que divídelo primero en Anki
import-nothing =
    { $count ->
        [one] no se encontraron tarjetas para importar ({ $count } fila omitida)
        [many] no se encontraron tarjetas para importar ({ $count } de filas omitidas)
       *[other] no se encontraron tarjetas para importar ({ $count } filas omitidas)
    }
import-csv-header-unreadable = no se pudo leer la fila de encabezado del CSV
import-csv-columns = el CSV necesita las columnas de encabezado 'front' y 'back'
import-not-a-zip = ese archivo no es un paquete de Anki (no es un archivo zip)
import-media-list-damaged = la lista de multimedia del paquete está dañada y no se pudo leer
import-compressed-damaged = el contenido comprimido está dañado y no se pudo descomprimir
import-too-many-files = ese paquete contiene más de { $max } archivos; cada importación admite hasta esa cantidad por archivo
import-media-list-large = la lista de multimedia del paquete es demasiado grande
import-media-missing = falta { $file } en el paquete
import-media-unreadable = no se pudo leer { $file } del paquete
import-media-too-large = supera el límite de { $max } bytes
import-media-hash = el contenido no coincide con su hash declarado
import-decompress-limit = al descomprimirse supera el límite de { $max } bytes
import-collection-damaged = la colección dentro del paquete está dañada y no se pudo leer
import-collection-layout = la colección dentro del paquete tiene una estructura inesperada
import-staging-failed = No se pudo preparar el paquete para leerlo; vuelve a intentarlo
import-collection-large = la colección dentro del archivo supera los { $mb } MB
import-no-collection = no se encontró ninguna colección de Anki dentro del archivo
import-expired = la vista previa expiró; vuelve a subir el archivo
import-not-yours = la vista previa pertenece a otra cuenta
import-needs-deck = algunas tarjetas no indican mazo: escribe el nombre de un mazo donde poner la importación
import-deck-required = falta el nombre del mazo
import-cut-off = esta importación se interrumpió a medias y algunas de sus tarjetas ya están en tus mazos; revísalas antes de volver a importar el archivo
import-busy-uploads = demasiadas subidas a la vez; vuelve a intentarlo en un momento
import-busy = hay otra importación en curso; vuelve a intentarlo en un momento
import-deck-too-long = el nombre del mazo es demasiado largo
import-no-file = falta el archivo
import-name-too-long = el nombre del archivo es demasiado largo
