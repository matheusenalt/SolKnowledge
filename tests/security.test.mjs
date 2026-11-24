import assert from "node:assert/strict";
import { readFileSync, readdirSync } from "node:fs";
import { test } from "node:test";
const read = (p) => readFileSync(new URL("../" + p, import.meta.url), "utf8");
test("fontes de runtime não têm cliente de rede nem executor de comandos", () => {
  for (const name of readdirSync(new URL("../src/", import.meta.url)).filter(
    (n) => n.endsWith(".ts"),
  )) {
    const source = read("src/" + name);
    assert.doesNotMatch(
      source,
      /\b(?:fetch|WebSocket|EventSource|XMLHttpRequest)\s*\(|sendBeacon\s*\(|\beval\s*\(|\bnew\s+Function\s*\(/,
      name,
    );
  }
  for (const name of readdirSync(
    new URL("../src-tauri/src/", import.meta.url),
  ).filter((n) => n.endsWith(".rs") && !n.startsWith("tests"))) {
    assert.doesNotMatch(
      read("src-tauri/src/" + name),
      /std::process::Command|Command::new|std::net::|reqwest::|ureq::|\.plugin\s*\(/,
      name,
    );
  }
  const cargo = read("src-tauri/Cargo.toml");
  assert.doesNotMatch(
    cargo,
    /^\s*(?:reqwest|ureq|tauri-plugin-(?:shell|http|fs|sql|updater))\s*=/m,
  );
});
test("conteúdo visual usa bibliotecas locais e PDF sem recursos ativos", () => {
  const v = read("src/visual.ts");
  assert.match(v, /securityLevel:\s*"strict"/);
  assert.match(v, /htmlLabels:\s*false/);
  assert.match(v, /target\.innerHTML\s*=\s*cleanSvg\(svg\)/);
  assert.match(v, /FORBID_TAGS:/);
  assert.match(v, /pdfjs-dist\/build\/pdf\.worker\.min\.mjs\?url/);
  for (const option of [
    "useWasm: false",
    "useSystemFonts: false",
    "disableFontFace: true",
    "isImageDecoderSupported: false",
  ])
    assert.ok(v.includes(option), option);
  assert.doesNotMatch(
    v,
    /getJSActions|executeJavaScript|annotationLayer|window\.open/,
  );
});
test("copiar comandos é somente escrita de texto, sem processo ou navegação", () => {
  const dom = read("src/dom.ts");
  assert.match(dom, /navigator\.clipboard\.writeText\(text\)/);
  const blocks = read("src/blocks.ts");
  const command = blocks.slice(
    blocks.indexOf("function command("),
    blocks.indexOf("function plain("),
  );
  assert.match(command, /copyButton\(content\)/);
  assert.doesNotMatch(command, /invoke\(|eval\(|window\.open|location\./);
});
