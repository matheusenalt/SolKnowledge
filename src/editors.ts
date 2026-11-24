import {
  createArea,
  createModule,
  createProfile,
  resolvePresentation,
  savePresentation,
  saveEntry,
  saveBlock,
} from "./ipc";
import type { Catalog, EntryDetail, BlockView } from "./ipc";
import {
  node,
  button,
  input,
  textarea,
  select,
  field,
  dialogForm,
  onSave,
  componentLabels,
  statusLabels,
  typeLabels,
} from "./dom";

const unique = (prefix: string) => `${prefix}-${crypto.randomUUID()}`;
const profiles = (c: Catalog): [string, string][] => [
  ["", "Herdar configuração"],
  ...c.profiles.map((p) => [p.id, p.name] as [string, string]),
];
const modules = (c: Catalog): [string, string][] =>
  c.modules.map((m) => [
    m.id,
    `${c.areas.find((a) => a.id === m.area_id)?.name ?? ""} / ${m.name}`,
  ]);

export function editEntry(
  c: Catalog,
  detail: EntryDetail | null,
  moduleId: string | null,
  saved: (e: EntryDetail) => Promise<void>,
): void {
  const { dialog, form, status } = dialogForm(
      detail ? "Editar registro" : "Novo registro",
    ),
    v = detail?.payload;
  const id = input(detail?.id ?? unique("registro"));
  id.disabled = !!detail;
  id.required = true;
  const title = input(v?.title ?? "");
  title.required = true;
  title.maxLength = 256;
  const module = select(
    modules(c),
    detail?.module_id ?? moduleId ?? c.modules[0]?.id ?? "",
  );
  module.required = true;
  const type = select(Object.entries(typeLabels), v?.type ?? "note");
  const name = input(v?.full_name ?? ""),
    technical = textarea(v?.technical_description ?? ""),
    simple = textarea(v?.simplified_description ?? ""),
    personal = textarea(v?.personal_description ?? ""),
    markdown = textarea(v?.markdown ?? "");
  const keywords = textarea((v?.keywords ?? []).join("\n"), 3),
    sources = textarea((v?.sources ?? []).join("\n"), 3),
    related = textarea((v?.related_entries ?? []).join("\n"), 3);
  const options = textarea(JSON.stringify(v?.options ?? [], null, 2), 7),
    learning = select(
      Object.entries(statusLabels),
      v?.learning_status ?? "not_assessed",
    ),
    coverage = select(
      [
        ["selected", "Seleção de opções"],
        ["complete", "Completa para versão documentada"],
      ],
      v?.options_coverage ?? "selected",
    );
  const risk = select(
    [
      ["", "Sem classificação explícita"],
      ["read_only", "Leitura"],
      ["changes_state", "Altera o sistema"],
      ["destructive", "Destrutivo"],
      ["context_dependent", "Depende do contexto"],
    ],
    v?.risk ?? "",
  );
  const platform = input(v?.platform ?? "");
  const confirm = input("", "checkbox");
  form.append(
    node(
      "p",
      "Campos omitidos em patches são preservados. Este formulário edita os campos apresentados; os blocos adicionais são mantidos.",
      "search-help",
    ),
  );
  for (const [label, control] of [
    ["ID estável", id],
    ["Título", title],
    ["Módulo", module],
    ["Tipo", type],
    ["Nome original", name],
    ["Explicação técnica", technical],
    ["Explicação simplificada", simple],
    ["Meu entendimento pessoal", personal],
    ["Estado de aprendizado", learning],
    ["Palavras-chave (uma por linha)", keywords],
    ["Anotações / Markdown (texto)", markdown],
    ["Fontes (uma por linha)", sources],
    ["Registros relacionados (IDs, um por linha)", related],
    ["Opções cadastradas (JSON)", options],
    ["Cobertura das opções", coverage],
    ["Plataforma e versão", platform],
    ["Risco contextual", risk],
  ] as [string, HTMLElement][])
    form.append(field(label, control));
  if (v?.personal_description)
    form.append(
      field(
        "Confirmo substituir ou limpar meu entendimento pessoal neste registro",
        confirm,
      ),
    );
  onSave(form, status, async () => {
    const lines = (s: string) =>
      s
        .split("\n")
        .map((s) => s.trim())
        .filter(Boolean);
    const patch: Record<string, unknown> = {
      module_id: module.value,
      type: type.value,
      title: title.value,
      full_name: name.value,
      technical_description: technical.value,
      simplified_description: simple.value,
      personal_description: personal.value,
      markdown: markdown.value,
      keywords: lines(keywords.value),
      sources: lines(sources.value),
      related_entries: lines(related.value),
      options: JSON.parse(options.value),
      learning_status: learning.value,
      options_coverage: coverage.value,
      platform: platform.value,
    };
    if (risk.value) patch.risk = risk.value;
    const e = await saveEntry({
      id: id.value,
      expected_revision: detail?.revision ?? null,
      patch,
      confirm_personal: confirm.checked,
    });
    dialog.close();
    await saved(e);
  });
}

