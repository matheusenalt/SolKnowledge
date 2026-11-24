# Persistência e pesquisa local

## Inicialização

O backend abre `knowledge.sqlite3` no app_data_dir resolvido pelo Tauri, usando
WAL, synchronous=FULL, foreign_keys=ON e busy_timeout de 5 segundos. Uma conexão
protegida por Mutex serializa os comandos IPC. Falha de abertura/migração aborta
a inicialização e preserva a base; não existe reset automático.

`schema_migrations` registra a versão aplicada. A migração inicial roda em uma
transação IMMEDIATE. Bancos com versão mais nova são recusados sem downgrade.
Os recursos de configuração e catálogo são incorporados ao binário e aplicados
em outra transação IMMEDIATE. `import_history` registra IDs, SHA-256 e formato
do hash. A migração 4 normaliza os hashes do bootstrap conhecido e marca hashes
legados. Novos pacotes usam JSON canônico: mesmo ID/hash é no-op; conteúdo
divergente com o mesmo ID é recusado. Veja [importação e backup](./importacao-e-backup.md).

O seed mantém compatibilidade v1 e preserva o JSON original. Após a aplicação,
reiniciar não repõe configurações ou textos alterados pelo usuário. Os 61
comandos/205 opções começam com cobertura selected e estado not_assessed;
nenhum acesso altera o estado de aprendizado. O vocabulário pessoal começa vazio.
Três sinônimos de referência são fornecidos: ficheiros ocultos, espaço livre e
gerenciar serviços. São editáveis e não representam entendimento pessoal.

## Modelo

`profiles`, `areas`, `modules`, `entries`, `blocks`, `assets`, `favorites`,
`search_history`, `visit_history`, `import_history`, `settings`, `synonyms` e
`personal_vocabulary`. A migração 2 adiciona histórico de consultas e revisões de
módulos/áreas. O motor de perfis e os editores são implementados; o visualizador
de topologias e os arquivos de anexos estão descritos em [material visual](./material-visual.md).

Registros guardam payload JSON e campos gerados para título, tipo, nome original,
cobertura e estado. Campos gerados evitam divergência entre o payload e colunas.
Módulos têm parent_id com chave estrangeira na mesma área, validação de raiz e
triggers contra ciclos. Blocos usam chave (registro, ID), posição e tipo imutável;
seus textos são indexados mesmo sem componente visível. Metadados de anexos
incluem ID, nome simples, SHA-256, media_type e tamanho; arquivos gerenciados
são validados e lidos por ID via IPC, com visualização segura mediante ação explícita.

`entries_fts` é sincronizado por triggers de inserção/alteração/remoção de registros
e blocos. A view entry_search_content inclui textos do payload e dos blocos.
O rollback da escrita também reverte a indexação. JSON não é um segundo banco.

## Pesquisa

A interface consulta automaticamente após 200 ms sem nova digitação, a partir
de um caractere. Enter/botão antecipam a consulta sem duplicar uma operação
pendente ou já concluída com o mesmo texto e filtros. O controlador descarta
respostas/erros antigos imediatamente após mudar a entrada, limpar, navegar ou
renovar catálogo/perfil; não substitui o motor SQLite por filtragem no frontend.
O campo permanece montado e mantém o foco. Uma letra alfanumérica usa prefixo
FTS5 citado pelo backend, incluindo letras normalmente tratadas como stop words.
Consultas maiores seguem a ordenação abaixo.

1. Título/nome original exato ou prefixo completo de um exemplo cadastrado
   (`ls -lh` encontra `ls -lh /var/log`). Flags mantêm maiúsculas/minúsculas nessa
   classificação exata.
2. Correspondência FTS5 de todos os tokens significativos, com bm25, ou associação
   explícita de sinônimo/vocabulário. O tokenizer unicode61 remove diacríticos.
3. Complemento aproximado local por Damerau-Levenshtein. Palavras de 4 a 7 letras
   aceitam distância 1; palavras maiores aceitam 2. Termos curtos exigem igualdade.
   Todos os tokens significativos devem ser encontrados. Desempates usam título/ID.

Normalização Unicode e lista curta de palavras funcionais em português permitem
expressões cotidianas. A associação de vocabulário é da expressão completa a um
ID de registro; não é geração de resposta nem IA semântica. O frontend mostra
trechos como texto, sem executar comandos nem abrir fontes.

A pesquisa aceita até 256 caracteres e considera até 24 tokens; resultados são
limitados a 100 na pesquisa da interface e do IPC. A navegação por registros
pagina de 40 em 40, com filtros aplicados no backend antes da paginação/limite.
O complemento aproximado percorre
textos do catálogo local e tem custo linear no tamanho da base; não foi medido
em bases grandes. O histórico registra consultas efetivamente executadas, incluindo
refinamentos por filtro, sem captura por tecla. Favoritos e histórico de consultas
não alteram o estado de aprendizado.

## Edição de expressões

Sinônimos e vocabulário pessoal são operações tipadas: criar, editar e remover
associação a um registro existente. Expressões têm até 128 caracteres, sem
controles. Duplicatas normalizadas para o mesmo destino são recusadas; notas do
registro não são modificadas. Alterar o tipo de associação exige criar outra.

## Verificação

Os testes Rust usam bases em memória e um arquivo temporário isolado, nunca o
banco pessoal. Cobrem contagens/IDs do seed, preservação de notas no bootstrap,
as pesquisas exigidas, acentos, sinônimos, vocabulário, rollback e sincronização
de blocos no FTS, hierarquia, entradas inválidas e reabertura com persistência.

Referências de implementação: [SQLite FTS5](https://sqlite.org/fts5.html) e
[estado no Tauri](https://v2.tauri.app/develop/state-management/).

A migração 3 amplia a busca com nomes de anexos e prévias associadas, sem OCR.

A importação aplica dados, FTS e histórico na mesma transação, após backup
consistente. A restauração valida snapshot/anexos e reconstrói o FTS antes da
publicação atômica pela API SQLite.
