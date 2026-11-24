import test from "node:test";
import assert from "node:assert/strict";
import { liveSearch } from "../src/live-search.ts";

const flush = () => new Promise((resolve) => setImmediate(resolve));
function fixture() {
  const jobs = new Map();
  const calls = [],
    events = [];
  let id = 0;
  const control = liveSearch({
    schedule(run, delay) {
      assert.equal(delay, 200);
      jobs.set(++id, run);
      return id;
    },
    unschedule(timer) {
      jobs.delete(timer);
    },
    search(query, scope) {
      return new Promise((resolve, reject) =>
        calls.push({ query, scope, resolve, reject }),
      );
    },
    pending() {
      events.push("pending");
    },
    start(query) {
      events.push(["start", query]);
    },
    results(value) {
      events.push(["results", value]);
    },
    error(error) {
      events.push(["error", error.message]);
    },
    clear() {
      events.push("clear");
    },
  });
  return {
    control,
    calls,
    events,
    tick: async () => {
      const scheduled = [...jobs.values()];
      jobs.clear();
      scheduled.forEach((run) => run());
      await flush();
    },
  };
}
test("digitação rápida consulta automaticamente após 200ms, sem redundância no Enter", async () => {
  const f = fixture();
  f.control.input("L", {});
  f.control.input("ls", {});
  f.control.input("ls -lh", {});
  assert.equal(f.calls.length, 0);
  await f.tick();
  assert.deepEqual(
    f.calls.map((c) => c.query),
    ["ls -lh"],
  );
  f.control.input("ls -lh", {}, true);
  assert.equal(f.calls.length, 1);
  f.calls[0].resolve(["ls"]);
  await flush();
  f.control.input(" ls -lh ", {}, true);
  await flush();
  assert.equal(f.calls.length, 1);
});
test("um caractere é enviado e Enter antecipa o timer uma única vez", async () => {
  const f = fixture();
  f.control.input("L", {});
  f.control.input("L", {}, true);
  await flush();
  await f.tick();
  assert.equal(f.calls.length, 1);
  assert.equal(f.calls[0].query, "L");
});
test("respostas e erros antigos são descartados inclusive durante debounce", async () => {
  const f = fixture();
  f.control.input("l", {}, true);
  await flush();
  f.control.input("ls", {});
  f.calls[0].resolve(["antigo"]);
  await flush();
  assert.equal(f.events.filter((e) => e[0] === "results").length, 0);
  await f.tick();
  f.control.input("df", {}, true);
  await flush();
  f.calls[2].resolve(["novo"]);
  await flush();
  f.calls[1].reject(new Error("antigo"));
  await flush();
  assert.deepEqual(
    f.events.filter(
      (e) => Array.isArray(e) && ["results", "error"].includes(e[0]),
    ),
    [["results", ["novo"]]],
  );
});
test("limpeza impede retorno antigo e espaços não disparam IPC", async () => {
  const f = fixture();
  f.control.input("L", {}, true);
  await flush();
  f.control.input("   ", {});
  f.calls[0].resolve(["antigo"]);
  await flush();
  await f.tick();
  assert.equal(f.calls.length, 1);
  assert.equal(f.events.at(-1), "clear");
});
test("troca de escopo e perfil cancela pendências e admite nova consulta", async () => {
  const f = fixture();
  f.control.input("L", { area: "linux" }, true);
  await flush();
  f.control.input("L", { area: "redes" }, true);
  await flush();
  f.calls[0].resolve(["linux"]);
  f.calls[1].resolve(["redes"]);
  await flush();
  assert.deepEqual(f.events.at(-1), ["results", ["redes"]]);
  f.control.cancel();
  f.control.input("L", { area: "redes" }, true);
  await flush();
  assert.equal(f.calls.length, 3);
  f.control.cancel();
  f.calls[2].resolve(["perfil anterior"]);
  await flush();
  assert.deepEqual(f.events.at(-1), ["start", "L"]);
});
test("zero resultados e erro atual encerram a consulta; submit permite tentar de novo", async () => {
  const f = fixture();
  f.control.input("inexistente", {}, true);
  await flush();
  f.calls[0].resolve([]);
  await flush();
  assert.deepEqual(f.events.at(-1), ["results", []]);
  f.control.input("*", {}, true);
  await flush();
  f.calls[1].reject(new Error("Falha local"));
  await flush();
  assert.deepEqual(f.events.at(-1), ["error", "Falha local"]);
  f.control.input("*", {}, true);
  await flush();
  assert.equal(f.calls.length, 3);
});
test("navegar cancela também o timer e não executa consulta atrasada", async () => {
  const f = fixture();
  f.control.input("L", {});
  f.control.cancel();
  await f.tick();
  assert.equal(f.calls.length, 0);
});
