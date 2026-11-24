# SOL Knowledge

Base pessoal de estudo, consulta e recuperação de entendimento técnico. Criada
para organizar referências de Linux, Redes, Cloud, Programação, Segurança e
Automações em uma biblioteca própria, local e estruturada por perfis de conhecimento.

O aplicativo funciona 100% offline depois de instalado e reúne conteúdo técnico
com notas e vocabulário do usuário. Não executa comandos nem simula equipamentos.
Copiar um comando apenas coloca seu texto na área de transferência.

## Recursos

- Busca instantânea no SQLite/FTS5, com debounce de 200 ms, consulta a partir de
  um caractere, relevância exata/textual/aproximada, sinônimos e vocabulário pessoal.
  Resultados antigos são descartados; Enter e o botão Pesquisar são opcionais.
- Perfis personalizáveis e componentes reordenáveis por módulo. Ocultar um bloco
  preserva o conteúdo e sua pesquisa; “Todos os blocos” permite consultá-lo.
- Áreas, módulos e submódulos, filtros, favoritos, histórico e estado de aprendizado
  explícito. Consultar uma referência não marca aprendizado como concluído.
- Topologias com nós, enlaces e tabela de portas, diagramas Mermaid locais,
  imagens, PDF em canvas e anexos gerenciados por ID e hash.
- Importação incremental JSON/ZIP com prévia, confirmação por campo, validação
  SHA-256, idempotência, backup anterior e transação de dados e índice de pesquisa.
- Exportação de configuração, registros e backup completo com anexos;
  restauração validada com confirmação forte.

O catálogo inicial contém 61 comandos Linux e 205 opções selecionadas. As demais
áreas podem receber conhecimento próprio. Os [exemplos](./examples/) são fictícios
e nunca são importados automaticamente na biblioteca pessoal.

## Arquitetura

```text
Interface TypeScript/CSS — Vite — WebView Tauri 2
                        │ IPC tipado
                        ▼
                Rust: validação e operações
                   │                 │
           SQLite + FTS5       Assets e backups locais
          fonte de verdade    diretório de dados do usuário
```

O frontend não envia SQL nem escolhe caminhos internos. Rust coordena migrações,
pesquisa, perfis, edição, importação e recuperação. O SQLite compilado com o app
mantém conteúdo, revisões e índice FTS5; anexos ficam fora da instalação.
Detalhes na [arquitetura](./docs/arquitetura.md) e em
[persistência e pesquisa](./docs/persistencia-e-busca.md).

## Instalação e uso

No Windows, execute o instalador NSIS gerado para a versão desejada. O aplicativo
usa WebView2 Runtime. Node.js, Cargo e ferramentas C++ são necessários para
compilar, não para usar a aplicação instalada.

Antes de atualizar, exporte um backup completo para um local seguro e feche o app.
Atualize na mesma instalação. Na desinstalação, mantenha desmarcada a opção de
excluir dados do aplicativo se quiser preservar a biblioteca para reinstalar.

1. Abra uma área e navegue pelos módulos na lateral; selecione um registro para
   consultar seus blocos e opções.
2. Digite no campo de pesquisa ou use Ctrl+K. `L`, `ls -lh`, `arquivos escondidos`
   e `sistemctl` exemplificam prefixo, correspondência exata, textual e aproximada.
   A busca exibe até 100 resultados; refine os termos/filtros quando necessário.
3. Em **Meu vocabulário**, associe sua expressão a um registro existente.
4. Em um módulo, use **Configurar perfil** para escolher e reordenar componentes.
5. Para receber conteúdo, abra **Importação e backups**, escolha **Importar
   conhecimento JSON/ZIP**, selecione o pacote e revise os valores anterior/novo.
   Confirme somente os campos que deseja alterar e aplique. Configuração e
   restauração completa têm ações separadas; um backup não é um patch incremental.
6. Use **Exportar backup completo** para salvar uma cópia com anexos fora da pasta
   gerenciada. Restaurar exige conferir a prévia e digitar **RESTAURAR**.

Os formatos estão em [importação, exportação e backup](./docs/importacao-e-backup.md).
Veja também [perfis e blocos](./docs/perfis-e-blocos.md) e
[material visual](./docs/material-visual.md).

### Onde ficam os dados

O Tauri resolve o diretório de dados do usuário:

- Windows: `%APPDATA%/com.solknowledge.desktop/`.
- Linux: `$XDG_DATA_HOME/com.solknowledge.desktop/`, normalmente
  `~/.local/share/com.solknowledge.desktop/`.

Ali ficam `knowledge.sqlite3`, anexos e backups internos. O caminho efetivo aparece
ao passar o mouse sobre as contagens da biblioteca. Nunca apague o banco para
corrigir uma falha. Os arquivos WAL/SHM pertencem ao SQLite aberto; use a exportação
de backup em vez de copiar somente o arquivo de banco em uso.

## Desenvolvimento

Pré-requisitos comuns: Node.js 22.12+ com npm, Rust 1.89+ com Cargo e ferramentas
de compilação da plataforma. Os lockfiles estão versionados.

