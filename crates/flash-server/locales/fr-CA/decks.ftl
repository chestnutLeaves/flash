### The open core's words in Quebec French: decks, where Quebec words them otherwise.

decks-more-settings = Réglages du paquet
deck-bulk-delete-warn = Les supprimer définitivement? Cette action est irréversible.
deck-settings-title = Réglages de { $name } - Flash
deck-settings-label = Réglages du paquet
deck-delete-confirm =
    { $subdecks ->
        [0] { $cards ->
            [0] Supprimer ce paquet? Cette action est irréversible.
            [one] Supprimer ce paquet et sa carte? Cette action est irréversible.
            [many] Supprimer ce paquet et ses { $cards } cartes? Cette action est irréversible.
           *[other] Supprimer ce paquet et ses { $cards } cartes? Cette action est irréversible.
        }
        [one] { $cards ->
            [0] Supprimer ce paquet et son sous-paquet? Cette action est irréversible.
            [one] Supprimer ce paquet, son sous-paquet et leur carte? Cette action est irréversible.
            [many] Supprimer ce paquet, son sous-paquet et leurs { $cards } cartes? Cette action est irréversible.
           *[other] Supprimer ce paquet, son sous-paquet et leurs { $cards } cartes? Cette action est irréversible.
        }
        [many] { $cards ->
            [0] Supprimer ce paquet et ses { $subdecks } sous-paquets? Cette action est irréversible.
            [one] Supprimer ce paquet, ses { $subdecks } sous-paquets et leur carte? Cette action est irréversible.
            [many] Supprimer ce paquet, ses { $subdecks } sous-paquets et leurs { $cards } cartes? Cette action est irréversible.
           *[other] Supprimer ce paquet, ses { $subdecks } sous-paquets et leurs { $cards } cartes? Cette action est irréversible.
        }
       *[other] { $cards ->
            [0] Supprimer ce paquet et ses { $subdecks } sous-paquets? Cette action est irréversible.
            [one] Supprimer ce paquet, ses { $subdecks } sous-paquets et leur carte? Cette action est irréversible.
            [many] Supprimer ce paquet, ses { $subdecks } sous-paquets et leurs { $cards } cartes? Cette action est irréversible.
           *[other] Supprimer ce paquet, ses { $subdecks } sous-paquets et leurs { $cards } cartes? Cette action est irréversible.
        }
    }
goal-tip-today = C’est le jour J. Bonne chance!
goal-date-label = Pour quand devez-vous le maîtriser?
