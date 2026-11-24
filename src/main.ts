import { resultCard } from "./result-card";
import { liveSearch } from "./live-search";
import { applySettings } from "./configuration";
import { transferPage, saveExport } from "./transfer";
import { importPackageButton, attachButton } from "./visual-actions";
import "./styles.css";
import * as api from "./ipc";
import type {
  Catalog,
  CatalogFilter,
  EntryDetail,
  SearchResult,
  Module,
  EntryIndex,
} from "./ipc";
import {
  el,
  node,
  button,
  link,
  select,
  field,
  notify,
  errorMessage,
  typeLabels,
  statusLabels,
} from "./dom";
import { renderBlock, orderedVisible } from "./blocks";
import {
  editEntry,
  editBlock,
  editPresentation,
  newHierarchy,
  newProfile,
} from "./editors";
import { loadVocabulary } from "./vocabulary";

let catalog: Catalog = { areas: [], modules: [], profiles: [], components: [] };
let entries: EntryIndex[] = [];
let ready = false,
  epoch = 0;
const allBlocks = new Set<string>();
let activeModule: string | null = null;
const page = el("page"),
  filters = el("filters"),
  query = el<HTMLInputElement>("search-query");
const areaFilter = el<HTMLSelectElement>("filter-area"),
  moduleFilter = el<HTMLSelectElement>("filter-module"),
  typeFilter = el<HTMLSelectElement>("filter-type"),
  statusFilter = el<HTMLSelectElement>("filter-status");
function go(hash: string): void {
  if (location.hash === hash) void route();
  else location.hash = hash;
}
const areaHash = (id: string) => `#area/${encodeURIComponent(id)}`,
  moduleHash = (id: string) => `#module/${encodeURIComponent(id)}`,
  entryHash = (id: string) => `#entry/${encodeURIComponent(id)}`;
