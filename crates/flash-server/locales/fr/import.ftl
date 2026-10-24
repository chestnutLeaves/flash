### The open core's words in French, both variants: import.

## La page d’importation

import-title = Importer - Flash
import-heading = Importer
import-intro-html =
    Un CSV avec l’en-tête <code>front,back,tags,deck</code>, ou un export Anki <code>.apkg</code>.
    Les cartes sont importées comme nouvelles, sauf si vous reprenez votre progression Anki avec elles.
import-file = Fichier
import-choose = Choisir un fichier
import-no-file-chosen = Aucun fichier choisi, ou déposez-en un ici
import-into = Dans le paquet
import-deck-placeholder = ex. Pharmacologie
import-reading = Envoi et lecture du fichier…
import-preview-button = Aperçu
import-preview-heading = Aperçu
import-ready-html =
    { $count ->
        [one] <strong>{ $count } carte</strong> prête à importer depuis <span class="mono">{ $name }</span> <span class="muted">({ $size })</span>.
        [many] <strong>{ $count } cartes</strong> prêtes à importer depuis <span class="mono">{ $name }</span> <span class="muted">({ $size })</span>.
       *[other] <strong>{ $count } cartes</strong> prêtes à importer depuis <span class="mono">{ $name }</span> <span class="muted">({ $size })</span>.
    }
import-skipped =
    { $count ->
        [one] { $count } ligne ignorée
        [many] { $count } lignes ignorées
       *[other] { $count } lignes ignorées
    }
import-decks-in-file = Paquets dans ce fichier
import-deck-cards =
    { $count ->
        [one] { $count } carte
        [many] { $count } cartes
       *[other] { $count } cartes
    }
import-more =
    { $count ->
        [one] +{ $count } autre
        [many] +{ $count } autres
       *[other] +{ $count } autres
    }
import-keep-hint-html = Chaque paquet est conservé, et <code>Parent::Child</code> devient un sous-paquet. Un paquet existant du même nom reçoit les cartes.
import-put-under = Placer sous (facultatif)
import-top-level = Niveau supérieur
import-put-under-hint = Nommez un paquet pour les y ranger tous comme sous-paquets, ou laissez vide.
import-name-deck = Nommez ce paquet
import-advanced = Avancé
import-keep-colors =
    { $count ->
        [one] Garder les couleurs du paquet, adaptées à la palette de Flash ({ $count } carte colorée).
        [many] Garder les couleurs du paquet, adaptées à la palette de Flash ({ $count } cartes colorées).
       *[other] Garder les couleurs du paquet, adaptées à la palette de Flash ({ $count } cartes colorées).
    }
import-clean-look = Aspect épuré, importer sans couleurs.
import-bring-progress =
    { $count ->
        [one]
            Reprendre ma progression ({ $count } révision trouvée) :
            les cartes gardent leur planification au lieu de repartir de zéro, et chaque révision
            apparaît dans vos statistiques, séries comprises.
        [many]
            Reprendre ma progression ({ $count } révisions trouvées) :
            les cartes gardent leur planification au lieu de repartir de zéro, et chaque révision
            apparaît dans vos statistiques, séries comprises.
       *[other]
            Reprendre ma progression ({ $count } révisions trouvées) :
            les cartes gardent leur planification au lieu de repartir de zéro, et chaque révision
            apparaît dans vos statistiques, séries comprises.
    }
import-adopt-settings = Adopter les paramètres FSRS Anki de ce paquet ({ $settings }).
import-adopt-hint = Remplace vos paramètres de planification actuels.
import-importing = Importation des cartes et des médias…
import-submit =
    { $count ->
        [one] Importer { $count } carte
        [many] Importer { $count } cartes
       *[other] Importer { $count } cartes
    }
import-imported = importé
import-added =
    { $count ->
        [one] { $count } carte ajoutée.
        [many] { $count } cartes ajoutées.
       *[other] { $count } cartes ajoutées.
    }

## La taille d’un fichier

import-size-mb = { $n } Mo
import-size-kb = { $n } Ko
import-size-b = { $n } o

## Les paramètres Anki du fichier, en bref

import-settings-retention = { $pct } % de rétention
import-settings-params = paramètres FSRS personnalisés ({ $count } valeurs)
import-settings-new =
    { $count ->
        [one] { $count } nouvelle carte par jour
        [many] { $count } nouvelles cartes par jour
       *[other] { $count } nouvelles cartes par jour
    }

## Ce que l’aperçu signale

import-merge = Le fichier contient { $count } paquets ({ $decks }). Toutes leurs cartes iront dans le paquet nommé ci-dessus.
import-media = ce paquet fait référence à { $what } ({ $mb } Mo)
import-media-images =
    { $count ->
        [one] { $count } image
        [many] { $count } images
       *[other] { $count } images
    }
import-media-audio =
    { $count ->
        [one] { $count } extrait audio
        [many] { $count } extraits audio
       *[other] { $count } extraits audio
    }
