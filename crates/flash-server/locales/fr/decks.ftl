### The open core's words in French, both variants: decks.

## Tableaux de paquets

decks-col-deck = Paquet
decks-col-due = À réviser
decks-col-new = Nouvelles
decks-fold-label = Sous-paquets de { $name }
decks-fold-show = Afficher les sous-paquets
decks-fold-hide = Masquer les sous-paquets
decks-archived-note = (archivé)
decks-unarchive = désarchiver
decks-study = étudier
decks-col-goal = Objectif
decks-more-label = Plus d’options pour { $name }
decks-more-title = Plus
decks-more-settings = Paramètres du paquet
decks-more-new = Nouvelles cartes en plus aujourd’hui

## Aujourd’hui

today-heading = Aujourd’hui
today-due = à réviser maintenant
today-new = nouvelles aujourd’hui
today-reviewed = révisées aujourd’hui
today-start = Commencer à réviser
today-caught-up = Tout est à jour. Rien à réviser pour le moment.
# accept: length: the same wording as today-more-new-boosted; "Plus de nouvelles cartes" read as "no more new cards".
today-more-new = Nouvelles cartes en plus aujourd’hui :
today-more-new-boosted = Nouvelles cartes en plus aujourd’hui (+{ $count } déjà) :
today-decks = Paquets
today-no-decks-html = Aucun paquet. <a href="/decks">Créez-en un</a> ou <a href="/import">importez-en un</a>.
today-connect = Connecter votre IA
today-connect-sub = Créez des cartes et révisez avec Claude, ChatGPT, Grok, Mistral ou Gemini. Une minute de configuration.
today-connect-dismiss = Fermer

## La page des paquets

decks-title = Paquets - Flash
decks-heading = Paquets
decks-show-archived = Afficher les archivés
decks-hide-archived = Masquer les archivés
decks-import = Importer
decks-new-name = Nom du nouveau paquet
decks-create = Créer
decks-none = Aucun paquet.
decks-all-archived = Tous les paquets sont archivés.

## La page d’un paquet

deck-title = { $name } - Flash
deck-crumbs = Paquets parents
deck-unarchive = Désarchiver
deck-archived-hint = Ce paquet est archivé. Ses cartes restent, mais hors de la file de révision.
deck-front = Recto
deck-back = Verso
deck-tags = Étiquettes
deck-tags-placeholder = examen-2, cardio
deck-tags-hint = Séparées par des virgules, facultatif.
deck-add = Ajouter la carte
deck-advanced = Avancé
deck-search = Rechercher dans ce paquet…
deck-subdecks-heading = Sous-paquets
deck-new-subdeck = Nom du nouveau sous-paquet
deck-add-subdeck = Ajouter un sous-paquet
deck-subnew-open = Nouveau sous-paquet
deck-subnew-cancel = Annuler

## La page d’un paquet : son en-tête, ses sous-paquets, la recherche et le
## filtre, Ajouter, et la barre d’actions sur les cartes sélectionnées

deck-meta-due =
    { $count ->
        [one] à réviser
        [many] à réviser
       *[other] à réviser
    }
deck-meta-new =
    { $count ->
        [one] nouvelle
        [many] nouvelles
       *[other] nouvelles
    }
deck-meta-cards =
    { $count ->
        [one] carte
        [many] cartes
       *[other] cartes
    }
deck-study-short = Étudier
deck-more-label = Plus d’options pour ce paquet
deck-more-title = Plus
deck-export-apkg = Exporter en paquet Anki (.apkg)
deck-export-csv = Exporter en tableur (.csv)
deck-archive-deck = Archiver le paquet
deck-sub-cards =
    { $count ->
        [one] { $count } carte
        [many] { $count } cartes
       *[other] { $count } cartes
    }
deck-sub-study-label = Étudier { $name }
deck-subs-all =
    { $count ->
        [one] Tout afficher ({ $count })
        [many] Tout afficher ({ $count })
       *[other] Tout afficher ({ $count })
    }
deck-subs-archived =
    { $count ->
        [one] { $count } sous-paquet archivé
        [many] { $count } sous-paquets archivés
       *[other] { $count } sous-paquets archivés
    }
deck-search-n =
    { $count ->
        [one] Rechercher parmi { $count } carte…
        [many] Rechercher parmi { $count } cartes…
       *[other] Rechercher parmi { $count } cartes…
    }