function routeParts(): [string, string] {
  const [kind, ...rest] = (location.hash.slice(1) || "home").split("/");
  return [kind ?? "home", decodeURIComponent(rest.join("/"))];
}
function getFilters(favorites = false): CatalogFilter {
  return {
    area_id: areaFilter.value || null,
    module_id: moduleFilter.value || null,
    entry_type: typeFilter.value || null,
    learning_status: statusFilter.value || null,
    favorites_only: favorites,
  };
}
function resetFilters(): void {
  areaFilter.value = "";
  moduleFilter.value = "";
  typeFilter.value = "";
  statusFilter.value = "";
  fillModuleFilter();
}
function fillSelect(
  target: HTMLSelectElement,
  options: [string, string][],
  first: string,
): void {
  const value = target.value;
  target.replaceChildren();
  for (const [id, label] of [["", first], ...options]) {
    const o = node("option", label);
    o.value = id ?? "";
    target.append(o);
  }
  target.value = value;
}
function fillModuleFilter(): void {
  fillSelect(
    moduleFilter,
    catalog.modules
      .filter((m) => !areaFilter.value || m.area_id === areaFilter.value)
      .map((m) => [
        m.id,
        modulePath(m.id)
          .map((p) => p.name)
          .join(" / "),
      ]),
    "Todos os módulos",
  );
}
function modulePath(id: string): Module[] {
  const path: Module[] = [];
  const seen = new Set<string>();
  let current = catalog.modules.find((m) => m.id === id);
  while (current && !seen.has(current.id)) {
    seen.add(current.id);
    path.unshift(current);
    current = catalog.modules.find((m) => m.id === current!.parent_id);
  }
  return path;
}
function breadcrumb(
  moduleId?: string,
  entryTitle?: string,
  areaId?: string,
): void {
  const root = el("breadcrumb");
  root.replaceChildren(link("Biblioteca pessoal", "#home"));
  const path = moduleId ? modulePath(moduleId) : [];
  const area = catalog.areas.find((a) => a.id === (path[0]?.area_id ?? areaId));
  if (area) root.append(node("span", "/"), link(area.name, areaHash(area.id)));
  for (const m of path)
    root.append(node("span", "/"), link(m.name, moduleHash(m.id)));
  if (entryTitle) root.append(node("span", "/"), node("span", entryTitle));
}
function tree(): void {
  const root = el("library-tree");
  root.replaceChildren();
  function branch(module: Module): HTMLElement {
    const item = node("li");
    const children = catalog.modules.filter((m) => m.parent_id === module.id);
    if (!children.length)
      item.append(
        link(`${module.name} · ${module.entry_count}`, moduleHash(module.id)),
      );
    else {
      const detail = node("details"),
        summary = node("summary", module.name);
      detail.open = modulePath(activeModule ?? "").some(
        (m) => m.id === module.id,
      );
      detail.append(
        summary,
        link(`Abrir ${module.name}`, moduleHash(module.id)),
      );
      const list = node("ul");
      for (const child of children) list.append(branch(child));
      detail.append(list);
      item.append(detail);
    }
    return item;
  }
  for (const area of catalog.areas) {
    const d = node("details"),
      s = node("summary", `${area.name} · ${area.entry_count}`);
    const [routeKind, id] = routeParts();
    d.open =
      area.id ===
        catalog.modules.find((m) => m.id === (activeModule ?? id))?.area_id ||
      (routeKind === "area" && id === area.id);
    d.append(s, link("Visão da área", areaHash(area.id)));
    const list = node("ul");
    for (const m of catalog.modules.filter(
      (m) => m.area_id === area.id && !m.parent_id,
    ))
      list.append(branch(m));
    d.append(list);
    root.append(d);
  }
}
async function refreshCatalog(): Promise<void> {
  instantSearch.cancel();
  ++epoch;
  const [next, overview, index] = await Promise.all([
    api.getCatalog(),
    api.getOverview(),
    api.getEntriesIndex(),
    applySettings(),
  ]);
  catalog = next;
  entries = index;
  fillSelect(
    areaFilter,
    catalog.areas.map((a) => [a.id, a.name]),
    "Todas as áreas",
  );
  fillModuleFilter();
  tree();
  el("library-summary").textContent =
    `${overview.entries} registros · ${overview.areas} áreas · ${overview.profiles} perfis`;
  el("library-summary").title = `Banco local: ${overview.database_path}`;
}
function heading(title: string, description = ""): HTMLElement {
  const wrap = node("header", "", "page-heading"),
    h = node("h1", title);
  h.tabIndex = -1;
  wrap.append(h);
  if (description) wrap.append(node("p", description, "search-help"));
  return wrap;
}
function actions(
  moduleId: string | null = null,
  areaId: string | null = null,
): HTMLElement {
  const row = node("div", "", "action-row");
  const create = button(
    "+ Novo registro",
    () => editEntry(catalog, null, moduleId, displaySaved),
    "primary",
  );
  create.disabled = !catalog.modules.length;
  row.append(
    create,
    importPackageButton("knowledge_update", async () => {
      await refreshCatalog();
      await route();
    }),
    button("+ Novo módulo", () =>
      newHierarchy(catalog, "module", areaId, moduleId, async (id) => {
        await refreshCatalog();
        go(moduleHash(id));
      }),
    ),
    button("Novo perfil JSON", () =>
      newProfile(async () => {
        await refreshCatalog();
        notify("Perfil criado. Selecione-o no editor de perfil de um módulo.");
      }),
    ),
  );
  if (moduleId)
    row.append(
      button("Configurar perfil", () =>
        editPresentation(catalog, moduleId, async () => {
          await refreshCatalog();
          await route();
          notify("Apresentação atualizada. Todos os blocos foram preservados.");
        }),
      ),
    );
  return row;
}
function home(): void {
  page.replaceChildren(
    node("p", "UM LUGAR PARA O QUE VOCÊ APRENDE", "eyebrow"),
  );
  const hero = node("section", "", "hero"),
    intro = node("div"),
    title = node("h1", "Conhecimento sem fronteiras.");
  title.tabIndex = -1;
  intro.append(
    title,
    node(
      "p",
      "Organize suas referências e reencontre seu próprio entendimento. O aprendizado acontece no seu ritmo.",
    ),
  );
  const logo = node("img");
  logo.src = "/assets/sol.png";
  logo.alt = "Logo oficial SOL Knowledge";
  logo.className = "hero-logo";
  logo.width = 210;
  logo.height = 199;
  hero.append(intro, logo);
  const toolbar = actions();
  toolbar.append(
    button("+ Nova área", () =>
      newHierarchy(catalog, "area", null, null, async (id) => {
        await refreshCatalog();
        go(areaHash(id));
      }),
    ),
  );
  page.append(hero, toolbar);
  page.append(node("h2", "Suas áreas", "section-title"));
  const grid = node("div", "", "area-grid");
  for (const [i, a] of catalog.areas.entries()) {
    const card = link("", areaHash(a.id), "area-card");
    card.append(
      node("span", String(i + 1).padStart(2, "0"), "eyebrow"),
      node("h3", a.name),
      node(
        "p",
        a.entry_count
          ? `${a.entry_count} registro(s) para consulta`
          : "Sem conhecimentos cadastrados.",
        "search-help",
      ),
      node("span", "Explorar ↗", "area-action"),
    );
    grid.append(card);
  }
  page.append(grid);
  if (!catalog.areas.length)
    page.append(
      node(
        "p",
        "Crie sua primeira área pelo botão + na biblioteca.",
        "empty-state",
      ),
    );
}
async function showList(
  kind: string,
  id: string,
  version: number,
): Promise<void> {
  if (kind === "search") {
    if (document.activeElement !== query) query.value = id;
    instantSearch.input(id, getFilters(), true);
    return;
  }
  const module = catalog.modules.find((m) => m.id === id),
    area = catalog.areas.find((a) => a.id === id);
  if ((kind === "module" && !module) || (kind === "area" && !area))
    throw new Error("Área ou módulo não encontrado.");
  if (kind === "area") {
    areaFilter.value = id;
    moduleFilter.value = "";
    fillModuleFilter();
    breadcrumb(undefined, undefined, id);
  }
  if (kind === "module" && module) {
    areaFilter.value = module.area_id;
    fillModuleFilter();
    moduleFilter.value = id;
    activeModule = id;
    breadcrumb(id);
  }
  const title =
    kind === "module"
      ? (module?.name ?? "Módulo não encontrado")
      : kind === "area"
        ? (area?.name ?? "Área não encontrada")
        : kind === "favorites"
          ? "Favoritos"
          : kind === "search"
            ? `Pesquisa: ${id}`
            : "Todos os registros";
  page.replaceChildren(
    heading(
      title,
      kind === "module"
        ? "Inclui registros deste módulo e de seus submódulos."
        : kind === "search"
          ? "Correspondências exatas, textuais e aproximadas. O conteúdo oculto nos perfis continua pesquisável."
          : "Referências pessoais. A consulta não altera seu estado de aprendizado.",
    ),
    actions(
      kind === "module" ? id : null,
      kind === "area" ? id : (module?.area_id ?? null),
    ),
  );
  const count = node("p", "Carregando…", "search-help"),
    list = node("div"),
    pager = node("div", "", "pagination");
  page.append(count, list, pager);
  const f = getFilters(kind === "favorites");
  let offset = 0,
    total = 0;
  async function load(): Promise<void> {
    const data = await api.browseEntries(f, offset);
    if (epoch !== version) return;
    total = data.total;
    for (const result of data.items) list.append(resultCard(result));
    offset += data.items.length;
    count.textContent = `${offset} de ${total} registro(s) · ordem por título`;
    pager.replaceChildren();
    if (offset < total)
      pager.append(
        button("Mostrar mais registros", async () => {
          pager.replaceChildren(node("p", "Carregando…"));
          await load();
        }),
      );
    if (!offset)
      list.append(
        node(
          "p",
          "Nenhum conhecimento neste recorte. Crie um registro ou ajuste os filtros.",
          "empty-state",
        ),
      );
  }
  await load();
}
async function displaySaved(detail: EntryDetail): Promise<void> {
  await refreshCatalog();
  notify("Registro salvo. Conteúdo e pesquisa atualizados.");
  if (location.hash === entryHash(detail.id)) displayEntry(detail);
  else go(entryHash(detail.id));
}
function displayEntry(detail: EntryDetail): void {
  activeModule = detail.module_id;
  breadcrumb(detail.module_id, detail.payload.title);
  tree();
  filters.hidden = true;
  const e = detail.payload;
  page.replaceChildren(heading(e.title, e.full_name ?? ""));
  const row = node("div", "", "action-row");
  row.append(
    attachButton(detail, displaySaved),
    button("Exportar registro JSON", () => saveExport("entry", detail.id)),
    button("Exportar registro com anexos", () =>
      saveExport("entry_zip", detail.id),
    ),
  );
  const star = button(
    detail.favorite ? "★ Favorito" : "☆ Favoritar",
    async () => {
      await api.setFavorite(detail.id, !detail.favorite);
      detail.favorite = !detail.favorite;
      star.textContent = detail.favorite ? "★ Favorito" : "☆ Favoritar";
    },
  );
  row.append(
    star,
    button("Editar registro", () =>
      editEntry(catalog, detail, null, displaySaved),
    ),
    button("+ Novo bloco", () => editBlock(detail, null, displaySaved)),
    button("Configurar perfil do módulo", () =>
      editPresentation(catalog, detail.module_id, async () => {
        await refreshCatalog();
        displayEntry(await api.getEntry(detail.id));
        notify("Perfil atualizado sem apagar blocos.");
      }),
    ),
  );
  page.append(row);
  const state = select(
      Object.entries(statusLabels),
      e.learning_status ?? "not_assessed",
    ),
    stateRow = node("div", "", "learning-row");
  stateRow.append(
    field("Meu estado de aprendizado", state),
    button("Salvar estado", async () => {
      await displaySaved(
        await api.saveEntry({
          id: detail.id,
          expected_revision: detail.revision,
          patch: { learning_status: state.value },
          confirm_personal: false,
        }),
      );
    }),
  );
  page.append(stateRow);
  if (e.type === "command")
    page.append(
      node(
        "p",
        "Consulta de comando: avalie o impacto e as permissões de cada opção antes de usá-la. Copiar um texto nunca o executa.",
        "command-warning",
      ),
    );
  const profileName =
    catalog.profiles.find((p) => p.id === detail.presentation.profile_id)
      ?.name ?? detail.presentation.profile_id;
  page.append(
    node(
      "p",
      `Perfil: ${profileName} · origem: ${detail.presentation.source}`,
      "search-help",
    ),
  );
  const toolbar = node("div", "", "section-heading"),
    blocks = node("div");
  let all = allBlocks.has(detail.id);
  const hidden = detail.blocks.filter(
    (b) => !detail.presentation.components.includes(b.payload.type),
  ).length;
  const toggle = button(`Todos os blocos (${detail.blocks.length})`, () => {
    all = !all;
    if (all) allBlocks.add(detail.id);
    else allBlocks.delete(detail.id);
    render();
  });
  toolbar.append(node("h2", "Conhecimento", "section-title"), toggle);
  page.append(
    toolbar,
    node(
      "p",
      hidden
        ? `${hidden} bloco(s) oculto(s) pelo perfil, preservados e pesquisáveis.`
        : "A ordem de apresentação segue os componentes do módulo.",
      "search-help",
    ),
    blocks,
  );
  function render(): void {
    blocks.replaceChildren();
    toggle.textContent = all
      ? "Voltar à apresentação do perfil"
      : `Todos os blocos (${detail.blocks.length})`;
    const visible = all
      ? detail.blocks
      : orderedVisible(detail.blocks, detail.presentation.components);
    const persisted = detail.blocks.filter((b) => !b.virtual_block);
    for (const view of visible) {
      const block = renderBlock(view),
        controls = node("div", "", "block-actions");
      controls.append(
        button(
          view.virtual_block ? "Editar campo de origem" : "Editar bloco",
          () => {
            if (view.virtual_block)
              editEntry(catalog, detail, null, displaySaved);
            else editBlock(detail, view, displaySaved);
          },
        ),
      );
      if (all && !view.virtual_block && persisted.length > 1) {
        const i = persisted.findIndex((b) => b.payload.id === view.payload.id);
        for (const [delta, label] of [
          [-1, "Subir bloco"],
          [1, "Descer bloco"],
        ] as [number, string][]) {
          const b = button(label, async () => {
            const ids = persisted.map((b) => b.payload.id);
            [ids[i], ids[i + delta]] = [ids[i + delta]!, ids[i]!];
            await displaySaved(
              await api.reorderBlocks({
                entry_id: detail.id,
                expected_revision: detail.revision,
                ids,
              }),
            );
          });
          b.disabled = i + delta < 0 || i + delta >= persisted.length;
          controls.append(b);
        }
      }
      if (!view.virtual_block && view.payload.type === "attachment")
        controls.append(attachButton(detail, displaySaved, view.payload.id));
      block.append(controls);
      blocks.append(block);
    }
    if (!visible.length)
      blocks.append(
        node(
          "p",
          all
            ? "Este registro ainda não tem blocos. Acrescente apenas conteúdo que você deseja documentar."
            : "Nenhum bloco habilitado neste perfil. Use Todos os blocos ou configure os componentes.",
          "empty-state",
        ),
      );
  }
  render();
  if (e.related_entries?.length) {
    page.append(node("h2", "Registros relacionados", "section-title"));
    const list = node("ul");
    for (const id of e.related_entries) {
      const li = node("li"),
        entry = entries.find((e) => e.id === id);
      li.append(
        entry
          ? link(entry.title, entryHash(id))
          : node("span", `Referência pendente: ${id}`),
      );
      list.append(li);
    }
    page.append(list);
  }
}
async function history(version: number): Promise<void> {
  page.replaceChildren(
    heading(
      "Histórico",
      "Consultas realizadas e registros abertos. Nenhum acesso indica aprendizado concluído.",
    ),
  );
  const data = await api.getHistory();
  if (epoch !== version) return;
  for (const [title, items] of [
    ["Registros consultados", data.visits],
    ["Pesquisas", data.searches],
  ] as const) {
    page.append(node("h2", title, "section-title"));
    const list = node("ul", "", "history-list");
    for (const item of items) {
      const li = node("li"),
        date = new Date(`${item.at.replace(" ", "T")}Z`).toLocaleString(
          "pt-BR",
        );
      if ("entry_id" in item)
        li.append(link(item.title, entryHash(item.entry_id)));
      else li.append(button(item.query, () => searchRoute(item.query)));
      li.append(node("time", date));
      list.append(li);
    }
    if (!items.length) list.append(node("li", "Nenhum item registrado."));
    page.append(list);
  }
}
let searchCount: HTMLElement;
let searchResults: HTMLElement;
const instantSearch = liveSearch<SearchResult[], CatalogFilter>({
  search: api.searchKnowledge,
  pending: () => {
    ++epoch;
    page.setAttribute("aria-busy", "true");
  },
  start: (value) => {
    window.history.replaceState(
      null,
      "",
      `#search/${encodeURIComponent(value)}`,
    );
    breadcrumb();
    page.hidden = false;
    el("vocabulary-view").hidden = true;
    filters.hidden = false;
    const title = heading("Resultados da pesquisa", `Pesquisa: ${value}`);
    title.classList.add("search-heading");
    searchCount = node("p", "Pesquisando…", "search-help");
    searchCount.setAttribute("role", "status");
    searchCount.setAttribute("aria-live", "polite");
    searchResults = node("div", "", "search-results");
    page.replaceChildren(title, searchCount, searchResults);
  },
  results: (results) => {
    page.setAttribute("aria-busy", "false");
    searchCount.textContent = `${results.length} resultado(s) · até 100 por consulta`;
    searchResults.replaceChildren(...results.map((r) => resultCard(r, true)));
    if (!results.length)
      searchResults.append(
        node(
          "p",
          "Nenhum registro encontrado. Tente outros termos ou ajuste os filtros e o vocabulário.",
          "empty-state",
        ),
      );
  },
  error: (error) => {
    page.setAttribute("aria-busy", "false");
    searchCount.textContent = "Não foi possível pesquisar.";
    searchResults.replaceChildren(
      node("p", errorMessage(error), "empty-state"),
      button("Tentar novamente", () => searchRoute(query.value)),
    );
  },
  clear: () => {
    window.history.replaceState(null, "", "#library");
    void route();
  },
});
function searchRoute(value: string, immediate = true): void {
  if (routeParts()[0] !== "search") resetFilters();
  if (query.value !== value) query.value = value;
  if (value.trim())
    window.history.replaceState(
      null,
      "",
      `#search/${encodeURIComponent(value.trim())}`,
    );
  instantSearch.input(value, getFilters(), immediate);
}
async function route(): Promise<void> {
  if (!ready) return;
  instantSearch.cancel();
  page.setAttribute("aria-busy", "false");
  const version = ++epoch;
  const [kind, id] = routeParts();
  page.hidden = kind === "vocabulary";
  el("vocabulary-view").hidden = kind !== "vocabulary";
  filters.hidden = ![
    "area",
    "module",
    "library",
    "favorites",
    "search",
  ].includes(kind);
  notify("");
  breadcrumb();
  for (const a of document.querySelectorAll<HTMLAnchorElement>(".main-nav a")) {
    if (a.hash === `#${kind}`) a.setAttribute("aria-current", "page");
    else a.removeAttribute("aria-current");
  }
  try {
    if (kind === "home") {
      resetFilters();
      activeModule = null;
      home();
    } else if (kind === "entry") {
      const detail = await api.getEntry(id);
      if (version !== epoch) return;
      displayEntry(detail);
    } else if (kind === "history") await history(version);
    else if (kind === "transfer")
      await transferPage(page, async () => {
        await refreshCatalog();
        await route();
      });
    else if (kind === "vocabulary") await loadVocabulary();
    else await showList(kind, id, version);
    if (version === epoch) {
      document.title = `${kind === "entry" ? (page.querySelector("h1")?.textContent ?? "Registro") : "SOL Knowledge"}${kind === "entry" ? " · SOL Knowledge" : ""}`;
      const title = (
        kind === "vocabulary" ? el("vocabulary-view") : page
      ).querySelector<HTMLElement>("h1");
      if (document.activeElement !== query)
        title?.focus({ preventScroll: true });
    }
  } catch (error) {
    if (version === epoch) {
      page.hidden = false;
      page.replaceChildren(
        heading("Não foi possível carregar"),
        node("p", errorMessage(error), "empty-state"),
        button("Tentar novamente", route),
      );
    }
  }
}
document.querySelector(".skip-link")?.addEventListener("click", (e) => {
  e.preventDefault();
  el("conteudo").focus();
});
el<HTMLFormElement>("search-form").addEventListener("submit", (e) => {
  e.preventDefault();
  searchRoute(query.value);
});
query.addEventListener("input", (event) => {
  if (!(event as InputEvent).isComposing) searchRoute(query.value, false);
});
query.addEventListener("compositionend", () => searchRoute(query.value, false));
document.addEventListener("keydown", (e) => {
  if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === "k") {
    e.preventDefault();
    query.focus();
    query.select();
  }
});
for (const control of [areaFilter, moduleFilter, typeFilter, statusFilter])
  control.addEventListener("change", () => {
    if (control === areaFilter) {
      moduleFilter.value = "";
      fillModuleFilter();
    }
    const [kind] = routeParts();
    if (kind === "search") void route();
    else if (kind === "favorites") void route();
    else go("#library");
  });
