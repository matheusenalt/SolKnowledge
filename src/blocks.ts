import { visual } from "./visual";
import type { BlockPayload, BlockView } from "./ipc";
import { node, button, copyButton, componentLabels } from "./dom";

let showAllCommandOptions = false;
export function configureCommandOptions(show: boolean): void {
  showAllCommandOptions = show;
}
function command(v: BlockPayload): HTMLElement {
  const section = node("div");
  const content = v.content ?? "";
  section.append(node("code", content), copyButton(content));
  section.append(
    node(
      "p",
      "Comando de referência. Confira caminhos, permissões e contexto antes de usá-lo. Copiar não executa.",
      "command-warning",
    ),
  );
  const risk: Record<string, string> = {
    read_only: "Operação de leitura; confira se há dados sensíveis na saída.",
    changes_state: "Altera o estado do sistema. Revise o alvo antes de usar.",
    destructive:
      "Pode apagar dados ou causar perda de acesso. Revise com cuidado.",
    context_dependent: "O risco depende do contexto e das opções.",
  };
  if (v.risk)
    section.append(node("p", risk[v.risk] ?? v.risk, "command-warning"));
  const options = v.options ?? [];
  if (!options.length) return section;
  section.append(
    node(
      "p",
      v.coverage === "complete"
        ? "Cobertura completa declarada pelo registro; confira as fontes e a versão."
        : "Opções selecionadas, sem garantia de cobertura exaustiva.",
      "search-help",
    ),
  );
  const list = node("div");
  let all = showAllCommandOptions;
  const toggle = button(`Todas as cadastradas (${options.length})`, () => {
    all = !all;
    render();
  });
  const render = () => {
    list.replaceChildren();
    for (const option of options.filter((o) => all || o.featured)) {
      const card = node("article", "", "option-card");
      card.append(node("code", option.example), copyButton(option.example));
      if (option.nomenclature)
        card.append(node("p", option.nomenclature, "search-help"));
      if (option.description) card.append(node("p", option.description));
      if (option.application)
        card.append(node("p", option.application, "search-help"));
      if (
        /perigo|apag|remov|interromp|destru|perda|pare ssh/i.test(
          `${option.description ?? ""} ${option.application ?? ""}`,
        )
      )
        card.append(
          node(
            "p",
            "Atenção ao impacto desta opção. Leia a descrição e confirme o contexto.",
            "command-warning",
          ),
        );
      list.append(card);
    }
    if (!list.childElementCount)
      list.append(
        node(
          "p",
          "Nenhuma opção marcada como principal. Consulte todas as cadastradas.",
          "search-help",
        ),
      );
    toggle.textContent = all
      ? "Mostrar só principais"
      : `Todas as cadastradas (${options.length})`;
  };
  render();
  section.append(list, toggle);
  return section;
}
function plain(v: BlockPayload): HTMLElement {
  return node("p", v.content ?? "", "block-text");
}
function code(v: BlockPayload): HTMLElement {
  const w = node("div");
  w.append(
    node("small", v.language ?? "texto"),
    node("pre", v.content ?? ""),
    copyButton(v.content ?? ""),
  );
  return w;
}
function items(v: BlockPayload): HTMLElement {
  const w = node("div");
  if (v.content) w.append(plain(v));
  if (v.items) {
    const list = node(v.type === "procedure" ? "ol" : "ul");
    for (const s of v.items) list.append(node("li", s));
    w.append(list);
  }
  return w;
}
function table(v: BlockPayload): HTMLElement {
  const w = node("div", "", "table-scroll");
  const t = node("table");
  const head = node("thead"),
    row = node("tr");
  for (const c of v.columns ?? []) row.append(node("th", c));
  head.append(row);
  const body = node("tbody");
  for (const values of v.rows ?? []) {
    const r = node("tr");
    for (const c of values) r.append(node("td", c));
    body.append(r);
  }
  t.append(head, body);
  w.append(t);
  return w;
}
const registry: Record<string, (v: BlockPayload) => HTMLElement> = {
  text: plain,
  personal_note: plain,
  warning: plain,
  reference: plain,
  command,
  code,
  table,
  equipment: items,
  cabling: items,
  procedure: items,
  troubleshooting: items,
  diagram: visual,
  topology: visual,
  image: visual,
  attachment: visual,
};
export function renderBlock(view: BlockView): HTMLElement {
  const v = view.payload;
  const article = node("section", "", `knowledge-block block-${v.type}`);
  article.dataset.blockId = v.id;
  article.append(
    node("h3", v.title ?? componentLabels[v.type] ?? v.type),
    node(
      "small",
      `${view.virtual_block ? "Campo original v1" : "Bloco"} · ${v.id}`,
      "block-id",
    ),
  );
  article.append(
    (
      registry[v.type] ??
      (() =>
        node("p", "Tipo de bloco não reconhecido; conteúdo não executado."))
    )(v),
  );
  if (v.caption) article.append(node("p", v.caption, "search-help"));
  return article;
}
export function orderedVisible(
  blocks: BlockView[],
  components: string[],
): BlockView[] {
  return components.flatMap((kind) =>
    blocks.filter((b) => b.payload.type === kind),
  );
}
