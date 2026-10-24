### The open core's words in Portuguese, both variants: settings.

settings-title = Configurações - Flash
settings-heading = Configurações
settings-save = Salvar
settings-saved = Salvo

## Nome

settings-name = Nome
settings-name-error = Informe um nome (até 80 caracteres)
settings-name-hint = Como o Flash e sua IA chamam você.

## Os cartões da pessoa

settings-cards = Seus cartões
settings-cards-count =
    { $count ->
        [one] { $count } cartão.
        [many] { $count } de cartões.
       *[other] { $count } cartões.
    }
settings-cards-export = Seus cartões são sempre seus, e a exportação nunca é bloqueada:
settings-export-apkg = .apkg do Anki
settings-export-csv = CSV

## Limites diários

settings-limits = Limites diários
settings-limits-hint = Padrões da conta. Qualquer baralho pode mudá-los na própria página.
settings-limit-new = Cartões novos / dia
settings-limit-reviews = Revisões máximas / dia
settings-due = Os cartões ficam disponíveis
settings-due-day = Durante o dia todo
settings-due-exact = Na hora exata
settings-due-hint = Durante o dia todo: tudo o que é para revisar hoje já está pronto quando você se senta para estudar, e uma sessão resolve o dia. Na hora exata: cada cartão espera até a hora para a qual foi agendado.

## Avaliação

settings-grading = Como o assistente avalia
settings-grading-hint = Como seu assistente de IA informa as avaliações quando você estuda com ele.
settings-grading-silent = Silenciosa: avalia sem dizer e mantém a conversa fluindo
settings-grading-announce = Anunciada: diz a avaliação; você pode mudá-la
settings-grading-self = Autoavaliação: pede que você avalie cada cartão

## Senha

settings-password = Senha
settings-password-error = Não funcionou. Confira a senha atual e se as novas coincidem (8 caracteres ou mais)
settings-password-add = Adicione uma senha para fazer login sem chave de acesso.
settings-password-add-external = Adicione uma senha para fazer login sem chave de acesso nem conta vinculada.
settings-password-new = Senha (8 caracteres ou mais)
settings-password-confirm = Confirmar senha
settings-password-set = Definir senha
settings-password-current = Senha atual
settings-password-new-change = Nova senha (8 caracteres ou mais)
settings-password-confirm-change = Confirmar nova senha
settings-password-change = Alterar senha
settings-password-change-hint = Alterar a senha encerra a sessão nos seus outros dispositivos.
settings-password-remove = Remover senha
settings-password-tip =
    Dica: chaves de acesso são mais rápidas e resistem a phishing. Você pode adicionar uma
    no próximo convite para cadastrar um dispositivo.

## Chaves de acesso

settings-passkeys = Chaves de acesso
settings-passkeys-none = Nenhuma configurada.
settings-passkey-added = adicionada em { $date }

## Dispositivos

settings-devices = Dispositivos
settings-devices-signed-out = Sessão encerrada
settings-devices-hint = Todos os navegadores e celulares com login nesta conta. Encerrar a sessão de um deles termina a sessão lá.
settings-device-sign-out = Sair
settings-devices-sign-out-others = Sair de todos os outros
settings-device-browser = { $browser } no { $os }
settings-device-browser-unknown = Navegador
settings-device-phone-unknown = Celular
settings-device-this = Este navegador
settings-device-used = Último uso { $ago }
settings-device-signed-in = Login { $ago }
settings-device-app-used = App do Flash · último uso { $ago }
settings-device-app-signed-in = App do Flash · login { $ago }
settings-device-via-link = { $detail } · login por um link de e-mail

## Apps conectados (assistentes de IA e o app para celular)

settings-apps = Apps conectados
settings-apps-disconnected = Desconectado
settings-apps-hint = Tudo o que pode ler e alterar seus cartões em seu nome. Desconectar encerra a sessão dele em todos os lugares; ele pode pedir para se conectar de novo.
settings-apps-none = Nada conectado.
settings-app-devices =
    { $count ->
        [one] { $count } dispositivo
        [many] { $count } de dispositivos
       *[other] { $count } dispositivos
    }
settings-app-since = desde { $date }
settings-app-last-used = último uso { $ago }
settings-app-disconnect = Desconectar
settings-app-flash = App do Flash
settings-app-unnamed = Conector

## Convidar alguém (só administradores)

settings-invite = Convidar alguém (admin)
settings-invite-hint =
    Cria um link de inscrição de uso único, válido por 7 dias.
    Quem abrir escolhe uma chave de acesso ou uma senha e já entra.
settings-invite-name = Nome da pessoa
settings-invite-create = Criar convite
settings-invite-send = Envie este link para a pessoa (mostrado uma vez):
settings-invite-copy = Copiar link

## Aparência

settings-appearance = Aparência
settings-appearance-hint = O escuro é o padrão.
theme-dark = Escuro
theme-light = Claro
theme-clay = Argila
theme-mocha = Moca
theme-taupe = Taupe

## Excluir a conta

settings-delete = Excluir conta
settings-delete-word = EXCLUIR
settings-delete-error-confirm = Digite { $word } exatamente para confirmar
settings-delete-error-password = Essa senha está incorreta
settings-delete-error-admin = Você é o único admin; torne outra pessoa admin primeiro
settings-delete-hint =
    Remove para sempre sua conta e tudo o que há nela: baralhos, cartões, histórico de
    revisões, mídia, chaves de acesso, sessões e qualquer IA conectada. Esta ação é irreversível.
settings-delete-copy = Quer uma cópia antes?
settings-delete-type = Digite { $word } para confirmar
settings-delete-type-tip = Digite { $word } em maiúsculas
settings-delete-submit = Excluir minha conta

## Erros

invite-name-required = Informe um nome