No Windows, use o target MSVC, Visual Studio Build Tools com ferramentas C++ e
Windows SDK, além do WebView2 Runtime. No Linux, instale as dependências nativas
do Tauri 2: compilador C/C++, WebKitGTK 4.1, OpenSSL, libxdo, appindicator e librsvg,
com os pacotes de desenvolvimento da sua distribuição. Consulte os
[pré-requisitos oficiais do Tauri](https://v2.tauri.app/start/prerequisites/)
para os comandos específicos de instalação.

```sh
git clone https://github.com/matheusenalt/SolKnowledge.git
cd SolKnowledge
npm install
npm run tauri dev
```

`npm run dev` inicia somente a prévia da interface em `127.0.0.1:1420`.
O navegador comum avisa que o backend está indisponível; a biblioteca real é
acessada na janela Tauri. O desenvolvimento usa loopback; a aplicação de produção
carrega os recursos empacotados e não depende desse servidor.

Para gerar o instalador Windows:

```sh
npm run tauri build
```

A configuração atual gera NSIS em `src-tauri/target/release/bundle/nsis/`.
A preparação do ambiente e a compilação podem baixar dependências; isso é
independente do funcionamento offline do aplicativo. Linux é alvo de portabilidade:
exige selecionar um formato compatível na configuração/CLI e ainda não teve
build e instalação certificados neste projeto.

### Verificações

Para a suíte Node, use Node.js 24: os testes do controlador importam TypeScript
diretamente pelo suporte nativo do runtime. O ambiente validado usou Node 24.22.3.

```sh
npm test
npm run check
npx prettier --check src tests index.html vite.config.ts
cargo test --manifest-path src-tauri/Cargo.toml --locked
cargo fmt --manifest-path src-tauri/Cargo.toml -- --check
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets --locked -- -D warnings
npm run tauri build
```

Para aplicar a formatação do frontend, use `npm run format`. Os testes Rust usam
bases em memória e diretórios temporários; fixtures não vão para a base pessoal.
O helper de reinício marcado como ignored é chamado explicitamente pelo teste
controlador em dois processos independentes.

Com Vite ativo, os checks de componente em [tests/browser-smoke.ts](./tests/browser-smoke.ts)
podem ser executados no console do navegador de desenvolvimento:

```js
const t = await import("/tests/browser-smoke.ts");
await t.runRendererChecks();
await t.runVisualChecks();
await t.runPdfCheck();
await t.runTransferChecks();
await t.runHostileContentChecks();
await t.runSearchBrowserChecks();
```

Mantenha a aba visível durante o teste PDF. Esses checks não emulam IPC nem
substituem testes do aplicativo instalado. A versão 0.1.1 teve 33 testes Rust,
15 testes frontend, 20 checks de regressão no navegador e verificações de layout
em cinco larguras aprovados, com instalador Windows gerado e aceite manual do usuário.

## Segurança e limites

Sem login, telemetria, CDN, IA online ou executor de comandos. A CSP de produção
restringe conexões ao IPC local; capabilities não concedem permissões de plugins
de rede, shell, arquivos ou SQL. O protocolo de assets está desabilitado e o
backend lê anexos apenas por ID validado.

Markdown é apresentado como texto inerte. SVG é sanitizado; Mermaid usa modo
strict, sem HTML ativo ou recursos externos. PDF usa worker local e canvas,
sem camada de links/anotações ou execução de scripts. ZIP rejeita caminhos
inseguros, symlinks, duplicatas, arquivos extras e hashes divergentes antes de
publicar anexos via staging. SHA-256 verifica integridade, não autenticidade.

- A busca é textual. O complemento aproximado percorre a base local e não foi
  medido em grandes volumes. A busca limita resultados a 100; a listagem pagina.
- `.pkt` é preservado como binário opaco; não há parser, simulador ou certificação
  do formato Packet Tracer. PDF pode recusar documentos não suportados.
- Backup completo v1 restaura o schema SQLite 4 exato. Limites: 50 MiB por arquivo,
  250 MiB total, 1000 membros e 8 MiB para JSON/manifesto de importação.
- Hashes raw de pacotes legados arbitrários não podem ser reconstruídos sem o
  original. Reaplicar pode exigir os mesmos bytes ou um novo package_id.
- Não foram certificadas individualmente as escalas reais de 100%, 125%, 150% e
  175% do Windows pela automação, nem instalação/desinstalação e todos os diálogos
  nativos de ponta a ponta. O aceite do usuário não transforma esses itens em
  medições automatizadas. Captura de tráfego de todos os fluxos nativos também
  não foi realizada.

## Estrutura e licença

- [src/](./src/): interface, navegação, busca instantânea, renderizadores e IPC.
- [src-tauri/](./src-tauri/): Rust, migrações, capacidades, seed e assets oficiais.
- [schemas/](./schemas/): contratos JSON v1/v2.
- [examples/](./examples/): exemplos fictícios rotulados para teste/intercâmbio.
- [tests/](./tests/): controlador de busca, segurança e checks de componentes.
- [docs/](./docs/): arquitetura, persistência e formatos implementados.

Licenciado sob [GPL v3](./LICENSE).
