import {
  getEntriesIndex,
  listVocabulary,
  saveVocabularyTerm,
  deleteVocabularyTerm,
} from "./ipc";
import type { TermKind, VocabularyTerm } from "./ipc";
import { el, node, button, errorMessage } from "./dom";

const form = el<HTMLFormElement>("vocabulary-form"),
  kind = el<HTMLSelectElement>("term-kind"),
  phrase = el<HTMLInputElement>("term-phrase"),
  target = el<HTMLSelectElement>("term-target"),
  save = el<HTMLButtonElement>("term-save"),
  cancel = el<HTMLButtonElement>("term-cancel"),
  status = el("term-status");
let editing: { id: number; kind: TermKind } | null = null;
function reset(): void {
  editing = null;
  form.reset();
  kind.disabled = false;
  cancel.hidden = true;
  save.textContent = "Salvar expressão";
}
function edit(term: VocabularyTerm): void {
  editing = term;
  kind.value = term.kind;
  kind.disabled = true;
  phrase.value = term.phrase;
  target.value = term.target_entry_id;
  save.textContent = "Salvar alteração";
  cancel.hidden = false;
  status.textContent = "Editando expressão existente.";
  phrase.focus();
}
async function refresh(): Promise<void> {
  const terms = await listVocabulary(),
    list = el("vocabulary-list");
  list.replaceChildren();
  if (!terms.some((t) => t.kind === "personal"))
    list.append(
      node("p", "Você ainda não cadastrou expressões pessoais.", "search-help"),
    );
  for (const term of terms) {
    const row = node("div", "", "vocabulary-term");
    row.append(
      node("span", `${term.phrase} → ${term.target_title}`),
      node("small", term.kind === "personal" ? "Pessoal" : "Sinônimo"),
      button("Editar", () => edit(term)),
      button("Remover", async () => {
        if (
          !window.confirm(
            `Remover a expressão “${term.phrase}”? O registro será preservado.`,
          )
        )
          return;
        await deleteVocabularyTerm(term.kind, term.id);
        if (editing?.id === term.id && editing.kind === term.kind) reset();
        await refresh();
        status.textContent = "Expressão removida.";
      }),
    );
    list.append(row);
  }
}
cancel.addEventListener("click", () => {
  reset();
  status.textContent = "Edição cancelada.";
});
form.addEventListener("submit", async (e) => {
  e.preventDefault();
  save.disabled = true;
  try {
    await saveVocabularyTerm({
      id: editing?.id ?? null,
      kind: editing?.kind ?? (kind.value as TermKind),
      phrase: phrase.value,
      target_entry_id: target.value,
    });
    reset();
    await refresh();
    status.textContent = "Expressão salva. Já pode ser usada na pesquisa.";
  } catch (error) {
    status.textContent = errorMessage(error);
  } finally {
    save.disabled = false;
  }
});
export async function loadVocabulary(): Promise<void> {
  const entries = await getEntriesIndex();
  target.replaceChildren();
  const empty = node("option", "Selecione um registro");
  empty.value = "";
  target.append(empty);
  for (const e of entries) {
    const o = node("option", `${e.title} · ${e.module_name}`);
    o.value = e.id;
    target.append(o);
  }
  reset();
  await refresh();
}
