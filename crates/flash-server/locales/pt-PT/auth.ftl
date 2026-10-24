### Portuguese as Portugal words it, where it differs from the shared base (pt): auth.

## Comum às páginas de login

auth-email = Email

## A página de login

login-title = Iniciar sessão - Flash
login-passkey = Iniciar sessão com chave de acesso
login-or-email-password = ou com o seu email e palavra-passe
login-or-password = ou com a sua palavra-passe
login-password-placeholder = Palavra-passe
login-submit = Iniciar sessão
login-forgot = Esqueceu-se da palavra-passe?
login-create-email = Criar conta com email
login-wrong = Email ou palavra-passe incorretos.
login-again = Por segurança, inicie sessão novamente para fazer isto.
login-deleted = A sua conta e todos os respetivos dados foram eliminados.

## Criar uma conta a partir de um convite ou de um link de cadastro

enroll-intro-verified = Email verificado. Agora crie uma chave de acesso para começar a estudar. Face ID, impressão digital ou o PIN do dispositivo. Sem palavra-passe.
enroll-intro = Crie uma chave de acesso e comece a estudar. Face ID, impressão digital ou PIN do dispositivo. Sem palavra-passe.
enroll-create-passkey = Criar a minha chave de acesso
enroll-use-password = Usar uma palavra-passe
enroll-password-placeholder = Palavra-passe (8 caracteres ou mais)
enroll-confirm-placeholder = Confirmar palavra-passe
enroll-create-password = Criar conta com palavra-passe

## Redefinir a senha

reset-title = Repor palavra-passe - Flash
reset-sent-heading = Verifique o seu email
reset-sent-body =
    Se esse endereço tiver uma conta, vai receber um link para repor a palavra-passe.
    Funciona uma vez e expira em 1 hora.
reset-heading = Repor a palavra-passe
reset-intro = Vamos enviar-lhe por email um link para definir uma nova.
reset-send = Enviar link para repor
reset-back = Voltar ao início de sessão
reset-new-title = Nova palavra-passe - Flash
reset-new-heading = Defina uma nova palavra-passe
reset-new-intro = Isto termina a sessão em todos os outros dispositivos.
reset-new-password = Nova palavra-passe
reset-new-save = Guardar palavra-passe

## Problemas ao fazer login ou criar uma conta (uma linha de texto na
## página, ou a página inteira para um link morto)

auth-email-required = Indique o email
auth-email-invalid = Isso não parece um endereço de email
auth-email-taken = Este email já está em uso
auth-password-short = A palavra-passe deve ter pelo menos 8 caracteres
auth-password-long = A palavra-passe é muito longa
auth-passwords-differ = As palavras-passe não coincidem
auth-reset-spent = Este link para repor a palavra-passe é inválido, expirou ou já foi usado.
auth-reset-spent-line = Este link para repor a palavra-passe é inválido, expirou ou já foi usado
auth-no-ceremony = Nenhuma verificação em curso

## Aprovar a conexão de um assistente de IA (OAuth)

consent-wants-html =
    <strong>{ $client }</strong> quer aceder aos seus cartões:
    sessões de estudo, conteúdo dos cartões e a possibilidade de criar e editar cartões.
consent-where-html =
    Se aprovar, o início de sessão será enviado para <strong>{ $host }</strong>.
    O nome acima é escolhido por quem registou a app; o endereço é para onde
    o seu acesso vai realmente.
consent-only-if =
    Só aprove se tiver sido a pessoa que iniciou isto, a partir de uma app em que confia
    (como ao adicionar um conector no Claude, ChatGPT, Grok, Mistral ou Gemini), e se o
    endereço for dessa app.
consent-deny = Recusar
consent-done-title = Ligado - Flash
consent-done-heading = Ligação concluída
consent-done-body-html =
    <strong>{ $client }</strong> já pode aceder aos seus cartões. Volte para lá
    para continuar.
oauth-error-body-html =
    A app que abriu esta página fez um pedido que o Flash não pode aceitar
    (<code>{ $code }</code>). Nada foi autorizado e não houve nenhum
    redirecionamento.
oauth-error-hint =
    Se estava a adicionar um conector, volte à app e
    tente novamente; se o problema se repetir, o conector da app está mal configurado
    e quem o criou vai querer saber o código acima.
