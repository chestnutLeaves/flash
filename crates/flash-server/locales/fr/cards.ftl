### The open core's words in French, both variants: cards.

## La liste des cartes d’un paquet

cards-none = Aucune carte.
cards-none-filtered = Aucune carte ne correspond.
cards-chip-archived = Archivées
cards-own = Ce paquet uniquement
cards-chip-remove = Retirer { $name } du filtre
cards-match-label = Correspondance des étiquettes
# accept: length: French has no one-word "any of these tags"; "Au moins une" is how a filter says it, and the two-button switch gives it room.
cards-match-any = Au moins une
cards-match-all = Toutes
cards-chip-clear = Effacer
cards-select-page = Sélectionner toutes les cartes de cette page
cards-select-card = Sélectionner cette carte
cards-from-title = Dans { $name }. Afficher uniquement ses cartes.
cards-col-front = Recto
cards-col-back = Verso
cards-col-tags = Étiquettes
cards-pages = Pages de cartes
cards-previous = Précédent
cards-next = Suivant
cards-page-of = Page { $page } sur { $pages } · { $count } cartes
cards-edit-label = Modifier la carte
cards-edit = Modifier
cards-unarchive-label = Désarchiver la carte
cards-unarchive = Désarchiver
cards-archive-label = Archiver la carte
cards-archive = Archiver
cards-delete-label = Supprimer la carte
cards-delete = Supprimer
cards-bulk-archived =
    { $count ->
        [one] { $count } carte archivée
        [many] { $count } cartes archivées
       *[other] { $count } cartes archivées
    }
cards-bulk-unarchived =
    { $count ->
        [one] { $count } carte remise en révision
        [many] { $count } cartes remises en révision
       *[other] { $count } cartes remises en révision
    }
cards-bulk-deleted =
    { $count ->
        [one] { $count } carte supprimée
        [many] { $count } cartes supprimées
       *[other] { $count } cartes supprimées
    }
cards-bulk-moved =
    { $count ->
        [one] { $count } carte déplacée vers { $deck }
        [many] { $count } cartes déplacées vers { $deck }
       *[other] { $count } cartes déplacées vers { $deck }
    }
cards-bulk-tagged =
    { $count ->
        [one] Étiquettes ajoutées à { $count } carte
        [many] Étiquettes ajoutées à { $count } cartes
       *[other] Étiquettes ajoutées à { $count } cartes
    }
cards-bulk-untagged =
    { $count ->
        [one] Étiquettes retirées de { $count } carte
        [many] Étiquettes retirées de { $count } cartes
       *[other] Étiquettes retirées de { $count } cartes
    }
cards-bulk-undone = Annulé
cards-bulk-undo = Annuler
cards-error-bulk-changed = La liste a changé pendant votre sélection. Sélectionnez de nouveau les cartes.
cards-error-bulk-many = Impossible de modifier plus de { $max } cartes à la fois.
cards-error-move-archived = Ce paquet est archivé. Désarchivez-le d’abord.
cards-error-tags-none = Indiquez au moins une étiquette.
cards-kind-cloze = trou { $n }
cards-kind-reversed = inversée
cards-kind-typed = saisie

## La page d’une carte et la fenêtre de modification

editor-page-title = Modifier la carte - Flash
editor-crumbs = Paquet
editor-heading = Modifier la carte
editor-tags = Étiquettes
editor-tags-hint = Séparées par des virgules, appliquées à chaque carte de cette note.
editor-history =
    { $count ->
        [one] Historique ({ $count } version antérieure)
        [many] Historique ({ $count } versions antérieures)
       *[other] Historique ({ $count } versions antérieures)
    }
editor-replaced = { $ago }, avant { $by }
editor-by-web = une modification ici
editor-by-mcp = une modification d’un assistant
editor-by-app = une modification dans l’application
editor-by-sync = une modification faite ailleurs
editor-by-restore = une restauration
editor-restore = Restaurer
editor-restored = Version antérieure restaurée.
editor-saved = Enregistré.
editor-cancel = Annuler
editor-save = Enregistrer

## L’éditeur

editor-type = Type
note-type-basic = Basique
note-type-reversed = Basique (et carte inversée)
note-type-typed = Basique (saisir la réponse)
note-type-cloze = Texte à trous
editor-siblings = { $count } cartes partagent cette note
editor-siblings-hint = L’enregistrement régénère toutes les cartes de cette note
editor-toolbar = Mise en forme
editor-bold-tip = Gras (Ctrl+B)
editor-bold = Gras
editor-italic-tip = Italique (Ctrl+I)
editor-italic = Italique
editor-underline-tip = Souligné (Ctrl+U)
editor-underline = Souligné
editor-strike = Barré
editor-superscript = Exposant
editor-subscript = Indice
editor-text-color = Couleur du texte
editor-text-hue = Texte { $hue }
editor-no-color = Aucune couleur
editor-highlight = Surlignage
editor-highlight-hue = Surlignage { $hue }
editor-no-highlight = Aucun surlignage
editor-bulleted = Liste à puces
editor-numbered = Liste numérotée
editor-clear = Effacer la mise en forme
editor-attach-tip = Joindre une image, un son ou une vidéo
editor-attach = Joindre un média
editor-cloze-tip = Texte à trous (Ctrl+Maj+C, ou Ctrl+Maj+Alt+C pour reprendre le dernier numéro)
editor-cloze = Texte à trous
editor-source-tip = Source HTML
editor-source = Afficher ou masquer la source HTML
editor-front = Recto
editor-back = Verso
editor-front-placeholder = Question
editor-back-placeholder = Réponse
editor-front-source = Source HTML du recto
editor-back-source = Source HTML du verso
editor-typed-hint = Pendant la révision, vous saisirez cette réponse et verrez les différences.
editor-drop-hint = Déposez ou collez des images et des sons directement dans un champ.
editor-cloze-hint-html = Sélectionnez une réponse et appuyez sur <kbd>Ctrl</kbd>+<kbd>Maj</kbd>+<kbd>C</kbd> pour la masquer.
editor-reversed-hint = Deux cartes : du recto au verso, et du verso au recto.
editor-save-hint-html = <kbd>Ctrl</kbd>+<kbd>Entrée</kbd> pour enregistrer.

## Couleurs

hue-red = rouge
hue-amber = ambre
hue-green = vert
hue-teal = bleu canard
hue-blue = bleu
hue-purple = violet
hue-pink = rose

## Pourquoi une carte ou une note est refusée

card-error-front-empty = le recto de la carte est vide
card-error-back-empty = le verso de la carte est vide
card-error-side-long = une face de la carte dépasse { $max } octets
card-error-field-long = un champ dépasse { $max } octets de HTML
card-error-no-cloze = ajoutez au moins un trou {"{{"}c1::…{"}}"}
card-error-cloze-empty = le trou c{ $n } a une réponse vide
card-error-cloze-card = trou c{ $n } : { $error }
card-error-media = le média { $id } ne fait pas partie de vos fichiers

## Une image ou un son refusé à l’envoi

media-error-format = { $filename } : format non pris en charge
media-error-empty = { $filename } : fichier vide
media-error-too-large = { $filename } : dépasse la limite de { $mb } Mo par fichier
media-error-mismatch = { $filename } : le contenu ne correspond pas à l’extension
media-error-count = limite de fichiers média atteinte. Supprimez d’abord quelques fichiers
media-error-storage = limite de stockage des médias atteinte. Contactez l’assistance
