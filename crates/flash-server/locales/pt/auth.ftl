### The open core's words in Portuguese, both variants: auth.

## Comum às páginas de login

auth-email = E-mail
auth-email-placeholder = nome@exemplo.com

## A página de login

login-title-fold = Continuar - Flash
login-title = Fazer login - Flash
login-tagline-fold = Cartões com repetição espaçada
login-tagline = Cartões que agendam as próprias revisões.
login-other-ways = Outras formas de continuar
login-passkey = Fazer login com chave de acesso
login-or-email-password = ou com seu e-mail e senha
login-or-password = ou com sua senha
login-password-placeholder = Senha
login-submit = Fazer login
login-forgot = Esqueceu a senha?
login-create-email = Criar conta com e-mail
login-new-here = Primeira vez aqui?
login-create = Criar conta
login-wrong = E-mail ou senha incorretos.
login-again = Por segurança, faça login de novo para fazer isso.
login-deleted = Sua conta e todos os dados dela foram excluídos.

## Criar uma conta a partir de um convite ou de um link de cadastro

enroll-title = Boas-vindas - Flash
enroll-heading = Boas-vindas, { $name }
enroll-intro-verified = E-mail verificado. Agora crie uma chave de acesso para começar a estudar. Face ID, impressão digital ou o PIN do dispositivo. Sem senha.
enroll-intro = Crie uma chave de acesso para começar a estudar. Face ID, impressão digital ou o PIN do dispositivo. Sem senha.
enroll-create-passkey = Criar minha chave de acesso
enroll-use-password = Usar uma senha
enroll-password-placeholder = Senha (8 caracteres ou mais)
enroll-confirm-placeholder = Confirmar senha
enroll-create-password = Criar conta com senha

## Redefinir a senha

reset-title = Redefinir senha - Flash
reset-sent-heading = Confira seu e-mail
reset-sent-body =
    Se esse endereço tiver uma conta, um link para redefinir a senha está a caminho.
    Ele funciona uma vez e expira em 1 hora.
reset-heading = Redefina sua senha
reset-intro = Vamos enviar por e-mail um link para criar uma nova.
reset-send = Enviar link de redefinição
reset-back = Voltar ao login
reset-new-title = Nova senha - Flash
reset-new-heading = Crie uma nova senha
reset-new-intro = Isso encerra sua sessão em todos os outros lugares.
reset-new-password = Nova senha
reset-new-confirm = Confirmar
reset-new-save = Salvar senha

## Problemas ao fazer login ou criar uma conta (uma linha de texto na
## página, ou a página inteira para um link morto)

auth-email-required = Informe o e-mail
auth-email-invalid = Isso não parece um endereço de e-mail
auth-email-taken = Este e-mail já está em uso
auth-password-short = A senha deve ter pelo menos 8 caracteres
auth-password-long = A senha é muito longa
auth-passwords-differ = As senhas não coincidem
auth-captcha = Conclua a verificação e tente novamente.
auth-reset-spent = Este link de redefinição é inválido, expirou ou já foi usado.
auth-reset-spent-line = Este link de redefinição é inválido, expirou ou já foi usado
auth-invite-malformed = Convite com formato inválido
auth-invite-spent = Este link de convite é inválido, expirou ou já foi usado.
auth-invite-spent-line = Este link de convite é inválido, expirou ou já foi usado
auth-invite-expired = O convite é inválido ou expirou
auth-no-passkeys = Nenhuma chave de acesso configurada
auth-passkey-failed = Não foi possível verificar a chave de acesso
auth-passkey-unknown = Credencial desconhecida
auth-no-ceremony = Nenhuma verificação em andamento

## Aprovar a conexão de um assistente de IA (OAuth)

consent-title = Autorizar - Flash
consent-heading = Autorizar acesso
consent-wants-html =
    <strong>{ $client }</strong> quer acessar seus cartões:
    sessões de estudo, conteúdo dos cartões e a capacidade de criar e editar cartões.
consent-where-html =
    Se você aprovar, o login será enviado para <strong>{ $host }</strong>.
    O nome acima é escolhido por quem registrou o app; o endereço é para onde
    seu acesso vai de fato.
consent-only-if =
    Só aprove se foi você quem começou isto, a partir de um app em que confia (como
    ao adicionar um conector no Claude, ChatGPT, Grok, Mistral ou Gemini), e se o
    endereço for desse app.
consent-approve = Aprovar
consent-deny = Negar
consent-done-title = Conectado - Flash
consent-done-heading = Conexão concluída
consent-done-body-html =
    <strong>{ $client }</strong> já pode acessar seus cartões. Volte para lá
    para continuar.
consent-done-open = Abrir o Flash
oauth-error-title = Pedido de autorização recusado - Flash
oauth-error-heading = Não é possível concluir esse pedido
oauth-error-body-html =
    O app que abriu esta página fez um pedido que o Flash não pode aceitar
    (<code>{ $code }</code>). Nada foi autorizado e não houve nenhum
    redirecionamento.
oauth-error-hint =
    Se você estava adicionando um conector, volte para o app e
    tente novamente; se o problema se repetir, o conector do app está mal configurado
    e quem o criou vai querer saber o código acima.
oauth-error-back = Voltar ao Flash
