import mermaid from "mermaid";
import DOMPurify from "dompurify";
import * as pdf from "pdfjs-dist";
import worker from "pdfjs-dist/build/pdf.worker.min.mjs?url";
import { invoke } from "@tauri-apps/api/core";
import type { BlockPayload } from "./ipc";
import { node, button, errorMessage } from "./dom";
pdf.GlobalWorkerOptions.workerSrc = worker;
mermaid.initialize({
  startOnLoad: false,
  securityLevel: "strict",
  suppressErrorRendering: true,
  theme: "dark",
  htmlLabels: false,
  look: "classic",
  flowchart: { htmlLabels: false },
  maxTextSize: 50000,
  maxEdges: 1000,
});
let sequence = 0;
export function cleanSvg(source: string): string {
  const safe = DOMPurify.sanitize(source, {
    USE_PROFILES: { svg: true, svgFilters: false },
    FORBID_TAGS: [
      "foreignObject",
      "a",
      "style",
      "image",
      "use",
      "animate",
      "set",
    ],
    FORBID_ATTR: ["href", "xlink:href", "style"],
  });
  const doc = new DOMParser().parseFromString(safe, "image/svg+xml");
  for (const e of doc.querySelectorAll("*"))
    for (const a of [...e.attributes])
      if (a.name.startsWith("on") || /url\(\s*(?!#)/i.test(a.value))
        e.removeAttribute(a.name);
  return new XMLSerializer().serializeToString(doc.documentElement);
}
export async function drawMermaid(
  target: HTMLElement,
  content: string,
): Promise<void> {
  if (
    content.length > 50000 ||
    content.trimStart().startsWith("---") ||
    /%%\{|\b(click|href|image|icon|style|classDef|linkStyle)\b|@\{|url\s*\(|<|\/\/|(?:https?|data|file|blob|javascript):/i.test(
      content,
    )
  )
    throw Error(
      "Mermaid contém diretivas, HTML ou recursos ativos não permitidos.",
    );
  const { svg } = await mermaid.render(`diagram-${++sequence}`, content);
  target.innerHTML = cleanSvg(svg);
}
function viewport(svg: SVGSVGElement): HTMLElement {
  const wrap = node("div", "", "visual-viewport");
  wrap.tabIndex = 0;
  wrap.setAttribute(
    "aria-label",
    "Diagrama: arraste para mover; use os botões para ampliar",
  );
  const scene = node("div", "", "visual-scene");
  scene.append(svg);
  wrap.append(scene);
  let scale = 1,
    x = 0,
    y = 0,
    start: { x: number; y: number } | null = null;
  const update = () => {
    scene.style.transform = `translate(${x}px,${y}px) scale(${scale})`;
  };
  wrap.addEventListener("pointerdown", (e) => {
    start = { x: e.clientX - x, y: e.clientY - y };
    wrap.setPointerCapture(e.pointerId);
  });
  wrap.addEventListener("pointermove", (e) => {
    if (start) {
      x = e.clientX - start.x;
      y = e.clientY - start.y;
      update();
    }
  });
  wrap.addEventListener("pointerup", () => {
    start = null;
  });
  wrap.addEventListener("pointercancel", () => {
    start = null;
  });
  const box = node("div");
  box.append(
    button("− Zoom", () => {
      scale = Math.max(0.25, scale / 1.25);
      update();
    }),
    button("+ Zoom", () => {
      scale = Math.min(4, scale * 1.25);
      update();
    }),
    button("Centralizar", () => {
      scale = 1;
      x = y = 0;
      update();
    }),
    wrap,
  );
  return box;
}
type N = { id: string; label: string; kind?: string; ip?: string };
type E = {
  id: string;
  from: string;
  to: string;
  from_port?: string;
  to_port?: string;
  medium?: string;
};
export function topology(v: BlockPayload): HTMLElement {
  const ns = (v.nodes ?? []) as N[],
    es = (v.edges ?? []) as E[];
  const w = node("div");
  const svg = document.createElementNS("http://www.w3.org/2000/svg", "svg");
  svg.setAttribute("viewBox", "0 0 800 480");
  svg.setAttribute("role", "img");
  svg.setAttribute(
    "aria-label",
    "Topologia física: conexões descritas na tabela",
  );
  const positions = new Map(
    ns.map((n, i) => [
      n.id,
      {
        x: 400 + 280 * Math.cos((2 * Math.PI * i) / Math.max(1, ns.length)),
        y: 240 + 160 * Math.sin((2 * Math.PI * i) / Math.max(1, ns.length)),
      },
    ]),
  );
  const shape = (tag: string, attrs: Record<string, string>, text?: string) => {
    const e = document.createElementNS(svg.namespaceURI, tag);
    for (const [k, val] of Object.entries(attrs)) e.setAttribute(k, val);
    e.textContent = text ?? "";
    svg.append(e);
    return e;
  };
  for (const e of es) {
    const a = positions.get(e.from),
      b = positions.get(e.to);
    if (!a || !b) continue;
    shape("line", {
      x1: String(a.x),
      y1: String(a.y),
      x2: String(b.x),
      y2: String(b.y),
      stroke: "#facc15",
      "stroke-width": "3",
    });
  }
  for (const n of ns) {
    const p = positions.get(n.id)!;
    shape("rect", {
      x: String(p.x - 80),
      y: String(p.y - 35),
      width: "160",
      height: "70",
      rx: "12",
      fill: "#202020",
      stroke: "#facc15",
    });
    shape(
      "text",
      {
        x: String(p.x),
        y: String(p.y - 5),
        fill: "#fff",
        "text-anchor": "middle",
      },
      n.label,
    );
    shape(
      "text",
      {
        x: String(p.x),
        y: String(p.y + 18),
        fill: "#facc15",
        "text-anchor": "middle",
        "font-size": "12",
      },
      [n.kind, n.ip].filter(Boolean).join(" · "),
    );
  }
  w.append(
    viewport(svg),
    node(
      "p",
      "Legenda: caixas = equipamentos; linhas = enlaces declarados. Posições automáticas não representam distância física.",
      "search-help",
    ),
    node("h4", "Tabela de conexões"),
  );
  const table = node("table"),
    head = node("tr");
  for (const s of ["Origem", "Porta", "Destino", "Porta", "Meio físico"])
    head.append(node("th", s));
  table.append(head);
  for (const e of es) {
    const row = node("tr");
    for (const s of [
      ns.find((n) => n.id === e.from)?.label ?? e.from,
      e.from_port ?? "—",
      ns.find((n) => n.id === e.to)?.label ?? e.to,
      e.to_port ?? "—",
      e.medium ?? "—",
    ])
      row.append(node("td", s));
    table.append(row);
  }
  const scroll = node("div", "", "table-scroll");
  scroll.append(table);
  w.append(scroll);
  return w;
}
interface Asset {
  id: string;
  filename: string;
  media_type: string;
  size: number;
  sha256: string;
}
export async function renderAsset(w: HTMLElement, id: string): Promise<void> {
  const a = await invoke<Asset>("get_asset", { id });
  w.append(node("p", `${a.filename} · ${a.size} bytes`));
  if (
    !["image/png", "image/jpeg", "image/svg+xml", "application/pdf"].includes(
      a.media_type,
    )
  ) {
    w.append(
      node(
        "p",
        "Original preservado. Associe uma exportação PNG, SVG ou PDF em outro bloco deste registro para visualizar.",
      ),
    );
    return;
  }
  const open = button(
    a.media_type === "application/pdf" ? "Abrir PDF local" : "Mostrar imagem",
    async () => {
      open.disabled = true;
      try {
        const bytes = new Uint8Array(
          await invoke<number[]>("read_asset", { id }),
        );
        if (a.media_type === "application/pdf") {
          await renderPdf(w, bytes);
        } else {
          const data =
            a.media_type === "image/svg+xml"
              ? new TextEncoder().encode(
                  cleanSvg(new TextDecoder().decode(bytes)),
                )
              : bytes;
          const url = URL.createObjectURL(
            new Blob([data], { type: a.media_type }),
          );
          const img = node("img");
          img.alt = a.filename;
          img.className = "asset-image";
          img.onload = img.onerror = () => URL.revokeObjectURL(url);
          img.src = url;
          w.append(img);
        }
      } catch (e) {
        open.disabled = false;
        w.append(node("p", errorMessage(e)));
      }
    },
  );
  w.append(open);
}
export function visual(v: BlockPayload): HTMLElement {
  if (v.type === "topology") return topology(v);
  const w = node("div");
  if (v.preview_asset_id) {
    const preview = node("div");
    preview.append(node("h4", "Exportação associada"));
    w.append(preview);
    void renderAsset(preview, v.preview_asset_id).catch((e) =>
      preview.append(node("p", errorMessage(e))),
    );
  }
  if (v.asset_id)
    void renderAsset(w, v.asset_id).catch((e) =>
      w.append(node("p", errorMessage(e))),
    );
  else if (v.format === "mermaid") {
    w.append(node("p", "Renderizando diagrama local…"));
    void drawMermaid(w, v.content ?? "")
      .then(() => {
        const svg = w.querySelector("svg");
        if (svg) w.replaceChildren(viewport(svg));
      })
      .catch((e) => w.replaceChildren(node("p", errorMessage(e))));
  }
  return w;
}

export async function renderPdf(
  w: HTMLElement,
  bytes: Uint8Array,
): Promise<void> {
  const doc = await pdf.getDocument({
    data: bytes,
    useWasm: false,
    isOffscreenCanvasSupported: false,
    isImageDecoderSupported: false,
    useSystemFonts: false,
    disableFontFace: true,
    disableAutoFetch: true,
    disableStream: true,
    stopAtErrors: true,
  }).promise;
  const view = node("div"),
    bar = node("div"),
    canvas = node("canvas");
  let current = 1;
  let rendering = false;
  const label = node("span");
  const paint = async () => {
    if (rendering) return;
    rendering = true;
    try {
      const p = await doc.getPage(current);
      const base = p.getViewport({ scale: 1 });
      const vp = p.getViewport({
        scale: Math.min(1.5, 1600 / Math.max(base.width, base.height)),
      });
      canvas.width = vp.width;
      canvas.height = vp.height;
      await p.render({
        canvas,
        canvasContext: canvas.getContext("2d")!,
        viewport: vp,
      }).promise;
      label.textContent = `Página ${current} de ${doc.numPages}`;
    } finally {
      rendering = false;
    }
  };
  bar.append(
    button("Anterior", async () => {
      if (!rendering && current > 1) {
        current--;
        await paint();
      }
    }),
    label,
    button("Próxima", async () => {
      if (!rendering && current < doc.numPages) {
        current++;
        await paint();
      }
    }),
  );
  view.append(bar, canvas);
  w.append(view);
  await paint();
}
