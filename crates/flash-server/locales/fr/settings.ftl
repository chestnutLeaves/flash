### The open core's words in French, both variants: settings.

settings-title = Paramètres - Flash
settings-heading = Paramètres
settings-save = Enregistrer
settings-saved = Enregistré

## Nom

settings-name = Nom
settings-name-error = un nom est requis (80 caractères au plus)
settings-name-hint = Le nom que Flash et votre IA utilisent pour vous.

## Les cartes

settings-cards = Vos cartes
settings-cards-count =
    { $count ->
        [one] { $count } carte.
        [many] { $count } cartes.
       *[other] { $count } cartes.
    }
settings-cards-export = Vos cartes vous appartiennent toujours, et l’exportation n’est jamais bloquée :
settings-export-apkg = Anki .apkg
settings-export-csv = CSV

## Limites quotidiennes

settings-limits = Limites quotidiennes
settings-limits-hint = Valeurs par défaut du compte. Chaque paquet peut les remplacer sur sa propre page.
settings-limit-new = Nouvelles cartes par jour
settings-limit-reviews = Maximum de révisions par jour
settings-due = Les cartes sont à réviser
settings-due-day = Pour toute la journée
settings-due-exact = À l’heure exacte prévue
settings-due-hint = Pour toute la journée : tout ce qui est à réviser aujourd’hui est prêt dès le début, et une séance suffit. À l’heure exacte : une carte attend l’heure prévue pour elle.

## Évaluation

settings-grading = Évaluation par votre assistant
settings-grading-hint = Comment votre assistant IA annonce les évaluations quand vous révisez avec lui.
settings-grading-silent = Silencieuse : évalue discrètement et laisse la conversation avancer
settings-grading-announce = Annoncée : dit l’évaluation, que vous pouvez modifier
settings-grading-self = Autoévaluation : vous demande d’évaluer chaque carte

## Mot de passe

settings-password = Mot de passe
settings-password-error = l’opération a échoué. Vérifiez le mot de passe actuel et que les nouveaux correspondent (8 caractères ou plus)
settings-password-add = Ajoutez un mot de passe pour vous connecter sans clé d’accès.
settings-password-add-external = Ajoutez un mot de passe pour vous connecter sans clé d’accès ni compte associé.
settings-password-new = Mot de passe (8 caractères ou plus)
settings-password-confirm = Confirmer le mot de passe
settings-password-set = Définir le mot de passe
settings-password-current = Mot de passe actuel
settings-password-new-change = Nouveau mot de passe (8 caractères ou plus)
settings-password-confirm-change = Confirmer le nouveau mot de passe
settings-password-change = Changer le mot de passe
settings-password-change-hint = Changer votre mot de passe ferme vos sessions sur vos autres appareils.
settings-password-remove = Supprimer le mot de passe
settings-password-tip = Astuce : les clés d’accès sont plus rapides et protègent de l’hameçonnage. Vous pourrez en ajouter une à votre prochaine invitation.

## Clés d’accès

settings-passkeys = Clés d’accès
settings-passkeys-none = Aucune clé enregistrée.
settings-passkey-added = ajoutée le { $date }

## Appareils

settings-devices = Appareils
settings-devices-signed-out = Déconnecté
settings-devices-hint = Tous les navigateurs et téléphones connectés à ce compte. Déconnecter un appareil met fin à sa session.
settings-device-sign-out = Déconnecter
settings-devices-sign-out-others = Déconnecter tous les autres appareils
settings-device-browser = { $browser } sous { $os }
settings-device-browser-unknown = Navigateur
settings-device-phone-unknown = Téléphone
settings-device-this = Ce navigateur
settings-device-used = Dernière utilisation { $ago }
settings-device-signed-in = Connecté { $ago }
settings-device-app-used = Application Flash · utilisée { $ago }
settings-device-app-signed-in = Application Flash · connectée { $ago }
settings-device-via-link = { $detail } · connexion par un lien reçu par e-mail

## Applications connectées

settings-apps = Applications connectées
settings-apps-disconnected = Déconnectée
settings-apps-hint = Tout ce qui peut lire et modifier vos cartes en votre nom. Déconnecter une application ferme toutes ses sessions. Elle pourra demander à se reconnecter.
settings-apps-none = Aucune application connectée.
settings-app-devices =
    { $count ->
        [one] { $count } appareil
        [many] { $count } appareils
       *[other] { $count } appareils
    }
settings-app-since = depuis le { $date }
settings-app-last-used = dernière utilisation { $ago }
settings-app-disconnect = Déconnecter
settings-app-flash = Application Flash
settings-app-unnamed = Connecteur

## Inviter quelqu’un (administrateurs)

settings-invite = Inviter quelqu’un (admin)
settings-invite-hint = Crée un lien d’inscription à usage unique, valable 7 jours.
    La personne qui l’ouvre choisit une clé d’accès ou un mot de passe, et c’est prêt.
settings-invite-name = Son nom
settings-invite-create = Créer l’invitation
settings-invite-send = Envoyez-lui ce lien (affiché une seule fois) :
settings-invite-copy = Copier le lien

## Apparence

settings-appearance = Apparence
settings-appearance-hint = Le thème sombre est le thème par défaut.
theme-dark = Sombre
theme-light = Clair
theme-clay = Argile
theme-mocha = Moka
theme-taupe = Taupe

## Supprimer le compte

settings-delete = Supprimer le compte
settings-delete-word = SUPPRIMER
settings-delete-error-confirm = saisissez { $word } exactement pour confirmer
settings-delete-error-password = ce mot de passe est incorrect
settings-delete-error-admin = ce compte est le seul administrateur. Nommez d’abord une autre personne administrateur
settings-delete-hint =
    Supprime définitivement votre compte et tout son contenu : paquets, cartes, historique
    de révisions, médias, clés d’accès, sessions et toute IA connectée. Cette action est irréversible.
settings-delete-copy = Une copie d’abord ?
settings-delete-type = Saisissez { $word } pour confirmer
settings-delete-type-tip = Saisissez { $word } en majuscules
settings-delete-submit = Supprimer mon compte

## Erreurs

invite-name-required = nom requis
