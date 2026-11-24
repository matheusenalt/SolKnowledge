export function el<T extends HTMLElement>(id: string): T {
  const v = document.getElementById(id);
  if (!v) throw new Error(`Elemento ausente: ${id}`);
  return v as T;
}
export function node<K extends keyof HTMLElementTagNameMap>(
  tag: K,
  text = "",
  className = "",
): HTMLElementTagNameMap[K] {
  const v = document.createElement(tag);
  v.textContent = text;
  if (className) v.className = className;
  return v;
}
export function errorMessage(error: unknown): string {
  return error instanceof Error
    ? error.message
    : typeof error === "object" &&
        error &&
        "message" in error &&
        typeof error.message === "string"
      ? error.message
      : "Não foi possível concluir. Recarregue os dados e tente novamente.";
}
export function notify(text: string): void {
  el("notice").textContent = text;
}
export function button(
  label: string,
  action: () => void | Promise<void>,
  className = "secondary",
): HTMLButtonElement {
  const b = node("button", label, className);
  b.type = "button";
  b.addEventListener("click", () => {
    Promise.resolve()
      .then(action)
      .catch((e) => notify(errorMessage(e)));
  });
  return b;
}
export function link(
  label: string,
  hash: string,
  className = "",
): HTMLAnchorElement {
  const a = node("a", label, className);
  a.href = hash;
  return a;
}
export function field(label: string, input: HTMLElement): HTMLLabelElement {
  const l = node("label", label, "field");
  l.append(input);
  return l;
}
export function input(value = "", type = "text"): HTMLInputElement {
  const v = node("input");
  v.type = type;
  v.value = value;
  return v;
}
export function textarea(value = "", rows = 5): HTMLTextAreaElement {
  const v = node("textarea");
  v.value = value;
  v.rows = rows;
  return v;
}
export function select(
  options: [string, string][],
  value = "",
): HTMLSelectElement {
  const v = node("select");
  for (const [id, label] of options) {
    const o = node("option", label);
    o.value = id;
    v.append(o);
  }
  v.value = value;
  return v;
}
export function copyButton(text: string): HTMLButtonElement {
  return button("Copiar", async () => {
    await navigator.clipboard.writeText(text);
    notify("Texto copiado. Nenhum comando foi executado.");
  });
}
export const componentLabels: Record<string, string> = {
  text: "Texto",
  personal_note: "Nota pessoal",
  command: "Comandos e opções",
  code: "Código",
  warning: "Aviso",
  reference: "Referências",
  diagram: "Diagrama",
  topology: "Topologia",
  image: "Imagem",
  attachment: "Anexo",
  equipment: "Equipamentos",
  cabling: "Cabeamento",
  procedure: "Procedimento",
  troubleshooting: "Diagnóstico",
  table: "Tabela",
};
export const typeLabels: Record<string, string> = {
  command: "Comando",
  concept: "Conceito",
  summary: "Resumo",
  procedure: "Procedimento",
  troubleshooting: "Diagnóstico",
  lab: "Laboratório",
  project: "Projeto",
  note: "Nota",
  reference: "Referência",
};
export const statusLabels: Record<string, string> = {
  not_assessed: "Não avaliado",
  studying: "Estudando",
  understood: "Compreendido",
  applied: "Aplicado",
};
export function dialogForm(title: string): {
  dialog: HTMLDialogElement;
  form: HTMLFormElement;
  status: HTMLParagraphElement;
} {
  const dialog = el<HTMLDialogElement>("editor-dialog");
  dialog.replaceChildren();
  const header = node("div", "", "dialog-heading");
  const heading = node("h2", title);
  heading.id = "editor-title";
  header.append(
    heading,
    button("Fechar ×", () => dialog.close()),
  );
  const form = node("form");
  const status = node("p", "", "form-status");
  status.setAttribute("role", "status");
  form.append(status);
  dialog.append(header, form);
  if (!dialog.open) dialog.showModal();
  return { dialog, form, status };
}
export function onSave(
  form: HTMLFormElement,
  status: HTMLElement,
  action: () => Promise<void>,
): void {
  const save = node("button", "Salvar");
  save.type = "submit";
  form.append(save);
  form.addEventListener("submit", async (e) => {
    e.preventDefault();
    save.disabled = true;
    status.textContent = "Salvando…";
    try {
      await action();
    } catch (error) {
      status.textContent = errorMessage(error);
    } finally {
      save.disabled = false;
    }
  });
}