export async function editPresentation(
  c: Catalog,
  moduleId: string,
  saved: () => Promise<void>,
): Promise<void> {
  const module = c.modules.find((m) => m.id === moduleId);
  if (!module) throw new Error("Módulo não encontrado");
  const effective = await resolvePresentation(moduleId);
  const { dialog, form, status } = dialogForm(`Perfil · ${module.name}`);
  const area = c.areas.find((a) => a.id === module.area_id);
  const inherited = module.parent_id
    ? (await resolvePresentation(module.parent_id)).components
    : (area?.enabled_components ??
      c.profiles.find((p) => p.id === (area?.profile_id ?? "custom"))
        ?.enabled_components ??
      []);
  form.append(
    node(
      "p",
      `Herança atual: ${effective.source}. Alterar a apresentação preserva todos os blocos e a pesquisa.`,
      "search-help",
    ),
  );
  const profile = select(profiles(c), module.profile_id ?? "");
  const custom = input("", "checkbox");
  custom.checked = module.enabled_components !== undefined;
  let order = [
    ...(module.enabled_components ?? effective.components),
    ...c.components.filter(
      (k) => !(module.enabled_components ?? effective.components).includes(k),
    ),
  ];
  let enabled = new Set(module.enabled_components ?? effective.components);
  form.append(
    field("Perfil do módulo", profile),
    field("Personalizar componentes neste módulo", custom),
  );
  const list = node("div", "", "component-list");
  form.append(list);
  function render(): void {
    list.replaceChildren();
    order.forEach((kind, i) => {
      const row = node("div", "", "component-row"),
        check = input("", "checkbox");
      check.checked = enabled.has(kind);
      check.disabled = !custom.checked;
      check.addEventListener("change", () => {
        if (check.checked) enabled.add(kind);
        else enabled.delete(kind);
      });
      const up = button("↑", () => {
        [order[i - 1], order[i]] = [order[i]!, order[i - 1]!];
        render();
      });
      up.disabled = !custom.checked || i === 0;
      up.setAttribute("aria-label", `Subir ${componentLabels[kind] ?? kind}`);
      const down = button("↓", () => {
        [order[i], order[i + 1]] = [order[i + 1]!, order[i]!];
        render();
      });
      down.disabled = !custom.checked || i === order.length - 1;
      down.setAttribute(
        "aria-label",
        `Descer ${componentLabels[kind] ?? kind}`,
      );
      row.append(field(componentLabels[kind] ?? kind, check), up, down);
      list.append(row);
    });
  }
  function syncBase(): void {
    if (!custom.checked) {
      const selected =
        c.profiles.find((p) => p.id === profile.value)?.enabled_components ??
        inherited;
      enabled = new Set(selected);
      order = [
        ...selected,
        ...c.components.filter((k) => !selected.includes(k)),
      ];
    }
    render();
  }
  profile.addEventListener("change", syncBase);
  custom.addEventListener("change", syncBase);
  render();
  form.append(
    node(
      "p",
      "Ao desmarcar a personalização, o módulo volta a herdar a lista. Uma lista personalizada vazia oculta todos os componentes, sem apagar conteúdo.",
      "search-help",
    ),
  );
  onSave(form, status, async () => {
    await savePresentation({
      module_id: moduleId,
      expected_revision: module.revision,
      profile_id: profile.value || null,
      enabled_components: custom.checked
        ? order.filter((k) => enabled.has(k))
        : null,
    });
    dialog.close();
    await saved();
  });
}

