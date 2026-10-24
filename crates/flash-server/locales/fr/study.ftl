### The open core's words in French, both variants: study.

## La page

study-title = Étudier - Flash
study-heading = Étudier

## Étudier par étiquettes

study-tags = Étiquettes
study-tags-some = Étiquettes : { $tags }
study-tags-more = Étiquettes : { $first }, { $second } +{ $more }
study-tags-heading = Étudier par étiquettes
study-tags-none-deck = Aucune étiquette dans ce paquet. Les étiquettes se définissent en ajoutant ou en modifiant une carte.
study-tags-none = Aucune étiquette pour l’instant. Elles se définissent en ajoutant ou en modifiant une carte.
study-tags-filter = Filtrer les étiquettes
study-tags-no-match = Aucune étiquette trouvée.
study-tags-every = Une carte n’entre dans la séance que si elle porte toutes les étiquettes choisies.
study-tags-clear = Effacer
study-tags-apply = Appliquer

## La carte

study-more-label = Plus d’options pour cette carte
study-more-title = Plus
study-edit-card = Modifier la carte
study-archive-card = Archiver la carte
study-type-answer = Saisissez la réponse…
study-show-answer = Afficher la réponse
study-key-enter = entrée
study-key-space = espace
study-left = Encore { $count }

## Les quatre évaluations

study-grade-again = À revoir
study-grade-hard = Difficile
study-grade-good = Correct
study-grade-easy = Facile

## Quand une carte revient après chaque évaluation

# accept: length: the unit is French's written abbreviation with a no-break space before it, where English glues one letter to the number.
interval-minutes = { $count } min
interval-hours = { $count } h
interval-days = { $count } j
interval-months = { $count } mois
# accept: length: "an" and "ans" are the unit spelled out, as French abbreviates no year.
interval-years =
    { $count ->
        [one] { $count } an
        [many] { $count } ans
       *[other] { $count } ans
    }

## Une séance terminée

study-done-limit-heading = Fin des nouvelles cartes du jour
study-done-heading = Séance terminée
study-done-caught-up = Tout est à jour
study-done-held-html =
    { $count ->
        [one] <strong>{ $count }</strong> autre carte attend au-delà de la limite du jour.
        [many] <strong>{ $count }</strong> autres cartes attendent au-delà de la limite du jour.
       *[other] <strong>{ $count }</strong> autres cartes attendent au-delà de la limite du jour.
    }
study-done-held-reviewed =
    { $count ->
        [one] { $count } carte révisée.
        [many] { $count } cartes révisées.
       *[other] { $count } cartes révisées.
    }
study-done-held-reviewed-took =
    { $count ->
        [one] { $count } carte révisée { $took }.
        [many] { $count } cartes révisées { $took }.
       *[other] { $count } cartes révisées { $took }.
    }
study-done-reviewed =
    { $count ->
        [one] { $count } carte révisée
        [many] { $count } cartes révisées
       *[other] { $count } cartes révisées
    }
study-done-reviewed-took =
    { $count ->
        [one] { $count } carte révisée { $took }
        [many] { $count } cartes révisées { $took }
       *[other] { $count } cartes révisées { $took }
    }
study-took-under-minute = en moins d’une minute
study-took-minutes =
    { $count ->
        [one] en { $count } minute
        [many] en { $count } minutes
       *[other] en { $count } minutes
    }
study-done-nothing-due = Rien à réviser pour le moment.
study-pill-good =
    { $count ->
        [one] { $count } correcte
        [many] { $count } correctes
       *[other] { $count } correctes
    }
study-pill-easy =
    { $count ->
        [one] { $count } facile
        [many] { $count } faciles
       *[other] { $count } faciles
    }
study-pill-hard =
    { $count ->
        [one] { $count } difficile
        [many] { $count } difficiles
       *[other] { $count } difficiles
    }
study-pill-again = { $count } à revoir
# accept: length: the same wording as today-more-new; "Plus de nouvelles cartes" read as "no more new cards".
study-more-new = Nouvelles cartes en plus aujourd’hui :
study-week = Cartes à réviser dans la semaine à venir
study-nothing-scheduled = Rien de prévu pour l’instant
study-remind = M’écrire quand des cartes sont à réviser
study-remind-elsewhere = Vous pouvez activer les rappels dans les Paramètres.
study-back-today = Retour à Aujourd’hui
study-goal-on-track = Dans les temps pour le { $date }
study-goal-behind =
    { $count ->
        [one] { $count } carte de retard pour le { $date }
        [many] { $count } cartes de retard pour le { $date }
       *[other] { $count } cartes de retard pour le { $date }
    }
study-goal-learned =
    { $total ->
        [one] { $learned } sur { $total } carte apprise dans { $deck }.
        [many] { $learned } sur { $total } cartes apprises dans { $deck }.
       *[other] { $learned } sur { $total } cartes apprises dans { $deck }.
    }
study-goal-days-left =
    { $count ->
        [one] { $count } jour restant.
        [many] { $count } jours restants.
       *[other] { $count } jours restants.
    }
study-goal-catch-up =
    { $count ->
        [one] { $count } de plus aujourd’hui suffit pour rattraper le retard.
        [many] { $count } de plus aujourd’hui suffisent pour rattraper le retard.
       *[other] { $count } de plus aujourd’hui suffisent pour rattraper le retard.
    }
study-goal-study-more = Étudier { $count } de plus
study-goal-percent = { $count } %
study-empty = Aucune carte pour l’instant.
study-go-decks = Voir vos paquets
study-archived = Carte archivée. Elle reste dans son paquet, hors révision.
study-undo = Annuler

## Quand reviennent les prochaines cartes

study-next-later-today =
    { $count ->
        [0] D’autres cartes plus tard aujourd’hui, vers { $time }
        [one] { $count } autre carte plus tard aujourd’hui, vers { $time }
        [many] { $count } autres cartes plus tard aujourd’hui, vers { $time }
       *[other] { $count } autres cartes plus tard aujourd’hui, vers { $time }
    }
study-next-tomorrow =
    { $count ->
        [0] Des cartes à réviser demain
        [one] { $count } carte à réviser demain
        [many] { $count } cartes à réviser demain
       *[other] { $count } cartes à réviser demain
    }
study-next-weekday =
    { $count ->
        [0] Des cartes à réviser { $day }
        [one] { $count } carte à réviser { $day }
        [many] { $count } cartes à réviser { $day }
       *[other] { $count } cartes à réviser { $day }
    }
study-next-in-days =
    { $count ->
        [0] Des cartes à réviser dans { $days } jours
        [one] { $count } carte à réviser dans { $days } jours
        [many] { $count } cartes à réviser dans { $days } jours
       *[other] { $count } cartes à réviser dans { $days } jours
    }

## Refus pendant la révision

study-edit-removes-card = Cette modification supprimerait la carte en cours. Gardez son trou, ou utilisez Modification avancée.
