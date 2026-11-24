# Importação, exportação e backup

## Operações da interface

A tela **Importação e backups** separa configuração, conhecimento e restauração.
A escolha de arquivos usa diálogo nativo. O frontend recebe um identificador de
prévia; caminhos e bytes de ZIP não são enviados pela interface como arrays JSON.
Há somente uma seleção pendente por instância. Cancelar libera a seleção e não
modifica a base; depois de confirmar, a operação termina antes de aceitar cancelamento.

- **Importar configuração JSON**: `type: settings`, schema v1/v2. Aparência,
  preferências, perfis, áreas e módulos; não incorpora registros de conhecimento.
- **Importar conhecimento JSON/ZIP**: `type: knowledge_update`, v1/v2 e package_id.
  Novos pacotes com perfis, blocos ou anexos usam v2. JSON textual não carrega
  bytes/base64; anexos precisam de ZIP com manifesto.
- **Restaurar backup completo**: somente o ZIP de backup descrito abaixo. Não é
  atualização incremental. Exige digitar **RESTAURAR**, após conferir contagens.
- **Exportar configuração**, **Exportar backup completo** e **Salvar cópia**:
  diálogo nativo para escolher o destino, gravação temporária e publicação do
  arquivo completo. Não é permitido exportar sobre o diretório interno de dados.
- No registro: **Exportar registro JSON** mantém os IDs e referências; para
  transportar os anexos junto, use **Exportar registro com anexos**. O módulo e
  perfil referenciados precisam existir no destino (importe a configuração antes).
- A tela apresenta um modelo JSON copiável. Troque package_id quando mudar o
  conteúdo; a coleção vazia do modelo não inventa conhecimento nem progresso.

O nome da biblioteca, tema/cores hexadecimais e preferência de mostrar todas as
opções são lidos da configuração persistida. Cores fornecidas são independentes
do tema; valores omitidos permanecem. A interface continua em português. O campo
logo é preservado como metadado; a logo oficial local continua sendo utilizada,
sem abrir caminhos ou recursos externos. Backup pré-importação/pré-restauração
é obrigatório mesmo se a preferência automatic_backup for false.

## Validação e prévia

JSON é UTF-8 estrito: rejeita comentários, dados adicionais e chaves duplicadas.
Os schemas locais validam campos, tipos, enums e listas. Campos desconhecidos
são recusados. A extensão de bloco `preview_asset_id`, introduzida no visualizador,
é aceita para anexos, mantendo intactos os schemas originais do kit.

A prévia ensaia a operação numa cópia SQLite em memória criada pela API de backup.
Valida IDs únicos, campos finais após fusão, referências, hierarquia completa,
mesma área do pai, ciclos, blocos, tipos imutáveis e anexos. A lista mostra os
valores **anterior/novo**, com contagens de entidades novas, atualizadas, iguais
e conflitos. Pacotes inválidos ou com conflitos impeditivos são recusados com
erro, sem confirmação/aplicação habilitada. As contagens incluem entidades e
blocos, não apenas registros; valores opcionais padrão podem ser explicitados.

Toda mudança em um campo já existente precisa de confirmação específica. Notas
pessoais e progresso fornecidos também são sinalizados. Não há confirmação
pré-marcada nem avaliação inferida. O backend verifica as confirmações e vincula
a prévia ao hash do pacote, mudanças e estado lógico da base. Alterações na base
invalidam a prévia; selecione novamente o arquivo para revisar o estado atual.

Na aplicação: validar novamente → backup completo consistente → staging privado
→ transação IMMEDIATE para dados, FTS e histórico → publicação de assets sem
sobrescrever arquivos existentes → commit. Falhas fazem rollback do SQLite e
removem as publicações pertencentes à operação, preservando arquivos anteriores.
O backup anterior fica disponível na lista local mesmo se a aplicação falhar.

## Regras de fusão

- Omitido permanece. null só é aceito em parent_id; texto vazio explícito pode
  limpar texto após confirmação. Importar nunca exclui registros/blocos omitidos.
- keywords, sources e related_entries são união estável sem duplicatas.
- options são upsert por ID local, preservando campos e opções omitidos.
- blocks são upsert por ID local. Tipo é imutável. Existentes mantêm posição;
  novos são anexados na ordem do pacote. Arrays internos fornecidos substituem
  o array completo, com validação da forma final. Reordenação/exclusão são ações
  de editor; listas de componentes podem ser explicitamente vazias.
- Perfis são resolvidos antes de áreas/módulos. A hierarquia resultante é validada
  e aplicada em ordem topológica; uma inversão válida de arestas não é rejeitada
  por um ciclo intermediário dentro da transação.
- Relações related_entries podem continuar pendentes; module_id, parent_id,
  profile_id e asset_id exigem referências existentes no estado resultante.

## Hash e compatibilidade

Novos pacotes usam SHA-256 do JSON canônico, com chaves de objetos ordenadas e
ordem de arrays preservada. O manifesto ZIP contém os hashes dos anexos, conferidos
contra os bytes, portanto o hash do pacote inclui a identidade dos arquivos.
Mudanças de espaços, ordem de chaves ou compressão não alteram essa identidade.
Mesmo package_id/hash é no-op; mesmo ID com conteúdo diferente é conflito.

