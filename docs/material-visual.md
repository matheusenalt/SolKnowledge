# Material visual e anexos

Topologias estruturadas apresentam os equipamentos declarados, seus tipos/IPs e
os enlaces, com zoom, arraste, legenda e tabela de conexões. O posicionamento é
automático e não indica distância física. IDs são únicos; extremos inexistentes
são recusados. Limites: 500 nós e 1000 enlaces por bloco. Blocos físicos e
lógicos podem coexistir e continuar pesquisáveis quando ocultos pelo perfil.

Mermaid é uma dependência local com `securityLevel: strict` e rótulos SVG. O
aplicativo recusa diretivas de configuração, HTML, links, imagens externas e
estilos fornecidos pelo conteúdo. A saída passa por DOMPurify e uma segunda
remoção de atributos ativos. CSS do diagrama vem da folha local do aplicativo;
estilos inline gerados pela biblioteca são descartados. A CSP não foi relaxada
para permitir esses estilos. Não há CDN nem carregamento de fontes remotas.

## Usar

Na biblioteca/módulo, **Importar conhecimento JSON/ZIP** abre o fluxo geral de
prévia e confirmação. Atualizações parciais, ZIP, backup e restauração estão
descritos em [importação e backup](./importacao-e-backup.md). O exemplo de
topologia continua sendo fixture, nunca seed automático.

Em um registro, **Adicionar anexo** copia o arquivo selecionado para o diretório
gerenciado. O ID é criado pelo aplicativo; o Rust confere nome, limite de 50 MiB,
formato aceito e SHA-256. Não há campo de caminho externo. O bloco recebe o ID
do asset e a legenda. Alterações concorrentes de registro são recusadas pela
revisão. Se a gravação do bloco falhar, o asset já copiado permanece preservado;
o backup completo inclui também esses assets cadastrados, preservando os bytes.

PNG/JPEG são decodificados com limites de memória/dimensões; extensão e conteúdo
devem coincidir. SVG usa uma lista conservadora de elementos/atributos de desenho:
scripts, eventos, HTML, links, imagens, estilos, DTD e referências remotas são
recusados. SVGs que dependam desses recursos devem ser exportados como PNG.

PDF tem assinatura/terminador verificados no backend e interpretação estrutural
pelo PDF.js local ao abrir. **Abrir PDF local** é explícito. Somente páginas são
desenhadas em canvas com controles anterior/próxima. Não existem camada de links,
formulários, scripts de documento, anexos executáveis ou abertura automática em
aplicativo externo. Fontes do sistema, Wasm e decodificadores opcionais de rede
estão desabilitados; PDFs com recursos não suportados podem falhar na exibição.

`.drawio` precisa ser XML com raiz mxfile/mxGraphModel; seu conteúdo original é
preservado, sem renderização nativa. `.pkt` é preservado como binário opaco
`application/octet-stream`: **não há certificação de formato interno Packet
Tracer**, editor ou simulador. Tamanho, ID e hash são verificados; não é aberto
nem executado. A extensão sozinha não prova que seja um projeto válido.

No bloco de anexo, **Associar exportação de prévia** aceita PNG/JPEG/SVG/PDF e
salva `preview_asset_id`; o original mantém seu `asset_id`. Somente a exportação
associada é visualizada. O editor JSON também aceita esse campo para anexos.
Os IDs/referências são validados no backend.

## Armazenamento e busca

Os bytes ficam em `assets/<id>-<sha256>`, ao lado do SQLite no diretório de dados
do usuário. Uma gravação inicial usa arquivo staging exclusivo, flush e rename.
O nome original é metadado, nunca caminho de destino. IDs/metadados existentes
são imutáveis; modificações exigem novo ID. Cada leitura reconfere tamanho, hash,
formato e rejeita links simbólicos. Falhas não apagam conteúdo existente. Não há
acesso genérico ao disco nem protocolo de assets habilitado: IPC devolve bytes
somente por ID cadastrado. Imagens usam blob local; PDFs usam canvas/worker local.

A migração 3 preserva dados e recompõe o FTS com texto dos blocos, nomes de anexos
e prévias. Rótulos, tipos de equipamento, IPs, portas, meios, legendas e palavras
chave são pesquisáveis sem OCR. Resultados visuais têm ícone, área/módulo e trecho.

## Validação executada

Testes Rust importam o arquivo original de exemplo numa base temporária depois
de confirmar que o settings criou `redes` e `redes-topologias`. Conferem blocos,
nós/enlaces, buscas Ethernet UTP/switch/ligação de computadores, idempotência,
rollback de inválidos, SVG ativo, formatos reais PNG/JPEG, originais drawio,
reabertura de anexos e adulteração de hash. A base pessoal não recebe fixtures.

Testes no navegador renderizam Mermaid, tabela e PDF real de uma página em canvas,
além das verificações anteriores de texto, opções e perfis. Foram repetidos com
a CSP de produção aplicada: os estilos inline temporários do Mermaid geram
bloqueios no console, mas a saída sanitizada e o PDF passam. O teste de renderer
não substitui o fluxo de cliques/IPC na janela Tauri; essa validação desktop
integral continua pendente. Build do frontend e verificações Rust são separados
dessa limitação. Não foi testado um arquivo proprietário Packet Tracer real.