deck-filter = Filtrer
deck-filter-show = Afficher
deck-filter-active = Actives
deck-filter-archived = Archivées
deck-filter-decks = Paquets
deck-filter-tags = Étiquettes
# accept: length: "étiquette" is the word for a tag and has no shorter form; the search field spans the whole filter menu.
deck-filter-find-tag = Rechercher une étiquette…
deck-add-open = Ajouter
deck-add-more = Autres façons d’ajouter des cartes
deck-add-write = Écrire une carte
deck-add-import = Importer un paquet Anki
deck-add-close = Fermer
deck-bulk-label = Cartes sélectionnées
deck-bulk-tags = Étiquettes
deck-bulk-tags-label = Ajouter ou retirer des étiquettes
deck-bulk-tag-add = Ajouter
deck-bulk-tag-remove = Retirer
deck-bulk-move = Déplacer
deck-bulk-move-label = Déplacer vers le paquet
deck-bulk-archive = Archiver
deck-bulk-unarchive = Désarchiver
deck-bulk-delete = Supprimer
deck-bulk-clear = Effacer la sélection (Échap)
deck-bulk-delete-warn = Les supprimer définitivement ? Cette action est irréversible.

## Les paramètres d’un paquet

deck-settings-title = Paramètres de { $name } - Flash
deck-settings-label = Paramètres du paquet
deck-settings-back = Retour au paquet
deck-name-heading = Nom
deck-name-label = Nom du paquet
deck-rename = Renommer
deck-saved = Enregistré
deck-parent-heading = Paquet parent
deck-parent-hint =
    { $count ->
        [0] Le paquet qui contient celui-ci. Étudier un paquet inclut ses sous-paquets.
        [one] Le paquet qui contient celui-ci. Étudier un paquet inclut ses sous-paquets, et le sous-paquet de ce paquet le suit.
        [many] Le paquet qui contient celui-ci. Étudier un paquet inclut ses sous-paquets, et les { $count } sous-paquets de ce paquet le suivent.
       *[other] Le paquet qui contient celui-ci. Étudier un paquet inclut ses sous-paquets, et les { $count } sous-paquets de ce paquet le suivent.
    }
deck-parent-label = Paquet parent
deck-top-level = (niveau supérieur)
deck-move = Déplacer
deck-moved = Déplacé
deck-limits-heading = Limites quotidiennes
deck-limit-new = Nouvelles cartes par jour
deck-limit-reviews = Maximum de révisions par jour
deck-limit-default = valeur par défaut du compte ({ $count })
# accept: length: the button names what it saves ("limites"); "Enregistrer" alone would sit beside two other save buttons on the page.
deck-save-limits = Enregistrer les limites
deck-boosted = +{ $count } aujourd’hui
deck-archived-heading = Paquet archivé
deck-archived-explain =
    { $count ->
        [0] Ce paquet est retiré de la file de révision et du décompte des cartes à réviser. Désarchivez-le et ses cartes reviendront avec leur historique de révisions.
        [one] Ce paquet est retiré de la file de révision et du décompte des cartes à réviser. Désarchivez-le et ses cartes reviendront avec leur historique de révisions, tout comme son sous-paquet.
        [many] Ce paquet est retiré de la file de révision et du décompte des cartes à réviser. Désarchivez-le et ses cartes reviendront avec leur historique de révisions, tout comme ses { $count } sous-paquets.
       *[other] Ce paquet est retiré de la file de révision et du décompte des cartes à réviser. Désarchivez-le et ses cartes reviendront avec leur historique de révisions, tout comme ses { $count } sous-paquets.
    }
deck-archived-parents = Les paquets qui le contiennent sont désarchivés avec lui.
deck-archived-toast = Archivé
deck-archive-heading = Archiver le paquet
deck-archive-explain =
    { $count ->
        [0] Conserve chaque carte, ses médias et son historique de révisions, et retire le paquet de la révision et du décompte des cartes à réviser. Vous pouvez le désarchiver à tout moment.
        [one] Conserve chaque carte, ses médias et son historique de révisions, et retire le paquet de la révision et du décompte des cartes à réviser, avec son sous-paquet. Vous pouvez le désarchiver à tout moment.
        [many] Conserve chaque carte, ses médias et son historique de révisions, et retire le paquet de la révision et du décompte des cartes à réviser, avec ses { $count } sous-paquets. Vous pouvez le désarchiver à tout moment.
       *[other] Conserve chaque carte, ses médias et son historique de révisions, et retire le paquet de la révision et du décompte des cartes à réviser, avec ses { $count } sous-paquets. Vous pouvez le désarchiver à tout moment.
    }
