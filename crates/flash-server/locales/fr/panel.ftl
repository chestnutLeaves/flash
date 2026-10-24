### The open core's words in French, both variants: panel.

## L’en-tête

panel-more = Plus
panel-more-label = Plus d’options pour cette carte
panel-edit-card = Modifier la carte
panel-archive-card = Archiver la carte
panel-expand = Agrandir
panel-expand-title = Afficher cette carte en plein écran
panel-expand-label = Agrandir : afficher cette carte en plein écran
panel-tags = Étiquettes :

## Réviser

panel-loading = Chargement de vos cartes…
panel-show-all = Tout afficher
panel-show-answer = Afficher la réponse
panel-key-space = Espace
panel-again = À revoir
panel-hard = Difficile
panel-good = Correct
panel-easy = Facile
panel-key = Touche { $key }
panel-to-go = Encore { $count }
panel-archived = Carte archivée
panel-undo = Annuler

## Modifier une carte dans le panneau

panel-front = Recto
panel-back = Verso
panel-advanced-edit = Modification avancée
panel-save = Enregistrer
panel-cancel = Annuler
panel-save-failed = Impossible d’enregistrer la carte. Réessayez.

## Une séance terminée

panel-held-title = Fin des nouvelles cartes du jour
panel-held-text =
    { $count ->
        [one] { $count } autre carte attend au-delà de la limite du jour.
        [many] { $count } autres cartes attendent au-delà de la limite du jour.
       *[other] { $count } autres cartes attendent au-delà de la limite du jour.
    }
panel-held-text-summary = { $held } { $summary }.
panel-study-more = En étudier { $count } de plus
panel-empty-title = Aucune carte pour l’instant
panel-empty-text = Demandez des cartes à partir de vos notes, d’un document ou d’un sujet, et révisez-les ici.
panel-make = Créer des cartes
panel-done-title = Séance terminée
panel-caught-up = Tout est à jour
panel-nothing-due = Rien à réviser pour le moment.
panel-cards-reviewed =
    { $count ->
        [one] { $count } carte révisée
        [many] { $count } cartes révisées
       *[other] { $count } cartes révisées
    }
panel-reviewed-in =
    { $count ->
        [one] { $reviewed } en { $count } minute
        [many] { $reviewed } en { $count } minutes
       *[other] { $reviewed } en { $count } minutes
    }
panel-reviewed-quick = { $reviewed } en moins d’une minute
panel-score-again = { $count } à revoir
panel-score-hard = { $count } difficile
panel-score-good = { $count } correct
panel-score-easy = { $count } facile
panel-next-today =
    { $count ->
        [0] D’autres cartes plus tard aujourd’hui, vers { $time }
        [one] { $count } autre carte plus tard aujourd’hui, vers { $time }
        [many] { $count } autres cartes plus tard aujourd’hui, vers { $time }
       *[other] { $count } autres cartes plus tard aujourd’hui, vers { $time }
    }
panel-next-tomorrow =
    { $count ->
        [0] Des cartes à réviser demain
        [one] { $count } carte à réviser demain
        [many] { $count } cartes à réviser demain
       *[other] { $count } cartes à réviser demain
    }
panel-next-weekday =
    { $count ->
        [0] Des cartes à réviser { $day }
        [one] { $count } carte à réviser { $day }
        [many] { $count } cartes à réviser { $day }
       *[other] { $count } cartes à réviser { $day }
    }
panel-next-days =
    { $count ->
        [0] Des cartes à réviser dans { $days } jours
        [one] { $count } carte à réviser dans { $days } jours
        [many] { $count } cartes à réviser dans { $days } jours
       *[other] { $count } cartes à réviser dans { $days } jours
    }
panel-nothing-scheduled = Rien de prévu pour l’instant
panel-remind = Me le rappeler
panel-remind-title = M’écrire quand des cartes sont à réviser
panel-reminded = Rappel activé
panel-remind-fallback = Vous pouvez activer les rappels dans les paramètres de Flash.
panel-progress = Voir votre progression
panel-ask-deck = Je voudrais créer des cartes de révision pour { $deck }.
panel-ask-tags = Je voudrais des cartes de révision étiquetées { $tags }.
panel-ask-first = Je voudrais créer mes premières cartes de révision.

## Les cartes qui viennent d’être créées ou modifiées

panel-new-cards =
    { $count ->
        [one] { $count } nouvelle carte
        [many] { $count } nouvelles cartes
       *[other] { $count } nouvelles cartes
    }
panel-edited =
    { $count ->
        [one] { $count } modifiée
        [many] { $count } modifiées
       *[other] { $count } modifiées
    }
panel-before = Avant
panel-previous = Carte précédente
panel-next = Carte suivante
panel-position = { $at } sur { $count }
panel-study-now = Étudier maintenant
panel-see-all = Tout voir

## Problèmes

panel-no-answer = Flash n’a pas répondu.
panel-no-answer-late = Flash n’a pas répondu à temps.
panel-try-again = Réessayer
panel-needs-host = Ce panneau nécessite un hôte qui affiche les MCP Apps.
