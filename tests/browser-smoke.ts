import { renderBlock, orderedVisible } from "../src/blocks";
import { resultCard } from "../src/result-card";
import { liveSearch } from "../src/live-search";
import topologyFixture from "../examples/topologia.json";

/** Component fixtures only: no Tauri IPC emulation and no personal-data writes. */
export async function runSearchBrowserChecks() {
  const page = document.getElementById("page")!;
  const original = [...page.childNodes];
  const host = document.createElement("div");
  host.className = "search-results";
  const fixture = {
    id: "layout-fixture",
    title: "ls",
    snippet: "Listar arquivos",
    kind: "command",
    area_id: "linux",
    area_name: "Linux",
    module_id: "fixture",
    module_name: "Arquivos e diretórios",
    learning_status: "not_assessed",
    favorite: false,
    visual: false,
    match_kind: "textual" as const,
  };
  const input = document.createElement("input");
  input.setAttribute("aria-label", "Teste isolado do controlador de pesquisa");
  host.append(input);
  page.replaceChildren(host);
  const calls: string[] = [];
  const control = liveSearch<(typeof fixture)[], string>({
    search: async (value) => {
      calls.push(value);
      return value === "vazio" ? [] : [fixture];
    },
    pending: () => {},
    start: () => {},
    clear: () => {
      host.querySelectorAll("article").forEach((e) => e.remove());
    },
    results: (rows) => {
      host.querySelectorAll("article").forEach((e) => e.remove());
      rows.forEach((row) => host.append(resultCard(row, true)));
    },
    error: (error) => {
      throw error;
    },
  });
  input.addEventListener("input", () => control.input(input.value, "perfil"));
  try {
    input.focus();
    for (const value of ["L", "ls", "ls -lh"]) {
      input.value = value;
      input.dispatchEvent(new Event("input"));
    }
    await new Promise((resolve) => setTimeout(resolve, 260));
    if (
      calls.join() !== "ls -lh" ||
      document.activeElement !== input ||
      !host.querySelector("article")
    )
      throw Error("Debounce/foco falhou");
    input.value = "vazio";
    input.dispatchEvent(new Event("input"));
    await new Promise((resolve) => setTimeout(resolve, 260));
    if (host.querySelector("article"))
      throw Error("Resultado vazio preservou cartão antigo");
    input.value = "";
    input.dispatchEvent(new Event("input"));
    const long = resultCard(
      {
        ...fixture,
        title: "L — " + "Referência longa ".repeat(12),
        snippet: "Trecho de conteúdo\n".repeat(18),
      },
      true,
    );
    host.append(resultCard(fixture, true), long);
    const short = host.querySelector("article")!;
    const snippet = long.querySelector<HTMLElement>(".result-snippet")!;
    const lineHeight = parseFloat(getComputedStyle(snippet).lineHeight);
    if (snippet.getBoundingClientRect().height > lineHeight * 3 + 1)
      throw Error("Trecho expandiu além de três linhas");
    for (const card of host.querySelectorAll("article")) {
      const rect = card.getBoundingClientRect();
      if (
        card.scrollWidth > card.clientWidth + 1 ||
        rect.right > innerWidth + 1
      )
        throw Error("Cartão transbordou horizontalmente");
    }
    return {
      checks: [
        "Digitação rápida dispara uma busca automática",
        "Foco permanece no campo",
        "Sem resultados remove cartões anteriores",
        "Trecho longo limitado a três linhas com detalhe acessível pelo título",
        "Cartões sem transbordamento horizontal",
      ],
      width: innerWidth,
      shortHeight: short.getBoundingClientRect().height,
      longHeight: long.getBoundingClientRect().height,
      titleSize: getComputedStyle(long.querySelector("a")!).fontSize,
    };
  } finally {
    control.cancel();
    page.replaceChildren(...original);
  }
}

