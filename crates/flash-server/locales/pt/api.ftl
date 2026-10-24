### The open core's words in Portuguese, both variants: api.

## Qualquer pedido

api-unauthorized = Faça login para continuar
api-invalid-credentials = Esses dados não coincidem
api-forbidden = Ação não permitida
# accept: length: English names the missing thing through $what, which counts as nothing; Portuguese writes the noun in each variant ("Sessão de estudo não encontrada").
api-not-found =
    { $what ->
        [deck] Baralho não encontrado
        [card] Cartão não encontrado
        [note] Nota não encontrada
        [media] Arquivo não encontrado
        [session] Sessão não encontrada
        [study_session] Sessão de estudo não encontrada
        [passkey] Chave de acesso não encontrada
        [device] Dispositivo não encontrado
       *[other] Não encontrado
    }
# accept: length: English names the feature through $what, which counts as nothing; Portuguese writes it in each variant ("As chaves de acesso não estão disponíveis").
api-not-available =
    { $what ->
        [passkeys] As chaves de acesso não estão disponíveis neste servidor
        [reset] Não é possível redefinir a senha neste servidor
       *[other] Isto não está disponível neste servidor
    }
api-too-large = Esse envio é muito grande
# accept: length: "slow down" has no equally short calm Portuguese ("mais devagar" reads as a scolding), so the refusal says why (too many requests) and what to do (wait).
api-rate-limited = Muitos pedidos seguidos; aguarde um momento

## Login e configurações da conta

api-passkey-expired = O tempo para a chave de acesso acabou; comece de novo
api-last-login-method = Essa é a única forma de fazer login; adicione outra antes
api-current-password = Informe a senha atual

## Baralhos e arquivos

api-deck-name-long = O nome do baralho é muito longo
api-file-name-long = O nome do arquivo é muito longo
store-error-tags-many = Um cartão pode ter no máximo { $max } etiquetas
store-error-tag-long = Uma etiqueta pode ter no máximo { $max } bytes
card-limit-reached = Limite de cartões atingido: { $current } de { $cap } cartões
heavy-busy-yours = Outra importação, exportação ou página de estatísticas sua ainda está em andamento; tente daqui a pouco
heavy-busy-everyone = O servidor está ocupado com outras importações e exportações; tente novamente daqui a pouco