deck-archive = Archiver
deck-restored = Restauré
deck-delete-heading = Supprimer le paquet
deck-delete-explain =
    { $count ->
        [0] Supprime ce paquet et toutes ses cartes. L’historique de révisions est conservé. Cette action est irréversible.
        [one] Supprime ce paquet, son sous-paquet et toutes leurs cartes. L’historique de révisions est conservé. Cette action est irréversible.
        [many] Supprime ce paquet, ses { $count } sous-paquets et toutes leurs cartes. L’historique de révisions est conservé. Cette action est irréversible.
       *[other] Supprime ce paquet, ses { $count } sous-paquets et toutes leurs cartes. L’historique de révisions est conservé. Cette action est irréversible.
    }
deck-delete-confirm =
    { $subdecks ->
        [0] { $cards ->
            [0] Supprimer ce paquet ? Cette action est irréversible.
            [one] Supprimer ce paquet et sa carte ? Cette action est irréversible.
            [many] Supprimer ce paquet et ses { $cards } cartes ? Cette action est irréversible.
           *[other] Supprimer ce paquet et ses { $cards } cartes ? Cette action est irréversible.
        }
        [one] { $cards ->
            [0] Supprimer ce paquet et son sous-paquet ? Cette action est irréversible.
            [one] Supprimer ce paquet, son sous-paquet et leur carte ? Cette action est irréversible.
            [many] Supprimer ce paquet, son sous-paquet et leurs { $cards } cartes ? Cette action est irréversible.
           *[other] Supprimer ce paquet, son sous-paquet et leurs { $cards } cartes ? Cette action est irréversible.
        }
        [many] { $cards ->
            [0] Supprimer ce paquet et ses { $subdecks } sous-paquets ? Cette action est irréversible.
            [one] Supprimer ce paquet, ses { $subdecks } sous-paquets et leur carte ? Cette action est irréversible.
            [many] Supprimer ce paquet, ses { $subdecks } sous-paquets et leurs { $cards } cartes ? Cette action est irréversible.
           *[other] Supprimer ce paquet, ses { $subdecks } sous-paquets et leurs { $cards } cartes ? Cette action est irréversible.
        }
       *[other] { $cards ->
            [0] Supprimer ce paquet et ses { $subdecks } sous-paquets ? Cette action est irréversible.
            [one] Supprimer ce paquet, ses { $subdecks } sous-paquets et leur carte ? Cette action est irréversible.
            [many] Supprimer ce paquet, ses { $subdecks } sous-paquets et leurs { $cards } cartes ? Cette action est irréversible.
           *[other] Supprimer ce paquet, ses { $subdecks } sous-paquets et leurs { $cards } cartes ? Cette action est irréversible.
        }
    }
deck-delete = Supprimer le paquet

## Pourquoi une carte n’a pas été ajoutée

card-not-added-cloze = Cette carte n’a pas été ajoutée : une note à trous doit contenir au moins un trou {"{{"}c1::…{"}}"}. Sélectionnez la réponse et appuyez sur Ctrl+Maj+C.
card-not-added-empty = Cette carte n’a pas été ajoutée : les deux faces doivent avoir un contenu.
card-not-added-long = Cette carte n’a pas été ajoutée : une face est trop longue.
card-not-added-media = Cette carte n’a pas été ajoutée : elle fait référence à des médias qui ne vous appartiennent pas.
card-not-added-invalid = Cette carte n’a pas été ajoutée : impossible d’enregistrer le contenu.
card-not-added-limit = Cette carte n’a pas été ajoutée : ce compte a atteint sa limite de cartes.

## Pourquoi un paquet n’a pas pu être renommé ou déplacé

