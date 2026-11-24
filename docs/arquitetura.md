# Arquitetura

## Fronteiras

A WebView carrega HTML/CSS/TypeScript empacotados pelo Vite. Os comandos IPC
tipados consultam metadados, contagens, índice de registros, detalhe, pesquisa e
vocabulário, catálogo, perfis, blocos, favoritos e histórico. As edições de
registros/blocos usam revisão esperada e confirmação de alteração de texto
pessoal. A migração 2 acrescenta revisões de módulos/áreas e histórico de consultas.
O frontend não escolhe caminhos nem envia SQL. Rust mantém uma conexão SQLite
protegida por Mutex e seleciona o diretório de dados usando a API do Tauri.

`rusqlite` usa SQLite compilado com o aplicativo, FTS5 e suporte à API de backup
(usada para snapshots e restauração completa). A migração inicial e o seed são
transacionais. Triggers sincronizam textos de registros e blocos no índice FTS
dentro da mesma transação. Veja [persistência e busca](./persistencia-e-busca.md).

## Recursos e contratos

Configuração e seed originais ficam em `src-tauri/resources/`, sem alterações.
São também incorporados ao binário via include_str para o bootstrap, sem depender
do diretório de trabalho. São somente leitura, não armazenamento pessoal. Imagens da marca
ficam em `public/assets/` e `src-tauri/icons/`. Os exemplos JSON são fictícios e
não são lidos automaticamente. Áreas e módulos não estão fixados em telas.

## Política de execução

A produção não depende de rede. O desenvolvimento usa servidor Vite e WebSocket
em loopback. A CSP bloqueia scripts externos, frames, objetos e formulários;
estilos de produção são locais, sem `unsafe-inline`. O protocolo de assets está
desabilitado, sem escopo de acesso ao disco. Capabilities não concedem permissões
de plugins; não há plugins de shell, rede, SQL ou arquivos.

Os campos `$schema` de arquivos de configuração são metadados de ferramentas,
não chamadas de rede da aplicação. O runtime não usa fetch, XHR, fontes remotas,
CDN ou mecanismos de atualização automática. Dependências transitivas de build
podem precisar de internet na preparação do ambiente.

## Conteúdo não confiável

Markdown continua como texto. Mermaid usa modo strict e sanitização; topologias,
imagens e PDF têm visualizadores locais. Anexos gerenciados por ID/hash ficam no
diretório de dados do usuário e são acessados somente por IPC validado. Nada do
catálogo pode ser executado. Veja [material visual](./material-visual.md).
Os textos das referências e fontes são inseridos por textContent, sem HTML
ativo ou links externos. A navegação usa links internos por IDs; copiar usa
somente escrita de texto no clipboard mediante ação do usuário. Termos de consulta são parametrizados e tokens FTS são
citados pelo backend, sem aceitar operadores FTS arbitrários do usuário.

O frontend se divide em roteamento/navegação, editores, registro de renderers,
vocabulário, helpers DOM e contratos IPC. O backend separa persistência,
pesquisa, workspace/perfis e validação/patch de blocos. Não existe código
executável importado por perfil. Veja [perfis e blocos](./perfis-e-blocos.md).

Importação/exportação têm módulos próprios para JSON estrito/ZIP, ensaio e fusão,
backup/restauração e seleção nativa com ticket. O frontend não fornece caminhos
arbitrários ao backend. Veja [importação e backup](./importacao-e-backup.md).