/** Detached renderer checks; never writes to the user's database or clipboard. */
export async function runRendererChecks(): Promise<string[]> {
  const checks: string[] = [];
  const check = (ok: boolean, name: string): void => {
    if (!ok) throw new Error(name);
    checks.push(name);
  };
  const unsafe = renderBlock({
    virtual_block: false,
    payload: {
      id: "test",
      type: "text",
      content: "<img src=x onerror=alert(1)><script>alert(1)</script>",
    },
  });
  check(
    !unsafe.querySelector("script,img") &&
      !!unsafe.textContent?.includes("<script>"),
    "HTML ativo exibido como texto",
  );
  const code = renderBlock({
    virtual_block: false,
    payload: {
      id: "code",
      type: "code",
      language: "html",
      content: "<script>neverRun()</script>",
    },
  });
  check(
    !code.querySelector("script") &&
      !!code.querySelector("pre")?.textContent?.includes("neverRun()"),
    "Código é conteúdo, sem execução",
  );
  const b = renderBlock({
    virtual_block: true,
    payload: {
      id: "v1:command",
      type: "command",
      content: "ls",
      options: [
        { id: "1", example: "ls -l", featured: true },
        { id: "2", example: "ls -a", featured: false },
      ],
    },
  });
  const toggle = [...b.querySelectorAll("button")].find((b) =>
    b.textContent?.startsWith("Todas"),
  );
  check(
    b.querySelectorAll(".option-card").length === 1,
    "Somente opções principais inicialmente",
  );
  if (!toggle) throw new Error("Botão de expansão ausente");
  toggle.click();
  await Promise.resolve();
  check(
    b.querySelectorAll(".option-card").length === 2,
    "Expansão de todas as cadastradas",
  );
  toggle.click();
  await Promise.resolve();
  check(
    b.querySelectorAll(".option-card").length === 1,
    "Retorno às principais",
  );
  const views = ["text", "code", "personal_note"].map((type, i) => ({
    virtual_block: false,
    payload: { id: String(i), type },
  }));
  check(
    orderedVisible(views, ["code", "text"])
      .map((v) => v.payload.id)
      .join(",") === "1,0" && views.length === 3,
    "Ordem por perfil preserva blocos ocultos",
  );
  return checks;
}

import { drawMermaid, topology, cleanSvg } from "../src/visual";
export async function runVisualChecks(): Promise<string[]> {
  const blocks = topologyFixture.entries[0]!.blocks;
  const graph = topology(blocks[0]);
  if (
    graph.querySelectorAll("line").length !== 2 ||
    graph.querySelectorAll("rect").length !== 3 ||
    graph.querySelectorAll("table tr").length !== 3 ||
    !graph.textContent?.includes("Ethernet UTP")
  )
    throw Error("Topologia/tabela divergente");
  const zoom = [...graph.querySelectorAll("button")].find(
    (b) => b.textContent === "+ Zoom",
  )!;
  zoom.click();
  await Promise.resolve();
  const scene = graph.querySelector<HTMLElement>(".visual-scene")!;
  if (!scene.style.transform.includes("scale(1.25)"))
    throw Error("Zoom não aplicado");
  const reset = [...graph.querySelectorAll("button")].find(
    (b) => b.textContent === "Centralizar",
  )!;
  reset.click();
  await Promise.resolve();
  if (!scene.style.transform.includes("scale(1)"))
    throw Error("Centralização não aplicada");
  const host = document.createElement("div");
  document.body.append(host);
  try {
    await drawMermaid(host, blocks[1].content);
    if (
      !host.querySelector("svg") ||
      !host.textContent?.includes("Switch") ||
      host.querySelector("script,a,foreignObject")
    )
      throw Error("Mermaid não renderizado com segurança");
  } finally {
    host.remove();
  }
  for (const source of [
    'graph TD\n A-->B\n click A "https://evil"',
    '%%{init:{securityLevel:"loose"}}%%\ngraph TD\n A-->B',
    "---\nconfig:\n securityLevel: loose\n---\ngraph TD\n A-->B",
  ]) {
    let rejected = false;
    try {
      await drawMermaid(host, source);
    } catch {
      rejected = true;
    }
    if (!rejected) throw Error("Mermaid ativo aceito");
  }
  const cleaned = cleanSvg(
    '<svg xmlns="http://www.w3.org/2000/svg" onload="alert(1)"><script>alert(1)</script><a href="https://evil"><text>link</text></a><image href="https://evil/x"/></svg>',
  );
  if (/onload|<script|href=|<image|<a\s/.test(cleaned))
    throw Error("SVG ativo escapou");
  return [
    "3 nós e 2 enlaces declarados",
    "Tabela de conexões com portas e Ethernet UTP",
    "Mermaid local renderizado",
    "Diretivas, links e configuração Mermaid rejeitados",
    "SVG sanitizado sem conteúdo ativo",
  ];
}

export async function runPdfCheck(): Promise<string> {
  const { renderPdf } = await import("../src/visual");
  const objects = [
    "<< /Type /Catalog /Pages 2 0 R >>",
    "<< /Type /Pages /Kids [3 0 R] /Count 1 >>",
    "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 200] /Resources << >> /Contents 4 0 R >>",
    "<< /Length 28 >>\nstream\n0 0 1 rg 10 10 80 80 re f\nendstream",
  ];
  let doc = "%PDF-1.4\n";
  const offsets = [0];
  for (let i = 0; i < objects.length; i++) {
    offsets.push(doc.length);
    doc += `${i + 1} 0 obj\n${objects[i]}\nendobj\n`;
  }
  const xref = doc.length;
  doc += `xref\n0 5\n0000000000 65535 f \n${offsets
    .slice(1)
    .map((n) => String(n).padStart(10, "0") + " 00000 n ")
    .join(
      "\n",
    )}\ntrailer\n<< /Size 5 /Root 1 0 R >>\nstartxref\n${xref}\n%%EOF`;
  const target = document.createElement("div");
  await renderPdf(target, new TextEncoder().encode(doc));
  const c = target.querySelector("canvas");
  if (!c || c.width < 1 || !target.textContent?.includes("Página 1 de 1"))
    throw Error("PDF não renderizado");
  const pixel = c.getContext("2d")!.getImageData(40, c.height - 40, 1, 1).data;
  if (pixel[2] < 200 || pixel[0] > 50)
    throw Error("Canvas PDF sem desenho esperado");
  return "PDF local renderizado em canvas com worker empacotado, sem links/scripts";
}

