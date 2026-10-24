### Portuguese as Portugal words it, where it differs from the shared base (pt): api.

## Qualquer pedido

api-unauthorized = Inicie sessão para continuar
# accept: length: English names the missing thing through $what, which counts as nothing; Portuguese writes the noun in each variant ("Sessão de estudo não encontrada").
api-not-found =
    { $what ->
        [deck] Baralho não encontrado
        [card] Cartão não encontrado
        [note] Nota não encontrada
        [media] Ficheiro não encontrado
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
        [reset] Não é possível repor a palavra-passe neste servidor
       *[other] Isto não está disponível neste servidor
    }

## Login e configurações da conta

api-last-login-method = Esta é a única forma de iniciar sessão; adicione outra primeiro
api-current-password = Indique a palavra-passe atual

## Baralhos e ficheiros

api-file-name-long = O nome do ficheiro é muito longo
heavy-busy-yours = Outra importação, exportação ou página de estatísticas sua ainda está em curso; tente daqui a pouco
