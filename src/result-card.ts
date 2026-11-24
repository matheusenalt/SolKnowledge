import type { SearchResult } from "./ipc";
import * as api from "./ipc";
import { node, link, button, typeLabels, statusLabels } from "./dom";
export function resultCard(
  result: SearchResult,
  searching = false,
): HTMLElement {
  const article = node("article", "", "result-card"),
    head = node("div", "", "result-heading"),
    h = node("h3");
  h.append(
    link(
      result.title,
      `#entry/${encodeURIComponent(result.id)}`,
      "result-title",
    ),
  );
  const star = button(
    result.favorite ? "★ Favorito" : "☆ Favoritar",
    async () => {
      star.disabled = true;
      try {
        await api.setFavorite(result.id, !result.favorite);
        result.favorite = !result.favorite;
        star.textContent = result.favorite ? "★ Favorito" : "☆ Favoritar";
        if (location.hash === "#favorites")
          window.dispatchEvent(new HashChangeEvent("hashchange"));
      } finally {
        star.disabled = false;
      }
    },
  );
  star.setAttribute(
    "aria-label",
    `${result.favorite ? "Remover dos favoritos" : "Favoritar"}: ${result.title}`,
  );
  if (result.visual) {
    const icon = node("span", "▧ Material visual");
    head.append(icon);
  }
  head.append(h, star);
  article.append(
    node("p", `${result.area_name} / ${result.module_name}`, "result-path"),
    head,
    node("p", result.snippet, "result-snippet"),
  );
  const foot = node("div", "", "result-meta");
  foot.append(
    node("span", typeLabels[result.kind] ?? result.kind, "match-badge"),
    node(
      "span",
      statusLabels[result.learning_status] ?? result.learning_status,
      "match-badge",
    ),
  );
  if (searching)
    foot.append(
      node(
        "span",
        { exact: "Exata", textual: "Textual", approximate: "Aproximada" }[
          result.match_kind
        ],
        "match-badge",
      ),
    );
  article.append(foot);
  return article;
}