import { changeList, type Preview } from "../src/transfer";
export async function runTransferChecks(): Promise<string[]> {
  const preview: Preview = {
    package_id: "fixture",
    kind: "knowledge_update",
    sha256: "",
    review_token: "",
    already_applied: false,
    counts: { new: 0, updated: 2, same: 0, conflicts: 0 },
    changes: [
      {
        id: "entries/fixture/personal_description",
        before: "Texto antigo",
        after: "<script>neverRun()</script>",
        requires_approval: true,
      },
      {
        id: "entries/fixture/learning_status",
        before: "studying",
        after: "applied",
        requires_approval: true,
      },
      {
        id: "entries/new/title",
        before: null,
        after: "Título de teste",
        requires_approval: false,
      },
    ],
  };
  const view = changeList(preview);
  if (view.approved().length !== 0) throw Error("Alteração pré-aprovada");
  if (view.element.querySelector("script")) throw Error("Prévia executa HTML");
  if (!view.element.textContent?.includes("<script>neverRun()</script>"))
    throw Error("Prévia omitiu valor novo");
  const checks = view.element.querySelectorAll<HTMLInputElement>(
    "input[type=checkbox]",
  );
  if (checks.length !== 2) throw Error("Confirmações não são específicas");
  checks[0].checked = true;
  if (
    JSON.stringify(view.approved()) !==
    JSON.stringify(["entries/fixture/personal_description"])
  )
    throw Error("Confirmar nota também confirmou progresso");
  checks[1].checked = true;
  if (view.approved().length !== 2) throw Error("Confirmações não capturadas");
  checks[0].checked = false;
  if (view.approved().includes("entries/fixture/personal_description"))
    throw Error("Confirmação cancelada permaneceu");
  return [
    "Prévia mostra anterior/novo como texto inerte",
    "Nenhuma alteração pré-aprovada",
    "Nota e progresso têm confirmações independentes",
    "Desmarcar revoga somente a confirmação correspondente",
  ];
}

export async function runHostileContentChecks(): Promise<string[]> {
  const host = document.createElement("div");
  document.body.append(host);
  try {
    const markdown = renderBlock({
      virtual_block: false,
      payload: {
        id: "hostile-markdown",
        type: "text",
        format: "markdown",
        content:
          '[abrir](javascript:alert(1)) <iframe src="https://invalid.test"></iframe> <img src=x onerror=alert(1)>',
      },
    });
    if (markdown.querySelector("a,iframe,img,script"))
      throw Error("Markdown ativo renderizado");
    const svg = cleanSvg(
      '<svg xmlns="http://www.w3.org/2000/svg"><foreignObject><div xmlns="http://www.w3.org/1999/xhtml"><img src="https://invalid.test/image" onerror="alert(1)"/></div></foreignObject><use href="https://invalid.test/shape"/><rect style="fill:url(https://invalid.test/x)" onmouseover="alert(1)"/><animate attributeName="href" values="javascript:alert(1)"/></svg>',
    );
    host.innerHTML = svg;
    if (
      host.querySelector("foreignObject,use,image,img,script,animate,a") ||
      host.querySelector("[onmouseover],[onload],[href],[style]")
    )
      throw Error("SVG ativo permaneceu");
    for (const code of [
      'graph TD\n A@{ img: "https://invalid.test/image" }',
      "graph TD\n A-->B\n style A fill:url(https://invalid.test/x)",
      "graph TD\n A[<img src=x onerror=alert(1)>]",
    ]) {
      let refused = false;
      try {
        await drawMermaid(host, code);
      } catch {
        refused = true;
      }
      if (!refused) throw Error("Mermaid hostil aceito");
    }
    await drawMermaid(
      host,
      'graph TD\n A["&lt;img src=x onerror=alert(1)&gt;"] --> B[Seguro]',
    );
    if (host.querySelector("img,script,foreignObject,iframe,a"))
      throw Error("HTML codificado tornou-se ativo");
    return [
      "Markdown/HTML malicioso permanece texto",
      "SVG sem HTML, animação, eventos ou referência externa",
      "Mermaid rejeita imagens, CSS e HTML ativos",
      "HTML codificado em rótulo não cria elementos ativos",
    ];
  } finally {
    host.remove();
  }
}