O schema original de settings não tem package_id. Sua identidade é derivada do
hash canônico (`settings-<hash>`), tornando a repetição idempotente. Reaplicar um
pacote já registrado não desfaz edições posteriores. Os hashes do bootstrap
conhecido são normalizados na migração 4 sem reimportar conteúdo pessoal.
Históricos antigos de pacotes arbitrários, sem manifesto original guardado,
continuam marcados como raw: exigem os mesmos bytes originais ou novo package_id;
não se adivinha seu conteúdo a partir da base já editada.

## ZIP multimídia

Raiz `manifest.json` com knowledge_update v2 e array assets. Cada asset contém
id, filename simples, sha256 minúsculo e media_type. Os bytes ficam somente em
`assets/<filename>`. O bloco referencia asset_id; preview_asset_id pode associar
uma exportação ao original. Limites: 50 MiB por membro, 250 MiB total
(descompactado e transporte), 1000 membros, JSON/manifesto até 8 MiB.

Rejeita caminhos absolutos, separadores em nomes simples, traversal, symlinks,
membros especiais, duplicatas (inclusive caixa ASCII), arquivos ausentes/extra,
hash/tamanho divergente e formatos ativos. Uma entrada vazia `assets/` é
permitida; outros diretórios não são extraídos. Metadados são examinados antes
da descompressão; os bytes também são limitados e verificados pela biblioteca ZIP.
Nenhum nome do arquivo ZIP é usado diretamente como caminho de extração.

PNG/JPEG são decodificados; SVG é conservador; drawio deve ser XML compatível;
PDF tem assinatura/terminador e validação no visualizador. `.pkt` permanece
binário opaco, sem certificação do formato proprietário nem execução. Consulte
[material visual](./material-visual.md) para os limites dos visualizadores.

Assets existentes são imutáveis por ID/hash/tipo/tamanho. Um nome de transporte
diferente, com bytes idênticos, preserva o nome já cadastrado. A exportação
individual usa nomes por ID para evitar colisões entre anexos homônimos. Legendas
e nomes da base original continuam no backup completo.

O staging fica no mesmo volume dos assets, em diretório exclusivo. A publicação
usa hard link sem sobrescrita, e o staging é removido após commit. Na reabertura,
a recuperação só remove publicação sem referência quando comprova que é o mesmo
arquivo do staging. Assets registrados ou referenciados permanecem; diretórios
com arquivos não reconhecidos são preservados para inspeção. Arquivos antigos
que deixem de ser usados após uma restauração não são apagados automaticamente.

## Formato de backup completo — versão 1

O ZIP tem exatamente:

```text
backup-manifest.json
knowledge.sqlite3
assets/<id>-<sha256>       # todos os assets cadastrados, inclusive não associados
```

O manifesto usa `type: backup`, `format_version: 1`, `schema_version: 4`, counts
por tabela e files com path, size e sha256 de **cada arquivo de dados**. O manifesto
não contém hash de si próprio, para evitar dependência circular. As entidades,
configuração, perfis, blocos, favoritos, vocabulário, históricos e revisões estão
no snapshot SQLite. SHA-256 verifica integridade; não é assinatura/autenticidade.

O snapshot é produzido pela API SQLite, capturando corretamente dados em WAL.
Não é cópia do arquivo de banco aberto. Os anexos são verificados por hash antes
de entrar no ZIP. A exportação aborta se ultrapassar os limites, inclusive se o
snapshot exceder 50 MiB. Não é produzido um backup parcial silencioso.

Backups automáticos ficam no diretório de dados do usuário em `backups/<id>.zip`,
fora do repo e da instalação. A lista permite salvar cópias em outro destino;
não há remoção automática nem envio à rede. Não inclua credenciais ou informações
de terceiros nos registros destinados a intercâmbio. Backups preservam a base
inteira; a aplicação não inventa nem remove conteúdo pessoal durante a exportação.

A restauração valida nomes, limites, hashes, contagens, SQLite quick_check,
foreign_key_check, histórico de migrações e payloads. O schema SQL deve ser
exatamente o schema seguro deste aplicativo; triggers/views adicionais são
recusados. O snapshot é aberto em staging com trusted_schema desabilitado, sem
carregar extensões. Anexos são conferidos contra os metadados do snapshot.

Depois da prévia e confirmação forte, salva backup completo da base atual,
prepara os anexos e reconstrói o FTS no snapshot validado. A API de backup SQLite
publica a base restaurada em uma transação de destino atômica. Falhas preservam
a base anterior e desfazem assets novos. SQLite e anexos permanecem nas pastas
gerenciadas; não há restauração por caminho fornecido pelo frontend.
Este formato inicial suporta schema 4; backups de schemas futuros são recusados.

## Cobertura real

Testes usam bases temporárias e o fixture original de topologia; não importam
exemplos na base pessoal. Cobrem patch, omissões, opções/blocos, confirmações,
idempotência canônica, versão legada, migração do histórico bruto, ciclos e
inversão válida de hierarquia, limites, traversal, links, duplicatas, hash,
arquivos extras, rollback e recuperação de publicação interrompida.

O fluxo real de ZIP com PNG e original drawio é exportado em backup e restaurado
em outra base limpa; hashes, estado lógico completo, favoritos e buscas são
conferidos. Backup anterior também é restaurado. Snapshot com trigger injetada,
contagens alteradas ou arquivo adulterado é recusado antes de alterar a base.
No navegador real, a prévia é testada como texto inerte e as confirmações de nota
e progresso são independentes. Esses testes não substituem a verificação integral
dos diálogos nativos e cliques IPC na janela desktop, ainda pendente.
