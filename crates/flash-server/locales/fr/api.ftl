### The open core's words in French, both variants: api.

## Toute requête

api-unauthorized = Connexion requise
api-invalid-credentials = Ces informations ne correspondent pas
api-forbidden = Action non autorisée
# accept: length: English names the thing through $what, which counts as nothing; French writes each thing's name in its variant ("Paquet introuvable").
api-not-found =
    { $what ->
        [deck] Paquet introuvable
        [card] Carte introuvable
        [note] Note introuvable
        [media] Fichier introuvable
        [session] Session introuvable
        [study_session] Séance introuvable
        [passkey] Clé d’accès introuvable
        [device] Appareil introuvable
       *[other] Introuvable
    }
# accept: length: English names the feature through $what, which counts as nothing; French writes it in each variant ("Les clés d’accès ne sont pas disponibles").
api-not-available =
    { $what ->
        [passkeys] Les clés d’accès ne sont pas disponibles sur ce serveur
        [reset] La réinitialisation du mot de passe n’est pas disponible sur ce serveur
       *[other] Cette fonction n’est pas disponible sur ce serveur
    }
api-too-large = Ce fichier est trop volumineux
# accept: length: "slow down" has no equally short calm French, so the refusal says why (too many requests) and what to do (wait).
api-rate-limited = Trop de requêtes. Patientez un instant.

## Connexion et paramètres du compte

api-passkey-expired = Le délai pour la clé d’accès a expiré. Recommencez.
api-last-login-method = C’est votre seul moyen de connexion. Ajoutez-en d’abord un autre.
api-current-password = Le mot de passe actuel est requis

## Paquets et fichiers

api-deck-name-long = Le nom du paquet est trop long
api-file-name-long = Le nom du fichier est trop long
store-error-tags-many = Une carte peut avoir au plus { $max } étiquettes
store-error-tag-long = Une étiquette peut faire au plus { $max } octets
card-limit-reached = Limite de cartes atteinte : { $current } cartes sur { $cap }
heavy-busy-yours = Une autre de vos importations, exportations ou pages de statistiques est en cours. Réessayez dans un instant.
heavy-busy-everyone = Le serveur est occupé par d’autres importations et exportations. Réessayez dans un instant.