el("clear-filters").addEventListener("click", () => {
  resetFilters();
  const [kind] = routeParts();
  if (kind === "search" || kind === "favorites") void route();
  else go("#library");
});
el("new-area").addEventListener("click", () =>
  newHierarchy(catalog, "area", null, null, async (id) => {
    await refreshCatalog();
    go(areaHash(id));
  }),
);
window.addEventListener("hashchange", () => {
  void route();
});
async function initialize(): Promise<void> {
  fillSelect(typeFilter, Object.entries(typeLabels), "Todos os tipos");
  fillSelect(statusFilter, Object.entries(statusLabels), "Todos os estados");
  try {
    const info = await api.getAppInfo();
    await refreshCatalog();
    el("app-version").textContent = info.version;
    ready = true;
    query.disabled = false;
    el<HTMLButtonElement>("search-submit").disabled = false;
    el<HTMLButtonElement>("new-area").disabled = false;
    el("runtime-status").textContent = "SQLite local · Sem serviços externos";
    await route();
  } catch {
    el("runtime-status").textContent = "Backend indisponível";
    el("library-tree").replaceChildren(
      node("p", "A biblioteca é acessada na janela desktop.", "search-help"),
    );
    el("startup-status").textContent =
      "Prévia da interface. Abra o aplicativo desktop para acessar o banco local. Os dados existentes devem ser preservados em caso de falha.";
  }
}
void initialize();