deck-error-nest = un nom ne peut pas contenir « :: ». Utilisez Paquet parent pour imbriquer ce paquet
deck-error-empty = le nom ne peut pas être vide
deck-error-taken = vous avez déjà un paquet de ce nom
deck-error-self = un paquet ne peut pas être déplacé dans lui-même
deck-error-long = le nom complet du paquet serait trop long
deck-error-archived = ce paquet est archivé. Désarchivez-le d’abord
deck-error-cap = ce compte a atteint son nombre maximal de paquets
deck-error-invalid = l’opération a échoué

## Refus pour nommer ou créer un paquet

deck-error-nest-sentence = Un nom ne peut pas contenir « :: ». Utilisez Paquet parent pour imbriquer ce paquet.
deck-error-name-new = Nommez le nouveau paquet.
deck-error-archived-sentence = Votre paquet « { $name } » est archivé. Désarchivez-le ou choisissez un autre nom.

## La date cible d’un paquet : le jour où l’apprenant veut le connaître (un
## examen, un voyage, une échéance). Flash répartit les nouvelles cartes pour
## finir avant cette date et ramène, les derniers jours, celles qui en ont besoin.

goal-error-past = Choisissez une date à partir de demain.
goal-error-far = Choisissez une date dans les cinq prochaines années.

goal-set = Définir une date cible…
goal-change = Modifier la date cible…
goal-remove = Supprimer la date cible

goal-left-days = { $count } j
goal-left-today = Aujourd’hui
goal-left-done = Terminé
goal-left-month-year = { $month } { $year }

goal-tip-on-track = Dans les temps pour le { $date }
goal-tip-behind = En retard pour le { $date }
goal-tip-learned =
    { $total ->
        [one] { $learned } sur { $total } carte apprise.
        [many] { $learned } sur { $total } cartes apprises.
       *[other] { $learned } sur { $total } cartes apprises.
    }
goal-tip-pace =
    { $count ->
        [one] { $count } nouvelle par jour jusqu’au { $date }, puis une révision finale.
        [many] { $count } nouvelles par jour jusqu’au { $date }, puis une révision finale.
       *[other] { $count } nouvelles par jour jusqu’au { $date }, puis une révision finale.
    }
goal-tip-final = Les dernières révisions avant le { $date }.
goal-tip-catch-up =
    { $count ->
        [one] { $count } nouvelle de plus aujourd’hui pour rattraper le retard.
        [many] { $count } nouvelles de plus aujourd’hui pour rattraper le retard.
       *[other] { $count } nouvelles de plus aujourd’hui pour rattraper le retard.
    }
goal-tip-today = C’est le jour J. Bonne chance !
goal-tip-passed = Votre date cible était le { $date }.
goal-passed = Votre date cible est passée.
goal-archive = Archiver le paquet
goal-keep = Continuer à réviser

goal-heading-set = Définir une date cible
goal-heading-change = Modifier la date cible
goal-card-heading = Date cible
goal-date-label = Pour quand devez-vous le maîtriser ?
goal-picks = Choix rapides
goal-pick-weeks =
    { $count ->
        [one] { $count } semaine
        [many] { $count } semaines
       *[other] { $count } semaines
    }
goal-pick-months = { $count } mois
goal-pick-year = 1 an
goal-plan-pace-html =
    { $count ->
        [one] <b>{ $count } nouvelle carte par jour</b> jusqu’au { $date } ({ $unseen } à découvrir)
        [many] <b>{ $count } nouvelles cartes par jour</b> jusqu’au { $date } ({ $unseen } à découvrir)
       *[other] <b>{ $count } nouvelles cartes par jour</b> jusqu’au { $date } ({ $unseen } à découvrir)
    }
goal-plan-seen-html = <b>Toutes les cartes ont été vues.</b> Les révisions les gardent en mémoire jusque-là.
goal-plan-final-html = <b>Du { $from } au { $to } :</b> une dernière révision des cartes qui en ont besoin
goal-plan-final-close-html = <b>Jusqu’au { $to } :</b> une dernière révision de toutes les cartes qui en ont besoin
goal-plan-after-html = <b>Après le { $date },</b> retour au rythme normal. Vous pourrez alors archiver le paquet.
goal-plan-pick = Choisissez une date pour voir le programme.
goal-tight = Cela fait environ { $count } nouvelles cartes par jour. Une date plus lointaine allège les journées.
goal-save = Enregistrer
goal-cancel = Annuler
