import { invoke } from "@tauri-apps/api/core";
import { node, button, input, field, notify, copyButton } from "./dom";
export interface Change {
  id: string;
  before: unknown;
  after: unknown;
  requires_approval: boolean;
}
export interface Preview {
  package_id: string;
  kind: string;
  sha256: string;
  review_token: string;
  already_applied: boolean;
  counts: { new: number; updated: number; same: number; conflicts: number };
  changes: Change[];
}
interface Backup {
  id: string;
  size: number;
}
interface RestorePreview {
  review_token: string;
  sha256: string;
  counts: Record<string, number>;
  current_counts: Record<string, number>;
  files: number;
  assets: number;
}
export async function saveExport(
  kind: "settings" | "entry" | "entry_zip" | "backup" | "stored_backup",
  id?: string,
): Promise<void> {
  const saved = await invoke<boolean>("save_export", { kind, id: id ?? null });
  if (saved) notify("Exportação salva no destino escolhido.");
}
interface Selection<T> {
  ticket: string;
  mode: string;
  preview: T;
}
async function cancel(ticket: string): Promise<void> {
  await invoke("cancel_transfer", { ticket });
}
export function changeList(preview: Preview): {
  element: HTMLElement;
  approved: () => string[];
} {
  const list = node("div", "", "import-changes");
  const checks = new Map<string, HTMLInputElement>();
  for (const c of preview.changes) {
    const card = node("details", "", "import-change");
    card.append(node("summary", c.id));
    const grid = node("div", "", "diff-values");
    for (const [label, value] of [
      ["Anterior", c.before],
      ["Novo", c.after],
    ] as const) {
      const col = node("div");
      col.append(
        node("strong", label),
        node("pre", JSON.stringify(value, null, 2)),
      );
      grid.append(col);
    }
    card.append(grid);
    if (c.requires_approval) {
      card.open = true;
      const check = input("", "checkbox");
      check.checked = false;
      card.append(field("Confirmo esta alteração específica", check));
      checks.set(c.id, check);
    }
    list.append(card);
  }
  return {
    element: list,
    approved: () => [...checks].filter(([, c]) => c.checked).map(([id]) => id),
  };
}
function showPreview(
  preview: Preview,
  ticket: string,
  done: () => Promise<void>,
): void {
  if (preview.already_applied) {
    void cancel(ticket);
    notify("Pacote já aplicado com o mesmo conteúdo. Nenhuma alteração.");
    return;
  }
  const dialog = node("dialog", "", "transfer-dialog");
  const counts = preview.counts;
  const review = changeList(preview);
  dialog.append(
    node("h2", "Revisar importação"),
    node(
      "p",
      `${counts.new} novo(s) · ${counts.updated} atualizado(s) · ${counts.same} igual(is) · ${counts.conflicts} conflito(s)`,
    ),
    node("p", `Pacote ${preview.package_id}`, "search-help"),
    node(
      "p",
      "Revise os valores e confirme separadamente as alterações sinalizadas. Antes da aplicação será salvo um backup completo local.",
    ),
    review.element,
  );
  const confirm = button("Criar backup e aplicar", async () => {
    confirm.disabled = true;
    try {
      const result = await invoke<{ backup_id: string | null }>(
        "confirm_import",
        {
          ticket,
          approval: {
            review_token: preview.review_token,
            approved_changes: review.approved(),
            confirm: true,
          },
        },
      );
      dialog.close();
      dialog.remove();
      await done();
      notify(
        `Importação aplicada. Backup local: ${result.backup_id ?? "nenhum (pacote já aplicado)"}.`,
      );
    } finally {
      confirm.disabled = false;
    }
  });
  dialog.append(
    button("Cancelar", async () => {
      if (confirm.disabled) return;
      await cancel(ticket);
      dialog.close();
      dialog.remove();
    }),
    confirm,
  );
  dialog.addEventListener("cancel", (e) => {
    if (confirm.disabled) {
      e.preventDefault();
      return;
    }
    void cancel(ticket);
    dialog.remove();
  });
  document.body.append(dialog);
  dialog.showModal();
}
export function importPackageButton(
  mode: "settings" | "knowledge_update",
  done: () => Promise<void>,
): HTMLButtonElement {
  return button(
    mode === "settings"
      ? "Importar configuração JSON"
      : "Importar conhecimento JSON/ZIP",
    async () => {
      const selected = await invoke<Selection<Preview> | null>(
        "choose_transfer",
        { mode },
      );
      if (selected) showPreview(selected.preview, selected.ticket, done);
    },
  );
}
export function restoreButton(done: () => Promise<void>): HTMLButtonElement {
  return button("Restaurar backup completo", async () => {
    const selected = await invoke<Selection<RestorePreview> | null>(
      "choose_transfer",
      { mode: "restore" },
    );
    if (!selected) return;
    const p = selected.preview,
      ticket = selected.ticket;
    const dialog = node("dialog", "", "transfer-dialog");
    const phrase = input();
    phrase.autocomplete = "off";
    let busy = false;
    const table = node("table"),
      head = node("tr");
    for (const h of ["Coleção", "Base atual", "Backup"])
      head.append(node("th", h));
    table.append(head);
    for (const [key, count] of Object.entries(p.counts)) {
      const row = node("tr");
      row.append(
        node("td", key),
        node("td", String(p.current_counts[key] ?? 0)),
        node("td", String(count)),
      );
      table.append(row);
    }
    dialog.append(
      node("h2", "Restaurar toda a base"),
      node(
        "p",
        "Esta operação substitui a base atual pelo snapshot validado. Um backup completo da base atual será salvo antes da restauração.",
      ),
      node(
        "p",
        p.files +
          " arquivos validados · " +
          p.assets +
          " anexos · SHA-256 " +
          p.sha256,
        "search-help",
      ),
      table,
      field("Digite RESTAURAR para confirmar", phrase),
    );
    const confirm = button("Salvar backup atual e restaurar", async () => {
      busy = true;
      confirm.disabled = true;
      try {
        const previous = await invoke<Backup>("confirm_restore", {
          ticket,
          approval: { review_token: p.review_token, phrase: phrase.value },
        });
        dialog.close();
        dialog.remove();
        await done();
        notify(
          "Base restaurada e índice recomposto. Backup anterior: " +
            previous.id +
            ".",
        );
      } finally {
        busy = false;
        confirm.disabled = phrase.value !== "RESTAURAR";
      }
    });
    confirm.disabled = true;
    phrase.addEventListener("input", () => {
      confirm.disabled = busy || phrase.value !== "RESTAURAR";
    });
    dialog.append(
      button("Cancelar", async () => {
        if (busy) return;
        await cancel(ticket);
        dialog.close();
        dialog.remove();
      }),
      confirm,
    );
    dialog.addEventListener("cancel", (e) => {
      if (busy) {
        e.preventDefault();
        return;
      }
      void cancel(ticket);
      dialog.remove();
    });
    document.body.append(dialog);
    dialog.showModal();
  });
}
export async function transferPage(
  target: HTMLElement,
  done: () => Promise<void>,
): Promise<void> {
  target.replaceChildren(
    node("h1", "Importação e backups"),
    node(
      "p",
      "Configuração, conhecimento e restauração têm validações e confirmações separadas. Importar preserva campos omitidos e não exclui registros.",
    ),
  );
  const config = node("section", "", "knowledge-block");
  config.append(
    node("h2", "Configuração"),
    node("p", "Aparência, preferências, perfis, áreas e módulos."),
    importPackageButton("settings", done),
    button("Exportar configuração", () => saveExport("settings")),
  );
  const knowledge = node("section", "", "knowledge-block");
  knowledge.append(
    node("h2", "Conhecimento incremental"),
    node(
      "p",
      "JSON v1/v2 ou ZIP com manifest.json e assets/. A prévia mostra os valores anteriores e novos.",
    ),
    importPackageButton("knowledge_update", done),
  );
  const model = JSON.stringify(
    {
      schema_version: 2,
      type: "knowledge_update",
      package_id: "novo-pacote-001",
      entries: [],
    },
    null,
    2,
  );
  knowledge.append(
    node("h3", "Modelo JSON"),
    node("pre", model),
    copyButton(model),
  );
  const backup = node("section", "", "knowledge-block");
  backup.append(
    node("h2", "Backup completo e restauração"),
    node(
      "p",
      "Inclui snapshot SQLite, configuração, conhecimento, favoritos, históricos e anexos. Guarde a exportação em um local de sua escolha.",
    ),
    button("Exportar backup completo", () => saveExport("backup")),
    restoreButton(done),
  );
  const list = node("div");
  for (const b of await invoke<Backup[]>("list_backups")) {
    const row = node("div", "", "action-row");
    row.append(
      node("span", `${b.id} · ${(b.size / 1024 / 1024).toFixed(2)} MiB`),
      button("Salvar cópia", () => saveExport("stored_backup", b.id)),
    );
    list.append(row);
  }
  if (!list.childElementCount)
    list.append(
      node("p", "Ainda não há backups automáticos locais.", "search-help"),
    );
  backup.append(node("h3", "Backups locais anteriores"), list);
  target.append(config, knowledge, backup);
}
