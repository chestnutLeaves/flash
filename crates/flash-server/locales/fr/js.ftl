### The open core's words in French, both variants: js.

## Comportements communs

js-ui-play-audio = Lire l’audio
js-ui-pause-audio = Mettre en pause
js-ui-copied = Copié
js-ui-copy-failed = Impossible de copier
js-ui-show-subdecks = Afficher les sous-paquets
js-ui-hide-subdecks = Masquer les sous-paquets
js-ui-show-archived = Afficher les archivés
js-ui-hide-archived = Masquer les archivés

## Connexion avec une clé d’accès

js-passkey-cancelled = Annulé ou délai dépassé.
js-passkey-failed = Impossible de terminer l’opération. Réessayez.
js-passkey-bad-list = La liste d’identifiants du serveur n’est pas une liste.
js-passkey-bad-entry = La liste d’identifiants du serveur contient autre chose.
js-passkey-not-passkey = Le serveur a indiqué un identifiant qui n’est pas une clé d’accès.
js-passkey-none-made = Le navigateur n’a pas créé de clé d’accès.
js-passkey-none-used = Le navigateur n’a pas utilisé de clé d’accès.
js-passkey-not-used = Aucune clé d’accès n’a été utilisée. Connectez-vous autrement, puis rajoutez-la dans Paramètres.
js-passkey-link-incomplete = Ce lien d’inscription est incomplet.

## La page d’un paquet (deck.js)

js-deck-selected =
    { $count ->
        [one] { $count } sélectionnée
        [many] { $count } sélectionnées
       *[other] { $count } sélectionnées
    }
js-deck-select-all =
    { $count ->
        [one] Tout sélectionner ({ $count })
        [many] Tout sélectionner ({ $count })
       *[other] Tout sélectionner ({ $count })
    }
js-deck-delete-n =
    { $count ->
        [one] Supprimer { $count } carte
        [many] Supprimer { $count } cartes
       *[other] Supprimer { $count } cartes
    }
js-deck-row-confirm = Confirmer la suppression
js-deck-add = Ajouter
js-deck-close = Fermer
js-deck-failed = Impossible d’effectuer l’action. Réessayez.

## L’éditeur de cartes

js-editor-front = Recto
js-editor-back = Verso
js-editor-question = Question
js-editor-answer = Réponse
js-editor-answer-to-type = Réponse à saisir
js-editor-text = Texte
js-editor-extra = Complément
js-editor-cloze-hint = Phrase avec des {"{{"}c1::trous{"}}"}
js-editor-extra-hint = Affiché avec la réponse (facultatif)
js-editor-uploading = Envoi de { $name }…
js-editor-upload-failed = Impossible d’envoyer.
js-editor-select-first = Sélectionnez d’abord du texte.
js-editor-wait-upload = Attendez la fin de l’envoi.
js-editor-simple = Simple
js-editor-advanced = Avancé
