### The open core's words in Quebec French: api, where Quebec words them otherwise.

# accept: length: English names the thing through $what, which counts as nothing; French writes each thing's name in its variant ("Paquet introuvable").
api-not-found =
    { $what ->
        [deck] Paquet introuvable
        [card] Carte introuvable
        [note] Note introuvable
        [media] Fichier introuvable
        [session] Session introuvable
        [study_session] Séance introuvable
        [passkey] Clé d’identification introuvable
        [device] Appareil introuvable
       *[other] Introuvable
    }
# accept: length: English names the feature through $what, which counts as nothing; French writes it in each variant ("Les clés d’identification ne sont pas disponibles").
api-not-available =
    { $what ->
        [passkeys] Les clés d’identification ne sont pas disponibles sur ce serveur
        [reset] La réinitialisation du mot de passe n’est pas disponible sur ce serveur
       *[other] Cette fonction n’est pas disponible sur ce serveur
    }
api-passkey-expired = Le délai pour la clé d’identification a expiré. Recommencez.
