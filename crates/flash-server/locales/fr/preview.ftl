### The open core's words in French, both variants: preview.

## La page

preview-title = Aperçu - Flash
preview-heading = Aperçu
preview-unaffected = Sans effet sur la répétition espacée
preview-end = Terminer
preview-empty = Ce paquet n’a aucune carte à parcourir.
preview-go-deck = Retour à { $name }
preview-missed = Raté
preview-got = Réussi

## La fin

preview-unaffected-line = L’aperçu n’a aucun effet sur la répétition espacée.
preview-ready =
    { $count ->
        [one] { $count } carte est à étudier maintenant.
        [many] { $count } de cartes sont à étudier maintenant.
       *[other] { $count } cartes sont à étudier maintenant.
    }
preview-next-later-today =
    { $count ->
        [0] Les prochaines révisions sont plus tard aujourd’hui.
        [one] La prochaine révision est plus tard aujourd’hui{ " " }: { $count } carte.
        [many] Les prochaines révisions sont plus tard aujourd’hui{ " " }: { $count } de cartes.
       *[other] Les prochaines révisions sont plus tard aujourd’hui{ " " }: { $count } cartes.
    }
preview-next-tomorrow =
    { $count ->
        [0] Les prochaines révisions sont demain.
        [one] La prochaine révision est demain{ " " }: { $count } carte.
        [many] Les prochaines révisions sont demain{ " " }: { $count } de cartes.
       *[other] Les prochaines révisions sont demain{ " " }: { $count } cartes.
    }
preview-next-weekday =
    { $count ->
        [0] Les prochaines révisions sont { $day }.
        [one] La prochaine révision est { $day }{ " " }: { $count } carte.
        [many] Les prochaines révisions sont { $day }{ " " }: { $count } de cartes.
       *[other] Les prochaines révisions sont { $day }{ " " }: { $count } cartes.
    }
preview-next-in-days =
    { $count ->
        [0] Les prochaines révisions sont dans { $days } jours.
        [one] La prochaine révision est dans { $days } jours{ " " }: { $count } carte.
        [many] Les prochaines révisions sont dans { $days } jours{ " " }: { $count } de cartes.
       *[other] Les prochaines révisions sont dans { $days } jours{ " " }: { $count } cartes.
    }
preview-start-over = Recommencer

## Le menu du paquet

preview-menu =
    { $count ->
        [one] Aperçu de la carte
        [many] Aperçu des { $count } de cartes
       *[other] Aperçu des { $count } cartes
    }
preview-menu-sub = Sans effet sur la répétition espacée

## Ce que dit le script de la page

js-preview-score =
    { $got ->
        [one] { $got } réussie sur { $total }
        [many] { $got } réussies sur { $total }
       *[other] { $got } réussies sur { $total }
    }
js-preview-percent = { $percent }{ " " }%
js-preview-review-missed =
    { $count ->
        [one] Revoir la carte ratée
        [many] Revoir les { $count } de cartes ratées
       *[other] Revoir les { $count } cartes ratées
    }
