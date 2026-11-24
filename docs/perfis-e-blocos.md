# Perfis, blocos e workspace

## Herança e apresentação

A ordem é: lista explícita de componentes do módulo, perfil explícito do módulo,
configuração resolvida do módulo pai, configuração/perfil da área e perfil
personalizado (`custom`). A resolução acontece no Rust, com verificação de ciclos.
Os nove perfis iniciais vêm da configuração original. Não há telas fixas por área.

No módulo, “Configurar perfil” permite escolher perfil, habilitar/ocultar tipos e
ordená-los com botões Subir/Descer. Desmarcar “Personalizar componentes” remove a
chave de override; não exporta null. Uma lista explícita vazia é válida e oculta
todos os componentes. A herança é novamente resolvida após salvar.

Essa edição altera apenas o módulo. Não remove, recria ou modifica blocos. Os
textos permanecem indexados pelo FTS. O botão “Todos os blocos” mostra também os
componentes ocultos. Na apresentação do perfil, tipos seguem a ordem dos
componentes e blocos do mesmo tipo preservam sua ordem estável.

Um perfil novo é criado por um objeto JSON com `id`, `name` e
`enabled_components`. A criação é exclusiva: um ID existente não é sobrescrito.
Tipos desconhecidos, componentes duplicados e campos extras são recusados. Esse
editor não é importação geral de configurações ou pacotes de conhecimento.

## Registro de tipos

O motor conhece text, personal_note, command, code, warning, reference, diagram,
topology, image, attachment, equipment, cabling, procedure, troubleshooting e table.
Um perfil pode usar somente esses tipos; JSON não acrescenta renderers executáveis.

Texto, notas, referências e código são exibidos como texto literal. Tabelas e
listas usam elementos DOM locais. Comandos têm aviso contextual, opções principais
e expansão de todas as cadastradas. “Copiar” escreve texto no clipboard e não
executa comandos. Fontes externas continuam como texto, sem abertura automática.

Mermaid/grafo têm visualizadores locais, imagens são sanitizadas e anexos usam
armazenamento gerenciado por ID/hash. PDF exige abertura explícita em canvas.
Veja [material visual](./material-visual.md). Não há simulador ou execução de código.

## Compatibilidade v1

O adaptador de leitura transforma campos legados em blocos virtuais: explicações
técnica/simplificada, entendimento pessoal, anotações, comando/opções e fontes.
IDs virtuais têm prefixo `v1:` e não são gravados em blocks. Seu conteúdo continua
somente no payload original. Editá-los abre os campos de origem do registro.

Blocos novos usam IDs estáveis por registro, sem `:`. O editor aceita o objeto
completo ou um patch com ID e somente os campos alterados. O tipo é imutável.
Campos omitidos permanecem; arrays fornecidos substituem explicitamente a lista
inteira no editor. Reordenação envia todos os IDs reais uma única vez. Não existe
exclusão implícita de blocos ao salvar uma ordem ou trocar perfil.

O patch de bloco e o incremento de revisão do registro são transacionais; as
triggers mantêm o FTS sincronizado. Diagramas, código e outros blocos não envolvidos
no patch são preservados. Nota pessoal existente exige confirmação específica para
troca/limpeza do conteúdo, assim como o campo legado de entendimento pessoal.

## Navegação e edição

Home com áreas e contagens reais; árvore área/módulo/submódulo de profundidade
variável; breadcrumb; pesquisa global com Ctrl+K; filtros de área, módulo (inclui
descendentes), tipo e aprendizado. Navegação por links, Tab/Enter, details nativos
e link “Pular para o conteúdo”. Editores em diálogo aceitam Escape e restauram foco.

Há criação de áreas/módulos/registros, edição de campos e blocos, favoritos,
histórico de consultas/pesquisas e estados not_assessed/studying/understood/applied.
Consultar não infere aprendizado. Referências relacionadas ainda ausentes são
rotuladas como pendentes. A base continua no diretório de dados do usuário.

Revisão esperada impede sobrescrever edições concorrentes. As alterações do editor
são explícitas e não substituem o futuro contrato de importação incremental.
Objetos de registro/bloco têm limite de 1 MiB e JSON de perfil de 64 KiB.

## Testes e limites de verificação

`cargo test --locked --manifest-path src-tauri/Cargo.toml` verifica herança em todas
as camadas, ocultação/reordenação, patch parcial, confirmação pessoal, revisões,
busca de conteúdo oculto, favoritos, histórico e migração aditiva de v1 para v2.
Um teste inicia dois processos independentes do backend de teste: grava numa base
temporária, encerra o processo e reabre a base em outro processo para conferir
perfil, blocos, favoritos e pesquisa. Não utiliza o banco pessoal. O worker é
marcado ignored na execução normal e invocado pelo teste controlador.

[browser-smoke.ts](../tests/browser-smoke.ts) exporta `runRendererChecks()`, que
verifica elementos destacados da página: HTML/código como texto, opções principais/
todas e ordenação sem mutação. Com `npm run dev`, um runner de navegador pode chamar:

```js
await page.goto('http://127.0.0.1:1420/');
const checks = await page.evaluate(async () => {
  const tests = await import('/tests/browser-smoke.ts');
  return tests.runRendererChecks();
});
```

Esses checks não substituem um teste completo de cliques com IPC desktop. No
navegador comum a interface sinaliza backend indisponível, sem fabricar dados. A
validação por cliques na janela desktop ficou limitada pela autorização expirada
da ferramenta de Computer Use nesta sessão. Não foi alegada aprovação desse fluxo.
