### The open core's words in French, both variants: auth.

## Commun aux pages de connexion

auth-email = E-mail
auth-email-placeholder = vous@exemple.com

## La page de connexion

login-title-fold = Continuer - Flash
login-title = Connexion - Flash
login-tagline-fold = Cartes de révision à répétition espacée
login-tagline = Des cartes qui se planifient toutes seules.
login-other-ways = Autres façons de continuer
login-passkey = Se connecter avec une clé d’accès
login-or-email-password = ou avec votre e-mail et votre mot de passe
login-or-password = ou avec votre mot de passe
login-password-placeholder = Mot de passe
login-submit = Se connecter
login-forgot = Mot de passe oublié ?
login-create-email = Créer un compte avec une adresse e-mail
login-new-here = Première visite ?
login-create = Créer un compte
login-wrong = E-mail ou mot de passe incorrect.
login-again = Pour votre sécurité, reconnectez-vous pour effectuer cette action.
login-deleted = Votre compte et toutes ses données ont été supprimés.

## Créer un compte à partir d’une invitation ou d’un lien d’inscription

enroll-title = Bienvenue - Flash
enroll-heading = Bienvenue, { $name }
enroll-intro-verified = E-mail vérifié. Créez une clé d’accès pour commencer : Face ID, empreinte ou code de l’appareil. Sans mot de passe.
enroll-intro = Créez une clé d’accès pour commencer : Face ID, empreinte ou code de l’appareil. Sans mot de passe.
enroll-create-passkey = Créer ma clé d’accès
enroll-use-password = Utiliser plutôt un mot de passe
enroll-password-placeholder = Mot de passe (8 caractères ou plus)
enroll-confirm-placeholder = Confirmer le mot de passe
enroll-create-password = Créer un compte avec un mot de passe

## Réinitialiser un mot de passe

reset-title = Réinitialiser le mot de passe - Flash
reset-sent-heading = Consultez vos e-mails
# accept: length: "Si un compte correspond à cette adresse" is the natural wording; an address does not "have" an account in French.
reset-sent-body = Si un compte correspond à cette adresse, un lien de réinitialisation arrive.
    Il ne fonctionne qu’une fois et expire dans 1 heure.
reset-heading = Réinitialisez votre mot de passe
reset-intro = Vous recevrez un e-mail avec un lien pour en choisir un nouveau.
reset-send = Envoyer le lien
reset-back = Retour à la connexion
reset-new-title = Nouveau mot de passe - Flash
reset-new-heading = Choisissez un nouveau mot de passe
reset-new-intro = Vos autres appareils seront déconnectés.
reset-new-password = Nouveau mot de passe
reset-new-confirm = Confirmer
reset-new-save = Enregistrer

## Problèmes de connexion ou de création de compte

auth-email-required = adresse e-mail requise
auth-email-invalid = cette adresse e-mail ne semble pas valide
auth-email-taken = adresse e-mail déjà utilisée
auth-password-short = le mot de passe doit contenir au moins 8 caractères
auth-password-long = le mot de passe est trop long
auth-passwords-differ = mots de passe différents
auth-captcha = Effectuez la vérification, puis réessayez.
auth-reset-spent = Ce lien de réinitialisation n’est pas valide, a expiré ou a déjà été utilisé.
auth-reset-spent-line = ce lien de réinitialisation n’est pas valide, a expiré ou a déjà été utilisé
auth-invite-malformed = invitation mal formée
auth-invite-spent = Ce lien d’invitation n’est pas valide, a expiré ou a déjà été utilisé.
auth-invite-spent-line = ce lien d’invitation n’est pas valide, a expiré ou a déjà été utilisé
auth-invite-expired = l’invitation n’est pas valide ou a expiré
auth-no-passkeys = aucune clé d’accès enregistrée
auth-passkey-failed = impossible de vérifier la clé d’accès
auth-passkey-unknown = identifiant inconnu
auth-no-ceremony = aucune connexion en cours

## Autoriser la connexion d’un assistant IA (OAuth)

consent-title = Autoriser - Flash
consent-heading = Autoriser l’accès
consent-wants-html =
    <strong>{ $client }</strong> demande l’accès à vos cartes :
    séances de révision, contenu des cartes, et la possibilité de créer et de modifier des cartes.
consent-where-html =
    Si vous acceptez, la connexion sera envoyée à <strong>{ $host }</strong>.
    Le nom ci-dessus est choisi par la personne qui a enregistré l’application. L’adresse indique
    où votre accès est réellement envoyé.
consent-only-if = N’acceptez que si vous avez lancé cette demande depuis une application de confiance (en ajoutant un connecteur dans Claude, ChatGPT, Grok, Mistral ou Gemini, par exemple) et que l’adresse est la sienne.
consent-approve = Autoriser
consent-deny = Refuser
consent-done-title = Connexion établie - Flash
consent-done-heading = Connexion établie
consent-done-body-html =
    <strong>{ $client }</strong> peut maintenant accéder à vos cartes. Retournez-y
    pour continuer.
consent-done-open = Ouvrir Flash
oauth-error-title = Demande d’autorisation refusée - Flash
oauth-error-heading = Impossible de traiter cette demande
oauth-error-body-html =
    L’application qui vous a envoyé ici a fait une demande que Flash ne peut pas accepter
    (<code>{ $code }</code>). Rien n’a été autorisé et aucune redirection n’a eu lieu.
oauth-error-hint =
    Si vous ajoutiez un connecteur, retournez dans l’application et réessayez.
    Si le problème persiste, le connecteur de l’application est mal configuré
    et son éditeur voudra connaître le code ci-dessus.
oauth-error-back = Retour à Flash