export function newProfile(saved: () => Promise<void>): void {
  const { dialog, form, status } = dialogForm("Novo perfil por JSON"),
    raw = textarea("", 9);
  raw.required = true;
  raw.placeholder =
    '{"id":"meu-perfil","name":"Meu perfil","enabled_components":["text","personal_note"]}';
  form.append(
    node(
      "p",
      "Cria somente um perfil novo. IDs existentes, componentes desconhecidos, scripts e campos extras são recusados. Esta operação não importa conhecimentos.",
      "search-help",
    ),
    field("Objeto JSON do perfil", raw),
  );
  onSave(form, status, async () => {
    await createProfile(raw.value);
    dialog.close();
    await saved();
  });
}

export function newHierarchy(
  c: Catalog,
  kind: "area" | "module",
  areaId: string | null,
  parentId: string | null,
  saved: (id: string) => Promise<void>,
): void {
  const { dialog, form, status } = dialogForm(
      kind === "area" ? "Nova área" : "Novo módulo",
    ),
    id = input(unique(kind === "area" ? "area" : "modulo")),
    name = input(),
    profile = select(profiles(c));
  id.required = true;
  name.required = true;
  name.maxLength = 160;
  form.append(
    field("ID estável", id),
    field("Nome", name),
    field("Perfil (opcional)", profile),
  );
  const area = select(
    c.areas.map((a) => [a.id, a.name]),
    areaId ?? c.areas[0]?.id ?? "",
  );
  const parent = select([["", "Raiz da área"]]);
  function fillParent(): void {
    parent.replaceChildren();
    for (const [value, label] of [
      ["", "Raiz da área"],
      ...c.modules
        .filter((m) => m.area_id === area.value)
        .map((m) => [m.id, m.name]),
    ]) {
      const o = node("option", label);
      o.value = value ?? "";
      parent.append(o);
    }
    parent.value = parentId ?? "";
  }
  if (kind === "module") {
    area.required = true;
    fillParent();
    area.addEventListener("change", () => {
      parentId = null;
      fillParent();
    });
    form.append(field("Área", area), field("Módulo pai", parent));
  }
  onSave(form, status, async () => {
    if (kind === "area")
      await createArea({
        id: id.value,
        name: name.value,
        profile_id: profile.value || null,
      });
    else
      await createModule({
        id: id.value,
        name: name.value,
        area_id: area.value,
        parent_id: parent.value || null,
        profile_id: profile.value || null,
      });
    dialog.close();
    await saved(id.value);
  });
}

export function editBlock(
  detail: EntryDetail,
  view: BlockView | null,
  saved: (e: EntryDetail) => Promise<void>,
): void {
  const { dialog, form, status } = dialogForm(
    view ? `Editar bloco · ${view.payload.id}` : "Novo bloco",
  );
  const payload = view?.payload ?? {
    id: unique("bloco"),
    type: "text",
    content: "",
  };
  const raw = textarea(JSON.stringify(payload, null, 2), 14);
  raw.required = true;
  const confirm = input("", "checkbox");
  form.append(
    node(
      "p",
      "Edite o objeto ou envie um patch com ID e somente os campos alterados. O tipo é imutável; omitir campos preserva o conteúdo. Arrays fornecidos substituem a lista inteira.",
      "search-help",
    ),
    field("JSON do bloco", raw),
  );
  if (view?.payload.type === "personal_note" && view.payload.content)
    form.append(
      field(
        "Confirmo substituir ou limpar o conteúdo desta nota pessoal",
        confirm,
      ),
    );
  onSave(form, status, async () => {
    const e = await saveBlock({
      entry_id: detail.id,
      expected_revision: detail.revision,
      patch: JSON.parse(raw.value),
      confirm_personal: confirm.checked,
    });
    dialog.close();
    await saved(e);
  });
}
