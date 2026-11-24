import assert from "node:assert/strict";
import { readFileSync, readdirSync } from "node:fs";
import { execFileSync } from "node:child_process";
import { fileURLToPath } from "node:url";
import { test } from "node:test";

const root = fileURLToPath(new URL("../", import.meta.url));
const read = (path) =>
  readFileSync(new URL(`../${path}`, import.meta.url), "utf8");
const json = (path) => JSON.parse(read(path));

test("produção restringe rede a IPC e não dá acesso a assets do usuário", () => {
  const security = json("src-tauri/tauri.conf.json").app.security;
  const directives = Object.fromEntries(
    security.csp.split(";").map((item) => {
      const [key, ...values] = item.trim().split(/\s+/);
      return [key, values];
    }),
  );
  assert.deepEqual(directives["default-src"], ["'none'"]);
  assert.deepEqual(directives["connect-src"], ["ipc:", "http://ipc.localhost"]);
  for (const key of ["script-src", "style-src", "font-src", "worker-src"]) {
    assert.deepEqual(directives[key], ["'self'"]);
  }
  for (const key of ["object-src", "base-uri", "frame-src", "form-action"]) {
    assert.deepEqual(directives[key], ["'none'"]);
  }
  assert.deepEqual(directives["img-src"], ["'self'", "blob:"]);
  assert.deepEqual(security.assetProtocol, { enable: false, scope: [] });
  const capability = json("src-tauri/capabilities/main-window.json");
  assert.deepEqual(capability.windows, ["main"]);
  assert.deepEqual(capability.permissions, []);
  assert.equal(capability.remote, undefined);
});

test("catálogo original contém 61 comandos, 205 opções e 24 módulos Linux", () => {
  const seed = json("src-tauri/resources/seed/sol-update-linux.json");
  const settings = json("src-tauri/resources/sol-settings.json");
  assert.equal(seed.entries.length, 61);
  assert.equal(
    seed.entries.reduce((sum, entry) => sum + entry.options.length, 0),
    205,
  );
  assert.equal(
    settings.modules.filter((module) => module.area_id === "linux").length,
    24,
  );
  assert.equal(settings.profiles.length, 9);
});

test("Git ignora gestão, dados e builds e permite os dois lockfiles", () => {
  const ignored = [
    "prompts/PROMPT-01.md",
    "AGENTS.md",
    "audit/decisoes.md",
    "userdata/test.txt",
    "data-test/test.txt",
    ".sol-data/test.txt",
    "example.db-wal",
    "example.sqlite3",
    "example.pdf",
    "node_modules/test.txt",
    "dist/index.html",
    "src-tauri/target/test.txt",
    "src-tauri/gen/test.json",
  ];
  const result = execFileSync("git", ["check-ignore", "--stdin"], {
    cwd: root,
    input: ignored.join("\n"),
    encoding: "utf8",
  })
    .trim()
    .split(/\r?\n/);
  assert.deepEqual(result, ignored);
  for (const path of ["package-lock.json", "src-tauri/Cargo.lock"]) {
    assert.doesNotMatch(
      read(".gitignore"),
      new RegExp(`^${path.replaceAll(".", "\\.")}$`, "m"),
    );
    assert.ok(read(path).length > 0);
  }
});

test("os arquivos copiados permanecem idênticos ao kit, quando ele está disponível", (t) => {
  let source;
  try {
    source = readFileSync(
      new URL("../prompts/kit/sol-settings.json", import.meta.url),
    );
  } catch (error) {
    if (error.code !== "ENOENT") throw error;
    t.skip("Kit de gestão local não está presente em um clone público.");
    return;
  }
  assert.deepEqual(
    source,
    readFileSync(
      new URL("../src-tauri/resources/sol-settings.json", import.meta.url),
    ),
  );
  const pairs = [
    ["sol-update-linux.json", "src-tauri/resources/seed/sol-update-linux.json"],
    ...["settings", "update"].map((name) => [
      `schemas/${name}.schema.json`,
      `schemas/${name}.schema.json`,
    ]),
    ...["topologia", "perfil-modulo", "patch-pessoal"].map((name) => [
      `examples/${name}.json`,
      `examples/${name}.json`,
    ]),
    ...["png", "ico"].flatMap((ext) => [
      [`assets/sol.${ext}`, `public/assets/sol.${ext}`],
      [`assets/sol.${ext}`, `src-tauri/icons/sol.${ext}`],
    ]),
  ];
  for (const [from, to] of pairs) {
    assert.deepEqual(
      readFileSync(new URL(`../prompts/kit/${from}`, import.meta.url)),
      readFileSync(new URL(`../${to}`, import.meta.url)),
      to,
    );
  }
});

test("recursos empacotados contêm somente configuração e seed, sem fixtures", () => {
  const config = json("src-tauri/tauri.conf.json");
  assert.deepEqual(config.bundle.resources, [
    "resources/sol-settings.json",
    "resources/seed/sol-update-linux.json",
  ]);
  assert.deepEqual(
    readdirSync(new URL("../src-tauri/resources/seed/", import.meta.url)),
    ["sol-update-linux.json"],
  );
  const source = read("src/main.ts") + read("src/ipc.ts");
  assert.doesNotMatch(source, /fetch\s*\(|XMLHttpRequest|WebSocket|https:\/\//);
});