import-media-video =
    { $count ->
        [one] { $count } vidéo
        [many] { $count } vidéos
       *[other] { $count } vidéos
    }
import-media-unsupported =
    { $count ->
        [one] { $count } fichier non pris en charge
        [many] { $count } fichiers non pris en charge
       *[other] { $count } fichiers non pris en charge
    }
import-history-capped = l’historique est limité aux { $max } révisions les plus récentes. Les plus anciennes restent dans Anki
import-history-capped-phone = sur téléphone, l’historique est limité aux { $max } révisions les plus récentes. Les autres restent dans Anki
import-row-invalid-utf8 = ligne { $row } : UTF-8 non valide dans le champ { $field }
import-notes-skipped =
    { $count ->
        [one] { $count } note ignorée (vide ou mise en page non prise en charge)
        [many] { $count } notes ignorées (vides ou mise en page non prise en charge)
       *[other] { $count } notes ignorées (vides ou mise en page non prise en charge)
    }
import-cloze-expanded =
    { $notes ->
        [one] { $notes } note à trous a donné
        [many] { $notes } notes à trous ont donné
       *[other] { $notes } notes à trous ont donné
    } { $cards ->
        [one] { $cards } carte
        [many] { $cards } cartes
       *[other] { $cards } cartes
    }
import-extra-layouts =
    { $count ->
        [one] { $count } type de carte supplémentaire inclus (inversée, etc.)
        [many] { $count } types de carte supplémentaires inclus (inversée, etc.)
       *[other] { $count } types de carte supplémentaires inclus (inversée, etc.)
    }
import-suspended =
    { $count ->
        [one] { $count } carte arrive suspendue, comme dans Anki
        [many] { $count } cartes arrivent suspendues, comme dans Anki
       *[other] { $count } cartes arrivent suspendues, comme dans Anki
    }
import-layouts-skipped =
    { $count ->
        [one] { $count } carte supplémentaire ignorée (mise en page de note illisible)
        [many] { $count } cartes supplémentaires ignorées (mise en page de note illisible)
       *[other] { $count } cartes supplémentaires ignorées (mise en page de note illisible)
    }
import-row-unreadable = ligne { $row } : { $error }
import-row-empty = ligne { $row } : recto ou verso vide
import-about-file = { $file } : { $what }

## Pourquoi un fichier n’a pas pu être importé

import-too-many-cards = ce fichier contient plus de { $max } cartes. Les importations sont limitées à { $max } par fichier. Divisez-le d’abord dans Anki
import-file-too-many-cards = ce fichier contient { $count } cartes. Les importations sont limitées à { $max } par fichier, divisez-le d’abord dans Anki
import-nothing =
    { $count ->
        [one] aucune carte importable trouvée ({ $count } ligne ignorée)
        [many] aucune carte importable trouvée ({ $count } lignes ignorées)
       *[other] aucune carte importable trouvée ({ $count } lignes ignorées)
    }
import-csv-header-unreadable = impossible de lire la ligne d’en-tête du CSV
import-csv-columns = le CSV doit avoir les colonnes d’en-tête « front » et « back »
import-not-a-zip = ce fichier n’est pas un export Anki (ce n’est pas une archive zip)
import-media-list-damaged = la liste des médias de l’archive est endommagée et illisible
import-compressed-damaged = le contenu compressé est endommagé et n’a pas pu être décompressé
import-too-many-files = cette archive contient plus de { $max } fichiers. Les importations sont limitées à ce nombre par fichier
import-media-list-large = la liste des médias de l’archive est trop volumineuse
import-media-missing = { $file } est absent de l’archive
import-media-unreadable = impossible de lire { $file } dans l’archive
import-media-too-large = dépasse la limite de { $max } octets
import-media-hash = le contenu ne correspond pas à son empreinte déclarée
import-decompress-limit = dépasse la limite de { $max } octets une fois décompressé
import-collection-damaged = la collection contenue dans l’archive est endommagée et illisible
import-collection-layout = la collection contenue dans l’archive a une structure inattendue
import-staging-failed = impossible de préparer l’archive pour la lecture. Réessayez
import-collection-large = la collection contenue dans le fichier dépasse { $mb } Mo
import-no-collection = aucune collection Anki trouvée dans le fichier
import-expired = aperçu expiré. Envoyez de nouveau le fichier
import-not-yours = cet aperçu appartient à un autre utilisateur
import-needs-deck = certaines cartes n’indiquent aucun paquet. Nommez un paquet où ranger l’importation
import-deck-required = nom de paquet requis
import-cut-off = cette importation s’est interrompue en cours de route et certaines de ses cartes sont déjà dans vos paquets. Vérifiez-les avant d’importer de nouveau le fichier
import-busy-uploads = trop d’envois en même temps. Réessayez dans un instant
import-busy = une autre importation est en cours. Réessayez dans un instant
import-deck-too-long = le nom du paquet est trop long
import-no-file = aucun fichier
import-name-too-long = le nom du fichier est trop long
